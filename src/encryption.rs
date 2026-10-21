//! SQLCipher database lifecycle. Plain media files are deliberately outside this boundary.
use anyhow::{bail, ensure, Context};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;

pub const PLAIN_DB: &str = ".media_index.db";
pub const SECURE_DB: &str = ".media_index.secure.db";

pub struct DbSecurity {
    pub encrypted: bool,
    pub password: Option<Zeroizing<String>>,
}

#[derive(Clone, Serialize)]
pub struct SecurityStatus {
    pub encrypted: bool,
    pub locked: bool,
    pub plaintext_original: bool,
}

impl DbSecurity {
    pub fn status(&self, root: &Path) -> SecurityStatus {
        SecurityStatus {
            encrypted: self.encrypted,
            locked: self.encrypted && self.password.is_none(),
            plaintext_original: self.encrypted && root.join(PLAIN_DB).exists(),
        }
    }
    pub fn path(&self, root: &Path) -> PathBuf {
        root.join(if self.encrypted { SECURE_DB } else { PLAIN_DB })
    }
}

pub fn open(path: &Path, password: Option<&str>, create: bool) -> anyhow::Result<Connection> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let conn = Connection::open_with_flags(path, flags)?;
    let version: String = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .context("SQLCipher 지원이 없는 빌드입니다.")?;
    ensure!(!version.is_empty(), "SQLCipher 지원이 없는 빌드입니다.");
    if let Some(password) = password {
        ensure!(!password.is_empty(), "비밀번호를 입력해 주세요.");
        conn.pragma_update(None, "key", password)?;
    }
    conn.busy_timeout(Duration::from_secs(2))?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .context("비밀번호가 맞지 않거나 DB가 손상되었습니다.")?;
    Ok(conn)
}

pub fn verify(conn: &Connection, encrypted: bool) -> anyhow::Result<()> {
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    ensure!(integrity == "ok", "DB 무결성 검사에 실패했습니다.");
    let mut foreign = conn.prepare("PRAGMA foreign_key_check")?;
    ensure!(
        foreign.query([])?.next()?.is_none(),
        "DB 참조 무결성 검사에 실패했습니다."
    );
    if encrypted {
        let mut cipher = conn.prepare("PRAGMA cipher_integrity_check")?;
        ensure!(
            cipher.query([])?.next()?.is_none(),
            "암호화 무결성 검사에 실패했습니다."
        );
    }
    Ok(())
}

// ATTACH parses a filename string itself. SQLCipher's Windows VFS expects
// a DOS/UNC path rather than Rust canonicalize()'s extended-length prefix.
fn sql_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(dos) = value.strip_prefix(r"\\?\") {
            return dos.to_string();
        }
    }
    value.into_owned()
}

// Keys and paths are bound parameters; never interpolate passwords into SQL.
// A write transaction prevents another process modifying the source mid-export.
pub fn export_verified(
    conn: &Connection,
    destination: &Path,
    password: &str,
) -> anyhow::Result<()> {
    ensure!(!destination.exists(), "출력 DB가 이미 존재합니다.");
    ensure!(conn.is_autocommit(), "DB 작업이 진행 중입니다.");
    // ATTACH inherits CREATE permissions from the source connection. Reserve
    // the new empty file explicitly, while existing secure DBs open without CREATE.
    drop(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?,
    );
    conn.execute(
        "ATTACH DATABASE ?1 AS encrypted KEY ?2",
        rusqlite::params![sql_path(destination), password],
    )?;
    let result = (|| -> anyhow::Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> anyhow::Result<()> {
            conn.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
            let user_version: i64 = conn.query_row("PRAGMA main.user_version", [], |r| r.get(0))?;
            conn.execute_batch(&format!("PRAGMA encrypted.user_version = {user_version}"))?;
            let mut schema = conn
                .prepare("SELECT name FROM main.sqlite_master WHERE type='table' ORDER BY name")?;
            let names = schema
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for name in names {
                let quoted = format!("\"{}\"", name.replace('"', "\"\""));
                let mismatch: bool = conn.query_row(&format!(
                    "SELECT (SELECT count(*) FROM main.{quoted}) != (SELECT count(*) FROM encrypted.{quoted})
                    OR EXISTS(SELECT * FROM main.{quoted} EXCEPT SELECT * FROM encrypted.{quoted})
                    OR EXISTS(SELECT * FROM encrypted.{quoted} EXCEPT SELECT * FROM main.{quoted})"), [], |r| r.get(0))?;
                ensure!(!mismatch, "변환 데이터 검증에 실패했습니다: {name}");
            }
            Ok(())
        })();
        if result.is_ok() {
            conn.execute_batch("COMMIT")?;
        } else {
            let _ = conn.execute_batch("ROLLBACK");
        }
        result
    })();
    if !conn.is_autocommit() {
        let _ = conn.execute_batch("ROLLBACK");
    }
    let detach = conn.execute_batch("DETACH DATABASE encrypted");
    result?;
    detach?;
    let check = open(destination, Some(password), false)?;
    verify(&check, true)?;
    drop(check);
    fs::OpenOptions::new()
        .write(true)
        .open(destination)?
        .sync_all()?;
    Ok(())
}

pub fn convert(conn: &Connection, root: &Path, password: &str) -> anyhow::Result<()> {
    ensure!(
        password.chars().count() >= 10 && password.len() <= 1024,
        "비밀번호는 10자 이상, UTF-8 기준 1024바이트 이하여야 합니다."
    );
    let target = root.join(SECURE_DB);
    ensure!(
        !target.exists(),
        "이미 암호화 DB가 있습니다. 덮어쓰지 않았습니다."
    );
    let temp = crate::persistence::temporary_sibling(&target)?;
    let result = (|| {
        verify(conn, false)?;
        export_verified(conn, &temp, password)?;
        ensure!(!target.exists(), "암호화 DB가 이미 생성되었습니다.");
        fs::rename(&temp, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn remove_plaintext(root: &Path) -> anyhow::Result<()> {
    let source = root.join(PLAIN_DB);
    if !source.exists() {
        return Ok(());
    }
    let conn = open(&source, None, false)?;
    let (busy, _, _): (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    ensure!(busy == 0, "원본 DB를 다른 프로그램이 사용 중입니다.");
    let mode: String = conn.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
    ensure!(mode == "delete", "원본 DB를 다른 프로그램이 사용 중입니다.");
    drop(conn);
    fs::remove_file(source)?;
    Ok(())
}

pub fn require_idle(library: &crate::state::Library) -> anyhow::Result<()> {
    require_idle_except_scan(library)?;
    require_idle_except_recovery(library)
}

pub fn require_idle_except_scan(library: &crate::state::Library) -> anyhow::Result<()> {
    ensure!(
        !library
            .cleanup_pending
            .load(std::sync::atomic::Ordering::Acquire),
        "이전 DB 연결을 정리하는 중입니다. 잠시 후 다시 잠금을 해제해 주세요."
    );
    anyhow::ensure!(!library.integrity.is_running(), crate::integrity::BUSY);
    anyhow::ensure!(
        !library.recovery.is_running(),
        crate::storage_recovery::STILL_RUNNING
    );
    anyhow::ensure!(
        !library
            .archives
            .busy
            .load(std::sync::atomic::Ordering::Acquire),
        "압축 파일 작업을 완료하거나 취소해 주세요."
    );
    Ok(())
}

pub fn require_idle_except_recovery(library: &crate::state::Library) -> anyhow::Result<()> {
    anyhow::ensure!(
        !library
            .archives
            .busy
            .load(std::sync::atomic::Ordering::Acquire),
        "압축 파일 작업을 완료하거나 취소해 주세요."
    );
    if library
        .scan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .running
    {
        bail!("폴더 동기화가 끝난 뒤 다시 시도해 주세요.");
    }
    Ok(())
}

#[cfg(test)]
#[path = "encryption_tests.rs"]
mod tests;
