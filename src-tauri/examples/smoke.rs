//! Headless smoke harness — exercises non-library handlers and the
//! pagination round-trip. Ensures `cargo run --example smoke` doesn't
//! panic and every domain module compiles into the binary.
//!
//! For library-dependent commands, run M2's Tauri dev with a real
//! fixture drive — those need ONNX/SQLite state that's expensive to
//! prepare in-process.

use photovault_tauri_lib::pagination::{self, Cursor};

fn main() {
    let drives = photovault::services::drive_detector::DriveDetector::detect();
    println!("drives: {} detected", drives.len());

    let h = photovault::bootstrap::asset_health();
    println!("assets: {}", h.summary());

    let c = Cursor {
        date_taken: Some(chrono::Utc::now()),
        id: 12345,
    };
    let s = pagination::encode(c);
    let d = pagination::decode(Some(&s)).unwrap().unwrap();
    assert_eq!(c.id, d.id);
    println!("cursor round-trip ok ({} bytes)", s.len());

    println!("smoke ok");
}
