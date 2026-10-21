use super::*;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use zeroize::Zeroizing;

#[cfg(test)]
#[path = "archive_page_delete_tests.rs"]
mod tests;

#[derive(serde::Deserialize)]
pub struct DeleteInput {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    entries: Vec<String>,
}

struct MeasuredWriter<W, F> {
    inner: W,
    sha: Sha256,
    size: u64,
    progress: F,
}
impl<W: Write, F: FnMut(u64) -> Result<()>> Write for MeasuredWriter<W, F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        (self.progress)(self.size).map_err(io::Error::other)?;
        if self.size.saturating_add(bytes.len() as u64) > 1024u64.pow(4) {
            return Err(io::Error::other("압축 파일 크기 제한 초과"));
        }
        let n = self.inner.write(bytes)?;
        self.sha.update(&bytes[..n]);
        self.size += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
struct PipeWriter(SyncSender<Zeroizing<Vec<u8>>>);
impl Write for PipeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = bytes.len().min(65536);
        if n > 0 {
            self.0
                .send(Zeroizing::new(bytes[..n].to_vec()))
                .map_err(|_| io::Error::other("작업 중단"))?;
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct PipeReader {
    receiver: Receiver<Zeroizing<Vec<u8>>>,
    chunk: io::Cursor<Zeroizing<Vec<u8>>>,
}
impl Read for PipeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.chunk.read(out)?;
            if n > 0 {
                return Ok(n);
            }
            match self.receiver.recv() {
                Ok(chunk) => self.chunk = io::Cursor::new(chunk),
                Err(_) => return Ok(0),
            }
        }
    }
}
struct InterruptibleReader {
    inner: crypto::Reader,
    library: Arc<Library>,
}
impl Read for InterruptibleReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.library.archives.cancel.load(Ordering::Acquire)
            || self.library.archives.revoked.load(Ordering::Acquire)
        {
            return Err(io::Error::other("작업이 취소되었습니다."));
        }
        self.inner.read(out)
    }
}
impl Seek for InterruptibleReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}
fn open_reader(
    path: &FsPath,
    key: &[u8; 32],
    library: &Arc<Library>,
) -> Result<archive::ArchiveReader> {
    Ok(Box::new(InterruptibleReader {
        inner: crypto::Reader::open(path, key)?,
        library: library.clone(),
    }))
}
fn names(
    path: &FsPath,
    key: &[u8; 32],
    format: &str,
    library: &Arc<Library>,
) -> Result<Vec<String>> {
    let reader = open_reader(path, key, library)?;
    if format == "zip" {
        archive::list_zip_images(&mut zip::ZipArchive::new(reader)?)
    } else {
        archive::list_tar_reader(reader)
    }
}

fn validate(library: &Library, id: i64, entries: &[String]) -> Result<()> {
    ensure!(
        !entries.is_empty()
            && entries.len() <= 100_000
            && entries
                .iter()
                .all(|entry| !entry.is_empty() && entry.len() <= 32768),
        "삭제할 이미지 파일명이 잘못되었습니다."
    );
    let guard = library.conn.lock().unwrap();
    let conn = guard.as_ref().context("DB 잠김")?;
    let valid: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM media_files m JOIN archive_store a ON a.file_id=m.id WHERE m.id=?1 AND m.file_type='ARCHIVE' AND m.is_trash=0 AND m.is_missing=0 AND a.state='ready')", [id], |r|r.get(0))?;
    ensure!(
        valid,
        "암호화가 완료된 미디어에서만 이미지를 삭제할 수 있습니다."
    );
    library.archives.secret()?;
    Ok(())
}

fn require_no_plaintext(library: &Library, name: &str) -> Result<()> {
    ensure!(
        !crypto::safe_child(&library.root_dir, name)?.try_exists()?,
        "평문 원본이 남아 있습니다. 미디어 암호화 관리에서 원본을 먼저 정리한 뒤 이미지를 삭제해 주세요."
    );
    Ok(())
}

#[cfg(test)]
pub(super) fn remove(library: &Arc<Library>, id: i64, entry: &str) -> Result<()> {
    remove_many(library, id, &[entry.to_owned()])
}

pub(super) fn remove_many(library: &Arc<Library>, id: i64, entries: &[String]) -> Result<()> {
    validate(library, id, entries)?;
    let selected: std::collections::HashSet<String> = entries.iter().cloned().collect();
    let (name, old_rel, key) = ready(library, id)?;
    require_no_plaintext(library, &name)?;
    let old = crypto::safe_child(&library.root_dir, &old_rel)?;
    let old_guard = crypto::open_source(&old)?;
    let mut metadata = crypto::verify(&old, &key, |n| {
        library.archives.progress("기존 파일 검증", n, 0)
    })?;
    ensure!(
        metadata.name == rename::restoration_name(library, id)?,
        "복원 파일명이 DB와 다릅니다."
    );
    let before = names(&old, &key, &metadata.format, library)?;
    let available: std::collections::HashSet<&String> = before.iter().collect();
    ensure!(
        selected.iter().all(|name| available.contains(name)),
        "삭제할 이미지가 없습니다. 목록을 새로 불러와 주세요."
    );
    let after: Vec<_> = before
        .iter()
        .filter(|name| !selected.contains(*name))
        .cloned()
        .collect();
    ensure!(
        !after.is_empty(),
        "마지막 이미지는 삭제할 수 없습니다. 압축 파일을 휴지통으로 이동해 주세요."
    );
    // First pass computes exact size/hash without storing the rebuilt plaintext.
    let mut measured = MeasuredWriter {
        inner: io::sink(),
        sha: Sha256::new(),
        size: 0,
        progress: |n| library.archives.progress("삭제 후 압축 파일 준비", n, 0),
    };
    archive::edit::rebuild(
        open_reader(&old, &key, library)?,
        &metadata.format,
        &selected,
        &mut measured,
        || library.archives.progress("삭제 후 압축 파일 준비", 0, 0),
    )?;
    metadata.size = measured.size;
    metadata.sha256 = crypto::hex(&measured.sha.finalize());
    let new_rel = format!(
        ".valtheum/archives/{}.vta",
        crypto::hex(&crypto::random::<16>()?)
    );
    let target = crypto::safe_child(&library.root_dir, &new_rel)?;
    ensure!(!target.try_exists()?, "새 파일 경로가 이미 있습니다.");
    crypto::check_space(
        target.parent().context("저장 폴더 없음")?,
        metadata
            .size
            .saturating_add(metadata.size / 4096)
            .saturating_add(1048576),
    )?;
    {
        let guard = library.conn.lock().unwrap();
        guard.as_ref().context("DB 잠김")?.execute(
            "INSERT INTO archive_gc(encrypted_path) VALUES(?1)",
            [&new_rel],
        )?;
    }
    let result = (|| -> Result<()> {
        std::thread::scope(|scope| -> Result<()> {
            let (sender, receiver) = sync_channel(2);
            let format = &metadata.format;
            let expected_hash = &metadata.sha256;
            let expected_size = metadata.size;
            let old = &old;
            let key = &key;
            let selected = &selected;
            let producer = scope.spawn(move || -> Result<()> {
                let mut output = MeasuredWriter {
                    inner: PipeWriter(sender),
                    sha: Sha256::new(),
                    size: 0,
                    progress: |_n| {
                        ensure!(
                            !library.archives.cancel.load(Ordering::Acquire)
                                && !library.archives.revoked.load(Ordering::Acquire),
                            "작업이 취소되었습니다."
                        );
                        Ok(())
                    },
                };
                archive::edit::rebuild(
                    open_reader(old, key, library)?,
                    format,
                    selected,
                    &mut output,
                    || {
                        ensure!(
                            !library.archives.cancel.load(Ordering::Acquire),
                            "작업이 취소되었습니다."
                        );
                        Ok(())
                    },
                )?;
                ensure!(
                    output.size == expected_size
                        && crypto::hex(&output.sha.finalize()) == *expected_hash,
                    "압축 재작성 결과가 일치하지 않습니다."
                );
                Ok(())
            });
            let mut input = PipeReader {
                receiver,
                chunk: io::Cursor::new(Zeroizing::new(Vec::new())),
            };
            let encrypted =
                crypto::encrypt_stream(&mut input, &target, metadata.clone(), &key, |p, n, t| {
                    library.archives.progress(p, n, t)
                });
            drop(input); // Release blocked producers on cancellation or disk errors.
            let produced = producer
                .join()
                .map_err(|_| anyhow::anyhow!("압축 재작성 중 오류"))?;
            produced?;
            encrypted?;
            Ok(())
        })?;
        ensure!(
            names(&target, &key, &metadata.format, library)? == after,
            "삭제 후 이미지 목록 검증 실패"
        );
        library.archives.progress("페이지 및 북마크 반영", 0, 0)?;
        require_no_plaintext(library, &name)?;
        let guard = library.conn.lock().unwrap();
        let conn = guard.as_ref().context("DB 잠김")?;
        let tx = conn.unchecked_transaction()?;
        let changed=tx.execute("UPDATE archive_store SET encrypted_path=?1,sha256=?2 WHERE file_id=?3 AND encrypted_path=?4 AND state='ready'",rusqlite::params![new_rel,metadata.sha256,id,old_rel])?;
        ensure!(changed == 1, "암호화 파일 연결이 변경되었습니다.");
        // Stable stored order is lexical, not the viewer's natural-number order.
        let bookmarks = tx
            .prepare("SELECT id,position FROM media_bookmarks WHERE file_id=?1 ORDER BY position")?
            .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (bookmark, position) in bookmarks {
            let next = before
                .get(position as usize)
                .filter(|_| position >= 0.0 && position.fract() == 0.0)
                .and_then(|name| after.iter().position(|n| n == name));
            if let Some(next) = next {
                tx.execute(
                    "UPDATE media_bookmarks SET position=?1 WHERE id=?2",
                    rusqlite::params![next as i64, bookmark],
                )?;
            } else {
                tx.execute("DELETE FROM media_bookmarks WHERE id=?1", [bookmark])?;
            }
        }
        let offset: Option<Option<i64>> = tx
            .query_row(
                "SELECT page_offset FROM thumbnails WHERE file_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(offset) = offset {
            let next = offset
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| before.get(n))
                .and_then(|name| after.iter().position(|n| n == name));
            if let Some(next) = next {
                tx.execute(
                    "UPDATE thumbnails SET page_offset=?1 WHERE file_id=?2",
                    rusqlite::params![next as i64, id],
                )?;
            } else {
                tx.execute("DELETE FROM thumbnails WHERE file_id=?1", [id])?;
            }
        }
        tx.execute(
            "UPDATE media_files SET file_size=?1 WHERE id=?2",
            rusqlite::params![metadata.size as i64, id],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO archive_gc(encrypted_path) VALUES(?1)",
            [&old_rel],
        )?;
        tx.execute("DELETE FROM archive_gc WHERE encrypted_path=?1", [&new_rel])?;
        tx.commit()?;
        Ok(())
    })();
    drop(old_guard);
    library.archives.clear_cache();
    if let Some(conn) = library.conn.lock().unwrap().as_ref() {
        let _ = rename::collect_unused(&library.root_dir, conn);
    }
    result
}

pub async fn delete_api(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(input): Json<DeleteInput>,
) -> ApiResult {
    let entries = match input.entry {
        Some(entry) if input.entries.is_empty() => vec![entry],
        None => input.entries,
        _ => return Err(error("삭제할 이미지 파일명이 잘못되었습니다.")),
    };
    let request = request_identity(&headers, ("pages", id, &entries))?;
    start_request(
        &app,
        &state,
        Job {
            phase: "이미지 삭제 준비".into(),
            ..Default::default()
        },
        request,
        move |state| {
            validate(state, id, &entries).and_then(|_| remove_many(&state.library, id, &entries))
        },
    )
    .await
}
