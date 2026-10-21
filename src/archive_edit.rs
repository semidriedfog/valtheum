//! Rebuild without selected images. Outputs stream through memory, never plaintext files.
use super::*;
use anyhow::{ensure, Result};
use std::io::{SeekFrom, Write};

// raw_copy_file knows sizes/CRC in advance and only queries the write position.
// Refuse any actual backward seek instead of silently corrupting the stream.
struct ForwardWriter<W> {
    inner: W,
    position: u64,
}
impl<W: Write> Write for ForwardWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.position += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<W: Write> Seek for ForwardWriter<W> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        match from {
            SeekFrom::Current(0) | SeekFrom::End(0) => Ok(self.position),
            SeekFrom::Start(n) if n == self.position => Ok(n),
            _ => Err(io::Error::other("ZIP 재작성 중 지원하지 않는 위치 이동")),
        }
    }
}

pub fn rebuild(
    mut reader: ArchiveReader,
    format: &str,
    remove: &std::collections::HashSet<String>,
    out: &mut impl Write,
    mut progress: impl FnMut() -> Result<()>,
) -> Result<()> {
    if format == "zip" {
        let mut source = ZipArchive::new(reader)?;
        ensure!(
            source.len() <= MAX_ARCHIVE_ENTRIES,
            "압축 항목이 너무 많습니다."
        );
        let mut target = zip::ZipWriter::new(ForwardWriter {
            inner: out,
            position: 0,
        });
        target.set_raw_comment(source.comment().to_vec().into_boxed_slice());
        let mut removed = false;
        for i in 0..source.len() {
            progress()?;
            let file = source.by_index(i)?;
            if remove.contains(file.name()) && !file.is_dir() {
                removed = true;
                continue;
            }
            target.raw_copy_file(file)?;
        }
        ensure!(removed, "삭제할 이미지가 없습니다.");
        target.finish()?;
        return Ok(());
    }
    ensure!(format == "tar.zst", "지원하지 않는 압축 형식");
    // Preserve original TAR headers, extended names, non-image entries and payloads.
    // A removed entry also owns any preceding GNU/PAX local extension headers.
    let mut ranges = Vec::new();
    {
        let budget = Rc::new(Cell::new(MAX_TAR_METADATA_BYTES));
        // Borrow via a separate trait object to allow rewinding afterwards.
        let mut decoder = zstd::stream::read::Decoder::new(&mut reader)?;
        decoder.window_log_max(27)?;
        let mut source = tar::Archive::new(LimitedReader {
            inner: decoder,
            remaining: MAX_TAR_BYTES,
            entry_budget: budget.clone(),
        });
        let mut entries = source.entries()?;
        let mut previous_end = 0;
        let mut count = 0;
        loop {
            budget.set(MAX_TAR_METADATA_BYTES);
            let Some(entry) = entries.next() else { break };
            progress()?;
            count += 1;
            ensure!(count <= MAX_ARCHIVE_ENTRIES, "압축 항목이 너무 많습니다.");
            let mut entry = entry?;
            let kind = entry.header().entry_type();
            ensure!(
                !kind.is_gnu_sparse() && !kind.is_pax_global_extensions(),
                "희소 파일 또는 전역 PAX 메타데이터가 있는 TAR는 편집할 수 없습니다."
            );
            if let Some(extensions) = entry.pax_extensions()? {
                for field in extensions {
                    ensure!(
                        !field?.key_bytes().starts_with(b"GNU.sparse"),
                        "희소 TAR 파일은 편집할 수 없습니다."
                    );
                }
            }
            ensure!(entry.size() <= MAX_TAR_BYTES, "TAR 항목 크기 제한 초과");
            let end = entry
                .raw_file_position()
                .checked_add(entry.size().div_ceil(512) * 512)
                .ok_or_else(|| anyhow::anyhow!("잘못된 TAR 항목 크기"))?;
            ensure!(
                end <= MAX_TAR_BYTES && end >= previous_end,
                "잘못된 TAR 항목 위치"
            );
            if tar_image_name(&entry)?.is_some_and(|name| remove.contains(&name)) {
                ranges.push((previous_end, end));
            }
            budget.set(MAX_TAR_BYTES);
            io::copy(&mut entry, &mut io::sink())?;
            previous_end = end;
        }
        budget.set(MAX_TAR_BYTES);
        io::copy(&mut source.into_inner(), &mut io::sink())?;
    }
    ensure!(!ranges.is_empty(), "삭제할 이미지가 없습니다.");
    reader.rewind()?;
    let mut decoder = zstd::stream::read::Decoder::new(reader)?;
    decoder.window_log_max(27)?;
    let mut encoder = zstd::stream::write::Encoder::new(out, 3)?;
    encoder.include_checksum(true)?;
    let mut buf = zeroize::Zeroizing::new(vec![0; 65536]);
    let mut position = 0u64;
    let mut range = 0;
    loop {
        progress()?;
        let n = decoder.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let end = position + n as u64;
        ensure!(end <= MAX_TAR_BYTES, "TAR 크기 제한 초과");
        let mut cursor = position;
        while cursor < end {
            while range < ranges.len() && ranges[range].1 <= cursor {
                range += 1;
            }
            let (start, stop) = ranges.get(range).copied().unwrap_or((u64::MAX, u64::MAX));
            if cursor < start {
                let keep_end = end.min(start);
                encoder.write_all(
                    &buf[(cursor - position) as usize..(keep_end - position) as usize],
                )?;
                cursor = keep_end;
            } else {
                cursor = end.min(stop);
            }
        }
        position = end;
    }
    encoder.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleting_long_tar_name_removes_its_extension_headers_and_preserves_following_entry(
    ) -> Result<()> {
        let name = format!("{}/remove.png", "long-directory/".repeat(20));
        let mut tar = tar::Builder::new(Vec::new());
        for (path, data) in [
            (name.as_str(), b"removed".as_slice()),
            ("keep.png", b"kept".as_slice()),
            ("note.txt", b"note".as_slice()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o640);
            header.set_mtime(42);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, path, data)?;
        }
        let bytes = zstd::stream::encode_all(tar.into_inner()?.as_slice(), 3)?;
        let mut output = Vec::new();
        rebuild(
            Box::new(io::Cursor::new(bytes)),
            "tar.zst",
            &std::collections::HashSet::from([name]),
            &mut output,
            || Ok(()),
        )?;
        let mut result = tar::Archive::new(zstd::stream::read::Decoder::new(output.as_slice())?);
        let mut entries = Vec::new();
        for e in result.entries()? {
            let mut e = e?;
            let path = e.path()?.to_string_lossy().into_owned();
            let mut data = Vec::new();
            e.read_to_end(&mut data)?;
            entries.push((path, data));
        }
        assert_eq!(
            entries,
            vec![
                ("keep.png".into(), b"kept".to_vec()),
                ("note.txt".into(), b"note".to_vec())
            ]
        );
        Ok(())
    }
}
