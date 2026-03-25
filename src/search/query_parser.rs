//! Search query parsing.

use super::date_parser::{DateParser, DateRange};

/// A parsed search filter.
#[derive(Debug, Clone)]
pub enum SearchFilter {
    Person(String),
    Location(String),
    DateRange(DateRange),
    Text(String),
}

/// A complete parsed search query.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub filters: Vec<SearchFilter>,
}

impl SearchQuery {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    pub fn person(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Person(name) => Some(name.as_str()),
            _ => None,
        })
    }

    pub fn location(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Location(loc) => Some(loc.as_str()),
            _ => None,
        })
    }

    pub fn date_range(&self) -> Option<&DateRange> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::DateRange(range) => Some(range),
            _ => None,
        })
    }
}

/// Query parser.
pub struct QueryParser;

impl QueryParser {
    pub fn parse(input: &str) -> SearchQuery {
        let input = input.trim();
        if input.is_empty() {
            return SearchQuery::new();
        }

        let mut query = SearchQuery::new();
        let mut remaining = input.to_string();

        if let Some((before, location)) = Self::extract_in_location(&remaining) {
            query.filters.push(SearchFilter::Location(location));
            remaining = before;
        }

        if let Some((non_date, date_range)) = Self::extract_date(&remaining) {
            query.filters.push(SearchFilter::DateRange(date_range));
            remaining = non_date;
        }

        let remaining = remaining.trim();
        if !remaining.is_empty() {
            if Self::looks_like_location(remaining) {
                query
                    .filters
                    .push(SearchFilter::Location(remaining.to_string()));
            } else {
                query
                    .filters
                    .push(SearchFilter::Person(remaining.to_string()));
            }
        }

        if query.is_empty() {
            query.filters.push(SearchFilter::Text(input.to_string()));
        }

        query
    }

    fn extract_in_location(input: &str) -> Option<(String, String)> {
        let lower = input.to_lowercase();
        if let Some(idx) = lower.rfind(" in ") {
            let before = input[..idx].trim().to_string();
            let location = input[idx + 4..].trim().to_string();
            if !location.is_empty() {
                return Some((before, location));
            }
        }
        None
    }

    fn extract_date(input: &str) -> Option<(String, DateRange)> {
        if let Some(range) = DateParser::parse(input) {
            return Some((String::new(), range));
        }

        let words: Vec<&str> = input.split_whitespace().collect();

        if words.len() >= 2 {
            let last_two = format!("{} {}", words[words.len() - 2], words[words.len() - 1]);
            if let Some(range) = DateParser::parse(&last_two) {
                let before = words[..words.len() - 2].join(" ");
                return Some((before, range));
            }
        }

        if let Some(last) = words.last() {
            if let Some(range) = DateParser::parse(last) {
                let before = words[..words.len() - 1].join(" ");
                return Some((before, range));
            }
        }

        None
    }

    fn looks_like_location(text: &str) -> bool {
        let lower = text.to_lowercase();
        let location_words = [
            "city",
            "country",
            "beach",
            "mountain",
            "park",
            "airport",
            "station",
            "hotel",
            "restaurant",
            "museum",
            "temple",
            "shrine",
        ];

        if location_words.iter().any(|w| lower.contains(w)) {
            return true;
        }

        let known_locations = [
            "japan",
            "tokyo",
            "usa",
            "new york",
            "london",
            "paris",
            "france",
            "germany",
            "berlin",
            "italy",
            "rome",
            "spain",
            "china",
            "beijing",
            "australia",
            "sydney",
            "canada",
            "toronto",
            "india",
            "hyderabad",
            "delhi",
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
    }

    #[test]
    fn test_parse_person_with_date() {
        let query = QueryParser::parse("Dad March 2019");
        assert_eq!(query.person(), Some("Dad"));
        assert!(query.date_range().is_some());
    }
}
