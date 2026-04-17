//! Mini-map embedded in the photo detail info panel. Pannable + zoomable
//! using its own center/zoom state, reset to photo GPS on open.

use iced::widget::{column, container, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::{Message, PhotoVault};
use crate::components::map_widget::{map_widget, InteractionMode, MapWidgetConfig};
use crate::models::Photo;
use crate::services::map_math::LatLng;
use crate::theme::colors;

const MINI_W: f32 = 240.0;
const MINI_H: f32 = 160.0;

pub fn photo_mini_map(app: &PhotoVault, photo: &Photo) -> Option<Element<'static, Message>> {
    let lat = photo.gps_latitude?;
    let lng = photo.gps_longitude?;
    let cache = app.tile_cache.as_ref()?;

    // Current view state: pannable/zoomable center+zoom tracked in app.
    let center = app.photo_map_center.unwrap_or(LatLng { lat, lng });
    let zoom = app.photo_map_zoom;

    // Always show the photo's actual pin at its real GPS, regardless of
    // where the user has panned to.
    let pin = (photo.id, LatLng { lat, lng });

    let widget = map_widget(MapWidgetConfig {
        cache,
        center,
        zoom,
        pins: std::slice::from_ref(&pin),
        width: MINI_W,
        height: MINI_H,
        interaction: InteractionMode::Photo,
        show_attribution: false,
        theme: app.config.theme,
        recent_fetch_failure: false,
    });

    let p = colors::palette(app.config.theme);
    let place = app
        .current_photo_location
        .clone()
        .or_else(|| photo.location_string())
        .unwrap_or_else(|| "Resolving location...".to_string());

    let border = p.border_subtle;
    let bg = p.bg_elevated;

    Some(
        column![
            container(widget)
                .width(Length::Fixed(MINI_W))
                .height(Length::Fixed(MINI_H))
                .style(move |_t: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        color: border,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }),
            Space::with_height(6),
            text(place).size(12).color(p.text_secondary),
        ]
        .align_x(Alignment::Start)
        .into(),
    )
}
