use crate::state::{AppState, RequestState};
use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::{
    path::{Path as FsPath, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

type ApiResult = Result<Json<serde_json::Value>, StatusCode>;
pub struct Runtime {
    data: Mutex<Gallery>,
    choosing: AtomicBool,
    request: AtomicU64,
    io_slots: Arc<tokio::sync::Semaphore>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            data: Default::default(),
            choosing: Default::default(),
            request: Default::default(),
            io_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}
#[derive(Default)]
struct Gallery {
    root: Option<PathBuf>,
    files: Vec<String>,
    revision: u64,
    status: &'static str,
}
fn settings_path() -> PathBuf {
    crate::get_config_path()
}
fn supported(path: &FsPath) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif")
    )
}
fn scan(root: Option<PathBuf>) -> Gallery {
    let mut gallery = Gallery {
        status: "empty",
        ..Default::default()
    };
    let Some(root) = root else {
        return gallery;
    };
    let Ok(root) = root.canonicalize() else {
        gallery.status = "unavailable";
        return gallery;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        gallery.status = "unavailable";
        return gallery;
    };
    gallery.root = Some(root);
    for entry in entries {
        let Ok(entry) = entry else {
            gallery.status = "partial";
            continue;
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) || !supported(&entry.path()) {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            gallery.files.push(name.to_owned());
        }
    }
    gallery.files.sort();
    if gallery.status != "partial" {
        gallery.status = "ready";
    }
    gallery
}
fn response(gallery: &Gallery, seconds: u32) -> Json<serde_json::Value> {
    let images: Vec<_> = gallery
        .files
        .iter()
        .enumerate()
        .map(|(index, name)| {
            serde_json::json!({"id":index.to_string(),"name":name,"src":format!("/api/gallery/image/{}/{index}", gallery.revision)})
        })
        .collect();
    let folder_name = gallery
        .root
        .as_ref()
        .and_then(|root| root.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    Json(
        serde_json::json!({"images":images,"status":gallery.status,"folder_name":folder_name,"frame_seconds":seconds.clamp(1,3600)}),
    )
}

fn require_covered(
    app: &AppState,
    expected: u64,
) -> Result<std::sync::MutexGuard<'_, RequestState>, StatusCode> {
    let current = app
        .current
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if current.generation != expected || !app.concealed.load(Ordering::Acquire) {
        return Err(StatusCode::CONFLICT);
    }
    Ok(current)
}
fn begin_request(app: &AppState, expected: u64) -> Result<u64, StatusCode> {
    let _current = require_covered(app, expected)?;
    Ok(app.gallery.request.fetch_add(1, Ordering::AcqRel) + 1)
}
fn request_at(
    app: &AppState,
    expected: u64,
    ticket: u64,
    path: &FsPath,
    change: Option<Option<PathBuf>>,
    read: impl FnOnce(Option<PathBuf>) -> Gallery,
) -> ApiResult {
    // NAS work must hold neither the app mutex nor the gallery mutex. A clear,
    // another selection, or entering the real library can supersede this read.
    let config = crate::load_config_at(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let folder = change.clone().unwrap_or(config.gallery_folder);
    let selected = change.is_some() && folder.is_some();
    let mut next = read(folder);
    let _current = require_covered(app, expected)?;
    if app.gallery.request.load(Ordering::Acquire) != ticket {
        return Err(StatusCode::CONFLICT);
    }
    if selected && next.root.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut config = crate::load_config_at(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if change.is_some() {
        config.gallery_folder = next.root.clone();
        crate::save_config_at(path, &config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let mut data = app
        .gallery
        .data
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    next.revision = data.revision + 1;
    *data = next;
    Ok(response(&data, config.gallery_frame_seconds))
}
#[cfg(test)]
fn load_at(app: &AppState, expected: u64, path: &FsPath) -> ApiResult {
    request_at(
        app,
        expected,
        begin_request(app, expected)?,
        path,
        None,
        scan,
    )
}
#[cfg(test)]
fn set_at(app: &AppState, expected: u64, path: &FsPath, folder: Option<PathBuf>) -> ApiResult {
    request_at(
        app,
        expected,
        begin_request(app, expected)?,
        path,
        Some(folder),
        scan,
    )
}
struct PendingRequest(Arc<Runtime>, u64);
impl Drop for PendingRequest {
    fn drop(&mut self) {
        let _ = self.0.request.compare_exchange(
            self.1,
            self.1 + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}
async fn run_request(app: AppState, expected: u64, change: Option<Option<PathBuf>>) -> ApiResult {
    let ticket = begin_request(&app, expected)?;
    let _pending = PendingRequest(app.gallery.clone(), ticket);
    // A clear never needs NAS access or an I/O slot. Bound workers that may stay
    // blocked in the OS even after the HTTP request has timed out.
    let permit = if matches!(change, Some(None)) {
        None
    } else {
        Some(
            app.gallery
                .io_slots
                .clone()
                .try_acquire_owned()
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
        )
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            request_at(&app, expected, ticket, &settings_path(), change, scan)
        }),
    )
    .await
    .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}
#[derive(serde::Deserialize)]
pub struct FrameSettings {
    seconds: u32,
}
fn frame_settings_at(app: &AppState, expected: u64, path: &FsPath, seconds: u32) -> ApiResult {
    let _current = require_covered(app, expected)?;
    if !(1..=3600).contains(&seconds) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut config = crate::load_config_at(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    config.gallery_frame_seconds = seconds;
    crate::save_config_at(path, &config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"frame_seconds":seconds})))
}
pub async fn frame_settings(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(input): Json<FrameSettings>,
) -> ApiResult {
    tokio::task::spawn_blocking(move || {
        frame_settings_at(&app, state.generation, &settings_path(), input.seconds)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}
pub async fn list(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    run_request(app, state.generation, None).await
}
pub async fn clear(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    run_request(app, state.generation, Some(None)).await
}
pub async fn select(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    drop(require_covered(&app, state.generation)?);
    if app.gallery.choosing.swap(true, Ordering::AcqRel) {
        return Err(StatusCode::CONFLICT);
    }
    struct Choosing(std::sync::Arc<Runtime>);
    impl Drop for Choosing {
        fn drop(&mut self) {
            self.0.choosing.store(false, Ordering::Release);
        }
    }
    let choosing = Choosing(app.gallery.clone());
    let (choosing, folder) = tokio::task::spawn_blocking(move || {
        let folder = rfd::FileDialog::new()
            .set_title(crate::language::t("사진 폴더 선택"))
            .pick_folder();
        (choosing, folder)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _choosing = choosing;
    let Some(folder) = folder else {
        return Ok(Json(serde_json::json!({"cancelled":true})));
    };
    run_request(app, state.generation, Some(Some(folder))).await
}
fn resolve_image(
    app: &AppState,
    expected: u64,
    revision: u64,
    index: usize,
) -> Result<PathBuf, StatusCode> {
    let (root, path) = {
        let _current = require_covered(app, expected)?;
        let data = app
            .gallery
            .data
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if revision != data.revision {
            return Err(StatusCode::NOT_FOUND);
        }
        let root = data.root.as_ref().ok_or(StatusCode::NOT_FOUND)?;
        let name = data.files.get(index).ok_or(StatusCode::NOT_FOUND)?;
        (root.clone(), root.join(name))
    };
    // The allowlist contains only immediate regular image files; do not follow replaced symlinks.
    if !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_file()) {
        return Err(StatusCode::NOT_FOUND);
    }
    let canonical = path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if canonical.parent() != Some(root.as_path()) || !supported(&canonical) {
        return Err(StatusCode::FORBIDDEN);
    }
    let _current = require_covered(app, expected)?;
    if app
        .gallery
        .data
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .revision
        != revision
    {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(canonical)
}
pub async fn image(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path((revision, index)): Path<(u64, usize)>,
    req: Request,
) -> Result<Response, StatusCode> {
    let permit = app
        .gallery
        .io_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let path = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        resolve_image(&app, state.generation, revision, index)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    let response = ServeFile::new(path)
        .oneshot(req)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(response.into_response())
}
#[cfg(test)]
#[path = "gallery_tests.rs"]
mod tests;
