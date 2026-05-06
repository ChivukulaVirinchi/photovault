//! Runtime state held by Tauri's `manage`.
//!
//! `AppState` owns the currently-open library (if any) and the registry of
//! background jobs (cancellation flags, identifiers). Every IPC handler
//! takes `State<'_, AppState>` to reach the open library.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use photovault::db::Database;
use photovault::services::thumbnail::ThumbnailService;
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
    /// On-demand thumbnail generator. Shared across handlers so the
    /// 8-permit concurrency limiter applies globally, not per-request.
    pub thumbnails: Arc<ThumbnailService>,
}

impl OpenLibrary {
    /// Construct an OpenLibrary, building a ThumbnailService rooted at
    /// `drive_root`. Returns an io::Error if the cache directories
    /// can't be created (e.g. read-only mount, permission denied).
    pub fn new(drive_root: PathBuf, database: Database) -> std::io::Result<Self> {
        let thumbnails = Self::build_thumbnails(&drive_root)?;
        Ok(Self {
            drive_root,
            db: Arc::new(Mutex::new(database)),
            thumbnails,
        })
    }

    fn build_thumbnails(drive_root: &Path) -> std::io::Result<Arc<ThumbnailService>> {
        // Cache budget is generous (4 GB). Real eviction is driven by
        // the service's byte-budget check, not this ceiling.
        let svc = ThumbnailService::new(drive_root, 4.0)?;
        let _ = svc.load_existing_thumbnails();
        Ok(Arc::new(svc))
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
