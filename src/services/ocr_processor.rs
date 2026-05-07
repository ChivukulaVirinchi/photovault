//! OCR processor scaffolding (Stage 2 deep analysis architecture).
//!
//! Stage 1 uses heuristic-only document detection. This service provides
//! progress/cancellation architecture so OCR can be plugged in with ONNX models.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::Database;
use crate::db::DocumentRepo;

#[derive(Debug, Clone)]
pub struct OcrProgress {
    pub processed: usize,
    pub total: usize,
    pub documents_found: usize,
}

pub struct OcrProcessor;

impl OcrProcessor {
    /// Mark every unprocessed photo as `Photo` and tick the
    /// content-categorisation flag. The earlier heuristic classifier
    /// (edge density + aspect ratio + filename keywords) produced too
    /// many false positives and was retired; this scaffolding stays so
    /// a learned classifier can be plugged in later without re-piping
    /// the progress / cancellation channels.
    pub fn process_stage1_heuristics(
        drive_path: &std::path::Path,
        progress_tx: Option<async_channel::Sender<OcrProgress>>,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Result<usize, String> {
        let db = Database::open_for_drive(drive_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        let repo = DocumentRepo::new(&db.conn);

        let targets = repo
            .get_unprocessed_for_document_analysis(100000)
            .map_err(|e| format!("Failed to load analysis targets: {}", e))?;

        let total = targets.len();
        if total == 0 {
            return Ok(0);
        }

        for (idx, (photo_id, _rel_path, _orientation)) in targets.iter().enumerate() {
            if let Some(ref flag) = cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    return Ok(0);
                }
            }

            let _ = repo
                .update_content_category(*photo_id, crate::models::ContentCategory::Photo.as_str());
            let _ = repo.update_ocr_metadata(*photo_id, None, None, true);

            if let Some(ref tx) = progress_tx {
                let _ = tx.try_send(OcrProgress {
                    processed: idx + 1,
                    total,
                    documents_found: 0,
                });
            }
        }

        Ok(0)
    }
}
