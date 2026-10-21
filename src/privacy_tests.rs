use super::*;
use crate::{db, AppConfig, SavedFolder};

struct Fixture {
    root: PathBuf,
    app: AppState,
    config: PathBuf,
    a: String,
    b: String,
    plain: String,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "vault-privacy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut folders = vec![];
        for (name, password) in [
            ("A", Some("folder A password")),
            ("B", Some("folder B password")),
            ("plain", None),
        ] {
            let path = root.join(name);
            std::fs::create_dir(&path).unwrap();
            let conn = encryption::open(
                &path.join(if password.is_some() {
                    encryption::SECURE_DB
                } else {
                    encryption::PLAIN_DB
                }),
                password,
                true,
            )
            .unwrap();
            db::init_db(&conn).unwrap();
            db::upsert_media_file(&conn, &format!("{name}.png"), "IMAGE", 5, 0).unwrap();
            folders.push(SavedFolder {
                path: path.to_string_lossy().into_owned(),
                name: name.into(),
                last_used: 0,
            });
        }
        let a = folders[0].path.clone();
        let b = folders[1].path.clone();
        let plain = folders[2].path.clone();
        let config = root.join("config.json");
        crate::save_config_at(
            &config,
            &AppConfig {
                folders,
                active_folder: Some(a.clone()),
                gallery_folder: None,
                gallery_frame_seconds: 10,
                language: "ko".into(),
            },
        )
        .unwrap();
        let app = AppState::new(Library::open(PathBuf::from(&a)).unwrap()).unwrap();
        Self {
            root,
            app,
            config,
            a,
            b,
            plain,
        }
    }
    async fn open(&self, path: &str, password: &str, generation: u64) -> ApiResult {
        open_at(
            self.app.clone(),
            generation,
            OpenFolder {
                path: path.into(),
                password: password.into(),
            },
            self.config.clone(),
        )
        .await
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.app.snapshot().unwrap().conn.lock().unwrap().take();
        self.app.current.lock().unwrap().library = Library::unconfigured();
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("vault-privacy-"));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn selected_folder_authenticates_its_own_password_and_commits_only_after_success() {
    let f = Fixture::new();
    let choices = choices_at(&f.app, &f.config).unwrap().0;
    assert_eq!(choices["folders"].as_array().unwrap().len(), 2);
    assert_eq!(choices["active_folder"], f.a);
    let saved = std::fs::read(&f.config).unwrap();
    for (path, password) in [
        (&f.b, "folder A password"),
        (&f.plain, "folder A password"),
        (&f.root.to_string_lossy().into_owned(), "folder A password"),
    ] {
        let error = f.open(path, password, 1).await.unwrap_err();
        assert_eq!(error, FAILURE);
        assert_eq!(std::fs::read(&f.config).unwrap(), saved);
        assert_eq!(f.app.snapshot().unwrap().generation, 1);
        assert_eq!(
            f.app.snapshot().unwrap().root_dir,
            PathBuf::from(&f.a).canonicalize().unwrap()
        );
        assert!(f.app.concealed.load(Ordering::Acquire));
    }
    assert_eq!(
        f.open(&f.b, "folder B password", 0).await.unwrap_err().0,
        StatusCode::CONFLICT
    );
    let old = f.app.snapshot().unwrap();
    let response = f.open(&f.b, "folder B password", 1).await.unwrap().0;
    assert_eq!(response["covered"], false);
    assert_eq!(response["generation"], 2);
    assert_eq!(
        crate::load_config_at(&f.config).unwrap().active_folder,
        Some(f.b.clone())
    );
    assert!(!f.app.concealed.load(Ordering::Acquire));
    assert!(old.conn.lock().unwrap().is_none());
    let state = f.app.snapshot().unwrap();
    assert_eq!(state.root_dir, PathBuf::from(&f.b).canonicalize().unwrap());
    let conn = state.conn.lock().unwrap();
    let path: String = conn
        .as_ref()
        .unwrap()
        .query_row("SELECT rel_path FROM media_files", [], |row| row.get(0))
        .unwrap();
    assert_eq!(path, "B.png");
    assert_eq!(
        f.open(&f.a, "folder A password", 1).await.unwrap_err().0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn unavailable_folder_and_config_write_failure_leave_current_library_concealed() {
    let f = Fixture::new();
    std::fs::rename(
        PathBuf::from(&f.b).join(encryption::SECURE_DB),
        f.root.join("offline.db"),
    )
    .unwrap();
    assert_eq!(
        choices_at(&f.app, &f.config).unwrap().0["folders"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        f.open(&f.b, "folder B password", 1).await.unwrap_err(),
        FAILURE
    );
    assert!(!PathBuf::from(&f.b).join(encryption::PLAIN_DB).exists());
    // The backup path being a directory forces atomic config persistence to fail.
    std::fs::create_dir(f.config.with_extension("json.bak")).unwrap();
    assert_eq!(
        f.open(&f.a, "folder A password", 1).await.unwrap_err(),
        FAILURE
    );
    assert!(f.app.concealed.load(Ordering::Acquire));
    assert!(f.app.snapshot().unwrap().conn.lock().unwrap().is_none());
    assert_eq!(f.app.snapshot().unwrap().generation, 1);
}
#[tokio::test]
async fn missing_config_starts_concealed_without_creating_a_database_and_allows_native_folder_setup(
) {
    let mut f = Fixture::new();
    std::fs::remove_file(&f.config).unwrap();
    f.app = crate::startup_state_at(&f.config).unwrap();
    assert!(f.app.concealed.load(Ordering::Acquire));
    assert!(f.app.snapshot().unwrap().root_dir.as_os_str().is_empty());
    assert!(f.app.snapshot().unwrap().conn.lock().unwrap().is_none());
    assert!(!f.config.exists());
    assert!(!f.root.join(encryption::PLAIN_DB).exists());
    assert!(choices_at(&f.app, &f.config).unwrap().0["folders"]
        .as_array()
        .unwrap()
        .is_empty());
    let api = crate::build_router(f.app.clone());
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let request = |path: &str| {
        Request::builder()
            .uri(path)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", f.app.token.as_str())
            .header("x-vault-library", "1")
            .body(Body::empty())
            .unwrap()
    };
    let response = api.clone().oneshot(request("/api/session")).await.unwrap();
    let session: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(session["covered"], true);
    assert_eq!(session.as_object().unwrap().len(), 2);
    assert_eq!(
        api.oneshot(request("/api/files")).await.unwrap().status(),
        StatusCode::LOCKED
    );
    let new_folder = f.root.join("new-library");
    std::fs::create_dir(&new_folder).unwrap();
    assert!(
        f.open(new_folder.to_str().unwrap(), "", 1).await.is_err(),
        "unselected client paths must be rejected"
    );
    let selection = select_at(&f.app, 1, new_folder.clone()).unwrap().0;
    assert_eq!(selection["folder"]["encrypted"], false);
    assert!(!new_folder.join(encryption::PLAIN_DB).exists());
    assert!(!f.config.exists());
    let path = selection["folder"]["path"].as_str().unwrap();
    let response = f.open(path, "", 1).await.unwrap().0;
    assert_eq!(response["covered"], false);
    assert_eq!(response["security"]["encrypted"], false);
    assert!(new_folder.join(encryption::PLAIN_DB).exists());
    let config = crate::load_config_at(&f.config).unwrap();
    assert_eq!(config.active_folder.as_deref(), Some(path));
    assert_eq!(config.folders.len(), 1);
    assert_eq!(config.folders[0].path, path);
}

#[tokio::test]
async fn first_run_can_select_an_existing_encrypted_library_and_requires_its_password() {
    let mut f = Fixture::new();
    std::fs::remove_file(&f.config).unwrap();
    f.app = crate::startup_state_at(&f.config).unwrap();
    let selected = select_at(&f.app, 1, PathBuf::from(&f.b)).unwrap().0;
    assert_eq!(selected["folder"]["encrypted"], true);
    let path = selected["folder"]["path"].as_str().unwrap();
    for password in ["", "folder A password"] {
        assert!(f.open(path, password, 1).await.is_err());
        assert!(f.app.concealed.load(Ordering::Acquire));
        assert!(!f.config.exists());
        assert!(f.app.snapshot().unwrap().root_dir.as_os_str().is_empty());
    }
    assert!(select_at(&f.app, 0, PathBuf::from(&f.a)).is_err());
    assert_eq!(
        f.open(path, "folder B password", 1).await.unwrap().0["covered"],
        false
    );
    assert_eq!(
        crate::load_config_at(&f.config)
            .unwrap()
            .active_folder
            .as_deref(),
        Some(path)
    );
}

#[test]
fn missing_primary_config_with_a_plain_library_backup_and_gallery_only_config_both_start_concealed()
{
    let f = Fixture::new();
    let mut config = crate::load_config_at(&f.config).unwrap();
    config.active_folder = Some(f.plain.clone());
    std::fs::write(
        f.config.with_extension("json.bak"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    std::fs::remove_file(&f.config).unwrap();
    let recovered = crate::startup_state_at(&f.config).unwrap();
    assert!(recovered.concealed.load(Ordering::Acquire));
    assert!(recovered.snapshot().unwrap().conn.lock().unwrap().is_none());
    assert!(f.config.exists());
    assert!(crate::load_config_at(&f.config).unwrap().folders.is_empty());
    let empty = AppConfig {
        gallery_folder: Some(f.root.join("photos")),
        ..Default::default()
    };
    crate::save_config_at(&f.config, &empty).unwrap();
    let gallery_only = crate::startup_state_at(&f.config).unwrap();
    assert!(gallery_only.concealed.load(Ordering::Acquire));
    assert!(gallery_only
        .snapshot()
        .unwrap()
        .root_dir
        .as_os_str()
        .is_empty());
    assert!(!f.root.join(encryption::PLAIN_DB).exists());
}
#[tokio::test]
async fn picking_a_registered_folder_again_preserves_its_saved_path_without_duplicates() {
    let f = Fixture::new();
    let selection = select_at(&f.app, 1, PathBuf::from(&f.b)).unwrap().0;
    let path = selection["folder"]["path"].as_str().unwrap();
    let _ = f.open(path, "folder B password", 1).await.unwrap();
    let config = crate::load_config_at(&f.config).unwrap();
    assert_eq!(config.folders.len(), 3);
    assert_eq!(config.active_folder, Some(f.b.clone()));
}

#[tokio::test]
async fn removing_the_last_folder_clears_registration_and_returns_to_decoy_without_deleting_data() {
    let f = Fixture::new();
    let _ = f.open(&f.a, "folder A password", 1).await.unwrap();
    let mut config = crate::load_config_at(&f.config).unwrap();
    config.folders.retain(|folder| folder.path == f.a);
    config.gallery_folder = Some(f.root.join("photos"));
    config.gallery_frame_seconds = 23;
    crate::save_config_at(&f.config, &config).unwrap();
    let original = PathBuf::from(&f.a).join("keep.zip");
    std::fs::write(&original, b"original media fixture").unwrap();
    let database = PathBuf::from(&f.a).join(encryption::SECURE_DB);
    let db_before = std::fs::read(&database).unwrap();
    let snapshot = f.app.snapshot().unwrap();
    let mut stale = snapshot.clone();
    stale.generation -= 1;
    let rejected = crate::remove_folder_at(
        f.app.clone(),
        stale,
        crate::SwitchFolderReq { path: f.a.clone() },
        f.config.clone(),
    )
    .await;
    assert_eq!(rejected.err().unwrap(), StatusCode::CONFLICT);
    assert_eq!(crate::load_config_at(&f.config).unwrap().folders.len(), 1);
    let result = crate::remove_folder_at(
        f.app.clone(),
        snapshot.clone(),
        crate::SwitchFolderReq { path: f.a.clone() },
        f.config.clone(),
    )
    .await
    .unwrap()
    .0;
    assert!(result.covered);
    assert_eq!(result.generation, snapshot.generation + 1);
    assert!(result.config.folders.is_empty());
    assert!(result.config.active_folder.is_none());
    let saved = crate::load_config_at(&f.config).unwrap();
    assert!(saved.folders.is_empty());
    assert!(saved.active_folder.is_none());
    assert_eq!(saved.gallery_folder, config.gallery_folder);
    assert_eq!(saved.gallery_frame_seconds, 23);
    let current = f.app.snapshot().unwrap();
    assert!(current.root_dir.as_os_str().is_empty());
    assert!(current.conn.lock().unwrap().is_none());
    assert!(f.app.concealed.load(Ordering::Acquire));
    assert_eq!(std::fs::read(original).unwrap(), b"original media fixture");
    assert_eq!(std::fs::read(database).unwrap(), db_before);
    assert!(crate::startup_state_at(&f.config)
        .unwrap()
        .concealed
        .load(Ordering::Acquire));
}

#[tokio::test]
async fn library_in_use_is_reported_without_changing_the_selected_library() {
    let f = Fixture::new();
    let other = Library::open(PathBuf::from(&f.b)).unwrap();
    let result = f.open(&f.b, "folder B password", 1).await.unwrap_err();
    assert_eq!(
        result,
        (StatusCode::LOCKED, crate::library_lock::UNAVAILABLE)
    );
    assert_eq!(
        f.app.snapshot().unwrap().root_dir,
        PathBuf::from(&f.a).canonicalize().unwrap()
    );
    assert_eq!(f.app.snapshot().unwrap().generation, 1);
    drop(other);
    assert!(f.open(&f.b, "folder B password", 1).await.is_ok());
}

#[test]
fn opening_blocked_database_can_be_closed_without_late_publication() {
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let previous = f.app.snapshot().unwrap();
    let original_config = std::fs::read(&f.config).unwrap();
    let db = encryption::open(
        &PathBuf::from(&f.b).join(encryption::SECURE_DB),
        Some("folder B password"),
        false,
    )
    .unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE")
        .unwrap();
    let app = f.app.clone();
    let config = f.config.clone();
    let path = f.b.clone();
    let opening = rt.spawn(open_at(
        app,
        1,
        OpenFolder {
            path,
            password: "folder B password".into(),
        },
        config,
    ));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !previous.archives.busy.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = previous.archives.busy.load(Ordering::Acquire);
    let duplicate = rt.block_on(f.open(&f.a, "folder A password", 1));
    let close = rt.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(2),
            crate::security_api::close(State(f.app.clone()), Extension(previous.clone())),
        )
        .await
    });
    // Always release the SQLite lock before any assertion can panic.
    db.execute_batch("ROLLBACK").unwrap();
    drop(db);
    let result = rt.block_on(opening).unwrap();
    assert!(started);
    assert!(duplicate.is_err());
    assert!(close.unwrap().is_ok());
    assert_eq!(result.unwrap_err().0, StatusCode::CONFLICT);
    assert_eq!(f.app.snapshot().unwrap().generation, 2);
    assert!(f.app.snapshot().unwrap().root_dir.as_os_str().is_empty());
    assert!(f.app.concealed.load(Ordering::Acquire));
    assert_eq!(std::fs::read(&f.config).unwrap(), original_config);
    assert_eq!(
        crate::load_config_at(&f.config).unwrap().active_folder,
        Some(f.a.clone())
    );
    assert!(!previous.archives.busy.load(Ordering::Acquire));
}
