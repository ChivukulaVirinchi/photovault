//! Documents view and OCR handlers.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use iced::Task;

use crate::models::{ContentCategory, Photo};
use crate::services::{OcrProcessor, OcrProgress};

use super::super::messages::Message;
use super::super::state::PhotoVault;

pub(crate) fn load_documents(app: &mut PhotoVault) -> Task<Message> {
    app.load_documents()
}

pub(crate) fn documents_loaded(app: &mut PhotoVault, items: Vec<Photo>) -> Task<Message> {
    app.documents = items;
    let valid_ids: HashSet<i64> = app.documents.iter().map(|p| p.id).collect();
    app.selected_timeline_photo_ids
        .retain(|photo_id| valid_ids.contains(photo_id));
    Task::none()
}

pub(crate) fn documents_search_changed(app: &mut PhotoVault, input: String) -> Task<Message> {
    app.documents_query = input;
    app.load_documents()
}

pub(crate) fn documents_filter_category(
    app: &mut PhotoVault,
    category: Option<String>,
) -> Task<Message> {
    app.documents_filter = category.as_deref().map(ContentCategory::from_db);
    app.load_documents()
}

pub(crate) fn run_document_analysis(app: &mut PhotoVault) -> Task<Message> {
    if app.document_analysis_active {
        return Task::none();
    }
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    app.document_analysis_active = true;
    app.ocr_progress = Some(OcrProgress {
        processed: 0,
        total: 0,
        documents_found: 0,
    });

    let drive_path = drive_path.clone();
    let (progress_tx, progress_rx) = async_channel::bounded(32);
    app.ocr_progress_receiver = Some(progress_rx);

    let cancel_flag = Arc::new(AtomicBool::new(false));
    app.ocr_cancel_flag = Some(cancel_flag.clone());

    Task::perform(
        async move {
            let handle = tokio::task::spawn_blocking(move || {
                OcrProcessor::process_stage1_heuristics(
                    &drive_path,
                    Some(progress_tx),
                    Some(cancel_flag),
                )
            });

            match handle.await {
                Ok(result) => result,
                Err(e) => Err(format!("Document analysis task panicked: {}", e)),
            }
        },
        Message::DocumentAnalysisComplete,
    )
}

pub(crate) fn document_analysis_complete(
    app: &mut PhotoVault,
    result: Result<usize, String>,
) -> Task<Message> {
    app.document_analysis_active = false;
    app.ocr_progress_receiver = None;
    app.ocr_cancel_flag = None;

    match result {
        Ok(found) => {
            tracing::info!("Document analysis complete: {} docs found", found);
        }
        Err(e) => {
            tracing::error!("Document analysis failed: {}", e);
        }
    }

    let reload_docs = app.load_documents();
    let reload_photos = app.load_photos();
    Task::batch([reload_docs, reload_photos])
}
