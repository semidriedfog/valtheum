//! Recoverable two-file password rotation. Media master/file keys never change.
use crate::{archive_crypto as crypto, encryption, persistence};
use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const DIRECTORY: &str = ".valtheum-password-change";
#[derive(Serialize, Deserialize)]
struct Journal {
    version: u32,
    phase: String,
    key: bool,
}
fn folder(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let path = root.join(DIRECTORY);
    if path.try_exists()? {
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink()
                && path.canonicalize()? == root.join(DIRECTORY),
            "비밀번호 변경 임시 경로가 안전하지 않습니다."
        );
    }
    Ok(path)
}
fn write_journal(dir: &Path, phase: &str, key: bool) -> Result<()> {
    persistence::atomic_write(
        &dir.join("journal.json"),
        &serde_json::to_vec(&Journal {
            version: 1,
            phase: phase.into(),
            key,
        })?,
    )
}
fn copy_new(source: &Path, target: &Path) -> Result<()> {
    // The exclusive SQLite lock stabilizes the live DB. A Windows write-denying
    // handle cannot coexist with SQLCipher's already-open read/write handle.
    let mut input = fs::File::open(source).context("비밀번호 변경 사본 읽기 실패")?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    std::io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    Ok(())
}
fn replace_copy(source: &Path, target: &Path) -> Result<()> {
    let temp = persistence::temporary_sibling(target)?;
    let result = (|| {
        copy_new(source, &temp)?;
        fs::rename(&temp, target).context("비밀번호 변경 파일 교체 실패")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn temporary_write(name: &str) -> bool {
    name.strip_prefix(".vault-write-")
        .and_then(|s| s.strip_suffix(".tmp"))
        .is_some_and(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn cleanup(dir: &Path) -> Result<()> {
    // Fixed, owned directory only; never recursively delete an unexpected entry.
    // Validate every entry before removing anything, preserving recovery evidence
    // if the directory contains an unexpected file or link.
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        ensure!(
            entry.file_type()?.is_file()
                && (matches!(
                    name.as_str(),
                    "old.db"
                        | "new.db"
                        | "old.key"
                        | "new.key"
                        | "journal.json"
                        | "new.db-journal"
                        | "new.db-wal"
                        | "new.db-shm"
                ) || temporary_write(&name)),
            "비밀번호 변경 임시 폴더에 알 수 없는 파일이 있습니다."
        );
        if name != "journal.json" {
            files.push(entry.path());
        }
    }
    for file in files {
        fs::remove_file(file)?;
    }
    let marker = dir.join("journal.json");
    if marker.try_exists()? {
        fs::remove_file(marker)?;
    }
    fs::remove_dir(dir)?;
    Ok(())
}

/// Called before opening any library DB. Interrupted publication rolls back both
/// files, while a committed operation only removes its recovery snapshots.
pub fn recover(root: &Path) -> Result<()> {
    let dir = folder(root)?;
    if !dir.try_exists()? {
        return Ok(());
    }
    if fs::read_dir(&dir)?.next().is_none() {
        fs::remove_dir(dir)?;
        return Ok(());
    }
    let marker = dir.join("journal.json");
    if !marker.try_exists()? {
        // Before the first journal is published only its atomic-write scratch
        // file can exist. Never guess a phase when DB/key snapshots also exist.
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            ensure!(
                entry.file_type()?.is_file()
                    && temporary_write(&entry.file_name().to_string_lossy()),
                "비밀번호 변경 복구 기록을 읽을 수 없습니다."
            );
        }
        return cleanup(&dir);
    }
    let mut bytes = Vec::new();
    fs::File::open(marker)?.take(4097).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "비밀번호 변경 기록이 손상되었습니다.");
    let journal: Journal =
        serde_json::from_slice(&bytes).context("비밀번호 변경 복구 기록을 읽을 수 없습니다.")?;
    ensure!(journal.version == 1, "지원하지 않는 비밀번호 변경 기록");
    match journal.phase.as_str() {
        "pending" => {
            // Source was checkpointed and switched out of WAL before publishing.
            // Refuse to discard an unexpected sidecar from another DB writer.
            for suffix in ["-wal", "-journal"] {
                ensure!(!root.join(format!("{}{suffix}",encryption::SECURE_DB)).try_exists()?,"DB 보조 파일이 있어 비밀번호 변경 복구를 중단했습니다. 다른 프로그램을 종료해 주세요.");
            }
            replace_copy(&dir.join("old.db"), &root.join(encryption::SECURE_DB))?;
            if journal.key {
                replace_copy(
                    &dir.join("old.key"),
                    &crypto::safe_child(root, crypto::KEY_PATH)?,
                )?;
            }
            write_journal(&dir, "rolled_back", journal.key)?;
        }
        "preparing" | "committed" | "rolled_back" => {}
        _ => anyhow::bail!("알 수 없는 비밀번호 변경 단계"),
    }
    cleanup(&dir)
}

pub fn change(
    root: &Path,
    connection: &mut Option<Connection>,
    old: &str,
    new: &str,
) -> Result<bool> {
    change_with_hook(root, connection, old, new, |_| Ok(()))
}
fn change_with_hook(
    root: &Path,
    connection: &mut Option<Connection>,
    old: &str,
    new: &str,
    mut hook: impl FnMut(&str) -> Result<()>,
) -> Result<bool> {
    ensure!(
        new.chars().count() >= 10 && new.len() <= 1024,
        "새 비밀번호는 10자 이상, UTF-8 기준 1024바이트 이하여야 합니다."
    );
    ensure!(old != new, "기존 비밀번호와 다른 비밀번호를 입력해 주세요.");
    recover(root)?;
    let source = connection
        .as_ref()
        .context("DB 잠금을 먼저 해제해 주세요.")?;
    let required:bool=source.query_row("SELECT EXISTS(SELECT 1 FROM library_info WHERE key='archive_key_sha256') OR EXISTS(SELECT 1 FROM archive_store)",[],|r|r.get(0))?;
    let key_path = root.join(crypto::KEY_PATH);
    let key = if required || key_path.try_exists()? {
        let path = crypto::safe_child(root, crypto::KEY_PATH)?;
        ensure!(
            path.metadata()?.len() == 96,
            "vault.key가 없거나 손상되었습니다."
        );
        let bytes = fs::read(path)?;
        crate::archive_security::check_key(source, &bytes)?;
        let updated = crypto::rewrap_key(&bytes, old, new)?;
        Some((bytes, updated))
    } else {
        None
    };
    // Verify the caller's current password even for DB-only libraries.
    drop(encryption::open(
        &root.join(encryption::SECURE_DB),
        Some(old),
        false,
    )?);
    encryption::verify(source, true)?;
    let (busy, _, _): (i64, i64, i64) =
        source.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    ensure!(busy == 0, "다른 프로그램이 DB를 사용 중입니다.");
    let mode: String = source.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
    ensure!(mode == "delete", "다른 프로그램이 DB를 사용 중입니다.");
    let dir = folder(root)?;
    crypto::check_space(
        root,
        fs::metadata(root.join(encryption::SECURE_DB))?
            .len()
            .saturating_mul(3)
            .saturating_add(10485760),
    )?;
    fs::create_dir(&dir)?;
    let mut committed = false;
    let result = (|| -> Result<bool> {
        write_journal(&dir, "preparing", key.is_some())?;
        let source = connection.as_ref().context("DB 연결 없음")?;
        source.execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE; COMMIT;")?;
        copy_new(&root.join(encryption::SECURE_DB), &dir.join("old.db"))?;
        let old_check = encryption::open(&dir.join("old.db"), Some(old), false)?;
        encryption::verify(&old_check, true)?;
        drop(old_check);
        encryption::export_verified(source, &dir.join("new.db"), new)?;
        if let Some((old_key, new_key)) = &key {
            persistence::atomic_write(&dir.join("old.key"), old_key)?;
            persistence::atomic_write(&dir.join("new.key"), new_key)?;
            let new_db = encryption::open(&dir.join("new.db"), Some(new), false)?;
            new_db.execute(
                "INSERT OR REPLACE INTO library_info(key,value) VALUES('archive_key_sha256',?1)",
                [crypto::hex(&Sha256::digest(new_key))],
            )?;
            encryption::verify(&new_db, true)?;
            drop(new_db);
            fs::OpenOptions::new()
                .write(true)
                .open(dir.join("new.db"))?
                .sync_all()?;
        }
        hook("prepared")?;
        write_journal(&dir, "pending", key.is_some())?;
        // Close every app-owned connection before replacing the live DB.
        connection.take();
        replace_copy(&dir.join("new.db"), &root.join(encryption::SECURE_DB))?;
        hook("database")?;
        if key.is_some() {
            replace_copy(
                &dir.join("new.key"),
                &crypto::safe_child(root, crypto::KEY_PATH)?,
            )?;
        }
        hook("key")?;
        let check = encryption::open(&root.join(encryption::SECURE_DB), Some(new), false)?;
        encryption::verify(&check, true)?;
        if let Some((_, new_key)) = &key {
            crate::archive_security::check_key(&check, new_key)?;
        }
        write_journal(&dir, "committed", key.is_some())?;
        committed = true;
        *connection = Some(check);
        hook("committed")?;
        // Publication succeeded even if the OS temporarily blocks cleanup.
        Ok(cleanup(&dir).is_ok())
    })();
    if result.is_err() && !committed {
        connection.take();
        recover(root)
            .context("비밀번호 변경을 복구하지 못했습니다. 임시 복구 파일을 보존했습니다.")?;
        *connection = Some(encryption::open(
            &root.join(encryption::SECURE_DB),
            Some(old),
            false,
        )?);
    }
    if let Some(conn) = connection.as_ref() {
        let _ = conn.execute_batch("PRAGMA locking_mode=NORMAL; PRAGMA journal_mode=WAL;");
    }
    result
}

#[cfg(test)]
#[path = "password_change_tests.rs"]
mod tests;
