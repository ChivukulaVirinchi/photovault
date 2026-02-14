//! Drive detection service
//!
//! Detects mounted external drives and directories that can be indexed.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Information about a detected drive or folder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_removable: bool,
    pub has_photovault_db: bool,
    pub total_size_bytes: Option<u64>,
    pub free_space_bytes: Option<u64>,
}

/// Drive detection service
pub struct DriveDetector;

impl DriveDetector {
    /// Detect available drives/mount points
    pub fn detect() -> Vec<DriveInfo> {
        let mut drives = Vec::new();

        #[cfg(target_os = "linux")]
        {
            // Check /media and /mnt for mounted drives
            for base in &["/media", "/mnt", "/run/media"] {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        // For /media and /run/media, we need to go one level deeper (user folder)
                        if *base == "/media" || base.starts_with("/run/media") {
                            if let Ok(user_entries) = std::fs::read_dir(entry.path()) {
                                for user_entry in user_entries.flatten() {
                                    if let Some(drive) = Self::check_path(user_entry.path()) {
                                        drives.push(drive);
                                    }
                                }
                            }
                        } else if let Some(drive) = Self::check_path(entry.path()) {
                            drives.push(drive);
                        }
                    }
                }
            }

            // Also check home directory as a valid target
            if let Some(home) = dirs::home_dir() {
                if let Some(drive) = Self::check_path(home.join("Pictures")) {
                    drives.push(drive);
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Check /Volumes for mounted drives
            if let Ok(entries) = std::fs::read_dir("/Volumes") {
                for entry in entries.flatten() {
                    if let Some(drive) = Self::check_path(entry.path()) {
                        drives.push(drive);
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Check drive letters
            for letter in 'A'..='Z' {
                let path = PathBuf::from(format!("{}:\\", letter));
                if path.exists() {
                    if let Some(drive) = Self::check_path(path) {
                        drives.push(drive);
                    }
                }
            }
        }

        drives
    }

    /// Check if a path is a valid drive/folder for indexing
    fn check_path(path: PathBuf) -> Option<DriveInfo> {
        if !path.exists() || !path.is_dir() {
            return None;
        }

        // Check if it's readable
        if std::fs::read_dir(&path).is_err() {
            return None;
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let has_photovault_db = path.join(".photovault").join("photovault.db").exists();

        Some(DriveInfo {
            name,
            path,
            is_removable: true, // Simplified - could detect properly
            has_photovault_db,
            total_size_bytes: None, // Could use platform-specific APIs
            free_space_bytes: None,
        })
    }
}
