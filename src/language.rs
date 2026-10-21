use std::sync::{
    atomic::{AtomicU8, Ordering},
    OnceLock,
};
static ACTIVE: AtomicU8 = AtomicU8::new(1);
pub fn default_code() -> String {
    "en".into()
}
pub fn valid(value: &str) -> bool {
    matches!(value, "ko" | "en" | "ja")
}
pub fn set(value: &str) {
    ACTIVE.store(
        match value {
            "ko" => 0,
            "ja" => 2,
            _ => 1,
        },
        Ordering::Release,
    );
}
pub fn code() -> &'static str {
    match ACTIVE.load(Ordering::Acquire) {
        0 => "ko",
        2 => "ja",
        _ => "en",
    }
}
pub fn translate(source: &str, language: &str) -> String {
    static CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../static/i18n-catalog.json"))
            .expect("validated translation catalog")
    });
    catalog
        .get(language)
        .and_then(|m| m.get(source))
        .and_then(|s| s.as_str())
        .unwrap_or(source)
        .to_owned()
}
pub fn t(source: &str) -> String {
    translate(source, code())
}

// The standalone restorer uses the same public preference file as the main app.
#[allow(dead_code)]
pub fn preference_path() -> std::path::PathBuf {
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .join("vault_config.json");
    if cwd.exists() || cwd.with_extension("json.bak").exists() {
        return cwd;
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("vault_config.json")))
        .unwrap_or(cwd)
}
#[allow(dead_code)]
pub fn load_preference(path: &std::path::Path) {
    for candidate in [path.to_owned(), path.with_extension("json.bak")] {
        if let Ok(bytes) = std::fs::read(candidate) {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                set(value
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("en"));
                return;
            }
        }
    }
    set("en");
}
#[allow(dead_code)]
pub fn save_preference(path: &std::path::Path, value: &str) -> anyhow::Result<()> {
    use std::io::Write;
    anyhow::ensure!(valid(value), "Unsupported language");
    let old = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::read(path.with_extension("json.bak")) {
                Ok(bytes) => serde_json::from_slice(&bytes)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                Err(e) => return Err(e.into()),
            }
        }
        Err(e) => return Err(e.into()),
    };
    let mut public = serde_json::Map::new();
    for key in ["gallery_folder", "gallery_frame_seconds"] {
        if let Some(v) = old.get(key) {
            public.insert(key.into(), v.clone());
        }
    }
    public.insert("language".into(), value.into());
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("{e}"))?;
    let temp = path.with_file_name(format!(".language-{:016x}.tmp", u64::from_le_bytes(random)));
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(&public)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    set(value);
    Ok(())
}
