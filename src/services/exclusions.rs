//! Folder exclusion matching and path normalization.

use std::path::{Component, Path};

use rusqlite::Connection;

use crate::db::ExcludedFolderRepo;
use crate::services::path_util::relative_path_for_storage;

#[derive(Debug, Clone, Default)]
pub struct ExclusionMatcher {
    relative_paths: Vec<String>,
}

impl ExclusionMatcher {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_db(conn: &Connection) -> rusqlite::Result<Self> {
        let paths = ExcludedFolderRepo::new(conn).relative_paths()?;
        Ok(Self::new(paths))
    }

    pub fn new(relative_paths: Vec<String>) -> Self {
        let mut relative_paths: Vec<String> = relative_paths
            .into_iter()
            .filter_map(|p| normalize_stored_relative(&p).ok())
            .collect();
        relative_paths.sort();
        relative_paths.dedup();
        Self { relative_paths }
    }

    pub fn is_excluded(&self, stored_relative_path: &str) -> bool {
        self.relative_paths
            .iter()
            .any(|excluded| path_is_self_or_descendant(stored_relative_path, excluded))
    }

    pub fn should_skip_path(&self, root: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let stored = relative_path_for_storage(relative);
        !stored.is_empty() && self.is_excluded(&stored)
    }
}

pub fn normalize_stored_relative(path: &str) -> Result<String, String> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() {
        return Err("path is empty".into());
    }

    let mut parts = Vec::new();
    for component in Path::new(&path).components() {
        match component {
            Component::Normal(part) => {
                let s = part
                    .to_str()
                    .ok_or_else(|| "path must be valid UTF-8".to_string())?;
                if !s.is_empty() {
                    parts.push(s.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => return Err("path contains parent traversal".into()),
            Component::RootDir | Component::Prefix(_) => return Err("path must be relative".into()),
        }
    }

    if parts.is_empty() {
        return Err("path is empty".into());
    }
    Ok(parts.join("/"))
}

pub fn path_is_self_or_descendant(path: &str, folder: &str) -> bool {
    path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_skips_descendants_not_similar_prefixes() {
        let matcher = ExclusionMatcher::new(vec!["Trips/Goa".into()]);

        assert!(matcher.is_excluded("Trips/Goa"));
        assert!(matcher.is_excluded("Trips/Goa/a.jpg"));
        assert!(matcher.is_excluded("Trips/Goa/Sub/b.jpg"));
        assert!(!matcher.is_excluded("Trips/Goa2/c.jpg"));
        assert!(!matcher.is_excluded("Other/Goa/d.jpg"));
    }

    #[test]
    fn normalization_rejects_unsafe_paths() {
        assert_eq!(
            normalize_stored_relative(r"Trips\Goa/./A").unwrap(),
            "Trips/Goa/A"
        );
        assert!(normalize_stored_relative("../Goa").is_err());
        assert!(normalize_stored_relative("").is_err());
    }
}
