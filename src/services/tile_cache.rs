//! On-demand tile fetching + disk cache.
//!
//! Cache layout: `<drive>/.photovault/tile_cache/{z}/{x}/{y}.png`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::Semaphore;

use crate::db;

use super::map_math::TileId;

pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 500 * 1024 * 1024;
pub const USER_AGENT: &str = "PhotoVault/0.1 (https://github.com/photovault)";

const MAX_CONCURRENT_FETCHES: usize = 6;
const FETCH_TIMEOUT_SECS: u64 = 8;
const CARTO_SUBDOMAINS: [&str; 3] = ["a", "b", "c"];

#[derive(Clone, Debug)]
pub struct TileCache {
    root: PathBuf,
    client: reqwest::Client,
    sem: Arc<Semaphore>,
    limit_bytes: u64,
}

impl TileCache {
    pub fn new(drive: &Path, limit_bytes: u64) -> Self {
        let root = db::tile_cache_dir(drive);
        let _ = std::fs::create_dir_all(&root);

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
            .build()
            .expect("reqwest client");

        Self {
            root,
            client,
            sem: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
            limit_bytes,
        }
    }

    pub fn tile_path(&self, t: TileId) -> PathBuf {
        self.root
            .join(t.z.to_string())
            .join(t.x.to_string())
            .join(format!("{}.png", t.y))
    }

    pub fn has(&self, t: TileId) -> bool {
        self.tile_path(t).exists()
    }

    /// Return cached tile path, or fetch/store first.
    pub async fn get_or_fetch(&self, t: TileId) -> Result<PathBuf, String> {
        let path = self.tile_path(t);
        if path.exists() {
            touch_mtime(&path);
            return Ok(path);
        }

        let _permit = self.sem.acquire().await.map_err(|e| e.to_string())?;

        if path.exists() {
            touch_mtime(&path);
            return Ok(path);
        }

        let sub = CARTO_SUBDOMAINS[(t.x as usize + t.y as usize) % CARTO_SUBDOMAINS.len()];
        let url = format!(
            "https://{}.basemaps.cartocdn.com/light_all/{}/{}/{}.png",
            sub, t.z, t.x, t.y
        );

        let bytes = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("fetch {}: {}", url, e))?
            .error_for_status()
            .map_err(|e| format!("status {}: {}", url, e))?
            .bytes()
            .await
            .map_err(|e| format!("read {}: {}", url, e))?;

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        std::fs::write(&path, &bytes).map_err(|e| format!("write {}: {}", path.display(), e))?;
        Ok(path)
    }

    /// Walk cache and evict oldest tiles until under cap.
    pub fn evict_if_over_limit(&self) -> Result<u64, String> {
        let mut entries: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let mut total: u64 = 0;
        walk_cache(&self.root, &mut entries, &mut total)?;

        if total <= self.limit_bytes {
            return Ok(total);
        }

        entries.sort_by_key(|e| e.1);
        let mut freed = 0u64;

        for (path, _, size) in entries {
            if total.saturating_sub(freed) <= self.limit_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                freed += size;
            }
        }

        Ok(total.saturating_sub(freed))
    }

    pub fn clear(&self) -> Result<(), String> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn current_size_bytes(&self) -> u64 {
        let mut total = 0u64;
        let mut entries = Vec::new();
        let _ = walk_cache(&self.root, &mut entries, &mut total);
        total
    }

    pub fn set_limit_bytes(&mut self, limit: u64) {
        self.limit_bytes = limit;
    }
}

fn touch_mtime(path: &Path) {
    let _ = std::fs::File::open(path).and_then(|f| f.set_modified(SystemTime::now()));
}

fn walk_cache(
    dir: &Path,
    out: &mut Vec<(PathBuf, SystemTime, u64)>,
    total: &mut u64,
) -> Result<(), String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };

    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_cache(&p, out, total)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("png") {
            if let Ok(md) = entry.metadata() {
                let size = md.len();
                *total += size;
                out.push((p, md.modified().unwrap_or(SystemTime::UNIX_EPOCH), size));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn tile_path_layout() {
        let td = TempDir::new().expect("tempdir");
        let cache = TileCache::new(td.path(), DEFAULT_CACHE_LIMIT_BYTES);
        let p = cache.tile_path(TileId { z: 5, x: 7, y: 11 });
        assert!(p.ends_with("tile_cache/5/7/11.png"));
    }

    #[test]
    fn has_false_on_empty() {
        let td = TempDir::new().expect("tempdir");
        let cache = TileCache::new(td.path(), DEFAULT_CACHE_LIMIT_BYTES);
        assert!(!cache.has(TileId { z: 0, x: 0, y: 0 }));
    }

    #[test]
    fn clear_removes_all() {
        let td = TempDir::new().expect("tempdir");
        let cache = TileCache::new(td.path(), DEFAULT_CACHE_LIMIT_BYTES);

        let p = cache.tile_path(TileId { z: 0, x: 0, y: 0 });
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdirs");
        std::fs::write(&p, b"PNG").expect("write");

        assert!(cache.has(TileId { z: 0, x: 0, y: 0 }));
        cache.clear().expect("clear");
        assert!(!cache.has(TileId { z: 0, x: 0, y: 0 }));
    }

    #[test]
    fn eviction_respects_limit() {
        let td = TempDir::new().expect("tempdir");
        let cache = TileCache::new(td.path(), 100);

        for i in 0..10 {
            let p = cache.tile_path(TileId { z: 1, x: i, y: 0 });
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdirs");
            std::fs::write(&p, vec![0u8; 50]).expect("write");
        }

        let remaining = cache.evict_if_over_limit().expect("evict");
        assert!(remaining <= 100, "remaining {} > 100", remaining);
    }
}
