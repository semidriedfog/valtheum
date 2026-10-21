//! Authenticated, bounded-memory HTTP byte ranges for encrypted video playback.
use super::*;
use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, Method},
    response::Response,
};
use futures_core::Stream;
use std::{
    io::{self, Read, Seek, SeekFrom},
    pin::Pin,
    task::{Context as TaskContext, Poll},
};
use tokio::sync::mpsc;

struct VideoStream {
    receiver: mpsc::Receiver<io::Result<Bytes>>,
    revoked: Arc<AtomicBool>,
    ended: bool,
}
impl Stream for VideoStream {
    type Item = io::Result<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if self.revoked.load(Ordering::Acquire) {
            self.receiver.close();
            self.ended = true;
            return Poll::Ready(Some(Err(io::Error::other(
                "라이브러리가 잠겼거나 변경되었습니다.",
            ))));
        }
        self.receiver.poll_recv(cx)
    }
}

fn range(value: Option<&str>, size: u64) -> Result<Option<(u64, u64)>> {
    let Some(value) = value else { return Ok(None) };
    let value = value
        .strip_prefix("bytes=")
        .context("지원하지 않는 Range")?;
    ensure!(!value.contains(','), "다중 Range는 지원하지 않습니다.");
    let (start, end) = value.split_once('-').context("잘못된 Range")?;
    ensure!(size > 0, "빈 영상 Range");
    if start.is_empty() {
        let suffix = end.parse::<u64>()?;
        ensure!(suffix > 0, "빈 suffix Range");
        return Ok(Some((size.saturating_sub(suffix), size - 1)));
    }
    let start = start.parse::<u64>()?;
    ensure!(start < size, "Range 범위 초과");
    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>()?.min(size - 1)
    };
    ensure!(end >= start, "역순 Range");
    Ok(Some((start, end)))
}

pub async fn serve(
    state: &RequestState,
    path: &str,
    method: Method,
    headers: HeaderMap,
) -> Result<Option<Response>, StatusCode> {
    if crypto::video::format(path).is_none() && crypto::image::format(path).is_none() {
        return Ok(None);
    }
    crate::state::validate_relative(path)?;
    let query_path = path.to_owned();
    let mapped: Option<(String, String)> = state.db_work(move |state| {
        let guard = state
            .conn
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let conn = guard.as_ref().ok_or(StatusCode::LOCKED)?;
        conn.query_row("SELECT a.encrypted_path,COALESCE(a.restore_name,m.rel_path) FROM archive_store a JOIN media_files m ON m.id=a.file_id WHERE m.rel_path=?1 AND m.file_type IN ('VIDEO','IMAGE') AND a.state='ready'",[query_path],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)
    }).await??;
    let Some((stored, expected_name)) = mapped else {
        return Ok(None);
    };
    let permit = state
        .archive_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let library = state.library.clone();
    let opened = tokio::task::spawn_blocking(move || -> Result<crypto::Reader> {
        let key = library.archives.secret()?;
        let path = crypto::safe_child(&library.root_dir, &stored)?;
        let reader =
            crypto::Reader::open(&path, &key)?.with_revocation(library.archives.revoked.clone());
        ensure!(
            reader.metadata.name == expected_name
                && crypto::video::format(&expected_name)
                    .or_else(|| crypto::image::format(&expected_name))
                    == Some(reader.metadata.format.as_str()),
            "영상 메타데이터 불일치"
        );
        ensure!(
            !library.archives.revoked.load(Ordering::Acquire),
            "라이브러리 잠김"
        );
        Ok(reader)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Never fall back to a retained plaintext original when encrypted storage fails.
    let mut reader = opened.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let size = reader.metadata.size;
    let requested = if headers.contains_key(header::IF_RANGE) || method == Method::HEAD {
        None
    } else {
        headers
            .get(header::RANGE)
            .map(|v| v.to_str().unwrap_or("invalid"))
    };
    let selected = match range(requested, size) {
        Ok(range) => range,
        Err(_) => {
            return Ok(Some(
                Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(Body::empty())
                    .unwrap(),
            ))
        }
    };
    let (start, length, status) = if let Some((start, end)) = selected {
        (start, end - start + 1, StatusCode::PARTIAL_CONTENT)
    } else {
        (0, size, StatusCode::OK)
    };
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store");
    if selected.is_some() {
        response = response.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{size}", start + length - 1),
        );
    }
    if method == Method::HEAD || length == 0 {
        return Ok(Some(response.body(Body::empty()).unwrap()));
    }
    let (sender, receiver) = mpsc::channel(2);
    let revoked = state.archives.revoked.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let result = (|| -> io::Result<()> {
            reader.seek(SeekFrom::Start(start))?;
            let mut remaining = length;
            let mut buffer = zeroize::Zeroizing::new(vec![0u8; 65536]);
            while remaining > 0 && !sender.is_closed() {
                let count = remaining.min(buffer.len() as u64) as usize;
                reader.read_exact(&mut buffer[..count])?;
                if sender
                    .blocking_send(Ok(Bytes::copy_from_slice(&buffer[..count])))
                    .is_err()
                {
                    break;
                }
                remaining -= count as u64;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = sender.blocking_send(Err(error));
        }
    });
    Ok(Some(
        response
            .body(Body::from_stream(VideoStream {
                receiver,
                revoked,
                ended: false,
            }))
            .unwrap(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_range_bounds() {
        assert_eq!(range(None, 100).unwrap(), None);
        for (input, expected) in [
            ("bytes=0-0", (0, 0)),
            ("bytes=20-", (20, 99)),
            ("bytes=-5", (95, 99)),
            ("bytes=95-200", (95, 99)),
            ("bytes=-200", (0, 99)),
        ] {
            assert_eq!(range(Some(input), 100).unwrap(), Some(expected));
        }
        for input in [
            "bytes=-0",
            "bytes=100-",
            "bytes=10-2",
            "bytes=0-1,3-4",
            "bytes=x-5",
            "bytes=18446744073709551616-",
            "items=0-1",
        ] {
            assert!(range(Some(input), 100).is_err());
        }
        assert!(range(Some("bytes=0-"), 0).is_err());
    }
}
