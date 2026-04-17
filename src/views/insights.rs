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
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::tooltip::with_tooltip;
use crate::config::AppTheme;
use crate::services::insights::InsightsData;
use crate::theme::colors;
use crate::views::insights_sections as sections;

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
            text("No insights data yet.")
                .size(14)
                .color(p.text_secondary)
                .into()
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
    content = content.push(section_header("Your photo rhythm", p));
    content = content.push(activity_heatmap(data, p));

    // 5. Monthly bars
    content = content.push(section_header("Moments by month", p));
    content = content.push(monthly_bars(data, p));

    // 6. Top people
    if !data.top_people.is_empty() {
        content = content.push(section_header("Most photographed people", p));
        content = content.push(sections::top_people(data, p));
    }

    // 7. Top locations
    if !data.top_locations.is_empty() {
        content = content.push(section_header("Places that shaped your story", p));
        content = content.push(sections::top_locations(data, p));
    }

    // 8. Camera breakdown (show when at least one camera is present)
    if !data.top_cameras.is_empty() {
        content = content.push(section_header("Cameras behind these memories", p));
        content = content.push(sections::camera_breakdown(data, p));
    }

    content = content.push(
        text("A quick look at the moments and people you photographed most.")
            .size(12)
            .color(p.text_tertiary),
    );

    // Bottom spacer
    content = content.push(Space::with_height(40));

    scrollable(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Section header
// ---------------------------------------------------------------------------

fn section_header(title: &str, p: &colors::Palette) -> Element<'static, Message> {
    text(title.to_owned()).size(18).color(p.text_primary).into()
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
        (Some(start), Some(end)) => format!(
            "{} to {}",
            friendly_date_long(start).unwrap_or_else(|| "--".to_string()),
            friendly_date_long(end).unwrap_or_else(|| "--".to_string())
        ),
        _ => "--".to_string(),
    };

    let cards = row![
        stat_card(&format_number(data.total_photos), "Memories captured", p),
        stat_card(&format_number(data.people_count), "People you kept", p),
        stat_card(&format_number(data.album_count), "Albums created", p),
        stat_card(&format_number(data.country_count), "Countries visited", p),
        stat_card(&format_number(data.city_count), "Cities explored", p),
        timeline_stat_card(&date_range, "Your timeline", p),
    ]
    .spacing(12);

    scrollable(cards)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4),
        ))
        .width(Length::Fill)
        .into()
}

fn stat_card(value: &str, label: &str, p: &colors::Palette) -> Element<'static, Message> {
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

fn timeline_stat_card(value: &str, label: &str, p: &colors::Palette) -> Element<'static, Message> {
    let bg = p.bg_elevated;
    let border_color = p.border_subtle;
    let text_primary = p.text_primary;
    let text_secondary = p.text_secondary;

    container(
        column![
            text(value.to_owned())
                .size(18)
                .wrapping(iced::widget::text::Wrapping::None)
                .width(Length::Fill)
                .color(text_primary),
            Space::with_height(4),
            text(label.to_owned()).size(11).color(text_secondary),
        ]
        .align_x(Alignment::Center)
        .width(Length::Fill),
    )
    .padding(Padding::from([16, 20]))
    .width(Length::Fixed(340.0))
    .height(Length::Fixed(95.0))
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
        let handle = iced::widget::image::Handle::from_path(path.clone());
        container(
            iced::widget::image::viewer(handle)
                .min_scale(1.0)
                .max_scale(1.0)
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
                    month_label_row =
                        month_label_row.push(Space::with_width(Length::Fixed((gap as f32) * 14.0)));
                }
                month_label_row = month_label_row.push(
                    container(
                        text(month_names[(m - 1) as usize])
                            .wrapping(iced::widget::text::Wrapping::None)
                            .size(9)
                            .color(p.text_tertiary),
                    )
                    .width(Length::Fixed(18.0)),
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
    let text_tertiary = p.text_tertiary;

    for day_of_week in 0..7usize {
        let mut week_row = row![container(
            text(day_labels[day_of_week].to_string())
                .size(9)
                .color(text_tertiary),
        )
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(14.0))];

        for week in 0..53usize {
            let day_index = week * 7 + day_of_week;
            // Convert to actual date
            let offset_from_jan1 = day_index as i32 - jan1_weekday as i32;
            let date_opt =
                jan1_date.checked_add_signed(chrono::Duration::days(offset_from_jan1 as i64));

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
                week_row = week_row.push(Space::new(Length::Fixed(14.0), Length::Fixed(14.0)));
            } else {
                let intensity = if count == 0 {
                    0.0f32
                } else {
                    // Map count -> 0.30 .. 1.0 for clearer differentiation.
                    let frac = (count as f32) / (max_count as f32);
                    0.30 + frac * 0.70
                };

                let cell_color = if count == 0 {
                    p.bg_hover
                } else {
                    iced::Color {
                        r: accent.r,
                        g: accent.g,
                        b: accent.b,
                        a: intensity,
                    }
                };

                let cell = container(Space::new(Length::Fixed(11.0), Length::Fixed(11.0)))
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
                    let tooltip_label = if count > 0 {
                        format!(
                            "{} · {} {}",
                            friendly_date_short(&ds).unwrap_or(ds.clone()),
                            count,
                            if count == 1 { "Photo" } else { "Photos" }
                        )
                    } else {
                        format!(
                            "{} · 0 Photos",
                            friendly_date_short(&ds).unwrap_or(ds.clone())
                        )
                    };
                    if count > 0 {
                        let ds_clone = ds.clone();
                        week_row = week_row.push(with_tooltip(
                            button(cell)
                                .padding(0)
                                .style(move |_theme: &iced::Theme, _status| button::Style {
                                    background: None,
                                    ..Default::default()
                                })
                                .on_press(Message::InsightsJumpToDate(ds_clone))
                                .into(),
                            tooltip_label,
                        ));
                    } else {
                        week_row = week_row.push(with_tooltip(cell.into(), tooltip_label));
                    }
                } else {
                    week_row = week_row.push(cell);
                }
            }
        }

        grid_rows.push(week_row.spacing(1.0).into());
    }

    let mut grid = column![].spacing(0);
    grid = grid.push(month_label_row);
    for r in grid_rows {
        grid = grid.push(r);
    }

    let bg_card = p.bg_elevated;
    let border_card = p.border_subtle;

    let heatmap_container =
        container(grid.padding(Padding::from([9, 9]))).style(move |_theme: &iced::Theme| {
            container::Style {
                background: Some(bg_card.into()),
                border: iced::Border {
                    color: border_card,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        });

    scrollable(heatmap_container)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4),
        ))
        .width(Length::Fill)
        .into()
}

fn friendly_date_short(input: &str) -> Option<String> {
    let raw = input.get(..10).unwrap_or(input);
    let d = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    let month = match d.month() {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => return None,
    };
    Some(format!("{} {}", month, ordinal(d.day())))
}

fn friendly_date_long(input: &str) -> Option<String> {
    let raw = input.get(..10).unwrap_or(input);
    let d = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    let month = match d.month() {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => return None,
    };
    Some(format!("{} {} {}", ordinal(d.day()), month, d.year()))
}

fn ordinal(day: u32) -> String {
    let suffix = match day % 100 {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{}{}", day, suffix)
}

// ---------------------------------------------------------------------------
// 5. Monthly bars — 12 horizontal bars
// ---------------------------------------------------------------------------

fn monthly_bars(data: &InsightsData, p: &colors::Palette) -> Element<'static, Message> {
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let max_count = data
        .monthly_counts
        .iter()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    let accent = p.accent_primary;
    let bg_elevated = p.bg_elevated;
    let text_secondary = p.text_secondary;
    let text_tertiary = p.text_tertiary;
    let border_card = p.border_subtle;
    let row_hover = p.bg_hover;

    let mut bars = column![].spacing(6);

    for (i, &count) in data.monthly_counts.iter().enumerate() {
        let bar_width = if max_count > 0 && count > 0 {
            (count as f32 / max_count as f32 * 300.0).max(4.0)
        } else {
            4.0
        };

        let bar_color = if count > 0 { accent } else { bg_elevated };

        let bar = container(Space::new(Length::Fixed(bar_width), Length::Fixed(16.0))).style(
            move |_theme: &iced::Theme| container::Style {
                background: Some(bar_color.into()),
                border: iced::Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

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

        if count > 0 {
            bars = bars.push(
                button(bar_row)
                    .padding(0)
                    .width(Length::Fill)
                    .style(move |_theme: &iced::Theme, status| button::Style {
                        background: match status {
                            button::Status::Hovered => Some(row_hover.into()),
                            _ => None,
                        },
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(Message::InsightsOpenMonth {
                        year: data.heatmap_year,
                        month: (i + 1) as u32,
                    }),
            );
        } else {
            bars = bars.push(bar_row);
        }
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

// moved to views/insights_sections.rs

// ---------------------------------------------------------------------------
// 7. Top locations — clickable rows
// ---------------------------------------------------------------------------

// moved to views/insights_sections.rs

// ---------------------------------------------------------------------------
// 8. Camera breakdown — horizontal bars
// ---------------------------------------------------------------------------

// moved to views/insights_sections.rs
