// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::license_detection::embedded::index::load_embedded_license_index_from_bytes;
use crate::license_detection::embedded::schema::{
    EmbeddedArtifactMetadata, EmbeddedLoaderSnapshot, SCHEMA_VERSION,
};
use crate::license_detection::models::{LoadedLicense, LoadedRule, RuleKind};
use std::path::PathBuf;
use std::sync::{LazyLock, Once};

static TEST_ENGINE: LazyLock<LicenseDetectionEngine> = LazyLock::new(|| {
    LicenseDetectionEngine::from_embedded().expect("Should initialize from embedded artifact")
});

static INIT: Once = Once::new();

fn get_engine() -> &'static LicenseDetectionEngine {
    INIT.call_once(|| {
        let _ = &*TEST_ENGINE;
    });
    &TEST_ENGINE
}

fn create_test_loaded_rule() -> LoadedRule {
    LoadedRule {
        identifier: "test.RULE".to_string(),
        license_expression: "mit".to_string(),
        text: "MIT License text".to_string(),
        rule_kind: RuleKind::Text,
        is_false_positive: false,
        is_required_phrase: false,
        skip_for_required_phrase_generation: false,
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
        replaced_by: vec![],
    }
}

fn create_test_loaded_license() -> LoadedLicense {
    LoadedLicense {
        key: "mit".to_string(),
        short_name: Some("MIT".to_string()),
        name: "MIT License".to_string(),
        language: Some("en".to_string()),
        spdx_license_key: Some("MIT".to_string()),
        other_spdx_license_keys: vec![],
        category: Some("Permissive".to_string()),
        owner: None,
        homepage_url: None,
        text: "MIT License text".to_string(),
        reference_urls: vec![],
        osi_license_key: None,
        text_urls: vec![],
        osi_url: None,
        faq_url: None,
        other_urls: vec![],
        notes: None,
        is_deprecated: false,
        is_exception: false,
        is_unknown: false,
        is_generic: false,
        replaced_by: vec![],
        minimum_coverage: None,
        standard_notice: None,
        ignorable_copyrights: None,
        ignorable_holders: None,
        ignorable_authors: None,
        ignorable_urls: None,
        ignorable_emails: None,
    }
}

fn serialize_loader_snapshot_to_bytes(
    rules: Vec<LoadedRule>,
    licenses: Vec<LoadedLicense>,
) -> Result<Vec<u8>, String> {
    let snapshot = EmbeddedLoaderSnapshot {
        schema_version: SCHEMA_VERSION,
        metadata: EmbeddedArtifactMetadata {
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: crate::models::LicenseIndexProvenance {
                source: "embedded-artifact".to_string(),
                dataset_fingerprint: "test".to_string(),
                ignored_rules: vec![],
                ignored_licenses: vec![],
                ignored_rules_due_to_licenses: vec![],
                added_rules: vec![],
                replaced_rules: vec![],
                added_licenses: vec![],
                replaced_licenses: vec![],
            },
        },
        rules,
        licenses,
    };

    let postcard_bytes = postcard::to_allocvec(&snapshot).map_err(|e| e.to_string())?;
    zstd::encode_all(&postcard_bytes[..], 0).map_err(|e| e.to_string())
}

mod engine_loading {
    use super::*;

    #[test]
    fn test_from_embedded_initializes() {
        let engine = get_engine();

        assert!(!engine.index().rules_by_rid.is_empty());
        assert!(!engine.index().licenses_by_key.is_empty());
        assert!(engine.index().len_legalese > 0);
    }

    #[test]
    fn test_from_embedded_detects_mit_text() {
        let engine_from_embedded = get_engine();

        let mit_text = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."#;

        let detections = engine_from_embedded
            .detect_with_kind(mit_text, false, false)
            .expect("Detection should succeed");

        assert!(!detections.is_empty());
        assert!(
            detections
                .iter()
                .any(|d| d.license_expression.as_deref() == Some("mit"))
        );
    }
}

mod determinism {
    use super::*;

    #[test]
    fn test_serialization_is_deterministic_after_sorting() {
        let mut rules = vec![create_test_loaded_rule()];
        rules[0].identifier = "bbb.RULE".to_string();
        let mut rule2 = create_test_loaded_rule();
        rule2.identifier = "aaa.RULE".to_string();
        rules.push(rule2);

        let mut licenses = vec![create_test_loaded_license()];
        licenses[0].key = "bbb".to_string();
        let mut license2 = create_test_loaded_license();
        license2.key = "aaa".to_string();
        licenses.push(license2);

        let mut rules1 = rules.clone();
        let mut licenses1 = licenses.clone();
        rules1.sort_by(|a, b| a.identifier.cmp(&b.identifier));
        licenses1.sort_by(|a, b| a.key.cmp(&b.key));

        let mut rules2 = rules;
        let mut licenses2 = licenses;
        rules2.sort_by(|a, b| a.identifier.cmp(&b.identifier));
        licenses2.sort_by(|a, b| a.key.cmp(&b.key));

        let bytes1 = serialize_loader_snapshot_to_bytes(rules1, licenses1).unwrap();
        let bytes2 = serialize_loader_snapshot_to_bytes(rules2, licenses2).unwrap();

        assert_eq!(bytes1, bytes2);
    }
}

mod failure_handling {
    use super::*;

    #[test]
    fn test_deserialize_corrupted_bytes_fails() {
        let corrupted_bytes: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE];

        let result =
            load_embedded_license_index_from_bytes(&corrupted_bytes).map(|loaded| loaded.index);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_version_mismatch_is_detected() {
        let snapshot = EmbeddedLoaderSnapshot {
            schema_version: 999,
            metadata: EmbeddedArtifactMetadata {
                spdx_license_list_version: "3.27".to_string(),
                license_index_provenance: crate::models::LicenseIndexProvenance {
                    source: "embedded-artifact".to_string(),
                    dataset_fingerprint: "test".to_string(),
                    ignored_rules: vec![],
                    ignored_licenses: vec![],
                    ignored_rules_due_to_licenses: vec![],
                    added_rules: vec![],
                    replaced_rules: vec![],
                    added_licenses: vec![],
                    replaced_licenses: vec![],
                },
            },
            rules: vec![create_test_loaded_rule()],
            licenses: vec![create_test_loaded_license()],
        };
        let postcard_bytes = postcard::to_allocvec(&snapshot).unwrap();
        let bytes = zstd::encode_all(&postcard_bytes[..], 0).unwrap();

        let error = load_embedded_license_index_from_bytes(&bytes)
            .map(|loaded| loaded.index)
            .unwrap_err();
        assert!(error.to_string().contains("schema version mismatch"));
    }
}

mod packaging {
    use super::*;
    use crate::license_detection::dataset::compute_dataset_fingerprint_string;

    #[test]
    fn test_embedded_artifact_exists() {
        let artifact_path = PathBuf::from("resources/license_detection/license_index.zst");
        assert!(artifact_path.exists());
    }

    #[test]
    fn test_embedded_artifact_is_loadable() {
        let engine = get_engine();

        assert!(!engine.index().rules_by_rid.is_empty());
        assert!(!engine.index().licenses_by_key.is_empty());
    }

    #[test]
    fn test_embedded_artifact_schema_version() {
        let artifact_bytes = include_bytes!("../../resources/license_detection/license_index.zst");
        let decompressed = zstd::decode_all(&artifact_bytes[..]).unwrap();
        let snapshot: EmbeddedLoaderSnapshot = postcard::from_bytes(&decompressed).unwrap();

        assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn test_embedded_artifact_has_non_empty_rules_and_licenses() {
        let artifact_bytes = include_bytes!("../../resources/license_detection/license_index.zst");
        let decompressed = zstd::decode_all(&artifact_bytes[..]).unwrap();
        let snapshot: EmbeddedLoaderSnapshot = postcard::from_bytes(&decompressed).unwrap();

        assert!(!snapshot.rules.is_empty());
        assert!(!snapshot.licenses.is_empty());
    }

    #[test]
    fn test_embedded_artifact_metadata_has_spdx_license_list_version() {
        let artifact_bytes = include_bytes!("../../resources/license_detection/license_index.zst");
        let decompressed = zstd::decode_all(&artifact_bytes[..]).unwrap();
        let snapshot: EmbeddedLoaderSnapshot = postcard::from_bytes(&decompressed).unwrap();

        assert!(!snapshot.metadata.spdx_license_list_version.is_empty());
        assert_eq!(
            snapshot.metadata.license_index_provenance.source,
            "embedded-artifact"
        );
        let expected_fingerprint =
            compute_dataset_fingerprint_string(&snapshot.rules, &snapshot.licenses)
                .expect("embedded dataset fingerprint should compute");
        assert_eq!(
            snapshot
                .metadata
                .license_index_provenance
                .dataset_fingerprint,
            expected_fingerprint
        );
    }
}
