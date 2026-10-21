use super::*;

pub fn restoration_name(library: &Library, id: i64) -> Result<String> {
    let guard = library.conn.lock().unwrap();
    Ok(guard.as_ref().context("DB 잠김")?.query_row(
        "SELECT COALESCE(a.restore_name,m.rel_path) FROM archive_store a JOIN media_files m ON m.id=a.file_id WHERE m.id=?1 AND a.state='ready'",[id],|r|r.get(0))?)
}

// Only unreferenced, generated ciphertext files are eligible for collection.
// The DB records the candidate before work begins; mapping + retirement commit together.
pub fn collect_unused(root: &FsPath, conn: &Connection) -> Result<()> {
    let paths=conn.prepare("SELECT g.encrypted_path FROM archive_gc g WHERE NOT EXISTS(SELECT 1 FROM archive_store a WHERE a.encrypted_path=g.encrypted_path)")?
        .query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for rel in paths {
        let leaf = rel
            .strip_prefix(".valtheum/archives/")
            .and_then(|s| s.strip_suffix(".vta"))
            .or_else(|| {
                rel.strip_prefix(".valtheum/images/")
                    .and_then(|s| s.strip_suffix(".vti"))
            })
            .or_else(|| {
                rel.strip_prefix(".valtheum/notes/")
                    .and_then(|s| s.strip_suffix(".vtn"))
            })
            .context("잘못된 임시 파일 경로")?;
        ensure!(
            leaf.len() == 32 && leaf.bytes().all(|b| b.is_ascii_hexdigit()),
            "잘못된 임시 파일 이름"
        );
        let path = crypto::safe_child(root, &rel)?;
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => continue, // A reader may still be using it; retry at next unlock.
        }
        conn.execute("DELETE FROM archive_gc WHERE encrypted_path=?1", [rel])?;
    }
    Ok(())
}

pub(super) fn rewrite(library: &Library, id: i64, name: &str) -> Result<()> {
    let (original, old_rel, key) = ready(library, id)?;
    let expected = restoration_name(library, id)?;
    let next = {
        let guard = library.conn.lock().unwrap();
        crate::media_name::validated_name(guard.as_ref().context("DB 잠김")?, id, name)?
    };
    if next == expected {
        let guard = library.conn.lock().unwrap();
        crate::media_name::set_name(guard.as_ref().context("DB 잠김")?, id, name)?;
        return Ok(());
    }
    let old_path = crypto::safe_child(&library.root_dir, &old_rel)?;
    let source_guard = crypto::open_source(&old_path)?;
    let reader = crypto::Reader::open(&old_path, &key)?;
    ensure!(
        reader.metadata.name == expected,
        "암호화 파일의 복원 이름이 DB와 다릅니다."
    );
    let expected_sha = reader.metadata.sha256.clone();
    drop(reader);
    let new_rel = format!(
        ".valtheum/archives/{}.vta",
        crypto::hex(&crypto::random::<16>()?)
    );
    let target = crypto::safe_child(&library.root_dir, &new_rel)?;
    ensure!(
        !target.try_exists()?,
        "새 암호화 파일 경로가 이미 있습니다."
    );
    {
        let guard = library.conn.lock().unwrap();
        guard.as_ref().context("DB 잠김")?.execute(
            "INSERT INTO archive_gc(encrypted_path) VALUES(?1)",
            [&new_rel],
        )?;
    }
    let result = (|| -> Result<()> {
        let meta = crypto::rewrite_name(&old_path, &target, &next, &key, |phase, done, total| {
            library.archives.progress(phase, done, total)
        })?;
        ensure!(
            meta.name == next && meta.sha256 == expected_sha,
            "변경 사본 검증 실패"
        );
        library.archives.progress("복원 파일명 반영", 0, 0)?;
        let guard = library.conn.lock().unwrap();
        let conn = guard.as_ref().context("DB 잠김")?;
        let tx = conn.unchecked_transaction()?;
        ensure!(
            crate::media_name::validated_name(&tx, id, name)? == next,
            "파일 이름이 변경되었습니다."
        );
        let changed=tx.execute("UPDATE archive_store SET encrypted_path=?1,restore_name=?2 WHERE file_id=?3 AND encrypted_path=?4 AND state='ready'",rusqlite::params![new_rel,next,id,old_rel])?;
        ensure!(changed == 1, "암호화 파일 연결이 변경되었습니다.");
        tx.execute("UPDATE media_files SET display_name=CASE WHEN rel_path=?1 THEN NULL ELSE ?1 END WHERE id=?2 AND rel_path=?3",rusqlite::params![next,id,original])?;
        tx.execute(
            "INSERT OR IGNORE INTO archive_gc(encrypted_path) VALUES(?1)",
            [&old_rel],
        )?;
        tx.execute("DELETE FROM archive_gc WHERE encrypted_path=?1", [&new_rel])?;
        tx.commit()?;
        Ok(())
    })();
    drop(source_guard);
    library.archives.clear_cache();
    if let Some(conn) = library.conn.lock().unwrap().as_ref() {
        let _ = collect_unused(&library.root_dir, conn);
    }
    result
}

pub async fn start(
    app: AppState,
    state: RequestState,
    id: i64,
    name: String,
    restore_name: bool,
    headers: HeaderMap,
) -> ApiResult {
    let request = request_identity(&headers, ("rename", id, &name, restore_name))?;
    start_request(
        &app,
        &state,
        Job {
            phase: "복원 이름 변경 준비".into(),
            ..Default::default()
        },
        request,
        move |state| {
            if restore_name {
                return rewrite(state, id, &name);
            }
            state.archives.progress("복원 파일명 반영", 0, 0)?;
            let guard = state.conn.lock().unwrap();
            state.archives.progress("복원 파일명 반영", 0, 0)?;
            crate::media_name::set_name(guard.as_ref().context("DB 잠김")?, id, &name)?;
            Ok(())
        },
    )
    .await
}
