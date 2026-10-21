//! VTA1 / VTV2: AES-256-GCM envelopes around the Tink AES-GCM-HKDF stream.
//! Shared with the standalone restore executable. No database or UI dependencies.
#[path = "image_crypto.rs"]
pub mod image;
use aes_gcm::{
    aead::{Aead, AeadInOut, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{ensure, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zeroize::Zeroizing;
#[path = "video_crypto.rs"]
pub mod video;

pub const KEY_PATH: &str = ".valtheum/vault.key";
const KEY_MAGIC: &[u8; 8] = b"VLTKEY01";
const MAGIC: &[u8; 8] = b"VLTARC01";
const SEG: u64 = 65536;
const FIRST: u64 = SEG - 40 - 16;
const NEXT: u64 = SEG - 16;
const PREFIX: u64 = 84;
pub type Secret = Zeroizing<[u8; 32]>;
pub fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0; N];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(b)
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn cipher(key: &[u8]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).expect("32 byte key")
}
fn seal(key: &[u8], nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    cipher(key)
        .encrypt(&Nonce::from(*nonce), Payload { msg: plain, aad })
        .map_err(|_| anyhow::anyhow!("암호화 실패"))
}
fn unseal(key: &[u8], nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    cipher(key)
        .decrypt(&Nonce::from(*nonce), Payload { msg: ct, aad })
        .map(Zeroizing::new)
        .map_err(|_| anyhow::anyhow!("비밀번호/키가 맞지 않거나 파일이 손상되었습니다."))
}
fn derive(password: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<Secret> {
    ensure!(
        m == 65536 && t == 3 && p == 1,
        "지원하지 않는 키 파생 매개변수"
    );
    let params = Params::new(m, t, p, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(key)
}
pub fn make_key(password: &str) -> Result<(Vec<u8>, Secret)> {
    let master = Zeroizing::new(random::<32>()?);
    Ok((wrap_key(password, &master)?, master))
}
pub fn rewrap_key(bytes: &[u8], old_password: &str, new_password: &str) -> Result<Vec<u8>> {
    let master = read_key(bytes, old_password)?;
    let wrapped = wrap_key(new_password, &master)?;
    ensure!(
        *read_key(&wrapped, new_password)? == *master,
        "키 재보호 검증 실패"
    );
    Ok(wrapped)
}
fn wrap_key(password: &str, master: &[u8; 32]) -> Result<Vec<u8>> {
    ensure!(
        password.chars().count() >= 10 && password.len() <= 1024,
        "10자 이상 비밀번호가 필요합니다."
    );
    let salt = random::<16>()?;
    let nonce = random::<12>()?;
    let mut header = KEY_MAGIC.to_vec();
    for n in [65536u32, 3, 1] {
        header.extend(n.to_le_bytes());
    }
    header.extend(salt);
    header.extend(nonce);
    let key = derive(password, &salt, 65536, 3, 1)?;
    let sealed = seal(&*key, &nonce, &header, &*master)?;
    header.extend(sealed);
    Ok(header)
}
pub fn read_key(bytes: &[u8], password: &str) -> Result<Secret> {
    ensure!(
        bytes.len() == 96 && &bytes[..8] == KEY_MAGIC,
        "지원하지 않거나 손상된 vault.key"
    );
    let n = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let key = derive(password, &bytes[20..36], n(8), n(12), n(16))?;
    let plain = unseal(
        &*key,
        bytes[36..48].try_into().unwrap(),
        &bytes[..48],
        &bytes[48..],
    )?;
    Ok(Zeroizing::new(plain.as_slice().try_into()?))
}
#[allow(dead_code)] // Also used by the independent restore executable.
pub fn load_key(path: &Path, password: &str) -> Result<Secret> {
    ensure!(path.metadata()?.len() == 96, "손상된 vault.key");
    read_key(&fs::read(path)?, password)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub format: String,
    pub size: u64,
    pub sha256: String,
}
fn nonce(prefix: &[u8; 7], index: u32, last: bool) -> [u8; 12] {
    let mut n = [0; 12];
    n[..7].copy_from_slice(prefix);
    n[7..11].copy_from_slice(&index.to_be_bytes());
    n[11] = u8::from(last);
    n
}
fn stream_key(file_key: &[u8], salt: &[u8], aad: &[u8]) -> Result<Secret> {
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(salt), file_key)
        .expand(aad, &mut *key)
        .map_err(|_| anyhow::anyhow!("HKDF 오류"))?;
    Ok(key)
}
pub fn open_source(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    Ok(options.open(path)?)
}

/// Keep the verified file handle locked against writers until deletion is committed.
pub fn open_removable(path: &Path) -> Result<File> {
    let mut o = OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.access_mode(0x80000000 | 0x00010000).share_mode(1);
    }
    Ok(o.open(path)?)
}
pub fn delete_open(file: File) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        extern "system" {
            fn SetFileInformationByHandle(
                h: *mut std::ffi::c_void,
                class: u32,
                info: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        let delete = 1i32;
        let ok = unsafe {
            SetFileInformationByHandle(file.as_raw_handle(), 4, &delete as *const _ as *const _, 4)
        };
        ensure!(ok != 0, "원본 삭제 실패: {}", io::Error::last_os_error());
        drop(file);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        anyhow::bail!("원본 정리는 Windows에서 지원합니다.")
    }
}

pub fn check_space(folder: &Path, required: u64) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                path: *const u16,
                available: *mut u64,
                total: *mut u64,
                free: *mut u64,
            ) -> i32;
        }
        let path: Vec<u16> = folder
            .canonicalize()?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut available = 0;
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        ensure!(
            ok != 0,
            "남은 공간을 확인할 수 없습니다: {}",
            io::Error::last_os_error()
        );
        ensure!(
            available >= required,
            "저장 공간이 부족합니다. 필요한 추가 공간: {} MiB",
            required.div_ceil(1048576)
        );
    }
    #[cfg(not(windows))]
    {
        let _ = (folder, required);
    }
    Ok(())
}
/// Publish a synced sibling without replacing an existing destination.
/// Windows rename works on removable volumes that do not support hard links.
pub fn publish_new(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // WRITE_THROUGH only: no REPLACE_EXISTING and no cross-volume copy.
        let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 8) };
        ensure!(ok != 0, "파일 게시 실패: {}", io::Error::last_os_error());
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)?;
    }
    Ok(())
}
pub fn hash_file(file: &mut File, mut progress: impl FnMut(u64) -> Result<()>) -> Result<String> {
    file.rewind()?;
    let mut sha = Sha256::new();
    let mut b = Zeroizing::new(vec![0; 65536]);
    let mut done = 0;
    loop {
        let n = file.read(&mut b)?;
        if n == 0 {
            break;
        }
        sha.update(&b[..n]);
        done += n as u64;
        progress(done)?;
    }
    file.rewind()?;
    Ok(hex(&sha.finalize()))
}
pub fn encrypt(
    source: &mut File,
    target: &Path,
    name: &str,
    master: &[u8; 32],
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    if image::format(name).is_some() {
        return image::encrypt(source, target, name, master, progress);
    }
    if video::format(name).is_some() {
        return video::encrypt(source, target, name, master, progress);
    }
    let format = if name.to_ascii_lowercase().ends_with(".tar.zst") {
        "tar.zst"
    } else if name.to_ascii_lowercase().ends_with(".zip") {
        "zip"
    } else {
        anyhow::bail!("ZIP과 tar.zst만 지원합니다.")
    };
    let size = source.metadata()?.len();
    ensure!(
        size <= 1024 * 1024 * 1024 * 1024u64,
        "1 TiB를 넘는 압축 파일은 지원하지 않습니다."
    );
    check_space(
        target.parent().context("대상 폴더 없음")?,
        size.saturating_add(size / 4096).saturating_add(1048576),
    )?;
    let sha256 = hash_file(source, |n| progress("원본 검사", n, size))?;
    let metadata = Metadata {
        name: name.into(),
        format: format.into(),
        size,
        sha256,
    };
    encrypt_stream(source, target, metadata, master, progress)
}

// Re-encrypt directly from authenticated chunks; never write plaintext to disk.
pub fn rewrite_name(
    source: &Path,
    target: &Path,
    name: &str,
    master: &[u8; 32],
    progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    let mut reader = Reader::open(source, master)?;
    let mut metadata = reader.metadata.clone();
    ensure!(
        name.to_ascii_lowercase()
            .ends_with(&format!(".{}", metadata.format)),
        "압축 형식은 변경할 수 없습니다."
    );
    metadata.name = name.into();
    check_space(
        target.parent().context("대상 폴더 없음")?,
        metadata
            .size
            .saturating_add(metadata.size / 4096)
            .saturating_add(1048576),
    )?;
    encrypt_stream(&mut reader, target, metadata, master, progress)
}

pub(crate) fn encrypt_stream(
    source: &mut impl Read,
    target: &Path,
    metadata: Metadata,
    master: &[u8; 32],
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<Metadata> {
    let size = metadata.size;
    let meta = serde_json::to_vec(&metadata)?;
    ensure!(meta.len() <= 16384, "파일명이 너무 깁니다.");
    let mut initial = (meta.len() as u32).to_le_bytes().to_vec();
    initial.extend(meta);
    let total = size + initial.len() as u64;
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
    let result = (|| -> Result<()> {
        out.write_all(&envelope)?;
        out.write_all(&[40])?;
        out.write_all(&salt)?;
        out.write_all(&np)?;
        let mut input = io::Cursor::new(initial).chain(source);
        let mut left = total;
        let mut i = 0u32;
        loop {
            let count = left.min(if i == 0 { FIRST } else { NEXT }) as usize;
            let mut b = Zeroizing::new(vec![0; count]);
            input.read_exact(&mut b)?;
            let last = left == count as u64;
            let ct = aes
                .encrypt(&Nonce::from(nonce(&np, i, last)), b.as_slice())
                .map_err(|_| anyhow::anyhow!("블록 암호화 실패"))?;
            out.write_all(&ct)?;
            left -= count as u64;
            progress("암호화", total - left, total)?;
            if last {
                break;
            }
            i = i.checked_add(1).context("파일이 너무 큽니다.")?;
        }
        out.sync_all()?;
        Ok(())
    })();
    drop(out);
    if result.is_err() {
        let _ = fs::remove_file(target);
    }
    result?;
    verify(target, master, |n| progress("사본 검증", n, size))?;
    Ok(metadata)
}
pub struct Reader {
    f: BufReader<File>,
    aes: Aes256Gcm,
    np: [u8; 7],
    stream_len: u64,
    last: u32,
    pos: u64,
    data_start: u64,
    data_len: u64,
    pub metadata: Metadata,
    image_index: Option<image::Index>,
    pub standalone_note: bool,
    note_key: Option<Secret>,
    cached: u32,
    buf: Zeroizing<Vec<u8>>,
    revoked: Option<Arc<AtomicBool>>,
}
impl Reader {
    pub fn open(path: &Path, master: &[u8; 32]) -> Result<Self> {
        let mut f = BufReader::new(open_source(path)?);
        let len = f.get_ref().metadata()?.len();
        ensure!(len >= PREFIX + 56, "잘린 암호화 파일");
        let mut envelope = [0; 84];
        f.read_exact(&mut envelope)?;
        let video = &envelope[..8] == video::MAGIC;
        let standalone_note = &envelope[..8] == image::NOTE_MAGIC;
        let image = &envelope[..8] == image::MAGIC || standalone_note;
        ensure!(
            &envelope[..8] == MAGIC || video || image,
            "지원하지 않는 암호화 형식"
        );
        let file_key = unseal(
            master,
            envelope[24..36].try_into().unwrap(),
            &envelope[..36],
            &envelope[36..],
        )?;
        ensure!(file_key.len() == 32, "파일 키 오류");
        let mut header = [0; 40];
        f.read_exact(&mut header)?;
        ensure!(header[0] == 40, "잘못된 스트림 헤더");
        let key = stream_key(&file_key, &header[1..33], &envelope)?;
        let ct_len = len - PREFIX;
        let segments = ct_len.div_ceil(SEG);
        ensure!(
            segments > 0 && segments <= u32::MAX as u64,
            "잘못된 블록 수"
        );
        let final_size = ct_len - (segments - 1) * SEG;
        ensure!(
            final_size >= if segments == 1 { 56 } else { 17 },
            "잘린 마지막 블록"
        );
        let stream_len = ct_len - 40 - segments * 16;
        let mut r = Self {
            f,
            aes: cipher(&*key),
            np: header[33..40].try_into().unwrap(),
            stream_len,
            last: (segments - 1) as u32,
            pos: 0,
            data_start: 0,
            data_len: stream_len,
            metadata: Metadata {
                name: String::new(),
                format: String::new(),
                size: 0,
                sha256: String::new(),
            },
            image_index: None,
            standalone_note,
            note_key: if image {
                Some(stream_key(
                    &file_key,
                    &envelope[8..24],
                    if standalone_note {
                        b"VTN1 note records"
                    } else {
                        b"VTI1 note records"
                    },
                )?)
            } else {
                None
            },
            cached: u32::MAX,
            buf: Zeroizing::new(Vec::new()),
            revoked: None,
        };
        r.load(r.last)?; // Authenticate the end before exposing the length.
        let mut n = [0; 4];
        r.read_exact(&mut n)?;
        let n = u32::from_le_bytes(n) as usize;
        ensure!(
            n <= if image { image::MAX_INDEX } else { 16384 },
            "메타데이터 크기 오류"
        );
        let mut m = Zeroizing::new(vec![0; n]);
        r.read_exact(&mut m)?;
        let mut metadata: Metadata = serde_json::from_slice(&m)?;
        ensure!(metadata.size <= 1024u64.pow(4), "파일 크기 제한 초과");
        if video {
            ensure!(
                video::format(&metadata.name) == Some(metadata.format.as_str())
                    && metadata.sha256.is_empty(),
                "영상 메타데이터 형식 오류"
            );
        } else {
            ensure!(
                (image || matches!(metadata.format.as_str(), "zip" | "tar.zst"))
                    && metadata.sha256.len() == 64
                    && metadata.sha256.bytes().all(|c| c.is_ascii_hexdigit()),
                "메타데이터 형식 오류"
            );
        }
        if image {
            r.image_index = Some(image::Index::parse(
                &m,
                &metadata,
                stream_len,
                standalone_note,
            )?);
        }
        ensure!(
            image
                || metadata
                    .size
                    .checked_add(4 + n as u64 + if video { 32 } else { 0 })
                    == Some(stream_len),
            "파일 길이 불일치"
        );
        if video {
            r.pos = 4 + n as u64 + metadata.size;
            let mut digest = Zeroizing::new([0u8; 32]);
            r.read_exact(&mut *digest)?;
            metadata.sha256 = hex(&*digest);
        }
        r.data_len = metadata.size;
        r.metadata = metadata;
        r.data_start = 4 + n as u64;
        r.pos = 0;
        Ok(r)
    }
    pub fn with_revocation(mut self, flag: Arc<AtomicBool>) -> Self {
        self.revoked = Some(flag);
        self
    }
    fn allowed(&self) -> io::Result<()> {
        if self
            .revoked
            .as_ref()
            .is_some_and(|v| v.load(Ordering::Acquire))
        {
            Err(io::Error::other("라이브러리가 잠겼거나 변경되었습니다."))
        } else {
            Ok(())
        }
    }
    fn load(&mut self, i: u32) -> io::Result<()> {
        self.allowed()?;
        if self.cached == i {
            return Ok(());
        }
        let offset = PREFIX + if i == 0 { 40 } else { i as u64 * SEG };
        let start = if i == 0 {
            0
        } else {
            FIRST + (i as u64 - 1) * NEXT
        };
        let cap = if i == 0 { FIRST } else { NEXT };
        let n = (self.stream_len - start).min(cap) as usize;
        // A failed read/authentication must never retain the previous cache label.
        self.cached = u32::MAX;
        self.f.seek(SeekFrom::Start(offset))?;
        self.buf.resize(n + 16, 0);
        self.f.read_exact(&mut self.buf)?;
        self.aes
            .decrypt_in_place(
                &Nonce::from(nonce(&self.np, i, i == self.last)),
                &[],
                &mut *self.buf,
            )
            .map_err(|_| io::Error::other("암호화 파일 인증 실패"))?;
        self.cached = i;
        Ok(())
    }
}
impl Read for Reader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.allowed()?;
        let p = self.pos + self.data_start;
        if out.is_empty() || self.pos >= self.data_len {
            return Ok(0);
        }
        let (idx, off) = if p < FIRST {
            (0, p)
        } else {
            (1 + (p - FIRST) / NEXT, (p - FIRST) % NEXT)
        };
        self.load(idx as u32)?;
        let n = out
            .len()
            .min(self.buf.len() - off as usize)
            .min((self.data_len - self.pos) as usize);
        out[..n].copy_from_slice(&self.buf[off as usize..off as usize + n]);
        self.pos += n as u64;
        Ok(n)
    }
}
impl Seek for Reader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.allowed()?;
        let length = self.data_len;
        let n = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
            SeekFrom::End(n) => length as i128 + n as i128,
        };
        if n < 0 || n > length as i128 {
            return Err(io::Error::other("범위를 벗어난 위치"));
        }
        self.pos = n as u64;
        Ok(self.pos)
    }
}
pub fn verify(
    path: &Path,
    master: &[u8; 32],
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<Metadata> {
    let mut reader = Reader::open(path, master)?;
    let mut sha = Sha256::new();
    let mut b = Zeroizing::new(vec![0; 65536]);
    let mut count = 0;
    loop {
        let n = reader.read(&mut b)?;
        if n == 0 {
            break;
        }
        sha.update(&b[..n]);
        count += n as u64;
        progress(count)?;
    }
    ensure!(
        count == reader.metadata.size && hex(&sha.finalize()) == reader.metadata.sha256,
        "복원 데이터 검증 실패"
    );
    if reader.image_index.is_some() {
        reader.all_notes()?;
    }
    Ok(reader.metadata)
}
#[allow(dead_code)] // Also used by the independent restore executable.
pub fn restore(path: &Path, key: &[u8; 32], destination: &Path) -> Result<Metadata> {
    restore_progress(path, key, destination, |_| Ok(()))
}
pub fn restore_progress(
    path: &Path,
    key: &[u8; 32],
    destination: &Path,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<Metadata> {
    ensure!(!destination.try_exists()?, "대상 파일이 이미 있습니다.");
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let target = parent.join(destination.file_name().context("파일명 없음")?);
    let temp = parent.join(format!(".restore-{}.tmp", hex(&random::<16>()?)));
    let result = (|| -> Result<Metadata> {
        let mut reader = Reader::open(path, key)?;
        if reader.standalone_note {
            let metadata = reader.metadata.clone();
            drop(reader);
            image::restore_notes(path, key, destination, progress)?;
            return Ok(metadata);
        }
        check_space(&parent, reader.metadata.size.saturating_add(1048576))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let mut sha = Sha256::new();
        let mut b = Zeroizing::new(vec![0; 65536]);
        let mut done = 0;
        loop {
            let n = reader.read(&mut b)?;
            if n == 0 {
                break;
            }
            sha.update(&b[..n]);
            file.write_all(&b[..n])?;
            done += n as u64;
            progress(done)?;
        }
        progress(done)?;
        ensure!(
            hex(&sha.finalize()) == reader.metadata.sha256,
            "복원 검증 실패"
        );
        if reader.image_index.is_some() {
            reader.all_notes()?;
            progress(done)?;
        }
        file.sync_all()?;
        drop(file);
        publish_new(&temp, &target).context("복원 파일 게시 실패 (기존 파일은 보존)")?;
        Ok(reader.metadata)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub fn safe_child(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        !relative.contains(':')
            && !relative.contains('\\')
            && !relative.contains('\0')
            && !Path::new(relative).is_absolute()
            && Path::new(relative)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "안전하지 않은 경로"
    );
    let root = root.canonicalize()?;
    let path = root.join(relative);
    let parent = path.parent().context("부모 폴더 없음")?.canonicalize()?;
    ensure!(parent.starts_with(&root), "라이브러리 외부 경로");
    if path.try_exists()? {
        ensure!(
            path.canonicalize()?.starts_with(&root),
            "라이브러리 외부 파일"
        );
    }
    Ok(path)
}
#[cfg(test)]
#[path = "archive_crypto_tests.rs"]
mod tests;
