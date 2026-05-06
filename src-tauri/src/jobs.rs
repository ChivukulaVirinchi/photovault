//! Shared helpers for long-running job commands.
//!
//! Pattern: a `#[tauri::command]` calls `start_job(state, kind)` to get a
//! `Job` (with `job_id` + cancel flag), spawns a tokio task that does the
//! work and forwards progress via `emit_progress(app, topic, payload)`,
//! then calls `finish_job(state, job_id)` when done. The handler returns
//! the `JobIdDto` immediately so the frontend can subscribe to events.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::state::{AppState, JobHandle, JobKind};
use crate::CommandError;

pub struct Job {
    pub id: String,
    pub cancel: Arc<AtomicBool>,
    pub started_at: std::time::Instant,
}

pub async fn start_job(state: &AppState, kind: JobKind) -> Result<Job, CommandError> {
    let id = Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut jobs = state.jobs.lock().await;
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
    let _ = app.emit(topic, payload);
}
