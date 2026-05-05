//! Application views.
//!
//! Several view-rendering entry points take many parameters because
//! they receive narrowly-scoped slices of the app state instead of the
//! monolithic `PhotoVault` struct. This is intentional during Phase 1;
//! Phase 2 splits the state into sub-structs and these signatures
//! collapse to a single param. Until then the lint is allowed module-wide.
#![allow(clippy::too_many_arguments)]

pub mod album_suggestion_preview;
pub mod albums;
pub mod albums_banner;
pub mod albums_picker;
pub mod bursts;
pub mod cull;
pub mod documents;
pub mod duplicates;
pub mod face_review;
pub mod insights;
pub mod insights_sections;
pub mod library_health;
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
pub use library_health::LibraryHealthView;
pub use people::PeopleView;
pub use photo_detail::PhotoDetailView;
pub use search::SearchView;
pub use settings::SettingsView;
pub use timeline::TimelineView;
pub use trash::TrashView;
pub use welcome::WelcomeView;
