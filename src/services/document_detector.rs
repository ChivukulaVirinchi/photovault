//! Heuristic document detector (Stage 1 fast filter).
//!
//! Temporarily disabled - call sites in ocr_processor.rs are commented out.
//! Kept compilable so we can revisit with a learned classifier later.
#![allow(dead_code)]

use std::path::Path;

use image::{DynamicImage, GenericImageView};

use crate::models::ContentCategory;

pub struct DocumentDetector;

impl DocumentDetector {
    pub fn classify(image: &DynamicImage, file_path: &str) -> ContentCategory {
        let file_name = Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_lowercase();

        if file_name.contains("screenshot")
            || file_name.starts_with("img_") && file_name.contains("wa")
        {
            return ContentCategory::Screenshot;
        }
        if file_name.contains("receipt")
            || file_name.contains("invoice")
            || file_name.contains("bill")
        {
            return ContentCategory::Receipt;
        }
        if file_name.contains("business card") || file_name.contains("bizcard") {
            return ContentCategory::BusinessCard;
        }
        if file_name.contains("slide") || file_name.contains("ppt") || file_name.contains("deck") {
            return ContentCategory::Presentation;
        }
        if file_name.contains("whiteboard") {
            return ContentCategory::Whiteboard;
        }

        let (w, h) = image.dimensions();
        if w == 0 || h == 0 {
            return ContentCategory::Photo;
        }

        let aspect = w as f32 / h as f32;
        let resized = image
            .resize(512, 512, image::imageops::FilterType::Triangle)
            .to_luma8();

        let edge_density = Self::edge_density(&resized);
        let contrast = Self::contrast_spread(&resized);

        // Screenshot-like: precise edges + wide/phone aspect.
        let looks_like_screen = edge_density > 0.20 && !(0.72..=1.45).contains(&aspect);
        if looks_like_screen {
            return ContentCategory::Screenshot;
        }

        // Presentation-like: 16:9-ish + high edge structure.
        if (aspect - 16.0 / 9.0).abs() < 0.20 && edge_density > 0.16 {
            return ContentCategory::Presentation;
        }

        // Card-like: compact aspect and high-contrast text blocks.
        if edge_density > 0.15 && contrast > 0.20 && (0.45..=2.2).contains(&aspect) {
            return ContentCategory::BusinessCard;
        }

        // Document-like: lots of text edges and moderate/high contrast.
        if edge_density > 0.14 && contrast > 0.18 {
            return ContentCategory::Document;
        }

        ContentCategory::Photo
    }

    pub fn classify_with_text_hints(
        image: &DynamicImage,
        file_path: &str,
        ocr_text: Option<&str>,
    ) -> ContentCategory {
        let mut category = Self::classify(image, file_path);

        let text = ocr_text.unwrap_or_default().to_lowercase();
        if !text.is_empty() {
            let has_phone =
                text.contains("phone") || text.contains("mobile") || text.contains("tel");
            let has_email = text.contains('@') || text.contains("email");
            if has_phone && has_email {
                return ContentCategory::BusinessCard;
            }

            let bullet_markers = text.matches("\n-").count()
                + text.matches("\n*").count()
                + text.matches("\n•").count();
            if bullet_markers >= 4 {
                category = ContentCategory::Presentation;
            }
        }

        category
    }

    fn edge_density(gray: &image::GrayImage) -> f32 {
        let (w, h) = gray.dimensions();
        if w < 3 || h < 3 {
            return 0.0;
        }

        let mut edge_count = 0u64;
        let mut total = 0u64;

        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                let l = gray.get_pixel(x - 1, y)[0] as i32;
                let r = gray.get_pixel(x + 1, y)[0] as i32;
                let u = gray.get_pixel(x, y - 1)[0] as i32;
                let d = gray.get_pixel(x, y + 1)[0] as i32;
                let grad = (r - l).abs() + (d - u).abs();
                if grad > 48 {
                    edge_count += 1;
                }
                total += 1;
            }
        }

        if total == 0 {
            0.0
        } else {
            edge_count as f32 / total as f32
        }
    }

    fn contrast_spread(gray: &image::GrayImage) -> f32 {
        let mut min_v = 255u8;
        let mut max_v = 0u8;
        for p in gray.pixels() {
            let v = p[0];
            if v < min_v {
                min_v = v;
            }
            if v > max_v {
                max_v = v;
            }
        }
        (max_v.saturating_sub(min_v) as f32) / 255.0
    }
}
