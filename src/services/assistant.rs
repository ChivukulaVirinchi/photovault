//! Assistant photo tools.
//!
//! This module is deliberately deterministic for the first Assistant release:
//! it turns a user request into typed photo filters, previews the candidate
//! photo set, and requires an approval token before creating anything. Provider
//! planning should drive these same typed tool boundaries instead of growing a
//! prompt-phrase table in application code.

use std::collections::HashSet;

use rusqlite::{
    params_from_iter, types::Value, Connection, OptionalExtension, Result as SqliteResult,
};
use serde::{Deserialize, Serialize};

use crate::db::album_repo::AlbumRepo;
use crate::search::date_parser::{DateParser, DateRange};

const MAX_PREVIEW_SAMPLE: usize = 12;
const MAX_ALBUM_PHOTOS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssistantRunStatus {
    Running,
    WaitingForApproval,
    WaitingForClarification,
    ResultsReady,
    Completed,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantActivity {
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPersonRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPlaceRef {
    pub city: Option<String>,
    pub country: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantDateRef {
    pub start: String,
    pub end: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPhotoSample {
    pub id: i64,
    pub thumbnail_path: Option<String>,
    pub date_taken: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantAlbumPreview {
    pub approval_id: String,
    pub album_name: String,
    pub photo_count: usize,
    pub sample: Vec<AssistantPhotoSample>,
    pub people: Vec<AssistantPersonRef>,
    pub places: Vec<AssistantPlaceRef>,
    pub date: Option<AssistantDateRef>,
    pub media_type: Option<String>,
    pub people_only: bool,
    pub semantic_text: Option<String>,
    pub intent: AssistantIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantRun {
    pub run_id: String,
    pub library_root: String,
    pub status: AssistantRunStatus,
    pub message: String,
    pub response: Option<String>,
    #[serde(default)]
    pub clarification_options: Vec<String>,
    pub activity: Vec<AssistantActivity>,
    pub preview: Option<AssistantAlbumPreview>,
    pub album_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AssistantDraft {
    pub album_name: String,
    pub photo_ids: Vec<i64>,
    pub preview: AssistantAlbumPreview,
}

#[derive(Debug, Clone, Default)]
pub struct AssistantRequestPlan {
    pub people: Vec<AssistantPersonRef>,
    pub places: Vec<AssistantPlaceRef>,
    pub date: Option<AssistantDateRef>,
    pub date_range: Option<DateRange>,
    pub media_type: Option<String>,
    pub people_only: bool,
    pub semantic_text: Option<String>,
    pub intent: AssistantIntent,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssistantSearchArgs {
    #[serde(default)]
    pub person_ids: Vec<i64>,
    #[serde(default)]
    pub places: Vec<AssistantPlaceRef>,
    pub date_phrase: Option<String>,
    pub media_type: Option<String>,
    #[serde(default)]
    pub people_only: bool,
    pub semantic_text: Option<String>,
    #[serde(default)]
    pub include_photo_ids: Vec<i64>,
    #[serde(default)]
    pub exclude_photo_ids: Vec<i64>,
    #[serde(default = "default_combine_mode")]
    pub combine_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantResolvedPeople {
    pub matches: Vec<AssistantPersonRef>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantResolvedPlaces {
    pub matches: Vec<AssistantPlaceRef>,
    pub missing: Vec<String>,
    pub candidates: Vec<AssistantPlaceCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPlaceCandidate {
    pub query: String,
    pub candidates: Vec<AssistantPlaceRef>,
}

fn default_combine_mode() -> String {
    "intersect".into()
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssistantIntent {
    CreateAlbum,
    #[default]
    Search,
}

pub struct AssistantService;

impl AssistantService {
    pub fn preview_album(
        conn: &Connection,
        message: &str,
        approval_id: String,
        semantic_photo_ids: &[i64],
    ) -> SqliteResult<(AssistantDraft, Vec<AssistantActivity>)> {
        Self::preview_album_with_activity(conn, message, approval_id, semantic_photo_ids, |_| {})
    }

    pub fn preview_album_with_activity(
        conn: &Connection,
        message: &str,
        approval_id: String,
        semantic_photo_ids: &[i64],
        mut on_activity: impl FnMut(&AssistantActivity),
    ) -> SqliteResult<(AssistantDraft, Vec<AssistantActivity>)> {
        let mut activity = Vec::new();
        push_activity(&mut activity, &mut on_activity, "Reading request");
        let plan = Self::plan_request(conn, message)?;
        if !plan.people.is_empty() {
            push_activity(
                &mut activity,
                &mut on_activity,
                format!(
                    "Resolving people: {}",
                    plan.people
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        if let Some(date) = &plan.date {
            push_activity(
                &mut activity,
                &mut on_activity,
                format!("Checking date range: {}", date.label),
            );
        }
        if !plan.places.is_empty() {
            push_activity(
                &mut activity,
                &mut on_activity,
                format!(
                    "Resolving places: {}",
                    plan.places
                        .iter()
                        .map(|p| p.label.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        if let Some(text) = &plan.semantic_text {
            push_activity(
                &mut activity,
                &mut on_activity,
                format!("Searching visual meaning: {text}"),
            );
        }
        push_activity(&mut activity, &mut on_activity, "Searching matching photos");

        let matches = Self::search_candidates(conn, &plan, semantic_photo_ids)?;
        push_activity(
            &mut activity,
            &mut on_activity,
            format!("Found {} photos", matches.len()),
        );

        let album_name = Self::album_name(message, &plan);
        let sample = matches
            .iter()
            .take(MAX_PREVIEW_SAMPLE)
            .map(|m| AssistantPhotoSample {
                id: m.id,
                thumbnail_path: m.thumbnail_path.clone(),
                date_taken: m.date_taken.clone(),
            })
            .collect();
        let preview = AssistantAlbumPreview {
            approval_id,
            album_name: album_name.clone(),
            photo_count: matches.len(),
            sample,
            people: plan.people.clone(),
            places: plan.places.clone(),
            date: plan.date.clone(),
            media_type: plan.media_type.clone(),
            people_only: plan.people_only,
            semantic_text: plan.semantic_text.clone(),
            intent: plan.intent,
        };
        push_activity(&mut activity, &mut on_activity, "Preparing preview");
        if plan.intent == AssistantIntent::CreateAlbum {
            push_activity(&mut activity, &mut on_activity, "Waiting for approval");
        }
        Ok((
            AssistantDraft {
                album_name,
                photo_ids: matches.into_iter().map(|m| m.id).collect(),
                preview,
            },
            activity,
        ))
    }

    pub fn create_album(conn: &Connection, draft: &AssistantDraft) -> SqliteResult<i64> {
        let repo = AlbumRepo::new(conn);
        let album_id = repo.create_with_source(&draft.album_name, "agent")?;
        repo.add_photos(album_id, &draft.photo_ids)?;
        repo.auto_pick_cover(album_id)?;
        Ok(album_id)
    }

    pub fn resolve_people_queries(
        conn: &Connection,
        queries: &[String],
    ) -> SqliteResult<AssistantResolvedPeople> {
        let all = load_people(conn)?;
        let mut matches = Vec::new();
        let mut missing = Vec::new();
        let mut seen = HashSet::new();
        for query in queries {
            let normalized = normalize_lookup(query);
            if normalized.is_empty() {
                continue;
            }
            let mut found = Vec::new();
            for person in &all {
                let name = normalize_lookup(&person.name);
                if name == normalized || name.contains(&normalized) || normalized.contains(&name) {
                    found.push(person.clone());
                }
            }
            if found.is_empty() {
                missing.push(query.clone());
            } else {
                found.sort_by_key(|p| p.name.len());
                for person in found.into_iter().take(3) {
                    if seen.insert(person.id) {
                        matches.push(person);
                    }
                }
            }
        }
        Ok(AssistantResolvedPeople { matches, missing })
    }

    pub fn resolve_place_queries(
        conn: &Connection,
        queries: &[String],
    ) -> SqliteResult<AssistantResolvedPlaces> {
        let all = load_places(conn)?;
        let mut matches = Vec::new();
        let mut missing = Vec::new();
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        for query in queries {
            let normalized = normalize_lookup(query);
            if normalized.is_empty() {
                continue;
            }
            let mut found = Vec::new();
            for place in &all {
                let label = normalize_lookup(&place.label);
                let city = place
                    .city
                    .as_deref()
                    .map(normalize_lookup)
                    .unwrap_or_default();
                let country = place
                    .country
                    .as_deref()
                    .map(normalize_lookup)
                    .unwrap_or_default();
                if label == normalized
                    || city == normalized
                    || country == normalized
                    || label.contains(&normalized)
                    || (!city.is_empty() && normalized.contains(&city))
                {
                    found.push(place.clone());
                }
            }
            if found.is_empty() {
                let fuzzy = place_candidates(&all, &normalized);
                if fuzzy.is_empty() {
                    missing.push(query.clone());
                } else {
                    candidates.push(AssistantPlaceCandidate {
                        query: query.clone(),
                        candidates: fuzzy,
                    });
                }
            } else {
                found.sort_by_key(|p| p.label.len());
                for place in found.into_iter().take(3) {
                    let key = place.label.to_lowercase();
                    if seen.insert(key) {
                        matches.push(place);
                    }
                }
            }
        }
        Ok(AssistantResolvedPlaces {
            matches,
            missing,
            candidates,
        })
    }

    pub fn resolve_date_phrase(phrase: &str) -> Option<AssistantDateRef> {
        DateParser::parse(phrase).map(|range| AssistantDateRef {
            start: range.start.to_rfc3339(),
            end: range.end.to_rfc3339(),
            label: phrase.trim().to_string(),
        })
    }

    pub fn search_with_args(
        conn: &Connection,
        args: &AssistantSearchArgs,
        semantic_photo_ids: &[i64],
    ) -> SqliteResult<AssistantDraft> {
        let date = args
            .date_phrase
            .as_deref()
            .and_then(DateParser::parse)
            .map(|range| AssistantDateRef {
                start: range.start.to_rfc3339(),
                end: range.end.to_rfc3339(),
                label: args.date_phrase.clone().unwrap_or_default(),
            });
        let date_range = args.date_phrase.as_deref().and_then(DateParser::parse);
        let mut people = Vec::new();
        for id in &args.person_ids {
            if let Some(person) = load_person(conn, *id)? {
                people.push(person);
            }
        }
        let plan = AssistantRequestPlan {
            people,
            places: args.places.clone(),
            date,
            date_range,
            media_type: args.media_type.clone(),
            people_only: args.people_only,
            semantic_text: args.semantic_text.clone(),
            intent: AssistantIntent::Search,
        };
        let mut matches = Self::search_candidates(conn, &plan, semantic_photo_ids)?;
        if !args.exclude_photo_ids.is_empty() {
            let exclude = args
                .exclude_photo_ids
                .iter()
                .copied()
                .collect::<HashSet<_>>();
            matches.retain(|m| !exclude.contains(&m.id));
        }
        if !args.include_photo_ids.is_empty() {
            let mut seen = matches.iter().map(|m| m.id).collect::<HashSet<_>>();
            if args.combine_mode == "union" {
                for id in &args.include_photo_ids {
                    if seen.insert(*id) {
                        if let Some(photo) = load_candidate_photo(conn, *id)? {
                            matches.push(photo);
                        }
                    }
                }
            } else {
                let include = args
                    .include_photo_ids
                    .iter()
                    .copied()
                    .collect::<HashSet<_>>();
                matches.retain(|m| include.contains(&m.id));
            }
        }
        let sample = matches
            .iter()
            .take(MAX_PREVIEW_SAMPLE)
            .map(|m| AssistantPhotoSample {
                id: m.id,
                thumbnail_path: m.thumbnail_path.clone(),
                date_taken: m.date_taken.clone(),
            })
            .collect();
        let album_name = "Assistant album".to_string();
        let preview = AssistantAlbumPreview {
            approval_id: String::new(),
            album_name: album_name.clone(),
            photo_count: matches.len(),
            sample,
            people: plan.people,
            places: plan.places,
            date: plan.date,
            media_type: plan.media_type,
            people_only: plan.people_only,
            semantic_text: plan.semantic_text,
            intent: AssistantIntent::Search,
        };
        Ok(AssistantDraft {
            album_name,
            photo_ids: matches.into_iter().map(|m| m.id).collect(),
            preview,
        })
    }

    pub fn preview_from_photo_ids(
        conn: &Connection,
        photo_ids: &[i64],
        album_name: &str,
        approval_id: String,
        intent: AssistantIntent,
    ) -> SqliteResult<AssistantDraft> {
        let mut matches = Vec::new();
        let mut seen = HashSet::new();
        for id in photo_ids.iter().take(MAX_ALBUM_PHOTOS) {
            if seen.insert(*id) {
                if let Some(photo) = load_candidate_photo(conn, *id)? {
                    matches.push(photo);
                }
            }
        }
        let sample = matches
            .iter()
            .take(MAX_PREVIEW_SAMPLE)
            .map(|m| AssistantPhotoSample {
                id: m.id,
                thumbnail_path: m.thumbnail_path.clone(),
                date_taken: m.date_taken.clone(),
            })
            .collect();
        let clean_name = sanitize_album_name(album_name);
        let preview = AssistantAlbumPreview {
            approval_id,
            album_name: clean_name.clone(),
            photo_count: matches.len(),
            sample,
            people: Vec::new(),
            places: Vec::new(),
            date: None,
            media_type: None,
            people_only: false,
            semantic_text: None,
            intent,
        };
        Ok(AssistantDraft {
            album_name: clean_name,
            photo_ids: matches.into_iter().map(|m| m.id).collect(),
            preview,
        })
    }

    pub fn plan_request(conn: &Connection, message: &str) -> SqliteResult<AssistantRequestPlan> {
        let mut plan = AssistantRequestPlan {
            intent: request_intent(message),
            ..Default::default()
        };
        let mut text = remove_tool_words(message, plan.intent);
        let lower = text.to_lowercase();
        plan.people_only = lower.contains(" only ") || lower.starts_with("only ");

        if lower.contains(" video") || lower.contains(" videos") {
            plan.media_type = Some("video".into());
            text = remove_words(&text, &["video", "videos"]);
        } else if lower.contains(" photo")
            || lower.contains(" photos")
            || lower.contains(" pictures")
        {
            plan.media_type = Some("photo".into());
            text = remove_words(&text, &["photo", "photos", "picture", "pictures"]);
        }

        if let Some((date_label, range)) = extract_date(&text) {
            text = remove_phrase(&text, &date_label);
            plan.date = Some(AssistantDateRef {
                start: range.start.to_rfc3339(),
                end: range.end.to_rfc3339(),
                label: date_label,
            });
            plan.date_range = Some(range);
        }

        let people = resolve_people(conn, &text)?;
        for person in &people {
            text = remove_phrase(&text, &person.name);
        }
        plan.people = people;

        let places = resolve_places(conn, &text)?;
        for place in &places {
            text = remove_phrase(&text, &place.label);
        }
        plan.places = places;

        let semantic = clean_semantic_text(&text);
        if !semantic.is_empty() {
            plan.semantic_text = Some(semantic);
        }
        Ok(plan)
    }

    fn search_candidates(
        conn: &Connection,
        plan: &AssistantRequestPlan,
        semantic_photo_ids: &[i64],
    ) -> SqliteResult<Vec<CandidatePhoto>> {
        let mut sql = String::from(
            "SELECT p.id, p.thumbnail_path, p.date_taken FROM photos p WHERE p.is_trashed = FALSE",
        );
        let mut bind: Vec<Value> = Vec::new();

        if let Some(range) = &plan.date_range {
            sql.push_str(" AND p.date_taken >= ? AND p.date_taken <= ?");
            bind.push(Value::Text(range.start.to_rfc3339()));
            bind.push(Value::Text(range.end.to_rfc3339()));
        }
        if let Some(media) = &plan.media_type {
            sql.push_str(" AND p.media_type = ?");
            bind.push(Value::Text(media.clone()));
        }
        for place in &plan.places {
            if let Some(city) = &place.city {
                sql.push_str(" AND LOWER(p.location_city) = LOWER(?)");
                bind.push(Value::Text(city.clone()));
            }
            if let Some(country) = &place.country {
                sql.push_str(" AND LOWER(p.location_country) = LOWER(?)");
                bind.push(Value::Text(country.clone()));
            }
        }
        for person in &plan.people {
            sql.push_str(
                " AND (EXISTS (
                    SELECT 1 FROM faces f
                    WHERE f.photo_id = p.id AND f.cluster_id = ?
                ) OR EXISTS (
                    SELECT 1 FROM photo_inferred_identities pii
                    WHERE pii.photo_id = p.id AND pii.cluster_id = ?
                ))",
            );
            bind.push(Value::Integer(person.id));
            bind.push(Value::Integer(person.id));
        }
        if plan.people_only && !plan.people.is_empty() {
            sql.push_str(" AND p.faces_processed = TRUE");
            let placeholders = std::iter::repeat_n("?", plan.people.len())
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(
                " AND NOT EXISTS (
                    SELECT 1 FROM faces f
                    WHERE f.photo_id = p.id
                      AND (f.cluster_id IS NULL OR f.cluster_id NOT IN ({placeholders}))
                )"
            ));
            for person in &plan.people {
                bind.push(Value::Integer(person.id));
            }
            sql.push_str(&format!(
                " AND NOT EXISTS (
                    SELECT 1 FROM photo_inferred_identities pii
                    WHERE pii.photo_id = p.id AND pii.cluster_id NOT IN ({placeholders})
                )"
            ));
            for person in &plan.people {
                bind.push(Value::Integer(person.id));
            }
        }
        if !semantic_photo_ids.is_empty() {
            sql.push_str(&format!(
                " AND p.id IN ({})",
                std::iter::repeat_n("?", semantic_photo_ids.len())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            for id in semantic_photo_ids {
                bind.push(Value::Integer(*id));
            }
        } else if let Some(text) = &plan.semantic_text {
            sql.push_str(
                " AND (
                    LOWER(p.file_name) LIKE LOWER(?) OR
                    LOWER(p.location_city) LIKE LOWER(?) OR
                    LOWER(p.location_country) LIKE LOWER(?) OR
                    LOWER(p.camera_make) LIKE LOWER(?) OR
                    LOWER(p.camera_model) LIKE LOWER(?)
                )",
            );
            let like = Value::Text(format!("%{text}%"));
            for _ in 0..5 {
                bind.push(like.clone());
            }
        }

        sql.push_str(&format!(
            " ORDER BY p.date_taken DESC, p.id DESC LIMIT {}",
            MAX_ALBUM_PHOTOS
        ));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bind.iter()), |row| {
            Ok(CandidatePhoto {
                id: row.get(0)?,
                thumbnail_path: row.get(1)?,
                date_taken: row.get(2)?,
            })
        })?;
        let mut out = rows.collect::<SqliteResult<Vec<_>>>()?;
        if !semantic_photo_ids.is_empty() {
            let rank = semantic_photo_ids
                .iter()
                .enumerate()
                .map(|(idx, id)| (*id, idx))
                .collect::<std::collections::HashMap<_, _>>();
            out.sort_by_key(|p| rank.get(&p.id).copied().unwrap_or(usize::MAX));
        }
        Ok(out)
    }

    fn album_name(message: &str, plan: &AssistantRequestPlan) -> String {
        if let Some(name) = explicit_album_name(message) {
            return name;
        }
        let mut parts = Vec::new();
        if !plan.people.is_empty() {
            parts.push(
                plan.people
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" and "),
            );
        }
        if !plan.places.is_empty() {
            parts.push(plan.places[0].label.clone());
        }
        if let Some(date) = &plan.date {
            parts.push(date.label.clone());
        }
        if let Some(text) = &plan.semantic_text {
            parts.push(title_case(text));
        }
        if parts.is_empty() {
            "Assistant album".into()
        } else {
            let title = parts.join(", ");
            match plan.intent {
                AssistantIntent::CreateAlbum => title,
                AssistantIntent::Search => format!("{title} photos"),
            }
        }
    }
}

fn push_activity(
    activity: &mut Vec<AssistantActivity>,
    on_activity: &mut impl FnMut(&AssistantActivity),
    label: impl Into<String>,
) {
    let item = AssistantActivity {
        label: label.into(),
    };
    on_activity(&item);
    activity.push(item);
}

#[derive(Debug)]
struct CandidatePhoto {
    id: i64,
    thumbnail_path: Option<String>,
    date_taken: Option<String>,
}

fn request_intent(message: &str) -> AssistantIntent {
    let tokens = normalized_tokens(message);
    let asks_for_album = tokens.iter().any(|t| t == "album" || t == "albums");
    let asks_to_create = tokens.iter().any(|t| {
        matches!(
            t.as_str(),
            "create" | "make" | "build" | "generate" | "save" | "collect"
        )
    });
    if asks_for_album && asks_to_create {
        AssistantIntent::CreateAlbum
    } else {
        AssistantIntent::Search
    }
}

fn remove_tool_words(text: &str, intent: AssistantIntent) -> String {
    text.split_whitespace()
        .filter(|word| {
            let token = clean_token(word);
            if token.is_empty() {
                return false;
            }
            let is_common_tool_word = matches!(
                token.as_str(),
                "find"
                    | "show"
                    | "search"
                    | "get"
                    | "give"
                    | "list"
                    | "all"
                    | "my"
                    | "photo"
                    | "photos"
                    | "picture"
                    | "pictures"
            );
            let is_album_tool_word = matches!(
                token.as_str(),
                "create"
                    | "make"
                    | "build"
                    | "generate"
                    | "save"
                    | "collect"
                    | "album"
                    | "albums"
                    | "collection"
            );
            !(is_common_tool_word || intent == AssistantIntent::CreateAlbum && is_album_tool_word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(clean_token)
        .filter(|token| !token.is_empty())
        .collect()
}

fn clean_token(text: &str) -> String {
    text.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn resolve_people(conn: &Connection, text: &str) -> SqliteResult<Vec<AssistantPersonRef>> {
    let lower = format!(" {} ", text.to_lowercase());
    let rows = load_people(conn)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for person in rows {
        let needle = format!(" {} ", person.name.to_lowercase());
        if lower.contains(&needle) && seen.insert(person.id) {
            out.push(person);
        }
    }
    Ok(out)
}

fn resolve_places(conn: &Connection, text: &str) -> SqliteResult<Vec<AssistantPlaceRef>> {
    let lower = format!(" {} ", text.to_lowercase());
    let rows = load_places(conn)?;
    let mut out = Vec::new();
    let mut seen_countries = HashSet::new();
    for place in rows {
        let city_match = place
            .city
            .as_ref()
            .map(|c| lower.contains(&format!(" {} ", c.to_lowercase())))
            .unwrap_or(false);
        let country_match = place
            .country
            .as_ref()
            .map(|c| lower.contains(&format!(" {} ", c.to_lowercase())))
            .unwrap_or(false);
        if city_match {
            out.push(place);
        } else if country_match {
            let Some(country) = place.country.clone() else {
                continue;
            };
            if seen_countries.insert(country.to_lowercase()) {
                out.push(AssistantPlaceRef {
                    city: None,
                    label: country.clone(),
                    country: Some(country),
                });
            }
        }
    }
    Ok(out)
}

fn load_people(conn: &Connection) -> SqliteResult<Vec<AssistantPersonRef>> {
    let mut stmt = conn.prepare(
        "SELECT id, name FROM face_clusters
         WHERE name IS NOT NULL AND TRIM(name) != '' AND photo_count > 0
         ORDER BY LENGTH(name) DESC, photo_count DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(AssistantPersonRef {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    rows.collect()
}

fn load_person(conn: &Connection, id: i64) -> SqliteResult<Option<AssistantPersonRef>> {
    conn.query_row(
        "SELECT id, name FROM face_clusters
         WHERE id = ?1 AND name IS NOT NULL AND TRIM(name) != ''",
        [id],
        |row| {
            Ok(AssistantPersonRef {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        },
    )
    .optional()
}

fn load_places(conn: &Connection) -> SqliteResult<Vec<AssistantPlaceRef>> {
    let mut stmt = conn.prepare(
        "SELECT location_city, location_country, COUNT(*) AS n
         FROM photos
         WHERE is_trashed = FALSE
           AND (location_city IS NOT NULL OR location_country IS NOT NULL)
         GROUP BY location_city, location_country
         ORDER BY n DESC
         LIMIT 500",
    )?;
    let rows = stmt.query_map([], |row| {
        let city: Option<String> = row.get(0)?;
        let country: Option<String> = row.get(1)?;
        let label = match (&city, &country) {
            (Some(c), Some(country)) => format!("{c}, {country}"),
            (Some(c), None) => c.clone(),
            (None, Some(country)) => country.clone(),
            (None, None) => String::new(),
        };
        Ok(AssistantPlaceRef {
            city,
            country,
            label,
        })
    })?;
    rows.collect()
}

fn load_candidate_photo(conn: &Connection, id: i64) -> SqliteResult<Option<CandidatePhoto>> {
    conn.query_row(
        "SELECT id, thumbnail_path, date_taken
         FROM photos
         WHERE id = ?1 AND is_trashed = FALSE",
        [id],
        |row| {
            Ok(CandidatePhoto {
                id: row.get(0)?,
                thumbnail_path: row.get(1)?,
                date_taken: row.get(2)?,
            })
        },
    )
    .optional()
}

fn normalize_lookup(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn place_candidates(all: &[AssistantPlaceRef], normalized: &str) -> Vec<AssistantPlaceRef> {
    if normalized.len() < 2 {
        return Vec::new();
    }
    let first = normalized.chars().next();
    let mut scored = all
        .iter()
        .filter_map(|place| {
            let city = place
                .city
                .as_deref()
                .map(normalize_lookup)
                .unwrap_or_default();
            let label = normalize_lookup(&place.label);
            if city.is_empty() && label.is_empty() {
                return None;
            }
            let score = if city.starts_with(normalized) || label.starts_with(normalized) {
                0
            } else if first.is_some_and(|c| city.starts_with(c) || label.starts_with(c))
                && (city.contains(normalized)
                    || label.contains(normalized)
                    || normalized.len() <= 4)
            {
                1
            } else {
                return None;
            };
            Some((score, place.label.len(), place.clone()))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, len, _)| (*score, *len));
    scored
        .into_iter()
        .map(|(_, _, place)| place)
        .take(5)
        .collect()
}

fn sanitize_album_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        "Assistant album".into()
    } else {
        cleaned.chars().take(80).collect()
    }
}

fn extract_date(text: &str) -> Option<(String, DateRange)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for window in (1..=3).rev() {
        if words.len() < window {
            continue;
        }
        for i in 0..=(words.len() - window) {
            let candidate = words[i..i + window].join(" ");
            if let Some(range) = DateParser::parse(&candidate) {
                return Some((candidate, range));
            }
        }
    }
    None
}

fn explicit_album_name(message: &str) -> Option<String> {
    let lower = message.to_lowercase();
    for marker in ["called ", "named "] {
        if let Some(idx) = lower.find(marker) {
            let name = message[idx + marker.len()..].trim();
            if !name.is_empty() {
                return Some(title_case(name));
            }
        }
    }
    None
}

fn clean_semantic_text(text: &str) -> String {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| {
            let lower = w.to_lowercase();
            !lower.is_empty()
                && !matches!(
                    lower.as_str(),
                    "and"
                        | "or"
                        | "the"
                        | "a"
                        | "an"
                        | "me"
                        | "my"
                        | "only"
                        | "just"
                        | "in"
                        | "at"
                        | "to"
                        | "of"
                )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn remove_words(text: &str, words: &[&str]) -> String {
    text.split_whitespace()
        .filter(|w| {
            let cleaned = w
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            !words.iter().any(|needle| *needle == cleaned)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn remove_phrase(text: &str, phrase: &str) -> String {
    let lower = text.to_lowercase();
    let phrase_lower = phrase.to_lowercase();
    if let Some(idx) = lower.find(&phrase_lower) {
        let end = idx + phrase.len();
        format!("{} {}", &text[..idx], &text[end..])
            .trim()
            .to_string()
    } else {
        text.to_string()
    }
}

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE photos (
                id INTEGER PRIMARY KEY,
                file_name TEXT NOT NULL,
                thumbnail_path TEXT,
                date_taken TEXT,
                media_type TEXT NOT NULL DEFAULT 'photo',
                location_city TEXT,
                location_country TEXT,
                camera_make TEXT,
                camera_model TEXT,
                width INTEGER,
                height INTEGER,
                faces_processed BOOLEAN DEFAULT TRUE,
                is_trashed BOOLEAN DEFAULT FALSE
            );
            CREATE TABLE face_clusters (
                id INTEGER PRIMARY KEY,
                name TEXT,
                photo_count INTEGER DEFAULT 0
            );
            CREATE TABLE faces (
                id INTEGER PRIMARY KEY,
                photo_id INTEGER NOT NULL,
                cluster_id INTEGER
            );
            CREATE TABLE photo_inferred_identities (
                id INTEGER PRIMARY KEY,
                photo_id INTEGER NOT NULL,
                cluster_id INTEGER NOT NULL
            );
            CREATE TABLE albums (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                cover_photo_id INTEGER,
                cover_auto_picked BOOLEAN DEFAULT TRUE,
                photo_count INTEGER DEFAULT 0,
                created_by TEXT NOT NULL DEFAULT 'user',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE album_photos (
                id INTEGER PRIMARY KEY,
                album_id INTEGER NOT NULL,
                photo_id INTEGER NOT NULL,
                UNIQUE(album_id, photo_id)
            );
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn preview_filters_people_date_and_place() {
        let conn = conn();
        conn.execute(
            "INSERT INTO face_clusters (id, name, photo_count) VALUES (1, 'me', 2), (2, 'mom', 2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_name, date_taken, location_city, location_country) VALUES
             (1, 'a.jpg', '2014-05-01T00:00:00Z', 'Goa', 'India'),
             (2, 'b.jpg', '2015-05-01T00:00:00Z', 'Goa', 'India'),
             (3, 'c.jpg', '2014-05-01T00:00:00Z', 'Mumbai', 'India')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO faces (photo_id, cluster_id) VALUES (1, 1), (1, 2), (2, 1), (2, 2), (3, 1), (3, 2)", []).unwrap();

        let (draft, _) = AssistantService::preview_album(
            &conn,
            "make an album of me and mom Goa 2014",
            "a1".into(),
            &[],
        )
        .unwrap();
        assert_eq!(draft.photo_ids, vec![1]);
        assert_eq!(draft.preview.people.len(), 2);
        assert_eq!(draft.preview.places[0].label, "Goa, India");
    }

    #[test]
    fn people_only_excludes_unknown_faces() {
        let conn = conn();
        conn.execute(
            "INSERT INTO face_clusters (id, name, photo_count) VALUES (1, 'me', 2), (2, 'mom', 2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO photos (id, file_name, date_taken, faces_processed) VALUES
             (1, 'a.jpg', '2014-05-01T00:00:00Z', TRUE),
             (2, 'b.jpg', '2014-05-01T00:00:00Z', TRUE)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO faces (photo_id, cluster_id) VALUES (1, 1), (1, 2), (2, 1), (2, 2), (2, NULL)", []).unwrap();

        let (draft, _) = AssistantService::preview_album(
            &conn,
            "make album only me and mom 2014",
            "a1".into(),
            &[],
        )
        .unwrap();
        assert_eq!(draft.photo_ids, vec![1]);
    }

    #[test]
    fn create_album_marks_agent_source() {
        let conn = conn();
        conn.execute("INSERT INTO photos (id, file_name, date_taken) VALUES (1, 'a.jpg', '2014-05-01T00:00:00Z')", []).unwrap();
        let (draft, _) =
            AssistantService::preview_album(&conn, "make album 2014", "a1".into(), &[]).unwrap();
        let id = AssistantService::create_album(&conn, &draft).unwrap();
        let created_by: String = conn
            .query_row(
                "SELECT created_by FROM albums WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(created_by, "agent");
    }

    #[test]
    fn country_only_place_filter_does_not_require_one_city() {
        let conn = conn();
        conn.execute(
            "INSERT INTO photos (id, file_name, date_taken, location_city, location_country) VALUES
             (1, 'a.jpg', '2014-05-01T00:00:00Z', 'Goa', 'India'),
             (2, 'b.jpg', '2014-06-01T00:00:00Z', 'Mumbai', 'India'),
             (3, 'c.jpg', '2014-06-01T00:00:00Z', 'Paris', 'France')",
            [],
        )
        .unwrap();

        let (draft, _) =
            AssistantService::preview_album(&conn, "make album India 2014", "a1".into(), &[])
                .unwrap();
        assert_eq!(draft.photo_ids, vec![2, 1]);
        assert_eq!(draft.preview.places[0].label, "India");
    }

    #[test]
    fn place_query_does_not_match_empty_city() {
        let conn = conn();
        conn.execute(
            "INSERT INTO photos (id, file_name, date_taken, location_country) VALUES
             (1, 'a.jpg', '2014-05-01T00:00:00Z', 'India')",
            [],
        )
        .unwrap();

        let resolved = AssistantService::resolve_place_queries(&conn, &["Goa".into()]).unwrap();
        assert!(resolved.matches.is_empty());
    }

    #[test]
    fn search_request_gets_clean_result_title_and_search_intent() {
        let conn = conn();
        conn.execute(
            "INSERT INTO photos (id, file_name, date_taken) VALUES
             (1, 'beach-family.jpg', '2024-05-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let (draft, _) = AssistantService::preview_album(
            &conn,
            "find me all photos at a beach",
            "a1".into(),
            &[],
        )
        .unwrap();
        assert_eq!(draft.preview.intent, AssistantIntent::Search);
        assert_eq!(draft.album_name, "Beach photos");
        assert_eq!(draft.photo_ids, vec![1]);
    }

    #[test]
    fn plain_request_searches_by_default() {
        let conn = conn();
        conn.execute(
            "INSERT INTO photos (id, file_name, date_taken, location_city) VALUES
             (1, 'a.jpg', '2024-05-01T00:00:00Z', 'Goa')",
            [],
        )
        .unwrap();

        let (draft, _) =
            AssistantService::preview_album(&conn, "Goa 2024", "a1".into(), &[]).unwrap();
        assert_eq!(draft.preview.intent, AssistantIntent::Search);
        assert_eq!(draft.album_name, "Goa, 2024 photos");
        assert_eq!(draft.photo_ids, vec![1]);
    }

    #[test]
    fn album_creation_requires_explicit_create_album_intent() {
        assert_eq!(request_intent("photos of Goa"), AssistantIntent::Search);
        assert_eq!(request_intent("album of Goa"), AssistantIntent::Search);
        assert_eq!(
            request_intent("create album of Goa"),
            AssistantIntent::CreateAlbum
        );
        assert_eq!(
            request_intent("make a Goa album"),
            AssistantIntent::CreateAlbum
        );
    }
}
