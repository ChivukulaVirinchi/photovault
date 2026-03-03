//! Timeline view - main photo browsing interface
//!
//! Displays photos organized by date with day headers.
//! Uses scrollable grid for the photo layout.

use chrono::{DateTime, Datelike, Utc};
use iced::widget::{column, container, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::components::photo_grid::{day_header, photo_grid_simple};
use crate::models::Photo;
use crate::theme::colors::{Backgrounds, Text};

/// Group of photos taken on the same date
#[derive(Debug, Clone)]
pub struct DateGroup {
    pub date: String,
    pub display_date: String,
    pub location: Option<String>,
    pub photos: Vec<Photo>,
}

/// Timeline view component
pub struct TimelineView;

impl TimelineView {
    /// Render the timeline view with photos
    pub fn view_with_photos(photos: &[Photo], columns: usize) -> Element<'static, Message> {
        if photos.is_empty() {
            return Self::empty_view();
        }

        // Group photos by date
        let groups = Self::group_by_date(photos);

        // Build the timeline content
        let mut timeline_items: Vec<Element<'static, Message>> = Vec::new();

        for group in groups {
            // Add day header
            timeline_items.push(day_header(
                &group.display_date,
                group.location.as_deref(),
                group.photos.len(),
            ));

            // Add photo grid for this day with dynamic column count
            timeline_items.push(photo_grid_simple(&group.photos, 160.0, columns));
        }

        let content = Column::with_children(timeline_items)
            .spacing(0)
            .width(Length::Fill);

        scrollable(content).height(Length::Fill).into()
    }

    /// Render empty timeline (backward compat, also used when no photos)
    pub fn view() -> Element<'static, Message> {
        Self::empty_view()
    }

    /// Empty state view
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Timeline").size(28).color(Text::PRIMARY),
            Space::with_height(16),
            text("Your photos will appear here after indexing.")
                .size(14)
                .color(Text::SECONDARY),
            Space::with_height(32),
            text("Photos are organized by date, newest first.")
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
                date: date_key,
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
