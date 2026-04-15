//! Interactive map widget — two canvases (tile + pin) so pins render on
//! top of tile images. Parameterized by `InteractionMode` so the same
//! widget serves the full-screen map AND the photo-detail mini-map with
//! independent state.

use std::cell::RefCell;

use iced::advanced::image;
use iced::mouse;
use iced::widget::canvas::{self, Path, Stroke, Text};
use iced::widget::{canvas as canvas_widget, container, mouse_area, stack, text};
use iced::{alignment, Color, Element, Length, Point, Rectangle, Size};

use crate::app::Message;
use crate::config::AppTheme;
use crate::services::map_math::{self, LatLng, TileId, Viewport};
use crate::services::TileCache;
use crate::theme::colors;

const TILE_PX: f32 = map_math::TILE_SIZE as f32;
const CLUSTER_DIST_PX: f32 = 40.0;
const MAX_PARENT_SCALE: u32 = 32; // look up to 5 zoom levels for parent fallback

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMode {
    None,
    Main,
    Photo,
}

#[derive(Debug, Clone)]
pub struct MapWidgetConfig<'a> {
    pub cache: &'a TileCache,
    pub center: LatLng,
    pub zoom: u8,
    pub pins: &'a [(i64, LatLng)],
    /// Explicit pixel dimensions. When set, canvases use Fixed sizing
    /// instead of Fill, ensuring they never exceed the allocated area.
    pub width: f32,
    pub height: f32,
    pub interaction: InteractionMode,
    pub show_attribution: bool,
    pub theme: AppTheme,
    pub recent_fetch_failure: bool,
}

pub fn map_widget(cfg: MapWidgetConfig<'_>) -> Element<'static, Message> {
    let w = Length::Fixed(cfg.width);
    let h = Length::Fixed(cfg.height);

    let tile_canvas: Element<'static, Message> = canvas_widget(TileCanvas {
        cache: cfg.cache.clone(),
        center: cfg.center,
        zoom: cfg.zoom,
        theme: cfg.theme,
    })
    .width(w)
    .height(h)
    .into();

    let pin_canvas: Element<'static, Message> = canvas_widget(PinCanvas {
        center: cfg.center,
        zoom: cfg.zoom,
        pins: cfg.pins.to_vec(),
        interaction: cfg.interaction,
    })
    .width(w)
    .height(h)
    .into();

    let mut layers = stack![tile_canvas, pin_canvas].width(w).height(h);

    if cfg.show_attribution {
        layers = layers.push(attribution_layer(cfg.theme));
    }

    if cfg.recent_fetch_failure && cfg.interaction == InteractionMode::Main {
        layers = layers.push(offline_banner_layer());
    }

    let base: Element<'static, Message> = container(layers)
        .width(w)
        .height(h)
        .clip(true)
        .into();

    if cfg.interaction == InteractionMode::None {
        return base;
    }

    // Photo mini-map: canvas Program::update handles all events directly
    // (pan, zoom) and returns Captured. No mouse_area wrapper needed —
    // adding one would cause duplicate/conflicting event handling and
    // scroll events leaking to parent widgets.
    if cfg.interaction == InteractionMode::Photo {
        return base;
    }

    // Main map: use mouse_area for pan/zoom (canvas only handles pin clicks).
    mouse_area(base)
        .on_press(Message::MapPanStart {
            x: f32::NAN,
            y: f32::NAN,
        })
        .on_release(Message::MapPanEnd)
        .on_move(|p| Message::MapPan {
            dx: p.x,
            dy: p.y,
        })
        .on_scroll(move |delta| {
            let amount = match delta {
                mouse::ScrollDelta::Lines { y, .. } => y,
                mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            let step = if amount > 0.0 {
                1
            } else if amount < 0.0 {
                -1
            } else {
                0
            };
            Message::MapScrollZoom { delta: step }
        })
        .into()
}

// --------------------------------------------------------------------
// Tile canvas (bottom layer)
// --------------------------------------------------------------------

struct TileCanvas {
    cache: TileCache,
    center: LatLng,
    zoom: u8,
    theme: AppTheme,
}

impl canvas::Program<Message> for TileCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let v = Viewport {
            width: bounds.width,
            height: bounds.height,
            center: self.center,
            zoom: self.zoom,
        };

        let grey = if matches!(self.theme, AppTheme::Dark) {
            Color::from_rgb(0.22, 0.23, 0.26)
        } else {
            Color::from_rgb(0.88, 0.88, 0.90)
        };

        for tile in map_math::visible_tiles(&v) {
            let (tx, ty) = tile_top_left(tile, &v);
            // Skip tiles fully outside the frame bounds to prevent
            // canvas overflow into adjacent layout regions.
            if tx + TILE_PX < 0.0
                || ty + TILE_PX < 0.0
                || tx > bounds.width
                || ty > bounds.height
            {
                continue;
            }
            draw_tile(&mut frame, tile, tx, ty, &self.cache, grey);
        }

        vec![frame.into_geometry()]
    }
}

// --------------------------------------------------------------------
// Pin canvas (top layer)
// --------------------------------------------------------------------

struct PinCanvas {
    center: LatLng,
    zoom: u8,
    pins: Vec<(i64, LatLng)>,
    interaction: InteractionMode,
}

#[derive(Default)]
pub struct PinCanvasState {
    cached_clusters: RefCell<Option<ClusterCacheEntry>>,
}

struct ClusterCacheEntry {
    key: ClusterKey,
    clusters: Vec<Cluster>,
}

#[derive(PartialEq, Eq)]
struct ClusterKey {
    bounds_w: u32,
    bounds_h: u32,
    center_lat: i64,
    center_lng: i64,
    zoom: u8,
    pin_count: usize,
}

fn cluster_key(bounds: Rectangle, center: &LatLng, zoom: u8, pin_count: usize) -> ClusterKey {
    ClusterKey {
        bounds_w: bounds.width as u32,
        bounds_h: bounds.height as u32,
        center_lat: (center.lat * 1_000_000.0) as i64,
        center_lng: (center.lng * 1_000_000.0) as i64,
        zoom,
        pin_count,
    }
}

impl canvas::Program<Message> for PinCanvas {
    type State = PinCanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        self.with_clusters(state, bounds, |clusters| {
            for c in clusters {
                draw_cluster(&mut frame, c);
            }
        });
        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        if self.interaction == InteractionMode::None {
            return (canvas::event::Status::Ignored, None);
        }

        let Some(pos) = cursor.position_in(bounds) else {
            return (canvas::event::Status::Ignored, None);
        };

        // For the photo mini-map, capture mouse events so they don't
        // propagate to parent widgets and cause the panel to move.
        if self.interaction == InteractionMode::Photo {
            let msg = match event {
                canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    Some(Message::PhotoMapPanStart { x: pos.x, y: pos.y })
                }
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::PhotoMapPanEnd)
                }
                canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    Some(Message::PhotoMapPan {
                        dx: pos.x,
                        dy: pos.y,
                    })
                }
                canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                    let amount = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y,
                        mouse::ScrollDelta::Pixels { y, .. } => y,
                    };
                    let step = if amount > 0.0 { 1 } else if amount < 0.0 { -1 } else { 0 };
                    Some(Message::PhotoMapZoomAt {
                        x: pos.x,
                        y: pos.y,
                        delta: step,
                    })
                }
                _ => None,
            };
            return (canvas::event::Status::Captured, msg);
        }

        // Main map: only handle pin clicks here (pan/zoom via mouse_area).
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            let mut hit: Option<Message> = None;
            self.with_clusters(state, bounds, |clusters| {
                for c in clusters {
                    let dx = c.x - pos.x;
                    let dy = c.y - pos.y;
                    if (dx * dx + dy * dy).sqrt() <= cluster_radius(c.count) + 4.0 {
                        hit = Some(Message::MapPinClicked {
                            photo_ids: c.photo_ids.clone(),
                            anchor: c.anchor,
                        });
                        break;
                    }
                }
            });
            if let Some(msg) = hit {
                return (canvas::event::Status::Captured, Some(msg));
            }
        }

        (canvas::event::Status::Ignored, None)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.interaction == InteractionMode::None {
            return mouse::Interaction::None;
        }
        let Some(pos) = cursor.position_in(bounds) else {
            return mouse::Interaction::None;
        };

        if self.interaction == InteractionMode::Main {
            let mut over_pin = false;
            self.with_clusters(state, bounds, |clusters| {
                over_pin = clusters.iter().any(|c| {
                    let dx = c.x - pos.x;
                    let dy = c.y - pos.y;
                    (dx * dx + dy * dy).sqrt() <= cluster_radius(c.count) + 4.0
                });
            });
            if over_pin {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::Grab
    }
}

impl PinCanvas {
    fn with_clusters<R>(
        &self,
        state: &PinCanvasState,
        bounds: Rectangle,
        f: impl FnOnce(&[Cluster]) -> R,
    ) -> R {
        let key = cluster_key(bounds, &self.center, self.zoom, self.pins.len());
        let needs_recompute = match state.cached_clusters.borrow().as_ref() {
            Some(entry) => entry.key != key,
            None => true,
        };
        if needs_recompute {
            let v = Viewport {
                width: bounds.width,
                height: bounds.height,
                center: self.center,
                zoom: self.zoom,
            };
            let clusters = cluster_pins(&v, &self.pins);
            *state.cached_clusters.borrow_mut() = Some(ClusterCacheEntry { key, clusters });
        }
        let borrow = state.cached_clusters.borrow();
        let entry = borrow
            .as_ref()
            .expect("cached_clusters populated just above");
        f(&entry.clusters)
    }
}

// --------------------------------------------------------------------
// Tile drawing with parent-tile fallback (no `translate`; the image
// Rectangle is positioned so the correct sub-region lands in the clip).
// --------------------------------------------------------------------

fn tile_top_left(tid: TileId, v: &Viewport) -> (f32, f32) {
    let (cx, cy) = map_math::viewport_center_tile(v);
    let dx = (tid.x as f64 - cx) * map_math::TILE_SIZE;
    let dy = (tid.y as f64 - cy) * map_math::TILE_SIZE;
    (
        (v.width as f64 / 2.0 + dx).round() as f32,
        (v.height as f64 / 2.0 + dy).round() as f32,
    )
}

fn draw_tile(
    frame: &mut canvas::Frame,
    tile: TileId,
    tx: f32,
    ty: f32,
    cache: &TileCache,
    grey: Color,
) {
    // Direct hit: cached at exact target zoom.
    if cache.has(tile) {
        let handle = image::Handle::from_path(cache.tile_path(tile));
        frame.draw_image(
            Rectangle {
                x: tx,
                y: ty,
                width: TILE_PX,
                height: TILE_PX,
            },
            image::Image::new(handle),
        );
        return;
    }

    // Parent fallback: walk up zoom levels, draw the first cached
    // ancestor scaled so the correct sub-region lands in our clip rect.
    let mut source = tile;
    let mut scale: u32 = 1;
    while scale < MAX_PARENT_SCALE {
        if source.z == 0 {
            break;
        }
        source = TileId {
            z: source.z - 1,
            x: source.x / 2,
            y: source.y / 2,
        };
        scale *= 2;
        if cache.has(source) {
            let handle = image::Handle::from_path(cache.tile_path(source));
            let dst = Rectangle {
                x: tx,
                y: ty,
                width: TILE_PX,
                height: TILE_PX,
            };
            // child_x/y = which of the scale^2 sub-tiles of `source` is our target
            let child_x = (tile.x & (scale - 1)) as f32;
            let child_y = (tile.y & (scale - 1)) as f32;
            // In the clipped sub-frame, origin == (tx, ty) in parent coords.
            // Position the parent image so its (child_x, child_y) quadrant
            // aligns with sub-frame origin. Image extends outside the clip
            // but the clip hides the overflow.
            let image_x = -child_x * TILE_PX;
            let image_y = -child_y * TILE_PX;
            let scaled_size = TILE_PX * scale as f32;
            frame.with_clip(dst, |sub| {
                sub.draw_image(
                    Rectangle {
                        x: image_x,
                        y: image_y,
                        width: scaled_size,
                        height: scaled_size,
                    },
                    image::Image::new(handle),
                );
            });
            return;
        }
    }

    // No cached ancestor — draw a neutral loading square.
    frame.fill_rectangle(Point::new(tx, ty), Size::new(TILE_PX, TILE_PX), grey);
}

// --------------------------------------------------------------------
// Pin clustering
// --------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Cluster {
    x: f32,
    y: f32,
    anchor: LatLng,
    count: usize,
    photo_ids: Vec<i64>,
}

fn cluster_pins(v: &Viewport, pins: &[(i64, LatLng)]) -> Vec<Cluster> {
    let projected: Vec<(i64, LatLng, f32, f32)> = pins
        .iter()
        .map(|(id, ll)| {
            let (px, py) = map_math::latlng_to_viewport_pixel(v, *ll);
            (*id, *ll, px, py)
        })
        .filter(|(_, _, px, py)| {
            *px >= -40.0 && *py >= -40.0 && *px <= v.width + 40.0 && *py <= v.height + 40.0
        })
        .collect();

    let mut out: Vec<Cluster> = Vec::new();
    let mut used = vec![false; projected.len()];

    for i in 0..projected.len() {
        if used[i] {
            continue;
        }

        let (id_i, ll_i, px_i, py_i) = projected[i];
        let mut cluster = Cluster {
            x: px_i,
            y: py_i,
            anchor: ll_i,
            count: 1,
            photo_ids: vec![id_i],
        };
        let mut sum_lat = ll_i.lat;
        let mut sum_lng = ll_i.lng;
        used[i] = true;

        for j in (i + 1)..projected.len() {
            if used[j] {
                continue;
            }

            let (id_j, ll_j, px_j, py_j) = projected[j];
            if (px_i - px_j).hypot(py_i - py_j) < CLUSTER_DIST_PX {
                used[j] = true;
                let old_count = cluster.count as f32;
                cluster.count += 1;
                let new_count = cluster.count as f32;
                cluster.photo_ids.push(id_j);
                cluster.x = (cluster.x * old_count + px_j) / new_count;
                cluster.y = (cluster.y * old_count + py_j) / new_count;
                sum_lat += ll_j.lat;
                sum_lng += ll_j.lng;
                cluster.anchor = LatLng {
                    lat: sum_lat / new_count as f64,
                    lng: sum_lng / new_count as f64,
                };
            }
        }

        out.push(cluster);
    }

    out
}

fn cluster_radius(count: usize) -> f32 {
    match count {
        1 => 8.0,
        2..=10 => 14.0,
        11..=100 => 18.0,
        _ => 24.0,
    }
}

fn draw_cluster(frame: &mut canvas::Frame, c: &Cluster) {
    let center = Point::new(c.x, c.y);
    let radius = cluster_radius(c.count);
    let color = Color::from_rgb(0.098, 0.463, 0.824);

    frame.fill(&Path::circle(center, radius), color);
    frame.stroke(
        &Path::circle(center, radius),
        Stroke::default().with_width(2.0).with_color(Color::WHITE),
    );

    if c.count > 1 {
        frame.fill_text(Text {
            content: c.count.to_string(),
            position: center,
            color: Color::WHITE,
            size: iced::Pixels(11.0),
            horizontal_alignment: alignment::Horizontal::Center,
            vertical_alignment: alignment::Vertical::Center,
            ..Text::default()
        });
    }
}

// --------------------------------------------------------------------
// Overlay layers
// --------------------------------------------------------------------

fn attribution_layer(theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    container(
        text("© OpenStreetMap contributors © CARTO")
            .size(10)
            .color(p.text_tertiary),
    )
    .padding([2, 6])
    .align_right(Length::Fill)
    .align_bottom(Length::Fill)
    .into()
}

fn offline_banner_layer() -> Element<'static, Message> {
    container(
        text("Offline — tiles for this region aren't cached yet.")
            .size(12)
            .color(Color::WHITE),
    )
    .padding([6, 12])
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.70).into()),
        ..Default::default()
    })
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Top)
    .into()
}
