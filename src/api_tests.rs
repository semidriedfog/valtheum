use super::*;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};

#[test]
fn navigation_blocks_note_loss_and_external_pages_but_allows_safe_reload() {
    let origin = "http://127.0.0.1:8080";
    for url in [origin, "http://127.0.0.1:8080/"] {
        assert!(navigation_allowed(url, origin, false));
        assert!(!navigation_allowed(url, origin, true));
    }
    for url in [
        "https://example.org/",
        "http://127.0.0.1:8081/",
        "http://127.0.0.1:8080/api/files",
    ] {
        assert!(!navigation_allowed(url, origin, false));
    }
}

// Run only in release mode: debug deliberately permits UI files from disk.
#[cfg(not(debug_assertions))]
#[tokio::test]
async fn release_static_ignores_working_directory_overrides() {
    const CHILD: &str = "VALTHEUM_STATIC_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        for path in ["/", "/app.js"] {
            let response = static_handler(path.parse().unwrap()).await.into_response();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
                .await
                .unwrap();
            let name = if path == "/" { "index.html" } else { "app.js" };
            assert_eq!(body.as_ref(), Assets::get(name).unwrap().data.as_ref());
            assert_ne!(body.as_ref(), b"untrusted working-directory script");
        }
        let response = static_handler("/untrusted-only.js".parse().unwrap())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        return;
    }
    let root = std::env::temp_dir().join(format!("valtheum-static-test-{}", std::process::id()));
    // Refuse to reuse a pre-existing directory before writing or cleaning up.
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("static")).unwrap();
    for name in ["index.html", "app.js", "untrusted-only.js"] {
        std::fs::write(
            root.join("static").join(name),
            b"untrusted working-directory script",
        )
        .unwrap();
    }
    // A child process isolates cwd and environment from parallel tests.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "api_tests::release_static_ignores_working_directory_overrides",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .current_dir(&root)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_metadata_writes_cannot_apply_after_waiting_for_the_database() {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let f = Fixture::new();
    let snapshot = f.state.snapshot().unwrap();
    let tag_id = {
        let guard = snapshot.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "test.png", "IMAGE", 10, 1).unwrap();
        db::upsert_media_file(conn, "test.pdf", "PDF", 10, 1).unwrap();
        db::save_bookmark(conn, 2, 0.0, &[0xff, 0xd8, 0xff]).unwrap();
        series::save(
            conn,
            &series::SaveSeries {
                id: None,
                name: "Kept series".into(),
                representative_id: 2,
                file_ids: vec![2],
            },
        )
        .unwrap();
        db::add_tag_to_file(conn, 1, "kept").unwrap().id
    };
    let app = build_router(f.state.clone());
    for (url, method, body) in [
        ("/api/files/1/rating".to_owned(), "POST", r#"{"rating":5}"#),
        (
            "/api/files/1/tags".to_owned(),
            "POST",
            r#"{"name":"expired"}"#,
        ),
        (format!("/api/files/1/tags/{tag_id}"), "DELETE", ""),
        ("/api/files/2/bookmarks".to_owned(), "POST", r#"{"position":1,"thumbnail_data":"/9j/","request_id":"12345678901234567890123456789012"}"#),
        ("/api/files/2/bookmarks/1".to_owned(), "DELETE", ""),
        ("/api/series".to_owned(), "POST", r#"{"id":1,"name":"Expired edit","representative_id":1,"file_ids":[1,2]}"#),
        ("/api/series".to_owned(), "POST", r#"{"name":"Expired creation","representative_id":1,"file_ids":[1]}"#),
        ("/api/series/1".to_owned(), "DELETE", ""),
        ("/api/thumbnail/1".to_owned(), "POST", "--fixture\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a.jpg\"\r\n\r\nimage bytes\r\n--fixture--\r\n"),
    ] {
        let deadline = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            + 100;
        // Simulate NAS I/O owning the connection until after the request expires.
        let held = snapshot.conn.lock().unwrap();
        let content_type = if url == "/api/thumbnail/1" {
            "multipart/form-data; boundary=fixture"
        } else {
            "application/json"
        };
        let request = f
            .request(&url)
            .method(method)
            .header("content-type", content_type)
            .header("X-Vault-Write-Deadline", deadline.to_string())
            .body(Body::from(body))
            .unwrap();
        let pending = tokio::spawn(app.clone().oneshot(request));
        tokio::time::sleep(Duration::from_millis(150)).await;
        drop(held);
        let response = tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    }
    let response = app
        .clone()
        .oneshot(f.request("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let files: Vec<db::MediaFile> =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    let image = files
        .iter()
        .find(|file| file.rel_path == "test.png")
        .unwrap();
    assert_eq!(image.rating, 0);
    assert_eq!(image.tags.len(), 1);
    assert_eq!(image.tags[0].name, "kept");
    {
        let guard = snapshot.conn.lock().unwrap();
        let bookmarks = db::list_bookmarks(guard.as_ref().unwrap(), 2).unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].position, 0.0);
        assert!(bookmarks[0].request_id.is_none());
        let saved_series = series::list(guard.as_ref().unwrap()).unwrap();
        assert_eq!(saved_series.len(), 1);
        assert_eq!(saved_series[0].name, "Kept series");
        assert_eq!(saved_series[0].representative_id, Some(2));
        assert_eq!(saved_series[0].file_ids, vec![2]);
    }
    assert_eq!(
        snapshot
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM thumbnails", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let deadline = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        + 15000;
    let response = app
        .oneshot(
            f.request("/api/files/1/rating")
                .method("POST")
                .header("content-type", "application/json")
                .header("X-Vault-Write-Deadline", deadline.to_string())
                .body(Body::from(r#"{"rating":4}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        snapshot
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row("SELECT rating FROM media_files WHERE id=1", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        4
    );
}

#[test]
fn lock_revokes_without_waiting_for_db_and_cleanup_blocks_early_unlock() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let router = build_router(f.state.clone());
    let password = "lock delay fixture password";
    let request = f
        .request("/api/security/encrypt")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"password":password,"confirmation":password}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        rt.block_on(router.clone().oneshot(request))
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let old = f.state.snapshot().unwrap();
    let held = old.conn.lock().unwrap();
    let request = f
        .request_generation("/api/security/lock", 2)
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let response = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), router.clone().oneshot(request)).await
    });
    // Always release the simulated NAS stall before asserting, including regressions.
    if response.is_err() {
        drop(held);
        panic!("lock waited behind the old DB connection");
    }
    assert_eq!(response.unwrap().unwrap().status(), StatusCode::OK);
    assert!(f.state.concealed.load(Ordering::Acquire));
    assert!(old.archives.revoked.load(Ordering::Acquire));
    let locked = f.state.snapshot().unwrap();
    assert!(locked.cleanup_pending.load(Ordering::Acquire));
    let request = f
        .request_generation("/api/security/unlock", 3)
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"password":password}).to_string(),
        ))
        .unwrap();
    let early = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), router.clone().oneshot(request)).await
    });
    drop(held);
    assert_eq!(early.unwrap().unwrap().status(), StatusCode::BAD_REQUEST);
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while locked.cleanup_pending.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    });
    let request = f
        .request_generation("/api/security/unlock", 3)
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"password":password}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        rt.block_on(router.oneshot(request)).unwrap().status(),
        StatusCode::OK
    );
    assert!(old.conn.lock().unwrap().is_none());
    assert!(!f.state.concealed.load(Ordering::Acquire));
}

#[test]
fn close_remains_available_during_locked_library_cleanup() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let old = f.state.snapshot().unwrap();
    old.security.lock().unwrap().encrypted = true;
    let held = old.conn.lock().unwrap();
    let router = build_router(f.state.clone());
    let locking = rt.spawn(
        router.clone().oneshot(
            f.request("/api/security/lock")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        ),
    );
    let result = rt.block_on(async { tokio::time::timeout(Duration::from_secs(2), locking).await });
    if result.is_err() {
        drop(held);
        panic!("lock blocked");
    }
    assert_eq!(result.unwrap().unwrap().unwrap().status(), StatusCode::OK);
    let request = f
        .request_generation("/api/folders/close", 2)
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let closed = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), router.oneshot(request)).await
    });
    drop(held);
    assert_eq!(closed.unwrap().unwrap().status(), StatusCode::OK);
    assert!(f.state.concealed.load(Ordering::Acquire));
    assert!(f.state.snapshot().unwrap().root_dir.as_os_str().is_empty());
}

struct Fixture {
    root: PathBuf,
    state: AppState,
}
impl Fixture {
    async fn wait_for_cleanup(&self) {
        let library = self.state.snapshot().unwrap().library;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while library
                .cleanup_pending
                .load(std::sync::atomic::Ordering::Acquire)
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("retired library cleanup did not complete");
    }
    fn new() -> Self {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let root = std::env::temp_dir().join(format!("vault-api-{:x}", u64::from_ne_bytes(random)));
        std::fs::create_dir(&root).unwrap();
        let library = Library::open(root.clone()).unwrap();
        Self {
            root,
            state: AppState::new(library).unwrap(),
        }
    }
    fn request(&self, uri: &str) -> axum::http::request::Builder {
        self.request_generation(uri, 1)
    }
    fn request_generation(&self, uri: &str, generation: u64) -> axum::http::request::Builder {
        Request::builder()
            .uri(uri)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", self.state.token.as_str())
            .header("x-vault-library", generation.to_string())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let snapshot = self.state.snapshot().unwrap();
        snapshot.conn.lock().unwrap().take();
        self.state.current.lock().unwrap().library = Library::unconfigured();
        drop(snapshot);
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("vault-api-"));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn closing_plain_and_encrypted_libraries_revokes_old_access_and_preserves_data() {
    for encrypted in [false, true] {
        let f = Fixture::new();
        std::fs::write(f.root.join("keep.jpg"), b"keep media").unwrap();
        let app = build_router(f.state.clone());
        {
            let snapshot = f.state.snapshot().unwrap();
            let guard = snapshot.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            db::upsert_media_file(conn, "keep.jpg", "IMAGE", 10, 1).unwrap();
            conn.execute("UPDATE media_files SET rating=5", []).unwrap();
        }
        if encrypted {
            let response = app.clone().oneshot(f.request("/api/security/encrypt")
                .method("POST").header("content-type","application/json")
                .body(Body::from(r#"{"password":"close test password","confirmation":"close test password"}"#)).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let before = f.state.snapshot().unwrap();
        let response = app
            .clone()
            .oneshot(
                f.request_generation("/api/folders/close", before.generation)
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(body["covered"], true);
        assert_eq!(body.as_object().unwrap().len(), 2);
        let after = f.state.snapshot().unwrap();
        assert_eq!(after.generation, before.generation + 1);
        assert!(after.root_dir.as_os_str().is_empty());
        assert!(after.security_status().locked);
        assert!(before.conn.lock().unwrap().is_none());
        assert!(before.security.lock().unwrap().password.is_none());
        assert!(before.archives.key.lock().unwrap().is_none());
        assert!(before
            .archives
            .revoked
            .load(std::sync::atomic::Ordering::Acquire));
        for (generation, expected) in [
            (before.generation, StatusCode::CONFLICT),
            (after.generation, StatusCode::LOCKED),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(
                        f.request_generation("/api/files", generation)
                            .body(Body::empty())
                            .unwrap()
                    )
                    .await
                    .unwrap()
                    .status(),
                expected
            );
        }
        assert_eq!(
            std::fs::read(f.root.join("keep.jpg")).unwrap(),
            b"keep media"
        );
        let path = f.root.join(if encrypted {
            encryption::SECURE_DB
        } else {
            encryption::PLAIN_DB
        });
        let conn =
            encryption::open(&path, encrypted.then_some("close test password"), false).unwrap();
        assert_eq!(
            conn.query_row("SELECT rating FROM media_files", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            5
        );
    }
}

#[tokio::test]
async fn closing_library_checks_credentials_and_detaches_active_scan() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let no_token = Request::builder()
        .uri("/api/folders/close")
        .method("POST")
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(no_token).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(
                f.request_generation("/api/folders/close", 0)
                    .method("POST")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let old = f.state.snapshot().unwrap();
    {
        old.scan.lock().unwrap().running = true;
        let response = app
            .clone()
            .oneshot(
                f.request("/api/folders/close")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(f.state.snapshot().unwrap().generation, 2);
        assert!(f.state.concealed.load(std::sync::atomic::Ordering::Acquire));
        assert!(old
            .archives
            .revoked
            .load(std::sync::atomic::Ordering::Acquire));
        old.scan.lock().unwrap().running = false;
    }
    old.archives
        .busy
        .store(false, std::sync::atomic::Ordering::Release);
}

#[tokio::test]
async fn credentials_origin_and_host_are_checked() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let request = Request::builder()
        .uri("/api/files")
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    for (name, value) in [
        ("origin", "https://untrusted.example"),
        ("sec-fetch-site", "cross-site"),
    ] {
        let request = f
            .request("/api/session")
            .header(name, value)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    let request = Request::builder()
        .uri("/api/session")
        .header("host", "untrusted.example:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    for path in [
        "/api/session",
        "/api/security/folders",
        "/api/files",
        "/api/gallery",
    ] {
        for token in [None, Some("invalid-local-client-token")] {
            let mut request = Request::builder()
                .uri(path)
                .header("host", "127.0.0.1:8080");
            if let Some(token) = token {
                request = request.header("x-vault-token", token);
            }
            assert_eq!(
                app.clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }
    let response = app
        .oneshot(f.request("/api/session").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let session: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(session["generation"], 1);
    assert!(session.get("token").is_none());
}

#[tokio::test]
async fn concealed_library_hides_session_metadata_and_rejects_folder_restore_and_media_apis() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    {
        let s = f.state.snapshot().unwrap();
        db::upsert_media_file(
            s.conn.lock().unwrap().as_ref().unwrap(),
            "private-name.png",
            "IMAGE",
            5,
            0,
        )
        .unwrap();
    }
    let make = |path: &str, method: &str, generation: u64, body: serde_json::Value| {
        Request::builder()
            .uri(path)
            .method(method)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", f.state.token.as_str())
            .header("x-vault-library", generation.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let password = "privacy fixture password 123";
    assert_eq!(
        app.clone()
            .oneshot(make(
                "/api/security/encrypt",
                "POST",
                1,
                serde_json::json!({"password":password,"confirmation":password})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let locked = app
        .clone()
        .oneshot(make("/api/security/lock", "POST", 2, serde_json::json!({})))
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(locked.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(body["covered"], true);
    f.wait_for_cleanup().await;
    let held = f.state.snapshot().unwrap();
    let fresh =
        AppState::new(Library::open_reusing(f.root.clone(), held.lease.as_ref()).unwrap()).unwrap();
    assert!(fresh.concealed.load(std::sync::atomic::Ordering::Acquire));
    let response = app
        .clone()
        .oneshot(make("/api/session", "GET", 3, serde_json::json!({})))
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(body["covered"], true);
    assert_eq!(
        body.as_object().unwrap().len(),
        2,
        "only generation and visibility are exposed"
    );
    assert!(body.get("security").is_none());
    assert!(body.get("scan").is_none());
    for (path, method) in [
        ("/api/folders", "GET"),
        ("/api/security", "GET"),
        ("/api/files", "GET"),
        ("/api/scan", "GET"),
        ("/api/trash/logs", "GET"),
        ("/api/folders/add", "POST"),
        ("/api/folders/switch", "POST"),
        ("/api/backup/restore/select", "POST"),
        ("/api/backup/restore", "POST"),
        ("/api/archive-security", "GET"),
    ] {
        let response = app
            .clone()
            .oneshot(make(path, method, 3, serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::LOCKED, "{path}");
        assert!(
            to_bytes(response.into_body(), 8192)
                .await
                .unwrap()
                .is_empty(),
            "no path or filenames in rejection"
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(make(
                "/api/security/unlock",
                "POST",
                3,
                serde_json::json!({"password":"incorrect"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(f.state.concealed.load(std::sync::atomic::Ordering::Acquire));
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/unlock",
            "POST",
            3,
            serde_json::json!({"password":password}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(body["covered"], false);
    let response = app
        .oneshot(make("/api/files", "GET", 4, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        String::from_utf8_lossy(&to_bytes(response.into_body(), 65536).await.unwrap())
            .contains("private-name.png")
    );
}

#[tokio::test]
async fn backup_restore_requires_session_current_generation_and_native_selection() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/backup/restore")
                .method("POST")
                .header("host", "127.0.0.1:8080")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"token":"unknown","password":"test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let make = || {
        f.request("/api/backup/restore")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"token":"unknown","password":"test"}"#))
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(make()).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    {
        let s = f.state.snapshot().unwrap();
        let mut security = s.security.lock().unwrap();
        security.encrypted = true;
        security.password = None;
    }
    assert_eq!(
        app.clone().oneshot(make()).await.unwrap().status(),
        StatusCode::BAD_REQUEST,
        "restoring an independent backup remains available while the current DB is locked"
    );
    f.state.current.lock().unwrap().generation = 2;
    assert_eq!(
        app.oneshot(make()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn stale_mutation_cannot_touch_replacement_library() {
    let first = Fixture::new();
    let second = Fixture::new();
    let old = first.state.snapshot().unwrap();
    let new = second.state.snapshot().unwrap();
    for library in [&old, &new] {
        db::upsert_media_file(
            library.conn.lock().unwrap().as_ref().unwrap(),
            "same.mp4",
            "VIDEO",
            1,
            1,
        )
        .unwrap();
    }
    {
        let mut current = first.state.current.lock().unwrap();
        current.library = new.library.clone();
        current.generation = 2;
    }
    let response = build_router(first.state.clone())
        .oneshot(
            first
                .request("/api/files/1/rating")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rating":5}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    // A request accepted before a switch remains bound to its original connection.
    old.update_db(StatusCode::SERVICE_UNAVAILABLE, |conn| {
        db::update_rating(conn, 1, 3)
    })
    .unwrap();
    assert_eq!(
        db::get_media_files(new.conn.lock().unwrap().as_ref().unwrap(), None, None).unwrap()[0]
            .rating,
        0
    );
    old.conn.lock().unwrap().take();
}

#[tokio::test]
async fn bookmarks_round_trip_replace_thumbnail_and_delete() {
    use base64::Engine;

    let f = Fixture::new();
    let snapshot = f.state.snapshot().unwrap();
    {
        let conn = snapshot.conn.lock().unwrap();
        db::upsert_media_file(conn.as_ref().unwrap(), "clip.mp4", "VIDEO", 10, 1).unwrap();
    }
    let app = build_router(f.state.clone());
    let first_image = [0xff, 0xd8, 0xff, 0x01, 0x02];
    let second_image = [0xff, 0xd8, 0xff, 0x03, 0x04];

    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/bookmarks")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({
                    "position": 12,
                    "thumbnail_data": base64::engine::general_purpose::STANDARD.encode(first_image),
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let created: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(created["id"], 1);
    assert_eq!(created["position"].as_f64(), Some(12.0));
    assert_eq!(
        created["thumbnail_url"],
        "/api/files/1/bookmarks/1/thumbnail"
    );

    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/bookmarks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let listed: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(
        listed,
        serde_json::json!([{
            "id": 1,
            "position": 12.0,
            "thumbnail_url": "/api/files/1/bookmarks/1/thumbnail"
        }])
    );

    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/bookmarks/1/thumbnail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/jpeg");
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap().as_ref(),
        &first_image
    );

    // Saving the same position updates the existing bookmark and keeps its id.
    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/bookmarks")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({
                    "position": 12.0,
                    "thumbnail_data": base64::engine::general_purpose::STANDARD.encode(second_image),
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let replaced: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(replaced["id"], 1);
    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/bookmarks/1/thumbnail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap().as_ref(),
        &second_image
    );

    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/bookmarks/1")
                .method("DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .oneshot(
            f.request("/api/files/1/bookmarks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap().as_ref(),
        b"[]"
    );
}

#[tokio::test]
async fn bookmark_save_receipt_distinguishes_replacement_from_an_existing_position() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    {
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "book.pdf", "PDF", 10, 1).unwrap();
        db::save_bookmark(conn, 1, 2.0, &[0xff, 0xd8, 0xff]).unwrap();
    }
    let app = build_router(f.state.clone());
    for token in [
        "11111111111111111111111111111111",
        "22222222222222222222222222222222",
    ] {
        let response = app.clone().oneshot(f.request("/api/files/1/bookmarks").method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"position":2,"thumbnail_data":"/9j/AA==","request_id":token}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        // Model an acknowledgement lost after committing: inspect only the next list.
        drop(response);
        let response = app
            .clone()
            .oneshot(
                f.request("/api/files/1/bookmarks")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let records: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(records.as_array().unwrap().len(), 1);
        assert_eq!(records[0]["id"], 1);
        assert_eq!(records[0]["request_id"], token);
    }
    let response = app
        .oneshot(
            f.request("/api/files/1/bookmarks")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"position":2,"thumbnail_data":"/9j/","request_id":"invalid"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let guard = state.conn.lock().unwrap();
    assert_eq!(
        db::list_bookmarks(guard.as_ref().unwrap(), 1).unwrap()[0]
            .request_id
            .as_deref(),
        Some("22222222222222222222222222222222")
    );
}

#[tokio::test]
async fn bookmarks_reject_invalid_inputs_and_cross_file_access() {
    use base64::Engine;

    let f = Fixture::new();
    let snapshot = f.state.snapshot().unwrap();
    {
        let guard = snapshot.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "photo.jpg", "IMAGE", 10, 1).unwrap();
        db::upsert_media_file(conn, "clip.mp4", "VIDEO", 10, 1).unwrap();
        db::upsert_media_file(conn, "pages.cbz", "ARCHIVE", 10, 1).unwrap();
    }
    let app = build_router(f.state.clone());
    let jpeg = |position: serde_json::Value, image: Vec<u8>| {
        serde_json::json!({
            "position": position,
            "thumbnail_data": base64::engine::general_purpose::STANDARD.encode(image),
        })
        .to_string()
    };
    let mut oversized = vec![0u8; 128 * 1024 + 1];
    oversized[..3].copy_from_slice(&[0xff, 0xd8, 0xff]);
    let post = |uri: &str, body: String| {
        app.clone().oneshot(
            f.request(uri)
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
    };

    assert_eq!(
        post(
            "/api/files/1/bookmarks",
            jpeg(1.into(), vec![0xff, 0xd8, 0xff])
        )
        .await
        .unwrap()
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(
            "/api/files/3/bookmarks",
            jpeg(1.5.into(), vec![0xff, 0xd8, 0xff])
        )
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(
            "/api/files/2/bookmarks",
            r#"{"position":1,"thumbnail_data":"%%%"}"#.to_string()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post("/api/files/2/bookmarks", jpeg(1.into(), b"png".to_vec()))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post("/api/files/2/bookmarks", jpeg(1.into(), oversized))
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    let created = post(
        "/api/files/2/bookmarks",
        jpeg(7.into(), vec![0xff, 0xd8, 0xff]),
    )
    .await
    .unwrap();
    let created: serde_json::Value =
        serde_json::from_slice(&to_bytes(created.into_body(), 4096).await.unwrap()).unwrap();
    let bookmark_id = created["id"].as_i64().unwrap();
    assert_eq!(
        app.clone()
            .oneshot(
                f.request(&format!("/api/files/3/bookmarks/{bookmark_id}/thumbnail"))
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.clone()
            .oneshot(
                f.request(&format!("/api/files/3/bookmarks/{bookmark_id}"))
                    .method("DELETE")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.clone()
            .oneshot(
                f.request("/api/files/999/bookmarks")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    for position in 0..100 {
        if position == 7 {
            continue;
        }
        assert_eq!(
            post(
                "/api/files/2/bookmarks",
                jpeg(position.into(), vec![0xff, 0xd8, 0xff]),
            )
            .await
            .unwrap()
            .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        post(
            "/api/files/2/bookmarks",
            jpeg(100.into(), vec![0xff, 0xd8, 0xff]),
        )
        .await
        .unwrap()
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn stale_generation_rejects_bookmark_mutation() {
    use base64::Engine;

    let first = Fixture::new();
    let second = Fixture::new();
    let old = first.state.snapshot().unwrap();
    let new = second.state.snapshot().unwrap();
    db::upsert_media_file(
        old.conn.lock().unwrap().as_ref().unwrap(),
        "clip.mp4",
        "VIDEO",
        1,
        1,
    )
    .unwrap();
    db::upsert_media_file(
        new.conn.lock().unwrap().as_ref().unwrap(),
        "clip.mp4",
        "VIDEO",
        1,
        1,
    )
    .unwrap();
    {
        let mut current = first.state.current.lock().unwrap();
        current.library = new.library.clone();
        current.generation = 2;
    }
    let response = build_router(first.state.clone())
        .oneshot(
            first
                .request("/api/files/1/bookmarks")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({
                    "position": 1,
                    "thumbnail_data": base64::engine::general_purpose::STANDARD.encode([0xff, 0xd8, 0xff]),
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(
        db::list_bookmarks(old.conn.lock().unwrap().as_ref().unwrap(), 1)
            .unwrap()
            .is_empty()
    );
    old.conn.lock().unwrap().take();
}

#[tokio::test]
async fn media_paths_and_thumbnail_payloads_are_safe() {
    let f = Fixture::new();
    std::fs::write(f.root.join("photo.jpg"), b"image").unwrap();
    let snapshot = f.state.snapshot().unwrap();
    {
        let guard = snapshot.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "photo.jpg", "IMAGE", 5, 1).unwrap();
        db::save_thumbnail(conn, 1, &[7; 1024 * 1024], 1, 1, None, None).unwrap();
    }
    let app = build_router(f.state.clone());
    for path in [
        "/api/media/../outside.txt",
        "/api/media/C:%5CWindows%5Cwin.ini",
        "/api/media/photo.jpg:stream",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(f.request(path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let response = app
        .clone()
        .oneshot(f.request("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(body.len() < 1024);
    let files: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(files[0]["thumbnail_data"], "/api/thumbnail/1?v=1");
    let uri = format!("/api/media/photo.jpg?session={}&library=1", f.state.token);
    let req = Request::builder()
        .uri(uri)
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn rescan_is_queued_and_reports_completion() {
    let f = Fixture::new();
    std::fs::write(f.root.join("photo.jpg"), b"image").unwrap();
    let snapshot = f.state.snapshot().unwrap();
    let app = build_router(f.state.clone());
    let response = app
        .clone()
        .oneshot(
            f.request("/api/folders/rescan")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(status["running"], true);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while snapshot.scan_status().running {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let status = snapshot.scan_status();
    assert!(status.error.is_none(), "{:?}", status.error);
    assert_eq!(status.report.unwrap()["scanned_count"], 1);
    let response = app
        .oneshot(f.request("/api/trash/count").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
        br#"{"count":0}"#
    );
}

// Manual browser fixture. Run this test alone; its config and media are isolated
// below target and it never opens the user's configured library.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn browser_fixture_server() {
    use base64::Engine;
    use std::io::Write;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("review-browser-{}", timestamp()));
    std::fs::create_dir(&root).unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=").unwrap();
    let mut folders = Vec::new();
    for name in ["Library-A", "Library-B"] {
        let path = root.join(name);
        std::fs::create_dir(&path).unwrap();
        for i in 0..125 {
            std::fs::write(path.join(format!("image-{i:03}.png")), &png).unwrap();
        }
        std::fs::write(path.join(format!("{name}-only.png")), &png).unwrap();
        std::fs::write(
            path.join("sample.pdf"),
            include_bytes!("../tests/fixtures/documents/sample.pdf"),
        )
        .unwrap();
        std::fs::write(path.join("unsupported.rar"), b"unsupported fixture").unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path.join("pages.zip")).unwrap());
        for i in 1..=4 {
            zip.start_file(format!("{i}.png"), zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&png).unwrap();
        }
        zip.finish().unwrap();
        let library = Library::open(path.clone()).unwrap();
        let guard = library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        scanner::scan_directory(&path, conn).unwrap();
        // A saved thumbnail makes the browser exercise the URL-only list contract.
        db::save_thumbnail(conn, 1, &png, 1, 1, None, None).unwrap();
        folders.push(SavedFolder {
            name: name.into(),
            path: path.to_string_lossy().into_owned(),
            last_used: 0,
        });
    }
    std::env::set_current_dir(&root).unwrap();
    save_config(&AppConfig {
        active_folder: Some(folders[0].path.clone()),
        folders: folders.clone(),
        gallery_folder: None,
        gallery_frame_seconds: 10,
        language: "ko".into(),
    })
    .unwrap();
    let state = AppState::new(Library::open(PathBuf::from(&folders[0].path)).unwrap()).unwrap();
    std::fs::write(root.join("browser-token.txt"), state.token.as_bytes()).unwrap();
    let port = std::env::var("VAULT_TEST_PORT").unwrap_or_else(|_| "8080".into());
    println!(
        "Browser fixture: http://127.0.0.1:{port}; config under {}",
        root.display()
    );
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let app = build_router(state).layer(axum::middleware::from_fn(
        |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
            // The isolated manual fixture may use another port while the user's
            // app stays open. Production origin checking is unchanged.
            request
                .headers_mut()
                .insert(header::HOST, HeaderValue::from_static("127.0.0.1:8080"));
            if request.headers().contains_key(header::ORIGIN) {
                request.headers_mut().insert(
                    header::ORIGIN,
                    HeaderValue::from_static("http://127.0.0.1:8080"),
                );
            }
            next.run(request).await
        },
    ));
    axum::serve(listener, app).await.unwrap();
}

#[tokio::test]
async fn tar_zstd_scan_list_and_image_routes() {
    let f = Fixture::new();
    let encoder = zstd::stream::write::Encoder::new(
        std::fs::File::create(f.root.join("pages.TAR.ZST")).unwrap(),
        1,
    )
    .unwrap();
    let mut builder = tar::Builder::new(encoder);
    let image = b"\x89PNG\r\n\x1a\nimage fixture";
    let mut header = tar::Header::new_gnu();
    header.set_size(image.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, "nested/page.png", &image[..])
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();
    std::fs::write(f.root.join("single.png.zst"), b"not a tar archive").unwrap();
    {
        let snapshot = f.state.snapshot().unwrap();
        let guard = snapshot.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let report = scanner::scan_directory(&f.root, conn).unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.scanned_count, 1);
        let kind: String = conn
            .query_row(
                "SELECT file_type FROM media_files WHERE rel_path = 'pages.TAR.ZST'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kind, "ARCHIVE");
    }
    let app = build_router(f.state.clone());
    let response = app
        .clone()
        .oneshot(
            f.request("/api/archive/list/pages.TAR.ZST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let names: Vec<String> =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(names, ["nested/page.png"]);
    let response = app
        .clone()
        .oneshot(
            f.request("/api/archive/file/pages.TAR.ZST?entry=nested%2Fpage.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap().as_ref(),
        image
    );
    let response = app
        .oneshot(
            f.request("/api/archive/list/single.png.zst")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(!f.root.join("nested").exists());
}

fn seed_deleted_history(f: &Fixture) {
    let snapshot = f.state.snapshot().unwrap();
    let guard = snapshot.conn.lock().unwrap();
    for name in ["first.png", "second.png"] {
        guard.as_ref().unwrap().execute("INSERT INTO deleted_files_log (original_rel_path, file_type, file_size, score, deleted_at) VALUES (?1, 'IMAGE', 10, 1000, 1)", [name]).unwrap();
    }
}

#[tokio::test]
async fn deletion_history_can_be_deleted_individually_or_cleared_per_library() {
    let f = Fixture::new();
    let other = Fixture::new();
    seed_deleted_history(&f);
    seed_deleted_history(&other);
    std::fs::write(f.root.join("first.png"), b"a recreated file").unwrap();
    let app = build_router(f.state.clone());
    for uri in ["/api/trash/logs/1", "/api/trash/logs/1"] {
        let response = app
            .clone()
            .oneshot(f.request(uri).method("DELETE").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = app
        .clone()
        .oneshot(f.request("/api/trash/logs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let logs: Vec<serde_json::Value> =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["id"], 2);
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                f.request("/api/trash/logs")
                    .method("DELETE")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = app
        .oneshot(f.request("/api/trash/logs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap().as_ref(),
        b"[]"
    );
    let snapshot = other.state.snapshot().unwrap();
    assert_eq!(
        db::get_deleted_logs(snapshot.conn.lock().unwrap().as_ref().unwrap())
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        std::fs::read(f.root.join("first.png")).unwrap(),
        b"a recreated file"
    );
}

#[tokio::test]
async fn deletion_history_mutations_require_credentials_and_current_library_generation() {
    let f = Fixture::new();
    seed_deleted_history(&f);
    let app = build_router(f.state.clone());
    for uri in ["/api/trash/logs/1", "/api/trash/logs"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method("DELETE")
                    .header("host", "127.0.0.1:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    f.state.current.lock().unwrap().generation = 2;
    for uri in ["/api/trash/logs/1", "/api/trash/logs"] {
        let response = app
            .clone()
            .oneshot(f.request(uri).method("DELETE").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
    let snapshot = f.state.snapshot().unwrap();
    assert_eq!(
        db::get_deleted_logs(snapshot.conn.lock().unwrap().as_ref().unwrap())
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn series_api_roundtrip_shared_rating_validation_and_dissolve() {
    let f = Fixture::new();
    {
        let state = f.state.snapshot().unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        for name in ["one.zip", "two.tar.zst"] {
            db::upsert_media_file(conn, name, "ARCHIVE", 10, 1).unwrap();
        }
    }
    let app = build_router(f.state.clone());
    for (payload, status) in [
        (
            r#"{"name":"Story","representative_id":2,"file_ids":[1,2]}"#,
            StatusCode::OK,
        ),
        (
            r#"{"name":"Story","representative_id":1,"file_ids":[1,2]}"#,
            StatusCode::CONFLICT,
        ),
        (
            r#"{"id":1,"name":"Changed","representative_id":2,"file_ids":[1]}"#,
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                f.request("/api/series")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/rating")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"rating":5}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(f.request("/api/series").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let series: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(series[0]["name"], "Story");
    assert_eq!(series[0]["rating"], 5);
    assert_eq!(series[0]["representative_id"], 2);
    let response = app
        .clone()
        .oneshot(f.request("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let files: Vec<db::MediaFile> =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(files.iter().map(|f| f.id).collect::<Vec<_>>(), [1, 2]);
    assert!(files
        .iter()
        .all(|f| f.rating == 5 && f.series_id == Some(1)));
    let response = app
        .clone()
        .oneshot(
            f.request("/api/arena/vote")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"winner_id":1,"loser_id":2}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = app
        .clone()
        .oneshot(
            f.request("/api/series/1")
                .method("DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let state = f.state.snapshot().unwrap();
    let guard = state.conn.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    assert!(series::list(conn).unwrap().is_empty());
    let files = db::get_media_files(conn, None, None).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.rating == 0 && f.series_id.is_none()));
}

#[tokio::test]
async fn series_mutations_require_session_and_current_library() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    for (uri, method) in [("/api/series", "POST"), ("/api/series/1", "DELETE")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .method(method)
                    .header("host", "127.0.0.1:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    f.state.current.lock().unwrap().generation = 2;
    for (uri, method) in [("/api/series", "POST"), ("/api/series/1", "DELETE")] {
        let response = app
            .clone()
            .oneshot(f.request(uri).method(method).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}

#[tokio::test]
async fn tags_added_to_one_volume_are_returned_and_removed_on_every_series_member() {
    let f = Fixture::new();
    {
        let state = f.state.snapshot().unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        for name in ["one.zip", "two.zip"] {
            db::upsert_media_file(conn, name, "ARCHIVE", 10, 1).unwrap();
        }
        series::save(
            conn,
            &series::SaveSeries {
                id: None,
                name: "Story".into(),
                representative_id: 1,
                file_ids: vec![1, 2],
            },
        )
        .unwrap();
    }
    let app = build_router(f.state.clone());
    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/2/tags")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"shared"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let tag: db::Tag =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    for expected in [1, 0] {
        let response = app
            .clone()
            .oneshot(f.request("/api/files").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let files: Vec<db::MediaFile> =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.tags.len() == expected));
        if expected == 1 {
            assert!(files.iter().all(|f| f.tags[0].name == "shared"));
            let response = app
                .clone()
                .oneshot(
                    f.request(&format!("/api/files/1/tags/{}", tag.id))
                        .method("DELETE")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }
}

#[tokio::test]
async fn encrypted_library_lifecycle_blocks_media_until_correct_password() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let password = "sample secure password 123";
    let make = |path: &str, method: &str, generation: u64, body: serde_json::Value| {
        Request::builder()
            .uri(path)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", f.state.token.as_str())
            .method(method)
            .header("x-vault-library", generation.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/encrypt",
            "POST",
            1,
            serde_json::json!({"password":password,"confirmation":password}),
        ))
        .await
        .unwrap();
    let code = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    assert_eq!(code, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["security"]["encrypted"], true);
    assert_eq!(payload["security"]["plaintext_original"], true);
    assert_eq!(payload["generation"], 2);
    let retired = f.state.snapshot().unwrap();
    let response = app
        .clone()
        .oneshot(make("/api/security/lock", "POST", 2, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    f.wait_for_cleanup().await;
    for path in [
        "/api/files",
        "/api/media/.media_index.db",
        "/api/media/picture.png",
        "/api/archive/list/pages.zip",
        "/api/thumbnail/1",
        "/api/trash/logs",
        "/api/scan",
    ] {
        let response = app
            .clone()
            .oneshot(make(path, "GET", 3, serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::LOCKED, "{path}");
    }
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/unlock",
            "POST",
            3,
            serde_json::json!({"password":"incorrect"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(f.state.snapshot().unwrap().security_status().locked);
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/unlock",
            "POST",
            3,
            serde_json::json!({"password":password}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!f.state.snapshot().unwrap().security_status().locked);
    assert!(
        retired.conn.lock().unwrap().is_none(),
        "pre-lock requests never gain the unlocked connection"
    );
    assert!(retired.security_status().locked);
    let response = app
        .clone()
        .oneshot(make("/api/files", "GET", 4, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(make(
            "/api/media/.media_index.db",
            "GET",
            4,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // An operation captured before the last unlock cannot lock the new session.
    let response = app
        .clone()
        .oneshot(make("/api/security/lock", "POST", 3, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(!f.state.snapshot().unwrap().security_status().locked);
    let response = app
        .oneshot(make(
            "/api/security/plaintext",
            "DELETE",
            4,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!f.root.join(encryption::PLAIN_DB).exists());
}

#[tokio::test]
async fn database_security_refuses_transitions_during_scan() {
    let f = Fixture::new();
    f.state.snapshot().unwrap().scan.lock().unwrap().running = true;
    let response = build_router(f.state.clone())
        .oneshot(
            f.request("/api/security/encrypt")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"password":"long password 123","confirmation":"long password 123"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!f.root.join(encryption::SECURE_DB).exists());
}

#[tokio::test]
async fn password_change_retires_old_session_and_unlocks_only_with_new_password() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let old = "old password test 123";
    let new = "new password test 456";
    let make = |path: &str, generation: u64, body: serde_json::Value| {
        Request::builder()
            .uri(path)
            .method("POST")
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", f.state.token.as_str())
            .header("x-vault-library", generation.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/encrypt",
            1,
            serde_json::json!({"password":old,"confirmation":old}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let retired = f.state.snapshot().unwrap();
    for (current, password, confirmation) in [
        ("wrong", new, new),
        (old, new, "mismatch"),
        (old, "short", "short"),
        (old, old, old),
    ] {
        let response=app.clone().oneshot(make("/api/security/password",2,serde_json::json!({"current_password":current,"password":password,"confirmation":confirmation}))).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/password",
            2,
            serde_json::json!({"current_password":old,"password":new,"confirmation":new}),
        ))
        .await
        .unwrap();
    let code = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    assert_eq!(code, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["generation"], 3);
    assert_eq!(payload["password_changed"], true);
    assert_eq!(payload["security"]["locked"], false);
    assert!(retired.conn.lock().unwrap().is_none());
    let response = app
        .clone()
        .oneshot(make(
            "/api/security/password",
            2,
            serde_json::json!({"current_password":old,"password":new,"confirmation":new}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        app.clone()
            .oneshot(make("/api/security/lock", 3, serde_json::json!({})))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    f.wait_for_cleanup().await;
    assert_eq!(
        app.clone()
            .oneshot(make(
                "/api/security/password",
                4,
                serde_json::json!({"current_password":new,"password":old,"confirmation":old})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::LOCKED
    );
    assert_eq!(
        app.clone()
            .oneshot(make(
                "/api/security/unlock",
                4,
                serde_json::json!({"password":old})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.oneshot(make(
            "/api/security/unlock",
            4,
            serde_json::json!({"password":new})
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    assert!(!f.state.snapshot().unwrap().security_status().locked);
}

#[tokio::test]
async fn archive_encryption_api_job_lock_key_loss_and_stale_requests() {
    use std::io::Write;
    let f = Fixture::new();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(f.root.join("book.zip")).unwrap());
    zip.start_file("1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"image bytes").unwrap();
    zip.finish().unwrap();
    {
        let s = f.state.snapshot().unwrap();
        let g = s.conn.lock().unwrap();
        scanner::scan_directory(&f.root, g.as_ref().unwrap()).unwrap();
    }
    let app = build_router(f.state.clone());
    let make = |uri: &str, method: &str, generation: u64, body: serde_json::Value| {
        Request::builder()
            .uri(uri)
            .method(method)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", f.state.token.as_str())
            .header("x-vault-library", generation.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let res = app
        .clone()
        .oneshot(make(
            "/api/archive-security/1/encrypt",
            "POST",
            1,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = app
        .clone()
        .oneshot(make(
            "/api/security/encrypt",
            "POST",
            1,
            serde_json::json!({"password":"test password 123","confirmation":"test password 123"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(make(
            "/api/archive-security/1/encrypt",
            "POST",
            2,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(make("/api/security/lock", "POST", 2, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    for _ in 0..500 {
        if !f
            .state
            .snapshot()
            .unwrap()
            .archives
            .busy
            .load(std::sync::atomic::Ordering::Acquire)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let s = f.state.snapshot().unwrap();
    assert!(s.archives.job.lock().unwrap().completed);
    drop(s);
    let res = app
        .clone()
        .oneshot(make(
            "/api/archive/file/book.zip?entry=1.png",
            "GET",
            2,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(res.into_body(), 4096).await.unwrap().as_ref(),
        b"image bytes"
    );
    let old = f.state.snapshot().unwrap();
    let res = app
        .clone()
        .oneshot(make("/api/security/lock", "POST", 2, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(old.archives.key.lock().unwrap().is_none());
    f.wait_for_cleanup().await;
    let res = app
        .clone()
        .oneshot(make(
            "/api/archive-security",
            "GET",
            3,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::LOCKED);
    let key = f.root.join(archive_crypto::KEY_PATH);
    let saved = f.root.join("saved.key");
    std::fs::rename(&key, &saved).unwrap();
    let res = app
        .clone()
        .oneshot(make(
            "/api/security/unlock",
            "POST",
            3,
            serde_json::json!({"password":"test password 123"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(!key.exists());
    assert!(f.state.snapshot().unwrap().security_status().locked);
    std::fs::rename(saved, key).unwrap();
    let res = app
        .clone()
        .oneshot(make(
            "/api/security/unlock",
            "POST",
            3,
            serde_json::json!({"password":"test password 123"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        archive_security::image(&f.state.snapshot().unwrap(), "book.zip", "1.png").unwrap(),
        b"image bytes"
    );
    let res = app
        .oneshot(make(
            "/api/archive-security/1/plaintext",
            "DELETE",
            2,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn encrypted_archive_name_api_requires_auth_current_library_and_ready_record() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    {
        let snapshot = f.state.snapshot().unwrap();
        let guard = snapshot.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "original.zip", "ARCHIVE", 10, 1).unwrap();
        db::upsert_media_file(conn, "plain.zip", "ARCHIVE", 10, 1).unwrap();
        conn.execute("INSERT INTO archive_store(file_id,encrypted_path,state) VALUES(1,'.valtheum/archives/a.vta','ready')",[]).unwrap();
    }
    let request = |id, generation| {
        f.request_generation(&format!("/api/files/{id}/name"), generation)
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"renamed.zip"}"#))
            .unwrap()
    };
    let anonymous = Request::builder()
        .uri("/api/files/1/name")
        .method("POST")
        .header("host", "127.0.0.1:8080")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"renamed.zip"}"#))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(anonymous).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone().oneshot(request(1, 0)).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.clone().oneshot(request(2, 1)).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let response = app.clone().oneshot(request(1, 1)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(body["display_name"], "renamed.zip");
    let snapshot = f.state.snapshot().unwrap();
    let files =
        db::get_media_files(snapshot.conn.lock().unwrap().as_ref().unwrap(), None, None).unwrap();
    let file = files.iter().find(|f| f.id == 1).unwrap();
    assert_eq!(file.rel_path, "original.zip");
    assert_eq!(file.display_name.as_deref(), Some("renamed.zip"));
    snapshot.scan.lock().unwrap().running = true;
    assert_eq!(
        app.clone().oneshot(request(1, 1)).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    snapshot.scan.lock().unwrap().running = false;
}

#[path = "storage_recovery_tests.rs"]
mod storage_recovery_tests;

#[path = "review_regression_tests.rs"]
mod review_regression_tests;

#[path = "api_integrity_tests.rs"]
mod api_integrity_tests;

#[path = "api_backup_integrity_tests.rs"]
mod api_backup_integrity_tests;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_progress_poll_is_scoped_and_does_not_wait_for_database() {
    let f = Fixture::new();
    let library = f.state.snapshot().unwrap().library;
    *library.archives.job.lock().unwrap() = archive_security::Job {
        running: true,
        phase: "암호화".into(),
        done: 25,
        total: 100,
        ..Default::default()
    };
    let held = library.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _db = held.conn.lock().unwrap();
        ready_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let router = build_router(f.state.clone());
    let req = f
        .request("/api/archive-security/progress")
        .body(Body::empty())
        .unwrap();
    let request = tokio::spawn(router.clone().oneshot(req));
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), request).await;
    release_tx.send(()).unwrap();
    worker.join().unwrap();
    let response = result
        .expect("progress must not wait for DB I/O")
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(data["job"]["done"], 25);
    assert_eq!(data["job"]["total"], 100);
    assert!(data.get("files").is_none());
    let stale = f
        .request_generation("/api/archive-security/progress", 0)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(stale).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    let unauthorized = Request::builder()
        .uri("/api/archive-security/progress")
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(unauthorized).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    f.state
        .concealed
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        router
            .oneshot(
                f.request("/api/archive-security/progress")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::LOCKED
    );
}

#[tokio::test]
async fn pdf_and_images_scan_serve_ranges_and_keep_shared_metadata() {
    let f = Fixture::new();
    let pdf = include_bytes!("../tests/fixtures/documents/sample.pdf");
    for name in ["book.PDF", "photo.AVIF", "photo.BMP", "photo.PNG"] {
        std::fs::write(f.root.join(name), pdf).unwrap();
    }
    let library = f.state.snapshot().unwrap().library;
    {
        let guard = library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let scan = scanner::scan_directory(&f.root, conn).unwrap();
        assert_eq!(scan.scanned_count, 4);
        let files = db::get_media_files(conn, None, None).unwrap();
        let document = files.iter().find(|file| file.file_type == "PDF").unwrap();
        assert_eq!(
            files
                .iter()
                .filter(|file| file.file_type == "IMAGE")
                .count(),
            3
        );
        let image = files.iter().find(|file| file.file_type == "IMAGE").unwrap();
        series::save(
            conn,
            &series::SaveSeries {
                id: None,
                name: "Documents".into(),
                representative_id: document.id,
                file_ids: vec![document.id, image.id],
            },
        )
        .unwrap();
        db::update_rating(conn, document.id, 4).unwrap();
        db::add_tag_to_file(conn, image.id, "shared").unwrap();
        let bookmark = db::save_bookmark(conn, document.id, 2.0, &[0xff, 0xd8, 0xff]).unwrap();
        assert_eq!(
            db::list_bookmarks(conn, document.id).unwrap()[0].position,
            2.0
        );
        assert!(matches!(
            db::save_bookmark(conn, document.id, 1.5, &[0xff, 0xd8, 0xff]),
            Err(db::BookmarkError::InvalidPosition)
        ));
        scanner::scan_directory(&f.root, conn).unwrap();
        let after = db::get_media_files(conn, None, None).unwrap();
        assert_eq!(
            after
                .iter()
                .find(|file| file.id == image.id)
                .unwrap()
                .rating,
            4
        );
        assert_eq!(
            db::list_bookmarks(conn, document.id).unwrap()[0].id,
            bookmark.id
        );
    }
    let router = build_router(f.state.clone());
    let response = router
        .clone()
        .oneshot(
            f.request("/api/media/book.PDF")
                .header("range", "bytes=0-7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        to_bytes(response.into_body(), 64).await.unwrap().as_ref(),
        &pdf[..8]
    );
    for (uri, status) in [
        ("/api/media/book.PDF", StatusCode::CONFLICT),
        ("/api/media/photo.AVIF", StatusCode::CONFLICT),
    ] {
        assert_eq!(
            router
                .clone()
                .oneshot(f.request_generation(uri, 0).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            status
        );
    }
    let no_token = Request::builder()
        .uri("/api/media/book.PDF")
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(no_token).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    f.state
        .concealed
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        router
            .oneshot(
                f.request("/api/media/book.PDF")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::LOCKED
    );
}

#[tokio::test]
async fn dynamic_server_port_preserves_host_origin_and_token_guards() {
    let mut f = Fixture::new();
    f.state.http_authority = Arc::new("127.0.0.1:54321".into());
    let app = build_router(f.state.clone());
    for (host, origin, token, expected) in [
        (
            "127.0.0.1:54321",
            "http://127.0.0.1:54321",
            f.state.token.as_str(),
            StatusCode::OK,
        ),
        (
            "127.0.0.1:8080",
            "http://127.0.0.1:54321",
            f.state.token.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (
            "127.0.0.1:54321",
            "http://127.0.0.1:8080",
            f.state.token.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (
            "127.0.0.1:54321",
            "http://untrusted.example",
            f.state.token.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (
            "127.0.0.1:54321",
            "http://127.0.0.1:54321",
            "wrong",
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/session")
                    .header("host", host)
                    .header("origin", origin)
                    .header("x-vault-token", token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn closing_busy_library_detaches_immediately_but_keeps_writer_connection_until_finished() {
    use std::sync::atomic::Ordering;
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let old = f.state.snapshot().unwrap();
    old.archives.busy.store(true, Ordering::Release);
    let security = old.security.lock().unwrap();
    let key = old.archives.key.lock().unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        app.clone().oneshot(
            f.request("/api/folders/close")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("close must not wait for a busy writer")
    .unwrap();
    drop(security);
    drop(key);
    assert_eq!(response.status(), StatusCode::OK);
    assert!(f.state.concealed.load(Ordering::Acquire));
    assert!(old.archives.cancel.load(Ordering::Acquire));
    assert!(old.archives.revoked.load(Ordering::Acquire));
    assert_eq!(f.state.snapshot().unwrap().generation, 2);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    // Simulate the last commit/rollback after a blocked filesystem write returns.
    assert!(old.conn.lock().unwrap().is_some());
    old.conn
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .execute_batch("BEGIN; ROLLBACK;")
        .unwrap();
    assert!(
        Library::open(f.root.clone()).is_err(),
        "lease must outlive the write"
    );
    let stale = app
        .oneshot(f.request("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    old.archives.busy.store(false, Ordering::Release);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if old.conn.lock().unwrap().is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn blocked_security_mutex_cannot_stall_progress_cancel_or_close() {
    use std::sync::{atomic::Ordering, mpsc};
    use std::time::Duration;
    struct Release(Option<mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let old = f.state.snapshot().unwrap();
    old.archives.busy.store(true, Ordering::Release);
    *old.archives.job.lock().unwrap() = archive_security::Job {
        job_id: Some("test-job".into()),
        running: true,
        ..Default::default()
    };
    let held = old.clone();
    let (tx, rx) = mpsc::channel();
    let release = Release(Some(tx));
    let (ready_tx, ready_rx) = mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = held.security.lock().unwrap();
        ready_tx.send(()).unwrap();
        rx.recv().unwrap();
    });
    ready_rx.recv().unwrap();
    let (started, ready) = mpsc::channel();
    let mut readers = vec![];
    for (url, method) in [
        ("/api/archive-security/progress", "GET"),
        ("/api/archive-security/cancel?job_id=test-job", "POST"),
    ] {
        let app = build_router(f.state.clone());
        let request = f.request(url).method(method).body(Body::empty()).unwrap();
        let started = started.clone();
        readers.push(rt.spawn(async move {
            started.send(()).unwrap();
            app.oneshot(request).await.unwrap()
        }));
    }
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let app = build_router(f.state.clone());
    let request = f
        .request("/api/folders/close")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let (closed_tx, closed_rx) = mpsc::channel();
    let closer = rt.spawn(async move {
        let response = app.oneshot(request).await.unwrap();
        closed_tx.send(response.status()).unwrap();
    });
    let before = closed_rx.recv_timeout(Duration::from_secs(2)).ok();
    drop(release);
    holder.join().unwrap();
    rt.block_on(async {
        for reader in readers {
            let _ = reader.await.unwrap();
        }
        closer.await.unwrap();
    });
    old.archives.busy.store(false, Ordering::Release);
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(150)).await;
    });
    println!(
        "close_completed_before_security_mutex_released={:?}",
        before
    );
    assert_eq!(
        before,
        Some(StatusCode::OK),
        "all HTTP workers must remain responsive while storage holds the security mutex"
    );
}

#[test]
fn alias_db_wait_does_not_block_close_or_write_after_revocation() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    struct Release(Option<mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let old = f.state.snapshot().unwrap();
    {
        let guard = old.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "old.zip", "ARCHIVE", 100, 1).unwrap();
        conn.execute(
            "INSERT INTO archive_store(file_id,encrypted_path,state) VALUES(1,'old.vta','ready')",
            [],
        )
        .unwrap();
    }
    // First verify the queued route still performs an ordinary alias update.
    let request = f
        .request("/api/files/1/name")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"before.zip","restore_name":false}"#))
        .unwrap();
    let response = rt
        .block_on(build_router(f.state.clone()).oneshot(request))
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let held = old.clone();
    let (tx, rx) = mpsc::channel();
    let release = Release(Some(tx));
    let (ready_tx, ready_rx) = mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = held.conn.lock().unwrap();
        ready_tx.send(()).unwrap();
        rx.recv().unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let request = f
        .request("/api/files/1/name")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"after.zip","restore_name":false}"#))
        .unwrap();
    let app = build_router(f.state.clone());
    let rename = rt.spawn(async move { app.oneshot(request).await.unwrap() });
    let deadline = Instant::now() + Duration::from_secs(2);
    while old.db_slots.available_permits() != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let queued = old.db_slots.available_permits() == 0;
    let request = f
        .request("/api/folders/close")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let app = build_router(f.state.clone());
    let (closed_tx, closed_rx) = mpsc::channel();
    let closer = rt.spawn(async move {
        let response = app.oneshot(request).await.unwrap();
        let _ = closed_tx.send(response.status());
    });
    let closed = closed_rx.recv_timeout(Duration::from_secs(2)).ok();
    drop(release);
    holder.join().unwrap();
    let renamed = rt.block_on(async {
        closer.await.unwrap();
        rename.await.unwrap().status()
    });
    assert!(queued, "rename must enter the asynchronous DB queue");
    assert_eq!(
        closed,
        Some(StatusCode::OK),
        "close must finish before the DB becomes available"
    );
    assert_eq!(renamed, StatusCode::CONFLICT);
    let conn = encryption::open(&f.root.join(encryption::PLAIN_DB), None, false).unwrap();
    let name: String = conn
        .query_row("SELECT display_name FROM media_files WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        name, "before.zip",
        "revoked work must not update the old library"
    );
    drop(conn);
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(150)).await;
    });
}

#[test]
fn waiting_metadata_writes_do_not_commit_after_library_close() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    for (method, uri, body, query, expected) in [
        (
            "POST",
            "/api/files/1/tags",
            r#"{"name":"late-write"}"#,
            "SELECT COUNT(*) FROM tags",
            0,
        ),
        (
            "POST",
            "/api/series",
            r#"{"name":"late-series","file_ids":[1,2],"representative_id":1}"#,
            "SELECT COUNT(*) FROM media_series",
            0,
        ),
        (
            "POST",
            "/api/arena/vote",
            r#"{"winner_id":1,"loser_id":2}"#,
            "SELECT score FROM media_files WHERE id=1",
            1000,
        ),
        (
            "POST",
            "/api/files/1/bookmarks",
            r#"{"position":5,"thumbnail_data":"/9j/"}"#,
            "SELECT COUNT(*) FROM media_bookmarks",
            1,
        ),
        (
            "DELETE",
            "/api/files/1/bookmarks/1",
            "",
            "SELECT COUNT(*) FROM media_bookmarks",
            1,
        ),
        (
            "POST",
            "/api/thumbnail/1",
            "",
            "SELECT COUNT(*) FROM thumbnails",
            0,
        ),
    ] {
        let f = Fixture::new();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let old = f.state.snapshot().unwrap();
        {
            let g = old.conn.lock().unwrap();
            let conn = g.as_ref().unwrap();
            db::upsert_media_file(conn, "a.mp4", "VIDEO", 1, 1).unwrap();
            db::upsert_media_file(conn, "b.mp4", "VIDEO", 1, 1).unwrap();
            db::save_bookmark(conn, 1, 0.0, &[0xff, 0xd8, 0xff]).unwrap();
        }
        let held = old.clone();
        let (release, released) = mpsc::channel();
        let (ready, waiting) = mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _g = held.conn.lock().unwrap();
            ready.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(10));
        });
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        let (content_type, body) = if uri == "/api/thumbnail/1" {
            ("multipart/form-data; boundary=fixture", "--fixture\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a.jpg\"\r\n\r\nimage bytes\r\n--fixture--\r\n")
        } else {
            ("application/json", body)
        };
        let request = f
            .request(uri)
            .method(method)
            .header("content-type", content_type)
            .body(Body::from(body))
            .unwrap();
        let router = build_router(f.state.clone());
        let writer = rt.spawn(async move { router.oneshot(request).await.unwrap() });
        let deadline = Instant::now() + Duration::from_secs(2);
        while old.db_slots.available_permits() != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        // Delay cleanup independently so the queued writer receives the held DB first.
        let security = old.security.lock().unwrap();
        let response = rt
            .block_on(
                build_router(f.state.clone()).oneshot(
                    f.request("/api/folders/close")
                        .method("POST")
                        .body(Body::empty())
                        .unwrap(),
                ),
            )
            .unwrap();
        release.send(()).unwrap();
        holder.join().unwrap();
        let write_status = rt.block_on(writer).unwrap().status();
        let value: i64 = old
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row(query, [], |r| r.get(0))
            .unwrap();
        drop(security);
        rt.block_on(async {
            tokio::time::sleep(Duration::from_millis(150)).await;
        });
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(write_status, StatusCode::CONFLICT);
        assert_eq!(value, expected, "late write changed DB: {uri}");
    }
}

#[test]
fn backup_db_wait_allows_close_and_preserves_existing_backup() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let old = f.state.snapshot().unwrap();
    let conn = old.conn.lock().unwrap();
    let app = f.state.clone();
    let snapshot = old.clone();
    let dest = f.root.join("synthetic.vtbackup");
    std::fs::write(&dest, b"previous backup").unwrap();
    let backup = std::thread::spawn(move || backup_library_at(&app, &snapshot, &dest));
    let deadline = Instant::now() + Duration::from_secs(2);
    let held = loop {
        if old.archives.busy.load(std::sync::atomic::Ordering::Acquire) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let request = f
        .request("/api/folders/close")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let router = build_router(f.state.clone());
    let (tx, rx) = mpsc::channel();
    let closer = rt.spawn(async move {
        let response = router.oneshot(request).await.unwrap();
        tx.send(response.status()).unwrap();
    });
    let before = rx.recv_timeout(Duration::from_secs(2)).ok();
    drop(conn);
    let result = backup.join().unwrap();
    rt.block_on(closer).unwrap();

    assert!(held);
    assert_eq!(before, Some(StatusCode::OK));
    assert!(result.is_err());
    assert!(!old.archives.busy.load(std::sync::atomic::Ordering::Acquire));
    assert_eq!(
        std::fs::read(f.root.join("synthetic.vtbackup")).unwrap(),
        b"previous backup"
    );
    assert!(!old.archives.job.lock().unwrap().completed);
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(150)).await;
    });
}

#[tokio::test]
async fn permanent_delete_rejects_restored_active_file() {
    let f = Fixture::new();
    std::fs::write(f.root.join("restored.png"), b"original bytes").unwrap();
    {
        let state = f.state.snapshot().unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "restored.png", "IMAGE", 14, 1).unwrap();
        db::move_to_trash(conn, 1).unwrap();
    }
    let router = build_router(f.state.clone());
    let restored = router
        .clone()
        .oneshot(
            f.request("/api/files/1/restore")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let deleted = router
        .clone()
        .oneshot(
            f.request("/api/files/1/permanent")
                .method("DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restored.status(), StatusCode::OK);
    assert_eq!(deleted.status(), StatusCode::CONFLICT);
    assert_eq!(
        std::fs::read(f.root.join("restored.png")).unwrap(),
        b"original bytes"
    );
    let state = f.state.snapshot().unwrap();
    {
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        assert_eq!(db::get_deleted_logs(conn).unwrap().len(), 0);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM delete_intents", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        db::move_to_trash(conn, 1).unwrap();
    }
    let deleted = router
        .oneshot(
            f.request("/api/files/1/permanent")
                .method("DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);
    assert!(!f.root.join("restored.png").exists());
}

#[test]
fn relink_after_picker_allows_close_and_rejects_retired_db_write() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    std::fs::write(f.root.join("renamed.png"), b"fixture").unwrap();
    let old = f.state.snapshot().unwrap();
    {
        let guard = old.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "old.png", "IMAGE", 7, 1).unwrap();
        conn.execute("UPDATE media_files SET is_missing=1", [])
            .unwrap();
    }
    let held = old.conn.lock().unwrap();
    let app = f.state.clone();
    let snapshot = old.clone();
    let path = f.root.join("renamed.png");
    let relink =
        rt.spawn(async move { crate::relink::reconnect_selected(app, snapshot, 1, path).await });
    let deadline = Instant::now() + Duration::from_secs(2);
    let locked = loop {
        if old.db_slots.available_permits() == 0 {
            break true;
        }
        if Instant::now() > deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let router = build_router(f.state.clone());
    let request = f
        .request("/api/folders/close")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let closing = rt.spawn(async move {
        let response = router.oneshot(request).await.unwrap();
        tx.send(response.status()).unwrap();
    });
    let before = rx.recv_timeout(Duration::from_secs(2)).ok();
    drop(held);
    let relinked = rt.block_on(relink).unwrap().unwrap_err().0;
    rt.block_on(closing).unwrap();

    assert!(locked);
    assert_eq!(before, Some(StatusCode::OK));
    assert_eq!(relinked, StatusCode::CONFLICT);
    let conn = encryption::open(&f.root.join(encryption::PLAIN_DB), None, false).unwrap();
    assert_eq!(
        conn.query_row("SELECT rel_path FROM media_files WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "old.png"
    );
    drop(conn);
    rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(150)).await;
    });
}

#[test]
fn locking_active_scan_is_immediate_and_waits_before_unlocking() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let router = build_router(f.state.clone());
    let password = "scan lock fixture password";
    let req = f
        .request("/api/security/encrypt")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"password":password,"confirmation":password}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        rt.block_on(router.clone().oneshot(req)).unwrap().status(),
        StatusCode::OK
    );
    let old = f.state.snapshot().unwrap();
    old.scan.lock().unwrap().running = true;
    let req = f
        .request_generation("/api/security/lock", 2)
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let result = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), router.clone().oneshot(req)).await
    });
    if result.is_err() {
        old.scan.lock().unwrap().running = false;
        panic!("lock waited for scan");
    }
    assert_eq!(result.unwrap().unwrap().status(), StatusCode::OK);
    let locked = f.state.snapshot().unwrap();
    assert!(f.state.concealed.load(Ordering::Acquire));
    assert!(old.archives.revoked.load(Ordering::Acquire));
    assert!(locked.cleanup_pending.load(Ordering::Acquire));
    assert!(encryption::require_idle(&locked).is_err());
    old.scan.lock().unwrap().running = false;
    rt.block_on(f.wait_for_cleanup());
    assert!(!locked.cleanup_pending.load(Ordering::Acquire));
}

#[test]
fn concealment_probe_ignores_blocked_security_and_db_handles() {
    use std::time::Duration;
    let f = Fixture::new();
    let old = f.state.snapshot().unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let security = old.security.lock().unwrap();
    let db = old.conn.lock().unwrap();
    let req = f
        .request_generation("/api/session?concealment=true", 999)
        .body(Body::empty())
        .unwrap();
    let result = rt.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(2),
            build_router(f.state.clone()).oneshot(req),
        )
        .await
    });
    drop(db);
    drop(security);
    let response = result.expect("probe waited for storage").unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = rt
        .block_on(to_bytes(response.into_body(), usize::MAX))
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value, serde_json::json!({"generation":1,"covered":false}));
}

#[test]
fn security_conversion_allows_close_while_db_waits_and_never_reopens_retired_library() {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let old = f.state.snapshot().unwrap();
    let db = old.conn.lock().unwrap();
    let router = build_router(f.state.clone());
    let make_request = || {
        f.request("/api/security/encrypt")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"password":"synthetic password","confirmation":"synthetic password"}"#,
            ))
            .unwrap()
    };
    let encrypt = rt.spawn(router.clone().oneshot(make_request()));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !old.archives.busy.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let active = old.archives.busy.load(Ordering::Acquire);
    let duplicate = rt.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(2),
            router.clone().oneshot(make_request()),
        )
        .await
    });
    let close = rt.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(2),
            router.oneshot(
                f.request("/api/folders/close")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            ),
        )
        .await
    });
    // Always release the worker before asserting, even if the close regresses.
    drop(db);
    let result = rt.block_on(encrypt).unwrap().unwrap();
    assert!(active);
    assert_eq!(duplicate.unwrap().unwrap().status(), StatusCode::CONFLICT);
    assert_eq!(close.unwrap().unwrap().status(), StatusCode::OK);
    assert_eq!(result.status(), StatusCode::CONFLICT);
    assert_eq!(f.state.snapshot().unwrap().generation, 2);
    assert!(f.state.snapshot().unwrap().root_dir.as_os_str().is_empty());
    assert!(f.state.concealed.load(Ordering::Acquire));
    assert!(old.security.lock().unwrap().password.is_none());
    assert!(!old.archives.busy.load(Ordering::Acquire));
}

#[tokio::test]
async fn pdf_resume_position_persists_and_rejects_other_media_and_stale_sessions() {
    let f = Fixture::new();
    {
        let snapshot = f.state.snapshot().unwrap();
        let guard = snapshot.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        for (path, kind) in [
            ("book.pdf", "PDF"),
            ("video.mp4", "VIDEO"),
            ("archive.zip", "ARCHIVE"),
            ("trash.pdf", "PDF"),
        ] {
            db::upsert_media_file(conn, path, kind, 1, 1).unwrap();
        }
        conn.execute("UPDATE media_files SET is_trash=1 WHERE id=4", [])
            .unwrap();
    }
    let app = build_router(f.state.clone());
    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/pdf-position")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(value["page"].is_null());
    for (id, body, expected) in [
        (1, r#"{"page":17}"#, StatusCode::NO_CONTENT),
        (1, r#"{"page":0}"#, StatusCode::BAD_REQUEST),
        (1, r#"{"page":null}"#, StatusCode::BAD_REQUEST),
        (1, r#"{"page":1.5}"#, StatusCode::UNPROCESSABLE_ENTITY),
        (2, r#"{"page":8}"#, StatusCode::NOT_FOUND),
        (3, r#"{"page":8}"#, StatusCode::NOT_FOUND),
        (4, r#"{"page":8}"#, StatusCode::NOT_FOUND),
        (999, r#"{"page":8}"#, StatusCode::NOT_FOUND),
    ] {
        let response = app
            .clone()
            .oneshot(
                f.request(&format!("/api/files/{id}/pdf-position"))
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{id}: {body}");
    }
    // Closing and reopening the connection retains the position, including after a rescan.
    {
        let snapshot = f.state.snapshot().unwrap();
        let mut guard = snapshot.conn.lock().unwrap();
        let old = guard.take().unwrap();
        let path = old.path().unwrap().to_owned();
        drop(old);
        let conn = rusqlite::Connection::open(path).unwrap();
        db::init_db(&conn).unwrap();
        db::upsert_media_file(&conn, "book.pdf", "PDF", 2, 2).unwrap();
        let count: i64 = conn
            .query_row("SELECT watch_count FROM media_files WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "position does not count as a qualified view");
        *guard = Some(conn);
    }
    let response = app
        .clone()
        .oneshot(
            f.request("/api/files/1/pdf-position")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(value["page"], 17);
    let response = app
        .clone()
        .oneshot(
            f.request_generation("/api/files/1/pdf-position", 0)
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"page":9}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn pdf_ranges_do_not_wait_for_the_database_queue() {
    let f = Fixture::new();
    let bytes = include_bytes!("../tests/fixtures/documents/sample.pdf");
    std::fs::write(f.root.join("queued.pdf"), bytes).unwrap();
    let snapshot = f.state.snapshot().unwrap();
    let held = snapshot.db_slots.clone().acquire_owned().await.unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        build_router(f.state.clone()).oneshot(
            f.request("/api/media/queued.pdf")
                .header("range", "bytes=0-7")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await;
    drop(held);
    let response = response
        .expect("PDF byte range blocked behind the database queue")
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        &to_bytes(response.into_body(), 64).await.unwrap()[..],
        &bytes[..8]
    );
}

#[tokio::test]
async fn waiting_pdf_file_reads_are_rejected_after_library_close() {
    let f = Fixture::new();
    std::fs::write(f.root.join("queued.pdf"), b"%PDF-synthetic").unwrap();
    let snapshot = f.state.snapshot().unwrap();
    let held = snapshot
        .file_slots
        .clone()
        .acquire_many_owned(4)
        .await
        .unwrap();
    let app = build_router(f.state.clone());
    let request = f
        .request("/api/media/queued.pdf")
        .body(Body::empty())
        .unwrap();
    let pending = tokio::spawn(app.clone().oneshot(request));
    tokio::task::yield_now().await;
    let response = app
        .oneshot(
            f.request("/api/folders/close")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), pending).await;
    drop(held);
    let status = result
        .expect("closed library left file read waiting")
        .unwrap()
        .unwrap()
        .status();
    assert!(matches!(status, StatusCode::CONFLICT | StatusCode::LOCKED));
    f.wait_for_cleanup().await;
}

#[tokio::test]
async fn plain_media_stream_stops_after_close_before_and_during_body_reads() {
    use futures_core::Stream;
    use std::{future::poll_fn, pin::Pin};
    for read_first in [false, true] {
        let f = Fixture::new();
        std::fs::write(f.root.join("stream.pdf"), vec![71; 2 * 1024 * 1024]).unwrap();
        let app = build_router(f.state.clone());
        let response = app
            .clone()
            .oneshot(
                f.request("/api/media/stream.pdf")
                    .header("range", "bytes=0-1048575")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "1048576");
        let mut stream = response.into_body().into_data_stream();
        if read_first {
            let bytes = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx))
                .await
                .unwrap()
                .unwrap();
            assert!(!bytes.is_empty());
            assert!(bytes.len() < 1048576);
        }
        let closed = app
            .oneshot(
                f.request("/api/folders/close")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(closed.status(), StatusCode::OK);
        assert!(poll_fn(|cx| Pin::new(&mut stream).poll_next(cx))
            .await
            .unwrap()
            .is_err());
        assert!(poll_fn(|cx| Pin::new(&mut stream).poll_next(cx))
            .await
            .is_none());
        f.wait_for_cleanup().await;
    }
}

#[tokio::test]
async fn pdf_position_revision_rejects_delayed_writes_and_survives_reopen() {
    let f = Fixture::new();
    {
        let snapshot = f.state.snapshot().unwrap();
        let guard = snapshot.conn.lock().unwrap();
        db::upsert_media_file(guard.as_ref().unwrap(), "revision.pdf", "PDF", 1, 1).unwrap();
    }
    let app = build_router(f.state.clone());
    for (page, revision, status) in [
        (4, 0, StatusCode::NO_CONTENT),
        (2, 0, StatusCode::CONFLICT),
        (5, 1, StatusCode::NO_CONTENT),
        (4, 1, StatusCode::CONFLICT),
    ] {
        let payload = serde_json::json!({"page":page,"revision":revision});
        let response = app
            .clone()
            .oneshot(
                f.request("/api/files/1/pdf-position")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    {
        let snapshot = f.state.snapshot().unwrap();
        let mut guard = snapshot.conn.lock().unwrap();
        let old = guard.take().unwrap();
        let path = old.path().unwrap().to_owned();
        drop(old);
        let conn = rusqlite::Connection::open(path).unwrap();
        db::init_db(&conn).unwrap();
        *guard = Some(conn);
    }
    let response = app
        .oneshot(
            f.request("/api/files/1/pdf-position")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let result: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(result, serde_json::json!({"page":5,"revision":2}));
}
