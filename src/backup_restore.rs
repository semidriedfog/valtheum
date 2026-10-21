//! Restore validated DB/key bundles into a new library without replacing user files.
use crate::{
    archive_crypto as crypto, encryption,
    state::{AppState, Library, RequestState},
};
use anyhow::{ensure, Context, Result};
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zeroize::{Zeroize, Zeroizing};

const MAX_DB: u64 = 64 * 1024 * 1024 * 1024;
#[derive(Clone, Deserialize, Serialize)]
struct Manifest {
    format: String,
    version: u32,
    database: String,
    key_included: bool,
    media_included: bool,
}
impl Manifest {
    fn encrypted(&self) -> bool {
        self.database == encryption::SECURE_DB
    }
}
pub struct Selection {
    token: String,
    generation: u64,
    file: Arc<Mutex<fs::File>>,
}
type ApiResult = Result<Json<serde_json::Value>, (StatusCode, String)>;
fn error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}

fn inspect<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<Manifest> {
    ensure!(
        (2..=4).contains(&zip.len()),
        "정상적인 valtheum DB 백업 묶음이 아닙니다."
    );
    let mut entry = zip
        .by_name("backup.json")
        .context("backup.json이 없는 백업입니다. .vtbackup 파일을 선택해 주세요.")?;
    ensure!(entry.size() <= 16384, "백업 설명이 너무 큽니다.");
    let mut bytes = Vec::new();
    (&mut entry).take(16385).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16384, "백업 설명이 너무 큽니다.");
    let manifest: Manifest =
        serde_json::from_slice(&bytes).context("백업 설명이 손상되었습니다.")?;
    drop(entry);
    ensure!(
        manifest.format == "valtheum-backup" && manifest.version == 1 && !manifest.media_included,
        "지원하지 않는 백업 형식입니다."
    );
    ensure!(
        matches!(
            manifest.database.as_str(),
            encryption::PLAIN_DB | encryption::SECURE_DB
        ),
        "백업 DB 이름이 잘못되었습니다."
    );
    ensure!(
        !manifest.key_included || manifest.encrypted(),
        "평문 DB에 미디어 키가 포함된 잘못된 백업입니다."
    );
    let mut names = std::collections::HashSet::new();
    for i in 0..zip.len() {
        let e = zip.by_index(i)?;
        ensure!(
            !e.is_dir()
                && e.unix_mode()
                    .is_none_or(|m| m & 0o170000 == 0 || m & 0o170000 == 0o100000),
            "백업에 특수 파일이 있습니다."
        );
        ensure!(
            names.insert(e.name().to_owned()),
            "백업에 중복된 파일이 있습니다."
        );
        let limit = match e.name() {
            n if n == manifest.database => MAX_DB,
            crypto::KEY_PATH if manifest.key_included => 96,
            "backup.json" | "RESTORE.txt" => 16384,
            _ => anyhow::bail!("백업에 예상하지 않은 파일이 있습니다."),
        };
        ensure!(e.size() <= limit, "백업 파일이 허용 크기를 넘었습니다.");
    }
    ensure!(names.contains(&manifest.database), "백업에 DB가 없습니다.");
    ensure!(
        names.contains(crypto::KEY_PATH) == manifest.key_included,
        "백업 키 포함 정보가 일치하지 않습니다."
    );
    ensure!(
        zip.by_name(&manifest.database)?.size() >= 4096,
        "백업 DB가 손상되었습니다."
    );
    if manifest.key_included {
        ensure!(
            zip.by_name(crypto::KEY_PATH)?.size() == 96,
            "백업의 vault.key가 손상되었습니다."
        );
    }
    Ok(manifest)
}

fn extract<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str, target: &Path) -> Result<()> {
    let mut entry = zip.by_name(name)?;
    let size = entry.size();
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let copied = std::io::copy(&mut (&mut entry).take(size + 1), &mut out)?;
    ensure!(copied == size, "백업 파일 길이가 일치하지 않습니다.");
    out.flush()?;
    out.sync_all()?;
    Ok(())
}
fn cleanup_stage(stage: &Path) {
    // Only known files created in this operation; unexpected entries are preserved.
    for db in [encryption::PLAIN_DB, encryption::SECURE_DB] {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = fs::remove_file(stage.join(format!("{db}{suffix}")));
        }
    }
    let _ = fs::remove_file(stage.join(crypto::KEY_PATH));
    let _ = fs::remove_dir(stage.join(".valtheum"));
    let _ = fs::remove_dir(stage);
}

fn restore_bundle(file: &mut fs::File, parent: &Path, password: &str) -> Result<PathBuf> {
    file.seek(SeekFrom::Start(0))?;
    let mut zip = zip::ZipArchive::new(file)?;
    let manifest = inspect(&mut zip)?;
    ensure!(
        !manifest.encrypted() || !password.is_empty(),
        "백업 당시 비밀번호를 입력해 주세요."
    );
    let parent = parent.canonicalize()?;
    ensure!(parent.is_dir(), "복원할 상위 폴더가 아닙니다.");
    crypto::check_space(
        &parent,
        zip.by_name(&manifest.database)?
            .size()
            .saturating_mul(2)
            .saturating_add(10485760),
    )?;
    let id = crypto::hex(&crypto::random::<12>()?);
    let stage = parent.join(format!(".valtheum-restore-{id}"));
    let target = parent.join(format!("valtheum-restored-{}-{id}", crate::timestamp()));
    fs::create_dir(&stage)?;
    let result = (|| -> Result<()> {
        extract(
            &mut zip,
            &manifest.database,
            &stage.join(&manifest.database),
        )?;
        if manifest.key_included {
            fs::create_dir(stage.join(".valtheum"))?;
            extract(&mut zip, crypto::KEY_PATH, &stage.join(crypto::KEY_PATH))?;
        }
        let conn = encryption::open(
            &stage.join(&manifest.database),
            manifest.encrypted().then_some(password),
            false,
        )?;
        conn.pragma_update(None, "trusted_schema", "OFF")?;
        encryption::verify(&conn, manifest.encrypted())?;
        // Reject unrelated SQLite files before applying valtheum migrations.
        conn.prepare("SELECT id,rel_path,file_type,file_size,rating,score,watch_count FROM media_files LIMIT 0").context("valtheum 미디어 DB가 아닙니다.")?;
        crate::db::init_db(&conn)?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        let required:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM library_info WHERE key='archive_key_sha256') OR EXISTS(SELECT 1 FROM archive_store)",[],|r|r.get(0))?;
        ensure!(
            !required || manifest.key_included,
            "미디어 암호화에 필요한 vault.key가 백업에 없습니다."
        );
        if manifest.key_included {
            let key = fs::read(stage.join(crypto::KEY_PATH))?;
            crate::archive_security::check_key(&conn, &key)?;
            crypto::read_key(&key, password)
                .context("백업 키 비밀번호가 맞지 않거나 키가 손상되었습니다.")?;
        }
        // The backup does not contain the old filesystem state. Do not replay
        // an unfinished deletion against media that has not been copied yet.
        conn.execute("DELETE FROM delete_intents", [])?;
        conn.execute(
            "INSERT OR REPLACE INTO library_info(key,value) VALUES('restore_pending_media','1')",
            [],
        )?;
        encryption::verify(&conn, manifest.encrypted())?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")?;
        drop(conn);
        fs::OpenOptions::new()
            .write(true)
            .open(stage.join(&manifest.database))?
            .sync_all()?;
        ensure!(!target.try_exists()?, "복원 폴더가 이미 존재합니다.");
        fs::rename(&stage, &target)?;
        Ok(())
    })();
    if result.is_err() {
        cleanup_stage(&stage);
    }
    result?;
    Ok(target)
}

pub async fn select_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    let chosen = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title(crate::language::t("DB 백업 파일 선택"))
            .add_filter(
                crate::language::t("valtheum DB·키 백업"),
                &["vtbackup", "zip"],
            )
            .pick_file()
    })
    .await
    .map_err(error)?;
    let Some(path) = chosen else {
        return Ok(Json(serde_json::json!({"status":"cancelled"})));
    };
    tokio::task::spawn_blocking(move||->Result<Json<serde_json::Value>>{
        let mut file=crypto::open_source(&path)?;
        let manifest=inspect(&mut zip::ZipArchive::new(&mut file)?)?;
        let current=app.current.lock().unwrap();
        ensure!(current.generation==state.generation,"폴더가 변경되었습니다.");
        let token=crypto::hex(&crypto::random::<24>()?);
        *app.restore_selection.lock().unwrap()=Some(Selection{token:token.clone(),generation:state.generation,file:Arc::new(Mutex::new(file))});
        Ok(Json(serde_json::json!({"status":"selected","token":token,"name":path.file_name().unwrap_or_default().to_string_lossy(),"encrypted":manifest.encrypted(),"key_included":manifest.key_included})))
    }).await.map_err(error)?.map_err(error)
}

#[derive(Deserialize)]
pub struct RestoreInput {
    token: String,
    #[serde(default)]
    password: String,
}
impl Drop for RestoreInput {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

pub async fn clear_api(State(app): State<AppState>, Json(input): Json<RestoreInput>) -> ApiResult {
    let mut selection = app.restore_selection.lock().unwrap();
    if selection.as_ref().is_some_and(|s| s.token == input.token) {
        selection.take();
    }
    Ok(Json(serde_json::json!({"status":"cleared"})))
}

fn prepare_restore(
    app: &AppState,
    expected: u64,
    file: Arc<Mutex<fs::File>>,
    parent: PathBuf,
    password: Zeroizing<String>,
) -> Result<(PathBuf, Arc<Library>)> {
    let _operation = crate::security_api::DbOperation::begin(app, expected)
        .map_err(|(_, message)| anyhow::anyhow!(message))?;
    let path = restore_bundle(&mut file.lock().unwrap(), &parent, &password)?;
    let library = Library::open(path.clone())?;
    if library.security_status().encrypted {
        let conn = encryption::open(&path.join(encryption::SECURE_DB), Some(&password), false)?;
        crate::db::init_db(&conn)?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        crate::archive_security::unlock(&library, &conn, &password)?;
        *library.conn.lock().unwrap() = Some(conn);
        library.security.lock().unwrap().password = Some(password);
    }
    Ok((path, library))
}

pub async fn restore_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(mut input): Json<RestoreInput>,
) -> ApiResult {
    if input.password.len() > 1024 {
        return Err(error("비밀번호가 너무 깁니다."));
    }
    let file = {
        let selection = app.restore_selection.lock().unwrap();
        let s = selection
            .as_ref()
            .ok_or_else(|| error("백업 파일을 먼저 선택해 주세요."))?;
        if s.token != input.token || s.generation != state.generation {
            return Err(error("백업을 다시 선택해 주세요."));
        }
        s.file.clone()
    };
    let parent = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title(crate::language::t("복원할 새 라이브러리의 상위 폴더 선택"))
            .pick_folder()
    })
    .await
    .map_err(error)?;
    let Some(parent) = parent else {
        return Ok(Json(serde_json::json!({"status":"cancelled"})));
    };
    let password = Zeroizing::new(std::mem::take(&mut input.password));
    let work_app = app.clone();
    let expected = state.generation;
    let restored = tokio::task::spawn_blocking(move || {
        prepare_restore(&work_app, expected, file, parent, password)
    })
    .await
    .map_err(error)?
    .map_err(error)?;
    let (path, library) = restored;
    let path_text = path.to_string_lossy().into_owned();
    // A completed restore remains usable if registering/switching fails afterwards.
    let installed = (|| -> Result<serde_json::Value> {
        let mut config = crate::load_config()?;
        config.folders.push(crate::SavedFolder {
            path: path_text.clone(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            last_used: crate::timestamp(),
        });
        config.active_folder = Some(path_text.clone());
        let response =
            crate::install_library(&app, state.generation, library, config).map_err(|_| {
                anyhow::anyhow!(
                    "복원은 완료됐지만 폴더를 자동으로 열지 못했습니다. 폴더 추가로 열어 주세요."
                )
            })?;
        Ok(serde_json::to_value(response.0)?)
    })();
    {
        let mut selection = app.restore_selection.lock().unwrap();
        if selection
            .as_ref()
            .is_some_and(|s| s.token == input.token && s.generation == state.generation)
        {
            *selection = None;
        }
    }
    match installed {
        Ok(mut data) => {
            data["status"] = "success".into();
            data["restore_path"] = path_text.into();
            Ok(Json(data))
        }
        Err(e) => Ok(Json(
            serde_json::json!({"status":"restored","restore_path":path_text,"message":e.to_string()}),
        )),
    }
}

#[cfg(test)]
#[path = "backup_restore_tests.rs"]
mod tests;
