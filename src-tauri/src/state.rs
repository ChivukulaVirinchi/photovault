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
    pub active_session: std::sync::atomic::AtomicU64,
    pub library_lifecycle: Mutex<()>,
    pub library: RwLock<Option<OpenLibrary>>,
    pub unsupported_library: RwLock<Option<UnsupportedLibrary>>,
    pub jobs: Mutex<JobRegistry>,
    pub assistant: Mutex<AssistantRuntime>,
    /// Application-wide text model. Model assets do not belong to a photo
    /// library, so opening or switching drives must never load them again.
    pub semantic_runner: Arc<std::sync::Mutex<Option<SemanticModelRunner>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_session: std::sync::atomic::AtomicU64::new(0),
            library_lifecycle: Mutex::new(()),
            library: RwLock::new(None),
            unsupported_library: RwLock::new(None),
            jobs: Mutex::new(JobRegistry::default()),
            assistant: Mutex::new(AssistantRuntime::default()),
            semantic_runner: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OpenLibrary {
    pub maintenance_cancel: Arc<AtomicBool>,
    pub session_id: u64,
    pub drive_root: PathBuf,
    pub db: Arc<Mutex<Database>>,
    /// On-demand thumbnail generator. Shared across handlers so the
    /// concurrency limiter applies globally, not per-request.
    pub thumbnails: Arc<ThumbnailService>,
    pub semantic_index: Arc<std::sync::Mutex<SemanticIndexCache>>,
    pub semantic_runner: Arc<std::sync::Mutex<Option<SemanticModelRunner>>>,
    /// Semantic IDs for the query currently being paged by Search. The first
    /// page computes them once; later pages must reuse the same snapshot so a
    /// 20,000-photo result does not run ONNX inference for every batch.
    pub semantic_queries: Arc<std::sync::Mutex<SemanticQueryPageCache>>,
}

/// Snapshot of the semantic candidate list for a query, keyed by query text
/// and index revision.
///
/// Search pages through results by offset, so every page must be ranked
/// against the *same* candidate list. Recomputing per page would mean running
/// the text embedding for every batch; worse, if the ranking changed between
/// pages the user would see duplicated and skipped photos while scrolling.
///
/// Access order is tracked explicitly so eviction is least-recently-used. A
/// plain `HashMap` with `keys().next()` eviction picks an *arbitrary* entry —
/// which could evict the query the user is actively scrolling, forcing a
/// recompute mid-scroll and, before this was fixed, silently changing the
/// result ordering.
#[derive(Debug, Default)]
pub struct SemanticQueryPageCache {
    entries: HashMap<String, (i64, Vec<i64>)>,
    /// Monotonic access counter; higher means used more recently.
    last_used: HashMap<String, u64>,
    clock: u64,
}

impl SemanticQueryPageCache {
    const MAX_ENTRIES: usize = 8;

    pub fn get(&mut self, query: &str, revision: i64) -> Option<Vec<i64>> {
        let matches_revision = self
            .entries
            .get(query)
            .is_some_and(|(cached_revision, _)| *cached_revision == revision);
        if !matches_revision {
            // A revision bump means the vectors changed under us; drop the
            // stale entry rather than leaving it to occupy a slot.
            if self.entries.contains_key(query) {
                self.entries.remove(query);
                self.last_used.remove(query);
            }
            return None;
        }
        self.touch(query);
        self.entries.get(query).map(|(_, ids)| ids.clone())
    }

    pub fn remember(&mut self, query: String, revision: i64, photo_ids: Vec<i64>) {
        if self.entries.len() >= Self::MAX_ENTRIES && !self.entries.contains_key(&query) {
            if let Some(lru) = self
                .last_used
                .iter()
                .min_by_key(|(_, used)| **used)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&lru);
                self.last_used.remove(&lru);
            }
        }
        self.entries.insert(query.clone(), (revision, photo_ids));
        self.touch(&query);
    }

    fn touch(&mut self, query: &str) {
        self.clock = self.clock.wrapping_add(1);
        self.last_used.insert(query.to_owned(), self.clock);
    }
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
        static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let thumbnails = Self::build_thumbnails(&drive_root)?;
        Ok(Self {
            maintenance_cancel: Arc::new(AtomicBool::new(false)),
            session_id: NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            drive_root,
            db: Arc::new(Mutex::new(database)),
            thumbnails,
            semantic_index: Arc::new(std::sync::Mutex::new(SemanticIndexCache::default())),
            // Replaced with AppState's application-wide runner by
            // `library_open`. Keeping a private default here makes this
            // constructor usable by isolated command tests.
            semantic_runner: Arc::new(std::sync::Mutex::new(None)),
            semantic_queries: Arc::new(std::sync::Mutex::new(SemanticQueryPageCache::default())),
        })
    }

    fn build_thumbnails(drive_root: &Path) -> std::io::Result<Arc<ThumbnailService>> {
        // Disk budget comes from user settings. Falls back to 5 GB if
        // the config file is missing or corrupt — same default as the
        // setting's bottom value, so users on small disks aren't
        // surprised after a fresh install.
        let cfg = smriti::config::AppConfig::load();
        let svc = ThumbnailService::new(drive_root, cfg.thumbnail_cache_gb)?;
        Ok(Arc::new(svc))
    }
}

impl Drop for OpenLibrary {
    fn drop(&mut self) {
        self.maintenance_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod semantic_query_cache_tests {
    use super::SemanticQueryPageCache;

    #[test]
    fn reuses_only_the_same_query_and_index_revision() {
        let mut cache = SemanticQueryPageCache::default();
        cache.remember("sunset".into(), 4, vec![7, 9]);

        assert_eq!(cache.get("sunset", 4), Some(vec![7, 9]));
        assert_eq!(cache.get("beach", 4), None);
        assert_eq!(cache.get("sunset", 5), None);
    }

    #[test]
    fn stays_bounded_during_fast_typing() {
        let mut cache = SemanticQueryPageCache::default();
        for n in 0..20 {
            cache.remember(format!("query-{n}"), 1, vec![n]);
        }

        assert_eq!(cache.entries.len(), SemanticQueryPageCache::MAX_ENTRIES);
    }

    /// Eviction must be least-recently-used, not arbitrary.
    ///
    /// Search pages by offset, so the query being scrolled has to survive
    /// while the user types other things. HashMap-iteration eviction (the
    /// previous behaviour) could drop exactly that entry, which forced a
    /// mid-scroll recompute and — before the paging fix — silently reordered
    /// the results.
    #[test]
    fn evicts_least_recently_used_and_keeps_the_active_query() {
        let mut cache = SemanticQueryPageCache::default();
        // The query the user is paging through.
        cache.remember("active".into(), 1, vec![1]);
        // Fill the rest of the cache.
        for n in 0..(SemanticQueryPageCache::MAX_ENTRIES - 1) {
            cache.remember(format!("filler-{n}"), 1, vec![n as i64]);
        }
        // Paging the active query touches it, so it is now the most recent.
        assert_eq!(cache.get("active", 1), Some(vec![1]));
        // A new query must evict something, but never the active one.
        cache.remember("newcomer".into(), 1, vec![99]);

        assert_eq!(
            cache.get("active", 1),
            Some(vec![1]),
            "the actively-paged query was evicted"
        );
        assert!(cache.entries.len() <= SemanticQueryPageCache::MAX_ENTRIES);
        assert_eq!(cache.last_used.len(), cache.entries.len());
    }

    /// A revision bump invalidates cached vectors rather than leaving stale
    /// ids to be served as if they were current.
    #[test]
    fn revision_change_drops_the_entry() {
        let mut cache = SemanticQueryPageCache::default();
        cache.remember("sunset".into(), 1, vec![5]);
        assert_eq!(cache.get("sunset", 2), None);
        assert!(!cache.entries.contains_key("sunset"));
        assert!(!cache.last_used.contains_key("sunset"));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Scan,
    MetadataExtraction,
    FaceProcessing,
    Duplicates,
    Bursts,
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
