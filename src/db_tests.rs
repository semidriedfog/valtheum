use super::*;

fn library() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_db(&conn).unwrap();
    for name in ["b.mp4", "A.mp4", "a.mp4"] {
        upsert_media_file(&conn, name, "VIDEO", 10, 100).unwrap();
    }
    conn
}

#[test]
fn unchanged_upsert_does_not_write_or_allocate_ids() {
    let conn = library();
    let changes = conn
        .query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
        .unwrap();
    assert_eq!(
        upsert_media_file(&conn, "b.mp4", "VIDEO", 10, 100).unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        changes
    );
    upsert_media_file(&conn, "new.mp4", "VIDEO", 10, 100).unwrap();
    let id: i64 = conn
        .query_row(
            "SELECT id FROM media_files WHERE rel_path = 'new.mp4'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(id, 4);
}

#[test]
fn init_enables_foreign_keys() {
    let conn = Connection::open_in_memory().unwrap();
    init_db(&conn).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn lists_keep_tags_thumbnails_and_watch_metadata() {
    let conn = library();
    add_tag_to_file(&conn, 1, "zebra").unwrap();
    add_tag_to_file(&conn, 1, "alpha").unwrap();
    add_tag_to_file(&conn, 2, "alpha").unwrap();
    update_rating(&conn, 1, 4).unwrap();
    update_last_watched(&conn, 1, 123).unwrap();
    update_duration(&conn, 1, 75).unwrap();
    save_thumbnail(&conn, 1, &[1, 2, 3], 320, 180, Some(2.5), Some(3)).unwrap();
    upsert_media_file(&conn, "b.mp4", "VIDEO", 20, 200).unwrap();
    let files = get_media_files(&conn, None, None).unwrap();
    let file = files.iter().find(|f| f.id == 1).unwrap();
    assert_eq!(
        file.tags
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zebra"]
    );
    assert_eq!(
        (
            file.rating,
            file.watch_count,
            file.last_watched_at,
            file.duration_secs
        ),
        (4, 1, Some(123), Some(75))
    );
    assert_eq!(file.thumbnail_data.as_deref(), Some("/api/thumbnail/1?v=1"));
    assert_eq!((file.time_offset, file.page_offset), (Some(2.5), Some(3)));
    assert!(files.iter().find(|f| f.id == 3).unwrap().tags.is_empty());
    move_to_trash(&conn, 1).unwrap();
    assert_eq!(get_media_files(&conn, None, None).unwrap().len(), 2);
    assert_eq!(get_trash_count(&conn).unwrap(), 1);
    let trash = get_trash_files(&conn).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].tags.len(), 2);
    assert!(trash[0].is_trash);
    restore_from_trash(&conn, 1).unwrap();
    assert_eq!(get_trash_count(&conn).unwrap(), 0);
    assert!(get_trash_files(&conn).unwrap().is_empty());
}

#[test]
fn thumbnail_revision_changes_without_loading_blob_into_lists() {
    let conn = library();
    save_thumbnail(&conn, 1, &[1, 2, 3], 320, 180, None, None).unwrap();
    assert_eq!(
        get_media_files(&conn, None, None)
            .unwrap()
            .iter()
            .find(|file| file.id == 1)
            .unwrap()
            .thumbnail_data,
        Some("/api/thumbnail/1?v=1".to_string())
    );
    save_thumbnail(&conn, 1, &[4, 5], 320, 180, None, None).unwrap();
    assert_eq!(
        get_media_files(&conn, None, None)
            .unwrap()
            .iter()
            .find(|file| file.id == 1)
            .unwrap()
            .thumbnail_data,
        Some("/api/thumbnail/1?v=2".to_string())
    );
}

#[test]
fn legacy_thumbnails_migrate_without_losing_image_or_rating() {
    let conn = library();
    save_thumbnail(&conn, 1, &[9, 8, 7], 10, 20, Some(2.0), None).unwrap();
    update_rating(&conn, 1, 4).unwrap();
    conn.execute("ALTER TABLE thumbnails DROP COLUMN revision", [])
        .unwrap();
    init_db(&conn).unwrap();
    init_db(&conn).unwrap();
    let file = get_media_files(&conn, None, None)
        .unwrap()
        .into_iter()
        .find(|f| f.id == 1)
        .unwrap();
    assert_eq!(file.rating, 4);
    assert_eq!(file.thumbnail_data.as_deref(), Some("/api/thumbnail/1?v=0"));
    assert_eq!(get_thumbnail(&conn, 1).unwrap(), Some(vec![9, 8, 7]));
}

#[test]
fn permanent_delete_failure_keeps_media_and_log() {
    let root = std::env::temp_dir().join(format!(
        "vault-delete-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("locked.mp4")).unwrap();
    let conn = Connection::open_in_memory().unwrap();
    init_db(&conn).unwrap();
    let id = upsert_media_file(&conn, "locked.mp4", "VIDEO", 10, 100).unwrap();

    move_to_trash(&conn, id as i64).unwrap();
    assert!(permanently_delete_file(&conn, &root, id as i64).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM media_files", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM deleted_files_log", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(root.join("locked.mp4").is_dir());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sorting_preserves_direction_and_tie_breaks() {
    let conn = library();
    for key in [
        "rating",
        "date",
        "modified_at",
        "watch_count",
        "last_watched",
    ] {
        for (order, expected) in [("asc", vec![1, 2, 3]), ("desc", vec![3, 2, 1])] {
            let ids: Vec<_> = get_media_files(&conn, Some(key), Some(order))
                .unwrap()
                .iter()
                .map(|f| f.id)
                .collect();
            assert_eq!(ids, expected, "{key} {order}");
        }
    }
    for (key, order, expected) in [
        ("score", "asc", vec![3, 2, 1]),
        ("invalid", "desc", vec![3, 2, 1]),
        ("name", "asc", vec![3, 2, 1]),
        ("name", "DESC", vec![1, 3, 2]),
    ] {
        let ids: Vec<_> = get_media_files(&conn, Some(key), Some(order))
            .unwrap()
            .iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(ids, expected, "{key} {order}");
    }
}

#[test]
fn arena_keeps_tags_and_excludes_trash_and_focus() {
    let conn = library();
    add_tag_to_file(&conn, 1, "favorite").unwrap();
    move_to_trash(&conn, 3).unwrap();
    let (focus, opponent) = get_arena_opponent(&conn, 1).unwrap().unwrap();
    assert_eq!((focus.id, opponent.id), (1, 2));
    assert_eq!(focus.tags[0].name, "favorite");
    assert!(get_arena_opponent(&conn, 3).unwrap().is_none());
    let (a, b) = get_arena_pair(&conn).unwrap().unwrap();
    assert_ne!(a.id, b.id);
    assert!(!a.is_trash && !b.is_trash);
    move_to_trash(&conn, 2).unwrap();
    assert!(get_arena_pair(&conn).unwrap().is_none());
}

fn deletion_fixture() -> (std::path::PathBuf, Connection) {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir().join(format!("vault-delete-{:x}", u64::from_ne_bytes(random)));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("clip.mp4"), b"fixture").unwrap();
    let conn = Connection::open_in_memory().unwrap();
    init_db(&conn).unwrap();
    upsert_media_file(&conn, "clip.mp4", "VIDEO", 7, 1).unwrap();
    add_tag_to_file(&conn, 1, "keep").unwrap();
    update_rating(&conn, 1, 5).unwrap();
    save_thumbnail(&conn, 1, &[1, 2, 3], 1, 1, None, None).unwrap();
    (root, conn)
}

#[cfg(windows)]
#[test]
fn locked_file_keeps_rating_tags_thumbnail_and_can_be_retried() {
    use std::os::windows::fs::OpenOptionsExt;
    let (root, conn) = deletion_fixture();
    move_to_trash(&conn, 1).unwrap();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(root.join("clip.mp4"))
        .unwrap();
    assert!(permanently_delete_file(&conn, &root, 1).is_err());
    let files = get_trash_files(&conn).unwrap();
    assert_eq!(files[0].rating, 5);
    assert_eq!(files[0].tags[0].name, "keep");
    assert_eq!(get_thumbnail(&conn, 1).unwrap(), Some(vec![1, 2, 3]));
    assert!(get_deleted_logs(&conn).unwrap().is_empty());
    drop(locked);
    permanently_delete_file(&conn, &root, 1).unwrap();
    assert!(!root.join("clip.mp4").exists());
    assert_eq!(get_deleted_logs(&conn).unwrap().len(), 1);
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn failed_database_finalization_is_recoverable_and_not_pruned() {
    let (root, conn) = deletion_fixture();
    move_to_trash(&conn, 1).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON media_files BEGIN SELECT RAISE(ABORT, 'test interruption'); END;").unwrap();
    assert!(permanently_delete_file(&conn, &root, 1).is_err());
    assert!(!root.join("clip.mp4").exists());
    assert!(get_deleted_logs(&conn).unwrap().is_empty());
    assert_eq!(get_trash_files(&conn).unwrap().len(), 1);
    assert!(get_all_active_rel_paths(&conn).unwrap().is_empty());
    conn.execute_batch("DROP TRIGGER fail_delete;").unwrap();
    recover_pending_deletions(&conn, &root).unwrap();
    assert!(get_trash_files(&conn).unwrap().is_empty());
    assert_eq!(
        get_deleted_logs(&conn).unwrap()[0].tags_info.as_deref(),
        Some("keep")
    );
    recover_pending_deletions(&conn, &root).unwrap();
    assert_eq!(get_deleted_logs(&conn).unwrap().len(), 1);
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn recovery_does_not_delete_files_when_only_intent_was_saved() {
    let (root, conn) = deletion_fixture();
    conn.execute(
        "INSERT INTO delete_intents(file_id, rel_path) VALUES(1, 'clip.mp4')",
        [],
    )
    .unwrap();
    recover_pending_deletions(&conn, &root).unwrap();
    assert_eq!(std::fs::read(root.join("clip.mp4")).unwrap(), b"fixture");
    assert_eq!(get_media_files(&conn, None, None).unwrap()[0].rating, 5);
    assert_eq!(get_all_active_rel_paths(&conn).unwrap().len(), 1);
    assert!(get_deleted_logs(&conn).unwrap().is_empty());
    std::fs::remove_file(root.join("clip.mp4")).unwrap();
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn bookmarks_persist_replace_and_isolate_by_file() {
    let conn = library();
    let first = save_bookmark(&conn, 1, 12.5, &[0xff, 0xd8, 0xff, 1]).unwrap();
    let replaced = save_bookmark(&conn, 1, 12.5, &[0xff, 0xd8, 0xff, 2]).unwrap();
    assert_eq!(first.id, replaced.id);
    let second = save_bookmark(&conn, 2, 12.5, &[0xff, 0xd8, 0xff, 3]).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(list_bookmarks(&conn, 1).unwrap().len(), 1);
    assert_eq!(list_bookmarks(&conn, 2).unwrap().len(), 1);
    assert_eq!(
        get_bookmark_thumbnail(&conn, 1, first.id).unwrap(),
        Some(vec![0xff, 0xd8, 0xff, 2])
    );
}

#[test]
fn bookmarks_validate_media_position_image_and_delete() {
    let conn = library();
    upsert_media_file(&conn, "pages.cbz", "ARCHIVE", 10, 100).unwrap();
    assert!(matches!(
        save_bookmark(&conn, 4, 1.5, &[0xff, 0xd8, 0xff]),
        Err(BookmarkError::InvalidPosition)
    ));
    assert!(matches!(
        save_bookmark(&conn, 4, 1.0, b"not jpeg"),
        Err(BookmarkError::InvalidImage)
    ));
    assert!(matches!(
        save_bookmark(&conn, 999, 1.0, &[0xff, 0xd8, 0xff]),
        Err(BookmarkError::NotFound)
    ));
    let bookmark = save_bookmark(&conn, 4, 1.0, &[0xff, 0xd8, 0xff]).unwrap();
    delete_bookmark(&conn, 4, bookmark.id).unwrap();
    assert!(get_bookmark_thumbnail(&conn, 4, bookmark.id)
        .unwrap()
        .is_none());
}

#[test]
fn bookmarks_are_removed_when_media_is_removed() {
    let conn = library();
    let bookmark = save_bookmark(&conn, 1, 1.0, &[0xff, 0xd8, 0xff]).unwrap();
    remove_missing_file_records(&conn, &[1]).unwrap();
    assert!(get_bookmark_thumbnail(&conn, 1, bookmark.id)
        .unwrap()
        .is_none());
}

#[test]
fn permanent_delete_removes_all_dependent_records() {
    let (root, conn) = deletion_fixture();
    move_to_trash(&conn, 1).unwrap();
    permanently_delete_file(&conn, &root, 1).unwrap();
    for table in ["media_bookmarks", "thumbnails", "file_tags"] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table} left orphaned rows");
    }
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn arena_vote_rejects_same_or_trash_file() {
    let conn = library();
    assert!(vote_arena(&conn, 1, 1).is_err());
    move_to_trash(&conn, 2).unwrap();
    assert!(vote_arena(&conn, 1, 2).is_err());
}

#[test]
fn arena_vote_rolls_back_winner_when_loser_update_fails() {
    let conn = library();
    conn.execute_batch(
        "CREATE TRIGGER fail_loser_update BEFORE UPDATE OF score ON media_files
         WHEN NEW.id = 2 BEGIN SELECT RAISE(ABORT, 'test interruption'); END;",
    )
    .unwrap();

    assert!(vote_arena(&conn, 1, 2).is_err());
    let scores: Vec<i64> = conn
        .prepare("SELECT score FROM media_files WHERE id IN (1, 2) ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(scores, vec![1000, 1000]);
}

#[test]
fn missing_file_cleanup_rolls_back_dependents_when_media_delete_fails() {
    let conn = library();
    let bookmark = save_bookmark(&conn, 1, 1.0, &[0xff, 0xd8, 0xff]).unwrap();
    add_tag_to_file(&conn, 1, "keep").unwrap();
    save_thumbnail(&conn, 1, &[1, 2, 3], 1, 1, None, None).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_missing_delete BEFORE DELETE ON media_files
         WHEN OLD.id = 1 BEGIN SELECT RAISE(ABORT, 'test interruption'); END;",
    )
    .unwrap();

    assert!(remove_missing_file_records(&conn, &[1]).is_err());
    assert_eq!(
        get_bookmark_thumbnail(&conn, 1, bookmark.id).unwrap(),
        Some(vec![0xff, 0xd8, 0xff])
    );
    assert_eq!(get_thumbnail(&conn, 1).unwrap(), Some(vec![1, 2, 3]));
    assert_eq!(get_tags_for_file(&conn, 1).unwrap().len(), 1);
    assert_eq!(get_media_files(&conn, None, None).unwrap().len(), 3);
}

#[test]
fn deleting_history_preserves_files_trash_metadata_and_recovery_intents() {
    let (root, conn) = deletion_fixture();
    move_to_trash(&conn, 1).unwrap();
    save_bookmark(&conn, 1, 2.0, &[0xff, 0xd8, 0xff]).unwrap();
    conn.execute(
        "INSERT INTO delete_intents (file_id, rel_path) VALUES (1, 'clip.mp4')",
        [],
    )
    .unwrap();
    for name in ["clip.mp4", "other.mp4"] {
        conn.execute("INSERT INTO deleted_files_log (original_rel_path, file_type, file_size, score, deleted_at) VALUES (?1, 'VIDEO', 7, 1000, 1)", [name]).unwrap();
    }
    assert_eq!(delete_deleted_log(&conn, 1).unwrap(), 1);
    assert_eq!(delete_deleted_log(&conn, 1).unwrap(), 0);
    let logs = get_deleted_logs(&conn).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].original_rel_path, "other.mp4");
    assert_eq!(clear_deleted_logs(&conn).unwrap(), 1);
    assert_eq!(clear_deleted_logs(&conn).unwrap(), 0);
    assert!(get_deleted_logs(&conn).unwrap().is_empty());
    let files = get_trash_files(&conn).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].rating, 5);
    assert_eq!(files[0].tags[0].name, "keep");
    assert_eq!(get_thumbnail(&conn, 1).unwrap(), Some(vec![1, 2, 3]));
    assert_eq!(list_bookmarks(&conn, 1).unwrap().len(), 1);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM delete_intents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(std::fs::read(root.join("clip.mp4")).unwrap(), b"fixture");
    std::fs::remove_file(root.join("clip.mp4")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn media_encryption_flag_tracks_ready_archive_storage_without_relying_on_filename() {
    let conn = Connection::open_in_memory().unwrap();
    init_db(&conn).unwrap();
    for name in [
        "plain.zip",
        "converted.tar.zst",
        "interrupted.zip",
        "name-only.vta",
    ] {
        upsert_media_file(&conn, name, "ARCHIVE", 10, 100).unwrap();
    }
    conn.execute("INSERT INTO archive_store(file_id,encrypted_path,state) VALUES(2,'converted.vta','ready'),(3,'interrupted.vta','pending')", []).unwrap();
    let files = get_media_files(&conn, Some("name"), Some("asc")).unwrap();
    assert_eq!(files.len(), 4);
    for file in &files {
        assert_eq!(file.encrypted, file.rel_path == "converted.tar.zst");
    }
    conn.execute("UPDATE archive_store SET state='ready' WHERE file_id=3", [])
        .unwrap();
    assert!(
        get_media_files(&conn, None, None)
            .unwrap()
            .iter()
            .find(|f| f.id == 3)
            .unwrap()
            .encrypted
    );
    move_to_trash(&conn, 2).unwrap();
    assert!(get_trash_files(&conn).unwrap()[0].encrypted);
}

#[test]
fn missing_files_keep_ratings_but_cannot_enter_or_vote_in_arena() {
    let conn = library();
    conn.execute(
        "UPDATE media_files SET is_missing=1,rating=5 WHERE id=1",
        [],
    )
    .unwrap();
    let pair = get_arena_pair(&conn).unwrap().unwrap();
    assert_ne!(pair.0.id, 1);
    assert_ne!(pair.1.id, 1);
    assert!(get_arena_opponent(&conn, 1).unwrap().is_none());
    assert!(vote_arena(&conn, 1, 2).is_err());
    assert_eq!(
        get_media_files(&conn, None, None)
            .unwrap()
            .iter()
            .find(|f| f.id == 1)
            .unwrap()
            .rating,
        5
    );
}
