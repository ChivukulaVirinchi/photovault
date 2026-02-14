//! Main application state and logic

use std::path::PathBuf;
use iced::widget::{container, row};
use iced::{Element, Length, Task};

use crate::components::Sidebar;
use crate::services::{DriveDetector, DriveInfo};
use crate::views::{WelcomeView, TimelineView, PeopleView, SearchView, SettingsView};
use crate::theme::colors::Backgrounds;
use crate::db::Database;

/// Current view in the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Welcome,
    Timeline,
    People,
    Search,
    Settings,
}

/// Application state
pub struct PhotoVault {
    /// Current active view
    current_view: View,
    
    /// Detected drives
    drives: Vec<DriveInfo>,
    
    /// Currently selected drive path
    selected_drive: Option<PathBuf>,
    
    /// Database connection (if a drive is selected)
    database: Option<Database>,
}

/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Navigate to a different view
    NavigateTo(View),
    
    /// Select a drive to index
    SelectDrive(PathBuf),
    
    /// Open folder browser dialog
    BrowseForFolder,
    
    /// Folder selected from browser
    FolderSelected(Option<PathBuf>),
    
    /// Refresh drive list
    RefreshDrives,
    
    /// Drives detected
    DrivesDetected(Vec<DriveInfo>),
}

impl PhotoVault {
    /// Create new application instance
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            current_view: View::Welcome,
            drives: Vec::new(),
            selected_drive: None,
            database: None,
        };
        
        // Detect drives on startup
        let task = Task::perform(
            async { DriveDetector::detect() },
            Message::DrivesDetected,
        );
        
        (app, task)
    }
    
    /// Application title
    pub fn title(&self) -> String {
        match &self.selected_drive {
            Some(path) => format!("PhotoVault - {}", path.display()),
            None => "PhotoVault".to_string(),
        }
    }
    
    /// Handle messages
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NavigateTo(view) => {
                self.current_view = view;
                Task::none()
            }
            
            Message::SelectDrive(path) => {
                tracing::info!("Selected drive: {:?}", path);
                
                match Database::open_for_drive(&path) {
                    Ok(db) => {
                        // Create schema if needed
                        if db.needs_schema().unwrap_or(true) {
                            if let Err(e) = crate::db::create_schema(&db.conn) {
                                tracing::error!("Failed to create schema: {}", e);
                            }
                        }
                        
                        self.database = Some(db);
                        self.selected_drive = Some(path);
                        self.current_view = View::Timeline;
                    }
                    Err(e) => {
                        tracing::error!("Failed to open database: {}", e);
                    }
                }
                
                Task::none()
            }
            
            Message::BrowseForFolder => {
                // TODO: Implement native folder picker
                tracing::info!("Browse for folder requested");
                Task::none()
            }
            
            Message::FolderSelected(path) => {
                if let Some(path) = path {
                    return self.update(Message::SelectDrive(path));
                }
                Task::none()
            }
            
            Message::RefreshDrives => {
                Task::perform(
                    async { DriveDetector::detect() },
                    Message::DrivesDetected,
                )
            }
            
            Message::DrivesDetected(drives) => {
                tracing::info!("Detected {} drives", drives.len());
                self.drives = drives;
                Task::none()
            }
        }
    }
    
    /// Render the application
    pub fn view(&self) -> Element<'_, Message> {
        // If no drive selected, show welcome screen
        if self.selected_drive.is_none() {
            return WelcomeView::view(&self.drives);
        }
        
        // Main layout: sidebar + content
        let sidebar = Sidebar::view(&self.current_view);
        
        let content = match self.current_view {
            View::Welcome => WelcomeView::view(&self.drives),
            View::Timeline => TimelineView::view(),
            View::People => PeopleView::view(),
            View::Search => SearchView::view(),
            View::Settings => SettingsView::view(),
        };
        
        let layout = row![
            sidebar,
            content,
        ];
        
        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }
}
