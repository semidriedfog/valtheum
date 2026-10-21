use crate::state::{AppState, RequestState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path as FsPath;
use std::sync::atomic::Ordering;

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, String)>;

// Identity stays with the old record. Never discard an independently edited target.
#[cfg(test)]
fn reconnect(conn: &Connection, root: &FsPath, id: i64, selected: &FsPath) -> anyhow::Result<()> {
    reconnect_checked(conn, root, id, selected, || Ok(()))
}

fn reconnect_checked(
    conn: &Connection,
    root: &FsPath,
    id: i64,
    selected: &FsPath,
    checkpoint: impl Fn() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    checkpoint()?;
    let root = root.canonicalize()?;
    let selected = selected.canonicalize()?;
    anyhow::ensure!(
        selected.starts_with(&root) && selected.is_file(),
        "현재 라이브러리 폴더 안의 파일을 선택해 주세요."
    );
    let rel = selected
        .strip_prefix(&root)?
        .to_string_lossy()
        .replace('\\', "/");
    crate::state::validate_relative(&rel)
        .map_err(|_| anyhow::anyhow!("허용되지 않은 경로입니다."))?;
    checkpoint()?;
    let tx = conn.unchecked_transaction()?;
    let (old_path, kind, size): (String, String, i64) = tx.query_row(
        "SELECT rel_path,file_type,file_size FROM media_files WHERE id=?1 AND is_missing=1 AND is_trash=0
        AND NOT EXISTS(SELECT 1 FROM archive_store WHERE file_id=?1)
        AND NOT EXISTS(SELECT 1 FROM delete_intents WHERE file_id=?1)", [id],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    anyhow::ensure!(
        !root.join(old_path).try_exists()?,
        "기존 파일이 다시 발견되었습니다. 목록을 동기화해 주세요."
    );
    let meta = selected.metadata()?;
    let ext = selected
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let selected_kind = match ext.as_str() {
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" => "VIDEO",
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif" => "IMAGE",
        "pdf" => "PDF",
        "zip" | "rar" | "7z" => "ARCHIVE",
        "zst" if crate::archive::is_tar_zstd(&selected) => "ARCHIVE",
        _ => "",
    };
    anyhow::ensure!(
        selected_kind == kind && meta.len() == size as u64,
        "기존 파일과 종류 및 크기가 같은 파일을 선택해 주세요."
    );
    checkpoint()?;
    if let Some(other) = tx
        .query_row(
            "SELECT id FROM media_files WHERE rel_path=?1",
            [&rel],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        let untouched: bool = tx.query_row("SELECT rating=0 AND score=1000 AND watch_count=0 AND last_watched_at IS NULL AND is_trash=0 AND duration_secs IS NULL
            AND pdf_last_page IS NULL AND pdf_position_revision=0
            AND NOT EXISTS(SELECT 1 FROM file_tags WHERE file_id=?1)
            AND NOT EXISTS(SELECT 1 FROM media_bookmarks WHERE file_id=?1)
            AND NOT EXISTS(SELECT 1 FROM thumbnails WHERE file_id=?1)
            AND NOT EXISTS(SELECT 1 FROM series_members WHERE file_id=?1)
            AND NOT EXISTS(SELECT 1 FROM archive_store WHERE file_id=?1)
            AND NOT EXISTS(SELECT 1 FROM delete_intents WHERE file_id=?1)
            FROM media_files WHERE id=?1", [other], |r|r.get(0))?;
        anyhow::ensure!(untouched, "선택한 파일에는 별도의 기록이 있어 재연결할 수 없습니다. 파일을 기존 경로로 되돌린 뒤 동기화해 주세요.");
        tx.execute("DELETE FROM media_files WHERE id=?1", [other])?;
    }
    let modified = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    tx.execute(
        "UPDATE media_files SET rel_path=?1,modified_at=?2,is_missing=0 WHERE id=?3",
        params![rel, modified, id],
    )?;
    checkpoint()?;
    tx.commit()?;
    Ok(())
}

pub async fn select(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
) -> ApiResult {
    let root = state.root_dir.clone();
    let path = tokio::task::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title(crate::language::t("이동하거나 이름을 바꾼 원본 파일 선택"))
            .set_directory(root)
            .pick_file()
    })
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "재연결 실패".into()))?;
    match path {
        Some(path) => reconnect_selected(app, state, id, path).await,
        None => Ok(Json(serde_json::json!({"status":"cancelled"}))),
    }
}

pub(crate) async fn reconnect_selected(
    app: AppState,
    state: RequestState,
    id: i64,
    path: std::path::PathBuf,
) -> ApiResult {
    // The native picker may outlive its library. Only check in-memory admission
    // while holding app.current; DB and filesystem waits use the captured state.
    {
        let current = app
            .current
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "상태 확인 실패".into()))?;
        if current.generation != state.generation || app.concealed.load(Ordering::Acquire) {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        crate::encryption::require_idle(&current)
            .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
    }
    state
        .db_write(move |state| {
            let guard = state
                .lock_db_for_write()
                .map_err(|s| (s, "DB 작업을 처리할 수 없습니다.".into()))?;
            let conn = guard
                .as_ref()
                .ok_or((StatusCode::LOCKED, "DB 잠금을 해제해 주세요.".into()))?;
            reconnect_checked(conn, &state.root_dir, id, &path, || {
                anyhow::ensure!(
                    !state.archives.revoked.load(Ordering::Acquire),
                    "폴더가 변경되었습니다."
                );
                crate::encryption::require_idle(&state)
            })
            .map_err(|e| {
                (
                    if state.archives.revoked.load(Ordering::Acquire) {
                        StatusCode::CONFLICT
                    } else {
                        StatusCode::BAD_REQUEST
                    },
                    e.to_string(),
                )
            })?;
            Ok(Json(serde_json::json!({"status":"success"})))
        })
        .await
        .map_err(|s| (s, "DB 작업을 처리할 수 없습니다.".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    #[test]
    fn reconnect_preserves_pdf_progress_and_refuses_targets_with_resume_history(
    ) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "valtheum-relink-pdf-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("new.pdf"), b"pdf")?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        db::upsert_media_file(&conn, "old.pdf", "PDF", 3, 1)?;
        db::upsert_media_file(&conn, "new.pdf", "PDF", 3, 1)?;
        conn.execute("UPDATE media_files SET is_missing=1,pdf_last_page=9,pdf_position_revision=3 WHERE id=1", [])?;

        // A legacy page, a revision-only record, and an ordinary saved page are all history.
        for (page, revision) in [(Some(4), 0), (None, 1), (Some(4), 1)] {
            conn.execute(
                "UPDATE media_files SET pdf_last_page=?1,pdf_position_revision=?2 WHERE id=2",
                params![page, revision],
            )?;
            assert!(reconnect(&conn, &root, 1, &root.join("new.pdf")).is_err());
            let records = conn.prepare("SELECT rel_path,pdf_last_page,pdf_position_revision,is_missing FROM media_files ORDER BY id")?
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, i64>(2)?, row.get::<_, bool>(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            assert_eq!(
                records,
                vec![
                    ("old.pdf".into(), Some(9), 3, true),
                    ("new.pdf".into(), page, revision, false)
                ]
            );
        }

        // Merging an actually untouched target still works and preserves the source position.
        conn.execute(
            "UPDATE media_files SET pdf_last_page=NULL,pdf_position_revision=0 WHERE id=2",
            [],
        )?;
        reconnect(&conn, &root, 1, &root.join("new.pdf"))?;
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM media_files", [], |row| row
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(conn.query_row("SELECT rel_path,pdf_last_page,pdf_position_revision,is_missing FROM media_files WHERE id=1", [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, bool>(3)?)))?, ("new.pdf".into(), 9, 3, false));
        std::fs::remove_file(root.join("new.pdf"))?;
        std::fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn cancelled_reconnect_rolls_back_target_merge_and_identity_change() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "valtheum-relink-cancel-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("renamed.png"), b"image")?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        db::upsert_media_file(&conn, "old.png", "IMAGE", 5, 1)?;
        db::upsert_media_file(&conn, "renamed.png", "IMAGE", 5, 1)?;
        conn.execute(
            "UPDATE media_files SET is_missing=1,rating=5 WHERE id=1",
            [],
        )?;
        let result = reconnect_checked(&conn, &root, 1, &root.join("renamed.png"), || {
            let name: String =
                conn.query_row("SELECT rel_path FROM media_files WHERE id=1", [], |r| {
                    r.get(0)
                })?;
            anyhow::ensure!(name == "old.png", "synthetic close before commit");
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM media_files", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            conn.query_row(
                "SELECT rel_path,rating,is_missing FROM media_files WHERE id=1",
                [],
                |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, bool>(2)?
                ))
            )?,
            ("old.png".into(), 5, true)
        );
        reconnect(&conn, &root, 1, &root.join("renamed.png"))?;
        assert_eq!(db::get_media_files(&conn, None, None)?.len(), 1);
        std::fs::remove_file(root.join("renamed.png"))?;
        std::fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn renamed_media_reconnects_without_losing_metadata_and_refuses_edited_targets(
    ) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "valtheum-relink-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&root)?;
        let conn = Connection::open_in_memory()?;
        db::init_db(&conn)?;
        std::fs::write(root.join("old.jpg"), b"image")?;
        crate::scanner::scan_directory(&root, &conn)?;
        conn.execute(
            "UPDATE media_files SET rating=5,score=1200,watch_count=7 WHERE id=1",
            [],
        )?;
        conn.execute("INSERT INTO tags(id,name) VALUES(1,'keep')", [])?;
        conn.execute("INSERT INTO file_tags VALUES(1,1)", [])?;
        conn.execute(
            "INSERT INTO media_bookmarks(file_id,position,thumbnail_data) VALUES(1,0,X'01')",
            [],
        )?;
        std::fs::rename(root.join("old.jpg"), root.join("new.jpg"))?;
        crate::scanner::scan_directory(&root, &conn)?;
        let files = db::get_media_files(&conn, None, None)?;
        assert!(files
            .iter()
            .any(|f| f.id == 1 && f.missing && f.rating == 5));
        conn.execute(
            "UPDATE media_files SET rating=2 WHERE rel_path='new.jpg'",
            [],
        )?;
        assert!(reconnect(&conn, &root, 1, &root.join("new.jpg")).is_err());
        assert_eq!(db::get_media_files(&conn, None, None)?.len(), 2);
        conn.execute(
            "UPDATE media_files SET rating=0 WHERE rel_path='new.jpg'",
            [],
        )?;
        reconnect(&conn, &root, 1, &root.join("new.jpg"))?;
        let files = db::get_media_files(&conn, None, None)?;
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(
            (file.id, file.rating, file.score, file.watch_count),
            (1, 5, 1200, 7)
        );
        assert_eq!(file.tags[0].name, "keep");
        assert_eq!(file.rel_path, "new.jpg");
        assert!(!file.missing);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM media_bookmarks WHERE file_id=1",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        std::fs::remove_file(root.join("new.jpg"))?;
        crate::scanner::scan_directory(&root, &conn)?;
        std::fs::write(root.join("new.jpg"), b"image")?;
        crate::scanner::scan_directory(&root, &conn)?;
        assert!(!db::get_media_files(&conn, None, None)?[0].missing);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
