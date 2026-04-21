// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use base64::Engine;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::bun_lockb::parse_bun_lockb;
    use crate::parsers::{BunLockbParser, PackageParser};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn load_testdata_file(path: &str) -> PathBuf {
        PathBuf::from(format!("testdata/bun/legacy/{}", path))
            .canonicalize()
            .expect("Failed to find test data file")
    }

    fn create_temp_bun_lockb(bytes: &[u8]) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let lock_path = temp_dir.path().join("bun.lockb");
        fs::write(&lock_path, bytes).expect("Failed to write bun.lockb");
        (temp_dir, lock_path)
    }

    fn bun_lockb_v2_without_scripts_fixture() -> Vec<u8> {
        let fixture = PathBuf::from("testdata/bun/legacy/bun.lockb.v2-no-scripts.base64");
        base64::engine::general_purpose::STANDARD
            .decode(
                fs::read_to_string(&fixture)
                    .expect("fixture should be readable")
                    .trim(),
            )
            .expect("fixture should decode")
    }

    #[test]
    fn test_is_match_bun_lockb_without_sibling_text_lock() {
        assert!(BunLockbParser::is_match(&PathBuf::from(
            "/some/path/bun.lockb"
        )));
    }

    #[test]
    fn test_is_not_match_bun_lockb_when_bun_lock_is_present() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let lockb = temp_dir.path().join("bun.lockb");
        let lock = temp_dir.path().join("bun.lock");
        fs::write(&lockb, b"placeholder").expect("Failed to write bun.lockb");
        fs::write(&lock, "{}\n").expect("Failed to write bun.lock");

        assert!(!BunLockbParser::is_match(&lockb));
    }

    #[test]
    fn test_parse_bun_lockb_v2_from_official_fixture() {
        let lock_path = load_testdata_file("bun.lockb.v2");
        let bytes = fs::read(&lock_path).expect("Failed to read fixture");
        parse_bun_lockb(&bytes).expect("internal parse should succeed");
        let package_data = BunLockbParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.primary_language.as_deref(), Some("JavaScript"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BunLockb));
        assert_eq!(package_data.name.as_deref(), Some("migrate-bun-lockb-v2"));
        assert!(package_data.version.is_none());

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert_eq!(
            extra_data.get("lockfileVersion"),
            Some(&serde_json::json!(2))
        );
        assert!(extra_data.get("meta_hash").is_some());

        let jquery = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/jquery@3.7.1"))
            .expect("expected jquery dependency");
        assert_eq!(jquery.scope.as_deref(), Some("dependencies"));
        assert_eq!(jquery.is_direct, Some(true));
        assert_eq!(jquery.is_runtime, Some(true));
        assert_eq!(jquery.is_optional, Some(false));
        assert_eq!(jquery.is_pinned, Some(true));
        let jquery_resolved = jquery
            .resolved_package
            .as_ref()
            .expect("jquery should have resolved package");
        assert_eq!(jquery_resolved.name, "jquery");
        assert_eq!(jquery_resolved.version, "3.7.1");
        assert_eq!(
            jquery_resolved.download_url.as_deref(),
            Some("https://registry.npmjs.org/jquery/-/jquery-3.7.1.tgz")
        );
        assert!(jquery_resolved.sha512.is_some());

        let is_even = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/is-even@1.0.0"))
            .expect("expected is-even dependency");
        let is_even_resolved = is_even
            .resolved_package
            .as_ref()
            .expect("is-even should have resolved package");
        assert!(
            is_even_resolved
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:npm/is-odd@0.1.2"))
        );
    }

    #[test]
    fn test_parse_bun_lockb_v2_most_features() {
        let lock_path = load_testdata_file("bun.lockb.v2-most-features");
        let bytes = fs::read(&lock_path).expect("Failed to read fixture");
        parse_bun_lockb(&bytes).expect("internal parse should succeed");
        let package_data = BunLockbParser::extract_first_package(&lock_path);

        assert_eq!(package_data.name.as_deref(), Some("migrate-everything"));

        let esbuild = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/esbuild@0.25.10"))
            .expect("expected esbuild dependency");
        assert_eq!(esbuild.scope.as_deref(), Some("devDependencies"));
        assert_eq!(esbuild.is_runtime, Some(false));
        assert_eq!(esbuild.is_optional, Some(true));

        let optional_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/is-number@7.0.0"))
            .expect("expected optional is-number dependency");
        assert_eq!(optional_dep.scope.as_deref(), Some("optionalDependencies"));
        assert_eq!(optional_dep.is_optional, Some(true));

        let peer_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/is-even@1.0.0"))
            .expect("expected peer is-even dependency");
        assert_eq!(peer_dep.scope.as_deref(), Some("peerDependencies"));

        let workspace_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/pkg-wat"))
            .expect("expected workspace dependency");
        assert_eq!(workspace_dep.scope.as_deref(), Some("workspaces"));
        assert_eq!(workspace_dep.is_direct, Some(true));
    }

    #[test]
    fn test_parse_bun_lockb_v2_without_scripts_field() {
        let bytes = bun_lockb_v2_without_scripts_fixture();
        parse_bun_lockb(&bytes).expect("seven-field bun.lockb should parse");

        let (_temp_dir, lock_path) = create_temp_bun_lockb(&bytes);
        let package_data = BunLockbParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BunLockb));
        assert_eq!(package_data.name.as_deref(), Some("bundle"));
        assert!(package_data.version.is_none());
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:npm/bun-types@0.5.8"))
        );
    }

    #[test]
    fn test_invalid_bun_lockb_header_returns_default_package() {
        let (_temp_dir, lock_path) = create_temp_bun_lockb(b"not-a-bun-lockb");
        let package_data = BunLockbParser::extract_first_package(&lock_path);

        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BunLockb));
    }

    #[test]
    fn test_unknown_bun_lockb_version_returns_default_package() {
        let fixture = load_testdata_file("bun.lockb.v2");
        let mut bytes = fs::read(&fixture).expect("Failed to read fixture");
        let version_offset = b"#!/usr/bin/env bun\nbun-lockfile-format-v0\n".len();
        bytes[version_offset..version_offset + 4].copy_from_slice(&99u32.to_le_bytes());

        let (_temp_dir, lock_path) = create_temp_bun_lockb(&bytes);
        let package_data = BunLockbParser::extract_first_package(&lock_path);

        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BunLockb));
    }

    #[test]
    fn test_current_bun_lockb_v3_is_not_supported_yet() {
        let fixture = load_testdata_file("bun.lockb.v2");
        let mut bytes = fs::read(&fixture).expect("Failed to read fixture");
        let version_offset = b"#!/usr/bin/env bun\nbun-lockfile-format-v0\n".len();
        bytes[version_offset..version_offset + 4].copy_from_slice(&3u32.to_le_bytes());

        let (_temp_dir, lock_path) = create_temp_bun_lockb(&bytes);
        let package_data = BunLockbParser::extract_first_package(&lock_path);

        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BunLockb));
    }
}
