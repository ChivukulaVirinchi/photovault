# Phase 4: Face Detection & Clustering

## Overview

This phase implements the AI-powered face recognition pipeline: detecting faces in photos, generating embeddings, and clustering them into people. This is the core feature that elevates PhotoVault beyond a simple file browser.

**Estimated Time:** 5-7 days  
**Difficulty:** Advanced  
**Prerequisites:** Phase 3 complete

---

## UI Design Guidelines

> **IMPORTANT:** When implementing any UI components in this phase, you MUST read and follow the design principles in `SKILL.md`. This file contains critical guidelines for:
> - Typography and spacing standards
> - Color usage and contrast requirements
> - Animation and interaction patterns
> - Component design principles
> - Accessibility requirements
>
> **Before writing ANY UI code, read SKILL.md thoroughly.** The goal is a delightful, polished user experience - not just functional code.

---

## Goals

- [ ] Integrate ONNX Runtime for ML inference
- [ ] Implement SCRFD face detection
- [ ] Implement ArcFace embedding generation
- [ ] Build DBSCAN clustering for face grouping
- [ ] Create background processing pipeline
- [ ] Build People view with face clusters
- [ ] Enable face labeling (naming people)
- [ ] Implement cluster merging

---

## ML Models

### Face Detection: SCRFD

**Model:** SCRFD-10GF (Sample and Computation Redistribution for Face Detection)
- Input: 640×640 RGB image
- Output: Bounding boxes, confidence scores, 5-point landmarks
- Size: ~10MB
- Speed: ~20ms per image on CPU

**Download:** [InsightFace SCRFD Models](https://github.com/deepinsight/insightface/tree/master/detection/scrfd)

### Face Embedding: ArcFace

**Model:** ArcFace-R100 (Additive Angular Margin Loss)
- Input: 112×112 aligned face image
- Output: 512-dimensional embedding vector
- Size: ~250MB
- Speed: ~10ms per face on CPU

**Download:** [InsightFace Recognition Models](https://github.com/deepinsight/insightface/tree/master/recognition/arcface_torch)

---

## New Files

```
src/
├── ml/
│   ├── mod.rs              # ML module exports
│   ├── runtime.rs          # ONNX Runtime wrapper
│   ├── face_detector.rs    # SCRFD implementation
│   ├── face_embedder.rs    # ArcFace implementation
│   └── clustering.rs       # DBSCAN clustering
├── services/
│   └── face_processor.rs   # Face processing pipeline
├── db/
│   └── face_repo.rs        # Face database operations
└── views/
    └── people.rs           # Updated People view
```

---

## Step 1: Add Dependencies

Update `Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...

# ONNX Runtime for ML inference
ort = { version = "2.0", features = ["download-binaries"] }

# Linear algebra for embeddings
ndarray = "0.15"

# Clustering
linfa = "0.7"
linfa-clustering = "0.7"

# Image processing for face alignment
imageproc = "0.24"
rusttype = "0.9"  # For drawing face boxes (debug)
```

---

## Step 2: ONNX Runtime Wrapper

### File: `src/ml/mod.rs`

```rust
//! Machine Learning module
//!
//! Provides face detection and recognition using ONNX models.

pub mod runtime;
pub mod face_detector;
pub mod face_embedder;
pub mod clustering;

pub use runtime::OnnxRuntime;
pub use face_detector::{FaceDetector, DetectedFace};
pub use face_embedder::{FaceEmbedder, FaceEmbedding};
pub use clustering::FaceClusterer;
```

### File: `src/ml/runtime.rs`

```rust
//! ONNX Runtime initialization and management

use std::path::Path;
use std::sync::Arc;

use ort::{Environment, ExecutionProvider, Session, SessionBuilder};

/// ONNX Runtime environment wrapper
pub struct OnnxRuntime {
    environment: Arc<Environment>,
}

impl OnnxRuntime {
    /// Initialize the ONNX Runtime
    pub fn new() -> ort::Result<Self> {
        let environment = Environment::builder()
            .with_name("photovault")
            .with_execution_providers([
                // Try CUDA first, fall back to CPU
                ExecutionProvider::CUDA(Default::default()),
                ExecutionProvider::CPU(Default::default()),
            ])
            .build()?;

        tracing::info!("ONNX Runtime initialized");

        Ok(Self {
            environment: Arc::new(environment),
        })
    }

    /// Load a model from a file
    pub fn load_model<P: AsRef<Path>>(&self, path: P) -> ort::Result<Session> {
        let session = SessionBuilder::new(&self.environment)?
            .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .with_model_from_file(path)?;

        Ok(session)
    }

    /// Get the environment reference
    pub fn environment(&self) -> &Arc<Environment> {
        &self.environment
    }
}

impl Default for OnnxRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ONNX Runtime")
    }
}
```

---

## Step 3: Face Detector

### File: `src/ml/face_detector.rs`

```rust
//! SCRFD Face Detection
//!
//! Detects faces in images and returns bounding boxes with landmarks.

use std::path::Path;

use image::{DynamicImage, GenericImageView, Rgb, RgbImage};
use ndarray::{Array, Array4, Axis};
use ort::{Session, Value};

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

impl FaceDetector {
    /// Load the SCRFD model
    pub fn new<P: AsRef<Path>>(
        runtime: &OnnxRuntime,
        model_path: P,
    ) -> ort::Result<Self> {
        let session = runtime.load_model(model_path)?;
        
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
    pub fn detect(&self, image: &DynamicImage) -> Vec<DetectedFace> {
        let (orig_width, orig_height) = image.dimensions();
        
        // Preprocess image
        let input = self.preprocess(image);
        
        // Run inference
        let outputs = match self.run_inference(&input) {
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

    /// Preprocess image for SCRFD
    fn preprocess(&self, image: &DynamicImage) -> Array4<f32> {
        let (target_w, target_h) = self.input_size;
        
        // Resize maintaining aspect ratio, pad if needed
        let resized = image.resize_exact(
            target_w,
            target_h,
            image::imageops::FilterType::Triangle,
        );
        
        let rgb = resized.to_rgb8();
        
        // Convert to float and normalize (mean=[127.5, 127.5, 127.5], std=[128, 128, 128])
        let mut input = Array4::<f32>::zeros((1, 3, target_h as usize, target_w as usize));
        
        for y in 0..target_h {
            for x in 0..target_w {
                let pixel = rgb.get_pixel(x, y);
                input[[0, 0, y as usize, x as usize]] = (pixel[0] as f32 - 127.5) / 128.0;
                input[[0, 1, y as usize, x as usize]] = (pixel[1] as f32 - 127.5) / 128.0;
                input[[0, 2, y as usize, x as usize]] = (pixel[2] as f32 - 127.5) / 128.0;
            }
        }
        
        input
    }

    /// Run ONNX inference
    fn run_inference(&self, input: &Array4<f32>) -> ort::Result<Vec<Array<f32, ndarray::IxDyn>>> {
        let input_tensor = Value::from_array(input.view())?;
        
        let outputs = self.session.run(ort::inputs!["input.1" => input_tensor]?)?;
        
        // Extract output tensors
        let mut results = Vec::new();
        for (_, value) in outputs {
            let tensor = value.try_extract_tensor::<f32>()?;
            results.push(tensor.view().to_owned().into_dyn());
        }
        
        Ok(results)
    }

    /// Post-process SCRFD outputs to DetectedFace
    fn postprocess(
        &self,
        outputs: &[Array<f32, ndarray::IxDyn>],
        orig_width: u32,
        orig_height: u32,
    ) -> Vec<DetectedFace> {
        let mut faces = Vec::new();
        
        // SCRFD outputs: scores, bboxes, landmarks at different scales
        // This is a simplified implementation - real SCRFD has 9 outputs
        // For MVP, we'll use a simplified single-scale approach
        
        // The actual implementation depends on the specific SCRFD variant
        // Here's a placeholder that shows the structure:
        
        if outputs.len() < 3 {
            return faces;
        }
        
        let scores = &outputs[0];
        let bboxes = &outputs[1];
        let landmarks = &outputs[2];
        
        let scale_x = orig_width as f32 / self.input_size.0 as f32;
        let scale_y = orig_height as f32 / self.input_size.1 as f32;
        
        // Process each detection
        for i in 0..scores.len() {
            let score = scores[[i]];
            
            if score < self.confidence_threshold {
                continue;
            }
            
            // Extract bounding box
            let x1 = bboxes[[i, 0]] * scale_x;
            let y1 = bboxes[[i, 1]] * scale_y;
            let x2 = bboxes[[i, 2]] * scale_x;
            let y2 = bboxes[[i, 3]] * scale_y;
            
            let width = x2 - x1;
            let height = y2 - y1;
            
            // Extract landmarks
            let mut lmks = [(0.0f32, 0.0f32); 5];
            for j in 0..5 {
                lmks[j] = (
                    landmarks[[i, j * 2]] * scale_x,
                    landmarks[[i, j * 2 + 1]] * scale_y,
                );
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
        
        faces
    }

    /// Non-maximum suppression to remove overlapping detections
    fn non_max_suppression(&self, mut faces: Vec<DetectedFace>) -> Vec<DetectedFace> {
        if faces.is_empty() {
            return faces;
        }
        
        // Sort by confidence (descending)
        faces.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        
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

    /// Calculate Intersection over Union
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

    /// Align face using landmarks (simplified affine transform)
    fn align_face(&self, image: &DynamicImage, landmarks: &[(f32, f32); 5]) -> RgbImage {
        // Target landmarks for aligned 112x112 face
        let dst_landmarks: [(f32, f32); 5] = [
            (38.2946, 51.6963),  // left eye
            (73.5318, 51.5014),  // right eye
            (56.0252, 71.7366),  // nose
            (41.5493, 92.3655),  // left mouth
            (70.7299, 92.2041),  // right mouth
        ];
        
        // Compute similarity transform (simplified - just crop and resize for MVP)
        // Full implementation would use proper affine transformation
        
        let left_eye = landmarks[0];
        let right_eye = landmarks[1];
        
        // Calculate eye center and angle
        let eye_center = (
            (left_eye.0 + right_eye.0) / 2.0,
            (left_eye.1 + right_eye.1) / 2.0,
        );
        
        // Calculate face size based on eye distance
        let eye_dist = ((right_eye.0 - left_eye.0).powi(2) + 
                       (right_eye.1 - left_eye.1).powi(2)).sqrt();
        let face_size = eye_dist * 2.5;
        
        // Calculate crop region
        let x = (eye_center.0 - face_size / 2.0).max(0.0) as u32;
        let y = (eye_center.1 - face_size / 2.0).max(0.0) as u32;
        let size = face_size as u32;
        
        // Crop and resize
        let cropped = image.crop_imm(
            x.min(image.width().saturating_sub(1)),
            y.min(image.height().saturating_sub(1)),
            size.min(image.width() - x),
            size.min(image.height() - y),
        );
        
        let resized = cropped.resize_exact(
            112,
            112,
            image::imageops::FilterType::Lanczos3,
        );
        
        resized.to_rgb8()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iou_calculation() {
        let detector = FaceDetector {
            session: todo!(), // Would need mock
            input_size: (640, 640),
            confidence_threshold: 0.5,
            nms_threshold: 0.4,
        };
        
        // Same box should have IoU of 1.0
        let box1 = (0.0, 0.0, 100.0, 100.0);
        assert!((detector.calculate_iou(&box1, &box1) - 1.0).abs() < 0.001);
        
        // Non-overlapping boxes should have IoU of 0.0
        let box2 = (200.0, 200.0, 100.0, 100.0);
        assert!((detector.calculate_iou(&box1, &box2) - 0.0).abs() < 0.001);
    }
}
```

---

## Step 4: Face Embedder

### File: `src/ml/face_embedder.rs`

```rust
//! ArcFace Face Embedding
//!
//! Generates 512-dimensional embeddings for face recognition.

use std::path::Path;

use image::RgbImage;
use ndarray::{Array, Array1, Array4};
use ort::{Session, Value};

use super::OnnxRuntime;

/// 512-dimensional face embedding
#[derive(Debug, Clone)]
pub struct FaceEmbedding {
    /// The embedding vector
    pub vector: Array1<f32>,
}

impl FaceEmbedding {
    /// Create from a vector
    pub fn new(vector: Array1<f32>) -> Self {
        Self { vector }
    }

    /// Calculate cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &FaceEmbedding) -> f32 {
        let dot: f32 = self.vector.iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum();
        
        let norm1: f32 = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm2: f32 = other.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm1 > 0.0 && norm2 > 0.0 {
            dot / (norm1 * norm2)
        } else {
            0.0
        }
    }

    /// Calculate Euclidean distance with another embedding
    pub fn euclidean_distance(&self, other: &FaceEmbedding) -> f32 {
        self.vector.iter()
            .zip(other.vector.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt()
    }

    /// Convert to bytes for database storage
    pub fn to_bytes(&self) -> Vec<u8> {
        self.vector.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect()
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 512 * 4 {
            return None;
        }
        
        let vector: Array1<f32> = Array1::from_iter(
            bytes.chunks(4)
                .map(|chunk| {
                    let arr: [u8; 4] = chunk.try_into().unwrap();
                    f32::from_le_bytes(arr)
                })
        );
        
        Some(Self { vector })
    }
}

/// ArcFace Face Embedder
pub struct FaceEmbedder {
    session: Session,
}

impl FaceEmbedder {
    /// Load the ArcFace model
    pub fn new<P: AsRef<Path>>(
        runtime: &OnnxRuntime,
        model_path: P,
    ) -> ort::Result<Self> {
        let session = runtime.load_model(model_path)?;
        
        Ok(Self { session })
    }

    /// Generate embedding for an aligned face image (112x112)
    pub fn embed(&self, aligned_face: &RgbImage) -> Option<FaceEmbedding> {
        if aligned_face.width() != 112 || aligned_face.height() != 112 {
            tracing::warn!("Face image must be 112x112, got {}x{}", 
                aligned_face.width(), aligned_face.height());
            return None;
        }
        
        // Preprocess
        let input = self.preprocess(aligned_face);
        
        // Run inference
        let embedding = self.run_inference(&input).ok()?;
        
        // Normalize embedding
        let normalized = self.normalize(&embedding);
        
        Some(FaceEmbedding::new(normalized))
    }

    /// Preprocess face image for ArcFace
    fn preprocess(&self, face: &RgbImage) -> Array4<f32> {
        let mut input = Array4::<f32>::zeros((1, 3, 112, 112));
        
        for y in 0..112 {
            for x in 0..112 {
                let pixel = face.get_pixel(x, y);
                // Normalize to [-1, 1]
                input[[0, 0, y as usize, x as usize]] = (pixel[0] as f32 - 127.5) / 127.5;
                input[[0, 1, y as usize, x as usize]] = (pixel[1] as f32 - 127.5) / 127.5;
                input[[0, 2, y as usize, x as usize]] = (pixel[2] as f32 - 127.5) / 127.5;
            }
        }
        
        input
    }

    /// Run ONNX inference
    fn run_inference(&self, input: &Array4<f32>) -> ort::Result<Array1<f32>> {
        let input_tensor = Value::from_array(input.view())?;
        
        let outputs = self.session.run(ort::inputs!["input" => input_tensor]?)?;
        
        let output = outputs.get("output").or_else(|| outputs.values().next())
            .ok_or_else(|| ort::Error::new("No output tensor"))?;
        
        let tensor = output.try_extract_tensor::<f32>()?;
        let view = tensor.view();
        
        // Flatten to 1D
        Ok(Array1::from_iter(view.iter().copied()))
    }

    /// L2 normalize the embedding
    fn normalize(&self, embedding: &Array1<f32>) -> Array1<f32> {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm > 0.0 {
            embedding / norm
        } else {
            embedding.clone()
        }
    }
}
```

---

## Step 5: Face Clustering

### File: `src/ml/clustering.rs`

```rust
//! Face Clustering using DBSCAN
//!
//! Groups similar face embeddings into clusters (people).

use std::collections::HashMap;

use ndarray::{Array1, Array2};

use super::FaceEmbedding;

/// Face cluster result
#[derive(Debug, Clone)]
pub struct FaceClusterResult {
    /// Cluster ID (-1 for noise/unclustered)
    pub cluster_id: i32,
    
    /// Face IDs in this cluster
    pub face_ids: Vec<i64>,
    
    /// Centroid embedding (average of all faces in cluster)
    pub centroid: Option<FaceEmbedding>,
}

/// DBSCAN-based face clusterer
pub struct FaceClusterer {
    /// Minimum samples to form a cluster
    min_samples: usize,
    
    /// Maximum distance (1 - cosine_similarity) for same cluster
    epsilon: f32,
}

impl FaceClusterer {
    /// Create a new clusterer
    pub fn new() -> Self {
        Self {
            min_samples: 2,   // At least 2 faces to form a cluster
            epsilon: 0.4,     // ~0.6 cosine similarity threshold
        }
    }

    /// Set epsilon (distance threshold)
    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    /// Set minimum samples per cluster
    pub fn with_min_samples(mut self, min_samples: usize) -> Self {
        self.min_samples = min_samples;
        self
    }

    /// Cluster faces and return cluster assignments
    /// 
    /// Returns a map from face_id to cluster_id (-1 for noise)
    pub fn cluster(
        &self,
        faces: &[(i64, FaceEmbedding)],
    ) -> HashMap<i64, i32> {
        if faces.is_empty() {
            return HashMap::new();
        }

        let n = faces.len();
        
        // Build distance matrix (1 - cosine_similarity)
        let mut distances = Array2::<f32>::zeros((n, n));
        for i in 0..n {
            for j in i..n {
                if i == j {
                    distances[[i, j]] = 0.0;
                } else {
                    let sim = faces[i].1.cosine_similarity(&faces[j].1);
                    let dist = 1.0 - sim;
                    distances[[i, j]] = dist;
                    distances[[j, i]] = dist;
                }
            }
        }

        // Run DBSCAN
        let labels = self.dbscan(&distances);

        // Map back to face IDs
        faces.iter()
            .enumerate()
            .map(|(i, (face_id, _))| (*face_id, labels[i]))
            .collect()
    }

    /// DBSCAN implementation
    fn dbscan(&self, distances: &Array2<f32>) -> Vec<i32> {
        let n = distances.nrows();
        let mut labels = vec![-1i32; n]; // -1 = undefined
        let mut cluster_id = 0;

        for i in 0..n {
            if labels[i] != -1 {
                continue; // Already processed
            }

            // Find neighbors
            let neighbors = self.region_query(distances, i);
            
            if neighbors.len() < self.min_samples {
                labels[i] = -1; // Mark as noise (will stay -1)
                continue;
            }

            // Start a new cluster
            labels[i] = cluster_id;
            
            // Expand cluster
            let mut seed_set: Vec<usize> = neighbors.clone();
            let mut j = 0;
            
            while j < seed_set.len() {
                let q = seed_set[j];
                
                if labels[q] == -1 {
                    labels[q] = cluster_id; // Change noise to cluster
                }
                
                if labels[q] != -1 && labels[q] != cluster_id {
                    j += 1;
                    continue; // Already in a cluster
                }
                
                labels[q] = cluster_id;
                
                let q_neighbors = self.region_query(distances, q);
                if q_neighbors.len() >= self.min_samples {
                    // Add new neighbors to seed set
                    for &neighbor in &q_neighbors {
                        if !seed_set.contains(&neighbor) {
                            seed_set.push(neighbor);
                        }
                    }
                }
                
                j += 1;
            }

            cluster_id += 1;
        }

        labels
    }

    /// Find all points within epsilon distance
    fn region_query(&self, distances: &Array2<f32>, point: usize) -> Vec<usize> {
        let n = distances.nrows();
        (0..n)
            .filter(|&i| distances[[point, i]] <= self.epsilon)
            .collect()
    }

    /// Assign a new face to an existing cluster (for incremental clustering)
    pub fn assign_to_cluster(
        &self,
        new_embedding: &FaceEmbedding,
        cluster_centroids: &[(i32, FaceEmbedding)],
    ) -> Option<i32> {
        let mut best_cluster = None;
        let mut best_distance = f32::MAX;

        for (cluster_id, centroid) in cluster_centroids {
            let distance = 1.0 - new_embedding.cosine_similarity(centroid);
            if distance < self.epsilon && distance < best_distance {
                best_distance = distance;
                best_cluster = Some(*cluster_id);
            }
        }

        best_cluster
    }

    /// Calculate centroid of embeddings
    pub fn calculate_centroid(embeddings: &[FaceEmbedding]) -> Option<FaceEmbedding> {
        if embeddings.is_empty() {
            return None;
        }

        let n = embeddings.len() as f32;
        let mut sum = Array1::<f32>::zeros(512);

        for emb in embeddings {
            sum = sum + &emb.vector;
        }

        let avg = sum / n;
        
        // Normalize
        let norm: f32 = avg.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized = if norm > 0.0 { avg / norm } else { avg };

        Some(FaceEmbedding::new(normalized))
    }
}

impl Default for FaceClusterer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let emb1 = FaceEmbedding::new(Array1::from_vec(vec![1.0, 0.0, 0.0]));
        let emb2 = FaceEmbedding::new(Array1::from_vec(vec![1.0, 0.0, 0.0]));
        
        assert!((emb1.cosine_similarity(&emb2) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_embedding_serialization() {
        let original = FaceEmbedding::new(Array1::from_vec(vec![1.0; 512]));
        let bytes = original.to_bytes();
        let restored = FaceEmbedding::from_bytes(&bytes).unwrap();
        
        assert!((original.cosine_similarity(&restored) - 1.0).abs() < 0.001);
    }
}
```

---

## Step 6: Face Repository

### File: `src/db/face_repo.rs`

```rust
//! Face database repository

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::ml::{FaceEmbedding, DetectedFace};

/// Face record from database
#[derive(Debug, Clone)]
pub struct FaceRecord {
    pub id: i64,
    pub photo_id: i64,
    pub bbox_x: f32,
    pub bbox_y: f32,
    pub bbox_width: f32,
    pub bbox_height: f32,
    pub confidence: f32,
    pub cluster_id: Option<i64>,
    pub embedding: FaceEmbedding,
}

/// Face cluster record
#[derive(Debug, Clone)]
pub struct FaceClusterRecord {
    pub id: i64,
    pub name: Option<String>,
    pub representative_face_id: Option<i64>,
    pub face_count: i64,
}

/// Face database repository
pub struct FaceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> FaceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert a detected face
    pub fn insert_face(
        &self,
        photo_id: i64,
        face: &DetectedFace,
        embedding: &FaceEmbedding,
    ) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO faces (
                photo_id,
                bbox_x, bbox_y, bbox_width, bbox_height,
                confidence, embedding
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                photo_id,
                face.bbox_normalized.0,
                face.bbox_normalized.1,
                face.bbox_normalized.2,
                face.bbox_normalized.3,
                face.confidence,
                embedding.to_bytes(),
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get all faces without cluster assignment
    pub fn get_unclustered_faces(&self) -> SqliteResult<Vec<FaceRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_width, bbox_height,
                   confidence, cluster_id, embedding
            FROM faces
            WHERE cluster_id IS NULL
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let embedding_bytes: Vec<u8> = row.get(8)?;
            let embedding = FaceEmbedding::from_bytes(&embedding_bytes)
                .unwrap_or_else(|| FaceEmbedding::new(ndarray::Array1::zeros(512)));
            
            Ok(FaceRecord {
                id: row.get(0)?,
                photo_id: row.get(1)?,
                bbox_x: row.get(2)?,
                bbox_y: row.get(3)?,
                bbox_width: row.get(4)?,
                bbox_height: row.get(5)?,
                confidence: row.get(6)?,
                cluster_id: row.get(7)?,
                embedding,
            })
        })?;

        let mut faces = Vec::new();
        for row in rows {
            faces.push(row?);
        }

        Ok(faces)
    }

    /// Get all faces with embeddings
    pub fn get_all_faces_with_embeddings(&self) -> SqliteResult<Vec<(i64, FaceEmbedding)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, embedding FROM faces"
        )?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            Ok((id, bytes))
        })?;

        let mut faces = Vec::new();
        for row in rows {
            let (id, bytes) = row?;
            if let Some(emb) = FaceEmbedding::from_bytes(&bytes) {
                faces.push((id, emb));
            }
        }

        Ok(faces)
    }

    /// Update cluster assignments for faces
    pub fn update_cluster_assignments(
        &self,
        assignments: &[(i64, i32)],
    ) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        for (face_id, cluster_id) in assignments {
            if *cluster_id >= 0 {
                self.conn.execute(
                    "UPDATE faces SET cluster_id = ?1 WHERE id = ?2",
                    params![cluster_id, face_id],
                )?;
            }
        }

        tx.commit()
    }

    /// Create a new face cluster
    pub fn create_cluster(&self, face_ids: &[i64]) -> SqliteResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO face_clusters (face_count)
            VALUES (?1)
            "#,
            params![face_ids.len() as i64],
        )?;

        let cluster_id = self.conn.last_insert_rowid();

        // Update faces with cluster ID
        for face_id in face_ids {
            self.conn.execute(
                "UPDATE faces SET cluster_id = ?1 WHERE id = ?2",
                params![cluster_id, face_id],
            )?;
        }

        // Set representative face (highest confidence)
        self.conn.execute(
            r#"
            UPDATE face_clusters SET representative_face_id = (
                SELECT id FROM faces 
                WHERE cluster_id = ?1 
                ORDER BY confidence DESC 
                LIMIT 1
            ) WHERE id = ?1
            "#,
            params![cluster_id],
        )?;

        Ok(cluster_id)
    }

    /// Get all clusters
    pub fn get_all_clusters(&self) -> SqliteResult<Vec<FaceClusterRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, representative_face_id, face_count
            FROM face_clusters
            ORDER BY face_count DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FaceClusterRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                representative_face_id: row.get(2)?,
                face_count: row.get(3)?,
            })
        })?;

        let mut clusters = Vec::new();
        for row in rows {
            clusters.push(row?);
        }

        Ok(clusters)
    }

    /// Name a cluster
    pub fn name_cluster(&self, cluster_id: i64, name: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE face_clusters SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, cluster_id],
        )?;
        Ok(())
    }

    /// Merge two clusters
    pub fn merge_clusters(&self, source_id: i64, target_id: i64) -> SqliteResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Move all faces from source to target
        self.conn.execute(
            "UPDATE faces SET cluster_id = ?1 WHERE cluster_id = ?2",
            params![target_id, source_id],
        )?;

        // Update face count
        self.conn.execute(
            r#"
            UPDATE face_clusters SET 
                face_count = (SELECT COUNT(*) FROM faces WHERE cluster_id = ?1),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![target_id],
        )?;

        // Delete source cluster
        self.conn.execute(
            "DELETE FROM face_clusters WHERE id = ?1",
            params![source_id],
        )?;

        tx.commit()
    }

    /// Get photos for a cluster
    pub fn get_photos_for_cluster(&self, cluster_id: i64) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT photo_id FROM faces
            WHERE cluster_id = ?1
            "#,
        )?;

        let rows = stmt.query_map(params![cluster_id], |row| row.get(0))?;

        let mut photo_ids = Vec::new();
        for row in rows {
            photo_ids.push(row?);
        }

        Ok(photo_ids)
    }
}
```

Update `src/db/mod.rs`:

```rust
pub mod face_repo;
pub use face_repo::{FaceRepo, FaceRecord, FaceClusterRecord};
```

---

## Step 7: People View

### File: `src/views/people.rs`

```rust
//! People view - face clusters display

use iced::widget::{button, column, container, row, scrollable, text, text_input, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::FaceClusterRecord;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// People view component
pub struct PeopleView;

impl PeopleView {
    /// Render with clusters
    pub fn view_with_clusters(
        clusters: &[FaceClusterRecord],
        editing_cluster: Option<i64>,
        edit_name: &str,
    ) -> Element<'static, Message> {
        if clusters.is_empty() {
            return Self::empty_view();
        }

        let title = text("People")
            .size(28)
            .color(Text::PRIMARY);

        let subtitle = text(format!("{} people recognized", clusters.len()))
            .size(14)
            .color(Text::SECONDARY);

        // Grid of people
        let mut grid_rows: Vec<Element<'static, Message>> = Vec::new();
        let mut current_row: Vec<Element<'static, Message>> = Vec::new();
        let columns = 4;

        for cluster in clusters {
            let is_editing = editing_cluster == Some(cluster.id);
            let card = Self::person_card(cluster, is_editing, edit_name);
            current_row.push(card);

            if current_row.len() >= columns {
                grid_rows.push(
                    Row::with_children(current_row)
                        .spacing(16)
                        .into()
                );
                current_row = Vec::new();
            }
        }

        // Add remaining cards
        if !current_row.is_empty() {
            grid_rows.push(
                Row::with_children(current_row)
                    .spacing(16)
                    .into()
            );
        }

        let grid = Column::with_children(grid_rows)
            .spacing(16);

        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(24),
            scrollable(grid).height(Length::Fill),
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Empty state
    pub fn view() -> Element<'static, Message> {
        Self::empty_view()
    }

    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("People")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Faces will appear here after processing.")
                .size(14)
                .color(Text::SECONDARY),
            Space::with_height(32),
            text("Face detection runs automatically in the background.")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a person card
    fn person_card(
        cluster: &FaceClusterRecord,
        is_editing: bool,
        edit_name: &str,
    ) -> Element<'static, Message> {
        let cluster_id = cluster.id;
        
        // Face thumbnail placeholder
        let face_circle = container(
            text("👤").size(32)
        )
        .width(80)
        .height(80)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Backgrounds::ELEVATED.into()),
            border: iced::Border {
                radius: 40.0.into(), // Circular
                ..Default::default()
            },
            ..Default::default()
        });

        // Name (editable)
        let name_element: Element<'static, Message> = if is_editing {
            text_input("Enter name...", edit_name)
                .on_input(move |s| Message::EditClusterName(cluster_id, s))
                .on_submit(Message::SaveClusterName(cluster_id))
                .size(14)
                .width(Length::Fixed(140.0))
                .into()
        } else {
            let display_name = cluster.name.as_deref()
                .unwrap_or(&format!("Person {}", cluster.id));
            
            button(
                text(display_name)
                    .size(14)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([4, 8]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::StartEditClusterName(cluster_id))
            .into()
        };

        // Photo count
        let count = text(format!("{} photos", cluster.face_count))
            .size(12)
            .color(Text::TERTIARY);

        let card_content = column![
            face_circle,
            Space::with_height(12),
            name_element,
            count,
        ]
        .spacing(4)
        .align_x(Alignment::Center);

        button(
            container(card_content)
                .padding(16)
                .width(Length::Fixed(160.0))
        )
        .style(|_theme, status| {
            let background = match status {
                button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                _ => Some(Backgrounds::ELEVATED.into()),
            };
            button::Style {
                background,
                border: iced::Border {
                    color: Border::SUBTLE,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Message::SelectCluster(cluster_id))
        .into()
    }
}
```

---

## Step 8: Add Messages to App

Add these messages to `src/app.rs`:

```rust
/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing messages ...

    // Face processing
    ProcessFaces,
    FaceProcessingProgress { processed: usize, total: usize },
    FaceProcessingComplete,

    // Clustering
    RunClustering,
    ClusteringComplete,

    // People view
    SelectCluster(i64),
    StartEditClusterName(i64),
    EditClusterName(i64, String),
    SaveClusterName(i64),
    MergeClusters(i64, i64),
}
```

---

## UI Design: People View

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  People                                           │
│             │  12 people recognized                              │
│  Timeline   │─────────────────────────────────────────────────── │
│  People  ●  │                                                    │
│  Search     │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌───────┐ │
│             │  │  ┌───┐  │  │  ┌───┐  │  │  ┌───┐  │  │ ┌───┐ │ │
│  ─────────  │  │  │ 😊│  │  │  │ 😊│  │  │  │ 😊│  │  │ │ 😊│ │ │
│             │  │  └───┘  │  │  └───┘  │  │  └───┘  │  │ └───┘ │ │
│  Settings   │  │         │  │         │  │         │  │       │ │
│             │  │   Dad   │  │   Mom   │  │  Sarah  │  │Person4│ │
│             │  │ 1,234   │  │   892   │  │   567   │  │  234  │ │
│             │  └─────────┘  └─────────┘  └─────────┘  └───────┘ │
│             │                                                    │
│             │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌───────┐ │
│             │  │  ┌───┐  │  │  ┌───┐  │  │  ┌───┐  │  │ ┌───┐ │ │
│             │  │  │ 😊│  │  │  │ 😊│  │  │  │ 😊│  │  │ │ 😊│ │ │
│             │  │  └───┘  │  │  └───┘  │  │  └───┘  │  │ └───┘ │ │
│             │  │         │  │         │  │         │  │       │ │
│             │  │Person 5 │  │Person 6 │  │Person 7 │  │Person8│ │
│             │  │   189   │  │   145   │  │    98   │  │   67  │ │
│             │  └─────────┘  └─────────┘  └─────────┘  └───────┘ │
│             │                                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Verification Checklist

- [ ] ONNX Runtime initializes correctly
- [ ] SCRFD model loads and detects faces
- [ ] Face detection returns bounding boxes and landmarks
- [ ] ArcFace generates 512-dim embeddings
- [ ] Embeddings stored correctly in SQLite (BLOB)
- [ ] DBSCAN clustering groups similar faces
- [ ] Clusters created in database
- [ ] People view shows all clusters
- [ ] Clicking cluster name enables editing
- [ ] Cluster merge works correctly
- [ ] Face count updates after merge
- [ ] Processing runs in background without blocking UI

---

## Performance Notes

For 100k photos:
- Face detection: ~20ms/photo × 100k = ~33 minutes (parallelizable)
- Embedding generation: ~10ms/face × ~150k faces = ~25 minutes
- Clustering: ~5 minutes for 500k embeddings

Total: ~1 hour for initial processing (background, incremental for new photos)

---

## Next Phase Preview

**Phase 5: Duplicate & Burst Detection** will add:
- Exact duplicate detection (SHA256)
- Burst photo grouping (timestamp proximity)
- Best-pick scoring (sharpness, blur detection)
- Duplicate/Burst review UI

---

## Expected Results & Behavior

> **IMPORTANT:** Do not proceed to Phase 5 until a human has verified ALL of the following behaviors. Each item must be manually tested and confirmed.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **People View** | Grid of face clusters, each showing representative face thumbnail and name |
| **Face Thumbnails** | Cropped face images clearly visible, properly centered on face |
| **Cluster Names** | "Unknown Person 1", "Unknown Person 2" etc. for unnamed clusters |
| **Face Count Badges** | Each cluster shows photo count (e.g., "42 photos") |
| **Processing Progress** | Background processing indicator visible during face detection |
| **Empty State** | Message shown when no faces detected yet or processing not started |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **Click a face cluster** | Opens cluster detail showing all photos containing that person |
| **Edit cluster name** | Click name to edit, type new name, press Enter to save |
| **Merge two clusters** | Select clusters, merge action combines them under one name |
| **Navigate cluster detail** | Click photo in cluster to open photo detail view |
| **Start face processing** | Background processing begins, progress indicator visible |
| **Face count updates after merge** | Merged cluster shows combined photo count |
| **Processing runs in background** | UI remains responsive during face detection/embedding |

### Technical Verification

```bash
# Check ONNX models are loaded
ls -la /path/to/photovault/models/

# Verify faces detected in database
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM faces;"

# Check face clusters created
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT id, name, face_count FROM face_clusters ORDER BY face_count DESC LIMIT 10;"

# Verify embeddings stored (512-dim vectors)
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT photo_id, LENGTH(embedding) FROM faces LIMIT 5;"

# Check cluster assignments
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT cluster_id, COUNT(*) as cnt FROM faces WHERE cluster_id IS NOT NULL GROUP BY cluster_id ORDER BY cnt DESC LIMIT 10;"
```

**Expected:** Faces table populated with bounding boxes and embeddings. Clusters created with reasonable groupings. Same person's photos mostly in the same cluster.

### Performance Verification

| Metric | Expected |
|--------|----------|
| **Face detection** | ~20ms per photo (GPU) or ~100ms (CPU) |
| **Embedding generation** | ~10ms per face |
| **DBSCAN clustering** | < 5 minutes for 100k faces |
| **People view load** | < 1 second to display cluster grid |
| **Memory during processing** | Under 1GB with ONNX runtime loaded |

### Sign-off Checklist

Before proceeding to Phase 5, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **ONNX runtime loads:** SCRFD and ArcFace models load without errors
- [ ] **Faces detected:** Bounding boxes correctly placed on faces in photos
- [ ] **Embeddings generated:** 512-dimensional vectors stored for each face
- [ ] **Clustering works:** DBSCAN groups similar faces into clusters
- [ ] **People view displays:** Face clusters shown in grid with thumbnails
- [ ] **Naming works:** Can rename clusters, name persists after restart
- [ ] **Merging works:** Two clusters can be merged into one
- [ ] **Background processing:** Face detection runs without blocking UI
- [ ] **No console errors:** ONNX runtime operates cleanly
- [ ] **SKILL.md followed:** People view UI matches design guidelines

**Signature:** ___________________ **Date:** _______________

---

## Proceed to Phase 5

Only after ALL items above are verified, proceed to:

📁 `plan/PHASE_5_DUPLICATES_BURSTS.md`
