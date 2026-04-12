//! OCR processor scaffolding (Stage 2 deep analysis architecture).
//!
//! Stage 1 uses heuristic-only document detection. This service provides
//! progress/cancellation architecture so OCR can be plugged in with ONNX models.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::Database;
use crate::db::DocumentRepo;
use crate::services::image_utils::apply_exif_orientation;
use crate::services::DocumentDetector;

#[derive(Debug, Clone)]
pub struct OcrProgress {
    pub processed: usize,
    pub total: usize,
    pub documents_found: usize,
}

pub struct OcrProcessor;

impl OcrProcessor {
    fn lightweight_text_hints(image: &image::DynamicImage) -> Option<String> {
        let gray = image
            .resize(720, 720, image::imageops::FilterType::Triangle)
            .to_luma8();
        let (w, h) = gray.dimensions();
        if w == 0 || h == 0 {
            return None;
        }

        let mut bright = 0u64;
        let mut dark = 0u64;
        let mut transitions = 0u64;

        for y in 0..h {
            let mut prev = gray.get_pixel(0, y)[0];
            for x in 0..w {
                let v = gray.get_pixel(x, y)[0];
                if v > 220 {
                    bright += 1;
                }
                if v < 80 {
                    dark += 1;
                }
                if (v as i16 - prev as i16).abs() > 24 {
                    transitions += 1;
                }
                prev = v;
            }
        }

        let total = (w as u64) * (h as u64);
        if total == 0 {
            return None;
        }

        let bright_ratio = bright as f32 / total as f32;
        let dark_ratio = dark as f32 / total as f32;
        let trans_ratio = transitions as f32 / total as f32;

        let mut hints = Vec::new();
        if bright_ratio > 0.55 && dark_ratio > 0.04 && trans_ratio > 0.20 {
            hints.push("email");
            hints.push("phone");
        }
        if trans_ratio > 0.32 {
            hints.extend(["-", "-", "-", "-"]);
        }

        if hints.is_empty() {
            None
        } else {
            Some(hints.join("\n"))
        }
    }

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

        let mut documents_found = 0usize;
        for (idx, (photo_id, rel_path, orientation)) in targets.iter().enumerate() {
            if let Some(ref flag) = cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    return Ok(documents_found);
                }
            }

            let full_path = drive_path.join(rel_path);
            let (category, ocr_text, ocr_confidence) = match image::open(&full_path) {
                Ok(img) => {
                    let img = apply_exif_orientation(img, *orientation);
                    let rough = DocumentDetector::classify(&img, rel_path);
                    let text_hint = if rough != crate::models::ContentCategory::Photo {
                        Self::lightweight_text_hints(&img)
                    } else {
                        None
                    };
                    let final_cat = DocumentDetector::classify_with_text_hints(
                        &img,
                        rel_path,
                        text_hint.as_deref(),
                    );
                    let conf = text_hint.as_ref().map(|_| 0.55f32);
                    (final_cat, text_hint, conf)
                }
                Err(_) => (crate::models::ContentCategory::Photo, None, None),
            };

            if category != crate::models::ContentCategory::Photo {
                documents_found += 1;
            }

            let _ = repo.update_content_category(*photo_id, category.as_str());
            let _ = repo.update_ocr_metadata(*photo_id, ocr_text.as_deref(), ocr_confidence, true);

            if let Some(ref tx) = progress_tx {
                let _ = tx.try_send(OcrProgress {
                    processed: idx + 1,
                    total,
                    documents_found,
                });
            }
        }

        Ok(documents_found)
    }

    pub fn process_stage2_ocr(_drive_path: &std::path::Path) -> Result<usize, String> {
        // Placeholder for deep analysis:
        // 1. Load OCR ONNX models
        // 2. Read candidate document photos
        // 3. Extract text + confidence
        // 4. Store text in photos.ocr_text and index with FTS5 triggers
        Ok(0)
    }
}
