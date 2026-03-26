//! People view - face clusters display
//!
//! Shows detected face clusters in a grid. Each card represents a person
//! with their face count. Names are editable inline.
//! Also includes the cluster detail view showing photos for a specific person.

use iced::widget::{
    button, column, container, row, scrollable, text, text_input, Column, Row, Space,
};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::photo_grid::photo_grid_simple;
use crate::db::FaceClusterRecord;
use crate::models::Photo;
use crate::services::FaceProcessingProgress;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// People view component
pub struct PeopleView;

impl PeopleView {
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
        ml_available: bool,
    ) -> Element<'static, Message> {
        let title = text("People").size(28).color(Text::PRIMARY);

        // Processing status bar
        let status_bar: Element<'static, Message> = if processing_active {
            let progress_text = if let Some(p) = progress {
                format!(
                    "Processing faces... {}/{} photos ({} faces found)",
                    p.processed, p.total, p.faces_found
                )
            } else {
                "Starting face processing...".to_string()
            };

            let cancel_btn = button(text("Cancel").size(12).color(Text::PRIMARY))
                .padding(Padding::from([4, 12]))
                .style(|_theme: &iced::Theme, status: button::Status| {
                    let background = match status {
                        button::Status::Hovered => Some(iced::Color::from_rgb(0.6, 0.2, 0.2).into()),
                        _ => Some(iced::Color::from_rgb(0.4, 0.15, 0.15).into()),
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

            container(
                row![
                    text(progress_text).size(13).color(Accent::PRIMARY),
                    Space::with_width(Length::Fill),
                    cancel_btn,
                ]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding(Padding::from([8, 16]))
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Accent::MUTED.into()),
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
                text("Face models not installed. Run setup_assets script to enable face detection.")
                    .size(13)
                    .color(Text::TERTIARY),
            )
            .padding(Padding::from([8, 16]))
            .into()
        } else {
            // "Detect Faces" button + "Merge" toggle button
            let process_btn = button(text("Detect Faces").size(13).color(Text::PRIMARY))
                .padding(Padding::from([8, 16]))
                .style(|_theme: &iced::Theme, status: button::Status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::HOVER.into()),
                        _ => Some(Accent::PRIMARY.into()),
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
            let merge_btn = button(text(merge_label).size(13).color(Text::PRIMARY))
                .padding(Padding::from([8, 16]))
                .style(move |_theme: &iced::Theme, status: button::Status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => {
                            if merge_mode_active {
                                Some(Accent::MUTED.into())
                            } else {
                                Some(Backgrounds::ELEVATED.into())
                            }
                        }
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: Border::SUBTLE,
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
            return Self::empty_view_with_button(title, status_bar);
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
        .color(Text::SECONDARY);

        // Merge action bar (shown when merge mode is active and >=2 selected)
        let merge_action: Option<Element<'static, Message>> = if merge_mode_active {
            let selected_count = merge_selected.len();
            if selected_count >= 2 {
                let merge_execute_btn = button(
                    text(format!("Merge Selected ({})", selected_count))
                        .size(13)
                        .color(Text::PRIMARY),
                )
                .padding(Padding::from([8, 16]))
                .style(|_theme: &iced::Theme, status: button::Status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::HOVER.into()),
                        _ => Some(Accent::PRIMARY.into()),
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
                            .color(Accent::PRIMARY),
                            Space::with_width(Length::Fill),
                            merge_execute_btn,
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding(Padding::from([8, 16]))
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(Accent::MUTED.into()),
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
                            .color(Text::SECONDARY),
                    )
                    .padding(Padding::from([8, 16]))
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(Accent::MUTED.into()),
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

        for cluster in clusters {
            let is_editing = editing_cluster == Some(cluster.id);
            let is_selected = merge_selected_owned.contains(&cluster.id);
            let card = if merge_mode_active {
                Self::person_card_merge(cluster, is_selected)
            } else {
                Self::person_card(cluster, is_editing, edit_name)
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
                container(text(err.to_string()).size(12).color(iced::Color::from_rgb(0.9, 0.4, 0.4)))
                    .padding(Padding::from([8, 12]))
                    .style(|_theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
                        border: iced::Border {
                            color: iced::Color::from_rgb(0.8, 0.3, 0.3),
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        content_children.push(Space::with_height(24).into());
        content_children.push(scrollable(grid).into());

        let content = Column::with_children(content_children).padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Empty state with the title and process button
    fn empty_view_with_button(
        title: iced::widget::Text<'static>,
        status_bar: Element<'static, Message>,
    ) -> Element<'static, Message> {
        let content = column![
            title,
            Space::with_height(8),
            status_bar,
            Space::with_height(32),
            text("Faces will appear here after processing.")
                .size(14)
                .color(Text::SECONDARY),
            Space::with_height(8),
            text("Click \"Detect Faces\" to start scanning your photos for faces.")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render cluster detail view showing photos for a specific person
    pub fn view_cluster_detail(
        cluster: &FaceClusterRecord,
        photos: &[Photo],
        editing: bool,
        edit_name: &str,
        columns: usize,
    ) -> Element<'static, Message> {
        let cluster_id = cluster.id;

        // Back button
        let back_btn = button(
            row![
                text("\u{2190}").size(16).color(Text::PRIMARY),
                text("People").size(14).color(Text::SECONDARY)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([8, 12]))
        .style(|_theme: &iced::Theme, status: button::Status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                _ => None,
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
        .on_press(Message::BackToPeople);

        // Face thumbnail for header
        let face_avatar: Element<'static, Message> =
            if let Some(ref thumb_path) = cluster.face_thumbnail_path {
                let path = std::path::PathBuf::from(thumb_path);
                container(
                    iced::widget::image(iced::widget::image::Handle::from_path(&path))
                        .width(64)
                        .height(64)
                        .content_fit(iced::ContentFit::Cover),
                )
                .width(64)
                .height(64)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        radius: 32.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            } else {
                container(text("?").size(24).color(Text::SECONDARY))
                    .width(64)
                    .height(64)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
                        border: iced::Border {
                            radius: 32.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
            };

        // Person name (editable)
        let display_name = cluster
            .name
            .clone()
            .unwrap_or_else(|| format!("Unknown Person {}", cluster.id));

        let name_element: Element<'static, Message> = if editing {
            let edit_name_owned = edit_name.to_string();
            row![text_input("Enter name...", &edit_name_owned)
                .on_input(move |s| Message::EditClusterName(cluster_id, s))
                .on_submit(Message::SaveClusterName(cluster_id))
                .size(22)
                .width(Length::Fixed(300.0)),]
            .into()
        } else {
            button(text(display_name.clone()).size(24).color(Text::PRIMARY))
                .padding(Padding::from([4, 8]))
                .style(|_theme: &iced::Theme, status: button::Status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => None,
                    };
                    button::Style {
                        background,
                        border: iced::Border::default(),
                        ..Default::default()
                    }
                })
                .on_press(Message::StartEditClusterName(cluster_id))
                .into()
        };

        // Face count subtitle
        let subtitle = text(format!(
            "{} {}",
            photos.len(),
            if photos.len() == 1 { "photo" } else { "photos" }
        ))
        .size(14)
        .color(Text::SECONDARY);

        // Photo grid
        let grid: Element<'static, Message> = if photos.is_empty() {
            container(
                text("No photos found for this person.")
                    .size(14)
                    .color(Text::TERTIARY),
            )
            .padding(32)
            .into()
        } else {
            photo_grid_simple(photos, 160.0, columns)
        };

        let content = column![
            back_btn,
            Space::with_height(16),
            row![
                face_avatar,
                Space::with_width(16),
                column![name_element, subtitle,].spacing(4),
            ]
            .align_y(Alignment::Center),
            Space::with_height(24),
            scrollable(grid),
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a person card in merge selection mode
    fn person_card_merge(
        cluster: &FaceClusterRecord,
        is_selected: bool,
    ) -> Element<'static, Message> {
        let cluster_id = cluster.id;

        // Face thumbnail — same as normal card
        let face_circle: Element<'static, Message> =
            if let Some(ref thumb_path) = cluster.face_thumbnail_path {
                let path = std::path::PathBuf::from(thumb_path);
                container(
                    iced::widget::image(iced::widget::image::Handle::from_path(&path))
                        .width(80)
                        .height(80)
                        .content_fit(iced::ContentFit::Cover),
                )
                .width(80)
                .height(80)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        radius: 40.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            } else {
                let initials = cluster
                    .name
                    .as_deref()
                    .and_then(|n| n.chars().next())
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string());

                container(text(initials).size(28).color(Text::SECONDARY))
                    .width(80)
                    .height(80)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
                        border: iced::Border {
                            radius: 40.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
            };

        // Selection indicator
        let check_indicator = if is_selected {
            text("\u{2713}").size(16).color(Accent::PRIMARY) // checkmark
        } else {
            text("\u{25CB}").size(16).color(Text::TERTIARY) // empty circle
        };

        let display_name = cluster
            .name
            .clone()
            .unwrap_or_else(|| format!("Unknown Person {}", cluster.id));

        let card_content = column![
            row![Space::with_width(Length::Fill), check_indicator,],
            face_circle,
            Space::with_height(8),
            text(display_name).size(14).color(Text::PRIMARY),
            text(format!(
                "{} {}",
                cluster.face_count,
                if cluster.face_count == 1 {
                    "photo"
                } else {
                    "photos"
                }
            ))
            .size(12)
            .color(Text::TERTIARY),
        ]
        .spacing(4)
        .align_x(Alignment::Center);

        let border_color = if is_selected {
            Accent::PRIMARY
        } else {
            Border::SUBTLE
        };
        let border_width = if is_selected { 2.0 } else { 1.0 };

        button(
            container(card_content)
                .padding(16)
                .width(Length::Fixed(160.0)),
        )
        .style(move |_theme: &iced::Theme, status: button::Status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                _ => {
                    if is_selected {
                        Some(Accent::MUTED.into())
                    } else {
                        Some(Backgrounds::ELEVATED.into())
                    }
                }
            };
            button::Style {
                background,
                border: iced::Border {
                    color: border_color,
                    width: border_width,
                    radius: 12.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::ToggleMergeSelect(cluster_id))
        .into()
    }

    /// Render a person card
    fn person_card(
        cluster: &FaceClusterRecord,
        is_editing: bool,
        edit_name: &str,
    ) -> Element<'static, Message> {
        let cluster_id = cluster.id;

        // Face thumbnail — show actual face crop if available, otherwise initials
        let face_circle: Element<'static, Message> =
            if let Some(ref thumb_path) = cluster.face_thumbnail_path {
                let path = std::path::PathBuf::from(thumb_path);
                container(
                    iced::widget::image(iced::widget::image::Handle::from_path(&path))
                        .width(80)
                        .height(80)
                        .content_fit(iced::ContentFit::Cover),
                )
                .width(80)
                .height(80)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        radius: 40.0.into(), // Circular
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            } else {
                // Fallback: show initials
                let initials = cluster
                    .name
                    .as_deref()
                    .and_then(|n| n.chars().next())
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string());

                container(text(initials).size(28).color(Text::SECONDARY))
                    .width(80)
                    .height(80)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(Backgrounds::ELEVATED.into()),
                        border: iced::Border {
                            radius: 40.0.into(), // Circular
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
            };

        // Name (editable on click)
        let name_element: Element<'static, Message> = if is_editing {
            let edit_name_owned = edit_name.to_string();
            text_input("Enter name...", &edit_name_owned)
                .on_input(move |s| Message::EditClusterName(cluster_id, s))
                .on_submit(Message::SaveClusterName(cluster_id))
                .size(14)
                .width(Length::Fixed(140.0))
                .into()
        } else {
            let display_name = cluster
                .name
                .clone()
                .unwrap_or_else(|| format!("Unknown Person {}", cluster.id));

            button(text(display_name).size(14).color(Text::PRIMARY))
                .padding(Padding::from([4, 8]))
                .style(|_theme: &iced::Theme, status: button::Status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => None,
                    };
                    button::Style {
                        background,
                        border: iced::Border::default(),
                        ..Default::default()
                    }
                })
                .on_press(Message::StartEditClusterName(cluster_id))
                .into()
        };

        // Photo count
        let count = text(format!(
            "{} {}",
            cluster.face_count,
            if cluster.face_count == 1 {
                "photo"
            } else {
                "photos"
            }
        ))
        .size(12)
        .color(Text::TERTIARY);

        let card_content = column![face_circle, Space::with_height(12), name_element, count,]
            .spacing(4)
            .align_x(Alignment::Center);

        button(
            container(card_content)
                .padding(16)
                .width(Length::Fixed(160.0)),
        )
        .style(|_theme: &iced::Theme, status: button::Status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                _ => Some(Backgrounds::ELEVATED.into()),
            };
            button::Style {
                background,
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectCluster(cluster_id))
        .into()
    }
}
