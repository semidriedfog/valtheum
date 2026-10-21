use super::*;
use std::{io::Read, sync::atomic::Ordering, time::Duration};

#[test]
fn backup_after_destination_selection_keeps_database_contents() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    {
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        db::upsert_media_file(conn, "keep.png", "IMAGE", 4, 1).unwrap();
        conn.execute("UPDATE media_files SET rating=5,score=1600", [])
            .unwrap();
    }
    let destination = f.root.join("normal.vtbackup");
    assert!(!backup_library_at(&f.state, &state, &destination).unwrap());
    let mut backup = zip::ZipArchive::new(std::fs::File::open(destination).unwrap()).unwrap();
    let mut manifest = String::new();
    backup
        .by_name("backup.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["database"], encryption::PLAIN_DB);
    assert_eq!(manifest["key_included"], false);
    assert_eq!(manifest["media_included"], false);
    let restored = f.root.join("restored-check.db");
    std::io::copy(
        &mut backup.by_name(encryption::PLAIN_DB).unwrap(),
        &mut std::fs::File::create(&restored).unwrap(),
    )
    .unwrap();
    let conn = encryption::open(&restored, None, false).unwrap();
    assert_eq!(
        conn.query_row("SELECT rel_path,rating,score FROM media_files", [], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .unwrap(),
        ("keep.png".into(), 5, 1600)
    );
}

struct Release(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_dialog_completion_rejects_running_inspection_without_blocking_close() {
    let f = Fixture::new();
    let app = build_router(f.state.clone());
    // Capture the same request state that the backup save dialog already owns.
    let before_dialog = f.state.snapshot().unwrap();
    let held = before_dialog.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let release = Release(Some(tx));
    let (entered, ready) = tokio::sync::oneshot::channel();
    let reader = std::thread::spawn(move || {
        let _guard = held.conn.lock().unwrap();
        let _ = entered.send(());
        let _ = rx.recv();
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
    assert!(before_dialog.integrity.is_running());

    let destination = f.root.join("blocked.vtbackup");
    let work_destination = destination.clone();
    let work_app = f.state.clone();
    let work_state = before_dialog.clone();
    let mut backup = tokio::task::spawn_blocking(move || {
        backup_library_at(&work_app, &work_state, &work_destination)
    });
    let outcome = tokio::time::timeout(Duration::from_secs(2), &mut backup).await;
    // On a regression, release the simulated NAS stall before failing, so the
    // pending blocking worker never strands the test runtime during teardown.
    let error = match outcome {
        Ok(result) => result.unwrap().unwrap_err(),
        Err(_) => {
            drop(release);
            reader.join().unwrap();
            let _ = backup.await;
            panic!("Backup waited on the inspection's DB connection instead of rejecting it");
        }
    };
    assert!(error.to_string().contains(crate::integrity::BUSY));
    assert!(!destination.exists());

    let request = f
        .request("/api/folders/close")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let mut closing = tokio::spawn(async move { app.oneshot(request).await.unwrap() });
    let closed = tokio::time::timeout(Duration::from_secs(2), &mut closing).await;
    drop(release);
    reader.join().unwrap();
    let response = match closed {
        Ok(result) => result.unwrap(),
        Err(_) => {
            let _ = closing.await;
            panic!("Rejected backup prevented closing the stalled library");
        }
    };
    assert_eq!(response.status(), StatusCode::OK);
    assert!(f.state.snapshot().unwrap().root_dir.as_os_str().is_empty());
    tokio::time::timeout(Duration::from_secs(2), async {
        while before_dialog.integrity.is_running() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn backup_dialog_completion_rechecks_scan_archive_work_and_generation() {
    let f = Fixture::new();
    let state = f.state.snapshot().unwrap();
    let destination = f.root.join("busy.vtbackup");
    state.scan.lock().unwrap().running = true;
    assert!(backup_library_at(&f.state, &state, &destination).is_err());
    state.scan.lock().unwrap().running = false;
    state.archives.busy.store(true, Ordering::Release);
    assert!(backup_library_at(&f.state, &state, &destination).is_err());
    state.archives.busy.store(false, Ordering::Release);
    let mut stale = state.clone();
    stale.generation += 1;
    assert!(backup_library_at(&f.state, &stale, &destination).is_err());
    assert!(!destination.exists());
}
