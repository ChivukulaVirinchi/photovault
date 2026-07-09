//! Provider-led Assistant IPC commands.
//!
//! The model orchestrates typed tools. The app validates and executes those
//! tools, and only creates albums after an explicit approval token.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};

use smriti::db::album_repo::AlbumRepo;
use smriti::db::connection::{db_path_for, open_secondary};
use smriti::services::assistant::{
    AssistantActivity, AssistantIntent, AssistantRun, AssistantRunStatus, AssistantSearchArgs,
    AssistantService,
};
use smriti::services::semantic::{
    relevant_text_search_candidates, SemanticIndexCache, SemanticModelRunner,
    SemanticSearchService, SEMANTIC_TEXT_SEARCH_LIMIT,
};

use crate::events::{AssistantActivityEvent, EV_ASSISTANT_ACTIVITY};
use crate::jobs::emit;
use crate::state::{AppState, AssistantMessage, AssistantSession};
use crate::{CommandError, CommandResult};

const MAX_AGENT_STEPS: usize = 8;
const PROVIDER_TIMEOUT_SECS: u64 = 12;
const MAX_RECENT_SESSION_MESSAGES: usize = 10;
const MAX_ASSISTANT_SESSIONS: usize = 20;
const MAX_ASSISTANT_MESSAGE_CHARS: usize = 4_000;

#[derive(Debug, Deserialize)]
pub struct AssistantStartArgs {
    pub message: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantRunArgs {
    pub run_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AssistantContinueArgs {
    pub run_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct AssistantApproveArgs {
    pub run_id: String,
    pub approval_id: String,
}

#[tauri::command]
pub async fn assistant_start(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AssistantStartArgs,
) -> CommandResult<AssistantRun> {
    let message = clean_message(args.message)?;
    let cfg = smriti::config::AppConfig::load();
    ensure_enabled(&cfg)?;

    let run_id = args.run_id.unwrap_or_else(|| new_id("assistant"));
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let library_root = lib.drive_root.display().to_string();
    drop(lib_guard);

    let seed_run = AssistantRun {
        run_id: run_id.clone(),
        library_root: library_root.clone(),
        status: AssistantRunStatus::Running,
        message: message.clone(),
        response: None,
        clarification_options: Vec::new(),
        activity: Vec::new(),
        preview: None,
        album_id: None,
    };
    {
        let mut assistant = state.assistant.lock().await;
        prune_sessions(
            &mut assistant.sessions,
            MAX_ASSISTANT_SESSIONS.saturating_sub(1),
        );
        assistant.sessions.insert(
            run_id.clone(),
            AssistantSession {
                run: seed_run,
                draft: None,
                library_root,
                messages: vec![AssistantMessage {
                    role: "user".into(),
                    content: message,
                }],
                current_result_ids: Vec::new(),
            },
        );
    }

    run_turn(app, &state, &run_id, &cfg).await
}

#[tauri::command]
pub async fn assistant_state(
    state: State<'_, AppState>,
    args: AssistantRunArgs,
) -> CommandResult<AssistantRun> {
    let assistant = state.assistant.lock().await;
    assistant
        .sessions
        .get(&args.run_id)
        .map(|s| s.run.clone())
        .ok_or_else(|| CommandError::not_found("assistant_run", args.run_id))
}

#[tauri::command]
pub async fn assistant_continue(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AssistantContinueArgs,
) -> CommandResult<AssistantRun> {
    let message = clean_message(args.message)?;
    let cfg = smriti::config::AppConfig::load();
    ensure_enabled(&cfg)?;

    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let library_root = lib.drive_root.display().to_string();
    drop(lib_guard);
    ensure_session_library(&state, &args.run_id, &library_root).await?;

    {
        let mut assistant = state.assistant.lock().await;
        let session = assistant
            .sessions
            .get_mut(&args.run_id)
            .ok_or_else(|| CommandError::not_found("assistant_run", &args.run_id))?;
        session.messages.push(AssistantMessage {
            role: "user".into(),
            content: message.clone(),
        });
        session.run.message = message;
        session.run.response = None;
        session.run.clarification_options.clear();
        session.run.preview = None;
        session.run.status = AssistantRunStatus::Running;
        session.run.activity.clear();
        session.draft = None;
    }

    run_turn(app, &state, &args.run_id, &cfg).await
}

#[tauri::command]
pub async fn assistant_stop(
    state: State<'_, AppState>,
    args: AssistantRunArgs,
) -> CommandResult<AssistantRun> {
    let mut assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get_mut(&args.run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", &args.run_id))?;
    session.run.status = AssistantRunStatus::Stopped;
    session.run.activity.push(AssistantActivity {
        label: "Stopped".into(),
    });
    Ok(session.run.clone())
}

#[tauri::command]
pub async fn assistant_reject(
    state: State<'_, AppState>,
    args: AssistantApproveArgs,
) -> CommandResult<AssistantRun> {
    let mut assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get_mut(&args.run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", &args.run_id))?;
    validate_approval(session, &args.approval_id)?;
    session.run.status = AssistantRunStatus::Stopped;
    session.run.activity.push(AssistantActivity {
        label: "Cancelled".into(),
    });
    session.draft = None;
    Ok(session.run.clone())
}

#[tauri::command]
pub async fn assistant_approve(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AssistantApproveArgs,
) -> CommandResult<AssistantRun> {
    let draft = {
        let assistant = state.assistant.lock().await;
        let session = assistant
            .sessions
            .get(&args.run_id)
            .ok_or_else(|| CommandError::not_found("assistant_run", &args.run_id))?;
        validate_approval(session, &args.approval_id)?;
        if session.run.status != AssistantRunStatus::WaitingForApproval {
            return Err(CommandError::Conflict {
                reason: "this Assistant result is not waiting for album approval".into(),
            });
        }
        session
            .draft
            .clone()
            .ok_or_else(|| CommandError::Conflict {
                reason: "no pending album preview".into(),
            })?
    };

    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    let library_root = lib.drive_root.display().to_string();
    ensure_session_library(&state, &args.run_id, &library_root).await?;
    emit_activity(&app, &state, &args.run_id, &library_root, "Creating album").await;
    let db = lib.db.lock().await;
    let album_id = AssistantService::create_album(&db.conn, &draft)?;
    drop(db);
    drop(lib_guard);

    let mut assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get_mut(&args.run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", &args.run_id))?;
    session.run.status = AssistantRunStatus::Completed;
    session.run.album_id = Some(album_id);
    session.run.response = Some(format!("Created album \"{}\".", draft.album_name));
    session.run.clarification_options.clear();
    session.draft = None;
    push_context_message(session);
    session.messages.push(AssistantMessage {
        role: "assistant".into(),
        content: session.run.response.clone().unwrap_or_default(),
    });
    Ok(session.run.clone())
}

#[tauri::command]
pub async fn assistant_clear(state: State<'_, AppState>) -> CommandResult<()> {
    state.assistant.lock().await.sessions.clear();
    Ok(())
}

async fn run_turn(
    app: AppHandle,
    state: &AppState,
    run_id: &str,
    cfg: &smriti::config::AppConfig,
) -> CommandResult<AssistantRun> {
    if cfg.assistant_provider == "local" || cfg.assistant_api_key.is_none() {
        return Err(CommandError::Validation {
            field: "assistant_provider".into(),
            reason: "Provider-backed Assistant requires an OpenAI-compatible provider and API key. Local parser fallback is disabled.".into(),
        });
    }

    let library_root = {
        let assistant = state.assistant.lock().await;
        assistant
            .sessions
            .get(run_id)
            .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?
            .library_root
            .clone()
    };

    let album_requested = current_user_album_requested(state, run_id).await?;
    if !album_requested {
        if let Some(run) = run_plain_search_fast_path(&app, state, run_id, &library_root).await? {
            return Ok(run);
        }
    }
    let mut messages = build_provider_messages(state, run_id).await?;
    for _ in 0..MAX_AGENT_STEPS {
        if stopped(state, run_id).await? {
            return assistant_state_by_id(state, run_id).await;
        }
        let response = match call_provider(cfg, &messages, json!("auto")).await {
            Ok(response) => response,
            Err(err) => {
                mark_failed(state, run_id, err.to_string()).await?;
                return assistant_state_by_id(state, run_id).await;
            }
        };
        if stopped(state, run_id).await? {
            return assistant_state_by_id(state, run_id).await;
        }
        let Some(choice) = response.choices.into_iter().next() else {
            let msg = "assistant provider returned no choices".to_string();
            mark_failed(state, run_id, msg).await?;
            return assistant_state_by_id(state, run_id).await;
        };
        let msg = choice.message;
        if let Some(tool_calls) = msg.tool_calls.filter(|calls| !calls.is_empty()) {
            messages.push(ProviderMessage {
                role: "assistant".into(),
                content: msg.content.clone(),
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
                name: None,
            });
            for call in tool_calls {
                if stopped(state, run_id).await? {
                    return assistant_state_by_id(state, run_id).await;
                }
                let result = execute_tool(&app, state, run_id, &library_root, &call).await?;
                messages.push(ProviderMessage {
                    role: "tool".into(),
                    content: Some(result.to_string()),
                    tool_calls: None,
                    tool_call_id: Some(call.id),
                    name: Some(call.function.name),
                });
                if let Some(run) = terminal_run(state, run_id).await? {
                    return Ok(run);
                }
            }
            continue;
        }

        let content = msg
            .content
            .unwrap_or_else(|| "I need a little more detail.".into());
        if album_requested {
            let status = {
                let assistant = state.assistant.lock().await;
                assistant
                    .sessions
                    .get(run_id)
                    .map(|s| s.run.status.clone())
                    .unwrap_or(AssistantRunStatus::Failed)
            };
            if status != AssistantRunStatus::WaitingForApproval {
                messages.push(ProviderMessage {
                    role: "user".into(),
                    content: Some("Tool policy violation: the user explicitly asked for an album. You must call preview_album with a clean album_name, or ask_user if you need clarification. Do not finish with plain search results.".into()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                continue;
            }
        }
        let mut assistant = state.assistant.lock().await;
        let session = assistant
            .sessions
            .get_mut(run_id)
            .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
        session.run.status = if session.current_result_ids.is_empty() {
            AssistantRunStatus::WaitingForClarification
        } else {
            AssistantRunStatus::ResultsReady
        };
        session.run.response = Some(content.clone());
        session.run.clarification_options.clear();
        session.messages.push(AssistantMessage {
            role: "assistant".into(),
            content,
        });
        return Ok(session.run.clone());
    }

    mark_failed(state, run_id, "assistant exceeded tool step limit".into()).await?;
    assistant_state_by_id(state, run_id).await
}

async fn execute_tool(
    app: &AppHandle,
    state: &AppState,
    run_id: &str,
    library_root: &str,
    call: &ToolCall,
) -> CommandResult<Value> {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
    match call.function.name.as_str() {
        "resolve_people" => {
            emit_activity(app, state, run_id, library_root, "Resolving people").await;
            let queries = string_vec_arg(&args, "queries");
            let lib_guard = state.library.read().await;
            let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
            ensure_current_library(&lib.drive_root, library_root)?;
            let db = lib.db.lock().await;
            let resolved = AssistantService::resolve_people_queries(&db.conn, &queries)?;
            Ok(json!(resolved))
        }
        "resolve_places" => {
            emit_activity(app, state, run_id, library_root, "Resolving places").await;
            let queries = string_vec_arg(&args, "queries");
            let lib_guard = state.library.read().await;
            let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
            ensure_current_library(&lib.drive_root, library_root)?;
            let db = lib.db.lock().await;
            let resolved = AssistantService::resolve_place_queries(&db.conn, &queries)?;
            Ok(json!(resolved))
        }
        "resolve_date_range" => {
            emit_activity(app, state, run_id, library_root, "Resolving date").await;
            let phrase = args
                .get("phrase")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(json!({ "date": AssistantService::resolve_date_phrase(phrase) }))
        }
        "search_photos" => {
            emit_activity(app, state, run_id, library_root, "Searching photos").await;
            let mut search_args: AssistantSearchArgs =
                serde_json::from_value(args).map_err(|e| CommandError::Validation {
                    field: "search_photos".into(),
                    reason: e.to_string(),
                })?;
            if let Some(error) = validate_search_tool_call(&search_args) {
                return Ok(json!({ "error": error, "retryable": true }));
            }
            if search_args.include_photo_ids.is_empty()
                && search_args.combine_mode == "union_with_current"
            {
                search_args.combine_mode = "union".into();
                search_args.include_photo_ids = {
                    let assistant = state.assistant.lock().await;
                    assistant
                        .sessions
                        .get(run_id)
                        .map(|s| s.current_result_ids.clone())
                        .unwrap_or_default()
                };
            }
            let (drive_root, db_path, semantic_index, semantic_runner) = {
                let lib_guard = state.library.read().await;
                let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
                ensure_current_library(&lib.drive_root, library_root)?;
                (
                    lib.drive_root.clone(),
                    db_path_for(&lib.drive_root),
                    lib.semantic_index.clone(),
                    lib.semantic_runner.clone(),
                )
            };
            let draft = tauri::async_runtime::spawn_blocking(move || {
                let conn = open_secondary(&db_path)?;
                let semantic_ids = if let Some(text) = search_args.semantic_text.as_deref() {
                    semantic_photo_ids(&drive_root, &semantic_index, &semantic_runner, &conn, text)?
                } else {
                    Vec::new()
                };
                Ok::<_, CommandError>(AssistantService::search_with_args(
                    &conn,
                    &search_args,
                    &semantic_ids,
                )?)
            })
            .await
            .map_err(|e| CommandError::Internal {
                message: format!("assistant search worker failed: {e}"),
            })??;

            if stopped(state, run_id).await? {
                return Ok(json!({ "stopped": true }));
            }
            let mut assistant = state.assistant.lock().await;
            let session = assistant
                .sessions
                .get_mut(run_id)
                .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
            session.current_result_ids = draft.photo_ids.clone();
            session.run.status = AssistantRunStatus::ResultsReady;
            session.run.preview = Some(draft.preview.clone());
            session.run.clarification_options.clear();
            session.draft = Some(draft.clone());
            push_context_message(session);
            Ok(json!({
                "count": draft.photo_ids.len(),
                "photo_ids": draft.photo_ids.iter().take(200).copied().collect::<Vec<_>>(),
                "sample": draft.preview.sample
            }))
        }
        "search_albums" => {
            emit_activity(app, state, run_id, library_root, "Finding albums").await;
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            let lib_guard = state.library.read().await;
            let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
            ensure_current_library(&lib.drive_root, library_root)?;
            let db = lib.db.lock().await;
            let repo = AlbumRepo::new(&db.conn);
            let mut albums = repo.get_all()?;
            if !query.is_empty() {
                albums.retain(|album| album.name.to_lowercase().contains(&query));
            }
            albums.truncate(10);
            Ok(json!({
                "albums": albums.into_iter().map(|album| json!({
                    "album_id": album.id,
                    "name": album.name,
                    "photo_count": album.photo_count,
                    "created_by": album.created_by
                })).collect::<Vec<_>>()
            }))
        }
        "add_photos_to_album" => {
            emit_activity(app, state, run_id, library_root, "Adding photos to album").await;
            let album_id = args
                .get("album_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| CommandError::Validation {
                    field: "album_id".into(),
                    reason: "album_id is required".into(),
                })?;
            let mut photo_ids = args
                .get("photo_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                .unwrap_or_default();
            if args
                .get("use_current_result_set")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let assistant = state.assistant.lock().await;
                photo_ids = assistant
                    .sessions
                    .get(run_id)
                    .map(|s| s.current_result_ids.clone())
                    .unwrap_or_default();
            }
            let lib_guard = state.library.read().await;
            let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
            ensure_current_library(&lib.drive_root, library_root)?;
            let db = lib.db.lock().await;
            let (album_name, added) = {
                let repo = AlbumRepo::new(&db.conn);
                let album_name = repo
                    .get_all()?
                    .into_iter()
                    .find(|album| album.id == album_id)
                    .map(|album| album.name)
                    .ok_or_else(|| CommandError::not_found("album", album_id.to_string()))?;
                let added = repo.add_photos(album_id, &photo_ids)?;
                (album_name, added)
            };
            drop(db);
            drop(lib_guard);
            if stopped(state, run_id).await? {
                return Ok(json!({ "stopped": true }));
            }
            let mut assistant = state.assistant.lock().await;
            let session = assistant
                .sessions
                .get_mut(run_id)
                .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
            session.run.status = AssistantRunStatus::Completed;
            session.run.album_id = Some(album_id);
            session.run.response = Some(format!("Added {added} photos to \"{album_name}\"."));
            session.run.clarification_options.clear();
            push_context_message(session);
            Ok(json!({
                "album_id": album_id,
                "album_name": album_name,
                "added": added
            }))
        }
        "preview_album" => {
            emit_activity(app, state, run_id, library_root, "Preparing album preview").await;
            let album_name = args
                .get("album_name")
                .and_then(Value::as_str)
                .unwrap_or("Assistant album");
            let mut photo_ids = args
                .get("photo_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                .unwrap_or_default();
            if args
                .get("use_current_result_set")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let assistant = state.assistant.lock().await;
                photo_ids = assistant
                    .sessions
                    .get(run_id)
                    .map(|s| s.current_result_ids.clone())
                    .unwrap_or_default();
            }
            let lib_guard = state.library.read().await;
            let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
            ensure_current_library(&lib.drive_root, library_root)?;
            let db = lib.db.lock().await;
            let draft = AssistantService::preview_from_photo_ids(
                &db.conn,
                &photo_ids,
                album_name,
                new_id("approval"),
                AssistantIntent::CreateAlbum,
            )?;
            drop(db);
            drop(lib_guard);
            if stopped(state, run_id).await? {
                return Ok(json!({ "stopped": true }));
            }
            let mut assistant = state.assistant.lock().await;
            let session = assistant
                .sessions
                .get_mut(run_id)
                .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
            session.current_result_ids = draft.photo_ids.clone();
            session.run.status = AssistantRunStatus::WaitingForApproval;
            session.run.preview = Some(draft.preview.clone());
            session.run.response = Some(format!(
                "I found {} photos for \"{}\". Create this album?",
                draft.photo_ids.len(),
                draft.album_name
            ));
            session.run.clarification_options.clear();
            session.draft = Some(draft.clone());
            push_context_message(session);
            Ok(json!({
                "approval_required": true,
                "album_name": draft.album_name,
                "count": draft.photo_ids.len()
            }))
        }
        "ask_user" => {
            let question = args
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("Can you clarify what you mean?")
                .to_string();
            let options = string_vec_arg(&args, "options")
                .into_iter()
                .filter(|option| !option.trim().is_empty())
                .take(6)
                .collect::<Vec<_>>();
            let mut assistant = state.assistant.lock().await;
            let session = assistant
                .sessions
                .get_mut(run_id)
                .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
            session.run.status = AssistantRunStatus::WaitingForClarification;
            session.run.response = Some(question.clone());
            session.run.clarification_options = options.clone();
            session.messages.push(AssistantMessage {
                role: "assistant".into(),
                content: if options.is_empty() {
                    question.clone()
                } else {
                    format!("{}\nOptions: {}", question, options.join(" | "))
                },
            });
            Ok(json!({ "question": question, "options": options }))
        }
        other => Ok(json!({ "error": format!("unknown tool {other}") })),
    }
}

async fn run_plain_search_fast_path(
    app: &AppHandle,
    state: &AppState,
    run_id: &str,
    library_root: &str,
) -> CommandResult<Option<AssistantRun>> {
    let Some(text) = plain_search_text(state, run_id).await? else {
        return Ok(None);
    };
    let args = AssistantSearchArgs {
        semantic_text: Some(text),
        ..Default::default()
    };
    if validate_search_tool_call(&args).is_some() {
        return Ok(None);
    }
    emit_activity(app, state, run_id, library_root, "Searching photos").await;
    let (drive_root, db_path, semantic_index, semantic_runner) = {
        let lib_guard = state.library.read().await;
        let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
        ensure_current_library(&lib.drive_root, library_root)?;
        (
            lib.drive_root.clone(),
            db_path_for(&lib.drive_root),
            lib.semantic_index.clone(),
            lib.semantic_runner.clone(),
        )
    };
    let draft = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_secondary(&db_path)?;
        let semantic_ids = args
            .semantic_text
            .as_deref()
            .map(|text| {
                semantic_photo_ids(&drive_root, &semantic_index, &semantic_runner, &conn, text)
            })
            .transpose()?
            .unwrap_or_default();
        Ok::<_, CommandError>(AssistantService::search_with_args(
            &conn,
            &args,
            &semantic_ids,
        )?)
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: format!("assistant search worker failed: {e}"),
    })??;

    let mut assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get_mut(run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
    session.current_result_ids = draft.photo_ids.clone();
    session.run.status = AssistantRunStatus::ResultsReady;
    session.run.preview = Some(draft.preview.clone());
    session.run.response = Some(format!("Found {} matching photos.", draft.photo_ids.len()));
    session.run.clarification_options.clear();
    session.draft = Some(draft);
    let response = session.run.response.clone().unwrap_or_default();
    push_context_message(session);
    session.messages.push(AssistantMessage {
        role: "assistant".into(),
        content: response,
    });
    Ok(Some(session.run.clone()))
}

async fn terminal_run(state: &AppState, run_id: &str) -> CommandResult<Option<AssistantRun>> {
    let assistant = state.assistant.lock().await;
    let Some(session) = assistant.sessions.get(run_id) else {
        return Err(CommandError::not_found("assistant_run", run_id));
    };
    Ok(match session.run.status {
        AssistantRunStatus::WaitingForApproval
        | AssistantRunStatus::WaitingForClarification
        | AssistantRunStatus::Completed => Some(session.run.clone()),
        _ => None,
    })
}

async fn assistant_state_by_id(state: &AppState, run_id: &str) -> CommandResult<AssistantRun> {
    let assistant = state.assistant.lock().await;
    assistant
        .sessions
        .get(run_id)
        .map(|s| s.run.clone())
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))
}

async fn stopped(state: &AppState, run_id: &str) -> CommandResult<bool> {
    let assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get(run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
    Ok(session.run.status == AssistantRunStatus::Stopped)
}

async fn mark_failed(state: &AppState, run_id: &str, message: String) -> CommandResult<()> {
    let mut assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get_mut(run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
    if session.run.status != AssistantRunStatus::Stopped {
        session.run.status = AssistantRunStatus::Failed;
        session.run.response = Some(message.clone());
        session.run.activity.push(AssistantActivity {
            label: "Failed".into(),
        });
        session.messages.push(AssistantMessage {
            role: "assistant".into(),
            content: message,
        });
    }
    Ok(())
}

async fn build_provider_messages(
    state: &AppState,
    run_id: &str,
) -> CommandResult<Vec<ProviderMessage>> {
    let (session_messages, result_count, pending_preview, current_album_id, library_root) = {
        let assistant = state.assistant.lock().await;
        let session = assistant
            .sessions
            .get(run_id)
            .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
        (
            session.messages.clone(),
            session.current_result_ids.len(),
            session
                .run
                .preview
                .as_ref()
                .map(|preview| (preview.album_name.clone(), preview.photo_count)),
            session.run.album_id,
            session.library_root.clone(),
        )
    };
    let state_summary = assistant_prompt_state(
        state,
        &library_root,
        result_count,
        pending_preview,
        current_album_id,
    )
    .await?;
    let mut messages = vec![ProviderMessage {
        role: "system".into(),
        content: Some(format!("{}\n{}", SYSTEM_PROMPT, state_summary,)),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }];
    let start = session_messages
        .len()
        .saturating_sub(MAX_RECENT_SESSION_MESSAGES);
    for item in &session_messages[start..] {
        messages.push(ProviderMessage {
            role: item.role.clone(),
            content: Some(item.content.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
    Ok(messages)
}

async fn assistant_prompt_state(
    state: &AppState,
    library_root: &str,
    result_count: usize,
    pending_preview: Option<(String, usize)>,
    current_album_id: Option<i64>,
) -> CommandResult<String> {
    let lib_guard = state.library.read().await;
    let lib = lib_guard.as_ref().ok_or(CommandError::LibraryClosed)?;
    ensure_current_library(&lib.drive_root, library_root)?;
    let db = lib.db.lock().await;
    let total_photos: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
        [],
        |row| row.get(0),
    )?;
    let semantic_available = SemanticSearchService::new(&lib.drive_root)
        .status(&db.conn)
        .map(|status| {
            status.assets_installed && status.onnx_runtime_installed && status.indexed_photos > 0
        })
        .unwrap_or(false);
    let pending = pending_preview
        .map(|(name, count)| format!("pending album preview: \"{name}\" ({count} photos)"))
        .unwrap_or_else(|| "pending album preview: none".into());
    let current_album = if let Some(album_id) = current_album_id {
        let repo = AlbumRepo::new(&db.conn);
        repo.get_all()?
            .into_iter()
            .find(|album| album.id == album_id)
            .map(|album| format!("current album: \"{}\" (id {})", album.name, album.id))
            .unwrap_or_else(|| "current album: none".into())
    } else {
        "current album: none".into()
    };
    Ok(format!(
        "State: active photos: {total_photos}; current result set: {result_count} photos; {pending}; {current_album}; semantic search: {}. For references like this, these, those, or previous results, use the current_result_ids recorded in recent conversation history.",
        if semantic_available { "available" } else { "unavailable" }
    ))
}

async fn call_provider(
    cfg: &smriti::config::AppConfig,
    messages: &[ProviderMessage],
    tool_choice: Value,
) -> CommandResult<ProviderResponse> {
    let key = cfg
        .assistant_api_key
        .as_deref()
        .ok_or_else(|| CommandError::Validation {
            field: "assistant_api_key".into(),
            reason: "Assistant API key is not configured".into(),
        })?;
    let url = format!(
        "{}/chat/completions",
        cfg.assistant_base_url.trim_end_matches('/')
    );
    let body = json!({
        "model": cfg.assistant_model,
        "messages": messages,
        "tools": assistant_tools(),
        "tool_choice": tool_choice,
        "temperature": 0.2,
        "max_tokens": 180
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(PROVIDER_TIMEOUT_SECS))
        .build()
        .map_err(|e| CommandError::internal(format!("assistant provider client failed: {e}")))?;
    let res = client
        .post(url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| CommandError::internal(format!("assistant provider request failed: {e}")))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(CommandError::internal(format!(
            "assistant provider returned {status}: {text}"
        )));
    }
    res.json::<ProviderResponse>().await.map_err(|e| {
        CommandError::internal(format!("assistant provider response was invalid: {e}"))
    })
}

fn assistant_tools() -> Value {
    json!([
      {"type":"function","function":{"name":"resolve_people","description":"Resolve explicit person names or relation labels only, such as Amma, mom, dad, John. Do not use for visual objects, brands, activities, animals, scenes, or ordinary nouns.","parameters":{"type":"object","properties":{"queries":{"type":"array","items":{"type":"string"}}},"required":["queries"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"resolve_places","description":"Resolve place names against places present in this library.","parameters":{"type":"object","properties":{"queries":{"type":"array","items":{"type":"string"}}},"required":["queries"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"resolve_date_range","description":"Resolve a date phrase such as 2014, last December, or Oct 2022.","parameters":{"type":"object","properties":{"phrase":{"type":"string"}},"required":["phrase"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"search_photos","description":"Search photos with structured filters. Default combine_mode is intersect. Use union_with_current to add matches to prior results.","parameters":{"type":"object","properties":{"person_ids":{"type":"array","items":{"type":"integer"}},"places":{"type":"array","items":{"type":"object","properties":{"city":{"type":["string","null"]},"country":{"type":["string","null"]},"label":{"type":"string"}},"required":["city","country","label"],"additionalProperties":false}},"date_phrase":{"type":["string","null"]},"media_type":{"type":["string","null"],"enum":["photo","video",null]},"people_only":{"type":"boolean"},"semantic_text":{"type":["string","null"]},"include_photo_ids":{"type":"array","items":{"type":"integer"}},"exclude_photo_ids":{"type":"array","items":{"type":"integer"}},"combine_mode":{"type":"string","enum":["intersect","union","union_with_current"]}},"additionalProperties":false}}},
      {"type":"function","function":{"name":"search_albums","description":"Find existing albums by name when the user refers to an album or asks to add photos to an existing album.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"add_photos_to_album","description":"Add photos to an existing album. Use only when the user explicitly asks to add photos to an album. Provide album_id and either photo_ids or use_current_result_set.","parameters":{"type":"object","properties":{"album_id":{"type":"integer"},"photo_ids":{"type":"array","items":{"type":"integer"}},"use_current_result_set":{"type":"boolean"}},"required":["album_id"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"preview_album","description":"Prepare an album for user approval. Use only when the user explicitly asks to create or make an album.","parameters":{"type":"object","properties":{"album_name":{"type":"string"},"photo_ids":{"type":"array","items":{"type":"integer"}},"use_current_result_set":{"type":"boolean"}},"required":["album_name"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"ask_user","description":"Ask a concise clarification question when intent or resolved entities are ambiguous. Provide options when the possible answers are known.","parameters":{"type":"object","properties":{"question":{"type":"string"},"options":{"type":"array","items":{"type":"string"}}},"required":["question"],"additionalProperties":false}}}
    ])
}

fn validate_search_tool_call(args: &AssistantSearchArgs) -> Option<String> {
    let text = args
        .semantic_text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let tokens = text
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Some("semantic_text was empty after normalization".into());
    }
    let blocked = [
        "add", "album", "albums", "these", "those", "this", "it", "taken", "other", "others",
        "all", "photos", "photo", "with", "from", "to", "create", "make",
    ];
    if let Some(token) = tokens
        .iter()
        .find(|token| blocked.contains(&token.as_str()))
    {
        return Some(format!(
            "semantic_text contains command/reference word '{token}'. Resolve entities or use current_result_ids from conversation history instead."
        ));
    }
    let allowed_short_visual_terms = ["car", "bus", "dog", "cat", "sea", "sky", "sun"];
    if let Some(token) = tokens
        .iter()
        .find(|token| token.len() <= 3 && !allowed_short_visual_terms.contains(&token.as_str()))
    {
        return Some(
            format!("semantic_text contains short unresolved token '{token}'. Resolve it as a person/place/date or ask_user before searching."),
        );
    }
    None
}

async fn current_user_album_requested(state: &AppState, run_id: &str) -> CommandResult<bool> {
    let assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get(run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
    Ok(session
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| album_requested_in_text(&m.content))
        .unwrap_or(false))
}

async fn plain_search_text(state: &AppState, run_id: &str) -> CommandResult<Option<String>> {
    let assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get(run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
    if !session.current_result_ids.is_empty() || session.messages.len() != 1 {
        return Ok(None);
    }
    let Some(message) = session.messages.last().filter(|m| m.role == "user") else {
        return Ok(None);
    };
    Ok(simple_find_query(&message.content))
}

fn simple_find_query(message: &str) -> Option<String> {
    let lower = message.to_lowercase();
    let has_search_verb = lower
        .split_whitespace()
        .any(|token| matches!(token, "find" | "show" | "search" | "photos" | "pictures"));
    if !has_search_verb || album_requested_in_text(message) || has_followup_reference(&lower) {
        return None;
    }
    let tokens = message
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|token| !token.is_empty())
        .filter(|token| {
            let lower = token.to_lowercase();
            !matches!(
                lower.as_str(),
                "find"
                    | "show"
                    | "search"
                    | "me"
                    | "all"
                    | "photo"
                    | "photos"
                    | "picture"
                    | "pictures"
                    | "image"
                    | "images"
                    | "of"
                    | "with"
                    | "containing"
                    | "that"
                    | "have"
                    | "has"
            )
        })
        .collect::<Vec<_>>();
    let text = tokens.join(" ");
    (!text.is_empty()).then_some(text)
}

fn has_followup_reference(lower: &str) -> bool {
    lower.split_whitespace().any(|token| {
        matches!(
            token.trim_matches(|c: char| !c.is_alphanumeric()),
            "this" | "these" | "those" | "previous" | "same" | "them" | "it"
        )
    })
}

fn album_requested_in_text(text: &str) -> bool {
    let tokens = text
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect::<Vec<_>>();
    let has_album = tokens.iter().any(|t| t == "album" || t == "albums");
    let has_action = tokens.iter().any(|t| {
        matches!(
            t.as_str(),
            "add" | "create" | "make" | "build" | "save" | "put"
        )
    });
    has_album && has_action
}

fn string_vec_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn push_context_message(session: &mut AssistantSession) {
    let ids = session
        .current_result_ids
        .iter()
        .take(200)
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let more = session.current_result_ids.len().saturating_sub(200);
    let more_text = if more > 0 {
        format!("; current_result_ids_truncated_by={more}")
    } else {
        String::new()
    };
    let album_text = session
        .run
        .album_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "none".into());
    session.messages.push(AssistantMessage {
        role: "assistant".into(),
        content: format!(
            "Context for follow-ups: current_result_count={}; current_result_ids=[{}]{}; current_album_id={}.",
            session.current_result_ids.len(),
            ids,
            more_text,
            album_text
        ),
    });
}

async fn emit_activity(
    app: &AppHandle,
    state: &AppState,
    run_id: &str,
    library_root: &str,
    label: &str,
) {
    emit(
        app,
        EV_ASSISTANT_ACTIVITY,
        AssistantActivityEvent {
            run_id: run_id.to_string(),
            library_root: library_root.to_string(),
            label: label.to_string(),
        },
    );
    let mut assistant = state.assistant.lock().await;
    if let Some(session) = assistant.sessions.get_mut(run_id) {
        session.run.activity.push(AssistantActivity {
            label: label.to_string(),
        });
    }
}

fn validate_approval(session: &AssistantSession, approval_id: &str) -> CommandResult<()> {
    let expected = session
        .run
        .preview
        .as_ref()
        .map(|p| p.approval_id.as_str())
        .ok_or_else(|| CommandError::Conflict {
            reason: "no pending album preview".into(),
        })?;
    if expected != approval_id {
        return Err(CommandError::Validation {
            field: "approval_id".into(),
            reason: "approval token does not match the pending preview".into(),
        });
    }
    Ok(())
}

async fn ensure_session_library(
    state: &AppState,
    run_id: &str,
    library_root: &str,
) -> CommandResult<()> {
    let assistant = state.assistant.lock().await;
    let session = assistant
        .sessions
        .get(run_id)
        .ok_or_else(|| CommandError::not_found("assistant_run", run_id))?;
    if session.library_root != library_root {
        return Err(CommandError::Conflict {
            reason: "Assistant run belongs to a different library".into(),
        });
    }
    Ok(())
}

fn ensure_current_library(
    drive_root: &std::path::Path,
    expected_library_root: &str,
) -> CommandResult<()> {
    if drive_root.display().to_string() != expected_library_root {
        return Err(CommandError::Conflict {
            reason: "Assistant run belongs to a different library".into(),
        });
    }
    Ok(())
}

fn ensure_enabled(cfg: &smriti::config::AppConfig) -> CommandResult<()> {
    if !cfg.ai_features_enabled || !cfg.assistant_enabled {
        return Err(CommandError::Validation {
            field: "assistant".into(),
            reason: "Assistant is disabled in Settings".into(),
        });
    }
    Ok(())
}

fn clean_message(message: String) -> CommandResult<String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err(CommandError::Validation {
            field: "message".into(),
            reason: "message is required".into(),
        });
    }
    if message.chars().count() > MAX_ASSISTANT_MESSAGE_CHARS {
        return Err(CommandError::Validation {
            field: "message".into(),
            reason: format!("message must be at most {MAX_ASSISTANT_MESSAGE_CHARS} characters"),
        });
    }
    Ok(message)
}

fn prune_sessions(sessions: &mut HashMap<String, AssistantSession>, max_sessions: usize) {
    if sessions.len() <= max_sessions {
        return;
    }
    let mut keys = sessions.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    let remove_count = sessions.len().saturating_sub(max_sessions);
    for key in keys.into_iter().take(remove_count) {
        sessions.remove(&key);
    }
}

fn semantic_photo_ids(
    drive_root: &std::path::Path,
    semantic_index: &std::sync::Arc<std::sync::Mutex<SemanticIndexCache>>,
    semantic_runner: &std::sync::Arc<std::sync::Mutex<Option<SemanticModelRunner>>>,
    conn: &rusqlite::Connection,
    text: &str,
) -> CommandResult<Vec<i64>> {
    let svc = SemanticSearchService::new(drive_root);
    match svc.status(conn) {
        Ok(status)
            if status.assets_installed
                && status.onnx_runtime_installed
                && status.indexed_photos > 0 =>
        {
            let mut cache = semantic_index
                .lock()
                .map_err(|_| CommandError::internal("semantic index cache poisoned"))?;
            let mut runner_guard = semantic_runner
                .lock()
                .map_err(|_| CommandError::internal("semantic model cache poisoned"))?;
            if runner_guard.is_none() {
                match SemanticSearchService::model_runner() {
                    Ok(runner) => *runner_guard = Some(runner),
                    Err(err) => {
                        tracing::debug!("assistant semantic model unavailable: {}", err);
                        return Ok(Vec::new());
                    }
                }
            }
            let Some(runner) = runner_guard.as_mut() else {
                tracing::debug!("assistant semantic runner missing after initialization");
                return Ok(Vec::new());
            };
            let candidates = svc
                .search_text_cached(conn, &mut cache, runner, text, SEMANTIC_TEXT_SEARCH_LIMIT)
                .unwrap_or_else(|err| {
                    tracing::debug!("assistant semantic search skipped: {}", err);
                    Vec::new()
                });
            Ok(relevant_text_search_candidates(candidates)
                .into_iter()
                .map(|c| c.photo_id)
                .collect())
        }
        _ => Ok(Vec::new()),
    }
}

fn new_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos}")
}

const SYSTEM_PROMPT: &str = r#"You are Smriti's photo assistant. You operate only through tools.

Default behavior: show matching photos. Create an album only when the user explicitly asks to create/make/build/save an album.
For follow-ups, use session context and the current_result_ids recorded in conversation history for words like this, those, these, or previous results.
Resolve explicit people, places, and dates with tools before searching. Visual concepts such as car, beach, hiking, food, dog, sea, or brands belong in semantic_text, not resolve_people. If a resolve tool returns missing items or multiple candidates, call ask_user; do not guess aliases.
When asking the user to choose between known candidates, call ask_user with an options array. Do not write bullet lists manually.
If the user refers to an existing album or "it" after an album was created, use the current album from State or search_albums before adding photos.
For album previews, provide a clean personal album name. Do not derive names from raw leftover prompt text.
Never claim to inspect original image files. Use semantic_text only for visual concepts the user actually requested, not command words.
Final responses must be one short sentence. Do not mention active photos, current result set, pending preview, semantic availability, tool names, or internal state."#;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderResponse {
    choices: Vec<ProviderChoice>,
}

#[derive(Debug, Deserialize)]
struct ProviderChoice {
    message: ProviderAssistantMessage,
}

#[derive(Debug, Deserialize)]
struct ProviderAssistantMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type", default = "tool_call_type")]
    kind: String,
    function: ToolFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolFunctionCall {
    name: String,
    arguments: String,
}

fn tool_call_type() -> String {
    "function".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_messages_include_session_context() {
        let msg = ProviderMessage {
            role: "system".into(),
            content: Some(SYSTEM_PROMPT.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        let json = serde_json::to_value(msg).unwrap();
        assert_eq!(json["role"], "system");
        assert!(json["content"]
            .as_str()
            .unwrap()
            .contains("Default behavior"));
    }

    #[test]
    fn tool_schema_contains_album_approval_boundary() {
        let schema = assistant_tools();
        let text = schema.to_string();
        assert!(!text.contains("get_library_context"));
        assert!(!text.contains("get_current_result_set"));
        assert!(text.contains("preview_album"));
        assert!(text.contains("search_albums"));
        assert!(text.contains("add_photos_to_album"));
        assert!(text.contains("options"));
        assert!(text.contains("explicitly asks"));
        assert!(text.contains("ask_user"));
    }

    #[test]
    fn recent_message_window_is_small() {
        assert_eq!(MAX_RECENT_SESSION_MESSAGES, 10);
    }

    #[test]
    fn assistant_message_length_is_bounded() {
        assert!(clean_message("show beach photos".into()).is_ok());
        let too_long = "x".repeat(MAX_ASSISTANT_MESSAGE_CHARS + 1);
        assert!(matches!(
            clean_message(too_long),
            Err(CommandError::Validation { .. })
        ));
    }

    #[test]
    fn assistant_session_count_is_bounded() {
        assert_eq!(MAX_ASSISTANT_SESSIONS, 20);
    }

    #[test]
    fn album_intent_is_validator_only_but_detects_followups() {
        assert!(album_requested_in_text(
            "add these to an album and include all photos from Vizianagaram"
        ));
        assert!(album_requested_in_text("create an album with this"));
        assert!(!album_requested_in_text(
            "show me car photos in Vizianagaram"
        ));
    }

    #[test]
    fn semantic_text_rejects_command_words_and_short_aliases() {
        let args = AssistantSearchArgs {
            semantic_text: Some("add these album taken vzm".into()),
            ..Default::default()
        };
        assert!(validate_search_tool_call(&args).is_some());

        let args = AssistantSearchArgs {
            semantic_text: Some("car tata".into()),
            ..Default::default()
        };
        assert!(validate_search_tool_call(&args).is_none());

        let args = AssistantSearchArgs {
            semantic_text: Some("car vzm".into()),
            person_ids: vec![1],
            ..Default::default()
        };
        assert!(validate_search_tool_call(&args).is_some());
    }

    #[test]
    fn simple_find_query_keeps_visual_terms() {
        assert_eq!(
            simple_find_query("find car tata hiking").as_deref(),
            Some("car tata hiking")
        );
        assert_eq!(
            simple_find_query("show me photos with beach and dog").as_deref(),
            Some("beach and dog")
        );
        assert!(simple_find_query("add these to an album").is_none());
        assert!(simple_find_query("create an album of beach photos").is_none());
    }

    #[test]
    fn running_status_is_not_terminal() {
        assert_eq!(AssistantRunStatus::Running, AssistantRunStatus::Running);
        assert_ne!(
            AssistantRunStatus::Running,
            AssistantRunStatus::WaitingForClarification
        );
    }
}
