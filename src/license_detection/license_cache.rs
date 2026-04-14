use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rancor::{Panic, ResultExt};
use sha2::{Digest, Sha256};

use crate::license_detection::index::CachedLicenseIndex;
use crate::license_detection::models::{LoadedLicense, LoadedRule};

const CACHE_FILENAME: &str = "license_cache.rkyv";

pub struct LicenseCacheConfig {
    pub cache_dir: PathBuf,
    pub reindex: bool,
}

impl LicenseCacheConfig {
    pub fn new(cache_dir: PathBuf, reindex: bool) -> Self {
        Self { cache_dir, reindex }
    }

    pub fn default_cache_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn cache_file_path(&self) -> PathBuf {
        self.cache_dir.join(CACHE_FILENAME)
    }
}

pub fn compute_rules_fingerprint(rules: &[LoadedRule], licenses: &[LoadedLicense]) -> [u8; 32] {
    let mut hasher = Sha256::new();

    let mut sorted_rules: Vec<_> = rules.iter().collect();
    sorted_rules.sort_by_key(|r| &r.identifier);
    for rule in &sorted_rules {
        hasher.update(rule.identifier.as_bytes());
        hasher.update(rule.license_expression.as_bytes());
        hasher.update(rule.text.as_bytes());
    }

    let mut sorted_licenses: Vec<_> = licenses.iter().collect();
    sorted_licenses.sort_by_key(|l| &l.key);
    for license in &sorted_licenses {
        hasher.update(license.key.as_bytes());
        hasher.update(license.text.as_bytes());
    }

    hasher.finalize().into()
}

pub fn compute_artifact_fingerprint(artifact_bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(artifact_bytes).into()
}

pub fn load_cached_index(
    config: &LicenseCacheConfig,
    fingerprint: &[u8; 32],
) -> Result<Option<CachedLicenseIndex>> {
    let cache_path = config.cache_file_path();

    if !cache_path.exists() {
        return Ok(None);
    }

    let bytes = match fs::read(&cache_path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };

    if bytes.len() < 32 {
        return Ok(None);
    }

    let stored_fingerprint: [u8; 32] = bytes[..32].try_into().unwrap();
    if stored_fingerprint != *fingerprint {
        return Ok(None);
    }

    let archived =
        match rkyv::access::<rkyv::Archived<CachedLicenseIndex>, rkyv::rancor::Error>(&bytes[32..])
        {
            Ok(archived) => archived,
            Err(_) => return Ok(None),
        };

    let cached: CachedLicenseIndex =
        rkyv::deserialize::<CachedLicenseIndex, Panic>(archived).always_ok();

    Ok(Some(cached))
}

pub fn save_cached_index(
    config: &LicenseCacheConfig,
    cached: &CachedLicenseIndex,
    fingerprint: &[u8; 32],
) -> Result<()> {
    let cache_dir = &config.cache_dir;
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir)
            .with_context(|| "Failed to create license index cache directory")?;
    }

    let rkyv_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(cached)
        .map_err(|e| anyhow::anyhow!("Failed to serialize license index cache: {}", e))?;

    let cache_path = config.cache_file_path();
    let mut file =
        fs::File::create(&cache_path).context("Failed to create license index cache file")?;
    file.write_all(fingerprint)
        .context("Failed to write cache fingerprint")?;
    file.write_all(&rkyv_bytes)
        .context("Failed to write license index cache file")?;

    Ok(())
}

pub fn delete_cache(config: &LicenseCacheConfig) -> Result<()> {
    let cache_path = config.cache_file_path();
    if cache_path.exists() {
        fs::remove_file(&cache_path).context("Failed to delete license index cache file")?;
    }
    Ok(())
}

pub fn cache_file_size(config: &LicenseCacheConfig) -> Option<u64> {
    fs::metadata(config.cache_file_path()).ok().map(|m| m.len())
}
