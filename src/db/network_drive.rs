//! Libraries kept on a NAS or network share.
//!
//! SQLite's WAL mode keeps its index in shared memory (the `-shm` file),
//! which only works when every connection runs on the machine that owns
//! the file. Over SMB or NFS the locks behind it are unreliable: scans
//! stall on "database is locked" and memory-mapped pages can go stale.
//! A library on a network filesystem therefore uses SQLite's classic
//! rollback journal and no memory mapping (see `Database::open_for_drive`).
//!
//! Detection is automatic. A marker file in the library's metadata folder
//! turns network mode on for mounts that look local — FUSE mounts the
//! detector misses, unusual setups — and travels with the library.
//! Treating a local disk as network costs some write speed; treating a
//! share as local costs a stuck library, so doubtful cases count as network.

use std::path::{Path, PathBuf};

use super::connection::library_metadata_dir;

const MARKER_FILE: &str = "network-drive";

/// Whether a library's database runs in network mode, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkDrive {
    /// The library sits on a filesystem Smriti recognises as networked.
    pub detected: bool,
    /// The user turned network mode on for this library.
    pub forced: bool,
}

impl NetworkDrive {
    pub fn active(&self) -> bool {
        self.detected || self.forced
    }
}

pub fn network_drive(drive_root: &Path) -> NetworkDrive {
    NetworkDrive {
        detected: is_network_filesystem(drive_root),
        forced: marker_path(drive_root).is_file(),
    }
}

/// Turn the user's network-mode override for a library on or off. Takes
/// effect the next time the library opens.
pub fn set_forced(drive_root: &Path, forced: bool) -> std::io::Result<()> {
    let marker = marker_path(drive_root);
    if forced {
        std::fs::create_dir_all(library_metadata_dir(drive_root))?;
        std::fs::write(
            marker,
            "Smriti keeps this library's database in network-drive mode.\n",
        )
    } else {
        match std::fs::remove_file(marker) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

fn marker_path(drive_root: &Path) -> PathBuf {
    library_metadata_dir(drive_root).join(MARKER_FILE)
}

/// True when `path` lives on a network filesystem.
pub fn is_network_filesystem(path: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    platform::is_network_filesystem(&path)
}

#[cfg(windows)]
mod platform {
    use std::path::{Component, Path, Prefix};

    const DRIVE_REMOTE: u32 = 4;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
    }

    pub fn is_network_filesystem(path: &Path) -> bool {
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return false;
        };
        match prefix.kind() {
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) => true,
            // Mapped network drives keep their letter.
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                let root: Vec<u16> = format!("{}:\\", letter as char)
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                // SAFETY: `root` is a NUL-terminated UTF-16 string that
                // outlives the call.
                unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
            }
            _ => false,
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::Path;

    pub fn is_network_filesystem(path: &Path) -> bool {
        std::fs::read_to_string("/proc/self/mountinfo")
            .ok()
            .and_then(|mountinfo| super::fstype_for(&mountinfo, path))
            .is_some_and(|fstype| super::is_network_fstype(&fstype))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn is_network_filesystem(path: &Path) -> bool {
        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: `c_path` is NUL-terminated and `stat` is a valid out-pointer.
        if unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return false;
        }
        // SAFETY: statfs succeeded, so it filled in `stat`, and
        // `f_fstypename` is NUL-terminated.
        let fstype = unsafe { CStr::from_ptr(stat.assume_init_ref().f_fstypename.as_ptr()) };
        matches!(
            fstype.to_bytes(),
            b"smbfs" | b"nfs" | b"afpfs" | b"webdav" | b"cifs" | b"ftp" | b"macfuse" | b"osxfuse"
        )
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    pub fn is_network_filesystem(_path: &std::path::Path) -> bool {
        false
    }
}

/// The filesystem type of the mount holding `path`, from the contents of
/// Linux's `/proc/self/mountinfo`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn fstype_for(mountinfo: &str, path: &Path) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for line in mountinfo.lines() {
        // `<id> <parent> <dev> <root> <mount point> <options> [tags] - <fstype> <source> <options>`
        let Some((mount, fs)) = line.split_once(" - ") else {
            continue;
        };
        let (Some(mount_point), Some(fstype)) = (mount.split(' ').nth(4), fs.split(' ').next())
        else {
            continue;
        };
        let mount_point = unescape_mount_point(mount_point);
        // Longest match wins; on a tie the later (newer, on top) mount does.
        let depth = Path::new(&mount_point).components().count();
        if path.starts_with(&mount_point) && best.is_none_or(|(d, _)| depth >= d) {
            best = Some((depth, fstype));
        }
    }
    best.map(|(_, fstype)| fstype.to_string())
}

/// mountinfo escapes space, tab, newline and backslash as `\ooo` octal.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn unescape_mount_point(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            .filter(|d| d.iter().all(|b| (b'0'..=b'7').contains(b)));
        if let (b'\\', Some(digits)) = (bytes[i], octal) {
            out.push(
                digits
                    .iter()
                    .fold(0u8, |n, d| n.wrapping_mul(8).wrapping_add(d - b'0')),
            );
            i += 4;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Network filesystems, plus every named FUSE filesystem (sshfs, rclone,
/// GNOME's gvfs SMB mounts). Block-device FUSE mounts such as ntfs-3g and
/// exFAT report `fuseblk` and stay local.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_network_fstype(fstype: &str) -> bool {
    matches!(
        fstype,
        "cifs"
            | "smb3"
            | "smbfs"
            | "nfs"
            | "nfs4"
            | "ncpfs"
            | "afs"
            | "ceph"
            | "glusterfs"
            | "9p"
            | "davfs"
            | "lustre"
            | "beegfs"
            | "gpfs"
    ) || fstype.starts_with("fuse.")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTINFO: &str = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
40 22 8:17 / /media/me/USB\\040Drive rw,nosuid shared:20 - fuseblk /dev/sdb1 rw
41 22 0:50 / /mnt/nas rw,relatime shared:21 - cifs //nas/photos rw,vers=3.0
42 41 0:51 / /mnt/nas/local rw,relatime shared:22 - ext4 /dev/sdc1 rw
43 22 0:52 / /run/user/1000/gvfs rw shared:23 - fuse.gvfsd-fuse gvfsd-fuse rw
";

    #[test]
    fn picks_the_deepest_mount_holding_the_path() {
        let fstype = |p: &str| fstype_for(MOUNTINFO, Path::new(p));
        assert_eq!(fstype("/home/me/Pictures").as_deref(), Some("ext4"));
        assert_eq!(fstype("/mnt/nas/2023").as_deref(), Some("cifs"));
        assert_eq!(fstype("/mnt/nas/local/x").as_deref(), Some("ext4"));
        // Component-wise: /mnt/nassy is not under /mnt/nas.
        assert_eq!(fstype("/mnt/nassy").as_deref(), Some("ext4"));
        assert_eq!(
            fstype("/media/me/USB Drive/DCIM").as_deref(),
            Some("fuseblk")
        );
    }

    #[test]
    fn classifies_filesystem_types() {
        for fstype in [
            "cifs",
            "smb3",
            "nfs4",
            "fuse.sshfs",
            "fuse.rclone",
            "fuse.gvfsd-fuse",
        ] {
            assert!(is_network_fstype(fstype), "{fstype}");
        }
        for fstype in ["ext4", "btrfs", "exfat", "vfat", "ntfs3", "fuseblk", "apfs"] {
            assert!(!is_network_fstype(fstype), "{fstype}");
        }
    }

    #[test]
    fn unescapes_octal_sequences() {
        assert_eq!(unescape_mount_point("/a\\040b\\134c"), "/a b\\c");
        assert_eq!(unescape_mount_point("/plain"), "/plain");
        assert_eq!(unescape_mount_point("/trailing\\04"), "/trailing\\04");
    }

    #[test]
    fn marker_turns_network_mode_on_and_off() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!network_drive(dir.path()).forced);
        set_forced(dir.path(), true).unwrap();
        assert!(network_drive(dir.path()).forced);
        assert!(network_drive(dir.path()).active());
        set_forced(dir.path(), false).unwrap();
        set_forced(dir.path(), false).unwrap(); // already off: no error
        assert!(!network_drive(dir.path()).forced);
    }

    #[test]
    fn temp_dirs_are_local() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_network_filesystem(dir.path()));
    }
}
