use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read, Seek, Take};
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}
pub type ArchiveReader = Box<dyn ReadSeek>;
pub type ZipReader = ZipArchive<ArchiveReader>;
use std::path::Path;
use std::rc::Rc;
use zip::ZipArchive;

#[path = "archive_edit.rs"]
pub mod edit;

const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_TAR_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_TAR_METADATA_BYTES: u64 = 1024 * 1024;

pub fn is_tar_zstd(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".tar.zst"))
}

fn ensure_zip<P: AsRef<Path>>(archive_path: P) -> anyhow::Result<()> {
    let path = archive_path.as_ref();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("zip") {
        anyhow::bail!(
            "unsupported archive format for {}: only ZIP and TAR.ZST archives are supported",
            path.display()
        );
    }
    Ok(())
}

pub fn list_archive_images<P: AsRef<Path>>(archive_path: P) -> anyhow::Result<Vec<String>> {
    if is_tar_zstd(archive_path.as_ref()) {
        return list_tar_zstd_images(archive_path.as_ref());
    }
    ensure_zip(&archive_path)?;
    let file = File::open(archive_path)?;
    let mut zip = ZipArchive::new(Box::new(file) as ArchiveReader)?;
    list_zip_images(&mut zip)
}

pub fn list_zip_images(zip: &mut ZipReader) -> anyhow::Result<Vec<String>> {
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        anyhow::bail!(
            "archive contains {} entries, exceeding the {} entry limit",
            zip.len(),
            MAX_ARCHIVE_ENTRIES
        );
    }
    let mut image_entries = Vec::new();

    for i in 0..zip.len() {
        let zip_file = zip.by_index(i)?;
        let name = zip_file.name().to_string();

        // 디렉토리 및 숨김파일 제외
        if zip_file.is_dir() || name.starts_with("__MACOSX") || name.starts_with('.') {
            continue;
        }

        let lower = name.to_lowercase();
        if lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
            || lower.ends_with(".webp")
            || lower.ends_with(".gif")
        {
            if zip_file.size() > MAX_IMAGE_BYTES {
                anyhow::bail!(
                    "archive image {} declares {} bytes, exceeding the 32 MiB limit",
                    name,
                    zip_file.size()
                );
            }
            image_entries.push(name);
        }
    }

    image_entries.sort();
    Ok(image_entries)
}

#[cfg(test)]
pub fn read_archive_file<P: AsRef<Path>>(
    archive_path: P,
    entry_name: &str,
) -> anyhow::Result<Vec<u8>> {
    if is_tar_zstd(archive_path.as_ref()) {
        return read_tar_zstd_file(archive_path.as_ref(), entry_name);
    }
    ensure_zip(&archive_path)?;
    let file = File::open(archive_path)?;
    let mut zip = ZipArchive::new(Box::new(file) as ArchiveReader)?;
    read_zip_file(&mut zip, entry_name)
}

pub fn read_zip_file(zip: &mut ZipReader, entry_name: &str) -> anyhow::Result<Vec<u8>> {
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        anyhow::bail!(
            "archive contains {} entries, exceeding the {} entry limit",
            zip.len(),
            MAX_ARCHIVE_ENTRIES
        );
    }
    let zip_file = zip.by_name(entry_name)?;
    if zip_file.size() > MAX_IMAGE_BYTES {
        anyhow::bail!(
            "archive image {} declares {} bytes, exceeding the 32 MiB limit",
            entry_name,
            zip_file.size()
        );
    }
    let mut buffer = Vec::new();
    let mut limited: Take<_> = zip_file.take(MAX_IMAGE_BYTES + 1);
    limited.read_to_end(&mut buffer)?;
    if buffer.len() as u64 > MAX_IMAGE_BYTES {
        anyhow::bail!(
            "archive image {} exceeds the 32 MiB decompressed limit",
            entry_name
        );
    }
    Ok(buffer)
}

// A streaming TAR cannot seek directly to a page. Decode only as far as needed,
// without extracting paths to disk or holding the entire archive in memory.
struct LimitedReader<R> {
    inner: R,
    remaining: u64,
    entry_budget: Rc<Cell<u64>>,
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut probe = [0];
            if self.inner.read(&mut probe)? == 0 {
                return Ok(0);
            }
            return Err(io::Error::other(
                "TAR.ZST exceeds the 16 GiB decompressed limit",
            ));
        }
        if self.entry_budget.get() == 0 {
            return Err(io::Error::other("TAR metadata exceeds the 1 MiB limit"));
        }
        let count = buffer.len().min(
            self.remaining
                .min(self.entry_budget.get())
                .min(usize::MAX as u64) as usize,
        );
        let read = self.inner.read(&mut buffer[..count])?;
        self.remaining -= read as u64;
        self.entry_budget.set(self.entry_budget.get() - read as u64);
        Ok(read)
    }
}

fn open_tar_reader(
    reader: ArchiveReader,
    entry_budget: Rc<Cell<u64>>,
) -> anyhow::Result<tar::Archive<impl Read>> {
    let mut decoder = zstd::stream::read::Decoder::new(reader)?;
    decoder.window_log_max(27)?; // At most 128 MiB of decoder history.
    Ok(tar::Archive::new(LimitedReader {
        inner: decoder,
        remaining: MAX_TAR_BYTES,
        entry_budget,
    }))
}

fn tar_image_name<R: Read>(entry: &tar::Entry<'_, R>) -> anyhow::Result<Option<String>> {
    // Links and special entries are never followed or exposed as images.
    if !entry.header().entry_type().is_file() {
        return Ok(None);
    }
    let path = entry.path()?;
    let Some(name) = path.to_str() else {
        return Ok(None);
    };
    let name = name.replace('\\', "/");
    let name = name.trim_start_matches("./");
    if name.is_empty()
        || name.starts_with('/')
        || name
            .split('/')
            .any(|part| part.starts_with('.') || part == "__MACOSX" || part.contains(':'))
    {
        return Ok(None);
    }
    let lower = name.to_ascii_lowercase();
    if ![".jpg", ".jpeg", ".png", ".webp", ".gif"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Ok(None);
    }
    if entry.size() > MAX_IMAGE_BYTES {
        anyhow::bail!(
            "archive image {} declares {} bytes, exceeding the 32 MiB limit",
            name,
            entry.size()
        );
    }
    Ok(Some(name.to_owned()))
}

fn list_tar_zstd_images(path: &Path) -> anyhow::Result<Vec<String>> {
    list_tar_reader(Box::new(File::open(path)?))
}
pub fn list_tar_reader(reader: ArchiveReader) -> anyhow::Result<Vec<String>> {
    let entry_budget = Rc::new(Cell::new(MAX_TAR_METADATA_BYTES));
    let mut archive = open_tar_reader(reader, entry_budget.clone())?;
    let mut names = Vec::new();
    let mut names_size = 0;
    let mut entries = archive.entries()?;
    let mut count = 0;
    loop {
        // tar internally buffers GNU/PAX metadata. Limit each header traversal
        // before it can allocate arbitrarily large extension records.
        entry_budget.set(MAX_TAR_METADATA_BYTES);
        let Some(entry) = entries.next() else { break };
        if count >= MAX_ARCHIVE_ENTRIES {
            anyhow::bail!("archive exceeds the {} entry limit", MAX_ARCHIVE_ENTRIES);
        }
        count += 1;
        let mut entry = entry?;
        if let Some(name) = tar_image_name(&entry)? {
            names_size += name.len() as u64;
            if names_size > MAX_IMAGE_BYTES {
                anyhow::bail!("archive image names exceed the 32 MiB limit");
            }
            names.push(name);
        }
        // Consume payload here so the next iteration's metadata budget does
        // not also include skipped file data. The overall stream stays capped.
        entry_budget.set(MAX_TAR_BYTES);
        io::copy(&mut entry, &mut io::sink())?;
    }
    // Finish the Zstandard frame to validate its trailing checksum.
    entry_budget.set(MAX_TAR_BYTES);
    io::copy(&mut archive.into_inner(), &mut io::sink())?;
    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
fn read_tar_zstd_file(path: &Path, entry_name: &str) -> anyhow::Result<Vec<u8>> {
    read_tar_reader(Box::new(File::open(path)?), entry_name)
}
pub fn read_tar_reader(reader: ArchiveReader, entry_name: &str) -> anyhow::Result<Vec<u8>> {
    let entry_budget = Rc::new(Cell::new(MAX_TAR_METADATA_BYTES));
    let mut archive = open_tar_reader(reader, entry_budget.clone())?;
    let mut entries = archive.entries()?;
    let mut count = 0;
    loop {
        entry_budget.set(MAX_TAR_METADATA_BYTES);
        let Some(entry) = entries.next() else { break };
        if count >= MAX_ARCHIVE_ENTRIES {
            anyhow::bail!("archive exceeds the {} entry limit", MAX_ARCHIVE_ENTRIES);
        }
        count += 1;
        let mut entry = entry?;
        let name = tar_image_name(&entry)?;
        entry_budget.set(MAX_TAR_BYTES);
        if name.as_deref() == Some(entry_name) {
            let expected_size = entry.size();
            let mut bytes = Vec::new();
            entry.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != expected_size {
                anyhow::bail!("archive image {} is truncated", entry_name);
            }
            return Ok(bytes);
        }
        io::copy(&mut entry, &mut io::sink())?;
    }
    anyhow::bail!("archive image not found: {}", entry_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;

    fn temp_archive(extension: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vault-archive-{}-{}.{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            extension
        ))
    }

    fn write_tar_zstd(
        path: &Path,
        entries: &[(&str, &[u8], tar::EntryType)],
    ) -> anyhow::Result<()> {
        let mut encoder = zstd::stream::write::Encoder::new(File::create(path)?, 1)?;
        encoder.include_checksum(true)?;
        let mut builder = tar::Builder::new(encoder);
        for (name, bytes, kind) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_entry_type(*kind);
            header.set_size(bytes.len() as u64);
            if kind.is_symlink() || kind.is_hard_link() {
                header.set_link_name("01.png")?;
            }
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes)?;
        }
        builder.into_inner()?.finish()?;
        Ok(())
    }

    #[test]
    fn tar_zstd_lists_sorted_images_and_reads_nested_unicode_and_long_paths() -> anyhow::Result<()>
    {
        let path = temp_archive("TAR.ZST");
        let long_name = format!("{}/page.webp", "long-folder/".repeat(20));
        write_tar_zstd(
            &path,
            &[
                ("./nested/02.JPG", b"second", tar::EntryType::Regular),
                ("01.png", b"first", tar::EntryType::Regular),
                ("한글/03.gif", b"third", tar::EntryType::Regular),
                (&long_name, b"long", tar::EntryType::Regular),
                ("notes.txt", b"ignored", tar::EntryType::Regular),
                (".hidden.png", b"hidden", tar::EntryType::Regular),
                ("__MACOSX/._01.png", b"metadata", tar::EntryType::Regular),
                ("nested/.hidden.png", b"hidden", tar::EntryType::Regular),
                ("linked.png", b"", tar::EntryType::Symlink),
                ("hard.png", b"", tar::EntryType::Link),
                ("directory.png", b"", tar::EntryType::Directory),
            ],
        )?;
        let mut expected = vec![
            "01.png".to_owned(),
            "nested/02.JPG".to_owned(),
            "한글/03.gif".to_owned(),
            long_name.clone(),
        ];
        expected.sort();
        assert_eq!(list_archive_images(&path)?, expected);
        for (name, bytes) in [
            ("01.png", b"first".as_slice()),
            ("nested/02.JPG", b"second"),
            ("한글/03.gif", b"third"),
            (&long_name, b"long"),
        ] {
            assert_eq!(read_archive_file(&path, name)?, bytes);
        }
        assert!(read_archive_file(&path, "missing.png").is_err());
        assert!(read_archive_file(&path, "linked.png").is_err());
        assert!(read_archive_file(&path, "notes.txt").is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn tar_zstd_empty_corrupt_and_truncated_archives() -> anyhow::Result<()> {
        let path = temp_archive("tar.zst");
        write_tar_zstd(&path, &[])?;
        assert!(list_archive_images(&path)?.is_empty());
        write_tar_zstd(&path, &[("page.png", b"image", tar::EntryType::Regular)])?;
        let mut bytes = std::fs::read(&path)?;
        bytes.pop(); // Remove part of the Zstandard checksum.
        std::fs::write(&path, bytes)?;
        assert!(list_archive_images(&path).is_err());
        std::fs::write(&path, b"not zstandard")?;
        assert!(list_archive_images(&path).is_err());
        assert!(read_archive_file(&path, "page.png").is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn tar_zstd_rejects_oversize_image_before_reading_payload() -> anyhow::Result<()> {
        let path = temp_archive("tar.zst");
        let mut header = tar::Header::new_gnu();
        header.set_path("large.png")?;
        header.set_size(MAX_IMAGE_BYTES + 1);
        header.set_mode(0o644);
        header.set_cksum();
        // Only a header: the size check must happen before touching the body.
        std::fs::write(&path, zstd::stream::encode_all(&header.as_bytes()[..], 1)?)?;
        for result in [
            list_archive_images(&path).map(|_| ()),
            read_archive_file(&path, "large.png").map(|_| ()),
        ] {
            assert!(result.unwrap_err().to_string().contains("32 MiB"));
        }
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn tar_stream_limit_reports_error_instead_of_silent_truncation() {
        let mut reader = LimitedReader {
            inner: &b"12345"[..],
            remaining: 4,
            entry_budget: Rc::new(Cell::new(MAX_TAR_BYTES)),
        };
        let mut bytes = Vec::new();
        assert!(reader
            .read_to_end(&mut bytes)
            .unwrap_err()
            .to_string()
            .contains("decompressed limit"));
        let mut reader = LimitedReader {
            inner: &b"1234"[..],
            remaining: 4,
            entry_budget: Rc::new(Cell::new(MAX_TAR_BYTES)),
        };
        bytes.clear();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"1234");
    }

    #[test]
    fn tar_zstd_bounds_extended_metadata() -> anyhow::Result<()> {
        let path = temp_archive("tar.zst");
        let mut header = tar::Header::new_gnu();
        header.set_path("././@LongLink")?;
        header.set_entry_type(tar::EntryType::GNULongName);
        header.set_size(MAX_TAR_METADATA_BYTES + 1);
        header.set_mode(0o644);
        header.set_cksum();
        let mut encoder = zstd::stream::write::Encoder::new(File::create(&path)?, 1)?;
        encoder.write_all(header.as_bytes())?;
        io::copy(
            &mut io::repeat(b'a').take(MAX_TAR_METADATA_BYTES + 1),
            &mut encoder,
        )?;
        encoder.finish()?;
        let error = list_archive_images(&path).unwrap_err();
        assert!(format!("{error:#}").contains("metadata exceeds"));
        assert!(read_archive_file(&path, "page.png").is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn valid_zip_lists_and_reads_images() -> anyhow::Result<()> {
        let path = temp_archive("zip");
        let file = File::create(&path)?;
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("page.JPG", SimpleFileOptions::default())?;
        zip.write_all(b"image bytes")?;
        zip.start_file("notes.txt", SimpleFileOptions::default())?;
        zip.write_all(b"ignored")?;
        zip.finish()?;

        assert_eq!(list_archive_images(&path)?, vec!["page.JPG"]);
        assert_eq!(read_archive_file(&path, "page.JPG")?, b"image bytes");
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn unsupported_archive_extension_is_rejected() {
        let result = list_archive_images(temp_archive("rar"));
        let error = result.unwrap_err().to_string();
        assert!(error.contains("only ZIP"));
    }

    #[test]
    fn declared_oversize_image_is_rejected_before_reading() -> anyhow::Result<()> {
        let path = temp_archive("zip");
        let file = File::create(&path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("large.png", options)?;
        zip.write_all(&vec![0u8; (MAX_IMAGE_BYTES + 1) as usize])?;
        zip.finish()?;

        let error = list_archive_images(&path).unwrap_err().to_string();
        assert!(error.contains("declares") && error.contains("32 MiB"));
        let error = read_archive_file(&path, "large.png")
            .unwrap_err()
            .to_string();
        assert!(error.contains("declares") && error.contains("32 MiB"));
        std::fs::remove_file(path)?;
        Ok(())
    }
}
