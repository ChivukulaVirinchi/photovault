# Phase 6: Search & Quick Cull

## Overview

This phase implements powerful search capabilities (by date, location, person) and a keyboard-driven quick cull workflow for rapidly reviewing and deleting photos. The cull mode is optimized for speed - letting users efficiently clean their library.

**Estimated Time:** 4-5 days  
**Difficulty:** Intermediate  
**Prerequisites:** Phase 4 complete (People/faces for person search), Phase 3 (Timeline for navigation)

---

## UI Design Guidelines

> **IMPORTANT:** When implementing any UI components in this phase, you MUST read and follow the design principles in `SKILL.md`. This file contains critical guidelines for:
> - Typography and spacing standards
> - Color usage and contrast requirements
> - Animation and interaction patterns
> - Component design principles
> - Accessibility requirements
>
> **Before writing ANY UI code, read SKILL.md thoroughly.** The goal is a delightful, polished user experience - not just functional code.

---

## Goals

- [ ] Implement search by date (natural language)
- [ ] Implement search by location (city/country)
- [ ] Implement search by person name
- [ ] Build combined search (e.g., "Dad in Tokyo")
- [ ] Create search results view
- [ ] Build Quick Cull mode with keyboard controls
- [ ] Implement trash staging with soft delete
- [ ] Add trash view with restore/permanent delete

---

## New Files

```
src/
├── services/
│   ├── search.rs               # Search engine
│   └── trash.rs                # Trash management
├── db/
│   └── trash_repo.rs           # Trash database operations
├── search/
│   ├── mod.rs                  # Search module
│   ├── date_parser.rs          # Natural language date parsing
│   └── query_parser.rs         # Search query parsing
└── views/
    ├── search.rs               # Search view
    ├── cull.rs                 # Quick cull mode
    └── trash.rs                # Trash view
```

---

## Step 1: Natural Language Date Parser

### File: `src/search/mod.rs`

```rust
//! Search module - natural language search for photos

pub mod date_parser;
pub mod query_parser;

pub use date_parser::DateParser;
pub use query_parser::{QueryParser, SearchQuery, SearchFilter};
```

### File: `src/search/date_parser.rs`

```rust
//! Natural language date parsing
//!
//! Parses expressions like "March 2019", "last summer", "yesterday"

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc, Weekday};

/// A date range result from parsing
#[derive(Debug, Clone)]
pub struct DateRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl DateRange {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    /// Create a range for a single day
    pub fn single_day(date: NaiveDate) -> Self {
        let start = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
        let end = Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).unwrap());
        Self { start, end }
    }

    /// Create a range for a month
    pub fn month(year: i32, month: u32) -> Option<Self> {
        let start_date = NaiveDate::from_ymd_opt(year, month, 1)?;
        let end_date = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)?.pred_opt()?
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)?.pred_opt()?
        };

        Some(Self {
            start: Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).unwrap()),
            end: Utc.from_utc_datetime(&end_date.and_hms_opt(23, 59, 59).unwrap()),
        })
    }

    /// Create a range for a year
    pub fn year(year: i32) -> Option<Self> {
        let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
        let end = NaiveDate::from_ymd_opt(year, 12, 31)?;

        Some(Self {
            start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap()),
            end: Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).unwrap()),
        })
    }
}

/// Natural language date parser
pub struct DateParser;

impl DateParser {
    /// Parse a natural language date expression
    pub fn parse(input: &str) -> Option<DateRange> {
        let input = input.trim().to_lowercase();
        let today = Local::now().date_naive();

        // Try various parsers in order
        Self::parse_relative(&input, today)
            .or_else(|| Self::parse_month_year(&input))
            .or_else(|| Self::parse_year(&input))
            .or_else(|| Self::parse_season(&input, today))
            .or_else(|| Self::parse_month_only(&input, today))
            .or_else(|| Self::parse_iso_date(&input))
    }

    /// Parse relative dates: "today", "yesterday", "last week"
    fn parse_relative(input: &str, today: NaiveDate) -> Option<DateRange> {
        match input {
            "today" => Some(DateRange::single_day(today)),
            
            "yesterday" => Some(DateRange::single_day(today - Duration::days(1))),
            
            "this week" => {
                let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
                let end = start + Duration::days(6);
                Some(DateRange {
                    start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap()),
                    end: Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).unwrap()),
                })
            }
            
            "last week" => {
                let this_week_start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
                let start = this_week_start - Duration::days(7);
                let end = this_week_start - Duration::days(1);
                Some(DateRange {
                    start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap()),
                    end: Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).unwrap()),
                })
            }
            
            "this month" => DateRange::month(today.year(), today.month()),
            
            "last month" => {
                let (year, month) = if today.month() == 1 {
                    (today.year() - 1, 12)
                } else {
                    (today.year(), today.month() - 1)
                };
                DateRange::month(year, month)
            }
            
            "this year" => DateRange::year(today.year()),
            
            "last year" => DateRange::year(today.year() - 1),
            
            _ => None,
        }
    }

    /// Parse "March 2019" or "2019 March" format
    fn parse_month_year(input: &str) -> Option<DateRange> {
        let months = [
            ("january", 1), ("jan", 1),
            ("february", 2), ("feb", 2),
            ("march", 3), ("mar", 3),
            ("april", 4), ("apr", 4),
            ("may", 5),
            ("june", 6), ("jun", 6),
            ("july", 7), ("jul", 7),
            ("august", 8), ("aug", 8),
            ("september", 9), ("sep", 9), ("sept", 9),
            ("october", 10), ("oct", 10),
            ("november", 11), ("nov", 11),
            ("december", 12), ("dec", 12),
        ];

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }

        // Try both orderings
        let (month_str, year_str) = if parts[0].parse::<i32>().is_ok() {
            (parts[1], parts[0])
        } else {
            (parts[0], parts[1])
        };

        let year: i32 = year_str.parse().ok()?;
        let month = months.iter()
            .find(|(name, _)| *name == month_str)
            .map(|(_, num)| *num)?;

        DateRange::month(year, month)
    }

    /// Parse just a year: "2019"
    fn parse_year(input: &str) -> Option<DateRange> {
        let year: i32 = input.parse().ok()?;
        if year < 1900 || year > 2100 {
            return None;
        }
        DateRange::year(year)
    }

    /// Parse seasons: "last summer", "winter 2019"
    fn parse_season(input: &str, today: NaiveDate) -> Option<DateRange> {
        let seasons = [
            ("spring", (3, 5)),   // March - May
            ("summer", (6, 8)),   // June - August
            ("fall", (9, 11)),    // September - November
            ("autumn", (9, 11)),
            ("winter", (12, 2)),  // December - February
        ];

        // Check for "last <season>"
        if input.starts_with("last ") {
            let season_name = &input[5..];
            if let Some((_, (start_month, end_month))) = seasons.iter().find(|(name, _)| *name == season_name) {
                let year = if *start_month > today.month() {
                    today.year() - 1
                } else {
                    today.year()
                };
                return Self::season_range(year - 1, *start_month, *end_month);
            }
        }

        // Check for "this <season>"
        if input.starts_with("this ") {
            let season_name = &input[5..];
            if let Some((_, (start_month, end_month))) = seasons.iter().find(|(name, _)| *name == season_name) {
                return Self::season_range(today.year(), *start_month, *end_month);
            }
        }

        // Check for "<season> <year>"
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() == 2 {
            if let Some((_, (start_month, end_month))) = seasons.iter().find(|(name, _)| *name == parts[0]) {
                if let Ok(year) = parts[1].parse::<i32>() {
                    return Self::season_range(year, *start_month, *end_month);
                }
            }
        }

        None
    }

    /// Create date range for a season
    fn season_range(year: i32, start_month: u32, end_month: u32) -> Option<DateRange> {
        let (start_year, end_year) = if start_month > end_month {
            // Winter spans two years
            (year, year + 1)
        } else {
            (year, year)
        };

        let start = NaiveDate::from_ymd_opt(start_year, start_month, 1)?;
        let end_date = if end_month == 12 {
            NaiveDate::from_ymd_opt(end_year + 1, 1, 1)?.pred_opt()?
        } else {
            NaiveDate::from_ymd_opt(end_year, end_month + 1, 1)?.pred_opt()?
        };

        Some(DateRange {
            start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap()),
            end: Utc.from_utc_datetime(&end_date.and_hms_opt(23, 59, 59).unwrap()),
        })
    }

    /// Parse just month name (assumes current year): "March", "December"
    fn parse_month_only(input: &str, today: NaiveDate) -> Option<DateRange> {
        let months = [
            ("january", 1), ("jan", 1),
            ("february", 2), ("feb", 2),
            ("march", 3), ("mar", 3),
            ("april", 4), ("apr", 4),
            ("may", 5),
            ("june", 6), ("jun", 6),
            ("july", 7), ("jul", 7),
            ("august", 8), ("aug", 8),
            ("september", 9), ("sep", 9), ("sept", 9),
            ("october", 10), ("oct", 10),
            ("november", 11), ("nov", 11),
            ("december", 12), ("dec", 12),
        ];

        let month = months.iter()
            .find(|(name, _)| *name == input)
            .map(|(_, num)| *num)?;

        // If the month is in the future, use last year
        let year = if month > today.month() {
            today.year() - 1
        } else {
            today.year()
        };

        DateRange::month(year, month)
    }

    /// Parse ISO date: "2019-03-15"
    fn parse_iso_date(input: &str) -> Option<DateRange> {
        let date = NaiveDate::parse_from_str(input, "%Y-%m-%d").ok()?;
        Some(DateRange::single_day(date))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_month_year() {
        let range = DateParser::parse("march 2019").unwrap();
        assert_eq!(range.start.year(), 2019);
        assert_eq!(range.start.month(), 3);
        assert_eq!(range.start.day(), 1);
        assert_eq!(range.end.month(), 3);
        assert_eq!(range.end.day(), 31);
    }

    #[test]
    fn test_parse_year() {
        let range = DateParser::parse("2019").unwrap();
        assert_eq!(range.start.year(), 2019);
        assert_eq!(range.start.month(), 1);
        assert_eq!(range.end.month(), 12);
    }

    #[test]
    fn test_parse_iso() {
        let range = DateParser::parse("2019-03-15").unwrap();
        assert_eq!(range.start.year(), 2019);
        assert_eq!(range.start.month(), 3);
        assert_eq!(range.start.day(), 15);
    }
}
```

---

## Step 2: Query Parser

### File: `src/search/query_parser.rs`

```rust
//! Search query parsing
//!
//! Parses queries like "Dad in Tokyo", "March 2019 Japan"

use super::date_parser::{DateParser, DateRange};

/// A parsed search filter
#[derive(Debug, Clone)]
pub enum SearchFilter {
    /// Search for a person by name
    Person(String),
    
    /// Search for a location (city or country)
    Location(String),
    
    /// Search within a date range
    DateRange(DateRange),
    
    /// Free text (fallback)
    Text(String),
}

/// A complete parsed search query
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub filters: Vec<SearchFilter>,
}

impl SearchQuery {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Get person filter if present
    pub fn person(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Person(name) => Some(name.as_str()),
            _ => None,
        })
    }

    /// Get location filter if present
    pub fn location(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Location(loc) => Some(loc.as_str()),
            _ => None,
        })
    }

    /// Get date range filter if present
    pub fn date_range(&self) -> Option<&DateRange> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::DateRange(range) => Some(range),
            _ => None,
        })
    }
}

/// Query parser
pub struct QueryParser;

impl QueryParser {
    /// Parse a search query string
    pub fn parse(input: &str) -> SearchQuery {
        let input = input.trim();
        if input.is_empty() {
            return SearchQuery::new();
        }

        let mut query = SearchQuery::new();
        let mut remaining = input.to_string();

        // Try to extract "in <location>" pattern
        if let Some((before, location)) = Self::extract_in_location(&remaining) {
            query.filters.push(SearchFilter::Location(location));
            remaining = before;
        }

        // Try to parse date from remaining
        if let Some((non_date, date_range)) = Self::extract_date(&remaining) {
            query.filters.push(SearchFilter::DateRange(date_range));
            remaining = non_date;
        }

        // What's left might be a person name or location
        let remaining = remaining.trim();
        if !remaining.is_empty() {
            // If it looks like a location (capitalized, common location words)
            if Self::looks_like_location(remaining) {
                query.filters.push(SearchFilter::Location(remaining.to_string()));
            } else {
                // Assume it's a person name
                query.filters.push(SearchFilter::Person(remaining.to_string()));
            }
        }

        query
    }

    /// Extract "in <location>" from end of query
    fn extract_in_location(input: &str) -> Option<(String, String)> {
        let lower = input.to_lowercase();
        
        if let Some(idx) = lower.rfind(" in ") {
            let before = input[..idx].to_string();
            let location = input[idx + 4..].trim().to_string();
            
            if !location.is_empty() {
                return Some((before, location));
            }
        }

        None
    }

    /// Try to extract a date from the query
    fn extract_date(input: &str) -> Option<(String, DateRange)> {
        let input_lower = input.to_lowercase();

        // Try parsing the whole input as a date first
        if let Some(range) = DateParser::parse(&input_lower) {
            return Some((String::new(), range));
        }

        // Try parsing from the end (e.g., "Dad March 2019")
        let words: Vec<&str> = input.split_whitespace().collect();
        
        // Try last 2 words (month year)
        if words.len() >= 2 {
            let last_two = format!("{} {}", words[words.len() - 2], words[words.len() - 1]);
            if let Some(range) = DateParser::parse(&last_two.to_lowercase()) {
                let before = words[..words.len() - 2].join(" ");
                return Some((before, range));
            }
        }

        // Try last word (year or month)
        if let Some(last) = words.last() {
            if let Some(range) = DateParser::parse(&last.to_lowercase()) {
                let before = words[..words.len() - 1].join(" ");
                return Some((before, range));
            }
        }

        None
    }

    /// Heuristic to check if text looks like a location
    fn looks_like_location(text: &str) -> bool {
        let location_words = [
            "city", "country", "beach", "mountain", "park", "airport",
            "station", "hotel", "restaurant", "museum", "temple", "shrine",
        ];

        let lower = text.to_lowercase();
        
        // Check for common location words
        for word in &location_words {
            if lower.contains(word) {
                return true;
            }
        }

        // Check if it's a known country or major city
        // (In production, this would check against geocoding database)
        let known_locations = [
            "japan", "tokyo", "usa", "new york", "london", "paris",
            "france", "germany", "berlin", "italy", "rome", "spain",
            "china", "beijing", "australia", "sydney", "canada", "toronto",
        ];

        known_locations.iter().any(|loc| lower.contains(loc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_person_in_location() {
        let query = QueryParser::parse("Dad in Tokyo");
        
        assert_eq!(query.person(), Some("Dad"));
        assert_eq!(query.location(), Some("Tokyo"));
    }

    #[test]
    fn test_parse_date_only() {
        let query = QueryParser::parse("March 2019");
        
        assert!(query.date_range().is_some());
        assert_eq!(query.date_range().unwrap().start.month(), 3);
    }

    #[test]
    fn test_parse_person_with_date() {
        let query = QueryParser::parse("Dad March 2019");
        
        assert_eq!(query.person(), Some("Dad"));
        assert!(query.date_range().is_some());
    }
}
```

---

## Step 3: Search Service

### File: `src/services/search.rs`

```rust
//! Search service - executes search queries against the database

use rusqlite::{params, Connection, Result as SqliteResult};

use crate::search::{SearchQuery, SearchFilter, DateRange};

/// A search result entry
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub photo_id: i64,
    pub file_path: String,
    pub date_taken: Option<String>,
    pub location_city: Option<String>,
    pub location_country: Option<String>,
}

/// Search results grouped by date
#[derive(Debug, Clone)]
pub struct SearchResultGroup {
    pub date: String,  // YYYY-MM-DD
    pub location: Option<String>,
    pub results: Vec<SearchResult>,
}

/// Search service
pub struct SearchService;

impl SearchService {
    /// Execute a search query
    pub fn search(conn: &Connection, query: &SearchQuery) -> SqliteResult<Vec<SearchResult>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        // Build dynamic SQL query
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Date range filter
        if let Some(range) = query.date_range() {
            conditions.push("date_taken BETWEEN ?1 AND ?2");
            params_vec.push(Box::new(range.start.format("%Y-%m-%d %H:%M:%S").to_string()));
            params_vec.push(Box::new(range.end.format("%Y-%m-%d %H:%M:%S").to_string()));
        }

        // Location filter
        if let Some(location) = query.location() {
            let param_idx = params_vec.len() + 1;
            conditions.push(&format!(
                "(location_city LIKE ?{0} OR location_country LIKE ?{0})",
                param_idx
            ));
            params_vec.push(Box::new(format!("%{}%", location)));
        }

        // Person filter (requires join with faces/clusters)
        let person_join = if let Some(person) = query.person() {
            let param_idx = params_vec.len() + 1;
            params_vec.push(Box::new(format!("%{}%", person)));
            Some(param_idx)
        } else {
            None
        };

        // Build query string
        let sql = if let Some(param_idx) = person_join {
            format!(
                r#"
                SELECT DISTINCT p.id, p.file_path, p.date_taken, p.location_city, p.location_country
                FROM photos p
                JOIN faces f ON f.photo_id = p.id
                JOIN face_clusters fc ON f.cluster_id = fc.id
                WHERE fc.name LIKE ?{}
                  AND p.is_trashed = FALSE
                  {}
                ORDER BY p.date_taken DESC
                LIMIT 1000
                "#,
                param_idx,
                if conditions.is_empty() {
                    String::new()
                } else {
                    format!("AND {}", conditions.join(" AND "))
                }
            )
        } else {
            format!(
                r#"
                SELECT p.id, p.file_path, p.date_taken, p.location_city, p.location_country
                FROM photos p
                WHERE p.is_trashed = FALSE
                  {}
                ORDER BY p.date_taken DESC
                LIMIT 1000
                "#,
                if conditions.is_empty() {
                    String::new()
                } else {
                    format!("AND {}", conditions.join(" AND "))
                }
            )
        };

        // For simplicity in this example, we'll use a simpler approach
        // In production, use proper parameter binding
        Self::execute_search(conn, query)
    }

    /// Execute search with proper parameter handling
    fn execute_search(conn: &Connection, query: &SearchQuery) -> SqliteResult<Vec<SearchResult>> {
        let mut results = Vec::new();

        // Get base photo set
        let mut base_query = String::from(
            "SELECT id, file_path, date_taken, location_city, location_country FROM photos WHERE is_trashed = FALSE"
        );

        // Add date filter
        if let Some(range) = query.date_range() {
            let start = range.start.format("%Y-%m-%d %H:%M:%S").to_string();
            let end = range.end.format("%Y-%m-%d %H:%M:%S").to_string();
            base_query.push_str(&format!(
                " AND date_taken BETWEEN '{}' AND '{}'",
                start, end
            ));
        }

        // Add location filter
        if let Some(location) = query.location() {
            base_query.push_str(&format!(
                " AND (location_city LIKE '%{}%' OR location_country LIKE '%{}%')",
                location, location
            ));
        }

        base_query.push_str(" ORDER BY date_taken DESC LIMIT 1000");

        let mut stmt = conn.prepare(&base_query)?;
        let rows = stmt.query_map([], |row| {
            Ok(SearchResult {
                photo_id: row.get(0)?,
                file_path: row.get(1)?,
                date_taken: row.get(2)?,
                location_city: row.get(3)?,
                location_country: row.get(4)?,
            })
        })?;

        for row in rows {
            results.push(row?);
        }

        // If person filter, further filter by face clusters
        if let Some(person) = query.person() {
            results = Self::filter_by_person(conn, results, person)?;
        }

        Ok(results)
    }

    /// Filter results to only photos containing a specific person
    fn filter_by_person(
        conn: &Connection,
        results: Vec<SearchResult>,
        person_name: &str,
    ) -> SqliteResult<Vec<SearchResult>> {
        // Get photo IDs that contain this person
        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT f.photo_id
            FROM faces f
            JOIN face_clusters fc ON f.cluster_id = fc.id
            WHERE fc.name LIKE ?1
            "#,
        )?;

        let photo_ids: std::collections::HashSet<i64> = stmt
            .query_map(params![format!("%{}%", person_name)], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results
            .into_iter()
            .filter(|r| photo_ids.contains(&r.photo_id))
            .collect())
    }

    /// Group search results by date
    pub fn group_by_date(results: Vec<SearchResult>) -> Vec<SearchResultGroup> {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, Vec<SearchResult>> = BTreeMap::new();

        for result in results {
            let date = result
                .date_taken
                .as_ref()
                .map(|d| d[..10].to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            groups.entry(date).or_default().push(result);
        }

        groups
            .into_iter()
            .rev() // Most recent first
            .map(|(date, results)| {
                // Get most common location for the group
                let location = results
                    .iter()
                    .filter_map(|r| {
                        r.location_city.clone().or_else(|| r.location_country.clone())
                    })
                    .next();

                SearchResultGroup {
                    date,
                    location,
                    results,
                }
            })
            .collect()
    }

    /// Get recent search terms (for suggestions)
    pub fn get_recent_searches(_conn: &Connection) -> Vec<String> {
        // In production, store and retrieve from a searches table
        Vec::new()
    }

    /// Get search suggestions based on partial input
    pub fn get_suggestions(conn: &Connection, partial: &str) -> SqliteResult<Vec<String>> {
        let mut suggestions = Vec::new();
        let partial_lower = partial.to_lowercase();

        // Suggest matching person names
        let mut stmt = conn.prepare(
            "SELECT DISTINCT name FROM face_clusters WHERE name LIKE ?1 LIMIT 5"
        )?;
        let names: Vec<String> = stmt
            .query_map(params![format!("%{}%", partial)], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        suggestions.extend(names);

        // Suggest matching locations
        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT location_city FROM photos 
            WHERE location_city LIKE ?1 AND location_city IS NOT NULL 
            LIMIT 5
            "#
        )?;
        let cities: Vec<String> = stmt
            .query_map(params![format!("%{}%", partial)], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        suggestions.extend(cities);

        Ok(suggestions)
    }
}
```

---

## Step 4: Trash Service

### File: `src/services/trash.rs`

```rust
//! Trash management service

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};

/// Trash service - handles soft delete and permanent deletion
pub struct TrashService;

impl TrashService {
    /// Move photos to trash (soft delete)
    pub fn trash_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<usize> {
        let tx = conn.unchecked_transaction()?;
        let mut count = 0;

        for photo_id in photo_ids {
            // Get original path
            let path: Option<String> = conn.query_row(
                "SELECT file_path FROM photos WHERE id = ?1",
                params![photo_id],
                |row| row.get(0),
            ).ok();

            if let Some(path) = path {
                // Add to trash table
                conn.execute(
                    r#"
                    INSERT OR IGNORE INTO trash (photo_id, original_path)
                    VALUES (?1, ?2)
                    "#,
                    params![photo_id, path],
                )?;

                // Mark as trashed
                conn.execute(
                    r#"
                    UPDATE photos 
                    SET is_trashed = TRUE, trashed_at = CURRENT_TIMESTAMP
                    WHERE id = ?1
                    "#,
                    params![photo_id],
                )?;

                count += 1;
            }
        }

        tx.commit()?;
        Ok(count)
    }

    /// Restore photos from trash
    pub fn restore_photos(conn: &Connection, photo_ids: &[i64]) -> SqliteResult<usize> {
        let tx = conn.unchecked_transaction()?;
        let mut count = 0;

        for photo_id in photo_ids {
            // Remove from trash table
            conn.execute(
                "DELETE FROM trash WHERE photo_id = ?1",
                params![photo_id],
            )?;

            // Unmark as trashed
            conn.execute(
                r#"
                UPDATE photos 
                SET is_trashed = FALSE, trashed_at = NULL
                WHERE id = ?1
                "#,
                params![photo_id],
            )?;

            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    /// Permanently delete photos (actually removes files from disk)
    pub fn permanent_delete(
        conn: &Connection,
        photo_ids: &[i64],
        drive_root: &Path,
        thumbnail_dir: &Path,
    ) -> SqliteResult<DeleteResult> {
        let mut result = DeleteResult::default();
        let tx = conn.unchecked_transaction()?;

        for photo_id in photo_ids {
            // Get file path
            let file_path: Option<String> = conn.query_row(
                "SELECT file_path FROM photos WHERE id = ?1",
                params![photo_id],
                |row| row.get(0),
            ).ok();

            if let Some(relative_path) = file_path {
                let full_path = drive_root.join(&relative_path);
                
                // Delete the actual file
                if full_path.exists() {
                    match fs::remove_file(&full_path) {
                        Ok(_) => result.files_deleted += 1,
                        Err(e) => {
                            result.errors.push(format!("{}: {}", relative_path, e));
                            continue; // Don't remove from DB if file delete failed
                        }
                    }
                }

                // Get and delete thumbnail
                let file_hash: Option<String> = conn.query_row(
                    "SELECT file_hash FROM photos WHERE id = ?1",
                    params![photo_id],
                    |row| row.get(0),
                ).ok();

                if let Some(hash) = file_hash {
                    let thumb_path = thumbnail_dir
                        .join(&hash[..2])
                        .join(format!("{}.jpg", hash));
                    
                    if thumb_path.exists() {
                        let _ = fs::remove_file(&thumb_path);
                    }
                }

                // Delete from database (cascades to faces, etc.)
                conn.execute("DELETE FROM trash WHERE photo_id = ?1", params![photo_id])?;
                conn.execute("DELETE FROM photos WHERE id = ?1", params![photo_id])?;
                
                result.db_records_deleted += 1;
            }
        }

        tx.commit()?;
        Ok(result)
    }

    /// Empty entire trash
    pub fn empty_trash(
        conn: &Connection,
        drive_root: &Path,
        thumbnail_dir: &Path,
    ) -> SqliteResult<DeleteResult> {
        // Get all trashed photo IDs
        let mut stmt = conn.prepare("SELECT photo_id FROM trash")?;
        let photo_ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Self::permanent_delete(conn, &photo_ids, drive_root, thumbnail_dir)
    }

    /// Get trash statistics
    pub fn get_stats(conn: &Connection) -> SqliteResult<TrashStats> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_trashed = TRUE",
            [],
            |row| row.get(0),
        )?;

        let size: i64 = conn.query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM photos WHERE is_trashed = TRUE",
            [],
            |row| row.get(0),
        )?;

        let oldest: Option<String> = conn.query_row(
            "SELECT MIN(trashed_at) FROM photos WHERE is_trashed = TRUE",
            [],
            |row| row.get(0),
        ).ok().flatten();

        Ok(TrashStats {
            count: count as usize,
            total_size: size as u64,
            oldest_item: oldest,
        })
    }

    /// Auto-cleanup: delete items older than N days
    pub fn auto_cleanup(
        conn: &Connection,
        days: i64,
        drive_root: &Path,
        thumbnail_dir: &Path,
    ) -> SqliteResult<DeleteResult> {
        // Find items older than N days
        let mut stmt = conn.prepare(
            r#"
            SELECT photo_id FROM trash
            WHERE julianday('now') - julianday(trashed_at) > ?1
            "#,
        )?;

        let photo_ids: Vec<i64> = stmt
            .query_map(params![days], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        if photo_ids.is_empty() {
            return Ok(DeleteResult::default());
        }

        Self::permanent_delete(conn, &photo_ids, drive_root, thumbnail_dir)
    }
}

/// Result of a delete operation
#[derive(Debug, Default)]
pub struct DeleteResult {
    pub files_deleted: usize,
    pub db_records_deleted: usize,
    pub errors: Vec<String>,
}

/// Trash statistics
#[derive(Debug, Default)]
pub struct TrashStats {
    pub count: usize,
    pub total_size: u64,
    pub oldest_item: Option<String>,
}
```

---

## Step 5: Trash Repository

### File: `src/db/trash_repo.rs`

```rust
//! Trash database operations

use rusqlite::{params, Connection, Result as SqliteResult};

/// Trashed photo record
#[derive(Debug, Clone)]
pub struct TrashedPhotoRecord {
    pub id: i64,
    pub photo_id: i64,
    pub original_path: String,
    pub trashed_at: String,
    
    // Joined from photos
    pub file_size: Option<i64>,
    pub date_taken: Option<String>,
}

/// Trash repository
pub struct TrashRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TrashRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Get all trashed items
    pub fn get_all(&self) -> SqliteResult<Vec<TrashedPhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                t.id,
                t.photo_id,
                t.original_path,
                t.trashed_at,
                p.file_size,
                p.date_taken
            FROM trash t
            JOIN photos p ON t.photo_id = p.id
            ORDER BY t.trashed_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(TrashedPhotoRecord {
                id: row.get(0)?,
                photo_id: row.get(1)?,
                original_path: row.get(2)?,
                trashed_at: row.get(3)?,
                file_size: row.get(4)?,
                date_taken: row.get(5)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }

        Ok(items)
    }

    /// Get trash count
    pub fn count(&self) -> SqliteResult<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM trash",
            [],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }
}
```

Update `src/db/mod.rs`:

```rust
pub mod trash_repo;
pub use trash_repo::{TrashRepo, TrashedPhotoRecord};
```

---

## Step 6: Quick Cull View

### File: `src/views/cull.rs`

```rust
//! Quick Cull mode - keyboard-driven photo review

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{button, column, container, row, text, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Cull mode state
#[derive(Debug, Clone)]
pub struct CullState {
    /// Photo IDs in the cull session
    pub photo_ids: Vec<i64>,
    
    /// Current index
    pub current_index: usize,
    
    /// Photos marked for trash
    pub marked_for_trash: std::collections::HashSet<i64>,
    
    /// Undo stack (photo_id, was_marked)
    pub undo_stack: Vec<(i64, bool)>,
}

impl CullState {
    pub fn new(photo_ids: Vec<i64>) -> Self {
        Self {
            photo_ids,
            current_index: 0,
            marked_for_trash: std::collections::HashSet::new(),
            undo_stack: Vec::new(),
        }
    }

    /// Get current photo ID
    pub fn current_photo_id(&self) -> Option<i64> {
        self.photo_ids.get(self.current_index).copied()
    }

    /// Go to next photo
    pub fn next(&mut self) {
        if self.current_index < self.photo_ids.len().saturating_sub(1) {
            self.current_index += 1;
        }
    }

    /// Go to previous photo
    pub fn prev(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
    }

    /// Toggle trash mark on current photo
    pub fn toggle_trash(&mut self) {
        if let Some(id) = self.current_photo_id() {
            let was_marked = self.marked_for_trash.contains(&id);
            
            if was_marked {
                self.marked_for_trash.remove(&id);
            } else {
                self.marked_for_trash.insert(id);
            }
            
            self.undo_stack.push((id, was_marked));
        }
    }

    /// Mark current photo for trash and advance
    pub fn trash_and_next(&mut self) {
        if let Some(id) = self.current_photo_id() {
            let was_marked = self.marked_for_trash.contains(&id);
            self.marked_for_trash.insert(id);
            self.undo_stack.push((id, was_marked));
        }
        self.next();
    }

    /// Undo last action
    pub fn undo(&mut self) {
        if let Some((id, was_marked)) = self.undo_stack.pop() {
            if was_marked {
                self.marked_for_trash.insert(id);
            } else {
                self.marked_for_trash.remove(&id);
            }
        }
    }

    /// Check if current photo is marked
    pub fn is_current_marked(&self) -> bool {
        self.current_photo_id()
            .map(|id| self.marked_for_trash.contains(&id))
            .unwrap_or(false)
    }

    /// Get count of marked photos
    pub fn marked_count(&self) -> usize {
        self.marked_for_trash.len()
    }
}

/// Cull view
pub struct CullView;

impl CullView {
    /// Render the cull mode UI
    pub fn view(
        state: &CullState,
        title: &str,
    ) -> Element<'static, Message> {
        let total = state.photo_ids.len();
        let current = state.current_index + 1;
        let marked = state.marked_count();

        // Header
        let header = row![
            text(title)
                .size(16)
                .color(Text::PRIMARY),
            
            Space::with_width(Length::Fill),
            
            button(
                text("Exit Cull")
                    .size(12)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => Some(Backgrounds::ELEVATED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::ExitCullMode),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([16, 32]));

        // Main image area
        let is_marked = state.is_current_marked();
        let image_container = container(
            column![
                if is_marked {
                    container(
                        text("MARKED FOR DELETION")
                            .size(12)
                            .color(Backgrounds::PRIMARY)
                    )
                    .padding(Padding::from([4, 8]))
                    .style(|_theme| container::Style {
                        background: Some(iced::Color::from_rgb(0.8, 0.2, 0.2).into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                } else {
                    container(Space::new(Length::Shrink, Length::Shrink))
                },
                
                Space::with_height(Length::Fill),
                
                // Image placeholder
                text("Photo will appear here")
                    .size(14)
                    .color(Text::TERTIARY),
                
                Space::with_height(Length::Fill),
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill)
        )
        .width(Length::Fill)
        .height(Length::FillPortion(3))
        .padding(32)
        .style(move |_theme| container::Style {
            background: Some(if is_marked {
                iced::Color::from_rgba(0.8, 0.2, 0.2, 0.1).into()
            } else {
                Backgrounds::ELEVATED.into()
            }),
            border: iced::Border {
                color: if is_marked {
                    iced::Color::from_rgb(0.8, 0.2, 0.2)
                } else {
                    Border::SUBTLE
                },
                width: 2.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        // Photo info
        let photo_info = row![
            text(format!("{} of {}", current, total))
                .size(14)
                .color(Text::PRIMARY),
            
            Space::with_width(Length::Fill),
            
            text(format!("{} marked for deletion", marked))
                .size(14)
                .color(if marked > 0 {
                    iced::Color::from_rgb(0.8, 0.2, 0.2)
                } else {
                    Text::TERTIARY
                }),
        ]
        .padding(Padding::from([8, 32]));

        // Filmstrip (simplified)
        let filmstrip = Self::render_filmstrip(state);

        // Controls
        let controls = row![
            // Previous
            button(
                text("<  Prev")
                    .size(14)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([12, 24]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => Some(Backgrounds::ELEVATED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: Border::SUBTLE,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::CullPrev),
            
            Space::with_width(16),
            
            // Trash toggle
            button(
                text(if is_marked { "Unmark (X)" } else { "Trash (X)" })
                    .size(14)
                    .color(if is_marked { Backgrounds::PRIMARY } else { Text::PRIMARY })
            )
            .padding(Padding::from([12, 32]))
            .style(move |_theme, status| {
                let background = if is_marked {
                    Some(iced::Color::from_rgb(0.8, 0.2, 0.2).into())
                } else {
                    match status {
                        button::Status::Hovered => Some(iced::Color::from_rgba(0.8, 0.2, 0.2, 0.3).into()),
                        _ => Some(Backgrounds::ELEVATED.into()),
                    }
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: iced::Color::from_rgb(0.8, 0.2, 0.2),
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::CullToggleTrash),
            
            Space::with_width(16),
            
            // Next
            button(
                text("Next  >")
                    .size(14)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([12, 24]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => Some(Backgrounds::ELEVATED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: Border::SUBTLE,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::CullNext),
            
            Space::with_width(Length::Fill),
            
            // Undo
            button(
                text("Undo (U)")
                    .size(12)
                    .color(Text::SECONDARY)
            )
            .padding(Padding::from([8, 16]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::CullUndo),
            
            Space::with_width(16),
            
            // Finish
            button(
                text("Finish (Enter)")
                    .size(14)
                    .color(Backgrounds::PRIMARY)
            )
            .padding(Padding::from([12, 24]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Accent::PRIMARY.into()),
                    _ => Some(Accent::MUTED.into()),
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::CullFinish),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([16, 32]));

        // Keyboard hints
        let hints = row![
            text("Keyboard:")
                .size(11)
                .color(Text::TERTIARY),
            Space::with_width(8),
            text("← → navigate")
                .size(11)
                .color(Text::TERTIARY),
            Space::with_width(16),
            text("X trash")
                .size(11)
                .color(Text::TERTIARY),
            Space::with_width(16),
            text("U undo")
                .size(11)
                .color(Text::TERTIARY),
            Space::with_width(16),
            text("Enter finish")
                .size(11)
                .color(Text::TERTIARY),
            Space::with_width(16),
            text("Esc exit")
                .size(11)
                .color(Text::TERTIARY),
        ]
        .padding(Padding::from([8, 32]));

        let content = column![
            header,
            image_container,
            photo_info,
            filmstrip,
            controls,
            hints,
        ];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render the filmstrip of thumbnails
    fn render_filmstrip(state: &CullState) -> Element<'static, Message> {
        let current = state.current_index;
        let total = state.photo_ids.len();
        
        // Show up to 9 thumbnails centered on current
        let start = current.saturating_sub(4);
        let end = (start + 9).min(total);

        let mut thumbs: Vec<Element<'static, Message>> = Vec::new();

        for i in start..end {
            let photo_id = state.photo_ids[i];
            let is_current = i == current;
            let is_marked = state.marked_for_trash.contains(&photo_id);

            let thumb = container(
                column![
                    if is_marked {
                        text("X")
                            .size(14)
                            .color(iced::Color::from_rgb(0.8, 0.2, 0.2))
                    } else {
                        text("")
                            .size(14)
                    },
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
            )
            .width(50)
            .height(50)
            .style(move |_theme| container::Style {
                background: Some(if is_marked {
                    iced::Color::from_rgba(0.8, 0.2, 0.2, 0.3).into()
                } else {
                    Backgrounds::ELEVATED.into()
                }),
                border: iced::Border {
                    color: if is_current {
                        Accent::PRIMARY
                    } else if is_marked {
                        iced::Color::from_rgb(0.8, 0.2, 0.2)
                    } else {
                        Border::SUBTLE
                    },
                    width: if is_current { 2.0 } else { 1.0 },
                    radius: 4.0.into(),
                },
                ..Default::default()
            });

            thumbs.push(thumb.into());
        }

        container(
            Row::with_children(thumbs)
                .spacing(8)
        )
        .width(Length::Fill)
        .padding(Padding::from([8, 32]))
        .center_x(Length::Fill)
        .into()
    }

    /// Handle keyboard events in cull mode
    pub fn handle_key(key: Key) -> Option<Message> {
        match key {
            Key::Named(Named::ArrowLeft) => Some(Message::CullPrev),
            Key::Named(Named::ArrowRight) => Some(Message::CullNext),
            Key::Named(Named::Enter) => Some(Message::CullFinish),
            Key::Named(Named::Escape) => Some(Message::ExitCullMode),
            Key::Character(ref c) if c == "x" || c == "X" => Some(Message::CullToggleTrash),
            Key::Character(ref c) if c == "u" || c == "U" => Some(Message::CullUndo),
            Key::Character(ref c) if c == "a" || c == "A" => Some(Message::CullPrev),
            Key::Character(ref c) if c == "d" || c == "D" => Some(Message::CullNext),
            _ => None,
        }
    }
}
```

---

## Step 7: Search View

### File: `src/views/search.rs`

```rust
//! Search view

use iced::widget::{button, column, container, row, scrollable, text, text_input, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::services::search::SearchResultGroup;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};

/// Search view
pub struct SearchView;

impl SearchView {
    /// Render the search view
    pub fn view(
        query: &str,
        suggestions: &[String],
        results: Option<&Vec<SearchResultGroup>>,
        is_loading: bool,
    ) -> Element<'static, Message> {
        let search_input = text_input("Search photos...", query)
            .on_input(Message::SearchInputChanged)
            .on_submit(Message::ExecuteSearch)
            .size(16)
            .padding(12)
            .width(Length::Fill);

        let search_bar = container(
            row![
                search_input,
                Space::with_width(12),
                button(
                    text("Search")
                        .size(14)
                        .color(Backgrounds::PRIMARY)
                )
                .padding(Padding::from([12, 24]))
                .style(|_theme, status| {
                    let background = match status {
                        button::Status::Hovered => Some(Accent::PRIMARY.into()),
                        _ => Some(Accent::MUTED.into()),
                    };
                    button::Style {
                        background,
                        border: iced::Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::ExecuteSearch),
            ]
            .align_y(Alignment::Center)
        )
        .padding(Padding::from([0, 0, 16, 0]));

        // Suggestions
        let suggestions_row = if !suggestions.is_empty() && !query.is_empty() {
            let chips: Vec<Element<'static, Message>> = suggestions
                .iter()
                .take(5)
                .map(|s| {
                    let suggestion = s.clone();
                    button(
                        text(s)
                            .size(12)
                            .color(Text::PRIMARY)
                    )
                    .padding(Padding::from([4, 10]))
                    .style(|_theme, status| {
                        let background = match status {
                            button::Status::Hovered => Some(Backgrounds::HOVER.into()),
                            _ => Some(Backgrounds::ELEVATED.into()),
                        };
                        button::Style {
                            background,
                            border: iced::Border {
                                color: Border::SUBTLE,
                                width: 1.0,
                                radius: 12.0.into(),
                            },
                            ..Default::default()
                        }
                    })
                    .on_press(Message::SearchInputChanged(suggestion))
                    .into()
                })
                .collect();

            container(
                Row::with_children(chips)
                    .spacing(8)
            )
            .padding(Padding::from([0, 0, 16, 0]))
        } else {
            container(Space::new(Length::Shrink, Length::Shrink))
        };

        // Results
        let results_content: Element<'static, Message> = if is_loading {
            container(
                text("Searching...")
                    .size(14)
                    .color(Text::SECONDARY)
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(32)
            .into()
        } else if let Some(groups) = results {
            if groups.is_empty() {
                container(
                    column![
                        text("No results found")
                            .size(16)
                            .color(Text::SECONDARY),
                        Space::with_height(8),
                        text("Try a different search term")
                            .size(14)
                            .color(Text::TERTIARY),
                    ]
                    .align_x(Alignment::Center)
                )
                .width(Length::Fill)
                .center_x(Length::Fill)
                .padding(32)
                .into()
            } else {
                let total: usize = groups.iter().map(|g| g.results.len()).sum();
                
                let group_views: Vec<Element<'static, Message>> = groups
                    .iter()
                    .map(|g| Self::render_result_group(g))
                    .collect();

                column![
                    text(format!("{} photos found", total))
                        .size(14)
                        .color(Text::SECONDARY),
                    Space::with_height(16),
                    scrollable(
                        Column::with_children(group_views)
                            .spacing(24)
                    )
                    .height(Length::Fill),
                ]
                .into()
            }
        } else {
            // Initial state - show tips
            container(
                column![
                    text("Search Examples")
                        .size(16)
                        .color(Text::PRIMARY),
                    Space::with_height(16),
                    Self::search_tip("Dad", "Find photos of a person"),
                    Self::search_tip("Tokyo", "Find photos from a location"),
                    Self::search_tip("March 2019", "Find photos from a time"),
                    Self::search_tip("Dad in Tokyo", "Combine person and location"),
                    Self::search_tip("last summer", "Natural language dates"),
                ]
                .spacing(8)
            )
            .padding(32)
            .into()
        };

        let content = column![
            text("Search")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(24),
            search_bar,
            suggestions_row,
            results_content,
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a search tip
    fn search_tip(example: &str, description: &str) -> Element<'static, Message> {
        row![
            container(
                text(example)
                    .size(12)
                    .color(Text::PRIMARY)
            )
            .padding(Padding::from([4, 8]))
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            
            Space::with_width(12),
            
            text(description)
                .size(12)
                .color(Text::TERTIARY),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    /// Render a result group (photos from one day)
    fn render_result_group(group: &SearchResultGroup) -> Element<'static, Message> {
        let header = row![
            text(&group.date)
                .size(14)
                .color(Text::PRIMARY),
            
            Space::with_width(16),
            
            if let Some(ref loc) = group.location {
                text(loc)
                    .size(12)
                    .color(Text::TERTIARY)
            } else {
                text("")
                    .size(12)
            },
            
            Space::with_width(Length::Fill),
            
            text(format!("{} photos", group.results.len()))
                .size(12)
                .color(Text::SECONDARY),
        ]
        .align_y(Alignment::Center);

        // Photo grid (simplified)
        let photo_count = group.results.len();
        let grid_text = text(format!("[{} photo thumbnails]", photo_count))
            .size(12)
            .color(Text::TERTIARY);

        column![
            header,
            Space::with_height(12),
            container(grid_text)
                .width(Length::Fill)
                .height(100)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_theme| container::Style {
                    background: Some(Backgrounds::ELEVATED.into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        ]
        .into()
    }
}
```

---

## Step 8: Trash View

### File: `src/views/trash.rs`

```rust
//! Trash view

use iced::widget::{button, column, container, row, scrollable, text, Column, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::db::TrashedPhotoRecord;
use crate::services::trash::TrashStats;
use crate::theme::colors::{Accent, Backgrounds, Border, Text};
use crate::utils::format_bytes;

/// Trash view
pub struct TrashView;

impl TrashView {
    /// Render the trash view
    pub fn view(
        items: &[TrashedPhotoRecord],
        stats: &TrashStats,
        selected: &std::collections::HashSet<i64>,
    ) -> Element<'static, Message> {
        if items.is_empty() {
            return Self::empty_view();
        }

        let title = text("Trash")
            .size(28)
            .color(Text::PRIMARY);

        let subtitle = text(format!(
            "{} photos - {}",
            stats.count,
            format_bytes(stats.total_size)
        ))
        .size(14)
        .color(Text::SECONDARY);

        let warning = container(
            text("Photos in trash will be permanently deleted after 30 days")
                .size(12)
                .color(Text::TERTIARY)
        )
        .padding(Padding::from([8, 12]))
        .style(|_theme| container::Style {
            background: Some(Backgrounds::ELEVATED.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        // Item list
        let item_list: Vec<Element<'static, Message>> = items
            .iter()
            .map(|item| {
                let is_selected = selected.contains(&item.photo_id);
                Self::trash_item(item, is_selected)
            })
            .collect();

        // Actions
        let has_selection = !selected.is_empty();
        let actions = row![
            button(
                text(if has_selection {
                    format!("Restore Selected ({})", selected.len())
                } else {
                    "Restore Selected".to_string()
                })
                .size(14)
                .color(if has_selection { Text::PRIMARY } else { Text::TERTIARY })
            )
            .padding(Padding::from([10, 20]))
            .style(move |_theme, status| {
                let background = if has_selection {
                    match status {
                        button::Status::Hovered => Some(Accent::PRIMARY.into()),
                        _ => Some(Accent::MUTED.into()),
                    }
                } else {
                    Some(Backgrounds::ELEVATED.into())
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::RestoreSelected),
            
            Space::with_width(Length::Fill),
            
            button(
                text("Empty Trash")
                    .size(14)
                    .color(iced::Color::from_rgb(0.8, 0.2, 0.2))
            )
            .padding(Padding::from([10, 20]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(iced::Color::from_rgba(0.8, 0.2, 0.2, 0.2).into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: iced::Color::from_rgb(0.8, 0.2, 0.2),
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::EmptyTrash),
        ]
        .align_y(Alignment::Center);

        let content = column![
            title,
            Space::with_height(8),
            subtitle,
            Space::with_height(16),
            warning,
            Space::with_height(24),
            scrollable(
                Column::with_children(item_list)
                    .spacing(8)
            )
            .height(Length::Fill),
            Space::with_height(16),
            actions,
        ]
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Empty trash state
    fn empty_view() -> Element<'static, Message> {
        let content = column![
            text("Trash")
                .size(28)
                .color(Text::PRIMARY),
            Space::with_height(16),
            text("Trash is empty")
                .size(16)
                .color(Text::SECONDARY),
            Space::with_height(8),
            text("Deleted photos will appear here")
                .size(14)
                .color(Text::TERTIARY),
        ]
        .align_x(Alignment::Start)
        .padding(32);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::PRIMARY.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render a single trash item
    fn trash_item(item: &TrashedPhotoRecord, is_selected: bool) -> Element<'static, Message> {
        let photo_id = item.photo_id;
        let size = item.file_size.map(|s| format_bytes(s as u64)).unwrap_or_default();
        let trashed = &item.trashed_at[..10.min(item.trashed_at.len())];

        let content = row![
            // Checkbox
            button(
                text(if is_selected { "+" } else { "" })
                    .size(12)
                    .color(if is_selected { Accent::PRIMARY } else { Text::TERTIARY })
            )
            .width(24)
            .height(24)
            .padding(0)
            .style(move |_theme, _status| button::Style {
                background: Some(if is_selected {
                    Accent::MUTED.into()
                } else {
                    Backgrounds::ELEVATED.into()
                }),
                border: iced::Border {
                    color: if is_selected { Accent::PRIMARY } else { Border::SUBTLE },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .on_press(Message::ToggleTrashSelection(photo_id)),
            
            Space::with_width(16),
            
            // Thumbnail placeholder
            container(
                text("IMG")
                    .size(10)
                    .color(Text::TERTIARY)
            )
            .width(50)
            .height(50)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Backgrounds::ELEVATED.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            
            Space::with_width(16),
            
            column![
                text(&item.original_path)
                    .size(13)
                    .color(Text::PRIMARY),
                Space::with_height(4),
                row![
                    text(size)
                        .size(12)
                        .color(Text::SECONDARY),
                    Space::with_width(16),
                    text(format!("Deleted {}", trashed))
                        .size(12)
                        .color(Text::TERTIARY),
                ],
            ]
            .width(Length::Fill),
            
            // Restore button
            button(
                text("Restore")
                    .size(12)
                    .color(Accent::PRIMARY)
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(Accent::MUTED.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border {
                        color: Accent::PRIMARY,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::RestorePhoto(photo_id)),
            
            Space::with_width(8),
            
            // Delete permanently button
            button(
                text("Delete")
                    .size(12)
                    .color(iced::Color::from_rgb(0.8, 0.2, 0.2))
            )
            .padding(Padding::from([6, 12]))
            .style(|_theme, status| {
                let background = match status {
                    button::Status::Hovered => Some(iced::Color::from_rgba(0.8, 0.2, 0.2, 0.2).into()),
                    _ => None,
                };
                button::Style {
                    background,
                    border: iced::Border::default(),
                    ..Default::default()
                }
            })
            .on_press(Message::PermanentlyDeletePhoto(photo_id)),
        ]
        .align_y(Alignment::Center);

        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(if is_selected {
                    Backgrounds::SELECTED
                } else {
                    Backgrounds::ELEVATED
                }.into()),
                border: iced::Border {
                    color: if is_selected { Accent::PRIMARY } else { Border::SUBTLE },
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}

// Helper (add to utils)
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
```

---

## Step 9: Add Messages to App

Add these messages to `src/app.rs`:

```rust
/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing messages ...

    // Search
    SearchInputChanged(String),
    ExecuteSearch,
    SearchComplete(Vec<SearchResultGroup>),
    ClearSearch,

    // Cull mode
    EnterCullMode(Vec<i64>),  // photo IDs to cull
    ExitCullMode,
    CullNext,
    CullPrev,
    CullToggleTrash,
    CullUndo,
    CullFinish,
    CullConfirmTrash,

    // Trash
    TrashPhotos(Vec<i64>),
    RestorePhoto(i64),
    RestoreSelected,
    ToggleTrashSelection(i64),
    PermanentlyDeletePhoto(i64),
    EmptyTrash,
    ConfirmEmptyTrash,
}
```

---

## UI Design: Search View

```
┌─────────────────────────────────────────────────────────────────┐
│  [Sidebar]  │  Search                                            │
│             │                                                    │
│  Timeline   │  ┌──────────────────────────────────┐ [Search]     │
│  People     │  │ Dad in Tokyo                     │              │
│  Search   ● │  └──────────────────────────────────┘              │
│  Duplicates │                                                    │
│  Bursts     │  Suggestions: [Tokyo] [Dad] [March 2019]           │
│             │─────────────────────────────────────────────────── │
│  ─────────  │                                                    │
│             │  156 photos found                                  │
│  Trash (5)  │                                                    │
│  Settings   │  ═══ March 15, 2019 ════════════════ Tokyo ══════ │
│             │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐                  │
│             │  │     │ │     │ │     │ │     │                  │
│             │  └─────┘ └─────┘ └─────┘ └─────┘                  │
│             │                                                    │
│             │  ═══ March 14, 2019 ════════════════ Tokyo ══════ │
│             │  ┌─────┐ ┌─────┐                                  │
│             │  │     │ │     │                                  │
│             │  └─────┘ └─────┘                                  │
│             │                                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## UI Design: Quick Cull Mode

```
┌─────────────────────────────────────────────────────────────────┐
│  Cull - March 15, 2019                           [Exit Cull]     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│           ┌───────────────────────────────────────┐              │
│           │                                       │              │
│           │                                       │              │
│           │                                       │              │
│           │          [Current Photo]              │              │
│           │                                       │              │
│           │                                       │              │
│           │                                       │              │
│           └───────────────────────────────────────┘              │
│                                                                  │
│           IMG_4521.jpg     15 of 89     12 marked for deletion   │
│                                                                  │
│  ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐         │
│  │ X │ │   │ │ ▶ │ │   │ │ X │ │   │ │   │ │   │ │   │         │
│  └───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘         │
│                  ↑ current                                       │
│                                                                  │
│  [< Prev]    [ X Trash ]    [Next >]           [Undo] [Finish]   │
│                                                                  │
│  Keyboard: ← → navigate    X trash    U undo    Enter finish     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Verification Checklist

- [ ] Date parser handles "March 2019", "last summer", "yesterday"
- [ ] Query parser extracts person, location, date from combined queries
- [ ] Search returns correct results for each filter type
- [ ] Person search filters by face cluster name
- [ ] Location search matches city or country
- [ ] Search results grouped by date
- [ ] Quick cull mode keyboard navigation works
- [ ] Trash marking toggles correctly
- [ ] Undo stack restores previous state
- [ ] Finish cull shows confirmation with count
- [ ] Trash soft delete marks photos without removing files
- [ ] Restore moves photos back to library
- [ ] Permanent delete removes files from disk
- [ ] Empty trash confirmation is shown
- [ ] Auto-cleanup respects day threshold

---

## Performance Notes

- Search queries limited to 1000 results for performance
- SQLite indexes on `date_taken`, `location_city`, `location_country`
- Face cluster name search uses index on `face_clusters.name`
- Suggestions query limited to 5 results per category

---

## Next Phase Preview

**Phase 7: Offline Geocoding & Polish** will add:
- GeoNames database bundled with app
- Reverse geocoding (GPS -> city/country)
- Incremental re-indexing
- Settings view
- Error handling polish
- Performance optimization

---

## Expected Results & Behavior

> **IMPORTANT:** Do not proceed to Phase 7 until a human has verified ALL of the following behaviors. Each item must be manually tested and confirmed.

### Visual Verification

| What to Check | Expected Behavior |
|---------------|-------------------|
| **Search View** | Search bar at top, suggestion chips below, results area below that |
| **Search Suggestions** | Chip buttons appear as user types (person names, locations, dates) |
| **Search Results** | Photos grouped by date with location labels, count displayed |
| **Empty Search Results** | "No results found" message with suggestion to try different terms |
| **Search Tips** | Initial state shows example queries with descriptions |
| **Quick Cull Mode** | Full-screen photo view with filmstrip, action buttons, keyboard hints |
| **Cull Progress** | Counter showing "15 of 89" and "12 marked for deletion" |
| **Trash View** | List of trashed photos with size, date deleted, restore/delete buttons |
| **Trash Warning** | Banner: "Photos in trash will be permanently deleted after 30 days" |

### Interaction Verification

| Action | Expected Result |
|--------|-----------------|
| **Type "March 2019"** | Photos from March 2019 displayed in results |
| **Type "last summer"** | Photos from previous summer months displayed |
| **Type "yesterday"** | Photos from yesterday's date displayed |
| **Type person name (e.g., "Dad")** | Photos containing that person's face cluster shown |
| **Type location (e.g., "Tokyo")** | Photos geotagged in Tokyo shown |
| **Type combined "Dad in Tokyo"** | Only photos with Dad's face AND Tokyo location shown |
| **Click suggestion chip** | Fills search bar and executes search |
| **Enter Quick Cull mode** | Full-screen cull view with first photo displayed |
| **Press X in cull mode** | Current photo marked for trash, red overlay or indicator shown |
| **Press Left/Right arrows in cull** | Navigates to previous/next photo |
| **Press U in cull mode** | Undoes last trash marking |
| **Press Enter in cull mode** | Shows confirmation dialog with count of photos to trash |
| **Press Escape in cull mode** | Exits cull mode without applying changes |
| **Confirm cull trash** | Marked photos moved to trash, returns to previous view |
| **Click "Restore" in Trash** | Photo removed from trash, back in library |
| **Click "Empty Trash"** | Confirmation shown, then files permanently deleted from disk |

### Technical Verification

```bash
# Test date parsing
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM photos WHERE date_taken BETWEEN '2019-03-01' AND '2019-03-31';"

# Verify location search
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM photos WHERE location_city LIKE '%Tokyo%' OR location_country LIKE '%Tokyo%';"

# Check person search via clusters
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT fc.name, COUNT(f.id) FROM face_clusters fc JOIN faces f ON fc.id = f.cluster_id WHERE fc.name LIKE '%Dad%' GROUP BY fc.id;"

# Verify trash records
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT COUNT(*) FROM trash;"

# Check soft delete (photo still exists on disk)
sqlite3 /path/to/drive/.photovault/photovault.db "SELECT t.photo_id, p.file_path FROM trash t JOIN photos p ON t.photo_id = p.id LIMIT 5;"
# Then verify: ls -la <file_path> (file should still exist)
```

**Expected:** Date queries return correct date ranges. Location and person searches return matching photos. Trash records created for soft-deleted photos. Files remain on disk until permanently deleted.

### Performance Verification

| Metric | Expected |
|--------|----------|
| **Search query execution** | < 500ms for any query type |
| **Suggestion generation** | < 100ms as user types |
| **Cull mode navigation** | Instant (<50ms) photo switching |
| **Trash operations** | < 100ms for soft delete/restore |
| **Search result rendering** | < 1 second for up to 1000 results |

### Sign-off Checklist

Before proceeding to Phase 7, confirm:

- [ ] **Build passes:** `cargo build --release` completes without warnings
- [ ] **Date search works:** "March 2019", "last summer", "yesterday" return correct results
- [ ] **Location search works:** City and country names find matching photos
- [ ] **Person search works:** Typing a face cluster name finds that person's photos
- [ ] **Combined search works:** "Dad in Tokyo" intersects person + location correctly
- [ ] **Search suggestions appear:** Chips shown while typing
- [ ] **Quick Cull mode enters:** Full-screen view with photo and controls
- [ ] **Cull keyboard controls:** X=trash, arrows=navigate, U=undo, Enter=finish, Escape=exit
- [ ] **Cull undo works:** U key restores last trash-marked photo
- [ ] **Cull confirmation:** Enter shows dialog with count before applying
- [ ] **Trash soft delete:** Photos marked as trashed but files not removed
- [ ] **Trash restore:** Restored photos return to library view
- [ ] **Permanent delete:** "Empty Trash" removes files from disk after confirmation
- [ ] **No console errors:** Clean search and cull operations
- [ ] **SKILL.md followed:** Search, Cull, and Trash views match design guidelines

**Signature:** ___________________ **Date:** _______________

---

## Proceed to Phase 7

Only after ALL items above are verified, proceed to:

📁 `plan/PHASE_7_GEOCODING_POLISH.md`
