use crate::{
    encryption,
    state::{AppState, Library, RequestState},
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use zeroize::{Zeroize, Zeroizing};

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, &'static str)>;
const FAILURE: (StatusCode, &str) = (
    StatusCode::BAD_REQUEST,
    "선택한 폴더와 비밀번호를 확인해 주세요.",
);

#[derive(Default)]
pub struct EntryState {
    selected: Mutex<Option<(u64, PathBuf)>>,
    choosing: AtomicBool,
}

impl EntryState {
    pub(crate) fn clear(&self) {
        *self.selected.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn folder_info(path: &std::path::Path, name: &str) -> serde_json::Value {
    serde_json::json!({"path":path.to_string_lossy(),"name":name,
        "encrypted":path.join(encryption::SECURE_DB).is_file()})
}

fn select_at(app: &AppState, expected: u64, root: PathBuf) -> ApiResult {
    let snapshot = app.snapshot().map_err(|_| FAILURE)?;
    if snapshot.generation != expected || !app.concealed.load(Ordering::Acquire) {
        return Err(FAILURE);
    }
    let root = root.canonicalize().map_err(|_| FAILURE)?;
    if !root.is_dir() {
        return Err(FAILURE);
    }
    let folder = folder_info(
        &root,
        &root
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
    );
    let current = app.current.lock().map_err(|_| FAILURE)?;
    if current.generation != expected || !app.concealed.load(Ordering::Acquire) {
        return Err(FAILURE);
    }
    *app.privacy_entry.selected.lock().map_err(|_| FAILURE)? = Some((expected, root));
    Ok(Json(
        serde_json::json!({"status":"selected","folder":folder}),
    ))
}

pub async fn select_folder(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    if !app.concealed.load(Ordering::Acquire)
        || app.privacy_entry.choosing.swap(true, Ordering::AcqRel)
    {
        return Err(FAILURE);
    }
    struct Choosing(std::sync::Arc<EntryState>);
    impl Drop for Choosing {
        fn drop(&mut self) {
            self.0.choosing.store(false, Ordering::Release);
        }
    }
    let choosing = Choosing(app.privacy_entry.clone());
    tokio::task::spawn_blocking(move || {
        let _choosing = choosing;
        let Some(root) = rfd::FileDialog::new()
            .set_title(crate::language::t("라이브러리 폴더 지정"))
            .pick_folder()
        else {
            return Ok(Json(serde_json::json!({"status":"cancelled"})));
        };
        select_at(&app, state.generation, root)
    })
    .await
    .map_err(|_| FAILURE)?
}

// Called only when the hidden entry dialog opens; session responses stay anonymous.
pub async fn folders(State(app): State<AppState>) -> ApiResult {
    tokio::task::spawn_blocking(move || choices_at(&app, &crate::get_config_path()))
        .await
        .map_err(|_| FAILURE)?
}

fn choices_at(app: &AppState, config_path: &std::path::Path) -> ApiResult {
    if !app.concealed.load(Ordering::Acquire) {
        return Err(FAILURE);
    }
    let snapshot = app.snapshot().map_err(|_| FAILURE)?;
    let unconfigured = snapshot.root_dir.as_os_str().is_empty();
    let config = crate::load_config_at(config_path).map_err(|_| FAILURE)?;
    let folders: Vec<_> = config
        .folders
        .iter()
        .filter(|folder| {
            let path = PathBuf::from(&folder.path);
            path.join(encryption::SECURE_DB).is_file() || (unconfigured && path.is_dir())
        })
        .map(|folder| folder_info(&PathBuf::from(&folder.path), &folder.name))
        .collect();
    let current = app.current.lock().map_err(|_| FAILURE)?;
    if current.generation != snapshot.generation || !app.concealed.load(Ordering::Acquire) {
        return Err(FAILURE);
    }
    Ok(Json(
        serde_json::json!({"folders":folders,"active_folder":config.active_folder}),
    ))
}

#[derive(Deserialize)]
pub struct OpenFolder {
    path: String,
    password: String,
}
impl Drop for OpenFolder {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

pub async fn open_folder(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(input): Json<OpenFolder>,
) -> ApiResult {
    open_at(app, state.generation, input, crate::get_config_path()).await
}

async fn open_at(
    app: AppState,
    expected: u64,
    mut input: OpenFolder,
    config_path: PathBuf,
) -> ApiResult {
    let password = Zeroizing::new(std::mem::take(&mut input.password));
    if password.len() > 1024 {
        return Err(FAILURE);
    }
    tokio::task::spawn_blocking(move || {
        // Reserve the captured library, but keep filesystem/SQLCipher work off
        // the global state lock so closing can revoke this attempt immediately.
        let operation = crate::security_api::DbOperation::begin(&app, expected)
            .map_err(|(status, _)| (status, FAILURE.1))?;
        if !app.concealed.load(Ordering::Acquire) {
            return Err(FAILURE);
        }
        let previous = &operation.state;
        let mut config = crate::load_config_at(&config_path).map_err(|_| FAILURE)?;
        let native = app
            .privacy_entry
            .selected
            .lock()
            .map_err(|_| FAILURE)?
            .clone();
        let native_selected = native.as_ref().is_some_and(|(generation, path)| {
            *generation == expected && path.to_string_lossy() == input.path
        });
        let registered = config
            .folders
            .iter()
            .any(|folder| folder.path == input.path);
        if !registered && !native_selected {
            return Err(FAILURE);
        }
        let root = PathBuf::from(&input.path);
        let encrypted = root.join(encryption::SECURE_DB).is_file();
        let allow_plain = native_selected || previous.root_dir.as_os_str().is_empty();
        if encrypted {
            if password.is_empty() {
                return Err(FAILURE);
            }
        } else if !allow_plain || !password.is_empty() {
            return Err(FAILURE);
        }
        let candidate = Library::open_reusing(root, previous.lease.as_ref()).map_err(|e| {
            if e.to_string() == crate::library_lock::UNAVAILABLE {
                (StatusCode::LOCKED, crate::library_lock::UNAVAILABLE)
            } else {
                FAILURE
            }
        })?;
        {
            let mut security = candidate.security.lock().map_err(|_| FAILURE)?;
            if encrypted && !security.encrypted {
                return Err(FAILURE);
            }
            if security.encrypted {
                crate::security_api::unlock_library(&candidate, &mut security, password)
                    .map_err(|_| FAILURE)?;
            }
        }
        let active_path = if let Some(folder) = config.folders.iter_mut().find(|folder| {
            folder.path == input.path
                || PathBuf::from(&folder.path).canonicalize().ok().as_ref()
                    == Some(&candidate.root_dir)
        }) {
            folder.last_used = crate::timestamp();
            folder.path.clone()
        } else {
            config.folders.push(crate::SavedFolder {
                path: input.path.clone(),
                name: candidate
                    .root_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| input.path.clone()),
                last_used: crate::timestamp(),
            });
            input.path.clone()
        };
        config.active_folder = Some(active_path);
        let security = candidate.security_status();
        let mut current = app.current.lock().map_err(|_| FAILURE)?;
        if current.generation != expected || !app.concealed.load(Ordering::Acquire) {
            return Err((
                StatusCode::CONFLICT,
                "화면 상태가 변경되었습니다. 다시 열어 주세요.",
            ));
        }
        // Authentication and key validation must succeed before changing selection on disk or in memory.
        crate::save_config_at(&config_path, &config).map_err(|_| FAILURE)?;
        crate::security_api::retire_in_background(&current.library, None);
        *app.restore_selection
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *app.privacy_entry
            .selected
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        current.library = candidate;
        current.generation += 1;
        app.concealed.store(false, Ordering::Release);
        Ok(Json(
            serde_json::json!({"generation":current.generation,"covered":false,
            "security":security,"scan":crate::state::ScanStatus::default()}),
        ))
    })
    .await
    .map_err(|_| FAILURE)?
}

#[cfg(test)]
#[path = "privacy_tests.rs"]
mod tests;
