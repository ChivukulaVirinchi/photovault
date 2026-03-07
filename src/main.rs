//! PhotoVault - Offline Photo Library Manager
//!
//! A desktop application for organizing and browsing photos from external drives.

mod app;
mod components;
mod db;
mod ml;
mod models;
mod services;
mod theme;
mod views;

use iced::Size;

fn main() -> iced::Result {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("photovault=debug,iced=warn")
        .init();

    tracing::info!("Starting PhotoVault...");

    // Run the application
    iced::application(
        app::PhotoVault::title,
        app::PhotoVault::update,
        app::PhotoVault::view,
    )
    .subscription(app::PhotoVault::subscription)
    .window_size(Size::new(1200.0, 800.0))
    .antialiasing(true)
    .run_with(app::PhotoVault::new)
}
