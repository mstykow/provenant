use super::schema::{EmbeddedArtifactMetadata, EmbeddedLoaderSnapshot, SCHEMA_VERSION};
use crate::license_detection::index::{LicenseIndex, build_index_from_loaded};

#[derive(Debug, Clone)]
pub struct LoadedEmbeddedLicenseIndex {
    pub index: LicenseIndex,
    pub metadata: EmbeddedArtifactMetadata,
}

#[derive(Debug, Clone)]
pub struct SerializationError(pub String);

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "License loader artifact error: {}", self.0)
    }
}

impl std::error::Error for SerializationError {}

fn load_loader_snapshot_from_bytes(
    bytes: &[u8],
) -> Result<EmbeddedLoaderSnapshot, SerializationError> {
    if bytes.is_empty() {
        return Err(SerializationError(
            "Embedded license index artifact is empty".to_string(),
        ));
    }

    let decompressed = zstd::decode_all(bytes).map_err(|e| {
        SerializationError(format!("Failed to decompress embedded artifact: {}", e))
    })?;

    let snapshot: EmbeddedLoaderSnapshot = rmp_serde::from_slice(&decompressed).map_err(|e| {
        SerializationError(format!("Failed to deserialize embedded artifact: {}", e))
    })?;

    if snapshot.schema_version != SCHEMA_VERSION {
        return Err(SerializationError(format!(
            "Embedded artifact schema version mismatch: expected {}, got {}",
            SCHEMA_VERSION, snapshot.schema_version
        )));
    }

    Ok(snapshot)
}

pub fn load_embedded_license_index_from_bytes(
    bytes: &[u8],
) -> Result<LoadedEmbeddedLicenseIndex, SerializationError> {
    let snapshot = load_loader_snapshot_from_bytes(bytes)?;
    let index = build_index_from_loaded(snapshot.rules, snapshot.licenses, false);

    Ok(LoadedEmbeddedLicenseIndex {
        index,
        metadata: snapshot.metadata,
    })
}

pub fn load_embedded_artifact_metadata_from_bytes(
    bytes: &[u8],
) -> Result<EmbeddedArtifactMetadata, SerializationError> {
    Ok(load_loader_snapshot_from_bytes(bytes)?.metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::{LoadedLicense, LoadedRule};

    fn create_test_metadata() -> EmbeddedArtifactMetadata {
        EmbeddedArtifactMetadata {
            spdx_license_list_version: "3.27".to_string(),
        }
    }

    fn serialize_loader_snapshot_to_bytes(
        rules: Vec<LoadedRule>,
        licenses: Vec<LoadedLicense>,
    ) -> Result<Vec<u8>, SerializationError> {
        let snapshot = EmbeddedLoaderSnapshot {
            schema_version: SCHEMA_VERSION,
            metadata: create_test_metadata(),
            rules,
            licenses,
        };

        let msgpack = rmp_serde::to_vec(&snapshot).map_err(|e| {
            SerializationError(format!("Failed to serialize embedded artifact: {}", e))
        })?;

        zstd::encode_all(&msgpack[..], 0)
            .map_err(|e| SerializationError(format!("Failed to compress embedded artifact: {}", e)))
    }

    fn create_test_loaded_rule() -> LoadedRule {
        LoadedRule {
            identifier: "test.RULE".to_string(),
            license_expression: "mit".to_string(),
            text: "MIT License text".to_string(),
            rule_kind: crate::license_detection::models::RuleKind::Text,
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

    #[test]
    fn test_load_license_index_from_bytes_roundtrip() {
        let bytes = serialize_loader_snapshot_to_bytes(
            vec![create_test_loaded_rule()],
            vec![create_test_loaded_license()],
        )
        .expect("Should serialize");

        let index = load_embedded_license_index_from_bytes(&bytes)
            .expect("Should deserialize")
            .index;

        assert_eq!(index.licenses_by_key.len(), 1);
        assert!(
            index
                .rules_by_rid
                .iter()
                .any(|rule| rule.identifier == "test.RULE"),
            "runtime index should retain the serialized rule"
        );
        assert!(
            index
                .rules_by_rid
                .iter()
                .any(|rule| rule.identifier == "mit.LICENSE"),
            "runtime index should synthesize a license-derived rule"
        );
    }

    #[test]
    fn test_load_embedded_artifact_metadata_from_bytes_roundtrip() {
        let bytes = serialize_loader_snapshot_to_bytes(
            vec![create_test_loaded_rule()],
            vec![create_test_loaded_license()],
        )
        .expect("Should serialize");

        let metadata = load_embedded_artifact_metadata_from_bytes(&bytes)
            .expect("Should deserialize metadata");

        assert_eq!(metadata.spdx_license_list_version, "3.27");
    }

    #[test]
    fn test_load_license_index_from_bytes_rejects_empty() {
        let error = load_embedded_license_index_from_bytes(&[]).unwrap_err();
        assert!(error.to_string().contains("artifact is empty"));
    }
}
