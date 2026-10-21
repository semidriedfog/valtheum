//! A persistent lock file; ownership belongs to the OS handle, never its contents.
//! Do not unlink it on release: unlinking lets clients lock different file objects.
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const UNAVAILABLE: &str = "다른 앱이나 PC에서 이 라이브러리를 사용 중이거나 저장소 잠금을 사용할 수 없습니다. 다른 앱에서 라이브러리를 닫고 다시 시도해 주세요.";

pub struct Lease {
    pub root: PathBuf,
    _file: File,
}

impl Lease {
    pub fn acquire(root: &Path) -> anyhow::Result<Arc<Self>> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        // Keep the lock file from being renamed/deleted while held (including on SMB).
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0x00000001 | 0x00000002);
        }
        let file = options
            .open(root.join(".valtheum.lock"))
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
        file.try_lock().map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
        Ok(Arc::new(Self {
            root: root.to_owned(),
            _file: file,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, Write},
        process::{Command, Stdio},
    };

    #[test]
    #[ignore = "subprocess helper; run by the ownership test"]
    fn lock_holder_process() {
        let root = PathBuf::from(std::env::var_os("VALTHEUM_LOCK_TEST_ROOT").expect("test root"));
        let _lease = Lease::acquire(&root).unwrap();
        println!("LOCK_READY");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    }

    #[test]
    fn process_ownership_blocks_open_before_db_creation_and_releases_after_crash() {
        let root = std::env::temp_dir().join(format!(
            "valtheum-lock-test-{}-{}",
            std::process::id(),
            crate::archive_crypto::hex(&crate::archive_crypto::random::<8>().unwrap())
        ));
        std::fs::create_dir(&root).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "library_lock::tests::lock_holder_process",
                "--ignored",
                "--nocapture",
            ])
            .env("VALTHEUM_LOCK_TEST_ROOT", &root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            assert_ne!(
                output.read_line(&mut line).unwrap(),
                0,
                "lock helper exited early"
            );
            if line.contains("LOCK_READY") {
                break;
            }
        }
        let blocked = crate::state::Library::open(root.clone())
            .err()
            .unwrap()
            .to_string();
        let db_created = root.join(crate::encryption::PLAIN_DB).exists();
        // Abrupt process exit must release ownership without deleting a stale file.
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(blocked, UNAVAILABLE);
        assert!(
            !db_created,
            "DB initialization must happen only after ownership"
        );
        assert!(root.join(".valtheum.lock").exists());
        let library = crate::state::Library::open(root.clone()).unwrap();
        assert!(crate::state::Library::open(root.clone()).is_err());
        drop(library);
        drop(crate::state::Library::open(root.clone()).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}
