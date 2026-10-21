//! Bounded UI wait around potentially uninterruptible NAS I/O.
use super::*;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, String)>;
pub const STILL_RUNNING: &str =
    "이전 NAS 연결 요청이 아직 종료되지 않았습니다. 잠시 기다리거나 라이브러리를 닫아 주세요.";
const CANCELLED: &str = "재연결 대기를 취소했습니다. 기존 목록은 유지됩니다.";
const TIMED_OUT: &str = "재연결 대기 시간(15초)을 초과했습니다. NAS 연결을 확인해 주세요. 기존 목록은 유지되며 라이브러리를 닫을 수 있습니다.";
const WAIT: Duration = Duration::from_secs(15);
const RUNNING: u8 = 0;
const CANCELLED_STATE: u8 = 1;
const COMMITTED: u8 = 2;

struct Attempt {
    id: Option<String>,
    state: AtomicU8,
    wake: Notify,
    deadline: Instant,
}
impl Attempt {
    fn cancel(&self) -> bool {
        let cancelled = self
            .state
            .compare_exchange(
                RUNNING,
                CANCELLED_STATE,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok();
        if cancelled {
            self.wake.notify_one();
        }
        cancelled
    }
}
#[derive(Default)]
pub struct Runtime {
    active: Mutex<Option<Arc<Attempt>>>,
    cancelled_ids: Mutex<std::collections::VecDeque<String>>,
}
impl Runtime {
    pub fn is_running(&self) -> bool {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
    pub fn cancel(&self) {
        if let Some(attempt) = self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            attempt.cancel();
        }
    }
}
struct InFlight(Arc<Runtime>);
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

pub async fn cancel(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    headers: axum::http::HeaderMap,
) -> ApiResult {
    let current = app.current.lock().map_err(|_| failure())?;
    if current.generation != state.generation {
        return Err(changed());
    }
    let id = request_id(&headers);
    // A cancel request can arrive before its reconnect request has entered here.
    if let Some(id) = &id {
        let mut cancelled = current
            .recovery
            .cancelled_ids
            .lock()
            .map_err(|_| failure())?;
        if cancelled.len() == 64 {
            cancelled.pop_front();
        }
        cancelled.push_back(id.clone());
    }
    if let Some(attempt) = current
        .recovery
        .active
        .lock()
        .map_err(|_| failure())?
        .as_ref()
    {
        if attempt.id == id {
            attempt.cancel();
        }
    }
    Ok(Json(serde_json::json!({"generation": state.generation})))
}

pub async fn reconnect(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    headers: axum::http::HeaderMap,
) -> ApiResult {
    reconnect_with(app, state, WAIT, request_id(&headers), |path, password| {
        let connection = encryption::open(&path, password.as_deref().map(|p| p.as_str()), false)?;
        connection.query_row("SELECT count(*) FROM media_files", [], |row| {
            row.get::<_, i64>(0)
        })?;
        Ok(connection)
    })
    .await
}

// Injectable open operation keeps timeout/cancellation tests independent of a real NAS.
pub(crate) async fn reconnect_with(
    app: AppState,
    state: RequestState,
    wait: Duration,
    id: Option<String>,
    open: impl FnOnce(PathBuf, Option<zeroize::Zeroizing<String>>) -> anyhow::Result<rusqlite::Connection>
        + Send
        + 'static,
) -> ApiResult {
    let (password, path, attempt, guard) = {
        let current = app.current.lock().map_err(|_| failure())?;
        if current.generation != state.generation {
            return Err(changed());
        }
        let security = current.security.lock().map_err(|_| failure())?;
        if app.concealed.load(Ordering::Acquire)
            || (security.encrypted && security.password.is_none())
            || current.root_dir.as_os_str().is_empty()
        {
            return Err((StatusCode::LOCKED, "DB가 잠겨 있습니다.".into()));
        }
        encryption::require_idle(&current).map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
        if id.as_ref().is_some_and(|id| {
            current
                .recovery
                .cancelled_ids
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(id)
        }) {
            return Err((StatusCode::REQUEST_TIMEOUT, CANCELLED.into()));
        }
        let attempt = Arc::new(Attempt {
            id,
            state: AtomicU8::new(RUNNING),
            wake: Notify::new(),
            deadline: Instant::now() + wait,
        });
        *current.recovery.active.lock().map_err(|_| failure())? = Some(attempt.clone());
        (
            security.password.clone(),
            security.path(&current.root_dir),
            attempt,
            InFlight(current.recovery.clone()),
        )
    };
    let generation = state.generation;
    let worker_attempt = attempt.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        // The captured library keeps the cross-PC lease until this OS request ends,
        // even when its HTTP waiter times out, is cancelled, or closes the library.
        let _guard = guard;
        let connection = open(path, password).map_err(|_| failure())?;
        // Finish potentially blocking handle cleanup before committing. The UI
        // can still time out/cancel here, and the old DB connection stays in place.
        state.archives.clear_cache();
        let current = app.current.lock().map_err(|_| failure())?;
        if current.generation != generation || app.concealed.load(Ordering::Acquire) {
            return Err(changed());
        }
        // Never wait behind another blocked DB read while holding app.current.
        let mut slot = current.conn.try_lock().map_err(|_| failure())?;
        if Instant::now() >= worker_attempt.deadline {
            worker_attempt.cancel();
        }
        // This CAS serializes publication against an accepted cancellation.
        if worker_attempt
            .state
            .compare_exchange(RUNNING, COMMITTED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err((StatusCode::REQUEST_TIMEOUT, CANCELLED.into()));
        }
        let old = slot.replace(connection);
        drop(slot);
        drop(current);
        let cleanup_library = state.library.clone();
        tokio::task::spawn_blocking(move || {
            drop(old);
            drop(cleanup_library);
        });
        drop(_guard);
        Ok(Json(serde_json::json!({"generation": generation})))
    });
    tokio::select! {
        result = &mut worker => result.map_err(|_| failure())?,
        _ = attempt.wake.notified() => Err((StatusCode::REQUEST_TIMEOUT, CANCELLED.into())),
        _ = tokio::time::sleep(wait) => {
            if attempt.cancel() || attempt.state.load(Ordering::Acquire) == CANCELLED_STATE {
                Err((StatusCode::GATEWAY_TIMEOUT, TIMED_OUT.into()))
            } else {
                // Publication won; obtain the actual completed result rather than
                // synthesizing success while a worker is still marked as running.
                worker.await.map_err(|_| failure())?
            }
        }
    }
}
fn changed() -> (StatusCode, String) {
    (StatusCode::CONFLICT, "폴더가 변경되었습니다.".into())
}
fn failure() -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, "저장소에 다시 연결하지 못했습니다. NAS 연결을 확인한 뒤 다시 시도해 주세요. 기존 목록은 유지됩니다.".into())
}

fn request_id(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-vault-reconnect")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 128)
        .map(str::to_owned)
}
