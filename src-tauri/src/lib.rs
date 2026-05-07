//! Tauri shell crate for PhotoVault.
//!
//! Wraps the `photovault` library (engine) in IPC handlers. The contract
//! is documented in `docs/COMMAND_SURFACE.md`. M1 lands the read-only
//! command surface and the runtime scaffolding; M2 adds mutations and
//! long-running jobs; M3 reaches view parity with the iced UI and the
//! iced binary is removed.

pub mod commands;
pub mod dto;
pub mod error;
pub mod events;
pub mod jobs;
pub mod pagination;
pub mod state;

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
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // library
            commands::library::library_list_drives,
            commands::library::library_current,
            commands::library::library_resolve_path,
            commands::library::library_detect_changes,
            commands::library::library_open,
            commands::library::library_close,
            commands::library::library_apply_changes,
            commands::library::library_start_scan,
            commands::library::library_cancel_scan,
            commands::library::library_regenerate_thumbnails,
            // photos
            commands::photos::photos_list,
            commands::photos::photos_get,
            commands::photos::photos_get_many,
            commands::photos::photos_list_by_album,
            commands::photos::photos_list_by_person,
            commands::photos::photos_list_by_date,
            commands::photos::photos_list_by_place,
            commands::photos::photos_people_in_photo,
            commands::photos::photos_albums_for_photo,
            commands::photos::photos_exif_extras,
            commands::photos::photos_request_thumbnail,
            // people
            commands::people::people_list,
            commands::people::people_get,
            commands::people::people_review_queue,
            commands::people::people_rename,
            commands::people::people_merge,
            commands::people::people_delete,
            commands::people::people_review_same,
            commands::people::people_review_different,
            commands::people::people_review_skip,
            commands::people::people_start_processing,
            commands::people::people_cancel_processing,
            // albums
            commands::albums::albums_list,
            commands::albums::albums_get,
            commands::albums::albums_suggestions_list,
            commands::albums::albums_suggestions_preview,
            commands::albums::albums_create,
            commands::albums::albums_rename,
            commands::albums::albums_delete,
            commands::albums::albums_add_photos,
            commands::albums::albums_remove_photos,
            commands::albums::albums_auto_pick_cover,
            commands::albums::albums_suggestions_run_detection,
            commands::albums::albums_suggestions_accept,
            commands::albums::albums_suggestions_dismiss,
            // search
            commands::search::search_query,
            commands::search::search_recent_list,
            commands::search::search_recent_remove,
            commands::search::search_recent_clear,
            // memories
            commands::memories::memories_today,
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
            commands::map::map_tile_cache_stats,
            commands::map::map_tile_cache_set_limit,
            commands::map::map_tile_cache_clear,
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
            commands::system::system_app_version,
            commands::system::system_open_in_explorer,
            commands::system::system_copy_path_to_clipboard,
            commands::system::system_updates_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
