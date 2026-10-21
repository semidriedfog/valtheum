use super::*;
use crate::archive_security::tests::Fixture;

#[test]
fn page_delete_zip_preserves_remaining_bytes_remaps_bookmarks_and_restores_independently(
) -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    rename::rewrite(&f.library, id, "renamed.zip")?;
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        c.execute(
            "INSERT INTO media_bookmarks(file_id,position,thumbnail_data) VALUES(?1,0,X'02')",
            [id],
        )?;
        crate::db::save_thumbnail(c, id, &[1, 2, 3], 1, 1, None, Some(1))?;
    }
    let old = ready(&f.library, id)?.1;
    remove(&f.library, id, "10.png")?;
    assert_eq!(list(&f.library, "series_01.zip")?, vec!["7.png"]);
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    assert!(image(&f.library, "series_01.zip", "10.png").is_err());
    assert!(!f.root.join("series_01.zip").try_exists()?);
    assert!(!f.root.join(old).exists());
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM media_bookmarks WHERE file_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT position FROM media_bookmarks WHERE file_id=?1",
                [id],
                |r| r.get::<_, f64>(0)
            )?,
            0.0
        );
        assert_eq!(
            c.query_row(
                "SELECT page_offset FROM thumbnails WHERE file_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT rating,score,watch_count FROM media_files WHERE id=?1",
                [id],
                |r| Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?
                ))
            )?,
            (5, 1700, 9)
        );
    }
    let (_, rel, key) = ready(&f.library, id)?;
    let path = f.root.join(rel);
    let restored = f.root.join("independent.zip");
    crypto::restore_progress(&path, &key, &restored, |_| Ok(()))?;
    assert_eq!(
        crypto::Reader::open(&path, &key)?.metadata.name,
        "renamed.zip"
    );
    assert_eq!(archive::list_archive_images(&restored)?, vec!["7.png"]);
    assert_eq!(
        archive::read_archive_file(&restored, "7.png")?,
        b"page seven"
    );
    assert!(remove(&f.library, id, "7.png")
        .unwrap_err()
        .to_string()
        .contains("마지막"));
    Ok(())
}

#[test]
fn page_delete_failure_cancellation_and_corruption_preserve_mapping_and_files() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    let (_, old, _) = ready(&f.library, id)?;
    let bytes = fs::read(f.root.join(&old))?;
    assert!(remove(&f.library, id, "absent.png").is_err());
    f.library.archives.cancel.store(true, Ordering::Release);
    assert!(remove(&f.library, id, "10.png").is_err());
    f.library.archives.cancel.store(false, Ordering::Release);
    {
        let g = f.library.conn.lock().unwrap();
        g.as_ref().unwrap().execute_batch("CREATE TRIGGER fail_page_update BEFORE UPDATE OF encrypted_path ON archive_store BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")?;
    }
    assert!(remove(&f.library, id, "10.png").is_err());
    assert_eq!(ready(&f.library, id)?.1, old);
    assert_eq!(fs::read(f.root.join(&old))?, bytes);
    assert_eq!(fs::read_dir(f.root.join(".valtheum/archives"))?.count(), 1);
    assert_eq!(list(&f.library, "series_01.zip")?.len(), 2);
    f.library.archives.clear_cache();
    let mut damaged = bytes.clone();
    let n = damaged.len();
    damaged[n - 1] ^= 1;
    fs::write(f.root.join(&old), &damaged)?;
    assert!(remove(&f.library, id, "10.png").is_err());
    assert_eq!(ready(&f.library, id)?.1, old);
    assert_eq!(fs::read(f.root.join(&old))?, damaged);
    Ok(())
}

#[test]
fn page_delete_tar_zstd_preserves_long_paths_non_images_and_removes_target_thumbnail() -> Result<()>
{
    let f = Fixture::new()?;
    let path = f.root.join("long.tar.zst");
    let long = format!("폴더/{}/7.png", "long-directory".repeat(15));
    let payload: Vec<u8> = (0..200000).map(|n| (n % 251) as u8).collect();
    {
        let encoder = zstd::stream::write::Encoder::new(fs::File::create(&path)?, 3)?;
        let mut tar = tar::Builder::new(encoder);
        for (name, bytes) in [
            ("10.png", b"delete".as_slice()),
            ("20.png", b"delete too".as_slice()),
            (long.as_str(), payload.as_slice()),
            ("note.txt", b"preserve non-image".as_slice()),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(bytes.len() as u64);
            h.set_mode(0o640);
            h.set_mtime(12345);
            h.set_cksum();
            tar.append_data(&mut h, name, bytes)?;
        }
        tar.into_inner()?.finish()?;
    }
    let id = {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, c)?;
        c.query_row(
            "SELECT id FROM media_files WHERE rel_path='long.tar.zst'",
            [],
            |r| r.get(0),
        )?
    };
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    {
        let g = f.library.conn.lock().unwrap();
        crate::db::save_thumbnail(g.as_ref().unwrap(), id, &[1], 1, 1, None, Some(0))?;
    }
    remove_many(&f.library, id, &["10.png".into(), "20.png".into()])?;
    assert_eq!(list(&f.library, "long.tar.zst")?, vec![long.clone()]);
    assert_eq!(image(&f.library, "long.tar.zst", &long)?, payload);
    {
        let g = f.library.conn.lock().unwrap();
        assert!(crate::db::get_thumbnail(g.as_ref().unwrap(), id)?.is_none());
    }
    let (_, rel, key) = ready(&f.library, id)?;
    let reader = crypto::Reader::open(&f.root.join(rel), &key)?;
    let mut tar = tar::Archive::new(zstd::stream::read::Decoder::new(reader)?);
    let mut count = 0;
    for entry in tar.entries()? {
        let mut e = entry?;
        let name = e.path()?.to_string_lossy().into_owned();
        assert_ne!(name, "10.png");
        assert_eq!(e.header().mtime()?, 12345);
        if name == "note.txt" {
            let mut bytes = Vec::new();
            e.read_to_end(&mut bytes)?;
            assert_eq!(bytes, b"preserve non-image");
        }
        count += 1;
    }
    assert_eq!(count, 2);
    Ok(())
}

#[tokio::test]
async fn page_delete_api_requires_auth_generation_and_commits_background_job() -> Result<()> {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let f = Fixture::new()?;
    let id = f.id();
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(f.root.join("series_01.zip"))?);
        for name in ["7.png", "10.png", "11.png"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())?;
            zip.write_all(b"page")?;
        }
        zip.finish()?;
    }
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    let app = AppState::new(f.library.clone())?;
    let router = crate::build_router(app.clone());
    for (token, generation, expected) in [
        ("invalid", "1", StatusCode::UNAUTHORIZED),
        (app.token.as_str(), "999", StatusCode::CONFLICT),
        (app.token.as_str(), "1", StatusCode::OK),
    ] {
        let req = Request::builder()
            .uri(format!("/api/archive-security/{id}/page"))
            .method("DELETE")
            .header("host", "127.0.0.1:8080")
            .header("x-vault-token", token)
            .header("x-vault-library", generation)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"entries":["10.png","11.png"]}"#))?;
        assert_eq!(router.clone().oneshot(req).await?.status(), expected);
    }
    for _ in 0..1000 {
        if !f.library.archives.busy.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let job = f.library.archives.job.lock().unwrap().clone();
    assert!(!job.running);
    assert!(job.completed, "{:?}", job.error);
    assert_eq!(list(&f.library, "series_01.zip")?, vec!["7.png"]);
    Ok(())
}

#[test]
fn page_delete_large_zip_preserves_compressed_payload_and_archive_comment() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    let data: Vec<u8> = (0..300000).map(|n| (n % 251) as u8).collect();
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(f.root.join("series_01.zip"))?);
        zip.set_comment("retained archive comment");
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in [
            ("1.png", b"delete".as_slice()),
            ("2.png", data.as_slice()),
            ("note.txt", b"keep note".as_slice()),
        ] {
            zip.start_file(name, options)?;
            zip.write_all(bytes)?;
        }
        zip.finish()?;
    }
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    remove(&f.library, id, "1.png")?;
    assert_eq!(image(&f.library, "series_01.zip", "2.png")?, data);
    let (_, rel, key) = ready(&f.library, id)?;
    let mut zip = zip::ZipArchive::new(crypto::Reader::open(&f.root.join(rel), &key)?)?;
    assert_eq!(zip.comment(), b"retained archive comment");
    let mut note = Vec::new();
    zip.by_name("note.txt")?.read_to_end(&mut note)?;
    assert_eq!(note, b"keep note");
    assert_eq!(
        zip.by_name("2.png")?.compression(),
        zip::CompressionMethod::Stored
    );
    Ok(())
}

#[test]
fn page_delete_mid_stream_cancel_releases_producer_and_removes_staging_ciphertext() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(f.root.join("series_01.zip"))?);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("1.png", options)?;
        zip.write_all(b"remove")?;
        zip.start_file("2.png", options)?;
        zip.write_all(&vec![42u8; 8 * 1024 * 1024])?;
        zip.finish()?;
    }
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    *f.library.archives.job.lock().unwrap() = Job::default();
    let old = ready(&f.library, id)?.1;
    std::thread::scope(|scope| -> Result<()> {
        let worker = scope.spawn(|| remove(&f.library, id, "1.png"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut interrupted = false;
        while !worker.is_finished() && std::time::Instant::now() < deadline {
            let job = f.library.archives.job.lock().unwrap().clone();
            if job.phase == "암호화" && job.done > 65536 {
                f.library.archives.cancel.store(true, Ordering::Release);
                interrupted = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        f.library.archives.cancel.store(true, Ordering::Release);
        let result = worker.join().unwrap();
        assert!(interrupted, "test must cancel during encrypted streaming");
        assert!(result.is_err());
        Ok(())
    })?;
    f.library.archives.cancel.store(false, Ordering::Release);
    assert_eq!(ready(&f.library, id)?.1, old);
    assert!(f.root.join(old).exists());
    assert_eq!(fs::read_dir(f.root.join(".valtheum/archives"))?.count(), 1);
    assert_eq!(list(&f.library, "series_01.zip")?.len(), 2);
    Ok(())
}

#[test]
fn page_delete_batch_zip_is_atomic_and_remaps_all_remaining_pages() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(f.root.join("series_01.zip"))?);
        for name in ["1.png", "2.png", "3.png", "4.png", "note.txt"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())?;
            zip.write_all(name.as_bytes())?;
        }
        zip.finish()?;
    }
    convert(&f.library, id)?;
    cleanup(&f.library, id)?;
    let old = ready(&f.library, id)?.1;
    let bytes = fs::read(f.root.join(&old))?;
    let selected = vec!["1.png".into(), "3.png".into()];
    for invalid in [
        vec![],
        vec!["1.png".into(), "absent.png".into()],
        vec![
            "1.png".into(),
            "2.png".into(),
            "3.png".into(),
            "4.png".into(),
        ],
    ] {
        assert!(remove_many(&f.library, id, &invalid).is_err());
        assert_eq!(ready(&f.library, id)?.1, old);
    }
    {
        let g = f.library.conn.lock().unwrap();
        let c = g.as_ref().unwrap();
        c.execute("INSERT INTO media_bookmarks(file_id,position,thumbnail_data) VALUES(?1,2,X'02'),(?1,3,X'03')",[id])?;
        crate::db::save_thumbnail(c, id, &[4], 1, 1, None, Some(3))?;
        c.execute_batch("CREATE TRIGGER fail_batch BEFORE UPDATE OF encrypted_path ON archive_store BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")?;
    }
    assert!(remove_many(&f.library, id, &selected).is_err());
    assert_eq!(ready(&f.library, id)?.1, old);
    assert_eq!(fs::read(f.root.join(&old))?, bytes);
    assert_eq!(fs::read_dir(f.root.join(".valtheum/archives"))?.count(), 1);
    f.library
        .conn
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_batch;")?;
    remove_many(&f.library, id, &selected)?;
    assert_eq!(list(&f.library, "series_01.zip")?, vec!["2.png", "4.png"]);
    let (_, rel, key) = ready(&f.library, id)?;
    let restored = f.root.join("batch-restored.zip");
    crypto::restore_progress(&f.root.join(rel), &key, &restored, |_| Ok(()))?;
    assert_eq!(
        archive::list_archive_images(&restored)?,
        vec!["2.png", "4.png"]
    );
    let mut zip = zip::ZipArchive::new(fs::File::open(restored)?)?;
    for name in ["2.png", "4.png", "note.txt"] {
        let mut value = Vec::new();
        zip.by_name(name)?.read_to_end(&mut value)?;
        assert_eq!(value, name.as_bytes());
    }
    let g = f.library.conn.lock().unwrap();
    let c = g.as_ref().unwrap();
    let positions = c
        .prepare("SELECT position FROM media_bookmarks WHERE file_id=?1 ORDER BY position")?
        .query_map([id], |r| r.get::<_, f64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert_eq!(positions, vec![0.0, 1.0]);
    assert_eq!(
        c.query_row(
            "SELECT page_offset FROM thumbnails WHERE file_id=?1",
            [id],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[test]
fn page_delete_requires_original_cleanup_and_remains_permanently_deletable() -> Result<()> {
    let f = Fixture::new()?;
    let id = f.id();
    let original = fs::read(f.root.join("series_01.zip"))?;
    convert(&f.library, id)?;
    let old = ready(&f.library, id)?.1;
    let encrypted = fs::read(f.root.join(&old))?;
    let error = remove(&f.library, id, "10.png").unwrap_err().to_string();
    assert!(error.contains("평문 원본"), "{error}");
    assert_eq!(ready(&f.library, id)?.1, old);
    assert_eq!(fs::read(f.root.join(&old))?, encrypted);
    assert_eq!(fs::read(f.root.join("series_01.zip"))?, original);
    assert_eq!(list(&f.library, "series_01.zip")?, vec!["10.png", "7.png"]);
    assert_eq!(fs::read_dir(f.root.join(".valtheum/archives"))?.count(), 1);
    cleanup(&f.library, id)?;
    remove(&f.library, id, "10.png")?;
    assert_eq!(list(&f.library, "series_01.zip")?, vec!["7.png"]);
    let current = ready(&f.library, id)?.1;
    // The permanent-delete API releases cached archive handles before deletion.
    f.library.archives.clear_cache();
    let g = f.library.conn.lock().unwrap();
    let c = g.as_ref().unwrap();
    crate::db::move_to_trash(c, id)?;
    crate::db::permanently_delete_file(c, &f.root, id)?;
    assert!(!f.root.join(current).try_exists()?);
    Ok(())
}

#[test]
fn page_delete_original_check_fails_closed_on_unresolvable_parent() -> Result<()> {
    let f = Fixture::new()?;
    assert!(require_no_plaintext(&f.library, "missing-directory/original.zip").is_err());
    assert!(require_no_plaintext(&f.library, "missing.zip").is_ok());
    Ok(())
}
