//! Handlers for the in-app update mechanism.
//!
//! Sister to `handlers/assets.rs` — same message-dispatch shape, same
//! task-spawning pattern. This module owns the async bridge between
//! the update-checker / self-replace services and the iced state.

use iced::Task;

use crate::components::toast::Toast;
use crate::services::install_method::InstallMethod;
use crate::services::self_replace::{self, InstallOutcome};
use crate::services::update_checker;

use super::super::messages::Message;
use super::super::state::PhotoVault;
use super::toasts;

/// Dispatched when the user clicks "Check for updates now" in
/// Settings or when the background subscription fires.
pub(crate) fn check_for_updates(app: &mut PhotoVault) -> Task<Message> {
    if app.update_check_in_progress {
        return Task::none();
    }
    app.update_check_in_progress = true;
    app.update_check_error = None;

    Task::perform(
        async {
            update_checker::check_for_updates()
                .await
                .map_err(|e| e.to_string())
        },
        Message::UpdateCheckResult,
    )
}

/// Async callback when the update check finishes.
pub(crate) fn update_check_result(
    app: &mut PhotoVault,
    result: Result<update_checker::UpdateStatus, String>,
) -> Task<Message> {
    app.update_check_in_progress = false;

    // Stamp the check-timestamp unconditionally (success or not) so
    // we don't retry every message tick on a persistent error.
    app.config.last_update_check_at_unix = Some(chrono::Utc::now().timestamp());
    let _ = app.config.save();

    match result {
        Ok(status) if status.newer_available => {
            // Suppress the banner if the user already dismissed THIS
            // exact tag in a previous session.
            let dismissed_for_this_tag = app
                .update_banner_dismissed_for_tag
                .as_deref()
                .map(|t| t == status.latest.tag_name.as_str())
                .unwrap_or(false);

            if dismissed_for_this_tag {
                app.pending_update = None;
                return Task::none();
            }

            tracing::info!(
                "Update available: {} (running {})",
                status.latest.tag_name,
                status.current
            );
            app.pending_update = Some(status.latest);
            Task::none()
        }
        Ok(_) => {
            // Up to date. No banner.
            app.pending_update = None;
            Task::none()
        }
        Err(msg) => {
            tracing::warn!("Update check failed: {}", msg);
            app.update_check_error = Some(msg.clone());
            // Only surface as a toast when the check was manual —
            // the background subscription failing silently is
            // expected on offline machines.
            toasts::show(app, Toast::error("Couldn't check for updates", msg))
        }
    }
}

/// User clicked "Download" in the update banner.
pub(crate) fn download_update(app: &mut PhotoVault) -> Task<Message> {
    let Some(release) = app.pending_update.clone() else {
        return Task::none();
    };
    if app.update_install_in_progress {
        return Task::none();
    }

    let method = app.install_method.clone();

    // Package-manager paths don't download anything — show a toast
    // with the upgrade command and short-circuit.
    if let InstallMethod::PackageManager { name, upgrade_cmd } = &method {
        return toasts::show(
            app,
            Toast::info(format!("Update via {}: {}", name, upgrade_cmd)),
        );
    }

    app.update_install_in_progress = true;
    app.update_download_progress =
        Some((0, release.assets.first().map(|a| a.size_bytes).unwrap_or(0)));

    Task::perform(
        async move {
            self_replace::install_update(method, release, None)
                .await
                .map_err(|e| e.to_string())
        },
        Message::UpdateReady,
    )
}

/// Async completion of the download + install pipeline.
pub(crate) fn update_ready(
    app: &mut PhotoVault,
    result: Result<InstallOutcome, String>,
) -> Task<Message> {
    app.update_install_in_progress = false;
    app.update_download_progress = None;

    match result {
        Ok(outcome) => {
            let toast = match &outcome {
                InstallOutcome::ReplacedRestartRequired => {
                    Toast::success("Update installed — relaunch PhotoVault to pick it up.")
                }
                InstallOutcome::InstallerLaunched { installer } => Toast::success(format!(
                    "Launched {}. PhotoVault will exit so the installer can complete.",
                    installer
                )),
                InstallOutcome::HandedOffToPackageManager { name, upgrade_cmd } => {
                    Toast::info(format!("Update via {}: {}", name, upgrade_cmd))
                }
                InstallOutcome::OpenedReleasePage { .. } => {
                    Toast::info("Opened release page — download the installer from your browser.")
                }
            };
            app.update_install_outcome = Some(outcome);
            app.pending_update = None;
            toasts::show(app, toast)
        }
        Err(msg) => {
            tracing::warn!("Update install failed: {}", msg);
            toasts::show(app, Toast::error("Update failed", msg))
        }
    }
}

/// User clicked "Later" on the banner.
pub(crate) fn dismiss_update_banner(app: &mut PhotoVault) -> Task<Message> {
    if let Some(release) = &app.pending_update {
        app.update_banner_dismissed_for_tag = Some(release.tag_name.clone());
    }
    app.pending_update = None;
    Task::none()
}

/// Settings toggle.
pub(crate) fn set_auto_update_check(app: &mut PhotoVault, enabled: bool) -> Task<Message> {
    app.auto_update_check_enabled = enabled;
    app.config.auto_update_check_enabled = enabled;
    let _ = app.config.save();

    if enabled {
        // Fire an immediate check so the user sees something happen.
        check_for_updates(app)
    } else {
        Task::none()
    }
}
