//! System: asset health, app version (read-only).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::dto::{AppVersionDto, AssetHealthDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn system_asset_health() -> CommandResult<AssetHealthDto> {
    let h = smriti::bootstrap::asset_health();
    Ok(h.into())
}

#[tauri::command]
pub async fn system_app_version() -> CommandResult<AppVersionDto> {
    Ok(AppVersionDto {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// ---------- mutations / OS integration ----------

#[derive(Debug, Deserialize)]
pub struct SystemPhotoIdArgs {
    pub photo_id: i64,
}

#[tauri::command]
pub async fn system_open_in_explorer(
    state: State<'_, AppState>,
    args: SystemPhotoIdArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = smriti::db::PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.photo_id)?
        .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
    let abs = lib.drive_root.join(&photo.file_path);
    select_in_file_manager(&abs).map_err(|e| CommandError::Io {
        message: e.to_string(),
    })?;
    Ok(())
}

/// Reveal a file in the platform's file manager — selecting the file
/// itself, not just opening the containing folder.
///
/// On modern GNOME / Fedora the naïve path (`xdg-open` → `gio open`)
/// creates a transient `systemd-run --scope --user` unit whose
/// `WorkingDirectory=` is sourced from glib's launch context, *not*
/// from the kernel-inherited cwd of the spawning process. So just
/// pinning `Command::current_dir("/")` on `xdg-open` doesn't reach
/// the failing systemd call. We bypass `xdg-open` / `gio` entirely:
/// first try the canonical D-Bus interface synchronously, then exec
/// known file manager binaries directly with `setsid` to escape our
/// process-group / cgroup scope.
#[cfg(target_os = "linux")]
fn select_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    use std::collections::HashSet;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let parent = abs
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| abs.clone());
    let uri = format!("file://{}", abs.display());

    // Build a Command that's fully detached from our cgroup / session:
    //   - cwd pinned to "/" so any process that *does* read it sees an
    //     absolute path
    //   - stdio nulled so child output doesn't leak into our terminal
    //   - pre_exec calls setsid() in the forked child, putting it into
    //     a brand-new session — escapes the systemd user scope that
    //     wraps our Tauri process
    let detached = |program: &str| -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir("/")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid() is async-signal-safe. The child is in a
        // freshly-forked state where only this closure runs before
        // exec; no other threads, no allocator state to corrupt.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        cmd
    };

    // Step 1: gdbus call to org.freedesktop.FileManager1.ShowItems.
    // `gdbus call` is synchronous (waits for reply) and propagates
    // D-Bus errors via non-zero exit, which lets us distinguish
    // "service not registered" from "succeeded".
    let array = format!("['{}']", uri.replace('\'', "\\'"));
    let gdbus = detached("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.FileManager1",
            "--object-path",
            "/org/freedesktop/FileManager1",
            "--method",
            "org.freedesktop.FileManager1.ShowItems",
            &array,
            "",
        ])
        .status();
    if matches!(gdbus, Ok(s) if s.success()) {
        return Ok(());
    }

    // Step 2: probe `xdg-mime query default inode/directory` for the
    // user's preferred file manager .desktop id, then exec the
    // matching binary directly. Skip xdg-open / gio open entirely.
    let preferred = Command::new("xdg-mime")
        .args(["query", "default", "inode/directory"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    // (.desktop id substring, binary, optional select flag).
    // Order: most common first; preferred-by-xdg-mime wins via the
    // sort below regardless of position here.
    let known: &[(&str, &str, Option<&str>)] = &[
        ("nautilus", "nautilus", Some("--select")),
        ("dolphin", "dolphin", Some("--select")),
        ("nemo", "nemo", None),
        ("thunar", "thunar", None),
        ("caja", "caja", None),
        ("pcmanfm", "pcmanfm", None),
        ("dde-file-manager", "dde-file-manager", Some("--show-item")),
        ("io.elementary.files", "io.elementary.files", None),
    ];

    // Try the user's preferred FM first, then the rest in declared order.
    let mut tried: HashSet<&str> = HashSet::new();
    let order = known
        .iter()
        .filter(|(id, _, _)| !preferred.is_empty() && preferred.contains(id))
        .chain(known.iter().filter(|(id, _, _)| !preferred.contains(id)));

    for (_, bin, flag) in order {
        if !tried.insert(bin) {
            continue;
        }
        let mut cmd = detached(bin);
        match flag {
            Some(f) => {
                cmd.arg(f).arg(&abs);
            }
            None => {
                // No --select equivalent: open the parent directory.
                cmd.arg(&parent);
            }
        }
        // spawn (don't wait) — file managers are long-lived GUI apps;
        // setsid above means orphaning is fine.
        if cmd.spawn().is_ok() {
            return Ok(());
        }
    }

    Err(std::io::Error::other(
        "no Linux file manager found (tried gdbus FileManager1, nautilus, dolphin, nemo, thunar, caja, pcmanfm, dde-file-manager)",
    ))
}

#[cfg(target_os = "macos")]
fn select_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    use std::process::Command;
    Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(std::io::Error::other("open -R failed"))
            }
        })
}

#[cfg(target_os = "windows")]
fn select_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    use std::process::Command;
    // explorer.exe accepts /select with the target path as a single
    // comma-joined argument. The combined token is one CLI arg.
    let arg = format!("/select,{}", path.display());
    Command::new("explorer.exe").arg(arg).status().map(|_| ())
}

#[derive(Debug, Serialize)]
pub struct CopiedPathDto {
    pub path: String,
}

#[tauri::command]
pub async fn system_copy_path_to_clipboard(
    state: State<'_, AppState>,
    args: SystemPhotoIdArgs,
) -> CommandResult<CopiedPathDto> {
    // Frontend uses the @tauri-apps/plugin-clipboard-manager to actually
    // write — this command resolves the absolute path and returns it,
    // and the plugin handles the OS clipboard. Keeps clipboard-permission
    // surface narrowly scoped to the frontend layer.
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = smriti::db::PhotoRepo::new(&db.conn);
    let photo = repo
        .get_by_id(args.photo_id)?
        .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
    let abs = lib.drive_root.join(&photo.file_path);
    Ok(CopiedPathDto {
        path: abs.display().to_string(),
    })
}

#[tauri::command]
pub async fn system_inference_provider() -> CommandResult<String> {
    Ok(smriti::ml::runtime::active_execution_provider().to_string())
}

#[derive(Debug, Serialize)]
pub struct UpdateStatusDto {
    pub current: String,
    pub latest: Option<String>,
    pub newer_available: bool,
    pub release_url: Option<String>,
    pub body: Option<String>,
}

#[tauri::command]
pub async fn system_updates_check() -> CommandResult<UpdateStatusDto> {
    let result = smriti::services::update_checker::check_for_updates()
        .await
        .map_err(|e| CommandError::Network {
            message: e.to_string(),
        })?;
    Ok(UpdateStatusDto {
        current: result.current.to_string(),
        latest: Some(result.latest.version.to_string()),
        newer_available: result.newer_available,
        release_url: Some(result.latest.html_url),
        body: Some(result.latest.body),
    })
}

#[derive(Debug, Deserialize)]
pub struct TestBridgeArgs {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct BridgeTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub gpu_name: String,
}

#[tauri::command]
pub fn system_test_gpu_bridge(args: TestBridgeArgs) -> CommandResult<BridgeTestResult> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e: reqwest::Error| CommandError::Network {
            message: e.to_string(),
        })?;
    let start = std::time::Instant::now();
    let resp = client
        .get(format!("{}/health", args.url))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .map_err(|e: reqwest::Error| CommandError::Network {
            message: e.to_string(),
        })?;
    let latency_ms = start.elapsed().as_millis() as u64;
    let body: serde_json::Value =
        resp.json()
            .map_err(|e: reqwest::Error| CommandError::Network {
                message: e.to_string(),
            })?;
    let gpu_name = body
        .get("provider")
        .and_then(|v: &serde_json::Value| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    Ok(BridgeTestResult {
        ok: true,
        latency_ms,
        gpu_name,
    })
}
