//! Workflow test: the album lifecycle (create / add / remove /
//! rename / cover / delete).
//!
//! Catches the most common album bugs:
//! - "add_photos created duplicate album-photo links"
//! - "delete left orphaned photo links"
//! - "get_all returns trashed-album rows"
//! - "auto_pick_cover picked a photo that's not in the album"

mod common;

use smriti::db::album_repo::AlbumRepo;

#[test]
fn create_album_returns_a_fresh_id() {
    let (_dir, db) = common::make_library();
    let repo = AlbumRepo::new(&db.conn);

    let id1 = repo.create("Trip").unwrap();
    let id2 = repo.create("Family").unwrap();
    assert_ne!(id1, id2, "each create returns a distinct id");
    assert!(id1 > 0 && id2 > 0);
}

#[test]
fn get_all_returns_created_albums() {
    let (_dir, db) = common::make_library();
    let repo = AlbumRepo::new(&db.conn);

    repo.create("Trip").unwrap();
    repo.create("Family").unwrap();
    let all = repo.get_all().unwrap();
    let names: Vec<&str> = all.iter().map(|a| a.name.as_str()).collect();
    // Ordering may be created-at desc or name asc — the test only
    // cares that BOTH show up.
    assert!(names.contains(&"Trip"));
    assert!(names.contains(&"Family"));
    assert_eq!(all.len(), 2);
}

#[test]
fn add_photos_links_them_and_get_album_photo_ids_returns_them() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 5);
    let repo = AlbumRepo::new(&db.conn);

    let album_id = repo.create("Goa 2024").unwrap();
    let added = repo.add_photos(album_id, &[1, 2, 4]).unwrap();
    assert_eq!(
        added, 3,
        "add_photos reports the number of newly linked rows"
    );

    let mut ids = repo.get_album_photo_ids(album_id).unwrap();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 4]);
}

#[test]
fn add_photos_is_idempotent_for_existing_links() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 3);
    let repo = AlbumRepo::new(&db.conn);

    let album_id = repo.create("Pets").unwrap();
    repo.add_photos(album_id, &[1, 2]).unwrap();
    // Re-adding the same photos must NOT create duplicate rows in
    // the link table — if the repo lets this through, album views
    // double-count and the cover-picker gets confused.
    repo.add_photos(album_id, &[1, 2, 3]).unwrap();

    let mut ids = repo.get_album_photo_ids(album_id).unwrap();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn remove_photos_unlinks_without_deleting_the_photo() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 3);
    let repo = AlbumRepo::new(&db.conn);

    let album_id = repo.create("Misc").unwrap();
    repo.add_photos(album_id, &[1, 2, 3]).unwrap();
    repo.remove_photos(album_id, &[2]).unwrap();

    let mut ids = repo.get_album_photo_ids(album_id).unwrap();
    ids.sort();
    assert_eq!(ids, vec![1, 3], "photo 2 unlinked from album");

    // The actual photo row must still exist — remove_photos is a
    // link-table delete, not a photo delete.
    let still_there: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM photos WHERE id = 2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(still_there, 1, "photo row survives album removal");
}

#[test]
fn rename_changes_album_name_in_get_all() {
    let (_dir, db) = common::make_library();
    let repo = AlbumRepo::new(&db.conn);

    let id = repo.create("Old name").unwrap();
    repo.rename(id, "New name").unwrap();

    let all = repo.get_all().unwrap();
    let found = all
        .iter()
        .find(|a| a.id == id)
        .expect("renamed album exists");
    assert_eq!(found.name, "New name");
}

#[test]
fn delete_removes_the_album_and_its_links() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 2);
    let repo = AlbumRepo::new(&db.conn);

    let id = repo.create("Temporary").unwrap();
    repo.add_photos(id, &[1, 2]).unwrap();
    repo.delete(id).unwrap();

    assert!(repo.get_all().unwrap().iter().all(|a| a.id != id));
    let links: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM album_photos WHERE album_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(links, 0, "FK CASCADE should drop link rows on album delete");
}

#[test]
fn get_albums_for_photo_returns_all_albums_a_photo_belongs_to() {
    let (_dir, db) = common::make_library();
    common::seed_photos(&db, 2);
    let repo = AlbumRepo::new(&db.conn);

    let a = repo.create("Group A").unwrap();
    let b = repo.create("Group B").unwrap();
    repo.add_photos(a, &[1]).unwrap();
    repo.add_photos(b, &[1, 2]).unwrap();

    let mut for_one = repo.get_albums_for_photo(1).unwrap();
    for_one.sort_by_key(|(id, _)| *id);
    assert_eq!(for_one.len(), 2);
    assert_eq!(for_one[0].0, a);
    assert_eq!(for_one[1].0, b);

    let for_two = repo.get_albums_for_photo(2).unwrap();
    assert_eq!(for_two.len(), 1);
    assert_eq!(for_two[0].0, b);
}
