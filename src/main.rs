#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod archive;
mod archive_crypto;
mod archive_security;
mod backup_restore;
mod db;
mod encryption;
mod gallery;
mod integrity;
mod language;
mod language_api;
mod library_lock;
mod local_server;
mod media_name;
mod media_stream;
mod password_change;
mod persistence;
mod privacy;
mod relink;
mod scanner;
mod security_api;
mod series;
mod state;
mod storage_recovery;
use state::{resolve_path, AppState, Library, RequestState, ScanStatus};

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Extension, Json, Router,
};
use base64::Engine;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use wry::WebViewBuilder;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SavedFolder {
    path: String,
    name: String,
    last_used: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppConfig {
    folders: Vec<SavedFolder>,
    active_folder: Option<String>,
    #[serde(default)]
    gallery_folder: Option<PathBuf>,
    #[serde(default = "default_frame_seconds")]
    gallery_frame_seconds: u32,
    #[serde(default = "language::default_code")]
    language: String,
}

fn default_frame_seconds() -> u32 {
    10
}
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            active_folder: None,
            gallery_folder: None,
            gallery_frame_seconds: default_frame_seconds(),
            language: language::default_code(),
        }
    }
}

static CONFIG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn get_config_path() -> PathBuf {
    CONFIG_PATH.get().cloned().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("vault_config.json")
    })
}

fn load_config() -> anyhow::Result<AppConfig> {
    load_config_at(&get_config_path())
}

type SessionFolders = (Vec<SavedFolder>, Option<String>);
static SESSION_FOLDERS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, SessionFolders>>,
> = std::sync::OnceLock::new();
fn session_folders() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, SessionFolders>>
{
    SESSION_FOLDERS.get_or_init(Default::default)
}
fn public_config(config: &AppConfig) -> serde_json::Value {
    serde_json::json!({"gallery_folder": config.gallery_folder, "gallery_frame_seconds": config.gallery_frame_seconds, "language": config.language})
}
fn load_config_at(path: &std::path::Path) -> anyhow::Result<AppConfig> {
    let mut config = load_public_config_at(path)?;
    if let Some((folders, active)) = session_folders().lock().unwrap().get(path) {
        config.folders = folders.clone();
        config.active_folder = active.clone();
    }
    Ok(config)
}
fn load_public_config_at(path: &std::path::Path) -> anyhow::Result<AppConfig> {
    match std::fs::read(path) {
        Ok(bytes) => match decode_config_at(path, &bytes) {
            Ok(config) => return Ok(config),
            Err(error) => {
                let backup = path.with_extension("json.bak");
                if let Ok(bytes) = std::fs::read(&backup) {
                    if let Ok(config) = decode_config_at(path, &bytes) {
                        eprintln!("설정 파일을 읽지 못해 마지막 정상 설정을 사용합니다: {error}");
                        return Ok(config);
                    }
                }
                anyhow::bail!(
                    "설정 파일이 손상되었습니다. 원본은 보존했습니다: {} ({error})",
                    path.display()
                );
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let backup = path.with_extension("json.bak");
            match std::fs::read(&backup) {
                Ok(bytes) => return decode_config_at(path, &bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(AppConfig {
        gallery_folder: read_legacy_gallery_at(path)?,
        gallery_frame_seconds: default_frame_seconds(),
        ..Default::default()
    })
}

// An explicit null means the user cleared the gallery; never re-import it.
fn decode_config_at(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<AppConfig> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    #[derive(Deserialize)]
    struct PublicConfig {
        #[serde(default)]
        gallery_folder: Option<PathBuf>,
        #[serde(default = "default_frame_seconds")]
        gallery_frame_seconds: u32,
        #[serde(default = "language::default_code")]
        language: String,
    }
    let public: PublicConfig = serde_json::from_value(value.clone())?;
    let mut config = AppConfig {
        gallery_folder: public.gallery_folder,
        gallery_frame_seconds: public.gallery_frame_seconds,
        language: if language::valid(&public.language) {
            public.language
        } else {
            language::default_code()
        },
        ..Default::default()
    };
    if value.get("gallery_folder").is_none() {
        config.gallery_folder = read_legacy_gallery_at(path)?;
    }
    Ok(config)
}

fn read_legacy_gallery_at(path: &std::path::Path) -> anyhow::Result<Option<PathBuf>> {
    #[derive(Deserialize)]
    struct LegacyGallery {
        folder: Option<PathBuf>,
    }
    match std::fs::read(path.with_file_name("valtheum_gallery.json")) {
        Ok(bytes) => Ok(serde_json::from_slice::<LegacyGallery>(&bytes)?.folder),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn save_config(config: &AppConfig) -> anyhow::Result<()> {
    save_config_at(&get_config_path(), config)
}

fn save_config_at(path: &std::path::Path, config: &AppConfig) -> anyhow::Result<()> {
    // Never copy legacy private fields into the recovery file.
    let backup_path = path.with_extension("json.bak");
    let previous = std::fs::read(path)
        .ok()
        .and_then(|b| decode_config_at(path, &b).ok())
        .or_else(|| {
            std::fs::read(&backup_path)
                .ok()
                .and_then(|b| decode_config_at(path, &b).ok())
        });
    if let Some(previous) = previous {
        persistence::atomic_write(
            &backup_path,
            &serde_json::to_vec_pretty(&public_config(&previous))?,
        )?;
    }
    persistence::atomic_write(path, &serde_json::to_vec_pretty(&public_config(config))?)?;
    session_folders().lock().unwrap().insert(
        path.to_owned(),
        (config.folders.clone(), config.active_folder.clone()),
    );
    // Retire the old settings only after the unified configuration is durable.
    if let Err(error) = std::fs::remove_file(path.with_file_name("valtheum_gallery.json")) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!("이전 갤러리 설정 정리 실패: {error}");
        }
    }
    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/session", get(get_session))
        .route("/i18n-preference.js", get(language_api::preference))
        .route("/api/settings/language", post(language_api::update))
        .route("/api/storage/reconnect", post(storage_recovery::reconnect))
        .route("/api/storage/cancel", post(storage_recovery::cancel))
        .route(
            "/api/integrity/quick",
            get(integrity::status).post(integrity::start),
        )
        .route("/api/integrity/quick/cancel", post(integrity::cancel))
        .route("/api/gallery", get(gallery::list).delete(gallery::clear))
        .route("/api/gallery/select", post(gallery::select))
        .route("/api/gallery/frame-settings", post(gallery::frame_settings))
        .route("/api/gallery/image/:revision/:index", get(gallery::image))
        .route("/api/security", get(security_api::status))
        .route("/api/security/folders", get(privacy::folders))
        .route("/api/security/select-folder", post(privacy::select_folder))
        .route(
            "/api/security/open-folder",
            post(privacy::open_folder).layer(axum::extract::DefaultBodyLimit::max(16384)),
        )
        .route(
            "/api/security/unlock",
            post(security_api::unlock).layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/api/security/encrypt",
            post(security_api::encrypt).layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route("/api/security/lock", post(security_api::lock))
        .route(
            "/api/security/password",
            post(security_api::change_password).layer(axum::extract::DefaultBodyLimit::max(16384)),
        )
        .route(
            "/api/security/plaintext",
            delete(security_api::remove_plaintext),
        )
        .route("/api/scan", get(get_scan_status))
        .route("/api/trash/count", get(get_trash_count))
        .route("/api/files", get(get_files))
        .route(
            "/api/series",
            get(list_series_handler).post(save_series_handler),
        )
        .route("/api/series/:id", delete(dissolve_series_handler))
        .route(
            "/api/files/:id/name",
            post(media_name::rename).layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route("/api/files/:id/relink", post(relink::select))
        .route("/api/files/:id/watch", post(record_watch))
        .route(
            "/api/files/:id/pdf-position",
            get(pdf_position).post(save_pdf_position),
        )
        .route(
            "/api/files/:id/bookmarks",
            get(list_bookmarks_handler).post(create_bookmark_handler),
        )
        .route(
            "/api/files/:id/bookmarks/:bookmark_id",
            delete(delete_bookmark_handler),
        )
        .route(
            "/api/files/:id/bookmarks/:bookmark_id/thumbnail",
            get(get_bookmark_thumbnail_handler),
        )
        .route("/api/files/:id/reset_watch", post(reset_watch_handler))
        .route("/api/files/:id/rating", post(update_rating_handler))
        .route("/api/files/:id/trash", post(move_to_trash_handler))
        .route("/api/files/:id/restore", post(restore_from_trash_handler))
        .route(
            "/api/files/:id/permanent",
            delete(permanently_delete_handler),
        )
        .route("/api/trash", get(get_trash_files_handler))
        .route(
            "/api/trash/logs",
            get(get_deleted_logs_handler).delete(clear_deleted_logs_handler),
        )
        .route("/api/trash/logs/:id", delete(delete_deleted_log_handler))
        .route("/api/arena/pair", get(get_arena_pair_handler))
        .route("/api/arena/vote", post(vote_arena_handler))
        .route("/api/folders", get(get_folders_handler))
        .route("/api/folders/add", post(add_folder_handler))
        .route("/api/folders/close", post(security_api::close))
        .route("/api/folders/switch", post(switch_folder_handler))
        .route("/api/folders/remove", post(remove_folder_handler))
        .route("/api/folders/rescan", post(rescan_folder_handler))
        .route("/api/folders/reset_watch", post(reset_folder_watch_handler))
        .route(
            "/api/folders/reset_scores",
            post(reset_folder_scores_handler),
        )
        .route(
            "/api/folders/reset_ratings",
            post(reset_folder_ratings_handler),
        )
        .route("/api/backup", post(backup_db_handler))
        .route(
            "/api/backup/restore/select",
            post(backup_restore::select_api),
        )
        .route("/api/backup/restore/clear", post(backup_restore::clear_api))
        .route(
            "/api/backup/restore",
            post(backup_restore::restore_api).layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/api/notes",
            post(archive_security::notes::create_api)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/api/note-saves/:request_id",
            get(archive_security::notes::save_status_api),
        )
        .route(
            "/api/notes/:id",
            get(archive_security::notes::read_api)
                .post(archive_security::notes::save_api)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/api/notes/:id/export",
            post(archive_security::notes::export_api),
        )
        .route("/api/archive-security", get(archive_security::status))
        .route(
            "/api/archive-security/progress",
            get(archive_security::progress_status),
        )
        .route(
            "/api/archive-security/:id/page",
            delete(archive_security::pages::delete_api),
        )
        .route(
            "/api/archive-security/batch",
            post(archive_security::batch_api).layer(axum::extract::DefaultBodyLimit::max(262144)),
        )
        .route(
            "/api/archive-security/cancel",
            post(archive_security::cancel),
        )
        .route(
            "/api/archive-security/:id/encrypt",
            post(archive_security::encrypt_api),
        )
        .route(
            "/api/archive-security/:id/plaintext",
            delete(archive_security::cleanup_api),
        )
        .route(
            "/api/archive-security/:id/export",
            post(archive_security::export_api),
        )
        .route("/api/thumbnail/:id", post(upload_thumbnail))
        .route("/api/thumbnail/:id", get(get_thumbnail))
        .route("/api/duration/:id", post(update_duration))
        .route("/api/media/*path", get(serve_media))
        .route("/api/archive/list/*path", get(get_archive_list))
        .route("/api/archive/file/*path", get(get_archive_file))
        .route("/api/files/:id/tags", post(add_tag))
        .route("/api/files/:id/tags/:tag_id", delete(remove_tag))
        .fallback(static_handler)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            state::guard_request,
        ))
        .with_state(state.clone())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("valtheum startup failed: {error:#}");
        rfd::MessageDialog::new()
            .set_title(crate::language::t("valtheum 실행 오류"))
            .set_description(if cfg!(debug_assertions) {
                format!("{error:#}\n\n설정을 확인한 뒤 다시 실행해 주세요.")
            } else if error.downcast_ref::<local_server::StartupError>().is_some() {
                crate::language::t("로컬 서버를 시작하지 못했습니다. 방화벽·보안 프로그램의 연결 차단 여부를 확인하고 다시 실행해 주세요.")
            } else {
                crate::language::t("valtheum을 시작하지 못했습니다. 저장장치 연결 상태를 확인하고 다시 실행해 주세요.")
            })
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}

fn startup_state_at(config_path: &std::path::Path) -> anyhow::Result<AppState> {
    session_folders().lock().unwrap().remove(config_path);
    let config = load_config_at(config_path)?;
    if config_path.try_exists()? || config_path.with_extension("json.bak").try_exists()? {
        save_config_at(config_path, &config)?;
    }
    AppState::new(Library::unconfigured())
}

fn navigation_allowed(url: &str, origin: &str, note_exit_risk: bool) -> bool {
    !note_exit_risk && (url == origin || url == format!("{origin}/"))
}

fn run() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let legacy_config = cwd.join("vault_config.json");
    let config_path =
        if legacy_config.try_exists()? || legacy_config.with_extension("json.bak").try_exists()? {
            legacy_config
        } else {
            std::env::current_exe()?
                .parent()
                .ok_or_else(|| anyhow::anyhow!("실행 파일 경로를 찾을 수 없습니다."))?
                .join("vault_config.json")
        };
    let _ = CONFIG_PATH.set(config_path.clone());
    let mut state = startup_state_at(&config_path)?;
    let listener = local_server::bind(8080)
        .map_err(anyhow::Error::from)
        .map_err(|e| e.context(local_server::StartupError))?;
    let addr = listener.local_addr()?;
    state.http_authority = Arc::new(addr.to_string());
    let origin = format!("http://{addr}");
    let navigation_origin = origin.clone();
    let ipc_authority = addr.to_string();
    let note_exit_risk = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ipc_note_exit_risk = note_exit_risk.clone();
    let navigation_note_exit_risk = note_exit_risk.clone();

    let app = build_router(state.clone());
    let concealed = state.concealed.clone();
    let native_token = state.token.clone();

    let (tx, rx) = std::sync::mpsc::channel();

    // Start Axum server in a background thread
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(Err(format!("Tokio runtime failed: {}", e)));
                return;
            }
        };
        rt.block_on(async {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => {
                    let _ = tx.send(Ok(()));
                    listener
                }
                Err(error) => {
                    let _ = tx.send(Err(error.to_string()));
                    return;
                }
            };

            println!("valtheum server running at http://{}", addr);
            if let Ok(snapshot) = state.snapshot() {
                state.start_scan(snapshot.library);
            }
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("[Server] Axum serve error: {}", e);
            }
        });
    });

    // Wait until server is definitely listening before launching webview
    match rx.recv_timeout(std::time::Duration::from_secs(6)) {
        Ok(Ok(())) => {
            println!("Axum server is ready.");
        }
        Ok(Err(err)) => {
            eprintln!("Fatal error starting server: {}", err);
            return Err(anyhow::anyhow!(err).context(local_server::StartupError));
        }
        Err(_) => {
            return Err(anyhow::anyhow!("Local server startup timed out")
                .context(local_server::StartupError));
        }
    }

    // Native Desktop Window Setup
    language::set(&load_config()?.language);
    let event_loop = EventLoopBuilder::<bool>::with_user_event().build();
    let frame_proxy = event_loop.create_proxy();
    let page_proxy = event_loop.create_proxy();
    let window_builder = WindowBuilder::new()
        .with_title("valtheum")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 850.0));
    #[cfg(target_os = "windows")]
    let window_builder = {
        use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
        use tao::window::Icon;
        // Resource 1 is embedded in the executable, so no external icon is needed.
        let small = Icon::from_resource(1, Some(tao::dpi::PhysicalSize::new(32, 32)))?;
        let large = Icon::from_resource(1, Some(tao::dpi::PhysicalSize::new(256, 256)))?;
        window_builder
            .with_window_icon(Some(small))
            .with_taskbar_icon(Some(large))
    };
    let window = window_builder.build(&event_loop)?;

    let webview_builder = WebViewBuilder::new();
    #[cfg(windows)]
    let webview_builder = {
        use wry::WebViewBuilderExtWindows;
        webview_builder.with_browser_accelerator_keys(false)
    };
    let _webview = webview_builder
        .with_devtools(cfg!(debug_assertions))
        .with_incognito(true)
        .with_initialization_script(&format!("if(window.top===window && location.origin==={}) Object.defineProperty(window,'__valtheumToken',{{value:{},writable:false,configurable:false}});", serde_json::to_string(&origin)?, serde_json::to_string(native_token.as_str())?))
        .with_navigation_handler(move |url| {
            navigation_allowed(&url, &navigation_origin,
                navigation_note_exit_risk.load(std::sync::atomic::Ordering::Acquire))
        })
        .with_new_window_req_handler(|_| false)
        .with_ipc_handler(move |request| {
            if request.uri().scheme_str() != Some("http")
                || request.uri().authority().map(|a| a.as_str()) != Some(ipc_authority.as_str())
            {
                return;
            }
            match request.body().as_str() {
                "notes:dirty" | "notes:clean" => {
                    ipc_note_exit_risk.store(request.body() == "notes:dirty", std::sync::atomic::Ordering::Release);
                }
                "frame:enter" => {
                    let _ = frame_proxy.send_event(true);
                }
                "frame:exit" => {
                    let _ = frame_proxy.send_event(false);
                }
                _ => {}
            }
        })
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, wry::PageLoadEvent::Started) {
                let _ = page_proxy.send_event(false);
            }
        })
        .with_url(&origin)
        .build(&window)?;

    println!("valtheum started successfully!");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::UserEvent(active) = event {
            let active = active && concealed.load(std::sync::atomic::Ordering::Acquire);
            window.set_fullscreen(
                active.then(|| tao::window::Fullscreen::Borderless(window.current_monitor())),
            );
        }
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            // Use a native prompt so a stalled WebView cannot make the window
            // impossible to close. Never transfer the note text over IPC.
            let confirmed = !note_exit_risk.load(std::sync::atomic::Ordering::Acquire)
                || rfd::MessageDialog::new()
                    .set_parent(&window)
                    .set_title("valtheum")
                    .set_description(language::t("저장하지 않은 메모 또는 완료되지 않은 메모 작업이 있습니다. 종료하면 미저장 내용이 사라지거나 작업이 중단됩니다. 종료할까요?"))
                    .set_level(rfd::MessageLevel::Warning)
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show() == rfd::MessageDialogResult::Yes;
            if confirmed {
                *control_flow = ControlFlow::Exit;
            }
        }
    });
}

#[derive(Serialize)]
struct FolderResponse {
    #[serde(flatten)]
    config: AppConfig,
    generation: u64,
    scan: ScanStatus,
    security: encryption::SecurityStatus,
    covered: bool,
}

#[derive(Deserialize)]
struct SessionQuery {
    #[serde(default)]
    concealment: bool,
}
async fn get_session(
    State(state): State<AppState>,
    Query(query): Query<SessionQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    tokio::task::spawn_blocking(move || {
    let snapshot = state.snapshot()?;
    let covered = state.concealed.load(std::sync::atomic::Ordering::Acquire);
    if query.concealment || covered {
        return Ok(Json(
            serde_json::json!({"generation":snapshot.generation,"covered":covered}),
        ));
    }
    Ok(Json(
        serde_json::json!({ "generation": snapshot.generation, "covered":false, "scan": snapshot.scan_status(), "security": snapshot.security_status() }),
    ))
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

async fn get_folders_handler(
    Extension(state): Extension<RequestState>,
) -> Result<Json<FolderResponse>, StatusCode> {
    state
        .db_work(move |state| {
            Ok(Json(FolderResponse {
                config: load_config().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
                generation: state.generation,
                scan: state.scan_status(),
                security: state.security_status(),
                covered: false,
            }))
        })
        .await?
}

#[derive(Deserialize)]
struct SwitchFolderReq {
    path: String,
}

async fn open_library(path: PathBuf, state: &RequestState) -> Result<Arc<Library>, StatusCode> {
    let existing = state.library.clone();
    tokio::task::spawn_blocking(move || {
        let root = path.canonicalize()?;
        if root == existing.root_dir {
            return Ok(existing);
        }
        Library::open(root)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|e| {
        if e.to_string() == library_lock::UNAVAILABLE {
            StatusCode::LOCKED
        } else {
            StatusCode::BAD_REQUEST
        }
    })
}

fn install_library(
    state: &AppState,
    expected: u64,
    library: Arc<Library>,
    config: AppConfig,
) -> Result<Json<FolderResponse>, StatusCode> {
    install_library_at(state, expected, library, config, &get_config_path())
}

fn install_library_at(
    state: &AppState,
    expected: u64,
    library: Arc<Library>,
    config: AppConfig,
    config_path: &std::path::Path,
) -> Result<Json<FolderResponse>, StatusCode> {
    let generation;
    let covered = library.root_dir.as_os_str().is_empty();
    {
        let mut current = state
            .current
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if current.generation != expected {
            return Err(StatusCode::CONFLICT);
        }
        if current.integrity.is_running()
            || current.recovery.is_running()
            || current
                .archives
                .busy
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(StatusCode::CONFLICT);
        }
        save_config_at(config_path, &config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if Arc::ptr_eq(&current.library, &library) {
            return Ok(Json(FolderResponse {
                config,
                generation: current.generation,
                scan: current.scan_status(),
                security: current.security_status(),
                covered,
            }));
        }
        current.archives.retire();
        *state.restore_selection.lock().unwrap() = None;
        current.generation += 1;
        current.library = library.clone();
        generation = current.generation;
        state
            .concealed
            .store(covered, std::sync::atomic::Ordering::Release);
    }
    let security = library.security_status();
    let scan = state.start_scan(library);
    Ok(Json(FolderResponse {
        config,
        generation,
        scan,
        security,
        covered,
    }))
}

fn timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn switch_folder_handler(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(payload): Json<SwitchFolderReq>,
) -> Result<Json<FolderResponse>, StatusCode> {
    let mut config = load_config().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let folder = config
        .folders
        .iter_mut()
        .find(|f| f.path == payload.path)
        .ok_or(StatusCode::BAD_REQUEST)?;
    folder.last_used = timestamp();
    let library = open_library(PathBuf::from(&payload.path), &state).await?;
    config.active_folder = Some(payload.path);
    install_library(&app, state.generation, library, config)
}

async fn add_folder_handler(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> Result<Json<FolderResponse>, StatusCode> {
    let folder = tokio::task::spawn_blocking(|| rfd::FileDialog::new().pick_folder())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(path) = folder else {
        return Ok(Json(FolderResponse {
            config: load_config().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            generation: state.generation,
            scan: state.scan_status(),
            security: state.security_status(),
            covered: false,
        }));
    };
    let library = open_library(path.clone(), &state).await?;
    let folder_str = path.to_string_lossy().into_owned();
    let mut config = load_config().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(folder) = config.folders.iter_mut().find(|f| f.path == folder_str) {
        folder.last_used = timestamp();
    } else {
        config.folders.push(SavedFolder {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&folder_str)
                .to_string(),
            path: folder_str.clone(),
            last_used: timestamp(),
        });
    }
    config.active_folder = Some(folder_str);
    install_library(&app, state.generation, library, config)
}

async fn remove_folder_handler(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(payload): Json<SwitchFolderReq>,
) -> Result<Json<FolderResponse>, StatusCode> {
    remove_folder_at(app, state, payload, get_config_path()).await
}

async fn remove_folder_at(
    app: AppState,
    state: RequestState,
    payload: SwitchFolderReq,
    config_path: PathBuf,
) -> Result<Json<FolderResponse>, StatusCode> {
    let mut config = load_config_at(&config_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !config
        .folders
        .iter()
        .any(|folder| folder.path == payload.path)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    config.folders.retain(|f| f.path != payload.path);
    if config.active_folder.as_deref() == Some(payload.path.as_str()) {
        if config.folders.is_empty() {
            config.active_folder = None;
            return install_library_at(
                &app,
                state.generation,
                Library::unconfigured(),
                config,
                &config_path,
            );
        }
        let path = config.folders[0].path.clone();
        let library = open_library(PathBuf::from(&path), &state).await?;
        config.active_folder = Some(path);
        install_library_at(&app, state.generation, library, config, &config_path)
    } else {
        let current = app
            .current
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if current.generation != state.generation {
            return Err(StatusCode::CONFLICT);
        }
        save_config_at(&config_path, &config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(FolderResponse {
            config,
            generation: state.generation,
            scan: state.scan_status(),
            security: state.security_status(),
            covered: false,
        }))
    }
}

async fn rescan_folder_handler(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> Json<ScanStatus> {
    Json(app.start_scan(state.library))
}

async fn get_scan_status(Extension(state): Extension<RequestState>) -> Json<ScanStatus> {
    Json(state.scan_status())
}

async fn get_trash_count(
    Extension(state): Extension<RequestState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .db_work(move |state| {
            let guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let count = db::get_trash_count(guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(serde_json::json!({ "count": count })))
        })
        .await?
}

async fn reset_folder_watch_handler(
    Extension(state): Extension<RequestState>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, db::reset_all_watch_counts)
        })
        .await?
}

async fn reset_folder_scores_handler(
    Extension(state): Extension<RequestState>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, db::reset_all_scores)
        })
        .await?
}

async fn reset_folder_ratings_handler(
    Extension(state): Extension<RequestState>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, db::reset_all_ratings)
        })
        .await?
}

async fn backup_db_handler(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let local_time: String = state
        .db_work(move |state| {
            let guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            guard
                .as_ref()
                .ok_or(StatusCode::LOCKED)?
                .query_row(
                    "SELECT strftime('%Y%m%d_%H%M%S', 'now', 'localtime')",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await??;
    let folder_name: String = state
        .root_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("library")
        .chars()
        .take(60)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let folder_name = folder_name.trim_end_matches(['.', ' ']);
    let folder_name = if folder_name.is_empty() {
        "library"
    } else {
        folder_name
    };
    let default_name = format!("valtheum_{folder_name}_backup_{local_time}.vtbackup");

    let file = tokio::task::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("valtheum DB + Key (ZIP)", &["vtbackup"])
            .save_file()
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(dest_path) = file {
        let dest_str = dest_path.to_string_lossy().to_string();
        let key_included =
            tokio::task::spawn_blocking(move || backup_library_at(&app, &state, &dest_path))
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map_err(|error| {
                    eprintln!("백업 실패: {error:#}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        Ok(Json(
            serde_json::json!({ "status": "success", "backup_path": dest_str, "key_included": key_included, "media_included": false }),
        ))
    } else {
        Ok(Json(serde_json::json!({
            "status": "cancelled"
        })))
    }
}

// Reserve exclusive work under the state lock, then release it before any DB
// or filesystem wait. The reservation also keeps close's deferred cleanup from
// dropping the old connection/lease until the backup has unwound.
fn backup_library_at(
    app: &AppState,
    state: &RequestState,
    destination: &std::path::Path,
) -> anyhow::Result<bool> {
    use std::sync::atomic::Ordering;
    struct Reservation<'a>(&'a archive_security::Runtime);
    impl Drop for Reservation<'_> {
        fn drop(&mut self) {
            self.0.job.lock().unwrap_or_else(|e| e.into_inner()).running = false;
            self.0.busy.store(false, Ordering::Release);
        }
    }
    let (password, source, _reservation) = {
        let current = app
            .current
            .lock()
            .map_err(|_| anyhow::anyhow!("폴더 상태 오류"))?;
        anyhow::ensure!(
            current.generation == state.generation
                && !app.concealed.load(Ordering::Acquire)
                && !state.archives.revoked.load(Ordering::Acquire),
            "폴더가 변경되었습니다."
        );
        let security = current
            .security
            .try_lock()
            .map_err(|_| anyhow::anyhow!("잠금 상태 오류"))?;
        encryption::require_idle(&current)?;
        anyhow::ensure!(
            !security.encrypted || security.password.is_some(),
            "DB가 잠겨 있습니다."
        );
        let password = security.password.clone();
        let source = security.path(&current.root_dir);
        current.archives.busy.store(true, Ordering::Release);
        let reservation = Reservation(&state.archives);
        current.archives.begin(archive_security::Job {
            phase: "DB 백업 중…".into(),
            ..Default::default()
        });
        (password, source, reservation)
    };
    let checkpoint = || -> anyhow::Result<()> {
        anyhow::ensure!(
            !state.archives.revoked.load(Ordering::Acquire)
                && !state.archives.cancel.load(Ordering::Acquire),
            "백업이 취소되었습니다. 기존 백업은 보존됩니다."
        );
        Ok(())
    };
    let result = (|| {
        let guard = state
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("DB 연결 오류"))?;
        checkpoint()?;
        let conn = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DB가 잠겨 있습니다."))?;
        persistence::backup_bundle_checked(
            conn,
            &state.root_dir,
            &source,
            destination,
            password.as_deref().map(|s| s.as_str()),
            checkpoint,
        )
    })();
    {
        let mut job = state.archives.job.lock().unwrap_or_else(|e| e.into_inner());
        job.completed = result.is_ok();
        job.error = result.as_ref().err().map(|e| format!("{e:#}"));
    }
    result
}

fn series_error(error: series::Error) -> (StatusCode, &'static str) {
    match error {
        series::Error::Invalid(message) => (StatusCode::BAD_REQUEST, message),
        series::Error::Conflict(message) => (StatusCode::CONFLICT, message),
        series::Error::NotFound => (
            StatusCode::NOT_FOUND,
            "시리즈 또는 파일을 찾을 수 없습니다.",
        ),
        series::Error::Database(error) => {
            eprintln!("Series database error: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "시리즈 정보를 저장하지 못했습니다.",
            )
        }
    }
}

async fn list_series_handler(
    Extension(state): Extension<RequestState>,
) -> Result<Json<Vec<series::Series>>, StatusCode> {
    state
        .db_work(move |state| {
            let guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            series::list(guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?)
                .map(Json)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await?
}

async fn save_series_handler(
    Extension(state): Extension<RequestState>,
    headers: HeaderMap,
    Json(input): Json<series::SaveSeries>,
) -> Result<Json<serde_json::Value>, (StatusCode, &'static str)> {
    let deadline = metadata_deadline(&headers).map_err(|s| (s, "잘못된 요청 기한"))?;
    state
        .db_write(move |state| {
            let guard = state
                .lock_db_for_write()
                .map_err(|status| (status, "데이터베이스 오류"))?;
            check_metadata_deadline(deadline).map_err(|s| (s, "요청 기한이 지났습니다."))?;
            let id = series::save(
                guard.as_ref().ok_or((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "라이브러리를 사용할 수 없습니다.",
                ))?,
                &input,
            )
            .map_err(series_error)?;
            Ok(Json(serde_json::json!({"id":id})))
        })
        .await
        .map_err(|status| (status, "데이터베이스를 사용할 수 없습니다."))?
}

async fn dissolve_series_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let deadline = metadata_deadline(&headers)?;
    state
        .db_write(move |state| {
            let guard = state.lock_db_for_write()?;
            check_metadata_deadline(deadline)?;
            let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            series::dissolve(conn, id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(StatusCode::OK)
        })
        .await?
}

#[derive(Deserialize)]
struct FilesQuery {
    sort_by: Option<String>,
    order: Option<String>,
}

async fn get_files(
    Extension(state): Extension<RequestState>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Vec<db::MediaFile>>, StatusCode> {
    state
        .db_work(move |state| {
            let conn_guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(ref conn) = *conn_guard {
                let files =
                    db::get_media_files(conn, query.sort_by.as_deref(), query.order.as_deref())
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                Ok(Json(files))
            } else {
                Ok(Json(vec![]))
            }
        })
        .await?
}

#[derive(Deserialize, Default)]
struct WatchReq {
    timestamp: Option<i64>,
}

#[derive(Deserialize)]
struct BookmarkReq {
    position: f64,
    thumbnail_data: String,
    request_id: Option<String>,
}

fn bookmark_status(error: db::BookmarkError) -> StatusCode {
    match error {
        db::BookmarkError::NotFound => StatusCode::NOT_FOUND,
        db::BookmarkError::InvalidPosition | db::BookmarkError::InvalidImage => {
            StatusCode::BAD_REQUEST
        }
        db::BookmarkError::ImageTooLarge | db::BookmarkError::LimitReached => {
            StatusCode::PAYLOAD_TOO_LARGE
        }
        db::BookmarkError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn bookmark_error_response(error: db::BookmarkError) -> (StatusCode, &'static str) {
    match error {
        db::BookmarkError::NotFound => (
            StatusCode::NOT_FOUND,
            "미디어 또는 북마크를 찾을 수 없습니다.",
        ),
        db::BookmarkError::InvalidPosition => (
            StatusCode::BAD_REQUEST,
            "북마크 위치는 유한한 0 이상의 값이어야 하며, 아카이브는 정수여야 합니다.",
        ),
        db::BookmarkError::InvalidImage => {
            (StatusCode::BAD_REQUEST, "썸네일은 JPEG 이미지여야 합니다.")
        }
        db::BookmarkError::ImageTooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "썸네일은 128KiB 이하여야 합니다.",
        ),
        db::BookmarkError::LimitReached => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "미디어 파일당 북마크는 최대 100개까지 저장할 수 있습니다.",
        ),
        db::BookmarkError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "북마크를 저장하지 못했습니다.",
        ),
    }
}

async fn list_bookmarks_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<db::Bookmark>>, StatusCode> {
    state
        .db_work(move |state| {
            let guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            db::list_bookmarks(conn, id)
                .map(Json)
                .map_err(bookmark_status)
        })
        .await?
}

async fn create_bookmark_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<BookmarkReq>,
) -> Result<Json<db::Bookmark>, (StatusCode, &'static str)> {
    let deadline = metadata_deadline(&headers).map_err(|s| (s, "잘못된 요청 기한"))?;
    if payload
        .request_id
        .as_ref()
        .is_some_and(|id| id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err((StatusCode::BAD_REQUEST, "잘못된 요청 번호"));
    }
    state
        .db_write(move |state| {
            // A 128 KiB decoded image has at most this many base64 characters.
            const MAX_BASE64_THUMBNAIL: usize = (128_usize * 1024).div_ceil(3) * 4;
            if payload.thumbnail_data.len() > MAX_BASE64_THUMBNAIL {
                return Err((
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "썸네일은 128KiB 이하여야 합니다.",
                ));
            }
            let image = base64::engine::general_purpose::STANDARD
                .decode(payload.thumbnail_data.as_bytes())
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "thumbnail_data는 base64 형식이어야 합니다.",
                    )
                })?;
            let guard = state
                .lock_db_for_write()
                .map_err(|status| (status, "데이터베이스를 사용할 수 없습니다."))?;
            let conn = guard.as_ref().ok_or((
                StatusCode::SERVICE_UNAVAILABLE,
                "미디어 라이브러리가 준비되지 않았습니다.",
            ))?;
            check_metadata_deadline(deadline).map_err(|s| (s, "요청 기한이 지났습니다."))?;
            let tx = conn
                .unchecked_transaction()
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "저장 실패"))?;
            let mut bookmark = db::save_bookmark(&tx, id, payload.position, &image)
                .map_err(bookmark_error_response)?;
            tx.execute(
                "UPDATE media_bookmarks SET request_id=?1 WHERE id=?2",
                rusqlite::params![payload.request_id, bookmark.id],
            )
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "저장 실패"))?;
            tx.commit()
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "저장 실패"))?;
            bookmark.request_id = payload.request_id;
            Ok(Json(bookmark))
        })
        .await
        .map_err(|status| (status, "데이터베이스를 사용할 수 없습니다."))?
}

async fn delete_bookmark_handler(
    Extension(state): Extension<RequestState>,
    Path((id, bookmark_id)): Path<(i64, i64)>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let deadline = metadata_deadline(&headers)?;
    state
        .db_write(move |state| {
            let guard = state.lock_db_for_write()?;
            let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            check_metadata_deadline(deadline)?;
            db::delete_bookmark(conn, id, bookmark_id)
                .map(|_| StatusCode::NO_CONTENT)
                .map_err(bookmark_status)
        })
        .await?
}

async fn get_bookmark_thumbnail_handler(
    Extension(state): Extension<RequestState>,
    Path((id, bookmark_id)): Path<(i64, i64)>,
) -> Result<Response, StatusCode> {
    state
        .db_work(move |state| {
            let guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            match db::get_bookmark_thumbnail(conn, id, bookmark_id).map_err(bookmark_status)? {
                Some(bytes) => Ok(([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response()),
                None => Err(StatusCode::NOT_FOUND),
            }
        })
        .await?
}

#[derive(Deserialize, Serialize)]
struct PdfPosition {
    page: Option<u32>,
    #[serde(default)]
    revision: i64,
}

async fn pdf_position(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<Json<PdfPosition>, StatusCode> {
    state.db_work(move |state| {
        let guard = state.conn.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        conn.query_row(
            "SELECT pdf_last_page,pdf_position_revision FROM media_files WHERE id=?1 AND file_type='PDF' AND COALESCE(is_trash,0)=0",
            [id], |row| Ok(Json(PdfPosition { page: row.get(0)?, revision: row.get(1)? })),
        ).map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })
    }).await?
}

async fn save_pdf_position(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    Json(payload): Json<PdfPosition>,
) -> Result<StatusCode, StatusCode> {
    if payload.revision < 0 || payload.revision >= i64::MAX {
        return Err(StatusCode::BAD_REQUEST);
    }
    let page = payload
        .page
        .filter(|page| *page > 0)
        .ok_or(StatusCode::BAD_REQUEST)?;
    state.db_write(move |state| {
        let guard = state.lock_db_for_write()?;
        let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        let changed = conn.execute(
            "UPDATE media_files SET pdf_last_page=?1,pdf_position_revision=pdf_position_revision+1 WHERE id=?2 AND file_type='PDF' AND COALESCE(is_trash,0)=0 AND is_missing=0 AND pdf_position_revision=?3",
            rusqlite::params![page, id, payload.revision],
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if changed == 0 {
            let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM media_files WHERE id=?1 AND file_type='PDF' AND COALESCE(is_trash,0)=0 AND is_missing=0)", [id], |row| row.get(0)).map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
            Err(if exists { StatusCode::CONFLICT } else { StatusCode::NOT_FOUND })
        } else { Ok(StatusCode::NO_CONTENT) }
    }).await?
}

async fn record_watch(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    Json(payload): Json<Option<WatchReq>>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            let timestamp = payload.and_then(|p| p.timestamp).unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0)
            });

            state.update_db(StatusCode::INTERNAL_SERVER_ERROR, |conn| {
                db::update_last_watched(conn, id, timestamp)
            })
        })
        .await?
}

async fn reset_watch_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, |conn| {
                db::reset_watch_count(conn, id)
            })
        })
        .await?
}

#[derive(Deserialize)]
struct RatingReq {
    rating: i64,
}

// A timed-out client can release its metadata queue. Reject its old write after
// acquiring the DB lock so it cannot later overwrite a newer change. A write
// already in progress retains that lock until it finishes.
fn metadata_deadline(headers: &HeaderMap) -> Result<Option<u128>, StatusCode> {
    headers
        .get("X-Vault-Write-Deadline")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|text| text.parse().ok())
                .ok_or(StatusCode::BAD_REQUEST)
        })
        .transpose()
}

fn check_metadata_deadline(deadline: Option<u128>) -> Result<(), StatusCode> {
    if let Some(deadline) = deadline {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .as_millis();
        if now >= deadline {
            return Err(StatusCode::REQUEST_TIMEOUT);
        }
    }
    Ok(())
}

async fn update_rating_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<RatingReq>,
) -> Result<StatusCode, StatusCode> {
    let deadline = metadata_deadline(&headers)?;
    state
        .db_write(move |state| {
            let guard = state.lock_db_for_write()?;
            check_metadata_deadline(deadline)?;
            let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            db::update_rating(conn, id, payload.rating)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(StatusCode::OK)
        })
        .await?
}

#[derive(Deserialize)]
struct ArenaPairQuery {
    focus_id: Option<i64>,
}

async fn get_arena_pair_handler(
    Extension(state): Extension<RequestState>,
    Query(query): Query<ArenaPairQuery>,
) -> Result<Json<Option<(db::MediaFile, db::MediaFile)>>, StatusCode> {
    state
        .db_work(move |state| {
            let conn_guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(ref conn) = *conn_guard {
                let pair = if let Some(focus_id) = query.focus_id {
                    db::get_arena_opponent(conn, focus_id)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                } else {
                    db::get_arena_pair(conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                };
                Ok(Json(pair))
            } else {
                Ok(Json(None))
            }
        })
        .await?
}

#[derive(Deserialize)]
struct VoteReq {
    winner_id: i64,
    loser_id: i64,
}

#[derive(serde::Serialize)]
struct VoteRes {
    winner_score: i64,
    loser_score: i64,
    winner_delta: i64,
    loser_delta: i64,
}

async fn vote_arena_handler(
    Extension(state): Extension<RequestState>,
    Json(payload): Json<VoteReq>,
) -> Result<Json<VoteRes>, StatusCode> {
    state
        .db_write(move |state| {
            let conn_guard = state.lock_db_for_write()?;
            if let Some(ref conn) = *conn_guard {
                let (winner_score, loser_score, winner_delta, loser_delta) =
                    db::vote_arena(conn, payload.winner_id, payload.loser_id).map_err(|error| {
                        match error {
                            rusqlite::Error::InvalidParameterName(_) => StatusCode::BAD_REQUEST,
                            rusqlite::Error::QueryReturnedNoRows => StatusCode::NOT_FOUND,
                            _ => StatusCode::INTERNAL_SERVER_ERROR,
                        }
                    })?;
                Ok(Json(VoteRes {
                    winner_score,
                    loser_score,
                    winner_delta,
                    loser_delta,
                }))
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        })
        .await?
}

async fn serve_media(
    Extension(state): Extension<RequestState>,
    Path(path): Path<String>,
    req: axum::extract::Request,
) -> Result<Response, StatusCode> {
    if let Some(response) = archive_security::video_http::serve(
        &state,
        &path,
        req.method().clone(),
        req.headers().clone(),
    )
    .await?
    {
        return Ok(response);
    }
    // Plain media reads do not touch SQLite. A stalled NAS DB query must not
    // serialize PDF byte ranges (or video/image reads) behind its DB permit.
    let revoked = state.archives.revoked.clone();
    let permit = state
        .file_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    let file_path = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if state
            .archives
            .revoked
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(StatusCode::CONFLICT);
        }
        let file_path = resolve_path(&state.root_dir, &path)?;
        // Database and configuration files must never be served as media.
        if !matches!(
            file_path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some(
                "mp4"
                    | "webm"
                    | "mkv"
                    | "avi"
                    | "mov"
                    | "m4v"
                    | "wmv"
                    | "jpg"
                    | "jpeg"
                    | "png"
                    | "webp"
                    | "gif"
                    | "bmp"
                    | "avif"
                    | "pdf"
                    | "vtt"
            )
        ) {
            return Err(StatusCode::FORBIDDEN);
        }
        if !file_path.exists() {
            return Err(StatusCode::NOT_FOUND);
        }

        if state
            .archives
            .revoked
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(StatusCode::CONFLICT);
        }

        Ok(file_path)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let serve_file = ServeFile::new(file_path);
    let res = serve_file
        .oneshot(req)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(media_stream::protect(res.into_response(), revoked))
}

fn archive_error(error: anyhow::Error) -> (StatusCode, String) {
    let message = error.to_string();
    let status = if message.contains("unsupported archive") {
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    } else if message.contains("32 MiB") {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    (status, message)
}

async fn get_archive_list(
    Extension(state): Extension<RequestState>,
    Path(path): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let permit = state
        .archive_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "작업 큐가 종료되었습니다.".into(),
            )
        })?;
    let entries = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        archive_security::list(&state, &path)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(archive_error)?;
    Ok(Json(entries))
}

#[derive(Deserialize)]
struct ArchiveFileQuery {
    entry: String,
}

async fn get_archive_file(
    Extension(state): Extension<RequestState>,
    Path(path): Path<String>,
    Query(query): Query<ArchiveFileQuery>,
) -> Result<Response, (StatusCode, String)> {
    let mime_type = mime_guess::from_path(&query.entry)
        .first_or_octet_stream()
        .to_string();
    let permit = state
        .archive_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "작업 큐가 종료되었습니다.".into(),
            )
        })?;
    let bytes = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        archive_security::image(&state, &path, &query.entry)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(archive_error)?;
    Ok(([(header::CONTENT_TYPE, mime_type)], bytes).into_response())
}

#[derive(Deserialize)]
struct AddTagReq {
    name: String,
}

async fn add_tag(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<AddTagReq>,
) -> Result<Json<db::Tag>, StatusCode> {
    let deadline = metadata_deadline(&headers)?;
    state
        .db_write(move |state| {
            let conn_guard = state.lock_db_for_write()?;
            check_metadata_deadline(deadline)?;
            if let Some(ref conn) = *conn_guard {
                let tag = db::add_tag_to_file(conn, id, &payload.name)
                    .map_err(|_| StatusCode::BAD_REQUEST)?;
                Ok(Json(tag))
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        })
        .await?
}

async fn remove_tag(
    Extension(state): Extension<RequestState>,
    Path((id, tag_id)): Path<(i64, i64)>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let deadline = metadata_deadline(&headers)?;
    state
        .db_write(move |state| {
            let guard = state.lock_db_for_write()?;
            check_metadata_deadline(deadline)?;
            let conn = guard.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            db::remove_tag_from_file(conn, id, tag_id)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(StatusCode::OK)
        })
        .await?
}

async fn upload_thumbnail(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let deadline = metadata_deadline(&headers)?;
    let mut data_bytes = None;
    let mut width = 320;
    let mut height = 180;
    let mut time_offset = None;
    let mut page_offset = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "image" {
            if let Ok(bytes) = field.bytes().await {
                data_bytes = Some(bytes.to_vec());
            }
        } else if name == "width" {
            if let Ok(text) = field.text().await {
                width = text.parse::<i32>().unwrap_or(320);
            }
        } else if name == "height" {
            if let Ok(text) = field.text().await {
                height = text.parse::<i32>().unwrap_or(180);
            }
        } else if name == "time_offset" {
            if let Ok(text) = field.text().await {
                time_offset = text.parse::<f64>().ok();
            }
        } else if name == "page_offset" {
            if let Ok(text) = field.text().await {
                page_offset = text.parse::<i64>().ok();
            }
        }
    }

    state
        .db_write(move |state| {
            if let Some(data) = data_bytes {
                let conn_guard = state.lock_db_for_write()?;
                check_metadata_deadline(deadline)?;
                if let Some(ref conn) = *conn_guard {
                    db::save_thumbnail(conn, id, &data, width, height, time_offset, page_offset)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    println!(
                        "Thumbnail saved for file id: {} (time: {:?}, page: {:?})",
                        id, time_offset, page_offset
                    );
                    Ok(StatusCode::OK)
                } else {
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        })
        .await?
}

async fn get_thumbnail(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<Response, StatusCode> {
    state
        .db_work(move |state| {
            let conn_guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(ref conn) = *conn_guard {
                if let Some(bytes) =
                    db::get_thumbnail(conn, id).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
                {
                    let mut res = Response::new(axum::body::Body::from(bytes));
                    res.headers_mut()
                        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
                    return Ok(res);
                }
            } else {
                return Err(StatusCode::LOCKED);
            }
            Err(StatusCode::NOT_FOUND)
        })
        .await?
}

#[derive(Deserialize)]
struct DurationReq {
    duration_secs: i64,
}

async fn update_duration(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    Json(payload): Json<DurationReq>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::INTERNAL_SERVER_ERROR, |conn| {
                db::update_duration(conn, id, payload.duration_secs)
            })
        })
        .await?
}

async fn move_to_trash_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, |conn| {
                db::move_to_trash(conn, id)
            })
        })
        .await?
}

async fn restore_from_trash_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, |conn| {
                db::restore_from_trash(conn, id)
            })
        })
        .await?
}

async fn permanently_delete_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .db_write(move |state| {
            state.archives.clear_cache();
            let guard = state
                .lock_db_for_write()
                .map_err(|s| (s, "DB 작업을 처리할 수 없습니다.".into()))?;
            let conn = guard.as_ref().ok_or((
                StatusCode::SERVICE_UNAVAILABLE,
                "DB 잠금을 해제해 주세요.".into(),
            ))?;
            db::permanently_delete_file(conn, &state.root_dir, id).map_err(|error| {
                let status = if error.is::<db::NotInTrash>() {
                    StatusCode::CONFLICT
                } else if matches!(
                    error.downcast_ref::<rusqlite::Error>(),
                    Some(rusqlite::Error::QueryReturnedNoRows)
                ) {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::BAD_REQUEST
                };
                (status, error.to_string())
            })?;
            Ok(StatusCode::OK)
        })
        .await
        .map_err(|s| (s, "DB 작업을 처리할 수 없습니다.".into()))?
}

async fn get_trash_files_handler(
    Extension(state): Extension<RequestState>,
) -> Result<Json<Vec<db::MediaFile>>, StatusCode> {
    state
        .db_work(move |state| {
            let conn_guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(ref conn) = *conn_guard {
                let files =
                    db::get_trash_files(conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                Ok(Json(files))
            } else {
                Ok(Json(vec![]))
            }
        })
        .await?
}

async fn delete_deleted_log_handler(
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, |conn| {
                db::delete_deleted_log(conn, id)
            })
        })
        .await?
}

async fn clear_deleted_logs_handler(
    Extension(state): Extension<RequestState>,
) -> Result<StatusCode, StatusCode> {
    state
        .db_write(move |state| {
            state.update_db(StatusCode::SERVICE_UNAVAILABLE, db::clear_deleted_logs)
        })
        .await?
}

async fn get_deleted_logs_handler(
    Extension(state): Extension<RequestState>,
) -> Result<Json<Vec<db::DeletedFileLog>>, StatusCode> {
    state
        .db_work(move |state| {
            let conn_guard = state
                .conn
                .lock()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(ref conn) = *conn_guard {
                let logs =
                    db::get_deleted_logs(conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                Ok(Json(logs))
            } else {
                Ok(Json(vec![]))
            }
        })
        .await?
}

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    if let Err(status) = state::validate_relative(&path) {
        return status.into_response();
    }
    // Only development builds may override embedded UI from disk. The working
    // directory is not a trusted source of scripts in a distributed executable.
    #[cfg(debug_assertions)]
    if let Ok(disk_path) = resolve_path(&PathBuf::from("static"), &path) {
        if let Ok(data) = std::fs::read(&disk_path) {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            return ([(header::CONTENT_TYPE, mime.as_ref())], data).into_response();
        }
    }

    // Release builds always serve the assets embedded at build time.
    match Assets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

#[cfg(test)]
mod api_tests;

#[cfg(test)]
mod config_tests;
