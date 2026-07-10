//! System: asset health, app version (read-only).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::dto::{AppVersionDto, AssetHealthDto, AssetInventoryDto, AssetItemDto, JobIdDto};
use crate::events::{JobProgress, EV_ASSETS_COMPLETE, EV_ASSETS_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn system_asset_health() -> CommandResult<AssetHealthDto> {
    let h = tauri::async_runtime::spawn_blocking(smriti::bootstrap::asset_health)
        .await
        .map_err(|e| CommandError::Internal {
            message: format!("asset health worker failed: {e}"),
        })?;
    Ok(h.into())
}

#[tauri::command]
pub async fn system_app_version() -> CommandResult<AppVersionDto> {
    Ok(AppVersionDto {
        version: smriti::services::update_checker::current_version().to_string(),
    })
}

#[tauri::command]
pub async fn system_assets_inventory() -> CommandResult<AssetInventoryDto> {
    tauri::async_runtime::spawn_blocking(build_asset_inventory)
        .await
        .map_err(|e| CommandError::Internal {
            message: format!("asset inventory worker failed: {e}"),
        })
}

#[tauri::command]
pub async fn system_install_assets(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<JobIdDto> {
    if state
        .jobs
        .lock()
        .await
        .has_any_of_kind(JobKind::AssetInstall)
    {
        return Err(CommandError::Conflict {
            reason: "asset installation is already in progress".into(),
        });
    }

    let job = jobs::start_job(&state, JobKind::AssetInstall).await?;
    let job_id = job.id.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        emit(
            &app_clone,
            EV_ASSETS_PROGRESS,
            JobProgress {
                job_id: job_id_clone.clone(),
                stage: "download".into(),
                processed: 0,
                total: None,
                elapsed_ms: 0,
                eta_ms: None,
                message: Some("Downloading Smriti assets...".into()),
            },
        );

        match smriti::bootstrap::install_asset_pack().await {
            Ok(message) => emit(
                &app_clone,
                EV_ASSETS_COMPLETE,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "complete".into(),
                    processed: 1,
                    total: Some(1),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(message),
                },
            ),
            Err(err) => emit(
                &app_clone,
                EV_ASSETS_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "error".into(),
                    processed: 0,
                    total: Some(1),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(format!("Asset install failed: {err}")),
                },
            ),
        }

        let st: tauri::State<AppState> = app_clone.state();
        jobs::finish_job(&st, &job_id_clone).await;
    });

    Ok(JobIdDto { job_id })
}

fn build_asset_inventory() -> AssetInventoryDto {
    let install_root = smriti::bootstrap::default_asset_install_dir();
    let roots = smriti::bootstrap::asset_roots()
        .into_iter()
        .map(display_path)
        .collect::<Vec<_>>();
    let detector = smriti::bootstrap::detector_model_path();
    let active_embedder = smriti::bootstrap::embedder_model_path();
    let mut assets = Vec::new();

    assets.push(asset_file(
        "runtime.onnx",
        "ONNX Runtime",
        "runtime",
        smriti::bootstrap::onnx_runtime_path()
            .or_else(|| Some(smriti::bootstrap::onnx_runtime_install_path())),
        true,
        true,
        "Required for local face detection and future local models.",
    ));
    assets.push(asset_file(
        "face.detector",
        "Face detector",
        "model",
        Some(detector),
        true,
        true,
        "Finds face boxes before recognition.",
    ));
    assets.push(asset_file(
        "face.embedder.active",
        "Face recognizer",
        "model",
        Some(active_embedder),
        true,
        true,
        "AdaFace face embedding model.",
    ));

    assets.push(asset_file(
        "geonames.db",
        "GeoNames database",
        "data",
        Some(smriti::db::geonames::geonames_db_path()),
        true,
        true,
        "Offline city and country lookup for GPS photos.",
    ));
    assets.push(planned_asset(
        "ocr.model",
        "OCR model",
        "model",
        "Not installed in this build.",
    ));
    assets.push(asset_file(
        "vision.semantic.visual",
        "Semantic visual encoder",
        "model",
        smriti::bootstrap::asset_roots()
            .into_iter()
            .map(|root| {
                root.join("models")
                    .join("semantic")
                    .join("vit-b-32-siglip2-256-webli")
                    .join("visual")
                    .join("model.onnx")
            })
            .find(|p| p.exists())
            .or_else(|| {
                Some(
                    install_root
                        .join("models")
                        .join("semantic")
                        .join("vit-b-32-siglip2-256-webli")
                        .join("visual")
                        .join("model.onnx"),
                )
            }),
        false,
        true,
        "Optional local visual search model.",
    ));
    assets.push(asset_file(
        "vision.semantic.text",
        "Semantic text encoder",
        "model",
        smriti::bootstrap::asset_roots()
            .into_iter()
            .map(|root| {
                root.join("models")
                    .join("semantic")
                    .join("vit-b-32-siglip2-256-webli")
                    .join("textual")
                    .join("model.onnx")
            })
            .find(|p| p.exists())
            .or_else(|| {
                Some(
                    install_root
                        .join("models")
                        .join("semantic")
                        .join("vit-b-32-siglip2-256-webli")
                        .join("textual")
                        .join("model.onnx"),
                )
            }),
        false,
        true,
        "Optional local text-to-image search model.",
    ));

    let total_size_bytes = assets.iter().filter_map(|a| a.size_bytes).sum();
    AssetInventoryDto {
        install_root: display_path(install_root),
        roots,
        total_size_bytes,
        assets,
    }
}

fn asset_file(
    id: &str,
    label: &str,
    kind: &str,
    path: Option<PathBuf>,
    required: bool,
    active: bool,
    note: &str,
) -> AssetItemDto {
    let exists = path.as_deref().is_some_and(Path::exists);
    let status = if exists && active {
        "active"
    } else if exists {
        "extra"
    } else {
        "missing"
    };
    AssetItemDto {
        id: id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        status: status.to_string(),
        required,
        active: exists && active,
        installable: false,
        removable: false,
        size_bytes: path.as_deref().and_then(file_size),
        path: path.map(display_path),
        note: Some(note.to_string()),
    }
}

fn planned_asset(id: &str, label: &str, kind: &str, note: &str) -> AssetItemDto {
    AssetItemDto {
        id: id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        status: "planned".to_string(),
        required: false,
        active: false,
        installable: false,
        removable: false,
        size_bytes: None,
        path: None,
        note: Some(note.to_string()),
    }
}

fn file_size(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
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
    let abs = {
        let db = lib.db.lock().await;
        let repo = smriti::db::PhotoRepo::new(&db.conn);
        let photo = repo
            .get_by_id(args.photo_id)?
            .ok_or_else(|| CommandError::not_found("photo", args.photo_id))?;
        smriti::services::path_util::safe_existing_path_under_root(
            &lib.drive_root,
            &photo.file_path,
        )
        .map_err(|e| CommandError::Validation {
            field: "photo.file_path".into(),
            reason: e,
        })?
    };
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
    // We tried two command-line approaches and both had failure modes:
    //   1. `explorer.exe /select,<path>` via Command::arg — Rust quotes
    //      the whole token, Explorer falls back to opening Documents.
    //   2. raw_arg with `/select,"<path>"` — better, but on paths with
    //      certain characters Explorer still opens the parent folder
    //      WITHOUT selecting the file.
    // The reliable path is `SHOpenFolderAndSelectItems`. It takes a
    // shell PIDL (item identifier list), which the shell parses with
    // full UNC / extended-length / long-path support and selects the
    // item atomically — no string-parsing quirks, no fallback paths.
    use std::process::Command;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

    fn shell_path(path: &std::path::Path) -> String {
        let raw = path.display().to_string().replace('/', "\\");
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            raw
        }
    }

    fn select_with_explorer(normalized: &str) -> std::io::Result<()> {
        use std::os::windows::process::CommandExt;

        Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{normalized}\""))
            .spawn()?;
        Ok(())
    }

    fn open_parent(path: &std::path::Path) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or(path);
        Command::new("explorer.exe").arg(parent).spawn()?;
        Ok(())
    }

    let normalized = shell_path(path);
    let mut wide: Vec<u16> = normalized.encode_utf16().collect();
    wide.push(0);

    // SAFETY: CoInitializeEx + COM calls are safe so long as we pair
    // initialise/uninitialise on the same thread. The Tauri command is
    // already on a tokio worker thread; the COM apartment is per-thread,
    // so re-initialising here is harmless (returns RPC_E_CHANGED_MODE
    // or S_FALSE if already inited, both fine). Errors from CoInit are
    // non-fatal — the shell APIs work without it on most systems.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
        if pidl.is_null() {
            CoUninitialize();
            return select_with_explorer(&normalized).or_else(|_| open_parent(path));
        }
        let result = SHOpenFolderAndSelectItems(pidl, None, 0);
        ILFree(Some(pidl));
        CoUninitialize();
        if result.is_err() {
            return select_with_explorer(&normalized).or_else(|_| open_parent(path));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct CopiedPathDto {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct SystemOpenPathArgs {
    pub path: String,
}

#[tauri::command]
pub async fn system_open_path(args: SystemOpenPathArgs) -> CommandResult<()> {
    let path = PathBuf::from(args.path);
    if !path.is_dir() {
        return Err(CommandError::Validation {
            field: "path".into(),
            reason: "path must be an existing folder".into(),
        });
    }
    open::that_detached(path)?;
    Ok(())
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
    let abs = smriti::services::path_util::safe_existing_path_under_root(
        &lib.drive_root,
        &photo.file_path,
    )
    .map_err(|e| CommandError::Validation {
        field: "photo.file_path".into(),
        reason: e,
    })?;
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
    pub model: Option<String>,
}

#[tauri::command]
pub async fn system_test_gpu_bridge(args: TestBridgeArgs) -> CommandResult<BridgeTestResult> {
    if !smriti::config::is_allowed_gpu_bridge_url(&args.url) {
        return Err(CommandError::Validation {
            field: "url".into(),
            reason: "must use https:// or local http://".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || test_gpu_bridge_blocking(args.url))
        .await
        .map_err(|e| CommandError::Internal {
            message: format!("GPU bridge test worker failed: {e}"),
        })?
}

fn test_gpu_bridge_blocking(url: String) -> CommandResult<BridgeTestResult> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e: reqwest::Error| CommandError::Network {
            message: e.to_string(),
        })?;
    let start = std::time::Instant::now();
    let resp = client
        .get(format!("{}/health", url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .map_err(|e: reqwest::Error| CommandError::Network {
            message: e.to_string(),
        })?;
    let latency_ms = start.elapsed().as_millis() as u64;
    if !resp.status().is_success() {
        return Err(CommandError::Network {
            message: format!("bridge health returned {}", resp.status()),
        });
    }
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
    let model = body
        .get("model")
        .and_then(|v: &serde_json::Value| v.as_str())
        .map(|s| s.to_string());
    let expected = smriti::config::AppConfig::load().face_embedder_model;
    let model_ok = model
        .as_deref()
        .is_some_and(|served| bridge_models_match(served, &expected));
    if !model_ok {
        return Err(CommandError::Validation {
            field: "model".into(),
            reason: format!(
                "bridge serves {:?}, but Smriti expects {}",
                model.as_deref().unwrap_or("unknown"),
                expected
            ),
        });
    }
    Ok(BridgeTestResult {
        ok: true,
        latency_ms,
        gpu_name,
        model,
    })
}

fn bridge_models_match(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim()
            .strip_suffix(".onnx")
            .unwrap_or(s.trim())
            .to_ascii_lowercase()
    }
    !a.is_empty() && !b.is_empty() && norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::bridge_models_match;

    #[test]
    fn bridge_model_match_ignores_case_and_onnx_suffix() {
        assert!(bridge_models_match("AdaFace.onnx", "adaface"));
        assert!(bridge_models_match(" adaface ", "ADAFACE.onnx"));
        assert!(!bridge_models_match("", "adaface"));
        assert!(!bridge_models_match("other", "adaface"));
    }
}
