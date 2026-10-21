use super::*;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Plaintext,
    Export,
}
#[derive(Deserialize)]
pub struct BatchInput {
    ids: Vec<i64>,
    action: Action,
}
#[derive(Clone, Default, Serialize)]
pub struct BatchProgress {
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current: String,
    pub failures: Vec<String>,
}

fn validate(library: &Library, ids: &[i64]) -> Result<()> {
    ensure!(
        !ids.is_empty() && ids.len() <= 10000,
        "1~10000개 파일을 선택해 주세요."
    );
    let guard = library.conn.lock().unwrap();
    let conn = guard.as_ref().context("DB가 잠겼습니다.")?;
    for id in ids {
        let eligible: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM media_files m JOIN archive_store a ON a.file_id=m.id WHERE m.id=?1 AND m.file_type IN ('ARCHIVE','VIDEO','IMAGE') AND COALESCE(m.is_trash,0)=0 AND a.state='ready')", [id], |r| r.get(0))?;
        ensure!(
            eligible,
            "암호화가 완료된 휴지통 밖의 파일만 일괄 처리할 수 있습니다. (ID {id})"
        );
    }
    library.archives.secret()?;
    Ok(())
}

// Create only validated descendants, checking each directory before descending.
fn destination(root: &FsPath, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && !name.contains([':', '\\', '\0']) && !FsPath::new(name).is_absolute(),
        "안전하지 않은 복원 경로"
    );
    ensure!(
        FsPath::new(name)
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_))),
        "안전하지 않은 복원 경로"
    );
    let root = root.canonicalize()?;
    let mut parent = root.clone();
    let parts: Vec<_> = FsPath::new(name).components().collect();
    for part in &parts[..parts.len() - 1] {
        parent.push(part.as_os_str());
        if !parent.try_exists()? {
            fs::create_dir(&parent)?;
        }
        ensure!(
            parent.is_dir() && parent.canonicalize()?.starts_with(&root),
            "복원 폴더 밖으로 연결된 경로입니다."
        );
    }
    crypto::safe_child(&root, name)
}

pub(super) fn restore_one(library: &Library, id: i64, folder: &FsPath) -> Result<bool> {
    let (_, rel, key) = ready(library, id)?;
    let name = rename::restoration_name(library, id)?;
    let source = crypto::safe_child(&library.root_dir, &rel)?;
    let reader = crypto::Reader::open(&source, &key)?;
    ensure!(
        reader.metadata.name == name,
        "압축 파일의 원래 경로가 다릅니다."
    );
    let size = reader.metadata.size;
    drop(reader);
    let target = destination(folder, &name)?;
    if target.try_exists()? {
        return Ok(false);
    }
    crypto::restore_progress(&source, &key, &target, |n| {
        library.archives.progress("원본 복원", n, size)
    })?;
    Ok(true)
}

fn run(library: &Library, ids: &[i64], action: Action, folder: Option<&FsPath>) -> Result<()> {
    for id in ids {
        library.archives.progress("다음 파일 준비", 0, 0)?;
        let name = logical(library, *id)?.0;
        library
            .archives
            .job
            .lock()
            .unwrap()
            .batch
            .as_mut()
            .unwrap()
            .current = name.clone();
        let result = if action == Action::Plaintext {
            if !library.root_dir.join(&name).try_exists()? {
                Ok(false)
            } else {
                cleanup(library, *id).map(|_| true)
            }
        } else {
            restore_one(library, *id, folder.context("복원 폴더 없음")?)
        };
        let cancelled = library.archives.cancel.load(Ordering::Acquire);
        let mut job = library.archives.job.lock().unwrap();
        let batch = job.batch.as_mut().unwrap();
        match result {
            Ok(true) => batch.completed += 1,
            Ok(false) => batch.skipped += 1,
            Err(_) if cancelled => {
                anyhow::bail!("작업을 취소했습니다. 이미 완료한 파일은 유지됩니다.")
            }
            Err(err) => {
                batch.failed += 1;
                if batch.failures.len() < 20 {
                    batch.failures.push(format!("{name}: {err:#}"));
                }
            }
        }
    }
    Ok(())
}

pub async fn batch_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    headers: HeaderMap,
    Json(input): Json<BatchInput>,
) -> ApiResult {
    let mut ids = input.ids;
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(*id));
    let request = request_identity(&headers, ("batch", input.action, &ids))?;
    start_request(
        &app,
        &state,
        Job {
            phase: "일괄 작업 준비".into(),
            batch: Some(BatchProgress {
                total: ids.len(),
                ..Default::default()
            }),
            ..Default::default()
        },
        request,
        move |state| {
            validate(state, &ids)?;
            let folder = if input.action == Action::Export {
                state.archives.job.lock().unwrap().phase = "복원할 폴더를 선택해 주세요.".into();
                Some(
                    rfd::FileDialog::new()
                        .set_title(crate::language::t(
                            "원본 일괄 복원 폴더 선택 (기존 파일은 건너뜁니다)",
                        ))
                        .pick_folder()
                        .context("복원 폴더 선택을 취소했습니다.")?,
                )
            } else {
                None
            };
            run(state, &ids, input.action, folder.as_deref())
        },
    )
    .await
}

#[cfg(test)]
#[path = "archive_security_batch_tests.rs"]
mod tests;
