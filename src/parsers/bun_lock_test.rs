// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{BunLockParser, PackageParser};
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn load_testdata_file(path: &str) -> PathBuf {
        PathBuf::from(format!("testdata/bun/{}", path))
            .canonicalize()
            .expect("Failed to find test data file")
    }

    fn create_temp_bun_lock(lock_content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let lock_path = temp_dir.path().join("bun.lock");
        fs::write(&lock_path, lock_content).expect("Failed to write bun.lock");
        (temp_dir, lock_path)
    }

    #[test]
    fn test_is_match_bun_lock() {
        assert!(BunLockParser::is_match(&PathBuf::from(
            "/some/path/bun.lock"
        )));
    }

    #[test]
    fn test_is_not_match_bun_lockb_yet() {
        assert!(!BunLockParser::is_match(&PathBuf::from(
            "/some/path/bun.lockb"
        )));
    }

    #[test]
    fn test_parse_basic_bun_lock_from_testdata() {
        let lock_path = load_testdata_file("basic/bun.lock");
        let package_data = BunLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.primary_language.as_deref(), Some("JavaScript"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::BunLock));
        assert_eq!(
            package_data.name.as_deref(),
            Some("basic-dependencies-test")
        );
        assert!(package_data.version.is_none());

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert_eq!(extra_data.get("lockfileVersion"), Some(&Value::from(1)));

        let lodash = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/lodash@4.17.21"))
            .expect("expected lodash dependency");
        assert_eq!(lodash.scope.as_deref(), Some("dependencies"));
        assert_eq!(lodash.is_direct, Some(true));
        assert_eq!(lodash.is_runtime, Some(true));
        assert_eq!(lodash.is_optional, Some(false));
        assert_eq!(lodash.is_pinned, Some(true));
        let resolved = lodash
            .resolved_package
            .as_ref()
            .expect("lodash should have resolved package");
        assert_eq!(resolved.name, "lodash");
        assert_eq!(resolved.version, "4.17.21");
        assert_eq!(
            resolved.download_url.as_deref(),
            Some("https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz")
        );
        assert!(resolved.sha512.is_some());

        let react = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/react@18.2.0"))
            .expect("expected react dependency");
        let react_resolved = react
            .resolved_package
            .as_ref()
            .expect("react should have resolved package");
        assert!(
            react_resolved
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:npm/loose-envify"))
        );

        let typescript = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/typescript@5.0.2"))
            .expect("expected typescript dependency");
        assert_eq!(typescript.scope.as_deref(), Some("devDependencies"));
        assert_eq!(typescript.is_direct, Some(true));
        assert_eq!(typescript.is_runtime, Some(false));
        assert_eq!(typescript.is_optional, Some(true));
    }

    #[test]
    fn test_parse_enhanced_bun_lock_tracks_workspaces_and_trusted_dependencies() {
        let lock_path = load_testdata_file("enhanced/bun.lock");
        let package_data = BunLockParser::extract_first_package(&lock_path);

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert_eq!(
            extra_data.get("trustedDependencies"),
            Some(&serde_json::json!(["typescript"]))
        );

        let workspace_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:npm/workspace-lib@1.0.0"))
            .expect("expected workspace-lib dependency");
        assert_eq!(workspace_dep.scope.as_deref(), Some("dependencies"));
        assert_eq!(workspace_dep.is_direct, Some(true));
        let workspace_resolved = workspace_dep
            .resolved_package
            .as_ref()
            .expect("workspace-lib should have resolved package");
        assert_eq!(workspace_resolved.version, "1.0.0");
        assert!(
            workspace_resolved
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:npm/lodash"))
        );
    }

    #[test]
    fn test_parse_bun_lock_jsonc_trailing_commas() {
        let content = r#"{
  "lockfileVersion": 1,
  "workspaces": {
    "": {
      "name": "jsonc-test",
      "dependencies": {
        "chalk": "^5.3.0",
      },
    },
  },
  "packages": {
    "chalk": ["chalk@5.4.1", "https://registry.npmjs.org/chalk/-/chalk-5.4.1.tgz", {}, "sha512-zgVZuo2WcZgfUEmsn6eO3kINexW8RAE4maiQ8QNs8CtpPCSyMiYsULR3HQYkm3w8FIA3SberyMJMSldGsW+U3w=="],
  },
}"#;

        let (_temp_dir, lock_path) = create_temp_bun_lock(content);
        let package_data = BunLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.name.as_deref(), Some("jsonc-test"));
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:npm/chalk@5.4.1"))
        );
    }
}
