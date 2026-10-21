use crate::db::MediaFile;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize)]
pub struct Series {
    pub id: i64,
    pub name: String,
    pub representative_id: Option<i64>,
    pub rating: i64,
    pub score: i64,
    pub file_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct SaveSeries {
    pub id: Option<i64>,
    pub name: String,
    pub representative_id: i64,
    pub file_ids: Vec<i64>,
}

#[derive(Debug)]
pub enum Error {
    Database(rusqlite::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    NotFound,
}
impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<Series>> {
    let mut stmt = conn.prepare("SELECT id, name, representative_id, rating, score FROM media_series ORDER BY name COLLATE NOCASE, id")?;
    let mut series = stmt
        .query_map([], |row| {
            Ok(Series {
                id: row.get(0)?,
                name: row.get(1)?,
                representative_id: row.get(2)?,
                rating: row.get(3)?,
                score: row.get(4)?,
                file_ids: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let positions: HashMap<_, _> = series.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut stmt =
        conn.prepare("SELECT series_id, file_id FROM series_members ORDER BY position, file_id")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if let Some(&index) = positions.get(&row.get::<_, i64>(0)?) {
            series[index].file_ids.push(row.get(1)?);
        }
    }
    Ok(series)
}

pub fn save(conn: &Connection, input: &SaveSeries) -> Result<i64, Error> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 200 || name.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "시리즈 이름은 1~200자의 한 줄 텍스트여야 합니다.",
        ));
    }
    let ids: HashSet<_> = input.file_ids.iter().copied().collect();
    if ids.is_empty()
        || ids.len() > 100_000
        || ids.len() != input.file_ids.len()
        || !ids.contains(&input.representative_id)
        || ids.iter().any(|id| *id <= 0)
    {
        return Err(Error::Invalid(
            "중복 없는 동영상·압축 파일과 그중 하나인 대표 파일을 선택해 주세요.",
        ));
    }
    let tx = conn.unchecked_transaction()?;
    if let Some(id) = input.id {
        if !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM media_series WHERE id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::NotFound);
        }
    }
    let duplicate: Option<i64> = tx
        .query_row("SELECT id FROM media_series WHERE name=?1", [name], |r| {
            r.get(0)
        })
        .optional()?;
    if duplicate.is_some() && duplicate != input.id {
        return Err(Error::Conflict("같은 이름의 시리즈가 이미 있습니다."));
    }
    {
        let mut stmt = tx.prepare("SELECT f.file_type, m.series_id FROM media_files f LEFT JOIN series_members m ON m.file_id=f.id WHERE f.id=?1")?;
        for id in &input.file_ids {
            let value = stmt
                .query_row([id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
                })
                .optional()?
                .ok_or(Error::NotFound)?;
            if !matches!(value.0.as_str(), "ARCHIVE" | "VIDEO" | "IMAGE" | "PDF") {
                return Err(Error::Invalid(
                    "시리즈에는 동영상, 이미지, PDF, 이미지 압축 파일을 추가할 수 있습니다.",
                ));
            }
            if value.1.is_some() && value.1 != input.id {
                return Err(Error::Conflict(
                    "다른 시리즈에 속한 파일은 먼저 기존 시리즈에서 제외해 주세요.",
                ));
            }
        }
    }
    let id = if let Some(id) = input.id {
        tx.execute(
            "UPDATE media_series SET name=?1, representative_id=?2 WHERE id=?3",
            params![name, input.representative_id, id],
        )?;
        id
    } else {
        tx.execute("INSERT INTO media_series(name,representative_id,rating,score) SELECT ?1,id,COALESCE(rating,0),COALESCE(score,1000) FROM media_files WHERE id=?2", params![name,input.representative_id])?;
        tx.last_insert_rowid()
    };
    // Merge original tags only from newly added members. Reordering or changing
    // the representative must not restore tags removed from the shared set.
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO series_tags(series_id,tag_id)
            SELECT ?1,ft.tag_id FROM file_tags ft WHERE ft.file_id=?2
            AND NOT EXISTS(SELECT 1 FROM series_members m WHERE m.series_id=?1 AND m.file_id=?2)",
        )?;
        for file_id in &input.file_ids {
            stmt.execute(params![id, file_id])?;
        }
    }
    tx.execute("DELETE FROM series_members WHERE series_id=?1", [id])?;
    {
        let mut stmt =
            tx.prepare("INSERT INTO series_members(series_id,file_id,position) VALUES (?1,?2,?3)")?;
        for (position, file_id) in input.file_ids.iter().enumerate() {
            stmt.execute(params![id, file_id, position as i64 + 1])?;
        }
    }
    tx.commit()?;
    Ok(id)
}

pub fn dissolve(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM media_series WHERE id=?1", [id])
}

// Pick the designated active representative, falling back to the first active
// volume when it is in Trash or was deleted. Series order never reverses.
pub fn sort_media(files: Vec<MediaFile>, key: Option<&str>, order: Option<&str>) -> Vec<MediaFile> {
    if !files.iter().any(|f| f.series_id.is_some()) {
        return files;
    }
    let mut groups: HashMap<(bool, i64), Vec<MediaFile>> = HashMap::new();
    for file in files {
        groups
            .entry((file.series_id.is_some(), file.series_id.unwrap_or(file.id)))
            .or_default()
            .push(file);
    }
    let mut groups: Vec<_> = groups
        .into_values()
        .map(|mut files| {
            files.sort_by_key(|f| (f.series_position.unwrap_or(0), f.id));
            let rep = files
                .iter()
                .position(|f| Some(f.id) == f.series_representative_id)
                .unwrap_or(0);
            (files, rep)
        })
        .collect();
    let asc = order.is_some_and(|o| o.eq_ignore_ascii_case("asc"));
    let key = key.unwrap_or("score");
    groups.sort_by(|(a, ar), (b, br)| {
        let a = &a[*ar];
        let b = &b[*br];
        let primary = match key {
            "rating" => a.rating.cmp(&b.rating),
            "date" | "modified_at" => a.modified_at.cmp(&b.modified_at),
            "watch_count" => a.watch_count.cmp(&b.watch_count),
            "last_watched" => a
                .last_watched_at
                .unwrap_or(0)
                .cmp(&b.last_watched_at.unwrap_or(0)),
            "name" => a
                .series_name
                .as_deref()
                .unwrap_or(a.display_name.as_deref().unwrap_or(&a.rel_path))
                .to_ascii_lowercase()
                .cmp(
                    &b.series_name
                        .as_deref()
                        .unwrap_or(b.display_name.as_deref().unwrap_or(&b.rel_path))
                        .to_ascii_lowercase(),
                ),
            _ => a.score.cmp(&b.score),
        };
        let primary = if asc { primary } else { primary.reverse() };
        let tie_asc = asc
            && matches!(
                key,
                "rating" | "date" | "modified_at" | "watch_count" | "last_watched"
            );
        primary.then_with(|| {
            if tie_asc {
                a.id.cmp(&b.id)
            } else {
                b.id.cmp(&a.id)
            }
        })
    });
    groups.into_iter().flat_map(|(files, _)| files).collect()
}

// Only designated, active representatives are eligible. A series without an
// available representative is omitted until its representative is changed/restored.
pub const ARENA_CANDIDATES: &str = "SELECT f.id FROM media_files f
    LEFT JOIN series_members m ON m.file_id=f.id
    LEFT JOIN media_series s ON s.id=m.series_id
    WHERE f.file_type<>'NOTE' AND COALESCE(f.is_trash,0)=0 AND f.is_missing=0 AND (s.id IS NULL OR f.id=s.representative_id)";

#[cfg(test)]
#[path = "series_tests.rs"]
mod tests;
