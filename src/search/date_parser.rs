//! Natural language date parsing.

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Utc};

/// A parsed date range in UTC.
#[derive(Debug, Clone)]
pub struct DateRange {
    pub start: chrono::DateTime<Utc>,
    pub end: chrono::DateTime<Utc>,
}

impl DateRange {
    pub fn single_day(date: NaiveDate) -> Self {
        Self {
            start: Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("valid hms")),
            end: Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).expect("valid hms")),
        }
    }

    pub fn month(year: i32, month: u32) -> Option<Self> {
        let start_date = NaiveDate::from_ymd_opt(year, month, 1)?;
        let end_date = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)?.pred_opt()?
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)?.pred_opt()?
        };

        Some(Self {
            start: Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).expect("valid hms")),
            end: Utc.from_utc_datetime(&end_date.and_hms_opt(23, 59, 59).expect("valid hms")),
        })
    }

    pub fn year(year: i32) -> Option<Self> {
        let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
        let end = NaiveDate::from_ymd_opt(year, 12, 31)?;

        Some(Self {
            start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).expect("valid hms")),
            end: Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).expect("valid hms")),
        })
    }
}

/// Natural language date parser.
pub struct DateParser;

impl DateParser {
    pub fn parse(input: &str) -> Option<DateRange> {
        let s = input.trim().to_lowercase();
        if s.is_empty() {
            return None;
        }
        let today = Local::now().date_naive();

        Self::parse_relative(&s, today)
            .or_else(|| Self::parse_month_year(&s))
            .or_else(|| Self::parse_year(&s))
            .or_else(|| Self::parse_season(&s, today))
            .or_else(|| Self::parse_month_only(&s, today))
            .or_else(|| Self::parse_iso_date(&s))
    }

    fn parse_relative(input: &str, today: NaiveDate) -> Option<DateRange> {
        match input {
            "today" => Some(DateRange::single_day(today)),
            "yesterday" => Some(DateRange::single_day(today - Duration::days(1))),
            "this week" => {
                let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
                let end = start + Duration::days(6);
                Some(DateRange {
                    start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).expect("valid hms")),
                    end: Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).expect("valid hms")),
                })
            }
            "last week" => {
                let this_week_start =
                    today - Duration::days(today.weekday().num_days_from_monday() as i64);
                let start = this_week_start - Duration::days(7);
                let end = this_week_start - Duration::days(1);
                Some(DateRange {
                    start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).expect("valid hms")),
                    end: Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).expect("valid hms")),
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

    fn parse_month_year(input: &str) -> Option<DateRange> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }

        let (month_str, year_str) = if parts[0].parse::<i32>().is_ok() {
            (parts[1], parts[0])
        } else {
            (parts[0], parts[1])
        };

        let year: i32 = year_str.parse().ok()?;
        let month = month_name_to_number(month_str)?;
        DateRange::month(year, month)
    }

    fn parse_year(input: &str) -> Option<DateRange> {
        let year: i32 = input.parse().ok()?;
        if !(1900..=2100).contains(&year) {
            return None;
        }
        DateRange::year(year)
    }

    fn parse_season(input: &str, today: NaiveDate) -> Option<DateRange> {
        if let Some(season_name) = input.strip_prefix("last ") {
            let (start_month, end_month) = season_bounds(season_name)?;
            let year = if start_month > today.month() {
                today.year() - 2
            } else {
                today.year() - 1
            };
            return Self::season_range(year, start_month, end_month);
        }

        if let Some(season_name) = input.strip_prefix("this ") {
            let (start_month, end_month) = season_bounds(season_name)?;
            return Self::season_range(today.year(), start_month, end_month);
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() == 2 {
            let (start_month, end_month) = season_bounds(parts[0])?;
            let year: i32 = parts[1].parse().ok()?;
            return Self::season_range(year, start_month, end_month);
        }

        None
    }

    fn season_range(year: i32, start_month: u32, end_month: u32) -> Option<DateRange> {
        let (start_year, end_year) = if start_month > end_month {
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
            start: Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).expect("valid hms")),
            end: Utc.from_utc_datetime(&end_date.and_hms_opt(23, 59, 59).expect("valid hms")),
        })
    }

    fn parse_month_only(input: &str, today: NaiveDate) -> Option<DateRange> {
        let month = month_name_to_number(input)?;
        DateRange::month(today.year(), month)
    }

    fn parse_iso_date(input: &str) -> Option<DateRange> {
        let date = NaiveDate::parse_from_str(input, "%Y-%m-%d").ok()?;
        Some(DateRange::single_day(date))
    }
}

fn month_name_to_number(s: &str) -> Option<u32> {
    match s {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sep" | "sept" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

fn season_bounds(s: &str) -> Option<(u32, u32)> {
    match s {
        "spring" => Some((3, 5)),
        "summer" => Some((6, 8)),
        "fall" | "autumn" => Some((9, 11)),
        "winter" => Some((12, 2)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_month_year() {
        let range = DateParser::parse("March 2019").expect("range");
        assert_eq!(range.start.year(), 2019);
        assert_eq!(range.start.month(), 3);
        assert_eq!(range.end.month(), 3);
    }

    #[test]
    fn test_parse_yesterday() {
        let range = DateParser::parse("yesterday").expect("range");
        assert!(range.end >= range.start);
    }

    #[test]
    fn test_parse_last_summer() {
        let range = DateParser::parse("last summer").expect("range");
        assert!(range.end >= range.start);
    }
}
