use super::*;
use sha2::{Digest, Sha256};

struct Fixture {
    root: PathBuf,
    bundle: PathBuf,
    password: &'static str,
}
impl Fixture {
    fn new(encrypted: bool, key: bool) -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "db-restore-test-{}",
            crypto::hex(&crypto::random::<12>()?)
        ));
        fs::create_dir(&root)?;
        fs::create_dir(root.join("source"))?;
        fs::create_dir(root.join("output"))?;
        let source = root.join("source");
        let password = "backup password 123";
        let db = source.join(if encrypted {
            encryption::SECURE_DB
        } else {
            encryption::PLAIN_DB
        });
        let conn = encryption::open(&db, encrypted.then_some(password), true)?;
        crate::db::init_db(&conn)?;
        crate::db::upsert_media_file(&conn, "saved.png", "IMAGE", 4, 0)?;
        conn.execute_batch("UPDATE media_files SET rating=5,watch_count=9;INSERT INTO tags(id,name) VALUES(1,'preserved');INSERT INTO file_tags(file_id,tag_id) SELECT id,1 FROM media_files;")?;
        if key {
            fs::create_dir(source.join(".valtheum"))?;
            let (bytes, _) = crypto::make_key(password)?;
            fs::write(source.join(crypto::KEY_PATH), &bytes)?;
            conn.execute(
                "INSERT INTO library_info(key,value) VALUES('archive_key_sha256',?1)",
                [crypto::hex(&Sha256::digest(&bytes))],
            )?;
        }
        let bundle = root.join("source.vtbackup");
        crate::persistence::backup_bundle(
            &conn,
            &source,
            &db,
            &bundle,
            encrypted.then_some(password),
        )?;
        drop(conn);
        Ok(Self {
            root,
            bundle,
            password,
        })
    }
    fn restore(&self, password: &str) -> Result<PathBuf> {
        restore_bundle(
            &mut crypto::open_source(&self.bundle)?,
            &self.root.join("output"),
            password,
        )
    }
    fn rewrite(&self, edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Result<()> {
        let mut zip = zip::ZipArchive::new(fs::File::open(&self.bundle)?)?;
        let mut entries = Vec::new();
        for i in 0..zip.len() {
            let mut e = zip.by_index(i)?;
            let mut bytes = Vec::new();
            e.read_to_end(&mut bytes)?;
            entries.push((e.name().into(), bytes));
        }
        drop(zip);
        edit(&mut entries);
        let mut zip = zip::ZipWriter::new(fs::File::create(&self.bundle)?);
        for (name, bytes) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())?;
            zip.write_all(&bytes)?;
        }
        zip.finish()?;
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("db-restore-test-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn plain_restore_keeps_metadata_until_media_arrives_and_never_overwrites() -> Result<()> {
    let f = Fixture::new(false, false)?;
    fs::write(f.root.join("output/keep.txt"), b"preserve")?;
    let before = fs::read(&f.bundle)?;
    let path = f.restore("")?;
    assert_eq!(fs::read(&f.bundle)?, before);
    let library = Library::open(path.clone())?;
    {
        let g = library.conn.lock().unwrap();
        let conn = g.as_ref().unwrap();
        let report = crate::scanner::scan_directory(&path, conn)?;
        assert_eq!(report.removed_count, 0);
        assert!(!report.errors.is_empty());
        assert_eq!(
            conn.query_row("SELECT rating*100+watch_count FROM media_files", [], |r| {
                r.get::<_, i64>(0)
            })?,
            509
        );
        assert_eq!(crate::db::get_tags_for_file(conn, 1)?.len(), 1);
        fs::write(path.join("saved.png"), b"data")?;
        assert!(crate::scanner::scan_directory(&path, conn)?
            .errors
            .is_empty());
        let pending: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM library_info WHERE key='restore_pending_media')",
            [],
            |r| r.get(0),
        )?;
        assert!(!pending);
    }
    drop(library);
    let second = f.restore("")?;
    assert_ne!(path, second);
    assert_eq!(fs::read(f.root.join("output/keep.txt"))?, b"preserve");
    Ok(())
}

#[test]
fn encrypted_restore_requires_backup_password_and_matching_key_without_changing_source(
) -> Result<()> {
    let f = Fixture::new(true, true)?;
    let before = fs::read(&f.bundle)?;
    assert!(f.restore("wrong").is_err());
    assert_eq!(fs::read_dir(f.root.join("output"))?.count(), 0);
    let path = f.restore(f.password)?;
    assert_eq!(
        fs::read(path.join(crypto::KEY_PATH))?,
        fs::read(f.root.join("source").join(crypto::KEY_PATH))?
    );
    let conn = encryption::open(&path.join(encryption::SECURE_DB), Some(f.password), false)?;
    encryption::verify(&conn, true)?;
    crate::archive_security::check_key(&conn, &fs::read(path.join(crypto::KEY_PATH))?)?;
    drop(conn);
    assert_eq!(fs::read(&f.bundle)?, before);
    Ok(())
}

#[test]
fn malformed_bundle_missing_or_wrong_key_and_path_entries_fail_without_publication() -> Result<()> {
    let f = Fixture::new(true, true)?;
    let original = fs::read(&f.bundle)?;
    f.rewrite(|entries| {
        entries.retain(|(name, _)| name != crypto::KEY_PATH);
        let manifest = entries
            .iter_mut()
            .find(|(n, _)| n == "backup.json")
            .unwrap();
        let mut data: serde_json::Value = serde_json::from_slice(&manifest.1).unwrap();
        data["key_included"] = false.into();
        manifest.1 = serde_json::to_vec(&data).unwrap();
    })?;
    assert!(f.restore(f.password).is_err());
    fs::write(&f.bundle, &original)?;
    let (other, _) = crypto::make_key(f.password)?;
    f.rewrite(|entries| {
        entries
            .iter_mut()
            .find(|(n, _)| n == crypto::KEY_PATH)
            .unwrap()
            .1 = other;
    })?;
    assert!(f.restore(f.password).is_err());
    fs::write(&f.bundle, &original)?;
    f.rewrite(|entries| {
        entries
            .iter_mut()
            .find(|(n, _)| n == "RESTORE.txt")
            .unwrap()
            .0 = "../outside.txt".into();
    })?;
    assert!(f.restore(f.password).is_err());
    fs::write(&f.bundle, &original)?;
    f.rewrite(|entries| {
        let db = &mut entries
            .iter_mut()
            .find(|(n, _)| n == encryption::SECURE_DB)
            .unwrap()
            .1;
        db[100] ^= 1;
    })?;
    assert!(f.restore(f.password).is_err());
    assert_eq!(fs::read_dir(f.root.join("output"))?.count(), 0);
    assert!(!f.root.join("outside.txt").exists());
    Ok(())
}

#[test]
fn stalled_restore_allows_close_and_preserves_completed_output_without_installing() -> Result<()> {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    let f = Fixture::new(false, false)?;
    let library = Library::open(f.root.join("source"))?;
    let app = AppState::new(library.clone())?;
    let file = Arc::new(Mutex::new(fs::File::open(&f.bundle)?));
    let held = file.lock().unwrap();
    let work_app = app.clone();
    let work_file = file.clone();
    let parent = f.root.join("output");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let worker = rt.spawn_blocking(move || {
        prepare_restore(
            &work_app,
            1,
            work_file,
            parent,
            Zeroizing::new(String::new()),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !library.archives.busy.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let active = library.archives.busy.load(Ordering::Acquire);
    let state = app.snapshot().unwrap();
    let close = rt.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(2),
            crate::security_api::close(State(app.clone()), Extension(state)),
        )
        .await
    });
    drop(held);
    let (path, restored) = rt.block_on(worker)??;
    assert!(active);
    assert!(close.unwrap().is_ok());
    assert!(path.join(encryption::PLAIN_DB).is_file());
    assert!(!library.archives.busy.load(Ordering::Acquire));
    assert_eq!(app.snapshot().unwrap().generation, 2);
    assert!(app.concealed.load(Ordering::Acquire));
    assert!(crate::install_library_at(
        &app,
        1,
        restored,
        crate::AppConfig::default(),
        &f.root.join("test-config.json")
    )
    .is_err());
    assert!(!f.root.join("test-config.json").exists());
    Ok(())
}
