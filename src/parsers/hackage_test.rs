// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::models::{DatasourceId, Dependency, PackageType};
    use crate::parsers::{
        HackageCabalParser, HackageCabalProjectParser, HackageStackYamlParser, PackageParser,
    };

    fn create_temp_file(file_name: &str, content: &str) -> (TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().expect("failed to create tempdir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("failed to write temp file");
        (temp_dir, file_path)
    }

    fn find_dependency<'a>(dependencies: &'a [Dependency], needle: &str) -> Option<&'a Dependency> {
        dependencies.iter().find(|dependency| {
            dependency
                .purl
                .as_deref()
                .is_some_and(|purl| purl.contains(needle))
                || dependency.extracted_requirement.as_deref() == Some(needle)
        })
    }

    #[test]
    fn test_is_match() {
        assert!(HackageCabalParser::is_match(Path::new("example.cabal")));
        assert!(!HackageCabalParser::is_match(Path::new("cabal.project")));

        assert!(HackageCabalProjectParser::is_match(Path::new(
            "cabal.project"
        )));
        assert!(!HackageCabalProjectParser::is_match(Path::new(
            "stack.yaml"
        )));

        assert!(HackageStackYamlParser::is_match(Path::new("stack.yaml")));
        assert!(!HackageStackYamlParser::is_match(Path::new("stack.yml")));
    }

    #[test]
    fn test_parse_cabal_extracts_metadata_and_component_dependencies() {
        let content = r#"
cabal-version:      3.0
name:               example-hackage
version:            0.1.0.0
synopsis:           Example Hackage package
description:
  Example Hackage package.
  .
  More details here.
license:            BSD-3-Clause
homepage:           https://example.com/example-hackage
bug-reports:        https://example.com/example-hackage/issues
author:             Alice Example <alice@example.com>, Bob Example
maintainer:         Carol Maintainer <carol@example.com>
category:           Web, CLI
keywords:           parser, haskell
source-repository head
  type: git
  location: https://github.com/example/example-hackage.git

library
  build-depends:
      base >=4.14 && <5,
      text ==1.2.5.0

test-suite example-test
  type: exitcode-stdio-1.0
  build-depends:
      hspec >=2.10
"#;

        let (_temp_dir, file_path) = create_temp_file("example-hackage.cabal", content);
        let package_data = HackageCabalParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Hackage));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::HackageCabal));
        assert_eq!(package_data.name.as_deref(), Some("example-hackage"));
        assert_eq!(package_data.version.as_deref(), Some("0.1.0.0"));
        assert_eq!(package_data.primary_language.as_deref(), Some("Haskell"));
        assert_eq!(
            package_data.description.as_deref(),
            Some("Example Hackage package\n\nExample Hackage package.\n\nMore details here.")
        );
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("BSD-3-Clause")
        );
        assert_eq!(
            package_data.homepage_url.as_deref(),
            Some("https://example.com/example-hackage")
        );
        assert_eq!(
            package_data.bug_tracking_url.as_deref(),
            Some("https://example.com/example-hackage/issues")
        );
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("https://github.com/example/example-hackage.git")
        );
        assert_eq!(
            package_data.keywords,
            vec!["Web", "CLI", "parser", "haskell"]
        );
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:hackage/example-hackage@0.1.0.0")
        );
        assert_eq!(package_data.parties.len(), 3);

        let text_dep = find_dependency(&package_data.dependencies, "/text")
            .expect("text dependency should exist");
        assert_eq!(text_dep.purl.as_deref(), Some("pkg:hackage/text@1.2.5.0"));
        assert_eq!(text_dep.extracted_requirement.as_deref(), Some("==1.2.5.0"));
        assert_eq!(text_dep.scope.as_deref(), Some("build-depends"));
        assert_eq!(text_dep.is_pinned, Some(true));
        assert_eq!(
            text_dep
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("component_type"))
                .and_then(|value| value.as_str()),
            Some("library")
        );

        let hspec_dep = find_dependency(&package_data.dependencies, "/hspec")
            .expect("hspec dependency should exist");
        assert_eq!(hspec_dep.is_runtime, Some(false));
        assert_eq!(
            hspec_dep
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("component_name"))
                .and_then(|value| value.as_str()),
            Some("example-test")
        );
    }

    #[test]
    fn test_parse_cabal_project_extracts_surfaces_and_preserves_other_config() {
        let content = r#"
packages:
  ./*.cabal
  app/*.cabal
optional-packages:
  vendor/*.cabal
extra-packages:
  lens-5.2.1
  text ==1.2.5.0
import: ./shared.project

source-repository-package
  type: git
  location: https://github.com/commercialhaskell/pantry.git
  tag: 0123456789abcdef
  subdir:
    pantry
    pantry-test

allow-newer: true
constraints:
  aeson >= 2.0
index-state: 2024-01-01T00:00:00Z
"#;

        let (_temp_dir, file_path) = create_temp_file("cabal.project", content);
        let package_data = HackageCabalProjectParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Hackage));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::HackageCabalProject)
        );
        assert_eq!(package_data.dependencies.len(), 7);

        let lens_dep = find_dependency(&package_data.dependencies, "/lens")
            .expect("lens extra-package should exist");
        assert_eq!(lens_dep.purl.as_deref(), Some("pkg:hackage/lens@5.2.1"));
        assert_eq!(lens_dep.scope.as_deref(), Some("extra-packages"));
        assert_eq!(lens_dep.is_pinned, Some(true));

        let import_dep = find_dependency(&package_data.dependencies, "./shared.project")
            .expect("import entry should exist");
        assert_eq!(import_dep.scope.as_deref(), Some("import"));

        let source_dep = package_data
            .dependencies
            .iter()
            .find(|dependency| dependency.scope.as_deref() == Some("source-repository-package"))
            .expect("source-repository-package dependency should exist");
        assert_eq!(source_dep.is_pinned, Some(true));
        assert_eq!(
            source_dep
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("subdir"))
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(2)
        );

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        assert_eq!(
            extra_data
                .get("allow_newer")
                .and_then(|value| value.as_str()),
            Some("true")
        );
        assert_eq!(
            extra_data
                .get("constraints")
                .and_then(|value| value.as_str()),
            Some("aeson >= 2.0")
        );
        assert_eq!(
            extra_data
                .get("index_state")
                .and_then(|value| value.as_str()),
            Some("2024-01-01T00:00:00Z")
        );
    }

    #[test]
    fn test_parse_stack_yaml_extracts_dependencies_and_preserves_config() {
        let content = r#"
resolver: lts-22.10
snapshot: nightly-2024-02-01
packages:
  - .
  - subdir/package-a
  - location: https://github.com/commercialhaskell/pantry.git
    extra-dep: true
    subdirs:
      - pantry
extra-deps:
  - aeson-2.2.1.0
  - text-2.0.2@sha256:abcdef,1234
  - git: https://github.com/haskell/wai.git
    commit: deadbeefcafebabe
    subdirs:
      - wai
flags:
  wai:
    warp: true
drop-packages:
  - old-package
ghc-options:
  "$locals": -Wall
"#;

        let (_temp_dir, file_path) = create_temp_file("stack.yaml", content);
        let package_data = HackageStackYamlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Hackage));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::HackageStackYaml)
        );
        assert_eq!(package_data.dependencies.len(), 6);

        let aeson_dep = find_dependency(&package_data.dependencies, "/aeson")
            .expect("aeson extra-dep should exist");
        assert_eq!(aeson_dep.purl.as_deref(), Some("pkg:hackage/aeson@2.2.1.0"));
        assert_eq!(aeson_dep.scope.as_deref(), Some("extra-deps"));
        assert_eq!(aeson_dep.is_pinned, Some(true));

        let pantry_dep = find_dependency(
            &package_data.dependencies,
            "https://github.com/commercialhaskell/pantry.git",
        )
        .expect("package location dependency should exist");
        assert_eq!(pantry_dep.scope.as_deref(), Some("packages"));

        let text_dep = find_dependency(&package_data.dependencies, "/text")
            .expect("text pantry dependency should exist");
        assert_eq!(text_dep.is_pinned, Some(true));
        assert_eq!(
            text_dep
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("pantry"))
                .and_then(|value| value.as_str()),
            Some("sha256:abcdef,1234")
        );

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        assert_eq!(
            extra_data.get("resolver").and_then(|value| value.as_str()),
            Some("lts-22.10")
        );
        assert_eq!(
            extra_data.get("snapshot").and_then(|value| value.as_str()),
            Some("nightly-2024-02-01")
        );
        assert!(extra_data.contains_key("flags"));
        assert!(extra_data.contains_key("drop-packages"));
        assert!(extra_data.contains_key("ghc-options"));
    }

    #[test]
    fn test_invalid_stack_yaml_returns_default_package() {
        let (_temp_dir, file_path) = create_temp_file("stack.yaml", "[invalid_yaml");
        let package_data = HackageStackYamlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Hackage));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::HackageStackYaml)
        );
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_common_stanza_dependencies_are_not_emitted_as_direct_dependencies() {
        let content = r#"
cabal-version: 3.0
name: common-demo
version: 0.1.0.0

common shared-deps
  build-depends:
    aeson >= 2.0

library
  import: shared-deps
  build-depends:
    base >= 4.14
"#;

        let (_temp_dir, file_path) = create_temp_file("common-demo.cabal", content);
        let package_data = HackageCabalParser::extract_first_package(&file_path);

        assert!(
            find_dependency(&package_data.dependencies, "/aeson").is_none(),
            "common stanza dependency should not be emitted directly"
        );
        assert!(
            find_dependency(&package_data.dependencies, "/base").is_some(),
            "real component dependency should still be emitted"
        );
    }

    #[test]
    fn test_numeric_suffix_package_name_is_not_misread_as_name_version_pair() {
        let content = r#"
cabal-version: 3.0
name: numeric-suffix-demo
version: 0.1.0.0

library
  build-depends:
    foo-9p,
    base >= 4.14
"#;

        let (_temp_dir, file_path) = create_temp_file("numeric-suffix-demo.cabal", content);
        let package_data = HackageCabalParser::extract_first_package(&file_path);

        let dependency = find_dependency(&package_data.dependencies, "pkg:hackage/foo-9p")
            .expect("numeric suffix package should be preserved as the package name");
        assert_eq!(dependency.purl.as_deref(), Some("pkg:hackage/foo-9p"));
        assert_eq!(dependency.extracted_requirement, None);
        assert_eq!(dependency.is_pinned, Some(false));
    }
}
