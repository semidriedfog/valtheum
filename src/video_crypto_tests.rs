use super::*;

#[test]
fn video_v2_roundtrip_seek_footer_boundaries_and_original_bytes() -> Result<()> {
    let f = Fixture::new();
    let key = random::<32>()?;
    let mut split = 65000u64;
    for _ in 0..3 {
        let meta = Metadata {
            name: "sample.mp4".into(),
            format: "mp4".into(),
            size: split,
            sha256: String::new(),
        };
        split = FIRST - 16 - 4 - serde_json::to_vec(&meta)?.len() as u64;
    }
    for (index, len) in [0, 1, split as usize, 65520, 65536, 131040, 220000]
        .into_iter()
        .enumerate()
    {
        let source = f.0.join(format!("{index}.mp4"));
        let target = f.0.join(format!("{index}.vtv"));
        let data: Vec<_> = (0..len).map(|i| ((i * 31 + i / 7) % 251) as u8).collect();
        fs::write(&source, &data)?;
        let mut phases = Vec::new();
        let meta = encrypt(
            &mut open_source(&source)?,
            &target,
            "sample.mp4",
            &key,
            |phase, _, _| {
                phases.push(phase.to_string());
                Ok(())
            },
        )?;
        assert!(!phases.iter().any(|p| p == "원본 검사"));
        assert!(phases.iter().any(|p| p == "해시·암호화"));
        assert_eq!(&fs::read(&target)?[..8], video::MAGIC);
        assert_eq!(meta.sha256, hex(&Sha256::digest(&data)));
        let mut reader = Reader::open(&target, &key)?;
        assert_eq!(reader.seek(SeekFrom::End(0))?, len as u64);
        assert_eq!(reader.read(&mut [0; 32])?, 0);
        assert!(reader.seek(SeekFrom::End(1)).is_err());
        for offset in [0, len / 2, len.saturating_sub(17)] {
            reader.seek(SeekFrom::Start(offset as u64))?;
            let mut bytes = vec![0; (len - offset).min(80000)];
            reader.read_exact(&mut bytes)?;
            assert_eq!(bytes, data[offset..offset + bytes.len()]);
        }
        drop(reader);
        let restored = f.0.join(format!("restored-{index}.mp4"));
        restore(&target, &key, &restored)?;
        assert_eq!(fs::read(restored)?, data);
        assert_eq!(fs::read(source)?, data);
    }
    Ok(())
}

#[test]
fn video_v2_rejects_tampering_reordering_truncation_and_revokes_cached_blocks() -> Result<()> {
    let f = Fixture::new();
    let key = random::<32>()?;
    let source = f.0.join("a.webm");
    let target = f.0.join("a.vtv");
    fs::write(&source, vec![71; 220000])?;
    encrypt(
        &mut open_source(&source)?,
        &target,
        "a.webm",
        &key,
        |_, _, _| Ok(()),
    )?;
    let good = fs::read(&target)?;
    let bad = f.0.join("bad.vtv");
    for index in [0, 12, 35, 64, 90, 125, 70000, good.len() - 1] {
        let mut bytes = good.clone();
        bytes[index] ^= 1;
        fs::write(&bad, bytes)?;
        assert!(verify(&bad, &key, |_| Ok(())).is_err());
    }
    let mut bytes = good.clone();
    let a = PREFIX as usize + SEG as usize;
    let b = a + SEG as usize;
    let first = bytes[a..b].to_vec();
    let second = bytes[b..b + SEG as usize].to_vec();
    bytes[a..b].copy_from_slice(&second);
    bytes[b..b + SEG as usize].copy_from_slice(&first);
    fs::write(&bad, bytes)?;
    assert!(verify(&bad, &key, |_| Ok(())).is_err());
    for len in [0, 84, good.len() - 1, good.len() - 32] {
        fs::write(&bad, &good[..len])?;
        assert!(Reader::open(&bad, &key).is_err());
    }
    let mut bytes = good;
    bytes.push(0);
    fs::write(&bad, bytes)?;
    assert!(Reader::open(&bad, &key).is_err());
    assert!(Reader::open(&target, &random::<32>()?).is_err());
    let revoked = Arc::new(AtomicBool::new(false));
    let mut reader = Reader::open(&target, &key)?.with_revocation(revoked.clone());
    reader.read_exact(&mut [0; 8])?;
    revoked.store(true, Ordering::Release);
    assert!(reader.read(&mut [0; 8]).is_err());
    assert!(reader.seek(SeekFrom::Start(0)).is_err());
    Ok(())
}

#[test]
fn video_v2_cancel_during_write_or_verify_preserves_source_and_cleans_output() -> Result<()> {
    let f = Fixture::new();
    let key = random::<32>()?;
    let source = f.0.join("cancel.mov");
    let target = f.0.join("cancel.vtv");
    fs::write(&source, vec![42; 200000])?;
    for phase in ["해시·암호화", "사본 검증"] {
        assert!(encrypt(
            &mut open_source(&source)?,
            &target,
            "cancel.mov",
            &key,
            |p, n, _| {
                ensure!(p != phase || n < 65536, "cancel test");
                Ok(())
            }
        )
        .is_err());
        assert!(!target.exists());
        assert_eq!(fs::read(&source)?, vec![42; 200000]);
    }
    fs::write(&target, b"existing destination")?;
    assert!(encrypt(
        &mut open_source(&source)?,
        &target,
        "cancel.mov",
        &key,
        |_, _, _| Ok(())
    )
    .is_err());
    assert_eq!(fs::read(&target)?, b"existing destination");
    Ok(())
}

#[test]
#[ignore = "writes public synthetic video interoperability fixtures into target"]
fn write_video_interop_fixture() -> Result<()> {
    let folder = PathBuf::from("target/video-v2-interop");
    fs::create_dir_all(&folder)?;
    let (key_bytes, key) = make_key("public fixture password 123")?;
    fs::write(folder.join("vault.key"), key_bytes)?;
    fs::write(
        folder.join("original.mp4"),
        (0..220000)
            .map(|i| ((i * 31 + i / 7) % 251) as u8)
            .collect::<Vec<_>>(),
    )?;
    let output = folder.join(format!("sample-{}.vtv", hex(&random::<8>()?)));
    encrypt(
        &mut open_source(&folder.join("original.mp4"))?,
        &output,
        "original.mp4",
        &key,
        |_, _, _| Ok(()),
    )?;
    fs::write(
        folder.join("latest.txt"),
        output.file_name().unwrap().to_string_lossy().as_bytes(),
    )?;
    Ok(())
}
