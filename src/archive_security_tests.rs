use super::*;
use std::io::{Read, Write};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_receipts_deduplicate_running_and_finished_jobs_and_reject_changed_payloads(
) -> Result<()> {
    let f = Fixture::new()?;
    let app = AppState::new(f.library.clone())?;
    let state = app.snapshot().unwrap();
    let request_id = "0123456789abcdef0123456789abcdef";
    let mut headers = HeaderMap::new();
    headers.insert("X-Vault-Request", request_id.parse().unwrap());
    let request = request_identity(&headers, ("test", 1)).unwrap();
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (release, blocked) = std::sync::mpsc::channel();
    let count = runs.clone();
    let first = start_request(&app, &state, Job::default(), request.clone(), move |_| {
        count.fetch_add(1, Ordering::AcqRel);
        blocked.recv().unwrap();
        Ok(())
    })
    .await
    .unwrap();
    let duplicate = start_request(&app, &state, Job::default(), request.clone(), |_| {
        panic!("duplicate worker")
    })
    .await
    .unwrap();
    assert_eq!(first.0["job_id"], duplicate.0["job_id"]);
    let pending = progress_status(
        Extension(state.clone()),
        Query(JobQuery {
            request_id: Some(request_id.into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(pending.0["job"]["running"], true);
    // The HTTP write barrier stays intact; read-only recovery works while busy.
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;
    let router = crate::build_router(app.clone());
    let request_builder = |uri: &str| {
        Request::builder()
            .uri(uri)
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", app.token.as_str())
            .header("x-vault-library", state.generation.to_string())
    };
    let blocked = router
        .clone()
        .oneshot(
            request_builder("/api/files/1/name")
                .method("POST")
                .header("content-type", "application/json")
                .header("X-Vault-Request", request_id)
                .body(Body::from(r#"{"name":"new.zip","restore_name":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    let recovered = router
        .oneshot(
            request_builder(&format!(
                "/api/archive-security/progress?request_id={request_id}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(recovered.status(), StatusCode::OK);
    let recovered: serde_json::Value =
        serde_json::from_slice(&to_bytes(recovered.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(recovered["job"]["job_id"], first.0["job_id"]);
    assert!(start_request(
        &app,
        &state,
        Job::default(),
        request_identity(&headers, ("test", 2)).unwrap(),
        |_| panic!("changed payload")
    )
    .await
    .is_err());
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.archives.busy.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(runs.load(Ordering::Acquire), 1);
    // The receipt outlives the bounded general progress history.
    for _ in 0..130 {
        state.archives.begin(Job::default());
        state.archives.job.lock().unwrap().running = false;
    }
    let completed = progress_status(
        Extension(state.clone()),
        Query(JobQuery {
            request_id: Some(request_id.into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(completed.0["job"]["completed"], true);
    assert!(
        state
            .archives
            .lookup(first.0["job_id"].as_str().unwrap())
            .unwrap()
            .completed
    );
    let replay = start_request(&app, &state, Job::default(), request, |_| {
        panic!("completed replay")
    })
    .await
    .unwrap();
    assert_eq!(first.0["job_id"], replay.0["job_id"]);
    assert!(request_identity(
        &HeaderMap::from_iter([(
            "X-Vault-Request".parse().unwrap(),
            "invalid".parse().unwrap()
        )]),
        1
    )
    .is_err());
    let other = Library::unconfigured();
    assert!(other.archives.lookup_request(request_id).is_none());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn losing_the_admission_caller_still_launches_the_registered_worker() -> Result<()> {
    let f = Fixture::new()?;
    let app = AppState::new(f.library.clone())?;
    let state = app.snapshot().unwrap();
    let request_id = "abcdef0123456789abcdef0123456789".to_owned();
    let (finished, done) = tokio::sync::oneshot::channel();
    // Keep admission queued while the HTTP-side caller disconnects.
    let held = app.current.lock().unwrap();
    let caller = tokio::spawn({
        let app = app.clone();
        let state = state.clone();
        let id = request_id.clone();
        async move {
            start_request(
                &app,
                &state,
                Job::default(),
                Some((id, "fingerprint".into())),
                move |_| {
                    let _ = finished.send(());
                    Ok(())
                },
            )
            .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    caller.abort();
    let _ = caller.await;
    drop(held);
    tokio::time::timeout(std::time::Duration::from_secs(2), done)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.archives.busy.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        state
            .archives
            .lookup_request(&request_id)
            .unwrap()
            .completed
    );
    Ok(())
}

#[tokio::test]
async fn media_edit_http_receipts_replay_completed_edits_without_rewriting_files() -> Result<()> {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    for (index, (uri, method, body)) in [
        (
            format!("/api/files/{id}/name"),
            "POST",
            r#"{"name":"alias.zip","restore_name":false}"#,
        ),
        (
            format!("/api/files/{id}/name"),
            "POST",
            r#"{"name":"restored.zip","restore_name":true}"#,
        ),
        (
            format!("/api/archive-security/{id}/page"),
            "DELETE",
            r#"{"entries":["7.png"]}"#,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let receipt = format!("{index:032x}");
        let request = || {
            Request::builder()
                .uri(&uri)
                .method(method)
                .header("host", "127.0.0.1:8080")
                .header("x-vault-token", app.token.as_str())
                .header("x-vault-library", "1")
                .header("X-Vault-Request", &receipt)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap()
        };
        let first = router.clone().oneshot(request()).await?;
        assert_eq!(first.status(), StatusCode::OK);
        let first: serde_json::Value =
            serde_json::from_slice(&to_bytes(first.into_body(), 8192).await?)?;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while f.library.archives.busy.load(Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await?;
        let job = f.library.archives.lookup_request(&receipt).unwrap();
        assert!(job.completed, "{:?}", job.error);
        let path = ready(&f.library, id)?.1;
        let replay = router.clone().oneshot(request()).await?;
        assert_eq!(replay.status(), StatusCode::OK);
        let replay: serde_json::Value =
            serde_json::from_slice(&to_bytes(replay.into_body(), 8192).await?)?;
        assert_eq!(first["job_id"], replay["job_id"]);
        assert_eq!(ready(&f.library, id)?.1, path);
        assert!(!f.library.archives.busy.load(Ordering::Acquire));
    }
    assert_eq!(rename::restoration_name(&f.library, id)?, "restored.zip");
    assert_eq!(image(&f.library, "series_01.zip", "10.png")?, b"page ten");
    assert!(image(&f.library, "series_01.zip", "7.png").is_err());
    Ok(())
}

#[tokio::test]
async fn job_results_and_cancellation_remain_bound_to_the_original_operation() {
    let library = Library::unconfigured();
    let app = AppState::new(library.clone()).unwrap();
    let state = app.snapshot().unwrap();
    let first = library.archives.begin(Job {
        phase: "encrypt A".into(),
        ..Default::default()
    });
    {
        let mut job = library.archives.job.lock().unwrap();
        job.running = false;
        job.error = Some("disk unavailable".into());
    }
    let second = library.archives.begin(Job {
        phase: "save B".into(),
        ..Default::default()
    });
    assert_ne!(first, second);
    let result = progress_status(
        Extension(state.clone()),
        Query(JobQuery {
            job_id: Some(first.clone()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(result.0["job"]["error"], "disk unavailable");
    assert_eq!(result.0["job"]["completed"], false);
    let stale = cancel(
        Extension(state.clone()),
        Query(JobQuery {
            job_id: Some(first),
            ..Default::default()
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert!(!library.archives.cancel.load(Ordering::Acquire));
    assert!(cancel(Extension(state.clone()), Query(JobQuery::default()))
        .await
        .is_err());
    cancel(
        Extension(state),
        Query(JobQuery {
            job_id: Some(second),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert!(library.archives.cancel.load(Ordering::Acquire));
}

#[test]
fn completed_job_history_is_bounded_and_unknown_ids_never_return_current_work() {
    let runtime = Runtime::default();
    let first = runtime.begin(Job::default());
    for _ in 0..130 {
        runtime.job.lock().unwrap().running = false;
        runtime.begin(Job::default());
    }
    assert_eq!(runtime.history.lock().unwrap().len(), 128);
    assert!(runtime.lookup(&first).is_none());
    assert!(runtime.lookup("unrelated").is_none());
}
pub(super) struct Fixture {
    pub(super) root: std::path::PathBuf,
    pub(super) library: Arc<Library>,
}
impl Fixture {
    pub(super) fn new() -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "vta-library-{}",
            crypto::hex(&crypto::random::<16>()?)
        ));
        fs::create_dir(&root)?;
        let source = root.join("series_01.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&source)?);
        let opt = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("7.png", b"page seven".as_slice()),
            ("10.png", b"page ten".as_slice()),
        ] {
            zip.start_file(name, opt)?;
            zip.write_all(data)?;
        }
        zip.finish()?;
        let library = Library::open(root.clone())?;
        {
            let mut guard = library.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            crate::scanner::scan_directory(&root, conn)?;
            conn.execute_batch("UPDATE media_files SET rating=5,score=1700,watch_count=9; INSERT INTO tags(id,name) VALUES(1,'keep'); INSERT INTO file_tags(file_id,tag_id) SELECT id,1 FROM media_files; INSERT INTO media_bookmarks(file_id,position,thumbnail_data) SELECT id,1,X'01' FROM media_files;")?;
            crate::encryption::convert(conn, &root, "fixture password 123")?;
            *guard = Some(crate::encryption::open(
                &root.join(crate::encryption::SECURE_DB),
                Some("fixture password 123"),
                false,
            )?);
            let mut s = library.security.lock().unwrap();
            s.encrypted = true;
            s.password = Some(zeroize::Zeroizing::new("fixture password 123".into()));
        }
        Ok(Self { root, library })
    }
    pub(super) fn id(&self) -> i64 {
        self.library
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT id FROM media_files WHERE rel_path='series_01.zip'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.library.archives.retire();
        self.library.conn.lock().unwrap().take();
        self.library = Library::unconfigured();
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("vta-library-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn conversion_view_scan_backup_cleanup_and_db_independent_restore() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    let original = fs::read(f.root.join("series_01.zip"))?;
    convert(&f.library, id)?;
    assert!(f.root.join(crypto::KEY_PATH).exists());
    assert_eq!(fs::read(f.root.join("series_01.zip"))?, original);
    assert_eq!(list(&f.library, "series_01.zip")?, vec!["10.png", "7.png"]);
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, c)?;
        assert_eq!(
            c.query_row(
                "SELECT rating*100+watch_count FROM media_files WHERE id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )?,
            509
        );
        assert_eq!(crate::db::get_tags_for_file(c, id)?.len(), 1);
        assert_eq!(crate::db::list_bookmarks(c, id)?.len(), 1);
        crate::db::move_to_trash(c, id)?;
        assert!(crate::db::permanently_delete_file(c, &f.root, id).is_err());
        crate::db::restore_from_trash(c, id)?;
    }
    let backup = f.root.join("backup.vtbackup");
    {
        let g = f.library.conn.lock().unwrap();
        assert!(crate::persistence::backup_bundle(
            g.as_ref().unwrap(),
            &f.root,
            &f.root.join(crate::encryption::SECURE_DB),
            &backup,
            Some("fixture password 123")
        )?);
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(&backup)?)?;
    let mut key = Vec::new();
    zip.by_name(crypto::KEY_PATH)?.read_to_end(&mut key)?;
    assert_eq!(key, fs::read(f.root.join(crypto::KEY_PATH))?);
    let mut db = Vec::new();
    zip.by_name(crate::encryption::SECURE_DB)?
        .read_to_end(&mut db)?;
    let copy = f.root.join("copied.db");
    fs::write(&copy, db)?;
    let c = crate::encryption::open(&copy, Some("fixture password 123"), false)?;
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM archive_store WHERE state='ready'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    drop(c);
    drop(zip);
    cleanup(&f.library, id)?;
    assert!(!f.root.join("series_01.zip").exists());
    assert_eq!(image(&f.library, "series_01.zip", "10.png")?, b"page ten");
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, c)?;
        assert_eq!(f.id_no_lock(c), id);
        c.execute(
            "UPDATE archive_store SET state='pending' WHERE file_id=?1",
            [id],
        )?;
    }
    // Simulate a crash after file publication and before the DB commit.
    convert(&f.library, id)?;
    let (_, rel, _) = ready(&f.library, id)?;
    let master = crypto::read_key(&key, "fixture password 123")?;
    let out = f.root.join("restored.zip");
    crypto::restore(&f.root.join(&rel), &master, &out)?;
    assert_eq!(fs::read(out)?, original);
    // A missing required key must not silently produce a DB-only backup or regenerate a key.
    let saved = fs::read(&backup)?;
    fs::rename(f.root.join(crypto::KEY_PATH), f.root.join("key.saved"))?;
    {
        let g = f.library.conn.lock().unwrap();
        assert!(crate::persistence::backup_bundle(
            g.as_ref().unwrap(),
            &f.root,
            &f.root.join(crate::encryption::SECURE_DB),
            &backup,
            Some("fixture password 123")
        )
        .is_err());
    }
    assert_eq!(fs::read(&backup)?, saved);
    assert!(initialize(&f.library).is_err());
    assert!(!f.root.join(crypto::KEY_PATH).exists());
    fs::rename(f.root.join("key.saved"), f.root.join(crypto::KEY_PATH))?;
    f.library.archives.clear_cache();
    {
        let g = f.library.conn.lock().unwrap();
        crate::db::move_to_trash(g.as_ref().unwrap(), id)?;
        crate::db::permanently_delete_file(g.as_ref().unwrap(), &f.root, id)?;
    }
    assert!(!f.root.join(rel).exists());
    Ok(())
}
impl Fixture {
    fn id_no_lock(&self, c: &Connection) -> i64 {
        c.query_row(
            "SELECT id FROM media_files WHERE rel_path='series_01.zip'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }
}
#[test]
fn changed_original_cancelled_work_and_locked_reader_are_safe() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    f.library.archives.cancel.store(true, Ordering::Release);
    assert!(convert(&f.library, id).is_err());
    assert!(f.root.join("series_01.zip").exists());
    f.library.archives.cancel.store(false, Ordering::Release);
    convert(&f.library, id)?;
    fs::write(f.root.join("series_01.zip"), b"changed")?;
    assert!(cleanup(&f.library, id).is_err());
    assert_eq!(fs::read(f.root.join("series_01.zip"))?, b"changed");
    let (_, rel, key) = ready(&f.library, id)?;
    let mut reader = crypto::Reader::open(&f.root.join(rel), &key)?
        .with_revocation(f.library.archives.revoked.clone());
    f.library.archives.retire();
    assert!(reader.read(&mut [0; 1]).is_err());
    assert!(image(&f.library, "series_01.zip", "7.png").is_err());
    Ok(())
}
#[test]
fn encrypted_tar_zstd_pages_and_original_bytes_survive() -> Result<()> {
    let f = Fixture::new()?;
    let path = f.root.join("book.tar.zst");
    let encoder = zstd::stream::write::Encoder::new(fs::File::create(&path)?, 1)?;
    let mut tar = tar::Builder::new(encoder);
    let mut h = tar::Header::new_gnu();
    h.set_size(8);
    h.set_mode(0o644);
    h.set_cksum();
    tar.append_data(&mut h, "nested/7.png", b"tar page".as_slice())?;
    tar.into_inner()?.finish()?;
    let id = {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, c)?;
        c.query_row(
            "SELECT id FROM media_files WHERE rel_path='book.tar.zst'",
            [],
            |r| r.get(0),
        )?
    };
    convert(&f.library, id)?;
    rename::rewrite(&f.library, id, "renamed.tar.zst")?;
    assert_eq!(rename::restoration_name(&f.library, id)?, "renamed.tar.zst");
    cleanup(&f.library, id)?;
    assert_eq!(list(&f.library, "book.tar.zst")?, vec!["nested/7.png"]);
    assert_eq!(
        image(&f.library, "book.tar.zst", "nested/7.png")?,
        b"tar page"
    );
    Ok(())
}

#[test]
fn display_name_preserves_ciphertext_view_cleanup_and_independent_restore() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    let (_, rel, key) = ready(&f.library, id)?;
    let encrypted = f.root.join(rel);
    let before = fs::read(&encrypted)?;
    {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        crate::media_name::set_name(conn, id, "renamed.zip")?;
        crate::scanner::scan_directory(&f.root, conn)?;
        let files = crate::db::get_media_files(conn, None, None)?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].display_name.as_deref(), Some("renamed.zip"));
        assert_eq!(
            (files[0].rating, files[0].score, files[0].watch_count),
            (5, 1700, 9)
        );
    }
    assert_eq!(fs::read(&encrypted)?, before);
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    let restored = f.root.join("restored.zip");
    let meta = crypto::restore(&encrypted, &key, &restored)?;
    assert_eq!(meta.name, "series_01.zip");
    assert_eq!(
        fs::read(&restored)?,
        fs::read(f.root.join("series_01.zip"))?
    );
    cleanup(&f.library, id)?;
    assert!(!f.root.join("series_01.zip").exists());
    assert_eq!(image(&f.library, "series_01.zip", "10.png")?, b"page ten");
    Ok(())
}

#[test]
fn restore_name_rewrite_keeps_original_and_metadata_and_supports_repeated_renames() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    let original = fs::read(f.root.join("series_01.zip"))?;
    let old_rel = ready(&f.library, id)?.1;
    // Populate the old archive cache before changing its mapping.
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    rename::rewrite(&f.library, id, "새 복원 이름.zip")?;
    assert_eq!(fs::read(f.root.join("series_01.zip"))?, original);
    let (_, rel, key) = ready(&f.library, id)?;
    assert_ne!(rel, old_rel);
    assert_eq!(
        crypto::verify(&f.root.join(&rel), &key, |_| Ok(()))?.name,
        "새 복원 이름.zip"
    );
    assert_eq!(image(&f.library, "series_01.zip", "10.png")?, b"page ten");
    {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, conn)?;
        let files = crate::db::get_media_files(conn, None, None)?;
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.restore_name.as_deref(), Some("새 복원 이름.zip"));
        assert_eq!(
            (file.id, file.rating, file.score, file.watch_count),
            (id, 5, 1700, 9)
        );
        assert_eq!(file.tags.len(), 1);
        assert_eq!(crate::db::list_bookmarks(conn, id)?.len(), 1);
    }
    let restored = f.root.join("restored.zip");
    assert_eq!(
        crypto::restore(&f.root.join(&rel), &key, &restored)?.name,
        "새 복원 이름.zip"
    );
    assert_eq!(fs::read(restored)?, original);
    cleanup(&f.library, id)?;
    assert!(!f.root.join("series_01.zip").exists());
    rename::rewrite(&f.library, id, "second.zip")?;
    assert_eq!(rename::restoration_name(&f.library, id)?, "second.zip");
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    Ok(())
}

#[test]
fn restore_name_failure_and_cancel_leave_old_mapping_and_ciphertext_intact() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    let old_rel = ready(&f.library, id)?.1;
    let bytes = fs::read(f.root.join(&old_rel))?;
    f.library.archives.cancel.store(true, Ordering::Release);
    assert!(rename::rewrite(&f.library, id, "cancelled.zip").is_err());
    f.library.archives.cancel.store(false, Ordering::Release);
    {
        let guard = f.library.conn.lock().unwrap();
        guard.as_ref().unwrap().execute_batch("CREATE TRIGGER fail_rename BEFORE UPDATE OF encrypted_path ON archive_store BEGIN SELECT RAISE(FAIL,'injected DB failure'); END;")?;
    }
    assert!(rename::rewrite(&f.library, id, "failed.zip").is_err());
    assert_eq!(ready(&f.library, id)?.1, old_rel);
    assert_eq!(fs::read(f.root.join(&old_rel))?, bytes);
    assert_eq!(rename::restoration_name(&f.library, id)?, "series_01.zip");
    assert_eq!(fs::read_dir(f.root.join(".valtheum/archives"))?.count(), 1);
    Ok(())
}

#[test]
fn interrupted_rewrite_collection_removes_only_unreferenced_staging_files() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    let active = ready(&f.library, id)?.1;
    let staged = format!(
        ".valtheum/archives/{}.vta",
        crypto::hex(&crypto::random::<16>()?)
    );
    fs::write(f.root.join(&staged), b"interrupted encrypted output")?;
    let guard = f.library.conn.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    for path in [&active, &staged] {
        conn.execute("INSERT INTO archive_gc VALUES(?1)", [path])?;
    }
    rename::collect_unused(&f.root, conn)?;
    assert!(f.root.join(&active).is_file());
    assert!(!f.root.join(&staged).exists());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM archive_gc", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn restore_name_api_runs_verified_background_job() -> Result<()> {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let request = Request::builder()
        .uri(format!("/api/files/{id}/name"))
        .method("POST")
        .header("host", "127.0.0.1:8080")
        .header("x-vault-token", app.token.as_str())
        .header("x-vault-library", "1")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"from-api.zip","restore_name":true}"#))?;
    let response = router.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    for _ in 0..1000 {
        if !f.library.archives.busy.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let job = f.library.archives.job.lock().unwrap().clone();
    assert!(!job.running);
    assert!(job.completed, "{:?}", job.error);
    assert_eq!(rename::restoration_name(&f.library, id)?, "from-api.zip");
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    Ok(())
}

#[test]
fn reconnect_cache_epoch_rejects_a_reader_that_finishes_after_invalidation() -> Result<()> {
    let f = Fixture::new()?;
    convert(&f.library, f.id())?;
    image(&f.library, "series_01.zip", "7.png")?;
    let held = f.library.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let (started, ready) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        with_archive(
            &held,
            "series_01.zip",
            None,
            move |_| {
                started.send(()).unwrap();
                wait.recv().unwrap();
                Ok(())
            },
            |_| unreachable!(),
        )
    });
    ready.recv().unwrap();
    // Must not wait for the reader or return its old handle to the new cache.
    f.library.archives.clear_cache();
    let result = image(&f.library, "series_01.zip", "10.png");
    release.send(()).unwrap();
    reader.join().unwrap()?;
    assert_eq!(result?, b"page ten");
    assert_eq!(f.library.archives.cache.lock().unwrap().entries.len(), 1);
    Ok(())
}
