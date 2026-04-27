// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::models::PackageType;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;

    #[test]
    fn test_is_match_cargo_lock() {
        assert!(CargoLockParser::is_match(&PathBuf::from("Cargo.lock")));
        assert!(CargoLockParser::is_match(&PathBuf::from("cargo.lock")));
        assert!(CargoLockParser::is_match(&PathBuf::from(
            "/path/to/project/Cargo.lock"
        )));
    }

    #[test]
    fn test_is_not_match_cargo_toml() {
        assert!(!CargoLockParser::is_match(&PathBuf::from("Cargo.toml")));
        assert!(!CargoLockParser::is_match(&PathBuf::from(
            "package-lock.json"
        )));
    }

    #[test]
    fn test_extract_from_real_cargo_lock() {
        let lock_path = PathBuf::from("Cargo.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Cargo));
        // The first package is alphabetically first, not the root
        assert!(package_data.name.is_some());
        assert!(package_data.version.is_some());
        assert!(!package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_from_testdata() {
        let lock_path = PathBuf::from("testdata/cargo/Cargo-lock-basic.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Cargo));
        assert_eq!(package_data.name, Some("test-project".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));
        assert!(package_data.sha256.is_some());
        assert!(!package_data.dependencies.is_empty());

        assert_eq!(
            package_data.purl,
            Some("pkg:cargo/test-project@0.1.0".to_string())
        );
        assert_eq!(
            package_data.api_data_url,
            Some("https://crates.io/api/v1/crates/test-project/0.1.0".to_string())
        );
    }

    #[test]
    fn test_extract_dependencies() {
        let lock_path = PathBuf::from("testdata/cargo/Cargo-lock-deps.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let deps = package_data.dependencies;
        assert!(!deps.is_empty());

        let serde_dep = deps
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("serde")));
        assert!(serde_dep.is_some());

        if let Some(dep) = serde_dep {
            assert_eq!(dep.is_pinned, Some(true));
            assert_eq!(dep.is_runtime, None);
            assert_eq!(dep.scope, None);
            assert_eq!(dep.is_optional, None);
        }
    }

    #[test]
    fn test_datasource_id() {
        let lock_path = PathBuf::from("testdata/cargo/Cargo-lock-basic.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.datasource_id, Some(DatasourceId::CargoLock));
    }

    #[test]
    fn test_is_direct_flag() {
        let lock_path = PathBuf::from("testdata/cargo/Cargo-lock-deps.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let deps = package_data.dependencies;
        assert!(!deps.is_empty());

        let direct_deps: Vec<_> = deps.iter().filter(|d| d.is_direct == Some(true)).collect();
        let transitive_deps: Vec<_> = deps.iter().filter(|d| d.is_direct == Some(false)).collect();

        assert!(
            !direct_deps.is_empty(),
            "Should have at least one direct dependency"
        );
        assert!(
            !transitive_deps.is_empty(),
            "Should have at least one transitive dependency"
        );
    }

    #[test]
    fn test_cargo_lock_dependency_kind_is_unknown_without_manifest_sections() {
        let content = r#"
[[package]]
name = "demo"
version = "0.1.0"
dependencies = ["cc 1.2.60", "serde 1.0.228", "tempfile 3.27.0"]

[[package]]
name = "cc"
version = "1.2.60"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "tempfile"
version = "3.27.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);
        let deps = package_data.dependencies;

        assert!(
            deps.iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:cargo/cc@1.2.60"))
        );
        assert!(
            deps.iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:cargo/serde@1.0.228"))
        );
        assert!(
            deps.iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:cargo/tempfile@3.27.0"))
        );

        for dep in &deps {
            assert_eq!(dep.scope, None);
            assert_eq!(dep.is_runtime, None);
            assert_eq!(dep.is_optional, None);
            assert_eq!(dep.is_pinned, Some(true));
        }
    }

    #[test]
    fn test_extract_root_package_when_not_first() {
        let content = r#"
[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"

[[package]]
name = "my-app"
version = "0.4.0"
dependencies = ["serde"]
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.name.as_deref(), Some("my-app"));
        assert_eq!(package_data.version.as_deref(), Some("0.4.0"));
        assert_eq!(package_data.purl.as_deref(), Some("pkg:cargo/my-app@0.4.0"));
    }

    #[test]
    fn test_extract_dependencies_resolves_bare_name_versions() {
        let lock_path = PathBuf::from("testdata/cargo/Cargo-lock-basic.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().is_some_and(|p| p.contains("serde")))
            .expect("Should find serde dependency");

        assert_eq!(serde_dep.purl.as_deref(), Some("pkg:cargo/serde@1.0.228"));
        assert_eq!(serde_dep.extracted_requirement.as_deref(), Some("1.0.228"));
    }

    #[test]
    fn test_extract_dependencies_preserves_source_and_checksum_provenance() {
        let lock_path = PathBuf::from("testdata/cargo/Cargo-lock-basic.lock");
        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().is_some_and(|p| p.contains("serde")))
            .expect("Should find serde dependency");

        let extra_data = serde_dep
            .extra_data
            .as_ref()
            .expect("lockfile dependency provenance should be preserved in extra_data");

        assert_eq!(
            extra_data.get("source").and_then(|value| value.as_str()),
            Some("registry+https://github.com/rust-lang/crates.io-index")
        );
        assert_eq!(
            extra_data.get("checksum").and_then(|value| value.as_str()),
            Some("320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa")
        );
    }

    #[test]
    fn test_extract_dependencies_with_annotated_source_strings() {
        let content = r#"
[[package]]
name = "my-app"
version = "0.4.0"
dependencies = [
 "serde 1.0.228 (registry+https://github.com/rust-lang/crates.io-index)",
]

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().is_some_and(|p| p.contains("serde")))
            .expect("Should find serde dependency");

        assert_eq!(serde_dep.purl.as_deref(), Some("pkg:cargo/serde@1.0.228"));
        assert_eq!(serde_dep.extracted_requirement.as_deref(), Some("1.0.228"));

        let extra_data = serde_dep
            .extra_data
            .as_ref()
            .expect("annotated dependency should preserve source provenance");

        assert_eq!(
            extra_data.get("source").and_then(|value| value.as_str()),
            Some("registry+https://github.com/rust-lang/crates.io-index")
        );
    }

    #[test]
    fn test_extract_dependencies_prefers_matching_source_identity() {
        let content = r#"
[[package]]
name = "my-app"
version = "0.4.0"
dependencies = [
 "serde 1.0.228 (git+https://github.com/example/serde?rev=abcdef#abcdef)",
]

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "registry-checksum"

[[package]]
name = "serde"
version = "1.0.228"
source = "git+https://github.com/example/serde?rev=abcdef#abcdef"
checksum = "git-checksum"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().is_some_and(|p| p.contains("serde")))
            .expect("Should find serde dependency");

        let extra_data = serde_dep
            .extra_data
            .as_ref()
            .expect("dependency should keep provenance for the matching source entry");

        assert_eq!(
            extra_data.get("source").and_then(|value| value.as_str()),
            Some("git+https://github.com/example/serde?rev=abcdef#abcdef")
        );
        assert_eq!(
            extra_data.get("checksum").and_then(|value| value.as_str()),
            Some("git-checksum")
        );
    }

    #[test]
    fn test_extract_dependencies_deduplicates_repeated_transitive_edges() {
        let content = r#"
[[package]]
name = "my-app"
version = "0.4.0"
dependencies = ["a 1.0.0", "b 1.0.0"]

[[package]]
name = "a"
version = "1.0.0"
dependencies = ["serde 1.0.228"]

[[package]]
name = "b"
version = "1.0.0"
dependencies = ["serde 1.0.228"]

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "serde-checksum"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);
        let serde_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.purl.as_deref() == Some("pkg:cargo/serde@1.0.228"))
            .collect();

        assert_eq!(serde_deps.len(), 1, "serde should be emitted once");
    }

    #[test]
    fn test_extract_dependencies_marks_deduplicated_root_dependency_as_direct() {
        let content = r#"
[[package]]
name = "my-app"
version = "0.4.0"
dependencies = ["serde 1.0.228", "helper 1.0.0"]

[[package]]
name = "helper"
version = "1.0.0"
dependencies = ["serde 1.0.228"]

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "serde-checksum"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);
        let serde_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cargo/serde@1.0.228"))
            .expect("serde dependency should exist");

        assert_eq!(serde_dep.is_direct, Some(true));
    }

    #[test]
    fn test_extract_dependencies_keeps_root_package_and_source_less_workspace_members() {
        let content = r#"
[[package]]
name = "my-app"
version = "0.4.0"
dependencies = ["serde 1.0.228"]

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa"

[[package]]
name = "workspace-tool"
version = "0.1.0"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);

        let dependency_purls: Vec<_> = package_data
            .dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref())
            .collect();

        assert!(dependency_purls.contains(&"pkg:cargo/serde@1.0.228"));
        assert!(dependency_purls.contains(&"pkg:cargo/my-app@0.4.0"));
        assert!(dependency_purls.contains(&"pkg:cargo/workspace-tool@0.1.0"));

        let root_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cargo/my-app@0.4.0"))
            .expect("root package should be preserved as a direct dependency");
        assert_eq!(root_dep.extracted_requirement.as_deref(), Some("0.4.0"));
        assert_eq!(root_dep.is_direct, Some(true));
    }

    #[test]
    fn test_extract_dependencies_includes_additional_workspace_members_without_source() {
        let content = r#"
[[package]]
name = "cargo"
version = "0.97.0"
dependencies = ["serde 1.0.228"]

[[package]]
name = "benchsuite"
version = "0.0.0"

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "serde-checksum"
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let lock_path = temp_dir.path().join("Cargo.lock");
        std::fs::write(&lock_path, content).unwrap();

        let package_data = CargoLockParser::extract_first_package(&lock_path);
        let benchsuite_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cargo/benchsuite@0.0.0"))
            .expect("workspace member should be surfaced as a direct dependency");

        assert_eq!(
            benchsuite_dep.extracted_requirement.as_deref(),
            Some("0.0.0")
        );
        assert_eq!(benchsuite_dep.is_direct, Some(true));
    }
}
