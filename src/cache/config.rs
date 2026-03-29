use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

pub const DEFAULT_CACHE_DIR_NAME: &str = ".provenant-cache";
pub const CACHE_DIR_ENV_VAR: &str = "PROVENANT_CACHE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CacheKind {
    #[value(alias = "scan")]
    ScanResults,
    #[value(alias = "license", alias = "warm")]
    LicenseIndex,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheKinds {
    scan_results: bool,
    license_index: bool,
}

impl CacheKinds {
    pub fn from_cli(kinds: &[CacheKind]) -> Self {
        let mut selected = Self::default();

        for kind in kinds {
            match kind {
                CacheKind::ScanResults => selected.scan_results = true,
                CacheKind::LicenseIndex => selected.license_index = true,
                CacheKind::All => {
                    selected.scan_results = true;
                    selected.license_index = true;
                }
            }
        }

        selected
    }

    pub const fn scan_results(self) -> bool {
        self.scan_results
    }

    pub const fn license_index(self) -> bool {
        self.license_index
    }

    pub const fn any_enabled(self) -> bool {
        self.scan_results || self.license_index
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheConfig {
    root_dir: PathBuf,
    kinds: CacheKinds,
}

impl CacheConfig {
    #[cfg(test)]
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            kinds: CacheKinds::default(),
        }
    }

    pub fn with_kinds(root_dir: PathBuf, kinds: CacheKinds) -> Self {
        Self { root_dir, kinds }
    }

    #[cfg(test)]
    pub fn from_scan_root(scan_root: &Path) -> Self {
        Self::new(scan_root.join(DEFAULT_CACHE_DIR_NAME))
    }

    pub fn from_scan_root_with_kinds(scan_root: &Path, kinds: CacheKinds) -> Self {
        Self::with_kinds(scan_root.join(DEFAULT_CACHE_DIR_NAME), kinds)
    }

    pub fn resolve_root_dir(
        scan_root: &Path,
        cli_cache_dir: Option<&Path>,
        env_cache_dir: Option<&Path>,
    ) -> PathBuf {
        if let Some(path) = cli_cache_dir {
            return path.to_path_buf();
        }

        if let Some(path) = env_cache_dir {
            return path.to_path_buf();
        }

        scan_root.join(DEFAULT_CACHE_DIR_NAME)
    }

    pub fn from_overrides(
        scan_root: &Path,
        cli_cache_dir: Option<&Path>,
        env_cache_dir: Option<&Path>,
        kinds: CacheKinds,
    ) -> Self {
        if cli_cache_dir.is_none() && env_cache_dir.is_none() {
            return Self::from_scan_root_with_kinds(scan_root, kinds);
        }

        Self::with_kinds(
            Self::resolve_root_dir(scan_root, cli_cache_dir, env_cache_dir),
            kinds,
        )
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn license_index_dir(&self) -> PathBuf {
        self.root_dir.join("license-index")
    }

    pub fn license_index_snapshot_path(&self) -> PathBuf {
        self.license_index_dir().join("snapshot.bin.zst")
    }

    pub fn scan_results_dir(&self) -> PathBuf {
        self.root_dir.join("scan-results")
    }

    pub const fn scan_results_enabled(&self) -> bool {
        self.kinds.scan_results()
    }

    pub const fn license_index_enabled(&self) -> bool {
        self.kinds.license_index()
    }

    pub const fn any_enabled(&self) -> bool {
        self.kinds.any_enabled()
    }

    pub fn ensure_dirs(&self) -> io::Result<()> {
        if self.license_index_enabled() {
            fs::create_dir_all(self.license_index_dir())?;
        }
        if self.scan_results_enabled() {
            fs::create_dir_all(self.scan_results_dir())?;
        }
        Ok(())
    }

    pub fn clear(&self) -> io::Result<()> {
        if self.root_dir().exists() {
            fs::remove_dir_all(&self.root_dir)?;
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
        let config = CacheConfig::from_scan_root_with_kinds(
            temp_dir.path(),
            CacheKinds {
                scan_results: true,
                license_index: true,
            },
        );

        config
            .ensure_dirs()
            .expect("Failed to create cache directories");

        assert!(config.root_dir().exists());
        assert!(config.license_index_dir().exists());
        assert!(config.scan_results_dir().exists());
    }

    #[test]
    fn test_ensure_dirs_only_creates_enabled_cache_subdirectories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = CacheConfig::from_scan_root_with_kinds(
            temp_dir.path(),
            CacheKinds {
                scan_results: true,
                license_index: false,
            },
        );

        config
            .ensure_dirs()
            .expect("Failed to create selected cache directories");

        assert!(config.scan_results_dir().exists());
        assert!(!config.license_index_dir().exists());
    }

    #[test]
    fn test_resolve_root_dir_prefers_cli_then_env_then_default() {
        let scan_root = Path::new("/scan-root");
        let cli_dir = Path::new("/cli-cache");
        let env_dir = Path::new("/env-cache");

        assert_eq!(
            CacheConfig::resolve_root_dir(scan_root, Some(cli_dir), Some(env_dir)),
            cli_dir
        );
        assert_eq!(
            CacheConfig::resolve_root_dir(scan_root, None, Some(env_dir)),
            env_dir
        );
        assert_eq!(
            CacheConfig::resolve_root_dir(scan_root, None, None),
            PathBuf::from(format!("/scan-root/{DEFAULT_CACHE_DIR_NAME}"))
        );
    }

    #[test]
    fn test_cache_kinds_from_cli_supports_all_and_specific_kinds() {
        let selected = CacheKinds::from_cli(&[CacheKind::ScanResults, CacheKind::LicenseIndex]);
        assert!(selected.scan_results());
        assert!(selected.license_index());

        let all = CacheKinds::from_cli(&[CacheKind::All]);
        assert!(all.scan_results());
        assert!(all.license_index());
    }

    #[test]
    fn test_clear_removes_cache_root_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = CacheConfig::with_kinds(
            temp_dir.path().join("cache-root"),
            CacheKinds {
                scan_results: true,
                license_index: true,
            },
        );

        config
            .ensure_dirs()
            .expect("Failed to create cache directories");
        assert!(config.root_dir().exists());

        config.clear().expect("Failed to clear cache directory");
        assert!(!config.root_dir().exists());
    }
}
