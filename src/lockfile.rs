use anyhow::Context;
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

/// RAII exclusive file lock for serializing read-modify-write operations on
/// the `.codeskel/` state files. Acquired lock is held for the lifetime of
/// the returned guard and released on drop.
///
/// Uses `flock(2)` advisory locking via fs2; safe across processes on the
/// same machine, ignored across NFS mounts.
pub struct LockGuard {
    file: File,
    #[allow(dead_code)]
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Acquire an exclusive lock on `<cache_dir>/.lock`, creating the directory
/// and lock file if needed. Blocks until the lock is available.
pub fn lock_cache_dir(cache_dir: &Path) -> anyhow::Result<LockGuard> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("creating {}", cache_dir.display()))?;
    let path = cache_dir.join(".lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("opening lock file {}", path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("acquiring lock on {}", path.display()))?;
    Ok(LockGuard { file, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn second_acquire_blocks_until_first_drops() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let first = lock_cache_dir(&path).expect("first lock");

        let (tx, rx) = mpsc::channel();
        let path_clone = path.clone();
        let handle = thread::spawn(move || {
            let _second = lock_cache_dir(&path_clone).expect("second lock");
            tx.send(()).ok();
        });

        // The contender should not be able to acquire while we hold the lock.
        assert!(
            rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "second lock acquired while first was still held"
        );

        drop(first);
        // Now it should acquire promptly.
        rx.recv_timeout(Duration::from_secs(2))
            .expect("second lock should acquire after first dropped");
        handle.join().unwrap();
    }
}
