//! Burst groups (read-only listing).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use smriti::db::burst_repo::BurstRepo;

use crate::dto::{BurstGroupDto, BurstGroupSummaryDto, BurstMemberDto, JobIdDto};
use crate::events::{JobProgress, EV_BURSTS_COMPLETE, EV_BURSTS_PROGRESS};
use crate::jobs::{self, emit};
use crate::state::{AppState, JobKind};
use crate::{CommandError, CommandResult};

#[tauri::command]
pub async fn bursts_list(state: State<'_, AppState>) -> CommandResult<Vec<BurstGroupSummaryDto>> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let db = lib.db.lock().await;
    let repo = BurstRepo::new(&db.conn);
    Ok(repo.get_all_groups()?.into_iter().map(Into::into).collect())
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
    repo.set_suggested_best(args.group_id, args.photo_id)?;
    let members = repo.get_group_members(args.group_id)?;
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
    let to_trash = repo.get_photos_to_trash(args.group_id)?;
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
    BurstRepo::new(&db.conn).delete_group(args.group_id)?;
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
    let job = jobs::start_job(&state, JobKind::Bursts).await?;
    let job_id = job.id.clone();
    let started = job.started_at;
    let app_clone = app.clone();
    let job_id_clone = job_id.clone();

    let drive_root = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        lib.drive_root.clone()
    };
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

    let db_path = smriti::db::db_path_for(&drive_root);
    tokio::task::spawn_blocking(move || {
        let conn = match smriti::db::open_secondary(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("bursts: open secondary DB failed: {}", e);
                return;
            }
        };
        let detector = smriti::services::burst_detector::BurstDetector::new(burst_cfg);
        let groups = detector
            .find_bursts(&conn, Some(&drive_root), Some(&thumbs_root))
            .unwrap_or_default();
        let count = groups.len();
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
        let _ = repo.sync_burst_groups(&triples);
        let _ = smriti::services::PhotoStackService::refresh(&conn, &drive_root);

        emit(
            &app_clone,
            EV_BURSTS_COMPLETE,
            BurstsCompleteDto {
                job_id: job_id_clone.clone(),
                groups_found: count as u64,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        );
        let rt = tokio::runtime::Handle::current();
        let app_for_finish = app_clone.clone();
        rt.spawn(async move {
            let st: tauri::State<AppState> = app_for_finish.state();
            jobs::finish_job(&st, &job_id_clone).await;
        });
    });

    Ok(JobIdDto { job_id })
}
