//! Semantic image search over local CLIP-style embeddings.
//!
//! The database stores only indexing state and vector offsets. The
//! high-volume embedding payload lives in `.photovault/semantic/...`
//! beside thumbnails and other per-library cache data.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use image::{DynamicImage, ImageBuffer, Rgb};
use ndarray::Array1;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

use crate::db::connection::library_metadata_dir;
use crate::ml::OnnxRuntime;
use crate::services::image_io;
use crate::services::path_util::safe_join_relative;

pub const SEMANTIC_MODEL_KEY: &str = "immich-app/ViT-B-32-SigLIP2-256__webli";
pub const SEMANTIC_MODEL_DISPLAY: &str = "ViT-B-32 SigLIP2 256";
pub const SEMANTIC_MODEL_REVISION: &str = "762c736d366fc253e9453021144f9fe71789b075";
pub const SEMANTIC_DIM: usize = 768;
pub const SEMANTIC_CONTEXT_LEN: usize = 64;
pub const SEMANTIC_TEXT_SEARCH_LIMIT: usize = 250;
pub const SEMANTIC_TEXT_RESULT_CAP: usize = 80;

const SEMANTIC_TEXT_MIN_SCORE: f32 = 0.06;
const SEMANTIC_TEXT_MAX_SCORE_DROP: f32 = 0.02;
const SEMANTIC_TEXT_MIN_SCORE_RATIO: f32 = 0.75;

const MODEL_DIR_NAME: &str = "vit-b-32-siglip2-256-webli";
const VECTOR_FILE: &str = "vectors.f32";
const MANIFEST_FILE: &str = "manifest.json";

const VISUAL_MODEL_URL: &str =
    "https://huggingface.co/immich-app/ViT-B-32-SigLIP2-256__webli/resolve/main/visual/model.onnx";
const TEXTUAL_MODEL_URL: &str =
    "https://huggingface.co/immich-app/ViT-B-32-SigLIP2-256__webli/resolve/main/textual/model.onnx";
const TOKENIZER_URL: &str = "https://huggingface.co/immich-app/ViT-B-32-SigLIP2-256__webli/resolve/main/textual/tokenizer.json";
const PREPROCESS_URL: &str = "https://huggingface.co/immich-app/ViT-B-32-SigLIP2-256__webli/resolve/main/visual/preprocess_cfg.json";
const CONFIG_URL: &str =
    "https://huggingface.co/immich-app/ViT-B-32-SigLIP2-256__webli/resolve/main/config.json";

const VISUAL_MODEL_BYTES: u64 = 378_359_772;
const TEXTUAL_MODEL_BYTES: u64 = 1_129_435_819;
const TOKENIZER_BYTES: u64 = 34_362_885;
const PREPROCESS_BYTES: u64 = 154;
const CONFIG_BYTES: u64 = 551;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticStatus {
    pub model_key: String,
    pub display_name: String,
    pub model_dir: String,
    pub assets_installed: bool,
    pub onnx_runtime_installed: bool,
    pub indexed_photos: u64,
    pub pending_photos: u64,
    pub failed_photos: u64,
    pub vector_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SemanticIndexStats {
    pub indexed: u64,
    pub pending: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SemanticIndexBatchOutcome {
    pub processed: u64,
    pub indexed: u64,
    pub failed: u64,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub photo_id: i64,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorManifest {
    model_key: String,
    revision: String,
    dim: usize,
    vector_count: u64,
}

#[derive(Debug, Clone)]
struct SemanticAssetPaths {
    root: PathBuf,
    visual_model: PathBuf,
    textual_model: PathBuf,
    tokenizer: PathBuf,
    preprocess: PathBuf,
    config: PathBuf,
}

impl SemanticAssetPaths {
    fn in_root(root: PathBuf) -> Self {
        let model_root = root.join("models").join("semantic").join(MODEL_DIR_NAME);
        Self {
            visual_model: model_root.join("visual").join("model.onnx"),
            textual_model: model_root.join("textual").join("model.onnx"),
            tokenizer: model_root.join("textual").join("tokenizer.json"),
            preprocess: model_root.join("visual").join("preprocess_cfg.json"),
            config: model_root.join("config.json"),
            root: model_root,
        }
    }

    fn installed(&self) -> bool {
        self.visual_model.exists()
            && self.textual_model.exists()
            && self.tokenizer.exists()
            && self.preprocess.exists()
            && self.config.exists()
    }
}

pub struct SemanticSearchService {
    drive_root: PathBuf,
}

#[derive(Default)]
pub struct SemanticIndexCache {
    indexed_count: u64,
    #[cfg(feature = "hnsw_clustering")]
    index: Option<SemanticHnswIndex>,
}

#[cfg(feature = "hnsw_clustering")]
struct SemanticHnswIndex {
    photo_ids: Vec<i64>,
    hnsw: hnsw_rs::prelude::Hnsw<'static, f32, hnsw_rs::prelude::DistCosine>,
}

#[cfg(feature = "hnsw_clustering")]
impl SemanticHnswIndex {
    fn search(&self, query: &[f32], limit: usize) -> Vec<SemanticCandidate> {
        if self.photo_ids.is_empty() || limit == 0 {
            return Vec::new();
        }
        self.hnsw
            .search(query, limit.min(self.photo_ids.len()).max(1), 200)
            .into_iter()
            .filter_map(|nb| {
                self.photo_ids
                    .get(nb.d_id)
                    .map(|photo_id| SemanticCandidate {
                        photo_id: *photo_id,
                        score: (1.0 - nb.distance).clamp(-1.0, 1.0),
                    })
            })
            .collect()
    }
}

impl SemanticSearchService {
    pub fn new(drive_root: impl Into<PathBuf>) -> Self {
        Self {
            drive_root: drive_root.into(),
        }
    }

    pub fn status(&self, conn: &Connection) -> rusqlite::Result<SemanticStatus> {
        let stats = self.index_stats(conn)?;
        let store = VectorStore::new(&self.drive_root)?;
        let assets = Self::find_assets();
        Ok(SemanticStatus {
            model_key: SEMANTIC_MODEL_KEY.to_string(),
            display_name: SEMANTIC_MODEL_DISPLAY.to_string(),
            model_dir: assets
                .as_ref()
                .map(|a| a.root.display().to_string())
                .unwrap_or_else(|| Self::default_asset_paths().root.display().to_string()),
            assets_installed: assets.as_ref().is_some_and(SemanticAssetPaths::installed),
            onnx_runtime_installed: crate::bootstrap::onnx_runtime_exists(),
            indexed_photos: stats.indexed,
            pending_photos: stats.pending,
            failed_photos: stats.failed,
            vector_bytes: std::fs::metadata(store.vector_path())
                .map(|m| m.len())
                .unwrap_or(0),
        })
    }

    pub async fn install_model_assets<F>(
        cancel: Option<&AtomicBool>,
        mut progress: F,
    ) -> Result<(), String>
    where
        F: FnMut(&str, u64, Option<u64>) + Send,
    {
        let paths = Self::default_asset_paths();
        let assets = [
            SemanticDownload {
                url: VISUAL_MODEL_URL,
                destination: paths.visual_model,
                stage: "visual-model",
                expected_size: VISUAL_MODEL_BYTES,
            },
            SemanticDownload {
                url: TEXTUAL_MODEL_URL,
                destination: paths.textual_model,
                stage: "text-model",
                expected_size: TEXTUAL_MODEL_BYTES,
            },
            SemanticDownload {
                url: TOKENIZER_URL,
                destination: paths.tokenizer,
                stage: "tokenizer",
                expected_size: TOKENIZER_BYTES,
            },
            SemanticDownload {
                url: PREPROCESS_URL,
                destination: paths.preprocess,
                stage: "preprocess",
                expected_size: PREPROCESS_BYTES,
            },
            SemanticDownload {
                url: CONFIG_URL,
                destination: paths.config,
                stage: "config",
                expected_size: CONFIG_BYTES,
            },
        ];
        let total = assets.iter().map(|a| a.expected_size).sum::<u64>();
        let mut completed = 0;
        for asset in assets {
            completed = download_asset(asset, completed, total, cancel, &mut progress).await?;
        }
        Ok(())
    }

    pub fn index_stats(&self, conn: &Connection) -> rusqlite::Result<SemanticIndexStats> {
        let indexed = count_state(conn, "indexed")?;
        let failed = count_state(conn, "failed")?;
        let total_active: u64 = conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = FALSE",
            [],
            |r| r.get::<_, i64>(0),
        )? as u64;
        let pending = total_active.saturating_sub(indexed + failed);
        Ok(SemanticIndexStats {
            indexed,
            pending,
            failed,
        })
    }

    pub fn next_pending_batch(
        &self,
        conn: &Connection,
        limit: usize,
    ) -> rusqlite::Result<Vec<SemanticPhotoInput>> {
        let mut stmt = conn.prepare(
            "SELECT p.id, p.file_path, p.thumbnail_path, p.media_type
             FROM photos p
             LEFT JOIN semantic_index_state s
               ON s.photo_id = p.id AND s.model_key = ?1
             WHERE p.is_trashed = FALSE
               AND COALESCE(s.status, 'pending') = 'pending'
             ORDER BY p.date_taken IS NULL ASC, p.date_taken DESC, p.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![SEMANTIC_MODEL_KEY, limit as i64], |row| {
            Ok(SemanticPhotoInput {
                photo_id: row.get(0)?,
                file_path: row.get(1)?,
                thumbnail_path: row.get(2)?,
                media_type: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    pub fn mark_failed(conn: &Connection, photo_id: i64, error: &str) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO semantic_index_state
                (photo_id, model_key, status, attempts, last_error)
             VALUES (?1, ?2, 'failed', 1, ?3)
             ON CONFLICT(photo_id, model_key) DO UPDATE SET
                status = 'failed',
                attempts = attempts + 1,
                last_error = excluded.last_error,
                updated_at = CURRENT_TIMESTAMP",
            params![photo_id, SEMANTIC_MODEL_KEY, truncate_error(error)],
        )?;
        Ok(())
    }

    pub fn mark_indexed(
        &self,
        conn: &mut Connection,
        photo_id: i64,
        vector: &[f32],
    ) -> rusqlite::Result<()> {
        self.record_index_batch(conn, &[(photo_id, vector.to_vec())], &[])
    }

    pub fn index_next_batch(
        &self,
        conn: &mut Connection,
        runner: &mut SemanticImageRunner,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<SemanticIndexBatchOutcome, String> {
        let batch = self
            .next_pending_batch(conn, limit)
            .map_err(|e| e.to_string())?;
        if batch.is_empty() {
            return Ok(SemanticIndexBatchOutcome {
                done: true,
                ..Default::default()
            });
        }

        let mut indexed = Vec::new();
        let mut failed = Vec::new();
        for photo in &batch {
            if cancel.load(Ordering::Relaxed) {
                return Err("Semantic indexing cancelled".into());
            }
            match photo
                .source_path(&self.drive_root)
                .and_then(|path| runner.embed_image_path(&path))
            {
                Ok(vector) => indexed.push((photo.photo_id, vector)),
                Err(err) => failed.push((photo.photo_id, err)),
            }
        }

        self.record_index_batch(conn, &indexed, &failed)
            .map_err(|e| e.to_string())?;

        Ok(SemanticIndexBatchOutcome {
            processed: batch.len() as u64,
            indexed: indexed.len() as u64,
            failed: failed.len() as u64,
            done: false,
        })
    }

    fn record_index_batch(
        &self,
        conn: &mut Connection,
        indexed: &[(i64, Vec<f32>)],
        failed: &[(i64, String)],
    ) -> rusqlite::Result<()> {
        let offsets = if indexed.is_empty() {
            Vec::new()
        } else {
            let mut store = VectorStore::new(&self.drive_root)?;
            store.append_many(indexed.iter().map(|(_, vector)| vector.as_slice()))?
        };

        let tx = conn.transaction()?;
        for ((photo_id, vector), offset) in indexed.iter().zip(offsets.iter()) {
            tx.execute(
                "INSERT INTO semantic_index_state
                    (photo_id, model_key, status, vector_offset, vector_dim, attempts, last_error, indexed_at)
                 VALUES (?1, ?2, 'indexed', ?3, ?4, 0, NULL, CURRENT_TIMESTAMP)
                 ON CONFLICT(photo_id, model_key) DO UPDATE SET
                    status = 'indexed',
                    vector_offset = excluded.vector_offset,
                    vector_dim = excluded.vector_dim,
                    attempts = 0,
                    last_error = NULL,
                    indexed_at = CURRENT_TIMESTAMP,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    photo_id,
                    SEMANTIC_MODEL_KEY,
                    *offset as i64,
                    vector.len() as i64
                ],
            )?;
        }
        for (photo_id, err) in failed {
            tx.execute(
                "INSERT INTO semantic_index_state
                    (photo_id, model_key, status, attempts, last_error)
                 VALUES (?1, ?2, 'failed', 1, ?3)
                 ON CONFLICT(photo_id, model_key) DO UPDATE SET
                    status = 'failed',
                    attempts = attempts + 1,
                    last_error = excluded.last_error,
                    updated_at = CURRENT_TIMESTAMP",
                params![photo_id, SEMANTIC_MODEL_KEY, truncate_error(err)],
            )?;
        }
        tx.commit()
    }

    pub fn search_text(
        &self,
        conn: &Connection,
        runner: &mut SemanticModelRunner,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SemanticCandidate>, String> {
        let vector = runner.embed_text(query)?;
        self.search_vector(conn, &vector, limit)
    }

    pub fn search_text_cached(
        &self,
        conn: &Connection,
        cache: &mut SemanticIndexCache,
        runner: &mut SemanticModelRunner,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SemanticCandidate>, String> {
        let vector = runner.embed_text(query)?;
        self.search_vector_cached(conn, cache, &vector, limit)
    }

    pub fn similar_to_photo(
        &self,
        conn: &Connection,
        photo_id: i64,
        limit: usize,
    ) -> Result<Vec<SemanticCandidate>, String> {
        let Some(vector) = self.vector_for_photo(conn, photo_id)? else {
            return Ok(Vec::new());
        };
        let mut out = self.search_vector(conn, &vector, limit + 1)?;
        out.retain(|c| c.photo_id != photo_id);
        out.truncate(limit);
        Ok(out)
    }

    pub fn similar_to_photo_cached(
        &self,
        conn: &Connection,
        cache: &mut SemanticIndexCache,
        photo_id: i64,
        limit: usize,
    ) -> Result<Vec<SemanticCandidate>, String> {
        let Some(vector) = self.vector_for_photo(conn, photo_id)? else {
            return Ok(Vec::new());
        };
        let mut out = self.search_vector_cached(conn, cache, &vector, limit + 1)?;
        out.retain(|c| c.photo_id != photo_id);
        out.truncate(limit);
        Ok(out)
    }

    pub fn search_vector_cached(
        &self,
        conn: &Connection,
        cache: &mut SemanticIndexCache,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SemanticCandidate>, String> {
        #[cfg(not(feature = "hnsw_clustering"))]
        {
            let _ = (conn, cache, query, limit);
            return Err("HNSW semantic search requires the hnsw_clustering feature".into());
        }

        #[cfg(feature = "hnsw_clustering")]
        {
            if query.len() != SEMANTIC_DIM {
                return Ok(Vec::new());
            }
            let indexed_count = self.index_stats(conn).map_err(|e| e.to_string())?.indexed;
            if cache.index.is_none() || cache.indexed_count != indexed_count {
                cache.index = Some(self.build_hnsw_index(conn)?);
                cache.indexed_count = indexed_count;
            }
            Ok(cache
                .index
                .as_ref()
                .map(|idx| idx.search(query, limit))
                .unwrap_or_default())
        }
    }

    pub fn search_vector(
        &self,
        conn: &Connection,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SemanticCandidate>, String> {
        #[cfg(not(feature = "hnsw_clustering"))]
        {
            let _ = (conn, query, limit);
            return Err("HNSW semantic search requires the hnsw_clustering feature".into());
        }

        #[cfg(feature = "hnsw_clustering")]
        {
            if query.len() != SEMANTIC_DIM {
                return Ok(Vec::new());
            }
            let index = self.build_hnsw_index(conn)?;
            Ok(index.search(query, limit))
        }
    }

    #[cfg(feature = "hnsw_clustering")]
    fn build_hnsw_index(&self, conn: &Connection) -> Result<SemanticHnswIndex, String> {
        use hnsw_rs::prelude::*;

        let rows = self.load_index_rows(conn).map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(SemanticHnswIndex {
                photo_ids: Vec::new(),
                hnsw: Hnsw::new(16, 1, 1, 200, DistCosine {}),
            });
        }

        let hnsw: Hnsw<f32, DistCosine> = Hnsw::new(
            16,
            rows.len(),
            16.min(rows.len().max(1)),
            200,
            DistCosine {},
        );
        let data: Vec<(&[f32], usize)> = rows
            .iter()
            .enumerate()
            .map(|(idx, row)| (row.vector.as_slice(), idx))
            .collect();
        hnsw.parallel_insert_slice(&data);
        let photo_ids = rows.into_iter().map(|row| row.photo_id).collect();
        Ok(SemanticHnswIndex { photo_ids, hnsw })
    }

    fn vector_for_photo(
        &self,
        conn: &Connection,
        photo_id: i64,
    ) -> Result<Option<Vec<f32>>, String> {
        let row: Option<(i64, i64)> = conn
            .query_row(
                "SELECT vector_offset, vector_dim
                 FROM semantic_index_state
                 WHERE photo_id = ?1 AND model_key = ?2 AND status = 'indexed'",
                params![photo_id, SEMANTIC_MODEL_KEY],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some((offset, dim)) = row else {
            return Ok(None);
        };
        if dim != SEMANTIC_DIM as i64 || offset < 0 {
            return Ok(None);
        }
        let store = VectorStore::new(&self.drive_root).map_err(|e| e.to_string())?;
        store
            .read(offset as u64, dim as usize)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    fn load_index_rows(&self, conn: &Connection) -> rusqlite::Result<Vec<IndexRow>> {
        let mut stmt = conn.prepare(
            "SELECT s.photo_id, s.vector_offset, s.vector_dim
             FROM semantic_index_state s
             JOIN photos p ON p.id = s.photo_id
             WHERE s.model_key = ?1
               AND s.status = 'indexed'
               AND s.vector_dim = ?2
               AND p.is_trashed = FALSE",
        )?;
        let rows = stmt.query_map(params![SEMANTIC_MODEL_KEY, SEMANTIC_DIM as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let store = VectorStore::new(&self.drive_root)?;
        let mut out = Vec::new();
        for row in rows {
            let (photo_id, offset, dim) = row?;
            if let Ok(vector) = store.read(offset as u64, dim as usize) {
                out.push(IndexRow { photo_id, vector });
            }
        }
        Ok(out)
    }

    fn find_assets() -> Option<SemanticAssetPaths> {
        crate::bootstrap::asset_roots()
            .into_iter()
            .map(SemanticAssetPaths::in_root)
            .find(SemanticAssetPaths::installed)
    }

    pub fn image_runner() -> Result<SemanticImageRunner, String> {
        let paths = Self::find_assets().ok_or_else(|| {
            format!(
                "Semantic search model is not installed. Install {} from Settings.",
                SEMANTIC_MODEL_DISPLAY
            )
        })?;
        if !crate::bootstrap::onnx_runtime_exists() {
            return Err(
                "ONNX Runtime is missing. Use Settings -> Assets -> Download assets before indexing visual search."
                    .into(),
            );
        }
        let rt = OnnxRuntime::init().map_err(|e| e.to_string())?;
        SemanticImageRunner::new(&rt, paths)
    }

    pub fn model_runner() -> Result<SemanticModelRunner, String> {
        let paths = Self::find_assets().ok_or_else(|| {
            format!(
                "Semantic search model is not installed. Install {} from Settings.",
                SEMANTIC_MODEL_DISPLAY
            )
        })?;
        if !crate::bootstrap::onnx_runtime_exists() {
            return Err(
                "ONNX Runtime is missing. Use Settings -> Assets -> Download assets before indexing visual search."
                    .into(),
            );
        }
        let rt = OnnxRuntime::init().map_err(|e| e.to_string())?;
        SemanticModelRunner::new(&rt, paths)
    }

    fn default_asset_paths() -> SemanticAssetPaths {
        SemanticAssetPaths::in_root(crate::bootstrap::default_asset_install_dir())
    }
}

#[derive(Debug, Clone)]
pub struct SemanticPhotoInput {
    pub photo_id: i64,
    pub file_path: String,
    pub thumbnail_path: Option<String>,
    pub media_type: String,
}

impl SemanticPhotoInput {
    pub fn source_path(&self, drive_root: &Path) -> Result<PathBuf, String> {
        if let Some(thumbnail) = &self.thumbnail_path {
            match safe_join_relative(drive_root, thumbnail) {
                Ok(path) if path.exists() => return Ok(path),
                Ok(_) if self.media_type == "video" => {
                    return Err("video poster thumbnail is not ready".into());
                }
                Err(e) if self.media_type == "video" => {
                    return Err(format!("invalid video thumbnail path: {e}"));
                }
                _ => {}
            }
        }
        if self.media_type == "video" {
            return Err("video poster thumbnail is not ready".into());
        }
        safe_join_relative(drive_root, &self.file_path)
            .map_err(|e| format!("invalid photo path: {e}"))
    }
}

struct IndexRow {
    photo_id: i64,
    vector: Vec<f32>,
}

pub struct SemanticImageRunner {
    visual: ort::session::Session,
}

impl SemanticImageRunner {
    fn new(rt: &OnnxRuntime, paths: SemanticAssetPaths) -> Result<Self, String> {
        let visual = rt
            .load_model_with_threads(&paths.visual_model, 1)
            .map_err(|e| format!("visual model load failed: {e}"))?;
        Ok(Self { visual })
    }

    pub fn embed_image_path(&mut self, path: &Path) -> Result<Vec<f32>, String> {
        let img = image_io::open_image(path)?;
        self.embed_image(&img)
    }

    pub fn embed_image(&mut self, img: &DynamicImage) -> Result<Vec<f32>, String> {
        let tensor = preprocess_image(img);
        let input = ort::value::TensorRef::<f32>::from_array_view((
            vec![1, 3, 256, 256],
            tensor.as_slice(),
        ))
        .map_err(|e| e.to_string())?;
        let outputs = self
            .visual
            .run(ort::inputs![input])
            .map_err(|e| format!("visual inference failed: {e}"))?;
        extract_normalized_output(outputs)
    }
}

pub struct SemanticModelRunner {
    textual: ort::session::Session,
    tokenizer: Tokenizer,
}

impl SemanticModelRunner {
    fn new(rt: &OnnxRuntime, paths: SemanticAssetPaths) -> Result<Self, String> {
        let textual = rt
            .load_model_with_threads(&paths.textual_model, 1)
            .map_err(|e| format!("text model load failed: {e}"))?;
        let tokenizer = Tokenizer::from_file(&paths.tokenizer)
            .map_err(|e| format!("tokenizer load failed: {e}"))?;
        Ok(Self { textual, tokenizer })
    }

    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("tokenization failed: {e}"))?;
        let ids = padded_text_context(encoding.get_ids());
        let input_ids = ort::value::TensorRef::<i32>::from_array_view((
            vec![1, SEMANTIC_CONTEXT_LEN as i64],
            ids.as_slice(),
        ))
        .map_err(|e| e.to_string())?;
        let outputs = self
            .textual
            .run(ort::inputs![input_ids])
            .map_err(|e| format!("text inference failed: {e}"))?;
        extract_normalized_output(outputs)
    }
}

fn padded_text_context(token_ids: &[u32]) -> Vec<i32> {
    let mut ids = vec![0i32; SEMANTIC_CONTEXT_LEN];
    for (idx, id) in token_ids.iter().take(SEMANTIC_CONTEXT_LEN).enumerate() {
        ids[idx] = *id as i32;
    }
    ids
}

fn preprocess_image(img: &DynamicImage) -> Vec<f32> {
    let resized = img.resize_exact(256, 256, image::imageops::FilterType::CatmullRom);
    let rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = resized.to_rgb8();
    let mut out = vec![0.0f32; 3 * 256 * 256];
    let hw = 256 * 256;
    for y in 0..256u32 {
        for x in 0..256u32 {
            let p = rgb.get_pixel(x, y);
            let idx = (y * 256 + x) as usize;
            out[idx] = (p[0] as f32 / 255.0 - 0.5) / 0.5;
            out[hw + idx] = (p[1] as f32 / 255.0 - 0.5) / 0.5;
            out[2 * hw + idx] = (p[2] as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    out
}

fn extract_normalized_output(outputs: ort::session::SessionOutputs) -> Result<Vec<f32>, String> {
    let (_name, output) = outputs
        .iter()
        .next()
        .ok_or_else(|| "model produced no output tensor".to_string())?;
    let (_shape, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let mut vector = data.to_vec();
    if vector.len() != SEMANTIC_DIM {
        return Err(format!(
            "unexpected semantic embedding dimension: expected {}, got {}",
            SEMANTIC_DIM,
            vector.len()
        ));
    }
    normalize_in_place(&mut vector);
    Ok(vector)
}

fn normalize_in_place(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v {
            *x /= norm;
        }
    }
}

struct VectorStore {
    root: PathBuf,
}

impl VectorStore {
    fn new(drive_root: &Path) -> rusqlite::Result<Self> {
        let root = library_metadata_dir(drive_root)
            .join("semantic")
            .join(MODEL_DIR_NAME);
        std::fs::create_dir_all(&root)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let manifest = root.join(MANIFEST_FILE);
        if !manifest.exists() {
            let data = serde_json::to_vec_pretty(&VectorManifest {
                model_key: SEMANTIC_MODEL_KEY.to_string(),
                revision: SEMANTIC_MODEL_REVISION.to_string(),
                dim: SEMANTIC_DIM,
                vector_count: 0,
            })
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            std::fs::write(&manifest, data)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        }
        Ok(Self { root })
    }

    fn vector_path(&self) -> PathBuf {
        self.root.join(VECTOR_FILE)
    }

    fn append_many<'a, I>(&mut self, vectors: I) -> rusqlite::Result<Vec<u64>>
    where
        I: IntoIterator<Item = &'a [f32]>,
    {
        let vectors = vectors.into_iter().collect::<Vec<_>>();
        if vectors.is_empty() {
            return Ok(Vec::new());
        }
        for vector in &vectors {
            if vector.len() != SEMANTIC_DIM {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "semantic vector dimension {} != {}",
                    vector.len(),
                    SEMANTIC_DIM
                )));
            }
        }
        let path = self.vector_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let mut offset = file
            .seek(SeekFrom::End(0))
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let mut offsets = Vec::with_capacity(vectors.len());
        for vector in vectors {
            offsets.push(offset);
            for value in vector {
                file.write_all(&value.to_le_bytes())
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            }
            offset += (SEMANTIC_DIM * 4) as u64;
        }
        self.bump_manifest_by(offsets.len() as u64)?;
        Ok(offsets)
    }

    fn read(&self, offset: u64, dim: usize) -> std::io::Result<Vec<f32>> {
        let mut file = File::open(self.vector_path())?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0u8; dim * 4];
        file.read_exact(&mut bytes)?;
        Ok(bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    fn bump_manifest_by(&self, count: u64) -> rusqlite::Result<()> {
        let path = self.root.join(MANIFEST_FILE);
        let mut manifest: VectorManifest = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(VectorManifest {
                model_key: SEMANTIC_MODEL_KEY.to_string(),
                revision: SEMANTIC_MODEL_REVISION.to_string(),
                dim: SEMANTIC_DIM,
                vector_count: 0,
            });
        manifest.vector_count += count;
        let data = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        std::fs::write(path, data)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok(())
    }
}

fn count_state(conn: &Connection, status: &str) -> rusqlite::Result<u64> {
    conn.query_row(
        "SELECT COUNT(*)
         FROM semantic_index_state s
         JOIN photos p ON p.id = s.photo_id
         WHERE s.model_key = ?1
           AND s.status = ?2
           AND p.is_trashed = FALSE",
        params![SEMANTIC_MODEL_KEY, status],
        |r| r.get::<_, i64>(0),
    )
    .map(|v| v as u64)
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

struct SemanticDownload {
    url: &'static str,
    stage: &'static str,
    destination: PathBuf,
    expected_size: u64,
}

async fn download_asset<F>(
    asset: SemanticDownload,
    completed_before: u64,
    total_bytes: u64,
    cancel: Option<&AtomicBool>,
    progress: &mut F,
) -> Result<u64, String>
where
    F: FnMut(&str, u64, Option<u64>) + Send,
{
    if asset.destination.exists() {
        let completed = completed_before + asset.expected_size;
        progress(asset.stage, completed, Some(total_bytes));
        return Ok(completed);
    }
    if let Some(parent) = asset.destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed creating {}: {e}", parent.display()))?;
    }
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err("Semantic model install cancelled".into());
    }

    let response = reqwest::get(asset.url)
        .await
        .map_err(|e| format!("download request failed for {}: {e}", asset.url))?;
    if !response.status().is_success() {
        return Err(format!(
            "download failed for {}: HTTP {}",
            asset.url,
            response.status()
        ));
    }
    let expected = response.content_length().unwrap_or(asset.expected_size);
    let tmp = asset.destination.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("failed writing {}: {e}", tmp.display()))?;
    let mut downloaded = 0u64;
    let mut last_emit = 0u64;
    let mut stream = response.bytes_stream();

    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err("Semantic model install cancelled".into());
        }
        let chunk = chunk.map_err(|e| format!("download body failed for {}: {e}", asset.url))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| format!("failed writing {}: {e}", tmp.display()))?;
        downloaded += chunk.len() as u64;
        if downloaded.saturating_sub(last_emit) >= 1_048_576 || downloaded >= expected {
            last_emit = downloaded;
            progress(
                asset.stage,
                completed_before + downloaded.min(asset.expected_size),
                Some(total_bytes),
            );
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|e| format!("failed flushing {}: {e}", tmp.display()))?;
    drop(file);
    tokio::fs::rename(&tmp, &asset.destination)
        .await
        .map_err(|e| format!("failed moving {}: {e}", asset.destination.display()))?;
    let completed = completed_before + asset.expected_size;
    progress(asset.stage, completed, Some(total_bytes));
    Ok(completed)
}
pub fn semantic_ids_by_score(candidates: &[SemanticCandidate]) -> HashMap<i64, usize> {
    candidates
        .iter()
        .enumerate()
        .map(|(idx, c)| (c.photo_id, idx))
        .collect()
}

pub fn relevant_text_search_candidates(
    mut candidates: Vec<SemanticCandidate>,
) -> Vec<SemanticCandidate> {
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let Some(top) = candidates.first().map(|c| c.score) else {
        return Vec::new();
    };
    if top < SEMANTIC_TEXT_MIN_SCORE {
        return Vec::new();
    }

    let threshold = SEMANTIC_TEXT_MIN_SCORE
        .max(top - SEMANTIC_TEXT_MAX_SCORE_DROP)
        .max(top * SEMANTIC_TEXT_MIN_SCORE_RATIO);
    candidates
        .into_iter()
        .filter(|c| c.score >= threshold)
        .take(SEMANTIC_TEXT_RESULT_CAP)
        .collect()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let av = Array1::from_vec(a.to_vec());
    let bv = Array1::from_vec(b.to_vec());
    let dot = av.dot(&bv);
    let na = av.dot(&av).sqrt();
    let nb = bv.dot(&bv).sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_semantic_test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                thumbnail_path TEXT,
                media_type TEXT NOT NULL DEFAULT 'photo',
                date_taken TEXT,
                is_trashed BOOLEAN NOT NULL DEFAULT FALSE
            );
            CREATE TABLE semantic_index_state (
                photo_id INTEGER NOT NULL,
                model_key TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                vector_offset INTEGER,
                vector_dim INTEGER,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                indexed_at TEXT,
                updated_at TEXT,
                PRIMARY KEY(photo_id, model_key)
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn vector_store_round_trips_fixed_width_vectors() {
        let dir = tempdir().unwrap();
        let mut store = VectorStore::new(dir.path()).unwrap();
        let mut first = vec![0.0f32; SEMANTIC_DIM];
        first[3] = 1.0;
        let mut second = vec![0.0f32; SEMANTIC_DIM];
        second[9] = 1.0;

        let offsets = store
            .append_many([first.as_slice(), second.as_slice()])
            .unwrap();
        let off_a = offsets[0];
        let off_b = offsets[1];

        assert_eq!(off_a, 0);
        assert_eq!(off_b, (SEMANTIC_DIM * 4) as u64);
        assert_eq!(store.read(off_a, SEMANTIC_DIM).unwrap(), first);
        assert_eq!(store.read(off_b, SEMANTIC_DIM).unwrap(), second);
    }

    #[test]
    fn vector_for_photo_ignores_corrupt_vector_dimension() {
        let conn = setup_semantic_test_conn();
        conn.execute(
            "INSERT INTO photos (id, file_path, media_type, is_trashed) VALUES
                (1, 'a.jpg', 'photo', FALSE)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO semantic_index_state
                (photo_id, model_key, status, vector_offset, vector_dim)
             VALUES (?1, ?2, 'indexed', 0, 999999999)",
            rusqlite::params![1_i64, SEMANTIC_MODEL_KEY],
        )
        .unwrap();
        let svc = SemanticSearchService::new(tempdir().unwrap().path());

        assert!(svc.vector_for_photo(&conn, 1).unwrap().is_none());
    }

    #[test]
    fn pending_batch_does_not_retry_failed_rows() {
        let conn = setup_semantic_test_conn();
        conn.execute(
            "INSERT INTO photos (id, file_path, media_type, is_trashed) VALUES
                (1, 'a.jpg', 'photo', FALSE),
                (2, 'b.jpg', 'photo', FALSE)",
            [],
        )
        .unwrap();
        SemanticSearchService::mark_failed(&conn, 1, "bad image").unwrap();

        let svc = SemanticSearchService::new(tempdir().unwrap().path());
        let batch = svc.next_pending_batch(&conn, 10).unwrap();

        assert_eq!(
            batch.iter().map(|p| p.photo_id).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn index_stats_ignore_trashed_index_state_rows() {
        let conn = setup_semantic_test_conn();
        conn.execute(
            "INSERT INTO photos (id, file_path, media_type, is_trashed) VALUES
                (1, 'a.jpg', 'photo', FALSE),
                (2, 'b.jpg', 'photo', TRUE),
                (3, 'c.jpg', 'photo', FALSE)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO semantic_index_state
                (photo_id, model_key, status, vector_offset, vector_dim)
             VALUES
                (1, ?1, 'indexed', 0, ?2),
                (2, ?1, 'indexed', 0, ?2),
                (3, ?1, 'failed', NULL, NULL)",
            params![SEMANTIC_MODEL_KEY, SEMANTIC_DIM as i64],
        )
        .unwrap();

        let svc = SemanticSearchService::new(tempdir().unwrap().path());
        let stats = svc.index_stats(&conn).unwrap();

        assert_eq!(stats.indexed, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.pending, 0);
    }

    #[test]
    fn record_index_batch_persists_vectors_and_failures_once() {
        let mut conn = setup_semantic_test_conn();
        conn.execute(
            "INSERT INTO photos (id, file_path, media_type, is_trashed) VALUES
                (1, 'a.jpg', 'photo', FALSE),
                (2, 'b.jpg', 'photo', FALSE),
                (3, 'c.jpg', 'photo', FALSE)",
            [],
        )
        .unwrap();
        let dir = tempdir().unwrap();
        let svc = SemanticSearchService::new(dir.path());
        let mut first = vec![0.0f32; SEMANTIC_DIM];
        first[0] = 1.0;
        let mut second = vec![0.0f32; SEMANTIC_DIM];
        second[1] = 1.0;

        svc.record_index_batch(
            &mut conn,
            &[(1, first.clone()), (2, second.clone())],
            &[(3, "decode failed".into())],
        )
        .unwrap();

        let rows = conn
            .prepare(
                "SELECT photo_id, status, vector_offset, vector_dim, attempts, COALESCE(last_error, '')
                 FROM semantic_index_state
                 ORDER BY photo_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[0].1, "indexed");
        assert_eq!(rows[0].2, Some(0));
        assert_eq!(rows[0].3, Some(SEMANTIC_DIM as i64));
        assert_eq!(rows[1].0, 2);
        assert_eq!(rows[1].1, "indexed");
        assert_eq!(rows[1].2, Some((SEMANTIC_DIM * 4) as i64));
        assert_eq!(rows[2].0, 3);
        assert_eq!(rows[2].1, "failed");
        assert_eq!(rows[2].4, 1);
        assert_eq!(rows[2].5, "decode failed");

        let store = VectorStore::new(dir.path()).unwrap();
        assert_eq!(store.read(0, SEMANTIC_DIM).unwrap(), first);
        assert_eq!(
            store.read((SEMANTIC_DIM * 4) as u64, SEMANTIC_DIM).unwrap(),
            second
        );
        assert_eq!(
            std::fs::metadata(store.vector_path()).unwrap().len(),
            (2 * SEMANTIC_DIM * 4) as u64
        );
    }

    #[test]
    fn photo_source_prefers_existing_thumbnail() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".photovault/thumbs")).unwrap();
        std::fs::write(dir.path().join("photo.jpg"), b"original").unwrap();
        std::fs::write(dir.path().join(".photovault/thumbs/photo.jpg"), b"thumb").unwrap();
        let input = SemanticPhotoInput {
            photo_id: 1,
            file_path: "photo.jpg".into(),
            thumbnail_path: Some(".photovault/thumbs/photo.jpg".into()),
            media_type: "photo".into(),
        };

        assert_eq!(
            input.source_path(dir.path()).unwrap(),
            dir.path().join(".photovault/thumbs/photo.jpg")
        );
    }

    #[test]
    fn search_vector_returns_indexed_candidates() {
        let mut conn = setup_semantic_test_conn();
        conn.execute(
            "INSERT INTO photos (id, file_path, media_type, is_trashed) VALUES
                (1, 'a.jpg', 'photo', FALSE),
                (2, 'b.jpg', 'photo', FALSE),
                (3, 'c.jpg', 'photo', TRUE)",
            [],
        )
        .unwrap();
        let dir = tempdir().unwrap();
        let svc = SemanticSearchService::new(dir.path());
        let mut first = vec![0.0f32; SEMANTIC_DIM];
        first[0] = 1.0;
        let mut second = vec![0.0f32; SEMANTIC_DIM];
        second[1] = 1.0;
        let mut trashed = vec![0.0f32; SEMANTIC_DIM];
        trashed[0] = 1.0;

        svc.record_index_batch(
            &mut conn,
            &[(1, first.clone()), (2, second), (3, trashed)],
            &[],
        )
        .unwrap();

        let matches = svc.search_vector(&conn, &first, 5).unwrap();

        assert_eq!(matches.first().map(|c| c.photo_id), Some(1));
        assert!(!matches.iter().any(|c| c.photo_id == 3));
    }

    #[test]
    fn cosine_handles_normal_vectors() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 0.001);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 0.001);
    }

    #[test]
    fn text_context_is_fixed_width_int32_and_padded() {
        let ids = padded_text_context(&[2, 101, 102, 1]);

        assert_eq!(ids.len(), SEMANTIC_CONTEXT_LEN);
        assert_eq!(&ids[..5], &[2, 101, 102, 1, 0]);

        let long = (0..(SEMANTIC_CONTEXT_LEN as u32 + 10)).collect::<Vec<_>>();
        let truncated = padded_text_context(&long);
        assert_eq!(truncated.len(), SEMANTIC_CONTEXT_LEN);
        assert_eq!(truncated[0], 0);
        assert_eq!(
            truncated[SEMANTIC_CONTEXT_LEN - 1],
            (SEMANTIC_CONTEXT_LEN - 1) as i32
        );
    }

    #[test]
    fn text_search_gate_rejects_weak_absent_queries() {
        let kept = relevant_text_search_candidates(vec![
            SemanticCandidate {
                photo_id: 1,
                score: 0.035,
            },
            SemanticCandidate {
                photo_id: 2,
                score: 0.030,
            },
        ]);

        assert!(kept.is_empty());
    }

    #[test]
    fn text_search_gate_keeps_only_standout_matches() {
        let kept = relevant_text_search_candidates(vec![
            SemanticCandidate {
                photo_id: 1,
                score: 0.095,
            },
            SemanticCandidate {
                photo_id: 2,
                score: 0.070,
            },
            SemanticCandidate {
                photo_id: 3,
                score: 0.040,
            },
        ]);

        assert_eq!(kept.iter().map(|c| c.photo_id).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn text_search_gate_keeps_dense_relevant_clusters() {
        let kept = relevant_text_search_candidates(vec![
            SemanticCandidate {
                photo_id: 1,
                score: 0.078,
            },
            SemanticCandidate {
                photo_id: 2,
                score: 0.074,
            },
            SemanticCandidate {
                photo_id: 3,
                score: 0.048,
            },
        ]);

        assert_eq!(
            kept.iter().map(|c| c.photo_id).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }
}
