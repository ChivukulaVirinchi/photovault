//! Status bar rendering extracted from app/views.rs.

use iced::widget::{container, text};
use iced::{Element, Length};

use crate::app::{Message, PhotoVault};
use crate::theme::colors;

pub fn has_status(app: &PhotoVault) -> bool {
    app.scan_state.is_some()
        || app.face_processing_active
        || app.duplicate_detection_running
        || app.burst_detection_running
        || app.geocoding_progress.is_some()
        || app.document_analysis_active
        || app.suggestion_detection_running
        || app.insights_loading
        || app.search_loading
        || app.photos_loading
}

pub fn status_bar(app: &PhotoVault) -> Element<'_, Message> {
    let mut status_parts: Vec<String> = Vec::new();

    if let Some(ref state) = app.scan_state {
        let p = &state.progress;
        if p.is_complete {
            status_parts.push(format!("Scan complete: {} files", p.files_processed));
        } else {
            status_parts.push(format!(
                "Scanning: {}/{} files",
                p.files_processed, p.files_found
            ));
        }
    }

    if app.face_processing_active {
        if let Some(ref prog) = app.face_processing_progress {
            let eta = if prog.processed > 0 && prog.total > 0 && prog.elapsed_secs > 0.5 {
                let frac = prog.processed as f64 / prog.total as f64;
                if frac < 1.0 {
                    let rem = prog.elapsed_secs / frac * (1.0 - frac);
                    if rem > 60.0 {
                        format!(" ~{}m", (rem / 60.0).ceil() as u32)
                    } else {
                        format!(" ~{}s", rem.ceil() as u32)
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            status_parts.push(format!(
                "Faces: {}/{} ({} found){}",
                prog.processed, prog.total, prog.faces_found, eta
            ));
        } else {
            status_parts.push("Faces: initializing...".to_string());
        }
    }

    if app.duplicate_detection_running {
        status_parts.push("Detecting duplicates...".to_string());
    }
    if app.burst_detection_running {
        status_parts.push("Detecting bursts...".to_string());
    }
    if let Some((processed, total)) = app.geocoding_progress {
        if total > 0 {
            status_parts.push(format!("Geocoding: {}/{}", processed, total));
        }
    }
    if app.document_analysis_active {
        if let Some(ref prog) = app.ocr_progress {
            status_parts.push(format!(
                "Documents: {}/{} analyzed ({} docs)",
                prog.processed, prog.total, prog.documents_found
            ));
        } else {
            status_parts.push("Documents: analyzing...".to_string());
        }
    }
    if app.suggestion_detection_running {
        status_parts.push("Detecting album suggestions...".to_string());
    }
    if app.insights_loading {
        status_parts.push("Computing insights...".to_string());
    }
    if app.search_loading {
        status_parts.push("Searching...".to_string());
    }
    if app.photos_loading {
        status_parts.push("Loading photos...".to_string());
    }

    let p = colors::palette(app.config.theme);
    let status_text = status_parts.join("  |  ");
    container(text(status_text).size(11).color(p.text_secondary))
        .width(Length::Fill)
        .padding([4, 16])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(p.bg_secondary.into()),
            border: iced::Border {
                color: p.border_subtle,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
