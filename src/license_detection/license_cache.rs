use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rancor::{Panic, ResultExt};

use crate::license_detection::index::CachedLicenseIndex;

const SCHEMA_VERSION: u32 = 1;

pub struct LicenseCacheConfig {
    cache_dir: PathBuf,
}

impl LicenseCacheConfig {
    pub fn from_default_dir() -> Self {
        let cache_dir = directories::ProjectDirs::from("", "", "provenant")
            .map(|dirs| dirs.cache_dir().join("license_index"))
            .unwrap_or_else(|| PathBuf::from("/tmp/provenant/license_index"));
        Self { cache_dir }
    }

    fn cache_file_path(&self) -> PathBuf {
        self.cache_dir.join("index_cache.rkyv")
    }

    fn version_file_path(&self) -> PathBuf {
        self.cache_dir.join("version")
    }
}

pub fn load_cached_index(config: &LicenseCacheConfig) -> Result<Option<CachedLicenseIndex>> {
    let version_path = config.version_file_path();
    let cache_path = config.cache_file_path();

    if !cache_path.exists() || !version_path.exists() {
        return Ok(None);
    }

    let stored_version: u32 = fs::read_to_string(&version_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    if stored_version != SCHEMA_VERSION {
        return Ok(None);
    }

    let bytes = fs::read(&cache_path).context("Failed to read license index cache file")?;

    let archived =
        match rkyv::access::<rkyv::Archived<CachedLicenseIndex>, rkyv::rancor::Error>(&bytes) {
            Ok(archived) => archived,
            Err(_) => return Ok(None),
        };

    let cached: CachedLicenseIndex =
        rkyv::deserialize::<CachedLicenseIndex, Panic>(archived).always_ok();

    Ok(Some(cached))
}

pub fn save_cached_index(config: &LicenseCacheConfig, cached: &CachedLicenseIndex) -> Result<()> {
    fs::create_dir_all(&config.cache_dir)
        .context("Failed to create license index cache directory")?;

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(cached)
        .map_err(|e| anyhow::anyhow!("Failed to serialize license index cache: {}", e))?;

    let cache_path = config.cache_file_path();
    let mut file =
        fs::File::create(&cache_path).context("Failed to create license index cache file")?;
    file.write_all(&bytes)
        .context("Failed to write license index cache file")?;

    let version_path = config.version_file_path();
    fs::write(&version_path, SCHEMA_VERSION.to_string())
        .context("Failed to write cache version file")?;

    Ok(())
}

pub fn cache_file_size(config: &LicenseCacheConfig) -> Option<u64> {
    fs::metadata(config.cache_file_path()).ok().map(|m| m.len())
}
