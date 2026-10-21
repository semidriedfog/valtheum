use super::*;
use crate::state::Library;
use axum::{
    body::{to_bytes, Body},
    http::Request as HttpRequest,
};
struct Fixture {
    root: PathBuf,
    config: PathBuf,
    app: AppState,
}

#[test]
fn stalled_gallery_scan_releases_locks_and_cannot_publish_after_clear_reveal_or_timeout() {
    for action in ["clear", "reveal", "timeout"] {
        let f = Fixture::new();
        let photos = f.root.join("photos");
        std::fs::create_dir(&photos).unwrap();
        std::fs::write(photos.join("one.png"), b"image").unwrap();
        let ticket = begin_request(&f.app, 1).unwrap();
        let app = f.app.clone();
        let config = f.config.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            request_at(&app, 1, ticket, &config, Some(Some(photos)), |root| {
                ready_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                scan(root)
            })
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            f.app.current.try_lock().is_ok(),
            "NAS read must release app mutex"
        );
        assert!(
            f.app.gallery.data.try_lock().is_ok(),
            "NAS read must release gallery mutex"
        );
        match action {
            "clear" => {
                let _ = set_at(&f.app, 1, &f.config, None).unwrap();
            }
            "reveal" => {
                f.app.current.lock().unwrap().generation += 1;
                f.app.concealed.store(false, Ordering::Release);
            }
            _ => drop(PendingRequest(f.app.gallery.clone(), ticket)),
        }
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap_err(), StatusCode::CONFLICT);
        assert!(f.app.gallery.data.lock().unwrap().files.is_empty());
        assert!(crate::load_config_at(&f.config)
            .unwrap()
            .gallery_folder
            .is_none());
    }
}

#[test]
fn frame_interval_round_trips_without_changing_gallery_or_library_and_rejects_invalid_or_stale_writes(
) {
    let f = Fixture::new();
    assert_eq!(
        load_at(&f.app, 1, &f.config).unwrap().0["frame_seconds"],
        10
    );
    let original = crate::load_config_at(&f.config).unwrap();
    let _ = frame_settings_at(&f.app, 1, &f.config, 37).unwrap();
    let saved = crate::load_config_at(&f.config).unwrap();
    assert_eq!(saved.gallery_frame_seconds, 37);
    assert_eq!(saved.gallery_folder, original.gallery_folder);
    assert_eq!(saved.active_folder, original.active_folder);
    assert_eq!(
        load_at(&f.app, 1, &f.config).unwrap().0["frame_seconds"],
        37
    );
    for invalid in [0, 3601, u32::MAX] {
        assert_eq!(
            frame_settings_at(&f.app, 1, &f.config, invalid).unwrap_err(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        frame_settings_at(&f.app, 2, &f.config, 20).unwrap_err(),
        StatusCode::CONFLICT
    );
    f.app.concealed.store(false, Ordering::Release);
    assert_eq!(
        frame_settings_at(&f.app, 1, &f.config, 20).unwrap_err(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        crate::load_config_at(&f.config)
            .unwrap()
            .gallery_frame_seconds,
        37
    );
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "vault-gallery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("library")).unwrap();
        let app = AppState::new(Library::open(root.join("library")).unwrap()).unwrap();
        app.concealed.store(true, Ordering::Release);
        let config = root.join("vault_config.json");
        let initial = crate::load_config_at(&config).unwrap();
        crate::save_config_at(&config, &initial).unwrap();
        Self { root, config, app }
    }
    fn request(&self, path: &str) -> axum::http::request::Builder {
        HttpRequest::builder()
            .uri(path)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", self.app.token.as_str())
            .header("x-vault-library", "1")
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
            .starts_with("vault-gallery-"));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[tokio::test]
async fn selected_gallery_is_persistent_read_only_scoped_and_accessible_while_concealed() {
    let f = Fixture::new();
    let root = f.root.join("photos");
    std::fs::create_dir(&root).unwrap();
    for name in ["two.PNG", "one.jpg", "movie.mp4", "vault.key", "active.svg"] {
        std::fs::write(root.join(name), name.as_bytes()).unwrap();
    }
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/hidden.png"), b"nested").unwrap();
    let initial = load_at(&f.app, 1, &f.config).unwrap().0;
    assert_eq!(initial["status"], "empty");
    assert!(initial["images"].as_array().unwrap().is_empty());
    let chosen = set_at(&f.app, 1, &f.config, Some(root.clone())).unwrap().0;
    let images = chosen["images"].as_array().unwrap();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0]["name"], "one.jpg");
    assert!(!root.join(crate::encryption::PLAIN_DB).exists());
    let api = crate::build_router(f.app.clone());
    let url = images[0]["src"].as_str().unwrap();
    let response = api
        .clone()
        .oneshot(f.request(url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        &to_bytes(response.into_body(), 100).await.unwrap()[..],
        b"one.jpg"
    );
    let response = api
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri(url)
                .header("host", "127.0.0.1:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        api.clone()
            .oneshot(f.request("/api/files").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        StatusCode::LOCKED
    );
    let loaded = load_at(&f.app, 1, &f.config).unwrap().0;
    assert_eq!(loaded["images"].as_array().unwrap().len(), 2);
    assert_eq!(
        api.oneshot(f.request(url).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let _ = set_at(&f.app, 1, &f.config, None).unwrap();
    assert!(root.join("one.jpg").exists());
    assert_eq!(
        load_at(&f.app, 1, &f.config).unwrap().0["images"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
#[test]
fn unavailable_gallery_and_stale_selection_do_not_change_library_or_saved_choice() {
    let f = Fixture::new();
    let root = f.root.join("photos");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("image.png"), b"fixture").unwrap();
    let _ = set_at(&f.app, 1, &f.config, Some(root.clone())).unwrap();
    let saved = std::fs::read(&f.config).unwrap();
    let before = f.app.snapshot().unwrap();
    assert_eq!(
        set_at(&f.app, 0, &f.config, None).unwrap_err(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        set_at(&f.app, 1, &f.config, Some(f.root.join("missing"))).unwrap_err(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(std::fs::read(&f.config).unwrap(), saved);
    std::fs::rename(&root, f.root.join("disconnected")).unwrap();
    let offline = load_at(&f.app, 1, &f.config).unwrap().0;
    assert_eq!(offline["status"], "unavailable");
    assert_eq!(offline["images"].as_array().unwrap().len(), 0);
    assert_eq!(std::fs::read(&f.config).unwrap(), saved);
    assert_eq!(f.app.snapshot().unwrap().root_dir, before.root_dir);
    assert_eq!(f.app.snapshot().unwrap().generation, before.generation);
    f.app.concealed.store(false, Ordering::Release);
    assert_eq!(
        load_at(&f.app, 1, &f.config).unwrap_err(),
        StatusCode::CONFLICT
    );
}
#[test]
fn legacy_gallery_migrates_without_changing_library_settings_and_survives_config_recovery() {
    let f = Fixture::new();
    let mut original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&f.config).unwrap()).unwrap();
    original.as_object_mut().unwrap().remove("gallery_folder");
    std::fs::write(&f.config, serde_json::to_vec(&original).unwrap()).unwrap();
    let legacy = f.root.join("valtheum_gallery.json");
    let photos = f.root.join("currently-disconnected-photos");
    std::fs::write(
        &legacy,
        serde_json::to_vec(&serde_json::json!({"folder":photos})).unwrap(),
    )
    .unwrap();
    let imported = crate::load_config_at(&f.config).unwrap();
    assert_eq!(imported.gallery_folder, Some(photos.clone()));
    assert!(
        legacy.exists(),
        "reading alone must retain the migration source"
    );
    crate::save_config_at(&f.config, &imported).unwrap();
    assert!(!legacy.exists());
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&f.config).unwrap()).unwrap();
    assert_eq!(saved["folders"], original["folders"]);
    assert_eq!(saved["active_folder"], original["active_folder"]);
    assert_eq!(saved["gallery_folder"], photos.to_string_lossy().as_ref());
    std::fs::write(&f.config, b"{broken").unwrap();
    assert_eq!(
        crate::load_config_at(&f.config).unwrap().gallery_folder,
        Some(photos)
    );
}

#[test]
fn unified_gallery_selection_and_explicit_clear_take_precedence_over_legacy_file() {
    let f = Fixture::new();
    let photos = f.root.join("photos");
    std::fs::create_dir(&photos).unwrap();
    let before = crate::load_config_at(&f.config).unwrap();
    let _ = set_at(&f.app, 1, &f.config, Some(photos.clone())).unwrap();
    let after = crate::load_config_at(&f.config).unwrap();
    assert_eq!(after.active_folder, before.active_folder);
    assert_eq!(
        serde_json::to_value(&after.folders).unwrap(),
        serde_json::to_value(&before.folders).unwrap()
    );
    assert_eq!(after.gallery_folder, Some(photos.canonicalize().unwrap()));
    let legacy = f.root.join("valtheum_gallery.json");
    std::fs::write(&legacy, b"{\"folder\":\"D:/obsolete\"}").unwrap();
    assert_eq!(
        crate::load_config_at(&f.config).unwrap().gallery_folder,
        after.gallery_folder
    );
    let _ = set_at(&f.app, 1, &f.config, None).unwrap();
    assert!(!legacy.exists());
    std::fs::write(&legacy, b"{\"folder\":\"D:/obsolete\"}").unwrap();
    assert_eq!(
        crate::load_config_at(&f.config).unwrap().gallery_folder,
        None
    );
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&f.config).unwrap()).unwrap();
    assert!(value.get("gallery_folder").unwrap().is_null());
}

#[test]
fn failed_migration_save_preserves_legacy_and_original_configuration() {
    let f = Fixture::new();
    let mut original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&f.config).unwrap()).unwrap();
    original.as_object_mut().unwrap().remove("gallery_folder");
    let bytes = serde_json::to_vec(&original).unwrap();
    std::fs::write(&f.config, &bytes).unwrap();
    let legacy = f.root.join("valtheum_gallery.json");
    let old = b"{\"folder\":\"D:/photos\"}";
    std::fs::write(&legacy, old).unwrap();
    let imported = crate::load_config_at(&f.config).unwrap();
    std::fs::create_dir(f.config.with_extension("json.bak")).unwrap();
    assert!(crate::save_config_at(&f.config, &imported).is_err());
    assert_eq!(std::fs::read(&f.config).unwrap(), bytes);
    assert_eq!(std::fs::read(&legacy).unwrap(), old);
}

#[tokio::test]
async fn unset_gallery_is_empty_and_bundled_photos_are_not_served() {
    let f = Fixture::new();
    let config_before = std::fs::read(&f.config).unwrap();
    let unset = load_at(&f.app, 1, &f.config).unwrap().0;
    assert_eq!(unset["status"], "empty");
    assert!(unset["folder_name"].is_null());
    assert!(unset["images"].as_array().unwrap().is_empty());
    let api = crate::build_router(f.app.clone());
    for name in ["architecture", "cat", "coast", "flowers", "forest", "lake"] {
        let url = format!("/samples/{name}.webp");
        assert_eq!(
            api.clone()
                .oneshot(f.request(&url).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(std::fs::read(&f.config).unwrap(), config_before);
    let empty = f.root.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let custom = set_at(&f.app, 1, &f.config, Some(empty)).unwrap().0;
    assert_eq!(custom["status"], "ready");
    assert!(custom["images"].as_array().unwrap().is_empty());
    let reset = set_at(&f.app, 1, &f.config, None).unwrap().0;
    assert_eq!(reset["status"], "empty");
    assert!(reset["images"].as_array().unwrap().is_empty());
    assert_eq!(
        crate::load_config_at(&f.config).unwrap().gallery_folder,
        None
    );
    std::fs::remove_file(&f.config).unwrap();
    std::fs::remove_file(f.config.with_extension("json.bak")).unwrap();
    let first_run = load_at(&f.app, 1, &f.config).unwrap().0;
    assert_eq!(first_run["status"], "empty");
    assert!(first_run["images"].as_array().unwrap().is_empty());
    assert!(!f.config.exists());
}
