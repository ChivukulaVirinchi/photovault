//! Scan progress indicator component
//!
//! Shows a progress display during directory scanning.

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
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
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
        .size(24)
        .color(Text::PRIMARY);

        // Progress stats
        let stats = row![
            Self::stat_item("Files Found", &progress.files_found.to_string()),
            Space::with_width(32),
            Self::stat_item("Processed", &progress.files_processed.to_string()),
            Space::with_width(32),
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
            .width(Length::Fixed(400.0))
            .height(Length::Fixed(8.0));

        // Current file/directory
        let current = if !progress.is_complete {
            let display = if !progress.current_file.is_empty() {
                progress.current_file.clone()
            } else if !progress.current_directory.is_empty() {
                format!("Scanning: {}", progress.current_directory)
            } else {
                "Preparing...".to_string()
            };
            text(display).size(12).color(Text::TERTIARY)
        } else {
            text(format!(
                "Completed in {:.1} seconds",
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
                    .size(12)
                    .color(Semantic::WARNING),
            )
        } else {
            None
        };

        // Cancel/Done button
        let action_button = if progress.is_complete {
            button(text("Continue").size(14).color(Text::PRIMARY))
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::HOVER.into()),
                        _ => Some(Accent::PRIMARY.into()),
                    };
                    button::Style {
                        background,
                        text_color: Backgrounds::PRIMARY,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::ScanComplete)
        } else {
            button(text("Cancel").size(14).color(Text::SECONDARY))
                .padding(Padding::from([10, 20]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    };
                    button::Style {
                        background,
                        text_color: Text::SECONDARY,
                        border: iced::Border {
                            color: Border::VISIBLE,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::CancelScan)
        };

        // Elapsed time display
        let elapsed = if !progress.is_complete && progress.elapsed_seconds > 0.0 {
            Some(
                text(format!("{:.0}s elapsed", progress.elapsed_seconds))
                    .size(12)
                    .color(Text::TERTIARY),
            )
        } else {
            None
        };

        // Assemble the layout
        let mut content = column![
            title,
            Space::with_height(24),
            stats,
            Space::with_height(16),
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

        content = content.push(Space::with_height(24));
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
            text(value.to_string()).size(28).color(Text::PRIMARY),
            text(label.to_string()).size(12).color(Text::SECONDARY),
        ]
        .align_x(Alignment::Center)
        .into()
    }
}
