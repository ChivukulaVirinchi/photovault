//! Settings view.

use iced::widget::{button, column, container, row, slider, text, text_input, toggler, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::tooltip::with_tooltip;
use crate::config::{AppConfig, AppTheme, DateFormat};
use crate::theme::colors;

pub struct SettingsView;

impl SettingsView {
    pub fn view(
        config: &AppConfig,
        geocoding_progress: Option<(usize, usize)>,
        rotated_fix_running: bool,
        map_cache_size_mb: f64,
        map_cache_limit_mb: u32,
        auto_update_check_enabled: bool,
        update_check_in_progress: bool,
        show_advanced: bool,
    ) -> Element<'static, Message> {
        let p = colors::palette(config.theme);
        let bg_primary = p.bg_primary;

        let content = column![
            text("Settings").size(28).color(p.text_primary),
            Space::with_height(24),
            Self::section_header(config.theme, "Appearance"),
            Self::theme_setting(config.theme),
            Space::with_height(24),
            Self::section_header(config.theme, "Indexing"),
            Self::thumbnail_size_setting(config.theme, config.thumbnail_size),
            Self::hidden_folders_setting(config.theme, config.scan_hidden_folders),
            Space::with_height(24),
            Self::section_header(config.theme, "Face Recognition"),
            Self::face_confidence_setting(config.theme, config.face_detection_confidence),
            Self::clustering_threshold_setting(config.theme, config.face_clustering_threshold),
            Space::with_height(24),
            Self::section_header(config.theme, "Burst Detection"),
            Self::burst_window_setting(config.theme, config.burst_time_window_seconds),
            Space::with_height(24),
            Self::section_header(config.theme, "Trash"),
            Self::auto_delete_setting(config.theme, config.trash_auto_delete_days),
            Space::with_height(24),
            Self::section_header(config.theme, "Date & Time"),
            Self::date_format_setting(config.theme, config.date_format),
            Space::with_height(24),
            Self::section_header(config.theme, "Memories"),
            Self::memories_setting(config.theme, config.memories_enabled),
            Space::with_height(24),
            Self::section_header(config.theme, "Album Suggestions"),
            Self::home_city_setting(config.theme, config.home_city_override.as_deref()),
            Space::with_height(24),
            Self::section_header(config.theme, "Map"),
            Self::map_cache_setting(config.theme, map_cache_size_mb, map_cache_limit_mb,),
            Space::with_height(24),
            Self::section_header(config.theme, "Updates"),
            Self::updates_setting(
                config.theme,
                auto_update_check_enabled,
                update_check_in_progress,
            ),
            Space::with_height(32),
            Self::actions_section(
                config.theme,
                geocoding_progress,
                rotated_fix_running,
                show_advanced,
            ),
            Space::with_height(32),
            Self::section_header(config.theme, "Keyboard Shortcuts"),
            Space::with_height(8),
            crate::views::shortcuts::reference_list(config.theme),
        ]
        .padding(32)
        .spacing(8);

        container(iced::widget::scrollable(content))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(bg_primary.into()),
                ..Default::default()
            })
            .into()
    }

    fn section_header(theme: AppTheme, title: &str) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_primary = p.text_primary;
        let border_subtle = p.border_subtle;

        let title = title.to_string();
        column![
            text(title).size(16).color(text_primary),
            container(Space::new(Length::Fill, Length::Fixed(1.0))).style(move |_theme| {
                container::Style {
                    background: Some(border_subtle.into()),
                    ..Default::default()
                }
            }),
        ]
        .spacing(8)
        .into()
    }

    fn theme_setting(theme: AppTheme) -> Element<'static, Message> {
        let options = vec![
            ("Dark", AppTheme::Dark),
            ("Light", AppTheme::Light),
            ("System", AppTheme::System),
        ];
        Self::setting_row(
            theme,
            "Theme",
            "Choose the app color scheme",
            Self::option_buttons(theme, &options, theme, Message::SetTheme),
        )
    }

    fn thumbnail_size_setting(theme: AppTheme, current: u32) -> Element<'static, Message> {
        let options = vec![
            ("Small (200px)", 200u32),
            ("Medium (300px)", 300u32),
            ("Large (400px)", 400u32),
        ];
        Self::setting_row(
            theme,
            "Thumbnail Size",
            "Size of generated thumbnails",
            Self::option_buttons(theme, &options, current, Message::SetThumbnailSize),
        )
    }

    fn hidden_folders_setting(theme: AppTheme, enabled: bool) -> Element<'static, Message> {
        Self::setting_row(
            theme,
            "Scan Hidden Folders",
            "Include folders starting with '.'",
            toggler(enabled)
                .on_toggle(Message::SetScanHiddenFolders)
                .into(),
        )
    }

    fn memories_setting(theme: AppTheme, enabled: bool) -> Element<'static, Message> {
        Self::setting_row(
            theme,
            "Memories",
            "Show 'N years ago today' cards on the Timeline",
            toggler(enabled)
                .on_toggle(Message::SetMemoriesEnabled)
                .into(),
        )
    }

    fn updates_setting(
        theme: AppTheme,
        auto_update_check_enabled: bool,
        check_in_progress: bool,
    ) -> Element<'static, Message> {
        let toggle_row = Self::setting_row(
            theme,
            "Automatically check for updates",
            "Query GitHub once every 24 hours. Off by default. See PRIVACY.md.",
            toggler(auto_update_check_enabled)
                .on_toggle(Message::SetAutoUpdateCheck)
                .into(),
        );

        let check_label = if check_in_progress {
            "Checking..."
        } else {
            "Check for updates now"
        };
        let on_press = if check_in_progress {
            None
        } else {
            Some(Message::CheckForUpdates)
        };

        let p = colors::palette(theme);
        let text_primary = p.text_primary;
        let bg_elevated = p.bg_elevated;
        let bg_hover = p.bg_hover;
        let border_subtle = p.border_subtle;

        let mut btn = button(text(check_label.to_string()).size(14).color(text_primary))
            .padding(Padding::from([8, 14]))
            .style(move |_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(bg_hover.into()),
                    _ => Some(bg_elevated.into()),
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
            });
        if let Some(msg) = on_press {
            btn = btn.on_press(msg);
        }

        column![toggle_row, Space::with_height(10), btn]
            .spacing(0)
            .into()
    }

    fn map_cache_setting(
        theme: AppTheme,
        cache_size_mb: f64,
        cache_limit_mb: u32,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_primary = p.text_primary;
        let text_secondary = p.text_secondary;

        let limit_value = cache_limit_mb.to_string();

        let bg_elevated = p.bg_elevated;
        let bg_hover = p.bg_hover;
        let border_subtle = p.border_subtle;
        let text_primary_btn = p.text_primary;

        column![
            row![
                text("Tile cache size:").size(12).color(text_secondary),
                Space::with_width(8),
                text(format!("{:.1} MB", cache_size_mb))
                    .size(12)
                    .color(text_primary),
                Space::with_width(Length::Fill),
                with_tooltip(
                    button(text("Clear cache").size(12).color(text_primary_btn))
                        .padding(Padding::from([6, 10]))
                        .style(move |_theme, status| button::Style {
                            background: Some(match status {
                                button::Status::Hovered => bg_hover.into(),
                                _ => bg_elevated.into(),
                            }),
                            border: iced::Border {
                                color: border_subtle,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        })
                        .on_press(Message::ClearMapCache)
                        .into(),
                    "Clear all cached map tiles",
                ),
            ]
            .align_y(Alignment::Center),
            Space::with_height(8),
            row![
                text("Cache size limit:").size(12).color(text_secondary),
                Space::with_width(8),
                text_input("MB", &limit_value)
                    .on_input(|s: String| {
                        let default_mb = (crate::services::tile_cache::DEFAULT_CACHE_LIMIT_BYTES
                            / 1024
                            / 1024) as u32;
                        s.parse::<u32>()
                            .map(Message::SetMapCacheLimit)
                            .unwrap_or(Message::SetMapCacheLimit(default_mb))
                    })
                    .width(Length::Fixed(80.0)),
                Space::with_width(4),
                text("MB").size(12).color(text_secondary),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(0)
        .into()
    }

    fn home_city_setting(theme: AppTheme, current: Option<&str>) -> Element<'static, Message> {
        let value = current.unwrap_or("").to_string();
        Self::setting_row(
            theme,
            "Home City",
            "Override auto-detected home city for trip suggestions (leave blank for auto)",
            text_input("Auto-detect", &value)
                .on_input(Message::SetHomeCity)
                .size(13)
                .width(Length::Fixed(200.0))
                .into(),
        )
    }

    fn face_confidence_setting(theme: AppTheme, current: f32) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_secondary = p.text_secondary;

        let percentage = (current * 100.0) as u32;
        Self::setting_row(
            theme,
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
                    .color(text_secondary),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    fn clustering_threshold_setting(theme: AppTheme, current: f32) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_secondary = p.text_secondary;

        let percentage = (current * 100.0) as u32;
        Self::setting_row(
            theme,
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
                    .color(text_secondary),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    fn burst_window_setting(theme: AppTheme, current: i64) -> Element<'static, Message> {
        let options = vec![
            ("2 seconds", 2i64),
            ("3 seconds", 3i64),
            ("5 seconds", 5i64),
            ("10 seconds", 10i64),
        ];
        Self::setting_row(
            theme,
            "Burst Time Window",
            "Maximum gap between photos in a burst",
            Self::option_buttons(theme, &options, current, Message::SetBurstWindow),
        )
    }

    fn auto_delete_setting(theme: AppTheme, current: u32) -> Element<'static, Message> {
        let options = vec![
            ("Never", 0u32),
            ("7 days", 7u32),
            ("30 days", 30u32),
            ("90 days", 90u32),
        ];
        Self::setting_row(
            theme,
            "Auto-Delete Trash",
            "Permanently delete trashed photos after",
            Self::option_buttons(theme, &options, current, Message::SetTrashAutoDelete),
        )
    }

    fn date_format_setting(theme: AppTheme, current: DateFormat) -> Element<'static, Message> {
        let options = vec![
            ("ISO (2019-03-15)", DateFormat::Iso),
            ("US (03/15/2019)", DateFormat::Us),
            ("EU (15/03/2019)", DateFormat::Eu),
        ];
        Self::setting_row(
            theme,
            "Date Format",
            "How dates are displayed",
            Self::option_buttons(theme, &options, current, Message::SetDateFormat),
        )
    }

    fn actions_section(
        theme: AppTheme,
        geocoding_progress: Option<(usize, usize)>,
        rotated_fix_running: bool,
        show_advanced: bool,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_secondary = p.text_secondary;
        let text_tertiary = p.text_tertiary;
        let bg_elevated = p.bg_elevated;
        let bg_hover = p.bg_hover;
        let border_subtle = p.border_subtle;

        let geocode_button: Element<'static, Message> =
            if let Some((done, total)) = geocoding_progress {
                let label = if total == 0 {
                    "Geocoding...".to_string()
                } else {
                    format!("Geocoding... {}/{}", done, total)
                };
                button(text(label).size(14).color(text_secondary))
                    .padding(Padding::from([10, 18]))
                    .style(move |_theme, _status| button::Style {
                        background: Some(bg_elevated.into()),
                        border: iced::Border {
                            color: border_subtle,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    })
                    .into()
            } else {
                Self::action_button(theme, "Run Geocoding", Message::RunGeocoding)
            };

        let rotated_button: Element<'static, Message> = if rotated_fix_running {
            button(text("Fixing Rotations...").size(14).color(text_secondary))
                .padding(Padding::from([10, 18]))
                .style(move |_theme, _status| button::Style {
                    background: Some(bg_elevated.into()),
                    border: iced::Border {
                        color: border_subtle,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        } else {
            Self::action_button(theme, "Fix Rotated Photos", Message::RegenerateRotatedData)
        };

        // Library Maintenance: friendly, non-destructive actions any
        // user can run. ProcessFaces here only processes photos that
        // haven't been analyzed yet — it does NOT discard existing
        // clusters (that's the destructive Rebuild in Advanced).
        let maintenance_row = row![
            Self::action_button(theme, "Find Faces in New Photos", Message::ProcessFaces),
            Space::with_width(16),
            Self::action_button(theme, "Check for New Photos", Message::CheckForChanges),
            Space::with_width(16),
            geocode_button,
        ];

        // Show advanced toggle (low-key text button).
        let toggle_label = if show_advanced {
            "Hide advanced"
        } else {
            "Show advanced"
        };
        let advanced_toggle = button(text(toggle_label).size(12).color(text_tertiary))
            .padding(Padding::from([6, 10]))
            .style(move |_t, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => bg_hover.into(),
                    _ => iced::Background::Color(iced::Color::TRANSPARENT),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::ToggleSettingsAdvanced);

        let mut content = column![
            Self::section_header(theme, "Library Maintenance"),
            Space::with_height(16),
            maintenance_row,
            Space::with_height(24),
            advanced_toggle,
        ];

        if show_advanced {
            content = content.push(Space::with_height(16)).push(
                column![
                    Self::section_header(theme, "Advanced (developer / destructive)"),
                    Space::with_height(16),
                    row![
                        Self::action_button(theme, "Re-scan Library", Message::RescanLibrary),
                        Space::with_width(16),
                        Self::action_button(
                            theme,
                            "Rebuild All Face Groups",
                            Message::RequestRebuildFaces,
                        ),
                        Space::with_width(16),
                        Self::action_button(
                            theme,
                            "Regenerate Thumbnails",
                            Message::RegenerateThumbnails,
                        ),
                        Space::with_width(16),
                        Self::action_button(
                            theme,
                            "Refresh Photo Dates",
                            Message::RefreshPhotoDates,
                        ),
                        Space::with_width(16),
                        rotated_button,
                        Space::with_width(16),
                        Self::action_button(theme, "Reinstall Assets", Message::InstallAssetPack),
                    ],
                ]
                .spacing(0),
            );
        }

        content
            .push(Space::with_height(24))
            .push(
                text(format!("PhotoVault v{}", env!("CARGO_PKG_VERSION")))
                    .size(12)
                    .color(text_tertiary),
            )
            .into()
    }

    fn action_button(theme: AppTheme, label: &str, on_press: Message) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_primary = p.text_primary;
        let bg_elevated = p.bg_elevated;
        let bg_hover = p.bg_hover;
        let border_subtle = p.border_subtle;

        with_tooltip(
            button(text(label.to_string()).size(14).color(text_primary))
                .padding(Padding::from([10, 18]))
                .style(move |_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(bg_hover.into()),
                        _ => Some(bg_elevated.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: border_subtle,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(on_press)
                .into(),
            label.to_string(),
        )
    }

    fn setting_row(
        theme: AppTheme,
        label: &str,
        description: &str,
        control: Element<'static, Message>,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_primary = p.text_primary;
        let text_tertiary = p.text_tertiary;

        let label = label.to_string();
        let description = description.to_string();
        container(
            row![
                column![
                    text(label).size(14).color(text_primary),
                    text(description).size(12).color(text_tertiary),
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
        theme: AppTheme,
        options: &[(&str, T)],
        current: T,
        on_select: impl Fn(T) -> Message + 'static + Clone,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let text_primary = p.text_primary;
        let accent_primary = p.accent_primary;
        let accent_muted = p.accent_muted;
        let bg_elevated = p.bg_elevated;
        let bg_hover = p.bg_hover;
        let border_subtle = p.border_subtle;

        let buttons: Vec<Element<'static, Message>> = options
            .iter()
            .map(|(label, value)| {
                let is_selected = *value == current;
                let value = *value;
                let on_select = on_select.clone();

                button(text((*label).to_string()).size(12).color(if is_selected {
                    accent_primary
                } else {
                    text_primary
                }))
                .padding(Padding::from([6, 12]))
                .style(move |_theme, status| {
                    let background = if is_selected {
                        Some(accent_muted.into())
                    } else {
                        match status {
                            button::Status::Hovered => Some(bg_hover.into()),
                            _ => Some(bg_elevated.into()),
                        }
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            color: if is_selected {
                                accent_primary
                            } else {
                                border_subtle
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
