//! Search and cull-mode handlers.

use iced::Task;

use crate::db::Database;
use crate::services::{SearchService, UnifiedSearchResults};
use crate::views::CullState;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn search_input_changed(app: &mut PhotoVault, input: String) -> Task<Message> {
    app.search_query = input.clone();
    app.search_generation = app.search_generation.wrapping_add(1);
    let gen = app.search_generation;

    if input.trim().is_empty() {
        // Clear results, show recent searches instead
        app.search_results = None;
        app.search_loading = false;
        return Task::none();
    }

    // Schedule debounced search
    Task::perform(
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            gen
        },
        Message::SearchDebouncedTick,
    )
}

pub(crate) fn search_debounced_tick(app: &mut PhotoVault, gen: u64) -> Task<Message> {
    // If input changed since this debounce was scheduled, skip
    if gen != app.search_generation {
        return Task::none();
    }
    if app.search_query.trim().is_empty() {
        return Task::none();
    }
    execute_search(app)
}

pub(crate) fn execute_search(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let query_text = app.search_query.clone();
    let drive_path = drive_path.clone();

    if query_text.trim().is_empty() {
        return Task::none();
    }

    // Bump generation so any in-flight debounced search with the prior
    // generation is discarded on return.
    app.search_generation = app.search_generation.wrapping_add(1);
    let gen = app.search_generation;

    app.search_loading = true;

    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                let db = Database::open_for_drive(&drive_path)
                    .map_err(|e| format!("DB open: {}", e))?;
                let mut results = SearchService::search_unified(&db.conn, &query_text)
                    .map_err(|e| format!("Search: {}", e))?;

                // Resolve face thumbnails for people hits
                for person in &mut results.people {
                    let face_id: Option<i64> = db
                        .conn
                        .query_row(
                            "SELECT id FROM faces WHERE cluster_id = ?1 ORDER BY confidence DESC LIMIT 1",
                            rusqlite::params![person.cluster_id],
                            |row| row.get(0),
                        )
                        .ok();
                    if let Some(fid) = face_id {
                        let crop = drive_path
                            .join(".photovault")
                            .join("faces")
                            .join(format!("{}.jpg", fid));
                        if crop.exists() {
                            person.face_thumbnail_path =
                                Some(crop.to_string_lossy().to_string());
                        } else {
                            let legacy = drive_path
                                .join(".photovault")
                                .join("face_crops")
                                .join(format!("{}.jpg", fid));
                            if legacy.exists() {
                                person.face_thumbnail_path =
                                    Some(legacy.to_string_lossy().to_string());
                            }
                        }
                    }
                }

                // Resolve album cover thumbnails
                let photo_repo = crate::db::PhotoRepo::new(&db.conn);
                for album in &mut results.albums {
                    let cover_id: Option<i64> = db
                        .conn
                        .query_row(
                            "SELECT cover_photo_id FROM albums WHERE id = ?1",
                            rusqlite::params![album.album_id],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten();
                    if let Some(cid) = cover_id {
                        if let Ok(Some(p)) = photo_repo.get_by_id(cid) {
                            if let Some(tp) = p.thumbnail_path {
                                let abs = drive_path.join(&tp);
                                if abs.exists() {
                                    album.cover_thumbnail_path =
                                        Some(abs.to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                }

                Ok::<_, String>(results)
            })
            .await;

            match result {
                Ok(Ok(r)) => (gen, Box::new(r)),
                Ok(Err(e)) => {
                    tracing::warn!("unified search failed: {}", e);
                    (gen, Box::new(UnifiedSearchResults::default()))
                }
                Err(e) => {
                    tracing::warn!("unified search task panicked: {}", e);
                    (gen, Box::new(UnifiedSearchResults::default()))
                }
            }
        },
        |(gen, results)| Message::SearchComplete(gen, results),
    )
}

pub(crate) fn search_complete(
    app: &mut PhotoVault,
    gen: u64,
    results: UnifiedSearchResults,
) -> Task<Message> {
    // Discard stale results from older generations
    if gen != app.search_generation {
        return Task::none();
    }
    app.search_loading = false;
    app.search_results = Some(results);

    // Record this search as recent (best-effort, non-blocking)
    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        let query = app.search_query.clone();
        return Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    if let Ok(db) = Database::open_for_drive(&drive_path) {
                        let repo = crate::db::RecentSearchRepo::new(&db.conn);
                        let _ = repo.record(&query);
                        repo.get_recent(10).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                })
                .await
                .unwrap_or_default()
            },
            Message::RecentSearchesLoaded,
        );
    }
    Task::none()
}

pub(crate) fn recent_searches_loaded(
    app: &mut PhotoVault,
    list: Vec<crate::db::RecentSearch>,
) -> Task<Message> {
    app.recent_searches = list;
    Task::none()
}

pub(crate) fn search_recent_selected(app: &mut PhotoVault, query: String) -> Task<Message> {
    app.search_query = query;
    app.search_generation = app.search_generation.wrapping_add(1);
    execute_search(app)
}

pub(crate) fn search_recent_remove(app: &mut PhotoVault, query: String) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        let _ = crate::db::RecentSearchRepo::new(&db.conn).remove(&query);
    }
    app.recent_searches.retain(|r| r.query != query);
    Task::none()
}

pub(crate) fn search_clear_recent(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        let _ = crate::db::RecentSearchRepo::new(&db.conn).clear();
    }
    app.recent_searches.clear();
    Task::none()
}

pub(crate) fn load_recent_searches(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };
    let drive_path = drive_path.clone();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                if let Ok(db) = Database::open_for_drive(&drive_path) {
                    crate::db::RecentSearchRepo::new(&db.conn)
                        .get_recent(10)
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            })
            .await
            .unwrap_or_default()
        },
        Message::RecentSearchesLoaded,
    )
}

pub(crate) fn search_open_person(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    super::handle(app, Message::SelectCluster(cluster_id))
}

pub(crate) fn search_open_album(app: &mut PhotoVault, album_id: i64) -> Task<Message> {
    super::handle(app, Message::OpenAlbum(album_id))
}

pub(crate) fn search_open_place(app: &mut PhotoVault, city: String) -> Task<Message> {
    // Re-run search scoped to that city
    app.search_query = city;
    app.search_generation = app.search_generation.wrapping_add(1);
    execute_search(app)
}

pub(crate) fn enter_cull_from_search(app: &mut PhotoVault) -> Task<Message> {
    let ids: Vec<i64> = app
        .search_results
        .as_ref()
        .map(|r| r.photo_ids.clone())
        .unwrap_or_default();
    if ids.is_empty() {
        return Task::none();
    }
    super::handle(app, Message::EnterCullMode(ids))
}

pub(crate) fn enter_cull_mode(app: &mut PhotoVault, photo_ids: Vec<i64>) -> Task<Message> {
    if photo_ids.is_empty() {
        return Task::none();
    }
    app.cull_return_view = Some(app.current_view.clone());
    app.cull_state = Some(CullState::new(photo_ids));
    app.cull_confirm_pending = false;
    app.current_view = View::Cull;
    Task::none()
}

pub(crate) fn exit_cull_mode(app: &mut PhotoVault) -> Task<Message> {
    app.cull_state = None;
    app.cull_confirm_pending = false;
    app.current_view = app.cull_return_view.take().unwrap_or(View::Timeline);
    Task::none()
}

pub(crate) fn cull_next(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.next();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_prev(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.prev();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_toggle_trash(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.toggle_trash();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_undo(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.undo();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_finish(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref cull) = app.cull_state {
        if cull.marked_for_trash.is_empty() {
            return super::handle(app, Message::ExitCullMode);
        }
        app.cull_confirm_pending = true;
        return Task::none();
    }
    Task::none()
}

pub(crate) fn cull_confirm_trash(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref cull) = app.cull_state {
        let ids = cull.marked_for_trash.iter().copied().collect::<Vec<_>>();
        app.cull_confirm_pending = false;
        return super::handle(app, Message::TrashPhotos(ids));
    }
    Task::none()
}
