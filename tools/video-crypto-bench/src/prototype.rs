//! Performance prototype with a fixed authenticated header, individually authenticated
//! chunks and an authenticated SHA-256 footer. Not compatible with VTA1; not production.
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};
use zeroize::Zeroizing;
const HEADER: usize = 104;
const MAGIC: &[u8; 8] = b"BENCHV02";
fn cipher(key: &[u8]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).expect("AES-256 key")
}
fn nonce(prefix: &[u8], index: u32) -> [u8; 12] {
    let mut n = [0; 12];
    n[..8].copy_from_slice(prefix);
    n[8..].copy_from_slice(&index.to_be_bytes());
    n
}
fn aad(header: &[u8], index: u32, footer: bool) -> Vec<u8> {
    let mut a = header.to_vec();
    a.extend(index.to_be_bytes());
    a.push(footer as u8);
    a
}
fn seal(c: &Aes256Gcm, n: &[u8; 12], a: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    c.encrypt(&Nonce::from(*n), Payload { msg: plain, aad: a })
        .map_err(|_| anyhow::anyhow!("encrypt failed"))
}
fn unseal(c: &Aes256Gcm, n: &[u8; 12], a: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    c.decrypt(&Nonce::from(*n), Payload { msg: ct, aad: a })
        .map(Zeroizing::new)
        .map_err(|_| anyhow::anyhow!("authentication failed"))
}
pub fn encrypt(source: &Path, target: &Path, master: &[u8; 32], chunk: usize) -> Result<()> {
    ensure!([65536, 1048576].contains(&chunk), "chunk size");
    let mut source = File::open(source)?;
    let size = source.metadata()?.len();
    ensure!(size <= 2_000_000_000, "benchmark size limit");
    let key = Zeroizing::new(crate::current::random::<32>()?);
    let mut header = MAGIC.to_vec();
    header.extend(size.to_le_bytes());
    header.extend((chunk as u32).to_le_bytes());
    header.extend(crate::current::random::<16>()?);
    let wrap_nonce = crate::current::random::<12>()?;
    header.extend(wrap_nonce);
    let wrapped = seal(&cipher(master), &wrap_nonce, &header, &*key)?;
    header.extend(wrapped);
    let prefix = crate::current::random::<8>()?;
    header.extend(prefix);
    ensure!(header.len() == HEADER, "header size");
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    out.write_all(&header)?;
    let aes = cipher(&*key);
    let mut buf = Zeroizing::new(vec![0; chunk]);
    let mut sha = Sha256::new();
    let mut left = size;
    let mut index = 0;
    while left > 0 {
        let n = left.min(chunk as u64) as usize;
        source.read_exact(&mut buf[..n])?;
        sha.update(&buf[..n]);
        out.write_all(&seal(
            &aes,
            &nonce(&prefix, index),
            &aad(&header, index, false),
            &buf[..n],
        )?)?;
        left -= n as u64;
        index += 1;
    }
    out.write_all(&seal(
        &aes,
        &nonce(&prefix, index),
        &aad(&header, index, true),
        &sha.finalize(),
    )?)?;
    out.sync_all()?;
    Ok(())
}
struct Reader {
    file: BufReader<File>,
    header: Vec<u8>,
    aes: Aes256Gcm,
    size: u64,
    chunk: usize,
    blocks: u32,
}
impl Reader {
    fn open(path: &Path, master: &[u8; 32]) -> Result<Self> {
        let mut file = BufReader::with_capacity(65536, File::open(path)?);
        let mut header = vec![0; HEADER];
        file.read_exact(&mut header)?;
        ensure!(&header[..8] == MAGIC, "magic");
        let size = u64::from_le_bytes(header[8..16].try_into()?);
        let chunk = u32::from_le_bytes(header[16..20].try_into()?) as usize;
        ensure!(
            size <= 2_000_000_000 && [65536, 1048576].contains(&chunk),
            "header bounds"
        );
        let blocks = size.div_ceil(chunk as u64) as u32;
        ensure!(
            file.get_ref().metadata()?.len() == HEADER as u64 + size + 16 * blocks as u64 + 48,
            "truncation or trailing bytes"
        );
        let key = unseal(
            &cipher(master),
            header[36..48].try_into()?,
            &header[..48],
            &header[48..96],
        )?;
        Ok(Self {
            file,
            header,
            aes: cipher(&key),
            size,
            chunk,
            blocks,
        })
    }
    fn block(&mut self, index: u32) -> Result<Zeroizing<Vec<u8>>> {
        ensure!(index < self.blocks, "block bounds");
        let start = index as u64 * self.chunk as u64;
        let n = (self.size - start).min(self.chunk as u64) as usize;
        self.file.seek(SeekFrom::Start(
            HEADER as u64 + index as u64 * (self.chunk as u64 + 16),
        ))?;
        let mut ct = vec![0; n + 16];
        self.file.read_exact(&mut ct)?;
        unseal(
            &self.aes,
            &nonce(&self.header[96..], index),
            &aad(&self.header, index, false),
            &ct,
        )
    }
    fn footer(&mut self) -> Result<Zeroizing<Vec<u8>>> {
        self.file.seek(SeekFrom::Start(
            HEADER as u64 + self.size + 16 * self.blocks as u64,
        ))?;
        let mut ct = [0; 48];
        self.file.read_exact(&mut ct)?;
        unseal(
            &self.aes,
            &nonce(&self.header[96..], self.blocks),
            &aad(&self.header, self.blocks, true),
            &ct,
        )
    }
}
pub fn verify(path: &Path, master: &[u8; 32]) -> Result<String> {
    let mut reader = Reader::open(path, master)?;
    let mut sha = Sha256::new();
    for i in 0..reader.blocks {
        sha.update(&*reader.block(i)?);
    }
    let digest = sha.finalize();
    ensure!(&digest[..] == &reader.footer()?[..], "digest mismatch");
    Ok(crate::current::hex(&digest))
}
pub fn read_range(path: &Path, master: &[u8; 32], offset: u64, length: usize) -> Result<Vec<u8>> {
    let mut r = Reader::open(path, master)?;
    ensure!(
        offset
            .checked_add(length as u64)
            .context("range overflow")?
            <= r.size,
        "range bounds"
    );
    r.footer()?;
    let mut out = Vec::with_capacity(length);
    let mut pos = offset;
    while out.len() < length {
        let index = (pos / r.chunk as u64) as u32;
        let within = (pos % r.chunk as u64) as usize;
        let bytes = r.block(index)?;
        let n = (bytes.len() - within).min(length - out.len());
        out.extend(&bytes[within..within + n]);
        pos += n as u64;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_ranges_tampering_truncation_and_wrong_keys() -> Result<()> {
        let dir = std::env::current_dir()?
            .join("target/video-crypto-bench-test")
            .join(crate::current::hex(&crate::current::random::<16>()?));
        std::fs::create_dir_all(&dir)?;
        let source = dir.join("synthetic.bin");
        let output = dir.join("cipher.bin");
        let key = crate::current::random::<32>()?;
        for size in [0, 65537, 1100000] {
            for chunk in [65536, 1048576] {
                let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
                std::fs::write(&source, &data)?;
                encrypt(&source, &output, &key, chunk)?;
                ensure!(
                    verify(&output, &key)? == crate::current::hex(&Sha256::digest(&data)),
                    "roundtrip"
                );
                if size > 0 {
                    let offset = size / 2;
                    ensure!(
                        read_range(&output, &key, offset as u64, size - offset)? == data[offset..],
                        "range"
                    );
                }
                ensure!(verify(&output, &[0; 32]).is_err(), "wrong key accepted");
                let good = std::fs::read(&output)?;
                for index in [20, HEADER, good.len() - 1] {
                    let mut bad = good.clone();
                    bad[index] ^= 1;
                    std::fs::write(&output, &bad)?;
                    ensure!(verify(&output, &key).is_err(), "corruption accepted");
                }
                std::fs::write(&output, &good[..good.len() - 1])?;
                ensure!(verify(&output, &key).is_err(), "truncation accepted");
                std::fs::remove_file(&output)?;
            }
        }
        std::fs::remove_file(source)?;
        std::fs::remove_dir(dir)?;
        Ok(())
    }
}
