use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::path::Component;

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaFile {
    pub id: i64,
    pub rel_path: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub restore_name: Option<String>,
    pub file_type: String,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub missing: bool,
    pub file_size: i64,
    pub modified_at: i64,
    pub duration_secs: Option<i64>,
    pub rating: i64,
    pub score: i64,
    pub last_watched_at: Option<i64>,
    pub watch_count: i64,
    pub is_trash: bool,
    pub created_at: String,
    pub thumbnail_data: Option<String>, // Versioned thumbnail endpoint; no image bytes in lists.
    pub time_offset: Option<f64>,
    pub page_offset: Option<i64>,
    pub tags: Vec<Tag>,
    pub series_id: Option<i64>,
    pub series_name: Option<String>,
    pub series_position: Option<i64>,
    pub series_representative_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeletedFileLog {
    pub id: i64,
    pub original_rel_path: String,
    pub file_type: String,
    pub file_size: i64,
    pub score: i64,
    pub tags_info: Option<String>,
    pub deleted_at: i64,
    pub deleted_time_str: String,
}

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS library_info (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS media_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rel_path TEXT NOT NULL UNIQUE,
            file_type TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            modified_at INTEGER NOT NULL,
            duration_secs INTEGER,
            rating INTEGER DEFAULT 0,
            score INTEGER DEFAULT 1000,
            last_watched_at INTEGER,
            watch_count INTEGER DEFAULT 0,
            is_trash INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS archive_store (
            file_id INTEGER PRIMARY KEY REFERENCES media_files(id) ON DELETE CASCADE,
            encrypted_path TEXT NOT NULL UNIQUE,
            state TEXT NOT NULL CHECK(state IN ('pending','ready')),
            sha256 TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS archive_gc (encrypted_path TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS note_save_requests (
            request_id TEXT PRIMARY KEY,
            fingerprint TEXT NOT NULL,
            file_id INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS media_series (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            representative_id INTEGER REFERENCES media_files(id) ON DELETE SET NULL,
            rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5),
            score INTEGER NOT NULL DEFAULT 1000
        );
        CREATE TABLE IF NOT EXISTS series_members (
            file_id INTEGER PRIMARY KEY REFERENCES media_files(id) ON DELETE CASCADE,
            series_id INTEGER NOT NULL REFERENCES media_series(id) ON DELETE CASCADE,
            position INTEGER NOT NULL CHECK(position > 0),
            UNIQUE(series_id, position)
        );
        CREATE INDEX IF NOT EXISTS series_members_series ON series_members(series_id);

        CREATE TABLE IF NOT EXISTS thumbnails (
            file_id INTEGER PRIMARY KEY,
            data BLOB NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            revision INTEGER NOT NULL DEFAULT 0,
            time_offset REAL,
            page_offset INTEGER,
            FOREIGN KEY(file_id) REFERENCES media_files(id) ON DELETE CASCADE
        );
 
        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS file_tags (
            file_id INTEGER,
            tag_id INTEGER,
            PRIMARY KEY(file_id, tag_id),
            FOREIGN KEY(file_id) REFERENCES media_files(id) ON DELETE CASCADE,
            FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS deleted_files_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            original_rel_path TEXT NOT NULL,
            file_type TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            score INTEGER NOT NULL,
            tags_info TEXT,
            deleted_at INTEGER NOT NULL,
            deleted_time_str DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS delete_intents (
            file_id INTEGER PRIMARY KEY,
            rel_path TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS media_bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id INTEGER NOT NULL,
            position REAL NOT NULL,
            thumbnail_data BLOB NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(file_id, position),
            FOREIGN KEY(file_id) REFERENCES media_files(id) ON DELETE CASCADE
        );
        ",
    )?;

    // Safe migration for existing databases. Duplicate-column errors are expected
    // on subsequent opens; all other errors must be surfaced to the caller.
    for sql in [
        "ALTER TABLE media_files ADD COLUMN display_name TEXT",
        "ALTER TABLE archive_store ADD COLUMN restore_name TEXT",
        "ALTER TABLE media_files ADD COLUMN is_missing INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE media_files ADD COLUMN score INTEGER DEFAULT 1000",
        "ALTER TABLE media_files ADD COLUMN last_watched_at INTEGER",
        "ALTER TABLE media_files ADD COLUMN watch_count INTEGER DEFAULT 0",
        "ALTER TABLE media_files ADD COLUMN pdf_last_page INTEGER CHECK(pdf_last_page >= 1)",
        "ALTER TABLE media_files ADD COLUMN pdf_position_revision INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE media_files ADD COLUMN is_trash INTEGER DEFAULT 0",
        "ALTER TABLE thumbnails ADD COLUMN time_offset REAL",
        "ALTER TABLE thumbnails ADD COLUMN page_offset INTEGER",
        "ALTER TABLE thumbnails ADD COLUMN revision INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE media_bookmarks ADD COLUMN request_id TEXT",
    ] {
        if let Err(error) = conn.execute(sql, []) {
            let duplicate = error
                .to_string()
                .to_ascii_lowercase()
                .contains("duplicate column name");
            if !duplicate {
                return Err(error);
            }
        }
    }

    // Seed existing series once. Creating the table and importing old tags are
    // atomic, so reopening never resurrects a removed shared tag.
    let tx = conn.unchecked_transaction()?;
    let has_series_tags: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='series_tags')",
        [],
        |row| row.get(0),
    )?;
    if !has_series_tags {
        tx.execute_batch("CREATE TABLE series_tags (
            series_id INTEGER NOT NULL REFERENCES media_series(id) ON DELETE CASCADE,
            tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY(series_id, tag_id)
        );
        INSERT OR IGNORE INTO series_tags(series_id,tag_id)
            SELECT m.series_id, ft.tag_id FROM series_members m JOIN file_tags ft ON ft.file_id=m.file_id;")?;
    }
    tx.commit()?;
    Ok(())
}

pub fn upsert_media_file(
    conn: &Connection,
    rel_path: &str,
    file_type: &str,
    file_size: i64,
    modified_at: i64,
) -> Result<usize> {
    conn.prepare_cached(
        "INSERT INTO media_files (rel_path, file_type, file_size, modified_at, score, is_trash)
         SELECT ?1, ?2, ?3, ?4, 1000, 0
         WHERE NOT EXISTS (
             SELECT 1 FROM media_files
             WHERE rel_path = ?1 AND file_size = ?3 AND modified_at = ?4 AND is_missing = 0
         )
         ON CONFLICT(rel_path) DO UPDATE SET
            file_size = excluded.file_size,
            modified_at = excluded.modified_at, is_missing = 0",
    )?
    .execute(params![rel_path, file_type, file_size, modified_at])
}

pub fn get_all_active_rel_paths(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.rel_path FROM media_files f
         WHERE f.is_trash = 0 AND NOT EXISTS
           (SELECT 1 FROM delete_intents d WHERE d.file_id = f.id)",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

#[cfg(test)]
pub fn remove_missing_file_records(conn: &Connection, missing_ids: &[i64]) -> Result<usize> {
    if missing_ids.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut deleted_count = 0;
    for &id in missing_ids {
        tx.execute(
            "DELETE FROM media_bookmarks WHERE file_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM thumbnails WHERE file_id = ?1", params![id])?;
        tx.execute("DELETE FROM file_tags WHERE file_id = ?1", params![id])?;
        let affected = tx.execute("DELETE FROM media_files WHERE id = ?1", params![id])?;
        deleted_count += affected;
    }
    tx.commit()?;
    Ok(deleted_count)
}

pub fn update_duration(conn: &Connection, file_id: i64, duration_secs: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_files SET duration_secs = ?1 WHERE id = ?2",
        params![duration_secs, file_id],
    )?;
    Ok(())
}

pub fn update_last_watched(conn: &Connection, file_id: i64, timestamp: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_files SET last_watched_at = ?1, watch_count = COALESCE(watch_count, 0) + 1 WHERE id = ?2",
        params![timestamp, file_id],
    )?;
    Ok(())
}

pub fn reset_watch_count(conn: &Connection, file_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_files SET watch_count = 0, last_watched_at = NULL WHERE id = ?1",
        params![file_id],
    )?;
    Ok(())
}

pub fn reset_all_watch_counts(conn: &Connection) -> Result<usize> {
    let affected = conn.execute(
        "UPDATE media_files SET watch_count = 0, last_watched_at = NULL",
        [],
    )?;
    Ok(affected)
}

pub fn reset_all_scores(conn: &Connection) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let affected = tx.execute("UPDATE media_files SET score = 1000", [])?;
    tx.execute("UPDATE media_series SET score = 1000", [])?;
    tx.commit()?;
    Ok(affected)
}

pub fn update_rating(conn: &Connection, file_id: i64, rating: i64) -> Result<()> {
    let clamped = rating.clamp(0, 5);
    let series_id: Option<i64> = conn
        .query_row(
            "SELECT series_id FROM series_members WHERE file_id=?1",
            [file_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = series_id {
        conn.execute(
            "UPDATE media_series SET rating=?1 WHERE id=?2",
            params![clamped, id],
        )?;
    } else {
        conn.execute(
            "UPDATE media_files SET rating=?1 WHERE id=?2",
            params![clamped, file_id],
        )?;
    }
    Ok(())
}

pub fn reset_all_ratings(conn: &Connection) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let affected = tx.execute("UPDATE media_files SET rating = 0", [])?;
    tx.execute("UPDATE media_series SET rating = 0", [])?;
    tx.commit()?;
    Ok(affected)
}

pub fn save_thumbnail(
    conn: &Connection,
    file_id: i64,
    data: &[u8],
    width: i32,
    height: i32,
    time_offset: Option<f64>,
    page_offset: Option<i64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO thumbnails (file_id, data, width, height, revision, time_offset, page_offset)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)
         ON CONFLICT(file_id) DO UPDATE SET
            data = excluded.data,
            width = excluded.width,
            height = excluded.height,
            revision = thumbnails.revision + 1,
            time_offset = excluded.time_offset,
            page_offset = excluded.page_offset",
        params![file_id, data, width, height, time_offset, page_offset],
    )?;
    Ok(())
}

// Standalone files retain their own tags while series members share one set.
const EFFECTIVE_TAGS: &str = "SELECT ft.file_id, ft.tag_id FROM file_tags ft
    WHERE NOT EXISTS (SELECT 1 FROM series_members m WHERE m.file_id=ft.file_id)
    UNION ALL
    SELECT m.file_id, st.tag_id FROM series_members m JOIN series_tags st ON st.series_id=m.series_id";

pub fn get_tags_for_file(conn: &Connection, file_id: i64) -> Result<Vec<Tag>> {
    let sql = format!(
        "SELECT t.id,t.name FROM ({EFFECTIVE_TAGS}) ft
        JOIN tags t ON t.id=ft.tag_id WHERE ft.file_id=?1 ORDER BY t.name ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let tags = stmt
        .query_map([file_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(tags)
}

pub fn add_tag_to_file(conn: &Connection, file_id: i64, tag_name: &str) -> Result<Tag> {
    let clean_name = tag_name.trim().trim_start_matches('#');
    if clean_name.is_empty() {
        return Err(rusqlite::Error::ExecuteReturnedResults);
    }
    let tx = conn.unchecked_transaction()?;
    let series_id: Option<i64> = tx.query_row(
        "SELECT m.series_id FROM media_files f LEFT JOIN series_members m ON m.file_id=f.id WHERE f.id=?1",
        [file_id], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO tags(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
        [clean_name],
    )?;
    let tag_id: i64 = tx.query_row("SELECT id FROM tags WHERE name=?1", [clean_name], |r| {
        r.get(0)
    })?;
    if let Some(series_id) = series_id {
        tx.execute(
            "INSERT OR IGNORE INTO series_tags(series_id,tag_id) VALUES (?1,?2)",
            params![series_id, tag_id],
        )?;
    } else {
        tx.execute(
            "INSERT OR IGNORE INTO file_tags(file_id,tag_id) VALUES (?1,?2)",
            params![file_id, tag_id],
        )?;
    }
    tx.commit()?;
    Ok(Tag {
        id: tag_id,
        name: clean_name.to_string(),
    })
}

pub fn remove_tag_from_file(conn: &Connection, file_id: i64, tag_id: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let series_id: Option<i64> = tx
        .query_row(
            "SELECT series_id FROM series_members WHERE file_id=?1",
            [file_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(series_id) = series_id {
        tx.execute(
            "DELETE FROM series_tags WHERE series_id=?1 AND tag_id=?2",
            params![series_id, tag_id],
        )?;
    } else {
        tx.execute(
            "DELETE FROM file_tags WHERE file_id=?1 AND tag_id=?2",
            params![file_id, tag_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

const MEDIA_SELECT: &str = "SELECT f.id, f.rel_path, f.file_type, f.file_size, f.modified_at, f.duration_secs, COALESCE(s.rating, f.rating, 0), COALESCE(s.score, f.score, 1000) as score, f.last_watched_at, COALESCE(f.is_trash, 0) as is_trash, f.created_at, t.revision, COALESCE(f.watch_count, 0) as watch_count, t.time_offset, t.page_offset, s.id, s.name, m.position, s.representative_id,
         (f.file_type IN ('ARCHIVE','VIDEO','IMAGE','NOTE') AND EXISTS (SELECT 1 FROM archive_store a WHERE a.file_id = f.id AND a.state = 'ready')), f.is_missing, f.display_name, (SELECT a.restore_name FROM archive_store a WHERE a.file_id=f.id)
         FROM media_files f
         LEFT JOIN thumbnails t ON f.id = t.file_id
         LEFT JOIN series_members m ON m.file_id = f.id
         LEFT JOIN media_series s ON s.id = m.series_id";

fn map_media_row(conn: &Connection, row: &rusqlite::Row) -> rusqlite::Result<MediaFile> {
    let mut file = map_media_fields(row)?;
    file.tags = get_tags_for_file(conn, file.id)?;
    Ok(file)
}

fn map_media_fields(row: &rusqlite::Row) -> Result<MediaFile> {
    let file_id: i64 = row.get(0)?;
    let thumbnail_revision: Option<i64> = row.get(11)?;
    let thumbnail_data =
        thumbnail_revision.map(|revision| format!("/api/thumbnail/{file_id}?v={revision}"));

    let is_trash_val: i64 = row.get(9).unwrap_or(0);
    let watch_count: i64 = row.get(12).unwrap_or(0);
    let time_offset: Option<f64> = row.get(13).ok();
    let page_offset: Option<i64> = row.get(14).ok();

    Ok(MediaFile {
        id: file_id,
        rel_path: row.get(1)?,
        display_name: row.get(21)?,
        restore_name: row.get(22)?,
        file_type: row.get(2)?,
        encrypted: row.get(19)?,
        missing: row.get(20)?,
        file_size: row.get(3)?,
        modified_at: row.get(4)?,
        duration_secs: row.get(5)?,
        rating: row.get(6)?,
        score: row.get(7)?,
        last_watched_at: row.get(8)?,
        watch_count,
        is_trash: is_trash_val == 1,
        created_at: row.get(10)?,
        thumbnail_data,
        time_offset,
        page_offset,
        tags: Vec::new(),
        series_id: row.get(15)?,
        series_name: row.get(16)?,
        series_position: row.get(17)?,
        series_representative_id: row.get(18)?,
    })
}

fn query_media_list(conn: &Connection, sql: &str, is_trash: i64) -> Result<Vec<MediaFile>> {
    let mut stmt = conn.prepare(sql)?;
    let mut files = stmt
        .query_map([], map_media_fields)?
        .collect::<Result<Vec<_>>>()?;
    let positions: std::collections::HashMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.id, index))
        .collect();
    let tags_sql = format!(
        "SELECT ft.file_id,t.id,t.name FROM ({EFFECTIVE_TAGS}) ft
        JOIN tags t ON t.id=ft.tag_id JOIN media_files f ON f.id=ft.file_id
        WHERE COALESCE(f.is_trash,0)=?1 ORDER BY t.name ASC"
    );
    let mut tags = conn.prepare_cached(&tags_sql)?;
    let mut rows = tags.query([is_trash])?;
    while let Some(row) = rows.next()? {
        if let Some(&index) = positions.get(&row.get::<_, i64>(0)?) {
            files[index].tags.push(Tag {
                id: row.get(1)?,
                name: row.get(2)?,
            });
        }
    }
    Ok(files)
}

pub fn get_media_files(
    conn: &Connection,
    sort_by: Option<&str>,
    order: Option<&str>,
) -> Result<Vec<MediaFile>> {
    let is_asc = order
        .map(|o| o.eq_ignore_ascii_case("asc"))
        .unwrap_or(false);
    let order_str = if is_asc { "ASC" } else { "DESC" };

    let (column, tie_order) = match sort_by.unwrap_or("score") {
        "rating" => ("COALESCE(f.rating, 0)", order_str),
        "date" | "modified_at" => ("COALESCE(f.modified_at, 0)", order_str),
        "watch_count" => ("COALESCE(f.watch_count, 0)", order_str),
        "last_watched" => ("COALESCE(f.last_watched_at, 0)", order_str),
        "name" => ("COALESCE(f.display_name,f.rel_path) COLLATE NOCASE", "DESC"),
        _ => ("f.score", "DESC"),
    };
    let order_clause = format!("{column} {order_str}, f.id {tie_order}");

    let sql = format!(
        "{MEDIA_SELECT}
         WHERE COALESCE(f.is_trash, 0) = 0
         ORDER BY {}",
        order_clause
    );

    Ok(crate::series::sort_media(
        query_media_list(conn, &sql, 0)?,
        sort_by,
        order,
    ))
}

pub fn get_trash_files(conn: &Connection) -> Result<Vec<MediaFile>> {
    let sql = format!(
        "
        {MEDIA_SELECT}
        WHERE COALESCE(f.is_trash, 0) = 1
        ORDER BY f.modified_at DESC, f.id DESC
    "
    );

    query_media_list(conn, &sql, 1)
}

pub fn get_trash_count(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_files WHERE COALESCE(is_trash, 0) = 1",
        [],
        |row| row.get(0),
    )
}

pub fn move_to_trash(conn: &Connection, file_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_files SET is_trash = 1 WHERE id = ?1",
        params![file_id],
    )?;
    Ok(())
}

pub fn restore_from_trash(conn: &Connection, file_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_files SET is_trash = 0 WHERE id = ?1",
        params![file_id],
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct NotInTrash;
impl std::fmt::Display for NotInTrash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("휴지통 밖의 파일은 영구 삭제할 수 없습니다. 목록을 새로 확인해 주세요.")
    }
}
impl std::error::Error for NotInTrash {}

pub fn permanently_delete_file(
    conn: &Connection,
    root_dir: &std::path::Path,
    file_id: i64,
) -> anyhow::Result<()> {
    let (rel_path, in_trash): (String, bool) = conn.query_row(
        "SELECT rel_path,COALESCE(is_trash,0) FROM media_files WHERE id = ?1",
        [file_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    // Recheck under the caller's DB lock before recording intent or touching
    // the filesystem: an old batch/list may include an item already restored.
    if !in_trash {
        return Err(NotInTrash.into());
    }
    validate_media_path(root_dir, &rel_path)?;

    let stored_path = crate::archive_security::physical_path(conn, file_id, &rel_path)?;
    validate_media_path(root_dir, &stored_path)?;
    // A retained original must be explicitly cleaned first, otherwise a scan
    // could re-import it after deleting only the encrypted copy.
    anyhow::ensure!(
        stored_path == rel_path || !root_dir.join(&rel_path).try_exists()?,
        "암호화 관리에서 남은 평문 원본을 먼저 정리해 주세요."
    );
    conn.execute(
        "INSERT OR REPLACE INTO delete_intents (file_id, rel_path) VALUES (?1, ?2)",
        params![file_id, rel_path],
    )?;
    let full_path = root_dir.join(&stored_path);
    if full_path.try_exists()? {
        if let Err(error) = std::fs::remove_file(&full_path) {
            conn.execute("DELETE FROM delete_intents WHERE file_id = ?1", [file_id])?;
            return Err(error.into());
        }
    }
    finalize_delete_intent(conn, file_id, &rel_path)
}

fn validate_media_path(root_dir: &std::path::Path, rel_path: &str) -> anyhow::Result<()> {
    let rel = std::path::Path::new(rel_path);
    if rel.is_absolute()
        || rel.components().any(|component| {
            matches!(component, Component::ParentDir | Component::Prefix(_))
                || component.as_os_str().to_string_lossy().contains(':')
        })
    {
        anyhow::bail!("invalid media relative path: {rel_path}");
    }
    if root_dir.join(rel).try_exists()?
        && !std::fs::canonicalize(root_dir.join(rel))?.starts_with(std::fs::canonicalize(root_dir)?)
    {
        anyhow::bail!("media path escapes library root: {rel_path}");
    }
    Ok(())
}

fn finalize_delete_intent(conn: &Connection, file_id: i64, rel_path: &str) -> anyhow::Result<()> {
    let row = conn.query_row(
        "SELECT f.file_type, f.file_size, COALESCE(s.score, f.score, 1000) FROM media_files f LEFT JOIN series_members m ON m.file_id=f.id LEFT JOIN media_series s ON s.id=m.series_id WHERE f.id = ?1",
        [file_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    );
    let Some((file_type, file_size, score)) = row.optional()? else {
        conn.execute("DELETE FROM delete_intents WHERE file_id = ?1", [file_id])?;
        return Ok(());
    };
    let tags = get_tags_for_file(conn, file_id)?;
    let tags_info = (!tags.is_empty()).then(|| {
        tags.iter()
            .map(|t| t.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO deleted_files_log (original_rel_path, file_type, file_size, score, tags_info, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%s','now'))",
        params![rel_path, file_type, file_size, score, tags_info],
    )?;
    tx.execute("DELETE FROM media_bookmarks WHERE file_id = ?1", [file_id])?;
    tx.execute("DELETE FROM thumbnails WHERE file_id = ?1", [file_id])?;
    tx.execute("DELETE FROM file_tags WHERE file_id = ?1", [file_id])?;
    tx.execute("DELETE FROM media_files WHERE id = ?1", [file_id])?;
    tx.execute("DELETE FROM delete_intents WHERE file_id = ?1", [file_id])?;
    tx.commit()?;
    Ok(())
}

pub fn recover_pending_deletions(
    conn: &Connection,
    root_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let intents: Vec<(i64, String)> = conn
        .prepare("SELECT file_id, rel_path FROM delete_intents")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_>>()?;
    for (file_id, rel_path) in intents {
        validate_media_path(root_dir, &rel_path)?;
        let stored_path = crate::archive_security::physical_path(conn, file_id, &rel_path)?;
        validate_media_path(root_dir, &stored_path)?;
        if root_dir.join(&stored_path).try_exists()? {
            conn.execute("DELETE FROM delete_intents WHERE file_id = ?1", [file_id])?;
        } else {
            finalize_delete_intent(conn, file_id, &rel_path)?;
        }
    }
    Ok(())
}

// History rows are independent of media and pending deletion recovery.
pub fn delete_deleted_log(conn: &Connection, log_id: i64) -> Result<usize> {
    conn.execute("DELETE FROM deleted_files_log WHERE id = ?1", [log_id])
}

pub fn clear_deleted_logs(conn: &Connection) -> Result<usize> {
    conn.execute("DELETE FROM deleted_files_log", [])
}

pub fn get_deleted_logs(conn: &Connection) -> Result<Vec<DeletedFileLog>> {
    let mut stmt = conn.prepare(
        "SELECT id, original_rel_path, file_type, file_size, score, tags_info, deleted_at, deleted_time_str
         FROM deleted_files_log
         ORDER BY deleted_at DESC",
    )?;

    let logs = stmt
        .query_map([], |row| {
            Ok(DeletedFileLog {
                id: row.get(0)?,
                original_rel_path: row.get(1)?,
                file_type: row.get(2)?,
                file_size: row.get(3)?,
                score: row.get(4)?,
                tags_info: row.get(5)?,
                deleted_at: row.get(6)?,
                deleted_time_str: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;

    Ok(logs)
}

pub fn get_arena_pair(conn: &Connection) -> Result<Option<(MediaFile, MediaFile)>> {
    let sql = format!(
        "
        {MEDIA_SELECT}
        WHERE f.id IN (
            SELECT id FROM ({}) ORDER BY RANDOM() LIMIT 2
        )
        ORDER BY RANDOM()
    ",
        crate::series::ARENA_CANDIDATES
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut files = stmt
        .query_map([], |row| map_media_row(conn, row))?
        .collect::<Result<Vec<_>>>()?;

    if files.len() == 2 {
        let second = files.pop().unwrap();
        let first = files.pop().unwrap();
        Ok(Some((first, second)))
    } else {
        Ok(None)
    }
}

pub fn get_arena_opponent(
    conn: &Connection,
    focus_id: i64,
) -> Result<Option<(MediaFile, MediaFile)>> {
    let focus_sql = format!(
        "
        {MEDIA_SELECT}
        WHERE f.id = (
            SELECT CASE WHEN membership.series_id IS NULL THEN requested.id ELSE series.representative_id END
            FROM media_files requested
            LEFT JOIN series_members membership ON membership.file_id=requested.id
            LEFT JOIN media_series series ON series.id=membership.series_id
            WHERE requested.id=?1 AND COALESCE(requested.is_trash,0)=0
        ) AND COALESCE(f.is_trash, 0) = 0 AND f.is_missing=0 AND f.file_type<>'NOTE'
    "
    );
    let mut stmt = conn.prepare(&focus_sql)?;
    let focus_file = match stmt.query_row(params![focus_id], |row| map_media_row(conn, row)) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };

    let opponent_sql = format!(
        "
        {MEDIA_SELECT}
        WHERE f.id = (
            SELECT id FROM ({}) WHERE id != ?1 AND id NOT IN (
                SELECT file_id FROM series_members WHERE series_id=(SELECT series_id FROM series_members WHERE file_id=?1)
            ) ORDER BY RANDOM() LIMIT 1
        )
    ", crate::series::ARENA_CANDIDATES
    );
    let mut opp_stmt = conn.prepare(&opponent_sql)?;
    let opponent_file =
        match opp_stmt.query_row(params![focus_file.id], |row| map_media_row(conn, row)) {
            Ok(file) => file,
            Err(_) => return Ok(None),
        };

    Ok(Some((focus_file, opponent_file)))
}

pub fn vote_arena(
    conn: &Connection,
    winner_id: i64,
    loser_id: i64,
) -> Result<(i64, i64, i64, i64)> {
    if winner_id == loser_id {
        return Err(rusqlite::Error::InvalidParameterName(
            "winner and loser must differ".to_string(),
        ));
    }
    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        "SELECT COALESCE(s.score, f.score, 1000), s.id FROM media_files f
         LEFT JOIN series_members m ON m.file_id=f.id LEFT JOIN media_series s ON s.id=m.series_id
         WHERE f.id = ?1 AND COALESCE(f.is_trash, 0) = 0 AND f.is_missing=0 AND f.file_type<>'NOTE'",
    )?;
    let (winner_score, winner_series): (f64, Option<i64>) =
        stmt.query_row(params![winner_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let (loser_score, loser_series): (f64, Option<i64>) =
        stmt.query_row(params![loser_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    if winner_series.is_some() && winner_series == loser_series {
        return Err(rusqlite::Error::InvalidParameterName(
            "같은 시리즈끼리는 대결할 수 없습니다.".into(),
        ));
    }
    drop(stmt);
    for (id, series_id) in [(winner_id, winner_series), (loser_id, loser_series)] {
        if let Some(series_id) = series_id {
            let is_representative: bool = tx.query_row(
                "SELECT COALESCE(representative_id=?1,0) FROM media_series WHERE id=?2",
                params![id, series_id],
                |r| r.get(0),
            )?;
            if !is_representative {
                return Err(rusqlite::Error::InvalidParameterName(
                    "시리즈 대표 권만 아레나에 참여할 수 있습니다.".into(),
                ));
            }
        }
    }

    let expected_winner = 1.0 / (1.0 + 10.0_f64.powf((loser_score - winner_score) / 400.0));
    let expected_loser = 1.0 / (1.0 + 10.0_f64.powf((winner_score - loser_score) / 400.0));

    let k = 32.0;
    let winner_delta = (k * (1.0 - expected_winner)).round() as i64;
    let loser_delta = (k * (0.0 - expected_loser)).round() as i64;

    let new_winner_score = (winner_score as i64 + winner_delta).max(100);
    let new_loser_score = (loser_score as i64 + loser_delta).max(100);

    for (id, series_id, score) in [
        (winner_id, winner_series, new_winner_score),
        (loser_id, loser_series, new_loser_score),
    ] {
        if let Some(series_id) = series_id {
            tx.execute(
                "UPDATE media_series SET score=?1 WHERE id=?2",
                params![score, series_id],
            )?;
        } else {
            tx.execute(
                "UPDATE media_files SET score=?1 WHERE id=?2",
                params![score, id],
            )?;
        }
    }

    tx.commit()?;
    Ok((new_winner_score, new_loser_score, winner_delta, loser_delta))
}

pub fn get_thumbnail(conn: &Connection, file_id: i64) -> Result<Option<Vec<u8>>> {
    let mut stmt = conn.prepare("SELECT data FROM thumbnails WHERE file_id = ?1")?;
    let mut rows = stmt.query(params![file_id])?;
    if let Some(row) = rows.next()? {
        let data: Vec<u8> = row.get(0)?;
        Ok(Some(data))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Bookmark {
    pub id: i64,
    pub position: f64,
    pub thumbnail_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug)]
pub enum BookmarkError {
    Database(rusqlite::Error),
    NotFound,
    InvalidPosition,
    InvalidImage,
    ImageTooLarge,
    LimitReached,
}

impl std::fmt::Display for BookmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::NotFound => write!(f, "media or bookmark not found"),
            Self::InvalidPosition => write!(f, "invalid bookmark position"),
            Self::InvalidImage => write!(f, "thumbnail must be a JPEG image"),
            Self::ImageTooLarge => write!(f, "thumbnail exceeds 128 KiB"),
            Self::LimitReached => write!(f, "bookmark limit reached"),
        }
    }
}

impl std::error::Error for BookmarkError {}
impl From<rusqlite::Error> for BookmarkError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e)
    }
}

fn validate_bookmark_media(
    conn: &Connection,
    file_id: i64,
    position: f64,
) -> Result<String, BookmarkError> {
    if !position.is_finite() || position < 0.0 {
        return Err(BookmarkError::InvalidPosition);
    }
    let file_type: Option<String> = conn
        .query_row(
            "SELECT file_type FROM media_files WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(file_type) = file_type else {
        return Err(BookmarkError::NotFound);
    };
    if (file_type.eq_ignore_ascii_case("ARCHIVE") || file_type.eq_ignore_ascii_case("PDF"))
        && position.fract() != 0.0
    {
        return Err(BookmarkError::InvalidPosition);
    }
    if !file_type.eq_ignore_ascii_case("VIDEO")
        && !file_type.eq_ignore_ascii_case("ARCHIVE")
        && !file_type.eq_ignore_ascii_case("PDF")
    {
        return Err(BookmarkError::NotFound);
    }
    Ok(file_type)
}

pub fn list_bookmarks(conn: &Connection, file_id: i64) -> Result<Vec<Bookmark>, BookmarkError> {
    let media: Option<String> = conn
        .query_row(
            "SELECT file_type FROM media_files WHERE id = ?1",
            [file_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(media) = media else {
        return Err(BookmarkError::NotFound);
    };
    if !media.eq_ignore_ascii_case("VIDEO")
        && !media.eq_ignore_ascii_case("ARCHIVE")
        && !media.eq_ignore_ascii_case("PDF")
    {
        return Err(BookmarkError::NotFound);
    }
    let mut stmt = conn.prepare(
        "SELECT id, position, request_id FROM media_bookmarks WHERE file_id = ?1 ORDER BY position, id",
    )?;
    let rows = stmt.query_map([file_id], |row| {
        let id: i64 = row.get(0)?;
        Ok(Bookmark {
            id,
            position: row.get(1)?,
            request_id: row.get(2)?,
            thumbnail_url: format!("/api/files/{file_id}/bookmarks/{id}/thumbnail"),
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(BookmarkError::Database)
}

pub fn save_bookmark(
    conn: &Connection,
    file_id: i64,
    position: f64,
    image: &[u8],
) -> Result<Bookmark, BookmarkError> {
    validate_bookmark_media(conn, file_id, position)?;
    if image.len() > 128 * 1024 {
        return Err(BookmarkError::ImageTooLarge);
    }
    if image.len() < 3 || image[0..3] != [0xff, 0xd8, 0xff] {
        return Err(BookmarkError::InvalidImage);
    }
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM media_bookmarks WHERE file_id = ?1 AND position = ?2",
            params![file_id, position],
            |r| r.get(0),
        )
        .optional()?;
    let id = if let Some(id) = existing {
        conn.execute(
            "UPDATE media_bookmarks SET thumbnail_data = ?1 WHERE id = ?2",
            params![image, id],
        )?;
        id
    } else {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM media_bookmarks WHERE file_id = ?1",
            [file_id],
            |r| r.get(0),
        )?;
        if count >= 100 {
            return Err(BookmarkError::LimitReached);
        }
        conn.execute(
            "INSERT INTO media_bookmarks(file_id, position, thumbnail_data) VALUES (?1, ?2, ?3)",
            params![file_id, position, image],
        )?;
        conn.last_insert_rowid()
    };
    Ok(Bookmark {
        id,
        position,
        request_id: None,
        thumbnail_url: format!("/api/files/{file_id}/bookmarks/{id}/thumbnail"),
    })
}

pub fn delete_bookmark(
    conn: &Connection,
    file_id: i64,
    bookmark_id: i64,
) -> Result<(), BookmarkError> {
    let changed = conn.execute(
        "DELETE FROM media_bookmarks WHERE id = ?1 AND file_id = ?2",
        params![bookmark_id, file_id],
    )?;
    if changed == 0 {
        Err(BookmarkError::NotFound)
    } else {
        Ok(())
    }
}

pub fn get_bookmark_thumbnail(
    conn: &Connection,
    file_id: i64,
    bookmark_id: i64,
) -> Result<Option<Vec<u8>>, BookmarkError> {
    conn.query_row(
        "SELECT thumbnail_data FROM media_bookmarks WHERE id = ?1 AND file_id = ?2",
        params![bookmark_id, file_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(BookmarkError::Database)
}
