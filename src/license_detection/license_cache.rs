use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rancor::{Panic, ResultExt};
use sha2::{Digest, Sha256};

use crate::cache::{CacheConfig, write_bytes_atomically};
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::{LoadedLicense, LoadedRule};

const CACHE_ROOT_DIR_NAME: &str = "license-index";
const CACHE_FILE_EXTENSION: &str = "rkyv";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseCacheNamespace {
    Embedded,
    CustomRules,
}

impl LicenseCacheNamespace {
    fn directory_name(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::CustomRules => "custom",
        }
    }
}

pub struct LicenseCacheConfig {
    pub root_dir: PathBuf,
    pub reindex: bool,
    pub enabled: bool,
}

impl LicenseCacheConfig {
    pub fn new(root_dir: PathBuf, reindex: bool, enabled: bool) -> Self {
        Self {
            root_dir,
            reindex,
            enabled,
        }
    }

    pub fn default_root_dir() -> PathBuf {
        CacheConfig::default_root_dir_without_scan_root()
    }

    fn namespace_dir(&self, namespace: LicenseCacheNamespace) -> PathBuf {
        self.root_dir
            .join(CACHE_ROOT_DIR_NAME)
            .join(namespace.directory_name())
    }

    fn cache_file_path(&self, namespace: LicenseCacheNamespace, fingerprint: &[u8; 32]) -> PathBuf {
        self.namespace_dir(namespace).join(format!(
            "{}.{}",
            fingerprint_hex(fingerprint),
            CACHE_FILE_EXTENSION
        ))
    }
}

fn fingerprint_hex(fingerprint: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(fingerprint.len() * 2);
    for byte in fingerprint {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn prune_namespace_dir(namespace_dir: &Path, active_path: &Path) -> Result<()> {
    if !namespace_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(namespace_dir)
        .with_context(|| format!("Failed to read license cache namespace {namespace_dir:?}"))?
    {
        let path = entry?.path();
        if path == active_path
            || path.extension().and_then(|ext| ext.to_str()) != Some(CACHE_FILE_EXTENSION)
        {
            continue;
        }
        fs::remove_file(&path)
            .with_context(|| format!("Failed to prune stale license cache file {path:?}"))?;
    }

    Ok(())
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
    namespace: LicenseCacheNamespace,
    fingerprint: &[u8; 32],
) -> Result<Option<LicenseIndex>> {
    if !config.enabled {
        return Ok(None);
    }

    let cache_path = config.cache_file_path(namespace, fingerprint);

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
        match rkyv::access::<rkyv::Archived<LicenseIndex>, rkyv::rancor::Error>(&bytes[32..]) {
            Ok(archived) => archived,
            Err(_) => return Ok(None),
        };

    let cached: LicenseIndex = rkyv::deserialize::<LicenseIndex, Panic>(archived).always_ok();

    Ok(Some(cached))
}

pub fn save_cached_index(
    config: &LicenseCacheConfig,
    namespace: LicenseCacheNamespace,
    cached: &LicenseIndex,
    fingerprint: &[u8; 32],
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let rkyv_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(cached)
        .map_err(|e| anyhow::anyhow!("Failed to serialize license index cache: {}", e))?;

    let mut payload = Vec::with_capacity(fingerprint.len() + rkyv_bytes.len());
    payload.extend_from_slice(fingerprint);
    payload.extend_from_slice(&rkyv_bytes);

    let namespace_dir = config.namespace_dir(namespace);
    let cache_path = config.cache_file_path(namespace, fingerprint);

    crate::cache::locking::with_exclusive_cache_lock(&config.root_dir, || {
        fs::create_dir_all(&namespace_dir)
            .with_context(|| "Failed to create license index cache directory")?;
        prune_namespace_dir(&namespace_dir, &cache_path)?;
        write_bytes_atomically(&cache_path, &payload)
            .with_context(|| "Failed to persist license index cache file")
    })?;

    Ok(())
}

pub fn delete_cache(
    config: &LicenseCacheConfig,
    namespace: LicenseCacheNamespace,
    fingerprint: &[u8; 32],
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let cache_path = config.cache_file_path(namespace, fingerprint);
    crate::cache::locking::with_exclusive_cache_lock(&config.root_dir, || -> Result<()> {
        if cache_path.exists() {
            fs::remove_file(&cache_path).context("Failed to delete license index cache file")?;
        }
        Ok(())
    })?;

    Ok(())
}

pub fn cache_file_size(
    config: &LicenseCacheConfig,
    namespace: LicenseCacheNamespace,
    fingerprint: &[u8; 32],
) -> Option<u64> {
    if !config.enabled {
        return None;
    }

    fs::metadata(config.cache_file_path(namespace, fingerprint))
        .ok()
        .map(|m| m.len())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::license_detection::automaton::Automaton;
    use crate::license_detection::index::dictionary::TokenDictionary;

    fn sample_cached_index() -> LicenseIndex {
        LicenseIndex {
            dictionary: TokenDictionary::default(),
            len_legalese: 0,
            rid_by_hash: Default::default(),
            rules_by_rid: Default::default(),
            tids_by_rid: Default::default(),
            rules_automaton: Automaton::empty(),
            unknown_automaton: Automaton::empty(),
            sets_by_rid: Default::default(),
            rule_metadata_by_identifier: Default::default(),
            msets_by_rid: Default::default(),
            high_sets_by_rid: Default::default(),
            high_postings_by_rid: Default::default(),
            false_positive_rids: Default::default(),
            approx_matchable_rids: Default::default(),
            licenses_by_key: Default::default(),
            pattern_id_to_rid: Default::default(),
            rid_by_spdx_key: Default::default(),
            unknown_spdx_rid: None,
            rids_by_high_tid: Default::default(),
            spdx_license_list_version: Some("test".to_string()),
        }
    }

    #[test]
    fn test_cache_file_path_uses_namespace_and_fingerprint() {
        let config = LicenseCacheConfig::new(PathBuf::from("/tmp/cache-root"), false, true);
        let fingerprint = [0xAB; 32];

        assert_eq!(
            config.cache_file_path(LicenseCacheNamespace::Embedded, &fingerprint),
            PathBuf::from(format!(
                "/tmp/cache-root/license-index/embedded/{}.rkyv",
                "ab".repeat(32)
            ))
        );
        assert_eq!(
            config.cache_file_path(LicenseCacheNamespace::CustomRules, &fingerprint),
            PathBuf::from(format!(
                "/tmp/cache-root/license-index/custom/{}.rkyv",
                "ab".repeat(32)
            ))
        );
    }

    #[test]
    fn test_save_cached_index_prunes_stale_namespace_entries() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = LicenseCacheConfig::new(temp_dir.path().to_path_buf(), false, true);
        let fingerprint = [0x11; 32];
        let namespace_dir = config.namespace_dir(LicenseCacheNamespace::Embedded);
        fs::create_dir_all(&namespace_dir).expect("create namespace dir");
        fs::write(namespace_dir.join("stale.rkyv"), b"old").expect("write stale cache file");

        let cached = sample_cached_index();
        save_cached_index(
            &config,
            LicenseCacheNamespace::Embedded,
            &cached,
            &fingerprint,
        )
        .expect("save cache");

        let entries = fs::read_dir(&namespace_dir)
            .expect("read namespace dir")
            .map(|entry| entry.expect("dir entry").path())
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            config.cache_file_path(LicenseCacheNamespace::Embedded, &fingerprint)
        );
    }

    #[test]
    fn test_disabled_cache_skips_persistence() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = LicenseCacheConfig::new(temp_dir.path().to_path_buf(), false, false);
        let fingerprint = [0x22; 32];

        save_cached_index(
            &config,
            LicenseCacheNamespace::Embedded,
            &sample_cached_index(),
            &fingerprint,
        )
        .expect("disabled save should succeed");

        assert!(
            !config
                .cache_file_path(LicenseCacheNamespace::Embedded, &fingerprint)
                .exists()
        );
        assert!(
            load_cached_index(&config, LicenseCacheNamespace::Embedded, &fingerprint)
                .expect("disabled load should succeed")
                .is_none()
        );
    }
}
