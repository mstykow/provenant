// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::parsers::PackageParser;
    use crate::parsers::go::{
        GoModParser, GoSumParser, GodepsParser, parse_go_mod, parse_go_sum, parse_godeps_json,
    };
    use std::path::PathBuf;

    #[test]
    fn test_go_mod_is_match() {
        assert!(GoModParser::is_match(&PathBuf::from("go.mod")));
        assert!(GoModParser::is_match(&PathBuf::from("/some/path/go.mod")));
        assert!(!GoModParser::is_match(&PathBuf::from("Gemfile")));
        assert!(!GoModParser::is_match(&PathBuf::from("go.sum")));
        assert!(!GoModParser::is_match(&PathBuf::from("go.mod.bak")));
        assert!(!GoModParser::is_match(&PathBuf::from("Cargo.toml")));
    }

    #[test]
    fn test_extract_module_declaration() {
        let content = "module github.com/alecthomas/kingpin\n\ngo 1.13\n";
        let result = parse_go_mod(content);

        assert_eq!(result.namespace.as_deref(), Some("github.com/alecthomas"));
        assert_eq!(result.name.as_deref(), Some("kingpin"));
        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.primary_language.as_deref(), Some("Go"));
        assert_eq!(result.datasource_id, Some(DatasourceId::GoMod));
    }

    #[test]
    fn test_extract_module_simple_name() {
        let content = "module app\n";
        let result = parse_go_mod(content);

        assert_eq!(result.namespace, None);
        assert_eq!(result.name.as_deref(), Some("app"));
    }

    #[test]
    fn test_extract_require_single_line() {
        let content = "\
module github.com/alecthomas/sample

require github.com/davecgh/go-spew v1.1.1
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert_eq!(dep.scope.as_deref(), Some("require"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v1.1.1"));
        assert_eq!(dep.is_runtime, Some(true));
        assert_eq!(dep.is_optional, Some(false));
        assert_eq!(dep.is_direct, Some(true));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("golang")));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("go-spew")));
    }

    #[test]
    fn test_extract_require_block() {
        let content = "\
module github.com/alecthomas/kingpin

require (
\tgithub.com/alecthomas/template v0.0.0-20160405071501-a0175ee3bccc
\tgithub.com/alecthomas/units v0.0.0-20151022065526-2efee857e7cf
\tgithub.com/stretchr/testify v1.2.2
)

go 1.13
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 3);
        assert!(
            result
                .dependencies
                .iter()
                .all(|d| d.scope.as_deref() == Some("require"))
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|d| d.is_direct == Some(true))
        );

        let template = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("template")))
            .expect("template dependency not found");
        assert_eq!(
            template.extracted_requirement.as_deref(),
            Some("v0.0.0-20160405071501-a0175ee3bccc")
        );
    }

    #[test]
    fn test_extract_indirect_marker() {
        let content = "\
module github.com/alecthomas/kingpin

require (
\tgithub.com/davecgh/go-spew v1.1.1 // indirect
\tgithub.com/stretchr/testify v1.2.2
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 2);

        let spew = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("go-spew")))
            .expect("go-spew dependency not found");
        assert_eq!(
            spew.is_direct,
            Some(false),
            "indirect dep should have is_direct=false"
        );

        let testify = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("testify")))
            .expect("testify dependency not found");
        assert_eq!(
            testify.is_direct,
            Some(true),
            "direct dep should have is_direct=true"
        );
    }

    #[test]
    fn test_extract_indirect_single_line() {
        let content = "\
module github.com/alecthomas/sample

require github.com/davecgh/go-spew v1.1.1 // indirect
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert_eq!(dep.is_direct, Some(false));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v1.1.1"));
    }

    #[test]
    fn test_extract_pseudo_version() {
        let content = "\
module github.com/example/repo

require (
\tgithub.com/alecthomas/template v0.0.0-20160405071501-a0175ee3bccc
\tgithub.com/alecthomas/units v0.0.0-20151022065526-2efee857e7cf
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 2);

        let template = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("template")))
            .expect("template dependency not found");
        assert_eq!(
            template.extracted_requirement.as_deref(),
            Some("v0.0.0-20160405071501-a0175ee3bccc")
        );

        let units = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("units")))
            .expect("units dependency not found");
        assert_eq!(
            units.extracted_requirement.as_deref(),
            Some("v0.0.0-20151022065526-2efee857e7cf")
        );
    }

    #[test]
    fn test_extract_incompatible_version() {
        let content = "\
module github.com/example/repo

require github.com/old/pkg v2.0.0+incompatible
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert_eq!(
            dep.extracted_requirement.as_deref(),
            Some("v2.0.0+incompatible")
        );
        assert!(dep.purl.is_some());
    }

    #[test]
    fn test_extract_exclude_directive() {
        let content = "\
module github.com/alecthomas/sample

exclude github.com/bad/pkg v1.0.0
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert_eq!(dep.scope.as_deref(), Some("exclude"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v1.0.0"));
        assert_eq!(dep.is_runtime, Some(true));
        assert_eq!(dep.is_optional, Some(false));
    }

    #[test]
    fn test_extract_exclude_block() {
        let content = "\
module github.com/alecthomas/sample

exclude (
\tgithub.com/alecthomas/repr v0.0.0
\tgithub.com/bad/other v1.2.3
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 2);
        assert!(
            result
                .dependencies
                .iter()
                .all(|d| d.scope.as_deref() == Some("exclude"))
        );
    }

    #[test]
    fn test_extract_multiple_requires() {
        let content = "\
module github.com/alecthomas/sample

require github.com/davecgh/go-spew v1.1.1 // indirect
require (
\tgithub.com/stretchr/testify v1.4.0
)
exclude (
\tgithub.com/alecthomas/repr v0.0.0
)

go 1.13
";
        let result = parse_go_mod(content);

        let require_count = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("require"))
            .count();
        let exclude_count = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("exclude"))
            .count();

        assert_eq!(require_count, 2);
        assert_eq!(exclude_count, 1);
        assert_eq!(result.dependencies.len(), 3);
    }

    #[test]
    fn test_extract_empty_go_mod() {
        let content = "";
        let result = parse_go_mod(content);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.name, None);
        assert_eq!(result.namespace, None);
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_graceful_error_handling() {
        let bad_path = PathBuf::from("/nonexistent/path/go.mod");
        let result = GoModParser::extract_first_package(&bad_path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.datasource_id, Some(DatasourceId::GoMod));
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_purl_generation() {
        let content = "\
module github.com/alecthomas/kingpin

require github.com/davecgh/go-spew v1.1.1
";
        let result = parse_go_mod(content);

        assert!(
            result
                .purl
                .as_ref()
                .is_some_and(|p| p.contains("pkg:golang/github.com"))
        );

        let dep = &result.dependencies[0];
        let purl = dep.purl.as_ref().expect("dep should have purl");
        assert!(purl.starts_with("pkg:golang/"));
        assert!(purl.contains("go-spew"));
        assert!(purl.contains("v1.1.1"));
    }

    #[test]
    fn test_version_parsing() {
        let content = "\
module github.com/example/repo

require (
\tgithub.com/simple/dep v1.2.3
\tgithub.com/pseudo/dep v0.0.0-20190101000000-abcd1234
\tgithub.com/incompat/dep v2.0.0+incompatible
\tgithub.com/prerelease/dep v1.0.0-beta.1
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 4);

        let versions: Vec<&str> = result
            .dependencies
            .iter()
            .filter_map(|d| d.extracted_requirement.as_deref())
            .collect();
        assert!(versions.contains(&"v1.2.3"));
        assert!(versions.contains(&"v0.0.0-20190101000000-abcd1234"));
        assert!(versions.contains(&"v2.0.0+incompatible"));
        assert!(versions.contains(&"v1.0.0-beta.1"));
    }

    #[test]
    fn test_comment_handling() {
        let content = "\
// This is a file-level comment
module github.com/example/repo // with inline comment

// This require is commented out
require (
\tgithub.com/foo/bar v1.0.0 // some note
\t// github.com/commented/out v0.0.0
\tgithub.com/baz/qux v2.0.0
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.name.as_deref(), Some("repo"));
        assert_eq!(result.dependencies.len(), 2);
    }

    #[test]
    fn test_url_generation() {
        let content = "module github.com/alecthomas/kingpin\n";
        let result = parse_go_mod(content);

        assert_eq!(
            result.homepage_url.as_deref(),
            Some("https://pkg.go.dev/github.com/alecthomas/kingpin")
        );
        assert_eq!(
            result.vcs_url.as_deref(),
            Some("https://github.com/alecthomas/kingpin.git")
        );
        assert_eq!(
            result.repository_homepage_url.as_deref(),
            Some("https://pkg.go.dev/github.com/alecthomas/kingpin")
        );
    }

    #[test]
    fn test_go_version_in_extra_data() {
        let content = "\
module github.com/example/repo

go 1.21
";
        let result = parse_go_mod(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("go_version").and_then(|v| v.as_str()),
            Some("1.21")
        );
    }

    // ========================================================================
    // Bug #1: Replace directive parsing
    // ========================================================================

    #[test]
    fn test_replace_single_line() {
        let content = "\
module github.com/census-instrumentation/opencensus-service

require (
\tcontrib.go.opencensus.io v0.0.0-20181029163544-2befc13012d0
\tgopkg.in/yaml.v2 v2.2.5
)

replace git.apache.org/thrift.git => github.com/apache/thrift v0.12.0

go 1.13
";
        let result = parse_go_mod(content);

        // Should have 2 require deps + 1 replace dep
        assert_eq!(result.dependencies.len(), 3);

        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();
        assert_eq!(replace_deps.len(), 1);

        let dep = replace_deps[0];
        let extra = dep.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("replace_old").and_then(|v| v.as_str()),
            Some("git.apache.org/thrift.git")
        );
        assert_eq!(
            extra.get("replace_new").and_then(|v| v.as_str()),
            Some("github.com/apache/thrift")
        );
        assert_eq!(
            extra.get("replace_version").and_then(|v| v.as_str()),
            Some("v0.12.0")
        );
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v0.12.0"));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("thrift")));
    }

    #[test]
    fn test_replace_single_line_with_old_version() {
        let content = "\
module github.com/example/repo

replace github.com/old/pkg v1.0.0 => github.com/new/pkg v2.0.0
";
        let result = parse_go_mod(content);

        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();
        assert_eq!(replace_deps.len(), 1);

        let dep = replace_deps[0];
        let extra = dep.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("replace_old").and_then(|v| v.as_str()),
            Some("github.com/old/pkg")
        );
        assert_eq!(
            extra.get("replace_old_version").and_then(|v| v.as_str()),
            Some("v1.0.0")
        );
        assert_eq!(
            extra.get("replace_new").and_then(|v| v.as_str()),
            Some("github.com/new/pkg")
        );
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v2.0.0"));
    }

    #[test]
    fn test_replace_block() {
        let content = "\
module github.com/example/repo

replace (
\tgithub.com/old/pkg => github.com/new/pkg v1.2.3
\tgithub.com/another/pkg => github.com/better/pkg v3.0.0
)
";
        let result = parse_go_mod(content);

        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();
        assert_eq!(replace_deps.len(), 2);

        let first = &replace_deps[0];
        let first_extra = first.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            first_extra.get("replace_old").and_then(|v| v.as_str()),
            Some("github.com/old/pkg")
        );
        assert_eq!(
            first_extra.get("replace_new").and_then(|v| v.as_str()),
            Some("github.com/new/pkg")
        );
        assert_eq!(first.extracted_requirement.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn test_replace_local_path() {
        let content = "\
module github.com/example/repo

replace github.com/old/pkg => ../local/path
";
        let result = parse_go_mod(content);

        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();
        assert_eq!(replace_deps.len(), 1);

        let dep = replace_deps[0];
        let extra = dep.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("replace_new").and_then(|v| v.as_str()),
            Some("../local/path")
        );
        // Local path replacements have no version
        assert_eq!(dep.extracted_requirement, None);
        assert!(
            extra.get("replace_version").is_none(),
            "local path should have no replace_version"
        );
    }

    // ========================================================================
    // Bug #3: Retract directive parsing
    // ========================================================================

    #[test]
    fn test_retract_single_version() {
        let content = "\
module github.com/example/repo

retract v1.0.0

go 1.18
";
        let result = parse_go_mod(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        let retracted = extra
            .get("retracted_versions")
            .expect("should have retracted_versions");
        let versions: Vec<&str> = retracted
            .as_array()
            .expect("should be array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(versions, vec!["v1.0.0"]);
    }

    #[test]
    fn test_retract_version_range() {
        let content = "\
module github.com/example/repo

retract [v1.0.0, v1.0.5]
";
        let result = parse_go_mod(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        let retracted = extra
            .get("retracted_versions")
            .expect("should have retracted_versions");
        let versions: Vec<&str> = retracted
            .as_array()
            .expect("should be array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(versions, vec!["v1.0.0", "v1.0.5"]);
    }

    #[test]
    fn test_retract_block() {
        let content = "\
module github.com/example/repo

retract (
\tv1.0.0
\t[v1.1.0, v1.1.5]
\tv1.2.0
)
";
        let result = parse_go_mod(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        let retracted = extra
            .get("retracted_versions")
            .expect("should have retracted_versions");
        let versions: Vec<&str> = retracted
            .as_array()
            .expect("should be array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(versions, vec!["v1.0.0", "v1.1.0", "v1.1.5", "v1.2.0"]);
    }

    // ========================================================================
    // Bug #4: Go version directive (already implemented, verify extra_data)
    // ========================================================================

    #[test]
    fn test_go_version_with_patch() {
        let content = "\
module github.com/example/repo

go 1.21.5
";
        let result = parse_go_mod(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("go_version").and_then(|v| v.as_str()),
            Some("1.21.5")
        );
    }

    // ========================================================================
    // Bug #5: State leakage between blocks
    // ========================================================================

    #[test]
    fn test_state_leakage_replace_then_require() {
        // Ensures that a replace block closing does not leak into
        // subsequent require block parsing.
        let content = "\
module github.com/example/repo

replace (
\tgithub.com/old/pkg => github.com/new/pkg v1.0.0
)

require (
\tgithub.com/foo/bar v2.0.0
)
";
        let result = parse_go_mod(content);

        let require_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("require"))
            .collect();
        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();

        assert_eq!(require_deps.len(), 1, "should have 1 require dep");
        assert_eq!(replace_deps.len(), 1, "should have 1 replace dep");
        assert_eq!(
            require_deps[0].extracted_requirement.as_deref(),
            Some("v2.0.0")
        );
    }

    #[test]
    fn test_state_leakage_retract_then_exclude() {
        // Ensures retract block closing doesn't leak into exclude parsing
        let content = "\
module github.com/example/repo

retract (
\tv1.0.0
)

exclude github.com/bad/pkg v0.5.0
";
        let result = parse_go_mod(content);

        let exclude_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("exclude"))
            .collect();
        assert_eq!(exclude_deps.len(), 1, "should have 1 exclude dep");

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        let retracted = extra
            .get("retracted_versions")
            .expect("should have retracted_versions");
        assert_eq!(retracted.as_array().expect("array").len(), 1);
    }

    // ========================================================================
    // Bug #6: Error handling - graceful degradation on malformed input
    // ========================================================================

    #[test]
    fn test_malformed_require_line_no_version() {
        let content = "\
module github.com/example/repo

require github.com/incomplete
";
        let result = parse_go_mod(content);

        // Malformed line should be skipped, not crash
        let require_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("require"))
            .collect();
        assert_eq!(require_deps.len(), 0, "malformed dep should be skipped");
    }

    #[test]
    fn test_malformed_replace_no_arrow() {
        let content = "\
module github.com/example/repo

replace github.com/old/pkg github.com/new/pkg v1.0.0
";
        let result = parse_go_mod(content);

        // Missing => should be skipped gracefully
        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();
        assert_eq!(replace_deps.len(), 0, "malformed replace should be skipped");
    }

    #[test]
    fn test_malformed_replace_empty_sides() {
        let content = "\
module github.com/example/repo

replace =>
";
        let result = parse_go_mod(content);

        let replace_deps: Vec<_> = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .collect();
        assert_eq!(
            replace_deps.len(),
            0,
            "malformed replace with empty sides should be skipped"
        );
    }

    #[test]
    fn test_malformed_retract_empty() {
        let content = "\
module github.com/example/repo

retract
";
        let result = parse_go_mod(content);

        // Should not crash - retract with no argument is skipped
        assert!(
            result
                .extra_data
                .as_ref()
                .and_then(|e| e.get("retracted_versions"))
                .is_none(),
            "empty retract should not produce retracted_versions"
        );
    }

    #[test]
    fn test_completely_malformed_content() {
        let content = "this is not a valid go.mod file at all\n\
garbage in garbage out\n\
= = = = =\n";
        let result = parse_go_mod(content);

        // Should return default data without crashing
        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert!(result.dependencies.is_empty());
        assert!(result.name.is_none());
    }

    // ========================================================================
    // Bug #7: Robust namespace/name splitting
    // ========================================================================

    #[test]
    fn test_namespace_splitting_standard() {
        let content = "\
module github.com/user/repo

require go.opencensus.io/module v0.1.0
";
        let result = parse_go_mod(content);

        // Module: github.com/user/repo
        assert_eq!(result.namespace.as_deref(), Some("github.com/user"));
        assert_eq!(result.name.as_deref(), Some("repo"));

        // Dependency: go.opencensus.io/module
        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("module")));
        assert!(
            dep.purl
                .as_ref()
                .is_some_and(|p| p.contains("go.opencensus.io"))
        );
    }

    #[test]
    fn test_namespace_splitting_no_slash() {
        let content = "module simplemodule\n";
        let result = parse_go_mod(content);

        assert_eq!(result.namespace, None);
        assert_eq!(result.name.as_deref(), Some("simplemodule"));
    }

    #[test]
    fn test_namespace_splitting_deep_path() {
        let content = "\
module github.com/org/repo/v2

require github.com/a/b/c/d v1.0.0
";
        let result = parse_go_mod(content);

        // Module: github.com/org/repo/v2
        assert_eq!(result.namespace.as_deref(), Some("github.com/org/repo"));
        assert_eq!(result.name.as_deref(), Some("v2"));
    }

    // ========================================================================
    // Toolchain directive
    // ========================================================================

    #[test]
    fn test_toolchain_directive() {
        let content = "\
module github.com/example/repo

go 1.21
toolchain go1.21.5
";
        let result = parse_go_mod(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("go_version").and_then(|v| v.as_str()),
            Some("1.21")
        );
        assert_eq!(
            extra.get("toolchain").and_then(|v| v.as_str()),
            Some("go1.21.5")
        );
    }

    // ========================================================================
    // Integration: Full go.mod with all directives
    // ========================================================================

    #[test]
    fn test_full_go_mod_all_directives() {
        let content = "\
module github.com/example/fulltest

go 1.21
toolchain go1.21.5

require (
\tgithub.com/foo/bar v1.0.0
\tgithub.com/baz/qux v2.0.0 // indirect
)

exclude github.com/bad/pkg v0.5.0

replace (
\tgithub.com/old/dep => github.com/new/dep v3.0.0
)

retract [v0.9.0, v0.9.5]
";
        let result = parse_go_mod(content);

        // Module
        assert_eq!(result.namespace.as_deref(), Some("github.com/example"));
        assert_eq!(result.name.as_deref(), Some("fulltest"));

        // Dependencies: 2 require + 1 exclude + 1 replace = 4
        assert_eq!(result.dependencies.len(), 4);

        let require_count = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("require"))
            .count();
        assert_eq!(require_count, 2);

        let exclude_count = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("exclude"))
            .count();
        assert_eq!(exclude_count, 1);

        let replace_count = result
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("replace"))
            .count();
        assert_eq!(replace_count, 1);

        // Indirect
        let indirect = result
            .dependencies
            .iter()
            .find(|d| d.is_direct == Some(false))
            .expect("should have indirect dep");
        assert!(indirect.purl.as_ref().is_some_and(|p| p.contains("qux")));

        // extra_data
        let extra = result.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("go_version").and_then(|v| v.as_str()),
            Some("1.21")
        );
        assert_eq!(
            extra.get("toolchain").and_then(|v| v.as_str()),
            Some("go1.21.5")
        );

        let retracted = extra
            .get("retracted_versions")
            .expect("should have retracted_versions");
        let versions: Vec<&str> = retracted
            .as_array()
            .expect("should be array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(versions, vec!["v0.9.0", "v0.9.5"]);
    }

    #[test]
    fn test_whitespace_variations() {
        let content = "\
module   github.com/example/repo

require (
    github.com/spaces/dep v1.0.0
\tgithub.com/tabs/dep v2.0.0
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.name.as_deref(), Some("repo"));
        assert_eq!(result.dependencies.len(), 2);
    }

    #[test]
    fn test_multiple_require_blocks() {
        let content = "\
module github.com/example/repo

require (
\tgithub.com/foo/bar v1.0.0
)

require (
\tgithub.com/baz/qux v2.0.0
)
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 2);
        assert!(
            result
                .dependencies
                .iter()
                .all(|d| d.scope.as_deref() == Some("require"))
        );
    }

    #[test]
    fn test_is_pinned_false_for_go_mod() {
        let content = "\
module github.com/example/repo

require github.com/foo/bar v1.2.3
";
        let result = parse_go_mod(content);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].is_pinned, Some(false));
    }

    #[test]
    fn test_kingpin_go_mod_full() {
        let content = "\
module github.com/alecthomas/kingpin

require (
\tgithub.com/alecthomas/template v0.0.0-20160405071501-a0175ee3bccc
\tgithub.com/alecthomas/units v0.0.0-20151022065526-2efee857e7cf
\tgithub.com/davecgh/go-spew v1.1.1 // indirect
\tgithub.com/pmezard/go-difflib v1.0.0 // indirect
\tgithub.com/stretchr/testify v1.2.2
)

go 1.13
";
        let result = parse_go_mod(content);

        assert_eq!(result.namespace.as_deref(), Some("github.com/alecthomas"));
        assert_eq!(result.name.as_deref(), Some("kingpin"));
        assert_eq!(result.dependencies.len(), 5);

        let indirect_count = result
            .dependencies
            .iter()
            .filter(|d| d.is_direct == Some(false))
            .count();
        assert_eq!(indirect_count, 2, "should have 2 indirect deps");

        let direct_count = result
            .dependencies
            .iter()
            .filter(|d| d.is_direct == Some(true))
            .count();
        assert_eq!(direct_count, 3, "should have 3 direct deps");
    }

    // ========================================================================
    // GoSumParser tests (Wave 3)
    // ========================================================================

    #[test]
    fn test_go_sum_is_match() {
        assert!(GoSumParser::is_match(&PathBuf::from("go.sum")));
        assert!(GoSumParser::is_match(&PathBuf::from("/some/path/go.sum")));
        assert!(!GoSumParser::is_match(&PathBuf::from("go.mod")));
        assert!(!GoSumParser::is_match(&PathBuf::from("go.sum.bak")));
        assert!(!GoSumParser::is_match(&PathBuf::from("Cargo.lock")));
    }

    #[test]
    fn test_extract_go_sum_basic() {
        let content = "\
github.com/BurntSushi/toml v0.3.1 h1:WXkYYl6Yr3qBf1K79EBnL4mak0OimBfB0XUf9Vl28OQ=
github.com/BurntSushi/toml v0.3.1/go.mod h1:xHWCNGjB5oqiDr8zfno3MHue2Ht5sIBksp03qcyfWMU=
github.com/cznic/golex v0.0.0-20181122101858-9c343928389c h1:G8zTsaqyVfIHpgMFcGgdbhHSFhlNc77rAKkhVbQ9kQg=
github.com/cznic/golex v0.0.0-20181122101858-9c343928389c/go.mod h1:+bmmJDNmKlhWNG+gwWCkaBoTy39Fs+bzRxVBzoTQbIc=
";
        let result = parse_go_sum(content);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.primary_language.as_deref(), Some("Go"));
        assert_eq!(result.datasource_id, Some(DatasourceId::GoSum));

        // Deduplication: 4 lines → 2 unique dependencies
        assert_eq!(result.dependencies.len(), 2);

        // No package-level metadata for go.sum
        assert!(result.name.is_none());
        assert!(result.namespace.is_none());
    }

    #[test]
    fn test_go_sum_dependency_attributes() {
        let content = "\
github.com/BurntSushi/toml v0.3.1 h1:WXkYYl6Yr3qBf1K79EBnL4mak0OimBfB0XUf9Vl28OQ=
github.com/BurntSushi/toml v0.3.1/go.mod h1:xHWCNGjB5oqiDr8zfno3MHue2Ht5sIBksp03qcyfWMU=
";
        let result = parse_go_sum(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];

        assert_eq!(dep.scope.as_deref(), Some("dependency"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v0.3.1"));
        assert_eq!(dep.is_runtime, Some(true));
        assert_eq!(dep.is_optional, Some(false));
        assert_eq!(dep.is_pinned, Some(true));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("golang")));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("toml")));
    }

    #[test]
    fn test_go_sum_deduplication() {
        // Each module has 2 lines: module hash and go.mod hash
        // They should be deduplicated to 1 entry
        let content = "\
github.com/foo/bar v1.0.0 h1:abc=
github.com/foo/bar v1.0.0/go.mod h1:def=
github.com/baz/qux v2.0.0 h1:ghi=
github.com/baz/qux v2.0.0/go.mod h1:jkl=
";
        let result = parse_go_sum(content);

        assert_eq!(result.dependencies.len(), 2);

        let bar = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("bar")));
        assert!(bar.is_some(), "should have bar dependency");

        let qux = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("qux")));
        assert!(qux.is_some(), "should have qux dependency");
    }

    #[test]
    fn test_go_sum_skip_go_mod_lines() {
        // The /go.mod suffix should be stripped during deduplication
        let content = "\
github.com/single/mod v1.0.0/go.mod h1:abc=
";
        let result = parse_go_sum(content);

        // Even a single /go.mod line should produce a dep (version without suffix)
        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert_eq!(dep.extracted_requirement.as_deref(), Some("v1.0.0"));
    }

    #[test]
    fn test_go_sum_pseudo_versions() {
        let content = "\
golang.org/x/text v0.3.1-0.20180807135948-17ff2d5776d2 h1:z99zHgr7hKfrUcX/KsoJk5FJfjTceCKIp96+biqP4To=
golang.org/x/text v0.3.1-0.20180807135948-17ff2d5776d2/go.mod h1:NqM8EUOU14njkJ3fqMW+pc6Ldnwhi/IjpwHt7yyuwOQ=
";
        let result = parse_go_sum(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        assert_eq!(
            dep.extracted_requirement.as_deref(),
            Some("v0.3.1-0.20180807135948-17ff2d5776d2")
        );
    }

    #[test]
    fn test_go_sum_empty() {
        let content = "";
        let result = parse_go_sum(content);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_go_sum_malformed_lines() {
        let content = "\
this is not valid
github.com/BurntSushi/toml v0.3.1 h1:WXkYYl6Yr3qBf1K79EBnL4mak0OimBfB0XUf9Vl28OQ=

just_two_fields something
";
        let result = parse_go_sum(content);

        // Only valid h1: lines should be parsed; "this is not valid" has no h1: hash
        assert_eq!(result.dependencies.len(), 1);
    }

    #[test]
    fn test_go_sum_purl_generation() {
        let content = "\
github.com/foo/bar v0.3.1 h1:WXkYYl6Yr3qBf1K79EBnL4mak0OimBfB0XUf9Vl28OQ=
";
        let result = parse_go_sum(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];
        let purl = dep.purl.as_ref().expect("should have purl");
        assert!(purl.starts_with("pkg:golang/"));
        assert!(purl.contains("bar"));
        assert!(purl.contains("v0.3.1"));
    }

    #[test]
    fn test_go_sum_graceful_error_handling() {
        let bad_path = PathBuf::from("/nonexistent/path/go.sum");
        let result = GoSumParser::extract_first_package(&bad_path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.datasource_id, Some(DatasourceId::GoSum));
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_go_sum_multiple_versions_same_module() {
        let content = "\
github.com/foo/bar v1.0.0 h1:abc=
github.com/foo/bar v1.0.0/go.mod h1:def=
github.com/foo/bar v1.1.0 h1:ghi=
github.com/foo/bar v1.1.0/go.mod h1:jkl=
";
        let result = parse_go_sum(content);

        // Two different versions of the same module → 2 entries
        assert_eq!(result.dependencies.len(), 2);
    }

    // Bug #9: Efficient scanning - each line processed once
    #[test]
    fn test_efficient_scanning_go_sum() {
        // Generate a large go.sum and verify it parses correctly.
        // The efficient implementation processes each line exactly once.
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!(
                "github.com/pkg{}/mod v1.0.{} h1:hash{}=\n",
                i, i, i
            ));
            content.push_str(&format!(
                "github.com/pkg{}/mod v1.0.{}/go.mod h1:hash{}=\n",
                i, i, i
            ));
        }

        let result = parse_go_sum(&content);

        // 100 unique modules after deduplication
        assert_eq!(result.dependencies.len(), 100);
    }

    // ========================================================================
    // GodepsParser tests (Wave 3)
    // ========================================================================

    #[test]
    fn test_godeps_json_is_match() {
        assert!(GodepsParser::is_match(&PathBuf::from("Godeps.json")));
        assert!(GodepsParser::is_match(&PathBuf::from(
            "/some/path/Godeps.json"
        )));
        assert!(GodepsParser::is_match(&PathBuf::from("Godeps/Godeps.json")));
        assert!(GodepsParser::is_match(&PathBuf::from(
            "/some/project/Godeps/Godeps.json"
        )));
        assert!(!GodepsParser::is_match(&PathBuf::from("godeps.json")));
        assert!(!GodepsParser::is_match(&PathBuf::from("package.json")));
        assert!(!GodepsParser::is_match(&PathBuf::from("go.mod")));
    }

    #[test]
    fn test_extract_godeps_json_mini() {
        let content = r#"{
    "ImportPath": "app",
    "GoVersion": "go1.2",
    "Deps": []
}"#;
        let result = parse_godeps_json(content);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.primary_language.as_deref(), Some("Go"));
        assert_eq!(result.datasource_id, Some(DatasourceId::Godeps));
        assert_eq!(result.name.as_deref(), Some("app"));
        assert_eq!(result.namespace, None);
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_extract_godeps_json_basic() {
        let content = r#"{
    "ImportPath": "github.com/cloudfoundry-incubator/etcd-metrics-server",
    "GoVersion": "go1.2.1",
    "Packages": ["./..."],
    "Deps": [
        {
            "ImportPath": "github.com/cloudfoundry-incubator/metricz",
            "Rev": "d9bdb1b78384f05faa694175782055d5ba3966b5"
        },
        {
            "ImportPath": "github.com/cloudfoundry/gosteno",
            "Comment": "scotty_09012012-40-g5eb8c6e",
            "Rev": "5eb8c6e554f0dfc39d6468813b8ac19ec28fe74f"
        }
    ]
}"#;
        let result = parse_godeps_json(content);

        assert_eq!(
            result.namespace.as_deref(),
            Some("github.com/cloudfoundry-incubator")
        );
        assert_eq!(result.name.as_deref(), Some("etcd-metrics-server"));
        assert_eq!(result.dependencies.len(), 2);
    }

    #[test]
    fn test_godeps_json_dependency_attributes() {
        let content = r#"{
    "ImportPath": "github.com/user/repo",
    "GoVersion": "go1.18",
    "Deps": [
        {
            "ImportPath": "github.com/foo/bar",
            "Rev": "abc123def456"
        }
    ]
}"#;
        let result = parse_godeps_json(content);

        assert_eq!(result.dependencies.len(), 1);
        let dep = &result.dependencies[0];

        assert_eq!(dep.scope.as_deref(), Some("Deps"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("abc123def456"));
        assert_eq!(dep.is_runtime, Some(true));
        assert_eq!(dep.is_optional, Some(false));
        assert_eq!(dep.is_pinned, Some(false));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("golang")));
        assert!(dep.purl.as_ref().is_some_and(|p| p.contains("bar")));
    }

    #[test]
    fn test_godeps_json_revisions() {
        let content = r#"{
    "ImportPath": "github.com/user/repo",
    "GoVersion": "go1.18",
    "Deps": [
        {
            "ImportPath": "github.com/foo/bar",
            "Rev": "d9bdb1b78384f05faa694175782055d5ba3966b5"
        },
        {
            "ImportPath": "github.com/coreos/go-etcd/etcd",
            "Comment": "v0.2.0-rc1-96-g1e26d8e",
            "Rev": "1e26d8ee84cf9b1000d2af8acfb45b2521f49be5"
        }
    ]
}"#;
        let result = parse_godeps_json(content);

        assert_eq!(result.dependencies.len(), 2);

        // First dep: revision as extracted_requirement
        let bar = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("bar")))
            .expect("should have bar dep");
        assert_eq!(
            bar.extracted_requirement.as_deref(),
            Some("d9bdb1b78384f05faa694175782055d5ba3966b5")
        );

        // Second dep: also has revision, comment is not stored in requirement
        let etcd = result
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("etcd")))
            .expect("should have etcd dep");
        assert_eq!(
            etcd.extracted_requirement.as_deref(),
            Some("1e26d8ee84cf9b1000d2af8acfb45b2521f49be5")
        );
    }

    #[test]
    fn test_godeps_json_go_version() {
        let content = r#"{
    "ImportPath": "github.com/user/repo",
    "GoVersion": "go1.18",
    "Deps": []
}"#;
        let result = parse_godeps_json(content);

        let extra = result.extra_data.as_ref().expect("should have extra_data");
        assert_eq!(
            extra.get("go_version").and_then(|v| v.as_str()),
            Some("go1.18")
        );
    }

    #[test]
    fn test_godeps_json_empty_deps() {
        let content = r#"{
    "ImportPath": "github.com/example/tool",
    "GoVersion": "go1.5",
    "Deps": []
}"#;
        let result = parse_godeps_json(content);

        assert_eq!(result.name.as_deref(), Some("tool"));
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_godeps_json_missing_optional_fields() {
        // Minimal Godeps.json - only ImportPath and Deps
        let content = r#"{
    "ImportPath": "github.com/user/minimal",
    "Deps": [
        {
            "ImportPath": "github.com/foo/bar",
            "Rev": "abc123"
        }
    ]
}"#;
        let result = parse_godeps_json(content);

        assert_eq!(result.name.as_deref(), Some("minimal"));
        assert_eq!(result.dependencies.len(), 1);
        // No go_version means no extra_data (or empty)
    }

    #[test]
    fn test_godeps_json_deep_import_path() {
        let content = r#"{
    "ImportPath": "github.com/cloudfoundry/gunk/natsrunner",
    "GoVersion": "go1.2",
    "Deps": [
        {
            "ImportPath": "github.com/cloudfoundry/gunk/test_server",
            "Rev": "8c166e2e973df123b29351b396bb8d356192bd81"
        }
    ]
}"#;
        let result = parse_godeps_json(content);

        // Deep path splitting: "github.com/cloudfoundry/gunk/natsrunner"
        // → namespace="github.com/cloudfoundry/gunk", name="natsrunner"
        assert_eq!(
            result.namespace.as_deref(),
            Some("github.com/cloudfoundry/gunk")
        );
        assert_eq!(result.name.as_deref(), Some("natsrunner"));
    }

    #[test]
    fn test_godeps_json_graceful_error_handling() {
        let bad_path = PathBuf::from("/nonexistent/path/Godeps.json");
        let result = GodepsParser::extract_first_package(&bad_path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.datasource_id, Some(DatasourceId::Godeps));
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_godeps_json_malformed_json() {
        let content = "this is not json at all {{{";
        let result = parse_godeps_json(content);

        // Should return default data without crashing
        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.datasource_id, Some(DatasourceId::Godeps));
        assert!(result.dependencies.is_empty());
    }

    // Bug #11: Proper JSON parser (serde_json instead of regex)
    #[test]
    fn test_proper_json_parser_godeps() {
        // This tests that serde_json properly handles JSON with
        // various formatting styles (indentation, whitespace, etc.)
        let content = r#"{"ImportPath":"github.com/compact/format","GoVersion":"go1.18","Deps":[{"ImportPath":"github.com/dep/one","Rev":"abc"},{"ImportPath":"github.com/dep/two","Rev":"def"}]}"#;
        let result = parse_godeps_json(content);

        assert_eq!(result.name.as_deref(), Some("format"));
        assert_eq!(result.dependencies.len(), 2);
    }

    #[test]
    fn test_godeps_json_purl_generation() {
        let content = r#"{
    "ImportPath": "github.com/user/repo",
    "Deps": [
        {
            "ImportPath": "github.com/foo/bar",
            "Rev": "abc123"
        }
    ]
}"#;
        let result = parse_godeps_json(content);

        // Package-level PURL
        let purl = result.purl.as_ref().expect("should have package purl");
        assert!(purl.starts_with("pkg:golang/"));
        assert!(purl.contains("repo"));

        // Dependency PURL
        let dep = &result.dependencies[0];
        let dep_purl = dep.purl.as_ref().expect("should have dep purl");
        assert!(dep_purl.starts_with("pkg:golang/"));
        assert!(dep_purl.contains("bar"));
    }

    #[test]
    fn test_godeps_testdata_mini() {
        let path = PathBuf::from("testdata/go/mini-godeps.json");
        let result = GodepsParser::extract_first_package(&path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.name.as_deref(), Some("app"));
        assert_eq!(result.namespace, None);
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_godeps_testdata_full() {
        let path = PathBuf::from("testdata/go/full-godeps.json");
        let result = GodepsParser::extract_first_package(&path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(
            result.namespace.as_deref(),
            Some("github.com/cloudfoundry-incubator")
        );
        assert_eq!(result.name.as_deref(), Some("etcd-metrics-server"));
        assert_eq!(result.dependencies.len(), 3);

        // All deps should have scope "Deps"
        assert!(
            result
                .dependencies
                .iter()
                .all(|d| d.scope.as_deref() == Some("Deps"))
        );
    }

    #[test]
    fn test_go_sum_testdata_basic() {
        let path = PathBuf::from("testdata/go/basic.go.sum");
        let result = GoSumParser::extract_first_package(&path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.datasource_id, Some(DatasourceId::GoSum));
        // 6 lines → 3 unique modules after dedup
        assert_eq!(result.dependencies.len(), 3);
    }
}
