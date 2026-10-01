//! Tauri shell crate for Smriti.
//!
//! Wraps the `smriti` library (engine) in IPC handlers. The contract
//! is documented in `docs/COMMAND_SURFACE.md`.

use tauri::Manager;

pub mod commands;
pub mod dto;
pub mod error;
pub mod events;
pub mod jobs;
pub mod pagination;
pub mod platform_share;
pub mod state;
pub mod thumbnail_upgrade;

pub use error::{CommandError, CommandResult};
pub use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
            // Existing instance: focus the main window. Tauri's plugin
            // doesn't restore focus by default — explicit show/focus
            // would go here once we track the window handle.
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            // The semantic text model is application-wide and optional.
            // Start it on a blocking worker as soon as the native shell is
            // alive; neither window creation nor library_open awaits it.
            // Search uses try_lock and remains responsive while this runs.
            if smriti::services::semantic::SemanticSearchService::model_assets_installed()
                && smriti::bootstrap::onnx_runtime_exists()
            {
                let runner = app.state::<AppState>().semantic_runner.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    // Loading a large optional model must yield to shell and
                    // library I/O on Windows, especially on low-end PCs.
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use windows::Win32::System::Threading::{
                            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
                        };
                        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                    }
                    let Ok(mut guard) = runner.lock() else {
                        tracing::debug!("semantic runtime preload skipped: cache poisoned");
                        return;
                    };
                    if guard.is_none() {
                        match smriti::services::semantic::SemanticSearchService::model_runner() {
                            Ok(loaded) => *guard = Some(loaded),
                            Err(error) => {
                                tracing::debug!("semantic runtime preload skipped: {error}")
                            }
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // library
            commands::library::library_list_drives,
            commands::library::library_current,
            commands::library::library_resolve_path,
            commands::library::library_detect_changes,
            commands::library::library_exclusions_list,
            commands::library::library_exclusions_preview,
            commands::library::library_exclusions_add,
            commands::library::library_exclusions_remove,
            commands::library::library_open,
            commands::library::library_close,
            commands::library::library_compat_photos_list,
            commands::library::library_force_thumbnail_repair,
            commands::library::library_apply_changes,
            commands::library::library_start_scan,
            commands::library::library_cancel_scan,
            commands::library::library_start_metadata_extraction,
            commands::library::library_refresh_photo_dates,
            commands::library::library_start_thumbnail_pass,
            commands::library::library_pending_metadata_count,
            commands::library::library_pending_thumbnail_count,
            commands::library::jobs_cancel,
            commands::library::library_regenerate_thumbnails,
            // Google Photos Takeout
            commands::takeout::takeout_start_import,
            // photos
            commands::photos::photos_list,
            commands::photos::photos_list_at,
            commands::photos::photos_get,
            commands::photos::photos_get_many,
            commands::photos::photos_set_favorite,
            commands::photos::photos_list_by_album,
            commands::photos::photos_list_by_person,
            commands::photos::photos_list_by_date,
            commands::photos::photos_list_by_place,
            commands::photos::photos_people_in_photo,
            commands::photos::photos_albums_for_photo,
            commands::photos::photos_exif_extras,
            commands::photos::photos_timeline_neighbors,
            commands::photos::photos_request_thumbnail,
            commands::photos::photos_request_thumbnails,
            commands::photos::photos_save_video_probe,
            // people
            commands::people::people_list,
            commands::people::people_get,
            commands::people::people_photo_ids,
            commands::people::people_review_queue,
            commands::people::people_rename,
            commands::people::people_merge,
            commands::people::people_delete,
            commands::people::people_review_same,
            commands::people::people_review_different,
            commands::people::people_review_skip,
            commands::people::people_start_processing,
            commands::people::people_cancel_processing,
            commands::people::people_reset_all,
            commands::people::people_reset_clusters,
            commands::people::people_pending_face_count,
            commands::people::people_clustering_diagnostics,
            commands::people::people_face_list,
            commands::people::people_unclustered_faces,
            commands::people::people_face_confirm,
            commands::people::people_face_confirm_to_cluster,
            commands::people::people_face_reject,
            commands::people::people_face_hide,
            commands::people::people_face_reassign,
            commands::people::people_face_suggest_clusters,
            commands::people::people_k_similar_to_cluster,
            commands::people::people_review_face_count,
            commands::people::people_next_unconfirmed_faces,
            // albums
            commands::albums::albums_list,
            commands::albums::albums_get,
            commands::albums::albums_photo_ids,
            commands::albums::albums_suggestions_list,
            commands::albums::albums_suggestions_preview,
            commands::albums::albums_create,
            commands::albums::albums_rename,
            commands::albums::albums_delete,
            commands::albums::albums_add_photos,
            commands::albums::albums_remove_photos,
            commands::albums::albums_auto_pick_cover,
            commands::albums::albums_export,
            commands::albums::albums_suggestions_run_detection,
            commands::albums::albums_suggestions_accept,
            commands::albums::albums_suggestions_dismiss,
            commands::albums::albums_suggestions_reset_all,
            // assistant
            commands::assistant::assistant_start,
            commands::assistant::assistant_continue,
            commands::assistant::assistant_state,
            commands::assistant::assistant_stop,
            commands::assistant::assistant_approve,
            commands::assistant::assistant_reject,
            commands::assistant::assistant_clear,
            // search
            commands::search::search_query,
            commands::search::search_recent_list,
            commands::search::search_recent_remove,
            commands::search::search_recent_clear,
            // semantic search
            commands::semantic::semantic_status,
            commands::semantic::semantic_warm_runtime,
            commands::semantic::semantic_install_model,
            commands::semantic::semantic_start_indexing,
            commands::semantic::semantic_similar_photos,
            // memories
            commands::memories::memories_today,
            commands::memories::memories_surprise,
            commands::memories::memories_detail,
            commands::memories::memories_blocked_people,
            commands::memories::memories_block_person,
            commands::memories::memories_unblock_person,
            commands::memories::memories_save_as_album,
            // duplicates
            commands::duplicates::duplicates_list,
            commands::duplicates::duplicates_get_group,
            commands::duplicates::duplicates_wasted_space,
            commands::duplicates::duplicates_set_keep,
            commands::duplicates::duplicates_trash_others,
            commands::duplicates::duplicates_dismiss,
            commands::duplicates::duplicates_run,
            // bursts
            commands::bursts::bursts_list,
            commands::bursts::bursts_get_group,
            commands::bursts::bursts_set_best,
            commands::bursts::bursts_trash_non_best,
            commands::bursts::bursts_dismiss,
            commands::bursts::bursts_run,
            // stacks
            commands::stacks::stacks_get,
            commands::stacks::stacks_get_for_photo,
            commands::stacks::stacks_set_cover,
            commands::stacks::stacks_remove_member,
            commands::stacks::stacks_unstack,
            commands::stacks::stacks_trash_others,
            commands::stacks::stacks_refresh,
            // trash
            commands::trash::trash_list,
            commands::trash::trash_stats,
            commands::trash::trash_trash_photos,
            commands::trash::trash_restore,
            commands::trash::trash_permanent_delete,
            commands::trash::trash_empty,
            // map
            commands::map::map_pins,
            commands::map::map_pins_all,
            commands::map::map_cluster_filmstrip,
            // insights
            commands::insights::insights_compute,
            commands::insights::insights_invalidate,
            commands::insights::insights_milestone_probe,
            // health
            commands::health::health_compute,
            // geocoding
            commands::geocoding::geocoding_resolve_one,
            commands::geocoding::geocoding_backfill,
            // settings
            commands::settings::settings_get,
            commands::settings::settings_update,
            // system
            commands::system::system_asset_health,
            commands::system::system_assets_inventory,
            commands::system::system_install_assets,
            commands::system::system_asset_setup_status,
            commands::system::system_asset_download_size,
            commands::system::system_app_version,
            commands::system::system_inference_provider,
            commands::system::system_open_in_explorer,
            commands::system::system_share_photo,
            commands::system::system_open_path,
            commands::system::system_copy_path_to_clipboard,
            commands::system::system_updates_check,
            commands::system::system_test_gpu_bridge,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Flush the open library's WAL into the main DB file before the
            // process dies. A catalog copied (or a USB drive yanked) right
            // after the window closes used to be able to lose recent commits
            // that were still sitting in the WAL.
            //
            // Only on `Exit`, not `ExitRequested`. `ExitRequested` fires while
            // the window is still on screen (and a handler may still veto the
            // exit), so doing a blocking checkpoint there made closing the app
            // look like a hang and, because both events fire on a normal quit,
            // ran the checkpoint twice. `Exit` runs once, after the event loop
            // has stopped and the window is gone.
            //
            // `try_read`/`try_lock` stay non-blocking: if a writer holds the
            // connection the checkpoint is skipped rather than stalling exit.
            // `Database::drop` performs a passive checkpoint as backstop.
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<crate::state::AppState>() {
                    if let Ok(library) = state.library.try_read() {
                        if let Some(lib) = library.as_ref() {
                            if let Ok(db) = lib.db.try_lock() {
                                db.checkpoint_truncate();
                            }
                        }
                    }
                }
            }
        });
}
#[cfg(test)]
mod ipc_contract_tests {
    #[test]
    fn packaged_asset_scope_is_cross_platform_and_runtime_granted() {
        let config: tauri::utils::config::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let asset_protocol = &config.app.security.asset_protocol;

        assert!(asset_protocol.enable);
        assert!(
            asset_protocol.scope.allowed_paths().is_empty(),
            "packaged config must not contain OS-specific globs; library_open grants the selected directory"
        );
    }

    #[test]
    fn frontend_envelopes_deserialize_into_command_arguments() {
        use crate::commands::{
            library::ResolvePathArgs, photos::SaveVideoProbeArgs, settings::SettingsUpdateArgs,
        };
        let contracts: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ipc.json")).unwrap();
        let path: ResolvePathArgs =
            serde_json::from_value(contracts["library_resolve_path"]["args"].clone()).unwrap();
        assert_eq!(path.photo_id, 42);
        assert!(path.for_display);
        let settings: SettingsUpdateArgs =
            serde_json::from_value(contracts["settings_update"]["args"].clone()).unwrap();
        assert_eq!(settings.assistant_api_key, Some(None));
        assert_eq!(settings.home_city_override, Some(None));
        let video: SaveVideoProbeArgs =
            serde_json::from_value(contracts["photos_save_video_probe"]["args"].clone()).unwrap();
        assert_eq!(video.library_session_id, 7);
        assert_eq!(video.file_hash, "abcd");
    }
}
