use super::*;

#[tokio::test]
async fn reconnect_reopens_plain_and_cipher_db_without_scanning_or_losing_metadata() {
    for encrypted in [false, true] {
        let f = Fixture::new();
        let app = build_router(f.state.clone());
        {
            let snapshot = f.state.snapshot().unwrap();
            let guard = snapshot.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            db::upsert_media_file(conn, "offline.mp4", "VIDEO", 100, 1).unwrap();
            conn.execute("UPDATE media_files SET rating=5", []).unwrap();
            db::save_thumbnail(conn, 1, b"saved-thumbnail", 1, 1, None, None).unwrap();
        }
        if encrypted {
            let response = app.clone().oneshot(f.request("/api/security/encrypt")
                .method("POST").header("content-type","application/json")
                .body(Body::from(r#"{"password":"reconnect test password","confirmation":"reconnect test password"}"#)).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let state = f.state.snapshot().unwrap();
        // A broken connection returns a read error. It must not look like a missing thumbnail.
        *state.conn.lock().unwrap() = Some(rusqlite::Connection::open_in_memory().unwrap());
        let request = || {
            f.request_generation("/api/thumbnail/1", state.generation)
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let response = app
            .clone()
            .oneshot(
                f.request_generation("/api/storage/reconnect", state.generation)
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(f.state.snapshot().unwrap().generation, state.generation);
        assert!(!state.scan_status().running);
        assert!(!state
            .archives
            .busy
            .load(std::sync::atomic::Ordering::Acquire));
        let thumbnail = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(thumbnail.status(), StatusCode::OK);
        assert_eq!(
            &to_bytes(thumbnail.into_body(), 100).await.unwrap()[..],
            b"saved-thumbnail"
        );
        let guard = state.conn.lock().unwrap();
        let (rating, missing): (i64, i64) = guard
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT rating,is_missing FROM media_files WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rating, missing), (5, 0)); // no filesystem scan for the intentionally absent movie
    }
}

#[tokio::test]
async fn reconnect_failure_never_creates_a_database_and_retains_the_old_connection() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let state = f.state.snapshot().unwrap();
    state.conn.lock().unwrap().take();
    let path = f.root.join(encryption::PLAIN_DB);
    let backup = f.root.join("original.db");
    std::fs::rename(&path, &backup).unwrap();
    let memory = rusqlite::Connection::open_in_memory().unwrap();
    memory
        .execute_batch("CREATE TABLE sentinel(value); INSERT INTO sentinel VALUES(123);")
        .unwrap();
    *state.conn.lock().unwrap() = Some(memory);
    let request = || {
        f.request("/api/storage/reconnect")
            .method("POST")
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(request()).await.unwrap().status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(!path.exists());
    assert!(!state
        .archives
        .busy
        .load(std::sync::atomic::Ordering::Acquire));
    let value: i64 = state
        .conn
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .query_row("SELECT value FROM sentinel", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, 123);
    std::fs::rename(&backup, &path).unwrap();
    assert_eq!(
        app.oneshot(request()).await.unwrap().status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn reconnect_requires_auth_current_library_visibility_and_idle_work() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let state = f.state.snapshot().unwrap();
    let request = || {
        f.request("/api/storage/reconnect")
            .method("POST")
            .body(Body::empty())
            .unwrap()
    };
    let unauth = Request::builder()
        .uri("/api/storage/reconnect")
        .method("POST")
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(unauth).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let stale = f
        .request_generation("/api/storage/reconnect", 0)
        .method("POST")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(stale).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    state.scan.lock().unwrap().running = true;
    assert_eq!(
        app.clone().oneshot(request()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    state.scan.lock().unwrap().running = false;
    state
        .archives
        .busy
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        app.clone().oneshot(request()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    state
        .archives
        .busy
        .store(false, std::sync::atomic::Ordering::Release);
    f.state
        .concealed
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        app.oneshot(request()).await.unwrap().status(),
        StatusCode::LOCKED
    );
}

struct ReleaseBlocked(Option<std::sync::mpsc::Sender<()>>);
impl Drop for ReleaseBlocked {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[tokio::test]
async fn timeout_returns_while_io_is_blocked_and_late_connection_is_discarded() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    let (release, blocked) = std::sync::mpsc::channel();
    let release = ReleaseBlocked(Some(release));
    let request = crate::storage_recovery::reconnect_with(
        f.state.clone(),
        state.clone(),
        std::time::Duration::from_millis(30),
        None,
        move |_, _| {
            blocked.recv().unwrap();
            Ok(rusqlite::Connection::open_in_memory()?)
        },
    );
    let response = tokio::time::timeout(std::time::Duration::from_secs(2), request)
        .await
        .unwrap();
    assert_eq!(response.unwrap_err().0, StatusCode::GATEWAY_TIMEOUT);
    assert!(
        state.recovery.is_running(),
        "do not allow writes while OS I/O still runs"
    );
    assert!(encryption::require_idle(&state).is_err());
    drop(release);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.recovery.is_running() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        state
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row("SELECT count(*) FROM media_files", [], |r| r
                .get::<_, i64>(0))
            .is_ok(),
        "late in-memory connection was not installed"
    );
}

#[tokio::test]
async fn cancel_is_scoped_blocks_duplicate_work_and_close_does_not_wait_for_nas() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    let app = build_router(f.state.clone());
    let (release, blocked) = std::sync::mpsc::channel();
    let release = ReleaseBlocked(Some(release));
    let (started, ready) = tokio::sync::oneshot::channel();
    let reconnect = tokio::spawn(crate::storage_recovery::reconnect_with(
        f.state.clone(),
        state.clone(),
        std::time::Duration::from_secs(15),
        Some("current".into()),
        move |_, _| {
            let _ = started.send(());
            blocked.recv().unwrap();
            Ok(rusqlite::Connection::open_in_memory()?)
        },
    ));
    ready.await.unwrap();
    let cancel = |id: &str| {
        f.request("/api/storage/cancel")
            .method("POST")
            .header("x-vault-reconnect", id)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(cancel("older")).await.unwrap().status(),
        StatusCode::OK
    );
    assert!(!reconnect.is_finished());
    assert_eq!(
        app.clone()
            .oneshot(
                f.request("/api/storage/reconnect")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.clone()
            .oneshot(cancel("current"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), reconnect)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap_err().0, StatusCode::REQUEST_TIMEOUT);
    assert!(state.recovery.is_running());
    let closing = app.oneshot(
        f.request("/api/folders/close")
            .method("POST")
            .body(Body::empty())
            .unwrap(),
    );
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), closing)
            .await
            .unwrap()
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(f.state.snapshot().unwrap().root_dir.as_os_str().is_empty());
    assert!(
        Library::open(f.root.clone()).is_err(),
        "old I/O retains the cross-PC lock"
    );
    drop(release);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.recovery.is_running() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(state.conn.lock().unwrap().is_none());
    assert_eq!(f.state.snapshot().unwrap().generation, 2);
}

#[tokio::test]
async fn closing_does_not_wait_for_a_blocked_read_holding_the_old_connection() {
    let f = Fixture::new();
    let old = f.state.snapshot().unwrap();
    let held = old.clone();
    let (release, blocked) = std::sync::mpsc::channel();
    let release = ReleaseBlocked(Some(release));
    let (started, ready) = tokio::sync::oneshot::channel();
    let reader = tokio::task::spawn_blocking(move || {
        let _slot = held.conn.lock().unwrap();
        let _ = started.send(());
        blocked.recv().unwrap();
    });
    ready.await.unwrap();
    let app = build_router(f.state.clone());
    let closing = app.oneshot(
        f.request("/api/folders/close")
            .method("POST")
            .body(Body::empty())
            .unwrap(),
    );
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), closing)
            .await
            .unwrap()
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(old
        .archives
        .revoked
        .load(std::sync::atomic::Ordering::Acquire));
    assert!(f.state.snapshot().unwrap().root_dir.as_os_str().is_empty());
    drop(release);
    reader.await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if old.conn.try_lock().is_ok_and(|slot| slot.is_none()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancellation_arriving_before_reconnect_prevents_any_nas_io() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let response = app
        .oneshot(
            f.request("/api/storage/cancel")
                .method("POST")
                .header("x-vault-reconnect", "cancel-first")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = crate::storage_recovery::reconnect_with(
        f.state.clone(),
        f.state.snapshot().unwrap(),
        std::time::Duration::from_secs(15),
        Some("cancel-first".into()),
        |_, _| panic!("cancelled request must not open NAS"),
    )
    .await;
    assert_eq!(result.unwrap_err().0, StatusCode::REQUEST_TIMEOUT);
    assert!(!f.state.snapshot().unwrap().recovery.is_running());
}
