//! Quick cull view.

use std::collections::HashSet;
use std::path::Path;

use iced::widget::{button, column, container, row, text, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::models::Photo;
use crate::theme::colors::{Accent, Backgrounds, Border, Semantic, Text};

/// Cull mode state.
#[derive(Debug, Clone, Default)]
pub struct CullState {
    pub photo_ids: Vec<i64>,
    pub current_index: usize,
    pub marked_for_trash: HashSet<i64>,
    pub undo_stack: Vec<(i64, bool)>,
}

impl CullState {
    pub fn new(photo_ids: Vec<i64>) -> Self {
        Self {
            photo_ids,
            current_index: 0,
            marked_for_trash: HashSet::new(),
            undo_stack: Vec::new(),
        }
    }

    pub fn current_photo_id(&self) -> Option<i64> {
        self.photo_ids.get(self.current_index).copied()
    }

    pub fn next(&mut self) {
        if self.current_index + 1 < self.photo_ids.len() {
            self.current_index += 1;
        }
    }

    pub fn prev(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
    }

    pub fn toggle_trash(&mut self) {
        if let Some(id) = self.current_photo_id() {
            let was_marked = self.marked_for_trash.contains(&id);
            if was_marked {
                self.marked_for_trash.remove(&id);
            } else {
                self.marked_for_trash.insert(id);
            }
            self.undo_stack.push((id, was_marked));
        }
    }

    pub fn undo(&mut self) {
        if let Some((id, was_marked)) = self.undo_stack.pop() {
            if was_marked {
                self.marked_for_trash.insert(id);
            } else {
                self.marked_for_trash.remove(&id);
            }
        }
    }

    pub fn marked_count(&self) -> usize {
        self.marked_for_trash.len()
    }

    pub fn is_current_marked(&self) -> bool {
        self.current_photo_id()
            .map(|id| self.marked_for_trash.contains(&id))
            .unwrap_or(false)
    }
}

/// Cull mode UI.
pub struct CullView;

impl CullView {
    pub fn view(
        state: &CullState,
        title: &str,
        photos: &[Photo],
        drive_path: Option<&Path>,
        confirm_pending: bool,
    ) -> Element<'static, Message> {
        let title_owned = title.to_string();
        let total = state.photo_ids.len();
        let current = if total == 0 {
            0
        } else {
            state.current_index + 1
        };
        let marked = state.marked_count();
        let is_marked = state.is_current_marked();

        let header = row![
            text(title_owned).size(16).color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            button(text("Exit Cull").size(12).color(Text::PRIMARY))
                .padding(Padding::from([6, 12]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
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
                .on_press(Message::ExitCullMode),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([16, 32]));

        let current_photo = state
            .current_photo_id()
            .and_then(|id| photos.iter().find(|p| p.id == id));

        let photo_widget: Element<'static, Message> =
            if let (Some(photo), Some(root)) = (current_photo, drive_path) {
                if let Some(ref t) = photo.thumbnail_path {
                    let p = std::path::PathBuf::from(t);
                    if p.exists() {
                        container(
                            iced::widget::image(iced::widget::image::Handle::from_path(p))
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .content_fit(iced::ContentFit::Contain),
                        )
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                    } else {
                        let full = root.join(&photo.file_path);
                        if full.exists() {
                            container(
                                iced::widget::image(iced::widget::image::Handle::from_path(full))
                                    .width(Length::Fill)
                                    .height(Length::Fill)
                                    .content_fit(iced::ContentFit::Contain),
                            )
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .into()
                        } else {
                            text("Current photo").size(14).color(Text::TERTIARY).into()
                        }
                    }
                } else {
                    text("Current photo").size(14).color(Text::TERTIARY).into()
                }
            } else {
                text("Current photo").size(14).color(Text::TERTIARY).into()
            };

        let image_area = container(
            column![
                if is_marked {
                    container(
                        text("MARKED FOR DELETION")
                            .size(12)
                            .color(Backgrounds::PRIMARY),
                    )
                    .padding(Padding::from([4, 8]))
                    .style(|_theme| container::Style {
                        background: Some(Semantic::DANGER.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                } else {
                    container(Space::with_height(Length::Shrink))
                },
                Space::with_height(Length::Fill),
                photo_widget,
                Space::with_height(Length::Fill),
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::FillPortion(3))
        .padding(32)
        .style(move |_theme| container::Style {
            background: Some(
                if is_marked {
                    iced::Color { a: 0.10, ..Semantic::DANGER }
                } else {
                    Backgrounds::ELEVATED
                }
                .into(),
            ),
            border: iced::Border {
                color: if is_marked {
                    Semantic::DANGER
                } else {
                    Border::SUBTLE
                },
                width: 2.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        let info = row![
            text(format!("{} of {}", current, total))
                .size(14)
                .color(Text::PRIMARY),
            Space::with_width(Length::Fill),
            text(format!("{} marked for deletion", marked))
                .size(14)
                .color(if marked > 0 {
                    Semantic::DANGER
                } else {
                    Text::TERTIARY
                }),
        ]
        .padding(Padding::from([8, 32]));

        let filmstrip = Self::render_filmstrip(state, photos, drive_path);

        let controls = row![
            button(text("< Prev").size(14).color(Text::PRIMARY))
                .padding(Padding::from([12, 20]))
                .on_press(Message::CullPrev),
            Space::with_width(10),
            button(
                text(if is_marked { "Unmark (X)" } else { "Trash (X)" })
                    .size(14)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([12, 24]))
            .on_press(Message::CullToggleTrash),
            Space::with_width(10),
            button(text("Next >").size(14).color(Text::PRIMARY))
                .padding(Padding::from([12, 20]))
                .on_press(Message::CullNext),
            Space::with_width(Length::Fill),
            button(text("Undo (U)").size(12).color(Text::SECONDARY))
                .padding(Padding::from([8, 14]))
                .on_press(Message::CullUndo),
            Space::with_width(10),
            button(
                text(if confirm_pending {
                    "Confirm Trash"
                } else {
                    "Finish (Enter)"
                })
                .size(14)
                .color(Backgrounds::PRIMARY)
            )
            .padding(Padding::from([12, 20]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Accent::PRIMARY.into()),
                    _ => Some(Accent::MUTED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(if confirm_pending {
                Message::CullConfirmTrash
            } else {
                Message::CullFinish
            }),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([16, 32]));

        let hints = row![
            text("Keyboard:").size(11).color(Text::TERTIARY),
            Space::with_width(8),
            text("<- -> navigate").size(11).color(Text::TERTIARY),
            Space::with_width(12),
            text("X trash").size(11).color(Text::TERTIARY),
            Space::with_width(12),
            text("U undo").size(11).color(Text::TERTIARY),
            Space::with_width(12),
            text("Enter finish").size(11).color(Text::TERTIARY),
            Space::with_width(12),
            text("Esc exit").size(11).color(Text::TERTIARY),
        ]
        .padding(Padding::from([8, 32]));

        let confirm_bar: Element<'static, Message> = if confirm_pending {
            container(
                text(format!(
                    "Confirm trashing {} marked photos",
                    state.marked_count()
                ))
                .size(12)
                .color(Semantic::DANGER),
            )
            .padding(Padding::from([6, 32]))
            .into()
        } else {
            Space::with_height(Length::Shrink).into()
        };

        container(column![
            header,
            image_area,
            info,
            filmstrip,
            confirm_bar,
            controls,
            hints
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Backgrounds::PRIMARY.into()),
            ..Default::default()
        })
        .into()
    }

    fn render_filmstrip(
        state: &CullState,
        photos: &[Photo],
        drive_path: Option<&Path>,
    ) -> Element<'static, Message> {
        let current = state.current_index;
        let total = state.photo_ids.len();

        let start = current.saturating_sub(4);
        let end = (start + 9).min(total);

        let mut thumbs: Vec<Element<'static, Message>> = Vec::new();

        for i in start..end {
            let photo_id = state.photo_ids[i];
            let is_current = i == current;
            let is_marked = state.marked_for_trash.contains(&photo_id);

            let base_thumb: Element<'static, Message> = if let (Some(root), Some(photo)) =
                (drive_path, photos.iter().find(|p| p.id == photo_id))
            {
                if let Some(ref t) = photo.thumbnail_path {
                    let p = std::path::PathBuf::from(t);
                    if p.exists() {
                        container(
                            iced::widget::image(iced::widget::image::Handle::from_path(p))
                                .width(50)
                                .height(50)
                                .content_fit(iced::ContentFit::Cover),
                        )
                        .width(50)
                        .height(50)
                        .into()
                    } else {
                        let full = root.join(&photo.file_path);
                        if full.exists() {
                            container(
                                iced::widget::image(iced::widget::image::Handle::from_path(full))
                                    .width(50)
                                    .height(50)
                                    .content_fit(iced::ContentFit::Cover),
                            )
                            .width(50)
                            .height(50)
                            .into()
                        } else {
                            container(text(if is_marked { "X" } else { "" }).size(14))
                                .width(50)
                                .height(50)
                                .align_x(iced::alignment::Horizontal::Center)
                                .align_y(iced::alignment::Vertical::Center)
                                .into()
                        }
                    }
                } else {
                    container(text(if is_marked { "X" } else { "" }).size(14))
                        .width(50)
                        .height(50)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .into()
                }
            } else {
                container(text(if is_marked { "X" } else { "" }).size(14))
                    .width(50)
                    .height(50)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .into()
            };

            let thumb = container(base_thumb)
                .width(50)
                .height(50)
                .style(move |_theme| container::Style {
                    background: Some(if is_marked {
                        iced::Color { a: 0.20, ..Semantic::DANGER }.into()
                    } else {
                        Backgrounds::ELEVATED.into()
                    }),
                    border: iced::Border {
                        color: if is_current {
                            Accent::PRIMARY
                        } else if is_marked {
                            Semantic::DANGER
                        } else {
                            Border::SUBTLE
                        },
                        width: if is_current { 2.0 } else { 1.0 },
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });

            thumbs.push(thumb.into());
        }

        container(Row::with_children(thumbs).spacing(8))
            .width(Length::Fill)
            .padding(Padding::from([8, 32]))
            .align_x(iced::alignment::Horizontal::Center)
            .into()
    }
}
