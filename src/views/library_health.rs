//! Library Health diagnostics view.
//!
//! Surfaces five counters about library data quality and a one-line
//! status for HEIC decode availability. Lives under the Library
//! sidebar group; loaded on view-enter via Message::LoadLibraryHealth.

use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::LibraryHealth;
use crate::theme::colors;

pub struct LibraryHealthView;

impl LibraryHealthView {
    pub fn view(
        data: Option<&LibraryHealth>,
        loading: bool,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let bg_primary = p.bg_primary;

        let title = text("Library Health").size(28).color(p.text_primary);
        let subtitle = text(
            "How clean is your library's metadata? Counters here help you find photos with weak EXIF, missing thumbnails, or formats this build can't decode.",
        )
        .size(13)
        .color(p.text_secondary);

        let body: Element<'static, Message> = if loading {
            text("Loading…").size(14).color(p.text_tertiary).into()
        } else if let Some(d) = data {
            Self::counters(d, theme)
        } else {
            text("Open the view to compute counters.")
                .size(13)
                .color(p.text_tertiary)
                .into()
        };

        container(scrollable(
            column![
                title,
                Space::with_height(8),
                subtitle,
                Space::with_height(24),
                body,
            ]
            .padding(32),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bg_primary.into()),
            ..Default::default()
        })
        .into()
    }

    fn counters(d: &LibraryHealth, theme: AppTheme) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let total = d.total_photos.max(1);
        let pct = |n: i64| -> f32 { (n as f32 / total as f32 * 100.0).min(100.0) };

        let mut col = column![Self::row(
            "Total photos",
            d.total_photos.to_string(),
            None,
            theme,
        )]
        .spacing(12);

        col = col.push(Self::row(
            "Photos waiting for thumbnails",
            d.missing_thumbnails.to_string(),
            Some(format!("{:.1}% of library", pct(d.missing_thumbnails))),
            theme,
        ));
        col = col.push(Self::row(
            "Photos with uncertain dates",
            d.inaccurate_dates.to_string(),
            Some("Date came from filename / file mtime, not EXIF capture time. Try Settings → Advanced → Refresh Photo Dates after upgrading.".to_string()),
            theme,
        ));
        col = col.push(Self::row(
            "Photos with no date",
            d.missing_dates.to_string(),
            None,
            theme,
        ));

        let heic_status = if d.heic_decoder_available {
            "decoder ENABLED in this build".to_string()
        } else {
            "decoder NOT ENABLED — these photos won't thumbnail / face-process. Rebuild with `--features heic` after installing libheif (Linux: apt install libheif-dev, macOS: brew install libheif).".to_string()
        };
        col = col.push(Self::row(
            "iPhone HEIC photos",
            d.heic_count.to_string(),
            Some(heic_status),
            theme,
        ));

        col = col.push(Self::row(
            "Photos we couldn't find faces in",
            d.face_processed_no_faces.to_string(),
            Some("Either genuinely face-less photos (landscapes, documents) or a model failed silently. Cross-check by clicking through to the Timeline.".to_string()),
            theme,
        ));

        container(col)
            .padding(Padding::from([0, 0]))
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(p.bg_primary.into()),
                ..Default::default()
            })
            .into()
    }

    fn row(
        label: &str,
        value: String,
        hint: Option<String>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);
        let label_owned = label.to_string();
        let main = row![
            text(label_owned).size(15).color(p.text_primary),
            Space::with_width(Length::Fill),
            text(value).size(20).color(p.text_primary),
        ]
        .align_y(Alignment::Center);

        let mut col = column![main].spacing(4);
        if let Some(h) = hint {
            col = col.push(text(h).size(12).color(p.text_tertiary));
        }

        let bg_elevated = p.bg_elevated;
        let border_subtle = p.border_subtle;
        container(col)
            .padding(Padding::from([14, 18]))
            .width(Length::Fill)
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(bg_elevated.into()),
                border: iced::Border {
                    color: border_subtle,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
