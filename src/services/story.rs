//! Titles for trips, days out, memories and collages, written from what the
//! library actually knows: dates, places, named people and earlier visits.
//! Nothing is guessed; when a detail isn't known the title simply leaves it
//! out.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike, Weekday};
use rusqlite::Connection;

/// A title plus a short line of context under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Story {
    pub title: String,
    pub context: String,
}

/// Album suggestions store the title and its context in one column.
pub const SEPARATOR: &str = "  ·  ";

impl Story {
    pub fn joined(&self) -> String {
        if self.context.is_empty() {
            self.title.clone()
        } else {
            format!("{}{SEPARATOR}{}", self.title, self.context)
        }
    }
}

/// Split a stored suggestion title back into title and context.
pub fn split(stored: &str) -> (&str, &str) {
    stored.split_once(SEPARATOR).unwrap_or((stored, ""))
}

#[derive(Debug, Clone)]
pub struct Place {
    pub city: String,
    pub country: String,
    pub photos: usize,
    pub first: NaiveDateTime,
}

/// What a set of photos has in common.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub start: Option<NaiveDateTime>,
    pub end: Option<NaiveDateTime>,
    /// In the order they were first visited.
    pub places: Vec<Place>,
    /// Named people and how many of the photos they're in, most first.
    pub people: Vec<(String, usize)>,
    /// Distinct face clusters, named or not.
    pub faces: usize,
    pub photos: usize,
    pub latitude: Option<f64>,
    pub home: Option<(String, String)>,
}

impl Facts {
    pub fn gather(conn: &Connection, ids: &[i64]) -> Facts {
        let home = crate::services::album_suggestions::detect_home_city(conn, None)
            .map(|(city, country, _, _)| (city, country));
        Self::gather_with_home(conn, ids, home)
    }

    pub fn gather_with_home(
        conn: &Connection,
        ids: &[i64],
        home: Option<(String, String)>,
    ) -> Facts {
        let mut facts = Facts {
            home,
            ..Facts::default()
        };
        let mut places: HashMap<(String, String), Place> = HashMap::new();
        let mut latitudes = Vec::new();
        let mut people: HashMap<String, usize> = HashMap::new();
        for chunk in ids.chunks(900) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT date_taken, location_city, COALESCE(location_country, ''), gps_latitude
                 FROM photos WHERE id IN ({marks}) AND is_trashed = FALSE"
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<f64>>(3)?,
                    ))
                });
                for (date, city, country, lat) in rows.into_iter().flatten().flatten() {
                    facts.photos += 1;
                    let when = date.as_deref().and_then(parse_datetime);
                    if let Some(when) = when {
                        facts.start = Some(facts.start.map_or(when, |s| s.min(when)));
                        facts.end = Some(facts.end.map_or(when, |e| e.max(when)));
                    }
                    if let Some(lat) = lat {
                        latitudes.push(lat);
                    }
                    if let Some(city) = city.filter(|c| !c.trim().is_empty()) {
                        let first = when.unwrap_or_default();
                        let place =
                            places
                                .entry((city.clone(), country.clone()))
                                .or_insert(Place {
                                    city,
                                    country,
                                    photos: 0,
                                    first,
                                });
                        place.photos += 1;
                        if when.is_some() && first < place.first {
                            place.first = first;
                        }
                    }
                }
            }
            // People hidden from memories never appear in a title.
            let sql = format!(
                "SELECT fc.name, COUNT(DISTINCT f.photo_id)
                 FROM faces f JOIN face_clusters fc ON fc.id = f.cluster_id
                 WHERE f.photo_id IN ({marks})
                   AND NOT EXISTS (SELECT 1 FROM memory_blocks mb
                                   WHERE mb.kind = 'person' AND mb.target_key = CAST(fc.id AS TEXT))
                 GROUP BY fc.id"
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |row| {
                    Ok((row.get::<_, Option<String>>(0)?, row.get::<_, usize>(1)?))
                });
                for (name, count) in rows.into_iter().flatten().flatten() {
                    facts.faces += 1;
                    if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                        *people.entry(name.trim().to_string()).or_default() += count;
                    }
                }
            }
        }
        let mut places: Vec<Place> = places.into_values().collect();
        places.sort_by(|a, b| a.first.cmp(&b.first).then(b.photos.cmp(&a.photos)));
        facts.places = places;
        let mut people: Vec<(String, usize)> = people.into_iter().collect();
        people.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        facts.people = people;
        if !latitudes.is_empty() {
            facts.latitude = Some(latitudes.iter().sum::<f64>() / latitudes.len() as f64);
        }
        facts
    }

    fn is_home(&self, place: &Place) -> bool {
        self.home
            .as_ref()
            .is_some_and(|(city, country)| &place.city == city && &place.country == country)
    }

    /// Places away from home, in the order they were visited.
    pub fn away(&self) -> Vec<&Place> {
        self.places.iter().filter(|p| !self.is_home(p)).collect()
    }

    /// The place most of the photos were taken, when one clearly dominates.
    pub fn main_place(&self) -> Option<&Place> {
        let best = self.places.iter().max_by_key(|p| p.photos)?;
        (best.photos * 2 >= self.photos.max(1)).then_some(best)
    }

    /// The main place, if it isn't home.
    pub fn main_away_place(&self) -> Option<&Place> {
        self.main_place().filter(|p| !self.is_home(p))
    }

    /// Named people who are in a good share of the photos (most first).
    pub fn regulars(&self, share: f64, limit: usize) -> Vec<&str> {
        let floor = ((self.photos as f64) * share).ceil().max(1.0) as usize;
        self.people
            .iter()
            .filter(|(_, count)| *count >= floor)
            .take(limit)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    fn days(&self) -> i64 {
        match (self.start, self.end) {
            (Some(s), Some(e)) => (e.date() - s.date()).num_days() + 1,
            _ => 1,
        }
    }
}

fn parse_datetime(raw: &str) -> Option<NaiveDateTime> {
    let raw = raw.get(..19).unwrap_or(raw).replace('T', " ");
    NaiveDateTime::parse_from_str(&raw, "%Y-%m-%d %H:%M:%S")
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(12, 0, 0))
        })
}

// ---------- Trips ----------

/// A trip away from home: "Christmas in Goa", "Paris to Bruges",
/// "First time in Switzerland", "Back in Goa", "Autumn in Bern".
pub fn trip(conn: &Connection, facts: &Facts) -> Story {
    let (Some(start), Some(end)) = (facts.start, facts.end) else {
        return Story {
            title: "A trip".into(),
            context: String::new(),
        };
    };
    let away = facts.away();
    let stops: Vec<&Place> = if away.is_empty() {
        facts.places.iter().collect()
    } else {
        away
    };
    let main = stops
        .iter()
        .copied()
        .max_by_key(|p| p.photos)
        .map(|p| p.city.as_str())
        .unwrap_or("somewhere new");
    let people = facts.regulars(0.3, 2);
    let mut extras: Vec<String> = Vec::new();

    let first_in_country = stops.first().and_then(|p| {
        let home_country = facts.home.as_ref().map(|h| h.1.as_str());
        (!p.country.is_empty()
            && Some(p.country.as_str()) != home_country
            && earlier_photos_in_country(conn, &p.country, start) == 0)
            .then(|| p.country.clone())
    });
    let earlier_years = stops
        .iter()
        .find(|p| p.city == main)
        .map(|p| earlier_years_in_city(conn, &p.city, &p.country, start))
        .unwrap_or(0);

    let title = if let Some(holiday) = holiday_span(start.date(), end.date()) {
        format!("{holiday} in {main}")
    } else if !people.is_empty() {
        format!("{main} with {}", join(&people))
    } else if stops.len() >= 2 {
        route(&stops)
    } else if let Some(country) = &first_in_country {
        format!("First time in {country}")
    } else if earlier_years > 0 {
        format!("Back in {main}")
    } else if is_weekend(start.date(), end.date()) {
        format!("A weekend in {main}")
    } else if let Some(season) = season(start.date(), facts.latitude) {
        format!("{season} in {main}")
    } else {
        format!("{} in {main}", capitalise(&duration(facts.days())))
    };

    if let Some(country) = &first_in_country {
        if !title.contains("First time") {
            extras.push(format!("your first time in {country}"));
        }
    }
    if earlier_years > 0 && !title.starts_with("Back in") {
        extras.push(format!("back in {main} after {}", since(conn, main, start)));
    }
    extras.push(duration(facts.days()));
    Story {
        title,
        context: with_extra(date_range(start.date(), end.date()), extras.first()),
    }
}

fn route(stops: &[&Place]) -> String {
    let countries: std::collections::HashSet<&str> = stops
        .iter()
        .map(|p| p.country.as_str())
        .filter(|c| !c.is_empty())
        .collect();
    match stops {
        [a, b] => format!("{} to {}", a.city, b.city),
        [a, b, c] => format!("{}, {} and {}", a.city, b.city, c.city),
        _ if countries.len() == 1 => format!("Across {}", countries.iter().next().unwrap()),
        _ => format!("{} to {}", stops[0].city, stops[stops.len() - 1].city),
    }
}

fn earlier_photos_in_country(conn: &Connection, country: &str, before: NaiveDateTime) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM photos WHERE location_country = ?1 AND date_taken < ?2 AND is_trashed = FALSE",
        rusqlite::params![country, before.format("%Y-%m-%d").to_string()],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

fn earlier_years_in_city(
    conn: &Connection,
    city: &str,
    country: &str,
    before: NaiveDateTime,
) -> i64 {
    // A visit counts when it was at least a month before this one, so a trip
    // that started a day earlier doesn't look like a return.
    let cutoff = (before.date() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    conn.query_row(
        "SELECT COUNT(DISTINCT strftime('%Y', date_taken)) FROM photos
         WHERE location_city = ?1 AND COALESCE(location_country, '') = ?2
           AND date_taken < ?3 AND is_trashed = FALSE",
        rusqlite::params![city, country, cutoff],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// "two years", "five months": time since the last earlier visit.
fn since(conn: &Connection, city: &str, before: NaiveDateTime) -> String {
    let cutoff = (before.date() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    let last: Option<String> = conn
        .query_row(
            "SELECT MAX(date_taken) FROM photos WHERE location_city = ?1 AND date_taken < ?2 AND is_trashed = FALSE",
            rusqlite::params![city, cutoff],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let Some(last) = last.as_deref().and_then(parse_datetime) else {
        return "a while".into();
    };
    let months = (before.year() - last.year()) * 12 + before.month() as i32 - last.month() as i32;
    match months {
        ..=1 => "a month".into(),
        2..=11 => format!("{} months", number(months as i64)),
        12..=17 => "a year".into(),
        _ => format!("{} years", number((months / 12) as i64)),
    }
}

// ---------- Days out ----------

/// A day or an evening: "Christmas Day with Meera", "A Sunday afternoon in
/// Paris", "A Saturday evening at home".
pub fn moment(facts: &Facts) -> Story {
    let Some(start) = facts.start else {
        return Story {
            title: "A day worth keeping".into(),
            context: String::new(),
        };
    };
    let end = facts.end.unwrap_or(start);
    let when = format!("{} {}", weekday(start.weekday()), daypart(start.hour()));
    let people = facts.regulars(0.25, 2);
    let away = facts.main_away_place().map(|p| p.city.clone());
    let at_home = facts.main_place().is_some() && away.is_none();

    let title = if let Some(day) = holiday_day(start.date()) {
        if !people.is_empty() {
            format!("{day} with {}", join(&people))
        } else if let Some(city) = &away {
            format!("{day} in {city}")
        } else if at_home {
            format!("{day} at home")
        } else {
            day.to_string()
        }
    } else if !people.is_empty() {
        format!("A {when} with {}", join(&people))
    } else if let Some(city) = &away {
        format!("A {when} in {city}")
    } else if at_home {
        format!("A {when} at home")
    } else {
        format!("A {when} worth keeping")
    };
    let span = (end - start).num_minutes();
    let extra = (facts.photos >= 20 && span <= 6 * 60)
        .then(|| format!("{} photos in {}", facts.photos, hours(span)));
    Story {
        title,
        context: with_extra(date_range(start.date(), end.date()), extra.as_ref()),
    }
}

/// People together over a day or two: "Meera, Dev and three others",
/// "Five people, one Saturday".
pub fn gathering(facts: &Facts) -> Story {
    let Some(start) = facts.start else {
        return Story {
            title: "A get-together".into(),
            context: String::new(),
        };
    };
    let end = facts.end.unwrap_or(start);
    if holiday_day(start.date()).is_some() {
        return moment(facts);
    }
    let named = facts.regulars(0.2, 2);
    let others = facts.faces.saturating_sub(named.len());
    let one_day = if end.date() == start.date() {
        format!("one {}", weekday(start.weekday()))
    } else if is_weekend(start.date(), end.date()) {
        "one weekend".into()
    } else {
        format!("{} days", number(facts.days()))
    };
    let title = match (named.as_slice(), others) {
        ([], _) => format!(
            "{} people, {one_day}",
            capitalise(&number(facts.faces as i64))
        ),
        (names, 0) => format!("{}, {one_day}", join(names)),
        (names, 1) => format!("{} and one other", names.join(", ")),
        (names, n) => format!("{} and {} others", names.join(", "), number(n as i64)),
    };
    Story {
        title,
        context: with_extra(
            date_range(start.date(), end.date()),
            Some(&format!("{} photos", facts.photos)),
        ),
    }
}

// ---------- Memories ----------

/// "Three years ago today" grows a place or a person when one stands out:
/// "Three years ago today, in Bern".
pub fn looking_back(base: &str, facts: &Facts) -> String {
    if let Some(place) = facts.main_away_place() {
        return format!("{base}, in {}", place.city);
    }
    match facts.regulars(0.4, 1).first() {
        Some(name) => format!("{base}, with {name}"),
        None => base.to_string(),
    }
}

/// A month from an earlier year: "Bern, October 2023", "October 2023 with
/// Meera", or "October 2023, a look back".
pub fn month_back(month: &str, year: i32, facts: &Facts) -> String {
    if let Some(place) = facts.main_away_place() {
        return format!("{}, {month} {year}", place.city);
    }
    match facts.regulars(0.4, 1).first() {
        Some(name) => format!("{month} {year} with {name}"),
        None => format!("{month} {year}, a look back"),
    }
}

/// A whole year: "2023: Bern, Goa and more" or "Your 2023".
pub fn year_back(year: i32, facts: &Facts) -> String {
    let mut away: Vec<&Place> = facts.away();
    away.sort_by_key(|p| std::cmp::Reverse(p.photos));
    let names: Vec<&str> = away.iter().take(2).map(|p| p.city.as_str()).collect();
    match (names.as_slice(), away.len()) {
        ([], _) => format!("Your {year}"),
        ([one], _) => format!("{year}: home and {one}"),
        ([a, b], 2) => format!("{year}: {a} and {b}"),
        ([a, b], _) => format!("{year}: {a}, {b} and more"),
        _ => format!("Your {year}"),
    }
}

/// "3 years ago today" → "Three years ago today"; "1 year ago" → "A year ago".
pub fn spelled(label: &str) -> String {
    let digits: String = label.chars().take_while(|c| c.is_ascii_digit()).collect();
    let Ok(n) = digits.parse::<i64>() else {
        return capitalise(label);
    };
    let rest = &label[digits.len()..];
    if n == 1 {
        format!("A{rest}")
    } else {
        format!("{}{rest}", capitalise(&number(n)))
    }
}

// ---------- Small words ----------

fn holiday_span(start: NaiveDate, end: NaiveDate) -> Option<&'static str> {
    let christmas = covers(start, end, 12, 25);
    let new_year = covers(start, end, 12, 31) || covers(start, end, 1, 1);
    match (christmas, new_year) {
        (true, true) => Some("The holidays"),
        (true, false) => Some("Christmas"),
        (false, true) => Some("New Year"),
        _ => None,
    }
}

fn covers(start: NaiveDate, end: NaiveDate, month: u32, day: u32) -> bool {
    (start.year()..=end.year())
        .any(|y| NaiveDate::from_ymd_opt(y, month, day).is_some_and(|d| d >= start && d <= end))
}

fn holiday_day(date: NaiveDate) -> Option<&'static str> {
    match (date.month(), date.day()) {
        (12, 24) => Some("Christmas Eve"),
        (12, 25) => Some("Christmas Day"),
        (12, 31) => Some("New Year's Eve"),
        (1, 1) => Some("New Year's Day"),
        _ => None,
    }
}

fn is_weekend(start: NaiveDate, end: NaiveDate) -> bool {
    let days = (end - start).num_days() + 1;
    (2..=3).contains(&days)
        && matches!(start.weekday(), Weekday::Fri | Weekday::Sat)
        && matches!(end.weekday(), Weekday::Sat | Weekday::Sun | Weekday::Mon)
}

/// Season where it means something; the month in the tropics.
fn season(date: NaiveDate, latitude: Option<f64>) -> Option<String> {
    let lat = latitude?;
    if lat.abs() < 23.5 {
        return Some(month_name(date.month()).to_string());
    }
    let north = [
        "Winter", "Winter", "Spring", "Spring", "Spring", "Summer", "Summer", "Summer", "Autumn",
        "Autumn", "Autumn", "Winter",
    ];
    let south = [
        "Summer", "Summer", "Autumn", "Autumn", "Autumn", "Winter", "Winter", "Winter", "Spring",
        "Spring", "Spring", "Summer",
    ];
    let table = if lat >= 0.0 { north } else { south };
    Some(table[date.month0() as usize].to_string())
}

fn daypart(hour: u32) -> &'static str {
    match hour {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=21 => "evening",
        _ => "night",
    }
}

fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

pub fn month_name(month: u32) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ]
    .get(month.saturating_sub(1) as usize)
    .copied()
    .unwrap_or("")
}

fn duration(days: i64) -> String {
    match days {
        ..=1 => "one day".into(),
        2..=6 => format!("{} days", number(days)),
        7..=10 => "a week".into(),
        11..=17 => "two weeks".into(),
        _ => format!("{} weeks", number((days + 3) / 7)),
    }
}

fn hours(minutes: i64) -> String {
    match minutes {
        ..=59 => "under an hour".into(),
        60..=119 => "an hour".into(),
        _ => format!("{} hours", number(minutes / 60)),
    }
}

fn number(n: i64) -> String {
    const WORDS: [&str; 13] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    WORDS
        .get(n as usize)
        .map(|w| w.to_string())
        .unwrap_or_else(|| n.to_string())
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn join(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn with_extra(base: String, extra: Option<&String>) -> String {
    match extra {
        Some(extra) => format!("{base} · {extra}"),
        None => base,
    }
}

/// When a set of photos was taken, briefly: "Dec 2023", "Oct – Dec 2023",
/// "2019 – 2023".
pub fn span_label(conn: &Connection, ids: &[i64]) -> Option<String> {
    let mut first: Option<NaiveDateTime> = None;
    let mut last: Option<NaiveDateTime> = None;
    for chunk in ids.chunks(900) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT MIN(date_taken), MAX(date_taken) FROM photos WHERE id IN ({marks}) AND is_trashed = FALSE"
        );
        let (lo, hi): (Option<String>, Option<String>) = conn
            .query_row(&sql, rusqlite::params_from_iter(chunk), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .ok()?;
        if let Some(lo) = lo.as_deref().and_then(parse_datetime) {
            first = Some(first.map_or(lo, |f| f.min(lo)));
        }
        if let Some(hi) = hi.as_deref().and_then(parse_datetime) {
            last = Some(last.map_or(hi, |l| l.max(hi)));
        }
    }
    let (first, last) = (first?, last?);
    Some(if first.year() != last.year() {
        format!("{} – {}", first.year(), last.year())
    } else if first.month() != last.month() {
        format!("{} – {}", first.format("%b"), last.format("%b %Y"))
    } else {
        first.format("%b %Y").to_string()
    })
}

/// "Oct 15 – 19, 2023", "Dec 30, 2023 – Jan 2, 2024", "Jun 11, 2022".
pub fn date_range(start: NaiveDate, end: NaiveDate) -> String {
    if start == end {
        start.format("%b %-d, %Y").to_string()
    } else if start.year() == end.year() && start.month() == end.month() {
        format!(
            "{} – {}, {}",
            start.format("%b %-d"),
            end.format("%-d"),
            start.year()
        )
    } else if start.year() == end.year() {
        format!("{} – {}", start.format("%b %-d"), end.format("%b %-d, %Y"))
    } else {
        format!(
            "{} – {}",
            start.format("%b %-d, %Y"),
            end.format("%b %-d, %Y")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
    }

    fn place(city: &str, country: &str, photos: usize, first: NaiveDateTime) -> Place {
        Place {
            city: city.into(),
            country: country.into(),
            photos,
            first,
        }
    }

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        conn
    }

    fn trip_facts(places: Vec<Place>, start: NaiveDateTime, end: NaiveDateTime) -> Facts {
        let photos = places.iter().map(|p| p.photos).sum();
        Facts {
            start: Some(start),
            end: Some(end),
            places,
            photos,
            latitude: Some(46.7),
            home: Some(("Vizianagaram".into(), "India".into())),
            ..Facts::default()
        }
    }

    #[test]
    fn trips_lead_with_the_most_specific_true_thing() {
        let conn = conn();
        let s = at(2023, 10, 15, 9);
        let e = at(2023, 10, 19, 18);
        let swiss = trip(
            &conn,
            &trip_facts(vec![place("Bern", "Switzerland", 16, s)], s, e),
        );
        assert_eq!(swiss.title, "First time in Switzerland");
        assert_eq!(swiss.context, "Oct 15 – 19, 2023 · five days");

        let home_country = trip(&conn, &trip_facts(vec![place("Goa", "India", 18, s)], s, e));
        assert_eq!(home_country.title, "Autumn in Goa");

        let xmas = trip(
            &conn,
            &trip_facts(
                vec![place("Goa", "India", 18, at(2023, 12, 23, 9))],
                at(2023, 12, 23, 9),
                at(2023, 12, 27, 9),
            ),
        );
        assert_eq!(xmas.title, "Christmas in Goa");

        let two_stops = trip(
            &conn,
            &trip_facts(
                vec![
                    place("Paris", "France", 4, s),
                    place("Bruges", "Belgium", 3, e),
                ],
                s,
                e,
            ),
        );
        assert_eq!(two_stops.title, "Paris to Bruges");

        let mut with_people = trip_facts(vec![place("Goa", "India", 10, s)], s, e);
        with_people.people = vec![("Meera".into(), 6), ("Dev".into(), 4)];
        assert_eq!(trip(&conn, &with_people).title, "Goa with Meera and Dev");
    }

    #[test]
    fn days_out_name_the_day_and_who_or_where() {
        let mut facts = Facts {
            start: Some(at(2024, 5, 19, 14)),
            end: Some(at(2024, 5, 19, 17)),
            places: vec![place("Paris", "France", 30, at(2024, 5, 19, 14))],
            photos: 30,
            home: Some(("Vizianagaram".into(), "India".into())),
            ..Facts::default()
        };
        let story = moment(&facts);
        assert_eq!(story.title, "A Sunday afternoon in Paris");
        assert_eq!(story.context, "May 19, 2024 · 30 photos in three hours");

        facts.people = vec![("Meera".into(), 20)];
        assert_eq!(moment(&facts).title, "A Sunday afternoon with Meera");

        facts.start = Some(at(2024, 12, 25, 10));
        facts.end = Some(at(2024, 12, 25, 12));
        assert_eq!(moment(&facts).title, "Christmas Day with Meera");
    }

    #[test]
    fn gatherings_count_the_people() {
        let facts = Facts {
            start: Some(at(2024, 6, 8, 11)),
            end: Some(at(2024, 6, 8, 16)),
            photos: 12,
            faces: 5,
            ..Facts::default()
        };
        assert_eq!(gathering(&facts).title, "Five people, one Saturday");
        let named = Facts {
            people: vec![("Meera".into(), 8), ("Dev".into(), 6)],
            ..facts
        };
        assert_eq!(gathering(&named).title, "Meera, Dev and three others");
    }

    #[test]
    fn memories_add_a_place_or_person_when_one_stands_out() {
        let s = at(2023, 10, 17, 9);
        let facts = trip_facts(vec![place("Bern", "Switzerland", 6, s)], s, s);
        assert_eq!(spelled("3 years ago today"), "Three years ago today");
        assert_eq!(spelled("1 year ago this week"), "A year ago this week");
        assert_eq!(spelled("14 years ago"), "14 years ago");
        assert_eq!(
            looking_back("Three years ago today", &facts),
            "Three years ago today, in Bern"
        );
        assert_eq!(month_back("October", 2023, &facts), "Bern, October 2023");
        let home = trip_facts(vec![place("Vizianagaram", "India", 6, s)], s, s);
        assert_eq!(
            month_back("October", 2023, &home),
            "October 2023, a look back"
        );
        assert_eq!(year_back(2023, &home), "Your 2023");
    }

    #[test]
    fn stored_titles_split_back_apart() {
        let story = Story {
            title: "Back in Goa".into(),
            context: "Dec 26 – 30, 2023".into(),
        };
        assert_eq!(split(&story.joined()), ("Back in Goa", "Dec 26 – 30, 2023"));
        assert_eq!(split("Old title"), ("Old title", ""));
    }
}
