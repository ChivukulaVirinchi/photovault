//! Unified search view — people, albums, places, photos in one grouped result.

use std::path::Path;

use iced::widget::{button, column, container, row, scrollable, text, text_input, Column, Space};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::RecentSearch;
use crate::models::Photo;
use crate::services::{AlbumHit, PersonHit, PlaceHit, SearchResult, UnifiedSearchResults};
use crate::theme::colors::{self, Palette};

/// Search view component.
pub struct SearchView;

impl SearchView {
    pub fn view(
        query: &str,
        results: Option<&UnifiedSearchResults>,
        recent: &[RecentSearch],
        is_loading: bool,
        drive_path: Option<&Path>,
        photos: &[Photo],
        spinner_phase: u32,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;
        let text_primary = p.text_primary;

        let body: Element<'static, Message> = if is_loading {
            loading_state(spinner_phase, theme)
        } else if let Some(res) = results {
            if is_empty_results(res) {
                no_results(query, p)
            } else {
                results_panel(res, None, drive_path, photos, theme)
            }
        } else if query.is_empty() && !recent.is_empty() {
            recent_searches_panel(recent, p)
        } else {
            examples_panel(p)
        };

        let content = column![
            text("Search").size(22).color(text_primary),
            Space::with_height(20),
            search_bar(query, p),
            Space::with_height(14),
            body,
            Space::with_height(32),
        ]
        .padding(Padding::from([0, 32]));

        let scrollable_body = container(
            scrollable(container(content).padding(Padding::default().top(32.0)))
                .id(scrollable::Id::new("search")),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        });

        scrollable_body.into()
    }
}

fn is_empty_results(r: &UnifiedSearchResults) -> bool {
    r.people.is_empty() && r.albums.is_empty() && r.places.is_empty() && r.photos.is_empty()
}

fn search_bar(query: &str, p: &Palette) -> Element<'static, Message> {
    let accent_primary = p.accent_primary;
    let accent_muted = p.accent_muted;
    let bg_primary = p.bg_primary;
    let text_tertiary = p.text_tertiary;
    let bg_hover = p.bg_hover;

    let input = text_input("Search people, albums, places, photos\u{2026}", query)
        .on_input(Message::SearchInputChanged)
        .on_submit(Message::ExecuteSearch)
        .size(14)
        .padding(12)
        .width(Length::Fill);

    let mut items: Vec<Element<'static, Message>> = vec![input.into()];

    if !query.is_empty() {
        items.push(Space::with_width(8).into());
        items.push(
            button(text("\u{00D7}").size(16).color(text_tertiary))
                .padding(Padding::from([10, 14]))
                .style(move |_t, status| button::Style {
                    background: Some(match status {
                        button::Status::Hovered => bg_hover.into(),
                        _ => iced::Background::Color(iced::Color::TRANSPARENT),
                    }),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Message::SearchInputChanged(String::new()))
                .into(),
        );
    }

    items.push(Space::with_width(8).into());
    items.push(
        button(text("Search").size(14).color(bg_primary))
            .padding(Padding::from([12, 20]))
            .style(move |_theme, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => accent_primary.into(),
                    _ => accent_muted.into(),
                }),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::ExecuteSearch)
            .into(),
    );

    row(items).align_y(Alignment::Center).into()
}

fn loading_state(phase: u32, theme: AppTheme) -> Element<'static, Message> {
    container(crate::components::spinner::spinner_with_label(
        phase,
        "Searching",
        theme,
    ))
    .width(Length::Fill)
    .padding(24)
    .into()
}

fn no_results(query: &str, p: &Palette) -> Element<'static, Message> {
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let msg = if query.is_empty() {
        "No matches".to_string()
    } else {
        format!("No matches for \u{201C}{}\u{201D}", query)
    };
    container(
        column![
            text(msg).size(16).color(text_secondary),
            Space::with_height(6),
            text("Try a different query").size(13).color(text_tertiary),
        ]
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(48)
    .into()
}

fn examples_panel(p: &Palette) -> Element<'static, Message> {
    let text_primary = p.text_primary;
    column![
        text("Search Examples").size(16).color(text_primary),
        Space::with_height(12),
        search_tip("Dad", "Find by person", p),
        search_tip("Tokyo", "Find by location", p),
        search_tip("March 2019", "Find by date", p),
        search_tip("Dad in Tokyo", "Combine filters", p),
        search_tip("last summer", "Natural date query", p),
    ]
    .spacing(8)
    .into()
}

fn search_tip(example: &str, description: &str, p: &Palette) -> Element<'static, Message> {
    let text_primary = p.text_primary;
    let text_tertiary = p.text_tertiary;
    let bg_elevated = p.bg_elevated;

    let example = example.to_string();
    let description = description.to_string();
    row![
        container(text(example).size(12).color(text_primary))
            .padding(Padding::from([4, 8]))
            .style(move |_theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        Space::with_width(10),
        text(description).size(12).color(text_tertiary),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn recent_searches_panel(recent: &[RecentSearch], p: &Palette) -> Element<'static, Message> {
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let text_primary = p.text_primary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;

    let mut items: Vec<Element<'static, Message>> = Vec::new();

    items.push(
        row![
            text("Recent searches").size(12).color(text_secondary),
            Space::with_width(Length::Fill),
            button(text("Clear all").size(11).color(text_tertiary))
                .padding(Padding::from([4, 8]))
                .style(move |_t, status| button::Style {
                    background: Some(match status {
                        button::Status::Hovered => bg_hover.into(),
                        _ => iced::Background::Color(iced::Color::TRANSPARENT),
                    }),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Message::SearchClearRecent),
        ]
        .align_y(Alignment::Center)
        .into(),
    );
    items.push(Space::with_height(8).into());

    for r in recent {
        let q = r.query.clone();
        let q_for_remove = r.query.clone();
        let label = r.query.clone();

        let row_el = row![
            button(
                row![
                    text("\u{1F55B}").size(12).color(text_tertiary),
                    Space::with_width(8),
                    text(label).size(13).color(text_primary),
                ]
                .align_y(Alignment::Center)
            )
            .padding(Padding::from([8, 12]))
            .width(Length::Fill)
            .style(move |_t, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => bg_hover.into(),
                    _ => bg_elevated.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::SearchRecentSelected(q)),
            Space::with_width(4),
            button(text("\u{00D7}").size(13).color(text_tertiary))
                .padding(Padding::from([8, 10]))
                .style(move |_t, status| button::Style {
                    background: Some(match status {
                        button::Status::Hovered => bg_hover.into(),
                        _ => bg_elevated.into(),
                    }),
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Message::SearchRecentRemove(q_for_remove)),
        ]
        .spacing(0)
        .align_y(Alignment::Center);

        items.push(row_el.into());
    }

    Column::with_children(items).spacing(6).into()
}

fn results_panel(
    results: &UnifiedSearchResults,
    highlighted: Option<usize>,
    drive_path: Option<&Path>,
    photos: &[Photo],
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let mut sections: Vec<Element<'static, Message>> = Vec::new();

    // Tracks the running offset of the highlighted index across sections.
    let mut cursor_offset: usize = 0;

    // --- People ---
    if !results.people.is_empty() {
        sections.push(section_header("People", results.people.len(), p));
        sections.push(Space::with_height(6).into());
        for (i, hit) in results.people.iter().enumerate() {
            let idx = cursor_offset + i;
            let is_hl = highlighted == Some(idx);
            sections.push(person_row(hit, is_hl, p));
        }
        cursor_offset += results.people.len();
        sections.push(Space::with_height(20).into());
    }

    // --- Albums ---
    if !results.albums.is_empty() {
        sections.push(section_header("Albums", results.albums.len(), p));
        sections.push(Space::with_height(6).into());
        for (i, hit) in results.albums.iter().enumerate() {
            let idx = cursor_offset + i;
            let is_hl = highlighted == Some(idx);
            sections.push(album_row(hit, is_hl, p));
        }
        cursor_offset += results.albums.len();
        sections.push(Space::with_height(20).into());
    }

    // --- Places ---
    if !results.places.is_empty() {
        sections.push(section_header("Places", results.places.len(), p));
        sections.push(Space::with_height(6).into());
        for (i, hit) in results.places.iter().enumerate() {
            let idx = cursor_offset + i;
            let is_hl = highlighted == Some(idx);
            sections.push(place_row(hit, is_hl, p));
        }
        cursor_offset += results.places.len();
        sections.push(Space::with_height(20).into());
    }

    // --- Photos ---
    if !results.photos.is_empty() {
        sections.push(section_header("Photos", results.photos.len(), p));
        sections.push(Space::with_height(6).into());
        sections.push(photos_section(&results.photos, drive_path, photos, theme));
        sections.push(Space::with_height(12).into());
        sections.push(cull_bar(results.photos.len(), p));
    }

    // Suppress unused warning for cursor_offset (it's intentionally used above;
    // this keeps rust happy if some branches are skipped).
    let _ = cursor_offset;

    Column::with_children(sections).spacing(0).into()
}

fn section_header(label: &str, count: usize, p: &Palette) -> Element<'static, Message> {
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let label = label.to_string();
    row![
        text(label).size(12).color(text_secondary),
        Space::with_width(8),
        text(format!("({})", count)).size(11).color(text_tertiary),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn person_row(hit: &PersonHit, is_highlighted: bool, p: &Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let bg_selected = p.bg_selected;
    let text_primary = p.text_primary;
    let text_tertiary = p.text_tertiary;
    let text_secondary = p.text_secondary;

    let face: Element<'static, Message> = if let Some(ref path) = hit.face_thumbnail_path {
        let path = path.clone();
        container(
            iced::widget::image(iced::widget::image::Handle::from_path(path))
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(40.0))
                .content_fit(ContentFit::Cover),
        )
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(40.0))
        .style(move |_t| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                radius: 20.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        let initial: String = hit
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_else(|| "?".to_string());
        container(text(initial).size(16).color(text_secondary))
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(40.0))
            .center_x(Length::Fixed(40.0))
            .center_y(Length::Fixed(40.0))
            .style(move |_t| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 20.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    };

    let bg = if is_highlighted {
        bg_selected
    } else {
        bg_elevated
    };
    let cluster_id = hit.cluster_id;
    let name = hit.name.clone();
    let count = hit.photo_count;

    button(
        row![
            face,
            Space::with_width(12),
            column![
                text(name).size(14).color(text_primary),
                text(format!("{} photos", count))
                    .size(11)
                    .color(text_tertiary),
            ]
            .spacing(2),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 12]))
    .width(Length::Fill)
    .style(move |_t, status| button::Style {
        background: Some(match status {
            button::Status::Hovered => bg_hover.into(),
            _ => bg.into(),
        }),
        border: iced::Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Message::SearchOpenPerson(cluster_id))
    .into()
}

fn album_row(hit: &AlbumHit, is_highlighted: bool, p: &Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let bg_selected = p.bg_selected;
    let text_primary = p.text_primary;
    let text_tertiary = p.text_tertiary;
    let text_secondary = p.text_secondary;

    let cover: Element<'static, Message> = if let Some(ref path) = hit.cover_thumbnail_path {
        let path = path.clone();
        container(
            iced::widget::image(iced::widget::image::Handle::from_path(path))
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(40.0))
                .content_fit(ContentFit::Cover),
        )
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(40.0))
        .style(move |_t| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    } else {
        container(text("\u{1F4C1}").size(18).color(text_secondary))
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(40.0))
            .center_x(Length::Fixed(40.0))
            .center_y(Length::Fixed(40.0))
            .style(move |_t| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    };

    let bg = if is_highlighted {
        bg_selected
    } else {
        bg_elevated
    };
    let album_id = hit.album_id;
    let name = hit.name.clone();
    let count = hit.photo_count;

    button(
        row![
            cover,
            Space::with_width(12),
            column![
                text(name).size(14).color(text_primary),
                text(format!("{} photos", count))
                    .size(11)
                    .color(text_tertiary),
            ]
            .spacing(2),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 12]))
    .width(Length::Fill)
    .style(move |_t, status| button::Style {
        background: Some(match status {
            button::Status::Hovered => bg_hover.into(),
            _ => bg.into(),
        }),
        border: iced::Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Message::SearchOpenAlbum(album_id))
    .into()
}

fn place_row(hit: &PlaceHit, is_highlighted: bool, p: &Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let bg_selected = p.bg_selected;
    let text_primary = p.text_primary;
    let text_tertiary = p.text_tertiary;
    let text_secondary = p.text_secondary;

    let pin: Element<'static, Message> =
        container(text("\u{1F4CD}").size(18).color(text_secondary))
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(40.0))
            .center_x(Length::Fixed(40.0))
            .center_y(Length::Fixed(40.0))
            .style(move |_t| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    radius: 20.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into();

    let bg = if is_highlighted {
        bg_selected
    } else {
        bg_elevated
    };
    let city = hit.city.clone();
    let city_for_click = hit.city.clone();
    let sub = match hit.country.clone() {
        Some(c) => format!("{} \u{2022} {} photos", c, hit.photo_count),
        None => format!("{} photos", hit.photo_count),
    };

    button(
        row![
            pin,
            Space::with_width(12),
            column![
                text(city).size(14).color(text_primary),
                text(sub).size(11).color(text_tertiary),
            ]
            .spacing(2),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 12]))
    .width(Length::Fill)
    .style(move |_t, status| button::Style {
        background: Some(match status {
            button::Status::Hovered => bg_hover.into(),
            _ => bg.into(),
        }),
        border: iced::Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .on_press(Message::SearchOpenPlace(city_for_click))
    .into()
}

fn photos_section(
    photos: &[SearchResult],
    drive_path: Option<&Path>,
    photos_full: &[Photo],
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    let mut items: Vec<Element<'static, Message>> = Vec::new();
    for r in photos.iter().take(16) {
        let thumb = thumb_for(r.photo_id, drive_path, photos_full, theme);
        let pid = r.photo_id;
        items.push(
            button(thumb)
                .padding(0)
                .style(|_theme, _status| button::Style::default())
                .on_press(Message::SelectPhoto(pid))
                .into(),
        );
    }

    let grid = row(items).spacing(8).wrap();

    if photos.len() > 16 {
        let total = photos.len();
        let text_secondary = p.text_secondary;
        let bg_hover = p.bg_hover;
        let bg_elevated = p.bg_elevated;
        column![
            grid,
            Space::with_height(10),
            button(
                text(format!("Show all {} photos", total))
                    .size(12)
                    .color(text_secondary),
            )
            .padding(Padding::from([6, 14]))
            .style(move |_t, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => bg_hover.into(),
                    _ => bg_elevated.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::EnterCullFromSearch),
        ]
        .into()
    } else {
        grid.into()
    }
}

fn thumb_for(
    photo_id: i64,
    drive_path: Option<&Path>,
    photos_full: &[Photo],
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let bg_elevated = p.bg_elevated;

    if let (Some(root), Some(photo)) = (drive_path, photos_full.iter().find(|ph| ph.id == photo_id))
    {
        if let Some(ref t) = photo.thumbnail_path {
            let tp = std::path::PathBuf::from(t);
            if tp.exists() {
                return thumb_image(tp, bg_elevated);
            }
            let full = root.join(&photo.file_path);
            if full.exists() {
                return thumb_image(full, bg_elevated);
            }
        } else {
            let full = root.join(&photo.file_path);
            if full.exists() {
                return thumb_image(full, bg_elevated);
            }
        }
    }
    thumb_placeholder(theme)
}

fn thumb_image(path: std::path::PathBuf, bg_elevated: iced::Color) -> Element<'static, Message> {
    container(
        iced::widget::image(iced::widget::image::Handle::from_path(path))
            .width(Length::Fixed(80.0))
            .height(Length::Fixed(80.0))
            .content_fit(ContentFit::Cover),
    )
    .width(Length::Fixed(80.0))
    .height(Length::Fixed(80.0))
    .style(move |_theme| container::Style {
        background: Some(bg_elevated.into()),
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn thumb_placeholder(theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let bg_elevated = p.bg_elevated;
    let text_tertiary = p.text_tertiary;
    container(text("IMG").size(10).color(text_tertiary))
        .width(Length::Fixed(80.0))
        .height(Length::Fixed(80.0))
        .center_x(Length::Fixed(80.0))
        .center_y(Length::Fixed(80.0))
        .style(move |_theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn cull_bar(total: usize, p: &Palette) -> Element<'static, Message> {
    let accent_primary = p.accent_primary;
    let accent_muted = p.accent_muted;
    let bg_primary = p.bg_primary;
    let text_secondary = p.text_secondary;
    row![
        text(format!("{} photos matched", total))
            .size(12)
            .color(text_secondary),
        Space::with_width(Length::Fill),
        button(text("Quick Cull").size(13).color(bg_primary))
            .padding(Padding::from([8, 14]))
            .style(move |_theme, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered => accent_primary.into(),
                    _ => accent_muted.into(),
                }),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::EnterCullFromSearch),
    ]
    .align_y(Alignment::Center)
    .into()
}
