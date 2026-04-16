//! Application views

pub mod albums;
pub mod bursts;
pub mod insights;
pub mod cull;
pub mod documents;
pub mod duplicates;
pub mod face_review;
pub mod map;
pub mod memories;
pub mod people;
pub mod photo_detail;
pub mod photo_detail_map;
pub mod search;
pub mod settings;
pub mod shortcuts;
pub mod timeline;
pub mod trash;
pub mod welcome;

pub use bursts::BurstsView;
pub use cull::{CullState, CullView};
pub use documents::DocumentsView;
pub use duplicates::DuplicatesView;
pub use face_review::FaceReviewView;
pub use people::PeopleView;
pub use photo_detail::PhotoDetailView;
pub use search::SearchView;
pub use settings::SettingsView;
pub use timeline::TimelineView;
pub use trash::TrashView;
pub use welcome::WelcomeView;
