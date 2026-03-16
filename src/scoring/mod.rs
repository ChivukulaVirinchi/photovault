//! Image quality scoring for best-pick selection

pub mod blur;
pub mod sharpness;

pub use blur::BlurDetector;
pub use sharpness::SharpnessScorer;

use image::DynamicImage;

/// Combined quality score for a photo
#[derive(Debug, Clone, Default)]
pub struct QualityScore {
    /// Sharpness score (0-1, higher is sharper)
    pub sharpness: f32,

    /// Blur score (0-1, higher means less blur)
    pub blur: f32,

    /// Face count (normalized)
    pub face_count: f32,

    /// Average face detection confidence
    pub face_confidence: f32,

    /// Combined weighted score
    pub combined: f32,
}

impl QualityScore {
    /// Calculate combined score from components
    pub fn calculate_combined(&mut self) {
        // Weights for each component
        const SHARPNESS_WEIGHT: f32 = 0.4;
        const BLUR_WEIGHT: f32 = 0.3;
        const FACE_CONFIDENCE_WEIGHT: f32 = 0.2;
        const FACE_COUNT_WEIGHT: f32 = 0.1;

        self.combined = (self.sharpness * SHARPNESS_WEIGHT)
            + (self.blur * BLUR_WEIGHT)
            + (self.face_confidence * FACE_CONFIDENCE_WEIGHT)
            + (self.face_count * FACE_COUNT_WEIGHT);
    }
}

/// Score a single image for quality
pub fn score_image(image: &DynamicImage) -> QualityScore {
    let sharpness = SharpnessScorer::score(image);
    let blur = BlurDetector::score(image);

    let mut score = QualityScore {
        sharpness,
        blur,
        face_count: 0.0,     // Will be set from database
        face_confidence: 0.0, // Will be set from database
        combined: 0.0,
    };

    score.calculate_combined();
    score
}
