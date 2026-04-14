//! ArcFace Face Embedding
//!
//! Generates 512-dimensional embeddings for face recognition.
//! Uses the ArcFace-R100 model via ONNX Runtime.

use std::path::Path;

use image::RgbImage;
use ndarray::Array1;
use ort::session::Session;
use ort::value::TensorRef;

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
        let dot: f32 = self
            .vector
            .iter()
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

    /// Convert to bytes for database storage (little-endian f32 values)
    pub fn to_bytes(&self) -> Vec<u8> {
        self.vector.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// Create from bytes (expects 512 * 4 = 2048 bytes, little-endian f32)
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 512 * 4 {
            return None;
        }

        let vector: Array1<f32> = Array1::from_iter(bytes.chunks(4).map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(arr)
        }));

        Some(Self { vector })
    }
}

/// ArcFace Face Embedder
pub struct FaceEmbedder {
    session: Session,
}

impl FaceEmbedder {
    /// Load the ArcFace model
    pub fn new<P: AsRef<Path>>(runtime: &OnnxRuntime, model_path: P) -> ort::Result<Self> {
        let session = runtime.load_model(model_path)?;

        Ok(Self { session })
    }

    /// Load the ArcFace model with a specific thread count per session.
    pub fn new_with_threads<P: AsRef<Path>>(
        runtime: &OnnxRuntime,
        model_path: P,
        intra_threads: usize,
    ) -> ort::Result<Self> {
        let session = runtime.load_model_with_threads(model_path, intra_threads)?;

        Ok(Self { session })
    }

    /// Generate embedding for an aligned face image (112x112)
    pub fn embed(&mut self, aligned_face: &RgbImage) -> Option<FaceEmbedding> {
        if aligned_face.width() != 112 || aligned_face.height() != 112 {
            tracing::warn!(
                "Face image must be 112x112, got {}x{}",
                aligned_face.width(),
                aligned_face.height()
            );
            return None;
        }

        // Preprocess
        let input_data = self.preprocess(aligned_face);

        // Run inference
        let embedding = self.run_inference(&input_data).ok()?;

        // Normalize embedding (L2)
        let normalized = self.normalize(&embedding);

        Some(FaceEmbedding::new(normalized))
    }

    /// Preprocess face image for ArcFace: normalize to [-1, 1], produce NCHW vec
    fn preprocess(&self, face: &RgbImage) -> Vec<f32> {
        let mut input = vec![0.0f32; 3 * 112 * 112];
        let hw = 112 * 112;

        for y in 0..112u32 {
            for x in 0..112u32 {
                let pixel = face.get_pixel(x, y);
                let idx = (y * 112 + x) as usize;
                input[idx] = (pixel[0] as f32 - 127.5) / 127.5;
                input[hw + idx] = (pixel[1] as f32 - 127.5) / 127.5;
                input[2 * hw + idx] = (pixel[2] as f32 - 127.5) / 127.5;
            }
        }

        input
    }

    /// Run ONNX inference and return raw embedding
    fn run_inference(&mut self, input_data: &[f32]) -> ort::Result<Array1<f32>> {
        let input_tensor =
            TensorRef::<f32>::from_array_view((vec![1i64, 3, 112, 112], input_data))?;

        let outputs = self.session.run(ort::inputs![input_tensor])?;

        // Get first output (the embedding)
        let (name, output) = outputs
            .iter()
            .next()
            .ok_or_else(|| ort::Error::new(format!("No output tensor from ArcFace model")))?;

        let (_shape, data) = output.try_extract_tensor::<f32>()?;
        let _ = name;

        Ok(Array1::from_vec(data.to_vec()))
    }

    /// L2 normalize the embedding vector
    fn normalize(&self, embedding: &Array1<f32>) -> Array1<f32> {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm > 0.0 {
            embedding / norm
        } else {
            embedding.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let emb1 = FaceEmbedding::new(Array1::from_vec(vec![1.0, 0.0, 0.0]));
        let emb2 = FaceEmbedding::new(Array1::from_vec(vec![1.0, 0.0, 0.0]));

        assert!((emb1.cosine_similarity(&emb2) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let emb1 = FaceEmbedding::new(Array1::from_vec(vec![1.0, 0.0, 0.0]));
        let emb2 = FaceEmbedding::new(Array1::from_vec(vec![0.0, 1.0, 0.0]));

        assert!((emb1.cosine_similarity(&emb2) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_embedding_serialization() {
        let original = FaceEmbedding::new(Array1::from_vec(vec![1.0; 512]));
        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), 512 * 4);

        let restored = FaceEmbedding::from_bytes(&bytes).unwrap();
        assert!((original.cosine_similarity(&restored) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_embedding_from_bytes_wrong_size() {
        let bytes = vec![0u8; 100];
        assert!(FaceEmbedding::from_bytes(&bytes).is_none());
    }
}
