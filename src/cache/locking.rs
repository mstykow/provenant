use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use fd_lock::RwLock;

const CACHE_LOCK_FILE_NAME: &str = "scans.lock";

pub fn scans_lock_path(cache_root: &Path) -> PathBuf {
    cache_root.join(CACHE_LOCK_FILE_NAME)
}

pub fn with_exclusive_cache_lock<T, E, F>(cache_root: &Path, operation: F) -> Result<T, E>
where
    E: From<io::Error>,
    F: FnOnce() -> Result<T, E>,
{
    fs::create_dir_all(cache_root).map_err(E::from)?;

    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(scans_lock_path(cache_root))
        .map_err(E::from)?;
    let mut lock = RwLock::new(lock_file);
    let _guard = lock.write().map_err(E::from)?;
    operation()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_with_exclusive_cache_lock_creates_sidecar_lock_file() {
        let temp_dir = TempDir::new().expect("create temp dir");

        let result: io::Result<()> = with_exclusive_cache_lock(temp_dir.path(), || Ok(()));

        result.expect("lock should succeed");
        assert!(fs::metadata(scans_lock_path(temp_dir.path())).is_ok());
    }
}
