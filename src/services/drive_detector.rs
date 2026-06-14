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
    pub stable_id: Option<String>,
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

    /// Inspect a user-provided folder path and return DriveInfo if valid.
    pub fn inspect_path(path: PathBuf) -> Option<DriveInfo> {
        Self::check_path(path)
    }

    /// Return a stable volume identity when the platform exposes one.
    /// On Windows this is the volume GUID path, which survives drive-letter
    /// changes such as the same disk moving from F:\ to D:\.
    pub fn stable_id_for_path(path: &std::path::Path) -> Option<String> {
        platform_stable_id(path)
    }

    /// Resolve a remembered library path that may have been remounted at a
    /// different location. If a stable id is known, prefer an exact mounted
    /// volume match. For legacy configs without stable ids, use the only
    /// currently-mounted indexed Smriti library if there is exactly one.
    pub fn resolve_remembered_library_path(
        remembered_path: &std::path::Path,
        stable_id: Option<&str>,
    ) -> Option<PathBuf> {
        if remembered_path.exists() {
            return Some(remembered_path.to_path_buf());
        }

        let indexed: Vec<DriveInfo> = Self::detect()
            .into_iter()
            .filter(|d| d.has_photovault_db)
            .collect();

        if let Some(stable_id) = stable_id {
            if let Some(drive) = indexed
                .iter()
                .find(|d| d.stable_id.as_deref() == Some(stable_id))
            {
                return Some(drive.path.clone());
            }
        }

        if indexed.len() == 1 {
            return Some(indexed[0].path.clone());
        }

        None
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
            stable_id: Self::stable_id_for_path(&path),
            path,
            is_removable: true, // Simplified - could detect properly
            has_photovault_db,
            total_size_bytes: None, // Could use platform-specific APIs
            free_space_bytes: None,
        })
    }
}

#[cfg(target_os = "windows")]
fn platform_stable_id(path: &std::path::Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;

    let mut mount = path.as_os_str().encode_wide().collect::<Vec<u16>>();
    if !mount.ends_with(&[b'\\' as u16]) && !mount.ends_with(&[b'/' as u16]) {
        mount.push(b'\\' as u16);
    }
    mount.push(0);

    let mut buf = vec![0u16; 512];
    let ok = unsafe {
        GetVolumeNameForVolumeMountPointW(mount.as_ptr(), buf.as_mut_ptr(), buf.len() as u32)
    };
    if ok == 0 {
        return None;
    }

    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let value = String::from_utf16_lossy(&buf[..len]);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(not(target_os = "windows"))]
fn platform_stable_id(_path: &std::path::Path) -> Option<String> {
    None
}
