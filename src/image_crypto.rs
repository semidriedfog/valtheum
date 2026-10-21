//! VTI1 image + independently sealed notes, carried by the existing 64 KiB stream.
use super::*;
use zeroize::Zeroize;
pub const NOTE_MAGIC: &[u8; 8] = b"VLTNOTE1";
pub const MAGIC: &[u8; 8] = b"VLTIMG01";
pub const MAX_NOTES: usize = 128;
pub const MAX_BODY: usize = 262144;
pub const MAX_TOTAL: u64 = 8 * 1024 * 1024;
pub const MAX_INDEX: usize = 262144;

fn valid_media(metadata: &Metadata, standalone: bool) -> bool {
    if standalone {
        metadata.format == "json"
            && metadata.name.ends_with(".json")
            && metadata.size == 0
            && metadata.sha256 == hex(&Sha256::digest(b""))
    } else {
        format(&metadata.name) == Some(metadata.format.as_str())
    }
}
pub fn write_note(
    target: &Path,
    name: &str,
    note: &Note,
    key: &[u8; 32],
    progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    write(
        &mut io::empty(),
        target,
        Metadata {
            name: name.into(),
            format: "json".into(),
            size: 0,
            sha256: hex(&Sha256::digest(b"")),
        },
        std::slice::from_ref(note),
        key,
        progress,
    )
}
pub fn format(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    ["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"]
        .into_iter()
        .find(|ext| lower.ends_with(&format!(".{ext}")))
}
#[derive(Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
}
impl Drop for Note {
    fn drop(&mut self) {
        self.title.zeroize();
        self.body.zeroize();
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub id: String,
    pub title: String,
    pub offset: u64,
    pub length: u64,
    pub nonce: [u8; 12],
}
impl Drop for Record {
    fn drop(&mut self) {
        self.title.zeroize();
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Index {
    pub revision: String,
    pub notes: Vec<Record>,
    pub notes_size: u64,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    #[serde(flatten)]
    media: Metadata,
    #[serde(flatten)]
    index: Index,
}
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn validate_notes(notes: &[Note]) -> Result<()> {
    ensure!(notes.len() <= MAX_NOTES, "메모는 최대 128개입니다.");
    let mut ids = std::collections::HashSet::new();
    let mut size = 0u64;
    for note in notes {
        ensure!(valid_id(&note.id) && ids.insert(&note.id), "메모 ID 오류");
        ensure!(
            note.title.len() <= 512 && note.body.len() <= MAX_BODY,
            "메모 제목 또는 본문이 너무 큽니다."
        );
        size += note.body.len() as u64 + 16;
    }
    ensure!(size <= MAX_TOTAL, "메모 전체 용량은 8 MiB 이하여야 합니다.");
    Ok(())
}
impl Index {
    pub(super) fn parse(
        bytes: &[u8],
        metadata: &Metadata,
        stream_len: u64,
        standalone: bool,
    ) -> Result<Self> {
        let manifest: Manifest = serde_json::from_slice(bytes)?;
        let index = manifest.index;
        ensure!(
            valid_id(&index.revision)
                && index.notes.len() <= MAX_NOTES
                && index.notes_size <= MAX_TOTAL,
            "메모 목록 크기 오류"
        );
        ensure!(
            valid_media(metadata, standalone) && (!standalone || index.notes.len() == 1),
            "이미지 메타데이터 형식 오류"
        );
        let mut ids = std::collections::HashSet::new();
        let mut nonces = std::collections::HashSet::new();
        let mut offset = 0;
        for record in &index.notes {
            ensure!(
                valid_id(&record.id)
                    && ids.insert(&record.id)
                    && nonces.insert(record.nonce)
                    && record.title.len() <= 512
                    && record.offset == offset
                    && (16..=MAX_BODY as u64 + 16).contains(&record.length),
                "메모 목록 형식 오류"
            );
            offset += record.length;
        }
        ensure!(
            offset == index.notes_size
                && metadata.size.checked_add(4 + bytes.len() as u64 + offset) == Some(stream_len),
            "메모 파일 길이 불일치"
        );
        Ok(index)
    }
}
pub fn encrypt(
    source: &mut File,
    target: &Path,
    name: &str,
    master: &[u8; 32],
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    let format = format(name).context("지원하지 않는 이미지 형식입니다.")?;
    let size = source.metadata()?.len();
    let sha256 = hash_file(source, |n| progress("원본 검사", n, size))?;
    write(
        source,
        target,
        Metadata {
            name: name.into(),
            format: format.into(),
            size,
            sha256,
        },
        &[],
        master,
        progress,
    )
}
pub fn write(
    source: &mut impl Read,
    target: &Path,
    metadata: Metadata,
    notes: &[Note],
    master: &[u8; 32],
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    validate_notes(notes)?;
    ensure!(
        valid_media(&metadata, metadata.format == "json")
            && metadata.size <= 1024u64.pow(4)
            && (metadata.format != "json" || notes.len() == 1),
        "이미지 메타데이터 형식 오류"
    );
    let file_key = Zeroizing::new(random::<32>()?);
    let standalone = metadata.format == "json";
    let mut envelope = if standalone { NOTE_MAGIC } else { MAGIC }.to_vec();
    envelope.extend(random::<16>()?);
    let wrap_nonce = random::<12>()?;
    envelope.extend(wrap_nonce);
    envelope.extend(seal(master, &wrap_nonce, &envelope, &*file_key)?);
    let note_key = stream_key(
        &*file_key,
        &envelope[8..24],
        if standalone {
            b"VTN1 note records"
        } else {
            b"VTI1 note records"
        },
    )?;
    let mut records = Vec::new();
    let mut bodies = Vec::new();
    for note in notes {
        let nonce = random::<12>()?;
        let sealed = seal(&*note_key, &nonce, note.id.as_bytes(), note.body.as_bytes())?;
        records.push(Record {
            id: note.id.clone(),
            title: note.title.clone(),
            offset: bodies.len() as u64,
            length: sealed.len() as u64,
            nonce,
        });
        bodies.extend(sealed);
    }
    let index = Index {
        revision: hex(&random::<16>()?),
        notes: records,
        notes_size: bodies.len() as u64,
    };
    let meta = Zeroizing::new(serde_json::to_vec(&Manifest {
        media: metadata.clone(),
        index,
    })?);
    ensure!(meta.len() <= MAX_INDEX, "메모 목록 크기 오류");
    let mut initial = Zeroizing::new((meta.len() as u32).to_le_bytes().to_vec());
    initial.extend_from_slice(&meta);
    let total = initial.len() as u64 + metadata.size + bodies.len() as u64;
    check_space(
        target.parent().context("저장 폴더 없음")?,
        total + total / 4096 + 1048576,
    )?;
    let salt = random::<32>()?;
    let np = random::<7>()?;
    let key = stream_key(&*file_key, &salt, &envelope)?;
    let aes = cipher(&*key);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let result = (|| -> Result<()> {
        output.write_all(&envelope)?;
        output.write_all(&[40])?;
        output.write_all(&salt)?;
        output.write_all(&np)?;
        let mut input = io::Cursor::new(initial)
            .chain(source.take(metadata.size))
            .chain(io::Cursor::new(bodies));
        let mut left = total;
        let mut i = 0u32;
        let mut buffer = Zeroizing::new(vec![0; NEXT as usize]);
        loop {
            let n = left.min(if i == 0 { FIRST } else { NEXT }) as usize;
            input.read_exact(&mut buffer[..n])?;
            let last = left == n as u64;
            output.write_all(
                &aes.encrypt(&Nonce::from(nonce(&np, i, last)), &buffer[..n])
                    .map_err(|_| anyhow::anyhow!("블록 암호화 실패"))?,
            )?;
            left -= n as u64;
            progress("암호화", total - left, total)?;
            if last {
                break;
            }
            i = i.checked_add(1).context("파일이 너무 큽니다.")?;
        }
        output.sync_all()?;
        Ok(())
    })();
    drop(output);
    let result = result.and_then(|_| {
        let verified = verify(target, master, |n| progress("사본 검증", n, metadata.size))?;
        ensure!(
            verified.sha256 == metadata.sha256 && verified.size == metadata.size,
            "이미지 검증 실패"
        );
        let mut reader = Reader::open(target, master)?;
        let actual = reader.all_notes()?;
        ensure!(actual == notes, "메모 원문 검증 실패");
        Ok(metadata)
    });
    if result.is_err() {
        let _ = fs::remove_file(target);
    }
    result
}
impl Reader {
    pub fn note_index(&self) -> Result<&Index> {
        self.image_index
            .as_ref()
            .context("메모를 지원하는 이미지가 아닙니다.")
    }
    pub fn read_note(&mut self, id: &str) -> Result<Note> {
        self.allowed()?;
        let record = self
            .note_index()?
            .notes
            .iter()
            .find(|r| r.id == id)
            .context("메모를 찾을 수 없습니다.")?
            .clone();
        let saved = (self.pos, self.data_start, self.data_len);
        self.pos = self.data_start + self.metadata.size + record.offset;
        self.data_start = 0;
        self.data_len = self.stream_len;
        let mut bytes = Zeroizing::new(vec![0; record.length as usize]);
        let result = self.read_exact(&mut bytes);
        (self.pos, self.data_start, self.data_len) = saved;
        result?;
        let key = self.note_key.as_ref().context("메모 키 없음")?;
        let plain = unseal(&**key, &record.nonce, record.id.as_bytes(), &bytes)?;
        Ok(Note {
            id: record.id.clone(),
            title: record.title.clone(),
            body: std::str::from_utf8(&plain)?.to_owned(),
        })
    }
    pub fn all_notes(&mut self) -> Result<Vec<Note>> {
        let ids: Vec<_> = self
            .note_index()?
            .notes
            .iter()
            .map(|r| r.id.clone())
            .collect();
        ids.iter().map(|id| self.read_note(id)).collect()
    }
}
pub fn restore_notes(
    path: &Path,
    key: &[u8; 32],
    destination: &Path,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    ensure!(!destination.try_exists()?, "대상 파일이 이미 있습니다.");
    let _guard = open_source(path)?;
    let verified = verify(path, key, &mut progress)?;
    let mut reader = Reader::open(path, key)?;
    let ids: Vec<_> = reader
        .note_index()?
        .notes
        .iter()
        .map(|r| r.id.clone())
        .collect();
    let mut notes = Vec::new();
    for id in &ids {
        progress(verified.size)?;
        notes.push(reader.read_note(id)?);
    }
    let bytes = Zeroizing::new(serde_json::to_vec_pretty(&notes)?);
    progress(verified.size)?;
    ensure!(!destination.try_exists()?, "대상 파일이 이미 있습니다.");
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let temp = parent.join(format!(".restore-notes-{}.tmp", hex(&random::<16>()?)));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        progress(verified.size)?;
        publish_new(&temp, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
#[path = "image_crypto_tests.rs"]
mod tests;
