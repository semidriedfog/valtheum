use super::*;
use anyhow::Result;
use sha2::{Digest, Sha256};

struct Fixture {
    root: PathBuf,
    library: Arc<Library>,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "valtheum-integrity-{}",
            crypto::hex(&crypto::random::<16>()?)
        ));
        fs::create_dir(&root)?;
        let library = Library::open(root.clone())?;
        Ok(Self { root, library })
    }

    fn begin(&self) {
        self.library
            .integrity
            .cancelled
            .store(false, Ordering::Release);
        *self.library.integrity.job.lock().unwrap() = Job {
            id: 1,
            running: true,
            phase: "db".into(),
            ..Default::default()
        };
    }

    fn cipher(&self, name: &str, key: &[u8; 32]) -> Result<Entry> {
        let video = crypto::video::format(name).is_some();
        let stored = if video { "sample.vtv" } else { "sample.vta" };
        let source = self.root.join("fixture-source.bin");
        // Several complete interior blocks make the quick-check boundary explicit.
        fs::write(&source, vec![0x5a; 320_000])?;
        let metadata = crypto::encrypt(
            &mut crypto::open_source(&source)?,
            &self.root.join(stored),
            name,
            key,
            |_, _, _| Ok(()),
        )?;
        Ok(Entry {
            id: 1,
            name: if video {
                "original.mp4"
            } else {
                "original.zip"
            }
            .into(),
            kind: if video { "VIDEO" } else { "ARCHIVE" }.into(),
            size: metadata.size as i64,
            stored: Some(stored.into()),
            state: Some("ready".into()),
            hash: Some(metadata.sha256),
            restore_name: name.into(),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.library.archives.retire();
        self.library.conn.lock().unwrap().take();
        self.library = Library::unconfigured();
        assert!(self.root.parent() == Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("valtheum-integrity-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn plain(name: &str, size: i64) -> Entry {
    Entry {
        id: 1,
        name: name.into(),
        kind: "IMAGE".into(),
        size,
        stored: None,
        state: None,
        hash: None,
        restore_name: name.into(),
    }
}

#[test]
fn plain_checks_detect_changes_missing_files_directories_and_invalid_paths() -> Result<()> {
    let f = Fixture::new()?;
    fs::write(f.root.join("photo.png"), b"photo")?;
    assert!(check_entry(&f.library, &plain("photo.png", 5), None).is_none());
    assert_eq!(
        check_entry(&f.library, &plain("photo.png", 6), None)
            .unwrap()
            .state,
        "mismatch"
    );
    assert_eq!(
        check_entry(&f.library, &plain("photo.png", -1), None)
            .unwrap()
            .state,
        "mismatch"
    );
    assert_eq!(
        check_entry(&f.library, &plain("missing.png", 5), None)
            .unwrap()
            .state,
        "missing"
    );
    fs::create_dir(f.root.join("directory.png"))?;
    assert_eq!(
        check_entry(&f.library, &plain("directory.png", 5), None)
            .unwrap()
            .state,
        "mismatch"
    );
    for path in ["../outside.png", "/outside.png", "C:/outside.png"] {
        assert_eq!(
            check_entry(&f.library, &plain(path, 5), None)
                .unwrap()
                .state,
            "invalid_path"
        );
    }
    Ok(())
}

#[test]
fn canonical_path_outside_library_is_rejected() -> Result<()> {
    let f = Fixture::new()?;
    let other = Fixture::new()?;
    let outside = other.root.join("outside.png");
    fs::write(&outside, b"outside")?;
    let error = within_root(&f.library.root_dir, &outside).unwrap_err();
    assert_eq!(
        path_failure(&f.library, Some(1), "outside.png", &error).state,
        "invalid_path"
    );
    Ok(())
}

#[test]
fn pending_cipher_checks_the_original_and_reports_incomplete_encryption() -> Result<()> {
    let f = Fixture::new()?;
    fs::write(f.root.join("photo.png"), b"photo")?;
    let mut entry = plain("photo.png", 5);
    entry.state = Some("pending".into());
    entry.stored = Some("not-published.vta".into());
    assert_eq!(
        check_entry(&f.library, &entry, None).unwrap().state,
        "pending"
    );
    fs::remove_file(f.root.join("photo.png"))?;
    assert_eq!(
        check_entry(&f.library, &entry, None).unwrap().state,
        "missing"
    );
    Ok(())
}

#[test]
fn cipher_checks_support_archive_video_and_changed_restore_names() -> Result<()> {
    for name in ["renamed.zip", "renamed.tar.zst", "renamed.mp4"] {
        let f = Fixture::new()?;
        let key = crypto::random::<32>()?;
        let mut entry = f.cipher(name, &key)?;
        assert_ne!(entry.name, entry.restore_name);
        assert!(
            check_entry(&f.library, &entry, Some(&key)).is_none(),
            "{name}"
        );
        entry.restore_name = entry.name.clone();
        assert_eq!(
            check_entry(&f.library, &entry, Some(&key)).unwrap().state,
            "mismatch"
        );
    }
    Ok(())
}

#[test]
fn cipher_metadata_must_match_the_database_size_format_and_stored_digest() -> Result<()> {
    let f = Fixture::new()?;
    let key = crypto::random::<32>()?;
    let mut entry = f.cipher("renamed.zip", &key)?;
    entry.size += 1;
    assert_eq!(
        check_entry(&f.library, &entry, Some(&key)).unwrap().state,
        "mismatch"
    );
    entry.size -= 1;
    entry.kind = "VIDEO".into();
    assert_eq!(
        check_entry(&f.library, &entry, Some(&key)).unwrap().state,
        "mismatch"
    );
    entry.kind = "ARCHIVE".into();
    entry.hash = Some("0".repeat(64));
    assert_eq!(
        check_entry(&f.library, &entry, Some(&key)).unwrap().state,
        "mismatch"
    );
    Ok(())
}

#[test]
fn quick_check_authenticates_boundaries_but_does_not_claim_to_check_interior_blocks() -> Result<()>
{
    for name in ["renamed.zip", "renamed.mp4"] {
        let f = Fixture::new()?;
        let key = crypto::random::<32>()?;
        let entry = f.cipher(name, &key)?;
        let path = f.root.join(entry.stored.as_ref().unwrap());
        let original = fs::read(&path)?;
        for offset in [0, 125, original.len() - 1] {
            let mut bytes = original.clone();
            bytes[offset] ^= 1;
            fs::write(&path, bytes)?;
            assert_eq!(
                check_entry(&f.library, &entry, Some(&key)).unwrap().state,
                "mismatch",
                "{name} offset {offset}"
            );
        }
        let mut bytes = original.clone();
        bytes[140_000] ^= 1;
        fs::write(&path, bytes)?;
        assert!(
            check_entry(&f.library, &entry, Some(&key)).is_none(),
            "An interior block is intentionally outside the quick check."
        );
        // Verify the fixture really is damaged using the separate full-verification API.
        assert!(crypto::verify(&path, &key, |_| Ok(())).is_err());
        fs::write(&path, &original[..original.len() - 1])?;
        assert_eq!(
            check_entry(&f.library, &entry, Some(&key)).unwrap().state,
            "mismatch"
        );
        let mut appended = original;
        appended.push(0);
        fs::write(&path, appended)?;
        assert_eq!(
            check_entry(&f.library, &entry, Some(&key)).unwrap().state,
            "mismatch"
        );
    }
    Ok(())
}

#[test]
fn wrong_cipher_key_is_a_verification_failure_and_absent_key_is_unavailable() -> Result<()> {
    let f = Fixture::new()?;
    let key = crypto::random::<32>()?;
    let entry = f.cipher("sample.zip", &key)?;
    let wrong = crypto::random::<32>()?;
    assert_eq!(
        check_entry(&f.library, &entry, Some(&wrong)).unwrap().state,
        "mismatch"
    );
    assert_eq!(
        check_entry(&f.library, &entry, None).unwrap().state,
        "unavailable"
    );
    Ok(())
}

#[test]
fn key_checks_validate_db_fingerprint_password_and_active_key() -> Result<()> {
    let f = Fixture::new()?;
    let password = "integrity fixture password";
    let (bytes, key) = crypto::make_key(password)?;
    fs::create_dir_all(f.root.join(".valtheum"))?;
    fs::write(f.root.join(crypto::KEY_PATH), &bytes)?;
    let expected = crypto::hex(&Sha256::digest(&bytes));
    let opened = check_key(&f.library, Some(&expected), Some(password))
        .map_err(|item| anyhow::anyhow!("{}: {}", item.state, item.message))?;
    assert_eq!(*opened, *key);
    assert_eq!(
        check_key(&f.library, Some(&"0".repeat(64)), Some(password))
            .err()
            .unwrap()
            .state,
        "key_error"
    );
    assert_eq!(
        check_key(&f.library, Some(&expected), Some("wrong password"))
            .err()
            .unwrap()
            .state,
        "key_error"
    );
    *f.library.archives.key.lock().unwrap() =
        Some(zeroize::Zeroizing::new(crypto::random::<32>()?));
    assert_eq!(
        check_key(&f.library, Some(&expected), Some(password))
            .err()
            .unwrap()
            .state,
        "key_error"
    );
    fs::write(f.root.join(crypto::KEY_PATH), b"invalid key")?;
    assert_eq!(
        check_key(&f.library, Some(&expected), Some(password))
            .err()
            .unwrap()
            .state,
        "key_error"
    );
    Ok(())
}

#[test]
fn io_failures_are_unavailable_and_missing_requires_accessible_storage() -> Result<()> {
    let f = Fixture::new()?;
    for kind in [
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::TimedOut,
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::Other,
    ] {
        assert_eq!(
            io_kind(&f.library, &io::Error::new(kind, "fixture I/O error")),
            "unavailable"
        );
    }
    for code in [53, 54, 59, 64, 67, 121, 1231, 1232, 1236, 2250] {
        assert_eq!(
            io_kind(&f.library, &io::Error::from_raw_os_error(code)),
            "unavailable"
        );
    }
    let missing = io::Error::new(io::ErrorKind::NotFound, "missing");
    assert_eq!(io_kind(&f.library, &missing), "missing");
    // A configured DB that cannot be found makes the storage probe fail too.
    f.library.security.lock().unwrap().encrypted = true;
    assert_eq!(io_kind(&f.library, &missing), "unavailable");
    assert_eq!(
        check_entry(&f.library, &plain("missing.png", 3), None)
            .unwrap()
            .state,
        "unavailable"
    );
    Ok(())
}

#[test]
fn inspect_is_read_only_and_includes_registered_trash_without_removing_missing_rows() -> Result<()>
{
    let f = Fixture::new()?;
    fs::write(f.root.join("photo.png"), b"photo")?;
    let snapshot = || -> Result<(i64, Vec<(i64, String, i64, i64, i64, i64, i64)>)> {
        let guard = f.library.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let changes = conn.query_row("SELECT total_changes()", [], |r| r.get(0))?;
        let mut query = conn.prepare("SELECT id,rel_path,rating,score,watch_count,is_missing,is_trash FROM media_files ORDER BY id")?;
        let rows = query
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((changes, rows))
    };
    {
        let guard = f.library.conn.lock().unwrap();
        guard.as_ref().unwrap().execute_batch("INSERT INTO media_files(id,rel_path,file_type,file_size,modified_at,rating,score,watch_count,is_trash) VALUES(1,'photo.png','IMAGE',5,1,4,1234,8,1),(2,'missing.mp4','VIDEO',12,1,3,4567,2,0);")?;
    }
    let before = snapshot()?;
    let file_before = fs::read(f.root.join("photo.png"))?;
    f.begin();
    inspect(&f.library, None).map_err(anyhow::Error::msg)?;
    let job = f.library.integrity.snapshot();
    assert_eq!(
        (job.total, job.checked, job.ok, job.issues, job.unavailable),
        (2, 2, 1, 1, 0)
    );
    assert_eq!(job.results[0].state, "missing");
    assert_eq!(job.results[0].file_id, Some(2));
    assert_eq!(snapshot()?, before);
    assert_eq!(fs::read(f.root.join("photo.png"))?, file_before);
    Ok(())
}

#[test]
fn findings_cap_preserves_total_issue_counts() {
    let mut job = Job::default();
    for i in 0..LIMIT + 7 {
        add(
            &mut job,
            finding(
                Some(i as i64),
                "sample",
                if i % 2 == 0 { "missing" } else { "unavailable" },
                "fixture",
            ),
        );
    }
    assert_eq!(job.results.len(), LIMIT);
    assert!(job.results_truncated);
    assert_eq!(job.issues + job.unavailable, (LIMIT + 7) as u64);
    assert_eq!(job.results[0].file_id, Some(0));
    assert_eq!(job.results[LIMIT - 1].file_id, Some((LIMIT - 1) as i64));
}

#[test]
fn cancellation_rejects_late_results_and_keeps_active_guard_until_worker_finishes() -> Result<()> {
    let f = Fixture::new()?;
    f.begin();
    f.library.integrity.active.store(true, Ordering::Release);
    let active = Active(f.library.clone());
    f.library.integrity.stop(Some(TIMEOUT));
    f.library.integrity.update(|job| {
        job.completed = true;
        job.checked += 1;
        add(job, finding(Some(1), "late.png", "missing", "late result"));
    });
    let job = f.library.integrity.snapshot();
    assert!(!job.running && job.cancelled && !job.completed);
    assert_eq!(job.error.as_deref(), Some(TIMEOUT));
    assert!(job.finished_at.is_some());
    assert_eq!(job.checked, 0);
    assert!(job.results.is_empty());
    assert!(f.library.integrity.is_running());
    drop(active);
    assert!(!f.library.integrity.is_running());
    Ok(())
}

#[test]
fn cancelled_or_revoked_library_stops_inspection_before_using_db() -> Result<()> {
    let f = Fixture::new()?;
    f.begin();
    f.library.conn.lock().unwrap().take();
    f.library.integrity.cancel();
    assert!(inspect(&f.library, None).is_ok());
    f.begin();
    f.library.archives.revoke();
    assert!(inspect(&f.library, None).is_ok());
    assert_eq!(f.library.integrity.snapshot().checked, 0);
    Ok(())
}

#[test]
fn watchdog_from_previous_job_cannot_cancel_a_new_job() {
    let runtime = Runtime::default();
    *runtime.job.lock().unwrap() = Job {
        id: 7,
        running: true,
        phase: "files".into(),
        ..Default::default()
    };
    runtime.stop_job(Some(7), Some(TIMEOUT));
    assert!(runtime.snapshot().cancelled);

    runtime.cancelled.store(false, Ordering::Release);
    *runtime.job.lock().unwrap() = Job {
        id: 8,
        running: true,
        phase: "db".into(),
        ..Default::default()
    };
    // An old watchdog may have captured job 7 immediately before job 8 starts.
    runtime.stop_job(Some(7), Some(TIMEOUT));
    let current = runtime.snapshot();
    assert_eq!(current.id, 8);
    assert!(current.running);
    assert!(!current.cancelled);
    assert_eq!(current.phase, "db");
    assert!(current.error.is_none());
    assert!(current.finished_at.is_none());
    assert!(!runtime.cancelled.load(Ordering::Acquire));
    runtime.update(|job| job.checked = 1);
    assert_eq!(runtime.snapshot().checked, 1);
}

#[test]
fn standalone_note_quick_check_validates_note_length_without_an_image() -> Result<()> {
    let f = Fixture::new()?;
    let key = crypto::random::<32>()?;
    let note = crypto::image::Note {
        id: "a".repeat(32),
        title: "memo".into(),
        body: "한글 body".into(),
    };
    let meta = crypto::image::write_note(
        &f.root.join("memo.vtn"),
        "notes/memo.json",
        &note,
        &key,
        |_, _, _| Ok(()),
    )?;
    let mut entry = Entry {
        id: 1,
        name: "notes/memo.json".into(),
        kind: "NOTE".into(),
        size: note.body.len() as i64,
        stored: Some("memo.vtn".into()),
        state: Some("ready".into()),
        hash: Some(meta.sha256),
        restore_name: "notes/memo.json".into(),
    };
    assert!(check_entry(&f.library, &entry, Some(&key)).is_none());
    entry.size += 1;
    assert!(check_entry(&f.library, &entry, Some(&key)).is_some());
    Ok(())
}
