//! Machine Learning module
//!
//! Provides face detection and recognition using ONNX models.

pub mod alignment;
pub mod clustering;
pub mod face_detector;
pub mod face_embedder;
pub mod remote_embedder;
pub mod resolver;
pub mod retrieval;
pub mod runtime;

pub use clustering::{ClusterInput, FaceClusterer};
pub use face_detector::FaceDetector;
pub use face_embedder::{EmbedderConfig, FaceEmbedder, FaceEmbedding, LocalEmbedder};
pub use resolver::{rerank, ResolverContext, ResolverWeights};
pub use retrieval::{retrieve_candidates, BandingConfig, ConfidenceBand, RetrievalHit};
pub use runtime::{active_execution_provider, OnnxRuntime};
