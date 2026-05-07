//! Documents (OCR'd text-bearing photos) — read-only listing & FTS search.

use serde::Deserialize;
use tauri::State;

use smriti::db::document_repo::DocumentRepo;

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
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DocumentRepo::new(&db.conn);

    let limit = pagination::clamp_limit(args.limit) as i64;
    // Translate cursor.id (which we treat as offset for these legacy
    // offset-based repo methods) into an offset.
    let offset = pagination::decode(args.cursor.as_deref())?
        .map(|c| c.id)
        .unwrap_or(0);

    let photos = match args.categories {
        Some(cats) if !cats.is_empty() => {
            // Repo supports a single category at a time; concatenate
            // results from each requested category. Capped by `limit`
            // overall — first-come ordering, since the per-call SQL
            // already orders by date_taken DESC.
            let mut all: Vec<smriti::models::Photo> = Vec::new();
            for c in cats {
                let name = smriti::models::ContentCategory::from(c)
                    .as_str()
                    .to_string();
                let rows = repo.get_documents_by_category(&name, limit, offset)?;
                all.extend(rows);
                if all.len() as i64 >= limit {
                    all.truncate(limit as usize);
                    break;
                }
            }
            all
        }
        _ => repo.get_non_photo_documents(limit, offset)?,
    };
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
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DocumentRepo::new(&db.conn);
    let limit = pagination::clamp_limit(args.limit) as i64;
    let offset = pagination::decode(args.cursor.as_deref())?
        .map(|c| c.id)
        .unwrap_or(0);
    let photos = repo.search_documents_fts(&args.q, limit, offset)?;
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
    DocumentRepo::new(&db.conn).update_content_category(args.photo_id, cat.as_str())?;
    Ok(())
}
