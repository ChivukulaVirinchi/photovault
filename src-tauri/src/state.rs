//! Runtime state held by Tauri's `manage`.
//!
//! `AppState` owns the currently-open library (if any) and the registry of
//! background jobs (cancellation flags, identifiers). Every IPC handler
//! takes `State<'_, AppState>` to reach the open library.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use smriti::db::Database;
use smriti::services::semantic::{SemanticIndexCache, SemanticModelRunner};
use smriti::services::thumbnail::ThumbnailService;
use tokio::sync::{Mutex, RwLock};

use smriti::services::assistant::{AssistantDraft, AssistantRun};

pub struct AppState {
    pub library: RwLock<Option<OpenLibrary>>,
    pub jobs: Mutex<JobRegistry>,
    pub assistant: Mutex<AssistantRuntime>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            library: RwLock::new(None),
            jobs: Mutex::new(JobRegistry::default()),
            assistant: Mutex::new(AssistantRuntime::default()),
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
    pub semantic_index: Arc<std::sync::Mutex<SemanticIndexCache>>,
    pub semantic_runner: Arc<std::sync::Mutex<Option<SemanticModelRunner>>>,
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
            semantic_index: Arc::new(std::sync::Mutex::new(SemanticIndexCache::default())),
            semantic_runner: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    fn build_thumbnails(drive_root: &Path) -> std::io::Result<Arc<ThumbnailService>> {
        // Disk budget comes from user settings. Falls back to 5 GB if
        // the config file is missing or corrupt — same default as the
        // setting's bottom value, so users on small disks aren't
        // surprised after a fresh install.
        let cfg = smriti::config::AppConfig::load();
        let svc = ThumbnailService::new(drive_root, cfg.thumbnail_cache_gb)?;
        let _ = svc.load_existing_thumbnails();
        Ok(Arc::new(svc))
    }
}

#[derive(Default)]
pub struct JobRegistry {
    inner: HashMap<String, JobHandle>,
}

#[derive(Default)]
pub struct AssistantRuntime {
    pub sessions: HashMap<String, AssistantSession>,
}

pub struct AssistantSession {
    pub run: AssistantRun,
    pub draft: Option<AssistantDraft>,
    pub library_root: String,
    pub messages: Vec<AssistantMessage>,
    pub current_result_ids: Vec<i64>,
}

#[derive(Clone, Debug)]
pub struct AssistantMessage {
    pub role: String,
    pub content: String,
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
    /// True iff some currently-registered job has the given kind. Used by
    /// `start_*` commands to refuse a second concurrent job of the same
    /// kind (e.g. double-click on "Scan" while one is already running).
    pub fn has_any_of_kind(&self, kind: JobKind) -> bool {
        self.inner.values().any(|h| h.kind == kind)
    }
}

pub struct JobHandle {
    pub cancel_flag: Arc<AtomicBool>,
    pub kind: JobKind,
}

#[allow(dead_code)] // variants used in M2
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Scan,
    MetadataExtraction,
    FaceProcessing,
    Duplicates,
    Bursts,
    Documents,
    Geocoding,
    Thumbnails,
    AssetInstall,
    SemanticAssets,
    SemanticIndex,
    UpdateDownload,
    /// Trip / event detection over photo metadata. Long enough on
    /// large libraries to deserve background-job tracking so the user
    /// can navigate freely while it runs.
    AlbumSuggestions,
    AlbumExport,
}
