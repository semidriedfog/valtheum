use super::*;
fn fixture() -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("valtheum-image-{}", hex(&random::<16>().unwrap())));
    fs::create_dir(&path).unwrap();
    path
}
#[test]
fn image_notes_roundtrip_random_access_verification_and_exports() -> Result<()> {
    let root = fixture();
    let master = random::<32>()?;
    let image: Vec<_> = (0..180000).map(|i| (i % 251) as u8).collect();
    let notes = vec![
        Note {
            id: hex(&random::<16>()?),
            title: "복구 코드 < >".into(),
            body: "  0O 1Il\t\r\n한글 日本語 🔐\n\u{200b}끝  ".into(),
        },
        Note {
            id: hex(&random::<16>()?),
            title: String::new(),
            body: "x".repeat(200000),
        },
    ];
    let metadata = Metadata {
        name: "photos/original.png".into(),
        format: "png".into(),
        size: image.len() as u64,
        sha256: hex(&Sha256::digest(&image)),
    };
    let path = root.join("photo.vti");
    write(
        &mut io::Cursor::new(&image),
        &path,
        metadata.clone(),
        &notes,
        &master,
        |_, _, _| Ok(()),
    )?;
    let raw = fs::read(&path)?;
    assert!(!raw
        .windows(notes[0].body.len())
        .any(|w| w == notes[0].body.as_bytes()));
    let mut reader = Reader::open(&path, &master)?;
    assert_eq!(reader.note_index()?.notes.len(), 2);
    assert!(reader.read_note(&notes[0].id)? == notes[0]);
    let mut first = vec![0; 70];
    reader.seek(SeekFrom::Start(65400))?;
    reader.read_exact(&mut first)?;
    assert_eq!(first, image[65400..65470]);
    assert!(reader.read_note(&notes[1].id)? == notes[1]);
    assert_eq!(reader.stream_position()?, 65470);
    verify(&path, &master, |_| Ok(()))?;
    restore(&path, &master, &root.join("restored.png"))?;
    assert_eq!(fs::read(root.join("restored.png"))?, image);
    restore_notes(&path, &master, &root.join("notes.json"), |_| Ok(()))?;
    let restored: Vec<Note> = serde_json::from_slice(&fs::read(root.join("notes.json"))?)?;
    assert!(restored == notes);
    assert!(restore_notes(&path, &master, &root.join("notes.json"), |_| Ok(())).is_err());
    drop(reader);
    let revoked = Arc::new(AtomicBool::new(false));
    let mut reader = Reader::open(&path, &master)?.with_revocation(revoked.clone());
    revoked.store(true, Ordering::Release);
    assert!(reader.read_note(&notes[0].id).is_err());
    drop(reader);
    // A changed interior ciphertext byte must not authenticate as a valid note or image.
    let damaged = root.join("damaged.vti");
    let mut bytes = raw;
    bytes[90000] ^= 1;
    fs::write(&damaged, bytes)?;
    assert!(verify(&damaged, &master, |_| Ok(())).is_err());
    let note_damage = root.join("note-damage.vti");
    let mut bytes = fs::read(&path)?;
    bytes[270000] ^= 1;
    fs::write(&note_damage, bytes)?;
    assert!(restore(&note_damage, &master, &root.join("rejected.png")).is_err());
    assert!(
        restore_notes(&note_damage, &master, &root.join("rejected.json"), |_| Ok(
            ()
        ))
        .is_err()
    );
    assert!(!root.join("rejected.png").exists() && !root.join("rejected.json").exists());
    assert!(Reader::open(&path, &random::<32>()?).is_err());
    let cancelled = root.join("cancelled.vti");
    assert!(write(
        &mut io::Cursor::new(&image),
        &cancelled,
        metadata,
        &notes,
        &master,
        |_, _, _| anyhow::bail!("cancel")
    )
    .is_err());
    assert!(!cancelled.exists());
    drop(restored);
    drop(notes);
    fs::remove_dir_all(root)?;
    Ok(())
}
#[test]
fn image_note_limits_and_duplicate_identity_reject_before_output() -> Result<()> {
    let root = fixture();
    let master = random::<32>()?;
    let note = Note {
        id: "a".repeat(32),
        title: String::new(),
        body: "secret".into(),
    };
    assert!(validate_notes(&[note.clone(), note.clone()]).is_err());
    assert!(validate_notes(&[Note {
        id: note.id.clone(),
        title: "x".repeat(513),
        body: String::new()
    }])
    .is_err());
    assert!(validate_notes(&[Note {
        id: note.id.clone(),
        title: String::new(),
        body: "x".repeat(MAX_BODY + 1)
    }])
    .is_err());
    let path = root.join("photo.vti");
    let metadata = Metadata {
        name: "x.png".into(),
        format: "png".into(),
        size: 5,
        sha256: hex(&Sha256::digest(b"image")),
    };
    assert!(write(
        &mut io::Cursor::new(b"image"),
        &path,
        metadata,
        &[note.clone(), note],
        &master,
        |_, _, _| Ok(())
    )
    .is_err());
    assert!(!path.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
#[ignore = "writes public synthetic interoperability fixtures under target"]
fn write_image_interop_fixture() -> Result<()> {
    let folder = PathBuf::from("target/image-v1-interop");
    fs::create_dir_all(&folder)?;
    let (key_bytes, key) = make_key("public fixture password 123")?;
    fs::write(folder.join("vault.key"), key_bytes)?;
    let bytes: Vec<u8> = (0..180000).map(|i| (i % 251) as u8).collect();
    fs::write(folder.join("original.png"), &bytes)?;
    let notes = vec![
        Note {
            id: hex(&random::<16>()?),
            title: "秘密 메모".into(),
            body: "  0O 1Il\t\r\n한글 日本語 🔐\n  ".into(),
        },
        Note {
            id: hex(&random::<16>()?),
            title: String::new(),
            body: "0123456789".repeat(19000),
        },
    ];
    fs::write(
        folder.join("expected-notes.json"),
        serde_json::to_vec(&notes)?,
    )?;
    let output = folder.join(format!("sample-{}.vti", hex(&random::<8>()?)));
    write(
        &mut io::Cursor::new(&bytes),
        &output,
        Metadata {
            name: "original.png".into(),
            format: "png".into(),
            size: bytes.len() as u64,
            sha256: hex(&Sha256::digest(&bytes)),
        },
        &notes,
        &key,
        |_, _, _| Ok(()),
    )?;
    fs::write(
        folder.join("latest.txt"),
        output.file_name().unwrap().to_string_lossy().as_bytes(),
    )?;
    Ok(())
}

#[test]
fn standalone_note_container_has_no_image_and_restores_exact_json() -> Result<()> {
    let root = fixture();
    let key = random::<32>()?;
    let note = Note {
        id: "a".repeat(32),
        title: "title / 日本語".into(),
        body: "  \t한글 🔐\r\nbody  ".into(),
    };
    let path = root.join("memo.vtn");
    write_note(&path, "notes/memo.json", &note, &key, |_, _, _| Ok(()))?;
    assert_eq!(&fs::read(&path)?[..8], NOTE_MAGIC);
    let mut r = Reader::open(&path, &key)?;
    assert!(r.standalone_note);
    assert_eq!(r.metadata.size, 0);
    assert!(r.read_note(&note.id)? == note);
    drop(r);
    restore(&path, &key, &root.join("memo.json"))?;
    let restored: Vec<Note> = serde_json::from_slice(&fs::read(root.join("memo.json"))?)?;
    assert!(restored == vec![note]);
    assert!(restore(&path, &key, &root.join("memo.json")).is_err());
    let mut data = fs::read(&path)?;
    let last = data.len() - 1;
    data[last] ^= 1;
    fs::write(root.join("bad.vtn"), data)?;
    assert!(restore(&root.join("bad.vtn"), &key, &root.join("bad.json")).is_err());
    assert!(!root.join("bad.json").exists());
    fs::remove_dir_all(root)?;
    Ok(())
}
#[test]
#[ignore = "writes public standalone note interoperability fixture under target"]
fn write_standalone_note_interop_fixture() -> Result<()> {
    let root = PathBuf::from("target/note-interop");
    fs::create_dir_all(&root)?;
    let (bytes, key) = make_key("public fixture password 123")?;
    fs::write(root.join("vault.key"), bytes)?;
    let note = Note {
        id: "a".repeat(32),
        title: "メモ / 메모".into(),
        body: "  한글 日本語 🔐\t\r\n".repeat(5000),
    };
    let target = root.join(format!("{}.vtn", hex(&random::<16>()?)));
    write_note(&target, "notes/note.json", &note, &key, |_, _, _| Ok(()))?;
    fs::write(root.join("expected.json"), serde_json::to_vec(&vec![note])?)?;
    fs::write(
        root.join("latest.txt"),
        target.file_name().unwrap().to_string_lossy().as_bytes(),
    )?;
    Ok(())
}
