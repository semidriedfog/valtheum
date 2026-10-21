//! Benchmark only. AES container is NOT a production encryption format.
use aes_gcm::{
    aead::{Aead, AeadInOut, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::Result;
use serde_json::json;
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Instant,
};
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}
const ROOT: &str = "target/archive-crypto-bench-data";
const N: usize = 96;
const REPEATS: usize = 9;
fn nonce(i: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&i.to_be_bytes());
    n
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "authentication failed")
}
struct AesReader {
    f: BufReader<File>,
    cipher: Aes256Gcm,
    len: u64,
    chunk: usize,
    pos: u64,
    cached: u64,
    buf: Vec<u8>,
}
impl AesReader {
    fn open(path: &Path, key: &[u8; 32], chunk: usize) -> Result<Self> {
        let mut f = BufReader::new(File::open(path)?);
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        Ok(Self {
            f,
            cipher: Aes256Gcm::new_from_slice(key).unwrap(),
            len: u64::from_le_bytes(h),
            chunk,
            pos: 0,
            cached: u64::MAX,
            buf: Vec::new(),
        })
    }
}
impl Read for AesReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let i = self.pos / self.chunk as u64;
        if self.cached != i {
            let count = (self.len - i * self.chunk as u64).min(self.chunk as u64) as usize;
            self.f
                .seek(SeekFrom::Start(8 + i * (self.chunk as u64 + 16)))?;
            self.buf.resize(count + 16, 0);
            self.f.read_exact(&mut self.buf)?;
            self.cipher
                .decrypt_in_place(
                    &Nonce::from(nonce(i)),
                    &self.len.to_le_bytes(),
                    &mut self.buf,
                )
                .map_err(|_| invalid())?;
            self.cached = i;
        }
        let off = (self.pos % self.chunk as u64) as usize;
        let n = out.len().min(self.buf.len() - off);
        out[..n].copy_from_slice(&self.buf[off..off + n]);
        self.pos += n as u64;
        Ok(n)
    }
}
impl Seek for AesReader {
    fn seek(&mut self, s: SeekFrom) -> io::Result<u64> {
        let p = match s {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(n) => self.len as i128 + n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
        };
        if p < 0 || p > u64::MAX as i128 {
            return Err(invalid());
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}
struct Backend {
    name: String,
    path: PathBuf,
    kind: u8,
    chunk: usize,
    key: [u8; 32],
    identity: age::x25519::Identity,
}
impl Backend {
    fn open(&self) -> Result<Box<dyn ReadSeek>> {
        match self.kind {
            0 => Ok(Box::new(BufReader::new(File::open(&self.path)?))),
            1 => {
                let d = age::Decryptor::new_buffered(BufReader::new(File::open(&self.path)?))?;
                Ok(Box::new(d.decrypt(std::iter::once(
                    &self.identity as &dyn age::Identity,
                ))?))
            }
            _ => Ok(Box::new(AesReader::open(
                &self.path, &self.key, self.chunk,
            )?)),
        }
    }
}
fn prepare(source: &Path, rows: &mut Vec<serde_json::Value>) -> Result<Vec<Backend>> {
    let identity = age::x25519::Identity::generate();
    let mut result = Vec::new();
    for (kind, name, chunk) in [
        (0, "plain", 0),
        (1, "age-64KiB", 65536),
        (2, "aes-gcm-64KiB", 65536),
        (2, "aes-gcm-1MiB", 1048576),
    ] {
        let mut key = [0u8; 32];
        // Fresh benchmark-only key for every encrypted file and every run.
        // OS-generated key bytes supplied separately from age's RNG via Windows BCrypt below.
        random_key(&mut key)?;
        let path = if kind == 0 {
            source.to_path_buf()
        } else {
            source.with_extension(format!(
                "{}.{}",
                source.extension().unwrap().to_string_lossy(),
                name
            ))
        };
        if kind != 0 {
            let started = Instant::now();
            let mut input = BufReader::new(File::open(source)?);
            let out = BufWriter::new(File::create(&path)?);
            if kind == 1 {
                let recipient = identity.to_public();
                let enc = age::Encryptor::with_recipients(std::iter::once(
                    &recipient as &dyn age::Recipient,
                ))?;
                let mut w = enc.wrap_output(out)?;
                io::copy(&mut input, &mut w)?;
                w.finish()?.flush()?;
            } else {
                let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
                let len = source.metadata()?.len();
                let mut out = out;
                out.write_all(&len.to_le_bytes())?;
                let mut buf = vec![0; chunk];
                let mut done = 0u64;
                let mut idx = 0;
                while done < len {
                    let n = (len - done).min(chunk as u64) as usize;
                    input.read_exact(&mut buf[..n])?;
                    let ct = cipher
                        .encrypt(
                            &Nonce::from(nonce(idx)),
                            Payload {
                                msg: &buf[..n],
                                aad: &len.to_le_bytes(),
                            },
                        )
                        .map_err(|_| anyhow::anyhow!("encrypt"))?;
                    out.write_all(&ct)?;
                    done += n as u64;
                    idx += 1;
                }
                out.flush()?;
            }
            rows.push(json!({"format":source.extension().unwrap().to_string_lossy(),"backend":name,"operation":"encrypt_no_fsync","ms":started.elapsed().as_secs_f64()*1000.0}));
        }
        result.push(Backend {
            name: name.into(),
            path,
            kind,
            chunk,
            key,
            identity: identity.clone(),
        });
    }
    Ok(result)
}
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(h: *mut std::ffi::c_void, p: *mut u8, n: u32, flags: u32) -> i32;
}
fn random_key(k: &mut [u8; 32]) -> Result<()> {
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), k.as_mut_ptr(), 32, 2) };
    anyhow::ensure!(status >= 0, "RNG failed");
    Ok(())
}
fn zip_page(z: &mut zip::ZipArchive<Box<dyn ReadSeek>>, i: usize) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    z.by_name(&format!("{i:04}.jpg"))?.read_to_end(&mut b)?;
    Ok(b)
}
fn tar_page(b: &Backend, i: usize) -> Result<Vec<u8>> {
    let decoder = zstd::stream::read::Decoder::new(b.open()?)?;
    let mut archive = tar::Archive::new(decoder);
    let name = format!("{i:04}.jpg");
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == name {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
        io::copy(&mut entry, &mut io::sink())?;
    }
    anyhow::bail!("missing page")
}
fn record(
    rows: &mut Vec<serde_json::Value>,
    fmt: &str,
    b: &Backend,
    op: &str,
    mut samples: Vec<f64>,
) {
    samples.sort_by(f64::total_cmp);
    let v = json!({"format":fmt,"backend":b.name,"operation":op,"samples":samples.len(),"median_ms":samples[samples.len()/2],"p95_ms":samples[((samples.len() as f64*0.95).ceil() as usize-1).min(samples.len()-1)]});
    println!("{}", v);
    rows.push(v);
}
fn main() -> Result<()> {
    let root = Path::new(ROOT);
    let mut rows = Vec::new();
    println!(
        "CPU AES={} PCLMUL={}",
        std::is_x86_feature_detected!("aes"),
        std::is_x86_feature_detected!("pclmulqdq")
    );
    let zst = root.join("pages.tar.zst");
    {
        let mut input = BufReader::new(File::open(root.join("pages.tar"))?);
        let mut encoder =
            zstd::stream::write::Encoder::new(BufWriter::new(File::create(&zst)?), 3)?;
        io::copy(&mut input, &mut encoder)?;
        encoder.finish()?.flush()?;
    }
    let zip = prepare(&root.join("pages.zip"), &mut rows)?;
    let tar = prepare(&zst, &mut rows)?;
    let mut reference_zip = zip::ZipArchive::new(zip[0].open()?)?;
    let expected: Vec<Vec<u8>> = (0..N)
        .map(|i| zip_page(&mut reference_zip, i).unwrap())
        .collect();
    // Warm all filesystem reads consistently; no claim of cold-storage measurements.
    for b in zip.iter().chain(tar.iter()) {
        io::copy(&mut File::open(&b.path)?, &mut io::sink())?;
    }
    for b in &zip {
        let mut z = zip::ZipArchive::new(b.open()?)?;
        let _ = zip_page(&mut z, 0)?;
    }
    for (op, indices) in [
        ("open_first", vec![0]),
        ("open_middle", vec![47]),
        ("open_last", vec![95]),
        ("arena_two_fresh", vec![0, 0]),
    ] {
        let mut samples = vec![Vec::new(); zip.len()];
        for round in 0..REPEATS {
            for k in 0..zip.len() {
                let j = (k + round) % zip.len();
                let b = &zip[j];
                let start = Instant::now();
                let mut outputs = Vec::new();
                for i in &indices {
                    let mut z = zip::ZipArchive::new(b.open()?)?;
                    outputs.push(zip_page(&mut z, *i)?);
                }
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                for (out, i) in outputs.iter().zip(&indices) {
                    anyhow::ensure!(*out == expected[*i], "zip content mismatch");
                }
                samples[j].push(ms);
            }
        }
        for (b, s) in zip.iter().zip(samples) {
            record(&mut rows, "zip", b, op, s);
        }
    }
    for (op, indices) in [
        ("retained_sequential", (0..24).collect::<Vec<_>>()),
        (
            "retained_random",
            (0..24).map(|i| (i * 37 + 17) % N).collect(),
        ),
    ] {
        let mut samples = vec![Vec::new(); zip.len()];
        for round in 0..5 {
            for k in 0..zip.len() {
                let j = (k + round) % zip.len();
                let mut z = zip::ZipArchive::new(zip[j].open()?)?;
                for i in &indices {
                    let start = Instant::now();
                    let out = zip_page(&mut z, *i)?;
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    anyhow::ensure!(out == expected[*i], "retained mismatch");
                    samples[j].push(ms);
                }
            }
        }
        for (b, s) in zip.iter().zip(samples) {
            record(&mut rows, "zip", b, op, s);
        }
    }
    for (op, i) in [("open_first", 0), ("open_middle", 47), ("open_last", 95)] {
        let mut samples = vec![Vec::new(); tar.len()];
        for round in 0..REPEATS {
            for k in 0..tar.len() {
                let j = (k + round) % tar.len();
                let start = Instant::now();
                let out = tar_page(&tar[j], i)?;
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                anyhow::ensure!(out == expected[i], "tar mismatch");
                samples[j].push(ms);
            }
        }
        for (b, s) in tar.iter().zip(samples) {
            record(&mut rows, "tar.zst", b, op, s);
        }
    }
    // Verify every decrypted byte, independently of timed page requests.
    for (backends, original) in [(&zip, root.join("pages.zip")), (&tar, zst.clone())] {
        for backend in backends {
            let mut expected = BufReader::new(File::open(&original)?);
            let mut actual = backend.open()?;
            let mut left = vec![0; 65536];
            let mut right = vec![0; 65536];
            let mut remaining = original.metadata()?.len();
            while remaining > 0 {
                let n = remaining.min(65536) as usize;
                expected.read_exact(&mut left[..n])?;
                actual.read_exact(&mut right[..n])?;
                anyhow::ensure!(left[..n] == right[..n], "full roundtrip mismatch");
                remaining -= n as u64;
            }
            anyhow::ensure!(
                actual.read(&mut right[..1])? == 0,
                "unexpected extra plaintext"
            );
        }
    }
    println!("All complete archive roundtrips verified byte-for-byte.");
    let result = json!({"cpu":std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default(),"aes":std::is_x86_feature_detected!("aes"),"pclmul":std::is_x86_feature_detected!("pclmulqdq"),"pages":N,"zip_bytes":root.join("pages.zip").metadata()?.len(),"tar_zst_bytes":zst.metadata()?.len(),"warm_os_cache":true,"includes_image_decode":false,"includes_password_kdf":false,"rows":rows});
    fs::write(
        root.join("results.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(())
}
