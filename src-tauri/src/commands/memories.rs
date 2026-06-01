//! Memories — N-years-ago rediscovery (read-only).

use chrono::Local;
use serde::Deserialize;
use tauri::State;

use crate::dto::{MemoryCardDto, MemoryDetailDto, PersonDto, PhotoSummaryDto};
use crate::state::AppState;
use crate::thumbnail_upgrade::{upgrade_covers_to_medium, CoverInput};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn memories_today(state: State<'_, AppState>) -> CommandResult<Vec<MemoryCardDto>> {
    let (cards, hero_inputs, drive_root, thumbs) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        let db = lib.db.lock().await;
        let today = Local::now().date_naive();
        if !smriti::services::memories::library_is_old_enough(&db.conn, today) {
            return Ok(Vec::new());
        }
        let cards = smriti::services::memories::generate_for_today(&db.conn, today)
            .map_err(|s| CommandError::internal(format!("memories: {s}")))?;
        let mut dtos: Vec<MemoryCardDto> = cards.into_iter().map(Into::into).collect();
        let inputs = collect_hero_inputs(&db.conn, &dtos)?;
        // The medium-thumbnail upgrade runs outside this lock to keep
        // the DB free for other handlers.
        for c in dtos.iter_mut() {
            c.hero_thumbnail_path = None;
        }
        (dtos, inputs, lib.drive_root.clone(), lib.thumbnails.clone())
    };

    let upgrades = upgrade_covers_to_medium(thumbs, drive_root, hero_inputs).await;
    let mut cards = cards;
    for (idx, path) in upgrades {
        if let Some(c) = cards.get_mut(idx) {
            c.hero_thumbnail_path = path;
        }
    }
    Ok(cards)
}

#[derive(Debug, Deserialize)]
pub struct MemoriesDetailArgs {
    pub memory_id: String,
}

#[tauri::command]
pub async fn memories_detail(
    state: State<'_, AppState>,
    args: MemoriesDetailArgs,
) -> CommandResult<MemoryDetailDto> {
    let (mut dto, photos, hero_input, drive_root, thumbs) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        let db = lib.db.lock().await;
        let today = Local::now().date_naive();
        let cards = smriti::services::memories::generate_for_today(&db.conn, today)
            .map_err(|s| CommandError::internal(format!("memories: {s}")))?;
        let card = cards
            .into_iter()
            .find(|c| c.id == args.memory_id)
            .ok_or_else(|| CommandError::not_found("memory", args.memory_id.clone()))?;

        let photo_repo = smriti::db::PhotoRepo::new(&db.conn);
        let photos = photo_repo.get_by_ids(&card.photo_ids)?;
        let dto: MemoryCardDto = card.into();
        let inputs = collect_hero_inputs(&db.conn, std::slice::from_ref(&dto))?;
        let summaries: Vec<PhotoSummaryDto> = photos.iter().map(PhotoSummaryDto::from).collect();
        (
            dto,
            summaries,
            inputs,
            lib.drive_root.clone(),
            lib.thumbnails.clone(),
        )
    };

    let upgrades = upgrade_covers_to_medium(thumbs, drive_root, hero_input).await;
    if let Some((_, path)) = upgrades.into_iter().next() {
        dto.hero_thumbnail_path = path;
    }
    Ok(MemoryDetailDto { card: dto, photos })
}

#[tauri::command]
pub async fn memories_blocked_people(state: State<'_, AppState>) -> CommandResult<Vec<PersonDto>> {
    // Returns clusters whose ID appears as a blocked target in `memory_blocks`.
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let conn = &db.conn;

    let mut blocked_ids: Vec<i64> = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT target_key FROM memory_blocks WHERE kind = 'person'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for r in rows {
            if let Ok(id) = r?.parse::<i64>() {
                blocked_ids.push(id);
            }
        }
    }
    if blocked_ids.is_empty() {
        return Ok(Vec::new());
    }

    let face_repo = smriti::db::face_repo::FaceRepo::new(conn);
    let all = face_repo.get_all_clusters()?;
    Ok(all
        .into_iter()
        .filter(|c| blocked_ids.contains(&c.id))
        .map(Into::into)
        .collect())
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct MemoriesPersonArgs {
    pub person_id: i64,
}

#[tauri::command]
pub async fn memories_block_person(
    state: State<'_, AppState>,
    args: MemoriesPersonArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    db.conn.execute(
        "INSERT OR IGNORE INTO memory_blocks (kind, target_key) VALUES ('person', ?1)",
        rusqlite::params![args.person_id.to_string()],
    )?;
    Ok(())
}

#[tauri::command]
pub async fn memories_unblock_person(
    state: State<'_, AppState>,
    args: MemoriesPersonArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    db.conn.execute(
        "DELETE FROM memory_blocks WHERE kind = 'person' AND target_key = ?1",
        rusqlite::params![args.person_id.to_string()],
    )?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct MemoriesSaveAsAlbumArgs {
    pub memory_id: String,
    pub name: Option<String>,
}

#[tauri::command]
pub async fn memories_save_as_album(
    state: State<'_, AppState>,
    args: MemoriesSaveAsAlbumArgs,
) -> CommandResult<crate::dto::AlbumDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let today = Local::now().date_naive();
    let cards = smriti::services::memories::generate_for_today(&db.conn, today)
        .map_err(|s| CommandError::internal(format!("memories: {s}")))?;
    let card = cards
        .into_iter()
        .find(|c| c.id == args.memory_id)
        .ok_or_else(|| CommandError::not_found("memory", args.memory_id.clone()))?;

    let album_repo = smriti::db::album_repo::AlbumRepo::new(&db.conn);
    let name = args.name.unwrap_or(card.title.clone());
    let normalized = name.trim().to_lowercase();
    if normalized == "favourites" || normalized == "favorites" {
        return Err(CommandError::Validation {
            field: "name".into(),
            reason: "reserved for the Favourites smart album".into(),
        });
    }
    let album_id = album_repo.create(&name)?;
    if !card.photo_ids.is_empty() {
        album_repo.add_photos(album_id, &card.photo_ids)?;
        album_repo.auto_pick_cover(album_id)?;
    }
    album_repo
        .get_all()?
        .into_iter()
        .find(|a| a.id == album_id)
        .map(Into::into)
        .ok_or_else(|| CommandError::not_found("album", album_id))
}

/// Collects (card_index, file_path, file_hash, orientation) for every
/// card whose hero photo exists. Feeds `upgrade_covers_to_medium` so
/// the medium thumbnail generation runs outside the DB lock.
fn collect_hero_inputs(
    conn: &rusqlite::Connection,
    cards: &[MemoryCardDto],
) -> rusqlite::Result<Vec<CoverInput>> {
    if cards.is_empty() {
        return Ok(Vec::new());
    }
    let repo = smriti::db::PhotoRepo::new(conn);
    let mut out = Vec::with_capacity(cards.len());
    for (idx, c) in cards.iter().enumerate() {
        if let Some(p) = repo.get_by_id(c.hero_photo_id)? {
            out.push((idx, p.file_path, p.file_hash, p.orientation));
        }
    }
    Ok(out)
}
