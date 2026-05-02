//! Main people grid view — status bar, merge bar, and grid of person cards.

use iced::widget::{button, column, container, row, scrollable, text, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::icon::{icon, Lucide};
use crate::config::AppTheme;
use crate::db::FaceClusterRecord;
use crate::services::{FaceProcessingProgress, FaceProcessingStage};
use crate::theme::colors;

use super::cards;

/// Render with clusters and processing state
pub fn view_with_clusters(
    clusters: &[FaceClusterRecord],
    editing_cluster: Option<i64>,
    edit_name: &str,
    processing_active: bool,
    progress: Option<&FaceProcessingProgress>,
    processing_error: Option<&str>,
    merge_mode_active: bool,
    merge_selected: &[i64],
    highlighted_cluster_index: Option<usize>,
    ml_available: bool,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let accent_primary = p.accent_primary;
    let accent_hover = p.accent_hover;
    let accent_muted = p.accent_muted;
    let bg_primary = p.bg_primary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let border_subtle = p.border_subtle;
    let semantic_danger = p.semantic_danger;

    let title = text("People").size(28).color(text_primary);

    // Processing status bar with progress bar and a single elapsed +
    // ETA timer. We deliberately don't surface stage names — the user
    // doesn't care whether we're detecting vs clustering, only how
    // long until they get their faces.
    let status_bar: Element<'static, Message> = if processing_active {
        let (progress_text, pct) = if let Some(p) = progress {
            let fraction = if p.total > 0 {
                p.processed as f32 / p.total as f32
            } else {
                0.0
            };
            let elapsed_str = format_duration(p.elapsed_secs);
            let progress_text = match p.stage {
                FaceProcessingStage::Finishing => {
                    format!("Almost done — wrapping up… · {} elapsed", elapsed_str)
                }
                FaceProcessingStage::Done => {
                    format!("Finishing up · {} elapsed", elapsed_str)
                }
                FaceProcessingStage::Detecting => {
                    let eta = if p.processed > 0 && p.elapsed_secs > 0.5 && fraction < 1.0 {
                        let remaining = p.elapsed_secs / fraction as f64 * (1.0 - fraction as f64);
                        format!(" · ~{} left", format_duration(remaining))
                    } else {
                        String::new()
                    };
                    format!(
                        "Finding faces — {} of {} photos · {} elapsed{}",
                        p.processed, p.total, elapsed_str, eta
                    )
                }
            };
            (progress_text, fraction)
        } else {
            ("Starting face processing…".to_string(), 0.0)
        };

        let cancel_btn = button(text("Cancel").size(12).color(text_primary))
            .padding(Padding::from([4, 12]))
            .style(move |_theme: &iced::Theme, status: button::Status| {
                let background = match status {
                    button::Status::Hovered => Some(
                        iced::Color {
                            a: 0.3,
                            ..semantic_danger
                        }
                        .into(),
                    ),
                    _ => Some(
                        iced::Color {
                            a: 0.15,
                            ..semantic_danger
                        }
                        .into(),
                    ),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::CancelFaceProcessing);

        // Visual progress bar
        let bar_bg = border_subtle;
        let bar_fill = accent_primary;
        let bar_width = 200.0_f32;
        let filled = (bar_width * pct).clamp(0.0, bar_width);
        let progress_bar = container(
            container(Space::new(Length::Fixed(filled), Length::Fixed(6.0))).style(
                move |_t: &iced::Theme| container::Style {
                    background: Some(bar_fill.into()),
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
        )
        .width(Length::Fixed(bar_width))
        .height(Length::Fixed(6.0))
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bar_bg.into()),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        container(
            column![
                row![
                    text(progress_text).size(13).color(accent_primary),
                    Space::with_width(Length::Fill),
                    cancel_btn,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                Space::with_height(6),
                progress_bar,
            ]
            .spacing(0),
        )
        .padding(Padding::from([8, 16]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(accent_muted.into()),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else if !ml_available {
        // Face models not installed — show setup hint
        container(
            text(format!(
                "Face models not installed. Run {} to enable face detection.",
                crate::bootstrap::SETUP_ASSETS_HINT
            ))
            .size(13)
            .color(text_tertiary),
        )
        .padding(Padding::from([8, 16]))
        .into()
    } else {
        // "Detect Faces" button + "Merge" toggle button
        let process_btn = button(text("Detect Faces").size(13).color(text_primary))
            .padding(Padding::from([8, 16]))
            .style(move |_theme: &iced::Theme, status: button::Status| {
                let background = match status {
                    button::Status::Hovered => Some(accent_hover.into()),
                    _ => Some(accent_primary.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::ProcessFaces);

        let merge_label = if merge_mode_active {
            "Cancel Merge"
        } else {
            "Merge"
        };
        let merge_btn = button(text(merge_label).size(13).color(text_primary))
            .padding(Padding::from([8, 16]))
            .style(move |_theme: &iced::Theme, status: button::Status| {
                let background = match status {
                    button::Status::Hovered => Some(bg_hover.into()),
                    _ => {
                        if merge_mode_active {
                            Some(accent_muted.into())
                        } else {
                            Some(bg_elevated.into())
                        }
                    }
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: border_subtle,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::ToggleMergeMode);

        // Only show merge button when there are clusters to merge
        if !clusters.is_empty() {
            row![process_btn, Space::with_width(8), merge_btn]
                .spacing(4)
                .align_y(Alignment::Center)
                .into()
        } else {
            process_btn.into()
        }
    };

    if clusters.is_empty() && !processing_active {
        return empty_view_with_button(title, status_bar, theme);
    }

    let subtitle = text(format!(
        "{} {} recognized",
        clusters.len(),
        if clusters.len() == 1 {
            "person"
        } else {
            "people"
        }
    ))
    .size(14)
    .color(text_secondary);

    // Merge action bar (shown when merge mode is active and >=2 selected)
    let merge_action: Option<Element<'static, Message>> = if merge_mode_active {
        let selected_count = merge_selected.len();
        if selected_count >= 2 {
            let merge_execute_btn = button(
                text(format!("Merge Selected ({})", selected_count))
                    .size(13)
                    .color(text_primary),
            )
            .padding(Padding::from([8, 16]))
            .style(move |_theme: &iced::Theme, status: button::Status| {
                let background = match status {
                    button::Status::Hovered => Some(accent_hover.into()),
                    _ => Some(accent_primary.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::MergeSelectedClusters);

            Some(
                container(
                    row![
                        text(format!(
                            "Select people to merge ({} selected)",
                            selected_count
                        ))
                        .size(13)
                        .color(accent_primary),
                        Space::with_width(Length::Fill),
                        merge_execute_btn,
                    ]
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([8, 16]))
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(accent_muted.into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into(),
            )
        } else {
            Some(
                container(
                    text("Select 2 or more people to merge them")
                        .size(13)
                        .color(text_secondary),
                )
                .padding(Padding::from([8, 16]))
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(accent_muted.into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into(),
            )
        }
    } else {
        None
    };

    // Grid of people cards
    let merge_selected_owned: Vec<i64> = merge_selected.to_vec();
    let mut grid_rows: Vec<Element<'static, Message>> = Vec::new();
    let mut current_row: Vec<Element<'static, Message>> = Vec::new();
    let columns = 4;

    for (index, cluster) in clusters.iter().enumerate() {
        let is_editing = editing_cluster == Some(cluster.id);
        let is_selected = merge_selected_owned.contains(&cluster.id);
        let is_highlighted = highlighted_cluster_index == Some(index);
        let card = if merge_mode_active {
            cards::person_card_merge(cluster, is_selected, is_highlighted, theme)
        } else {
            cards::person_card(cluster, is_editing, edit_name, is_highlighted, theme)
        };
        current_row.push(card);

        if current_row.len() >= columns {
            grid_rows.push(Row::with_children(current_row).spacing(16).into());
            current_row = Vec::new();
        }
    }

    // Add remaining cards in the last row
    if !current_row.is_empty() {
        grid_rows.push(Row::with_children(current_row).spacing(16).into());
    }

    let grid = Column::with_children(grid_rows).spacing(16);

    let mut content_children: Vec<Element<'static, Message>> = vec![
        title.into(),
        Space::with_height(8).into(),
        row![subtitle, Space::with_width(Length::Fill), status_bar,]
            .align_y(Alignment::Center)
            .into(),
    ];

    if let Some(merge_bar) = merge_action {
        content_children.push(Space::with_height(12).into());
        content_children.push(merge_bar);
    }

    if let Some(err) = processing_error {
        content_children.push(Space::with_height(12).into());
        content_children.push(
            container(text(err.to_string()).size(12).color(semantic_danger))
                .padding(Padding::from([8, 12]))
                .style(move |_theme| container::Style {
                    background: Some(bg_elevated.into()),
                    border: iced::Border {
                        color: semantic_danger,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                })
                .into(),
        );
    }

    content_children.push(Space::with_height(24).into());
    content_children.push(
        scrollable(grid)
            .id(iced::widget::scrollable::Id::new("people"))
            .into(),
    );

    let content = Column::with_children(content_children).padding(32);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        })
        .into()
}

/// Empty state with the title and process button
fn empty_view_with_button(
    title: iced::widget::Text<'static>,
    status_bar: Element<'static, Message>,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let bg_primary = p.bg_primary;
    let bg_primary_for_bg = bg_primary;
    let accent_primary = p.accent_primary;
    let accent_hover = p.accent_hover;

    let action_btn = iced::widget::button(
        iced::widget::text("Detect faces")
            .size(13)
            .color(bg_primary_for_bg),
    )
    .padding(Padding::from([8, 16]))
    .style(move |_t, status| iced::widget::button::Style {
        background: Some(match status {
            iced::widget::button::Status::Hovered => accent_hover.into(),
            _ => accent_primary.into(),
        }),
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Message::ProcessFaces);

    let empty_card = container(
        column![
            icon(Lucide::Person, 36, text_tertiary),
            Space::with_height(12),
            text("No people detected yet")
                .size(16)
                .color(text_secondary),
            Space::with_height(8),
            text("Run face processing to find people in your photos.")
                .size(13)
                .color(text_tertiary),
            Space::with_height(16),
            action_btn,
        ]
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::from([64, 32]))
    .center_x(Length::Fill);

    let content = column![title, Space::with_height(8), status_bar, empty_card,]
        .align_x(Alignment::Start)
        .padding(32);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        })
        .into()
}

/// Format a wallclock duration in seconds as a short human string:
/// "12 s", "3 m 04 s", "1 h 02 m". Used for both elapsed and ETA so
/// they read consistently in the status bar.
fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    if total < 60 {
        format!("{} s", total)
    } else if total < 3600 {
        let m = total / 60;
        let s = total % 60;
        format!("{} m {:02} s", m, s)
    } else {
        let h = total / 3600;
        let m = (total % 3600) / 60;
        format!("{} h {:02} m", h, m)
    }
}
