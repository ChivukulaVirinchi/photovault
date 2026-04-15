//! Map view handlers.

use iced::Task;

use crate::db::Database;
use crate::services::{map_math, TileCache};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

fn viewport_size(app: &PhotoVault) -> (f32, f32) {
    let sidebar_w = if app.config.sidebar_collapsed {
        36.0
    } else {
        180.0
    };
    (
        (app.window_width - sidebar_w).max(320.0),
        (app.window_height - 64.0).max(240.0),
    )
}

pub(crate) fn init_tile_cache(app: &mut PhotoVault) {
    let Some(drive) = app.selected_drive.clone() else {
        return;
    };

    let cache = TileCache::new(&drive, app.map_cache_limit_bytes);
    app.tile_cache = Some(cache.clone());

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            let _ = cache.evict_if_over_limit();
        }
    });
}

pub(crate) fn load_pins(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive) = app.selected_drive.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let Ok(db) = Database::open_for_drive(&drive) else {
                    return Vec::new();
                };

                let mut stmt = match db.conn.prepare(
                    "SELECT id, gps_latitude, gps_longitude FROM photos
                     WHERE is_trashed = FALSE
                       AND gps_latitude IS NOT NULL
                       AND gps_longitude IS NOT NULL",
                ) {
                    Ok(s) => s,
                    Err(_) => return Vec::new(),
                };

                stmt.query_map([], |r| {
                    let id: i64 = r.get(0)?;
                    let lat: f64 = r.get(1)?;
                    let lng: f64 = r.get(2)?;
                    Ok((id, map_math::LatLng { lat, lng }))
                })
                .ok()
                .into_iter()
                .flat_map(|it| it.flatten())
                .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default()
        },
        Message::MapPinsLoaded,
    )
}

pub(crate) fn pins_loaded(
    app: &mut PhotoVault,
    pins: Vec<(i64, map_math::LatLng)>,
) -> Task<Message> {
    app.map_pins_cache = pins;
    app.map_recent_fetch_failure = false;

    let positions: Vec<_> = app.map_pins_cache.iter().map(|(_, ll)| *ll).collect();
    let (vw, vh) = viewport_size(app);
    let (center, zoom) = map_math::fit_bounds(&positions, vw, vh);
    app.map_center = center;
    app.map_zoom = zoom;

    enqueue_visible_tile_fetches(app)
}

pub(crate) fn pan_start(app: &mut PhotoVault, x: f32, y: f32) -> Task<Message> {
    if x.is_finite() && y.is_finite() {
        app.map_drag_origin = Some((x, y));
    } else {
        app.map_drag_origin = Some((f32::NAN, f32::NAN));
    }
    Task::none()
}

pub(crate) fn pan(app: &mut PhotoVault, cursor_x: f32, cursor_y: f32) -> Task<Message> {
    if app.map_drag_origin.is_none() {
        return Task::none();
    }

    let Some((origin_x, origin_y)) = app.map_drag_origin else {
        return Task::none();
    };

    if origin_x.is_nan() || origin_y.is_nan() {
        app.map_drag_origin = Some((cursor_x, cursor_y));
        return Task::none();
    }

    let dx = cursor_x - origin_x;
    let dy = cursor_y - origin_y;

    let (vw, vh) = viewport_size(app);
    let v = map_math::Viewport {
        width: vw,
        height: vh,
        center: app.map_center,
        zoom: app.map_zoom,
    };

    let new_center_px = (v.width / 2.0 - dx, v.height / 2.0 - dy);
    app.map_center = map_math::viewport_pixel_to_latlng(&v, new_center_px.0, new_center_px.1);
    app.map_drag_origin = Some((cursor_x, cursor_y));

    enqueue_visible_tile_fetches(app)
}

pub(crate) fn pan_by(app: &mut PhotoVault, dx: f32, dy: f32) -> Task<Message> {
    let (vw, vh) = viewport_size(app);
    let v = map_math::Viewport {
        width: vw,
        height: vh,
        center: app.map_center,
        zoom: app.map_zoom,
    };

    let new_center_px = (v.width / 2.0 - dx, v.height / 2.0 - dy);
    app.map_center = map_math::viewport_pixel_to_latlng(&v, new_center_px.0, new_center_px.1);

    enqueue_visible_tile_fetches(app)
}

pub(crate) fn pan_end(app: &mut PhotoVault) -> Task<Message> {
    app.map_drag_origin = None;
    Task::none()
}

pub(crate) fn zoom_at(app: &mut PhotoVault, x: f32, y: f32, delta: i8) -> Task<Message> {
    let old_zoom = app.map_zoom;
    let new_zoom = ((old_zoom as i16) + delta as i16)
        .clamp(map_math::MIN_ZOOM as i16, map_math::MAX_ZOOM as i16) as u8;

    if new_zoom == old_zoom {
        return Task::none();
    }

    let (vw, vh) = viewport_size(app);

    let x = if x.is_finite() { x } else { vw / 2.0 };
    let y = if y.is_finite() { y } else { vh / 2.0 };

    let v_old = map_math::Viewport {
        width: vw,
        height: vh,
        center: app.map_center,
        zoom: old_zoom,
    };

    let anchor_geo = map_math::viewport_pixel_to_latlng(&v_old, x, y);

    let v_new_current = map_math::Viewport {
        width: vw,
        height: vh,
        center: app.map_center,
        zoom: new_zoom,
    };
    let (anchor_px_after, anchor_py_after) =
        map_math::latlng_to_viewport_pixel(&v_new_current, anchor_geo);

    let dx = x - anchor_px_after;
    let dy = y - anchor_py_after;

    let corrected_center = map_math::viewport_pixel_to_latlng(
        &v_new_current,
        v_new_current.width / 2.0 - dx,
        v_new_current.height / 2.0 - dy,
    );

    app.map_center = corrected_center;
    app.map_zoom = new_zoom;

    enqueue_visible_tile_fetches(app)
}

pub(crate) fn reset_view(app: &mut PhotoVault) -> Task<Message> {
    let positions: Vec<_> = app.map_pins_cache.iter().map(|(_, ll)| *ll).collect();
    let (vw, vh) = viewport_size(app);
    let (center, zoom) = map_math::fit_bounds(&positions, vw, vh);
    app.map_center = center;
    app.map_zoom = zoom;

    enqueue_visible_tile_fetches(app)
}

pub(crate) fn pin_clicked(
    app: &mut PhotoVault,
    photo_ids: Vec<i64>,
    anchor: map_math::LatLng,
) -> Task<Message> {
    // If the same cluster is already open, close it (toggle behavior).
    // Identity = identical photo_ids set.
    let existing = app
        .open_popovers
        .iter()
        .position(|p| p.photo_ids == photo_ids);
    if let Some(idx) = existing {
        app.open_popovers.remove(idx);
    } else {
        app.open_popovers
            .push(crate::app::messages::MapPopover { anchor, photo_ids });
    }
    Task::none()
}

pub(crate) fn close_popover(app: &mut PhotoVault) -> Task<Message> {
    app.open_popovers.clear();
    Task::none()
}

pub(crate) fn close_popover_at(app: &mut PhotoVault, idx: usize) -> Task<Message> {
    if idx < app.open_popovers.len() {
        app.open_popovers.remove(idx);
    }
    Task::none()
}

pub(crate) fn open_cluster_filmstrip(
    app: &mut PhotoVault,
    photo_ids: Vec<i64>,
) -> Task<Message> {
    if photo_ids.is_empty() {
        return Task::none();
    }

    app.memory_photos = photo_ids
        .iter()
        .filter_map(|id| app.photos.iter().find(|p| p.id == *id).cloned())
        .collect();

    if app.memory_photos.is_empty() {
        return Task::none();
    }

    let first = app.memory_photos[0].id;
    if let Some(idx) = app.photos.iter().position(|p| p.id == first) {
        app.previous_view = Some(View::Map);
        app.selected_photo_index = Some(idx);
        app.current_view = View::PhotoDetail;
        return app.load_photo_detail_for_index(idx);
    }

    Task::none()
}

// --------------------------------------------------------------------
// Photo detail mini-map handlers (independent center/zoom state)
// --------------------------------------------------------------------

pub(crate) fn photo_map_pan_start(app: &mut PhotoVault, x: f32, y: f32) -> Task<Message> {
    app.photo_map_drag_origin = if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        Some((f32::NAN, f32::NAN))
    };
    Task::none()
}

pub(crate) fn photo_map_pan(app: &mut PhotoVault, cursor_x: f32, cursor_y: f32) -> Task<Message> {
    let Some((origin_x, origin_y)) = app.photo_map_drag_origin else {
        return Task::none();
    };
    if origin_x.is_nan() || origin_y.is_nan() {
        app.photo_map_drag_origin = Some((cursor_x, cursor_y));
        return Task::none();
    }
    let Some(center) = app.photo_map_center else {
        return Task::none();
    };
    let dx = cursor_x - origin_x;
    let dy = cursor_y - origin_y;
    let v = map_math::Viewport {
        width: 240.0,
        height: 160.0,
        center,
        zoom: app.photo_map_zoom,
    };
    let new_center_px = (v.width / 2.0 - dx, v.height / 2.0 - dy);
    app.photo_map_center = Some(map_math::viewport_pixel_to_latlng(
        &v,
        new_center_px.0,
        new_center_px.1,
    ));
    app.photo_map_drag_origin = Some((cursor_x, cursor_y));
    prefetch_photo_map_visible(app)
}

pub(crate) fn photo_map_pan_end(app: &mut PhotoVault) -> Task<Message> {
    app.photo_map_drag_origin = None;
    Task::none()
}

pub(crate) fn photo_map_zoom_at(
    app: &mut PhotoVault,
    x: f32,
    y: f32,
    delta: i8,
) -> Task<Message> {
    let Some(center) = app.photo_map_center else {
        return Task::none();
    };
    let old_zoom = app.photo_map_zoom;
    let new_zoom = ((old_zoom as i16) + delta as i16)
        .clamp(map_math::MIN_ZOOM as i16, map_math::MAX_ZOOM as i16) as u8;
    if new_zoom == old_zoom {
        return Task::none();
    }
    let vw = 240.0;
    let vh = 160.0;
    let x = if x.is_finite() { x } else { vw / 2.0 };
    let y = if y.is_finite() { y } else { vh / 2.0 };
    let v_old = map_math::Viewport {
        width: vw,
        height: vh,
        center,
        zoom: old_zoom,
    };
    let anchor_geo = map_math::viewport_pixel_to_latlng(&v_old, x, y);
    let v_probe = map_math::Viewport {
        width: vw,
        height: vh,
        center: anchor_geo,
        zoom: new_zoom,
    };
    let (px, py) = map_math::latlng_to_viewport_pixel(&v_probe, anchor_geo);
    let dx = x - px;
    let dy = y - py;
    let new_center = map_math::viewport_pixel_to_latlng(
        &v_probe,
        v_probe.width / 2.0 - dx,
        v_probe.height / 2.0 - dy,
    );
    app.photo_map_center = Some(new_center);
    app.photo_map_zoom = new_zoom;
    prefetch_photo_map_visible(app)
}

fn prefetch_photo_map_visible(app: &mut PhotoVault) -> Task<Message> {
    let Some(cache) = app.tile_cache.clone() else {
        return Task::none();
    };
    let Some(center) = app.photo_map_center else {
        return Task::none();
    };
    let v = map_math::Viewport {
        width: 240.0,
        height: 160.0,
        center,
        zoom: app.photo_map_zoom,
    };
    let mut tasks = Vec::new();
    for tid in map_math::visible_tiles(&v) {
        if cache.has(tid) || app.map_inflight_tiles.contains(&tid) {
            continue;
        }
        tasks.push(queue_tile_fetch(app, tid));
    }
    if tasks.is_empty() {
        Task::none()
    } else {
        Task::batch(tasks)
    }
}


pub(crate) fn set_cache_limit(app: &mut PhotoVault, mb: u32) -> Task<Message> {
    let mb = mb.clamp(50, 10_000);
    app.map_cache_limit_bytes = (mb as u64) * 1024 * 1024;
    app.config.map_cache_limit_mb = mb;
    let _ = app.config.save();

    if let Some(ref mut c) = app.tile_cache {
        c.set_limit_bytes(app.map_cache_limit_bytes);
        let _ = c.evict_if_over_limit();
    }

    Task::none()
}

pub(crate) fn clear_cache(app: &mut PhotoVault) -> Task<Message> {
    let cache = app.tile_cache.clone();
    Task::perform(
        async move {
            if let Some(c) = cache {
                let _ = c.clear();
            }
        },
        |_| Message::MapCacheCleared,
    )
}

pub(crate) fn cache_cleared(_app: &mut PhotoVault) -> Task<Message> {
    tracing::info!("Map tile cache cleared");
    Task::none()
}

pub(crate) fn tile_fetched(app: &mut PhotoVault, tid: map_math::TileId) -> Task<Message> {
    if app.map_inflight_tiles.remove(&tid) {
        app.map_recent_fetch_failure = false;
    }
    enqueue_visible_tile_fetches(app)
}

pub(crate) fn queue_tile_fetch(app: &mut PhotoVault, tid: map_math::TileId) -> Task<Message> {
    let Some(cache) = app.tile_cache.clone() else {
        return Task::none();
    };

    if !app.map_inflight_tiles.insert(tid) {
        return Task::none();
    }

    Task::perform(
        async move {
            match cache.get_or_fetch(tid).await {
                Ok(_) => Message::MapTileFetched(tid),
                Err(err) => Message::MapTileFetchFailed(tid, err),
            }
        },
        |m| m,
    )
}

pub(crate) fn tile_fetch_failed(
    app: &mut PhotoVault,
    tid: map_math::TileId,
    err: String,
) -> Task<Message> {
    app.map_inflight_tiles.remove(&tid);
    tracing::warn!("Tile fetch failed: {}", err);
    app.map_recent_fetch_failure = true;
    enqueue_visible_tile_fetches(app)
}

pub(crate) fn enqueue_visible_tile_fetches(app: &mut PhotoVault) -> Task<Message> {
    let Some(cache) = app.tile_cache.clone() else {
        return Task::none();
    };

    let viewport = map_math::Viewport {
        width: viewport_size(app).0,
        height: viewport_size(app).1,
        center: app.map_center,
        zoom: app.map_zoom,
    };

    let mut tasks = Vec::new();
    for tid in map_math::visible_tiles(&viewport) {
        if cache.has(tid) || app.map_inflight_tiles.contains(&tid) {
            continue;
        }
        tasks.push(queue_tile_fetch(app, tid));
    }

    if tasks.is_empty() {
        Task::none()
    } else {
        Task::batch(tasks)
    }
}
