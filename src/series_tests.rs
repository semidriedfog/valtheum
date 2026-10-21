use super::*;
use crate::db;
fn library() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    db::init_db(&conn).unwrap();
    for (name, kind, date) in [
        ("vol1.zip", "ARCHIVE", 90),
        ("vol2.tar.zst", "ARCHIVE", 10),
        ("middle.png", "IMAGE", 50),
        ("vol3.zip", "ARCHIVE", 99),
    ] {
        db::upsert_media_file(&conn, name, kind, 10, date).unwrap();
    }
    conn.execute_batch(
        "UPDATE media_files SET rating=3,score=1300,watch_count=5,last_watched_at=50 WHERE id=3;
        UPDATE media_files SET rating=1,score=1100,watch_count=1,last_watched_at=10 WHERE id=2;
        UPDATE media_files SET rating=5,score=1500,watch_count=9,last_watched_at=90 WHERE id=1;",
    )
    .unwrap();
    conn
}
fn input(id: Option<i64>, rep: i64, ids: &[i64]) -> SaveSeries {
    SaveSeries {
        id,
        name: "series".into(),
        representative_id: rep,
        file_ids: ids.to_vec(),
    }
}
fn ids(conn: &Connection, key: &str, order: &str) -> Vec<i64> {
    db::get_media_files(conn, Some(key), Some(order))
        .unwrap()
        .iter()
        .map(|f| f.id)
        .collect()
}
#[test]
fn video_series_share_metadata_sort_as_one_and_only_send_representative_to_arena() {
    let conn = library();
    conn.execute_batch("UPDATE media_files SET file_type='VIDEO',rel_path='show_' || id || '.mp4' WHERE id IN (1,2,4)").unwrap();
    let sid = save(&conn, &input(None, 2, &[1, 2, 4])).unwrap();
    assert_eq!(ids(&conn, "date", "desc"), [3, 1, 2, 4]);
    db::update_rating(&conn, 1, 4).unwrap();
    db::add_tag_to_file(&conn, 4, "series-tag").unwrap();
    assert_eq!(db::get_arena_opponent(&conn, 1).unwrap().unwrap().0.id, 2);
    assert!(db::vote_arena(&conn, 1, 3).is_err());
    let score = db::vote_arena(&conn, 2, 3).unwrap().0;
    let files = db::get_media_files(&conn, None, None).unwrap();
    for file in files.iter().filter(|f| f.series_id == Some(sid)) {
        assert_eq!(file.file_type, "VIDEO");
        assert_eq!((file.rating, file.score), (4, score));
        assert_eq!(file.tags[0].name, "series-tag");
    }
    assert_eq!(files.iter().find(|f| f.id == 1).unwrap().watch_count, 9);
    assert_eq!(files.iter().find(|f| f.id == 2).unwrap().watch_count, 1);
    save(&conn, &input(Some(sid), 4, &[4, 1, 2])).unwrap();
    assert_eq!(ids(&conn, "date", "desc"), [4, 1, 2, 3]);
    assert_eq!(db::get_arena_opponent(&conn, 1).unwrap().unwrap().0.id, 4);
    dissolve(&conn, sid).unwrap();
    assert!(db::get_media_files(&conn, None, None)
        .unwrap()
        .iter()
        .all(|f| f.series_id.is_none()));
}
#[test]
fn series_sort_as_units_by_representative_and_keep_volume_order_in_both_directions() {
    let conn = library();
    save(&conn, &input(None, 2, &[1, 2, 4])).unwrap();
    for key in [
        "score",
        "rating",
        "date",
        "modified_at",
        "watch_count",
        "last_watched",
    ] {
        assert_eq!(ids(&conn, key, "asc"), [1, 2, 4, 3], "{key}");
        assert_eq!(ids(&conn, key, "desc"), [3, 1, 2, 4], "{key}");
    }
    assert_eq!(ids(&conn, "name", "asc"), [3, 1, 2, 4]);
    assert_eq!(ids(&conn, "name", "desc"), [1, 2, 4, 3]);
    let sid = list(&conn).unwrap()[0].id;
    save(&conn, &input(Some(sid), 1, &[4, 2, 1])).unwrap();
    assert_eq!(ids(&conn, "date", "desc"), [4, 2, 1, 3]);
    assert_eq!(ids(&conn, "score", "desc"), [3, 4, 2, 1]);
}
#[test]
fn shared_rating_and_arena_score_apply_to_all_volumes_and_dissolve_restores_file_values() {
    let conn = library();
    let sid = save(&conn, &input(None, 2, &[1, 2, 4])).unwrap();
    db::update_rating(&conn, 1, 4).unwrap();
    let result = db::vote_arena(&conn, 2, 3).unwrap();
    assert!(result.0 > 1100);
    for f in db::get_media_files(&conn, None, None)
        .unwrap()
        .into_iter()
        .filter(|f| f.series_id.is_some())
    {
        assert_eq!((f.rating, f.score), (4, result.0));
    }
    assert!(db::vote_arena(&conn, 1, 2).is_err());
    assert_eq!(list(&conn).unwrap()[0].score, result.0);
    db::upsert_media_file(&conn, "vol1.zip", "ARCHIVE", 20, 200).unwrap();
    db::init_db(&conn).unwrap();
    assert_eq!(list(&conn).unwrap()[0].file_ids, [1, 2, 4]);
    dissolve(&conn, sid).unwrap();
    let files = db::get_media_files(&conn, None, None).unwrap();
    assert_eq!(files.len(), 4);
    assert!(files.iter().all(|f| f.series_id.is_none()));
    let first = files.iter().find(|f| f.id == 1).unwrap();
    assert_eq!((first.rating, first.score), (5, 1500));
    let second = files.iter().find(|f| f.id == 2).unwrap();
    assert_eq!((second.rating, second.score), (1, 1100));
}
#[test]
fn arena_uses_one_candidate_per_series_and_excludes_pinned_members_of_same_series() {
    let conn = library();
    save(&conn, &input(None, 2, &[1, 2, 4])).unwrap();
    for _ in 0..20 {
        let (a, b) = db::get_arena_pair(&conn).unwrap().unwrap();
        assert!(matches!((a.id, b.id), (2, 3) | (3, 2)));
        let (a, b) = db::get_arena_opponent(&conn, 1).unwrap().unwrap();
        assert_eq!((a.id, b.id), (2, 3));
    }
    db::move_to_trash(&conn, 3).unwrap();
    assert!(db::get_arena_pair(&conn).unwrap().is_none());
    assert!(db::get_arena_opponent(&conn, 1).unwrap().is_none());
}
#[test]
fn trash_restore_and_deleted_representative_use_next_active_volume_without_losing_series() {
    let conn = library();
    save(&conn, &input(None, 2, &[1, 2, 4])).unwrap();
    db::move_to_trash(&conn, 2).unwrap();
    assert_eq!(ids(&conn, "date", "desc"), [1, 4, 3]);
    assert!(db::get_arena_opponent(&conn, 3).unwrap().is_none());
    assert!(db::get_arena_opponent(&conn, 1).unwrap().is_none());
    assert_eq!(
        db::get_trash_files(&conn).unwrap()[0].series_position,
        Some(2)
    );
    db::restore_from_trash(&conn, 2).unwrap();
    assert_eq!(ids(&conn, "date", "desc"), [3, 1, 2, 4]);
    conn.execute("DELETE FROM media_files WHERE id=2", [])
        .unwrap();
    assert_eq!(list(&conn).unwrap()[0].representative_id, None);
    assert_eq!(list(&conn).unwrap()[0].file_ids, [1, 4]);
    assert_eq!(ids(&conn, "date", "desc"), [1, 4, 3]);
    let violations: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}
#[test]
fn invalid_series_updates_leave_name_members_and_shared_values_unchanged() {
    let conn = library();
    let sid = save(&conn, &input(None, 1, &[1, 2])).unwrap();
    let mut other = input(None, 4, &[4]);
    other.name = "other".into();
    save(&conn, &other).unwrap();
    for (rep, files) in [
        (1, vec![]),
        (1, vec![1, 1]),
        (4, vec![1, 2]),
        (1, vec![1, 99]),
        (1, vec![1, 4]),
    ] {
        let mut request = input(Some(sid), rep, &files);
        request.name = "renamed".into();
        assert!(save(&conn, &request).is_err());
    }
    let mut duplicate = input(Some(sid), 1, &[1]);
    duplicate.name = "other".into();
    assert!(matches!(save(&conn, &duplicate), Err(Error::Conflict(_))));
    let series = list(&conn)
        .unwrap()
        .into_iter()
        .find(|s| s.id == sid)
        .unwrap();
    assert_eq!(
        (
            series.name.as_str(),
            series.file_ids,
            series.rating,
            series.score
        ),
        ("series", vec![1, 2], 5, 1500)
    );
    save(&conn, &input(Some(sid), 2, &[2])).unwrap();
    assert!(db::get_media_files(&conn, None, None)
        .unwrap()
        .iter()
        .find(|f| f.id == 1)
        .unwrap()
        .series_id
        .is_none());
}
#[test]
fn reset_scores_and_ratings_include_shared_and_underlying_file_evaluations() {
    let conn = library();
    save(&conn, &input(None, 1, &[1, 2])).unwrap();
    db::reset_all_scores(&conn).unwrap();
    db::reset_all_ratings(&conn).unwrap();
    assert!(db::get_media_files(&conn, None, None)
        .unwrap()
        .iter()
        .all(|f| f.score == 1000 && f.rating == 0));
    dissolve(&conn, 1).unwrap();
    assert!(db::get_media_files(&conn, None, None)
        .unwrap()
        .iter()
        .all(|f| f.score == 1000 && f.rating == 0));
}

#[test]
fn arena_rejects_non_representative_votes_and_tracks_representative_changes() {
    let conn = library();
    let sid = save(&conn, &input(None, 2, &[1, 2, 4])).unwrap();
    assert!(db::vote_arena(&conn, 1, 3).is_err());
    assert_eq!(list(&conn).unwrap()[0].score, 1100);
    save(&conn, &input(Some(sid), 4, &[1, 2, 4])).unwrap();
    assert_eq!(db::get_arena_opponent(&conn, 2).unwrap().unwrap().0.id, 4);
    assert!(db::vote_arena(&conn, 2, 3).is_err());
    assert!(db::vote_arena(&conn, 3, 2).is_err());
    let vote = db::vote_arena(&conn, 4, 3).unwrap();
    assert!(vote.0 > 1100);
    db::move_to_trash(&conn, 4).unwrap();
    assert!(db::get_arena_pair(&conn).unwrap().is_none());
    db::restore_from_trash(&conn, 4).unwrap();
    assert_eq!(db::get_arena_opponent(&conn, 1).unwrap().unwrap().0.id, 4);
    conn.execute("DELETE FROM media_files WHERE id=4", [])
        .unwrap();
    assert!(db::get_arena_pair(&conn).unwrap().is_none());
    assert!(db::get_arena_opponent(&conn, 1).unwrap().is_none());
}

fn tag_names(conn: &Connection, id: i64) -> Vec<String> {
    db::get_tags_for_file(conn, id)
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect()
}

#[test]
fn series_tags_merge_share_and_keep_standalone_series_and_trash_isolated() {
    let conn = library();
    let a = db::add_tag_to_file(&conn, 1, "alpha").unwrap();
    db::add_tag_to_file(&conn, 2, "beta").unwrap();
    db::add_tag_to_file(&conn, 2, "alpha").unwrap();
    db::add_tag_to_file(&conn, 3, "solo").unwrap();
    db::add_tag_to_file(&conn, 4, "other").unwrap();
    save(&conn, &input(None, 2, &[1, 2])).unwrap();
    let mut other = input(None, 4, &[4]);
    other.name = "Other".into();
    save(&conn, &other).unwrap();
    for id in [1, 2] {
        assert_eq!(tag_names(&conn, id), ["alpha", "beta"]);
    }
    db::add_tag_to_file(&conn, 1, "#shared").unwrap();
    db::add_tag_to_file(&conn, 2, "shared").unwrap();
    db::remove_tag_from_file(&conn, 2, a.id).unwrap();
    assert_eq!(tag_names(&conn, 1), ["beta", "shared"]);
    assert_eq!(tag_names(&conn, 3), ["solo"]);
    assert_eq!(tag_names(&conn, 4), ["other"]);
    for file in db::get_media_files(&conn, None, None)
        .unwrap()
        .iter()
        .filter(|f| f.series_id == Some(1))
    {
        assert_eq!(
            file.tags
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["beta", "shared"]
        );
    }
    let (representative, _) = db::get_arena_opponent(&conn, 1).unwrap().unwrap();
    assert_eq!(
        representative
            .tags
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["beta", "shared"]
    );
    db::move_to_trash(&conn, 1).unwrap();
    db::add_tag_to_file(&conn, 2, "trash-shared").unwrap();
    assert_eq!(db::get_trash_files(&conn).unwrap()[0].tags.len(), 3);
    db::restore_from_trash(&conn, 1).unwrap();
    assert_eq!(tag_names(&conn, 1), ["beta", "shared", "trash-shared"]);
}

#[test]
fn series_tag_migration_runs_once_and_member_edits_do_not_resurrect_deleted_tags() {
    let conn = library();
    let a = db::add_tag_to_file(&conn, 1, "alpha").unwrap();
    db::add_tag_to_file(&conn, 2, "beta").unwrap();
    db::add_tag_to_file(&conn, 4, "new-member").unwrap();
    let sid = save(&conn, &input(None, 2, &[1, 2])).unwrap();
    // Model an existing pre-sharing database, including a member in Trash.
    conn.execute("DROP TABLE series_tags", []).unwrap();
    db::move_to_trash(&conn, 1).unwrap();
    db::init_db(&conn).unwrap();
    assert_eq!(tag_names(&conn, 2), ["alpha", "beta"]);
    db::remove_tag_from_file(&conn, 2, a.id).unwrap();
    db::init_db(&conn).unwrap();
    save(&conn, &input(Some(sid), 1, &[2, 1])).unwrap();
    assert_eq!(tag_names(&conn, 1), ["beta"]);
    save(&conn, &input(Some(sid), 2, &[2, 1, 4])).unwrap();
    assert_eq!(tag_names(&conn, 4), ["beta", "new-member"]);
    db::add_tag_to_file(&conn, 4, "shared").unwrap();
    save(&conn, &input(Some(sid), 2, &[2, 4])).unwrap();
    assert_eq!(tag_names(&conn, 1), ["alpha"]);
    assert_eq!(tag_names(&conn, 2), ["beta", "new-member", "shared"]);
    dissolve(&conn, sid).unwrap();
    assert_eq!(tag_names(&conn, 2), ["beta"]);
    assert_eq!(tag_names(&conn, 4), ["new-member"]);
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM series_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn shared_tags_survive_member_deletion_and_tag_mutations_are_atomic() {
    let conn = library();
    save(&conn, &input(None, 2, &[1, 2])).unwrap();
    db::add_tag_to_file(&conn, 1, "keep").unwrap();
    conn.execute("DELETE FROM media_files WHERE id=1", [])
        .unwrap();
    assert_eq!(tag_names(&conn, 2), ["keep"]);
    conn.execute_batch(
        "CREATE TRIGGER fail_shared_tag BEFORE INSERT ON series_tags
        BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    )
    .unwrap();
    assert!(db::add_tag_to_file(&conn, 2, "failed").is_err());
    let count: i64 = conn
        .query_row("SELECT count(*) FROM tags WHERE name='failed'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(tag_names(&conn, 2), ["keep"]);
}
