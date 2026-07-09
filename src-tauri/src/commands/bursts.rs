//! Burst groups (read-only listing).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::burst_repo::BurstRepo;

use crate::dto::{BurstGroupDto, BurstGroupSummaryDto, BurstMemberDto, JobIdDto};
use crate::events::{JobProgress, EV_BURSTS_COMPLETE, EV_BURSTS_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

fn burst_group_exists(repo: &BurstRepo<'_>, group_id: i64) -> CommandResult<bool> {
    Ok(!repo.get_group_members(group_id)?.is_empty())
}

#[derive(Debug, Default, Deserialize)]
pub struct BurstsListArgs {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[tauri::command]
pub async fn bursts_list(
    state: State<'_, AppState>,
    args: BurstsListArgs,
) -> CommandResult<Vec<BurstGroupSummaryDto>> {
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
            BurstRepo::new(&conn)
                .get_groups(limit, offset)?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("burst list worker failed: {e}"),
    })?
}

#[derive(Debug, Deserialize)]
pub struct BurstsGetGroupArgs {
    pub id: i64,
}

#[tauri::command]
pub async fn bursts_get_group(
    state: State<'_, AppState>,
    args: BurstsGetGroupArgs,
) -> CommandResult<BurstGroupDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    // Self-heal: groups created before auto-suggest landed have no best
    // member, so the UI shows zero "Pick this" candidates and the user
    // can't trash non-best. Default-suggest the first member by date,
    // idempotently. Same applies if the previously-best photo was
    // trashed and removed from the group.
    repo.ensure_suggested_best(args.id)?;
    let members = repo.get_group_members(args.id)?;
    if members.is_empty() {
        return Err(CommandError::not_found("burst_group", args.id));
    }
    Ok(BurstGroupDto {
        id: args.id,
        members: members.into_iter().map(BurstMemberDto::from).collect(),
    })
}

// ---------- mutations ----------

#[derive(Debug, Deserialize)]
pub struct BurstsSetBestArgs {
    pub group_id: i64,
    pub photo_id: i64,
}

#[tauri::command]
pub async fn bursts_set_best(
    state: State<'_, AppState>,
    args: BurstsSetBestArgs,
) -> CommandResult<BurstGroupDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    match repo.set_suggested_best(args.group_id, args.photo_id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("burst_group_member", args.photo_id));
        }
        Err(e) => return Err(e.into()),
    }
    let members = repo.get_group_members(args.group_id)?;
    if members.is_empty() {
        return Err(CommandError::not_found("burst_group", args.group_id));
    }
    Ok(BurstGroupDto {
        id: args.group_id,
        members: members.into_iter().map(BurstMemberDto::from).collect(),
    })
}

#[derive(Debug, Deserialize)]
pub struct BurstsGroupActionArgs {
    pub group_id: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct BurstsCountResultDto {
    pub count: u64,
}

#[tauri::command]
pub async fn bursts_trash_non_best(
    state: State<'_, AppState>,
    args: BurstsGroupActionArgs,
) -> CommandResult<BurstsCountResultDto> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    repo.ensure_suggested_best(args.group_id)?;
    if !burst_group_exists(&repo, args.group_id)? {
        return Err(CommandError::not_found("burst_group", args.group_id));
    }
    let to_trash = repo.get_photos_to_trash(args.group_id)?;
    if to_trash.is_empty() {
        return Err(CommandError::Conflict {
            reason: "burst group has no non-best photos to trash".into(),
        });
    }
    let trashed = smriti::services::trash::TrashService::trash_photos(&db.conn, &to_trash)? as u64;
    repo.delete_group(args.group_id)?;
    Ok(BurstsCountResultDto { count: trashed })
}

#[tauri::command]
pub async fn bursts_dismiss(
    state: State<'_, AppState>,
    args: BurstsGroupActionArgs,
) -> CommandResult<()> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    if !burst_group_exists(&repo, args.group_id)? {
        return Err(CommandError::not_found("burst_group", args.group_id));
    }
    match repo.dismiss_group(args.group_id) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CommandError::not_found("burst_group", args.group_id));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

// ---------- job ----------

#[derive(Debug, Serialize, Clone)]
pub struct BurstsCompleteDto {
    pub job_id: String,
    pub groups_found: u64,
    pub elapsed_ms: u64,
}

#[tauri::command]
pub async fn bursts_run(app: AppHandle, state: State<'_, AppState>) -> CommandResult<JobIdDto> {
    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
    let db_path = smriti::db::db_path_for(&drive_root);

    let job = jobs::start_job(&state, JobKind::Bursts).await?;
    let job_id = job.id.clone();
    let cancel = job.cancel.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    // Burst detection opens its OWN sqlite connection to the same DB.
    // SQLite WAL mode allows the foreground photos_list query to keep
    // running while we read here — without this, holding the shared
    // Arc<Mutex<Database>> blocks every other handler for the entire
    // detection duration, making the timeline appear empty.

    emit(
        &app_clone,
        EV_BURSTS_PROGRESS,
        JobProgress {
            job_id: job_id.clone(),
            stage: "scan".into(),
            processed: 0,
            total: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
            eta_ms: None,
            message: Some("detecting burst groups".into()),
        },
    );

    let cfg = smriti::config::AppConfig::load();
    let burst_cfg = smriti::services::burst_detector::BurstConfig {
        max_gap_seconds: cfg.burst_time_window_seconds,
        ..Default::default()
    };
    // ThumbnailService v2 layout: <drive>/.photovault/thumbnails/small/v2/<2hash>/<hash>.jpg
    let thumbs_root = drive_root.join(".photovault/thumbnails/small/v2");

    tokio::task::spawn_blocking(move || {
        use std::sync::atomic::Ordering;
        let conn = match smriti::db::open_secondary(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("bursts: open secondary DB failed: {}", e);
                emit(
                    &app_clone,
                    EV_BURSTS_PROGRESS,
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
        let detector = smriti::services::burst_detector::BurstDetector::new(burst_cfg);
        let mut streamed: Vec<(String, String, Vec<i64>)> = Vec::new();
        let mut inserted_live_sets: Vec<Vec<i64>> = Vec::new();
        let mut flush_streamed = |batch: &mut Vec<(String, String, Vec<i64>)>| {
            if batch.is_empty() {
                return;
            }
            let repo = BurstRepo::new(&conn);
            match repo.upsert_burst_groups_collecting_inserted(batch) {
                Ok(inserted) if !inserted.is_empty() => {
                    let count = inserted.len();
                    inserted_live_sets.extend(inserted);
                    emit(
                        &app_clone,
                        EV_BURSTS_PROGRESS,
                        JobProgress {
                            job_id: job_id_clone.clone(),
                            stage: "persisted".into(),
                            processed: count as u64,
                            total: None,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                            eta_ms: None,
                            message: Some(format!("{} burst groups available", count)),
                        },
                    );
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("burst live persist: {}", e),
            }
            batch.clear();
        };
        let groups = match detector.find_bursts_streaming(
            &conn,
            Some(&drive_root),
            Some(&thumbs_root),
            Some(cancel.as_ref()),
            |p| {
                emit(
                    &app_clone,
                    EV_BURSTS_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "scan".into(),
                        processed: p.processed,
                        total: Some(p.total),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(p.message),
                    },
                );
            },
            |g| {
                streamed.push((
                    g.start_time.to_rfc3339(),
                    g.end_time.to_rfc3339(),
                    g.photo_ids.clone(),
                ));
                if streamed.len() >= 10 {
                    flush_streamed(&mut streamed);
                }
            },
        ) {
            Ok(groups) => groups,
            Err(e) => {
                tracing::error!("bursts: scan failed: {}", e);
                emit(
                    &app_clone,
                    EV_BURSTS_PROGRESS,
                    JobProgress {
                        job_id: job_id_clone.clone(),
                        stage: "error".into(),
                        processed: 0,
                        total: None,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        eta_ms: None,
                        message: Some(format!("Burst detection failed: {e}")),
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
        flush_streamed(&mut streamed);
        let count = groups.len();
        let mut job_error: Option<String> = None;

        if cancel.load(Ordering::Relaxed) {
            if !inserted_live_sets.is_empty() {
                let repo = BurstRepo::new(&conn);
                if let Err(e) = repo.delete_unresolved_groups_by_member_sets(&inserted_live_sets) {
                    tracing::warn!("burst cancel cleanup failed: {}", e);
                }
            }
        } else {
            emit(
                &app_clone,
                EV_BURSTS_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "persist".into(),
                    processed: 0,
                    total: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some("saving burst groups".into()),
                },
            );
            let repo = BurstRepo::new(&conn);
            let triples: Vec<(String, String, Vec<i64>)> = groups
                .into_iter()
                .map(|g| {
                    (
                        g.start_time.to_rfc3339(),
                        g.end_time.to_rfc3339(),
                        g.photo_ids,
                    )
                })
                .collect();
            if let Err(e) = repo.sync_burst_groups(&triples) {
                let msg = format!("saving burst groups failed: {e}");
                tracing::error!("{msg}");
                job_error = Some(msg);
            }
        }
        if job_error.is_none() && !cancel.load(Ordering::Relaxed) {
            emit(
                &app_clone,
                EV_BURSTS_PROGRESS,
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
                tracing::warn!("burst stack refresh failed: {}", e);
            }
        }

        if let Some(message) = job_error {
            emit(
                &app_clone,
                EV_BURSTS_PROGRESS,
                JobProgress {
                    job_id: job_id_clone.clone(),
                    stage: "error".into(),
                    processed: count as u64,
                    total: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    eta_ms: None,
                    message: Some(message),
                },
            );
        } else {
            emit(
                &app_clone,
                EV_BURSTS_COMPLETE,
                BurstsCompleteDto {
                    job_id: job_id_clone.clone(),
                    groups_found: count as u64,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            );
        }
        let rt = tokio::runtime::Handle::current();
        let app_for_finish = app_clone.clone();
        rt.spawn(async move {
            let st: tauri::State<AppState> = app_for_finish.state();
            jobs::finish_job(&st, &job_id_clone).await;
        });
    });

    Ok(JobIdDto { job_id })
}
