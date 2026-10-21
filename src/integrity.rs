//! Read-only, bounded-header inspection. This is NOT a full integrity/hash check.
use crate::{
    archive_crypto as crypto, encryption,
    state::{AppState, Library, RequestState},
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const BUSY: &str = "빠른 검사가 진행 중이거나 이전 저장소 요청이 끝나지 않았습니다. 기다리거나 라이브러리를 닫아 주세요.";
const TIMEOUT: &str =
    "저장소 응답이 15초 동안 없어 검사를 중단했습니다. 연결을 확인한 뒤 다시 시도해 주세요.";
const LIMIT: usize = 1000;
type ApiResult = Result<Json<Job>, (StatusCode, String)>;

#[derive(Clone, Serialize)]
pub struct Finding {
    file_id: Option<i64>,
    name: String,
    state: String,
    message: String,
}
#[derive(Clone, Default, Serialize)]
pub struct Job {
    id: u64,
    running: bool,
    cancelled: bool,
    completed: bool,
    checked: u64,
    total: u64,
    ok: u64,
    issues: u64,
    unavailable: u64,
    phase: String,
    error: Option<String>,
    started_at: u64,
    finished_at: Option<u64>,
    results: Vec<Finding>,
    results_truncated: bool,
}
pub struct Runtime {
    active: AtomicBool,
    cancelled: AtomicBool,
    sequence: AtomicU64,
    job: Mutex<Job>,
    activity: Mutex<Instant>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            sequence: AtomicU64::new(0),
            job: Mutex::new(Job::default()),
            activity: Mutex::new(Instant::now()),
        }
    }
}
impl Runtime {
    pub fn is_running(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.stop(None);
    }
    fn stop(&self, error: Option<&str>) {
        self.stop_job(None, error);
    }
    fn stop_job(&self, expected: Option<u64>, error: Option<&str>) {
        let mut job = self.job.lock().unwrap_or_else(|e| e.into_inner());
        if expected.is_some_and(|id| id != job.id) {
            return;
        }
        self.cancelled.store(true, Ordering::Release);
        if job.running {
            job.running = false;
            job.cancelled = true;
            job.phase = "cancelled".into();
            job.error = error.map(str::to_owned);
            job.finished_at = Some(now());
        }
    }
    fn snapshot(&self) -> Job {
        self.job.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn touch(&self) {
        *self.activity.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }
    fn update(&self, update: impl FnOnce(&mut Job)) {
        let mut job = self.job.lock().unwrap_or_else(|e| e.into_inner());
        if job.running && !self.cancelled.load(Ordering::Acquire) {
            update(&mut job);
        }
    }
}
struct Active(Arc<Library>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.integrity.active.store(false, Ordering::Release);
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn live(library: &Library) -> bool {
    !library.integrity.cancelled.load(Ordering::Acquire)
        && !library.archives.revoked.load(Ordering::Acquire)
}
fn finding(id: Option<i64>, name: &str, state: &str, message: &str) -> Finding {
    Finding {
        file_id: id,
        name: name.into(),
        state: state.into(),
        message: message.into(),
    }
}
fn add(job: &mut Job, item: Finding) {
    if item.state == "unavailable" {
        job.unavailable += 1;
    } else {
        job.issues += 1;
    }
    if job.results.len() < LIMIT {
        job.results.push(item);
    } else {
        job.results_truncated = true;
    }
}

pub async fn status(Extension(state): Extension<RequestState>) -> Json<Job> {
    Json(state.integrity.snapshot())
}
#[derive(Deserialize)]
pub struct CancelInput {
    id: u64,
}
pub async fn cancel(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(input): Json<CancelInput>,
) -> ApiResult {
    let current = app
        .current
        .lock()
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, BUSY.into()))?;
    if current.generation != state.generation {
        return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
    }
    if current.integrity.snapshot().id == input.id {
        current.integrity.cancel();
    }
    Ok(Json(current.integrity.snapshot()))
}
pub async fn start(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    let password = {
        let current = app
            .current
            .lock()
            .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, BUSY.into()))?;
        if current.generation != state.generation {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        let security = current
            .security
            .lock()
            .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, BUSY.into()))?;
        if app.concealed.load(Ordering::Acquire)
            || current.root_dir.as_os_str().is_empty()
            || (security.encrypted && security.password.is_none())
        {
            return Err((StatusCode::LOCKED, "DB가 잠겨 있습니다.".into()));
        }
        encryption::require_idle(&current).map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
        let runtime = &current.integrity;
        if runtime.active.swap(true, Ordering::AcqRel) {
            return Err((StatusCode::CONFLICT, BUSY.into()));
        }
        runtime.cancelled.store(false, Ordering::Release);
        runtime.touch();
        *runtime.job.lock().unwrap() = Job {
            id: runtime.sequence.fetch_add(1, Ordering::AcqRel) + 1,
            running: true,
            phase: "db".into(),
            started_at: now(),
            ..Default::default()
        };
        security.password.clone()
    };
    let initial = state.integrity.snapshot();
    let library = state.library.clone();
    let watched = library.integrity.clone();
    let id = initial.id;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let job = watched.snapshot();
            if job.id != id || !job.running {
                break;
            }
            if watched.activity.lock().unwrap().elapsed() >= Duration::from_secs(15) {
                watched.stop_job(Some(id), Some(TIMEOUT));
                break;
            }
        }
    });
    tokio::spawn(async move {
        let active = Active(library.clone());
        // Drain a write that passed its busy check before this inspection began.
        // Newly queued writes observe active=true and are rejected. Release the
        // permit immediately so ordinary reads remain available during inspection.
        let Ok(permit) = library.db_slots.clone().acquire_owned().await else {
            library.integrity.cancel();
            return;
        };
        drop(permit);
        if !live(&library) {
            return;
        }
        tokio::task::spawn_blocking(move || {
            let _active = active;
            let result = inspect(&library, password.as_deref().map(|p| p.as_str()));
            library.integrity.update(|job| {
                job.running = false;
                job.finished_at = Some(now());
                match result {
                    Ok(()) => {
                        job.completed = true;
                        job.phase = "done".into();
                    }
                    Err(message) => {
                        job.phase = "error".into();
                        job.error = Some(message);
                    }
                }
            });
        });
    });
    Ok(Json(initial))
}

struct Entry {
    id: i64,
    name: String,
    kind: String,
    size: i64,
    stored: Option<String>,
    state: Option<String>,
    hash: Option<String>,
    restore_name: String,
}
fn inspect(library: &Library, password: Option<&str>) -> Result<(), String> {
    // SELECT metadata only: no thumbnail BLOBs, full DB scan pragma, reindex or repair.
    let (entries, expected_key, orphans) = {
        let guard = library
            .conn
            .lock()
            .map_err(|_| "DB 등록 정보를 읽지 못했습니다.".to_owned())?;
        if !live(library) {
            return Ok(());
        }
        let conn = guard.as_ref().ok_or("DB가 잠겨 있습니다.")?;
        let read = || -> rusqlite::Result<_> {
            let mut query = conn.prepare("SELECT m.id,m.rel_path,m.file_type,m.file_size,a.encrypted_path,a.state,a.sha256,COALESCE(a.restore_name,m.rel_path) FROM media_files m LEFT JOIN archive_store a ON a.file_id=m.id ORDER BY m.id")?;
            let entries = query
                .query_map([], |r| {
                    Ok(Entry {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        kind: r.get(2)?,
                        size: r.get(3)?,
                        stored: r.get(4)?,
                        state: r.get(5)?,
                        hash: r.get(6)?,
                        restore_name: r.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let expected: Option<String> = conn
                .query_row(
                    "SELECT value FROM library_info WHERE key='archive_key_sha256'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let orphans: i64 = conn.query_row("SELECT count(*) FROM archive_store a LEFT JOIN media_files m ON m.id=a.file_id WHERE m.id IS NULL", [], |r| r.get(0))?;
            Ok((entries, expected, orphans))
        };
        read().map_err(|_| "DB 등록 정보를 읽지 못했습니다.".to_owned())?
    };
    if !live(library) {
        return Ok(());
    }
    library.integrity.touch();
    library.integrity.update(|j| {
        j.total = entries.len() as u64;
        j.phase = "files".into();
        if orphans > 0 {
            add(
                j,
                finding(
                    None,
                    "DB",
                    "db_error",
                    "미디어와 연결되지 않은 암호화 파일 등록 정보가 있습니다.",
                ),
            );
        }
    });
    // Confirm the root and current DB are accessible before interpreting absent media.
    if !storage_available(library) {
        return Err(
            "저장소에 접근할 수 없어 검사하지 못했습니다. NAS 연결과 권한을 확인해 주세요.".into(),
        );
    }
    let needs_key = expected_key.is_some() || entries.iter().any(|e| e.stored.is_some());
    let key = if needs_key {
        match check_key(library, expected_key.as_deref(), password) {
            Ok(key) => Some(key),
            Err(item) => {
                library.integrity.update(|j| add(j, item));
                None
            }
        }
    } else {
        None
    };
    for entry in entries {
        if !live(library) {
            return Ok(());
        }
        library.integrity.touch();
        let result = check_entry(library, &entry, key.as_deref());
        if !live(library) {
            return Ok(());
        }
        library.integrity.update(|j| {
            j.checked += 1;
            if let Some(item) = result {
                add(j, item);
            } else {
                j.ok += 1;
            }
        });
        library.integrity.touch();
    }
    Ok(())
}
fn storage_available(library: &Library) -> bool {
    let path = {
        let security = library.security.lock().unwrap_or_else(|e| e.into_inner());
        security.path(&library.root_dir)
    };
    fs::metadata(&library.root_dir).is_ok_and(|m| m.is_dir())
        && fs::metadata(path).is_ok_and(|m| m.is_file())
}
fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, &'static str> {
    crate::state::validate_relative(relative)
        .map_err(|_| "라이브러리 밖의 경로이거나 잘못된 경로입니다.")?;
    Ok(root.join(relative))
}
fn within_root(root: &Path, path: &Path) -> Result<PathBuf, anyhow::Error> {
    let actual = path.canonicalize()?;
    anyhow::ensure!(
        actual.starts_with(root),
        "라이브러리 밖의 경로이거나 잘못된 경로입니다."
    );
    Ok(actual)
}
fn io_kind(library: &Library, error: &io::Error) -> &'static str {
    // Windows network errors may be mapped to NotFound; never label them missing.
    let network = matches!(
        error.raw_os_error(),
        Some(53 | 54 | 59 | 64 | 67 | 121 | 1231 | 1232 | 1236 | 2250)
    );
    if !network && error.kind() == io::ErrorKind::NotFound && storage_available(library) {
        "missing"
    } else {
        "unavailable"
    }
}
fn path_failure(library: &Library, id: Option<i64>, name: &str, error: &anyhow::Error) -> Finding {
    if let Some(io) = error.chain().find_map(|e| e.downcast_ref::<io::Error>()) {
        let state = io_kind(library, io);
        finding(
            id,
            name,
            state,
            if state == "missing" {
                "파일을 찾을 수 없습니다. 이동 여부와 저장소 연결을 확인해 주세요."
            } else {
                "파일에 접근할 수 없습니다. 저장소 연결과 권한을 확인해 주세요."
            },
        )
    } else {
        finding(
            id,
            name,
            "invalid_path",
            "라이브러리 밖의 경로이거나 잘못된 경로입니다.",
        )
    }
}
fn check_key(
    library: &Library,
    expected: Option<&str>,
    password: Option<&str>,
) -> Result<crypto::Secret, Finding> {
    use sha2::{Digest, Sha256};
    let name = crypto::KEY_PATH;
    let path = within_root(&library.root_dir, &library.root_dir.join(name))
        .map_err(|e| path_failure(library, None, "vault.key", &e))?;
    let bytes = (|| -> anyhow::Result<Vec<u8>> {
        use std::io::Read;
        let mut file = crypto::open_source(&path)?;
        anyhow::ensure!(file.metadata()?.len() == 96, "key size");
        let mut bytes = vec![0; 96];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    })()
    .map_err(|e| {
        if e.chain().any(|e| e.downcast_ref::<io::Error>().is_some()) {
            path_failure(library, None, "vault.key", &e)
        } else {
            finding(
                None,
                "vault.key",
                "key_error",
                "키 파일 형식이나 크기가 올바르지 않습니다.",
            )
        }
    })?;
    if expected.is_some_and(|expected| crypto::hex(&Sha256::digest(&bytes)) != expected) {
        return Err(finding(
            None,
            "vault.key",
            "key_error",
            "vault.key가 이 DB의 키와 일치하지 않습니다.",
        ));
    }
    let key = crypto::read_key(&bytes, password.unwrap_or("")).map_err(|_| {
        finding(
            None,
            "vault.key",
            "key_error",
            "키 파일을 현재 비밀번호로 열 수 없습니다.",
        )
    })?;
    if library
        .archives
        .key
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| **active != *key)
    {
        return Err(finding(
            None,
            "vault.key",
            "key_error",
            "키 파일과 현재 잠금 해제된 미디어 키가 일치하지 않습니다.",
        ));
    }
    Ok(key)
}
fn check_entry(library: &Library, entry: &Entry, key: Option<&[u8; 32]>) -> Option<Finding> {
    let issue = |state, message| finding(Some(entry.id), &entry.name, state, message);
    let ready = entry.state.as_deref() == Some("ready");
    let relative = if ready {
        entry.stored.as_deref().unwrap_or("")
    } else {
        &entry.name
    };
    let path = match checked_path(&library.root_dir, relative) {
        Ok(path) => path,
        Err(message) => return Some(issue("invalid_path", message)),
    };
    let path = match within_root(&library.root_dir, &path) {
        Ok(path) => path,
        Err(e) => return Some(path_failure(library, Some(entry.id), &entry.name, &e)),
    };
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            return Some(path_failure(
                library,
                Some(entry.id),
                &entry.name,
                &e.into(),
            ))
        }
    };
    if !meta.is_file() {
        return Some(issue("mismatch", "등록된 경로가 일반 파일이 아닙니다."));
    }
    if !ready {
        if entry.size < 0 || meta.len() != entry.size as u64 {
            return Some(issue(
                "mismatch",
                "파일 크기가 DB 등록 정보와 다릅니다. 외부에서 변경되었을 수 있습니다.",
            ));
        }
        if entry.state.as_deref() == Some("pending") {
            return Some(issue(
                "pending",
                "암호화가 완료되지 않은 항목입니다. 원본 상태를 확인해 주세요.",
            ));
        }
        return None;
    }
    let Some(key) = key else {
        return Some(issue(
            "unavailable",
            "키 확인에 실패하여 암호화 파일의 헤더를 검사하지 못했습니다.",
        ));
    };
    // Opening authenticates only the envelope, metadata and boundary blocks.
    // Do not call verify(), read_to_end(), extract images, or compare a computed full hash.
    let reader = match crypto::Reader::open(&path, key) {
        Ok(reader) => reader,
        Err(e) => {
            if let Some(io) = e.chain().find_map(|e| e.downcast_ref::<io::Error>()) {
                if io.kind() != io::ErrorKind::UnexpectedEof
                    && io.to_string() != "암호화 파일 인증 실패"
                {
                    return Some(path_failure(library, Some(entry.id), &entry.name, &e));
                }
            }
            return Some(issue("mismatch","암호화 파일의 헤더·끝부분·길이를 확인하지 못했습니다. 키 불일치나 손상 가능성이 있습니다."));
        }
    };
    let metadata = &reader.metadata;
    let format = if entry.kind == "NOTE" && reader.standalone_note {
        Some("json")
    } else if entry.kind == "VIDEO" {
        crypto::video::format(&entry.restore_name)
    } else if entry.kind == "IMAGE" {
        crypto::image::format(&entry.restore_name)
    } else if crate::archive::is_tar_zstd(Path::new(&entry.restore_name)) {
        Some("tar.zst")
    } else if entry.restore_name.to_ascii_lowercase().ends_with(".zip") {
        Some("zip")
    } else {
        None
    };
    if metadata.name != entry.restore_name
        || entry.size < 0
        || (if entry.kind == "NOTE" {
            reader
                .note_index()
                .ok()
                .and_then(|i| i.notes.first())
                .map(|n| n.length - 16)
        } else {
            Some(metadata.size)
        }) != Some(entry.size as u64)
        || Some(metadata.format.as_str()) != format
        || entry.hash.as_deref() != Some(metadata.sha256.as_str())
    {
        return Some(issue(
            "mismatch",
            "암호화 메타데이터와 DB의 파일명·크기·형식·저장 해시가 일치하지 않습니다.",
        ));
    }
    None
}

#[cfg(test)]
#[path = "integrity_tests.rs"]
mod tests;
