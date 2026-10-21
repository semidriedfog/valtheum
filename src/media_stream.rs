//! Revoke an already-open plain media response when its library is closed.
use axum::{
    body::{Body, BodyDataStream, Bytes},
    response::Response,
};
use futures_core::Stream;
use std::{
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

struct RevocableBody {
    inner: Option<BodyDataStream>,
    revoked: Arc<AtomicBool>,
}

impl Stream for RevocableBody {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.inner.is_none() {
            return Poll::Ready(None);
        }
        if self.revoked.load(Ordering::Acquire) {
            self.inner.take();
            return Poll::Ready(Some(Err(io::Error::other("Library access revoked"))));
        }
        let result = Pin::new(self.inner.as_mut().unwrap()).poll_next(cx);
        // Revocation may happen while the underlying read is completing.
        if self.revoked.load(Ordering::Acquire) {
            self.inner.take();
            return Poll::Ready(Some(Err(io::Error::other("Library access revoked"))));
        }
        match result {
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(io::Error::other(error)))),
            Poll::Ready(None) => {
                self.inner.take();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub fn protect(response: Response, revoked: Arc<AtomicBool>) -> Response {
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::from_stream(RevocableBody {
            inner: Some(body.into_data_stream()),
            revoked,
        }),
    )
}
