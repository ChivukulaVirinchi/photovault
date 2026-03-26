//! Scan progress indicator component
//!
//! Shows a progress display during directory scanning.
//! Design: calm, centered, with clear stats — no visual noise.

use iced::widget::{button, column, container, progress_bar, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::services::ScanProgress;
use crate::theme::colors::{Accent, Backgrounds, Border, Semantic, Text};

/// Format bytes as human-readable size
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Scan progress component
pub struct ScanProgressView;

impl ScanProgressView {
    /// Render the scan progress view
    pub fn view(progress: &ScanProgress) -> Element<'static, Message> {
        let title = text(if progress.is_complete {
            "Scan Complete"
        } else {
            "Scanning..."
        })
        .size(22)
        .color(Text::PRIMARY);

        // Progress stats
        let stats = row![
            Self::stat_item("Found", &progress.files_found.to_string()),
            Space::with_width(40),
            Self::stat_item("Processed", &progress.files_processed.to_string()),
            Space::with_width(40),
            Self::stat_item("Size", &format_bytes(progress.bytes_processed)),
        ]
        .align_y(Alignment::Center);

        // Progress bar
        let progress_value = if progress.files_found > 0 {
            progress.files_processed as f32 / progress.files_found as f32
        } else {
            0.0
        };

        let bar = progress_bar(0.0..=1.0, progress_value)
            .width(Length::Fixed(380.0))
            .height(Length::Fixed(4.0));

        // Current file/directory
        let current = if !progress.is_complete {
            let display = if !progress.current_file.is_empty() {
                progress.current_file.clone()
            } else if !progress.current_directory.is_empty() {
                progress.current_directory.clone()
            } else {
                "Preparing...".to_string()
            };
            text(display).size(11).color(Text::TERTIARY)
        } else {
            text(format!(
                "Completed in {:.1}s",
                progress.elapsed_seconds
            ))
            .size(12)
            .color(Semantic::SUCCESS)
        };

        // Errors summary
        let errors = if !progress.errors.is_empty() {
            let error_count = progress.errors.len();
            Some(
                text(format!("{} errors encountered", error_count))
                    .size(11)
                    .color(Semantic::WARNING),
            )
        } else {
            None
        };

        // Action button
        let action_button = if progress.is_complete {
            button(text("Continue").size(13).color(Backgrounds::PRIMARY))
                .padding(Padding::from([10, 24]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::HOVER.into()),
                        _ => Some(Accent::PRIMARY.into()),
                    };
                    button::Style {
                        background,
                        text_color: Backgrounds::PRIMARY,
                        border: iced::Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::ScanComplete)
        } else {
            button(text("Cancel").size(13).color(Text::SECONDARY))
                .padding(Padding::from([10, 24]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => None,
                    };
                    button::Style {
                        background,
                        text_color: Text::SECONDARY,
                        border: iced::Border {
                            color: Border::VISIBLE,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::CancelScan)
        };

        // Elapsed time
        let elapsed = if !progress.is_complete && progress.elapsed_seconds > 0.0 {
            Some(
                text(format!("{:.0}s", progress.elapsed_seconds))
                    .size(11)
                    .color(Text::TERTIARY),
            )
        } else {
            None
        };

        let mut content = column![
            title,
            Space::with_height(28),
            stats,
            Space::with_height(20),
            bar,
            Space::with_height(8),
            current,
        ]
        .spacing(0)
        .align_x(Alignment::Center);

        if let Some(elapsed_text) = elapsed {
            content = content.push(Space::with_height(4));
            content = content.push(elapsed_text);
        }

        if let Some(err_text) = errors {
            content = content.push(Space::with_height(8));
            content = content.push(err_text);
        }

        content = content.push(Space::with_height(28));
        content = content.push(action_button);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single stat item
    fn stat_item<'a>(label: &str, value: &str) -> Element<'a, Message> {
        column![
            text(value.to_string()).size(24).color(Text::PRIMARY),
            text(label.to_string()).size(11).color(Text::SECONDARY),
        ]
        .align_x(Alignment::Center)
        .into()
    }
}
