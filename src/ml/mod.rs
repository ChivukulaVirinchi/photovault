//! Machine Learning module
//!
//! Provides face detection and recognition using ONNX models.

pub mod clustering;
pub mod face_detector;
pub mod face_embedder;
pub mod runtime;

pub use clustering::FaceClusterer;
pub use face_detector::{DetectedFace, FaceDetector};
pub use face_embedder::{FaceEmbedder, FaceEmbedding};
pub use runtime::OnnxRuntime;
