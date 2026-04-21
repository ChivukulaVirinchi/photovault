//! PhotoVault - Offline Photo Library Manager
//!
//! A desktop application for organizing and browsing photos from external drives.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod bootstrap;
mod components;
mod config;
mod db;
mod ml;
mod models;
mod search;
mod services;
mod theme;
mod utils;
mod views;

use iced::Size;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};

fn instance_lock_file() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("photovault")
        .join("app.lock")
}

enum LockOutcome {
    Acquired(std::fs::File),
    HeldBy(Option<u32>),
    OpenFailed(std::io::Error),
}

fn try_acquire_single_instance_lock() -> LockOutcome {
    let lock_path = instance_lock_file();
    if let Some(parent) = lock_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return LockOutcome::OpenFailed(e);
        }
    }

    let mut file = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => return LockOutcome::OpenFailed(e),
    };

    // OS file locks (flock / LockFileEx) auto-release on process termination,
    // so the lockfile existing on disk is not itself proof another instance is
    // running. The real signal is whether try_lock_exclusive succeeds.
    if fs2::FileExt::try_lock_exclusive(&file).is_ok() {
        // Stamp our PID into the file so a future failed-lock attempt can
        // report which process is holding it. Best-effort; ignore errors.
        let _ = file.set_len(0);
        let _ = file.seek(SeekFrom::Start(0));
        let _ = writeln!(file, "{}", std::process::id());
        let _ = file.flush();
        LockOutcome::Acquired(file)
    } else {
        let mut existing = String::new();
        let _ = file.seek(SeekFrom::Start(0));
        let _ = file.read_to_string(&mut existing);
        let pid = existing.trim().parse::<u32>().ok();
        LockOutcome::HeldBy(pid)
    }
}

fn main() -> iced::Result {
    let _instance_lock = match try_acquire_single_instance_lock() {
        LockOutcome::Acquired(file) => file,
        LockOutcome::HeldBy(pid) => {
            match pid {
                Some(p) => eprintln!(
                    "PhotoVault is already running (pid {}); refusing to start a second instance.",
                    p
                ),
                None => {
                    eprintln!("PhotoVault is already running; refusing to start a second instance.")
                }
            }
            return Ok(());
        }
        LockOutcome::OpenFailed(e) => {
            // Don't lock the user out of the app over a transient lockfile
            // issue (permission denied, missing config dir, full disk).
            // Log it, then continue without single-instance protection.
            eprintln!(
                "Could not acquire single-instance lock at {} ({}). Continuing without it.",
                instance_lock_file().display(),
                e
            );
            // Sentinel handle so the binding's lifetime matches the Acquired arm.
            let p = std::env::temp_dir().join(".photovault-noop.lock");
            std::fs::File::create(&p).expect("failed to create fallback lock file")
        }
    };

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("photovault=debug,iced=warn")
        .init();

    tracing::info!(
        "Starting PhotoVault... pid={} args={:?}",
        std::process::id(),
        std::env::args().collect::<Vec<_>>()
    );

    if !crate::bootstrap::has_face_models() {
        tracing::warn!(
            "Face models missing. Expected {} and {}. Run {}",
            crate::bootstrap::detector_model_path().display(),
            crate::bootstrap::embedder_model_path().display(),
            crate::bootstrap::SETUP_ASSETS_HINT
        );
    }

    if !crate::bootstrap::onnx_runtime_exists() {
        tracing::warn!(
            "ONNX Runtime missing under libs/onnxruntime. Run {}",
            crate::bootstrap::SETUP_ASSETS_HINT
        );
    }

    crate::bootstrap::ensure_geonames_db();

    // Run the application
    iced::application(
        app::PhotoVault::title,
        app::PhotoVault::update,
        app::PhotoVault::view,
    )
    .theme(app::PhotoVault::theme)
    .subscription(app::PhotoVault::subscription)
    .window_size(Size::new(1200.0, 800.0))
    .antialiasing(true)
    .run_with(app::PhotoVault::new)
}
