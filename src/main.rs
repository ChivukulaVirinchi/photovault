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

fn instance_lock_file() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("photovault")
        .join("app.lock")
}

fn try_acquire_single_instance_lock() -> Option<std::fs::File> {
    let lock_path = instance_lock_file();
    if let Some(parent) = lock_path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return None;
        }
    }

    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .ok()?;

    if fs2::FileExt::try_lock_exclusive(&file).is_ok() {
        Some(file)
    } else {
        None
    }
}

fn main() -> iced::Result {
    let _instance_lock = match try_acquire_single_instance_lock() {
        Some(file) => file,
        None => {
            eprintln!("PhotoVault is already running; refusing to start a second instance.");
            return Ok(());
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
