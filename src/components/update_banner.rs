//! Top-of-app banner shown when a newer release is available.
//!
//! Much smaller than the asset-install overlay: appears in-line with
//! the view rather than as a modal, because "update available" isn't
//! a blocking condition — the user can keep working and come back to
//! it. Patterns borrowed from `components/asset_prompt.rs`.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::install_method::InstallMethod;
use crate::services::update_checker::LatestRelease;
use crate::theme::colors;

/// Render the update banner when `pending_update` is Some. Returns
/// `None` when there's nothing to show, so the caller can skip
/// mounting the overlay.
pub fn banner<'a>(
    release: &'a LatestRelease,
    install_method: &'a InstallMethod,
    install_in_progress: bool,
    download_progress: Option<(u64, u64)>,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let accent_primary = p.accent_primary;
    let accent_hover = p.accent_hover;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let border_subtle = p.border_subtle;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;

    let version_label = format!("PhotoVault {} is available", release.tag_name);
    let body_preview = release
        .body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join(" · ");
    let preview_truncated: String = if body_preview.chars().count() > 180 {
        body_preview.chars().take(180).collect::<String>() + "..."
    } else {
        body_preview
    };

    // --- primary action label depends on install method -------------
    let (primary_label, primary_enabled) = match install_method {
        InstallMethod::PackageManager { name, .. } => {
            (format!("Update via {}", name), !install_in_progress)
        }
        InstallMethod::SourceBuild => ("Rebuild from source".to_string(), false),
        InstallMethod::MsiInstaller => ("Download installer".to_string(), !install_in_progress),
        InstallMethod::MacOsApp => ("Open .dmg".to_string(), !install_in_progress),
        InstallMethod::Portable => ("Download & install".to_string(), !install_in_progress),
        InstallMethod::Unknown => ("Open release page".to_string(), !install_in_progress),
    };
    let primary_label = if install_in_progress {
        match download_progress {
            Some((d, t)) if t > 0 => {
                format!("Downloading {}%", (d * 100 / t).min(100))
            }
            _ => "Downloading...".to_string(),
        }
    } else {
        primary_label
    };

    let mut primary_btn = button(text(primary_label).size(13).color(iced::Color::WHITE))
        .padding(Padding::from([8, 16]))
        .style(move |_t, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => accent_hover.into(),
                _ => accent_primary.into(),
            }),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    if primary_enabled {
        primary_btn = primary_btn.on_press(Message::DownloadUpdate);
    }

    let dismiss_btn = button(text("Later").size(13).color(text_secondary))
        .padding(Padding::from([8, 14]))
        .style(move |_t, status| button::Style {
            background: Some(match status {
                button::Status::Hovered => bg_hover.into(),
                _ => bg_elevated.into(),
            }),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::DismissUpdateBanner);

    let mut content_col = column![text(version_label).size(14).color(text_primary),].spacing(4);

    if !preview_truncated.is_empty() {
        content_col = content_col.push(text(preview_truncated).size(12).color(text_secondary));
    }

    let row_content = row![
        content_col.width(Length::Fill),
        Space::with_width(16),
        primary_btn,
        Space::with_width(8),
        dismiss_btn,
    ]
    .align_y(Alignment::Center);

    container(row_content)
        .padding(Padding::from([10, 16]))
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
