use crate::db;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

const MAX_SCAN_DEPTH: usize = 128;

#[derive(Debug, Default, Serialize)]
pub struct ScanReport {
    pub scanned_count: usize,
    pub new_or_updated: usize,
    pub removed_count: usize,
    pub missing_count: usize,
    pub errors: Vec<String>,
}

#[cfg(test)]
pub fn scan_directory<P: AsRef<Path>>(
    root_dir: P,
    conn: &Connection,
) -> anyhow::Result<ScanReport> {
    scan_directory_with_progress(root_dir, conn, |_| {})
}

#[cfg(test)]
pub fn scan_directory_with_progress<P: AsRef<Path>, F: Fn(usize)>(
    root_dir: P,
    conn: &Connection,
    progress: F,
) -> anyhow::Result<ScanReport> {
    scan_directory_checked(root_dir, conn, progress, || Ok(()))
}

pub fn scan_directory_checked<P: AsRef<Path>, F: Fn(usize), C: Fn() -> anyhow::Result<()>>(
    root_dir: P,
    conn: &Connection,
    progress: F,
    check: C,
) -> anyhow::Result<ScanReport> {
    check()?;
    let root = root_dir.as_ref();
    if !root.exists() {
        return Ok(ScanReport {
            errors: vec![format!("{}: root directory does not exist", root.display())],
            ..ScanReport::default()
        });
    }

    // Remember identities before filesystem traversal. A concurrent permanent
    // deletion must not be undone by observations collected earlier in the scan.
    let initial_ids: HashMap<String, i64> = conn
        .prepare("SELECT rel_path, id FROM media_files")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut found_rel_paths = HashSet::new();
    let mut new_or_updated = 0;
    let mut errors = Vec::new();
    let mut visited_dirs = HashSet::new();
    let mut discovered_files = Vec::new();
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(err) => {
            return Ok(ScanReport {
                errors: vec![format!(
                    "{}: cannot resolve root directory: {err}",
                    root.display()
                )],
                ..ScanReport::default()
            });
        }
    };
    {
        let mut context = ScanContext {
            found_rel_paths: &mut found_rel_paths,
            errors: &mut errors,
            visited_dirs: &mut visited_dirs,
            discovered_files: &mut discovered_files,
            progress: &progress,
            check: &check,
        };
        scan_recursive(root, &canonical_root, root, 0, &mut context)?;
    }

    check()?;
    // Encrypted records retain their logical name even after original removal.
    let protected = crate::archive_security::protected_paths(conn)?;
    discovered_files.retain(|file| !protected.contains(&file.rel_path));
    found_rel_paths.extend(protected);

    // Keep the transaction short: a scan can take minutes on a large library.
    // Holding it during filesystem I/O makes every other DB operation wait.
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    for discovered in discovered_files {
        check()?;
        if let Some(id) = initial_ids.get(&discovered.rel_path) {
            let still_present: bool = tx
                .prepare_cached(
                    "SELECT EXISTS(SELECT 1 FROM media_files WHERE id=?1 AND rel_path=?2)
                 AND NOT EXISTS(SELECT 1 FROM delete_intents WHERE file_id=?1)",
                )?
                .query_row(rusqlite::params![id, discovered.rel_path], |r| r.get(0))?;
            if !still_present {
                found_rel_paths.remove(&discovered.rel_path);
                continue;
            }
        }
        new_or_updated += db::upsert_media_file(
            &tx,
            &discovered.rel_path,
            discovered.file_type,
            discovered.file_size,
            discovered.modified_at,
        )?;
    }

    check()?;

    // Keep discoveries and missing flags atomic when a retired scan is cancelled.
    let existing_db_files = db::get_all_active_rel_paths(conn)?;
    let mut missing_ids = Vec::new();

    for (id, rel_path) in existing_db_files {
        check()?;
        if !found_rel_paths.contains(&rel_path) {
            missing_ids.push(id);
        }
    }

    let scan_complete = errors.is_empty();
    // A DB-only restore precedes copying media. Preserve its metadata until all
    // ordinary media have been found in a successful scan.
    let restoring: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM library_info WHERE key='restore_pending_media')",
        [],
        |r| r.get(0),
    )?;
    if restoring {
        if !missing_ids.is_empty() {
            errors.push(format!("DB 백업 복원 후 미디어 {}개가 아직 없습니다. 원래 경로에 미디어를 복사한 뒤 동기화해 주세요. 기존 기록은 보존됩니다.", missing_ids.len()));
        } else if errors.is_empty() {
            conn.execute(
                "DELETE FROM library_info WHERE key='restore_pending_media'",
                [],
            )?;
        }
    }

    // Missing media keep their identity and all user metadata until explicitly deleted.
    if scan_complete {
        for id in &missing_ids {
            check()?;
            tx.prepare_cached("UPDATE media_files SET is_missing=1 WHERE id=?1 AND is_missing=0")?
                .execute([id])?;
        }
        for path in &found_rel_paths {
            check()?;
            tx.prepare_cached(
                "UPDATE media_files SET is_missing=0 WHERE rel_path=?1 AND is_missing<>0",
            )?
            .execute([path])?;
        }
    }
    check()?;
    tx.commit()?;
    let removed_count = 0;

    println!(
        "[Scanner] Finished {:?}: {} files on disk, {} updated/new, {} missing records preserved",
        root,
        found_rel_paths.len(),
        new_or_updated,
        missing_ids.len()
    );

    Ok(ScanReport {
        scanned_count: found_rel_paths.len(),
        new_or_updated,
        removed_count,
        missing_count: missing_ids.len(),
        errors,
    })
}

struct ScanContext<'a, F: Fn(usize)> {
    found_rel_paths: &'a mut HashSet<String>,
    errors: &'a mut Vec<String>,
    visited_dirs: &'a mut HashSet<std::path::PathBuf>,
    discovered_files: &'a mut Vec<DiscoveredFile>,
    progress: &'a F,
    check: &'a dyn Fn() -> anyhow::Result<()>,
}

struct DiscoveredFile {
    rel_path: String,
    file_type: &'static str,
    file_size: i64,
    modified_at: i64,
}

fn scan_recursive<F: Fn(usize)>(
    scan_root: &Path,
    canonical_root: &Path,
    current: &Path,
    depth: usize,
    context: &mut ScanContext<'_, F>,
) -> anyhow::Result<()> {
    (context.check)()?;
    if depth > MAX_SCAN_DEPTH {
        context.errors.push(format!(
            "{}: maximum directory depth ({MAX_SCAN_DEPTH}) exceeded",
            current.display()
        ));
        return Ok(());
    }
    let canonical = match fs::canonicalize(current) {
        Ok(path) => path,
        Err(err) => {
            context.errors.push(format!(
                "{}: cannot resolve directory: {err}",
                current.display()
            ));
            return Ok(());
        }
    };
    if !canonical.starts_with(canonical_root) {
        context.errors.push(format!(
            "{}: directory is outside scan root",
            current.display()
        ));
        return Ok(());
    }
    if !context.visited_dirs.insert(canonical) {
        context
            .errors
            .push(format!("{}: directory cycle skipped", current.display()));
        return Ok(());
    }

    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(err) => {
            context.errors.push(format!(
                "{}: cannot read directory: {err}",
                current.display()
            ));
            return Ok(());
        }
    };
    for entry_result in entries {
        (context.check)()?;
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(err) => {
                context.errors.push(format!(
                    "{}: cannot read directory entry: {err}",
                    current.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        // Inspect the entry type first. For symlinks, follow the link once
        // so files and directories retain the scanner's historical behavior.
        let entry_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                context.errors.push(format!(
                    "{}: cannot inspect file type: {err}",
                    path.display()
                ));
                continue;
            }
        };
        let followed_metadata = if entry_type.is_symlink() {
            match fs::metadata(&path) {
                Ok(metadata) => Some(metadata),
                Err(err) => {
                    context
                        .errors
                        .push(format!("{}: cannot resolve symlink: {err}", path.display()));
                    continue;
                }
            }
        } else {
            None
        };
        if entry_type.is_symlink() {
            let target = match fs::canonicalize(&path) {
                Ok(target) => target,
                Err(err) => {
                    context
                        .errors
                        .push(format!("{}: cannot resolve symlink: {err}", path.display()));
                    continue;
                }
            };
            if !target.starts_with(canonical_root) {
                context.errors.push(format!(
                    "{}: symlink target is outside scan root",
                    path.display()
                ));
                continue;
            }
        }
        let is_dir = followed_metadata
            .as_ref()
            .map_or(entry_type.is_dir(), fs::Metadata::is_dir);
        let is_file = followed_metadata
            .as_ref()
            .map_or(entry_type.is_file(), fs::Metadata::is_file);

        if is_dir {
            // 숨김 폴더 패스
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }
            scan_recursive(scan_root, canonical_root, &path, depth + 1, context)?;
        } else if is_file {
            // DB 파일 자체 제외
            if path.file_name().and_then(|n| n.to_str()) == Some(".media_index.db") {
                continue;
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let file_type = match ext.as_str() {
                "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" => "VIDEO",
                "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif" => "IMAGE",
                "pdf" => "PDF",
                "zip" | "rar" | "7z" => "ARCHIVE",
                "zst" if crate::archive::is_tar_zstd(&path) => "ARCHIVE",
                _ => continue,
            };

            let rel_path = path
                .strip_prefix(scan_root)?
                .to_string_lossy()
                .replace('\\', "/");

            context.found_rel_paths.insert(rel_path.clone());

            let metadata = match followed_metadata {
                Some(metadata) => metadata,
                None => match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(err) => {
                        context
                            .errors
                            .push(format!("{}: cannot read metadata: {err}", path.display()));
                        continue;
                    }
                },
            };
            let file_size = metadata.len() as i64;
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            context.discovered_files.push(DiscoveredFile {
                rel_path,
                file_type,
                file_size,
                modified_at,
            });
            (context.progress)(context.found_rel_paths.len());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vault-scanner-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn cancelled_scan_never_commits_discoveries_or_missing_flags() -> anyhow::Result<()> {
        let root = test_root();
        fs::create_dir_all(&root)?;
        fs::write(root.join("new.png"), b"synthetic")?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        db::upsert_media_file(&conn, "old.png", "IMAGE", 4, 1)?;
        let cancelled = std::cell::Cell::new(false);
        let result = scan_directory_checked(
            &root,
            &conn,
            |_| cancelled.set(true),
            || {
                anyhow::ensure!(!cancelled.get(), "cancelled during discovery");
                Ok(())
            },
        );
        assert!(result.is_err());
        let rolled_back = std::cell::Cell::new(false);
        let result = scan_directory_checked(
            &root,
            &conn,
            |_| {},
            || {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM media_files WHERE rel_path='new.png'",
                    [],
                    |r| r.get(0),
                )?;
                if count > 0 {
                    rolled_back.set(true);
                    anyhow::bail!("cancelled before commit");
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(rolled_back.get());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM media_files", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT is_missing FROM media_files WHERE rel_path='old.png'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        fs::remove_file(root.join("new.png"))?;
        fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn repeated_scan_reports_zero_and_preserves_user_metadata() -> anyhow::Result<()> {
        let root = test_root();
        fs::create_dir_all(&root)?;
        fs::write(root.join("clip.mp4"), b"initial")?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;

        let first = scan_directory(&root, &conn)?;
        assert_eq!(first.scanned_count, 1);
        assert_eq!(first.new_or_updated, 1);

        conn.execute(
            "UPDATE media_files SET rating = 5, score = 42 WHERE rel_path = 'clip.mp4'",
            [],
        )?;
        let second = scan_directory(&root, &conn)?;
        assert_eq!(second.new_or_updated, 0);
        let metadata: (i64, i64) = conn.query_row(
            "SELECT rating, score FROM media_files WHERE rel_path = 'clip.mp4'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(metadata, (5, 42));

        fs::write(root.join("clip.mp4"), b"changed size")?;
        let changed = scan_directory(&root, &conn)?;
        assert_eq!(changed.scanned_count, 1);
        assert_eq!(changed.new_or_updated, 1);
        let preserved_rating: i64 = conn.query_row(
            "SELECT rating FROM media_files WHERE rel_path = 'clip.mp4'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(preserved_rating, 5);

        fs::remove_file(root.join("clip.mp4"))?;
        let removed = scan_directory(&root, &conn)?;
        assert_eq!(removed.scanned_count, 0);
        assert_eq!(removed.new_or_updated, 0);
        assert_eq!(removed.removed_count, 0);
        let remaining: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_files", [], |row| row.get(0))?;
        assert_eq!(remaining, 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn incomplete_scan_preserves_records_when_root_is_not_a_directory() -> anyhow::Result<()> {
        let root = test_root();
        fs::write(&root, b"this is a file")?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        db::upsert_media_file(&conn, "clip.mp4", "VIDEO", 1, 1)?;
        conn.execute(
            "UPDATE media_files SET rating = 5 WHERE rel_path = 'clip.mp4'",
            [],
        )?;

        let report = scan_directory(&root, &conn)?;
        assert!(!report.errors.is_empty());
        assert_eq!(report.removed_count, 0);
        let rating: i64 = conn.query_row(
            "SELECT rating FROM media_files WHERE rel_path = 'clip.mp4'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rating, 5);
        fs::remove_file(root)?;
        Ok(())
    }

    #[test]
    fn missing_root_is_incomplete_and_preserves_records() -> anyhow::Result<()> {
        let root = test_root();
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        db::upsert_media_file(&conn, "clip.mp4", "VIDEO", 1, 1)?;

        let report = scan_directory(&root, &conn)?;
        assert!(!report.errors.is_empty());
        assert_eq!(report.removed_count, 0);
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_files", [], |row| row.get(0))?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlink_directory_cycle_is_reported_and_does_not_recurse_forever() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;
        let root = test_root();
        fs::create_dir_all(root.join("nested"))?;
        symlink(&root, root.join("nested/loop"))?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;

        let report = scan_directory(&root, &conn)?;
        assert!(report.errors.iter().any(|error| error.contains("cycle")));
        fs::remove_file(root.join("nested/loop"))?;
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_root_is_skipped_and_marks_scan_incomplete() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;
        let root = test_root();
        let outside = test_root();
        fs::create_dir_all(&root)?;
        fs::create_dir_all(&outside)?;
        fs::write(outside.join("secret.mp4"), b"outside")?;
        symlink(outside.join("secret.mp4"), root.join("linked.mp4"))?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;

        let report = scan_directory(&root, &conn)?;
        assert_eq!(report.scanned_count, 0);
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("outside scan root")));
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM media_files", [], |row| row
                .get::<_, i64>(0))?,
            0
        );

        fs::remove_file(root.join("linked.mp4"))?;
        fs::remove_dir_all(root)?;
        fs::remove_dir_all(outside)?;
        Ok(())
    }

    #[test]
    fn concurrent_permanent_delete_is_not_reimported_by_stale_scan() -> anyhow::Result<()> {
        let root = test_root();
        fs::create_dir_all(&root)?;
        fs::write(root.join("clip.mp4"), b"clip")?;
        let path = root.join("scanner-test.sqlite");
        let conn = Connection::open(&path)?;
        db::init_db(&conn)?;
        scan_directory(&root, &conn)?;
        let id: i64 = conn.query_row("SELECT id FROM media_files", [], |r| r.get(0))?;
        db::move_to_trash(&conn, id)?;
        let other = Connection::open(&path)?;
        db::init_db(&other)?;
        let deleted = std::cell::Cell::new(false);
        let report = scan_directory_with_progress(&root, &conn, |_| {
            if !deleted.replace(true) {
                db::permanently_delete_file(&other, &root, id).unwrap();
                // Clearing visible deletion history must not undo the protection.
                db::clear_deleted_logs(&other).unwrap();
            }
        })?;
        assert!(deleted.get());
        assert_eq!(report.new_or_updated, 0);
        assert_eq!(report.scanned_count, 0);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM media_files", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert!(!root.join("clip.mp4").exists());
        // A file deliberately copied back after that scan can be imported again.
        fs::write(root.join("clip.mp4"), b"replacement")?;
        assert_eq!(scan_directory(&root, &conn)?.new_or_updated, 1);
        drop(other);
        drop(conn);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn progress_callback_can_write_with_second_connection() -> anyhow::Result<()> {
        use std::cell::Cell;
        let root = test_root();
        fs::create_dir_all(&root)?;
        fs::write(root.join("clip.mp4"), b"clip")?;
        let database_path = root.join("scanner-test.sqlite");
        let conn = Connection::open(&database_path)?;
        db::init_db(&conn)?;
        let callback_conn = Connection::open(&database_path)?;
        db::init_db(&callback_conn)?;
        let callback_succeeded = Cell::new(false);

        let report = scan_directory_with_progress(&root, &conn, |_| {
            callback_succeeded.set(
                callback_conn
                    .execute_batch("PRAGMA user_version = 7")
                    .is_ok(),
            );
        })?;
        assert_eq!(report.scanned_count, 1);
        assert!(callback_succeeded.get());
        drop(callback_conn);
        drop(conn);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn excessively_deep_directory_tree_is_reported_as_incomplete() -> anyhow::Result<()> {
        let root = test_root();
        let mut current = root.clone();
        for _ in 0..(MAX_SCAN_DEPTH + 1) {
            current.push("d");
            fs::create_dir_all(&current)?;
        }
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        let report = scan_directory(&root, &conn)?;
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("maximum directory depth")));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
