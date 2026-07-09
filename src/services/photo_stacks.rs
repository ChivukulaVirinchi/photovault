//! Timeline photo stack generation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use image::DynamicImage;
use rusqlite::{params, Connection, Result as SqliteResult};

use crate::db::{PhotoStackRepo, StackCandidate};
use crate::services::path_util::safe_join_relative;

#[derive(Debug, Clone, Default)]
pub struct StackRefreshResult {
    pub stacks_found: usize,
}

#[derive(Debug, Clone)]
struct CandidatePhoto {
    id: i64,
    file_path: String,
    thumbnail_path: Option<String>,
    file_size: i64,
    width: Option<i32>,
    height: Option<i32>,
    suggested: bool,
    face_count: i64,
}

#[derive(Debug, Clone)]
struct Score {
    value: f32,
    reasons: String,
}

pub struct PhotoStackService;

impl PhotoStackService {
    pub fn refresh(conn: &Connection, drive_root: &Path) -> SqliteResult<StackRefreshResult> {
        let mut candidates = Vec::new();
        let mut claimed = HashSet::new();

        candidates.extend(Self::duplicate_candidates(
            conn,
            drive_root,
            "exact",
            "exact_duplicate",
            1.0,
            &mut claimed,
        )?);
        candidates.extend(Self::duplicate_candidates(
            conn,
            drive_root,
            "perceptual",
            "perceptual_duplicate",
            0.94,
            &mut claimed,
        )?);
        candidates.extend(Self::burst_candidates(conn, drive_root, &mut claimed)?);

        let count = candidates.len();
        PhotoStackRepo::new(conn).sync_stacks(&candidates)?;
        Ok(StackRefreshResult {
            stacks_found: count,
        })
    }

    fn duplicate_candidates(
        conn: &Connection,
        drive_root: &Path,
        duplicate_type: &str,
        stack_kind: &str,
        confidence: f32,
        claimed: &mut HashSet<i64>,
    ) -> SqliteResult<Vec<StackCandidate>> {
        let mut stmt =
            conn.prepare("SELECT id, group_hash FROM duplicate_groups WHERE duplicate_type = ?1")?;
        let groups: Vec<(i64, String)> = stmt
            .query_map(params![duplicate_type], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<SqliteResult<Vec<_>>>()?;

        let mut out = Vec::new();
        for (group_id, group_hash) in groups {
            let mut photos = Self::duplicate_members(conn, group_id)?;
            photos.retain(|p| !claimed.contains(&p.id));
            if duplicate_type == "exact" {
                photos = Self::verified_exact_members(drive_root, photos);
            }
            if photos.len() < 2 {
                continue;
            }
            let c = Self::build_candidate(
                stack_kind,
                group_id,
                Some(group_hash),
                confidence,
                photos,
                drive_root,
            );
            for id in &c.photo_ids {
                claimed.insert(*id);
            }
            out.push(c);
        }
        Ok(out)
    }

    fn duplicate_members(conn: &Connection, group_id: i64) -> SqliteResult<Vec<CandidatePhoto>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT p.id, p.file_path, p.thumbnail_path, p.file_size, p.width, p.height,
                   m.is_suggested_keep,
                   (SELECT COUNT(*) FROM faces f WHERE f.photo_id = p.id) AS face_count
              FROM duplicate_group_members m
              JOIN photos p ON p.id = m.photo_id
             WHERE m.group_id = ?1 AND p.is_trashed = FALSE
          ORDER BY m.is_suggested_keep DESC, p.date_taken ASC, p.id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![group_id], row_to_candidate)?;
        rows.collect()
    }

    fn verified_exact_members(
        drive_root: &Path,
        photos: Vec<CandidatePhoto>,
    ) -> Vec<CandidatePhoto> {
        let mut by_hash: HashMap<String, Vec<CandidatePhoto>> = HashMap::new();
        for p in photos {
            let abs = match safe_join_relative(drive_root, &p.file_path) {
                Ok(path) => path,
                Err(e) => {
                    tracing::debug!(
                        "stack exact verification skipped invalid path {}: {}",
                        p.file_path,
                        e
                    );
                    continue;
                }
            };
            match crate::services::scanner::calculate_hash(&abs) {
                Ok(hash) => by_hash.entry(hash).or_default().push(p),
                Err(e) => {
                    tracing::debug!("stack exact verification skipped {}: {}", abs.display(), e)
                }
            }
        }
        by_hash
            .into_values()
            .filter(|v| v.len() >= 2)
            .max_by_key(|v| v.len())
            .unwrap_or_default()
    }

    fn burst_candidates(
        conn: &Connection,
        drive_root: &Path,
        claimed: &mut HashSet<i64>,
    ) -> SqliteResult<Vec<StackCandidate>> {
        let mut stmt = conn.prepare("SELECT id FROM burst_groups")?;
        let groups: Vec<i64> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<SqliteResult<Vec<_>>>()?;

        let mut out = Vec::new();
        for group_id in groups {
            let mut photos = Self::burst_members(conn, group_id)?;
            photos.retain(|p| !claimed.contains(&p.id));
            if photos.len() < 2 || !Self::has_visual_evidence(&photos) {
                continue;
            }
            let c = Self::build_candidate("burst", group_id, None, 0.80, photos, drive_root);
            for id in &c.photo_ids {
                claimed.insert(*id);
            }
            out.push(c);
        }
        Ok(out)
    }

    fn burst_members(conn: &Connection, group_id: i64) -> SqliteResult<Vec<CandidatePhoto>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT p.id, p.file_path, p.thumbnail_path, p.file_size, p.width, p.height,
                   m.is_suggested_best,
                   (SELECT COUNT(*) FROM faces f WHERE f.photo_id = p.id) AS face_count
              FROM burst_group_members m
              JOIN photos p ON p.id = m.photo_id
             WHERE m.group_id = ?1 AND p.is_trashed = FALSE
          ORDER BY m.is_suggested_best DESC, p.date_taken ASC, p.id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![group_id], row_to_candidate)?;
        rows.collect()
    }

    fn has_visual_evidence(photos: &[CandidatePhoto]) -> bool {
        photos.iter().all(|p| p.thumbnail_path.is_some())
    }

    fn build_candidate(
        kind: &str,
        source_group_id: i64,
        source_group_hash: Option<String>,
        confidence: f32,
        photos: Vec<CandidatePhoto>,
        drive_root: &Path,
    ) -> StackCandidate {
        let mut scored: Vec<(CandidatePhoto, Score)> = photos
            .into_iter()
            .map(|p| {
                let s = Self::score_photo(&p, drive_root);
                (p, s)
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.value
                .partial_cmp(&a.1.value)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.0.suggested.cmp(&a.0.suggested))
                .then_with(|| b.0.file_size.cmp(&a.0.file_size))
                .then_with(|| a.0.id.cmp(&b.0.id))
        });
        let cover_photo_id = scored.first().map(|(p, _)| p.id).unwrap_or_default();
        StackCandidate {
            kind: kind.to_string(),
            source_group_id,
            source_group_hash,
            photo_ids: scored.iter().map(|(p, _)| p.id).collect(),
            cover_photo_id,
            confidence,
            member_scores: scored
                .into_iter()
                .map(|(p, s)| (p.id, s.value, s.reasons))
                .collect(),
        }
    }

    fn score_photo(photo: &CandidatePhoto, drive_root: &Path) -> Score {
        let mut score = 0.0f32;
        let mut reasons = Vec::new();

        if photo.suggested {
            score += 20.0;
            reasons.push("existing pick");
        }
        let megapixels = match (photo.width, photo.height) {
            (Some(w), Some(h)) if w > 0 && h > 0 => (w as f32 * h as f32) / 1_000_000.0,
            _ => 0.0,
        };
        if megapixels > 0.0 {
            score += megapixels.min(60.0) * 1.2;
            reasons.push("resolution");
        }
        if photo.file_size > 0 {
            score += ((photo.file_size as f32).ln() / 20.0).min(4.0);
            reasons.push("file size");
        }
        if photo.face_count > 0 {
            score += (photo.face_count as f32).min(5.0) * 2.0;
            reasons.push("faces");
        }
        if let Some(img) = Self::open_scoring_image(photo, drive_root) {
            let (sharpness, brightness_score) = Self::image_quality(&img);
            score += sharpness.min(40.0);
            score += brightness_score * 8.0;
            reasons.push("sharpness");
            reasons.push("exposure");
        }

        Score {
            value: score,
            reasons: reasons.join(", "),
        }
    }

    fn open_scoring_image(photo: &CandidatePhoto, drive_root: &Path) -> Option<DynamicImage> {
        let path = photo
            .thumbnail_path
            .as_ref()
            .and_then(|p| safe_join_relative(drive_root, p).ok())
            .or_else(|| safe_join_relative(drive_root, &photo.file_path).ok())?;
        crate::services::image_io::open_image(&path).ok()
    }

    fn image_quality(img: &DynamicImage) -> (f32, f32) {
        let gray = img
            .resize(128, 128, image::imageops::FilterType::Triangle)
            .to_luma8();
        let (w, h) = gray.dimensions();
        if w < 3 || h < 3 {
            return (0.0, 0.0);
        }

        let mut edge_sum = 0.0f32;
        let mut count = 0.0f32;
        let mut luminance = 0.0f32;
        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                let c = gray.get_pixel(x, y)[0] as f32;
                let l = gray.get_pixel(x - 1, y)[0] as f32;
                let r = gray.get_pixel(x + 1, y)[0] as f32;
                let u = gray.get_pixel(x, y - 1)[0] as f32;
                let d = gray.get_pixel(x, y + 1)[0] as f32;
                edge_sum += ((r - l).abs() + (d - u).abs()) / 255.0;
                luminance += c / 255.0;
                count += 1.0;
            }
        }
        let sharpness = if count > 0.0 {
            (edge_sum / count) * 80.0
        } else {
            0.0
        };
        let avg = if count > 0.0 { luminance / count } else { 0.5 };
        let brightness_score = (1.0 - ((avg - 0.5).abs() * 2.0)).clamp(0.0, 1.0);
        (sharpness, brightness_score)
    }
}

fn row_to_candidate(row: &rusqlite::Row) -> SqliteResult<CandidatePhoto> {
    Ok(CandidatePhoto {
        id: row.get(0)?,
        file_path: row.get(1)?,
        thumbnail_path: row.get(2)?,
        file_size: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        suggested: row.get(6)?,
        face_count: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: i64, width: i32, height: i32, file_size: i64) -> CandidatePhoto {
        CandidatePhoto {
            id,
            file_path: format!("{id}.jpg"),
            thumbnail_path: None,
            file_size,
            width: Some(width),
            height: Some(height),
            suggested: false,
            face_count: 0,
        }
    }

    #[test]
    fn stack_cover_uses_quality_score_not_first_member() {
        let photos = vec![
            candidate(1, 800, 600, 250_000),
            candidate(2, 6000, 4000, 5_000_000),
        ];

        let stack = PhotoStackService::build_candidate(
            "burst",
            10,
            None,
            0.8,
            photos,
            Path::new("/missing"),
        );

        assert_eq!(stack.cover_photo_id, 2);
    }
}
