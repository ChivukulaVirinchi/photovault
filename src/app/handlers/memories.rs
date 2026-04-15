//! Message handlers for the Memories feature.

use iced::Task;

use crate::db::Database;
use crate::services::{memories, MemoryCard};

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

/// 60-second tick: check whether we've crossed a day boundary; if so, regen.
pub(crate) fn tick(app: &mut PhotoVault) -> Task<Message> {
    if !app.memories_enabled {
        return Task::none();
    }
    let today = chrono::Local::now().date_naive();
    if app.memories_for_date == Some(today) {
        return Task::none();
    }
    regenerate(app, today)
}

/// Spawn the async generator pipeline. Called at drive-select, day-rollover,
/// and after the user re-enables memories in Settings.
pub(crate) fn regenerate(app: &mut PhotoVault, today: chrono::NaiveDate) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };

    app.memories_for_date = Some(today);

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let db = match Database::open_for_drive(&drive_path) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("memories::regenerate: open DB failed: {}", e);
                        return Vec::new();
                    }
                };
                if !memories::library_is_old_enough(&db.conn, today) {
                    return Vec::new();
                }
                match memories::generate_for_today(&db.conn, today) {
                    Ok(cards) => cards,
                    Err(e) => {
                        tracing::warn!("memories::generate_for_today: {}", e);
                        Vec::new()
                    }
                }
            })
            .await
            .unwrap_or_default()
        },
        Message::MemoriesRegenerated,
    )
}

/// Populate hero thumbnail paths then stash the cards in app state.
pub(crate) fn regenerated(app: &mut PhotoVault, cards: Vec<MemoryCard>) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        app.memories = cards;
        return Task::none();
    };

    let mut enriched = cards;
    if let Ok(db) = Database::open_for_drive(&drive_path) {
        for card in enriched.iter_mut() {
            let rel: Option<String> = db
                .conn
                .query_row(
                    "SELECT thumbnail_path FROM photos WHERE id = ?1",
                    rusqlite::params![card.hero_photo_id],
                    |row| row.get(0),
                )
                .ok()
                .flatten();
            card.hero_thumbnail_path =
                rel.map(|p| drive_path.join(p).to_string_lossy().to_string());
        }
    }

    tracing::info!("Memories regenerated: {} cards", enriched.len());
    app.memories = enriched;
    Task::none()
}

pub(crate) fn open_memory(app: &mut PhotoVault, id: String) -> Task<Message> {
    // Resolve which photos belong to this memory (look them up in app.photos).
    let photos: Vec<crate::models::Photo> = app
        .memories
        .iter()
        .find(|c| c.id == id)
        .map(|card| {
            card.photo_ids
                .iter()
                .filter_map(|pid| app.photos.iter().find(|p| p.id == *pid).cloned())
                .collect()
        })
        .unwrap_or_default();

    app.previous_view = Some(app.current_view.clone());
    app.selected_memory_id = Some(id);
    app.memory_photos = photos;
    app.memory_slideshow_index = 0;
    app.memory_slideshow_paused = false;
    app.current_view = View::MemoryDetail;
    Task::none()
}

pub(crate) fn slideshow_tick(app: &mut PhotoVault) -> Task<Message> {
    if app.current_view != View::MemoryDetail || app.memory_slideshow_paused {
        return Task::none();
    }
    advance(app, 1);
    Task::none()
}

pub(crate) fn slideshow_next(app: &mut PhotoVault) -> Task<Message> {
    advance(app, 1);
    Task::none()
}

pub(crate) fn slideshow_prev(app: &mut PhotoVault) -> Task<Message> {
    advance(app, -1);
    Task::none()
}

pub(crate) fn slideshow_toggle_pause(app: &mut PhotoVault) -> Task<Message> {
    app.memory_slideshow_paused = !app.memory_slideshow_paused;
    Task::none()
}

fn advance(app: &mut PhotoVault, delta: i32) {
    let n = app.memory_photos.len();
    if n == 0 {
        return;
    }
    let idx = app.memory_slideshow_index as i32 + delta;
    let n_i = n as i32;
    let wrapped = ((idx % n_i) + n_i) % n_i;
    app.memory_slideshow_index = wrapped as usize;
}

pub(crate) fn close_memory_detail(app: &mut PhotoVault) -> Task<Message> {
    app.selected_memory_id = None;
    app.current_view = app.previous_view.take().unwrap_or(View::Timeline);
    Task::none()
}

pub(crate) fn block_memories_for_person(app: &mut PhotoVault, cluster_id: i64) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };

    if let Ok(db) = Database::open_for_drive(&drive_path) {
        if let Err(e) = db.conn.execute(
            "INSERT OR IGNORE INTO memory_blocks (kind, target_key) VALUES ('person', ?1)",
            rusqlite::params![cluster_id.to_string()],
        ) {
            tracing::warn!("block_memories_for_person: insert failed: {}", e);
        }
    }

    // Instant feedback: drop cards whose photos feature this cluster.
    let drive_path_ref = drive_path.clone();
    let filtered: Vec<MemoryCard> = app
        .memories
        .iter()
        .filter(|card| !memory_involves_cluster(&drive_path_ref, card, cluster_id))
        .cloned()
        .collect();
    app.memories = filtered;

    Task::none()
}

pub(crate) fn set_memories_enabled(app: &mut PhotoVault, enabled: bool) -> Task<Message> {
    app.memories_enabled = enabled;
    app.config.memories_enabled = enabled;
    if let Err(e) = app.config.save() {
        tracing::warn!("Failed to save config: {}", e);
    }

    if !enabled {
        app.memories.clear();
        app.memories_for_date = None;
        Task::none()
    } else {
        let today = chrono::Local::now().date_naive();
        regenerate(app, today)
    }
}

fn memory_involves_cluster(
    drive_path: &std::path::Path,
    card: &MemoryCard,
    cluster_id: i64,
) -> bool {
    let Ok(db) = Database::open_for_drive(drive_path) else {
        return false;
    };
    if card.photo_ids.is_empty() {
        return false;
    }
    let in_clause: Vec<String> = card.photo_ids.iter().map(|id| id.to_string()).collect();
    let sql = format!(
        "SELECT COUNT(*) FROM faces WHERE cluster_id = ?1 AND photo_id IN ({})",
        in_clause.join(",")
    );
    let count: i64 = db
        .conn
        .query_row(&sql, rusqlite::params![cluster_id], |row| row.get(0))
        .unwrap_or(0);
    count > 0
}
