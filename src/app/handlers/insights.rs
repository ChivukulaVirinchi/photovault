//! Message handlers for the Insights Dashboard.

use iced::Task;

use crate::db::Database;
use crate::services::insights;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn invalidate(app: &mut PhotoVault) -> Task<Message> {
    app.invalidate_insights_cache();
    Task::none()
}

/// User selected a year (or All Time) in the insights view.
pub(crate) fn select_year(app: &mut PhotoVault, year: Option<i32>) -> Task<Message> {
    app.insights_selected_year = year;
    if let Some(cached) = app.insights_cache.get(&year).cloned() {
        app.insights_data = Some(cached);
        app.insights_loading = false;
        return Task::none();
    }
    load_insights(app)
}

/// Insights data finished loading.
pub(crate) fn loaded(app: &mut PhotoVault, data: Box<insights::InsightsData>) -> Task<Message> {
    app.insights_loading = false;
    let data = *data;
    app.insights_cache
        .insert(app.insights_selected_year, data.clone());
    app.insights_data = Some(data);
    Task::none()
}

/// Jump to search view with a date query (from heatmap click).
pub(crate) fn jump_to_date(app: &mut PhotoVault, date: String) -> Task<Message> {
    let mut scoped: Vec<crate::models::Photo> = app
        .photos
        .iter()
        .filter(|p| {
            p.date_taken
                .map(|d| d.format("%Y-%m-%d").to_string())
                .as_deref()
                == Some(date.as_str())
        })
        .cloned()
        .collect();

    if scoped.is_empty() {
        app.search_query = date;
        app.current_view = View::Search;
        return super::handle(app, Message::ExecuteSearch);
    }

    scoped.sort_by_key(|p| p.date_taken);
    let first_id = scoped[0].id;
    app.insights_scope_photos = scoped;
    app.previous_view = Some(View::Insights);
    super::handle(app, Message::SelectPhoto(first_id))
}

pub(crate) fn open_month(app: &mut PhotoVault, year: i32, month: u32) -> Task<Message> {
    if !(1..=12).contains(&month) {
        return Task::none();
    }

    let mut scoped: Vec<crate::models::Photo> = app
        .photos
        .iter()
        .filter(|p| {
            p.date_taken
                .map(|d| d.year() == year && d.month() == month)
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    if scoped.is_empty() {
        return Task::none();
    }

    scoped.sort_by_key(|p| p.date_taken);
    let first_id = scoped[0].id;
    app.insights_scope_photos = scoped;
    app.previous_view = Some(View::Insights);
    super::handle(app, Message::SelectPhoto(first_id))
}

/// Jump to search view with a city query (from top-locations click).
pub(crate) fn search_city(app: &mut PhotoVault, city: String) -> Task<Message> {
    app.search_query = city;
    app.current_view = View::Search;
    super::handle(app, Message::ExecuteSearch)
}

/// Spawn the async insights computation.
pub(crate) fn load_insights(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    if let Some(cached) = app.insights_cache.get(&app.insights_selected_year).cloned() {
        app.insights_data = Some(cached);
        app.insights_loading = false;
        return Task::none();
    }

    app.insights_loading = true;
    let drive_path = drive_path.clone();
    let year = app.insights_selected_year;

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let db = match Database::open_for_drive(&drive_path) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("insights: open DB failed: {}", e);
                        return Box::new(insights::InsightsData {
                            total_photos: 0,
                            date_range_start: None,
                            date_range_end: None,
                            people_count: 0,
                            album_count: 0,
                            country_count: 0,
                            city_count: 0,
                            photos_with_gps: 0,
                            hero_photo_id: None,
                            hero_thumbnail_path: None,
                            heatmap: std::collections::HashMap::new(),
                            heatmap_year: chrono::Local::now().date_naive().year(),
                            monthly_counts: [0; 12],
                            top_people: Vec::new(),
                            top_locations: Vec::new(),
                            top_cameras: Vec::new(),
                            available_years: Vec::new(),
                        });
                    }
                };

                let mut data = match insights::compute(&db.conn, year) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("insights::compute failed: {}", e);
                        return Box::new(insights::InsightsData {
                            total_photos: 0,
                            date_range_start: None,
                            date_range_end: None,
                            people_count: 0,
                            album_count: 0,
                            country_count: 0,
                            city_count: 0,
                            photos_with_gps: 0,
                            hero_photo_id: None,
                            hero_thumbnail_path: None,
                            heatmap: std::collections::HashMap::new(),
                            heatmap_year: chrono::Local::now().date_naive().year(),
                            monthly_counts: [0; 12],
                            top_people: Vec::new(),
                            top_locations: Vec::new(),
                            top_cameras: Vec::new(),
                            available_years: Vec::new(),
                        });
                    }
                };

                // Resolve hero thumbnail path — prefer original file, fall
                // back to thumbnail (same pattern as memories hero).
                if let Some(hero_id) = data.hero_photo_id {
                    let orig: Option<String> = db
                        .conn
                        .query_row(
                            "SELECT file_path FROM photos WHERE id = ?1",
                            rusqlite::params![hero_id],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten();
                    let orig_abs = orig.map(|p| drive_path.join(&p)).filter(|p| p.exists());
                    if let Some(p) = orig_abs {
                        data.hero_thumbnail_path = Some(p.to_string_lossy().to_string());
                    } else {
                        let rel: Option<String> = db
                            .conn
                            .query_row(
                                "SELECT thumbnail_path FROM photos WHERE id = ?1",
                                rusqlite::params![hero_id],
                                |row| row.get(0),
                            )
                            .ok()
                            .flatten();
                        data.hero_thumbnail_path =
                            rel.map(|p| drive_path.join(p).to_string_lossy().to_string());
                    }
                }

                // Resolve face crop paths for top people.
                // Face crops live at .photovault/faces/{face_id}.jpg.
                for person in &mut data.top_people {
                    if let Some(face_id) = person.face_id {
                        let crop_path = drive_path
                            .join(".photovault")
                            .join("faces")
                            .join(format!("{}.jpg", face_id));
                        if crop_path.exists() {
                            person.face_crop_path = Some(crop_path.to_string_lossy().to_string());
                        } else {
                            // Backward-compat for older paths.
                            let legacy = drive_path
                                .join(".photovault")
                                .join("face_crops")
                                .join(format!("{}.jpg", face_id));
                            if legacy.exists() {
                                person.face_crop_path = Some(legacy.to_string_lossy().to_string());
                            }
                        }
                    }
                }

                Box::new(data)
            })
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("insights spawn_blocking panicked: {}", e);
                Box::new(insights::InsightsData {
                    total_photos: 0,
                    date_range_start: None,
                    date_range_end: None,
                    people_count: 0,
                    album_count: 0,
                    country_count: 0,
                    city_count: 0,
                    photos_with_gps: 0,
                    hero_photo_id: None,
                    hero_thumbnail_path: None,
                    heatmap: std::collections::HashMap::new(),
                    heatmap_year: chrono::Local::now().date_naive().year(),
                    monthly_counts: [0; 12],
                    top_people: Vec::new(),
                    top_locations: Vec::new(),
                    top_cameras: Vec::new(),
                    available_years: Vec::new(),
                })
            })
        },
        Message::InsightsLoaded,
    )
}

use chrono::Datelike;
