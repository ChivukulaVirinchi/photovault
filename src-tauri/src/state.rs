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
    pub unsupported_library: RwLock<Option<UnsupportedLibrary>>,
    pub jobs: Mutex<JobRegistry>,
    pub assistant: Mutex<AssistantRuntime>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            library: RwLock::new(None),
            unsupported_library: RwLock::new(None),
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
    /// concurrency limiter applies globally, not per-request.
    pub thumbnails: Arc<ThumbnailService>,
    pub semantic_index: Arc<std::sync::Mutex<SemanticIndexCache>>,
    pub semantic_runner: Arc<std::sync::Mutex<Option<SemanticModelRunner>>>,
}

pub struct UnsupportedLibrary {
    pub drive_root: PathBuf,
    pub db_version: i32,
    pub max_supported: i32,
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
        if let Err(e) = svc.load_existing_thumbnails() {
            tracing::warn!("failed to load existing thumbnails: {}", e);
        }
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

    pub fn cancel_library_scoped(&mut self) {
        self.inner.retain(|_, handle| {
            if !handle.kind.is_library_scoped() {
                return true;
            }
            handle
                .cancel_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
            false
        });
    }
}

pub struct JobHandle {
    pub cancel_flag: Arc<AtomicBool>,
    pub kind: JobKind,
}

#[allow(dead_code)] // variants used in M2
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    GoogleTakeoutImport,
}

impl JobKind {
    pub fn is_library_scoped(self) -> bool {
        !matches!(
            self,
            JobKind::AssetInstall | JobKind::SemanticAssets | JobKind::UpdateDownload
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn handle(kind: JobKind) -> (Arc<AtomicBool>, JobHandle) {
        let flag = Arc::new(AtomicBool::new(false));
        (
            flag.clone(),
            JobHandle {
                cancel_flag: flag,
                kind,
            },
        )
    }

    #[test]
    fn cancel_library_scoped_leaves_install_jobs_running() {
        let mut registry = JobRegistry::default();
        let (scan_flag, scan) = handle(JobKind::Scan);
        let (takeout_flag, takeout) = handle(JobKind::GoogleTakeoutImport);
        let (assets_flag, assets) = handle(JobKind::AssetInstall);
        let (semantic_assets_flag, semantic_assets) = handle(JobKind::SemanticAssets);

        registry.register("scan".into(), scan);
        registry.register("takeout".into(), takeout);
        registry.register("assets".into(), assets);
        registry.register("semantic-assets".into(), semantic_assets);

        registry.cancel_library_scoped();

        assert!(scan_flag.load(Ordering::Relaxed));
        assert!(takeout_flag.load(Ordering::Relaxed));
        assert!(!assets_flag.load(Ordering::Relaxed));
        assert!(!semantic_assets_flag.load(Ordering::Relaxed));
        assert!(!registry.has_any_of_kind(JobKind::Scan));
        assert!(!registry.has_any_of_kind(JobKind::GoogleTakeoutImport));
        assert!(registry.has_any_of_kind(JobKind::AssetInstall));
        assert!(registry.has_any_of_kind(JobKind::SemanticAssets));
    }
}
