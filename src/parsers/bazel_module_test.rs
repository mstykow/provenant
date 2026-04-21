// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{BazelModuleParser, PackageParser};

    #[test]
    fn test_is_match_module_bazel() {
        assert!(BazelModuleParser::is_match(Path::new("MODULE.bazel")));
        assert!(!BazelModuleParser::is_match(Path::new("BUILD")));
        assert!(!BazelModuleParser::is_match(Path::new("module.bazel")));
    }

    #[test]
    fn test_extract_basic_module() {
        let package = BazelModuleParser::extract_first_package(&PathBuf::from(
            "testdata/bazel-golden/module/MODULE.bazel",
        ));

        assert_eq!(package.package_type, Some(PackageType::Bazel));
        assert_eq!(package.datasource_id, Some(DatasourceId::BazelModule));
        assert_eq!(package.name.as_deref(), Some("my_sample_project"));
        assert_eq!(package.version.as_deref(), Some("0.5.0"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:bazel/my_sample_project@0.5.0")
        );
        assert_eq!(package.dependencies.len(), 4);

        let runtime_dep = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:bazel/rules_python@0.24.0"))
            .expect("rules_python dependency missing");
        assert_eq!(runtime_dep.scope.as_deref(), Some("dependencies"));
        assert_eq!(runtime_dep.is_runtime, Some(true));
        assert_eq!(runtime_dep.is_optional, Some(false));
        let runtime_extra = runtime_dep
            .extra_data
            .as_ref()
            .expect("rules_python extra_data missing");
        assert_eq!(
            runtime_extra
                .get("registry")
                .and_then(|value| value.as_str()),
            Some("https://registry.bazel.build")
        );

        let dev_dep = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:bazel/googletest@1.14.0"))
            .expect("googletest dependency missing");
        assert_eq!(dev_dep.scope.as_deref(), Some("dev"));
        assert_eq!(dev_dep.is_runtime, Some(false));
        assert_eq!(dev_dep.is_optional, Some(true));

        let extra_data = package.extra_data.expect("extra_data should exist");
        assert_eq!(
            extra_data
                .get("bazel_compatibility")
                .and_then(|value| value.as_array())
                .map(|values| values.len()),
            Some(2)
        );
    }

    #[test]
    fn test_extract_module_without_version() {
        let package = BazelModuleParser::extract_first_package(&PathBuf::from(
            "testdata/bazel-golden/module/MODULE_no_version.bazel",
        ));

        assert_eq!(package.name.as_deref(), Some("minimal_module"));
        assert!(package.version.is_none());
        assert_eq!(package.dependencies.len(), 1);
    }

    #[test]
    fn test_extract_module_preserves_overrides() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("MODULE.bazel");
        let content = r#"
module(name = "override-demo", version = "1.0.0", compatibility_level = 2, bazel_compatibility = [">=7.0.0", "<9.0.0"])

bazel_dep(name = "rules_python", version = "0.24.0", registry = "https://registry.bazel.build", max_compatibility_level = 3)

archive_override(
    module_name = "rules_python",
    urls = ["https://example.com/rules_python.tar.gz"],
    integrity = "sha256-demo"
)

git_override(
    module_name = "rules_java",
    remote = "https://github.com/bazelbuild/rules_java.git",
    commit = "deadbeef"
)

local_path_override(
    module_name = "local_mod",
    path = "../local_mod"
)

single_version_override(
    module_name = "rules_cc",
    version = "0.0.9"
)

multiple_version_override(
    module_name = "rules_proto",
    versions = ["1.0.0", "1.1.0"]
)
"#;
        fs::write(&file_path, content).unwrap();

        let package = BazelModuleParser::extract_first_package(&file_path);
        let extra_data = package.extra_data.expect("extra_data should exist");
        assert_eq!(
            extra_data
                .get("compatibility_level")
                .and_then(|value| value.as_i64()),
            Some(2)
        );
        assert_eq!(
            extra_data
                .get("bazel_compatibility")
                .and_then(|value| value.as_array())
                .map(|values| values.len()),
            Some(2)
        );
        let overrides = extra_data
            .get("overrides")
            .and_then(|value| value.as_array())
            .expect("overrides should exist");
        assert_eq!(overrides.len(), 5);
        assert!(overrides.iter().any(|entry| {
            entry.get("kind").and_then(|value| value.as_str()) == Some("archive_override")
        }));
        assert!(overrides.iter().any(|entry| {
            entry.get("kind").and_then(|value| value.as_str()) == Some("git_override")
        }));
        assert!(overrides.iter().any(|entry| {
            entry.get("kind").and_then(|value| value.as_str()) == Some("local_path_override")
        }));
        assert!(overrides.iter().any(|entry| {
            entry.get("kind").and_then(|value| value.as_str()) == Some("single_version_override")
        }));
        assert!(overrides.iter().any(|entry| {
            entry.get("kind").and_then(|value| value.as_str()) == Some("multiple_version_override")
        }));

        let rules_python = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:bazel/rules_python@0.24.0"))
            .expect("rules_python dependency missing");
        let dep_extra = rules_python
            .extra_data
            .as_ref()
            .expect("dependency extra_data should exist");
        assert_eq!(
            dep_extra.get("registry").and_then(|value| value.as_str()),
            Some("https://registry.bazel.build")
        );
        assert_eq!(
            dep_extra
                .get("max_compatibility_level")
                .and_then(|value| value.as_i64()),
            Some(3)
        );
    }

    #[test]
    fn test_extract_module_ignores_indirect_or_dynamic_calls() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("MODULE.bazel");
        let content = r#"
def wrapper(**kwargs):
    module(**kwargs)

wrapper(name = "not_detected", version = "1.0.0")

name = "rules_python"
bazel_dep(name = name, version = "0.24.0")
"#;
        fs::write(&file_path, content).unwrap();

        let package = BazelModuleParser::extract_first_package(&file_path);
        assert!(package.name.is_none());
        assert!(package.dependencies.is_empty());
    }

    #[test]
    fn test_extract_invalid_module_returns_default() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("MODULE.bazel");
        fs::write(&file_path, "not valid starlark(").unwrap();

        let package = BazelModuleParser::extract_first_package(&file_path);
        assert_eq!(package.datasource_id, Some(DatasourceId::BazelModule));
        assert!(package.name.is_none());
        assert!(package.dependencies.is_empty());
    }
}
