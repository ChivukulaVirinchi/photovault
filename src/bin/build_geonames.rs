//! Build helper for GeoNames SQLite database.

use std::fs::File;
use std::io::{BufRead, BufReader};

use rusqlite::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open("data/geonames.db")?;

    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = OFF;")?;

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS cities (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            ascii_name TEXT NOT NULL,
            latitude REAL NOT NULL,
            longitude REAL NOT NULL,
            country_code TEXT NOT NULL,
            country_name TEXT NOT NULL,
            population INTEGER,
            timezone TEXT
        );

        CREATE TABLE IF NOT EXISTS countries (
            code TEXT PRIMARY KEY,
            name TEXT NOT NULL
        );
        "#,
    )?;

    // Load countries first
    let countries = std::fs::read_to_string("data/country_codes.txt")?;
    {
        let tx = conn.unchecked_transaction()?;
        for line in countries.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                tx.execute(
                    "INSERT OR IGNORE INTO countries (code, name) VALUES (?1, ?2)",
                    [parts[0], parts[1]],
                )?;
            }
        }
        tx.commit()?;
    }

    // Load cities in a single transaction (167K rows — MUST be transactional for speed)
    let file = File::open("data/cities1000.txt")?;
    let reader = BufReader::new(file);

    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare(
            r#"
            INSERT OR IGNORE INTO cities
                (id, name, ascii_name, latitude, longitude, country_code, country_name, population, timezone)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, (SELECT name FROM countries WHERE code = ?6), ?7, ?8)
            "#,
        )?;

        let mut count = 0;
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();

            if parts.len() >= 18 {
                let id: i64 = parts[0].parse().unwrap_or(0);
                let name = parts[1];
                let ascii_name = parts[2];
                let lat: f64 = parts[4].parse().unwrap_or(0.0);
                let lon: f64 = parts[5].parse().unwrap_or(0.0);
                let country_code = parts[8];
                let population: i64 = parts[14].parse().unwrap_or(0);
                let timezone = parts[17];

                stmt.execute(rusqlite::params![
                    id,
                    name,
                    ascii_name,
                    lat,
                    lon,
                    country_code,
                    population,
                    timezone
                ])?;
                count += 1;
            }
        }

        println!("Inserted {} cities", count);
    }

    tx.commit()?;

    // Create index after bulk insert (faster than during)
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_cities_coords ON cities(latitude, longitude);",
    )?;

    println!("GeoNames database created at data/geonames.db");
    Ok(())
}
