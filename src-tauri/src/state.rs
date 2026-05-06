//! Runtime state held by Tauri's `manage`.
//!
//! `AppState` owns the currently-open library (if any) and the registry of
//! background jobs (cancellation flags, identifiers). Every IPC handler
//! takes `State<'_, AppState>` to reach the open library.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use photovault::db::Database;
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    pub library: RwLock<Option<OpenLibrary>>,
    pub jobs: Mutex<JobRegistry>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            library: RwLock::new(None),
            jobs: Mutex::new(JobRegistry::default()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OpenLibrary {
    pub drive_root: PathBuf,
    pub db: Arc<Mutex<Database>>,
}

impl OpenLibrary {
    pub fn new(drive_root: PathBuf, database: Database) -> Self {
        Self {
            drive_root,
            db: Arc::new(Mutex::new(database)),
        }
    }
}

#[derive(Default)]
pub struct JobRegistry {
    inner: HashMap<String, JobHandle>,
}

impl JobRegistry {
    pub fn register(&mut self, job_id: String, handle: JobHandle) {
        self.inner.insert(job_id, handle);
    }
    pub fn cancel(&mut self, job_id: &str) -> bool {
        if let Some(h) = self.inner.get(job_id) {
            h.cancel_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            false
        }
    }
    pub fn finish(&mut self, job_id: &str) {
        self.inner.remove(job_id);
    }
}

pub struct JobHandle {
    pub cancel_flag: Arc<AtomicBool>,
    pub kind: JobKind,
}

#[allow(dead_code)] // variants used in M2
pub enum JobKind {
    Scan,
    FaceProcessing,
    Duplicates,
    Bursts,
    Documents,
    Geocoding,
    Thumbnails,
    AssetInstall,
    UpdateDownload,
}
