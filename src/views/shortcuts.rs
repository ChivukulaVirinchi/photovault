//! Keyboard shortcuts overlay + Settings reference list.

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

use super::super::app::state::View;

pub struct ShortcutGroup {
    pub title: &'static str,
    pub items: Vec<(&'static str, &'static str)>,
}

pub fn shortcuts_for(view: &View) -> Vec<ShortcutGroup> {
    let global = ShortcutGroup {
        title: "Global",
        items: vec![
            ("?", "Show this help"),
            ("/  or  F", "Jump to Search"),
            ("[", "Toggle sidebar"),
            ("Cmd+,", "Open Settings"),
            ("Cmd+1-9", "Jump to sidebar item"),
            ("Cmd+W", "Close current detail view"),
            ("Cmd+Z", "Undo last action (where supported)"),
            ("Esc", "Cancel / go back"),
        ],
    };

    let view_specific = match view {
        View::Timeline => ShortcutGroup {
            title: "Timeline",
            items: vec![
                ("\u{2190} \u{2192} \u{2191} \u{2193}", "Move grid cursor"),
                ("Enter", "Open highlighted photo"),
                ("Space", "Toggle selection"),
                ("Delete / Backspace", "Trash selected"),
            ],
        },
        View::PhotoDetail => ShortcutGroup {
            title: "Photo Detail",
            items: vec![
                ("\u{2190} \u{2192}", "Previous / next photo"),
                ("R", "Rotate"),
                ("I", "Toggle metadata panel"),
                ("Delete", "Trash photo"),
                ("Esc", "Close"),
            ],
        },
        View::Cull => ShortcutGroup {
            title: "Cull",
            items: vec![
                ("\u{2190} \u{2192}", "Previous / next"),
                ("X", "Toggle trash mark"),
                ("U  or  Cmd+Z", "Undo"),
                ("Enter", "Finish"),
                ("Esc", "Exit"),
            ],
        },
        View::FaceReview => ShortcutGroup {
            title: "Face Review",
            items: vec![
                ("Y  or  \u{2192}", "Same person"),
                ("N  or  \u{2190}", "Different person"),
                ("S  or  Space", "Skip"),
                ("U  or  Cmd+Z", "Undo"),
                ("Enter  or  Esc", "Finish"),
            ],
        },
        View::Map => ShortcutGroup {
            title: "Map",
            items: vec![
                ("\u{2190} \u{2192} \u{2191} \u{2193}", "Pan"),
                ("+  or  =", "Zoom in"),
                ("-  or  _", "Zoom out"),
                ("Esc", "Close popover"),
            ],
        },
        View::MemoryDetail => ShortcutGroup {
            title: "Memory Slideshow",
            items: vec![
                ("\u{2190} \u{2192}", "Previous / next"),
                ("Space", "Pause / resume"),
                ("Esc", "Close"),
            ],
        },
        _ => ShortcutGroup {
            title: "This View",
            items: vec![],
        },
    };

    vec![global, view_specific]
}

pub fn overlay(view: &View, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let groups = shortcuts_for(view);

    let mut col = column![
        row![
            text("Keyboard Shortcuts").size(20).color(p.text_primary),
            Space::with_width(Length::Fill),
            button(text("\u{00D7}").size(18).color(p.text_secondary))
                .padding(Padding::from([4, 10]))
                .style(|_t, _s| button::Style::default())
                .on_press(Message::ToggleShortcutsOverlay),
        ]
        .align_y(Alignment::Center),
        Space::with_height(16),
    ]
    .spacing(6);

    for group in groups {
        if group.items.is_empty() {
            continue;
        }
        col = col.push(
            text(group.title.to_string())
                .size(13)
                .color(p.text_secondary),
        );
        col = col.push(Space::with_height(6));
        for (keys, desc) in group.items {
            col = col.push(
                row![
                    container(text(keys.to_string()).size(12).color(p.text_primary))
                        .width(Length::Fixed(180.0)),
                    text(desc.to_string()).size(12).color(p.text_secondary),
                ]
                .align_y(Alignment::Center),
            );
            col = col.push(Space::with_height(3));
        }
        col = col.push(Space::with_height(14));
    }

    col = col.push(
        text("Press ? or Esc to close")
            .size(11)
            .color(p.text_tertiary),
    );

    let bg_elevated = p.bg_elevated;
    let border_subtle = p.border_subtle;
    let panel = container(scrollable(col).height(Length::Shrink))
        .padding(24)
        .max_width(560.0)
        .max_height(640.0)
        .style(move |_| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_subtle,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        });

    let backdrop = mouse_area(
        container(Space::new(Length::Fill, Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.50,
                })),
                ..Default::default()
            }),
    )
    .on_press(Message::ToggleShortcutsOverlay);

    let centered = container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    iced::widget::stack![backdrop, centered].into()
}

/// Render the full shortcuts reference as a Settings page section.
pub fn reference_list(theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let views = [
        View::Timeline,
        View::PhotoDetail,
        View::Cull,
        View::FaceReview,
        View::Map,
        View::MemoryDetail,
    ];

    let mut col = column![].spacing(6);

    // Global first
    if let Some(global) = shortcuts_for(&View::Timeline).into_iter().next() {
        col = col.push(
            text(global.title.to_string())
                .size(13)
                .color(p.text_secondary),
        );
        col = col.push(Space::with_height(6));
        for (keys, desc) in global.items {
            col = col.push(
                row![
                    container(text(keys.to_string()).size(12).color(p.text_primary))
                        .width(Length::Fixed(200.0)),
                    text(desc.to_string()).size(12).color(p.text_secondary),
                ]
                .align_y(Alignment::Center),
            );
            col = col.push(Space::with_height(3));
        }
        col = col.push(Space::with_height(14));
    }

    for v in views {
        let groups = shortcuts_for(&v);
        if let Some(g) = groups.into_iter().nth(1) {
            if g.items.is_empty() {
                continue;
            }
            col = col.push(text(g.title.to_string()).size(13).color(p.text_secondary));
            col = col.push(Space::with_height(6));
            for (keys, desc) in g.items {
                col = col.push(
                    row![
                        container(text(keys.to_string()).size(12).color(p.text_primary))
                            .width(Length::Fixed(200.0)),
                        text(desc.to_string()).size(12).color(p.text_secondary),
                    ]
                    .align_y(Alignment::Center),
                );
                col = col.push(Space::with_height(3));
            }
            col = col.push(Space::with_height(14));
        }
    }

    col.into()
}
