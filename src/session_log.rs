//! Retain bounded diagnostic evidence when a restart is needed to recover audio.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const MAX_PREVIOUS_BYTES: u64 = 1024 * 1024;

/// Start a fresh log only after preserving the last non-empty session's tail.
/// The caller must hold the single-instance guard. No extra diagnostic data is
/// collected, and retention is limited to one backup of at most 1 MiB.
pub(crate) fn open(dir: &Path) -> io::Result<File> {
    let current_path = dir.join("eyeflow.log");
    let mut current = match File::open(&current_path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return File::create(current_path);
        }
        Err(error) => return Err(error),
    };
    let metadata = current.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("current log is not a regular file"));
    }
    if metadata.len() == 0 {
        // An interrupted startup with no log entries must not erase evidence.
        drop(current);
        return File::create(current_path);
    }
    current.seek(SeekFrom::Start(
        metadata.len().saturating_sub(MAX_PREVIOUS_BYTES),
    ))?;

    let previous_path = dir.join("eyeflow.log.previous");
    let temporary_path = dir.join("eyeflow.log.previous.tmp");
    // A crash before replacement may leave our temporary file. Refuse links,
    // directories and other special files rather than following/removing them.
    match fs::symlink_metadata(&temporary_path) {
        Ok(metadata) if metadata.file_type().is_file() => fs::remove_file(&temporary_path)?,
        Ok(_) => return Err(io::Error::other("temporary log is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut temporary = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)?;
    let preserved = (|| {
        io::copy(&mut current.take(MAX_PREVIOUS_BYTES), &mut temporary)?;
        temporary.sync_all()?;
        // Close before replacement, including on Windows.
        drop(temporary);
        match fs::remove_file(&previous_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        // Do not rely on rename replacing an existing destination on any OS.
        fs::rename(&temporary_path, &previous_path)
    })();
    if let Err(error) = preserved {
        let _ = fs::remove_file(&temporary_path);
        // Crucially, leave the current log untouched when preservation fails.
        return Err(error);
    }
    File::create(current_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "eyeflow-session-log-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, contents: &[u8]) {
            fs::write(self.0.join(name), contents).unwrap();
        }

        fn read(&self, name: &str) -> Vec<u8> {
            fs::read(self.0.join(name)).unwrap()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn first_start_creates_writable_log_without_backup() {
        let dir = TestDir::new();
        let mut file = open(&dir.0).unwrap();
        file.write_all(b"first startup").unwrap();
        drop(file);
        assert_eq!(dir.read("eyeflow.log"), b"first startup");
        assert!(!dir.0.join("eyeflow.log.previous").exists());
    }

    #[test]
    fn restart_preserves_diagnostic_evidence_before_starting_empty_log() {
        let dir = TestDir::new();
        dir.write("eyeflow.log", b"audio device error before restart\n");
        drop(open(&dir.0).unwrap());
        assert_eq!(
            dir.read("eyeflow.log.previous"),
            b"audio device error before restart\n"
        );
        assert!(dir.read("eyeflow.log").is_empty());
        assert!(!dir.0.join("eyeflow.log.previous.tmp").exists());
    }

    #[test]
    fn repeated_restarts_keep_only_the_latest_nonempty_session() {
        let dir = TestDir::new();
        for session in 0..8 {
            dir.write("eyeflow.log", format!("session {session}").as_bytes());
            drop(open(&dir.0).unwrap());
            assert_eq!(
                dir.read("eyeflow.log.previous"),
                format!("session {session}").as_bytes()
            );
            assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
        }
    }

    #[test]
    fn large_log_retains_only_last_mebibyte_without_text_assumptions() {
        let dir = TestDir::new();
        let contents: Vec<u8> = (0..MAX_PREVIOUS_BYTES as usize + 257)
            .map(|index| (index % 251) as u8)
            .collect();
        dir.write("eyeflow.log", &contents);
        drop(open(&dir.0).unwrap());
        assert_eq!(dir.read("eyeflow.log.previous"), contents[257..]);
        assert!(dir.read("eyeflow.log").is_empty());
    }

    #[test]
    fn empty_log_does_not_replace_previous_evidence() {
        let dir = TestDir::new();
        dir.write("eyeflow.log", b"");
        dir.write("eyeflow.log.previous", b"prior audio failure");
        drop(open(&dir.0).unwrap());
        assert_eq!(dir.read("eyeflow.log.previous"), b"prior audio failure");
    }

    #[test]
    fn backup_destination_failure_does_not_truncate_current_log() {
        let dir = TestDir::new();
        dir.write("eyeflow.log", b"must survive failed backup");
        fs::create_dir(dir.0.join("eyeflow.log.previous")).unwrap();
        dir.write("eyeflow.log.previous/marker", b"do not remove");
        assert!(open(&dir.0).is_err());
        assert_eq!(dir.read("eyeflow.log"), b"must survive failed backup");
        assert_eq!(dir.read("eyeflow.log.previous/marker"), b"do not remove");
        assert!(!dir.0.join("eyeflow.log.previous.tmp").exists());
    }

    #[test]
    fn temporary_directory_is_not_removed_and_current_log_survives() {
        let dir = TestDir::new();
        dir.write("eyeflow.log", b"current audio failure");
        dir.write("eyeflow.log.previous", b"older audio failure");
        fs::create_dir(dir.0.join("eyeflow.log.previous.tmp")).unwrap();
        assert!(open(&dir.0).is_err());
        assert_eq!(dir.read("eyeflow.log"), b"current audio failure");
        assert_eq!(dir.read("eyeflow.log.previous"), b"older audio failure");
        assert!(dir.0.join("eyeflow.log.previous.tmp").is_dir());
    }

    #[test]
    fn stale_temporary_regular_file_is_replaced_safely() {
        let dir = TestDir::new();
        dir.write("eyeflow.log", b"current audio failure");
        dir.write("eyeflow.log.previous.tmp", b"incomplete earlier copy");
        drop(open(&dir.0).unwrap());
        assert_eq!(dir.read("eyeflow.log.previous"), b"current audio failure");
        assert!(!dir.0.join("eyeflow.log.previous.tmp").exists());
    }

    #[test]
    fn non_file_current_log_does_not_replace_backup() {
        let dir = TestDir::new();
        fs::create_dir(dir.0.join("eyeflow.log")).unwrap();
        dir.write("eyeflow.log.previous", b"older audio failure");
        assert!(open(&dir.0).is_err());
        assert_eq!(dir.read("eyeflow.log.previous"), b"older audio failure");
    }

    #[cfg(unix)]
    #[test]
    fn temporary_symlink_is_not_followed_or_removed() {
        let dir = TestDir::new();
        dir.write("eyeflow.log", b"current audio failure");
        dir.write("unrelated", b"unrelated data");
        std::os::unix::fs::symlink(
            dir.0.join("unrelated"),
            dir.0.join("eyeflow.log.previous.tmp"),
        )
        .unwrap();
        assert!(open(&dir.0).is_err());
        assert_eq!(dir.read("eyeflow.log"), b"current audio failure");
        assert_eq!(dir.read("unrelated"), b"unrelated data");
        assert!(dir.0.join("eyeflow.log.previous.tmp").is_symlink());
    }
}
