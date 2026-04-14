//! Parser for CRAN R package DESCRIPTION files.
//!
//! Extracts package metadata and dependencies from R package DESCRIPTION files
//! which use Debian Control File (DCF) format similar to RFC822.
//!
//! # Supported Formats
//! - DESCRIPTION (CRAN R package manifest)
//!
//! # Key Features
//! - Multi-type dependency extraction (Depends, Imports, Suggests, Enhances, LinkingTo)
//! - Version constraint parsing with operators (>=, <=, >, <, ==)
//! - Filters out R version requirements (not actual packages)
//! - Author/Maintainer party extraction with email parsing
//! - Package URL (purl) generation
//!
//! # Implementation Notes
//! - Uses DCF/RFC822-like format with continuation lines
//! - Field names are case-sensitive (Package, Version, Description, etc.)
//! - Dependencies are comma-separated with optional version constraints
//! - R version requirements (e.g., "R (>= 4.1.0)") are filtered out
//! - Authors@R field is NOT parsed (requires R interpreter)

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use regex::Regex;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::PackageParser;

/// CRAN R package DESCRIPTION file parser.
///
/// Extracts package metadata, dependencies, and party information from
/// standard DESCRIPTION files used by R packages in the CRAN ecosystem.
pub struct CranParser;

impl PackageParser for CranParser {
    const PACKAGE_TYPE: PackageType = PackageType::Cran;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "DESCRIPTION")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read DESCRIPTION at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };
        let fields = parse_dcf(&content);

        let name = fields
            .get("Package")
            .map(|s| truncate_field(s.trim().to_string()));
        let version = fields
            .get("Version")
            .map(|s| truncate_field(s.trim().to_string()));

        // Generate PURL
        let purl = create_package_url(&name, &version);

        // Generate repository URLs
        let repository_homepage_url = name
            .as_ref()
            .map(|n| truncate_field(format!("https://cran.r-project.org/package={}", n)));

        // Build description from Title and Description fields
        let description = build_description(&fields);

        // Extract license statement
        let extracted_license_statement = fields
            .get("License")
            .map(|s| truncate_field(s.trim().to_string()));

        // Extract URL field
        let homepage_url = fields
            .get("URL")
            .map(|s| truncate_field(s.split(',').next().unwrap_or("").trim().to_string()))
            .filter(|s| !s.is_empty());

        // Extract parties (Author and Maintainer)
        let mut parties = Vec::new();

        // Parse Maintainer field
        if let Some(maintainer_str) = fields.get("Maintainer")
            && let Some(party) = parse_party(maintainer_str, "maintainer")
        {
            parties.push(party);
        }

        // Parse Author field
        if let Some(author_str) = fields.get("Author") {
            for author_part in split_author_entries(author_str) {
                if let Some(party) = parse_party(author_part, "author") {
                    parties.push(party);
                }
            }
        }

        // Extract dependencies from all dependency fields
        let mut dependencies = Vec::new();

        // Process each dependency type
        for (field_name, scope) in [
            ("Depends", None),
            ("Imports", Some("imports")),
            ("Suggests", Some("suggests")),
            ("Enhances", Some("enhances")),
            ("LinkingTo", Some("linkingto")),
        ] {
            if let Some(deps_str) = fields.get(field_name) {
                dependencies.extend(parse_dependencies(deps_str, scope));
            }
        }

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name,
            version,
            qualifiers: None,
            subpath: None,
            primary_language: Some("R".to_string()),
            description,
            release_date: None,
            parties,
            keywords: Vec::new(),
            homepage_url,
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
            extracted_license_statement,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            is_private: false,
            is_virtual: false,
            extra_data: None,
            dependencies,
            repository_homepage_url,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::CranDescription),
            purl,
        }]
    }
}

fn parse_dcf(content: &str) -> HashMap<String, String> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut current_field: Option<String> = None;
    let mut current_value = String::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        // Check if line is a continuation (starts with whitespace)
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_field.is_some() {
                // Append to current value, replacing continuation line indent with space
                if !current_value.is_empty() {
                    current_value.push(' ');
                }
                current_value.push_str(line.trim_start());
            }
        } else if let Some((field_name, field_value)) = line.split_once(':') {
            // New field: save previous field if any
            if let Some(field) = current_field.take() {
                fields.insert(field, truncate_field(current_value.clone()));
                current_value.clear();
            }

            // Start new field
            current_field = Some(field_name.trim().to_string());
            current_value = field_value.trim_start().to_string();
        }
        // Else: empty line or invalid line - ignore
    }

    // Save the last field
    if let Some(field) = current_field {
        fields.insert(field, truncate_field(current_value));
    }

    fields
}

/// Parse a comma-separated dependency list with optional version constraints.
///
/// Format: "package1 (>= 1.0), package2, package3 (== 2.0)"
/// Filters out R version requirements like "R (>= 4.1.0)"
fn parse_dependencies(deps_str: &str, scope: Option<&str>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for dep in deps_str.split(',').take(MAX_ITERATION_COUNT) {
        let dep = dep.trim();
        if dep.is_empty() {
            continue;
        }

        let (name, extracted_requirement, is_pinned) = parse_version_constraint(dep);

        // Skip R version requirements (not actual package dependencies)
        if name == "R" {
            continue;
        }

        // Create PURL for dependency
        let purl = if is_pinned {
            // For pinned versions, extract version from requirement
            if let Some(ref req) = extracted_requirement {
                if let Some(version) = extract_version_from_requirement(req) {
                    match PackageUrl::new("cran", &name) {
                        Ok(mut p) => {
                            if p.with_version(&version).is_ok() {
                                Some(p.to_string())
                            } else {
                                // Failed to set version, create without it
                                PackageUrl::new("cran", &name).ok().map(|p| p.to_string())
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create PURL for CRAN dependency '{}': {}",
                                name, e
                            );
                            None
                        }
                    }
                } else {
                    // No version found in requirement
                    PackageUrl::new("cran", &name).ok().map(|p| p.to_string())
                }
            } else {
                // No requirement
                PackageUrl::new("cran", &name).ok().map(|p| p.to_string())
            }
        } else {
            // Not pinned, create PURL without version
            PackageUrl::new("cran", &name).ok().map(|p| p.to_string())
        };

        dependencies.push(Dependency {
            purl,
            extracted_requirement: extracted_requirement.map(truncate_field),
            scope: scope.map(|s| truncate_field(s.to_string())),
            is_runtime: Some(scope.is_none() || scope == Some("imports")),
            is_optional: Some(scope == Some("suggests") || scope == Some("enhances")),
            is_pinned: Some(is_pinned),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    dependencies
}

static VERSION_CONSTRAINT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9.]+)\s*\(([><=]+)\s*([0-9.]+)\)\s*$").expect("valid regex")
});

/// Examples:
/// - "cli (>= 3.6.2)" -> ("cli", Some(">= 3.6.2"), true)
/// - "generics" -> ("generics", None, false)
/// - "glue (== 1.3.2)" -> ("glue", Some("== 1.3.2"), true)
fn parse_version_constraint(dep: &str) -> (String, Option<String>, bool) {
    if let Some(captures) = VERSION_CONSTRAINT_RE.captures(dep) {
        let name = match captures.get(1) {
            Some(m) => truncate_field(m.as_str().to_string()),
            None => return (truncate_field(dep.trim().to_string()), None, false),
        };
        let operator = match captures.get(2) {
            Some(m) => m.as_str(),
            None => return (name, None, false),
        };
        let version = match captures.get(3) {
            Some(m) => m.as_str(),
            None => return (name, None, false),
        };
        let requirement = truncate_field(format!("{} {}", operator, version));
        let is_pinned = operator == "==";

        (name, Some(requirement), is_pinned)
    } else {
        (truncate_field(dep.trim().to_string()), None, false)
    }
}

/// Extract version number from a requirement string like ">= 3.6.2" or "== 1.0.0".
fn extract_version_from_requirement(requirement: &str) -> Option<String> {
    requirement
        .split_whitespace()
        .nth(1)
        .map(|s| truncate_field(s.to_string()))
}

/// Build description from Title and Description fields.
fn build_description(fields: &HashMap<String, String>) -> Option<String> {
    let title = fields.get("Title").map(|s| s.trim());
    let desc = fields.get("Description").map(|s| s.trim());

    match (title, desc) {
        (Some(t), Some(d)) if !t.is_empty() && !d.is_empty() => {
            Some(truncate_field(format!("{}\n{}", t, d)))
        }
        (Some(t), _) if !t.is_empty() => Some(truncate_field(t.to_string())),
        (_, Some(d)) if !d.is_empty() => Some(truncate_field(d.to_string())),
        _ => None,
    }
}

fn split_author_entries(author_str: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut bracket_depth: usize = 0;
    let mut paren_depth: usize = 0;

    for (idx, ch) in author_str.char_indices().take(MAX_ITERATION_COUNT) {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ',' if bracket_depth == 0 && paren_depth == 0 => {
                let entry = author_str[start..idx].trim();
                if !entry.is_empty() {
                    entries.push(entry);
                }
                start = idx + 1;
            }
            _ => {}
        }
    }

    let final_entry = author_str[start..].trim();
    if !final_entry.is_empty() {
        entries.push(final_entry);
    }

    entries
}

/// Parse party information from Author or Maintainer field.
///
/// Formats supported:
/// - "Name <email@domain.com>"
/// - "Name"
/// - "email@domain.com"
fn parse_party(info: &str, role: &str) -> Option<Party> {
    let info = info.trim();
    if info.is_empty() {
        return None;
    }

    // Check for "Name <email>" format
    if info.contains('<') && info.contains('>') {
        let parts: Vec<&str> = info.split('<').collect();
        if parts.len() == 2 {
            let name = parts[0].trim().to_string();
            let email = parts[1].trim_end_matches('>').trim().to_string();

            if !email.contains('@') {
                return Some(Party {
                    r#type: Some(truncate_field("person".to_string())),
                    role: Some(truncate_field(role.to_string())),
                    name: Some(truncate_field(info.to_string())),
                    email: None,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                });
            }

            return Some(Party {
                r#type: Some(truncate_field("person".to_string())),
                role: Some(truncate_field(role.to_string())),
                name: if name.is_empty() {
                    None
                } else {
                    Some(truncate_field(name))
                },
                email: if email.is_empty() {
                    None
                } else {
                    Some(truncate_field(email))
                },
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }
    }

    // Just a name or email
    Some(Party {
        r#type: Some(truncate_field("person".to_string())),
        role: Some(truncate_field(role.to_string())),
        name: Some(truncate_field(info.to_string())),
        email: None,
        url: None,
        organization: None,
        organization_url: None,
        timezone: None,
    })
}

/// Create a package URL for a CRAN package.
fn create_package_url(name: &Option<String>, version: &Option<String>) -> Option<String> {
    name.as_ref().and_then(|name| {
        let mut package_url = match PackageUrl::new("cran", name) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to create PackageUrl for CRAN package '{}': {}",
                    name, e
                );
                return None;
            }
        };

        if let Some(v) = version
            && let Err(e) = package_url.with_version(v)
        {
            warn!(
                "Failed to set version '{}' for CRAN package '{}': {}",
                v, name, e
            );
            return None;
        }

        Some(package_url.to_string())
    })
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(CranParser::PACKAGE_TYPE),
        primary_language: Some("R".to_string()),
        datasource_id: Some(DatasourceId::CranDescription),
        ..Default::default()
    }
}

crate::register_parser!(
    "CRAN R package DESCRIPTION file",
    &["**/DESCRIPTION"],
    "cran",
    "R",
    Some("https://cran.r-project.org/doc/manuals/r-release/R-exts.html#The-DESCRIPTION-file"),
);
