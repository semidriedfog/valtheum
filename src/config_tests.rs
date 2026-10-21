use super::*;

#[test]
fn startup_discards_legacy_library_paths_in_primary_and_backup_but_keeps_gallery(
) -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "vault-config-private-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let path = root.join("vault_config.json");
    let private = root.join("private-library");
    std::fs::create_dir(&private)?;
    let legacy = serde_json::json!({"folders":[{"path":private,"name":"private-library","last_used":1}],"active_folder":private,"gallery_folder":"D:/public-photos","gallery_frame_seconds":23});
    std::fs::write(&path, serde_json::to_vec(&legacy)?)?;
    std::fs::write(
        path.with_extension("json.bak"),
        serde_json::to_vec(&legacy)?,
    )?;
    let app = startup_state_at(&path)?;
    assert!(app.concealed.load(std::sync::atomic::Ordering::Acquire));
    assert!(app.snapshot().unwrap().root_dir.as_os_str().is_empty());
    assert!(!private.join(encryption::PLAIN_DB).exists());
    for file in [&path, &path.with_extension("json.bak")] {
        let bytes = std::fs::read(file)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert!(value.get("folders").is_none());
        assert!(value.get("active_folder").is_none());
        assert!(!String::from_utf8(bytes)?.contains("private-library"));
        assert_eq!(value["gallery_folder"], "D:/public-photos");
        assert_eq!(value["gallery_frame_seconds"], 23);
    }
    assert!(load_config_at(&path)?.folders.is_empty());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn corrupt_config_recovers_last_valid_copy_without_erasing_it() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "vault-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let path = root.join("vault_config.json");
    let mut first = load_config_at(&path)?;
    assert!(first.folders.is_empty());
    first.folders.push(SavedFolder {
        path: root.to_string_lossy().into_owned(),
        name: "Fixture".into(),
        last_used: 0,
    });
    first.active_folder = Some(first.folders[0].path.clone());
    first.gallery_frame_seconds = 12;
    save_config_at(&path, &first)?;
    let mut second = first.clone();
    second.folders[0].name = "Updated".into();
    second.gallery_frame_seconds = 20;
    save_config_at(&path, &second)?;
    assert_eq!(load_config_at(&path)?.folders[0].name, "Updated");
    std::fs::write(&path, b"{truncated")?;
    let recovered = load_config_at(&path)?;
    assert_eq!(recovered.gallery_frame_seconds, 12);
    assert_eq!(recovered.folders[0].name, "Updated");
    save_config_at(&path, &recovered)?;
    let backup: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("json.bak"))?)?;
    assert!(backup.get("folders").is_none());
    assert!(backup.get("active_folder").is_none());
    assert_eq!(backup["gallery_frame_seconds"], 12);
    std::fs::remove_file(&path)?;
    assert_eq!(load_config_at(&path)?.gallery_frame_seconds, 12);
    let app = startup_state_at(&path)?;
    assert!(app.concealed.load(std::sync::atomic::Ordering::Acquire));
    assert!(load_config_at(&path)?.folders.is_empty());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn unreadable_config_is_never_replaced_with_defaults() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "vault-config-bad-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let path = root.join("vault_config.json");
    std::fs::write(&path, b"{broken")?;
    assert!(load_config_at(&path).is_err());
    assert_eq!(std::fs::read(&path)?, b"{broken");
    std::fs::remove_dir_all(root)?;
    Ok(())
}
