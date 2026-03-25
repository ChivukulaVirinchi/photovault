//! Settings view.

use iced::widget::{button, column, container, row, slider, text, toggler, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::{AppConfig, AppTheme, DateFormat};
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

pub struct SettingsView;

impl SettingsView {
    pub fn view(config: &AppConfig, geocoding_progress: Option<(usize, usize)>) -> Element<'static, Message> {
        let content = column![
            text("Settings").size(28).color(Text::PRIMARY),
            Space::with_height(24),
            Self::section_header("Appearance"),
            Self::theme_setting(config.theme),
            Space::with_height(24),
            Self::section_header("Indexing"),
            Self::thumbnail_size_setting(config.thumbnail_size),
            Self::hidden_folders_setting(config.scan_hidden_folders),
            Space::with_height(24),
            Self::section_header("Face Recognition"),
            Self::face_confidence_setting(config.face_detection_confidence),
            Self::clustering_threshold_setting(config.face_clustering_threshold),
            Space::with_height(24),
            Self::section_header("Burst Detection"),
            Self::burst_window_setting(config.burst_time_window_seconds),
            Space::with_height(24),
            Self::section_header("Trash"),
            Self::auto_delete_setting(config.trash_auto_delete_days),
            Space::with_height(24),
            Self::section_header("Date & Time"),
            Self::date_format_setting(config.date_format),
            Space::with_height(32),
            Self::actions_section(geocoding_progress),
        ]
        .padding(32)
        .spacing(8);

        container(iced::widget::scrollable(content))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    fn section_header(title: &str) -> Element<'static, Message> {
        let title = title.to_string();
        column![
            text(title).size(16).color(Text::PRIMARY),
            container(Space::new(Length::Fill, Length::Fixed(1.0))).style(|_theme| {
                container::Style {
                    background: Some(Border::SUBTLE.into()),
                    ..Default::default()
                }
            }),
        ]
        .spacing(8)
        .into()
    }

    fn theme_setting(current: AppTheme) -> Element<'static, Message> {
        let options = vec![
            ("Dark", AppTheme::Dark),
            ("Light", AppTheme::Light),
            ("System", AppTheme::System),
        ];
        Self::setting_row(
            "Theme",
            "Choose the app color scheme",
            Self::option_buttons(&options, current, Message::SetTheme),
        )
    }

    fn thumbnail_size_setting(current: u32) -> Element<'static, Message> {
        let options = vec![
            ("Small (200px)", 200u32),
            ("Medium (300px)", 300u32),
            ("Large (400px)", 400u32),
        ];
        Self::setting_row(
            "Thumbnail Size",
            "Size of generated thumbnails",
            Self::option_buttons(&options, current, Message::SetThumbnailSize),
        )
    }

    fn hidden_folders_setting(enabled: bool) -> Element<'static, Message> {
        Self::setting_row(
            "Scan Hidden Folders",
            "Include folders starting with '.'",
            toggler(enabled)
                .on_toggle(Message::SetScanHiddenFolders)
                .into(),
        )
    }

    fn face_confidence_setting(current: f32) -> Element<'static, Message> {
        let percentage = (current * 100.0) as u32;
        Self::setting_row(
            "Face Detection Confidence",
            "Minimum confidence for face detection",
            row![
                slider(30..=90, percentage, |v| Message::SetFaceConfidence(
                    v as f32 / 100.0
                ))
                .width(200),
                Space::with_width(16),
                text(format!("{}%", percentage))
                    .size(14)
                    .color(Text::SECONDARY),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    fn clustering_threshold_setting(current: f32) -> Element<'static, Message> {
        let percentage = (current * 100.0) as u32;
        Self::setting_row(
            "Face Grouping Similarity",
            "How similar faces must be to group together",
            row![
                slider(40..=80, percentage, |v| Message::SetClusteringThreshold(
                    v as f32 / 100.0
                ))
                .width(200),
                Space::with_width(16),
                text(format!("{}%", percentage))
                    .size(14)
                    .color(Text::SECONDARY),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    fn burst_window_setting(current: i64) -> Element<'static, Message> {
        let options = vec![
            ("2 seconds", 2i64),
            ("3 seconds", 3i64),
            ("5 seconds", 5i64),
            ("10 seconds", 10i64),
        ];
        Self::setting_row(
            "Burst Time Window",
            "Maximum gap between photos in a burst",
            Self::option_buttons(&options, current, Message::SetBurstWindow),
        )
    }

    fn auto_delete_setting(current: u32) -> Element<'static, Message> {
        let options = vec![
            ("Never", 0u32),
            ("7 days", 7u32),
            ("30 days", 30u32),
            ("90 days", 90u32),
        ];
        Self::setting_row(
            "Auto-Delete Trash",
            "Permanently delete trashed photos after",
            Self::option_buttons(&options, current, Message::SetTrashAutoDelete),
        )
    }

    fn date_format_setting(current: DateFormat) -> Element<'static, Message> {
        let options = vec![
            ("ISO (2019-03-15)", DateFormat::Iso),
            ("US (03/15/2019)", DateFormat::Us),
            ("EU (15/03/2019)", DateFormat::Eu),
        ];
        Self::setting_row(
            "Date Format",
            "How dates are displayed",
            Self::option_buttons(&options, current, Message::SetDateFormat),
        )
    }

    fn actions_section(geocoding_progress: Option<(usize, usize)>) -> Element<'static, Message> {
        let geocode_button: Element<'static, Message> = if let Some((done, total)) = geocoding_progress {
            let label = if total == 0 {
                "Geocoding...".to_string()
            } else {
                format!("Geocoding... {}/{}", done, total)
            };
            button(text(label).size(14).color(Text::SECONDARY))
                .padding(Padding::from([10, 18]))
                .style(|_theme, _status| button::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        color: Border::SUBTLE,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        } else {
            Self::action_button("Run Geocoding", Message::RunGeocoding)
        };

        column![
            Self::section_header("Advanced"),
            Space::with_height(16),
            row![
                Self::action_button("Re-scan Library", Message::RescanLibrary),
                Space::with_width(16),
                Self::action_button("Rebuild Face Clusters", Message::RebuildFaceClusters),
                Space::with_width(16),
                Self::action_button("Check for Changes", Message::CheckForChanges),
                Space::with_width(16),
                geocode_button,
            ],
            Space::with_height(24),
            text("PhotoVault v0.1.0").size(12).color(Text::TERTIARY),
        ]
        .into()
    }

    fn action_button(label: &str, on_press: Message) -> Element<'static, Message> {
        button(text(label.to_string()).size(14).color(Text::PRIMARY))
            .padding(Padding::from([10, 18]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => Some(Backgrounds::ELEVATED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: Border::SUBTLE,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(on_press)
            .into()
    }

    fn setting_row(
        label: &str,
        description: &str,
        control: Element<'static, Message>,
    ) -> Element<'static, Message> {
        let label = label.to_string();
        let description = description.to_string();
        container(
            row![
                column![
                    text(label).size(14).color(Text::PRIMARY),
                    text(description).size(12).color(Text::TERTIARY),
                ]
                .width(Length::FillPortion(2)),
                container(control)
                    .width(Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([12, 0]))
        .into()
    }

    fn option_buttons<T: PartialEq + Copy + 'static>(
        options: &[(&str, T)],
        current: T,
        on_select: impl Fn(T) -> Message + 'static + Clone,
    ) -> Element<'static, Message> {
        let buttons: Vec<Element<'static, Message>> = options
            .iter()
            .map(|(label, value)| {
                let is_selected = *value == current;
                let value = *value;
                let on_select = on_select.clone();

                button(text((*label).to_string()).size(12).color(if is_selected {
                    Accent::PRIMARY
                } else {
                    Text::PRIMARY
                }))
                .padding(Padding::from([6, 12]))
                .style(move |_theme, status| {
                    let background = if is_selected {
                        Some(Accent::MUTED.into())
                    } else {
                        match status {
                            button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                            _ => Some(Backgrounds::ELEVATED.into()),
                        }
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: if is_selected {
                                Accent::PRIMARY
                            } else {
                                Border::SUBTLE
                            },
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(on_select(value))
                .into()
            })
            .collect();

        Row::with_children(buttons).spacing(8).into()
    }
}
