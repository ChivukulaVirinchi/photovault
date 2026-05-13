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
}
