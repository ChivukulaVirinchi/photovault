//! Memories — N-years-ago rediscovery (read-only).

use chrono::Local;
use serde::Deserialize;
use tauri::State;

use crate::dto::{MemoryCardDto, MemoryDetailDto, PersonDto, PhotoSummaryDto};
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn memories_today(state: State<'_, AppState>) -> CommandResult<Vec<MemoryCardDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let today = Local::now().date_naive();
    if !photovault::services::memories::library_is_old_enough(&db.conn, today) {
        return Ok(Vec::new());
    }
    let cards = photovault::services::memories::generate_for_today(&db.conn, today)
        .map_err(|s| CommandError::internal(format!("memories: {s}")))?;
    Ok(cards.into_iter().map(Into::into).collect())
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
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let today = Local::now().date_naive();
    let cards = photovault::services::memories::generate_for_today(&db.conn, today)
        .map_err(|s| CommandError::internal(format!("memories: {s}")))?;
    let card = cards
        .into_iter()
        .find(|c| c.id == args.memory_id)
        .ok_or_else(|| CommandError::not_found("memory", args.memory_id.clone()))?;

    let photo_repo = photovault::db::PhotoRepo::new(&db.conn);
    let photos = photo_repo.get_by_ids(&card.photo_ids)?;
    let dto: MemoryCardDto = card.into();
    Ok(MemoryDetailDto {
        card: dto,
        photos: photos.iter().map(PhotoSummaryDto::from).collect(),
    })
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

    let face_repo = photovault::db::face_repo::FaceRepo::new(conn);
    let all = face_repo.get_all_clusters()?;
    Ok(all
        .into_iter()
        .filter(|c| blocked_ids.contains(&c.id))
        .map(Into::into)
        .collect())
}
