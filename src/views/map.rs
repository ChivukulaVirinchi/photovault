//! Full-screen map view.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::{MapPopover, Message, PhotoVault};
use crate::components::map_widget::{map_widget, InteractionMode, MapWidgetConfig};
use crate::services::map_math;
use crate::theme::colors;

pub fn map_view(app: &PhotoVault) -> Element<'static, Message> {
    let p = colors::palette(app.config.theme);

    let Some(ref cache) = app.tile_cache else {
        return empty_state(app);
    };

    let sidebar_w = if app.config.sidebar_collapsed {
        36.0
    } else {
        180.0
    };
    let viewport_w = (app.window_width - sidebar_w).max(320.0);
    let viewport_h = (app.window_height - 64.0).max(240.0);

    let widget = map_widget(MapWidgetConfig {
        cache,
        center: app.map_center,
        zoom: app.map_zoom,
        pins: &app.map_pins_cache,
        width: viewport_w,
        height: viewport_h,
        interaction: InteractionMode::Main,
        show_attribution: true,
        theme: app.config.theme,
        recent_fetch_failure: app.map_recent_fetch_failure,
    });

    let header = container(
        row![
            text("Map").size(22).color(p.text_primary),
            Space::with_width(Length::Fixed(12.0)),
            text(format!("{} pins", app.map_pins_cache.len()))
                .size(12)
                .color(p.text_secondary),
            Space::with_width(Length::Fill),
            button(text("Reset view").size(12)).on_press(Message::MapResetView),
        ]
        .align_y(Alignment::Center)
        .spacing(8),
    )
    .padding(Padding {
        top: 16.0,
        right: 24.0,
        bottom: 8.0,
        left: 24.0,
    });

    let zoom_controls = container(
        column![
            button(text("+").size(20)).on_press(Message::MapZoomAt {
                x: viewport_w / 2.0,
                y: viewport_h / 2.0,
                delta: 1,
            }),
            Space::with_height(4),
            button(text("-").size(20)).on_press(Message::MapZoomAt {
                x: viewport_w / 2.0,
                y: viewport_h / 2.0,
                delta: -1,
            }),
        ]
        .spacing(4),
    )
    .padding(Padding {
        top: 16.0,
        right: 16.0,
        bottom: 0.0,
        left: 0.0,
    })
    .align_right(Length::Fill)
    .align_top(Length::Fill);

    // Build popover overlays: each popover's anchor (LatLng) is
    // re-projected to the current viewport every render, so cards
    // follow the map when panned.
    let mut body = iced::widget::stack![widget, zoom_controls]
        .width(Length::Fill)
        .height(Length::Fill);

    let v = map_math::Viewport {
        width: viewport_w,
        height: viewport_h,
        center: app.map_center,
        zoom: app.map_zoom,
    };

    for (idx, pop) in app.open_popovers.iter().enumerate() {
        let (px, py) = map_math::latlng_to_viewport_pixel(&v, pop.anchor);
        if px < -200.0 || py < -100.0 || px > viewport_w + 200.0 || py > viewport_h + 100.0 {
            continue;
        }
        body = body.push(popover_at(app, idx, pop, px, py));
    }

    container(column![header, body])
        .width(Length::Fixed(viewport_w))
        .height(Length::Fill)
        .clip(true)
        .into()
}

fn empty_state(app: &PhotoVault) -> Element<'static, Message> {
    let p = colors::palette(app.config.theme);
    container(
        column![
            text("Map").size(24).color(p.text_primary),
            text("Select a drive to see your photos on a map.")
                .size(14)
                .color(p.text_secondary),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

fn popover_at(
    app: &PhotoVault,
    idx: usize,
    pop: &MapPopover,
    px: f32,
    py: f32,
) -> Element<'static, Message> {
    let p = colors::palette(app.config.theme);
    let count = pop.photo_ids.len();
    let ids = pop.photo_ids.clone();

    let thumbs: Element<'static, Message> = if count == 1 {
        single_photo_preview(app, pop.photo_ids[0])
    } else {
        cluster_grid_preview(app, &pop.photo_ids)
    };

    let box_el = container(
        column![
            row![
                text(format!(
                    "{} photo{}",
                    count,
                    if count == 1 { "" } else { "s" }
                ))
                .size(14)
                .color(p.text_primary),
                Space::with_width(Length::Fill),
                button(text("x").size(14)).on_press(Message::MapClosePopoverAt(idx)),
            ]
            .align_y(Alignment::Center),
            Space::with_height(8),
            thumbs,
            Space::with_height(8),
            button(text("Open filmstrip").size(12)).on_press(Message::MapOpenClusterFilmstrip(ids)),
        ]
        .spacing(0)
        .padding(12)
        .width(Length::Fixed(280.0)),
    )
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(p.bg_elevated.into()),
        border: iced::Border {
            color: p.border_subtle,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    });

    // Position: box anchored above the pin, offset slightly up so tip
    // points at pin.
    let top = (py - 200.0).max(0.0);
    let left = (px - 140.0).max(0.0);
    container(box_el)
        .padding(Padding {
            top,
            left,
            right: 0.0,
            bottom: 0.0,
        })
        .into()
}

fn single_photo_preview(app: &PhotoVault, photo_id: i64) -> Element<'static, Message> {
    let p = colors::palette(app.config.theme);
    let photo = app.photos.iter().find(|ph| ph.id == photo_id);

    match photo.and_then(|ph| ph.thumbnail_path.clone()) {
        Some(path) => iced::widget::image(iced::widget::image::Handle::from_path(path))
            .width(Length::Fixed(256.0))
            .height(Length::Fixed(192.0))
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => container(text("(no preview)").color(p.text_tertiary))
            .width(Length::Fixed(256.0))
            .height(Length::Fixed(192.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
    }
}

fn cluster_grid_preview(app: &PhotoVault, photo_ids: &[i64]) -> Element<'static, Message> {
    let mut grid = iced::widget::column![].spacing(4);
    let mut current_row = iced::widget::row![].spacing(4);
    let mut in_row = 0usize;

    for id in photo_ids.iter().take(6) {
        let photo = app.photos.iter().find(|p| p.id == *id);
        let thumb: Element<'static, Message> = match photo.and_then(|p| p.thumbnail_path.clone()) {
            Some(path) => iced::widget::image(iced::widget::image::Handle::from_path(path))
                .width(Length::Fixed(80.0))
                .height(Length::Fixed(80.0))
                .content_fit(iced::ContentFit::Cover)
                .into(),
            None => Space::new(Length::Fixed(80.0), Length::Fixed(80.0)).into(),
        };

        current_row = current_row.push(thumb);
        in_row += 1;

        if in_row == 3 {
            grid = grid.push(current_row);
            current_row = iced::widget::row![].spacing(4);
            in_row = 0;
        }
    }

    if in_row > 0 {
        grid = grid.push(current_row);
    }

    grid.into()
}
