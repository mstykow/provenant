use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use super::locking::scans_lock_path;

pub const DEFAULT_CACHE_DIR_NAME: &str = ".provenant-cache";
pub const CACHE_DIR_ENV_VAR: &str = "PROVENANT_CACHE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheConfig {
    root_dir: PathBuf,
    incremental: bool,
}

impl CacheConfig {
    #[cfg(test)]
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            incremental: false,
        }
    }

    pub fn with_options(root_dir: PathBuf, incremental: bool) -> Self {
        Self {
            root_dir,
            incremental,
        }
    }

    #[cfg(test)]
    pub fn from_scan_root(scan_root: &Path) -> Self {
        Self::new(scan_root.join(DEFAULT_CACHE_DIR_NAME))
    }

    pub(crate) fn project_cache_root() -> Option<PathBuf> {
        ProjectDirs::from("com", "Provenant", "provenant")
            .map(|dirs| dirs.cache_dir().to_path_buf())
    }

    pub fn default_root_dir(scan_root: &Path) -> PathBuf {
        Self::project_cache_root().unwrap_or_else(|| scan_root.join(DEFAULT_CACHE_DIR_NAME))
    }

    pub fn default_root_dir_without_scan_root() -> PathBuf {
        Self::project_cache_root().unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_DIR_NAME))
    }

    pub fn resolve_root_dir(
        scan_root: Option<&Path>,
        cli_cache_dir: Option<&Path>,
        env_cache_dir: Option<&Path>,
    ) -> PathBuf {
        if let Some(path) = cli_cache_dir {
            return path.to_path_buf();
        }

        if let Some(path) = env_cache_dir {
            return path.to_path_buf();
        }

        match scan_root {
            Some(scan_root) => Self::default_root_dir(scan_root),
            None => Self::default_root_dir_without_scan_root(),
        }
    }

    pub fn from_overrides(
        scan_root: Option<&Path>,
        cli_cache_dir: Option<&Path>,
        env_cache_dir: Option<&Path>,
        incremental: bool,
    ) -> Self {
        Self::with_options(
            Self::resolve_root_dir(scan_root, cli_cache_dir, env_cache_dir),
            incremental,
        )
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn incremental_dir(&self) -> PathBuf {
        self.root_dir.join("incremental")
    }

    pub const fn incremental_enabled(&self) -> bool {
        self.incremental
    }

    pub fn ensure_dirs(&self) -> io::Result<()> {
        if self.incremental_enabled() {
            fs::create_dir_all(self.incremental_dir())?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn clear(&self) -> io::Result<()> {
        if self.root_dir().exists() {
            fs::remove_dir_all(&self.root_dir)?;
        }
        Ok(())
    }

    pub fn clear_contents(&self) -> io::Result<()> {
        if !self.root_dir().exists() {
            return Ok(());
        }

        let lock_path = scans_lock_path(self.root_dir());
        for entry in fs::read_dir(self.root_dir())? {
            let entry = entry?;
            let path = entry.path();
            if path == lock_path {
                continue;
            }

            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_from_scan_root_uses_expected_directory_name() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = CacheConfig::from_scan_root(temp_dir.path());
        assert_eq!(
            config.root_dir(),
            temp_dir.path().join(DEFAULT_CACHE_DIR_NAME)
        );
    }

    #[test]
    fn test_ensure_dirs_creates_expected_tree() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = CacheConfig::with_options(temp_dir.path().join(DEFAULT_CACHE_DIR_NAME), true);

        config
            .ensure_dirs()
            .expect("Failed to create cache directories");

        assert!(config.root_dir().exists());
        assert!(config.incremental_dir().exists());
    }

    #[test]
    fn test_resolve_root_dir_prefers_cli_then_env_then_default() {
        let scan_root = Path::new("/scan-root");
        let cli_dir = Path::new("/cli-cache");
        let env_dir = Path::new("/env-cache");

        assert_eq!(
            CacheConfig::resolve_root_dir(Some(scan_root), Some(cli_dir), Some(env_dir)),
            cli_dir
        );
        assert_eq!(
            CacheConfig::resolve_root_dir(Some(scan_root), None, Some(env_dir)),
            env_dir
        );
        assert_eq!(
            CacheConfig::resolve_root_dir(Some(scan_root), None, None),
            CacheConfig::default_root_dir(scan_root)
        );
    }

    #[test]
    fn test_resolve_root_dir_without_scan_root_uses_project_or_relative_default() {
        assert_eq!(
            CacheConfig::resolve_root_dir(None, None, None),
            CacheConfig::default_root_dir_without_scan_root()
        );
    }

    #[test]
    fn test_clear_removes_cache_root_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = CacheConfig::with_options(temp_dir.path().join("cache-root"), true);

        config
            .ensure_dirs()
            .expect("Failed to create cache directories");
        assert!(config.root_dir().exists());

        config.clear().expect("Failed to clear cache directory");
        assert!(!config.root_dir().exists());
    }
}
