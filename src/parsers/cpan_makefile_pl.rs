//! Parser for CPAN Perl Makefile.PL files.
//!
//! Extracts Perl package metadata from `Makefile.PL` files used by ExtUtils::MakeMaker.
//!
//! # Supported Formats
//! - `Makefile.PL` - CPAN ExtUtils::MakeMaker build configuration
//!
//! # Implementation Notes
//! - Format: Perl script with WriteMakefile() or WriteMakefile1() calls
//! - Spec: https://metacpan.org/pod/ExtUtils::MakeMaker
//! - Extracts: NAME, VERSION, AUTHOR, LICENSE, ABSTRACT, PREREQ_PM, BUILD_REQUIRES, TEST_REQUIRES, CONFIGURE_REQUIRES
//! - Uses regex-based extraction (no Perl code execution for security)
//! - Python reference has stub-only handler with no parse() method - this is BEYOND PARITY

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use regex::Regex;
use serde_json::json;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    empty_declared_license_data, normalize_declared_license_key, normalize_spdx_expression,
};

static RE_WRITEMAKEFILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"WriteMakefile1?\s*\(").unwrap());
static RE_SIMPLE_KV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*([A-Z_]+)\s*=>\s*(?:'([^']*)'|"([^"]*)"|q\{([^}]*)\}|q\(([^)]*)\))"#)
        .unwrap()
});
static RE_HASH_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Z_]+)\s*=>\s*\{([^}]*)\}").unwrap());
static RE_AUTHOR_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"AUTHOR\s*=>\s*\[([^\]]*)\]").unwrap());
static RE_QUOTED_STRING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"['"]([^'"]*)['"']"#).unwrap());
static RE_DEP_PAIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"['"]([^'"]+)['"]\s*=>\s*(?:'([^']*)'|"([^"]*)"|(\d+))"#).unwrap()
});
static RE_VERSION_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^\s*(?:our\s+)?\$(?:[A-Za-z_][\w:]*::)?VERSION\s*=\s*(?:'([^']+)'|"([^"]+)")"#,
    )
    .unwrap()
});

const PACKAGE_TYPE: PackageType = PackageType::Cpan;
const MAX_METADATA_FILE_SIZE: u64 = 1024 * 1024;

pub struct CpanMakefilePlParser;

impl PackageParser for CpanMakefilePlParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "Makefile.PL")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Makefile.PL file {:?}: {}", path, e);
                return vec![PackageData {
                    package_type: Some(PACKAGE_TYPE),
                    primary_language: Some("Perl".to_string()),
                    datasource_id: Some(DatasourceId::CpanMakefile),
                    ..Default::default()
                }];
            }
        };

        vec![parse_makefile_pl_with_base(&content, path.parent())]
    }
}

#[cfg(test)]
pub(crate) fn parse_makefile_pl(content: &str) -> PackageData {
    parse_makefile_pl_with_base(content, None)
}

pub(crate) fn parse_makefile_pl_with_base(content: &str, base_dir: Option<&Path>) -> PackageData {
    // Find WriteMakefile or WriteMakefile1 call
    let makefile_block = extract_writemakefile_block(content);
    if makefile_block.is_empty() {
        return default_package_data();
    }

    let fields = parse_hash_fields(&makefile_block);

    let name = fields.get("NAME").map(|n| truncate_field(n.to_string()));
    let resolved_metadata = resolve_referenced_metadata(&fields, base_dir);

    let version = fields
        .get("VERSION")
        .map(|v| truncate_field(v.to_string()))
        .or_else(|| resolved_metadata.version.clone());
    let description = fields
        .get("ABSTRACT")
        .map(|d| truncate_field(d.to_string()))
        .or_else(|| resolved_metadata.abstract_text.clone());
    let extracted_license_statement = fields.get("LICENSE").map(|l| truncate_field(l.to_string()));
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        extracted_license_statement
            .as_deref()
            .and_then(normalize_cpan_makefile_license)
            .map(|normalized| {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(
                        extracted_license_statement.as_deref().unwrap_or_default(),
                    ),
                )
            })
            .unwrap_or_else(empty_declared_license_data);

    let parties = parse_author(&fields);
    let dependencies = parse_dependencies(&fields);

    let mut extra_data = HashMap::new();
    if let Some(min_perl) = fields.get("MIN_PERL_VERSION") {
        extra_data.insert(
            "MIN_PERL_VERSION".to_string(),
            json!(truncate_field(min_perl.to_string())),
        );
    }
    if let Some(version_from) = fields.get("VERSION_FROM") {
        extra_data.insert(
            "VERSION_FROM".to_string(),
            json!(truncate_field(version_from.to_string())),
        );
    }
    if let Some(abstract_from) = fields.get("ABSTRACT_FROM") {
        extra_data.insert(
            "ABSTRACT_FROM".to_string(),
            json!(truncate_field(abstract_from.to_string())),
        );
    }

    // Build PURL: convert Foo::Bar to Foo-Bar for CPAN naming convention
    let purl = name.as_ref().and_then(|n| {
        let purl_name = n.replace("::", "-");
        PackageUrl::new("cpan", &purl_name).ok().map(|mut p| {
            if let Some(v) = &version {
                let _ = p.with_version(v).ok();
            }
            p.to_string()
        })
    });

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: Some("cpan".to_string()),
        name,
        version,
        description,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        parties,
        dependencies,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        purl,
        datasource_id: Some(DatasourceId::CpanMakefile),
        primary_language: Some("Perl".to_string()),
        ..Default::default()
    }
}

#[derive(Default)]
struct ResolvedMetadata {
    version: Option<String>,
    abstract_text: Option<String>,
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("Perl".to_string()),
        datasource_id: Some(DatasourceId::CpanMakefile),
        ..Default::default()
    }
}

fn normalize_cpan_makefile_license(value: &str) -> Option<NormalizedDeclaredLicense> {
    match value.trim() {
        "perl_5" | "Perl_5" => Some(NormalizedDeclaredLicense::new(
            "gpl-1.0-plus OR artistic-perl-1.0",
            "GPL-1.0-or-later OR Artistic-1.0-Perl",
        )),
        "artistic_2" => Some(NormalizedDeclaredLicense::new(
            "artistic-2.0",
            "Artistic-2.0",
        )),
        "apache_2_0" => Some(NormalizedDeclaredLicense::new("apache-2.0", "Apache-2.0")),
        other => normalize_spdx_expression(other).or_else(|| normalize_declared_license_key(other)),
    }
}

fn resolve_referenced_metadata(
    fields: &HashMap<String, String>,
    base_dir: Option<&Path>,
) -> ResolvedMetadata {
    let Some(base_dir) = base_dir else {
        return ResolvedMetadata::default();
    };

    let mut resolved = ResolvedMetadata::default();
    let mut cache: HashMap<String, Option<String>> = HashMap::new();

    if let Some(version_from) = fields.get("VERSION_FROM")
        && let Some(content) = load_referenced_metadata_file(base_dir, version_from, &mut cache)
    {
        resolved.version = extract_version_from_module_content(content);
    }

    if let Some(abstract_from) = fields.get("ABSTRACT_FROM")
        && let Some(content) = load_referenced_metadata_file(base_dir, abstract_from, &mut cache)
    {
        resolved.abstract_text = extract_abstract_from_module_content(content);
    }

    resolved
}

fn load_referenced_metadata_file<'a>(
    base_dir: &Path,
    relative_path: &str,
    cache: &'a mut HashMap<String, Option<String>>,
) -> Option<&'a String> {
    let entry = cache
        .entry(relative_path.to_string())
        .or_insert_with(|| read_safe_metadata_file(base_dir, relative_path));
    entry.as_ref()
}

fn read_safe_metadata_file(base_dir: &Path, relative_path: &str) -> Option<String> {
    let ref_path = Path::new(relative_path);
    if ref_path.is_absolute() {
        return None;
    }

    let base_dir = base_dir.canonicalize().ok()?;
    let candidate = base_dir.join(ref_path);
    let canonical_candidate = candidate.canonicalize().ok()?;
    if !canonical_candidate.starts_with(&base_dir) {
        return None;
    }

    let metadata = std::fs::metadata(&canonical_candidate).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_METADATA_FILE_SIZE {
        return None;
    }

    read_file_to_string(&canonical_candidate, None).ok()
}

fn extract_version_from_module_content(content: &str) -> Option<String> {
    RE_VERSION_ASSIGNMENT
        .captures(content)
        .and_then(|caps| caps.get(1).or_else(|| caps.get(2)))
        .map(|m| m.as_str().trim().to_string())
        .map(truncate_field)
        .filter(|value| !value.is_empty())
}

fn extract_abstract_from_module_content(content: &str) -> Option<String> {
    let mut in_name_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "=head1 NAME" {
            in_name_section = true;
            continue;
        }

        if in_name_section {
            if trimmed.starts_with('=') {
                break;
            }
            if trimmed.is_empty() {
                continue;
            }

            if let Some((_, abstract_text)) = trimmed.split_once(" - ") {
                let abstract_text = abstract_text.trim();
                if !abstract_text.is_empty() {
                    return Some(truncate_field(abstract_text.to_string()));
                }
            }
        }
    }

    None
}

fn extract_writemakefile_block(content: &str) -> String {
    let start_match = match RE_WRITEMAKEFILE.find(content) {
        Some(m) => m,
        None => return String::new(),
    };

    let start_pos = start_match.end();
    let content_from_start = &content[start_pos..];

    // Find the matching closing parenthesis
    let mut depth = 1;
    let mut end_pos = 0;
    let chars: Vec<char> = content_from_start.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if i >= MAX_ITERATION_COUNT {
            break;
        }
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if end_pos > 0 {
        content_from_start[..end_pos].to_string()
    } else {
        String::new()
    }
}

fn parse_hash_fields(content: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    for cap in RE_SIMPLE_KV
        .captures_iter(content)
        .take(MAX_ITERATION_COUNT)
    {
        let key = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let value = cap
            .get(2)
            .or_else(|| cap.get(3))
            .or_else(|| cap.get(4))
            .or_else(|| cap.get(5))
            .map(|m| m.as_str().to_string());

        if let Some(v) = value {
            fields.insert(key, v);
        }
    }

    // Parse hash values (PREREQ_PM, BUILD_REQUIRES, etc.)
    parse_hash_dependencies(content, &mut fields);

    // Parse array refs for AUTHOR
    parse_author_array(content, &mut fields);

    fields
}

fn parse_hash_dependencies(content: &str, fields: &mut HashMap<String, String>) {
    for cap in RE_HASH_BLOCK
        .captures_iter(content)
        .take(MAX_ITERATION_COUNT)
    {
        let key = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let hash_content = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        // For dependency hashes, we'll store them with a special marker
        // so parse_dependencies can find them
        if matches!(
            key,
            "PREREQ_PM" | "BUILD_REQUIRES" | "TEST_REQUIRES" | "CONFIGURE_REQUIRES"
        ) {
            fields.insert(format!("_HASH_{}", key), hash_content.to_string());
        }
    }
}

fn parse_author_array(content: &str, fields: &mut HashMap<String, String>) {
    if let Some(cap) = RE_AUTHOR_ARRAY.captures(content) {
        let array_content = cap.get(1).map(|m| m.as_str()).unwrap_or("");

        let authors: Vec<String> = RE_QUOTED_STRING
            .captures_iter(array_content)
            .take(MAX_ITERATION_COUNT)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();

        if !authors.is_empty() {
            // Store as JSON array for later processing
            fields.insert("_ARRAY_AUTHOR".to_string(), authors.join("||"));
        }
    }
}

fn parse_author(fields: &HashMap<String, String>) -> Vec<Party> {
    // Check for array of authors first
    if let Some(authors_str) = fields.get("_ARRAY_AUTHOR") {
        return authors_str
            .split("||")
            .filter_map(|author_str| {
                if author_str.trim().is_empty() {
                    return None;
                }
                let (name, email) = parse_author_string(author_str);
                Some(Party {
                    role: Some("author".to_string()),
                    name,
                    email,
                    r#type: Some("person".to_string()),
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                })
            })
            .collect();
    }

    if let Some(author_str) = fields.get("AUTHOR") {
        let (name, email) = parse_author_string(author_str);
        return vec![Party {
            role: Some("author".to_string()),
            name,
            email,
            r#type: Some("person".to_string()),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        }];
    }

    Vec::new()
}

fn parse_author_string(s: &str) -> (Option<String>, Option<String>) {
    if let Some(start) = s.find('<')
        && let Some(end) = s.find('>')
        && start < end
    {
        let name = s[..start].trim();
        let email = s[start + 1..end].trim();
        return (
            if name.is_empty() {
                None
            } else {
                Some(truncate_field(name.to_string()))
            },
            if email.is_empty() {
                None
            } else {
                Some(truncate_field(email.to_string()))
            },
        );
    }
    (Some(truncate_field(s.trim().to_string())), None)
}

fn parse_dependencies(fields: &HashMap<String, String>) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    // Parse PREREQ_PM as runtime dependencies
    if let Some(hash_content) = fields.get("_HASH_PREREQ_PM") {
        dependencies.extend(extract_deps_from_hash(hash_content, "runtime", true));
    }

    // Parse BUILD_REQUIRES
    if let Some(hash_content) = fields.get("_HASH_BUILD_REQUIRES") {
        dependencies.extend(extract_deps_from_hash(hash_content, "build", false));
    }

    // Parse TEST_REQUIRES
    if let Some(hash_content) = fields.get("_HASH_TEST_REQUIRES") {
        dependencies.extend(extract_deps_from_hash(hash_content, "test", false));
    }

    // Parse CONFIGURE_REQUIRES
    if let Some(hash_content) = fields.get("_HASH_CONFIGURE_REQUIRES") {
        dependencies.extend(extract_deps_from_hash(hash_content, "configure", false));
    }

    dependencies
}

fn extract_deps_from_hash(hash_content: &str, scope: &str, is_runtime: bool) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for cap in RE_DEP_PAIR
        .captures_iter(hash_content)
        .take(MAX_ITERATION_COUNT)
    {
        let module_name = cap.get(1).map(|m| m.as_str()).unwrap_or("");

        // Skip perl itself
        if module_name == "perl" {
            continue;
        }

        let version = cap
            .get(2)
            .or_else(|| cap.get(3))
            .or_else(|| cap.get(4))
            .map(|m| m.as_str());

        let extracted_requirement = match version {
            Some("0") | Some("") | None => None,
            Some(v) => Some(truncate_field(v.to_string())),
        };

        let purl = PackageUrl::new("cpan", module_name)
            .ok()
            .map(|p| p.to_string());

        deps.push(Dependency {
            purl,
            extracted_requirement,
            scope: Some(truncate_field(scope.to_string())),
            is_runtime: Some(is_runtime),
            is_optional: Some(false),
            is_pinned: None,
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        });
    }

    deps
}

crate::register_parser!(
    "CPAN Perl Makefile.PL",
    &["*/Makefile.PL"],
    "cpan",
    "Perl",
    Some("https://metacpan.org/pod/ExtUtils::MakeMaker"),
);
