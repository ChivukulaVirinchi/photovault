//! Workflow test: libraries on a NAS or network share.
//!
//! SQLite's WAL mode does not work over SMB/NFS, so a library there runs
//! on the rollback journal (`smriti::db::network_drive`). These tests
//! check the switch in both directions on a local disk, by forcing
//! network mode, and — when pointed at a real share — that a scan runs
//! to completion alongside the background connections the app opens.
//!
//! Real share: set `SMRITI_NETWORK_TEST_DIR` to a writable folder on it
//! (e.g. `\\nas\photos\tmp` or `/mnt/nas/tmp`) and run
//! `cargo test -p smriti --test workflow_network_drive -- --ignored`.

mod common;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

use smriti::db::network_drive::{network_drive, set_forced};
use smriti::db::{db_path_for, open_secondary, Database, PhotoRepo};
use smriti::services::scanner;

fn journal_mode(conn: &Connection) -> String {
    conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap()
}

fn pragma_i64(conn: &Connection, name: &str) -> i64 {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .unwrap()
}

#[test]
fn forced_network_mode_switches_journal_and_back() {
    let (dir, db) = common::make_library();
    assert_eq!(journal_mode(&db.conn), "wal");
    assert_eq!(common::seed_photos(&db, 25), 25);
    db.flush_and_close();

    set_forced(dir.path(), true).unwrap();
    let db = Database::open_for_drive(dir.path()).unwrap();
    assert_eq!(journal_mode(&db.conn), "delete");
    assert_eq!(pragma_i64(&db.conn, "mmap_size"), 0);
    assert_eq!(pragma_i64(&db.conn, "synchronous"), 2, "FULL");
    assert_eq!(PhotoRepo::new(&db.conn).count().unwrap(), 25);
    assert!(!db_path_for(dir.path()).with_extension("db-wal").exists());

    // Background connections follow the file instead of forcing WAL back.
    let secondary = open_secondary(&db_path_for(dir.path())).unwrap();
    assert_eq!(journal_mode(&secondary), "delete");
    assert_eq!(pragma_i64(&secondary, "mmap_size"), 0);
    drop(secondary);
    assert_eq!(journal_mode(&db.conn), "delete");
    // Normal locking is back: a second connection can write meanwhile.
    let writer = open_secondary(&db_path_for(dir.path())).unwrap();
    writer
        .execute("UPDATE photos SET is_favorite = TRUE WHERE id = 1", [])
        .unwrap();
    drop(writer);
    drop(db);

    set_forced(dir.path(), false).unwrap();
    let db = Database::open_for_drive(dir.path()).unwrap();
    assert_eq!(journal_mode(&db.conn), "wal");
    assert_eq!(PhotoRepo::new(&db.conn).count().unwrap(), 25);
    let secondary = open_secondary(&db_path_for(dir.path())).unwrap();
    assert_ne!(pragma_i64(&secondary, "mmap_size"), 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SMRITI_NETWORK_TEST_DIR on a network share"]
async fn scan_completes_on_a_network_share() {
    let share = std::env::var("SMRITI_NETWORK_TEST_DIR")
        .expect("set SMRITI_NETWORK_TEST_DIR to a writable folder on a network share");
    let dir = tempfile::Builder::new()
        .prefix("smriti-test-")
        .tempdir_in(share)
        .unwrap();
    assert!(
        network_drive(dir.path()).detected,
        "{} is not recognised as a network share",
        dir.path().display()
    );
    for i in 0..40 {
        let shade = (i * 6) as u8;
        common::write_jpeg(
            dir.path(),
            &format!("IMG_{i:03}.jpg"),
            256,
            256,
            [shade, 90, 200],
        );
    }

    let db = Database::open_for_drive(dir.path()).unwrap();
    smriti::db::create_schema(&db.conn).unwrap();
    smriti::db::migrations::run_migrations(&db.conn).unwrap();
    assert_eq!(journal_mode(&db.conn), "delete");
    let db = Arc::new(Mutex::new(db));

    // While the scan writes, other connections read and write the way the
    // UI and the background passes do.
    let db_path = db_path_for(dir.path());
    let stop = Arc::new(AtomicBool::new(false));
    let traffic = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let conn = open_secondary(&db_path).unwrap();
            let mut rounds = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                PhotoRepo::new(&conn).count().unwrap();
                conn.execute(
                    "UPDATE photos SET is_favorite = NOT is_favorite WHERE id = 1",
                    [],
                )
                .unwrap();
                rounds += 1;
            }
            rounds
        })
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let (rx, handle) = scanner::start_scan(dir.path().to_path_buf(), db.clone(), cancel, false);
    tokio::spawn(async move { while rx.recv().await.is_ok() {} });
    let report = handle.await.unwrap();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(traffic.join().unwrap() > 0);

    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(report.files_inserted, 40);
    let db = db.lock().await;
    assert_eq!(PhotoRepo::new(&db.conn).count().unwrap(), 40);
}
