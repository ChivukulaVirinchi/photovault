//! IPC command handlers, grouped by domain.
//!
//! Each handler is a thin wrapper: lock the library, call the existing
//! `smriti` service or repo, transform via DTO. Anything more complex
//! than ~15 lines is a service-layer change, not a handler change.

pub mod albums;
pub mod bursts;
pub mod documents;
pub mod duplicates;
pub mod geocoding;
pub mod health;
pub mod insights;
pub mod library;
pub mod map;
pub mod memories;
pub mod people;
pub mod photos;
pub mod search;
pub mod settings;
pub mod stacks;
pub mod system;
pub mod trash;

use crate::pagination::Cursor;

pub(crate) fn cursor_for_lite(p: &smriti::db::photo_repo::PhotoLite) -> Cursor {
    Cursor {
        date_taken: p.date_taken,
        id: p.id,
    }
}
