//! Parser for gradle.lockfile dependency lock files.
//!
//! Extracts resolved dependency information from Gradle's gradle.lockfile format.
//! This format is used by Gradle to lock exact dependency versions.
//!
//! # Supported Formats
//! - gradle.lockfile (text-based dependency declarations)
//!
//! # Key Features
//! - Exact version resolution from lockfile
//! - Group and artifact extraction
//! - Preserves lockfile configuration membership per dependency
//! - Package URL (purl) generation for Maven packages
//!
//! # Implementation Notes
//! - gradle.lockfile is a simple text format with dependency lines
//! - Format: `<group>:<artifact>:<version>=<configuration>[,<configuration>...]` (one per line)
//! - Comments and empty lines are skipped
//! - All dependencies are pinned (is_pinned: true)

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::PackageParser;

/// Gradle gradle.lockfile parser.
///
/// Extracts pinned dependency versions from Gradle's dependency lock files.
pub struct GradleLockfileParser;

impl PackageParser for GradleLockfileParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "gradle.lockfile")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open gradle.lockfile at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let reader = BufReader::new(file);
        let dependencies = extract_dependencies(reader);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name: None,
            version: None,
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: Vec::new(),
            keywords: Vec::new(),
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: None,
            declared_license_expression_spdx: None,
            license_detections: Vec::new(),
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: Vec::new(),
            extracted_license_statement: None,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            is_private: false,
            is_virtual: false,
            extra_data: None,
            dependencies,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::GradleLockfile),
            purl: None,
        }]
    }
}

/// Extract dependencies from gradle.lockfile
fn extract_dependencies<R: BufRead>(reader: R) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                warn!("Failed to read line from gradle.lockfile: {}", e);
                continue;
            }
        };

        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse dependency line format: group:artifact:version=config[,config...]
        if let Some(dep) = parse_dependency_line(line) {
            dependencies.push(dep);
        }
    }

    dependencies
}

/// Parse a single dependency line from gradle.lockfile
///
/// Expected format: `group:artifact:version=configuration[,configuration...]`
/// Example: `com.example:my-lib:1.0.0=compileClasspath,runtimeClasspath`
fn parse_dependency_line(line: &str) -> Option<Dependency> {
    // Split by = to separate GAV from the list of configurations that include this dependency.
    let (gav_part, configurations_part) = line.split_once('=')?;

    if gav_part == "empty" {
        return None;
    }

    let configurations: Vec<String> = configurations_part
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();

    // Parse GAV (group:artifact:version)
    let parts: Vec<&str> = gav_part.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let group = parts[0].to_string();
    let artifact = parts[1].to_string();
    let version = parts[2].to_string();

    // Generate purl
    let purl = PackageUrl::new("maven", &artifact).ok().and_then(|mut p| {
        p.with_namespace(&group).ok()?;
        p.with_version(&version).ok()?;
        Some(p.to_string())
    });

    // Build extra_data with group and artifact separately
    let mut extra_data: Option<HashMap<String, serde_json::Value>> = None;
    if !group.is_empty() || !artifact.is_empty() {
        let mut map = HashMap::new();
        if !group.is_empty() {
            map.insert(
                "group".to_string(),
                serde_json::Value::String(group.clone()),
            );
        }
        if !artifact.is_empty() {
            map.insert(
                "artifact".to_string(),
                serde_json::Value::String(artifact.clone()),
            );
        }
        if !configurations.is_empty() {
            map.insert(
                "configurations".to_string(),
                serde_json::Value::Array(
                    configurations
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
        extra_data = Some(map);
    }

    // Create resolved_package
    let resolved_package = ResolvedPackage {
        primary_language: None,
        download_url: None,
        sha1: None,
        sha256: None,
        sha512: None,
        md5: None,
        is_virtual: false,
        extra_data: None,
        dependencies: Vec::new(),
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::GradleLockfile),
        purl: purl.clone(),
        ..ResolvedPackage::new(PackageType::Maven, group, artifact, version)
    };

    Some(Dependency {
        purl,
        extracted_requirement: None,
        scope: None,
        is_pinned: Some(true),
        is_direct: None,
        is_optional: None,
        is_runtime: None,
        resolved_package: Some(Box::new(resolved_package)),
        extra_data,
    })
}

/// Returns a default empty PackageData for error cases
fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(GradleLockfileParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::GradleLockfile),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_is_match_gradle_lockfile() {
        assert!(GradleLockfileParser::is_match(Path::new("gradle.lockfile")));
        assert!(GradleLockfileParser::is_match(Path::new(
            "/path/to/gradle.lockfile"
        )));
    }

    #[test]
    fn test_is_match_not_gradle_lockfile() {
        assert!(!GradleLockfileParser::is_match(Path::new("package.json")));
        assert!(!GradleLockfileParser::is_match(Path::new("Cargo.lock")));
        assert!(!GradleLockfileParser::is_match(Path::new("gradle.lock")));
    }

    #[test]
    fn test_parse_dependency_line_simple() {
        let line = "com.example:my-lib:1.0.0=compileClasspath,runtimeClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert_eq!(
            dep.resolved_package.as_ref().unwrap().name,
            "my-lib".to_string()
        );
        assert_eq!(
            dep.resolved_package.as_ref().unwrap().version,
            "1.0.0".to_string()
        );
        assert_eq!(
            dep.resolved_package.as_ref().unwrap().namespace,
            "com.example".to_string()
        );
        assert_eq!(
            dep.resolved_package.as_ref().unwrap().package_type,
            PackageType::Maven
        );
    }

    #[test]
    fn test_parse_dependency_line_complex_group() {
        let line = "org.springframework.boot:spring-boot-starter-web:2.7.0=compileClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert_eq!(
            dep.resolved_package.as_ref().unwrap().name,
            "spring-boot-starter-web".to_string()
        );
        assert_eq!(
            dep.resolved_package.as_ref().unwrap().version,
            "2.7.0".to_string()
        );
        assert_eq!(
            dep.resolved_package.as_ref().unwrap().namespace,
            "org.springframework.boot".to_string()
        );
    }

    #[test]
    fn test_parse_dependency_line_with_single_configuration() {
        let line = "com.example:my-lib:1.0.0=runtimeClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert_eq!(
            dep.resolved_package.as_ref().unwrap().name,
            "my-lib".to_string()
        );
        assert_eq!(
            dep.resolved_package.as_ref().unwrap().version,
            "1.0.0".to_string()
        );
    }

    #[test]
    fn test_parse_dependency_line_invalid_format() {
        // Missing version
        let line = "com.example:my-lib=abc123";
        assert!(parse_dependency_line(line).is_none());

        // No configuration separator
        let line = "com.example:my-lib:1.0.0";
        assert!(parse_dependency_line(line).is_none());
    }

    #[test]
    fn test_extract_dependencies_multiple_lines() {
        let content = "com.example:lib1:1.0.0=compileClasspath\ncom.example:lib2:2.0.0=runtimeClasspath\ncom.test:lib3:3.0.0=testRuntimeClasspath";
        let reader = Cursor::new(content);
        let deps = extract_dependencies(reader);

        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].resolved_package.as_ref().unwrap().name, "lib1");
        assert_eq!(deps[1].resolved_package.as_ref().unwrap().name, "lib2");
        assert_eq!(deps[2].resolved_package.as_ref().unwrap().name, "lib3");
    }

    #[test]
    fn test_extract_dependencies_with_comments_and_empty_lines() {
        let content = "# This is a comment\ncom.example:lib1:1.0.0=compileClasspath\n\n# Another comment\ncom.example:lib2:2.0.0=runtimeClasspath\n";
        let reader = Cursor::new(content);
        let deps = extract_dependencies(reader);

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].resolved_package.as_ref().unwrap().name, "lib1");
        assert_eq!(deps[1].resolved_package.as_ref().unwrap().name, "lib2");
    }

    #[test]
    fn test_extract_dependencies_empty_file() {
        let content = "";
        let reader = Cursor::new(content);
        let deps = extract_dependencies(reader);

        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_extract_dependencies_only_comments() {
        let content = "# Comment 1\n# Comment 2\n# Comment 3";
        let reader = Cursor::new(content);
        let deps = extract_dependencies(reader);

        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_extract_first_package_returns_correct_package_type() {
        let content = "com.example:lib:1.0.0=compileClasspath";
        let reader = Cursor::new(content);
        let deps = extract_dependencies(reader);

        assert!(!deps.is_empty());
        assert_eq!(
            deps[0].resolved_package.as_ref().unwrap().package_type,
            PackageType::Maven
        );
    }

    #[test]
    fn test_parse_dependency_generates_purl() {
        let line = "com.google.guava:guava:30.1-jre=runtimeClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert!(dep.purl.is_some());
        let purl = dep.purl.unwrap();
        assert!(purl.contains("maven"));
        assert!(purl.contains("guava"));
        assert!(purl.contains("30.1-jre"));
    }

    #[test]
    fn test_parse_dependency_extra_data_contains_group_and_artifact() {
        let line =
            "org.junit.jupiter:junit-jupiter-api:5.8.0=testRuntimeClasspath,compileClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert!(dep.extra_data.is_some());
        let extra = dep.extra_data.unwrap();
        assert!(extra.contains_key("group"));
        assert!(extra.contains_key("artifact"));
        assert!(extra.contains_key("configurations"));
    }

    #[test]
    fn test_extract_dependencies_malformed_lines_ignored() {
        let content = "com.example:lib1:1.0.0=compileClasspath\ninvalid-line\ncom.example:lib2:2.0.0=runtimeClasspath";
        let reader = Cursor::new(content);
        let deps = extract_dependencies(reader);

        // Only valid dependencies are extracted
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].resolved_package.as_ref().unwrap().name, "lib1");
        assert_eq!(deps[1].resolved_package.as_ref().unwrap().name, "lib2");
    }

    #[test]
    fn test_dependency_has_correct_flags() {
        let line = "com.example:lib:1.0.0=compileClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert_eq!(dep.is_pinned, Some(true));
        assert_eq!(dep.is_optional, None);
        assert_eq!(dep.is_runtime, None);
    }

    #[test]
    fn test_parse_dependency_line_preserves_configurations_not_runtime_semantics() {
        let line = "com.example:my-lib:1.0.0=compileClasspath,runtimeClasspath";
        let dep = parse_dependency_line(line).expect("Failed to parse dependency");

        assert_eq!(dep.is_runtime, None);
        assert_eq!(dep.is_optional, None);
        assert_eq!(dep.is_direct, None);

        let extra = dep.extra_data.as_ref().expect("expected extra_data");
        assert_eq!(
            extra.get("configurations"),
            Some(&serde_json::json!(["compileClasspath", "runtimeClasspath"]))
        );
    }

    #[test]
    fn test_parse_dependency_line_skips_empty_configuration_marker() {
        assert!(parse_dependency_line("empty=annotationProcessor").is_none());
    }
}

crate::register_parser!(
    "Gradle lockfile",
    &["**/gradle.lockfile"],
    "maven",
    "Java",
    Some("https://docs.gradle.org/current/userguide/dependency_locking.html"),
);
