use smriti::db::{create_schema, Database};
use smriti::services::{
    reindexer::{IndexChanges, Reindexer},
    trash::TrashService,
};

fn library() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open_for_drive(dir.path()).unwrap();
    create_schema(&db.conn).unwrap();
    db.conn.execute("INSERT INTO photos(id,file_path,file_name,file_hash,file_size) VALUES(1,'one.jpg','one.jpg','abcd',8)", []).unwrap();
    std::fs::write(dir.path().join("one.jpg"), b"original").unwrap();
    (dir, db)
}

#[test]
fn deletion_sql_failure_restores_staged_original_and_database_row() {
    let (dir, db) = library();
    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    db.conn
        .execute_batch(
            "CREATE TRIGGER reject_delete BEFORE DELETE ON photos
        BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;",
        )
        .unwrap();
    assert!(TrashService::permanent_delete(&db.conn, &[1], dir.path()).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("one.jpg")).unwrap(),
        b"original"
    );
    assert!(db
        .conn
        .query_row("SELECT is_trashed FROM photos WHERE id=1", [], |r| r
            .get::<_, bool>(0))
        .unwrap());
    TrashService::recover_deletions(&db.conn, dir.path()).unwrap();
    db.conn.execute_batch("DROP TRIGGER reject_delete").unwrap();
    let result = TrashService::permanent_delete(&db.conn, &[1], dir.path()).unwrap();
    assert_eq!(result.db_records_deleted, 1);
    assert!(!dir.path().join("one.jpg").exists());
}

#[test]
fn trash_restore_preserves_named_person() {
    let (_dir, db) = library();
    db.conn.execute_batch("INSERT INTO face_clusters(id,name,is_user_named,face_count,photo_count) VALUES(10,'Alice',1,1,1);
      INSERT INTO faces(id,photo_id,bbox_x,bbox_y,bbox_width,bbox_height,embedding,cluster_id,confidence) VALUES(1,1,0.1,0.1,0.2,0.2,zeroblob(16),10,0.9);
      UPDATE face_clusters SET representative_face_id=1 WHERE id=10;").unwrap();
    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    TrashService::restore_photos(&db.conn, &[1]).unwrap();
    let name: String = db
        .conn
        .query_row("SELECT name FROM face_clusters WHERE id=10", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(name, "Alice");
}

#[test]
fn unavailable_root_is_not_an_empty_library() {
    let (dir, db) = library();
    assert!(Reindexer::new()
        .detect_changes(&db.conn, &dir.path().join("unmounted"))
        .is_err());
    assert!(Reindexer::new()
        .detect_changes(&db.conn, &dir.path().join("one.jpg"))
        .is_err());
}

#[test]
fn backup_includes_committed_wal_rows() {
    let (dir, _db) = library();
    assert!(Database::backup(dir.path(), 0).is_err());
    let backup = Database::backup(dir.path(), 5).unwrap();
    let conn = rusqlite::Connection::open(backup).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn failed_sql_delete_restores_original() {
    let (dir, db) = library();
    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    db.conn.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON photos BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(TrashService::permanent_delete(&db.conn, &[1], dir.path()).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("one.jpg")).unwrap(),
        b"original"
    );
}

#[test]
fn deletion_recovery_never_overwrites_a_replacement_file() {
    let (dir, db) = library();
    let staged = dir.path().join(".photovault/delete-test");
    std::fs::create_dir(&staged).unwrap();
    std::fs::write(
        staged.join("intent.json"),
        br#"{"photo_id":1,"relative_path":"one.jpg"}"#,
    )
    .unwrap();
    std::fs::rename(dir.path().join("one.jpg"), staged.join("original")).unwrap();
    std::fs::write(dir.path().join("one.jpg"), b"replacement").unwrap();
    assert!(TrashService::recover_deletions(&db.conn, dir.path()).is_err());
    assert_eq!(std::fs::read(staged.join("original")).unwrap(), b"original");
    assert_eq!(
        std::fs::read(dir.path().join("one.jpg")).unwrap(),
        b"replacement"
    );
}

#[test]
fn permanent_delete_restores_normal_sqlite_sync_mode() {
    let (dir, db) = library();
    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    let before: i64 = db
        .conn
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .unwrap();
    let result = TrashService::permanent_delete(&db.conn, &[1], dir.path()).unwrap();
    assert_eq!(result.files_deleted, 1);
    let after: i64 = db
        .conn
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .unwrap();
    assert_eq!(before, after);
}

#[cfg(unix)]
#[test]
fn thumbnail_cleanup_cannot_follow_external_directory_symlink() {
    let (dir, db) = library();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("victim.jpg"), b"unrelated").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".photovault/thumbnails")).unwrap();
    db.conn
        .execute(
            "UPDATE photos SET thumbnail_path='.photovault/thumbnails/victim.jpg' WHERE id=1",
            [],
        )
        .unwrap();
    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    let result = TrashService::permanent_delete(&db.conn, &[1], dir.path()).unwrap();
    assert_eq!(result.db_records_deleted, 1);
    assert!(outside.path().join("victim.jpg").exists());
}

#[test]
fn modified_photo_invalidates_visual_derivatives() {
    let (dir, db) = library();
    db.conn
        .execute(
            "UPDATE photos SET phash=123,brightness=0.5,ocr_text='old' WHERE id=1",
            [],
        )
        .unwrap();
    let changes = IndexChanges {
        modified: vec![(1, dir.path().join("one.jpg"))],
        ..Default::default()
    };
    Reindexer::new().apply_changes(&db.conn, &changes).unwrap();
    let values: (Option<i64>, Option<f64>, Option<String>) = db
        .conn
        .query_row(
            "SELECT phash,brightness,ocr_text FROM photos WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(values, (None, None, None));
}

#[test]
fn exclusions_use_literal_directory_names() {
    for excluded in ["Trip_A", "Trip%A", "tripXA"] {
        let (_dir, db) = library();
        db.conn
            .execute(
                "UPDATE photos SET file_path='TripXA/one.jpg' WHERE id=1",
                [],
            )
            .unwrap();
        smriti::db::ExcludedFolderRepo::new(&db.conn)
            .insert_and_remove_indexed(excluded)
            .unwrap();
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "excluded {excluded}");
    }
}

#[test]
fn ocr_replacement_and_delete_remove_old_tokens() {
    let (_dir, db) = library();
    db.conn
        .execute("UPDATE photos SET ocr_text='oldtoken' WHERE id=1", [])
        .unwrap();
    db.conn
        .execute("UPDATE photos SET ocr_text='newtoken' WHERE id=1", [])
        .unwrap();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'oldtoken'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    db.conn
        .execute("DELETE FROM photos WHERE id=1", [])
        .unwrap();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'newtoken'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn assistant_http_requires_loopback_host() {
    for url in [
        "http://localhost.attacker.example/v1",
        "http://127.0.0.1.attacker.example",
        "https://user:password@example.com",
    ] {
        assert!(!smriti::config::is_allowed_assistant_url(url), "{url}");
    }
    for url in [
        "http://localhost:1234/v1",
        "http://127.0.0.2/v1",
        "http://[::1]:1234/v1",
        "https://example.com/v1",
    ] {
        assert!(smriti::config::is_allowed_assistant_url(url), "{url}");
    }
}

#[test]
fn secondary_connection_never_creates_a_missing_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.db");
    assert!(smriti::db::open_secondary(&path).is_err());
    assert!(!path.exists());
}

#[test]
fn added_files_are_inserted_once_and_trash_is_not_rediscovered() {
    let (dir, db) = library();
    let path = dir.path().join("two.jpg");
    std::fs::write(&path, b"new photo").unwrap();
    let changes = IndexChanges {
        added: vec![path],
        ..Default::default()
    };
    assert_eq!(
        Reindexer::new()
            .apply_changes(&db.conn, &changes)
            .unwrap()
            .new_files,
        1
    );
    assert_eq!(
        Reindexer::new()
            .apply_changes(&db.conn, &changes)
            .unwrap()
            .new_files,
        0
    );
    TrashService::trash_photos(&db.conn, &[1]).unwrap();
    assert!(Reindexer::new()
        .detect_changes(&db.conn, dir.path())
        .unwrap()
        .added
        .is_empty());
}

#[test]
fn migration_rebuilds_existing_ocr_index() {
    let (_dir, db) = library();
    db.conn
        .execute_batch(
            "DELETE FROM schema_version; INSERT INTO schema_version(version) VALUES(28);
        UPDATE photos SET ocr_text='newtoken';
        INSERT INTO photos_fts(rowid,ocr_text) VALUES(1,'oldtoken');
        DROP TRIGGER photos_fts_update;
        CREATE TRIGGER photos_fts_update AFTER UPDATE OF ocr_text ON photos BEGIN
            DELETE FROM photos_fts WHERE rowid=old.id;
            INSERT INTO photos_fts(rowid,ocr_text) VALUES(new.id,new.ocr_text);
        END;",
        )
        .unwrap();
    smriti::db::migrations::run_migrations(&db.conn).unwrap();
    smriti::db::migrations::run_migrations(&db.conn).unwrap();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'oldtoken'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    db.conn
        .execute("UPDATE photos SET ocr_text=NULL", [])
        .unwrap();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'newtoken'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn interrupted_deletion_recovers_using_database_commit_state() {
    for committed in [false, true] {
        let (dir, db) = library();
        let staged = dir.path().join(".photovault/delete-test");
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(
            staged.join("intent.json"),
            br#"{"photo_id":1,"relative_path":"one.jpg"}"#,
        )
        .unwrap();
        std::fs::rename(dir.path().join("one.jpg"), staged.join("original")).unwrap();
        if committed {
            db.conn
                .execute("DELETE FROM photos WHERE id=1", [])
                .unwrap();
        }
        TrashService::recover_deletions(&db.conn, dir.path()).unwrap();
        TrashService::recover_deletions(&db.conn, dir.path()).unwrap();
        assert_eq!(dir.path().join("one.jpg").exists(), !committed);
        assert!(!staged.exists());
    }
}

#[test]
fn duplicate_grouping_preserves_equal_hashes_and_transitive_matches() {
    let (dir, db) = library();
    db.conn.execute("DELETE FROM photos", []).unwrap();
    for (index, hash) in [0_i64, 0, 0, 15, 255, 65535].into_iter().enumerate() {
        db.conn.execute("INSERT INTO photos(id,file_path,file_name,file_hash,file_size,phash) VALUES(?1,?2,?2,?2,8,?3)", rusqlite::params![index as i64 + 1, format!("{index}.jpg"), hash]).unwrap();
    }
    let groups =
        smriti::services::duplicate_detector::DuplicateDetector::find_perceptual_duplicates(
            &db.conn,
            dir.path(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(groups.len(), 1);
    let mut ids = groups[0].photo_ids.clone();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2, 3, 4, 5]);
}

#[cfg(feature = "raw")]
#[test]
fn raw_thumbnail_uses_embedded_preview_decoder() {
    let (dir, _db) = library();
    let mut preview = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(32, 16)
        .write_to(&mut preview, image::ImageFormat::Jpeg)
        .unwrap();
    let preview = preview.into_inner();
    let mut bytes = b"II".to_vec();
    bytes.extend_from_slice(&42u16.to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    for (tag, value) in [(0x0201u16, 38u32), (0x0202u16, preview.len() as u32)] {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&preview);
    let path = dir.path().join("photo.nef");
    std::fs::write(&path, bytes).unwrap();
    let service = smriti::services::thumbnail::ThumbnailService::new(dir.path(), 1.0).unwrap();
    let result = service
        .generate_thumbnail(
            &path,
            "abcd1234",
            1,
            smriti::services::thumbnail::ThumbnailSize::Medium,
        )
        .unwrap();
    assert!(image::open(result).is_ok());
    let display = service
        .generate_thumbnail(
            &path,
            "abcd1234",
            6,
            smriti::services::thumbnail::ThumbnailSize::Original,
        )
        .unwrap();
    assert_eq!(image::image_dimensions(&display).unwrap(), (16, 32));
    assert!(smriti::services::image_io::needs_display_rendition(&path));
}
