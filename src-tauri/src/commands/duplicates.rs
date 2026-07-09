//! Duplicate groups (read-only listing).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::duplicate_repo::DuplicateRepo;
use smriti::services::duplicate_detector::DuplicateDetector;

use crate::dto::{DuplicateGroupDto, DuplicateGroupSummaryDto, DuplicateMemberDto, JobIdDto};
use crate::events::{JobProgress, EV_DUPLICATES_COMPLETE, EV_DUPLICATES_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

fn duplicate_group_exists(repo: &DuplicateRepo<'_>, group_id: i64) -> CommandResult<bool> {
    Ok(!repo.get_group_members(group_id)?.is_empty())
}

#[derive(Debug, Default, Deserialize)]
pub struct DuplicatesListArgs {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[tauri::command]
pub async fn duplicates_list(
    state: State<'_, AppState>,
    args: DuplicatesListArgs,
) -> CommandResult<Vec<DuplicateGroupSummaryDto>> {
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        smriti::db::db_path_for(&lib.drive_root)
    };
    let limit = args.limit.unwrap_or(200).clamp(1, 500) as i64;
    let offset = args.offset.unwrap_or(0) as i64;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(
            DuplicateRepo::new(&conn)
                .get_groups(limit, offset)?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("duplicate list worker failed: {e}"),
    })?
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
    let db_path = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        smriti::db::db_path_for(&lib.drive_root)
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let conn = smriti::db::open_secondary(&db_path)?;
        Ok::<_, CommandError>(DuplicateDetector::calculate_wasted_space(&conn)?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("duplicate wasted-space worker failed: {e}"),
    })??;
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
    match repo.set_keep_photo(args.group_id, args.photo_id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found(
                "duplicate_group_member",
                args.photo_id,
            ));
        }
        Err(e) => return Err(e.into()),
    }
    let members = repo.get_group_members(args.group_id)?;
    if members.is_empty() {
        return Err(CommandError::not_found("duplicate_group", args.group_id));
    }
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
    if !duplicate_group_exists(&repo, args.group_id)? {
        return Err(CommandError::not_found("duplicate_group", args.group_id));
    }
    let to_trash = repo.get_photos_to_trash(args.group_id)?;
    if to_trash.is_empty() {
        return Err(CommandError::Conflict {
            reason: "duplicate group has no non-keep photos to trash".into(),
        });
    }
    let trashed = smriti::services::trash::TrashService::trash_photos(&db.conn, &to_trash)? as u64;
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
    let repo = DuplicateRepo::new(&db.conn);
    if !duplicate_group_exists(&repo, args.group_id)? {
        return Err(CommandError::not_found("duplicate_group", args.group_id));
    }
    match repo.dismiss_group(args.group_id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("duplicate_group", args.group_id));
        }
        Err(e) => return Err(e.into()),
    }
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
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);

    let job = jobs::start_job(&state, JobKind::Duplicates).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    // Open a fresh sqlite connection inside the spawn_blocking thread
    // (see bursts.rs for the same pattern). Holding the shared
    // Arc<Mutex<Database>> for the entire detection blocks every
    // foreground read for ~seconds, which is what makes the timeline
    // appear empty until detection finishes.
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
        use std::sync::atomic::Ordering;
        let conn = match smriti::db::open_secondary(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("duplicates: open secondary DB failed: {}", e);
                emit(
                    &app_clone,
                    EV_DUPLICATES_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "error".into(),
                        processed: 0,
                        total: None,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("Opening the library database failed: {e}")),
                    },
                );
                let rt = tokio::runtime::Handle::current();
                let app_for_finish = app_clone.clone();
                let finish_job_id = job_id_clone.clone();
                rt.spawn(async move {
                    let st: tauri::State<AppState> = app_for_finish.state();
                    jobs::finish_job(&st, &finish_job_id).await;
                });
                return;
            }
        };
        if cancel.load(Ordering::Relaxed) {
            emit(
                &app_clone,
                EV_DUPLICATES_COMPLETE,
                DuplicatesCompleteDto {
                    job_id: job_id_clone.clone(),
                    groups_found: 0,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            );
            let rt = tokio::runtime::Handle::current();
            let app_for_finish = app_clone.clone();
            let finish_job_id = job_id_clone.clone();
            rt.spawn(async move {
                let st: tauri::State<AppState> = app_for_finish.state();
                jobs::finish_job(&st, &finish_job_id).await;
            });
            return;
        }
        let exact = match DuplicateDetector::find_duplicates(&conn, &drive_root) {
            Ok(groups) => groups,
            Err(e) => {
                tracing::error!("duplicates: exact pass failed: {}", e);
                emit(
                    &app_clone,
                    EV_DUPLICATES_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "error".into(),
                        processed: 0,
                        total: None,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("Duplicate detection failed: {e}")),
                    },
                );
                let rt = tokio::runtime::Handle::current();
                let app_for_finish = app_clone.clone();
                let finish_job_id = job_id_clone.clone();
                rt.spawn(async move {
                    let st: tauri::State<AppState> = app_for_finish.state();
                    jobs::finish_job(&st, &finish_job_id).await;
                });
                return;
            }
        };
        let mut groups_found = exact.len();
        let mut to_persist: Vec<(String, Vec<i64>, Option<i64>, &'static str)> =
            Vec::with_capacity(exact.len());
        for g in &exact {
            to_persist.push((
                g.hash.clone(),
                g.photo_ids.clone(),
                g.suggested_keep_id,
                g.duplicate_type,
            ));
        }
        emit(
            &app_clone,
            EV_DUPLICATES_PROGRESS,
            JobProgress {
                job_id: job_id_clone.clone(),
                stage: "exact".into(),
                processed: exact.len() as u64,
                total: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
                eta_ms: None,
                message: Some(format!("{} exact duplicate groups", exact.len())),
            },
        );
        if !to_persist.is_empty() {
            let repo = smriti::db::duplicate_repo::DuplicateRepo::new(&conn);
            if let Err(e) = repo.upsert_duplicate_groups(&to_persist) {
                tracing::warn!("dup exact live persist: {}", e);
            } else {
                emit(
                    &app_clone,
                    EV_DUPLICATES_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "persisted".into(),
                        processed: to_persist.len() as u64,
                        total: None,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("{} duplicate groups available", to_persist.len())),
                    },
                );
            }
        }

        let mut job_error: Option<String> = None;

        if args.include_perceptual && !cancel.load(Ordering::Relaxed) {
            let exclude_ids: std::collections::HashSet<i64> = exact
                .iter()
                .flat_map(|g| g.photo_ids.iter().copied())
                .collect();
            match DuplicateDetector::find_perceptual_duplicates_with_progress(
                &conn,
                &drive_root,
                &exclude_ids,
                Some(cancel.as_ref()),
                |p| {
                    emit(
                        &app_clone,
                        EV_DUPLICATES_PROGRESS,
                        JobProgress {
                            job_id: job_id_clone.clone(),
                            stage: p.stage.into(),
                            processed: p.processed,
                            total: p.total,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                            eta_ms: None,
                            message: Some(p.message),
                        },
                    );
                },
            ) {
                Ok(perc) => {
                    groups_found += perc.len();
                    to_persist.reserve(perc.len());
                    for g in &perc {
                        to_persist.push((
                            g.hash.clone(),
                            g.photo_ids.clone(),
                            g.suggested_keep_id,
                            g.duplicate_type,
                        ));
                    }
                    if !perc.is_empty() && !cancel.load(Ordering::Relaxed) {
                        let batch: Vec<(String, Vec<i64>, Option<i64>, &'static str)> = perc
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
                        let repo = smriti::db::duplicate_repo::DuplicateRepo::new(&conn);
                        if let Err(e) = repo.upsert_duplicate_groups(&batch) {
                            tracing::warn!("dup perceptual live persist: {}", e);
                        } else {
                            emit(
                                &app_clone,
                                EV_DUPLICATES_PROGRESS,
                                JobProgress {
                                    job_id: job_id_clone.clone(),
                                    stage: "persisted".into(),
                                    processed: batch.len() as u64,
                                    total: None,
                                    elapsed_ms: started.elapsed().as_millis() as u64,
                                    eta_ms: None,
                                    message: Some(format!(
                                        "{} visual duplicate groups available",
                                        batch.len()
                                    )),
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("visual duplicate detection failed: {e}");
                    tracing::error!("{msg}");
                    job_error = Some(msg);
                }
            }
        }

        if job_error.is_none() && !cancel.load(Ordering::Relaxed) {
            emit(
                &app_clone,
                EV_DUPLICATES_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "persist".into(),
                    processed: 0,
                    total: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some("saving duplicate groups".into()),
                },
            );
            let repo = smriti::db::duplicate_repo::DuplicateRepo::new(&conn);
            if let Err(e) = repo.sync_duplicate_groups(&to_persist) {
                let msg = format!("saving duplicate groups failed: {e}");
                tracing::error!("{msg}");
                job_error = Some(msg);
            }
        }
        if job_error.is_none() && !cancel.load(Ordering::Relaxed) {
            emit(
                &app_clone,
                EV_DUPLICATES_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "stacks".into(),
                    processed: 0,
                    total: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some("refreshing stacks".into()),
                },
            );
            if let Err(e) = smriti::services::PhotoStackService::refresh(&conn, &drive_root) {
                tracing::warn!("duplicate stack refresh failed: {}", e);
            }
        }

        if let Some(message) = job_error {
            emit(
                &app_clone,
                EV_DUPLICATES_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "error".into(),
                    processed: groups_found as u64,
                    total: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(message),
                },
            );
        } else {
            emit(
                &app_clone,
                EV_DUPLICATES_COMPLETE,
                DuplicatesCompleteDto {
                    job_id: job_id_clone.clone(),
                    groups_found: groups_found as u64,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            );
        }
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
