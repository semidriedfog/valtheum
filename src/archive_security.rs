#[path = "notes.rs"]
pub mod notes;
#[path = "archive_page_delete.rs"]
pub mod pages;
#[path = "archive_rename.rs"]
pub mod rename;
#[path = "encrypted_video.rs"]
pub mod video_http;
use crate::{
    archive, archive_crypto as crypto,
    state::{AppState, Library, RequestState},
};
use anyhow::{ensure, Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{
    fs,
    path::Path as FsPath,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
#[derive(Clone, Default, Serialize)]
pub struct Job {
    pub job_id: Option<String>,
    pub running: bool,
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
    pub completed: bool,
    pub result_id: Option<i64>,
    pub request_id: Option<String>,
    pub batch: Option<batch::BatchProgress>,
}
#[derive(Default)]
struct ReaderCache {
    epoch: u64,
    entries: Vec<(String, archive::ZipReader)>,
}

pub struct Runtime {
    pub key: Mutex<Option<crypto::Secret>>,
    pub revoked: Arc<AtomicBool>,
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
    pub job: Mutex<Job>,
    history: Mutex<std::collections::VecDeque<Job>>,
    requests: Mutex<std::collections::HashMap<String, (String, Job)>>,
    cache: Mutex<ReaderCache>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            key: Mutex::new(None),
            revoked: Arc::new(AtomicBool::new(false)),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            job: Mutex::new(Job::default()),
            history: Mutex::new(Default::default()),
            requests: Mutex::new(Default::default()),
            cache: Mutex::new(ReaderCache::default()),
        }
    }
}
impl Runtime {
    pub(crate) fn begin(&self, mut next: Job) -> String {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string();
        next.job_id = Some(id.clone());
        next.running = true;
        let mut current = self.job.lock().unwrap();
        if let Some(request_id) = &current.request_id {
            if let Some((_, saved)) = self.requests.lock().unwrap().get_mut(request_id) {
                *saved = current.clone();
            }
        }
        if current.job_id.is_some() {
            let mut history = self.history.lock().unwrap();
            history.push_back(current.clone());
            while history.len() > 128 {
                history.pop_front();
            }
        }
        self.cancel.store(false, Ordering::Release);
        *current = next;
        id
    }
    fn lookup(&self, id: &str) -> Option<Job> {
        let current = self.job.lock().unwrap();
        if current.job_id.as_deref() == Some(id) {
            return Some(current.clone());
        }
        self.history
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.job_id.as_deref() == Some(id))
            .cloned()
            .or_else(|| {
                self.requests
                    .lock()
                    .unwrap()
                    .values()
                    .find(|(_, job)| job.job_id.as_deref() == Some(id))
                    .map(|(_, job)| job.clone())
            })
    }
    fn lookup_request(&self, id: &str) -> Option<Job> {
        let current = self.job.lock().unwrap();
        if current.request_id.as_deref() == Some(id) {
            return Some(current.clone());
        }
        self.requests
            .lock()
            .unwrap()
            .get(id)
            .map(|(_, job)| job.clone())
    }
    pub fn clear_cache(&self) {
        let retired = {
            let mut cache = self.cache.lock().unwrap();
            cache.epoch = cache.epoch.wrapping_add(1);
            std::mem::take(&mut cache.entries)
        };
        // File-handle cleanup may block on a NAS, but must not hold the cache lock.
        drop(retired);
    }
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
        self.cancel.store(true, Ordering::Release);
        self.key.lock().unwrap().take();
    }
    pub fn retire(&self) {
        self.revoke();
        self.clear_cache();
    }
    pub fn take_for_transition(&self) -> Self {
        let key = self.key.lock().unwrap().take();
        self.retire();
        let next = Self::default();
        *next.key.lock().unwrap() = key;
        next
    }
    fn secret(&self) -> Result<crypto::Secret> {
        ensure!(
            !self.revoked.load(Ordering::Acquire),
            "이전 라이브러리 요청입니다."
        );
        self.key
            .lock()
            .unwrap()
            .clone()
            .context("미디어 키가 잠겨 있습니다.")
    }
    fn progress(&self, phase: &str, done: u64, total: u64) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Acquire) && !self.revoked.load(Ordering::Acquire),
            "작업이 취소되었습니다. 원본은 보존됩니다."
        );
        let mut j = self.job.lock().unwrap();
        j.phase = phase.into();
        j.done = done;
        j.total = total;
        Ok(())
    }
}
fn expected_key(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM library_info WHERE key='archive_key_sha256'",
            [],
            |r| r.get(0),
        )
        .optional()?)
}
pub fn check_key(conn: &Connection, bytes: &[u8]) -> Result<()> {
    use sha2::{Digest, Sha256};
    if let Some(expected) = expected_key(conn)? {
        ensure!(
            crypto::hex(&Sha256::digest(bytes)) == expected,
            "vault.key가 이 DB의 키와 다릅니다. 올바른 키 백업을 복원해 주세요."
        );
    }
    Ok(())
}
pub fn unlock(library: &Library, conn: &Connection, password: &str) -> Result<()> {
    let required = expected_key(conn)?.is_some()
        || conn.query_row("SELECT EXISTS(SELECT 1 FROM archive_store)", [], |r| {
            r.get::<_, bool>(0)
        })?;
    let path = library.root_dir.join(crypto::KEY_PATH);
    if required || path.try_exists()? {
        let path = crypto::safe_child(&library.root_dir, crypto::KEY_PATH)?;
        ensure!(
            path.metadata()?.len() == 96,
            "vault.key가 없거나 손상되었습니다."
        );
        let bytes = fs::read(path)?;
        check_key(conn, &bytes)?;
        let key = crypto::read_key(&bytes, password)?;
        let mut slot = library.archives.key.lock().unwrap();
        ensure!(
            !library.archives.revoked.load(Ordering::Acquire),
            "이전 라이브러리 요청입니다."
        );
        *slot = Some(key);
    }
    Ok(())
}
fn initialize(library: &Library) -> Result<()> {
    let password = {
        let security = library.security.lock().unwrap();
        ensure!(security.encrypted, "먼저 DB 암호화를 설정해 주세요.");
        security
            .password
            .clone()
            .context("DB 잠금을 해제해 주세요.")?
    }; // Never retain the authentication lock during filesystem or DB I/O.
    library.archives.progress("미디어 키 준비", 0, 0)?;
    let guard = library.conn.lock().unwrap();
    let conn = guard.as_ref().context("DB가 잠겼습니다.")?;
    let folder = library.root_dir.join(".valtheum");
    fs::create_dir_all(&folder)?;
    ensure!(
        folder
            .canonicalize()?
            .starts_with(library.root_dir.canonicalize()?),
        "외부 저장 경로"
    );
    let path = crypto::safe_child(&library.root_dir, crypto::KEY_PATH)?;
    if !path.try_exists()? {
        ensure!(
            expected_key(conn)?.is_none()
                && !conn.query_row("SELECT EXISTS(SELECT 1 FROM archive_store)", [], |r| r
                    .get::<_, bool>(0))?,
            "필요한 vault.key가 없습니다. 키 백업을 복원해 주세요."
        );
        let (bytes, _) = crypto::make_key(&password)?;
        let temp = crate::persistence::temporary_sibling(&path)?;
        crate::persistence::atomic_write(&temp, &bytes)?;
        let result = crypto::publish_new(&temp, &path);
        let _ = fs::remove_file(temp);
        result?;
    }
    let needs_unlock = library.archives.key.lock().unwrap().is_none();
    if needs_unlock {
        unlock(library, conn, &password)?;
    } else {
        check_key(conn, &fs::read(&path)?)?;
    }
    use sha2::{Digest, Sha256};
    let hash = crypto::hex(&Sha256::digest(fs::read(&path)?));
    conn.execute(
        "INSERT OR IGNORE INTO library_info(key,value) VALUES('archive_key_sha256',?1)",
        [hash],
    )?;
    Ok(())
}
pub fn physical_path(conn: &Connection, id: i64, logical: &str) -> Result<String> {
    Ok(conn
        .query_row(
            "SELECT encrypted_path FROM archive_store WHERE file_id=?1 AND state='ready'",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| logical.into()))
}
pub fn protected_paths(conn: &Connection) -> Result<Vec<String>> {
    Ok(conn
        .prepare("SELECT m.rel_path FROM media_files m JOIN archive_store a ON a.file_id=m.id")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
fn row(conn: &Connection, id: i64) -> Result<(String, Option<String>, Option<String>)> {
    Ok(conn.query_row("SELECT m.rel_path,a.encrypted_path,a.state FROM media_files m LEFT JOIN archive_store a ON a.file_id=m.id WHERE m.id=?1 AND m.file_type IN ('ARCHIVE','VIDEO','IMAGE','NOTE')",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?)
}
fn logical(library: &Library, id: i64) -> Result<(String, Option<String>, Option<String>)> {
    let g = library.conn.lock().unwrap();
    row(g.as_ref().context("DB 잠김")?, id)
}
fn ready(library: &Library, id: i64) -> Result<(String, String, crypto::Secret)> {
    let (name, path, state) = logical(library, id)?;
    ensure!(
        state.as_deref() == Some("ready"),
        "암호화 완료된 파일이 아닙니다."
    );
    Ok((name, path.unwrap(), library.archives.secret()?))
}
pub fn convert(library: &Library, id: i64) -> Result<()> {
    initialize(library)?;
    let key = library.archives.secret()?;
    let (name, existing, state) = logical(library, id)?;
    ensure!(
        state.as_deref() != Some("ready"),
        "이미 암호화된 파일입니다."
    );
    ensure!(
        name.to_ascii_lowercase().ends_with(".zip")
            || archive::is_tar_zstd(FsPath::new(&name))
            || crypto::video::format(&name).is_some()
            || crypto::image::format(&name).is_some(),
        "ZIP·tar.zst와 지원되는 영상·이미지만 암호화할 수 있습니다."
    );
    let video = crypto::video::format(&name).is_some();
    let image = crypto::image::format(&name).is_some();
    let folder = if image {
        ".valtheum/images"
    } else if video {
        ".valtheum/videos"
    } else {
        ".valtheum/archives"
    };
    let extension = if image {
        "vti"
    } else if video {
        "vtv"
    } else {
        "vta"
    };
    fs::create_dir_all(library.root_dir.join(folder))?;
    let rel = existing.unwrap_or(format!(
        "{folder}/{}.{extension}",
        crypto::hex(&crypto::random::<16>()?)
    ));
    let target = crypto::safe_child(&library.root_dir, &rel)?;
    {
        let g = library.conn.lock().unwrap();
        g.as_ref().context("DB 잠김")?.execute("INSERT OR IGNORE INTO archive_store(file_id,encrypted_path,state) VALUES(?1,?2,'pending')",rusqlite::params![id,rel])?;
    }
    let metadata = if target.try_exists()? {
        crypto::verify(&target, &key, |n| {
            library.archives.progress("중단 작업 검증", n, 0)
        })?
    } else {
        let source_path = crypto::safe_child(&library.root_dir, &name)?;
        let mut source = crypto::open_source(&source_path)?;
        ensure!(
            video || image || !archive::list_archive_images(&source_path)?.is_empty(),
            "볼 수 있는 이미지가 없는 압축 파일입니다."
        );
        let part = crypto::safe_child(&library.root_dir, &format!("{rel}.part"))?;
        if part.try_exists()? {
            let safe = crypto::safe_child(&library.root_dir, &format!("{rel}.part"))?;
            fs::remove_file(safe)?;
        }
        let m = crypto::encrypt(&mut source, &part, &name, &key, |phase, n, total| {
            library.archives.progress(phase, n, total)
        })?;
        library.archives.progress("전환", 0, 0)?;
        crypto::publish_new(&part, &target)?;
        m
    };
    ensure!(metadata.name == name, "중단 파일의 원래 경로가 다릅니다.");
    let g = library.conn.lock().unwrap();
    g.as_ref().context("DB 잠김")?.execute(
        "UPDATE archive_store SET state='ready',sha256=?2 WHERE file_id=?1",
        rusqlite::params![id, metadata.sha256],
    )?;
    Ok(())
}
pub fn cleanup(library: &Library, id: i64) -> Result<()> {
    let (name, rel, key) = ready(library, id)?;
    library.archives.clear_cache();
    let path = crypto::safe_child(&library.root_dir, &name)?;
    if !path.try_exists()? {
        return Ok(());
    }
    let mut source = crypto::open_removable(&path)?;
    let encrypted_path = crypto::safe_child(&library.root_dir, &rel)?;
    // Keep the verified recovery copy protected against replacement or writes
    // until deleting the original has committed.
    let encrypted_guard = crypto::open_source(&encrypted_path)?;
    let m = crypto::verify(&encrypted_path, &key, |n| {
        library.archives.progress("암호화 사본 검증", n, 0)
    })?;
    ensure!(
        m.name == rename::restoration_name(library, id)?
            && crypto::hash_file(&mut source, |n| library.archives.progress(
                "원본 확인",
                n,
                m.size
            ))? == m.sha256,
        "원본이 변경되어 삭제하지 않았습니다. 원본 내용을 확인한 뒤 라이브러리 밖으로 옮겨 보관하고 다시 시도해 주세요."
    );
    crypto::delete_open(source)?;
    drop(encrypted_guard);
    Ok(())
}
pub fn with_archive<T>(
    library: &Library,
    name: &str,
    entry: Option<&str>,
    work: impl FnOnce(&mut archive::ZipReader) -> Result<T>,
    tar: impl FnOnce(archive::ArchiveReader) -> Result<T>,
) -> Result<T> {
    crate::state::validate_relative(name).map_err(|_| anyhow::anyhow!("잘못된 경로"))?;
    let (rel, encrypted, expected_name) = {
        let g = library.conn.lock().unwrap();
        let conn = g.as_ref().context("DB가 잠겼습니다.")?;
        let mapped:Option<(String,String)>=conn.query_row("SELECT a.encrypted_path,COALESCE(a.restore_name,m.rel_path) FROM archive_store a JOIN media_files m ON m.id=a.file_id WHERE m.rel_path=?1 AND a.state='ready'",[name],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        (
            mapped
                .as_ref()
                .map(|m| m.0.clone())
                .unwrap_or_else(|| name.into()),
            mapped.is_some(),
            mapped.map(|m| m.1).unwrap_or_else(|| name.into()),
        )
    };
    ensure!(
        !library.archives.revoked.load(Ordering::Acquire),
        "라이브러리가 변경되었습니다."
    );
    let format = if archive::is_tar_zstd(FsPath::new(name)) {
        "tar.zst"
    } else if name.to_ascii_lowercase().ends_with(".zip") {
        "zip"
    } else {
        anyhow::bail!("unsupported archive: ZIP과 tar.zst만 지원합니다.")
    };
    let path = crypto::safe_child(&library.root_dir, &rel)?;
    let cache_key = format!("{}:{}", rel, encrypted);
    let open = || -> Result<archive::ArchiveReader> {
        if encrypted {
            let key = library.archives.secret()?;
            let reader = crypto::Reader::open(&path, &key)?;
            ensure!(
                reader.metadata.name == expected_name && reader.metadata.format == format,
                "압축 파일 메타데이터 불일치"
            );
            Ok(Box::new(
                reader.with_revocation(library.archives.revoked.clone()),
            ))
        } else {
            Ok(Box::new(std::io::BufReader::new(crypto::open_source(
                &path,
            )?)))
        }
    };
    let result = if format == "zip" {
        // Take ownership of one reader. Opening, decoding and NAS I/O happen
        // outside the cache mutex, so reconnect can invalidate it immediately.
        let (epoch, cached) = {
            let mut cache = library.archives.cache.lock().unwrap();
            let cached = cache
                .entries
                .iter()
                .position(|(key, _)| key == &cache_key)
                .map(|i| cache.entries.remove(i).1);
            (cache.epoch, cached)
        };
        let mut z = if let Some(z) = cached {
            z
        } else {
            let mut z = zip::ZipArchive::new(open()?)?;
            archive::list_zip_images(&mut z)?;
            z
        };
        let result = work(&mut z);
        let mut returned = Some(z);
        let mut evicted = None;
        if encrypted && result.is_ok() {
            let mut cache = library.archives.cache.lock().unwrap();
            if cache.epoch == epoch && !library.archives.revoked.load(Ordering::Acquire) {
                cache.entries.push((cache_key, returned.take().unwrap()));
                if cache.entries.len() > 4 {
                    evicted = Some(cache.entries.remove(0));
                }
            }
        }
        // A reader from an earlier epoch must never repopulate the new cache.
        drop(returned);
        drop(evicted);
        result
    } else {
        tar(open()?)
    };
    let _ = entry;
    ensure!(
        !library.archives.revoked.load(Ordering::Acquire),
        "라이브러리가 변경되었습니다."
    );
    result
}
pub fn list(library: &Library, name: &str) -> Result<Vec<String>> {
    with_archive(
        library,
        name,
        None,
        archive::list_zip_images,
        archive::list_tar_reader,
    )
}
pub fn image(library: &Library, name: &str, entry: &str) -> Result<Vec<u8>> {
    with_archive(
        library,
        name,
        Some(entry),
        |z| archive::read_zip_file(z, entry),
        |r| archive::read_tar_reader(r, entry),
    )
}
type ApiResult = Result<Json<serde_json::Value>, (StatusCode, String)>;
fn error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}
pub async fn status(Extension(state): Extension<RequestState>) -> ApiResult {
    state.db_work(move |state| {
    let g = state.conn.lock().unwrap();
    let conn = g.as_ref().ok_or_else(|| error("DB 잠김"))?;
    let mut statement=conn.prepare("SELECT m.id,m.rel_path,m.file_size,a.state,a.encrypted_path,COALESCE(m.is_trash,0),m.display_name,m.file_type FROM media_files m LEFT JOIN archive_store a ON a.file_id=m.id WHERE m.file_type IN ('ARCHIVE','VIDEO','IMAGE') ORDER BY COALESCE(m.display_name,m.rel_path) COLLATE NOCASE").map_err(error)?;
    let items = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(error)?;
    let files:Vec<_>=items.into_iter().filter(|(_,n,_,_,_,_,_,_)|n.to_ascii_lowercase().ends_with(".zip")||archive::is_tar_zstd(FsPath::new(n))||crypto::video::format(n).is_some()||crypto::image::format(n).is_some()).map(|(id,name,size,status,_,trash,display_name,file_type)|{let original=state.root_dir.join(&name).is_file();serde_json::json!({"id":id,"name":name,"display_name":display_name,"file_type":file_type,"size":size,"state":status.unwrap_or_else(||"plain".into()),"original":original,"trash":trash})}).collect();
    Ok(Json(
        serde_json::json!({"files":files,"key_exists":state.root_dir.join(crypto::KEY_PATH).is_file(),"job":state.archives.job.lock().unwrap().clone()}),
    ))

    }).await.map_err(|status| (status, "데이터베이스를 사용할 수 없습니다.".into()))?
}
// Progress and cancellation access only bounded in-memory job state.
#[derive(Default, serde::Deserialize)]
pub struct JobQuery {
    job_id: Option<String>,
    request_id: Option<String>,
}
pub async fn progress_status(
    Extension(state): Extension<RequestState>,
    Query(query): Query<JobQuery>,
) -> ApiResult {
    if let Some(id) = query.request_id {
        validate_request_id(&id)?;
        return Ok(Json(match state.archives.lookup_request(&id) {
            Some(job) => serde_json::json!({"job":job}),
            None => serde_json::json!({"status":"unknown"}),
        }));
    }
    let job = match query.job_id {
        Some(id) => state.archives.lookup(&id).ok_or((
            StatusCode::NOT_FOUND,
            "작업 결과를 찾을 수 없습니다. 목록을 새로 확인해 주세요.".into(),
        ))?,
        None => state.archives.job.lock().unwrap().clone(),
    };
    Ok(Json(serde_json::json!({"job":job})))
}
pub async fn cancel(
    Extension(state): Extension<RequestState>,
    Query(query): Query<JobQuery>,
) -> ApiResult {
    let id = query
        .job_id
        .ok_or((StatusCode::BAD_REQUEST, "작업 번호가 필요합니다.".into()))?;
    let job = state.archives.job.lock().unwrap();
    if job.job_id.as_deref() != Some(id.as_str()) {
        return Err((StatusCode::CONFLICT, "취소할 작업이 변경되었습니다.".into()));
    }
    if job.running {
        state.archives.cancel.store(true, Ordering::Release);
    }
    Ok(Json(
        serde_json::json!({"status":if job.running {"cancelling"} else {"finished"}}),
    ))
}
pub(crate) fn request_identity(
    headers: &HeaderMap,
    payload: impl Serialize,
) -> Result<Option<(String, String)>, (StatusCode, String)> {
    let Some(id) = headers.get("X-Vault-Request") else {
        return Ok(None);
    };
    let id = id.to_str().map_err(error)?.to_owned();
    validate_request_id(&id)?;
    use sha2::{Digest, Sha256};
    let fingerprint = crypto::hex(&Sha256::digest(
        serde_json::to_vec(&payload).map_err(error)?,
    ));
    Ok(Some((id, fingerprint)))
}
fn validate_request_id(id: &str) -> Result<(), (StatusCode, String)> {
    ensure_api(
        id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        "저장 요청 번호가 올바르지 않습니다.",
    )
}

// Serialize registration with all other admissions. A replay only returns the
// original job; it never spawns another worker, even after that job completed.
async fn admit_request(
    app: &AppState,
    state: &RequestState,
    mut job: Job,
    request: Option<(String, String)>,
    work: Box<dyn FnOnce(&RequestState) -> Result<()> + Send>,
) -> Result<(String, bool), (StatusCode, String)> {
    let app = app.clone();
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let current = app.current.lock().unwrap();
        ensure_api(
            current.generation == state.generation && !app.concealed.load(Ordering::Acquire),
            "폴더가 변경되었습니다.",
        )?;
        if let Some((id, fingerprint)) = &request {
            if let Some((previous, job)) = state.archives.requests.lock().unwrap().get(id) {
                ensure_api(
                    previous == fingerprint,
                    "같은 요청 번호로 다른 작업을 실행할 수 없습니다.",
                )?;
                return Ok((job.job_id.clone().unwrap(), false));
            }
            ensure_api(
                state.archives.requests.lock().unwrap().len() < 100_000,
                "작업 기록 한도에 도달했습니다. 라이브러리를 닫았다가 다시 열어 주세요.",
            )?;
            job.request_id = Some(id.clone());
        }
        {
            let security = state.security.lock().unwrap();
            ensure_api(
                security.encrypted && security.password.is_some(),
                "먼저 DB를 암호화하고 잠금을 해제해 주세요.",
            )?;
        }
        crate::encryption::require_idle(&state).map_err(error)?;
        ensure_api(
            !state.archives.busy.swap(true, Ordering::AcqRel),
            "이미 작업 중입니다.",
        )?;
        let id = state.archives.begin(job);
        if let Some((request_id, fingerprint)) = request {
            let job = state.archives.job.lock().unwrap().clone();
            state
                .archives
                .requests
                .lock()
                .unwrap()
                .insert(request_id, (fingerprint, job));
        }
        drop(current);
        {
            // Launch before acknowledging admission, even if the HTTP caller disconnected.
            tokio::task::spawn_blocking(move || {
                let result = work(&state);
                let mut job = state.archives.job.lock().unwrap();
                job.running = false;
                job.completed = result.is_ok() && job.batch.as_ref().is_none_or(|b| b.failed == 0);
                job.error = result.err().map(|e| format!("{e:#}"));
                state.archives.busy.store(false, Ordering::Release);
            });
        }
        Ok((id, true))
    })
    .await
    .map_err(error)?
}
async fn start_request(
    app: &AppState,
    state: &RequestState,
    job: Job,
    request: Option<(String, String)>,
    work: impl FnOnce(&RequestState) -> Result<()> + Send + 'static,
) -> ApiResult {
    let (job_id, _) = admit_request(app, state, job, request, Box::new(work)).await?;
    Ok(Json(
        serde_json::json!({"status":"started","job_id":job_id}),
    ))
}
async fn start(
    app: AppState,
    state: RequestState,
    id: i64,
    remove: bool,
    headers: HeaderMap,
) -> ApiResult {
    let request = request_identity(&headers, (if remove { "plaintext" } else { "encrypt" }, id))?;
    start_request(
        &app,
        &state,
        Job {
            phase: "준비".into(),
            ..Default::default()
        },
        request,
        move |state| {
            if remove {
                cleanup(state, id)
            } else {
                convert(state, id)
            }
        },
    )
    .await
}
fn ensure_api(ok: bool, msg: &str) -> Result<(), (StatusCode, String)> {
    if ok {
        Ok(())
    } else {
        Err(error(msg))
    }
}
pub async fn encrypt_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    start(app, state, id, false, headers).await
}
pub async fn cleanup_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    start(app, state, id, true, headers).await
}
pub async fn export_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    let request = request_identity(&headers, ("export", id))?;
    start_request(
        &app,
        &state,
        Job {
            phase: "복원 준비".into(),
            ..Default::default()
        },
        request,
        move |state| {
            state.archives.progress("복원 준비", 0, 0)?;
            let name = rename::restoration_name(state, id)?;
            let filename = FsPath::new(&name)
                .file_name()
                .context("파일명 없음")?
                .to_string_lossy()
                .into_owned();
            state.archives.progress("복원 준비", 0, 0)?;
            let destination = rfd::FileDialog::new()
                .set_file_name(&filename)
                .save_file()
                .context("복원 폴더 선택을 취소했습니다.")?;
            state.archives.progress("원본 복원", 0, 0)?;
            let (_, rel, key) = ready(state, id)?;
            crypto::restore_progress(
                &crypto::safe_child(&state.root_dir, &rel)?,
                &key,
                &destination,
                |n| state.archives.progress("원본 복원", n, 0),
            )?;
            Ok(())
        },
    )
    .await
}

#[cfg(test)]
#[path = "archive_security_tests.rs"]
mod tests;

#[path = "archive_security_batch.rs"]
mod batch;
pub use batch::batch_api;

#[cfg(test)]
#[path = "video_security_tests.rs"]
mod video_tests;

#[cfg(test)]
#[path = "notes_tests.rs"]
mod note_tests;
