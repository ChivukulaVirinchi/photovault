//! Documents (OCR'd text-bearing photos) — read-only listing & FTS search.

use serde::Deserialize;
use tauri::State;

use smriti::db::document_repo::DocumentRepo;
use smriti::db::{db_path_for, open_secondary};

use crate::dto::{ContentCategoryDto, Page, PhotoSummaryDto};
use crate::pagination;
use crate::state::AppState;
use crate::{CommandError, CommandResult};

#[derive(Debug, Default, Deserialize)]
pub struct DocumentsListArgs {
    pub categories: Option<Vec<ContentCategoryDto>>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn documents_list(
    state: State<'_, AppState>,
    args: DocumentsListArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        db_path_for(&lib.drive_root)
    };

    let limit = pagination::clamp_limit(args.limit) as i64;
    // Translate cursor.id (which we treat as offset for these legacy
    // offset-based repo methods) into an offset.
    let offset = pagination::decode(args.cursor.as_deref())?
        .map(|c| c.id)
        .unwrap_or(0);

    let photos = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let repo = DocumentRepo::new(&conn);
        let photos = match args.categories {
            Some(cats) if !cats.is_empty() => {
                let names = cats
                    .into_iter()
                    .map(|c| {
                        smriti::models::ContentCategory::from(c)
                            .as_str()
                            .to_string()
                    })
                    .collect::<Vec<_>>();
                repo.get_documents_by_categories(&names, limit, offset)?
            }
            _ => repo.get_non_photo_documents(limit, offset)?,
        };
        Ok::<_, CommandError>(photos)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("document list worker failed: {e}"),
    })??;
    let has_more = photos.len() as i64 == limit;
    let next_cursor = if has_more {
        Some(pagination::encode(crate::pagination::Cursor {
            date_taken: None,
            id: offset + photos.len() as i64,
        }))
    } else {
        None
    };
    Ok(Page {
        items: photos.iter().map(PhotoSummaryDto::from).collect(),
        next_cursor,
        has_more,
        total: None,
    })
}

#[derive(Debug, Deserialize)]
pub struct DocumentsSearchArgs {
    pub q: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub async fn documents_search(
    state: State<'_, AppState>,
    args: DocumentsSearchArgs,
) -> CommandResult<Page<PhotoSummaryDto>> {
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        db_path_for(&lib.drive_root)
    };
    let limit = pagination::clamp_limit(args.limit) as i64;
    let offset = pagination::decode(args.cursor.as_deref())?
        .map(|c| c.id)
        .unwrap_or(0);
    let photos = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        Ok::<_, CommandError>(
            DocumentRepo::new(&conn).search_documents_fts(&args.q, limit, offset)?,
        )
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("document search worker failed: {e}"),
    })??;
    let has_more = photos.len() as i64 == limit;
    let next_cursor = if has_more {
        Some(pagination::encode(crate::pagination::Cursor {
            date_taken: None,
            id: offset + photos.len() as i64,
        }))
    } else {
        None
    };
    Ok(Page {
        items: photos.iter().map(PhotoSummaryDto::from).collect(),
        next_cursor,
        has_more,
        total: None,
    })
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct DocumentsSetCategoryArgs {
    pub photo_id: i64,
    pub category: ContentCategoryDto,
}

#[tauri::command]
pub async fn documents_set_category(
    state: State<'_, AppState>,
    args: DocumentsSetCategoryArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let cat = smriti::models::ContentCategory::from(args.category);
    let changed =
        DocumentRepo::new(&db.conn).update_content_category(args.photo_id, cat.as_str())?;
    if changed == 0 {
        return Err(CommandError::not_found("photo", args.photo_id));
    }
    Ok(())
}
