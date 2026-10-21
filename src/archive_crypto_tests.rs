use super::*;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("vta-test-{}", hex(&random::<16>().unwrap())));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("vta-test-"));
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn key_roundtrip_rejects_password_tamper_and_hostile_kdf() -> Result<()> {
    let (bytes, key) = make_key("fixture password 123")?;
    assert_eq!(*read_key(&bytes, "fixture password 123")?, *key);
    assert!(read_key(&bytes, "wrong").is_err());
    for i in [0, 8, 20, 36, 48, 95] {
        let mut bad = bytes.clone();
        bad[i] ^= 1;
        assert!(read_key(&bad, "fixture password 123").is_err());
    }
    Ok(())
}
#[test]
fn random_seek_restore_and_all_segment_boundaries() -> Result<()> {
    let f = Fixture::new();
    let key = random::<32>()?;
    for (j, len) in [0usize, 1, 65000, 65480, 65520, 65536, 131040, 200000]
        .into_iter()
        .enumerate()
    {
        let data: Vec<u8> = (0..len).map(|i| ((i * 31 + i / 7) % 251) as u8).collect();
        let source = f.0.join(format!("{j}.zip"));
        let target = f.0.join(format!("{j}.vta"));
        fs::write(&source, &data)?;
        let mut file = open_source(&source)?;
        let metadata = encrypt(&mut file, &target, "한글_01.zip", &key, |_, _, _| Ok(()))?;
        assert_eq!(metadata.size, len as u64);
        drop(file);
        let mut reader = Reader::open(&target, &key)?;
        for offset in [0, len / 2, len.saturating_sub(7), len] {
            reader.seek(SeekFrom::Start(offset as u64))?;
            let mut b = [0; 5];
            let n = reader.read(&mut b)?;
            assert_eq!(&b[..n], &data[offset..offset + n]);
        }
        drop(reader);
        let output = f.0.join(format!("out{j}.zip"));
        restore(&target, &key, &output)?;
        assert_eq!(fs::read(&output)?, data);
        assert!(restore(&target, &key, &output).is_err());
    }
    Ok(())
}
#[test]
fn corruption_truncation_reorder_append_and_revocation_fail() -> Result<()> {
    let f = Fixture::new();
    let key = random::<32>()?;
    let source = f.0.join("a.zip");
    let target = f.0.join("a.vta");
    fs::write(&source, vec![71; 220000])?;
    encrypt(
        &mut open_source(&source)?,
        &target,
        "a.zip",
        &key,
        |_, _, _| Ok(()),
    )?;
    let original = fs::read(&target)?;
    assert!(Reader::open(&target, &random::<32>()?).is_err());
    // Reading a bad segment must invalidate, rather than mislabel, the cache.
    let cache_test = f.0.join("cache-corrupt.vta");
    let mut damaged = original.clone();
    damaged[70000] ^= 1;
    fs::write(&cache_test, damaged)?;
    let mut reader = Reader::open(&cache_test, &key)?;
    reader.seek(SeekFrom::Start(70000))?;
    assert!(reader.read(&mut [0; 1]).is_err());
    reader.rewind()?;
    let mut byte = [0; 1];
    reader.read_exact(&mut byte)?;
    assert_eq!(byte, [71]);
    drop(reader);
    for offset in [0, 8, 24, 36, 84, 85, 117, 125, 70000, original.len() - 1] {
        let mut b = original.clone();
        b[offset] ^= 1;
        let path = f.0.join("corrupt.vta");
        fs::write(&path, b)?;
        assert!(verify(&path, &key, |_| Ok(())).is_err(), "offset {offset}");
    }
    for cut in [0, 84, 124, 65536 + 84, original.len() - 1] {
        let path = f.0.join("cut.vta");
        fs::write(&path, &original[..cut])?;
        assert!(verify(&path, &key, |_| Ok(())).is_err());
    }
    let path = f.0.join("append.vta");
    let mut b = original.clone();
    b.push(0);
    fs::write(&path, b)?;
    assert!(verify(&path, &key, |_| Ok(())).is_err());
    let mut b = original.clone();
    let a = b[84 + 65536..84 + 131072].to_vec();
    let c = b[84 + 131072..84 + 196608].to_vec();
    b[84 + 65536..84 + 131072].copy_from_slice(&c);
    b[84 + 131072..84 + 196608].copy_from_slice(&a);
    fs::write(&path, b)?;
    assert!(verify(&path, &key, |_| Ok(())).is_err());
    let flag = Arc::new(AtomicBool::new(false));
    let mut reader = Reader::open(&target, &key)?.with_revocation(flag.clone());
    flag.store(true, Ordering::Release);
    assert!(reader.read(&mut [0; 1]).is_err());
    assert!(reader.rewind().is_err());
    Ok(())
}
#[test]
fn cancelled_conversion_preserves_source() -> Result<()> {
    let f = Fixture::new();
    let source = f.0.join("a.zip");
    let target = f.0.join("a.vta");
    fs::write(&source, vec![19; 200000])?;
    let key = random::<32>()?;
    let result = encrypt(
        &mut open_source(&source)?,
        &target,
        "a.zip",
        &key,
        |phase, _, _| {
            ensure!(phase != "암호화", "cancel");
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!target.exists());
    assert_eq!(fs::read(source)?, vec![19; 200000]);
    Ok(())
}
#[test]
#[ignore = "writes only synthetic interoperability fixtures into target"]
fn write_interop_fixture() -> Result<()> {
    let root = Path::new("target/archive-interop");
    fs::create_dir_all(root)?;
    let (bytes, key) = make_key("interop password 123")?;
    fs::write(root.join("vault.key"), bytes)?;
    let source = root.join("original.zip");
    let target = root.join("sample.vta");
    if target.exists() {
        fs::remove_file(&target)?;
    }
    fs::write(
        &source,
        (0..180000).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
    )?;
    encrypt(
        &mut open_source(&source)?,
        &target,
        "original.zip",
        &key,
        |_, _, _| Ok(()),
    )?;
    Ok(())
}

#[test]
fn rewrite_name_streams_across_chunk_boundaries_and_rejects_corruption() -> Result<()> {
    let f = Fixture::new();
    let key = random::<32>()?;
    let data: Vec<u8> = (0..200000).map(|i| (i % 251) as u8).collect();
    let source = f.0.join("source.zip");
    fs::write(&source, &data)?;
    let original = f.0.join("original.vta");
    let renamed = f.0.join("renamed.vta");
    encrypt(
        &mut open_source(&source)?,
        &original,
        "old.zip",
        &key,
        |_, _, _| Ok(()),
    )?;
    rewrite_name(
        &original,
        &renamed,
        "nested/아주 긴 복원 파일명.zip",
        &key,
        |_, _, _| Ok(()),
    )?;
    let target = f.0.join("restored.zip");
    assert_eq!(
        restore(&renamed, &key, &target)?.name,
        "nested/아주 긴 복원 파일명.zip"
    );
    assert_eq!(fs::read(target)?, data);
    let mut corrupt = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&original)?;
    let offset = PREFIX + SEG + 30;
    corrupt.seek(SeekFrom::Start(offset))?;
    let mut byte = [0];
    corrupt.read_exact(&mut byte)?;
    corrupt.seek(SeekFrom::Start(offset))?;
    corrupt.write_all(&[byte[0] ^ 1])?;
    drop(corrupt);
    assert!(rewrite_name(
        &original,
        &f.0.join("bad.vta"),
        "bad.zip",
        &key,
        |_, _, _| Ok(())
    )
    .is_err());
    Ok(())
}

#[path = "video_crypto_tests.rs"]
mod video_tests;
