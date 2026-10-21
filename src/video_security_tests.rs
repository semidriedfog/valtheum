use super::tests::Fixture;
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request},
    Router,
};

use tower::ServiceExt;

fn add_video(f: &Fixture, name: &str, data: &[u8]) -> Result<i64> {
    let path = f.root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)?;
    let guard = f.library.conn.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    crate::db::upsert_media_file(conn, name, "VIDEO", data.len() as i64, 1)?;
    Ok(conn.query_row(
        "SELECT id FROM media_files WHERE rel_path=?1",
        [name],
        |r| r.get(0),
    )?)
}
fn request(app: &AppState, path: &str) -> axum::http::request::Builder {
    Request::builder()
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("x-vault-token", app.token.as_str())
        .header("x-vault-library", "1")
}
async fn body(router: &Router, app: &AppState, path: &str) -> Result<serde_json::Value> {
    let response = router
        .clone()
        .oneshot(request(app, path).body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024).await?,
    )?)
}

#[tokio::test]
async fn encrypted_video_ranges_head_and_full_playback_work_after_plaintext_removal() -> Result<()>
{
    let f = Fixture::new()?;
    let data: Vec<u8> = (0..250000)
        .map(|i| ((i * 31 + i / 7) % 251) as u8)
        .collect();
    let id = add_video(&f, "videos/sample.mp4", &data)?;
    convert(&f.library, id)?;
    let (_, rel, _) = ready(&f.library, id)?;
    assert!(rel.starts_with(".valtheum/videos/") && rel.ends_with(".vtv"));
    assert_eq!(fs::read(f.root.join("videos/sample.mp4"))?, data);
    cleanup(&f.library, id)?;
    fs::remove_dir(f.root.join("videos"))?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let files = body(&router, &app, "/api/files").await?;
    assert!(files
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == id && v["encrypted"] == true));
    let status = body(&router, &app, "/api/archive-security").await?;
    assert!(status["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == id && v["file_type"] == "VIDEO" && v["state"] == "ready"));
    for (range, lo, hi) in [
        ("bytes=0-0", 0, 1),
        ("bytes=65000-66000", 65000, 66001),
        ("bytes=-12", data.len() - 12, data.len()),
        ("bytes=249990-", 249990, data.len()),
        ("bytes=249990-999999", 249990, data.len()),
    ] {
        let response = router
            .clone()
            .oneshot(
                request(&app, "/api/media/videos/sample.mp4")
                    .header(header::RANGE, range)
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            (hi - lo).to_string()
        );
        assert_eq!(
            response.headers()[header::CONTENT_RANGE],
            format!("bytes {lo}-{}/{}", hi - 1, data.len())
        );
        assert_eq!(
            &to_bytes(response.into_body(), data.len()).await?[..],
            &data[lo..hi]
        );
    }
    let response = router
        .clone()
        .oneshot(request(&app, "/api/media/videos/sample.mp4").body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(
        &to_bytes(response.into_body(), data.len()).await?[..],
        &data
    );
    let response = router
        .clone()
        .oneshot(
            request(&app, "/api/media/videos/sample.mp4")
                .method("HEAD")
                .header(header::RANGE, "bytes=0-10")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_LENGTH],
        data.len().to_string()
    );
    assert!(to_bytes(response.into_body(), 10).await?.is_empty());
    let response = router
        .clone()
        .oneshot(
            request(&app, "/api/media/videos/sample.mp4")
                .header(header::RANGE, "bytes=250000-")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */250000");
    let response = router
        .clone()
        .oneshot(
            request(&app, "/api/media/videos/sample.mp4")
                .header(header::RANGE, "bytes=0-1")
                .header(header::IF_RANGE, "old-etag")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        &to_bytes(response.into_body(), data.len()).await?[..],
        &data
    );
    Ok(())
}

#[tokio::test]
async fn encrypted_video_api_conversion_preserves_metadata_scan_backup_and_restore() -> Result<()> {
    let f = Fixture::new()?;
    let data = vec![43; 180000];
    let id = add_video(&f, "a.webm", &data)?;
    {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "UPDATE media_files SET rating=4,watch_count=7,duration_secs=200 WHERE id=?1",
            [id],
        )?;
        conn.execute(
            "INSERT INTO media_bookmarks(file_id,position,thumbnail_data) VALUES(?1,123.5,X'01')",
            [id],
        )?;
        conn.execute("INSERT INTO file_tags(file_id,tag_id) VALUES(?1,1)", [id])?;
    }
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let response = router
        .clone()
        .oneshot(
            request(&app, &format!("/api/archive-security/{id}/encrypt"))
                .method("POST")
                .body(Body::empty())?,
        )
        .await?;
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
    let backup = f.root.join("video.vtbackup");
    {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, conn)?;
        assert_eq!(
            conn.query_row(
                "SELECT rating,watch_count,duration_secs FROM media_files WHERE id=?1",
                [id],
                |r| Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?
                ))
            )?,
            (4, 7, 200)
        );
        assert_eq!(
            conn.query_row(
                "SELECT position FROM media_bookmarks WHERE file_id=?1",
                [id],
                |r| r.get::<_, f64>(0)
            )?,
            123.5
        );
        assert_eq!(crate::db::get_tags_for_file(conn, id)?.len(), 1);
        crate::persistence::backup_bundle(
            conn,
            &f.root,
            &f.root.join(crate::encryption::SECURE_DB),
            &backup,
            Some("fixture password 123"),
        )?;
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(&backup)?)?;
    assert_eq!(zip.by_name(crypto::KEY_PATH)?.size(), 96);
    cleanup(&f.library, id)?;
    let out = f.root.join("export");
    fs::create_dir(&out)?;
    assert!(batch::restore_one(&f.library, id, &out)?);
    assert_eq!(fs::read(out.join("a.webm"))?, data);
    assert!(!batch::restore_one(&f.library, id, &out)?);
    Ok(())
}

#[tokio::test]
async fn encrypted_video_never_falls_back_to_original_and_revokes_open_streams() -> Result<()> {
    let f = Fixture::new()?;
    let id = add_video(&f, "private.mp4", &vec![81; 2 * 1024 * 1024])?;
    convert(&f.library, id)?;
    let (_, rel, _) = ready(&f.library, id)?;
    let encrypted = f.root.join(&rel);
    let good = fs::read(&encrypted)?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    let unauthorized = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/media/private.mp4")
                .header("host", "127.0.0.1:8080")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    fs::remove_file(&encrypted)?;
    let response = router
        .clone()
        .oneshot(request(&app, "/api/media/private.mp4").body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    fs::write(&encrypted, &good)?;
    let mut corrupt = good.clone();
    corrupt[90000] ^= 1;
    fs::write(&encrypted, &corrupt)?;
    let response = router
        .clone()
        .oneshot(
            request(&app, "/api/media/private.mp4")
                .header(header::RANGE, "bytes=70000-71000")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert!(to_bytes(response.into_body(), 10000).await.is_err());
    // Let the cancelled body release its read handle before replacing this test file.
    for _ in 0..100 {
        if fs::write(&encrypted, &good).is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let response = router
        .clone()
        .oneshot(request(&app, "/api/media/private.mp4").body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    f.library.archives.retire();
    assert!(to_bytes(response.into_body(), 3 * 1024 * 1024)
        .await
        .is_err());
    Ok(())
}
