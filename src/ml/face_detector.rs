//! SCRFD Face Detection
//!
//! Detects faces in images and returns bounding boxes with landmarks.
//! Uses the SCRFD-10GF model via ONNX Runtime.
//!
//! SCRFD outputs 9 tensors (3 strides x 3 types):
//!   Stride 8:  scores[12800,1], bboxes[12800,4], landmarks[12800,10]
//!   Stride 16: scores[3200,1],  bboxes[3200,4],  landmarks[3200,10]
//!   Stride 32: scores[800,1],   bboxes[800,4],   landmarks[800,10]
//!
//! Bboxes are anchor-distance format (left, top, right, bottom distances from anchor center).
//! Landmarks are (dx, dy) offsets from anchor center for 5 keypoints.

use std::path::Path;

use image::{DynamicImage, RgbImage};
#[allow(unused_imports)]
use image::GenericImageView; // needed by `DynamicImage::dimensions()` in some configurations
use ort::session::Session;
use ort::value::TensorRef;

use super::OnnxRuntime;

/// A detected face with bounding box and landmarks
#[derive(Debug, Clone)]
pub struct DetectedFace {
    /// Bounding box (x, y, width, height) in pixel coordinates
    pub bbox: (f32, f32, f32, f32),

    /// Normalized bounding box (0-1 range)
    pub bbox_normalized: (f32, f32, f32, f32),

    /// Detection confidence (0-1)
    pub confidence: f32,

    /// 5-point landmarks: left_eye, right_eye, nose, left_mouth, right_mouth
    /// Each point is (x, y) in pixel coordinates
    pub landmarks: [(f32, f32); 5],

    /// Cropped and aligned face image (112x112)
    pub aligned_face: Option<RgbImage>,
}

/// SCRFD Face Detector
pub struct FaceDetector {
    session: Session,
    input_size: (u32, u32),
    confidence_threshold: f32,
    nms_threshold: f32,
}

/// Per-stride output from the model
struct StrideOutput {
    scores: Vec<f32>,
    bboxes: Vec<f32>,
    landmarks: Vec<f32>,
    num_anchors: usize,
    stride: u32,
}

impl FaceDetector {
    /// Load the SCRFD model with a specific thread count per session.
    pub fn new_with_threads<P: AsRef<Path>>(
        runtime: &OnnxRuntime,
        model_path: P,
        intra_threads: usize,
    ) -> ort::Result<Self> {
        let session = runtime.load_model_with_threads(model_path, intra_threads)?;

        Ok(Self {
            session,
            input_size: (640, 640),
            confidence_threshold: 0.5,
            nms_threshold: 0.4,
        })
    }

    /// Set confidence threshold for detection
    pub fn with_confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    /// Detect faces in an image
    pub fn detect(&mut self, image: &DynamicImage) -> Vec<DetectedFace> {
        let (orig_width, orig_height) = image.dimensions();

        // Preprocess image to NCHW float tensor
        let input_data = self.preprocess(image);

        // Run inference
        let outputs = match self.run_inference(&input_data) {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("Face detection inference failed: {}", e);
                return Vec::new();
            }
        };

        // Post-process outputs
        let mut faces = self.postprocess(&outputs, orig_width, orig_height);

        // Apply NMS
        faces = self.non_max_suppression(faces);

        // Align faces for embedding
        for face in &mut faces {
            face.aligned_face = Some(self.align_face(image, &face.landmarks));
        }

        faces
    }

    /// Detect faces with adaptive multi-scale fallback for high-res images.
    ///
    /// Pass 1: normal full-frame detection.
    /// Pass 2 (fallback): overlapping high-res tiles if pass 1 found no faces
    /// and image is large, to recover small/profile faces lost by global resizing.
    pub fn detect_adaptive(&mut self, image: &DynamicImage) -> Vec<DetectedFace> {
        let first_pass = self.detect(image);
        if !first_pass.is_empty() {
            return first_pass;
        }

        let (orig_w, orig_h) = image.dimensions();
        if orig_w.max(orig_h) <= 2048 {
            return first_pass;
        }

        let tile = 1600u32;
        let step = 1200u32; // overlap for boundary faces
        let mut all_faces = Vec::new();

        let max_x = orig_w.saturating_sub(1);
        let max_y = orig_h.saturating_sub(1);

        let mut y = 0u32;
        while y <= max_y {
            let mut x = 0u32;
            while x <= max_x {
                let crop_w = tile.min(orig_w.saturating_sub(x)).max(1);
                let crop_h = tile.min(orig_h.saturating_sub(y)).max(1);
                let crop = image.crop_imm(x, y, crop_w, crop_h);

                let mut local = self.detect(&crop);
                if !local.is_empty() {
                    for face in &mut local {
                        let (bx, by, bw, bh) = face.bbox;
                        let gx = bx + x as f32;
                        let gy = by + y as f32;
                        face.bbox = (gx, gy, bw, bh);
                        face.bbox_normalized = (
                            gx / orig_w as f32,
                            gy / orig_h as f32,
                            bw / orig_w as f32,
                            bh / orig_h as f32,
                        );

                        for lm in &mut face.landmarks {
                            lm.0 += x as f32;
                            lm.1 += y as f32;
                        }
                    }
                    all_faces.extend(local);
                }

                if x + step >= orig_w {
                    break;
                }
                x += step;
            }

            if y + step >= orig_h {
                break;
            }
            y += step;
        }

        if all_faces.is_empty() {
            return all_faces;
        }

        self.non_max_suppression(all_faces)
    }

    /// Preprocess image for SCRFD: resize to 640x640, normalize, produce NCHW vec
    fn preprocess(&self, image: &DynamicImage) -> Vec<f32> {
        let (target_w, target_h) = self.input_size;

        // Resize to target dimensions
        let resized = image.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle);

        let rgb = resized.to_rgb8();

        // Convert to NCHW float and normalize (mean=127.5, std=128)
        let mut input = vec![0.0f32; (3 * target_h * target_w) as usize];
        let hw = (target_h * target_w) as usize;

        for y in 0..target_h {
            for x in 0..target_w {
                let pixel = rgb.get_pixel(x, y);
                let idx = (y * target_w + x) as usize;
                input[idx] = (pixel[0] as f32 - 127.5) / 128.0; // R channel
                input[hw + idx] = (pixel[1] as f32 - 127.5) / 128.0; // G channel
                input[2 * hw + idx] = (pixel[2] as f32 - 127.5) / 128.0; // B channel
            }
        }

        input
    }

    /// Run ONNX inference and extract output tensors with their shapes
    fn run_inference(&mut self, input_data: &[f32]) -> ort::Result<Vec<(Vec<i64>, Vec<f32>)>> {
        let (target_w, target_h) = self.input_size;

        // Create input tensor (shape [1, 3, 640, 640])
        let input_tensor = TensorRef::<f32>::from_array_view((
            vec![1i64, 3, target_h as i64, target_w as i64],
            input_data,
        ))?;

        let outputs = self.session.run(ort::inputs![input_tensor])?;

        // Extract all output tensors with shapes
        let mut results = Vec::new();
        for (_name, value) in outputs.iter() {
            let (shape, data) = value.try_extract_tensor::<f32>()?;
            results.push((shape.to_vec(), data.to_vec()));
        }

        Ok(results)
    }

    /// Classify and group the 9 output tensors into 3 stride groups.
    ///
    /// SCRFD outputs 9 tensors. We classify them by their last dimension:
    ///   - dim=1 → scores
    ///   - dim=4 → bounding boxes (anchor-distance format)
    ///   - dim=10 → landmarks (5 keypoints * 2 coords)
    ///
    /// Within each type, they are ordered by descending anchor count (stride 8, 16, 32).
    fn group_outputs(&self, outputs: &[(Vec<i64>, Vec<f32>)]) -> Vec<StrideOutput> {
        let strides = [8u32, 16, 32];

        let mut score_tensors: Vec<(usize, &Vec<f32>)> = Vec::new();
        let mut bbox_tensors: Vec<(usize, &Vec<f32>)> = Vec::new();
        let mut lmk_tensors: Vec<(usize, &Vec<f32>)> = Vec::new();

        for (shape, data) in outputs {
            let last_dim = *shape.last().unwrap_or(&0);
            let num_anchors = if shape.len() >= 2 {
                shape[shape.len() - 2] as usize
            } else {
                data.len() / last_dim as usize
            };

            match last_dim {
                1 => score_tensors.push((num_anchors, data)),
                4 => bbox_tensors.push((num_anchors, data)),
                10 => lmk_tensors.push((num_anchors, data)),
                _ => {
                    tracing::debug!("Unknown output tensor shape: {:?}", shape);
                }
            }
        }

        // Sort each group by descending anchor count (stride 8 has most anchors)
        score_tensors.sort_by(|a, b| b.0.cmp(&a.0));
        bbox_tensors.sort_by(|a, b| b.0.cmp(&a.0));
        lmk_tensors.sort_by(|a, b| b.0.cmp(&a.0));

        let mut stride_outputs = Vec::new();

        for i in 0..strides
            .len()
            .min(score_tensors.len())
            .min(bbox_tensors.len())
        {
            let num_anchors = score_tensors[i].0;
            let has_landmarks = i < lmk_tensors.len();

            stride_outputs.push(StrideOutput {
                scores: score_tensors[i].1.clone(),
                bboxes: bbox_tensors[i].1.clone(),
                landmarks: if has_landmarks {
                    lmk_tensors[i].1.clone()
                } else {
                    vec![0.0; num_anchors * 10]
                },
                num_anchors,
                stride: strides[i],
            });
        }

        stride_outputs
    }

    /// Post-process SCRFD outputs to DetectedFace structs
    ///
    /// SCRFD uses anchor-based detection across 3 stride levels.
    /// Each anchor center is at (col * stride + stride/2, row * stride + stride/2).
    /// BBox outputs are distances from anchor center: (left, top, right, bottom).
    /// Landmark outputs are (dx, dy) offsets from anchor center for each keypoint.
    fn postprocess(
        &self,
        outputs: &[(Vec<i64>, Vec<f32>)],
        orig_width: u32,
        orig_height: u32,
    ) -> Vec<DetectedFace> {
        let mut faces = Vec::new();

        if outputs.len() < 6 {
            tracing::warn!(
                "SCRFD: Expected at least 6 output tensors, got {}",
                outputs.len()
            );
            return faces;
        }

        let stride_outputs = self.group_outputs(outputs);

        let (input_w, input_h) = self.input_size;
        let scale_x = orig_width as f32 / input_w as f32;
        let scale_y = orig_height as f32 / input_h as f32;

        for so in &stride_outputs {
            let stride = so.stride as f32;
            let grid_w = input_w as f32 / stride;
            let grid_h = input_h as f32 / stride;
            let grid_cols = grid_w as usize;
            let grid_rows = grid_h as usize;
            let anchors_per_cell = so.num_anchors / (grid_cols * grid_rows).max(1);

            for row in 0..grid_rows {
                for col in 0..grid_cols {
                    for a in 0..anchors_per_cell {
                        let idx = (row * grid_cols + col) * anchors_per_cell + a;
                        if idx >= so.num_anchors {
                            continue;
                        }

                        // Score — SCRFD already applies sigmoid internally,
                        // so raw output is in [0, 1] range.
                        let score = so.scores[idx];

                        if score < self.confidence_threshold {
                            continue;
                        }

                        // Anchor center in input image coords
                        let cx = (col as f32 + 0.5) * stride;
                        let cy = (row as f32 + 0.5) * stride;

                        // Decode bounding box (distances from anchor center)
                        let bbox_base = idx * 4;
                        if bbox_base + 3 >= so.bboxes.len() {
                            continue;
                        }
                        let dl = so.bboxes[bbox_base] * stride;
                        let dt = so.bboxes[bbox_base + 1] * stride;
                        let dr = so.bboxes[bbox_base + 2] * stride;
                        let db = so.bboxes[bbox_base + 3] * stride;

                        let x1 = (cx - dl) * scale_x;
                        let y1 = (cy - dt) * scale_y;
                        let x2 = (cx + dr) * scale_x;
                        let y2 = (cy + db) * scale_y;

                        // Clamp to image bounds
                        let x1 = x1.max(0.0).min(orig_width as f32);
                        let y1 = y1.max(0.0).min(orig_height as f32);
                        let x2 = x2.max(0.0).min(orig_width as f32);
                        let y2 = y2.max(0.0).min(orig_height as f32);

                        let width = x2 - x1;
                        let height = y2 - y1;

                        if width < 10.0 || height < 10.0 {
                            continue;
                        }

                        // Decode landmarks
                        let lm_base = idx * 10;
                        let mut lmks = [(0.0f32, 0.0f32); 5];
                        if lm_base + 9 < so.landmarks.len() {
                            for j in 0..5 {
                                lmks[j] = (
                                    (cx + so.landmarks[lm_base + j * 2] * stride) * scale_x,
                                    (cy + so.landmarks[lm_base + j * 2 + 1] * stride) * scale_y,
                                );
                            }
                        }

                        faces.push(DetectedFace {
                            bbox: (x1, y1, width, height),
                            bbox_normalized: (
                                x1 / orig_width as f32,
                                y1 / orig_height as f32,
                                width / orig_width as f32,
                                height / orig_height as f32,
                            ),
                            confidence: score,
                            landmarks: lmks,
                            aligned_face: None,
                        });
                    }
                }
            }
        }

        tracing::debug!(
            "SCRFD raw detections: {} (before NMS, threshold={})",
            faces.len(),
            self.confidence_threshold
        );

        faces
    }

    /// Non-maximum suppression to remove overlapping detections
    fn non_max_suppression(&self, mut faces: Vec<DetectedFace>) -> Vec<DetectedFace> {
        if faces.is_empty() {
            return faces;
        }

        // Sort by confidence (descending)
        faces.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut keep = vec![true; faces.len()];

        for i in 0..faces.len() {
            if !keep[i] {
                continue;
            }

            for j in (i + 1)..faces.len() {
                if !keep[j] {
                    continue;
                }

                let iou = self.calculate_iou(&faces[i].bbox, &faces[j].bbox);
                if iou > self.nms_threshold {
                    keep[j] = false;
                }
            }
        }

        faces
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep[*i])
            .map(|(_, f)| f)
            .collect()
    }

    /// Calculate Intersection over Union for two bounding boxes
    /// Each box is (x, y, width, height)
    fn calculate_iou(&self, box1: &(f32, f32, f32, f32), box2: &(f32, f32, f32, f32)) -> f32 {
        let (x1, y1, w1, h1) = *box1;
        let (x2, y2, w2, h2) = *box2;

        let xi1 = x1.max(x2);
        let yi1 = y1.max(y2);
        let xi2 = (x1 + w1).min(x2 + w2);
        let yi2 = (y1 + h1).min(y2 + h2);

        let inter_width = (xi2 - xi1).max(0.0);
        let inter_height = (yi2 - yi1).max(0.0);
        let inter_area = inter_width * inter_height;

        let area1 = w1 * h1;
        let area2 = w2 * h2;
        let union_area = area1 + area2 - inter_area;

        if union_area > 0.0 {
            inter_area / union_area
        } else {
            0.0
        }
    }

    /// Align face using all 5 landmarks via similarity transform onto the
    /// canonical InsightFace 112x112 template. Falls back to eye-center crop
    /// if the landmarks are degenerate (rare).
    fn align_face(&self, image: &DynamicImage, landmarks: &[(f32, f32); 5]) -> RgbImage {
        if let Some(aligned) = super::alignment::align_face_112(image, landmarks) {
            return aligned;
        }

        // Degenerate landmarks: fall back to the old eye-center crop.
        let left_eye = landmarks[0];
        let right_eye = landmarks[1];
        let eye_center = (
            (left_eye.0 + right_eye.0) / 2.0,
            (left_eye.1 + right_eye.1) / 2.0,
        );
        let eye_dist =
            ((right_eye.0 - left_eye.0).powi(2) + (right_eye.1 - left_eye.1).powi(2)).sqrt();
        let face_size = (eye_dist * 2.5).max(10.0);

        let img_w = image.width();
        let img_h = image.height();
        let x = (eye_center.0 - face_size / 2.0).max(0.0) as u32;
        let y = (eye_center.1 - face_size / 2.0).max(0.0) as u32;
        let x = x.min(img_w.saturating_sub(1));
        let y = y.min(img_h.saturating_sub(1));
        let crop_w = (face_size as u32).min(img_w.saturating_sub(x)).max(1);
        let crop_h = (face_size as u32).min(img_h.saturating_sub(y)).max(1);

        let cropped = image.crop_imm(x, y, crop_w, crop_h);
        let resized = cropped.resize_exact(112, 112, image::imageops::FilterType::Lanczos3);
        resized.to_rgb8()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a FaceDetector-like struct for testing IoU
    struct IouTester;

    impl IouTester {
        fn calculate_iou(box1: &(f32, f32, f32, f32), box2: &(f32, f32, f32, f32)) -> f32 {
            let (x1, y1, w1, h1) = *box1;
            let (x2, y2, w2, h2) = *box2;

            let xi1 = x1.max(x2);
            let yi1 = y1.max(y2);
            let xi2 = (x1 + w1).min(x2 + w2);
            let yi2 = (y1 + h1).min(y2 + h2);

            let inter_width = (xi2 - xi1).max(0.0);
            let inter_height = (yi2 - yi1).max(0.0);
            let inter_area = inter_width * inter_height;

            let area1 = w1 * h1;
            let area2 = w2 * h2;
            let union_area = area1 + area2 - inter_area;

            if union_area > 0.0 {
                inter_area / union_area
            } else {
                0.0
            }
        }
    }

    #[test]
    fn test_iou_same_box() {
        let box1 = (0.0, 0.0, 100.0, 100.0);
        assert!((IouTester::calculate_iou(&box1, &box1) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_iou_non_overlapping() {
        let box1 = (0.0, 0.0, 100.0, 100.0);
        let box2 = (200.0, 200.0, 100.0, 100.0);
        assert!((IouTester::calculate_iou(&box1, &box2) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_iou_partial_overlap() {
        let box1 = (0.0, 0.0, 100.0, 100.0);
        let box2 = (50.0, 50.0, 100.0, 100.0);
        let iou = IouTester::calculate_iou(&box1, &box2);
        // Intersection: 50x50 = 2500, Union: 10000 + 10000 - 2500 = 17500
        assert!((iou - 2500.0 / 17500.0).abs() < 0.001);
    }
}
