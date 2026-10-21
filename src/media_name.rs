use crate::state::{AppState, RequestState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use rusqlite::{params, Connection};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Input {
    name: String,
    #[serde(default)]
    restore_name: bool,
}

pub(crate) fn validated_name(conn: &Connection, id: i64, name: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        !name.is_empty()
            && name.encode_utf16().count() <= 240
            && !name
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            && name.trim() == name
            && !name.ends_with('.'),
        "폴더 경로 없이 올바른 파일명을 입력해 주세요. (최대 240자)"
    );
    let tx = conn;
    let original: String = tx.query_row("SELECT m.rel_path FROM media_files m JOIN archive_store a ON a.file_id=m.id WHERE m.id=?1 AND m.file_type='ARCHIVE' AND m.is_trash=0 AND a.state='ready'", [id], |r|r.get(0))?;
    let ext = if original.to_ascii_lowercase().ends_with(".tar.zst") {
        ".tar.zst"
    } else {
        ".zip"
    };
    anyhow::ensure!(
        name.to_ascii_lowercase().ends_with(ext) && name.len() > ext.len(),
        "기존 압축 형식의 확장자를 유지해 주세요."
    );
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    anyhow::ensure!(
        !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM0"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT0"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        ),
        "예약된 파일명은 사용할 수 없습니다."
    );
    let next = match original.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/{name}"),
        None => name.to_owned(),
    };
    let conflict: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM media_files m WHERE m.id<>?1 AND (m.rel_path=?2 COLLATE NOCASE OR COALESCE(m.display_name,m.rel_path)=?2 COLLATE NOCASE OR EXISTS(SELECT 1 FROM archive_store a WHERE a.file_id=m.id AND a.restore_name=?2 COLLATE NOCASE)))", params![id,next], |r|r.get(0))?;
    anyhow::ensure!(
        !conflict,
        "같은 폴더에 같은 이름의 항목이 있습니다. 다른 이름을 입력해 주세요."
    );
    Ok(next)
}

pub(crate) fn set_name(conn: &Connection, id: i64, name: &str) -> anyhow::Result<String> {
    let tx = conn.unchecked_transaction()?;
    let next = validated_name(&tx, id, name)?;
    tx.execute("UPDATE media_files SET display_name=CASE WHEN rel_path=?1 THEN NULL ELSE ?1 END WHERE id=?2",params![next,id])?;
    tx.commit()?;
    Ok(next)
}

pub async fn rename(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if input.restore_name || headers.contains_key("X-Vault-Request") {
        return crate::archive_security::rename::start(
            app,
            state,
            id,
            input.name,
            input.restore_name,
            headers,
        )
        .await;
    }
    state
        .db_write(move |state| {
            let fail = |e: anyhow::Error| (StatusCode::BAD_REQUEST, e.to_string());
            crate::encryption::require_idle(&state).map_err(fail)?;
            // Queue the DB work off the HTTP runtime without retaining app.current.
            // Closing can revoke this library even while its NAS connection is busy.
            let guard = state
                .lock_db_for_write()
                .map_err(|status| (status, "DB 확인 실패".into()))?;
            if state
                .archives
                .revoked
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
            }
            let conn = guard
                .as_ref()
                .ok_or((StatusCode::LOCKED, "DB 잠금을 해제해 주세요.".into()))?;
            let name = set_name(conn, id, &input.name).map_err(fail)?;
            Ok(Json(serde_json::json!({"id":id,"display_name":name})))
        })
        .await
        .map_err(|status| (status, "DB 작업을 처리할 수 없습니다.".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_keeps_identity_metadata_and_original_path_and_validates_names() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        crate::db::init_db(&conn)?;
        for name in ["folder/old.zip", "folder/another.zip"] {
            crate::db::upsert_media_file(&conn, name, "ARCHIVE", 100, 1)?;
        }
        conn.execute_batch("INSERT INTO archive_store(file_id,encrypted_path,state) VALUES(1,'.valtheum/archives/1.vta','ready'); UPDATE media_files SET rating=5,score=1500,watch_count=9 WHERE id=1; INSERT INTO tags VALUES(1,'keep'); INSERT INTO file_tags VALUES(1,1); INSERT INTO media_bookmarks(file_id,position,thumbnail_data) VALUES(1,0,X'01');")?;
        assert_eq!(set_name(&conn, 1, "A renamed.zip")?, "folder/A renamed.zip");
        let files = crate::db::get_media_files(&conn, Some("name"), Some("asc"))?;
        assert_eq!(files[0].id, 1);
        let file = files.iter().find(|f| f.id == 1).unwrap();
        assert_eq!(file.rel_path, "folder/old.zip");
        assert_eq!(file.display_name.as_deref(), Some("folder/A renamed.zip"));
        assert_eq!((file.rating, file.score, file.watch_count), (5, 1500, 9));
        assert_eq!(file.tags[0].name, "keep");
        assert_eq!(crate::db::list_bookmarks(&conn, 1)?.len(), 1);
        for name in [
            "../bad.zip",
            "new.tar.zst",
            "",
            ".zip",
            "new.zip ",
            "CON.zip",
            "another.zip",
        ] {
            assert!(set_name(&conn, 1, name).is_err(), "{name}");
        }
        assert!(set_name(&conn, 2, "plain.zip").is_err());
        conn.execute("INSERT INTO archive_store(file_id,encrypted_path,state,restore_name) VALUES(2,'.valtheum/archives/2.vta','ready','folder/restored.zip')",[])?;
        assert!(validated_name(&conn, 1, "restored.zip").is_err());

        set_name(&conn, 1, "old.zip")?;
        assert!(crate::db::get_media_files(&conn, None, None)?
            .iter()
            .find(|f| f.id == 1)
            .unwrap()
            .display_name
            .is_none());
        Ok(())
    }
}
