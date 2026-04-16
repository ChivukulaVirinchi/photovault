//! Timeline view - main photo browsing interface
//!
//! Displays photos organized by date with day headers.
//! Uses scrollable grid for the photo layout.

use chrono::{DateTime, Datelike, Utc};
use iced::widget::{column, container, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashSet;

use crate::app::Message;
use crate::components::photo_grid::{day_header, photo_grid_simple};
use crate::config::AppTheme;
use crate::models::Photo;
use crate::theme::colors;

/// Group of photos taken on the same date
#[derive(Debug, Clone)]
pub struct DateGroup {
    pub day_key: String,
    pub display_date: String,
    pub location: Option<String>,
    pub photos: Vec<Photo>,
}

/// Timeline view component
pub struct TimelineView;

impl TimelineView {
    /// Render the timeline view with photos.
    /// An optional `banner` element (e.g. memories carousel) is placed at the
    /// top of the scrollable area so it scrolls with the photo content.
    pub fn view_with_photos(
        photos: &[Photo],
        columns: usize,
        selected_photo_ids: &HashSet<i64>,
        hovered_photo_id: Option<i64>,
        hovered_day_key: Option<&str>,
        highlighted_photo_id: Option<i64>,
        banner: Option<Element<'static, Message>>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        if photos.is_empty() {
            return Self::empty_view(theme);
        }

        // Group photos by date
        let groups = Self::group_by_date(photos);

        // Build the timeline content
        let mut timeline_items: Vec<Element<'static, Message>> = Vec::new();

        // Banner scrolls with the rest of the content
        if let Some(b) = banner {
            timeline_items.push(b);
        }

        for group in groups {
            let selected_count_for_day = group
                .photos
                .iter()
                .filter(|p| selected_photo_ids.contains(&p.id))
                .count();

            // Add day header
            timeline_items.push(day_header(
                &group.day_key,
                &group.display_date,
                group.photos.len(),
                selected_count_for_day,
                hovered_day_key == Some(group.day_key.as_str()),
                theme,
            ));

            // Add photo grid for this day with dynamic column count
            timeline_items.push(photo_grid_simple(
                &group.photos,
                160.0,
                columns,
                Some(selected_photo_ids),
                hovered_photo_id,
                highlighted_photo_id,
                theme,
            ));
        }

        let content = Column::with_children(timeline_items)
            .spacing(0)
            .width(Length::Fill);

        scrollable(content)
            .id(iced::widget::scrollable::Id::new("timeline"))
            .on_scroll(|vp| Message::TimelineScrolled(vp.absolute_offset()))
            .into()
    }

    /// Render empty timeline (backward compat, also used when no photos)
    pub fn view(theme: AppTheme) -> Element<'static, Message> {
        Self::empty_view(theme)
    }

    /// Skeleton placeholder while photos are loading.
    pub fn skeleton(columns: usize, theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg = p.bg_primary;
        let content = column![
            text("Timeline").size(22).color(p.text_primary),
            Space::with_height(12),
            crate::components::photo_grid::skeleton_grid(4, columns, 160.0, theme),
        ]
        .padding(32);
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                ..Default::default()
            })
            .into()
    }

    /// Empty state view
    fn empty_view(theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);

        let content = column![
            text("Timeline").size(28).color(p.text_primary),
            Space::with_height(16),
            text("Your photos will appear here after indexing.")
                .size(14)
                .color(p.text_secondary),
            Space::with_height(32),
            text("Photos are organized by date, newest first.")
                .size(14)
                .color(p.text_tertiary),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        let bg = p.bg_primary;
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                ..Default::default()
            })
            .into()
    }

    /// Group photos by date (newest first)
    fn group_by_date(photos: &[Photo]) -> Vec<DateGroup> {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, DateGroup> = BTreeMap::new();

        for photo in photos {
            let date_key = photo
                .date_taken
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "0000-00-00".to_string());

            let display_date = photo
                .date_taken
                .map(|d| Self::format_display_date(&d))
                .unwrap_or_else(|| "Unknown Date".to_string());

            let group = groups.entry(date_key.clone()).or_insert_with(|| DateGroup {
                day_key: date_key.clone(),
                display_date,
                location: photo.location_string(),
                photos: Vec::new(),
            });

            // Update location if this photo has one and group doesn't yet
            if group.location.is_none() && photo.has_location() {
                group.location = photo.location_string();
            }

            group.photos.push(photo.clone());
        }

        // Convert to vec and reverse (newest first, since BTreeMap is ascending)
        let mut result: Vec<_> = groups.into_values().collect();
        result.reverse();
        result
    }

    /// Format a date for display
    fn format_display_date(date: &DateTime<Utc>) -> String {
        let now = Utc::now();
        let today = now.date_naive();
        let photo_date = date.date_naive();

        if photo_date == today {
            "Today".to_string()
        } else if photo_date == today.pred_opt().unwrap_or(today) {
            "Yesterday".to_string()
        } else if photo_date.year() == today.year() {
            date.format("%B %d").to_string() // "March 15"
        } else {
            date.format("%B %d, %Y").to_string() // "March 15, 2019"
        }
    }
}
