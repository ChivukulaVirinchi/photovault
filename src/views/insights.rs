//! Insights Dashboard — aggregate library statistics view.
//!
//! Seven sections, all wrapped in a single scrollable column:
//! 1. Year selector pills
//! 2. Stat cards (Photos, People, Albums, Countries, Cities, Date range)
//! 3. Hero banner image
//! 4. Activity heatmap (53x7 grid, day-level)
//! 5. Monthly bars (Jan-Dec)
//! 6. Top people (face crop + name + count)
//! 7. Camera breakdown (horizontal bars)

use chrono::Datelike;
use iced::widget::{
    button, column, container, image as iced_image, row, scrollable, text, Space,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::insights::InsightsData;
use crate::theme::colors;

// ---------------------------------------------------------------------------
// Main dashboard view
// ---------------------------------------------------------------------------

pub fn insights_view(
    data: Option<&InsightsData>,
    selected_year: Option<i32>,
    loading: bool,
    spinner_phase: u32,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);

    if loading || data.is_none() {
        let status: Element<'static, Message> = if loading {
            crate::components::spinner::spinner_with_label(
                spinner_phase,
                "Computing insights",
                theme,
            )
        } else {
            text("No insights data yet.").size(14).color(p.text_secondary).into()
        };
        return container(
            column![
                text("Insights").size(24).color(p.text_primary),
                Space::with_height(20),
                status,
            ]
            .spacing(8)
            .padding(Padding::from([24, 24])),
        )
        .width(Length::Fill)
        .into();
    }

    let data = data.unwrap();

    let mut content = column![].spacing(20).padding(Padding::from([24, 24]));

    // Header
    content = content.push(text("Insights").size(24).color(p.text_primary));

    // 1. Year selector
    content = content.push(year_selector(data, selected_year, p));

    // 2. Stat cards
    content = content.push(stat_cards(data, p));

    // 3. Hero banner
    if data.hero_thumbnail_path.is_some() {
        content = content.push(hero_banner(data, p));
    }

    // 4. Activity heatmap
    content = content.push(section_header("Activity", p));
    content = content.push(activity_heatmap(data, p));

    // 5. Monthly bars
    content = content.push(section_header("Monthly Breakdown", p));
    content = content.push(monthly_bars(data, p));

    // 6. Top people
    if !data.top_people.is_empty() {
        content = content.push(section_header("Top People", p));
        content = content.push(top_people(data, p));
    }

    // 7. Top locations
    if !data.top_locations.is_empty() {
        content = content.push(section_header("Top Locations", p));
        content = content.push(top_locations(data, p));
    }

    // 8. Camera breakdown (only if >1 camera)
    if data.top_cameras.len() > 1 {
        content = content.push(section_header("Cameras", p));
        content = content.push(camera_breakdown(data, p));
    }

    // Bottom spacer
    content = content.push(Space::with_height(40));

    scrollable(content)
        .width(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Section header
// ---------------------------------------------------------------------------

fn section_header(title: &str, p: &colors::Palette) -> Element<'static, Message> {
    text(title.to_owned())
        .size(18)
        .color(p.text_primary)
        .into()
}

// ---------------------------------------------------------------------------
// 1. Year selector — horizontal scrollable row of pill buttons
// ---------------------------------------------------------------------------

fn year_selector(
    data: &InsightsData,
    selected_year: Option<i32>,
    p: &colors::Palette,
) -> Element<'static, Message> {
    let mut pills = row![].spacing(8);

    // "All Time" pill
    pills = pills.push(year_pill("All Time", None, selected_year, p));

    // Year pills (ascending)
    let mut years = data.available_years.clone();
    years.sort();
    for y in years {
        pills = pills.push(year_pill(&y.to_string(), Some(y), selected_year, p));
    }

    scrollable(pills)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4),
        ))
        .width(Length::Fill)
        .into()
}

fn year_pill(
    label: &str,
    year: Option<i32>,
    selected: Option<i32>,
    p: &colors::Palette,
) -> Element<'static, Message> {
    let is_selected = year == selected;
    let accent = p.accent_primary;
    let bg_elevated = p.bg_elevated;
    let bg_hover = p.bg_hover;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;

    let label_color = if is_selected {
        iced::Color::WHITE
    } else {
        text_secondary
    };

    let pill = button(
        container(text(label.to_owned()).size(13).color(label_color))
            .padding(Padding::from([6, 16])),
    )
    .padding(0)
    .style(move |_theme: &iced::Theme, status| {
        let bg = if is_selected {
            accent
        } else {
            match status {
                button::Status::Hovered => bg_hover,
                _ => bg_elevated,
            }
        };
        button::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 16.0.into(),
                ..Default::default()
            },
            text_color: if is_selected {
                iced::Color::WHITE
            } else {
                text_primary
            },
            ..Default::default()
        }
    })
    .on_press(Message::InsightsSelectYear(year));

    pill.into()
}

// ---------------------------------------------------------------------------
// 2. Stat cards — horizontal row of 6 cards
// ---------------------------------------------------------------------------

fn stat_cards(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let date_range = match (&data.date_range_start, &data.date_range_end) {
        (Some(start), Some(end)) => {
            let s = start.get(..10).unwrap_or(start);
            let e = end.get(..10).unwrap_or(end);
            format!("{} - {}", s, e)
        }
        _ => "--".to_string(),
    };

    let cards = row![
        stat_card(&format_number(data.total_photos), "Photos", p),
        stat_card(&format_number(data.people_count), "People", p),
        stat_card(&format_number(data.album_count), "Albums", p),
        stat_card(&format_number(data.country_count), "Countries", p),
        stat_card(&format_number(data.city_count), "Cities", p),
        stat_card(&date_range, "Date Range", p),
    ]
    .spacing(12);

    scrollable(cards)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4),
        ))
        .width(Length::Fill)
        .into()
}

fn stat_card(
    value: &str,
    label: &str,
    p: &colors::Palette,
) -> Element<'static, Message> {
    let bg = p.bg_elevated;
    let border_color = p.border_subtle;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;

    container(
        column![
            text(value.to_owned()).size(24).color(text_primary),
            Space::with_height(4),
            text(label.to_owned()).size(11).color(text_secondary),
        ]
        .align_x(Alignment::Center),
    )
    .padding(Padding::from([16, 20]))
    .width(Length::Fixed(140.0))
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(bg.into()),
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Format a number with comma separators for readability.
fn format_number(n: i64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

// ---------------------------------------------------------------------------
// 3. Hero banner
// ---------------------------------------------------------------------------

fn hero_banner(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let bg = p.bg_elevated;
    let border_color = p.border_subtle;

    if let Some(ref path) = data.hero_thumbnail_path {
        container(
            iced_image(path.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fixed(280.0)),
        )
        .width(Length::Fill)
        .clip(true)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                color: border_color,
                width: 0.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
    } else {
        Space::new(0, 0).into()
    }
}

// ---------------------------------------------------------------------------
// 4. Activity heatmap — 53 cols x 7 rows
// ---------------------------------------------------------------------------

fn activity_heatmap(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let year = data.heatmap_year;

    // Find max count for colour scaling
    let max_count = data.heatmap.values().copied().max().unwrap_or(1).max(1);

    // Month labels row
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Build the month label row. We need to figure out which column each
    // month starts in. The heatmap starts on the first day of the year's
    // first ISO week that overlaps with this year (roughly Jan 1).
    // For simplicity, we compute the column offset for the 1st of each month.
    let jan1 = chrono::NaiveDate::from_ymd_opt(year, 1, 1);

    let mut month_label_row = row![Space::with_width(28)]; // offset for day labels
    if let Some(jan1_date) = jan1 {
        let jan1_weekday = jan1_date.weekday().num_days_from_monday() as i32; // Mon=0
        let mut last_col: i32 = -1;
        for m in 1..=12u32 {
            if let Some(first_of_month) = chrono::NaiveDate::from_ymd_opt(year, m, 1) {
                let day_of_year = first_of_month.ordinal0() as i32;
                let col = (day_of_year + jan1_weekday) / 7;
                let gap = col - last_col - 1;
                if gap > 0 {
                    month_label_row = month_label_row
                        .push(Space::with_width(Length::Fixed((gap as f32) * 14.0)));
                }
                month_label_row = month_label_row.push(
                    container(
                        text(month_names[(m - 1) as usize])
                            .size(9)
                            .color(p.text_tertiary),
                    )
                    .width(Length::Fixed(14.0)),
                );
                last_col = col;
            }
        }
    }

    // Day labels (left column)
    let day_labels = ["", "M", "", "W", "", "F", ""];

    // Build the 53x7 grid. Each column is a week.
    // Row 0 = Monday, Row 6 = Sunday.
    let jan1_date = jan1.unwrap_or(chrono::NaiveDate::from_ymd_opt(year, 1, 1).unwrap());
    let jan1_weekday = jan1_date.weekday().num_days_from_monday() as usize;

    // We lay out as rows (7) of columns (53).
    let mut grid_rows: Vec<Element<'static, Message>> = Vec::new();

    let accent = p.accent_primary;
    let bg_elevated = p.bg_elevated;
    let text_tertiary = p.text_tertiary;

    for day_of_week in 0..7usize {
        let mut week_row = row![
            container(
                text(day_labels[day_of_week].to_string())
                    .size(9)
                    .color(text_tertiary),
            )
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(14.0))
        ];

        for week in 0..53usize {
            let day_index = week * 7 + day_of_week;
            // Convert to actual date
            let offset_from_jan1 = day_index as i32 - jan1_weekday as i32;
            let date_opt = jan1_date.checked_add_signed(chrono::Duration::days(offset_from_jan1 as i64));

            let (count, date_str) = if let Some(date) = date_opt {
                if date.year() == year {
                    let key = date.format("%Y-%m-%d").to_string();
                    let c = data.heatmap.get(&key).copied().unwrap_or(0);
                    (c, Some(key))
                } else {
                    (-1, None) // out of year range
                }
            } else {
                (-1, None)
            };

            if count < 0 {
                // Empty placeholder cell
                week_row = week_row.push(
                    Space::new(Length::Fixed(14.0), Length::Fixed(14.0))
                );
            } else {
                let intensity = if count == 0 {
                    0.0f32
                } else {
                    // Map count -> 0.15 .. 1.0
                    let frac = (count as f32) / (max_count as f32);
                    0.15 + frac * 0.85
                };

                let cell_color = if count == 0 {
                    bg_elevated
                } else {
                    iced::Color {
                        r: accent.r,
                        g: accent.g,
                        b: accent.b,
                        a: intensity,
                    }
                };

                let cell = container(Space::new(Length::Fixed(12.0), Length::Fixed(12.0)))
                    .width(Length::Fixed(14.0))
                    .height(Length::Fixed(14.0))
                    .center_x(Length::Fixed(14.0))
                    .center_y(Length::Fixed(14.0))
                    .style(move |_theme: &iced::Theme| container::Style {
                        background: Some(cell_color.into()),
                        border: iced::Border {
                            radius: 2.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                if let Some(ds) = date_str {
                    if count > 0 {
                        let ds_clone = ds.clone();
                        week_row = week_row.push(
                            button(cell)
                                .padding(0)
                                .style(move |_theme: &iced::Theme, _status| button::Style {
                                    background: None,
                                    ..Default::default()
                                })
                                .on_press(Message::InsightsJumpToDate(ds_clone)),
                        );
                    } else {
                        week_row = week_row.push(cell);
                    }
                } else {
                    week_row = week_row.push(cell);
                }
            }
        }

        grid_rows.push(week_row.spacing(0).into());
    }

    let mut grid = column![].spacing(0);
    grid = grid.push(month_label_row);
    for r in grid_rows {
        grid = grid.push(r);
    }

    let bg_card = p.bg_elevated;
    let border_card = p.border_subtle;

    let heatmap_container = container(grid.padding(Padding::from([8, 8])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_card.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

    scrollable(heatmap_container)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4),
        ))
        .width(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// 5. Monthly bars — 12 horizontal bars
// ---------------------------------------------------------------------------

fn monthly_bars(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let max_count = data.monthly_counts.iter().copied().max().unwrap_or(1).max(1);
    let accent = p.accent_primary;
    let bg_elevated = p.bg_elevated;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let border_card = p.border_subtle;

    let mut bars = column![].spacing(6);

    for (i, &count) in data.monthly_counts.iter().enumerate() {
        let bar_width = if max_count > 0 && count > 0 {
            (count as f32 / max_count as f32 * 300.0).max(4.0)
        } else {
            4.0
        };

        let bar_color = if count > 0 { accent } else { bg_elevated };

        let bar = container(Space::new(
            Length::Fixed(bar_width),
            Length::Fixed(16.0),
        ))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bar_color.into()),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let count_label = if count > 0 {
            format_number(count)
        } else {
            String::new()
        };

        let bar_row = row![
            container(
                text(month_names[i].to_string())
                    .size(12)
                    .color(text_secondary),
            )
            .width(Length::Fixed(36.0)),
            bar,
            Space::with_width(8),
            text(count_label).size(11).color(text_tertiary),
        ]
        .align_y(Alignment::Center)
        .spacing(4);

        bars = bars.push(bar_row);
    }

    container(bars.padding(Padding::from([12, 16])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 6. Top people — face crop + name + count
// ---------------------------------------------------------------------------

fn top_people(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let border_card = p.border_subtle;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let bg_hover = p.bg_hover;

    let mut people_col = column![].spacing(8);

    for person in &data.top_people {
        let face: Element<'static, Message> = if let Some(ref path) = person.face_crop_path {
            container(
                iced_image(path.clone())
                    .content_fit(ContentFit::Cover)
                    .width(Length::Fixed(48.0))
                    .height(Length::Fixed(48.0)),
            )
            .width(Length::Fixed(48.0))
            .height(Length::Fixed(48.0))
            .clip(true)
            .style(move |_theme: &iced::Theme| container::Style {
                border: iced::Border {
                    radius: 24.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            container(Space::new(48, 48))
                .width(Length::Fixed(48.0))
                .height(Length::Fixed(48.0))
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg_hover.into()),
                    border: iced::Border {
                        radius: 24.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

        let name = person.name.clone();
        let count_str = format!("{} photos", person.photo_count);
        let cluster_id = person.cluster_id;

        let person_row = button(
            row![
                face,
                Space::with_width(12),
                column![
                    text(name).size(14).color(text_primary),
                    text(count_str).size(11).color(text_secondary),
                ]
                .spacing(2),
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([4, 8])),
        )
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => bg_hover,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectCluster(cluster_id));

        people_col = people_col.push(person_row);
    }

    container(people_col.padding(Padding::from([12, 12])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 7. Top locations — clickable rows
// ---------------------------------------------------------------------------

fn top_locations(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let bg_elevated = p.bg_elevated;
    let border_card = p.border_subtle;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;
    let bg_hover = p.bg_hover;

    let mut loc_col = column![].spacing(6);

    for loc in &data.top_locations {
        let label = if loc.country.is_empty() {
            loc.city.clone()
        } else {
            format!("{}, {}", loc.city, loc.country)
        };
        let count_str = format!("{} photos", loc.photo_count);
        let city_clone = loc.city.clone();

        let loc_row = button(
            row![
                column![
                    text(label).size(14).color(text_primary),
                    text(count_str).size(11).color(text_secondary),
                ]
                .spacing(2),
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([6, 12])),
        )
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => bg_hover,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::InsightsSearchCity(city_clone));

        loc_col = loc_col.push(loc_row);
    }

    container(loc_col.padding(Padding::from([12, 12])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 8. Camera breakdown — horizontal bars
// ---------------------------------------------------------------------------

fn camera_breakdown(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let max_count = data
        .top_cameras
        .iter()
        .map(|c| c.photo_count)
        .max()
        .unwrap_or(1)
        .max(1);

    let accent = p.accent_primary;
    let bg_elevated = p.bg_elevated;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let border_card = p.border_subtle;

    let mut bars = column![].spacing(6);

    for cam in &data.top_cameras {
        let bar_width = (cam.photo_count as f32 / max_count as f32 * 260.0).max(4.0);

        let bar_color = accent;
        let bar = container(Space::new(
            Length::Fixed(bar_width),
            Length::Fixed(16.0),
        ))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bar_color.into()),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let bar_row = row![
            container(
                text(cam.camera.clone()).size(12).color(text_secondary),
            )
            .width(Length::Fixed(160.0)),
            bar,
            Space::with_width(8),
            text(format_number(cam.photo_count))
                .size(11)
                .color(text_tertiary),
        ]
        .align_y(Alignment::Center)
        .spacing(4);

        bars = bars.push(bar_row);
    }

    container(bars.padding(Padding::from([12, 16])))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_elevated.into()),
            border: iced::Border {
                color: border_card,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}
