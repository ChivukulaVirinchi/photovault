//! Shared helpers for long-running job commands.
//!
//! Pattern: a `#[tauri::command]` calls `start_job(state, kind)` to get a
//! `Job` (with `job_id` + cancel flag), spawns a tokio task that does the
//! work and forwards progress via `emit_progress(app, topic, payload)`,
//! then calls `finish_job(state, job_id)` when done. The handler returns
//! the `JobIdDto` immediately so the frontend can subscribe to events.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::state::{AppState, JobHandle, JobKind};
use crate::CommandError;

pub struct Job {
    pub id: String,
    pub cancel: Arc<AtomicBool>,
    pub started_at: std::time::Instant,
}

pub async fn start_job(state: &AppState, kind: JobKind) -> Result<Job, CommandError> {
    // Library-scoped callers hold library_lifecycle from snapshot capture
    // through registration. Open/close use the same lock, so a worker
    // cannot capture one database and register against another session.
    let session = if kind.is_library_scoped() {
        state
            .library
            .read()
            .await
            .as_ref()
            .ok_or(CommandError::LibraryClosed)?
            .session_id
    } else {
        0
    };
    let id = format!("{session}:{}", Uuid::new_v4());
    let cancel = Arc::new(AtomicBool::new(false));
    let mut jobs = state.jobs.lock().await;
    if jobs.has_any_of_kind(kind) {
        return Err(CommandError::Conflict {
            reason: format!("{kind:?} is already in progress"),
        });
    }
    jobs.register(
        id.clone(),
        JobHandle {
            cancel_flag: cancel.clone(),
            kind,
        },
    );
    Ok(Job {
        id,
        cancel,
        started_at: std::time::Instant::now(),
    })
}

pub async fn finish_job(state: &AppState, job_id: &str) {
    state.jobs.lock().await.finish(job_id);
}

pub fn emit<T: serde::Serialize + Clone>(app: &AppHandle, topic: &str, payload: T) {
    let Ok(payload) = serde_json::to_value(payload) else {
        return;
    };
    if let Some(job_id) = payload.get("job_id").and_then(|value| value.as_str()) {
        let state = app.state::<AppState>();
        let active = state
            .active_session
            .load(std::sync::atomic::Ordering::Acquire);
        if !job_belongs_to_session(job_id, active) {
            return;
        }
    }
    let _ = app.emit(topic, payload);
}

fn job_belongs_to_session(job_id: &str, session: u64) -> bool {
    let Some((prefix, _)) = job_id.split_once(':') else {
        return false;
    };
    prefix
        .parse::<u64>()
        .is_ok_and(|id| id == 0 || id == session)
}

#[cfg(test)]
mod tests {
    use super::job_belongs_to_session;

    #[tokio::test]
    async fn library_switch_cannot_interleave_snapshot_and_registration() {
        use super::*;
        use crate::state::OpenLibrary;
        use smriti::db::Database;
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new());
        let library = OpenLibrary::new(
            dir.path().to_owned(),
            Database::open_for_drive(dir.path()).unwrap(),
        )
        .unwrap();
        let session = library.session_id;
        *state.library.write().await = Some(library);
        let lifecycle = state.library_lifecycle.lock().await;
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let closer_state = state.clone();
        let closer = tokio::spawn(async move {
            ready_tx.send(()).unwrap();
            let _lifecycle = closer_state.library_lifecycle.lock().await;
            closer_state.jobs.lock().await.cancel_library_scoped();
            *closer_state.library.write().await = None;
        });
        ready_rx.await.unwrap();
        let job = start_job(&state, JobKind::Scan).await.unwrap();
        assert!(job_belongs_to_session(&job.id, session));
        assert!(!job.cancel.load(std::sync::atomic::Ordering::Relaxed));
        drop(lifecycle);
        closer.await.unwrap();
        assert!(job.cancel.load(std::sync::atomic::Ordering::Relaxed));
        assert!(state.library.read().await.is_none());
        assert!(start_job(&state, JobKind::Scan).await.is_err());
    }
    #[test]
    fn suppresses_old_library_jobs_but_keeps_global_installs() {
        assert!(job_belongs_to_session("2:scan", 2));
        assert!(job_belongs_to_session("0:assets", 2));
        assert!(!job_belongs_to_session("1:scan", 2));
        assert!(!job_belongs_to_session("2:scan", 0));
        assert!(!job_belongs_to_session("malformed", 2));
    }
}
