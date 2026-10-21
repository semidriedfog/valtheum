use anyhow::Context;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

// Replace only after a complete sibling file has reached the disk. A failed
// write/rename leaves the previous destination intact, including on Windows.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temp = temporary_sibling(path)?;
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(crate) fn temporary_sibling(path: &Path) -> anyhow::Result<PathBuf> {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(path.with_file_name(format!(
        ".vault-write-{:016x}.tmp",
        u64::from_ne_bytes(random)
    )))
}

#[cfg(test)]
pub fn backup_database(
    conn: &rusqlite::Connection,
    source: &Path,
    destination: &Path,
) -> anyhow::Result<()> {
    backup_database_with_key(conn, source, destination, None)
}

pub fn backup_database_with_key(
    conn: &rusqlite::Connection,
    source: &Path,
    destination: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("백업 폴더가 없습니다.")?
        .canonicalize()?;
    let name = destination
        .file_name()
        .context("백업 파일 이름이 없습니다.")?;
    let target = parent.join(name);
    let lower = name.to_string_lossy().to_ascii_lowercase();
    anyhow::ensure!(
        !lower.starts_with(".media_index"),
        "라이브러리 DB에는 백업을 덮어쓸 수 없습니다."
    );
    anyhow::ensure!(
        !target.try_exists()? || target.canonicalize()? != source.canonicalize()?,
        "현재 사용 중인 DB에는 백업할 수 없습니다."
    );
    let temp = temporary_sibling(&target)?;
    let result = (|| -> anyhow::Result<()> {
        // VACUUM INTO reads a consistent snapshot including committed WAL data.
        if let Some(password) = password {
            crate::encryption::export_verified(conn, &temp, password)?;
        } else {
            conn.execute("VACUUM INTO ?1", [temp.to_string_lossy().as_ref()])?;
        }
        fs::OpenOptions::new().write(true).open(&temp)?.sync_all()?;
        fs::rename(&temp, &target).context("백업 파일 교체 실패: 기존 백업은 보존됩니다.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

// .vtbackup is a ZIP container. The database and vault.key remain encrypted.
#[cfg(test)]
pub fn backup_bundle(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &Path,
    destination: &Path,
    password: Option<&str>,
) -> anyhow::Result<bool> {
    backup_bundle_checked(conn, root, source, destination, password, || Ok(()))
}

pub fn backup_bundle_checked(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &Path,
    destination: &Path,
    password: Option<&str>,
    checkpoint: impl Fn() -> anyhow::Result<()>,
) -> anyhow::Result<bool> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    checkpoint()?;
    let parent = destination
        .parent()
        .context("백업 폴더 없음")?
        .canonicalize()?;
    let target = parent.join(destination.file_name().context("백업 이름 없음")?);
    let root = root.canonicalize()?;
    anyhow::ensure!(
        !target.starts_with(root.join(".valtheum"))
            && !target
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(".media_index"),
        "라이브러리 내부 보안 파일을 덮어쓸 수 없습니다."
    );
    anyhow::ensure!(
        !target.try_exists()? || target.canonicalize()? != source.canonicalize()?,
        "사용 중인 DB를 덮어쓸 수 없습니다."
    );
    let key_path = root.join(crate::archive_crypto::KEY_PATH);
    let required:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM library_info WHERE key='archive_key_sha256') OR EXISTS(SELECT 1 FROM archive_store)",[],|r|r.get(0))?;
    let key = if required || key_path.try_exists()? {
        let key_path = crate::archive_crypto::safe_child(&root, crate::archive_crypto::KEY_PATH)?;
        anyhow::ensure!(
            key_path.metadata()?.len() == 96,
            "vault.key가 없거나 손상되어 백업을 중단했습니다."
        );
        let bytes = fs::read(&key_path)?;
        crate::archive_security::check_key(conn, &bytes)?;
        crate::archive_crypto::read_key(
            &bytes,
            password.context("미디어 키는 암호화 DB에서만 백업할 수 있습니다.")?,
        )?;
        Some(bytes)
    } else {
        None
    };
    let snapshot = temporary_sibling(&target)?;
    let temp = temporary_sibling(&target)?;
    let result = (|| -> anyhow::Result<bool> {
        checkpoint()?;
        backup_database_with_key(conn, source, &snapshot, password)?;
        checkpoint()?;
        let db_name = if password.is_some() {
            crate::encryption::SECURE_DB
        } else {
            crate::encryption::PLAIN_DB
        };
        let file = fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&temp)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        zip.start_file(db_name, options)?;
        let mut snapshot_file = fs::File::open(&snapshot)?;
        let mut buffer = [0u8; 65536];
        loop {
            checkpoint()?;
            let n = snapshot_file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            zip.write_all(&buffer[..n])?;
        }
        drop(snapshot_file);
        if let Some(bytes) = &key {
            zip.start_file(crate::archive_crypto::KEY_PATH, options)?;
            zip.write_all(bytes)?;
        }
        zip.start_file("backup.json", options)?;
        zip.write_all(&serde_json::to_vec_pretty(&serde_json::json!({"format":"valtheum-backup","version":1,"database":db_name,"key_included":key.is_some(),"media_included":false}))?)?;
        zip.start_file("RESTORE.txt", options)?;
        zip.write_all("valtheum DB + key backup (ZIP).\nMedia files are NOT included.\nClose valtheum. Extract into a NEW folder, preserving .valtheum/vault.key.\nCopy separately backed up encrypted media into .valtheum/archives, .valtheum/videos, .valtheum/images and .valtheum/notes, preserving names.\nOpen that folder with valtheum and enter the original password.\nDo not mix this DB with old WAL/SHM files or another library's key.\nWithout the DB, valtheum-restore can restore media and notes using vault.key and the password.\n".as_bytes())?;
        let file = zip.finish()?;
        file.sync_all()?;
        drop(file);
        let mut check = zip::ZipArchive::new(fs::File::open(&temp)?)?;
        let digest = |r: &mut dyn Read| -> anyhow::Result<Vec<u8>> {
            let mut h = Sha256::new();
            let mut b = [0u8; 65536];
            loop {
                checkpoint()?;
                let n = r.read(&mut b)?;
                if n == 0 {
                    break;
                }
                h.update(&b[..n]);
            }
            Ok(h.finalize().to_vec())
        };
        anyhow::ensure!(
            digest(&mut check.by_name(db_name)?)? == digest(&mut fs::File::open(&snapshot)?)?,
            "백업 DB 검증 실패"
        );
        if let Some(bytes) = &key {
            let mut restored = Vec::new();
            check
                .by_name(crate::archive_crypto::KEY_PATH)?
                .read_to_end(&mut restored)?;
            anyhow::ensure!(&restored == bytes, "백업 키 검증 실패");
            anyhow::ensure!(
                fs::read(&key_path)? == *bytes,
                "백업 중 키 파일이 변경되었습니다."
            );
        }
        drop(check);
        checkpoint()?;
        fs::rename(&temp, &target).context("백업 교체 실패: 기존 백업은 보존됩니다.")?;
        Ok(key.is_some())
    })();
    let _ = fs::remove_file(&snapshot);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_bundle_preserves_previous_backup_and_removes_staging_files() -> anyhow::Result<()>
    {
        let root = temporary_sibling(&std::env::temp_dir().join("backup-cancel-fixture"))?;
        fs::create_dir(&root)?;
        let source = root.join(crate::encryption::PLAIN_DB);
        let conn = rusqlite::Connection::open(&source)?;
        crate::db::init_db(&conn)?;
        let target = root.join("existing.vtbackup");
        fs::write(&target, b"previous backup")?;
        for cancel_after_zip in [false, true] {
            let cancelled = std::cell::Cell::new(false);
            let result = backup_bundle_checked(&conn, &root, &source, &target, None, || {
                for entry in fs::read_dir(&root)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|ext| ext == "tmp")
                        && (!cancel_after_zip
                            || zip::ZipArchive::new(fs::File::open(&path)?).is_ok())
                    {
                        cancelled.set(true);
                        anyhow::bail!("synthetic cancellation");
                    }
                }
                Ok(())
            });
            assert!(cancelled.get(), "reach the requested cancellation stage");
            assert!(result.is_err());
            assert_eq!(fs::read(&target)?, b"previous backup");
            assert!(!fs::read_dir(&root)?.any(|entry| entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "tmp")));
        }
        // Cancellation must not poison the source connection or a subsequent retry.
        assert!(!backup_bundle(&conn, &root, &source, &target, None)?);
        assert!(zip::ZipArchive::new(fs::File::open(&target)?).is_ok());
        drop(conn);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn backup_includes_wal_and_preserves_destination_on_failure() -> anyhow::Result<()> {
        let root = temporary_sibling(&std::env::temp_dir().join("fixture"))?;
        fs::create_dir(&root)?;
        let source = root.join(".media_index.db");
        let conn = rusqlite::Connection::open(&source)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; CREATE TABLE records(value); INSERT INTO records VALUES(42);",
        )?;
        let target = root.join("backup.db");
        fs::write(&target, b"old backup")?;
        let reader = rusqlite::Connection::open_with_flags(
            &source,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        backup_database(&reader, &source, &target)?;
        drop(reader);
        let backup = rusqlite::Connection::open(&target)?;
        assert_eq!(
            backup.query_row("SELECT value FROM records", [], |r| r.get::<_, i64>(0))?,
            42
        );
        drop(backup);
        assert!(backup_database(&conn, &source, &source).is_err());
        let old = fs::read(&target)?;
        conn.execute_batch("BEGIN")?;
        assert!(backup_database(&conn, &source, &target).is_err());
        assert_eq!(fs::read(&target)?, old);
        conn.execute_batch("ROLLBACK")?;
        drop(conn);
        fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn atomic_write_replaces_existing_file() -> anyhow::Result<()> {
        let path = temporary_sibling(&std::env::temp_dir().join("config"))?;
        atomic_write(&path, b"before")?;
        atomic_write(&path, b"after")?;
        assert_eq!(fs::read(&path)?, b"after");
        fs::remove_file(path)?;
        Ok(())
    }
}
