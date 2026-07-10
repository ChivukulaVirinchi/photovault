//! Path utilities used by storage layer.
//!
//! Stored relative paths are normalized to forward slashes regardless
//! of the host OS. This lets the same drive be opened on Linux,
//! Windows, and macOS without the reindexer flagging every photo as
//! "new" because the stored path string differs by separator.

use std::path::{Component, Path, PathBuf};

/// Convert a relative path into the canonical string form used in the
/// `photos.file_path` and related columns: forward slashes only.
///
/// `Path::join(s)` accepts forward slashes on every platform, so the
/// read side stays unchanged — only the storage side needs to settle
/// on one form.
pub fn relative_path_for_storage(p: &Path) -> String {
    p.to_string_lossy().replace("\\", "/")
}

/// Join a DB-stored relative path to a trusted root.
///
/// Database rows live on a user-writable drive, so callers must not
/// blindly `root.join(file_path)` before opening or deleting files.
pub fn safe_join_relative(root: &Path, stored_relative: &str) -> Result<PathBuf, String> {
    if stored_relative.is_empty() {
        return Err("path is empty".to_string());
    }
    if stored_relative.contains('\\') {
        return Err("path must use normalized forward slashes".to_string());
    }

    let rel = Path::new(stored_relative);
    let mut clean = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err("path contains parent traversal".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("path must be relative".to_string())
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err("path is empty".to_string());
    }
    Ok(root.join(clean))
}

/// Resolve an existing DB-stored path and prove it still lives under
/// the trusted root after following symlinks.
pub fn safe_existing_path_under_root(
    root: &Path,
    stored_relative: &str,
) -> Result<PathBuf, String> {
    let joined = safe_join_relative(root, stored_relative)?;
    let root = root
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize root: {e}"))?;
    let resolved = joined
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize path: {e}"))?;
    if !resolved.starts_with(&root) {
        return Err("path resolves outside library root".to_string());
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn forward_slashes_pass_through() {
        let s = relative_path_for_storage(&PathBuf::from("subdir/photo.jpg"));
        assert_eq!(s, "subdir/photo.jpg");
    }

    #[test]
    fn backslashes_become_forward_slashes() {
        // Use a string with backslashes directly — PathBuf parsing on
        // Linux preserves them as part of the file name component.
        let s = relative_path_for_storage(Path::new(r"subdir\photo.jpg"));
        assert_eq!(s, "subdir/photo.jpg");
    }

    #[test]
    fn mixed_separators_normalize() {
        let s = relative_path_for_storage(Path::new(r"a\b/c\d.jpg"));
        assert_eq!(s, "a/b/c/d.jpg");
    }

    #[test]
    fn safe_join_accepts_normal_relative_paths() {
        let joined = safe_join_relative(Path::new("/photos"), "2026/IMG_001.jpg").unwrap();
        assert_eq!(joined, PathBuf::from("/photos/2026/IMG_001.jpg"));
    }

    #[test]
    fn safe_join_rejects_escape_paths() {
        assert!(safe_join_relative(Path::new("/photos"), "../secret").is_err());
        assert!(safe_join_relative(Path::new("/photos"), "/etc/passwd").is_err());
        assert!(safe_join_relative(Path::new("/photos"), r"..\secret").is_err());
    }

    #[test]
    fn safe_existing_path_rejects_symlink_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.jpg");
        std::fs::write(&outside_file, b"secret").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
            assert!(safe_existing_path_under_root(root.path(), "link/secret.jpg").is_err());
        }

        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_dir(outside.path(), root.path().join("link")).is_ok() {
                assert!(safe_existing_path_under_root(root.path(), "link/secret.jpg").is_err());
            }
        }
    }
}
