//! Duplicate groups (read-only listing).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use photovault::db::duplicate_repo::DuplicateRepo;
use photovault::services::duplicate_detector::DuplicateDetector;

use crate::dto::{DuplicateGroupDto, DuplicateGroupSummaryDto, DuplicateMemberDto, JobIdDto};
use crate::events::{JobProgress, EV_DUPLICATES_COMPLETE, EV_DUPLICATES_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn duplicates_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<DuplicateGroupSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DuplicateRepo::new(&db.conn);
    Ok(repo.get_all_groups()?.into_iter().map(Into::into).collect())
}

#[derive(Debug, Deserialize)]
pub struct DuplicatesGetGroupArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn duplicates_get_group(
    state: State<'_, AppState>,
    args: DuplicatesGetGroupArgs,
) -> CommandResult<DuplicateGroupDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DuplicateRepo::new(&db.conn);
    let members = repo.get_group_members(args.id)?;
    if members.is_empty() {
        return Err(CommandError::not_found("duplicate_group", args.id));
    }
    Ok(DuplicateGroupDto {
        id: args.id,
        members: members.into_iter().map(DuplicateMemberDto::from).collect(),
    })
}

#[derive(Debug, serde::Serialize)]
pub struct WastedSpaceDto {
    pub bytes: u64,
}

#[tauri::command]
pub async fn duplicates_wasted_space(state: State<'_, AppState>) -> CommandResult<WastedSpaceDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let bytes = DuplicateDetector::calculate_wasted_space(&db.conn)?;
    Ok(WastedSpaceDto { bytes })
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct DuplicatesSetKeepArgs {
    pub group_id: i64,
    pub photo_id: i64,
}

#[tauri::command]
pub async fn duplicates_set_keep(
    state: State<'_, AppState>,
    args: DuplicatesSetKeepArgs,
) -> CommandResult<DuplicateGroupDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DuplicateRepo::new(&db.conn);
    repo.set_keep_photo(args.group_id, args.photo_id)?;
    let members = repo.get_group_members(args.group_id)?;
    Ok(DuplicateGroupDto {
        id: args.group_id,
        members: members.into_iter().map(DuplicateMemberDto::from).collect(),
    })
}

#[derive(Debug, Deserialize)]
pub struct DuplicatesGroupActionArgs {
    pub group_id: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct CountResultDto {
    pub count: u64,
}

#[tauri::command]
pub async fn duplicates_trash_others(
    state: State<'_, AppState>,
    args: DuplicatesGroupActionArgs,
) -> CommandResult<CountResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = DuplicateRepo::new(&db.conn);
    let to_trash = repo.get_photos_to_trash(args.group_id)?;
    let trashed =
        photovault::services::trash::TrashService::trash_photos(&db.conn, &to_trash)? as u64;
    repo.delete_group(args.group_id)?;
    Ok(CountResultDto { count: trashed })
}

#[tauri::command]
pub async fn duplicates_dismiss(
    state: State<'_, AppState>,
    args: DuplicatesGroupActionArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    DuplicateRepo::new(&db.conn).delete_group(args.group_id)?;
    Ok(())
}

// ---------- job ----------

#[derive(Debug, Default, Deserialize)]
pub struct DuplicatesRunArgs {
    /// When true, also run the perceptual (DCT) pass after the exact pass.
    /// Defaults to true; perceptual is what catches near-duplicates after
    /// edits/recompression.
    #[serde(default = "yes")]
    pub include_perceptual: bool,
}
fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, Clone)]
pub struct DuplicatesCompleteDto {
    pub job_id: String,
    pub groups_found: u64,
    pub elapsed_ms: u64,
}

#[tauri::command]
pub async fn duplicates_run(
    app: AppHandle,
    state: State<'_, AppState>,
    args: DuplicatesRunArgs,
) -> CommandResult<JobIdDto> {
    let job = jobs::start_job(&state, JobKind::Duplicates).await?;
    let job_id = job.id.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    // Open a fresh sqlite connection inside the spawn_blocking thread
    // (see bursts.rs for the same pattern). Holding the shared
    // Arc<Mutex<Database>> for the entire detection blocks every
    // foreground read for ~seconds, which is what makes the timeline
    // appear empty until detection finishes.
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = photovault::db::db_path_for(&drive_root);

    emit(
        &app_clone,
        EV_DUPLICATES_PROGRESS,
        JobProgress {
            job_id: job_id.clone(),
            stage: "exact".into(),
            processed: 0,
            total: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
            eta_ms: None,
            message: Some("scanning for byte-identical duplicates".into()),
        },
    );

    tokio::task::spawn_blocking(move || {
        let conn = match photovault::db::open_secondary(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("duplicates: open secondary DB failed: {}", e);
                return;
            }
        };
        let exact = DuplicateDetector::find_duplicates(&conn).unwrap_or_default();
        let mut groups_found = exact.len();

        if args.include_perceptual {
            let exclude_ids: std::collections::HashSet<i64> = exact
                .iter()
                .flat_map(|g| g.photo_ids.iter().copied())
                .collect();
            // Pass the drive root; `find_perceptual_duplicates` resolves
            // each photo's small-thumbnail path itself, matching the
            // ThumbnailService v2 layout.
            if let Ok(perc) =
                DuplicateDetector::find_perceptual_duplicates(&conn, &drive_root, &exclude_ids)
            {
                groups_found += perc.len();
                // Persist both passes' groups so the UI can read them.
                let mut to_persist: Vec<(String, Vec<i64>, Option<i64>, &'static str)> =
                    Vec::with_capacity(exact.len() + perc.len());
                for g in exact.iter().chain(perc.iter()) {
                    to_persist.push((
                        g.hash.clone(),
                        g.photo_ids.clone(),
                        g.suggested_keep_id,
                        g.duplicate_type,
                    ));
                }
                let repo = photovault::db::duplicate_repo::DuplicateRepo::new(&conn);
                if let Err(e) = repo.sync_duplicate_groups(&to_persist) {
                    tracing::warn!("dup persist: {}", e);
                }
            }
        } else {
            // Even when perceptual is off, persist the exact pass.
            let to_persist: Vec<(String, Vec<i64>, Option<i64>, &'static str)> = exact
                .iter()
                .map(|g| {
                    (
                        g.hash.clone(),
                        g.photo_ids.clone(),
                        g.suggested_keep_id,
                        g.duplicate_type,
                    )
                })
                .collect();
            let repo = photovault::db::duplicate_repo::DuplicateRepo::new(&conn);
            if let Err(e) = repo.sync_duplicate_groups(&to_persist) {
                tracing::warn!("dup persist: {}", e);
            }
        }

        emit(
            &app_clone,
            EV_DUPLICATES_COMPLETE,
            DuplicatesCompleteDto {
                job_id: job_id_clone.clone(),
                groups_found: groups_found as u64,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        );
        // finish_job is async; bridge via a runtime handle.
        let rt = tokio::runtime::Handle::current();
        let app_for_finish = app_clone.clone();
        rt.spawn(async move {
            let st: tauri::State<AppState> = app_for_finish.state();
            jobs::finish_job(&st, &job_id_clone).await;
        });
    });

    Ok(JobIdDto { job_id })
}
