mod tests {
    use crate::models::Dependency;
    use crate::models::PackageType;
    use crate::models::Sha256Digest;
    use crate::parsers::{PackageParser, PubspecLockParser, PubspecYamlParser};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_temp_file(file_name: &str, content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("Failed to write temp file");
        (temp_dir, file_path)
    }

    fn find_dependency<'a>(dependencies: &'a [Dependency], name: &str) -> Option<&'a Dependency> {
        let needle = format!("/{}", name);
        dependencies.iter().find(|dep| {
            dep.purl
                .as_deref()
                .is_some_and(|purl| purl.contains(&needle))
        })
    }

    #[test]
    fn test_pubspec_yaml_is_match() {
        assert!(PubspecYamlParser::is_match(&PathBuf::from("pubspec.yaml")));
        assert!(!PubspecYamlParser::is_match(&PathBuf::from("pubspec.lock")));
    }

    #[test]
    fn test_pubspec_lock_is_match() {
        assert!(PubspecLockParser::is_match(&PathBuf::from("pubspec.lock")));
        assert!(!PubspecLockParser::is_match(&PathBuf::from("pubspec.yaml")));
    }

    #[test]
    fn test_extract_simple_dependencies() {
        let content = r#"
name: example
version: 1.2.3
description: Example package
homepage: https://example.com
issue_tracker: https://example.com/issues
repository: https://github.com/example/example
dependencies:
  http: ^0.13.0
  path: 1.8.0
dependency_overrides:
  matcher: ^0.12.0
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(package_data.package_type, Some(PackageType::Dart));
        assert_eq!(package_data.name.as_deref(), Some("example"));
        assert_eq!(package_data.version.as_deref(), Some("1.2.3"));
        assert_eq!(package_data.description.as_deref(), Some("Example package"));
        assert_eq!(
            package_data.homepage_url.as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            package_data.bug_tracking_url.as_deref(),
            Some("https://example.com/issues")
        );
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("https://github.com/example/example")
        );
        assert_eq!(package_data.purl.as_deref(), Some("pkg:dart/example@1.2.3"));
        assert_eq!(package_data.dependencies.len(), 3);

        let http_dep = find_dependency(&package_data.dependencies, "http")
            .expect("http dependency should be present");
        assert_eq!(http_dep.extracted_requirement.as_deref(), Some("^0.13.0"));
        assert_eq!(http_dep.scope.as_deref(), Some("dependencies"));
        assert_eq!(http_dep.is_runtime, Some(true));
        assert_eq!(http_dep.is_optional, Some(false));
        assert_eq!(http_dep.is_pinned, Some(false));
        assert_eq!(http_dep.is_direct, Some(true));

        let path_dep = find_dependency(&package_data.dependencies, "path")
            .expect("path dependency should be present");
        assert_eq!(path_dep.extracted_requirement.as_deref(), Some("1.8.0"));
        assert_eq!(path_dep.is_pinned, Some(true));
        assert!(
            path_dep
                .purl
                .as_deref()
                .is_some_and(|purl| purl.ends_with("@1.8.0"))
        );

        let override_dep = find_dependency(&package_data.dependencies, "matcher")
            .expect("dependency override should be present");
        assert_eq!(
            override_dep.extracted_requirement.as_deref(),
            Some("^0.12.0")
        );
        assert_eq!(override_dep.scope.as_deref(), Some("dependency_overrides"));
        assert_eq!(override_dep.is_runtime, Some(true));
        assert_eq!(override_dep.is_optional, Some(false));
        assert_eq!(override_dep.is_direct, Some(true));
    }

    #[test]
    fn test_extract_dev_dependencies() {
        let content = r#"
name: example
dev_dependencies:
  test: ^1.0.0
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        let dev_dep = find_dependency(&package_data.dependencies, "test")
            .expect("dev dependency should be present");
        assert_eq!(dev_dep.extracted_requirement.as_deref(), Some("^1.0.0"));
        assert_eq!(dev_dep.scope.as_deref(), Some("dev_dependencies"));
        assert_eq!(dev_dep.is_runtime, Some(false));
        assert_eq!(dev_dep.is_optional, Some(true));
        assert_eq!(dev_dep.is_pinned, Some(false));
        assert_eq!(dev_dep.is_direct, Some(true));
    }

    #[test]
    fn test_extract_sha256_hashes() {
        let content = r#"
packages:
  foo:
    dependency: "direct main"
    description:
      name: foo
      url: "https://pub.dev"
      sha256: "abc1230000000000000000000000000000000000000000000000000000000000"
    source: hosted
    version: "1.0.0"
  bar:
    dependency: transitive
    description:
      name: bar
      url: "https://pub.dev"
      sha256: "def4560000000000000000000000000000000000000000000000000000000000"
    source: hosted
    version: "2.0.0"
"#;

        let (_temp_dir, lock_path) = create_temp_file("pubspec.lock", content);
        let package_data = PubspecLockParser::extract_first_package(&lock_path);

        let foo_dep = find_dependency(&package_data.dependencies, "foo")
            .expect("foo dependency should be present");
        let foo_resolved = foo_dep
            .resolved_package
            .as_ref()
            .expect("foo should have resolved package");
        assert_eq!(
            foo_resolved.sha256,
            Some(
                Sha256Digest::from_hex(
                    "abc1230000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            )
        );

        let bar_dep = find_dependency(&package_data.dependencies, "bar")
            .expect("bar dependency should be present");
        let bar_resolved = bar_dep
            .resolved_package
            .as_ref()
            .expect("bar should have resolved package");
        assert_eq!(
            bar_resolved.sha256,
            Some(
                Sha256Digest::from_hex(
                    "def4560000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn test_extract_dependency_tree() {
        let content = r#"
packages:
  foo:
    dependency: "direct main"
    description:
      name: foo
      url: "https://pub.dev"
    source: hosted
    version: "1.0.0"
    dependencies:
      bar: 2.0.0
      baz: ^3.0.0
  bar:
    dependency: transitive
    description:
      name: bar
      url: "https://pub.dev"
    source: hosted
    version: "2.0.0"
  baz:
    dependency: transitive
    description:
      name: baz
      url: "https://pub.dev"
    source: hosted
    version: "3.1.0"
"#;

        let (_temp_dir, lock_path) = create_temp_file("pubspec.lock", content);
        let package_data = PubspecLockParser::extract_first_package(&lock_path);

        let foo_dep = find_dependency(&package_data.dependencies, "foo")
            .expect("foo dependency should be present");
        let foo_resolved = foo_dep
            .resolved_package
            .as_ref()
            .expect("foo should have resolved package");

        assert_eq!(foo_resolved.dependencies.len(), 2);
        let bar_dep = find_dependency(&foo_resolved.dependencies, "bar")
            .expect("bar should be listed as a dependency");
        assert_eq!(bar_dep.extracted_requirement.as_deref(), Some("2.0.0"));
        assert_eq!(bar_dep.is_pinned, Some(true));

        let baz_dep = find_dependency(&foo_resolved.dependencies, "baz")
            .expect("baz should be listed as a dependency");
        assert_eq!(baz_dep.extracted_requirement.as_deref(), Some("^3.0.0"));
        assert_eq!(baz_dep.is_pinned, Some(false));
    }

    #[test]
    fn test_graceful_error_handling() {
        let content = "[invalid_yaml";

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(package_data.package_type, Some(PackageType::Dart));
        assert_eq!(
            package_data.datasource_id,
            Some(crate::models::DatasourceId::PubspecYaml)
        );
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_pubspec_issue_tracker_to_bug_tracking_url() {
        let content = r#"
name: issue-demo
version: 0.1.0
issue_tracker: https://example.com/bugs
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(
            package_data.bug_tracking_url.as_deref(),
            Some("https://example.com/bugs")
        );
    }

    #[test]
    fn test_pubspec_lock_error_keeps_lock_identity() {
        let content = "[invalid_yaml";

        let (_temp_dir, lock_path) = create_temp_file("pubspec.lock", content);
        let package_data = PubspecLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pubspec));
        assert_eq!(
            package_data.datasource_id,
            Some(crate::models::DatasourceId::PubspecLock)
        );
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_no_unwrap_no_expect() {
        let dart_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("parsers")
            .join("dart.rs");
        let content = fs::read_to_string(&dart_path).expect("Failed to read dart.rs");
        assert!(!content.contains(".unwrap()"));
        assert!(!content.contains(".expect("));
    }

    #[test]
    fn test_extract_environment_dependencies() {
        let content = r#"
name: example
version: 1.0.0
environment:
  sdk: '>=2.12.0 <3.0.0'
  flutter: '>=2.0.0'
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        let sdk_dep = find_dependency(&package_data.dependencies, "sdk")
            .expect("sdk environment dependency should be present");
        assert_eq!(
            sdk_dep.extracted_requirement.as_deref(),
            Some(">=2.12.0 <3.0.0")
        );
        assert_eq!(sdk_dep.scope.as_deref(), Some("environment"));
        assert_eq!(sdk_dep.is_runtime, Some(true));
        assert_eq!(sdk_dep.is_optional, Some(false));

        let flutter_dep = find_dependency(&package_data.dependencies, "flutter")
            .expect("flutter environment dependency should be present");
        assert_eq!(
            flutter_dep.extracted_requirement.as_deref(),
            Some(">=2.0.0")
        );
        assert_eq!(flutter_dep.scope.as_deref(), Some("environment"));
    }

    #[test]
    fn test_extract_single_author() {
        let content = r#"
name: example
author: John Doe <john@example.com>
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(package_data.parties.len(), 1);
        let author = &package_data.parties[0];
        assert_eq!(author.role.as_deref(), Some("author"));
        assert_eq!(author.name.as_deref(), Some("John Doe <john@example.com>"));
    }

    #[test]
    fn test_extract_multiple_authors() {
        let content = r#"
name: example
authors:
  - Alice Smith
  - Bob Jones
  - Carol Williams
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(package_data.parties.len(), 3);
        assert_eq!(package_data.parties[0].name.as_deref(), Some("Alice Smith"));
        assert_eq!(package_data.parties[1].name.as_deref(), Some("Bob Jones"));
        assert_eq!(
            package_data.parties[2].name.as_deref(),
            Some("Carol Williams")
        );

        for party in &package_data.parties {
            assert_eq!(party.role.as_deref(), Some("author"));
        }
    }

    #[test]
    fn test_extract_repository_vcs_url() {
        let content = r#"
name: example
repository: https://github.com/example/repo
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("https://github.com/example/repo")
        );
    }

    #[test]
    fn test_extract_extra_data_fields() {
        let content = r#"
name: example
issue_tracker: https://github.com/example/repo/issues
documentation: https://example.com/docs
publish_to: https://custom-pub.dev
executables:
  example_cli: main
  tool: bin/tool
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present");

        assert_eq!(
            extra_data.get("issue_tracker").and_then(|v| v.as_str()),
            Some("https://github.com/example/repo/issues")
        );
        assert_eq!(
            extra_data.get("documentation").and_then(|v| v.as_str()),
            Some("https://example.com/docs")
        );
        assert_eq!(
            extra_data.get("publish_to").and_then(|v| v.as_str()),
            Some("https://custom-pub.dev")
        );

        let executables = extra_data
            .get("executables")
            .expect("executables should be present");
        assert!(executables.is_object());
        assert_eq!(
            executables.get("example_cli").and_then(|v| v.as_str()),
            Some("main")
        );
        assert_eq!(
            executables.get("tool").and_then(|v| v.as_str()),
            Some("bin/tool")
        );
    }

    #[test]
    fn test_publish_to_none_marks_package_private_and_surfaces_metadata_fields() {
        let content = r#"
name: example
version: 1.0.0
publish_to: none
platforms:
  android:
  ios:
funding:
  - https://example.com/sponsor
topics:
  - widgets
  - ui
screenshots:
  - description: home screen
    path: screenshots/home.png
false_secrets:
  - test/fixtures/**
ignored_advisories:
  - GHSA-1234-5678-90ab
archive_url: https://mirror.example.com/example-1.0.0.tar.gz
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert!(package_data.is_private);
        assert_eq!(
            package_data.download_url.as_deref(),
            Some("https://mirror.example.com/example-1.0.0.tar.gz")
        );
        assert_eq!(package_data.keywords, vec!["widgets", "ui"]);

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert!(extra_data.contains_key("platforms"));
        assert!(extra_data.contains_key("funding"));
        assert!(extra_data.contains_key("topics"));
        assert!(extra_data.contains_key("screenshots"));
        assert!(extra_data.contains_key("false_secrets"));
        assert!(extra_data.contains_key("ignored_advisories"));
    }

    #[test]
    fn test_manifest_dependency_descriptors_are_preserved() {
        let content = r#"
name: example
dependencies:
  my_hosted:
    hosted:
      name: actual_pkg
      url: https://pub.example.com
    version: ^1.2.0
  my_git:
    git:
      url: https://github.com/example/repo.git
      ref: main
      path: pkgs/example
  flutter:
    sdk: flutter
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        let hosted = find_dependency(&package_data.dependencies, "my_hosted")
            .expect("hosted dependency should exist");
        assert_eq!(
            hosted.extracted_requirement.as_deref(),
            Some("hosted: name: actual_pkg, url: https://pub.example.com, version: ^1.2.0")
        );
        assert!(
            hosted
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("hosted"))
                .is_some()
        );

        let git = find_dependency(&package_data.dependencies, "my_git")
            .expect("git dependency should exist");
        assert!(
            git.extra_data
                .as_ref()
                .and_then(|extra| extra.get("git"))
                .is_some()
        );

        let flutter = find_dependency(&package_data.dependencies, "flutter")
            .expect("sdk dependency should exist");
        assert!(
            flutter
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("sdk"))
                .is_some()
        );
    }

    #[test]
    fn test_pubspec_lock_classifies_direct_dev_and_dev_only_transitives() {
        let content = r#"
sdks:
  dart: ">=3.0.0 <4.0.0"
packages:
  build_runner:
    dependency: "direct dev"
    description:
      name: build_runner
      url: "https://pub.dev"
    source: hosted
    version: "2.4.0"
    dependencies:
      watcher: "1.0.0"
  http:
    dependency: "direct main"
    description:
      name: http
      url: "https://pub.dev"
    source: hosted
    version: "1.1.0"
    dependencies:
      async: "2.11.0"
  watcher:
    dependency: transitive
    description:
      name: watcher
      url: "https://pub.dev"
    source: hosted
    version: "1.0.0"
  async:
    dependency: transitive
    description:
      name: async
      url: "https://pub.dev"
    source: hosted
    version: "2.11.0"
"#;

        let (_temp_dir, lock_path) = create_temp_file("pubspec.lock", content);
        let package_data = PubspecLockParser::extract_first_package(&lock_path);

        let build_runner = find_dependency(&package_data.dependencies, "build_runner")
            .expect("direct dev dep missing");
        assert_eq!(build_runner.is_direct, Some(true));
        assert_eq!(build_runner.is_runtime, Some(false));
        assert_eq!(build_runner.is_optional, Some(true));

        let watcher = find_dependency(&package_data.dependencies, "watcher")
            .expect("dev-only transitive dep missing");
        assert_eq!(watcher.is_direct, Some(false));
        assert_eq!(watcher.is_runtime, Some(false));
        assert_eq!(watcher.is_optional, Some(true));

        let async_dep = find_dependency(&package_data.dependencies, "async")
            .expect("runtime transitive dep missing");
        assert_eq!(async_dep.is_direct, Some(false));
        assert_eq!(async_dep.is_runtime, Some(true));
        assert_eq!(async_dep.is_optional, Some(false));
    }

    #[test]
    fn test_pubspec_lock_preserves_path_source_and_legacy_sdk() {
        let content = r#"
sdk: ">=2.12.0 <3.0.0"
packages:
  pub_dartdoc_data:
    dependency: "direct main"
    description:
      path: "../pub_dartdoc_data"
      relative: true
    source: path
    version: "0.0.0"
"#;

        let (_temp_dir, lock_path) = create_temp_file("pubspec.lock", content);
        let package_data = PubspecLockParser::extract_first_package(&lock_path);

        let sdk_dep =
            find_dependency(&package_data.dependencies, "dart").expect("legacy sdk dep missing");
        assert_eq!(sdk_dep.scope.as_deref(), Some("sdk"));
        assert_eq!(
            sdk_dep.extracted_requirement.as_deref(),
            Some(">=2.12.0 <3.0.0")
        );

        let path_dep = find_dependency(&package_data.dependencies, "pub_dartdoc_data")
            .expect("path dependency missing");
        let extra = path_dep
            .extra_data
            .as_ref()
            .expect("path extra_data should exist");
        assert_eq!(extra.get("source").and_then(|v| v.as_str()), Some("path"));
        assert!(extra.get("description").is_some());
        let resolved = path_dep
            .resolved_package
            .as_ref()
            .expect("resolved package missing");
        assert!(
            resolved
                .extra_data
                .as_ref()
                .and_then(|v| v.get("description"))
                .is_some()
        );
    }

    #[test]
    fn test_pubspec_lock_sdks() {
        let content = r#"
sdks:
  dart: ">=2.19.0 <4.0.0"
  flutter: ">=3.3.0"
packages:
  http:
    dependency: "direct main"
    description:
      name: http
      sha256: "abc123"
      url: "https://pub.dev"
    source: hosted
    version: "1.1.0"
"#;

        let (_temp_dir, lock_path) = create_temp_file("pubspec.lock", content);
        let package_data = PubspecLockParser::extract_first_package(&lock_path);

        let dart_sdk = find_dependency(&package_data.dependencies, "dart")
            .expect("dart SDK should be present");
        assert_eq!(
            dart_sdk.extracted_requirement.as_deref(),
            Some(">=2.19.0 <4.0.0")
        );
        assert_eq!(dart_sdk.scope.as_deref(), Some("sdk"));
        assert_eq!(dart_sdk.is_runtime, Some(true));

        let flutter_sdk = find_dependency(&package_data.dependencies, "flutter")
            .expect("flutter SDK should be present");
        assert_eq!(
            flutter_sdk.extracted_requirement.as_deref(),
            Some(">=3.3.0")
        );
        assert_eq!(flutter_sdk.scope.as_deref(), Some("sdk"));
    }

    #[test]
    fn test_generated_urls() {
        let content = r#"
name: my_package
version: 1.2.3
description: Test package for URL generation
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(
            package_data.api_data_url.as_deref(),
            Some("https://pub.dev/api/packages/my_package/versions/1.2.3")
        );
        assert_eq!(
            package_data.repository_homepage_url.as_deref(),
            Some("https://pub.dev/packages/my_package/versions/1.2.3")
        );
        assert_eq!(
            package_data.repository_download_url.as_deref(),
            Some("https://pub.dartlang.org/packages/my_package/versions/1.2.3.tar.gz")
        );
        assert_eq!(
            package_data.download_url.as_deref(),
            Some("https://pub.dartlang.org/packages/my_package/versions/1.2.3.tar.gz")
        );
    }

    #[test]
    fn test_urls_not_generated_without_name_or_version() {
        let content_no_version = r#"
name: my_package
description: Package without version
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content_no_version);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert!(package_data.api_data_url.is_none());
        assert!(package_data.repository_homepage_url.is_none());
        assert!(package_data.repository_download_url.is_none());
        assert!(package_data.download_url.is_none());

        let content_no_name = r#"
version: 1.0.0
description: Package without name
"#;

        let (_temp_dir2, pubspec_path2) = create_temp_file("pubspec.yaml", content_no_name);
        let package_data2 = PubspecYamlParser::extract_first_package(&pubspec_path2);

        assert!(package_data2.api_data_url.is_none());
        assert!(package_data2.repository_homepage_url.is_none());
        assert!(package_data2.repository_download_url.is_none());
        assert!(package_data2.download_url.is_none());
    }

    #[test]
    fn test_all_features_combined() {
        let content = r#"
name: full_example
version: 2.3.4
description: A comprehensive test package
homepage: https://example.com
repository: https://github.com/example/full_example
author: Original Author
authors:
  - Contributor One
  - Contributor Two
issue_tracker: https://github.com/example/full_example/issues
documentation: https://example.com/docs/full_example
publish_to: none
environment:
  sdk: '>=2.18.0 <4.0.0'
  flutter: '>=3.0.0'
dependencies:
  http: ^1.0.0
dev_dependencies:
  test: ^1.24.0
dependency_overrides:
  path: 1.9.0
executables:
  example: main
"#;

        let (_temp_dir, pubspec_path) = create_temp_file("pubspec.yaml", content);
        let package_data = PubspecYamlParser::extract_first_package(&pubspec_path);

        assert_eq!(package_data.name.as_deref(), Some("full_example"));
        assert_eq!(package_data.version.as_deref(), Some("2.3.4"));
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("https://github.com/example/full_example")
        );

        assert_eq!(package_data.parties.len(), 3);
        assert_eq!(
            package_data.parties[0].name.as_deref(),
            Some("Original Author")
        );
        assert_eq!(
            package_data.parties[1].name.as_deref(),
            Some("Contributor One")
        );
        assert_eq!(
            package_data.parties[2].name.as_deref(),
            Some("Contributor Two")
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should be present");
        assert_eq!(
            extra_data.get("issue_tracker").and_then(|v| v.as_str()),
            Some("https://github.com/example/full_example/issues")
        );
        assert_eq!(
            extra_data.get("documentation").and_then(|v| v.as_str()),
            Some("https://example.com/docs/full_example")
        );
        assert_eq!(
            extra_data.get("publish_to").and_then(|v| v.as_str()),
            Some("none")
        );

        let sdk_dep = find_dependency(&package_data.dependencies, "sdk");
        assert!(sdk_dep.is_some());

        let http_dep = find_dependency(&package_data.dependencies, "http");
        assert!(http_dep.is_some());

        let test_dep = find_dependency(&package_data.dependencies, "test");
        assert!(test_dep.is_some());

        let override_dep = find_dependency(&package_data.dependencies, "path");
        assert!(override_dep.is_some());
    }
}
