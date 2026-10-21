//! VTV2: the existing Tink-compatible 64 KiB stream with an encrypted SHA-256 footer.
//! Original video bytes are read once; complete authenticated verification follows.
use super::*;

pub const MAGIC: &[u8; 8] = b"VLTARC02";

pub fn format(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    ["mp4", "webm", "mkv", "avi", "mov", "m4v", "wmv"]
        .into_iter()
        .find(|ext| lower.ends_with(&format!(".{ext}")))
}

struct HashingInput<'a> {
    source: &'a mut File,
    remaining: u64,
    sha: Sha256,
    footer: Option<Zeroizing<Vec<u8>>>,
    footer_position: usize,
}
impl Read for HashingInput<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.remaining > 0 {
            let count = out.len().min(self.remaining as usize);
            let n = self.source.read(&mut out[..count])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "영상 원본이 잘렸습니다.",
                ));
            }
            self.sha.update(&out[..n]);
            self.remaining -= n as u64;
            return Ok(n);
        }
        let footer = self
            .footer
            .get_or_insert_with(|| Zeroizing::new(self.sha.clone().finalize().to_vec()));
        let count = out.len().min(footer.len() - self.footer_position);
        out[..count].copy_from_slice(&footer[self.footer_position..self.footer_position + count]);
        self.footer_position += count;
        Ok(count)
    }
}

pub fn encrypt(
    source: &mut File,
    target: &Path,
    name: &str,
    master: &[u8; 32],
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    let format = format(name).context("지원하지 않는 영상 형식입니다.")?;
    let size = source.metadata()?.len();
    ensure!(
        size <= 1024u64.pow(4),
        "1 TiB를 넘는 영상은 지원하지 않습니다."
    );
    source.rewind()?;
    check_space(
        target.parent().context("저장 폴더 없음")?,
        size.saturating_add(size / 4096).saturating_add(1048576),
    )?;
    progress("해시·암호화", 0, size)?;
    let metadata = Metadata {
        name: name.into(),
        format: format.into(),
        size,
        sha256: String::new(),
    };
    let meta = serde_json::to_vec(&metadata)?;
    ensure!(meta.len() <= 16384, "파일명이 너무 깁니다.");
    let mut initial = (meta.len() as u32).to_le_bytes().to_vec();
    initial.extend(meta);
    let initial_len = initial.len() as u64;
    let total = initial_len + size + 32;
    let file_key = Zeroizing::new(random::<32>()?);
    let mut envelope = MAGIC.to_vec();
    envelope.extend(random::<16>()?);
    let wrap_nonce = random::<12>()?;
    envelope.extend(wrap_nonce);
    let wrapped = seal(master, &wrap_nonce, &envelope, &*file_key)?;
    envelope.extend(wrapped);
    let salt = random::<32>()?;
    let np = random::<7>()?;
    let key = stream_key(&*file_key, &salt, &envelope)?;
    let aes = cipher(&*key);
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let written = (|| -> Result<String> {
        out.write_all(&envelope)?;
        out.write_all(&[40])?;
        out.write_all(&salt)?;
        out.write_all(&np)?;
        let mut hashed = HashingInput {
            source,
            remaining: size,
            sha: Sha256::new(),
            footer: None,
            footer_position: 0,
        };
        {
            let mut input = io::Cursor::new(initial).chain(&mut hashed);
            let mut left = total;
            let mut index = 0u32;
            let mut buffer = Zeroizing::new(vec![0; NEXT as usize]);
            loop {
                let count = left.min(if index == 0 { FIRST } else { NEXT }) as usize;
                input.read_exact(&mut buffer[..count])?;
                let last = left == count as u64;
                out.write_all(
                    &aes.encrypt(&Nonce::from(nonce(&np, index, last)), &buffer[..count])
                        .map_err(|_| anyhow::anyhow!("영상 블록 암호화 실패"))?,
                )?;
                left -= count as u64;
                progress(
                    "해시·암호화",
                    (total - left).saturating_sub(initial_len).min(size),
                    size,
                )?;
                if last {
                    break;
                }
                index = index.checked_add(1).context("영상이 너무 큽니다.")?;
            }
        }
        ensure!(
            hashed.remaining == 0
                && hashed.footer_position == 32
                && hashed.source.metadata()?.len() == size,
            "영상 원본 길이가 변경되었습니다."
        );
        out.sync_all()?;
        Ok(hex(&hashed.sha.finalize()))
    })();
    drop(out);
    let result = written.and_then(|expected| {
        let verified = verify(target, master, |n| progress("사본 검증", n, size))?;
        ensure!(
            verified.sha256 == expected
                && verified.name == name
                && verified.size == size
                && verified.format == format,
            "영상 사본 검증 실패"
        );
        Ok(verified)
    });
    if result.is_err() {
        let _ = fs::remove_file(target);
    }
    result
}
