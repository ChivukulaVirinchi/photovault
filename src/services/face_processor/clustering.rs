use super::*;

/// Upper bound on unresolved faces fed into Stage B when the legacy
/// complete-link path is in use. With `hnsw_clustering` (default-on)
/// FaceClusterer::cluster runs in O(n log n) so the cap is effectively
/// infinite; we keep the constant only to bound the legacy fallback
/// when someone disables the feature for A/B testing.
#[cfg(not(feature = "hnsw_clustering"))]
const MAX_STAGE_B_INPUT: usize = 800;

impl FaceProcessor {
    /// Run incremental clustering in two stages:
    /// 1) Gallery k-NN retrieval + confidence bands:
    ///    HIGH      -> auto-assign to top cluster
    ///    AMBIGUOUS -> queue for user review
    ///    LOW       -> leave for Stage 2
    /// 2) Complete-link agglomerative clustering on still-unresolved faces
    pub(crate) fn run_clustering(
        face_repo: &FaceRepo,
        clustering_threshold: f32,
        resolver_weights: crate::ml::ResolverWeights,
    ) -> Result<usize, String> {
        let mut assigned_to_existing = 0usize;
        let mut queued_for_review = 0usize;
        let strict_max_distance = clustering_threshold.clamp(0.15, 0.6);

        // Stage A: gallery-based retrieval with confidence bands.
        let galleries = face_repo
            .get_gallery_embeddings()
            .map_err(|e| format!("Failed to load person galleries: {}", e))?;
        let cluster_photo_rows = face_repo
            .get_cluster_photo_ids()
            .map_err(|e| format!("Failed to load cluster-photo map: {}", e))?;
        let cannot_merge = face_repo
            .get_cannot_merge_map()
            .map_err(|e| format!("Failed to load cannot-merge constraints: {}", e))?;

        let mut cluster_photo_ids: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
            std::collections::HashMap::new();
        for (cluster_id, photo_id) in cluster_photo_rows {
            cluster_photo_ids
                .entry(cluster_id)
                .or_default()
                .insert(photo_id);
        }

        // Build the gallery slice form retrieve_candidates expects.
        let mut gallery_by_cluster: std::collections::HashMap<
            i64,
            Vec<(i64, crate::ml::FaceEmbedding)>,
        > = std::collections::HashMap::new();
        for g in galleries {
            gallery_by_cluster
                .entry(g.cluster_id)
                .or_default()
                .push((g.face_id, g.embedding));
        }
        let gallery_vec: Vec<(i64, Vec<(i64, crate::ml::FaceEmbedding)>)> =
            gallery_by_cluster.into_iter().collect();

        let banding = crate::ml::BandingConfig::default();

        let unclustered = face_repo
            .get_unclustered_faces_with_photo_embeddings()
            .map_err(|e| format!("Failed to get unclustered faces: {}", e))?;

        for (face_id, photo_id, embedding) in &unclustered {
            // Build exclusion set: clusters already present in this photo
            // (same-photo conflict) and anything cannot-merge with those.
            let mut exclude: std::collections::HashSet<i64> = std::collections::HashSet::new();
            let clusters_in_photo: Vec<i64> = cluster_photo_ids
                .iter()
                .filter(|(_, set)| set.contains(photo_id))
                .map(|(cid, _)| *cid)
                .collect();
            for cid in &clusters_in_photo {
                exclude.insert(*cid);
                if let Some(forbidden) = cannot_merge.get(cid) {
                    for f in forbidden {
                        exclude.insert(*f);
                    }
                }
            }

            let hits = crate::ml::retrieve_candidates(
                embedding,
                &gallery_vec,
                5,
                1.0 - strict_max_distance, // treat distance threshold as similarity lower bound
                &exclude,
            );

            // Context re-rank using co-occurrence and temporal-neighbor signals.
            let resolver_ctx = Self::build_resolver_context(face_repo, *photo_id, *face_id, &hits);
            let reranked = crate::ml::rerank(&hits, &resolver_ctx, resolver_weights);
            let band = crate::ml::retrieval::classify(&reranked, &banding);

            match band {
                crate::ml::ConfidenceBand::High { hit } => {
                    face_repo
                        .assign_face_to_cluster(*face_id, hit.cluster_id)
                        .map_err(|e| format!("Failed to assign face to cluster: {}", e))?;
                    cluster_photo_ids
                        .entry(hit.cluster_id)
                        .or_default()
                        .insert(*photo_id);
                    assigned_to_existing += 1;
                }
                crate::ml::ConfidenceBand::Ambiguous { top, runner_up } => {
                    let ambiguity = runner_up.as_ref().map(|r| top.score - r.score);
                    if let Err(e) =
                        face_repo.enqueue_review(*face_id, top.cluster_id, top.score, ambiguity)
                    {
                        tracing::warn!("Failed to enqueue review for face {}: {}", face_id, e);
                    }
                    queued_for_review += 1;
                }
                crate::ml::ConfidenceBand::Low => {
                    // Leave unassigned; Stage B (agglomerative) will handle it.
                }
            }
        }

        if queued_for_review > 0 {
            tracing::info!(
                "Queued {} ambiguous faces for user review",
                queued_for_review
            );
        }

        // Stage B: complete-link agglomerative clustering on unresolved faces.
        let unresolved = face_repo
            .get_unclustered_faces_with_photo_embeddings()
            .map_err(|e| format!("Failed to reload unresolved faces: {}", e))?;

        if unresolved.is_empty() {
            face_repo
                .refresh_all_galleries()
                .map_err(|e| format!("Failed to refresh galleries: {}", e))?;
            tracing::info!(
                "Agglomerative clustering: assigned {} faces to existing galleries; no unresolved faces left",
                assigned_to_existing
            );
            return Ok(0);
        }

        #[cfg(not(feature = "hnsw_clustering"))]
        if unresolved.len() > MAX_STAGE_B_INPUT {
            // Legacy path only: skip Stage B; route unresolved faces
            // through the rescue pipeline so they remain visible
            // (auto-matched against galleries or queued for ambiguous
            // review) instead of paying the quadratic cost.
            tracing::warn!(
                "Skipping Stage B complete-link clustering: {} unresolved faces exceeds cap of {}. Enable `hnsw_clustering` to remove this cap.",
                unresolved.len(),
                MAX_STAGE_B_INPUT
            );
            face_repo
                .refresh_all_galleries()
                .map_err(|e| format!("Failed to refresh galleries: {}", e))?;
            let (rescued, queued) = Self::rescue_orphan_faces(face_repo)?;
            tracing::info!(
                "Clustering (Stage B skipped): {} to-existing, {} rescued, {} queued, from {} unresolved",
                assigned_to_existing,
                rescued,
                queued,
                unresolved.len()
            );
            return Ok(0);
        }

        let inputs: Vec<ClusterInput> = unresolved
            .iter()
            .map(|(face_id, photo_id, emb)| ClusterInput {
                face_id: *face_id,
                photo_id: *photo_id,
                embedding: emb.clone(),
            })
            .collect();

        let clusterer = FaceClusterer::new().with_max_distance(strict_max_distance);
        let assignments = clusterer.cluster(&inputs);

        let mut cluster_groups: HashMap<i32, Vec<i64>> = HashMap::new();
        for (face_id, cluster_id) in assignments {
            if cluster_id >= 0 {
                cluster_groups.entry(cluster_id).or_default().push(face_id);
            }
        }

        let mut clusters_created = 0usize;
        for face_ids in cluster_groups.values() {
            if face_ids.len() >= 2 {
                face_repo
                    .create_cluster(face_ids)
                    .map_err(|e| format!("Failed to create cluster: {}", e))?;
                clusters_created += 1;
            }
        }

        face_repo
            .refresh_all_galleries()
            .map_err(|e| format!("Failed to refresh galleries: {}", e))?;

        // Post-pass: unify clusters that are actually the same person but
        // got split by complete-link's strictness under lighting variance.
        // Uses single-link (best pair) across gallery members with a stricter
        // "multiple good pairs" requirement to avoid false positives.
        let merged = Self::merge_similar_clusters(face_repo)?;

        // Rescue pass: faces that ended up as singletons (not placed into
        // any cluster by Stage A or Stage B) need a second chance. Otherwise
        // they're invisible to the user: they have cluster_id = NULL so they
        // don't show in People, and nothing queues them for review.
        let (rescued, queued) = Self::rescue_orphan_faces(face_repo)?;

        tracing::info!(
            "Clustering: {} to-existing, {} new, merged {}, rescued {}, queued {}, from {} unresolved",
            assigned_to_existing,
            clusters_created,
            merged,
            rescued,
            queued,
            unresolved.len()
        );

        Ok(clusters_created.saturating_sub(merged))
    }

    /// Rescue pass for orphan faces (cluster_id IS NULL after all earlier
    /// stages). For each orphan, k-NN retrieve against existing cluster
    /// galleries with a looser threshold than the first-pass retrieval.
    /// Classify into three bands:
    ///   - HIGH       (mean top-k sim >= 0.60, 0.08 margin over runner-up):
    ///     auto-assign to best cluster
    ///   - AMBIGUOUS  (0.45 <= sim < 0.60):
    ///     enqueue for user review — the review badge will prompt
    ///     them to resolve without any explicit UI "check now" button
    ///   - LOW        (< 0.45):
    ///     leave as orphan; a future scan or better gallery may
    ///     eventually pick it up
    ///
    /// Returns (auto_rescued, queued_for_review).
    fn rescue_orphan_faces(face_repo: &FaceRepo) -> Result<(usize, usize), String> {
        // Deliberately looser than first-pass retrieval:
        // first-pass min_similarity = 1.0 - strict_max_distance ~= 0.58.
        // We accept weaker gallery members here because the orphan either
        // didn't fit Stage B's strict complete-link or matched nothing in
        // Stage A — it may still be a valid match under some gallery member.
        const RESCUE_MIN_SIM: f32 = 0.45;

        let rescue_banding = crate::ml::BandingConfig {
            low_threshold: 0.45,
            high_threshold: 0.60,
            margin: 0.08,
        };

        let galleries = face_repo
            .get_gallery_embeddings()
            .map_err(|e| format!("Failed to load galleries for rescue: {}", e))?;

        if galleries.is_empty() {
            // No clusters exist -> nothing to match against.
            return Ok((0, 0));
        }

        let mut gallery_by_cluster: HashMap<i64, Vec<(i64, crate::ml::FaceEmbedding)>> =
            HashMap::new();
        for g in galleries {
            gallery_by_cluster
                .entry(g.cluster_id)
                .or_default()
                .push((g.face_id, g.embedding));
        }
        let gallery_vec: Vec<(i64, Vec<(i64, crate::ml::FaceEmbedding)>)> =
            gallery_by_cluster.into_iter().collect();

        let cannot_merge = face_repo
            .get_cannot_merge_map()
            .map_err(|e| format!("Failed to load cannot-merge: {}", e))?;

        let cluster_photo_rows = face_repo
            .get_cluster_photo_ids()
            .map_err(|e| format!("Failed to load cluster-photo map: {}", e))?;
        let mut cluster_photo_ids: HashMap<i64, std::collections::HashSet<i64>> = HashMap::new();
        for (cid, pid) in cluster_photo_rows {
            cluster_photo_ids.entry(cid).or_default().insert(pid);
        }

        let orphans = face_repo
            .get_unclustered_faces_with_photo_embeddings()
            .map_err(|e| format!("Failed to load orphan faces: {}", e))?;

        let mut rescued = 0usize;
        let mut queued = 0usize;

        for (face_id, photo_id, embedding) in &orphans {
            // Same-photo conflict + cannot-merge-transitively exclusions.
            let mut exclude: std::collections::HashSet<i64> = std::collections::HashSet::new();
            let clusters_in_photo: Vec<i64> = cluster_photo_ids
                .iter()
                .filter(|(_, set)| set.contains(photo_id))
                .map(|(cid, _)| *cid)
                .collect();
            for cid in &clusters_in_photo {
                exclude.insert(*cid);
                if let Some(forbidden) = cannot_merge.get(cid) {
                    for f in forbidden {
                        exclude.insert(*f);
                    }
                }
            }

            let hits = crate::ml::retrieve_candidates(
                embedding,
                &gallery_vec,
                5,
                RESCUE_MIN_SIM,
                &exclude,
            );
            let band = crate::ml::retrieval::classify(&hits, &rescue_banding);

            match band {
                crate::ml::ConfidenceBand::High { hit } => {
                    if let Err(e) = face_repo.assign_face_to_cluster(*face_id, hit.cluster_id) {
                        tracing::warn!("rescue: assign_face_to_cluster failed: {}", e);
                        continue;
                    }
                    cluster_photo_ids
                        .entry(hit.cluster_id)
                        .or_default()
                        .insert(*photo_id);
                    rescued += 1;
                }
                crate::ml::ConfidenceBand::Ambiguous { top, runner_up } => {
                    let ambiguity = runner_up.as_ref().map(|r| top.score - r.score);
                    if let Err(e) =
                        face_repo.enqueue_review(*face_id, top.cluster_id, top.score, ambiguity)
                    {
                        tracing::warn!("rescue: enqueue_review failed: {}", e);
                        continue;
                    }
                    queued += 1;
                }
                crate::ml::ConfidenceBand::Low => {
                    // Stays an orphan for now.
                }
            }
        }

        Ok((rescued, queued))
    }

    /// Unify clusters that likely represent the same person but got split by
    /// complete-link's strict pairwise requirement.
    ///
    /// For each pair (A, B) of existing clusters:
    ///   - Skip if cannot-merge is set.
    ///   - Skip if they share any photo (same-photo-different-faces means
    ///     they must be different people).
    ///   - Compute cross-cluster similarity using single-link on the gallery:
    ///     mean of the top-3 cosine similarities across all member pairs.
    ///   - If the mean top-3 exceeds `merge_threshold`, queue the merge.
    ///
    /// Merges are applied in descending score order with a union-find over
    /// live cluster IDs, so a chain of fragments can all collapse into one.
    fn merge_similar_clusters(face_repo: &FaceRepo) -> Result<usize, String> {
        // Tunable: how aggressively to unify fragments. Higher = safer.
        // Mean of top-3 cross-cluster similarities above this -> merge.
        // 0.68 is the bar at which two clusters are confidently the same
        // person. Lower values silently merged distinct people whose
        // gallery shots happened to share lighting / angle / accessories.
        const MERGE_THRESHOLD: f32 = 0.68;
        const TOP_K_PAIRS: usize = 3;

        let galleries = face_repo
            .get_gallery_embeddings()
            .map_err(|e| format!("Failed to load galleries: {}", e))?;

        let mut gallery_by_cluster: HashMap<i64, Vec<crate::ml::FaceEmbedding>> = HashMap::new();
        for g in galleries {
            gallery_by_cluster
                .entry(g.cluster_id)
                .or_default()
                .push(g.embedding);
        }

        let cannot_merge = face_repo
            .get_cannot_merge_map()
            .map_err(|e| format!("Failed to load cannot-merge: {}", e))?;

        let cluster_photo_rows = face_repo
            .get_cluster_photo_ids()
            .map_err(|e| format!("Failed to load cluster-photo map: {}", e))?;
        let mut cluster_photos: HashMap<i64, std::collections::HashSet<i64>> = HashMap::new();
        for (cid, pid) in cluster_photo_rows {
            cluster_photos.entry(cid).or_default().insert(pid);
        }

        let cluster_ids: Vec<i64> = gallery_by_cluster.keys().copied().collect();
        let mut candidates: Vec<(f32, i64, i64)> = Vec::new();

        for i in 0..cluster_ids.len() {
            for j in (i + 1)..cluster_ids.len() {
                let a = cluster_ids[i];
                let b = cluster_ids[j];

                if cannot_merge.get(&a).is_some_and(|set| set.contains(&b)) {
                    continue;
                }

                let photos_a = cluster_photos.get(&a);
                let photos_b = cluster_photos.get(&b);
                let shares_photo = match (photos_a, photos_b) {
                    (Some(pa), Some(pb)) => pa.iter().any(|p| pb.contains(p)),
                    _ => false,
                };
                if shares_photo {
                    continue;
                }

                let ga = gallery_by_cluster
                    .get(&a)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                let gb = gallery_by_cluster
                    .get(&b)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                if ga.is_empty() || gb.is_empty() {
                    continue;
                }

                let mut sims: Vec<f32> = Vec::with_capacity(ga.len() * gb.len());
                for ea in ga {
                    for eb in gb {
                        let s = ea.cosine_similarity(eb);
                        if s.is_nan() {
                            continue;
                        }
                        sims.push(s);
                    }
                }
                if sims.is_empty() {
                    continue;
                }
                sims.sort_by(|x, y| y.total_cmp(x));
                let k = TOP_K_PAIRS.min(sims.len());
                let mean_top_k: f32 = sims.iter().take(k).sum::<f32>() / k as f32;

                if mean_top_k >= MERGE_THRESHOLD {
                    candidates.push((mean_top_k, a, b));
                }
            }
        }

        candidates.sort_by(|x, y| y.0.total_cmp(&x.0));

        // Union-find: each cluster starts pointing to itself; merges redirect.
        let mut parent: HashMap<i64, i64> = cluster_ids.iter().map(|&c| (c, c)).collect();
        fn find(parent: &mut HashMap<i64, i64>, mut x: i64) -> i64 {
            loop {
                let p = *parent.get(&x).unwrap_or(&x);
                if p == x {
                    return x;
                }
                // Path compression.
                let pp = *parent.get(&p).unwrap_or(&p);
                parent.insert(x, pp);
                x = pp;
            }
        }

        let mut merged_count = 0usize;
        for (score, a, b) in candidates {
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            if ra == rb {
                continue;
            }
            // Keep the smaller cluster id as the survivor (arbitrary but stable).
            let (survivor, casualty) = if ra < rb { (ra, rb) } else { (rb, ra) };
            match face_repo.merge_clusters(casualty, survivor) {
                Ok(_) => {
                    parent.insert(casualty, survivor);
                    merged_count += 1;
                    tracing::info!(
                        "Post-pass merge: cluster {} -> {} (score {:.3})",
                        casualty,
                        survivor,
                        score
                    );
                }
                Err(e) => {
                    tracing::warn!("merge_similar_clusters: merge failed {}: {}", casualty, e);
                }
            }
        }

        Ok(merged_count)
    }

    /// Save a face crop image to disk as JPEG.
    pub(crate) fn save_face_crop(
        aligned_face: &image::RgbImage,
        path: &Path,
    ) -> Result<(), image::ImageError> {
        let dynamic = image::DynamicImage::ImageRgb8(aligned_face.clone());
        let resized = dynamic.resize_exact(80, 80, image::imageops::FilterType::Lanczos3);
        resized.save(path)
    }

    /// Regenerate missing face crop files from stored bounding box data.
    pub fn regenerate_missing_crops(drive_path: &Path) -> Result<usize, String> {
        let db = Database::open_for_drive(drive_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        let face_repo = FaceRepo::new(&db.conn);

        let faces_dir = Self::faces_dir(drive_path);
        if let Err(e) = std::fs::create_dir_all(&faces_dir) {
            return Err(format!("Failed to create faces directory: {}", e));
        }

        let all_faces = face_repo
            .get_all_faces_with_paths()
            .map_err(|e| format!("Failed to get faces: {}", e))?;

        let mut regenerated = 0usize;
        for (face_id, file_path, orientation, bbox_x, bbox_y, bbox_w, bbox_h) in &all_faces {
            let crop_path = faces_dir.join(format!("{}.jpg", face_id));
            if crop_path.exists() {
                continue;
            }

            let full_path = drive_path.join(file_path);
            let img = match image::open(&full_path) {
                Ok(img) => apply_exif_orientation(img, *orientation),
                Err(_) => continue,
            };

            let (img_w, img_h) = (img.width() as f32, img.height() as f32);

            let px = (bbox_x * img_w) as u32;
            let py = (bbox_y * img_h) as u32;
            let pw = (bbox_w * img_w) as u32;
            let ph = (bbox_h * img_h) as u32;

            let pad_x = (pw as f32 * 0.2) as u32;
            let pad_y = (ph as f32 * 0.2) as u32;
            let crop_x = px.saturating_sub(pad_x);
            let crop_y = py.saturating_sub(pad_y);
            let crop_w = (pw + 2 * pad_x).min(img.width() - crop_x);
            let crop_h = (ph + 2 * pad_y).min(img.height() - crop_y);

            if crop_w == 0 || crop_h == 0 {
                continue;
            }

            let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h);
            let resized = cropped.resize_exact(80, 80, image::imageops::FilterType::Lanczos3);
            if resized.save(&crop_path).is_ok() {
                regenerated += 1;
            }
        }

        if regenerated > 0 {
            tracing::info!("Regenerated {} missing face crop thumbnails", regenerated);
        }

        Ok(regenerated)
    }

    /// Get the face crops directory for a drive.
    pub fn faces_dir(drive_path: &Path) -> PathBuf {
        drive_path.join(".photovault").join("faces")
    }

    /// Streaming Stage A — run *only* the gallery-match leg of the
    /// clustering pipeline so faces appear in the People view as the
    /// writer thread commits chunks. Stage B (complete-link / new
    /// cluster creation) and the rescue pass stay end-of-pipeline,
    /// because both need the full unresolved set to make sane
    /// decisions.
    ///
    /// HIGH-band hits are auto-assigned. AMBIGUOUS hits go onto the
    /// review queue (so the badge bumps). LOW hits stay unassigned
    /// for end-of-run clustering to handle.
    pub(crate) fn stream_assign_existing_clusters(
        face_repo: &FaceRepo,
        clustering_threshold: f32,
        resolver_weights: crate::ml::ResolverWeights,
    ) -> Result<usize, String> {
        let strict_max_distance = clustering_threshold.clamp(0.15, 0.6);

        let galleries = face_repo
            .get_gallery_embeddings()
            .map_err(|e| format!("stream Stage A: load galleries: {}", e))?;
        if galleries.is_empty() {
            // No existing clusters yet; nothing to match against. Wait
            // for end-of-run Stage B to seed the first set of people.
            return Ok(0);
        }

        let cluster_photo_rows = face_repo
            .get_cluster_photo_ids()
            .map_err(|e| format!("stream Stage A: cluster-photo map: {}", e))?;
        let cannot_merge = face_repo
            .get_cannot_merge_map()
            .map_err(|e| format!("stream Stage A: cannot-merge: {}", e))?;

        let mut cluster_photo_ids: HashMap<i64, std::collections::HashSet<i64>> = HashMap::new();
        for (cid, pid) in cluster_photo_rows {
            cluster_photo_ids.entry(cid).or_default().insert(pid);
        }

        let mut gallery_by_cluster: HashMap<i64, Vec<(i64, crate::ml::FaceEmbedding)>> =
            HashMap::new();
        for g in galleries {
            gallery_by_cluster
                .entry(g.cluster_id)
                .or_default()
                .push((g.face_id, g.embedding));
        }
        let gallery_vec: Vec<(i64, Vec<(i64, crate::ml::FaceEmbedding)>)> =
            gallery_by_cluster.into_iter().collect();

        let banding = crate::ml::BandingConfig::default();
        let unclustered = face_repo
            .get_unclustered_faces_with_photo_embeddings()
            .map_err(|e| format!("stream Stage A: get unclustered: {}", e))?;

        let mut assigned = 0usize;
        for (face_id, photo_id, embedding) in &unclustered {
            let mut exclude: std::collections::HashSet<i64> = std::collections::HashSet::new();
            let clusters_in_photo: Vec<i64> = cluster_photo_ids
                .iter()
                .filter(|(_, set)| set.contains(photo_id))
                .map(|(cid, _)| *cid)
                .collect();
            for cid in &clusters_in_photo {
                exclude.insert(*cid);
                if let Some(forbidden) = cannot_merge.get(cid) {
                    for f in forbidden {
                        exclude.insert(*f);
                    }
                }
            }

            let hits = crate::ml::retrieve_candidates(
                embedding,
                &gallery_vec,
                5,
                1.0 - strict_max_distance,
                &exclude,
            );
            let resolver_ctx = Self::build_resolver_context(face_repo, *photo_id, *face_id, &hits);
            let reranked = crate::ml::rerank(&hits, &resolver_ctx, resolver_weights);
            let band = crate::ml::retrieval::classify(&reranked, &banding);

            match band {
                crate::ml::ConfidenceBand::High { hit }
                    if face_repo
                        .assign_face_to_cluster(*face_id, hit.cluster_id)
                        .is_ok() =>
                {
                    cluster_photo_ids
                        .entry(hit.cluster_id)
                        .or_default()
                        .insert(*photo_id);
                    assigned += 1;
                }
                _ => {
                    // AMBIGUOUS / LOW: leave for end-of-run pipeline.
                }
            }
        }

        Ok(assigned)
    }
}
