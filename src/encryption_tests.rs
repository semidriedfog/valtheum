use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            crate::persistence::temporary_sibling(&std::env::temp_dir().join("cipher-fixture"))
                .unwrap();
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn source(&self) -> Connection {
        let conn = open(&self.0.join(PLAIN_DB), None, true).unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute_batch("PRAGMA user_version=27;
            INSERT INTO media_files(id,rel_path,file_type,file_size,modified_at,rating,score,watch_count) VALUES(1,'secret_1.zip','ARCHIVE',42,100,5,1600,7),(2,'secret_2.zip','ARCHIVE',43,101,5,1600,3);
            INSERT INTO media_series(id,name,representative_id,rating,score) VALUES(1,'비밀 시리즈',1,5,1600);
            INSERT INTO series_members(file_id,series_id,position) VALUES(1,1,1),(2,1,2);
            INSERT INTO tags(id,name) VALUES(1,'private-tag');
            INSERT INTO file_tags(file_id,tag_id) VALUES(1,1);
            INSERT INTO series_tags(series_id,tag_id) VALUES(1,1);
            INSERT INTO thumbnails(file_id,data,width,height,revision,page_offset) VALUES(1,X'0102030405',16,16,2,4);
            INSERT INTO media_bookmarks(file_id,position,thumbnail_data) VALUES(1,4,X'11223344');
            INSERT INTO deleted_files_log(original_rel_path,file_type,file_size,score,tags_info,deleted_at) VALUES('removed.zip','ARCHIVE',2,900,'private-tag',100);
            INSERT INTO library_info(key,value) VALUES('fixture','keep-me');").unwrap();
        conn
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".vault-write-"));
        let _ = fs::remove_dir_all(&self.0);
    }
}
const PASSWORD: &str = "테스트 암호 ' with spaces 123";

#[test]
fn conversion_preserves_all_metadata_wal_and_encrypted_backup() -> anyhow::Result<()> {
    let f = Fixture::new();
    let source = f.source();
    convert(&source, &f.0, PASSWORD)?;
    let secure = f.0.join(SECURE_DB);
    assert!(
        f.0.join(PLAIN_DB).exists(),
        "original is retained until explicit cleanup"
    );
    assert!(!fs::read(&secure)?.starts_with(b"SQLite format 3"));
    assert!(open(&secure, None, false).is_err());
    assert!(open(&secure, Some("wrong password"), false).is_err());
    let conn = open(&secure, Some(PASSWORD), false)?;
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        27
    );
    assert_eq!(
        conn.query_row("SELECT watch_count FROM media_files WHERE id=1", [], |r| {
            r.get::<_, i64>(0)
        })?,
        7
    );
    assert_eq!(
        conn.query_row("SELECT name FROM media_series", [], |r| r
            .get::<_, String>(0))?,
        "비밀 시리즈"
    );
    assert_eq!(
        conn.query_row("SELECT thumbnail_data FROM media_bookmarks", [], |r| r
            .get::<_, Vec<u8>>(0))?,
        [0x11, 0x22, 0x33, 0x44]
    );
    crate::db::init_db(&conn)?;
    conn.execute("UPDATE media_files SET watch_count=8 WHERE id=1", [])?;
    let backup = f.0.join("backup.db");
    crate::persistence::backup_database_with_key(&conn, &secure, &backup, Some(PASSWORD))?;
    assert!(open(&backup, None, false).is_err());
    let restored = open(&backup, Some(PASSWORD), false)?;
    verify(&restored, true)?;
    assert_eq!(
        restored.query_row("SELECT watch_count FROM media_files WHERE id=1", [], |r| {
            r.get::<_, i64>(0)
        })?,
        8
    );
    let before = fs::read(&backup)?;
    conn.execute_batch("BEGIN")?;
    assert!(
        crate::persistence::backup_database_with_key(&conn, &secure, &backup, Some(PASSWORD))
            .is_err()
    );
    assert_eq!(fs::read(&backup)?, before);
    conn.execute_batch("ROLLBACK")?;
    assert!(
        crate::persistence::backup_database_with_key(&conn, &secure, &secure, Some(PASSWORD))
            .is_err()
    );
    drop(source);
    remove_plaintext(&f.0)?;
    assert!(!f.0.join(PLAIN_DB).exists());
    assert_eq!(
        conn.query_row("SELECT count(*) FROM media_files", [], |r| r
            .get::<_, i64>(0))?,
        2
    );
    drop(restored);
    drop(conn);
    Ok(())
}

#[test]
fn failures_do_not_replace_existing_db_or_modify_plain_source() -> anyhow::Result<()> {
    let f = Fixture::new();
    let source = f.source();
    assert!(convert(&source, &f.0, "short").is_err());
    assert!(!f.0.join(SECURE_DB).exists());
    fs::write(f.0.join(SECURE_DB), b"existing encrypted database")?;
    assert!(convert(&source, &f.0, PASSWORD).is_err());
    assert_eq!(
        fs::read(f.0.join(SECURE_DB))?,
        b"existing encrypted database"
    );
    assert_eq!(
        source.query_row("SELECT count(*) FROM media_files", [], |r| r
            .get::<_, i64>(0))?,
        2
    );
    drop(source);
    let library = crate::state::Library::open(f.0.clone())?;
    assert!(
        library.security_status().locked,
        "never fall back to plaintext when encrypted DB exists, even if corrupt"
    );
    assert!(library.conn.lock().unwrap().is_none());
    Ok(())
}

#[test]
fn encrypted_reopen_starts_locked_and_tampering_is_rejected() -> anyhow::Result<()> {
    let f = Fixture::new();
    let source = f.source();
    convert(&source, &f.0, PASSWORD)?;
    drop(source);
    let library = crate::state::Library::open(f.0.clone())?;
    assert!(library.security_status().locked);
    assert!(library.conn.lock().unwrap().is_none());
    let path = f.0.join(SECURE_DB);
    let mut bytes = fs::read(&path)?;
    bytes[100] ^= 0x80;
    fs::write(&path, bytes)?;
    assert!(open(&path, Some(PASSWORD), false).is_err());
    Ok(())
}
