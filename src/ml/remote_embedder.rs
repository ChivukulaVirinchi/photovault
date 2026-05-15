//! Remote GPU bridge embedder.
//!
//! Sends face crops via HTTP multipart to a Colab/Kaggle-hosted GPU
//! inference server. Falls back to local ONNX when the bridge is
//! unreachable or returns errors. The bridge must advertise the same
//! face-embedding model the desktop is configured for — embeddings
//! from different models live in incompatible metric spaces and would
//! corrupt clustering if mixed.

use std::io::Cursor;
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use image::RgbImage;

use super::face_embedder::FaceEmbedding;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const JPEG_QUALITY: u8 = 85;

pub struct RemoteEmbedder {
    client: reqwest::blocking::Client,
    base_url: String,
    /// Model name the desktop expects the bridge to serve (e.g.
    /// `adaface_ir101_webface12m` or `glintr100`). Compared to the
    /// notebook's `/health.model` field, normalized by stripping any
    /// trailing `.onnx`.
    expected_model: String,
    healthy: bool,
    consecutive_failures: u32,
    last_health_at: Instant,
}

impl RemoteEmbedder {
    pub fn new(base_url: String, expected_model: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        let healthy = Self::check_health(&client, &base_url, &expected_model);
        if !healthy {
            tracing::warn!(
                "Remote GPU bridge at {} unhealthy or wrong model (expected {}); will use local fallback",
                base_url,
                expected_model
            );
        }
        Self {
            client,
            base_url,
            expected_model,
            healthy,
            consecutive_failures: 0,
            last_health_at: Instant::now(),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// Health check. Returns true only when the bridge is reachable,
    /// reports a GPU provider, AND the served model matches the one
    /// the desktop is configured to use. Mismatched models silently
    /// produce embeddings in a different metric space — refusing here
    /// is the only safe response.
    fn check_health(
        client: &reqwest::blocking::Client,
        base_url: &str,
        expected_model: &str,
    ) -> bool {
        let resp = match client
            .get(format!("{}/health", base_url))
            .timeout(Duration::from_secs(2))
            .send()
        {
            Ok(r) => r,
            Err(_) => return false,
        };
        if !resp.status().is_success() {
            return false;
        }
        let body: serde_json::Value = match resp.json() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let has_gpu = body
            .get("provider")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.contains("CUDA") || s.contains("GPU"));
        if !has_gpu {
            return false;
        }
        let served_model = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if !models_match(served_model, expected_model) {
            tracing::warn!(
                "Remote bridge model mismatch: served={:?}, expected={:?}",
                served_model,
                expected_model
            );
            return false;
        }
        true
    }

    pub fn embed_batch(&mut self, faces: &[RgbImage]) -> Vec<Option<FaceEmbedding>> {
        let n = faces.len();
        if n == 0 {
            return Vec::new();
        }

        if !self.healthy {
            return vec![None; n];
        }

        // Lazy heartbeat: if it's been a while since we last verified
        // the bridge, ping /health before sending real work. Catches a
        // bridge that died silently without us having to wait for the
        // 3-strikes reactive path.
        if self.last_health_at.elapsed() >= HEARTBEAT_INTERVAL {
            let ok = Self::check_health(&self.client, &self.base_url, &self.expected_model);
            self.last_health_at = Instant::now();
            if !ok {
                self.healthy = false;
                tracing::warn!("Remote GPU bridge failed lazy heartbeat; marking unhealthy");
                return vec![None; n];
            }
        }

        match self.do_embed_batch(faces) {
            Ok(embeddings) => {
                self.consecutive_failures = 0;
                self.last_health_at = Instant::now();
                embeddings
            }
            Err(e) => {
                self.consecutive_failures += 1;
                tracing::warn!(
                    "Remote embed batch failed ({}/3): {}",
                    self.consecutive_failures,
                    e
                );
                if self.consecutive_failures >= 3 {
                    self.healthy = false;
                    tracing::error!(
                        "Remote GPU bridge marked unhealthy after {} consecutive failures",
                        self.consecutive_failures
                    );
                }
                vec![None; n]
            }
        }
    }

    pub fn embed(&mut self, face: &RgbImage) -> Option<FaceEmbedding> {
        self.embed_batch(std::slice::from_ref(face))
            .into_iter()
            .next()
            .flatten()
    }

    fn do_embed_batch(&mut self, faces: &[RgbImage]) -> Result<Vec<Option<FaceEmbedding>>, String> {
        let mut form = reqwest::blocking::multipart::Form::new();
        for (i, face) in faces.iter().enumerate() {
            let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
            {
                let mut encoder =
                    JpegEncoder::new_with_quality(Cursor::new(&mut buf), JPEG_QUALITY);
                encoder
                    .encode(
                        face.as_raw(),
                        face.width(),
                        face.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| format!("JPEG encode face[{}]: {}", i, e))?;
            }
            let part = reqwest::blocking::multipart::Part::bytes(buf)
                .file_name(format!("{}.jpg", i))
                .mime_str("image/jpeg")
                .map_err(|e| format!("mime error: {}", e))?;
            // FastAPI's `files: list[UploadFile] = File(...)` expects
            // repeated multipart fields with the same name.
            form = form.part("files", part);
        }

        let resp = self
            .client
            .post(format!("{}/embed", self.base_url))
            .multipart(form)
            .send()
            .map_err(|e| format!("POST failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("server returned {}", resp.status()));
        }

        let json: serde_json::Value = resp.json().map_err(|e| format!("JSON parse: {}", e))?;

        let arr = json
            .get("embeddings")
            .and_then(|v: &serde_json::Value| v.as_array())
            .ok_or_else(|| "missing 'embeddings' array in response".to_string())?;

        let mut results: Vec<Option<FaceEmbedding>> = Vec::with_capacity(faces.len());
        for item in arr.iter() {
            let item: &serde_json::Value = item;
            let vec: Vec<f32> = item
                .as_array()
                .ok_or_else(|| "embedding entry is not an array".to_string())?
                .iter()
                .map(|v: &serde_json::Value| {
                    v.as_f64()
                        .map(|f| f as f32)
                        .ok_or_else(|| "non-f32 value".to_string())
                })
                .collect::<Result<Vec<f32>, String>>()?;
            if vec.len() != 512 {
                return Err(format!("expected 512-d embedding, got {}-d", vec.len()));
            }
            let emb = FaceEmbedding::new(ndarray::Array1::from_vec(vec));
            results.push(Some(emb));
        }

        // If the server returned fewer embeddings than faces, pad with
        // None. In practice they should always match, but be defensive.
        while results.len() < faces.len() {
            results.push(None);
        }

        Ok(results)
    }
}

/// Normalize and compare model names. Both sides are case-insensitive
/// and `.onnx` suffix is optional — so `glintr100`, `glintr100.onnx`,
/// `GLINTR100.onnx` all match.
fn models_match(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim()
            .strip_suffix(".onnx")
            .unwrap_or(s.trim())
            .to_ascii_lowercase()
    }
    !a.is_empty() && !b.is_empty() && norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_match_handles_onnx_suffix_and_case() {
        assert!(models_match("glintr100", "glintr100.onnx"));
        assert!(models_match("GLINTR100.onnx", "glintr100"));
        assert!(models_match(
            "adaface_ir101_webface12m",
            "adaface_ir101_webface12m.onnx"
        ));
        assert!(!models_match("glintr100", "adaface_ir101_webface12m"));
        assert!(!models_match("", "glintr100"));
    }
}
