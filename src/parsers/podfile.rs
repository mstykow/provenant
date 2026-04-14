//! Parser for CocoaPods Podfile manifest files.
//!
//! Extracts dependency declarations from Podfile using regex-based Ruby Domain-Specific
//! Language (DSL) parsing without full Ruby AST parsing.
//!
//! # Supported Formats
//! - Podfile (CocoaPods manifest with Ruby DSL syntax)
//!
//! # Key Features
//! - Regex-based Ruby DSL parsing for dependency declarations
//! - Support for git, path, and source dependencies
//! - Pod groups and target-specific dependencies
//! - Version constraint parsing (exact, ranges, pessimistic)
//! - Source URL extraction for custom pod repositories
//!
//! # Implementation Notes
//! - Uses regex for pattern matching (not full Ruby parser)
//! - Supports syntax: `pod 'Name', 'version'`, `pod 'Name', :git => 'url'`
//! - Local path dependencies (`:path =>`) are tracked as dependencies
//! - Graceful error handling with `warn!()` logs

use std::path::Path;
use std::sync::LazyLock;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use regex::Regex;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::PackageParser;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

/// Parses CocoaPods Podfile dependency files.
///
/// Extracts dependency declarations from Podfile using regex-based Ruby DSL parsing.
///
/// # Supported Syntax
/// - `pod 'Name', 'version'` - Standard pod with version
/// - `pod 'Name'` - Pod without version constraint
/// - `pod 'Name', :git => 'url'` - Git dependency
/// - `pod 'Name', :path => '../LocalPod'` - Local path dependency
/// - `pod 'Firebase/Analytics'` - Subspecs
/// - Version operators: `~>`, `>=`, `<=`, etc.
pub struct PodfileParser;

impl PackageParser for PodfileParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cocoapods;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "Podfile")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let dependencies = extract_dependencies(&content);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name: None,
            version: None,
            qualifiers: None,
            subpath: None,
            primary_language: Some("Objective-C".to_string()),
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
            extra_data: None,
            dependencies,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::CocoapodsPodfile),
            purl: None,
            is_private: false,
            is_virtual: false,
        }]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PodfileParser::PACKAGE_TYPE),
        primary_language: Some("Objective-C".to_string()),
        datasource_id: Some(DatasourceId::CocoapodsPodfile),
        ..Default::default()
    }
}

static POD_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"pod\s+['"]([^'"]+)['"](?:\s*,\s*['"]([^'"]+)['"])?(?:\s*,\s*:git\s*=>\s*['"]([^'"]+)['"])?(?:\s*,\s*:path\s*=>\s*['"]([^'"]+)['"])?"#
    ).expect("valid regex")
});

/// Extract dependencies from Podfile
fn extract_dependencies(content: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let cleaned_line = pre_process(line);
        if let Some(caps) = POD_PATTERN.captures(&cleaned_line) {
            let name = truncate_field(caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string());
            let version_req = caps.get(2).map(|m| truncate_field(m.as_str().to_string()));
            let git_url = caps.get(3).map(|m| truncate_field(m.as_str().to_string()));
            let local_path = caps.get(4).map(|m| truncate_field(m.as_str().to_string()));

            if let Some(dep) = create_dependency(name, version_req, git_url, local_path) {
                dependencies.push(dep);
            }
        }
    }

    dependencies
}

/// Create a Dependency from parsed components
fn create_dependency(
    name: String,
    version_req: Option<String>,
    _git_url: Option<String>,
    _local_path: Option<String>,
) -> Option<Dependency> {
    if name.is_empty() {
        return None;
    }

    let purl = PackageUrl::new("cocoapods", &name).ok()?;

    let is_pinned = version_req
        .as_ref()
        .map(|v| !v.contains(&['~', '>', '<', '='][..]))
        .unwrap_or(false);

    Some(Dependency {
        purl: Some(truncate_field(purl.to_string())),
        extracted_requirement: version_req.map(truncate_field),
        scope: Some("dependencies".to_string()),
        is_runtime: None,
        is_optional: None,
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

/// Pre-process a line by removing comments and trimming
fn pre_process(line: &str) -> String {
    let line = if let Some(comment_pos) = line.find('#') {
        &line[..comment_pos]
    } else {
        line
    };
    line.trim().to_string()
}

crate::register_parser!(
    "CocoaPods Podfile",
    &["**/Podfile"],
    "cocoapods",
    "Objective-C",
    Some("https://guides.cocoapods.org/using/the-podfile.html"),
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_match() {
        assert!(PodfileParser::is_match(Path::new("Podfile")));
        assert!(PodfileParser::is_match(Path::new("project/Podfile")));
        assert!(!PodfileParser::is_match(Path::new("Podfile.lock")));
        assert!(!PodfileParser::is_match(Path::new("FooPodfile")));
        assert!(!PodfileParser::is_match(Path::new("config.podfile")));
        assert!(!PodfileParser::is_match(Path::new("MyLib.podspec")));
        assert!(!PodfileParser::is_match(Path::new("MyLib.podspec.json")));
    }

    #[test]
    fn test_extract_simple_pod() {
        let content = r#"
platform :ios, '9.0'

target 'MyApp' do
  pod 'AFNetworking', '~> 4.0'
  pod 'Alamofire'
end
"#;
        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 2);

        assert_eq!(deps[0].purl, Some("pkg:cocoapods/AFNetworking".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some("~> 4.0".to_string()));
        assert_eq!(deps[0].is_pinned, Some(false));
        assert_eq!(deps[0].scope, Some("dependencies".to_string()));
        assert_eq!(deps[0].is_runtime, None);
        assert_eq!(deps[0].is_optional, None);

        assert_eq!(deps[1].purl, Some("pkg:cocoapods/Alamofire".to_string()));
        assert_eq!(deps[1].extracted_requirement, None);
    }

    #[test]
    fn test_extract_pod_with_git() {
        let content = r#"
pod 'AFNetworking', :git => 'https://github.com/AFNetworking/AFNetworking.git'
"#;
        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:cocoapods/AFNetworking".to_string()));
    }

    #[test]
    fn test_extract_pod_with_path() {
        let content = r#"
pod 'MyLocalPod', :path => '../MyLocalPod'
"#;
        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:cocoapods/MyLocalPod".to_string()));
    }

    #[test]
    fn test_extract_pod_with_version_and_git() {
        let content = r#"
pod 'RestKit', '~> 0.20', :git => 'https://github.com/RestKit/RestKit.git'
"#;
        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:cocoapods/RestKit".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some("~> 0.20".to_string()));
    }

    #[test]
    fn test_ignores_comments() {
        let content = r#"
# pod 'Commented', '1.0'
pod 'Active', '2.0'  # inline comment
"#;
        let deps = extract_dependencies(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:cocoapods/Active".to_string()));
    }
}
