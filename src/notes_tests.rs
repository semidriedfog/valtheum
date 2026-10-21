use super::tests::Fixture;
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request},
};
use tower::ServiceExt;
fn request(app: &AppState, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .uri(uri)
        .header("host", "127.0.0.1:8080")
        .header("x-vault-token", app.token.as_str())
        .header("x-vault-library", "1")
}
#[tokio::test]
async fn standalone_note_create_edit_scan_trash_restore_and_authentication() -> Result<()> {
    let f = Fixture::new()?;
    let input = notes::Input {
        request_id: None,
        revision: None,
        title: "private / title".into(),
        body: "  secret\r\n日本語 한글 🔐\t  ".into(),
    };
    let id = notes::save(&f.library, None, &input)?;
    let (name, old_rel, key) = ready(&f.library, id)?;
    assert!(old_rel.ends_with(".vtn"));
    assert!(!f.root.join(&name).exists());
    let mut r = crypto::Reader::open(&f.root.join(&old_rel), &key)?;
    assert!(r.standalone_note);
    let revision = r.note_index()?.revision.clone();
    let note_id = r.note_index()?.notes[0].id.clone();
    assert_eq!(r.read_note(&note_id)?.body, input.body);
    drop(r);
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        c.execute("UPDATE media_files SET rating=4 WHERE id=?1", [id])?;
        c.execute("INSERT INTO file_tags(file_id,tag_id) VALUES(?1,1)", [id])?;
        crate::scanner::scan_directory(&f.root, c)?;
        let row: (String, i64, bool) = c.query_row(
            "SELECT file_type,rating,is_missing FROM media_files WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        assert_eq!(row, ("NOTE".into(), 4, false));
        assert!(crate::db::get_arena_pair(c)?.is_none());
        assert!(crate::db::get_arena_opponent(c, id)?.is_none());
    }
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let res = router
        .clone()
        .oneshot(request(&app, "/api/files").body(Body::empty())?)
        .await?;
    let data = to_bytes(res.into_body(), 1024 * 1024).await?;
    assert!(String::from_utf8_lossy(&data).contains("private / title"));
    assert!(!String::from_utf8_lossy(&data).contains("secret"));
    let res = router
        .clone()
        .oneshot(request(&app, &format!("/api/notes/{id}")).body(Body::empty())?)
        .await?;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(res.into_body(), 1024 * 1024).await?)?;
    assert_eq!(data["body"], input.body);
    let edit = notes::Input {
        request_id: None,
        revision: Some(revision),
        title: "updated".into(),
        body: "replacement".into(),
    };
    notes::save(&f.library, Some(id), &edit)?;
    let (_, rel, _) = ready(&f.library, id)?;
    assert_ne!(rel, old_rel);
    assert!(notes::save(&f.library, Some(id), &edit).is_err());
    assert_eq!(ready(&f.library, id)?.1, rel);
    let r = crypto::Reader::open(&f.root.join(&rel), &key)?;
    let revision = r.note_index()?.revision.clone();
    drop(r);
    f.library.archives.cancel.store(true, Ordering::Release);
    assert!(notes::save(
        &f.library,
        Some(id),
        &notes::Input {
            request_id: None,
            revision: Some(revision),
            title: "cancel".into(),
            body: "cancel".into()
        }
    )
    .is_err());
    f.library.archives.cancel.store(false, Ordering::Release);
    assert_eq!(ready(&f.library, id)?.1, rel);
    crypto::restore(&f.root.join(&rel), &key, &f.root.join("export.json"))?;
    let restored: Vec<crypto::image::Note> =
        serde_json::from_slice(&fs::read(f.root.join("export.json"))?)?;
    assert_eq!(restored[0].body, "replacement");
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        assert_eq!(
            c.query_row("SELECT rating FROM media_files WHERE id=?1", [id], |r| r
                .get::<_, i64>(0))?,
            4
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM file_tags WHERE file_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        crate::db::move_to_trash(c, id)?;
    }
    assert!(notes::save(&f.library, Some(id), &edit).is_err());
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        crate::db::restore_from_trash(c, id)?;
    }
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/notes/{id}"))
                .header("host", "127.0.0.1:8080")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = router
        .clone()
        .oneshot(request(&app, &format!("/api/image-notes/{id}")).body(Body::empty())?)
        .await?;
    assert_ne!(res.status(), StatusCode::OK);
    app.concealed.store(true, Ordering::Release);
    let res = router
        .oneshot(request(&app, &format!("/api/notes/{id}")).body(Body::empty())?)
        .await?;
    assert_eq!(res.status(), StatusCode::LOCKED);
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        crate::db::move_to_trash(c, id)?;
        crate::db::permanently_delete_file(c, &f.root, id)?;
    }
    assert!(!f.root.join(rel).exists());
    Ok(())
}
#[tokio::test]
async fn standalone_note_job_creation_and_photo_write_rejection() -> Result<()> {
    let f = Fixture::new()?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let res = router
        .clone()
        .oneshot(
            request(&app, "/api/notes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"request_id":"0123456789abcdef0123456789abcdef","title":"job","body":"exact body"}"#))?,
        )
        .await?;
    assert_eq!(res.status(), StatusCode::OK);
    for _ in 0..1500 {
        if !f.library.archives.busy.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let job = f.library.archives.job.lock().unwrap().clone();
    assert!(job.completed, "{:?}", job.error);
    let id = job.result_id.unwrap();
    assert!(notes::save(
        &f.library,
        Some(f.id()),
        &notes::Input {
            request_id: None,
            revision: None,
            title: "no photo".into(),
            body: String::new()
        }
    )
    .is_err());
    assert!(ready(&f.library, id)?.1.ends_with(".vtn"));
    f.library.security.lock().unwrap().encrypted = false;
    let res = router
        .oneshot(
            request(&app, "/api/notes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"denied","body":"secret"}"#))?,
        )
        .await?;
    assert!(!res.status().is_success());
    Ok(())
}

#[tokio::test]
async fn note_save_receipts_survive_reopen_and_reject_changed_retries() -> Result<()> {
    let f = Fixture::new()?;
    let mut input = notes::Input {
        request_id: Some("11111111111111111111111111111111".into()),
        revision: None,
        title: "receipt".into(),
        body: "same body".into(),
    };
    let id = notes::save(&f.library, None, &input)?;
    let (_, original_rel, key) = ready(&f.library, id)?;
    assert_eq!(notes::save(&f.library, None, &input)?, id);
    assert_eq!(ready(&f.library, id)?.1, original_rel);
    input.body = "changed payload".into();
    assert!(notes::save(&f.library, None, &input).is_err());
    input.body = "same body".into();
    let reader = crypto::Reader::open(&f.root.join(&original_rel), &key)?;
    let edit = notes::Input {
        request_id: Some("22222222222222222222222222222222".into()),
        revision: Some(reader.note_index()?.revision.clone()),
        title: "edited".into(),
        body: "edited body".into(),
    };
    drop(reader);
    notes::save(&f.library, Some(id), &edit)?;
    let (_, edited_rel, _) = ready(&f.library, id)?;
    assert_eq!(notes::save(&f.library, Some(id), &edit)?, id);
    assert_eq!(ready(&f.library, id)?.1, edited_rel);
    {
        let mut g = f.library.conn.lock().unwrap();
        g.take();
        *g = Some(crate::encryption::open(
            &f.root.join(crate::encryption::SECURE_DB),
            Some("fixture password 123"),
            false,
        )?);
    }
    assert_eq!(notes::save(&f.library, None, &input)?, id);
    *f.library.archives.job.lock().unwrap() = Job::default();
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let uri = format!("/api/note-saves/{}", input.request_id.as_ref().unwrap());
    let response = router
        .clone()
        .oneshot(request(&app, &uri).body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(data["job"]["result_id"], id);
    assert_eq!(data["job"]["completed"], true);
    let g = f.library.conn.lock().unwrap();
    let c = g.as_ref().unwrap();
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM media_files WHERE file_type='NOTE'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    // Receipts are tombstones too: a delayed retry must not resurrect deleted notes.
    crate::db::move_to_trash(c, id)?;
    crate::db::permanently_delete_file(c, &f.root, id)?;
    drop(g);
    assert_eq!(notes::save(&f.library, None, &input)?, id);
    assert!(ready(&f.library, id).is_err());
    Ok(())
}

#[tokio::test]
async fn note_save_progress_is_scoped_to_its_request_and_does_not_wait_for_nas_db() -> Result<()> {
    let f = Fixture::new()?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let token = "33333333333333333333333333333333";
    *f.library.archives.job.lock().unwrap() = Job {
        request_id: Some(token.into()),
        running: true,
        ..Default::default()
    };
    let locked = f.library.conn.lock().unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        router
            .clone()
            .oneshot(request(&app, &format!("/api/note-saves/{token}")).body(Body::empty())?),
    )
    .await??;
    assert_eq!(response.status(), StatusCode::OK);
    drop(locked);
    let response = router
        .oneshot(
            request(&app, "/api/note-saves/44444444444444444444444444444444").body(Body::empty())?,
        )
        .await?;
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(data["status"], "unknown");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnected_note_save_admission_finishes_and_releases_busy() -> Result<()> {
    let f = Fixture::new()?;
    let app = AppState::new(f.library.clone())?;
    let state = app.snapshot().unwrap();
    let token = "11112222333344445555666677778888";
    let held = app.current.lock().unwrap();
    let mut caller = Box::pin(notes::create_api(
        State(app.clone()),
        Extension(state.clone()),
        Json(notes::Input {
            request_id: Some(token.into()),
            revision: None,
            title: "probe".into(),
            body: "synthetic".into(),
        }),
    ));
    // Poll through spawn_blocking before disconnecting. Holding current keeps
    // admission pending, so dropping the HTTP future tests a known boundary
    // rather than depending on whether a spawned caller ran within 100 ms.
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(caller.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(caller);
    drop(held);
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let done = {
                let job = state.archives.job.lock().unwrap();
                job.request_id.as_deref() == Some(token) && !job.running
            };
            if done {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?;
    let job = state.archives.job.lock().unwrap().clone();
    assert!(job.completed, "{:?}", job.error);
    assert!(job.result_id.is_some());
    assert!(!state.archives.busy.load(Ordering::Acquire));
    let status = notes::save_status_api(Extension(state.clone()), Path(token.into()))
        .await
        .unwrap();
    assert_eq!(status.0["job"]["result_id"], job.result_id.unwrap());
    let count: i64 = state.conn.lock().unwrap().as_ref().unwrap().query_row(
        "SELECT COUNT(*) FROM media_files WHERE file_type='NOTE'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn note_export_receipt_replays_without_another_picker_or_write() -> Result<()> {
    let f = Fixture::new()?;
    let id = notes::save(
        &f.library,
        None,
        &notes::Input {
            request_id: None,
            revision: None,
            title: "export".into(),
            body: "synthetic body".into(),
        },
    )?;
    let app = AppState::new(f.library.clone())?;
    let state = app.snapshot().unwrap();
    let token = "aaaabbbbccccddddeeeeffff00001111";
    let mut headers = HeaderMap::new();
    headers.insert("X-Vault-Request", token.parse()?);
    let destination = f.root.join("export.json");
    let target = destination.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let first = notes::start_export(app.clone(), state.clone(), id, headers.clone(), move || {
        wait.recv().unwrap();
        Some(target)
    })
    .await
    .unwrap();
    let again = notes::start_export(app.clone(), state.clone(), id, headers.clone(), || {
        panic!("duplicate picker")
    })
    .await
    .unwrap();
    assert_eq!(first.0["job_id"], again.0["job_id"]);
    let progress = progress_status(
        Extension(state.clone()),
        Query(JobQuery {
            request_id: Some(token.into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(progress.0["job"]["running"], true);
    release.send(())?;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while state.archives.busy.load(Ordering::Acquire) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?;
    let job = state.archives.lookup_request(token).unwrap();
    assert!(job.completed, "{:?}", job.error);
    let bytes = fs::read(&destination)?;
    assert!(String::from_utf8_lossy(&bytes).contains("synthetic body"));
    let response = crate::build_router(app.clone())
        .oneshot(
            request(&app, &format!("/api/notes/{id}/export"))
                .method("POST")
                .header("X-Vault-Request", token)
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let data: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(data["job_id"], first.0["job_id"]);
    assert_eq!(fs::read(destination)?, bytes);
    assert!(!state.archives.busy.load(Ordering::Acquire));
    assert!(
        notes::start_export(app, state, id + 1, headers, || panic!("changed request"))
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn note_export_cancel_during_picker_does_not_publish_plaintext() -> Result<()> {
    let f = Fixture::new()?;
    let id = notes::save(
        &f.library,
        None,
        &notes::Input {
            request_id: None,
            revision: None,
            title: "cancel".into(),
            body: "synthetic".into(),
        },
    )?;
    let app = AppState::new(f.library.clone())?;
    let state = app.snapshot().unwrap();
    let target = f.root.join("cancelled.json");
    let picked = target.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let (shown, shown_rx) = tokio::sync::oneshot::channel();
    let started = notes::start_export(app, state.clone(), id, HeaderMap::new(), move || {
        shown.send(()).unwrap();
        wait.recv().unwrap();
        Some(picked)
    })
    .await
    .unwrap();
    shown_rx.await?;
    let _ = cancel(
        Extension(state.clone()),
        Query(JobQuery {
            job_id: Some(started.0["job_id"].as_str().unwrap().into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    release.send(())?;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while state.archives.busy.load(Ordering::Acquire) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?;
    let job = state.archives.job.lock().unwrap().clone();
    assert!(!job.running);
    assert!(!job.completed);
    assert!(job.error.is_some());
    assert!(!target.exists());
    Ok(())
}
