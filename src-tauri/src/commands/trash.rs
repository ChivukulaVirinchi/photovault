//! Trash (read-only — list & stats. Trash/restore/delete are M2).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use tauri::State;

use smriti::db::trash_repo::TrashRepo;
use smriti::services::trash::TrashService;

use crate::dto::{Page, TrashStatsDto, TrashedPhotoDto};
use crate::pagination;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

const MAX_BULK_PHOTO_IDS: usize = 10_000;

fn encode_trash_cursor(trashed_at: &str, photo_id: i64) -> String {
    URL_SAFE_NO_PAD.encode(format!("{trashed_at}|{photo_id}"))
}

fn decode_trash_cursor(raw: Option<&str>) -> CommandResult<Option<(String, i64)>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| CommandError::Validation {
            field: "cursor".into(),
            reason: "not base64".into(),
        })?;
    let decoded = std::str::from_utf8(&bytes).map_err(|_| CommandError::Validation {
        field: "cursor".into(),
        reason: "not utf8".into(),
    })?;
    let (trashed_at, id) = decoded.rsplit_once('|').ok_or(CommandError::Validation {
        field: "cursor".into(),
        reason: "malformed".into(),
    })?;
    let photo_id = id.parse().map_err(|_| CommandError::Validation {
        field: "cursor".into(),
        reason: "id not i64".into(),
    })?;
    if trashed_at.is_empty() {
        return Ok(None);
    }
    Ok(Some((trashed_at.to_string(), photo_id)))
}

fn normalize_bulk_photo_ids(ids: Vec<i64>) -> CommandResult<Vec<i64>> {
    if ids.len() > MAX_BULK_PHOTO_IDS {
        return Err(CommandError::Validation {
            field: "photo_ids".into(),
            reason: format!("too many ids; maximum is {MAX_BULK_PHOTO_IDS}"),
        });
    }
    let mut seen = std::collections::HashSet::new();
    Ok(ids
        .into_iter()
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect())
}

#[derive(Debug, Default, Deserialize)]
pub struct TrashListArgs {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn trash_list(
    state: State<'_, AppState>,
    args: TrashListArgs,
) -> CommandResult<Page<TrashedPhotoDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = TrashRepo::new(&db.conn);

    let limit = pagination::clamp_limit(args.limit) as usize;
    let cursor = decode_trash_cursor(args.cursor.as_deref())?;
    let mut rows = repo.page_after(cursor.clone(), limit as i64 + 1)?;
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }
    let next_cursor = if has_more {
        rows.last()
            .map(|t| encode_trash_cursor(&t.trashed_at, t.photo_id))
    } else {
        None
    };
    let total = if cursor.is_none() {
        Some(repo.count_all()?.max(0) as u64)
    } else {
        None
    };
    Ok(Page {
        items: rows.into_iter().map(Into::into).collect(),
        next_cursor,
        has_more,
        total,
    })
}

#[tauri::command]
pub async fn trash_stats(state: State<'_, AppState>) -> CommandResult<TrashStatsDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    Ok(TrashService::get_stats(&db.conn)?.into())
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct TrashPhotoIdsArgs {
    pub photo_ids: Vec<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct TrashCountDto {
    pub count: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct TrashDeleteResultDto {
    pub files_deleted: u64,
    pub db_records_deleted: u64,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn trash_trash_photos(
    state: State<'_, AppState>,
    args: TrashPhotoIdsArgs,
) -> CommandResult<TrashCountDto> {
    let photo_ids = normalize_bulk_photo_ids(args.photo_ids)?;
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let n = TrashService::trash_photos(&db.conn, &photo_ids)? as u64;
    Ok(TrashCountDto { count: n })
}

#[tauri::command]
pub async fn trash_restore(
    state: State<'_, AppState>,
    args: TrashPhotoIdsArgs,
) -> CommandResult<TrashCountDto> {
    let photo_ids = normalize_bulk_photo_ids(args.photo_ids)?;
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let n = TrashService::restore_photos(&db.conn, &photo_ids)? as u64;
    Ok(TrashCountDto { count: n })
}

#[tauri::command]
pub async fn trash_permanent_delete(
    state: State<'_, AppState>,
    args: TrashPhotoIdsArgs,
) -> CommandResult<TrashDeleteResultDto> {
    let photo_ids = normalize_bulk_photo_ids(args.photo_ids)?;
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);
    let r = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(TrashService::permanent_delete(
            &conn,
            &photo_ids,
            &drive_root,
        )?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("trash delete worker failed: {e}"),
    })??;
    Ok(TrashDeleteResultDto {
        files_deleted: r.files_deleted as u64,
        db_records_deleted: r.db_records_deleted as u64,
        errors: r.errors,
    })
}

#[tauri::command]
pub async fn trash_empty(state: State<'_, AppState>) -> CommandResult<TrashDeleteResultDto> {
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);
    let r = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(TrashService::empty_trash(&conn, &drive_root)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("empty trash worker failed: {e}"),
    })??;
    Ok(TrashDeleteResultDto {
        files_deleted: r.files_deleted as u64,
        db_records_deleted: r.db_records_deleted as u64,
        errors: r.errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_photo_ids_are_positive_deduped_and_bounded() {
        assert_eq!(
            normalize_bulk_photo_ids(vec![7, 0, 7, -3, 8]).unwrap(),
            vec![7, 8]
        );
        assert!(normalize_bulk_photo_ids(vec![1; MAX_BULK_PHOTO_IDS + 1]).is_err());
    }

    #[test]
    fn trash_cursor_roundtrips_raw_trashed_at() {
        let cursor = encode_trash_cursor("2026-01-02 03:04:05", 42);
        assert_eq!(
            decode_trash_cursor(Some(&cursor)).unwrap(),
            Some(("2026-01-02 03:04:05".into(), 42))
        );
    }

    #[test]
    fn empty_trash_cursor_is_first_page() {
        let cursor = URL_SAFE_NO_PAD.encode("|42");
        assert_eq!(decode_trash_cursor(Some(&cursor)).unwrap(), None);
    }
}
