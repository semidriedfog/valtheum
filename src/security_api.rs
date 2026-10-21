use crate::{
    encryption,
    state::{AppState, RequestState},
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Deserialize;
use zeroize::{Zeroize, Zeroizing};

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, String)>;

#[derive(Deserialize)]
pub struct Password {
    password: String,
    #[serde(default)]
    confirmation: String,
}
impl Drop for Password {
    fn drop(&mut self) {
        self.password.zeroize();
        self.confirmation.zeroize();
    }
}
#[derive(Deserialize)]
pub struct PasswordChange {
    current_password: String,
    password: String,
    confirmation: String,
}
impl Drop for PasswordChange {
    fn drop(&mut self) {
        self.current_password.zeroize();
        self.password.zeroize();
        self.confirmation.zeroize();
    }
}
pub async fn change_password(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(mut input): Json<PasswordChange>,
) -> ApiResult {
    if input.password != input.confirmation {
        return Err(error("새 비밀번호 확인이 일치하지 않습니다."));
    }
    let old = Zeroizing::new(std::mem::take(&mut input.current_password));
    let new = Zeroizing::new(std::mem::take(&mut input.password));
    let clean = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let result_clean = clean.clone();
    let mut response = transition(app, state.generation, false, move |library, security| {
        anyhow::ensure!(
            security.encrypted && security.password.is_some(),
            "암호화 DB의 잠금을 먼저 해제해 주세요."
        );
        anyhow::ensure!(
            security.password.as_deref().map(|p| p.as_str()) == Some(old.as_str()),
            "현재 비밀번호가 맞지 않습니다."
        );
        let mut guard = library
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("DB 연결 오류"))?;
        let changed = crate::password_change::change(&library.root_dir, &mut guard, &old, &new);
        if changed.is_err() && guard.is_none() {
            security.password.take();
            library.archives.retire();
        }
        result_clean.store(changed?, std::sync::atomic::Ordering::Release);
        security.password = Some(new);
        Ok(())
    })
    .await?;
    response.0["password_changed"] = true.into();
    response.0["message"]=if clean.load(std::sync::atomic::Ordering::Acquire) {
        "DB와 미디어 키의 비밀번호를 변경했습니다. DB·키 백업을 새로 만들어 주세요. 기존 백업은 이전 비밀번호로 열립니다."
    } else {
        "비밀번호 변경은 완료됐지만 이전 비밀번호의 임시 복구 사본 정리가 끝나지 않았습니다. 앱을 종료하고 다시 열어 정리를 완료한 뒤 새 DB·키 백업을 만들어 주세요."
    }.into();
    Ok(response)
}

pub async fn status(
    Extension(state): Extension<RequestState>,
) -> Result<Json<encryption::SecurityStatus>, StatusCode> {
    tokio::task::spawn_blocking(move || Json(state.security_status()))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, error.to_string())
}

// Reserve exclusive work without retaining the global state lock over DB/NAS I/O.
// Closing can revoke the captured library; its lease remains held until work ends.
pub(crate) struct DbOperation {
    pub state: RequestState,
}
impl DbOperation {
    pub fn begin(app: &AppState, expected: u64) -> Result<Self, (StatusCode, String)> {
        use std::sync::atomic::Ordering;
        let current = app.current.lock().map_err(|_| error("폴더 상태 오류"))?;
        if current.generation != expected || current.archives.revoked.load(Ordering::Acquire) {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        // Scan admission also holds this mutex. Never wait for it under current.
        let _security = current
            .security
            .try_lock()
            .map_err(|_| error("잠금 상태 오류"))?;
        encryption::require_idle(&current).map_err(error)?;
        current.archives.busy.store(true, Ordering::Release);
        Ok(Self {
            state: current.clone(),
        })
    }
}
impl Drop for DbOperation {
    fn drop(&mut self) {
        self.state
            .archives
            .busy
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

async fn transition(
    app: AppState,
    expected: u64,
    reveal: bool,
    work: impl FnOnce(&crate::state::Library, &mut encryption::DbSecurity) -> anyhow::Result<()>
        + Send
        + 'static,
) -> ApiResult {
    tokio::task::spawn_blocking(move || {
        use std::sync::atomic::Ordering;
        let operation = DbOperation::begin(&app, expected)?;
        let library = &operation.state.library;
        let mut security = {
            let stored = library
                .security
                .lock()
                .map_err(|_| error("잠금 상태 오류"))?;
            encryption::DbSecurity {
                encrypted: stored.encrypted,
                password: stored.password.clone(),
            }
        };
        if library.archives.revoked.load(Ordering::Acquire) {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        let result = work(library, &mut security);
        if let Err(err) = result {
            let current = app.current.lock().map_err(|_| error("폴더 상태 오류"))?;
            if current.generation == expected && std::sync::Arc::ptr_eq(&current.library, library) {
                // Conversion can have committed before reopening failed. Keep that
                // state, but never restore credentials to a closed library.
                *library
                    .security
                    .lock()
                    .map_err(|_| error("잠금 상태 오류"))? = security;
            }
            return Err(error(err));
        }
        // Handle disposal and status metadata reads can also stall on a NAS.
        // Prepare everything before taking the short publication lock.
        let conn = library
            .conn
            .lock()
            .map_err(|_| error("DB 연결 오류"))?
            .take();
        let next = std::sync::Arc::new(crate::state::Library {
            cleanup_pending: Default::default(),
            archives: library.archives.take_for_transition(),
            integrity: Default::default(),
            db_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            file_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(4)),
            recovery: Default::default(),
            lease: library.lease.clone(),
            root_dir: library.root_dir.clone(),
            conn: std::sync::Mutex::new(conn),
            security: std::sync::Mutex::new(security),
            scan: std::sync::Mutex::new(Default::default()),
        });
        library
            .security
            .lock()
            .map_err(|_| error("잠금 상태 오류"))?
            .password
            .take();
        let status = next.security_status();
        let mut current = app.current.lock().map_err(|_| error("폴더 상태 오류"))?;
        if current.generation != expected || !std::sync::Arc::ptr_eq(&current.library, library) {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        current.library = next;
        current.generation += 1;
        if reveal {
            app.concealed.store(false, Ordering::Release);
        }
        Ok(Json(serde_json::json!({ "generation": current.generation,
            "covered":app.concealed.load(Ordering::Acquire), "security":status,
            "scan":crate::state::ScanStatus::default() })))
    })
    .await
    .map_err(|_| error("암호화 작업을 완료하지 못했습니다."))?
}

pub async fn encrypt(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(mut input): Json<Password>,
) -> ApiResult {
    if input.password != input.confirmation {
        return Err(error("비밀번호 확인이 일치하지 않습니다."));
    }
    let password = Zeroizing::new(std::mem::take(&mut input.password));
    transition(app, state.generation, false, move |library, security| {
        anyhow::ensure!(!security.encrypted, "이미 암호화된 DB입니다.");
        let mut guard = library
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("DB 연결 오류"))?;
        let source = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DB 연결이 없습니다."))?;
        encryption::convert(source, &library.root_dir, &password)?;
        // Once the encrypted file is published, never keep using the old plaintext DB.
        security.encrypted = true;
        security.password = None;
        guard.take();
        let conn = encryption::open(
            &library.root_dir.join(encryption::SECURE_DB),
            Some(&password),
            false,
        )?;
        crate::db::init_db(&conn)?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        *guard = Some(conn);
        security.encrypted = true;
        security.password = Some(password);
        Ok(())
    })
    .await
}

pub async fn unlock(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
    Json(mut input): Json<Password>,
) -> ApiResult {
    if input.password.is_empty() || input.password.len() > 1024 {
        return Err(error("비밀번호를 확인해 주세요."));
    }
    let password = Zeroizing::new(std::mem::take(&mut input.password));
    let result = transition(
        app.clone(),
        state.generation,
        true,
        move |library, security| {
            unlock_library(library, security, password)?;
            Ok(())
        },
    )
    .await;
    result
}

pub(crate) fn unlock_library(
    library: &crate::state::Library,
    security: &mut encryption::DbSecurity,
    password: Zeroizing<String>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        security.encrypted
            && security.password.is_none()
            && !library.root_dir.as_os_str().is_empty(),
        "잠긴 암호화 DB가 아닙니다."
    );
    let conn = encryption::open(&security.path(&library.root_dir), Some(&password), false)
        .map_err(|_| anyhow::anyhow!("비밀번호가 맞지 않거나 DB가 손상되었습니다."))?;
    crate::db::init_db(&conn)?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    crate::archive_security::unlock(library, &conn, &password)?;
    let _ = crate::archive_security::rename::collect_unused(&library.root_dir, &conn);
    crate::db::recover_pending_deletions(&conn, &library.root_dir)?;
    *library
        .conn
        .lock()
        .map_err(|_| anyhow::anyhow!("DB 연결 오류"))? = Some(conn);
    security.password = Some(password);
    Ok(())
}

pub async fn lock(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    tokio::task::spawn_blocking(move || {
        let mut current = app.current.lock().map_err(|_| error("폴더 상태 오류"))?;
        if current.generation != state.generation {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        encryption::require_idle_except_scan(&current).map_err(error)?;
        let mut security = current
            .security
            .try_lock()
            .map_err(|_| error("잠금 상태 오류"))?;
        if !security.encrypted {
            return Err(error("암호화된 DB에서 사용할 수 있습니다."));
        }
        security.password.take();
        drop(security);
        // Revoke before cleanup: DB and cache handle disposal may wait on a NAS.
        let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let next = std::sync::Arc::new(crate::state::Library {
            cleanup_pending: pending.clone(),
            archives: Default::default(),
            integrity: Default::default(),
            recovery: Default::default(),
            db_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            file_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(4)),
            root_dir: current.root_dir.clone(),
            lease: current.lease.clone(),
            conn: std::sync::Mutex::new(None),
            scan: Default::default(),
            security: std::sync::Mutex::new(encryption::DbSecurity {
                encrypted: true,
                password: None,
            }),
        });
        retire_in_background(&current.library, Some(pending));
        current.library = next;
        current.generation += 1;
        app.concealed
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(Json(
            serde_json::json!({"generation":current.generation,"covered":true}),
        ))
    })
    .await
    .map_err(|_| error("라이브러리를 닫지 못했습니다."))?
}

pub(crate) fn retire_in_background(
    library: &std::sync::Arc<crate::state::Library>,
    complete: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) {
    use std::sync::atomic::Ordering;
    library.recovery.cancel();
    library.integrity.cancel();
    library.archives.revoked.store(true, Ordering::Release);
    library.archives.cancel.store(true, Ordering::Release);
    if let Ok(mut key) = library.archives.key.try_lock() {
        key.take();
    }
    if let Ok(mut security) = library.security.try_lock() {
        security.password.take();
    }
    library.db_slots.close();
    library.file_slots.close();
    let old = if library.archives.busy.load(Ordering::Acquire) {
        None
    } else {
        library
            .conn
            .try_lock()
            .ok()
            .and_then(|mut slot| slot.take())
    };
    let retired = library.clone();
    tokio::spawn(async move {
        while retired.archives.busy.load(Ordering::Acquire)
            || retired
                .scan
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .running
        {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        tokio::task::spawn_blocking(move || {
            drop(old);
            retired
                .security
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .password
                .take();
            retired.archives.revoke();
            retired
                .conn
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            retired.archives.clear_cache();
            if let Some(pending) = complete {
                pending.store(false, Ordering::Release);
            }
        });
    });
}

// Closing also works for plaintext libraries. Detach the old object so requests
// captured before closing cannot retain its connection or encryption keys.
pub async fn close(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    tokio::task::spawn_blocking(move || {
        let mut current = app.current.lock().map_err(|_| error("폴더 상태 오류"))?;
        if current.generation != state.generation {
            return Err((StatusCode::CONFLICT, "폴더가 변경되었습니다.".into()));
        }
        retire_in_background(&current.library, None);
        *app.restore_selection
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        app.privacy_entry.clear();
        current.library = crate::state::Library::unconfigured();
        current.generation += 1;
        app.concealed
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(Json(
            serde_json::json!({"generation":current.generation,"covered":true}),
        ))
    })
    .await
    .map_err(|_| error("라이브러리를 닫지 못했습니다."))?
}

pub async fn remove_plaintext(
    State(app): State<AppState>,
    Extension(state): Extension<RequestState>,
) -> ApiResult {
    transition(app, state.generation, false, |library, security| {
        anyhow::ensure!(
            security.encrypted && security.password.is_some(),
            "암호화 DB의 잠금을 먼저 해제해 주세요."
        );
        let guard = library
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("DB 연결 오류"))?;
        encryption::verify(
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("DB 연결 오류"))?,
            true,
        )?;
        encryption::remove_plaintext(&library.root_dir)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::Ordering, Arc};
    use std::time::Duration;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn late_unlock_success_and_failure_cannot_restore_closed_credentials() {
        for fail in [false, true] {
            let mut library = crate::state::Library::unconfigured();
            Arc::get_mut(&mut library).unwrap().root_dir = std::env::temp_dir().join(format!(
                "vault-transition-{}",
                crate::archive_crypto::hex(&crate::archive_crypto::random::<12>().unwrap())
            ));
            let app = AppState::new(library.clone()).unwrap();
            let state = app.snapshot().unwrap();
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let operation = tokio::spawn(transition(app.clone(), 1, true, move |_, security| {
                security.password = Some(Zeroizing::new("synthetic password".into()));
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                anyhow::ensure!(!fail, "synthetic failure after work");
                Ok(())
            }));
            started_rx.await.unwrap();
            let closed = tokio::time::timeout(
                Duration::from_secs(2),
                close(State(app.clone()), Extension(state)),
            )
            .await;
            release_tx.send(()).unwrap();
            let result = operation.await.unwrap();
            assert!(closed.unwrap().is_ok());
            assert!(result.is_err());
            assert_eq!(app.snapshot().unwrap().generation, 2);
            assert!(app.concealed.load(Ordering::Acquire));
            assert!(library.security.lock().unwrap().password.is_none());
            assert!(app
                .snapshot()
                .unwrap()
                .security
                .lock()
                .unwrap()
                .password
                .is_none());
            assert!(!library.archives.busy.load(Ordering::Acquire));
        }
    }
}
