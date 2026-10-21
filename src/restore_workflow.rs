//! Database-free restore workflow shared by the desktop UI and its tests.
use crate::archive_crypto as crypto;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn read_key_source(path: &Path, password: &str) -> Result<crypto::Secret> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("key"))
    {
        return crypto::load_key(path, password);
    }
    let mut zip = zip::ZipArchive::new(crypto::open_source(path)?)
        .context("키 백업을 읽을 수 없습니다. vault.key 또는 정상적인 vtbackup을 선택해 주세요.")?;
    let mut entry = zip.by_name(crypto::KEY_PATH)
        .context("이 백업에 vault.key가 없습니다. 미디어 암호화 후 만든 백업 또는 해당 vault.key를 선택해 주세요.")?;
    ensure!(entry.size() == 96, "백업의 vault.key가 손상되었습니다.");
    let mut bytes = Vec::new();
    (&mut entry).take(97).read_to_end(&mut bytes)?;
    crypto::read_key(&bytes, password)
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub index: usize,
    pub total: usize,
    pub name: String,
    pub done: u64,
    pub bytes: u64,
}
#[derive(Clone, Serialize)]
pub struct FileResult {
    pub source: String,
    pub output: Option<String>,
    pub error: Option<String>,
}
#[derive(Default, Serialize)]
pub struct Summary {
    pub success: usize,
    pub failed: usize,
    pub cancelled: bool,
}

fn output_name(metadata: &crypto::Metadata, index: usize) -> String {
    // Embedded paths never determine directories outside the user-selected folder.
    let base = metadata.name.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = base.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    let invalid = base.is_empty()
        || base.encode_utf16().count() > 240
        || reserved
        || base.ends_with(['.', ' '])
        || base
            .chars()
            .any(|c| c.is_control() || "<>:\"|?*".contains(c))
        || !base
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", metadata.format));
    if invalid {
        format!("recovered-{}.{}", index + 1, metadata.format)
    } else {
        base.to_owned()
    }
}

pub fn restore_files(
    key: &crypto::Secret,
    files: &[PathBuf],
    destination: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
    completed: impl FnMut(FileResult),
) -> Result<Summary> {
    restore_files_mode(key, files, destination, cancel, false, progress, completed)
}
pub fn restore_files_mode(
    key: &crypto::Secret,
    files: &[PathBuf],
    destination: &Path,
    cancel: &AtomicBool,
    notes_only: bool,
    mut progress: impl FnMut(Progress),
    mut completed: impl FnMut(FileResult),
) -> Result<Summary> {
    ensure!(
        !files.is_empty(),
        "복원할 .vta, .vtv, .vti 또는 .vtn 파일을 선택해 주세요."
    );
    ensure!(
        destination.is_dir(),
        "복원 결과를 저장할 폴더를 선택해 주세요."
    );
    let destination = destination.canonicalize()?;
    let mut summary = Summary::default();
    for (index, file) in files.iter().enumerate() {
        if cancel.load(Ordering::Acquire) {
            summary.cancelled = true;
            break;
        }
        let source = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let result = (|| -> Result<PathBuf> {
            // Hold a write-denying handle throughout metadata lookup and restoration.
            let _guard = crypto::open_source(file)?;
            let metadata = crypto::Reader::open(file, key)?.metadata;
            let name = output_name(&metadata, index);
            let output = destination.join(if notes_only {
                format!("{name}.notes.json")
            } else {
                name
            });
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut report = |done| -> Result<()> {
                ensure!(!cancel.load(Ordering::Acquire), "복원이 취소되었습니다.");
                if done == 0
                    || done == metadata.size
                    || last.elapsed() >= Duration::from_millis(100)
                {
                    progress(Progress {
                        index: index + 1,
                        total: files.len(),
                        name: metadata.name.clone(),
                        done,
                        bytes: metadata.size,
                    });
                    last = Instant::now();
                }
                Ok(())
            };
            report(0)?;
            if notes_only {
                crypto::image::restore_notes(file, key, &output, report)?;
            } else {
                crypto::restore_progress(file, key, &output, report)?;
            }
            Ok(output)
        })();
        match result {
            Ok(output) => {
                summary.success += 1;
                completed(FileResult {
                    source,
                    output: Some(output.display().to_string()),
                    error: None,
                });
            }
            Err(error) => {
                if cancel.load(Ordering::Acquire) {
                    summary.cancelled = true;
                    break;
                }
                summary.failed += 1;
                completed(FileResult {
                    source,
                    output: None,
                    error: Some(format!("{error:#}")),
                });
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "restore-ui-test-{}",
                crypto::hex(&crypto::random::<16>().unwrap())
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn backup_key_and_batch_restore_reject_missing_key_collision_and_corruption() -> Result<()> {
        use std::io::Write;
        let f = Fixture::new();
        let fixture = Path::new("tests/fixtures/vta1");
        let backup = f.0.join("test.vtbackup");
        let mut zip = zip::ZipWriter::new(fs::File::create(&backup)?);
        zip.start_file(crypto::KEY_PATH, zip::write::SimpleFileOptions::default())?;
        zip.write_all(&fs::read(fixture.join("vault.key"))?)?;
        zip.finish()?;
        let key = read_key_source(&backup, "interop password 123")?;
        assert!(read_key_source(&backup, "wrong password").is_err());
        assert_eq!(
            *key,
            *read_key_source(&fixture.join("vault.key"), "interop password 123")?
        );
        let empty = f.0.join("empty.vtbackup");
        zip::ZipWriter::new(fs::File::create(&empty)?).finish()?;
        assert!(read_key_source(&empty, "interop password 123")
            .unwrap_err()
            .to_string()
            .contains("vault.key"));
        let output = f.0.join("output");
        fs::create_dir(&output)?;
        let bad = f.0.join("bad.vta");
        fs::write(&bad, b"corrupt")?;
        let files = vec![fixture.join("sample.vta"), bad, fixture.join("sample.vta")];
        let cancel = AtomicBool::new(false);
        let mut records = Vec::new();
        let result = restore_files(&key, &files, &output, &cancel, |_| {}, |r| records.push(r))?;
        assert_eq!((result.success, result.failed), (1, 2));
        assert_eq!(
            fs::read(output.join("original.zip"))?,
            fs::read(fixture.join("original.zip"))?
        );
        assert!(records[2].error.as_ref().unwrap().contains("이미"));
        fs::remove_file(output.join("original.zip"))?;
        let result = restore_files(
            &key,
            &files,
            &output,
            &cancel,
            |p| {
                if p.done > 0 {
                    cancel.store(true, Ordering::Release);
                }
            },
            |_| {},
        )?;
        assert!(result.cancelled);
        assert_eq!(fs::read_dir(&output)?.count(), 0);
        Ok(())
    }
    #[test]
    fn rewritten_name_is_used_by_database_free_restore() -> Result<()> {
        let f = Fixture::new();
        let key = zeroize::Zeroizing::new(crypto::random::<32>()?);
        let source = f.0.join("source.zip");
        fs::write(&source, b"original archive bytes")?;
        let encrypted = f.0.join("old.vta");
        crypto::encrypt(
            &mut crypto::open_source(&source)?,
            &encrypted,
            "original.zip",
            &key,
            |_, _, _| Ok(()),
        )?;
        let renamed = f.0.join("new.vta");
        let name = format!("{}.zip", "이름".repeat(35));
        crypto::rewrite_name(
            &encrypted,
            &renamed,
            &format!("nested/{name}"),
            &key,
            |_, _, _| Ok(()),
        )?;
        let output = f.0.join("output");
        fs::create_dir(&output)?;
        let summary = restore_files(
            &key,
            &[renamed],
            &output,
            &AtomicBool::new(false),
            |_| {},
            |_| {},
        )?;
        assert_eq!(summary.success, 1);
        assert_eq!(fs::read(output.join(name))?, b"original archive bytes");
        Ok(())
    }
    #[test]
    fn video_v2_restores_original_name_and_bytes_without_database() -> Result<()> {
        let f = Fixture::new();
        let key = zeroize::Zeroizing::new(crypto::random::<32>()?);
        let original = vec![71u8; 170000];
        let source = f.0.join("source.mp4");
        fs::write(&source, &original)?;
        let encrypted = f.0.join("video.vtv");
        crypto::encrypt(
            &mut crypto::open_source(&source)?,
            &encrypted,
            "nested/영상.mp4",
            &key,
            |_, _, _| Ok(()),
        )?;
        let output = f.0.join("output");
        fs::create_dir(&output)?;
        let summary = restore_files(
            &key,
            &[encrypted],
            &output,
            &AtomicBool::new(false),
            |_| {},
            |_| {},
        )?;
        assert_eq!((summary.success, summary.failed), (1, 0));
        assert_eq!(fs::read(output.join("영상.mp4"))?, original);
        Ok(())
    }
    #[test]
    fn image_notes_restore_mode_exports_only_explicit_json() -> Result<()> {
        use sha2::{Digest, Sha256};
        let f = Fixture::new();
        let key = zeroize::Zeroizing::new(crypto::random::<32>()?);
        let bytes = b"synthetic image";
        let notes = vec![crypto::image::Note {
            id: "a".repeat(32),
            title: "title".into(),
            body: "  text\r\n日本語  ".into(),
        }];
        let encrypted = f.0.join("photo.vti");
        crypto::image::write(
            &mut std::io::Cursor::new(bytes),
            &encrypted,
            crypto::Metadata {
                name: "nested/photo.png".into(),
                format: "png".into(),
                size: bytes.len() as u64,
                sha256: crypto::hex(&Sha256::digest(bytes)),
            },
            &notes,
            &key,
            |_, _, _| Ok(()),
        )?;
        let out = f.0.join("out");
        fs::create_dir(&out)?;
        let summary = restore_files_mode(
            &key,
            &[encrypted.clone()],
            &out,
            &AtomicBool::new(false),
            true,
            |_| {},
            |_| {},
        )?;
        assert_eq!((summary.success, summary.failed), (1, 0));
        assert!(!out.join("photo.png").exists());
        let restored: Vec<crypto::image::Note> =
            serde_json::from_slice(&fs::read(out.join("photo.png.notes.json"))?)?;
        assert!(restored == notes);
        let summary = restore_files(
            &key,
            &[encrypted],
            &out,
            &AtomicBool::new(false),
            |_| {},
            |_| {},
        )?;
        assert_eq!(summary.success, 1);
        assert_eq!(fs::read(out.join("photo.png"))?, bytes);
        Ok(())
    }
    #[test]
    fn embedded_names_cannot_escape_output_or_use_windows_devices() {
        let mut m = crypto::Metadata {
            name: "../../book.zip".into(),
            format: "zip".into(),
            size: 0,
            sha256: String::new(),
        };
        assert_eq!(output_name(&m, 0), "book.zip");
        for name in [
            "CON.zip",
            "C:evil.zip",
            "a?b.zip",
            "x.zip ",
            "not-an-archive",
            "",
        ] {
            m.name = name.into();
            assert_eq!(output_name(&m, 0), "recovered-1.zip");
        }
        m.name = "시리즈/제1권.tar.zst".into();
        m.format = "tar.zst".into();
        assert_eq!(output_name(&m, 0), "제1권.tar.zst");
    }
}
