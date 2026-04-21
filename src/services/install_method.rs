//! Detect *how* PhotoVault was installed, so the update UI can take
//! the right action when the user clicks "Download".
//!
//! We deliberately avoid self-replacing package-manager-managed
//! binaries (apt, Homebrew, winget, Flatpak, Snap) — that path is
//! the package manager's job, and stomping on it would confuse the
//! system. Instead, for those users the update banner shows the
//! matching upgrade command.
//!
//! Detection is path-based because it's cheap and robust. False
//! negatives map to `Unknown`, which the update handler turns into
//! an "open release page in browser" fallback, so a wrong answer
//! here fails safely rather than corrupting an install.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// How PhotoVault was installed on this machine.
///
/// Several variants are only constructed on specific target OSes
/// (`MsiInstaller` on Windows, `MacOsApp` on macOS); allow dead-code
/// at the type level so non-target-OS builds don't fail `-D warnings`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InstallMethod {
    /// Self-contained binary — AppImage on Linux, portable zip on
    /// Windows, or the tar.gz unpacked anywhere. `self_replace` can
    /// atomically swap the running binary with a newly-downloaded one.
    Portable,

    /// Installed via a Windows MSI. The self-replace path invokes
    /// msiexec on the newly-downloaded MSI with a UAC prompt.
    MsiInstaller,

    /// macOS `.app` bundle in /Applications. The self-replace path
    /// downloads the new .dmg and opens it so the user can drag the
    /// new bundle in. Automating the copy is fragile under Gatekeeper
    /// + SIP, especially before notarization is set up.
    MacOsApp,

    /// System package manager owns the binary. We show the matching
    /// upgrade command in the banner instead of downloading.
    PackageManager { name: String, upgrade_cmd: String },

    /// Running a `cargo build` binary from the source tree. Show a
    /// `git pull && cargo build --release` hint.
    SourceBuild,

    /// Couldn't tell. Falls back to opening the release page in the
    /// user's browser.
    Unknown,
}

/// Detect the install method for the currently-running executable.
/// Uses `std::env::current_exe` internally.
pub fn detect() -> InstallMethod {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return InstallMethod::Unknown,
    };
    classify_path(&exe)
}

/// Pure, testable version of the detection heuristic.
#[allow(clippy::needless_return)]
pub(crate) fn classify_path(exe: &Path) -> InstallMethod {
    let path_str = exe.to_string_lossy();
    let lower = path_str.to_lowercase();

    // --- source build is checked first so `target/release/photovault`
    //     on a Windows box doesn't get mis-bucketed as portable.
    if lower.contains("/target/debug/") || lower.contains("/target/release/") {
        return InstallMethod::SourceBuild;
    }
    if lower.contains("\\target\\debug\\") || lower.contains("\\target\\release\\") {
        return InstallMethod::SourceBuild;
    }

    // --- Linux ----------------------------------------------------
    #[cfg(target_os = "linux")]
    {
        if lower.ends_with(".appimage") || lower.contains("/squashfs-root/") {
            return InstallMethod::Portable;
        }
        if lower.contains("/var/lib/flatpak/") || lower.starts_with("/app/bin/") {
            return InstallMethod::PackageManager {
                name: "flatpak".into(),
                upgrade_cmd: "flatpak update com.chivukulavirinchi.photovault".into(),
            };
        }
        if lower.starts_with("/snap/") {
            return InstallMethod::PackageManager {
                name: "snap".into(),
                upgrade_cmd: "snap refresh photovault".into(),
            };
        }
        // Default apt/.deb layout: binary at /usr/bin, resources under /usr/lib/photovault.
        if path_str == "/usr/bin/photovault" && PathBuf::from("/usr/lib/photovault").exists() {
            return InstallMethod::PackageManager {
                name: "apt".into(),
                upgrade_cmd: "sudo apt update && sudo apt install --only-upgrade photovault".into(),
            };
        }
    }

    // --- macOS ----------------------------------------------------
    #[cfg(target_os = "macos")]
    {
        if lower.contains("/opt/homebrew/") || lower.contains("/usr/local/cellar/") {
            return InstallMethod::PackageManager {
                name: "brew".into(),
                upgrade_cmd: "brew update && brew upgrade --cask photovault".into(),
            };
        }
        if lower.contains(".app/contents/macos/")
            && (lower.contains("/applications/") || lower.contains("/users/"))
        {
            return InstallMethod::MacOsApp;
        }
    }

    // --- Windows --------------------------------------------------
    #[cfg(target_os = "windows")]
    {
        // MSI-installed binaries typically land under Program Files.
        if lower.contains("\\program files\\") || lower.contains("\\program files (x86)\\") {
            return InstallMethod::MsiInstaller;
        }
        // Microsoft Store (MSIX) installs live under WindowsApps.
        if lower.contains("\\windowsapps\\") {
            return InstallMethod::PackageManager {
                name: "msstore".into(),
                upgrade_cmd: "Microsoft Store -> Updates".into(),
            };
        }
        // Scoop drops binaries under the user's scoop root.
        if lower.contains("\\scoop\\apps\\") {
            return InstallMethod::PackageManager {
                name: "scoop".into(),
                upgrade_cmd: "scoop update photovault".into(),
            };
        }
        // Everything else on Windows: portable zip extracted somewhere.
        return InstallMethod::Portable;
    }

    // Cross-platform fallback: any .exe that wasn't caught above, or
    // an unrecognized Linux layout, or an unrecognized macOS bundle.
    #[cfg(not(target_os = "windows"))]
    {
        InstallMethod::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn source_build_detected_unix() {
        let p = PathBuf::from("/home/user/code/photovault/target/release/photovault");
        assert_eq!(classify_path(&p), InstallMethod::SourceBuild);
    }

    #[test]
    fn source_build_detected_debug() {
        let p = PathBuf::from("/home/user/code/photovault/target/debug/photovault");
        assert_eq!(classify_path(&p), InstallMethod::SourceBuild);
    }

    #[test]
    fn source_build_detected_windows() {
        let p = PathBuf::from("C:\\Users\\foo\\photovault\\target\\release\\photovault.exe");
        assert_eq!(classify_path(&p), InstallMethod::SourceBuild);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_detected() {
        let p = PathBuf::from("/home/user/Applications/PhotoVault-x86_64.AppImage");
        assert_eq!(classify_path(&p), InstallMethod::Portable);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn flatpak_detected() {
        let p = PathBuf::from("/var/lib/flatpak/app/com.chivukulavirinchi.photovault/x86_64/stable/current/files/bin/photovault");
        match classify_path(&p) {
            InstallMethod::PackageManager { name, .. } => assert_eq!(name, "flatpak"),
            other => panic!("expected flatpak, got {:?}", other),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn snap_detected() {
        let p = PathBuf::from("/snap/photovault/current/usr/bin/photovault");
        match classify_path(&p) {
            InstallMethod::PackageManager { name, .. } => assert_eq!(name, "snap"),
            other => panic!("expected snap, got {:?}", other),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn msi_installer_detected() {
        let p = PathBuf::from("C:\\Program Files\\PhotoVault\\photovault.exe");
        assert_eq!(classify_path(&p), InstallMethod::MsiInstaller);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn scoop_detected() {
        let p = PathBuf::from("C:\\Users\\user\\scoop\\apps\\photovault\\current\\photovault.exe");
        match classify_path(&p) {
            InstallMethod::PackageManager { name, .. } => assert_eq!(name, "scoop"),
            other => panic!("expected scoop, got {:?}", other),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_app_bundle_detected() {
        let p = PathBuf::from("/Applications/PhotoVault.app/Contents/MacOS/photovault");
        assert_eq!(classify_path(&p), InstallMethod::MacOsApp);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn homebrew_cask_detected() {
        let p = PathBuf::from("/opt/homebrew/Caskroom/photovault/1.0.0/photovault");
        match classify_path(&p) {
            InstallMethod::PackageManager { name, .. } => assert_eq!(name, "brew"),
            other => panic!("expected brew, got {:?}", other),
        }
    }
}
