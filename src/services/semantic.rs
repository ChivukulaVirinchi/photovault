//! Semantic image search over local CLIP-style embeddings.
//!
//! The database stores only indexing state and vector offsets. The
//! high-volume embedding payload lives in `.photovault/semantic/...`
//! beside thumbnails and other per-library cache data.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticStatus {
    pub model_key: String,
    pub display_name: String,
    pub model_dir: String,
    pub assets_installed: bool,
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
            indexed_photos: stats.indexed,
            pending_photos: stats.pending,
            failed_photos: stats.failed,
            vector_bytes: std::fs::metadata(store.vector_path())
                .map(|m| m.len())
                .unwrap_or(0),
        })
    }

    pub async fn install_model_assets<F>(mut progress: F) -> Result<(), String>
    where
        F: FnMut(&str, u64, Option<u64>) + Send,
    {
        let paths = Self::default_asset_paths();
        download_asset(
            VISUAL_MODEL_URL,
            &paths.visual_model,
            "visual-model",
            &mut progress,
        )
        .await?;
        download_asset(
            TEXTUAL_MODEL_URL,
            &paths.textual_model,
            "text-model",
            &mut progress,
        )
        .await?;
        download_asset(TOKENIZER_URL, &paths.tokenizer, "tokenizer", &mut progress).await?;
        download_asset(
            PREPROCESS_URL,
            &paths.preprocess,
            "preprocess",
            &mut progress,
        )
        .await?;
        download_asset(CONFIG_URL, &paths.config, "config", &mut progress).await?;
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
               AND COALESCE(s.status, 'pending') != 'indexed'
               AND COALESCE(s.status, 'pending') != 'unsupported'
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
        conn: &Connection,
        photo_id: i64,
        vector: &[f32],
    ) -> rusqlite::Result<()> {
        let mut store = VectorStore::new(&self.drive_root)?;
        let offset = store.append(vector)?;
        conn.execute(
            "INSERT INTO semantic_index_state
                (photo_id, model_key, status, vector_offset, vector_dim, attempts, last_error, indexed_at)
             VALUES (?1, ?2, 'indexed', ?3, ?4, 0, NULL, CURRENT_TIMESTAMP)
             ON CONFLICT(photo_id, model_key) DO UPDATE SET
                status = 'indexed',
                vector_offset = excluded.vector_offset,
                vector_dim = excluded.vector_dim,
                last_error = NULL,
                indexed_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP",
            params![photo_id, SEMANTIC_MODEL_KEY, offset as i64, vector.len() as i64],
        )?;
        Ok(())
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

    pub fn model_runner() -> Result<SemanticModelRunner, String> {
        let paths = Self::find_assets().ok_or_else(|| {
            format!(
                "Semantic search model is not installed. Install {} from Settings.",
                SEMANTIC_MODEL_DISPLAY
            )
        })?;
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
        if self.media_type == "video" {
            if let Some(thumbnail) = &self.thumbnail_path {
                return safe_join_relative(drive_root, thumbnail)
                    .map_err(|e| format!("invalid video thumbnail path: {e}"));
            }
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

pub struct SemanticModelRunner {
    visual: ort::session::Session,
    textual: ort::session::Session,
    tokenizer: Tokenizer,
}

impl SemanticModelRunner {
    fn new(rt: &OnnxRuntime, paths: SemanticAssetPaths) -> Result<Self, String> {
        let visual = rt
            .load_model_with_threads(&paths.visual_model, 1)
            .map_err(|e| format!("visual model load failed: {e}"))?;
        let textual = rt
            .load_model_with_threads(&paths.textual_model, 1)
            .map_err(|e| format!("text model load failed: {e}"))?;
        let tokenizer = Tokenizer::from_file(&paths.tokenizer)
            .map_err(|e| format!("tokenizer load failed: {e}"))?;
        Ok(Self {
            visual,
            textual,
            tokenizer,
        })
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

    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("tokenization failed: {e}"))?;
        let mut ids = vec![0i64; SEMANTIC_CONTEXT_LEN];
        let mut mask = vec![0i64; SEMANTIC_CONTEXT_LEN];
        for (idx, id) in encoding
            .get_ids()
            .iter()
            .take(SEMANTIC_CONTEXT_LEN)
            .enumerate()
        {
            ids[idx] = *id as i64;
            mask[idx] = 1;
        }
        let input_ids = ort::value::TensorRef::<i64>::from_array_view((
            vec![1, SEMANTIC_CONTEXT_LEN as i64],
            ids.as_slice(),
        ))
        .map_err(|e| e.to_string())?;
        let attention = ort::value::TensorRef::<i64>::from_array_view((
            vec![1, SEMANTIC_CONTEXT_LEN as i64],
            mask.as_slice(),
        ))
        .map_err(|e| e.to_string())?;
        let outputs = self
            .textual
            .run(ort::inputs![input_ids, attention])
            .map_err(|e| format!("text inference failed: {e}"))?;
        extract_normalized_output(outputs)
    }
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

    fn append(&mut self, vector: &[f32]) -> rusqlite::Result<u64> {
        if vector.len() != SEMANTIC_DIM {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "semantic vector dimension {} != {}",
                vector.len(),
                SEMANTIC_DIM
            )));
        }
        let path = self.vector_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let offset = file
            .seek(SeekFrom::End(0))
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        for value in vector {
            file.write_all(&value.to_le_bytes())
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        }
        self.bump_manifest()?;
        Ok(offset)
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

    fn bump_manifest(&self) -> rusqlite::Result<()> {
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
        manifest.vector_count += 1;
        let data = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        std::fs::write(path, data)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok(())
    }
}

fn count_state(conn: &Connection, status: &str) -> rusqlite::Result<u64> {
    conn.query_row(
        "SELECT COUNT(*) FROM semantic_index_state
         WHERE model_key = ?1 AND status = ?2",
        params![SEMANTIC_MODEL_KEY, status],
        |r| r.get::<_, i64>(0),
    )
    .map(|v| v as u64)
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

async fn download_asset<F>(
    url: &str,
    destination: &Path,
    stage: &str,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&str, u64, Option<u64>) + Send,
{
    if destination.exists() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed creating {}: {e}", parent.display()))?;
    }
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("download request failed for {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "download failed for {url}: HTTP {}",
            response.status()
        ));
    }
    let total = response.content_length();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("download body failed for {url}: {e}"))?;
    progress(stage, bytes.len() as u64, total);
    let tmp = destination.with_extension("tmp");
    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|e| format!("failed writing {}: {e}", tmp.display()))?;
    tokio::fs::rename(&tmp, destination)
        .await
        .map_err(|e| format!("failed moving {}: {e}", destination.display()))?;
    Ok(())
}

pub fn semantic_ids_by_score(candidates: &[SemanticCandidate]) -> HashMap<i64, usize> {
    candidates
        .iter()
        .enumerate()
        .map(|(idx, c)| (c.photo_id, idx))
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

    #[test]
    fn vector_store_round_trips_fixed_width_vectors() {
        let dir = tempdir().unwrap();
        let mut store = VectorStore::new(dir.path()).unwrap();
        let mut first = vec![0.0f32; SEMANTIC_DIM];
        first[3] = 1.0;
        let mut second = vec![0.0f32; SEMANTIC_DIM];
        second[9] = 1.0;

        let off_a = store.append(&first).unwrap();
        let off_b = store.append(&second).unwrap();

        assert_eq!(off_a, 0);
        assert_eq!(off_b, (SEMANTIC_DIM * 4) as u64);
        assert_eq!(store.read(off_a, SEMANTIC_DIM).unwrap(), first);
        assert_eq!(store.read(off_b, SEMANTIC_DIM).unwrap(), second);
    }

    #[test]
    fn cosine_handles_normal_vectors() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 0.001);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 0.001);
    }
}
