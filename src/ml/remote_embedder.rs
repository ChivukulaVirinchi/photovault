//! Remote GPU bridge embedder.
//!
//! Sends face crops via HTTP multipart to a Colab/Kaggle-hosted GPU
//! inference server. Falls back to local ONNX when the bridge is
//! unreachable or returns errors.

use image::RgbImage;

use super::face_embedder::FaceEmbedding;

pub struct RemoteEmbedder {
    client: reqwest::blocking::Client,
    base_url: String,
    healthy: bool,
    consecutive_failures: u32,
}

impl RemoteEmbedder {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        let healthy = Self::check_health(&client, &base_url);
        if !healthy {
            tracing::warn!(
                "Remote GPU bridge at {} is not healthy; will use local fallback",
                base_url
            );
        }
        Self {
            client,
            base_url,
            healthy,
            consecutive_failures: 0,
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    fn check_health(client: &reqwest::blocking::Client, base_url: &str) -> bool {
        match client
            .get(format!("{}/health", base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(body) = resp.json::<serde_json::Value>() {
                        let has_gpu = body
                            .get("provider")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| s.contains("CUDA") || s.contains("GPU"));
                        return has_gpu;
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    pub fn embed_batch(&mut self, faces: &[RgbImage]) -> Vec<Option<FaceEmbedding>> {
        let n = faces.len();
        if n == 0 {
            return Vec::new();
        }

        if !self.healthy {
            return vec![None; n];
        }

        match self.do_embed_batch(faces) {
            Ok(embeddings) => {
                self.consecutive_failures = 0;
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
            let mut buf = std::io::Cursor::new(Vec::new());
            let dyn_img = image::DynamicImage::ImageRgb8(face.clone());
            dyn_img
                .write_to(&mut buf, image::ImageFormat::Jpeg)
                .map_err(|e| format!("JPEG encode face[{}]: {}", i, e))?;
            let bytes = buf.into_inner();
            let part = reqwest::blocking::multipart::Part::bytes(bytes)
                .file_name(format!("{}.jpg", i))
                .mime_str("image/jpeg")
                .map_err(|e| format!("mime error: {}", e))?;
            form = form.part(format!("face_{}", i), part);
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
