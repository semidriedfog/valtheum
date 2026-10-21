use super::*;
use crypto::image::Note;
use serde::Deserialize;
use zeroize::Zeroize;

#[derive(Deserialize)]
pub struct Input {
    pub revision: Option<String>,
    pub request_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
}
impl Drop for Input {
    fn drop(&mut self) {
        self.title.zeroize();
        self.body.zeroize();
    }
}
fn validate(library: &Library, id: i64) -> Result<()> {
    let g = library.conn.lock().unwrap();
    ensure!(g.as_ref().context("DB 잠김")?.query_row("SELECT EXISTS(SELECT 1 FROM media_files WHERE id=?1 AND file_type='NOTE' AND is_trash=0 AND is_missing=0)",[id],|r|r.get::<_,bool>(0))?,"현재 라이브러리의 메모를 선택해 주세요.");
    Ok(())
}
fn reader(library: &Library, id: i64) -> Result<crypto::Reader> {
    validate(library, id)?;
    let (name, rel, key) = ready(library, id)?;
    let r = crypto::Reader::open(&crypto::safe_child(&library.root_dir, &rel)?, &key)?
        .with_revocation(library.archives.revoked.clone());
    ensure!(
        r.standalone_note && r.metadata.name == name,
        "메모 파일 정보가 일치하지 않습니다."
    );
    Ok(r)
}
pub async fn read_api(Extension(state): Extension<RequestState>, Path(id): Path<i64>) -> ApiResult {
    state
        .db_work(move |state| -> ApiResult {
            let mut r = reader(&state, id).map_err(error)?;
            let index = r.note_index().map_err(error)?;
            let revision = index.revision.clone();
            let note_id = index.notes[0].id.clone();
            let note = r.read_note(&note_id).map_err(error)?;
            Ok(Json(
                serde_json::json!({"revision":revision,"title":note.title,"body":note.body}),
            ))
        })
        .await
        .map_err(error)?
}

// A new verified ciphertext is published by changing its DB mapping in one transaction.
// GC registration precedes writing so crashes never leave an untracked replacement.
pub(super) fn save(library: &Library, id: Option<i64>, input: &Input) -> Result<i64> {
    ensure!(
        input.title.len() <= 512 && input.body.len() <= crypto::image::MAX_BODY,
        "메모 제목 또는 본문이 너무 큽니다."
    );
    let fingerprint = if let Some(request_id) = &input.request_id {
        validate_request_id(request_id)?;
        use sha2::{Digest, Sha256};
        let bytes = zeroize::Zeroizing::new(serde_json::to_vec(&(
            id,
            &input.revision,
            &input.title,
            &input.body,
        ))?);
        let fingerprint = crypto::hex(&Sha256::digest(bytes.as_slice()));
        let g = library.conn.lock().unwrap();
        let conn = g.as_ref().context("DB 잠김")?;
        let saved: Option<(String, i64)> = conn
            .query_row(
                "SELECT fingerprint,file_id FROM note_save_requests WHERE request_id=?1",
                [request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((previous, file_id)) = saved {
            ensure!(
                previous == fingerprint,
                "같은 저장 요청의 내용이 변경되었습니다."
            );
            return Ok(file_id);
        }
        Some(fingerprint)
    } else {
        None
    };
    initialize(library)?;
    let key = library.archives.secret()?;
    let (logical_name, old_rel, note_id) = if let Some(id) = id {
        let mut source = reader(library, id)?;
        let index = source.note_index()?;
        ensure!(
            input.revision.as_ref() == Some(&index.revision),
            "메모가 변경되었습니다. 다시 열어 주세요."
        );
        let note_id = index.notes[0].id.clone();
        source.read_note(&note_id)?;
        let (name, rel, _) = ready(library, id)?;
        (name, Some(rel), note_id)
    } else {
        ensure!(
            input.revision.is_none(),
            "새 메모의 정보가 올바르지 않습니다."
        );
        let random = crypto::hex(&crypto::random::<16>()?);
        (format!("notes/{random}.json"), None, random)
    };
    fs::create_dir_all(library.root_dir.join(".valtheum/notes"))?;
    let rel = format!(
        ".valtheum/notes/{}.vtn",
        crypto::hex(&crypto::random::<16>()?)
    );
    let target = crypto::safe_child(&library.root_dir, &rel)?;
    {
        let g = library.conn.lock().unwrap();
        g.as_ref()
            .context("DB 잠김")?
            .execute("INSERT INTO archive_gc(encrypted_path) VALUES(?1)", [&rel])?;
    }
    let note = Note {
        id: note_id,
        title: input.title.clone(),
        body: input.body.clone(),
    };
    let result = (|| -> Result<i64> {
        let meta = crypto::image::write_note(&target, &logical_name, &note, &key, |p, n, t| {
            library.archives.progress(p, n, t)
        })?;
        library.archives.progress("메모 반영", 0, 0)?;
        let g = library.conn.lock().unwrap();
        let conn = g.as_ref().context("DB 잠김")?;
        let tx = conn.unchecked_transaction()?;
        let file_id = if let Some(id) = id {
            let n=tx.execute("UPDATE archive_store SET encrypted_path=?1,sha256=?2 WHERE file_id=?3 AND encrypted_path=?4 AND state='ready'",rusqlite::params![rel,meta.sha256,id,old_rel])?;
            ensure!(n == 1, "메모 파일 연결이 변경되었습니다.");
            tx.execute(
                "INSERT OR IGNORE INTO archive_gc(encrypted_path) VALUES(?1)",
                [old_rel.as_ref().unwrap()],
            )?;
            id
        } else {
            tx.execute("INSERT INTO media_files(rel_path,file_type,file_size,modified_at) VALUES(?1,'NOTE',?2,strftime('%s','now'))",rusqlite::params![logical_name,input.body.len() as i64])?;
            let id = tx.last_insert_rowid();
            tx.execute("INSERT INTO archive_store(file_id,encrypted_path,state,sha256) VALUES(?1,?2,'ready',?3)",rusqlite::params![id,rel,meta.sha256])?;
            id
        };
        // Title is encrypted at rest by the required SQLCipher database; never index the body.
        tx.execute("UPDATE media_files SET display_name=?1,file_size=?2,modified_at=strftime('%s','now') WHERE id=?3",rusqlite::params![if input.title.is_empty(){"Untitled note"}else{&input.title},input.body.len() as i64,file_id])?;
        tx.execute("DELETE FROM archive_gc WHERE encrypted_path=?1", [&rel])?;
        if let Some(request_id) = &input.request_id {
            tx.execute(
                "INSERT INTO note_save_requests(request_id,fingerprint,file_id) VALUES(?1,?2,?3)",
                rusqlite::params![request_id, fingerprint, file_id],
            )?;
        }
        tx.commit()?;
        Ok(file_id)
    })();
    if let Some(conn) = library.conn.lock().unwrap().as_ref() {
        let _ = rename::collect_unused(&library.root_dir, conn);
    }
    result
}
async fn start_save(
    app: AppState,
    state: RequestState,
    id: Option<i64>,
    input: Input,
) -> ApiResult {
    ensure_api(
        input.title.len() <= 512 && input.body.len() <= crypto::image::MAX_BODY,
        "메모 제목 또는 본문이 너무 큽니다.",
    )?;
    validate_request_id(input.request_id.as_deref().unwrap_or("")).map_err(error)?;
    start_request(
        &app,
        &state,
        Job {
            phase: "메모 저장 준비".into(),
            request_id: input.request_id.clone(),
            ..Default::default()
        },
        None,
        move |state| {
            let id = save(state, id, &input)?;
            state.archives.job.lock().unwrap().result_id = Some(id);
            Ok(())
        },
    )
    .await
}
pub async fn create_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(input): Json<Input>,
) -> ApiResult {
    start_save(app, state, None, input).await
}
pub async fn save_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    Json(input): Json<Input>,
) -> ApiResult {
    start_save(app, state, Some(id), input).await
}

pub async fn export_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult {
    start_export(app, state, id, headers, || {
        rfd::FileDialog::new()
            .set_file_name("note.json")
            .save_file()
    })
    .await
}

pub(super) async fn start_export(
    app: AppState,
    state: RequestState,
    id: i64,
    headers: HeaderMap,
    pick: impl FnOnce() -> Option<std::path::PathBuf> + Send + 'static,
) -> ApiResult {
    let request = request_identity(&headers, ("note-export", id))?;
    start_request(
        &app,
        &state,
        Job {
            phase: "메모 내보내기".into(),
            ..Default::default()
        },
        request,
        move |state| {
            state.archives.progress("메모 내보내기", 0, 0)?;
            validate(state, id)?;
            state.archives.progress("메모 내보내기", 0, 0)?;
            let destination = pick().context("요청을 취소했습니다.")?;
            state.archives.progress("메모 내보내기", 0, 0)?;
            let (_, rel, key) = ready(state, id)?;
            crypto::image::restore_notes(
                &crypto::safe_child(&state.root_dir, &rel)?,
                &key,
                &destination,
                |n| state.archives.progress("메모 내보내기", n, 0),
            )?;
            Ok(())
        },
    )
    .await
}

fn validate_request_id(id: &str) -> Result<()> {
    ensure!(
        id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        "저장 요청 번호가 올바르지 않습니다."
    );
    Ok(())
}

pub async fn save_status_api(
    Extension(state): Extension<RequestState>,
    Path(request_id): Path<String>,
) -> ApiResult {
    validate_request_id(&request_id).map_err(error)?;
    // Progress remains available even when a NAS file/DB operation is stalled.
    let job = state.archives.job.lock().unwrap().clone();
    if job.request_id.as_deref() == Some(request_id.as_str()) {
        return Ok(Json(serde_json::json!({"job":job})));
    }
    let previous = state
        .archives
        .history
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|j| j.request_id.as_deref() == Some(request_id.as_str()))
        .cloned();
    if let Some(job) = previous {
        return Ok(Json(serde_json::json!({"job":job})));
    }
    state
        .db_work(move |state| -> ApiResult {
            let g = state.conn.lock().unwrap();
            let conn = g.as_ref().context("DB 잠김").map_err(error)?;
            let saved: Option<i64> = conn
                .query_row(
                    "SELECT file_id FROM note_save_requests WHERE request_id=?1",
                    [request_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(error)?;
            Ok(Json(match saved {
                Some(id) => {
                    serde_json::json!({"job":{"running":false,"completed":true,"result_id":id}})
                }
                None => serde_json::json!({"status":"unknown"}),
            }))
        })
        .await
        .map_err(error)?
}
