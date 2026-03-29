use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::config::CacheConfig;
use super::io::{load_snapshot_payload, write_snapshot_payload};
use super::metadata::{CacheInvalidationKey, CacheSnapshotMetadata};
use crate::license_detection::embedded::index::load_license_index_from_bytes;
use crate::license_detection::embedded::schema::SCHEMA_VERSION as EMBEDDED_SCHEMA_VERSION;
use crate::license_detection::embedded_artifact_bytes;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::index::builder::rebuild_automatons_from_runtime_index;
use crate::license_detection::index::dictionary::{
    TokenDictionary, TokenDictionarySnapshot, TokenId,
};
use crate::license_detection::models::{License, Rule};
use crate::utils::hash::calculate_sha256;

const LICENSE_INDEX_CACHE_SCHEMA_VERSION: u32 = 1;
const LICENSE_INDEX_CACHE_ENGINE_VERSION: &str =
    concat!("license-index-cache-v1:", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseIndexCacheSource {
    WarmCache,
    EmbeddedArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicenseIndexSnapshot {
    dictionary: TokenDictionarySnapshot,
    len_legalese: usize,
    rid_by_hash: HashMap<[u8; 20], usize>,
    rules_by_rid: Vec<Rule>,
    tids_by_rid: Vec<Vec<TokenId>>,
    sets_by_rid: HashMap<usize, HashSet<TokenId>>,
    msets_by_rid: HashMap<usize, HashMap<TokenId, usize>>,
    high_sets_by_rid: HashMap<usize, HashSet<TokenId>>,
    high_postings_by_rid: HashMap<usize, HashMap<TokenId, Vec<usize>>>,
    false_positive_rids: HashSet<usize>,
    approx_matchable_rids: HashSet<usize>,
    licenses_by_key: HashMap<String, License>,
    pattern_id_to_rid: Vec<Vec<usize>>,
    rid_by_spdx_key: HashMap<String, usize>,
    unknown_spdx_rid: Option<usize>,
    rids_by_high_tid: HashMap<TokenId, HashSet<usize>>,
}

impl LicenseIndexSnapshot {
    fn from_index(index: &LicenseIndex) -> Self {
        Self {
            dictionary: index.dictionary.to_snapshot(),
            len_legalese: index.len_legalese,
            rid_by_hash: index.rid_by_hash.clone(),
            rules_by_rid: index.rules_by_rid.clone(),
            tids_by_rid: index.tids_by_rid.clone(),
            sets_by_rid: index.sets_by_rid.clone(),
            msets_by_rid: index.msets_by_rid.clone(),
            high_sets_by_rid: index.high_sets_by_rid.clone(),
            high_postings_by_rid: index.high_postings_by_rid.clone(),
            false_positive_rids: index.false_positive_rids.clone(),
            approx_matchable_rids: index.approx_matchable_rids.clone(),
            licenses_by_key: index.licenses_by_key.clone(),
            pattern_id_to_rid: index.pattern_id_to_rid.clone(),
            rid_by_spdx_key: index.rid_by_spdx_key.clone(),
            unknown_spdx_rid: index.unknown_spdx_rid,
            rids_by_high_tid: index.rids_by_high_tid.clone(),
        }
    }

    fn into_index(self) -> LicenseIndex {
        let dictionary = TokenDictionary::from_snapshot(self.dictionary);
        let (rules_automaton, unknown_automaton) = rebuild_automatons_from_runtime_index(
            &dictionary,
            &self.rules_by_rid,
            &self.tids_by_rid,
            &self.pattern_id_to_rid,
        );

        LicenseIndex {
            dictionary,
            len_legalese: self.len_legalese,
            rid_by_hash: self.rid_by_hash,
            rules_by_rid: self.rules_by_rid,
            tids_by_rid: self.tids_by_rid,
            rules_automaton,
            unknown_automaton,
            sets_by_rid: self.sets_by_rid,
            msets_by_rid: self.msets_by_rid,
            high_sets_by_rid: self.high_sets_by_rid,
            high_postings_by_rid: self.high_postings_by_rid,
            false_positive_rids: self.false_positive_rids,
            approx_matchable_rids: self.approx_matchable_rids,
            licenses_by_key: self.licenses_by_key,
            pattern_id_to_rid: self.pattern_id_to_rid,
            rid_by_spdx_key: self.rid_by_spdx_key,
            unknown_spdx_rid: self.unknown_spdx_rid,
            rids_by_high_tid: self.rids_by_high_tid,
        }
    }
}

pub fn load_or_build_embedded_license_index(
    cache_config: &CacheConfig,
) -> Result<(LicenseIndex, LicenseIndexCacheSource)> {
    load_or_build_embedded_license_index_from_bytes(cache_config, embedded_artifact_bytes())
}

fn load_or_build_embedded_license_index_from_bytes(
    cache_config: &CacheConfig,
    artifact_bytes: &[u8],
) -> Result<(LicenseIndex, LicenseIndexCacheSource)> {
    if !cache_config.license_index_enabled() {
        let index = load_license_index_from_bytes(artifact_bytes)
            .map_err(|err| anyhow::anyhow!("Failed to load embedded license index: {err}"))?;
        return Ok((index, LicenseIndexCacheSource::EmbeddedArtifact));
    }

    let rules_fingerprint = calculate_sha256(artifact_bytes);
    let build_options_fingerprint = format!(
        "embedded-only;embedded-schema={EMBEDDED_SCHEMA_VERSION};artifact-format=compact-loader-snapshot"
    );
    let cache_key = CacheInvalidationKey {
        cache_schema_version: LICENSE_INDEX_CACHE_SCHEMA_VERSION,
        engine_version: LICENSE_INDEX_CACHE_ENGINE_VERSION,
        rules_fingerprint: &rules_fingerprint,
        build_options_fingerprint: &build_options_fingerprint,
    };
    let cache_path = cache_config.license_index_snapshot_path();

    if let Ok(Some(payload)) = load_snapshot_payload(&cache_path, &cache_key)
        && let Ok(snapshot) = rmp_serde::from_slice::<LicenseIndexSnapshot>(&payload)
    {
        return Ok((snapshot.into_index(), LicenseIndexCacheSource::WarmCache));
    }

    let index = load_license_index_from_bytes(artifact_bytes)
        .map_err(|err| anyhow::anyhow!("Failed to load embedded license index: {err}"))?;

    let metadata = CacheSnapshotMetadata {
        cache_schema_version: LICENSE_INDEX_CACHE_SCHEMA_VERSION,
        engine_version: LICENSE_INDEX_CACHE_ENGINE_VERSION.to_string(),
        rules_fingerprint,
        build_options_fingerprint,
        created_at: Utc::now().to_rfc3339(),
    };

    if let Ok(payload) = rmp_serde::to_vec(&LicenseIndexSnapshot::from_index(&index)) {
        let _ = write_snapshot_payload(&cache_path, &metadata, &payload);
    }

    Ok((index, LicenseIndexCacheSource::EmbeddedArtifact))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use super::*;
    use crate::cache::CacheKinds;
    use crate::license_detection::embedded::schema::EmbeddedLoaderSnapshot;
    use crate::license_detection::models::{LoadedLicense, LoadedRule, RuleKind};

    fn create_loaded_rule() -> LoadedRule {
        LoadedRule {
            identifier: "test.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: "Permission is hereby granted".to_string(),
            rule_kind: RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            relevance: Some(100),
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: false,
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: None,
            is_deprecated: false,
        }
    }

    fn create_loaded_license() -> LoadedLicense {
        LoadedLicense {
            key: "mit".to_string(),
            name: "MIT License".to_string(),
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            text: "MIT License text".to_string(),
            reference_urls: vec![],
            notes: None,
            is_deprecated: false,
            replaced_by: vec![],
            minimum_coverage: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        }
    }

    fn serialize_loader_snapshot_to_bytes() -> Vec<u8> {
        let snapshot = EmbeddedLoaderSnapshot {
            schema_version: EMBEDDED_SCHEMA_VERSION,
            rules: vec![create_loaded_rule()],
            licenses: vec![create_loaded_license()],
        };
        let msgpack = rmp_serde::to_vec(&snapshot).expect("serialize snapshot");
        zstd::encode_all(&msgpack[..], 3).expect("compress snapshot")
    }

    #[test]
    fn test_load_or_build_embedded_license_index_uses_warm_cache_after_first_build() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = CacheConfig::with_kinds(
            temp_dir.path().join("cache"),
            CacheKinds::from_cli(&[crate::cache::CacheKind::LicenseIndex]),
        );
        config.ensure_dirs().expect("create cache dirs");
        let artifact_bytes = serialize_loader_snapshot_to_bytes();

        let (first_index, first_source) =
            load_or_build_embedded_license_index_from_bytes(&config, &artifact_bytes)
                .expect("first load should succeed");
        assert_eq!(first_source, LicenseIndexCacheSource::EmbeddedArtifact);
        assert!(config.license_index_snapshot_path().exists());

        let (second_index, second_source) =
            load_or_build_embedded_license_index_from_bytes(&config, &artifact_bytes)
                .expect("second load should succeed");
        assert_eq!(second_source, LicenseIndexCacheSource::WarmCache);
        assert_eq!(
            first_index.rules_by_rid.len(),
            second_index.rules_by_rid.len()
        );
        assert_eq!(
            first_index.licenses_by_key.len(),
            second_index.licenses_by_key.len()
        );
        assert!(second_index.licenses_by_key.contains_key("mit"));
    }

    #[test]
    fn test_load_or_build_embedded_license_index_skips_cache_when_disabled() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = CacheConfig::new(temp_dir.path().join("cache"));
        let artifact_bytes = serialize_loader_snapshot_to_bytes();

        let (_, source) = load_or_build_embedded_license_index_from_bytes(&config, &artifact_bytes)
            .expect("load should succeed");
        assert_eq!(source, LicenseIndexCacheSource::EmbeddedArtifact);
        assert!(!config.license_index_snapshot_path().exists());
    }

    #[test]
    fn test_load_or_build_embedded_license_index_invalidates_on_artifact_change() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = CacheConfig::with_kinds(
            temp_dir.path().join("cache"),
            CacheKinds::from_cli(&[crate::cache::CacheKind::LicenseIndex]),
        );
        config.ensure_dirs().expect("create cache dirs");

        let first_bytes = serialize_loader_snapshot_to_bytes();
        let mut second_license = create_loaded_license();
        second_license.name = "MIT License Updated".to_string();
        let second_snapshot = EmbeddedLoaderSnapshot {
            schema_version: EMBEDDED_SCHEMA_VERSION,
            rules: vec![create_loaded_rule()],
            licenses: vec![second_license],
        };
        let second_bytes = zstd::encode_all(
            &rmp_serde::to_vec(&second_snapshot).expect("serialize changed snapshot")[..],
            3,
        )
        .expect("compress changed snapshot");

        let _ = load_or_build_embedded_license_index_from_bytes(&config, &first_bytes)
            .expect("seed cache");
        let (index, source) =
            load_or_build_embedded_license_index_from_bytes(&config, &second_bytes)
                .expect("changed artifact should rebuild");

        assert_eq!(source, LicenseIndexCacheSource::EmbeddedArtifact);
        assert_eq!(
            index
                .licenses_by_key
                .get("mit")
                .map(|license| license.name.as_str()),
            Some("MIT License Updated")
        );
    }

    #[test]
    fn test_license_index_snapshot_roundtrip_preserves_essential_index_data() {
        let snapshot = LicenseIndexSnapshot {
            dictionary: TokenDictionary::new(0).to_snapshot(),
            len_legalese: 0,
            rid_by_hash: HashMap::new(),
            rules_by_rid: Vec::new(),
            tids_by_rid: Vec::new(),
            sets_by_rid: HashMap::new(),
            msets_by_rid: HashMap::new(),
            high_sets_by_rid: HashMap::new(),
            high_postings_by_rid: HashMap::new(),
            false_positive_rids: HashSet::new(),
            approx_matchable_rids: HashSet::new(),
            licenses_by_key: HashMap::new(),
            pattern_id_to_rid: Vec::new(),
            rid_by_spdx_key: HashMap::new(),
            unknown_spdx_rid: None,
            rids_by_high_tid: HashMap::new(),
        };

        let index = snapshot.into_index();
        assert_eq!(index.len_legalese, 0);
        assert!(index.rules_by_rid.is_empty());
        assert!(index.licenses_by_key.is_empty());
    }
}
