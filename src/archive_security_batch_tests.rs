use super::super::tests::Fixture;
use super::*;
use std::io::{Seek, SeekFrom, Write};

fn setup() -> Result<(Fixture, Vec<i64>, Vec<u8>)> {
    let f = Fixture::new()?;
    let bytes = fs::read(f.root.join("series_01.zip"))?;
    fs::create_dir(f.root.join("nested"))?;
    fs::write(f.root.join("nested/series_01.zip"), &bytes)?;
    let ids = {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        crate::scanner::scan_directory(&f.root, conn)?;
        let mut stmt = conn.prepare("SELECT id FROM media_files ORDER BY id")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        ids
    };
    for id in &ids {
        convert(&f.library, *id)?;
    }
    Ok((f, ids, bytes))
}
fn prepare(f: &Fixture, total: usize) {
    *f.library.archives.job.lock().unwrap() = Job {
        batch: Some(BatchProgress {
            total,
            ..Default::default()
        }),
        ..Default::default()
    };
}
fn counts(f: &Fixture) -> (usize, usize, usize) {
    let job = f.library.archives.job.lock().unwrap();
    let b = job.batch.as_ref().unwrap();
    (b.completed, b.skipped, b.failed)
}

#[test]
fn batch_cleanup_restore_preserves_paths_metadata_ciphertext_and_existing_files() -> Result<()> {
    let (f, ids, bytes) = setup()?;
    validate(&f.library, &ids)?;
    let paths: Vec<_> = ids
        .iter()
        .map(|id| ready(&f.library, *id).unwrap().1)
        .collect();
    let ciphertext: Vec<_> = paths
        .iter()
        .map(|p| fs::read(f.root.join(p)).unwrap())
        .collect();
    prepare(&f, 2);
    run(&f.library, &ids, Action::Plaintext, None)?;
    assert_eq!(counts(&f), (2, 0, 0));
    assert!(!f.root.join("series_01.zip").exists());
    assert!(!f.root.join("nested/series_01.zip").exists());
    let out = f.root.join("output");
    fs::create_dir(&out)?;
    prepare(&f, 2);
    run(&f.library, &ids, Action::Export, Some(&out))?;
    assert_eq!(counts(&f), (2, 0, 0));
    assert_eq!(fs::read(out.join("series_01.zip"))?, bytes);
    assert_eq!(fs::read(out.join("nested/series_01.zip"))?, bytes);
    fs::write(out.join("series_01.zip"), b"keep existing")?;
    prepare(&f, 2);
    run(&f.library, &ids, Action::Export, Some(&out))?;
    assert_eq!(counts(&f), (0, 2, 0));
    assert_eq!(fs::read(out.join("series_01.zip"))?, b"keep existing");
    for (p, original) in paths.iter().zip(ciphertext) {
        assert_eq!(fs::read(f.root.join(p))?, original);
    }
    assert_eq!(image(&f.library, "series_01.zip", "7.png")?, b"page seven");
    let id = f.id();
    assert_eq!(
        crate::db::get_tags_for_file(f.library.conn.lock().unwrap().as_ref().unwrap(), id)?.len(),
        1
    );
    Ok(())
}

#[test]
fn batch_corruption_preserves_original_and_reports_failure_while_other_files_finish() -> Result<()>
{
    let (f, ids, bytes) = setup()?;
    let rel = ready(&f.library, ids[0])?.1;
    let mut file = fs::OpenOptions::new().write(true).open(f.root.join(rel))?;
    let last = *fs::read(f.root.join(ready(&f.library, ids[0])?.1))?
        .last()
        .unwrap();
    file.seek(SeekFrom::End(-1))?;
    file.write_all(&[last ^ 1])?;
    drop(file);
    prepare(&f, 2);
    run(&f.library, &ids, Action::Plaintext, None)?;
    assert_eq!(counts(&f), (1, 0, 1));
    assert_eq!(fs::read(f.root.join("series_01.zip"))?, bytes);
    let out = f.root.join("output");
    fs::create_dir(&out)?;
    prepare(&f, 2);
    run(&f.library, &ids, Action::Export, Some(&out))?;
    assert_eq!(counts(&f), (1, 0, 1));
    assert!(!out.join("series_01.zip").exists());
    assert_eq!(fs::read(out.join("nested/series_01.zip"))?, bytes);
    assert_eq!(
        fs::read_dir(&out)?.count(),
        1,
        "failed plaintext temporary file must be removed"
    );
    f.library.archives.cancel.store(true, Ordering::Release);
    prepare(&f, 2);
    assert!(run(&f.library, &ids, Action::Plaintext, None).is_err());
    assert_eq!(counts(&f), (0, 0, 0));
    assert!(f.root.join("series_01.zip").exists());
    for name in [
        "../outside.zip",
        "nested/../../outside.zip",
        "C:/outside.zip",
        "nested\\outside.zip",
        "",
    ] {
        assert!(destination(&out, name).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn batch_api_validates_whole_selection_and_deduplicates_cleanup() -> Result<()> {
    let (f, ids, _) = setup()?;
    let app = AppState::new(f.library.clone())?;
    let invalid = batch_api(
        State(app.clone()),
        Extension(app.snapshot().unwrap()),
        HeaderMap::new(),
        Json(BatchInput {
            ids: vec![ids[0], 99999],
            action: Action::Plaintext,
        }),
    )
    .await;
    // Acceptance is immediate; the worker validates the entire selection before deleting.
    assert!(invalid.is_ok());
    for _ in 0..500 {
        if !f.library.archives.busy.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(f.library.archives.job.lock().unwrap().error.is_some());
    assert!(!f.library.archives.job.lock().unwrap().completed);
    assert!(!f.library.archives.busy.load(Ordering::Acquire));
    assert!(f.root.join("series_01.zip").exists());
    assert!(batch_api(
        State(app.clone()),
        Extension(app.snapshot().unwrap()),
        HeaderMap::new(),
        Json(BatchInput {
            ids: vec![ids[0], ids[0], ids[1]],
            action: Action::Plaintext
        })
    )
    .await
    .is_ok());
    for _ in 0..500 {
        if !f.library.archives.busy.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(!f.library.archives.busy.load(Ordering::Acquire));
    assert!(f.library.archives.job.lock().unwrap().completed);
    assert_eq!(counts(&f), (2, 0, 0));
    Ok(())
}

#[test]
fn batch_restore_uses_changed_embedded_filename() -> Result<()> {
    let (f, ids, bytes) = setup()?;
    super::super::rename::rewrite(&f.library, ids[0], "renamed.zip")?;
    let output = f.root.join("output");
    fs::create_dir(&output)?;
    assert!(restore_one(&f.library, ids[0], &output)?);
    let name = super::super::rename::restoration_name(&f.library, ids[0])?;
    assert_eq!(fs::read(output.join(name))?, bytes);
    Ok(())
}

#[test]
fn batch_images_and_archives_roundtrip_and_corruption_preserves_originals() -> Result<()> {
    let f = Fixture::new()?;
    let image_bytes = b"synthetic image bytes preserved exactly";
    fs::write(f.root.join("photo.png"), image_bytes)?;
    let image_id = {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        crate::db::upsert_media_file(conn, "photo.png", "IMAGE", image_bytes.len() as i64, 1)?;
        conn.query_row(
            "SELECT id FROM media_files WHERE rel_path='photo.png'",
            [],
            |r| r.get::<_, i64>(0),
        )?
    };
    let ids = [f.id(), image_id];
    let archive_bytes = fs::read(f.root.join("series_01.zip"))?;
    for id in ids {
        convert(&f.library, id)?;
    }
    validate(&f.library, &ids)?;
    let image_path = f.root.join(ready(&f.library, image_id)?.1);
    let ciphertext = fs::read(&image_path)?;
    prepare(&f, 2);
    run(&f.library, &ids, Action::Plaintext, None)?;
    assert_eq!(counts(&f), (2, 0, 0));
    assert!(!f.root.join("photo.png").exists());
    assert!(!f.root.join("series_01.zip").exists());
    let out = f.root.join("output");
    fs::create_dir(&out)?;
    prepare(&f, 2);
    run(&f.library, &ids, Action::Export, Some(&out))?;
    assert_eq!(counts(&f), (2, 0, 0));
    assert_eq!(fs::read(out.join("photo.png"))?, image_bytes);
    assert_eq!(fs::read(out.join("series_01.zip"))?, archive_bytes);
    fs::write(out.join("photo.png"), b"keep existing")?;
    prepare(&f, 2);
    run(&f.library, &ids, Action::Export, Some(&out))?;
    assert_eq!(counts(&f), (0, 2, 0));
    assert_eq!(fs::read(out.join("photo.png"))?, b"keep existing");
    assert_eq!(fs::read(&image_path)?, ciphertext);
    // Corrupted ciphertext must never authorize removal of a still-good original.
    fs::write(f.root.join("photo.png"), image_bytes)?;
    let mut damaged = ciphertext;
    *damaged.last_mut().unwrap() ^= 1;
    fs::write(&image_path, &damaged)?;
    prepare(&f, 1);
    run(&f.library, &[image_id], Action::Plaintext, None)?;
    assert_eq!(counts(&f), (0, 0, 1));
    assert_eq!(fs::read(f.root.join("photo.png"))?, image_bytes);
    let failed_out = f.root.join("failed-output");
    fs::create_dir(&failed_out)?;
    prepare(&f, 1);
    run(&f.library, &[image_id], Action::Export, Some(&failed_out))?;
    assert_eq!(counts(&f), (0, 0, 1));
    assert_eq!(fs::read_dir(failed_out)?.count(), 0);
    Ok(())
}
