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
/// Both branches set `current_dir("/")` because on Fedora 40+ / modern
/// GNOME the default `xdg-open` flow can be wrapped in
/// `systemd-run --scope --user`, which inherits Tauri's cwd. If that
/// inherited cwd is anything systemd considers non-absolute the launch
/// fails with `WorkingDirectory= expects an absolute path or '~'`.
/// Pinning cwd to `/` defuses the entire class of issues.
#[cfg(target_os = "linux")]
fn select_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    use std::process::Command;
    // Resolve to absolute — `file://` URIs need absolute paths and the
    // canonicalised form survives downstream pickup.
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // org.freedesktop.FileManager1.ShowItems is the canonical interface
    // implemented by Nautilus, Nemo, Dolphin, Caja, Thunar, and others.
    let uri = format!("file://{}", abs.display());
    let dbus_status = Command::new("dbus-send")
        .current_dir("/")
        .args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{}", uri),
            "string:",
        ])
        .status();
    if matches!(dbus_status, Ok(s) if s.success()) {
        return Ok(());
    }
    // Fallback: open the parent directory in the default file manager.
    // Using `xdg-open` directly (instead of the `open` crate) so we
    // can pin cwd; the `open` crate doesn't expose that knob.
    let parent = abs.parent().unwrap_or(&abs);
    let status = Command::new("xdg-open")
        .current_dir("/")
        .arg(parent)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("xdg-open exited non-zero"))
    }
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
