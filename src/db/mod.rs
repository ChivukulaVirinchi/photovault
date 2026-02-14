//! Database module for PhotoVault
//! 
//! Uses SQLite with the database stored on the indexed drive itself,
//! making the index fully portable.

pub mod connection;
pub mod schema;
pub mod migrations;

pub use connection::Database;
pub use schema::create_schema;
