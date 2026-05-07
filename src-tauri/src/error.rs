//! Error type for IPC commands.
//!
//! `CommandError` is the only error shape that crosses the Tauri boundary.
//! Internal errors from the `photovault` library are mapped here once via
//! `From` impls; handler bodies stay free of error chain massaging.

use serde::Serialize;
use thiserror::Error;

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, Serialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandError {
    #[error("not found: {entity} #{id}")]
    NotFound { entity: String, id: String },

    #[error("validation: {field}: {reason}")]
    Validation { field: String, reason: String },

    #[error("library not opened")]
    LibraryClosed,

    #[error("drive not mounted: {path}")]
    DriveNotMounted { path: String },

    #[error("ml unavailable: {reason}")]
    MlUnavailable { reason: String },

    #[error("conflict: {reason}")]
    Conflict { reason: String },

    #[error("operation cancelled")]
    Cancelled,

    #[error("database error: {message}")]
    Database { message: String },

    #[error("io error: {message}")]
    Io { message: String },

    #[error("network error: {message}")]
    Network { message: String },

    #[error("internal error: {message}")]
    Internal { message: String },
}

impl CommandError {
    pub fn not_found(entity: impl Into<String>, id: impl ToString) -> Self {
        Self::NotFound {
            entity: entity.into(),
            id: id.to_string(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal {
            message: msg.into(),
        }
    }
}

impl From<smriti::error::AppError> for CommandError {
    fn from(e: smriti::error::AppError) -> Self {
        use smriti::error::AppError;
        match e {
            AppError::Database(err) => Self::Database {
                message: err.to_string(),
            },
            AppError::Io(err) => Self::Io {
                message: err.to_string(),
            },
            AppError::Image(err) => Self::Internal {
                message: format!("image: {err}"),
            },
            AppError::Exif(s) => Self::Internal {
                message: format!("exif: {s}"),
            },
            AppError::Ml(s) => Self::MlUnavailable { reason: s },
            AppError::FileNotFound(p) => Self::NotFound {
                entity: "file".into(),
                id: p.display().to_string(),
            },
            AppError::DriveNotMounted(p) => Self::DriveNotMounted {
                path: p.display().to_string(),
            },
            AppError::Config(s) => Self::Internal {
                message: format!("config: {s}"),
            },
            AppError::Cancelled => Self::Cancelled,
            AppError::Other(s) => Self::Internal { message: s },
        }
    }
}

impl From<rusqlite::Error> for CommandError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database {
            message: e.to_string(),
        }
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            message: e.to_string(),
        }
    }
}

impl From<smriti::db::connection::DatabaseError> for CommandError {
    fn from(e: smriti::db::connection::DatabaseError) -> Self {
        use smriti::db::connection::DatabaseError;
        match e {
            DatabaseError::OpenError(err) => Self::Database {
                message: err.to_string(),
            },
            DatabaseError::PathNotFound(p) => Self::DriveNotMounted {
                path: p.display().to_string(),
            },
            DatabaseError::DirectoryCreationError(err) => Self::Io {
                message: err.to_string(),
            },
        }
    }
}
