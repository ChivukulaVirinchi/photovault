//! ArcFace Face Embedding
//!
//! Generates 512-dimensional embeddings for face recognition.
//! Uses the ArcFace-R100 model via ONNX Runtime, with optional
//! offloading to a remote GPU bridge.

use std::path::PathBuf;

use image::RgbImage;
use ndarray::Array1;
use ort::session::Session;
use ort::value::TensorRef;

use super::remote_embedder::RemoteEmbedder;
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

/// Configuration for creating a FaceEmbedder.
#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub model_path: PathBuf,
    pub gpu_bridge_url: Option<String>,
    /// Name of the model the desktop is configured to use (e.g.
    /// `adaface_ir101_webface12m.onnx`). Passed to the bridge so it
    /// can refuse if its loaded model is different — mismatched models
    /// produce embeddings in different metric spaces and would corrupt
    /// clustering.
    pub expected_model: String,
    pub intra_threads: usize,
}

/// Dispatch enum — either local ONNX or remote GPU bridge.
///
/// The `Remote` variant always carries a `LocalEmbedder` alongside it.
/// When the remote bridge fails for a batch, we fall back to local for
/// just that batch instead of dropping the faces silently. The remote's
/// own 3-strikes / unhealthy check still applies; this is the per-batch
/// safety net while it's nominally healthy.
pub enum FaceEmbedder {
    Local(LocalEmbedder),
    Remote {
        remote: RemoteEmbedder,
        local_fallback: LocalEmbedder,
    },
}

impl FaceEmbedder {
    /// Build the configured embedder. Always constructs a local
    /// instance — it's required as a fallback even when the bridge is
    /// healthy. If a remote bridge URL is configured AND the bridge
    /// passes its initial health check (including model match), the
    /// returned embedder routes batches through the remote first.
    pub fn from_config(rt: &OnnxRuntime, cfg: &EmbedderConfig) -> Result<Self, ort::Error> {
        let local = LocalEmbedder::new_with_threads(rt, &cfg.model_path, cfg.intra_threads)?;
        if let Some(ref url) = cfg.gpu_bridge_url {
            let remote = RemoteEmbedder::new(url.clone(), cfg.expected_model.clone());
            if remote.is_healthy() {
                tracing::info!("Face embedding routed to remote GPU bridge at {}", url);
                return Ok(FaceEmbedder::Remote {
                    remote,
                    local_fallback: local,
                });
            }
            tracing::info!("Remote GPU bridge unavailable; using local embedder");
        }
        Ok(FaceEmbedder::Local(local))
    }

    /// Generate embedding for an aligned face image (112x112).
    pub fn embed(&mut self, aligned_face: &RgbImage) -> Option<FaceEmbedding> {
        self.embed_batch(std::slice::from_ref(aligned_face))
            .into_iter()
            .next()
            .flatten()
    }

    /// Generate embeddings for a batch of aligned face images.
    /// Returns one Option per input; None only if both remote and
    /// local-fallback produced no embedding for that input.
    pub fn embed_batch(&mut self, faces: &[RgbImage]) -> Vec<Option<FaceEmbedding>> {
        match self {
            FaceEmbedder::Local(e) => e.embed_batch(faces),
            FaceEmbedder::Remote {
                remote,
                local_fallback,
            } => {
                let mut out = remote.embed_batch(faces);
                if out.len() != faces.len() {
                    // Defensive: should never happen, but if it does,
                    // recompute fully on local.
                    return local_fallback.embed_batch(faces);
                }
                let missing: Vec<usize> = out
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| if r.is_none() { Some(i) } else { None })
                    .collect();
                if missing.is_empty() {
                    return out;
                }
                let crops: Vec<RgbImage> =
                    missing.iter().map(|&i| faces[i].clone()).collect();
                let fb = local_fallback.embed_batch(&crops);
                for (k, &orig_i) in missing.iter().enumerate() {
                    if let Some(emb) = fb.get(k).cloned().flatten() {
                        out[orig_i] = Some(emb);
                    }
                }
                out
            }
        }
    }
}

/// Local ArcFace Face Embedder using ONNX Runtime.
pub struct LocalEmbedder {
    session: Session,
}

impl LocalEmbedder {
    /// Load the ArcFace model with a specific thread count per session.
    pub fn new_with_threads<P: AsRef<std::path::Path>>(
        runtime: &OnnxRuntime,
        model_path: P,
        intra_threads: usize,
    ) -> ort::Result<Self> {
        let session = runtime.load_model_with_threads(model_path, intra_threads)?;

        Ok(Self { session })
    }

    /// Generate embedding for an aligned face image (112x112)
    pub fn embed(&mut self, aligned_face: &RgbImage) -> Option<FaceEmbedding> {
        self.embed_batch(std::slice::from_ref(aligned_face))
            .into_iter()
            .next()
            .flatten()
    }

    /// Generate embeddings for a batch of aligned face images.
    /// Returns one Option per input; None if that input was invalid.
    pub fn embed_batch(&mut self, faces: &[RgbImage]) -> Vec<Option<FaceEmbedding>> {
        let n = faces.len();
        if n == 0 {
            return Vec::new();
        }

        // Validate dimensions and preprocess each face into a flat [3*112*112] row.
        let mut valid_indices: Vec<usize> = Vec::with_capacity(n);
        let mut batch_data: Vec<f32> = Vec::with_capacity(n * 3 * 112 * 112);
        for (i, face) in faces.iter().enumerate() {
            if face.width() != 112 || face.height() != 112 {
                continue;
            }
            valid_indices.push(i);
            let row = self.preprocess(face);
            batch_data.extend_from_slice(&row);
        }

        if valid_indices.is_empty() {
            return vec![None; n];
        }

        let batch_n = valid_indices.len();
        let raw_outputs = match self.run_inference_batch(&batch_data, batch_n as i64) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Batch embedding inference failed: {}", e);
                return vec![None; n];
            }
        };

        // Each output row is 512 floats. L2-normalize each.
        let mut embeddings: Vec<Option<FaceEmbedding>> = vec![None; n];
        for (out_i, orig_i) in valid_indices.iter().enumerate() {
            let start = out_i * 512;
            let end = start + 512;
            if end > raw_outputs.len() {
                break;
            }
            let row = Array1::from_vec(raw_outputs[start..end].to_vec());
            let normalized = self.normalize(&row);
            embeddings[*orig_i] = Some(FaceEmbedding::new(normalized));
        }
        embeddings
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

    /// Run ONNX inference with a batch of faces [N, 3, 112, 112].
    /// Returns a flat Vec<f32> of N * 512 floats.
    fn run_inference_batch(
        &mut self,
        input_data: &[f32],
        batch_size: i64,
    ) -> ort::Result<Vec<f32>> {
        let input_tensor =
            TensorRef::<f32>::from_array_view((vec![batch_size, 3, 112, 112], input_data))?;

        let outputs = self.session.run(ort::inputs![input_tensor])?;

        let (_name, output) = outputs
            .iter()
            .next()
            .ok_or_else(|| ort::Error::new("No output tensor from ArcFace model".to_string()))?;

        let (_shape, data) = output.try_extract_tensor::<f32>()?;
        Ok(data.to_vec())
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
