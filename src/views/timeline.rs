//! Timeline view - main photo browsing interface
//!
//! Displays photos organized by date with day headers.
//! Uses scrollable grid for the photo layout.

use iced::widget::{column, container, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashSet;

use crate::app::Message;
use crate::components::photo_grid::{day_header, photo_grid_simple};
use crate::config::AppTheme;
use crate::models::{DateGroupRange, Photo};
use crate::theme::colors;

/// Timeline view component
pub struct TimelineView;

impl TimelineView {
    /// Pre-compute timeline date groups. Delegates to the pure
    /// `models::timeline_group::compute_groups` implementation so the
    /// grouping logic can be exercised by integration tests that
    /// don't depend on the iced view layer.
    pub fn compute_groups(photos: &[Photo]) -> Vec<DateGroupRange> {
        crate::models::compute_groups(photos)
    }

    /// Render the timeline view with pre-computed date groups.
    /// An optional `banner` element (e.g. memories carousel) is placed at the
    /// top of the scrollable area so it scrolls with the photo content.
    pub fn view_with_photos(
        photos: &[Photo],
        groups: &[DateGroupRange],
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

        let mut timeline_items: Vec<Element<'static, Message>> =
            Vec::with_capacity(groups.len() * 2 + 1);

        // Banner scrolls with the rest of the content
        if let Some(b) = banner {
            timeline_items.push(b);
        }

        for group in groups {
            let group_photos = &photos[group.start..group.end];

            let selected_count_for_day = group_photos
                .iter()
                .filter(|p| selected_photo_ids.contains(&p.id))
                .count();

            timeline_items.push(day_header(
                &group.day_key,
                &group.display_date,
                group_photos.len(),
                selected_count_for_day,
                hovered_day_key == Some(group.day_key.as_str()),
                theme,
            ));

            timeline_items.push(photo_grid_simple(
                group_photos,
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
}
