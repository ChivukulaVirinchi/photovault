//! People view - face clusters display
//!
//! Shows detected face clusters in a grid. Each card represents a person
//! with their face count. Names are editable inline.
//! Also includes the cluster detail view showing photos for a specific person.

mod cards;
mod detail;
mod grid;

use iced::Element;

use crate::app::Message;
use crate::config::AppTheme;
use crate::db::FaceClusterRecord;
use crate::models::Photo;
use crate::services::FaceProcessingProgress;

/// People view component
pub struct PeopleView;

impl PeopleView {
    /// Render with clusters and processing state
    pub fn view_with_clusters(
        clusters: &[FaceClusterRecord],
        editing_cluster: Option<i64>,
        edit_name: &str,
        processing_active: bool,
        progress: Option<&FaceProcessingProgress>,
        processing_error: Option<&str>,
        merge_mode_active: bool,
        merge_selected: &[i64],
        ml_available: bool,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        grid::view_with_clusters(
            clusters,
            editing_cluster,
            edit_name,
            processing_active,
            progress,
            processing_error,
            merge_mode_active,
            merge_selected,
            ml_available,
            theme,
        )
    }

    /// Render cluster detail view showing photos for a specific person
    pub fn view_cluster_detail(
        cluster: &FaceClusterRecord,
        photos: &[Photo],
        editing: bool,
        edit_name: &str,
        columns: usize,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        detail::view_cluster_detail(cluster, photos, editing, edit_name, columns, theme)
    }
}
