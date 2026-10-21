use super::*;

async fn job(f: &Fixture, app: &Router) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            f.request("/api/integrity/quick")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
#[tokio::test]
async fn quick_check_reports_missing_and_changed_files_without_modifying_library() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    let app = build_router(f.state.clone());
    std::fs::write(f.root.join("normal.png"), b"keep").unwrap();
    std::fs::write(f.root.join("changed.png"), b"changed").unwrap();
    {
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        for name in ["normal.png", "missing.png", "changed.png"] {
            db::upsert_media_file(conn, name, "IMAGE", 4, 1).unwrap();
        }
    }
    assert_eq!(job(&f, &app).await["id"], 0);
    let response = app
        .clone()
        .oneshot(
            f.request("/api/integrity/quick")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let finished = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let j = job(&f, &app).await;
            if !j["running"].as_bool().unwrap() {
                break j;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(finished["completed"], true);
    assert_eq!(finished["checked"], 3);
    assert_eq!(finished["ok"], 1);
    assert_eq!(finished["issues"], 2);
    assert_eq!(finished["unavailable"], 0);
    let states: Vec<_> = finished["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["state"].as_str().unwrap())
        .collect();
    assert!(states.contains(&"missing"));
    assert!(states.contains(&"mismatch"));
    assert_eq!(std::fs::read(f.root.join("normal.png")).unwrap(), b"keep");
    assert_eq!(
        state
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row("SELECT sum(is_missing) FROM media_files", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

struct Release(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn quick_check_waits_for_write_that_already_passed_the_busy_check() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    let app = build_router(f.state.clone());
    std::fs::write(f.root.join("retiring.png"), b"keep").unwrap();
    {
        let guard = state.conn.lock().unwrap();
        db::upsert_media_file(guard.as_ref().unwrap(), "retiring.png", "IMAGE", 4, 1).unwrap();
    }

    let (tx, rx) = std::sync::mpsc::channel();
    // Even a failed assertion must release the blocking writer before runtime teardown.
    let release = Release(Some(tx));
    let (entered, ready) = tokio::sync::oneshot::channel();
    let writing = state.clone();
    let writer = tokio::spawn(async move {
        writing
            .db_write(move |state| {
                // db_write has acquired its permit and passed the busy check. This
                // models clear_cache() delaying an already accepted deletion before
                // it acquires the DB connection.
                let _ = entered.send(());
                rx.recv().unwrap();
                let guard = state.conn.lock().unwrap();
                std::fs::remove_file(state.root_dir.join("retiring.png")).unwrap();
                guard
                    .as_ref()
                    .unwrap()
                    .execute("DELETE FROM media_files WHERE rel_path='retiring.png'", [])
                    .unwrap();
            })
            .await
            .unwrap();
    });
    ready.await.unwrap();

    let response = app
        .clone()
        .oneshot(
            f.request("/api/integrity/quick")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The connection itself is available, but inspection must wait for the older
    // writer's permit before capturing a list that includes the retiring file.
    let premature_snapshot = tokio::time::timeout(std::time::Duration::from_millis(150), async {
        loop {
            let current = job(&f, &app).await;
            if current["phase"] != "db" || current["total"] != 0 {
                break current;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(
        premature_snapshot.is_err(),
        "Inspection read the DB before the accepted write finished: {premature_snapshot:?}"
    );
    assert!(state.integrity.is_running());
    assert!(f.root.join("retiring.png").exists());

    drop(release);
    tokio::time::timeout(std::time::Duration::from_secs(5), writer)
        .await
        .unwrap()
        .unwrap();
    let finished = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let current = job(&f, &app).await;
            if !current["running"].as_bool().unwrap() && !state.integrity.is_running() {
                break current;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(finished["completed"], true);
    assert_eq!(finished["total"], 0);
    assert_eq!(finished["checked"], 0);
    assert_eq!(finished["issues"], 0);
    assert_eq!(finished["unavailable"], 0);
    assert!(finished["results"].as_array().unwrap().is_empty());
    assert!(!f.root.join("retiring.png").exists());
}

#[tokio::test]
async fn quick_check_cancel_and_close_stay_responsive_during_blocked_db_read() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let old = f.state.snapshot().unwrap();
    let held = old.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let release = Release(Some(tx));
    let (ready, started) = tokio::sync::oneshot::channel();
    let reader = std::thread::spawn(move || {
        let _conn = held.conn.lock().unwrap();
        let _ = ready.send(());
        rx.recv().unwrap();
    });
    started.await.unwrap();
    let post = |path: &str| f.request(path).method("POST").body(Body::empty()).unwrap();
    assert_eq!(
        app.clone()
            .oneshot(post("/api/integrity/quick"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let id = job(&f, &app).await["id"].as_u64().unwrap();
    assert_eq!(
        app.clone()
            .oneshot(post("/api/folders/reset_ratings"))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let cancel = |id| {
        f.request("/api/integrity/quick/cancel")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"id":id}).to_string()))
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(cancel(id + 1)).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(job(&f, &app).await["running"], true);
    assert_eq!(
        app.clone().oneshot(cancel(id)).await.unwrap().status(),
        StatusCode::OK
    );
    let stopped = job(&f, &app).await;
    assert_eq!(stopped["cancelled"], true);
    assert_eq!(stopped["running"], false);
    assert!(
        old.integrity.is_running(),
        "retain exclusion until OS request ends"
    );
    assert_eq!(
        app.clone()
            .oneshot(post("/api/integrity/quick"))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let closed = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        app.oneshot(post("/api/folders/close")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(closed.status(), StatusCode::OK);
    drop(release);
    reader.join().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while old.integrity.is_running() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(f.state.snapshot().unwrap().root_dir.as_os_str().is_empty());
    let retired = crate::integrity::status(Extension(old)).await;
    assert_eq!(serde_json::to_value(retired.0).unwrap()["cancelled"], true);
}

#[tokio::test]
async fn quick_check_requires_token_generation_unlocked_library_and_idle_work() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    let request = Request::builder()
        .uri("/api/integrity/quick")
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(
                f.request_generation("/api/integrity/quick", 0)
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let state = f.state.snapshot().unwrap();
    state.scan.lock().unwrap().running = true;
    let start = || {
        f.request("/api/integrity/quick")
            .method("POST")
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(start()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    state.scan.lock().unwrap().running = false;
    f.state
        .concealed
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        app.oneshot(start()).await.unwrap().status(),
        StatusCode::LOCKED
    );
}
