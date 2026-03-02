//! Database module for PhotoVault
//!
//! Uses SQLite with the database stored on the indexed drive itself,
//! making the index fully portable.

pub mod connection;
pub mod migrations;
pub mod photo_repo;
pub mod schema;

pub use connection::Database;
pub use photo_repo::PhotoRepo;
pub use schema::create_schema;
