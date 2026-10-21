use super::*;
const OLD: &str = "interop password 123";
const NEW: &str = "changed test password 456";

#[test]
fn preparing_recovery_accepts_sqlite_sidecars_and_preserves_live_db_and_key() -> Result<()> {
    let mut f = Fixture::new(true)?;
    f.conn.take();
    let before = fs::read(f.root.join(encryption::SECURE_DB))?;
    let key = fs::read(f.root.join(crypto::KEY_PATH))?;
    let dir = folder(&f.root)?;
    fs::create_dir(&dir)?;
    write_journal(&dir, "preparing", true)?;
    for name in [
        "old.db",
        "new.db",
        "new.db-journal",
        "new.db-wal",
        "new.db-shm",
        "old.key",
        "new.key",
    ] {
        fs::write(dir.join(name), b"interrupted preparation")?;
    }
    drop(crate::state::Library::open(f.root.clone())?);
    assert!(!dir.exists());
    assert_eq!(fs::read(f.root.join(encryption::SECURE_DB))?, before);
    assert_eq!(fs::read(f.root.join(crypto::KEY_PATH))?, key);
    f.check(OLD)?;
    Ok(())
}

#[test]
fn initial_marker_interruption_is_recoverable_but_missing_snapshot_journal_is_not() -> Result<()> {
    let f = Fixture::new(false)?;
    let dir = folder(&f.root)?;
    fs::create_dir(&dir)?;
    fs::write(
        dir.join(".vault-write-0123456789abcdef.tmp"),
        b"partial marker",
    )?;
    recover(&f.root)?;
    assert!(!dir.exists());
    f.check(OLD)?;

    fs::create_dir(&dir)?;
    fs::write(dir.join("old.db"), b"recovery evidence")?;
    assert!(recover(&f.root).is_err());
    assert_eq!(fs::read(dir.join("old.db"))?, b"recovery evidence");
    write_journal(&dir, "preparing", false)?;
    fs::write(dir.join("unexpected.txt"), b"keep")?;
    assert!(recover(&f.root).is_err());
    assert_eq!(fs::read(dir.join("old.db"))?, b"recovery evidence");
    assert!(dir.join("journal.json").exists());
    Ok(())
}

struct Fixture {
    root: PathBuf,
    conn: Option<Connection>,
}
impl Fixture {
    fn new(media: bool) -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "password-change-test-{}",
            crypto::hex(&crypto::random::<16>()?)
        ));
        fs::create_dir(&root)?;
        let conn = encryption::open(&root.join(encryption::SECURE_DB), Some(OLD), true)?;
        crate::db::init_db(&conn)?;
        crate::db::upsert_media_file(&conn, "original.zip", "ARCHIVE", 180000, 1)?;
        crate::db::update_rating(&conn, 1, 5)?;
        crate::db::add_tag_to_file(&conn, 1, "preserved")?;
        if media {
            fs::create_dir_all(root.join(".valtheum/archives"))?;
            fs::copy("tests/fixtures/vta1/vault.key", root.join(crypto::KEY_PATH))?;
            fs::copy(
                "tests/fixtures/vta1/sample.vta",
                root.join(".valtheum/archives/sample.vta"),
            )?;
            let key = fs::read(root.join(crypto::KEY_PATH))?;
            conn.execute(
                "INSERT INTO library_info(key,value) VALUES('archive_key_sha256',?1)",
                [crypto::hex(&Sha256::digest(key))],
            )?;
            conn.execute("INSERT INTO archive_store(file_id,encrypted_path,state) VALUES(1,'.valtheum/archives/sample.vta','ready')",[])?;
        }
        Ok(Self {
            root,
            conn: Some(conn),
        })
    }
    fn check(&self, password: &str) -> Result<()> {
        let conn = encryption::open(
            &self.root.join(encryption::SECURE_DB),
            Some(password),
            false,
        )?;
        encryption::verify(&conn, true)?;
        let files = crate::db::get_media_files(&conn, None, None)?;
        assert_eq!(files[0].rating, 5);
        assert_eq!(files[0].tags[0].name, "preserved");
        let path = self.root.join(crypto::KEY_PATH);
        if path.exists() {
            let bytes = fs::read(path)?;
            crate::archive_security::check_key(&conn, &bytes)?;
            let key = crypto::read_key(&bytes, password)?;
            crypto::verify(
                &self.root.join(".valtheum/archives/sample.vta"),
                &key,
                |_| Ok(()),
            )?;
        }
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.conn.take();
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("password-change-test-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn rotation_preserves_media_master_key_bytes_and_metadata_and_backups_work() -> Result<()> {
    let mut f = Fixture::new(true)?;
    let old_key = fs::read(f.root.join(crypto::KEY_PATH))?;
    let ciphertext = fs::read(f.root.join(".valtheum/archives/sample.vta"))?;
    assert!(change(&f.root, &mut f.conn, OLD, NEW)?);
    f.check(NEW)?;
    let new_key = fs::read(f.root.join(crypto::KEY_PATH))?;
    assert_ne!(old_key, new_key);
    assert_eq!(
        *crypto::read_key(&old_key, OLD)?,
        *crypto::read_key(&new_key, NEW)?
    );
    assert!(crypto::read_key(&new_key, OLD).is_err());
    assert!(encryption::open(&f.root.join(encryption::SECURE_DB), Some(OLD), false).is_err());
    assert_eq!(
        fs::read(f.root.join(".valtheum/archives/sample.vta"))?,
        ciphertext
    );
    assert!(persistence::backup_bundle(
        f.conn.as_ref().unwrap(),
        &f.root,
        &f.root.join(encryption::SECURE_DB),
        &f.root.join("new.vtbackup"),
        Some(NEW)
    )?);
    assert!(!folder(&f.root)?.exists());
    Ok(())
}
#[test]
fn failed_publication_rolls_back_both_files_and_can_retry() -> Result<()> {
    for phase in ["prepared", "database", "key"] {
        let mut f = Fixture::new(true)?;
        let mut reached = false;
        assert!(change_with_hook(&f.root, &mut f.conn, OLD, NEW, |at| {
            if at == phase {
                reached = true;
            }
            ensure!(at != phase, "injected disk failure");
            Ok(())
        })
        .is_err());
        assert!(reached, "fault stage was not reached: {phase}");
        f.check(OLD)?;
        assert!(f.conn.is_some());
        assert!(!folder(&f.root)?.exists());
    }
    Ok(())
}
#[test]
fn restart_recovers_interruption_between_database_and_key_publication() -> Result<()> {
    for phase in ["database", "key"] {
        let mut f = Fixture::new(true)?;
        let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = change_with_hook(&f.root, &mut f.conn, OLD, NEW, |at| {
                if at == phase {
                    panic!("simulated process termination")
                };
                Ok(())
            });
        }));
        assert!(crash.is_err());
        f.conn.take();
        let library = crate::state::Library::open(f.root.clone())?;
        assert!(library.security_status().locked);
        drop(library);
        f.check(OLD)?;
        assert!(!folder(&f.root)?.exists());
    }
    Ok(())
}
#[test]
fn database_only_rotation_and_invalid_passwords_do_not_create_media_key() -> Result<()> {
    let mut f = Fixture::new(false)?;
    assert!(change(&f.root, &mut f.conn, "wrong", NEW).is_err());
    assert!(change(&f.root, &mut f.conn, OLD, "short").is_err());
    assert!(change(&f.root, &mut f.conn, OLD, OLD).is_err());
    f.check(OLD)?;
    change(&f.root, &mut f.conn, OLD, NEW)?;
    f.check(NEW)?;
    assert!(!f.root.join(crypto::KEY_PATH).exists());
    Ok(())
}

#[test]
fn restart_after_commit_keeps_new_password_and_cleans_snapshots() -> Result<()> {
    let mut f = Fixture::new(true)?;
    let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = change_with_hook(&f.root, &mut f.conn, OLD, NEW, |at| {
            if at == "committed" {
                panic!("simulated termination after commit");
            }
            Ok(())
        });
    }));
    assert!(crash.is_err());
    f.conn.take();
    drop(crate::state::Library::open(f.root.clone())?);
    f.check(NEW)?;
    assert!(!folder(&f.root)?.exists());
    Ok(())
}

#[test]
fn missing_required_key_preserves_database_and_never_generates_replacement() -> Result<()> {
    let mut f = Fixture::new(true)?;
    fs::remove_file(f.root.join(crypto::KEY_PATH))?;
    assert!(change(&f.root, &mut f.conn, OLD, NEW).is_err());
    encryption::verify(
        &encryption::open(&f.root.join(encryption::SECURE_DB), Some(OLD), false)?,
        true,
    )?;
    assert!(!f.root.join(crypto::KEY_PATH).exists());
    assert!(!folder(&f.root)?.exists());
    Ok(())
}
