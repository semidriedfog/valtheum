use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;

#[derive(Clone, Default, Serialize)]
pub struct ScanStatus {
    pub running: bool,
    pub scanned_count: usize,
    pub report: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub struct Library {
    // A locked successor must not reopen the DB while retired handles are closing.
    pub cleanup_pending: Arc<std::sync::atomic::AtomicBool>,
    pub integrity: Arc<crate::integrity::Runtime>,
    pub db_slots: Arc<Semaphore>,
    pub file_slots: Arc<Semaphore>,
    pub recovery: Arc<crate::storage_recovery::Runtime>,
    pub archives: crate::archive_security::Runtime,
    pub root_dir: PathBuf,
    pub conn: Mutex<Option<Connection>>,
    pub scan: Mutex<ScanStatus>,
    pub security: Mutex<crate::encryption::DbSecurity>,
    // Declared last so connections and runtime state are dropped before ownership.
    pub lease: Option<Arc<crate::library_lock::Lease>>,
}

impl Library {
    pub fn unconfigured() -> Arc<Self> {
        Arc::new(Self {
            cleanup_pending: Default::default(),
            archives: Default::default(),
            integrity: Default::default(),
            db_slots: Arc::new(Semaphore::new(1)),
            file_slots: Arc::new(Semaphore::new(4)),
            recovery: Default::default(),
            root_dir: PathBuf::new(),
            lease: None,
            conn: Mutex::new(None),
            scan: Mutex::new(ScanStatus::default()),
            security: Mutex::new(crate::encryption::DbSecurity {
                encrypted: true,
                password: None,
            }),
        })
    }

    pub fn security_status(&self) -> crate::encryption::SecurityStatus {
        let (encrypted, locked) = {
            let security = self.security.lock().unwrap_or_else(|e| e.into_inner());
            (
                security.encrypted,
                security.encrypted && security.password.is_none(),
            )
        };
        crate::encryption::SecurityStatus {
            encrypted,
            locked,
            // Never hold the authentication mutex over a NAS metadata request.
            plaintext_original: encrypted
                && self.root_dir.join(crate::encryption::PLAIN_DB).exists(),
        }
    }
    pub fn open(root: PathBuf) -> anyhow::Result<Arc<Self>> {
        Self::open_reusing(root, None)
    }
    pub fn open_reusing(
        root: PathBuf,
        previous: Option<&Arc<crate::library_lock::Lease>>,
    ) -> anyhow::Result<Arc<Self>> {
        let root = root.canonicalize()?;
        anyhow::ensure!(root.is_dir(), "폴더가 아닙니다.");
        let lease = match previous.filter(|lease| lease.root == root) {
            Some(lease) => lease.clone(),
            None => crate::library_lock::Lease::acquire(&root)?,
        };
        crate::password_change::recover(&root)?;
        let encrypted = root.join(crate::encryption::SECURE_DB).try_exists()?;
        let conn = if encrypted {
            None
        } else {
            let conn =
                crate::encryption::open(&root.join(crate::encryption::PLAIN_DB), None, true)?;
            crate::db::init_db(&conn)?;
            conn.pragma_update(None, "synchronous", "FULL")?;
            crate::db::recover_pending_deletions(&conn, &root)?;
            Some(conn)
        };
        Ok(Arc::new(Self {
            cleanup_pending: Default::default(),
            archives: Default::default(),
            integrity: Default::default(),
            db_slots: Arc::new(Semaphore::new(1)),
            file_slots: Arc::new(Semaphore::new(4)),
            recovery: Default::default(),
            root_dir: root,
            lease: Some(lease),
            conn: Mutex::new(conn),
            security: Mutex::new(crate::encryption::DbSecurity {
                encrypted,
                password: None,
            }),
            scan: Mutex::new(ScanStatus::default()),
        }))
    }
}

#[derive(Clone)]
pub struct RequestState {
    pub library: Arc<Library>,
    pub generation: u64,
    pub archive_slots: Arc<Semaphore>,
}

impl std::ops::Deref for RequestState {
    type Target = Library;
    fn deref(&self) -> &Library {
        &self.library
    }
}

impl RequestState {
    /// Queue database work asynchronously; never occupy the HTTP runtime while
    /// waiting for a NAS connection. The permit stays with an abandoned worker.
    pub async fn db_work<T: Send + 'static>(
        &self,
        work: impl FnOnce(RequestState) -> T + Send + 'static,
    ) -> Result<T, StatusCode> {
        self.queued_db_work(false, work).await
    }

    pub async fn db_write<T: Send + 'static>(
        &self,
        work: impl FnOnce(RequestState) -> T + Send + 'static,
    ) -> Result<T, StatusCode> {
        self.queued_db_work(true, work).await
    }

    async fn queued_db_work<T: Send + 'static>(
        &self,
        write: bool,
        work: impl FnOnce(RequestState) -> T + Send + 'static,
    ) -> Result<T, StatusCode> {
        let permit = self
            .db_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| StatusCode::CONFLICT)?;
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if state
                .archives
                .revoked
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(StatusCode::CONFLICT);
            }
            // A write may have passed middleware before joining this queue.
            if write
                && (state.integrity.is_running()
                    || state.recovery.is_running()
                    || state
                        .archives
                        .busy
                        .load(std::sync::atomic::Ordering::Acquire))
            {
                return Err(StatusCode::CONFLICT);
            }
            let result = work(state.clone());
            if state
                .archives
                .revoked
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(StatusCode::CONFLICT);
            }
            Ok(result)
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    }

    /// Validate write admission after waiting for the connection, not only when
    /// entering the asynchronous queue. Closing or exclusive work can start
    /// while another reader holds this mutex.
    pub fn lock_db_for_write(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<Connection>>, StatusCode> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if self
            .archives
            .revoked
            .load(std::sync::atomic::Ordering::Acquire)
            || self
                .archives
                .busy
                .load(std::sync::atomic::Ordering::Acquire)
            || self.integrity.is_running()
            || self.recovery.is_running()
        {
            return Err(StatusCode::CONFLICT);
        }
        Ok(guard)
    }

    pub fn update_db<T, E: std::fmt::Display>(
        &self,
        unavailable: StatusCode,
        update: impl FnOnce(&Connection) -> Result<T, E>,
    ) -> Result<StatusCode, StatusCode> {
        let guard = self.lock_db_for_write()?;
        update(guard.as_ref().ok_or(unavailable)?).map_err(|err| {
            eprintln!("Database update failed: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        Ok(StatusCode::OK)
    }
    pub fn scan_status(&self) -> ScanStatus {
        if {
            let security = self.security.lock().unwrap_or_else(|e| e.into_inner());
            security.encrypted && security.password.is_none()
        } {
            return ScanStatus::default();
        }
        self.scan.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub current: Arc<Mutex<RequestState>>,
    pub token: Arc<String>,
    pub http_authority: Arc<String>,
    pub concealed: Arc<std::sync::atomic::AtomicBool>,
    pub gallery: Arc<crate::gallery::Runtime>,
    pub privacy_entry: Arc<crate::privacy::EntryState>,
    pub restore_selection: Arc<Mutex<Option<crate::backup_restore::Selection>>>,
}

impl AppState {
    pub fn new(library: Arc<Library>) -> anyhow::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("Session random source: {e}"))?;
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let concealed = library.security_status().locked;
        Ok(Self {
            concealed: Arc::new(std::sync::atomic::AtomicBool::new(concealed)),
            current: Arc::new(Mutex::new(RequestState {
                library,
                generation: 1,
                archive_slots: Arc::new(Semaphore::new(2)),
            })),
            gallery: Arc::new(crate::gallery::Runtime::default()),
            privacy_entry: Arc::new(crate::privacy::EntryState::default()),
            restore_selection: Arc::new(Mutex::new(None)),
            token: Arc::new(token),
            http_authority: Arc::new("127.0.0.1:8080".into()),
        })
    }
    pub fn snapshot(&self) -> Result<RequestState, StatusCode> {
        Ok(self
            .current
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .clone())
    }
    pub fn start_scan(&self, library: Arc<Library>) -> ScanStatus {
        let security = library.security.lock().unwrap_or_else(|e| e.into_inner());
        if library
            .archives
            .revoked
            .load(std::sync::atomic::Ordering::Acquire)
            || library.integrity.is_running()
            || library.recovery.is_running()
            || (security.encrypted && security.password.is_none())
            || library
                .archives
                .busy
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return ScanStatus::default();
        }
        let path = security.path(&library.root_dir);
        let password = security.password.clone();
        {
            let mut status = library.scan.lock().unwrap_or_else(|e| e.into_inner());
            if status.running {
                return status.clone();
            }
            *status = ScanStatus {
                running: true,
                ..ScanStatus::default()
            };
        }
        drop(security);
        let result_library = library.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(
                move || -> anyhow::Result<crate::scanner::ScanReport> {
                    let check = || {
                        anyhow::ensure!(
                            !library
                                .archives
                                .revoked
                                .load(std::sync::atomic::Ordering::Acquire),
                            "폴더가 변경되었습니다."
                        );
                        Ok(())
                    };
                    check()?;
                    let conn = crate::encryption::open(
                        &path,
                        password.as_deref().map(|s| s.as_str()),
                        false,
                    )?;
                    conn.busy_timeout(Duration::from_secs(2))?;
                    conn.pragma_update(None, "foreign_keys", "ON")?;
                    conn.pragma_update(None, "synchronous", "FULL")?;
                    crate::scanner::scan_directory_checked(
                        &library.root_dir,
                        &conn,
                        |count| {
                            library
                                .scan
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .scanned_count = count;
                        },
                        check,
                    )
                },
            )
            .await;
            let mut status = result_library
                .scan
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            status.running = false;
            match result {
                Ok(Ok(report)) => {
                    status.report =
                        Some(serde_json::to_value(report).expect("scan report serializable"))
                }
                Ok(Err(err)) => status.error = Some(err.to_string()),
                Err(err) => status.error = Some(err.to_string()),
            }
        });
        ScanStatus {
            running: true,
            ..ScanStatus::default()
        }
    }
}

pub fn validate_relative(path: &str) -> Result<(), StatusCode> {
    if path.contains(':')
        || path.contains('\0')
        || path.is_empty()
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub fn resolve_path(root: &Path, relative: &str) -> Result<PathBuf, StatusCode> {
    validate_relative(relative)?;
    let root = root.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !path.starts_with(root) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(path)
}

#[derive(Default, Deserialize)]
struct Credentials {
    session: Option<String>,
    library: Option<u64>,
}

pub async fn guard_request(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();
    if headers.get(header::HOST).and_then(|v| v.to_str().ok())
        != Some(state.http_authority.as_str())
        || headers
            .get(header::ORIGIN)
            .is_some_and(|v| v.as_bytes() != format!("http://{}", state.http_authority).as_bytes())
        || headers
            .get("sec-fetch-site")
            .is_some_and(|v| v != "same-origin" && v != "none")
    {
        return (StatusCode::FORBIDDEN, "허용되지 않은 요청 출처입니다.").into_response();
    }
    let is_api = req.uri().path().starts_with("/api/");
    if is_api {
        let credentials = match axum::extract::Query::<Credentials>::try_from_uri(req.uri()) {
            Ok(query) => query.0,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };
        let token = headers
            .get("x-vault-token")
            .and_then(|v| v.to_str().ok())
            .or(credentials.session.as_deref());
        if token != Some(state.token.as_str()) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        if req.uri().path() != "/api/session" {
            let generation = headers
                .get("x-vault-library")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .or(credentials.library);
            let immediate = state.current.try_lock().ok().map(|current| current.clone());
            let snapshot = match immediate {
                Some(current) => current,
                None => {
                    let app = state.clone();
                    match tokio::task::spawn_blocking(move || app.snapshot()).await {
                        Ok(Ok(current)) => current,
                        _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                    }
                }
            };
            if generation != Some(snapshot.generation) {
                return (
                    StatusCode::CONFLICT,
                    "폴더가 변경되었습니다. 목록을 새로 불러오세요.",
                )
                    .into_response();
            }
            let gallery_allowed = matches!(
                req.uri().path(),
                "/api/gallery"
                    | "/api/gallery/select"
                    | "/api/gallery/frame-settings"
                    | "/api/settings/language"
            ) || req.uri().path().starts_with("/api/gallery/image/");
            if state.concealed.load(std::sync::atomic::Ordering::Acquire)
                && !gallery_allowed
                && !matches!(
                    req.uri().path(),
                    "/api/security/unlock"
                        | "/api/folders/close"
                        | "/api/security/folders"
                        | "/api/security/open-folder"
                        | "/api/security/select-folder"
                )
            {
                return StatusCode::LOCKED.into_response();
            }
            // Authentication must not stat the NAS on every thumbnail/range request.
            // Closing only revokes access. It must not wait behind a worker that
            // holds the security mutex while accessing an unavailable NAS.
            let job_control = req.uri().path() == "/api/archive-security/progress"
                || req.uri().path() == "/api/archive-security/cancel"
                || req.uri().path().starts_with("/api/note-saves/");
            let locked = if matches!(
                req.uri().path(),
                "/api/folders/close" | "/api/security/lock"
            ) || (job_control
                && snapshot
                    .archives
                    .busy
                    .load(std::sync::atomic::Ordering::Acquire))
            {
                false
            } else {
                let immediate = snapshot
                    .library
                    .security
                    .try_lock()
                    .ok()
                    .map(|security| security.encrypted && security.password.is_none());
                match immediate {
                    Some(locked) => locked,
                    None => {
                        let library = snapshot.library.clone();
                        match tokio::task::spawn_blocking(move || {
                            let security =
                                library.security.lock().unwrap_or_else(|e| e.into_inner());
                            security.encrypted && security.password.is_none()
                        })
                        .await
                        {
                            Ok(locked) => locked,
                            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                        }
                    }
                }
            };
            let lock_allowed = matches!(
                req.uri().path(),
                "/api/backup/restore/clear"
                    | "/api/backup/restore/select"
                    | "/api/backup/restore"
                    | "/api/security"
                    | "/api/security/unlock"
                    | "/api/security/folders"
                    | "/api/security/open-folder"
                    | "/api/security/select-folder"
                    | "/api/folders"
                    | "/api/folders/switch"
                    | "/api/folders/add"
                    | "/api/folders/remove"
            );
            if locked && !lock_allowed && !gallery_allowed {
                return (
                    StatusCode::LOCKED,
                    "DB가 잠겨 있습니다. 비밀번호로 잠금을 해제해 주세요.",
                )
                    .into_response();
            }
            if snapshot
                .archives
                .busy
                .load(std::sync::atomic::Ordering::Acquire)
                && req.method() != axum::http::Method::GET
                && !matches!(
                    req.uri().path(),
                    "/api/archive-security/cancel" | "/api/folders/close"
                )
            {
                return (
                    StatusCode::CONFLICT,
                    "압축 파일 작업을 완료하거나 취소한 뒤 다시 시도해 주세요.",
                )
                    .into_response();
            }
            if snapshot.recovery.is_running()
                && req.method() != axum::http::Method::GET
                && !matches!(
                    req.uri().path(),
                    "/api/storage/cancel" | "/api/folders/close"
                )
            {
                return (StatusCode::CONFLICT, crate::storage_recovery::STILL_RUNNING)
                    .into_response();
            }
            if snapshot.integrity.is_running()
                && req.method() != axum::http::Method::GET
                && !matches!(
                    req.uri().path(),
                    "/api/integrity/quick/cancel" | "/api/folders/close"
                )
            {
                return (StatusCode::CONFLICT, crate::integrity::BUSY).into_response();
            }
            req.extensions_mut().insert(snapshot);
        }
    }
    let mut response = next.run(req).await;
    if is_api {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
    }
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    response
}
