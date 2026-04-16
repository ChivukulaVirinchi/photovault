//! Documents view.

use chrono::{DateTime, Datelike, Utc};
use std::collections::{BTreeMap, HashSet};

use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::components::photo_grid::{day_header, photo_grid_simple};
use crate::config::AppTheme;
use crate::models::{ContentCategory, Photo};
use crate::theme::colors;

pub struct DocumentsView;

impl DocumentsView {
    pub fn view(
        photos: &[Photo],
        query: &str,
        selected_category: Option<ContentCategory>,
        selected_photo_ids: &HashSet<i64>,
        columns: usize,
        hovered_photo_id: Option<i64>,
        hovered_day_key: Option<&str>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);

        let title = text("Documents").size(28).color(p.text_primary);
        let subtitle = text(format!("{} items", photos.len()))
            .size(14)
            .color(p.text_secondary);

        let search = text_input("Search extracted text...", query)
            .on_input(Message::DocumentsSearchChanged)
            .on_submit(Message::LoadDocuments)
            .size(14)
            .padding(10)
            .width(Length::Fill);

        let filter_row = row![
            Self::filter_chip("All", None, selected_category, theme),
            Self::filter_chip(
                "Docs",
                Some(ContentCategory::Document),
                selected_category,
                theme
            ),
            Self::filter_chip(
                "Cards",
                Some(ContentCategory::BusinessCard),
                selected_category,
                theme
            ),
            Self::filter_chip(
                "Shots",
                Some(ContentCategory::Screenshot),
                selected_category,
                theme
            ),
            Self::filter_chip(
                "Slides",
                Some(ContentCategory::Presentation),
                selected_category,
                theme
            ),
            Self::filter_chip(
                "Board",
                Some(ContentCategory::Whiteboard),
                selected_category,
                theme
            ),
            Self::filter_chip(
                "Receipts",
                Some(ContentCategory::Receipt),
                selected_category,
                theme
            ),
            Space::with_width(Length::Fill),
            button(text("Refresh").size(12).color(p.text_primary))
                .padding(Padding::from([7, 12]))
                .on_press(Message::RunDocumentAnalysis),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let grid: Element<'static, Message> = if photos.is_empty() {
            container(
                text("No documents found yet.")
                    .size(14)
                    .color(p.text_tertiary),
            )
            .padding(20)
            .width(Length::Fill)
            .into()
        } else {
            let groups = Self::group_by_date(photos);
            let mut items: Vec<Element<'static, Message>> = Vec::new();

            for group in groups {
                let selected_count_for_day = group
                    .photos
                    .iter()
                    .filter(|photo| selected_photo_ids.contains(&photo.id))
                    .count();

                items.push(day_header(
                    &group.day_key,
                    &group.display_date,
                    group.photos.len(),
                    selected_count_for_day,
                    hovered_day_key == Some(group.day_key.as_str()),
                    theme,
                ));

                items.push(photo_grid_simple(
                    &group.photos,
                    160.0,
                    columns,
                    Some(selected_photo_ids),
                    hovered_photo_id,
                    None,
                    theme,
                ));
            }

            column(items).spacing(0).into()
        };

        container(
            column![
                title,
                Space::with_height(6),
                subtitle,
                Space::with_height(16),
                search,
                Space::with_height(10),
                filter_row,
                Space::with_height(16),
                grid,
            ]
            .padding(32),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(p.bg_primary.into()),
            ..Default::default()
        })
        .into()
    }

    fn filter_chip(
        label: &str,
        category: Option<ContentCategory>,
        selected: Option<ContentCategory>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let active = category == selected;
        let chip_label = label.to_string();
        button(text(chip_label).size(12).color(p.text_primary))
            .padding(Padding::from([5, 10]))
            .style(move |_theme: &iced::Theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(p.bg_hover.into()),
                    _ if active => Some(p.accent_muted.into()),
                    _ => Some(p.bg_elevated.into()),
                };

                button::Style {
                    background,
                    border: iced::Border {
                        color: p.border_subtle,
                        width: 1.0,
                        radius: 14.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::DocumentsFilterCategory(
                category.map(|c| c.as_str().to_string()),
            ))
            .into()
    }

    fn group_by_date(photos: &[Photo]) -> Vec<DocumentDateGroup> {
        let mut groups: BTreeMap<String, DocumentDateGroup> = BTreeMap::new();

        for photo in photos {
            let date_key = photo
                .date_taken
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "0000-00-00".to_string());

            let display_date = photo
                .date_taken
                .map(|d| Self::format_display_date(&d))
                .unwrap_or_else(|| "Unknown Date".to_string());

            let group = groups
                .entry(date_key.clone())
                .or_insert_with(|| DocumentDateGroup {
                    day_key: date_key,
                    display_date,
                    location: photo.location_string(),
                    photos: Vec::new(),
                });

            if group.location.is_none() && photo.has_location() {
                group.location = photo.location_string();
            }

            group.photos.push(photo.clone());
        }

        let mut result: Vec<_> = groups.into_values().collect();
        result.reverse();
        result
    }

    fn format_display_date(date: &DateTime<Utc>) -> String {
        let today = Utc::now().date_naive();
        let photo_date = date.date_naive();

        if photo_date == today {
            "Today".to_string()
        } else if photo_date == today.pred_opt().unwrap_or(today) {
            "Yesterday".to_string()
        } else if photo_date.year() == today.year() {
            date.format("%B %d").to_string()
        } else {
            date.format("%B %d, %Y").to_string()
        }
    }
}

#[derive(Debug, Clone)]
struct DocumentDateGroup {
    day_key: String,
    display_date: String,
    location: Option<String>,
    photos: Vec<Photo>,
}
