//! Cursor-based pagination for unbounded lists (timeline, search, trash).
//!
//! Cursor is opaque to the frontend: a base64(url-safe, no-pad) encoding
//! of "<rfc3339 date or empty>|<i64 id>". Server decodes back to a
//! `Cursor` struct and uses `(date_taken, id)` keyset comparison.

use crate::CommandError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub date_taken: Option<DateTime<Utc>>,
    pub id: i64,
}

pub fn encode(c: Cursor) -> String {
    let date = c.date_taken.map(|d| d.to_rfc3339()).unwrap_or_default();
    URL_SAFE_NO_PAD.encode(format!("{}|{}", date, c.id))
}

pub fn decode(s: Option<&str>) -> Result<Option<Cursor>, CommandError> {
    let Some(s) = s else {
        return Ok(None);
    };
    let bytes = URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| CommandError::Validation {
            field: "cursor".into(),
            reason: "not base64".into(),
        })?;
    let raw = std::str::from_utf8(&bytes).map_err(|_| CommandError::Validation {
        field: "cursor".into(),
        reason: "not utf8".into(),
    })?;
    let (date_str, id_str) = raw.split_once('|').ok_or(CommandError::Validation {
        field: "cursor".into(),
        reason: "malformed".into(),
    })?;
    let id: i64 = id_str.parse().map_err(|_| CommandError::Validation {
        field: "cursor".into(),
        reason: "id not i64".into(),
    })?;
    let date_taken = if date_str.is_empty() {
        None
    } else {
        Some(
            DateTime::parse_from_rfc3339(date_str)
                .map_err(|_| CommandError::Validation {
                    field: "cursor".into(),
                    reason: "date not rfc3339".into(),
                })?
                .with_timezone(&Utc),
        )
    };
    Ok(Some(Cursor { date_taken, id }))
}

pub fn clamp_limit(requested: Option<u32>) -> u32 {
    requested.unwrap_or(200).min(500)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_date() {
        let c = Cursor {
            date_taken: Some(Utc::now()),
            id: 42,
        };
        let s = encode(c);
        let d = decode(Some(&s)).unwrap().unwrap();
        assert_eq!(c.id, d.id);
        assert!(c.date_taken.is_some() && d.date_taken.is_some());
    }

    #[test]
    fn roundtrip_without_date() {
        let c = Cursor {
            date_taken: None,
            id: 1,
        };
        let s = encode(c);
        let d = decode(Some(&s)).unwrap().unwrap();
        assert_eq!(c.id, d.id);
        assert!(d.date_taken.is_none());
    }

    #[test]
    fn decode_none() {
        assert!(decode(None).unwrap().is_none());
    }

    #[test]
    fn decode_malformed() {
        assert!(decode(Some("!!!not-base64!!!")).is_err());
    }

    #[test]
    fn clamp_limit_default() {
        assert_eq!(clamp_limit(None), 200);
    }

    #[test]
    fn clamp_limit_caps_at_500() {
        assert_eq!(clamp_limit(Some(10000)), 500);
    }
}
