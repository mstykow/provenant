// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{MesonParser, PackageParser};

    fn create_temp_meson_build(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let meson_build = temp_dir.path().join("meson.build");
        fs::write(&meson_build, content).expect("Failed to write meson.build");
        (temp_dir, meson_build)
    }

    #[test]
    fn test_is_match_meson_build_only() {
        assert!(MesonParser::is_match(&PathBuf::from("/repo/meson.build")));
        assert!(!MesonParser::is_match(&PathBuf::from(
            "/repo/meson_options.txt"
        )));
        assert!(!MesonParser::is_match(&PathBuf::from("/repo/subdir.build")));
        assert!(!MesonParser::is_match(&PathBuf::from(
            "/repo/meson.build.in"
        )));
    }

    #[test]
    fn test_extract_literal_project_metadata_and_dependencies() {
        let content = r#"
project(
  'demo-project',
  ['c', 'cpp'],
  version: '1.2.3',
  license: ['BSD-2-Clause', 'GPL-2.0-or-later'],
  license_files: ['COPYING', 'LICENSES/BSD'],
  meson_version: '>=0.47.0',
)

zlib_dep = dependency('zlib', version: '>=1.2.8')
threads_dep = dependency(
  'threads',
  required: false,
  method: 'system',
  modules: ['core', 'extra'],
  fallback: ['threads-subproject', 'threads_dep'],
  native: true,
)
dependency('libarchive', version: ['>=3.0.0', '<4.0.0'])
        "#;

        let (_temp_dir, path) = create_temp_meson_build(content);
        let package_data = MesonParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Meson));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::MesonBuild));
        assert_eq!(package_data.name.as_deref(), Some("demo-project"));
        assert_eq!(package_data.version.as_deref(), Some("1.2.3"));
        assert_eq!(package_data.primary_language.as_deref(), Some("c"));
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("BSD-2-Clause\nGPL-2.0-or-later")
        );
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:meson/demo-project@1.2.3")
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing extra_data");
        assert_eq!(
            extra_data
                .get("meson_version")
                .and_then(|value| value.as_str()),
            Some(">=0.47.0")
        );
        assert_eq!(
            extra_data
                .get("languages")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>()
                }),
            Some(vec!["c", "cpp"])
        );
        assert_eq!(
            extra_data
                .get("license_files")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>()
                }),
            Some(vec!["COPYING", "LICENSES/BSD"])
        );
        let referenced_filenames = package_data.license_detections[0].matches[0]
            .referenced_filenames
            .as_ref()
            .expect("referenced filenames should be present");
        assert_eq!(
            referenced_filenames,
            &vec!["COPYING".to_string(), "LICENSES/BSD".to_string()]
        );

        assert_eq!(package_data.dependencies.len(), 3);

        let zlib = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:generic/meson/zlib"))
            .expect("zlib dependency missing");
        assert_eq!(zlib.extracted_requirement.as_deref(), Some(">=1.2.8"));
        assert_eq!(zlib.scope.as_deref(), Some("dependencies"));
        assert_eq!(zlib.is_runtime, Some(true));
        assert_eq!(zlib.is_optional, Some(false));
        assert_eq!(zlib.is_pinned, Some(false));
        assert_eq!(zlib.is_direct, Some(true));

        let threads = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:generic/meson/threads"))
            .expect("threads dependency missing");
        assert_eq!(threads.extracted_requirement, None);
        assert_eq!(threads.is_runtime, Some(false));
        assert_eq!(threads.is_optional, Some(true));
        let threads_extra = threads
            .extra_data
            .as_ref()
            .expect("threads extra_data missing");
        assert_eq!(
            threads_extra.get("method").and_then(|value| value.as_str()),
            Some("system")
        );
        assert_eq!(
            threads_extra
                .get("native")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            threads_extra
                .get("modules")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>()
                }),
            Some(vec!["core", "extra"])
        );
        assert_eq!(
            threads_extra
                .get("fallback")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>()
                }),
            Some(vec!["threads-subproject", "threads_dep"])
        );

        let libarchive = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:generic/meson/libarchive"))
            .expect("libarchive dependency missing");
        assert_eq!(
            libarchive.extracted_requirement.as_deref(),
            Some(">=3.0.0, <4.0.0")
        );
    }

    #[test]
    fn test_skips_non_literal_constructs_and_nested_dependencies() {
        let content = r#"
project(
  'guardrails',
  'c',
  version: version_value,
  license: license_value,
)

version_value = '9.9.9'
license_value = 'Apache-2.0'

direct_dep = dependency('libcurl', required: false)
computed_dep = dependency(dep_name)

if get_option('use_ssl')
  dependency('openssl', version: '>=3')
endif
        "#;

        let (_temp_dir, path) = create_temp_meson_build(content);
        let package_data = MesonParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("guardrails"));
        assert_eq!(package_data.primary_language.as_deref(), Some("c"));
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.extracted_license_statement, None);
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:generic/meson/libcurl")
        );
        assert_eq!(package_data.dependencies[0].is_optional, Some(true));
    }

    #[test]
    fn test_single_literal_license_uses_shared_normalization() {
        let content = r#"
project(
  'licensed-project',
  'c',
  version: '1.0.0',
  license: 'Apache-2.0',
)
        "#;

        let (_temp_dir, path) = create_temp_meson_build(content);
        let package_data = MesonParser::extract_first_package(&path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_handles_comments_and_multiline_project_call() {
        let content = r#"
# Leading comment
project(
  'commented', # inline comment
  [
    'cpp',
    'c',
  ],
  version: '2.0.0',
  license: 'MIT',
)

# dependency comment
dependency(
  'fmt',
  version: '10.1.1',
)
        "#;

        let (_temp_dir, path) = create_temp_meson_build(content);
        let package_data = MesonParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("commented"));
        assert_eq!(package_data.primary_language.as_deref(), Some("cpp"));
        assert_eq!(package_data.version.as_deref(), Some("2.0.0"));
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:generic/meson/fmt")
        );
    }

    #[test]
    fn test_skips_unsupported_top_level_noise_after_valid_project() {
        let content = r#"
project('noise-safe', 'c', version: '1.0.0')

dependency('zlib')
answer = 42
meson.project_version().split('.')
dependency('fmt', required: false)
        "#;

        let (_temp_dir, path) = create_temp_meson_build(content);
        let package_data = MesonParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("noise-safe"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(package_data.dependencies.len(), 2);
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:generic/meson/zlib"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:generic/meson/fmt"))
        );
    }

    #[test]
    fn test_extract_malformed_input_returns_default_package() {
        let content = "project('broken', version: ['unterminated'";
        let (_temp_dir, path) = create_temp_meson_build(content);
        let package_data = MesonParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Meson));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::MesonBuild));
    }
}
