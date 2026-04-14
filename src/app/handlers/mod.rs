//! Message dispatch for the application.

use iced::Task;

use super::messages::Message;
use super::state::PhotoVault;

mod bursts;
mod documents;
mod duplicates;
mod faces;
mod scanning;
mod search_cull;
mod settings;
mod thumbnails;
mod timeline;
mod trash;

pub(crate) fn handle(app: &mut PhotoVault, message: Message) -> Task<Message> {
    match message {
        // --- Scanning / navigation ---
        Message::NavigateTo(view) => scanning::navigate_to(app, view),
        Message::SelectDrive(path) => scanning::select_drive(app, path),
        Message::BrowseForFolder => scanning::browse_for_folder(app),
        Message::FolderSelected(path) => scanning::folder_selected(app, path),
        Message::DrivesDetected(drives) => scanning::drives_detected(app, drives),
        Message::StartScan => scanning::start_scan(app),
        Message::PollScanChannels => scanning::poll_scan_channels(app),
        Message::CancelScan => scanning::cancel_scan(app),
        Message::ScanFinished(result) => scanning::scan_finished(app, result),
        Message::ScanComplete => scanning::scan_complete(app),

        // --- Timeline / photo detail ---
        Message::PhotosLoaded(photos) => timeline::photos_loaded(app, photos),
        Message::SelectPhoto(photo_id) => timeline::select_photo(app, photo_id),
        Message::ToggleTimelinePhotoSelection(photo_id) => {
            timeline::toggle_timeline_photo_selection(app, photo_id)
        }
        Message::ToggleTimelineDaySelection(day_key) => {
            timeline::toggle_timeline_day_selection(app, day_key)
        }
        Message::TimelinePhotoHover(photo_id) => timeline::timeline_photo_hover(app, photo_id),
        Message::TimelineDayHover(day_key) => timeline::timeline_day_hover(app, day_key),
        Message::ClearTimelinePhotoSelection => timeline::clear_timeline_photo_selection(app),
        Message::ClosePhotoDetail => timeline::close_photo_detail(app),
        Message::PreviousPhoto => timeline::previous_photo(app),
        Message::NextPhoto => timeline::next_photo(app),
        Message::KeyPressed(key) => timeline::key_pressed(app, key),
        Message::RotatePhoto => timeline::rotate_photo(app),
        Message::ToggleMetadataPanel => timeline::toggle_metadata_panel(app),
        Message::DisplayImageReady(bytes_opt, w, h) => {
            timeline::display_image_ready(app, bytes_opt, w, h)
        }

        // --- Thumbnails ---
        Message::ThumbnailBatchReady(epoch, results) => {
            thumbnails::thumbnail_batch_ready(app, epoch, results)
        }
        Message::ThumbnailBatchSaved(epoch) => thumbnails::thumbnail_batch_saved(app, epoch),
        Message::ContinueThumbnailScheduling(epoch) => {
            thumbnails::continue_thumbnail_scheduling(app, epoch)
        }

        // --- Faces ---
        Message::ProcessFaces => faces::process_faces(app),
        Message::FaceProcessingComplete(result) => faces::face_processing_complete(app, result),
        Message::RunClustering => faces::run_clustering(app),
        Message::ClusteringComplete(result) => faces::clustering_complete(app, result),
        Message::FaceClustersLoaded(clusters) => faces::face_clusters_loaded(app, clusters),
        Message::SelectCluster(cluster_id) => faces::select_cluster(app, cluster_id),
        Message::BackToPeople => faces::back_to_people(app),
        Message::StartEditClusterName(cluster_id) => {
            faces::start_edit_cluster_name(app, cluster_id)
        }
        Message::EditClusterName(cluster_id, name) => {
            faces::edit_cluster_name(app, cluster_id, name)
        }
        Message::SaveClusterName(cluster_id) => faces::save_cluster_name(app, cluster_id),
        Message::ToggleMergeMode => faces::toggle_merge_mode(app),
        Message::ToggleMergeSelect(id) => faces::toggle_merge_select(app, id),
        Message::MergeSelectedClusters => faces::merge_selected_clusters(app),
        Message::CancelFaceProcessing => faces::cancel_face_processing(app),
        Message::RebuildFaceClusters => faces::rebuild_face_clusters(app),
        Message::FaceDataResetComplete(result) => faces::face_data_reset_complete(app, result),

        // --- Duplicates ---
        Message::RunDuplicateDetection => duplicates::run_duplicate_detection(app),
        Message::DuplicateDetectionComplete(groups, wasted, overview) => {
            duplicates::duplicate_detection_complete(app, groups, wasted, overview)
        }
        Message::OpenDuplicateGroup(group_id) => duplicates::open_duplicate_group(app, group_id),
        Message::CloseDuplicateDetail => duplicates::close_duplicate_detail(app),
        Message::SetKeepDuplicate(group_id, photo_id) => {
            duplicates::set_keep_duplicate(app, group_id, photo_id)
        }
        Message::KeepSuggestedDuplicate(group_id) => {
            duplicates::keep_suggested_duplicate(app, group_id)
        }
        Message::TrashNonSuggestedDuplicates(group_id) => {
            duplicates::trash_non_suggested_duplicates(app, group_id)
        }
        Message::DismissDuplicateGroup(group_id) => {
            duplicates::dismiss_duplicate_group(app, group_id)
        }

        // --- Bursts ---
        Message::RunBurstDetection => bursts::run_burst_detection(app),
        Message::BurstDetectionComplete(groups, saveable, previews) => {
            bursts::burst_detection_complete(app, groups, saveable, previews)
        }
        Message::OpenBurstGroup(group_id) => bursts::open_burst_group(app, group_id),
        Message::CloseBurstDetail => bursts::close_burst_detail(app),
        Message::SetBestFromBurst(group_id, photo_id) => {
            bursts::set_best_from_burst(app, group_id, photo_id)
        }
        Message::KeepBestFromBurst(group_id) => bursts::keep_best_from_burst(app, group_id),
        Message::TrashNonBestFromBurst(group_id) => {
            bursts::trash_non_best_from_burst(app, group_id)
        }
        Message::DismissBurstGroup(group_id) => bursts::dismiss_burst_group(app, group_id),

        // --- Search / cull ---
        Message::SearchInputChanged(input) => search_cull::search_input_changed(app, input),
        Message::SearchSuggestionSelected(value) => {
            search_cull::search_suggestion_selected(app, value)
        }
        Message::ExecuteSearch => search_cull::execute_search(app),
        Message::SearchComplete(groups, ids) => search_cull::search_complete(app, groups, ids),
        Message::SearchSuggestionsLoaded(suggestions) => {
            search_cull::search_suggestions_loaded(app, suggestions)
        }
        Message::EnterCullMode(photo_ids) => search_cull::enter_cull_mode(app, photo_ids),
        Message::EnterCullFromSearch => search_cull::enter_cull_from_search(app),
        Message::ExitCullMode => search_cull::exit_cull_mode(app),
        Message::CullNext => search_cull::cull_next(app),
        Message::CullPrev => search_cull::cull_prev(app),
        Message::CullToggleTrash => search_cull::cull_toggle_trash(app),
        Message::CullUndo => search_cull::cull_undo(app),
        Message::CullFinish => search_cull::cull_finish(app),
        Message::CullConfirmTrash => search_cull::cull_confirm_trash(app),

        // --- Trash ---
        Message::LoadTrash => trash::load_trash(app),
        Message::TrashLoaded(items, stats) => trash::trash_loaded(app, items, stats),
        Message::TrashPhotos(photo_ids) => trash::trash_photos(app, photo_ids),
        Message::RestorePhoto(photo_id) => trash::restore_photo(app, photo_id),
        Message::RestoreSelected => trash::restore_selected(app),
        Message::ToggleTrashSelection(photo_id) => trash::toggle_trash_selection(app, photo_id),
        Message::PermanentlyDeletePhoto(photo_id) => {
            trash::permanently_delete_photo(app, photo_id)
        }
        Message::ConfirmPermanentlyDeletePhoto(photo_id) => {
            trash::confirm_permanently_delete_photo(app, photo_id)
        }
        Message::EmptyTrash => trash::empty_trash(app),
        Message::ConfirmEmptyTrash => trash::confirm_empty_trash(app),

        // --- Documents ---
        Message::LoadDocuments => documents::load_documents(app),
        Message::DocumentsLoaded(items) => documents::documents_loaded(app, items),
        Message::DocumentsSearchChanged(input) => documents::documents_search_changed(app, input),
        Message::DocumentsFilterCategory(category) => {
            documents::documents_filter_category(app, category)
        }
        Message::RunDocumentAnalysis => documents::run_document_analysis(app),
        Message::DocumentAnalysisComplete(result) => {
            documents::document_analysis_complete(app, result)
        }

        // --- Settings / reindexing / geocoding ---
        Message::SetTheme(theme) => settings::set_theme(app, theme),
        Message::SetThumbnailSize(size) => settings::set_thumbnail_size(app, size),
        Message::SetScanHiddenFolders(enabled) => settings::set_scan_hidden_folders(app, enabled),
        Message::SetFaceConfidence(v) => settings::set_face_confidence(app, v),
        Message::SetClusteringThreshold(v) => settings::set_clustering_threshold(app, v),
        Message::SetBurstWindow(seconds) => settings::set_burst_window(app, seconds),
        Message::SetTrashAutoDelete(days) => settings::set_trash_auto_delete(app, days),
        Message::SetDateFormat(format) => settings::set_date_format(app, format),
        Message::RescanLibrary => settings::rescan_library(app),
        Message::CheckForChanges => settings::check_for_changes(app),
        Message::ChangesDetected(changes) => settings::changes_detected(app, changes),
        Message::ApplyChanges => settings::apply_changes(app),
        Message::ChangesApplied(result) => settings::changes_applied(app, result),
        Message::RunGeocoding => settings::run_geocoding(app),
        Message::GeocodingProgress { processed, total } => {
            settings::geocoding_progress(app, processed, total)
        }
        Message::GeocodingComplete => settings::geocoding_complete(app),
        Message::RegenerateRotatedData => settings::regenerate_rotated_data(app),
        Message::RotatedDataRegenerated {
            cleared_thumbnails,
            reset_faces,
        } => settings::rotated_data_regenerated(app, cleared_thumbnails, reset_faces),
        Message::ToggleSidebar => settings::toggle_sidebar(app),
        Message::NoOp => settings::no_op(app),
    }
}
