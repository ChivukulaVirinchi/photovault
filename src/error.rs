//! Application error handling.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Application error types.
#[derive(Debug)]
pub enum AppError {
    Database(rusqlite::Error),
    Io(io::Error),
    Image(image::ImageError),
    Exif(String),
    Ml(String),
    FileNotFound(PathBuf),
    DriveNotMounted(PathBuf),
    Config(String),
    Cancelled,
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Database(e) => write!(f, "Database error: {}", e),
            AppError::Io(e) => write!(f, "I/O error: {}", e),
            AppError::Image(e) => write!(f, "Image error: {}", e),
            AppError::Exif(msg) => write!(f, "EXIF error: {}", msg),
            AppError::Ml(msg) => write!(f, "ML error: {}", msg),
            AppError::FileNotFound(path) => write!(f, "File not found: {}", path.display()),
            AppError::DriveNotMounted(path) => {
                write!(f, "Drive not mounted: {}", path.display())
            }
            AppError::Config(msg) => write!(f, "Configuration error: {}", msg),
            AppError::Cancelled => write!(f, "Operation cancelled"),
            AppError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e)
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<image::ImageError> for AppError {
    fn from(e: image::ImageError) -> Self {
        AppError::Image(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn recovery_suggestion(&self) -> &str {
        match self {
            AppError::Database(_) => {
                "Try restarting the app. If it persists, database may be corrupted."
            }
            AppError::Io(_) => "Check file permissions and available disk space.",
            AppError::Image(_) => "This image may be corrupted or unsupported.",
            AppError::Exif(_) => "Metadata could not be read. Photo can still be indexed.",
            AppError::Ml(_) => "Face detection failed. Try restarting the app.",
            AppError::FileNotFound(_) => "The file may have been moved or deleted.",
            AppError::DriveNotMounted(_) => "Reconnect the drive and try again.",
            AppError::Config(_) => "Reset settings to defaults and try again.",
            AppError::Cancelled => "",
            AppError::Other(_) => "Please try again.",
        }
    }

    pub fn should_notify_user(&self) -> bool {
        !matches!(self, AppError::Cancelled | AppError::Exif(_))
    }

    pub fn is_recoverable(&self) -> bool {
        !matches!(self, AppError::Database(_))
    }
}

#[derive(Debug, Clone)]
pub struct ErrorNotification {
    pub title: String,
    pub message: String,
    pub suggestion: String,
    pub is_critical: bool,
}

impl From<&AppError> for ErrorNotification {
    fn from(error: &AppError) -> Self {
        let title = match error {
            AppError::Database(_) => "Database Error",
            AppError::Io(_) => "File Error",
            AppError::Image(_) => "Image Error",
            AppError::Exif(_) => "Metadata Error",
            AppError::Ml(_) => "Face Detection Error",
            AppError::FileNotFound(_) => "File Not Found",
            AppError::DriveNotMounted(_) => "Drive Not Connected",
            AppError::Config(_) => "Settings Error",
            AppError::Cancelled => "Cancelled",
            AppError::Other(_) => "Error",
        };

        Self {
            title: title.to_string(),
            message: error.to_string(),
            suggestion: error.recovery_suggestion().to_string(),
            is_critical: !error.is_recoverable(),
        }
    }
}
