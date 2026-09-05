//! Tauri shell crate for Smriti.
//!
//! Wraps the `smriti` library (engine) in IPC handlers. The contract
//! is documented in `docs/COMMAND_SURFACE.md`.

pub mod commands;
pub mod dto;
pub mod error;
pub mod events;
pub mod jobs;
pub mod pagination;
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
            // documents
            commands::documents::documents_list,
            commands::documents::documents_search,
            commands::documents::documents_set_category,
            // map
            commands::map::map_pins,
            commands::map::map_pins_all,
            commands::map::map_cluster_filmstrip,
            // insights
            commands::insights::insights_compute,
            commands::insights::insights_invalidate,
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
            commands::system::system_app_version,
            commands::system::system_inference_provider,
            commands::system::system_open_in_explorer,
            commands::system::system_open_path,
            commands::system::system_copy_path_to_clipboard,
            commands::system::system_updates_check,
            commands::system::system_test_gpu_bridge,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
#[cfg(test)]
mod ipc_contract_tests {
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
