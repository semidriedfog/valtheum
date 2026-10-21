use super::*;

struct ReviewRelease(Option<std::sync::mpsc::Sender<()>>);
impl Drop for ReviewRelease {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn reconnect_finishes_while_an_archive_reader_is_stalled() {
    use std::io::Write;
    let f = Fixture::new();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(f.root.join("test.zip")).unwrap());
    zip.start_file("1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"image").unwrap();
    zip.finish().unwrap();
    let state = f.state.snapshot().unwrap();
    let held = state.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let release = ReviewRelease(Some(tx));
    let (ready, started) = tokio::sync::oneshot::channel();
    let cache_reader = tokio::task::spawn_blocking(move || {
        archive_security::with_archive(
            &held,
            "test.zip",
            None,
            move |_| {
                let _ = ready.send(());
                rx.recv().unwrap();
                Ok(())
            },
            |_| unreachable!(),
        )
        .unwrap();
    });
    started.await.unwrap();
    let response = storage_recovery::reconnect_with(
        f.state.clone(),
        state.clone(),
        std::time::Duration::from_millis(100),
        None,
        |path, password| encryption::open(&path, password.as_deref().map(|p| p.as_str()), false),
    )
    .await;
    let reports_success = response.is_ok();
    let still_busy = state.recovery.is_running();
    let write_status = build_router(f.state.clone())
        .oneshot(
            f.request("/api/folders/reset_ratings")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status();
    drop(release);
    cache_reader.await.unwrap();
    while state.recovery.is_running() {
        tokio::task::yield_now().await;
    }
    println!("REVIEW cache: success={reports_success}, still_busy={still_busy}, mutation_status={write_status}");
    assert!(reports_success && !still_busy);
    assert_eq!(write_status, StatusCode::OK);
}

#[tokio::test]
async fn selecting_current_folder_preserves_connection_and_generation() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    *state.scan.lock().unwrap() = ScanStatus {
        scanned_count: 42,
        ..Default::default()
    };
    let library = open_library(f.root.join("."), &state).await.unwrap();
    assert!(Arc::ptr_eq(&library, &state.library));
    let response = install_library_at(
        &f.state,
        state.generation,
        library,
        AppConfig::default(),
        &f.root.join("test-config.json"),
    )
    .unwrap()
    .0;
    assert_eq!(response.generation, state.generation);
    assert_eq!(response.scan.scanned_count, 42);
    assert!(!response.scan.running);
    assert!(!state
        .archives
        .revoked
        .load(std::sync::atomic::Ordering::Acquire));
    assert!(state.conn.lock().unwrap().is_some());
    assert!(
        Library::open(f.root.clone()).is_err(),
        "other owners must still be excluded"
    );
}

#[test]
fn blocked_thumbnail_requests_leave_http_workers_available_to_close() {
    let f = Fixture::new();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let state = f.state.snapshot().unwrap();
    let held = state.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let release = ReviewRelease(Some(release));
    let (locked, ready) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = held.conn.lock().unwrap();
        locked.send(()).unwrap();
        wait.recv().unwrap();
    });
    ready.recv().unwrap();
    let (started, ready) = std::sync::mpsc::channel();
    let mut readers = vec![];
    for _ in 0..2 {
        let state = state.clone();
        let started = started.clone();
        readers.push(rt.spawn(async move {
            started.send(()).unwrap();
            get_thumbnail(Extension(state), Path(1)).await
        }));
    }
    ready.recv().unwrap();
    ready.recv().unwrap();
    let app = f.state.clone();
    let close_state = state.clone();
    let (closed, closing) = std::sync::mpsc::channel();
    let closer = rt.spawn(async move {
        let result = security_api::close(State(app), Extension(close_state)).await;
        closed.send(result.is_ok()).unwrap();
    });
    let closed_before_release = closing.recv_timeout(std::time::Duration::from_secs(2)).ok();
    drop(release);
    holder.join().unwrap();
    rt.block_on(async {
        for reader in readers {
            let _ = reader.await;
        }
        closer.await.unwrap();
    });
    assert_eq!(
        closed_before_release,
        Some(true),
        "closing must complete before the NAS read is released"
    );
}

#[tokio::test]
async fn reselecting_encrypted_folder_keeps_password_media_key_and_rejects_stale_install() {
    let f = Fixture::new();
    let response=build_router(f.state.clone()).oneshot(f.request("/api/security/encrypt").method("POST").header("content-type","application/json").body(Body::from(r#"{"password":"same folder test password","confirmation":"same folder test password"}"#)).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let state = f.state.snapshot().unwrap();
    *state.archives.key.lock().unwrap() = Some(zeroize::Zeroizing::new([7u8; 32]));
    let candidate = open_library(f.root.clone(), &state).await.unwrap();
    let response = install_library_at(
        &f.state,
        state.generation,
        candidate.clone(),
        AppConfig::default(),
        &f.root.join("same.json"),
    )
    .unwrap()
    .0;
    assert_eq!(response.generation, state.generation);
    assert!(!response.security.locked);
    assert_eq!(
        state.archives.key.lock().unwrap().as_deref().unwrap(),
        &[7u8; 32]
    );
    assert!(!state
        .archives
        .revoked
        .load(std::sync::atomic::Ordering::Acquire));
    f.state.current.lock().unwrap().generation += 1;
    assert_eq!(
        install_library_at(
            &f.state,
            state.generation,
            candidate,
            AppConfig::default(),
            &f.root.join("stale.json")
        )
        .err(),
        Some(StatusCode::CONFLICT)
    );
    assert!(!f.root.join("stale.json").exists());
}

#[tokio::test]
async fn queued_write_rechecks_busy_state_before_touching_the_db() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    let slot = state.db_slots.clone().acquire_owned().await.unwrap();
    let write_state = state.clone();
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = ran.clone();
    let queued = tokio::spawn(async move {
        write_state
            .db_write(move |_| {
                flag.store(true, std::sync::atomic::Ordering::Release);
            })
            .await
    });
    tokio::task::yield_now().await;
    state
        .archives
        .busy
        .store(true, std::sync::atomic::Ordering::Release);
    drop(slot);
    let result = queued.await.unwrap();
    state
        .archives
        .busy
        .store(false, std::sync::atomic::Ordering::Release);
    assert_eq!(result, Err(StatusCode::CONFLICT));
    assert!(!ran.load(std::sync::atomic::Ordering::Acquire));
}
