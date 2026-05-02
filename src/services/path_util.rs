//! Path utilities used by storage layer.
//!
//! Stored relative paths are normalized to forward slashes regardless
//! of the host OS. This lets the same drive be opened on Linux,
//! Windows, and macOS without the reindexer flagging every photo as
//! "new" because the stored path string differs by separator.

use std::path::Path;

/// Convert a relative path into the canonical string form used in the
/// `photos.file_path` and related columns: forward slashes only.
///
/// `Path::join(s)` accepts forward slashes on every platform, so the
/// read side stays unchanged — only the storage side needs to settle
/// on one form.
pub fn relative_path_for_storage(p: &Path) -> String {
    p.to_string_lossy().replace("\\", "/")
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
}
