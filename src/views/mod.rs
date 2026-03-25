//! Application views

pub mod bursts;
pub mod cull;
pub mod duplicates;
pub mod people;
pub mod photo_detail;
pub mod search;
pub mod settings;
pub mod timeline;
pub mod trash;
pub mod welcome;

pub use bursts::BurstsView;
pub use cull::{CullState, CullView};
pub use duplicates::DuplicatesView;
pub use people::PeopleView;
pub use photo_detail::PhotoDetailView;
pub use search::SearchView;
pub use settings::SettingsView;
pub use timeline::TimelineView;
pub use trash::TrashView;
pub use welcome::WelcomeView;
