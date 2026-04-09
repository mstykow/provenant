//! Parser for Debian package metadata files.
//!
//! Extracts package metadata from Debian package management files using RFC 822
//! format parsing for control files and installed package databases.
//!
//! # Supported Formats
//! - `debian/control` (Source package control files - multi-paragraph)
//! - `/var/lib/dpkg/status` (Installed package database - multi-paragraph)
//! - `/var/lib/dpkg/status.d/*` (Distroless installed packages)
//! - `*.dsc` (Debian source control files)
//! - `*.orig.tar.*` (Original upstream tarballs)
//! - `*.debian.tar.*` (Debian packaging tarballs)
//! - `/var/lib/dpkg/info/*.list` (Installed file lists)
//! - `/var/lib/dpkg/info/*.md5sums` (Installed file checksums)
//! - `debian/copyright` (Copyright/license declarations)
//! - `*.deb` (Debian binary package archives)
//! - `control` (extracted from .deb archives)
//! - `md5sums` (extracted from .deb archives)
//!
//! # Key Features
//! - RFC 822 format parsing for control files
//! - Dependency extraction with scope tracking (Depends, Build-Depends, etc.)
//! - Debian vs Ubuntu namespace detection from version and maintainer fields
//! - Multi-paragraph record parsing for package databases
//! - License and copyright information extraction
//! - Package URL (purl) generation with namespace
//!
//! # Implementation Notes
//! - Uses RFC 822 parser from `crate::parsers::rfc822` module
//! - Multi-paragraph records separated by blank lines
//! - Graceful error handling with `warn!()` logs

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use regex::Regex;

use crate::models::{
    DatasourceId, Dependency, FileReference, LicenseDetection, Md5Digest, PackageData, PackageType,
    Party,
};
use crate::parsers::rfc822::{self, Rfc822Metadata};
use crate::parsers::utils::{read_file_to_string, split_name_email};
use crate::utils::spdx::combine_license_expressions;

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_detection,
    normalize_declared_license_key,
};

const PACKAGE_TYPE: PackageType = PackageType::Deb;

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

// Namespace detection clues from version strings
const VERSION_CLUES_DEBIAN: &[&str] = &["deb"];
const VERSION_CLUES_UBUNTU: &[&str] = &["ubuntu"];

// Namespace detection clues from maintainer fields
const MAINTAINER_CLUES_DEBIAN: &[&str] = &[
    "packages.debian.org",
    "lists.debian.org",
    "lists.alioth.debian.org",
    "@debian.org",
    "debian-init-diversity@",
];
const MAINTAINER_CLUES_UBUNTU: &[&str] = &["lists.ubuntu.com", "@canonical.com"];

// Dependency field names and their scope/flags
struct DepFieldSpec {
    field: &'static str,
    scope: &'static str,
    is_runtime: bool,
    is_optional: bool,
}

const DEP_FIELDS: &[DepFieldSpec] = &[
    DepFieldSpec {
        field: "depends",
        scope: "depends",
        is_runtime: true,
        is_optional: false,
    },
    DepFieldSpec {
        field: "pre-depends",
        scope: "pre-depends",
        is_runtime: true,
        is_optional: false,
    },
    DepFieldSpec {
        field: "recommends",
        scope: "recommends",
        is_runtime: true,
        is_optional: true,
    },
    DepFieldSpec {
        field: "suggests",
        scope: "suggests",
        is_runtime: true,
        is_optional: true,
    },
    DepFieldSpec {
        field: "breaks",
        scope: "breaks",
        is_runtime: false,
        is_optional: false,
    },
    DepFieldSpec {
        field: "conflicts",
        scope: "conflicts",
        is_runtime: false,
        is_optional: false,
    },
    DepFieldSpec {
        field: "replaces",
        scope: "replaces",
        is_runtime: false,
        is_optional: false,
    },
    DepFieldSpec {
        field: "provides",
        scope: "provides",
        is_runtime: false,
        is_optional: false,
    },
    DepFieldSpec {
        field: "build-depends",
        scope: "build-depends",
        is_runtime: false,
        is_optional: false,
    },
    DepFieldSpec {
        field: "build-depends-indep",
        scope: "build-depends-indep",
        is_runtime: false,
        is_optional: false,
    },
    DepFieldSpec {
        field: "build-conflicts",
        scope: "build-conflicts",
        is_runtime: false,
        is_optional: false,
    },
];

// ---------------------------------------------------------------------------
// DebianControlParser: debian/control files (source + binary paragraphs)
// ---------------------------------------------------------------------------

pub struct DebianControlParser;

impl PackageParser for DebianControlParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        if let Some(name) = path.file_name()
            && name == "control"
            && let Some(parent) = path.parent()
            && let Some(parent_name) = parent.file_name()
        {
            return parent_name == "debian";
        }
        false
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read debian/control at {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianControlInSource)];
            }
        };

        let packages = parse_debian_control(&content);
        if packages.is_empty() {
            vec![default_package_data(DatasourceId::DebianControlInSource)]
        } else {
            packages
        }
    }
}

// ---------------------------------------------------------------------------
// DebianInstalledParser: /var/lib/dpkg/status
// ---------------------------------------------------------------------------

pub struct DebianInstalledParser;

impl PackageParser for DebianInstalledParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.ends_with("var/lib/dpkg/status")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read dpkg/status at {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianInstalledStatusDb)];
            }
        };

        let packages = parse_dpkg_status(&content);
        if packages.is_empty() {
            vec![default_package_data(DatasourceId::DebianInstalledStatusDb)]
        } else {
            packages
        }
    }
}

pub struct DebianDistrolessInstalledParser;

impl PackageParser for DebianDistrolessInstalledParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.contains("var/lib/dpkg/status.d/")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read distroless status file at {:?}: {}", path, e);
                return vec![default_package_data(
                    DatasourceId::DebianDistrolessInstalledDb,
                )];
            }
        };

        vec![parse_distroless_status(&content)]
    }
}

fn parse_distroless_status(content: &str) -> PackageData {
    let paragraphs = rfc822::parse_rfc822_paragraphs(content);

    if paragraphs.is_empty() {
        return default_package_data(DatasourceId::DebianDistrolessInstalledDb);
    }

    build_package_from_paragraph(
        &paragraphs[0],
        None,
        DatasourceId::DebianDistrolessInstalledDb,
    )
    .unwrap_or_else(|| default_package_data(DatasourceId::DebianDistrolessInstalledDb))
}

// ---------------------------------------------------------------------------
// Parsing logic
// ---------------------------------------------------------------------------

/// Parses a debian/control file into PackageData entries.
///
/// A debian/control file has a Source paragraph followed by one or more Binary
/// paragraphs. Source-level metadata (maintainer, homepage, VCS URLs) is merged
/// into each binary package.
fn parse_debian_control(content: &str) -> Vec<PackageData> {
    let paragraphs = rfc822::parse_rfc822_paragraphs(content);
    if paragraphs.is_empty() {
        return Vec::new();
    }

    // Determine if first paragraph is a Source paragraph
    let has_source = rfc822::get_header_first(&paragraphs[0].headers, "source").is_some();

    let (source_paragraph, binary_start) = if has_source {
        (Some(&paragraphs[0]), 1)
    } else {
        (None, 0)
    };

    // Extract source-level shared metadata
    let source_meta = source_paragraph.map(extract_source_meta);

    let mut packages = Vec::new();

    for para in &paragraphs[binary_start..] {
        if let Some(pkg) = build_package_from_paragraph(
            para,
            source_meta.as_ref(),
            DatasourceId::DebianControlInSource,
        ) {
            packages.push(pkg);
        }
    }

    if packages.is_empty()
        && let Some(source_para) = source_paragraph
        && let Some(pkg) = build_package_from_source_paragraph(source_para)
    {
        packages.push(pkg);
    }

    packages
}

/// Parses a dpkg/status file into PackageData entries.
///
/// Each paragraph represents an installed package. Only packages with
/// `Status: install ok installed` are included.
fn parse_dpkg_status(content: &str) -> Vec<PackageData> {
    let paragraphs = rfc822::parse_rfc822_paragraphs(content);
    let mut packages = Vec::new();

    for para in &paragraphs {
        let status = rfc822::get_header_first(&para.headers, "status");
        if status.as_deref() != Some("install ok installed") {
            continue;
        }

        if let Some(pkg) =
            build_package_from_paragraph(para, None, DatasourceId::DebianInstalledStatusDb)
        {
            packages.push(pkg);
        }
    }

    packages
}

// ---------------------------------------------------------------------------
// Source paragraph metadata (shared across binary packages)
// ---------------------------------------------------------------------------

struct SourceMeta {
    parties: Vec<Party>,
    homepage_url: Option<String>,
    vcs_url: Option<String>,
    code_view_url: Option<String>,
    bug_tracking_url: Option<String>,
}

fn extract_source_meta(paragraph: &Rfc822Metadata) -> SourceMeta {
    let mut parties = Vec::new();

    // Maintainer
    if let Some(maintainer) = rfc822::get_header_first(&paragraph.headers, "maintainer") {
        let (name, email) = split_name_email(&maintainer);
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name,
            email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Original-Maintainer
    if let Some(orig_maintainer) =
        rfc822::get_header_first(&paragraph.headers, "original-maintainer")
    {
        let (name, email) = split_name_email(&orig_maintainer);
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("maintainer".to_string()),
            name,
            email,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Uploaders (comma-separated)
    if let Some(uploaders_str) = rfc822::get_header_first(&paragraph.headers, "uploaders") {
        for uploader in uploaders_str.split(',') {
            let trimmed = uploader.trim();
            if !trimmed.is_empty() {
                let (name, email) = split_name_email(trimmed);
                parties.push(Party {
                    r#type: Some("person".to_string()),
                    role: Some("uploader".to_string()),
                    name,
                    email,
                    url: None,
                    organization: None,
                    organization_url: None,
                    timezone: None,
                });
            }
        }
    }

    let homepage_url = rfc822::get_header_first(&paragraph.headers, "homepage");

    // VCS-Git: may contain branch info after space
    let vcs_url = rfc822::get_header_first(&paragraph.headers, "vcs-git")
        .map(|url| url.split_whitespace().next().unwrap_or(&url).to_string());

    let code_view_url = rfc822::get_header_first(&paragraph.headers, "vcs-browser");

    let bug_tracking_url = rfc822::get_header_first(&paragraph.headers, "bugs");

    SourceMeta {
        parties,
        homepage_url,
        vcs_url,
        code_view_url,
        bug_tracking_url,
    }
}

// ---------------------------------------------------------------------------
// Package building
// ---------------------------------------------------------------------------

fn build_package_from_paragraph(
    paragraph: &Rfc822Metadata,
    source_meta: Option<&SourceMeta>,
    datasource_id: DatasourceId,
) -> Option<PackageData> {
    let name = rfc822::get_header_first(&paragraph.headers, "package")?;
    let version = rfc822::get_header_first(&paragraph.headers, "version");
    let architecture = rfc822::get_header_first(&paragraph.headers, "architecture");
    let description = rfc822::get_header_first(&paragraph.headers, "description");
    let maintainer_str = rfc822::get_header_first(&paragraph.headers, "maintainer");
    let homepage = rfc822::get_header_first(&paragraph.headers, "homepage");
    let source_field = rfc822::get_header_first(&paragraph.headers, "source");
    let section = rfc822::get_header_first(&paragraph.headers, "section");
    let installed_size = rfc822::get_header_first(&paragraph.headers, "installed-size");
    let multi_arch = rfc822::get_header_first(&paragraph.headers, "multi-arch");

    let namespace = detect_namespace(version.as_deref(), maintainer_str.as_deref());

    // Build parties: use source_meta parties if available, otherwise parse from paragraph
    let parties = if let Some(meta) = source_meta {
        meta.parties.clone()
    } else {
        let mut p = Vec::new();
        if let Some(m) = &maintainer_str {
            let (n, e) = split_name_email(m);
            p.push(Party {
                r#type: Some("person".to_string()),
                role: Some("maintainer".to_string()),
                name: n,
                email: e,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }
        p
    };

    // Resolve homepage: paragraph's own, or from source metadata
    let homepage_url = homepage.or_else(|| source_meta.and_then(|m| m.homepage_url.clone()));
    let vcs_url = source_meta.and_then(|m| m.vcs_url.clone());
    let code_view_url = source_meta.and_then(|m| m.code_view_url.clone());
    let bug_tracking_url = source_meta.and_then(|m| m.bug_tracking_url.clone());

    // Build PURL
    let purl = build_debian_purl(
        &name,
        version.as_deref(),
        namespace.as_deref(),
        architecture.as_deref(),
    );

    // Parse dependencies from all dependency fields
    let dependencies = parse_all_dependencies(&paragraph.headers, namespace.as_deref());

    // Keywords from section
    let keywords = section.into_iter().collect();

    // Source packages
    let source_packages = parse_source_field(source_field.as_deref(), namespace.as_deref());

    // Extra data
    let mut extra_data: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(ma) = &multi_arch
        && !ma.is_empty()
    {
        extra_data.insert(
            "multi_arch".to_string(),
            serde_json::Value::String(ma.clone()),
        );
    }
    if let Some(size_str) = &installed_size
        && let Ok(size) = size_str.parse::<u64>()
    {
        extra_data.insert(
            "installed_size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(size)),
        );
    }

    // Qualifiers for architecture
    let qualifiers = architecture.as_ref().map(|arch| {
        let mut q = HashMap::new();
        q.insert("arch".to_string(), arch.clone());
        q
    });

    Some(PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name),
        version,
        qualifiers,
        subpath: None,
        primary_language: None,
        description,
        release_date: None,
        parties,
        keywords,
        homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url,
        code_view_url,
        vcs_url,
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
        source_packages,
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(datasource_id),
        purl,
    })
}

fn build_package_from_source_paragraph(paragraph: &Rfc822Metadata) -> Option<PackageData> {
    let name = rfc822::get_header_first(&paragraph.headers, "source")?;
    let version = rfc822::get_header_first(&paragraph.headers, "version");
    let maintainer_str = rfc822::get_header_first(&paragraph.headers, "maintainer");

    let namespace = detect_namespace(version.as_deref(), maintainer_str.as_deref());
    let source_meta = extract_source_meta(paragraph);

    let purl = build_debian_purl(&name, version.as_deref(), namespace.as_deref(), None);
    let dependencies = parse_all_dependencies(&paragraph.headers, namespace.as_deref());

    let section = rfc822::get_header_first(&paragraph.headers, "section");
    let keywords = section.into_iter().collect();

    Some(PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name),
        version,
        qualifiers: None,
        subpath: None,
        primary_language: None,
        description: None,
        release_date: None,
        parties: source_meta.parties,
        keywords,
        homepage_url: source_meta.homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: source_meta.bug_tracking_url,
        code_view_url: source_meta.code_view_url,
        vcs_url: source_meta.vcs_url,
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
        datasource_id: Some(DatasourceId::DebianControlInSource),
        purl,
    })
}

// ---------------------------------------------------------------------------
// Namespace detection
// ---------------------------------------------------------------------------

fn detect_namespace(version: Option<&str>, maintainer: Option<&str>) -> Option<String> {
    // Check version clues first
    if let Some(ver) = version {
        let ver_lower = ver.to_lowercase();
        for clue in VERSION_CLUES_UBUNTU {
            if ver_lower.contains(clue) {
                return Some("ubuntu".to_string());
            }
        }
        for clue in VERSION_CLUES_DEBIAN {
            if ver_lower.contains(clue) {
                return Some("debian".to_string());
            }
        }
    }

    // Check maintainer clues
    if let Some(maint) = maintainer {
        let maint_lower = maint.to_lowercase();
        for clue in MAINTAINER_CLUES_UBUNTU {
            if maint_lower.contains(clue) {
                return Some("ubuntu".to_string());
            }
        }
        for clue in MAINTAINER_CLUES_DEBIAN {
            if maint_lower.contains(clue) {
                return Some("debian".to_string());
            }
        }
    }

    // Default to debian
    Some("debian".to_string())
}

// ---------------------------------------------------------------------------
// PURL generation
// ---------------------------------------------------------------------------

fn build_debian_purl(
    name: &str,
    version: Option<&str>,
    namespace: Option<&str>,
    architecture: Option<&str>,
) -> Option<String> {
    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(ns) = namespace {
        purl.with_namespace(ns).ok()?;
    }

    if let Some(ver) = version {
        purl.with_version(ver).ok()?;
    }

    if let Some(arch) = architecture {
        purl.add_qualifier("arch", arch).ok()?;
    }

    Some(purl.to_string())
}

// ---------------------------------------------------------------------------
// Dependency parsing
// ---------------------------------------------------------------------------

fn parse_all_dependencies(
    headers: &HashMap<String, Vec<String>>,
    namespace: Option<&str>,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for spec in DEP_FIELDS {
        if let Some(dep_str) = rfc822::get_header_first(headers, spec.field) {
            dependencies.extend(parse_dependency_field(
                &dep_str,
                spec.scope,
                spec.is_runtime,
                spec.is_optional,
                namespace,
            ));
        }
    }

    dependencies
}

/// Parses a Debian dependency field value.
///
/// Debian dependencies are comma-separated, with optional version constraints
/// in parentheses and alternative packages separated by `|`.
///
/// Format: `pkg1 (>= 1.0), pkg2 | pkg3 (<< 2.0), pkg4`
///
/// Alternatives (|) are treated as separate optional dependencies.
fn parse_dependency_field(
    dep_str: &str,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
    namespace: Option<&str>,
) -> Vec<Dependency> {
    let mut deps = Vec::new();

    // Regex for parsing individual dependency: name (operator version)
    // Debian operators: <<, <=, =, >=, >>
    let dep_re = Regex::new(
        r"^\s*([a-zA-Z0-9][a-zA-Z0-9.+\-]+)\s*(?:\(([<>=!]+)\s*([^)]+)\))?\s*(?:\[.*\])?\s*$",
    )
    .unwrap();

    for group in dep_str.split(',') {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }

        // Handle alternatives (|)
        let alternatives: Vec<&str> = group.split('|').collect();
        let has_alternatives = alternatives.len() > 1;

        for alt in alternatives {
            let alt = alt.trim();
            if alt.is_empty() {
                continue;
            }

            if let Some(caps) = dep_re.captures(alt) {
                let pkg_name = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let operator = caps.get(2).map(|m| m.as_str().trim());
                let version = caps.get(3).map(|m| m.as_str().trim());

                if pkg_name.is_empty() {
                    continue;
                }

                // Skip substitution variables like ${shlibs:Depends}
                if pkg_name.starts_with('$') {
                    continue;
                }

                let extracted_requirement = match (operator, version) {
                    (Some(op), Some(ver)) => Some(format!("{} {}", op, ver)),
                    _ => None,
                };

                let is_pinned = operator.map(|op| op == "=");

                let purl = build_debian_purl(pkg_name, None, namespace, None);

                deps.push(Dependency {
                    purl,
                    extracted_requirement,
                    scope: Some(scope.to_string()),
                    is_runtime: Some(is_runtime),
                    is_optional: Some(is_optional || has_alternatives),
                    is_pinned,
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                });
            }
        }
    }

    deps
}

// ---------------------------------------------------------------------------
// Source field parsing
// ---------------------------------------------------------------------------

/// Parses the Source field which may contain a version in parentheses.
///
/// Format: `source-name` or `source-name (version)`
fn parse_source_field(source: Option<&str>, namespace: Option<&str>) -> Vec<String> {
    let Some(source_str) = source else {
        return Vec::new();
    };

    let trimmed = source_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Extract name and optional version from "name (version)" format
    let (name, version) = if let Some(paren_start) = trimmed.find(" (") {
        let name = trimmed[..paren_start].trim();
        let version = trimmed[paren_start + 2..].trim_end_matches(')').trim();
        (
            name,
            if version.is_empty() {
                None
            } else {
                Some(version)
            },
        )
    } else {
        (trimmed, None)
    };

    if let Some(purl) = build_debian_purl(name, version, namespace, None) {
        vec![purl]
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Parser registration macros
// ---------------------------------------------------------------------------

crate::register_parser!(
    "Debian source package control file (debian/control)",
    &["**/debian/control"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

crate::register_parser!(
    "Debian installed package database (dpkg status)",
    &["**/var/lib/dpkg/status"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

crate::register_parser!(
    "Debian distroless package database (status.d)",
    &["**/var/lib/dpkg/status.d/*"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

// Note: DebianInstalledParser uses try_parse_installed for Vec<PackageData>,
// but we register it for the single-package interface too.

// ============================================================================
// WAVE 2 PARSERS: Additional Debian Format Support
// ============================================================================

/// Parser for Debian Source Control (.dsc) files
pub struct DebianDscParser;

impl PackageParser for DebianDscParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("dsc")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .dsc file {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianSourceControlDsc)];
            }
        };

        vec![parse_dsc_content(&content)]
    }
}

crate::register_parser!(
    "Debian source control file (.dsc)",
    &["**/*.dsc"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

fn strip_pgp_signature(content: &str) -> String {
    let mut result = String::new();
    let mut in_pgp_block = false;
    let mut in_signature = false;

    for line in content.lines() {
        if line.starts_with("-----BEGIN PGP SIGNED MESSAGE-----") {
            in_pgp_block = true;
            continue;
        }
        if line.starts_with("-----BEGIN PGP SIGNATURE-----") {
            in_signature = true;
            continue;
        }
        if line.starts_with("-----END PGP SIGNATURE-----") {
            in_signature = false;
            continue;
        }
        if in_pgp_block && line.starts_with("Hash:") {
            continue;
        }
        if in_pgp_block && line.is_empty() && result.is_empty() {
            in_pgp_block = false;
            continue;
        }
        if !in_signature {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn parse_dsc_content(content: &str) -> PackageData {
    let clean_content = strip_pgp_signature(content);
    let metadata = rfc822::parse_rfc822_content(&clean_content);
    let headers = &metadata.headers;

    let name = rfc822::get_header_first(headers, "source");
    let version = rfc822::get_header_first(headers, "version");
    let architecture = rfc822::get_header_first(headers, "architecture");
    let namespace = Some("debian".to_string());

    let mut package = PackageData {
        datasource_id: Some(DatasourceId::DebianSourceControlDsc),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        description: rfc822::get_header_first(headers, "description"),
        homepage_url: rfc822::get_header_first(headers, "homepage"),
        vcs_url: rfc822::get_header_first(headers, "vcs-git"),
        code_view_url: rfc822::get_header_first(headers, "vcs-browser"),
        ..Default::default()
    };

    // Build PURL with architecture qualifier
    if let (Some(n), Some(v)) = (&name, &version) {
        package.purl = build_debian_purl(n, Some(v), namespace.as_deref(), architecture.as_deref());
    }

    // Set source_packages to point to the source itself (without version)
    if let Some(n) = &name
        && let Some(source_purl) = build_debian_purl(n, None, namespace.as_deref(), None)
    {
        package.source_packages.push(source_purl);
    }

    if let Some(maintainer) = rfc822::get_header_first(headers, "maintainer") {
        let (name_opt, email_opt) = split_name_email(&maintainer);
        package.parties.push(Party {
            r#type: None,
            role: Some("maintainer".to_string()),
            name: name_opt,
            email: email_opt,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    if let Some(uploaders_str) = rfc822::get_header_first(headers, "uploaders") {
        for uploader in uploaders_str.split(',') {
            let uploader = uploader.trim();
            if uploader.is_empty() {
                continue;
            }
            let (name_opt, email_opt) = split_name_email(uploader);
            package.parties.push(Party {
                r#type: None,
                role: Some("uploader".to_string()),
                name: name_opt,
                email: email_opt,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }
    }

    // Parse Build-Depends
    if let Some(build_deps) = rfc822::get_header_first(headers, "build-depends") {
        package.dependencies.extend(parse_dependency_field(
            &build_deps,
            "build",
            false,
            false,
            namespace.as_deref(),
        ));
    }

    // Store Standards-Version in extra_data
    if let Some(standards) = rfc822::get_header_first(headers, "standards-version") {
        let map = package.extra_data.get_or_insert_with(HashMap::new);
        map.insert("standards_version".to_string(), standards.into());
    }

    package
}

/// Parser for Debian original source tarballs (*.orig.tar.*)
pub struct DebianOrigTarParser;

impl PackageParser for DebianOrigTarParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|name| name.contains(".orig.tar."))
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(
                    DatasourceId::DebianOriginalSourceTarball,
                )];
            }
        };

        vec![parse_source_tarball_filename(
            filename,
            DatasourceId::DebianOriginalSourceTarball,
        )]
    }
}

crate::register_parser!(
    "Debian original source tarball",
    &["**/*.orig.tar.*"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-source.html"),
);

/// Parser for Debian source package metadata tarballs (*.debian.tar.*)
pub struct DebianDebianTarParser;

impl PackageParser for DebianDebianTarParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|name| name.contains(".debian.tar."))
            .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(
                    DatasourceId::DebianSourceMetadataTarball,
                )];
            }
        };

        vec![parse_source_tarball_filename(
            filename,
            DatasourceId::DebianSourceMetadataTarball,
        )]
    }
}

crate::register_parser!(
    "Debian source metadata tarball",
    &["**/*.debian.tar.*"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-source.html"),
);

fn parse_source_tarball_filename(filename: &str, datasource_id: DatasourceId) -> PackageData {
    let without_tar_ext = filename
        .trim_end_matches(".gz")
        .trim_end_matches(".xz")
        .trim_end_matches(".bz2")
        .trim_end_matches(".tar");

    let parts: Vec<&str> = without_tar_ext.splitn(2, '_').collect();
    if parts.len() < 2 {
        return default_package_data(datasource_id);
    }

    let name = parts[0].to_string();
    let version_with_suffix = parts[1];

    let version = version_with_suffix
        .trim_end_matches(".orig")
        .trim_end_matches(".debian")
        .to_string();

    let namespace = Some("debian".to_string());

    PackageData {
        datasource_id: Some(datasource_id),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name.clone()),
        version: Some(version.clone()),
        purl: build_debian_purl(&name, Some(&version), namespace.as_deref(), None),
        ..Default::default()
    }
}

/// Parser for Debian installed file lists (*.list)
pub struct DebianInstalledListParser;

impl PackageParser for DebianInstalledListParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("list")
            && path
                .to_str()
                .map(|p| p.contains("/var/lib/dpkg/info/"))
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_stem().and_then(|s| s.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(DatasourceId::DebianInstalledFilesList)];
            }
        };

        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .list file {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianInstalledFilesList)];
            }
        };

        vec![parse_debian_file_list(
            &content,
            filename,
            DatasourceId::DebianInstalledFilesList,
        )]
    }
}

crate::register_parser!(
    "Debian installed files list",
    &["**/var/lib/dpkg/info/*.list"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-files.html"),
);

/// Parser for Debian installed MD5 checksum files (*.md5sums)
pub struct DebianInstalledMd5sumsParser;

impl PackageParser for DebianInstalledMd5sumsParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("md5sums")
            && path
                .to_str()
                .map(|p| p.contains("/var/lib/dpkg/info/"))
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_stem().and_then(|s| s.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(DatasourceId::DebianInstalledMd5Sums)];
            }
        };

        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .md5sums file {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianInstalledMd5Sums)];
            }
        };

        vec![parse_debian_file_list(
            &content,
            filename,
            DatasourceId::DebianInstalledMd5Sums,
        )]
    }
}

crate::register_parser!(
    "Debian installed package md5sums",
    &["**/var/lib/dpkg/info/*.md5sums"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-files.html"),
);

const IGNORED_ROOT_DIRS: &[&str] = &["/.", "/bin", "/etc", "/lib", "/sbin", "/usr", "/var"];

fn parse_debian_file_list(
    content: &str,
    filename: &str,
    datasource_id: DatasourceId,
) -> PackageData {
    let (name, arch_qualifier) = if let Some((pkg, arch)) = filename.split_once(':') {
        (Some(pkg.to_string()), Some(arch.to_string()))
    } else if filename == "md5sums" {
        (None, None)
    } else {
        (Some(filename.to_string()), None)
    };

    let mut file_references = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (md5sum, path) = if let Some((hash, p)) = line.split_once(' ') {
            (Md5Digest::from_hex(hash.trim()).ok(), p.trim())
        } else {
            (None, line)
        };

        if IGNORED_ROOT_DIRS.contains(&path) {
            continue;
        }

        file_references.push(FileReference {
            path: path.to_string(),
            size: None,
            sha1: None,
            md5: md5sum,
            sha256: None,
            sha512: None,
            extra_data: None,
        });
    }

    if file_references.is_empty() {
        return default_package_data(datasource_id);
    }

    let namespace = Some("debian".to_string());
    let mut package = PackageData {
        datasource_id: Some(datasource_id),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: name.clone(),
        file_references,
        ..Default::default()
    };

    if let Some(n) = &name {
        package.purl = build_debian_purl(n, None, namespace.as_deref(), arch_qualifier.as_deref());
    }

    package
}

/// Parser for Debian machine-readable copyright files (DEP-5 format)
pub struct DebianCopyrightParser;

impl PackageParser for DebianCopyrightParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename != "copyright" {
                return filename.ends_with("_copyright");
            }
            let path_str = path.to_string_lossy();
            path_str.contains("/debian/")
                || path_str.contains("/packages/deb/")
                || path_str.contains("/usr/share/doc/")
                || path_str.ends_with("debian/copyright")
        } else {
            false
        }
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let datasource_id = detect_debian_copyright_datasource(path);
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read copyright file {:?}: {}", path, e);
                return vec![default_package_data(datasource_id)];
            }
        };

        let package_name = extract_package_name_from_path(path);
        let mut package_data = parse_copyright_file(&content, package_name.as_deref());
        package_data.datasource_id = Some(datasource_id);
        vec![package_data]
    }
}

crate::register_parser!(
    "Debian machine-readable copyright file",
    &[
        "**/debian/copyright",
        "**/packages/deb/copyright",
        "**/usr/share/doc/*/copyright",
        "**/*_copyright"
    ],
    "deb",
    "",
    Some("https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"),
);

fn detect_debian_copyright_datasource(path: &Path) -> DatasourceId {
    let path_str = path.to_string_lossy();
    if path_str.contains("/debian/") || path_str.ends_with("debian/copyright") {
        DatasourceId::DebianCopyrightInSource
    } else if path_str.contains("/usr/share/doc/") {
        DatasourceId::DebianCopyrightInPackage
    } else {
        DatasourceId::DebianCopyrightStandalone
    }
}

fn extract_package_name_from_path(path: &Path) -> Option<String> {
    let components: Vec<_> = path.components().collect();

    for (i, component) in components.iter().enumerate() {
        if let std::path::Component::Normal(os_str) = component
            && os_str.to_str() == Some("doc")
            && i + 1 < components.len()
            && let std::path::Component::Normal(next) = components[i + 1]
        {
            return next.to_str().map(|s| s.to_string());
        }
    }
    None
}

fn parse_copyright_file(content: &str, package_name: Option<&str>) -> PackageData {
    let paragraphs = parse_copyright_paragraphs_with_lines(content);

    let is_dep5 = paragraphs
        .first()
        .and_then(|p| rfc822::get_header_first(&p.metadata.headers, "format"))
        .is_some();

    let namespace = Some("debian".to_string());
    let mut parties = Vec::new();
    let mut license_statements = Vec::new();
    let mut primary_license_detection = None;
    let mut header_license_detection = None;
    let mut other_license_detections = Vec::new();

    if is_dep5 {
        for para in &paragraphs {
            if let Some(copyright_text) =
                rfc822::get_header_first(&para.metadata.headers, "copyright")
            {
                for holder in parse_copyright_holders(&copyright_text) {
                    if !holder.is_empty() {
                        parties.push(Party {
                            r#type: None,
                            role: Some("copyright-holder".to_string()),
                            name: Some(holder),
                            email: None,
                            url: None,
                            organization: None,
                            organization_url: None,
                            timezone: None,
                        });
                    }
                }
            }

            if let Some(license) = rfc822::get_header_first(&para.metadata.headers, "license") {
                let license_name = license.lines().next().unwrap_or(&license).trim();
                if !license_name.is_empty()
                    && !license_statements.contains(&license_name.to_string())
                {
                    license_statements.push(license_name.to_string());
                }

                if let Some((matched_text, line_no)) = para.license_header_line.clone() {
                    let detection =
                        build_primary_license_detection(license_name, matched_text, line_no);
                    let is_header_paragraph =
                        rfc822::get_header_first(&para.metadata.headers, "format").is_some();
                    if rfc822::get_header_first(&para.metadata.headers, "files").as_deref()
                        == Some("*")
                    {
                        primary_license_detection = Some(detection);
                    } else if is_header_paragraph {
                        header_license_detection.get_or_insert(detection);
                    } else {
                        other_license_detections.push(detection);
                    }
                }
            }
        }

        if primary_license_detection.is_none() && header_license_detection.is_some() {
            primary_license_detection = header_license_detection;
        }
    } else {
        let copyright_block = extract_unstructured_field(content, "Copyright:");
        if let Some(text) = copyright_block {
            for holder in parse_copyright_holders(&text) {
                if !holder.is_empty() {
                    parties.push(Party {
                        r#type: None,
                        role: Some("copyright-holder".to_string()),
                        name: Some(holder),
                        email: None,
                        url: None,
                        organization: None,
                        organization_url: None,
                        timezone: None,
                    });
                }
            }
        }

        let license_block = extract_unstructured_field(content, "License:");
        if let Some(text) = license_block {
            license_statements.push(text.lines().next().unwrap_or(&text).trim().to_string());
        }
    }

    let extracted_license_statement = if license_statements.is_empty() {
        None
    } else {
        Some(license_statements.join(" AND "))
    };

    let license_detections = primary_license_detection.into_iter().collect::<Vec<_>>();
    let declared_license_expression = license_detections
        .first()
        .map(|detection| detection.license_expression.clone());
    let declared_license_expression_spdx = license_detections
        .first()
        .map(|detection| detection.license_expression_spdx.clone());
    let other_license_expression = combine_license_expressions(
        other_license_detections
            .iter()
            .map(|detection| detection.license_expression.clone()),
    );
    let other_license_expression_spdx = combine_license_expressions(
        other_license_detections
            .iter()
            .map(|detection| detection.license_expression_spdx.clone()),
    );

    PackageData {
        datasource_id: Some(DatasourceId::DebianCopyright),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: package_name.map(|s| s.to_string()),
        parties,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression,
        other_license_expression_spdx,
        other_license_detections,
        extracted_license_statement,
        purl: package_name.and_then(|n| build_debian_purl(n, None, namespace.as_deref(), None)),
        ..Default::default()
    }
}

#[derive(Debug)]
struct CopyrightParagraph {
    metadata: Rfc822Metadata,
    license_header_line: Option<(String, usize)>,
}

fn parse_copyright_paragraphs_with_lines(content: &str) -> Vec<CopyrightParagraph> {
    let mut paragraphs = Vec::new();
    let mut current_lines = Vec::new();
    let mut current_start_line = 1usize;

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if line.is_empty() {
            if !current_lines.is_empty() {
                paragraphs.push(finalize_copyright_paragraph(
                    std::mem::take(&mut current_lines),
                    current_start_line,
                ));
            }
            current_start_line = line_no + 1;
        } else {
            if current_lines.is_empty() {
                current_start_line = line_no;
            }
            current_lines.push(line.to_string());
        }
    }

    if !current_lines.is_empty() {
        paragraphs.push(finalize_copyright_paragraph(
            current_lines,
            current_start_line,
        ));
    }

    paragraphs
}

fn finalize_copyright_paragraph(raw_lines: Vec<String>, start_line: usize) -> CopyrightParagraph {
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_value = String::new();
    let mut license_header_line = None;

    for (idx, line) in raw_lines.iter().enumerate() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_name.is_some() {
                current_value.push('\n');
                current_value.push_str(line);
            }
            continue;
        }

        if let Some(name) = current_name.take() {
            add_copyright_header_value(&mut headers, &name, &current_value);
            current_value.clear();
        }

        if let Some((name, value)) = line.split_once(':') {
            let normalized_name = name.trim().to_ascii_lowercase();
            if normalized_name == "license" && license_header_line.is_none() {
                license_header_line = Some((line.trim_end().to_string(), start_line + idx));
            }
            current_name = Some(normalized_name);
            current_value = value.trim_start().to_string();
        }
    }

    if let Some(name) = current_name.take() {
        add_copyright_header_value(&mut headers, &name, &current_value);
    }

    CopyrightParagraph {
        metadata: Rfc822Metadata {
            headers,
            body: String::new(),
        },
        license_header_line,
    }
}

fn add_copyright_header_value(headers: &mut HashMap<String, Vec<String>>, name: &str, value: &str) {
    let entry = headers.entry(name.to_string()).or_default();
    let trimmed = value.trim_end();
    if !trimmed.is_empty() {
        entry.push(trimmed.to_string());
    }
}

fn build_primary_license_detection(
    license_name: &str,
    matched_text: String,
    line_no: usize,
) -> LicenseDetection {
    let normalized = normalize_debian_license_name(license_name);

    build_declared_license_detection(
        &normalized,
        DeclaredLicenseMatchMetadata::new(&matched_text, line_no, line_no),
    )
}

fn normalize_debian_license_name(license_name: &str) -> NormalizedDeclaredLicense {
    match license_name.trim() {
        "GPL-2+" => NormalizedDeclaredLicense::new("gpl-2.0-plus", "GPL-2.0-or-later"),
        "GPL-2" => NormalizedDeclaredLicense::new("gpl-2.0", "GPL-2.0-only"),
        "LGPL-2+" => NormalizedDeclaredLicense::new("lgpl-2.0-plus", "LGPL-2.0-or-later"),
        "LGPL-2.1" => NormalizedDeclaredLicense::new("lgpl-2.1", "LGPL-2.1-only"),
        "LGPL-2.1+" => NormalizedDeclaredLicense::new("lgpl-2.1-plus", "LGPL-2.1-or-later"),
        "LGPL-3+" => NormalizedDeclaredLicense::new("lgpl-3.0-plus", "LGPL-3.0-or-later"),
        "BSD-4-clause" => NormalizedDeclaredLicense::new("bsd-original-uc", "BSD-4-Clause-UC"),
        "public-domain" => {
            NormalizedDeclaredLicense::new("public-domain", "LicenseRef-provenant-public-domain")
        }
        other => normalize_declared_license_key(other)
            .unwrap_or_else(|| NormalizedDeclaredLicense::new(other.to_ascii_lowercase(), other)),
    }
}

fn parse_copyright_holders(text: &str) -> Vec<String> {
    let mut holders = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let cleaned = line
            .trim_start_matches("Copyright")
            .trim_start_matches("copyright")
            .trim_start_matches("(C)")
            .trim_start_matches("(c)")
            .trim_start_matches("©")
            .trim();

        if let Some(year_end) = cleaned.find(char::is_alphabetic) {
            let without_years = &cleaned[year_end..];
            let holder = without_years
                .trim_start_matches(',')
                .trim_start_matches('-')
                .trim();

            if !holder.is_empty() && holder.len() > 2 {
                holders.push(holder.to_string());
            }
        }
    }

    holders
}

fn extract_unstructured_field(content: &str, field_name: &str) -> Option<String> {
    let mut in_field = false;
    let mut field_content = String::new();

    for line in content.lines() {
        if line.starts_with(field_name) {
            in_field = true;
            field_content.push_str(line.trim_start_matches(field_name).trim());
            field_content.push('\n');
        } else if in_field {
            if line.starts_with(char::is_whitespace) {
                field_content.push_str(line.trim());
                field_content.push('\n');
            } else if !line.trim().is_empty() {
                break;
            }
        }
    }

    let trimmed = field_content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parser for Debian binary package archives (.deb files)
pub struct DebianDebParser;

impl PackageParser for DebianDebParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("deb")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        // Try to extract metadata from archive contents first
        if let Ok(data) = extract_deb_archive(path) {
            return vec![data];
        }

        // Fallback to filename parsing
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(DatasourceId::DebianDeb)];
            }
        };

        vec![parse_deb_filename(filename)]
    }
}

crate::register_parser!(
    "Debian binary package archive (.deb)",
    &["**/*.deb"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-binary.html"),
);

fn extract_deb_archive(path: &Path) -> Result<PackageData, String> {
    use flate2::read::GzDecoder;
    use liblzma::read::XzDecoder;
    use std::io::{Cursor, Read};

    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open .deb file: {}", e))?;

    let mut archive = ar::Archive::new(file);
    let mut package: Option<PackageData> = None;

    while let Some(entry_result) = archive.next_entry() {
        let mut entry = entry_result.map_err(|e| format!("Failed to read ar entry: {}", e))?;

        let entry_name = std::str::from_utf8(entry.header().identifier())
            .map_err(|e| format!("Invalid entry name: {}", e))?;
        let entry_name = entry_name.trim().to_string();

        if entry_name == "control.tar.gz" || entry_name.starts_with("control.tar") {
            let mut control_data = Vec::new();
            entry
                .read_to_end(&mut control_data)
                .map_err(|e| format!("Failed to read control.tar.gz: {}", e))?;

            if entry_name.ends_with(".gz") {
                let decoder = GzDecoder::new(Cursor::new(control_data));
                if let Some(parsed_package) = parse_control_tar_archive(decoder)? {
                    package = Some(parsed_package);
                }
            } else if entry_name.ends_with(".xz") {
                let decoder = XzDecoder::new(Cursor::new(control_data));
                if let Some(parsed_package) = parse_control_tar_archive(decoder)? {
                    package = Some(parsed_package);
                }
            }
        } else if entry_name.starts_with("data.tar") {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| format!("Failed to read data archive: {}", e))?;

            let Some(current_package) = package.as_mut() else {
                continue;
            };

            if entry_name.ends_with(".gz") {
                let decoder = GzDecoder::new(Cursor::new(data));
                merge_deb_data_archive(decoder, current_package)?;
            } else if entry_name.ends_with(".xz") {
                let decoder = XzDecoder::new(Cursor::new(data));
                merge_deb_data_archive(decoder, current_package)?;
            }
        }
    }

    package.ok_or_else(|| ".deb archive does not contain control.tar.* metadata".to_string())
}

fn parse_control_tar_archive<R: std::io::Read>(reader: R) -> Result<Option<PackageData>, String> {
    use std::io::Read;

    let mut tar_archive = tar::Archive::new(reader);

    for tar_entry_result in tar_archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?
    {
        let mut tar_entry =
            tar_entry_result.map_err(|e| format!("Failed to read tar entry: {}", e))?;

        let tar_path = tar_entry
            .path()
            .map_err(|e| format!("Failed to get tar path: {}", e))?;

        if tar_path.ends_with("control") {
            let mut control_content = String::new();
            tar_entry
                .read_to_string(&mut control_content)
                .map_err(|e| format!("Failed to read control file: {}", e))?;

            let paragraphs = rfc822::parse_rfc822_paragraphs(&control_content);
            if paragraphs.is_empty() {
                return Err("No paragraphs in control file".to_string());
            }

            if let Some(package) =
                build_package_from_paragraph(&paragraphs[0], None, DatasourceId::DebianDeb)
            {
                return Ok(Some(package));
            }

            return Err("Failed to parse control file".to_string());
        }
    }

    Ok(None)
}

fn merge_deb_data_archive<R: std::io::Read>(
    reader: R,
    package: &mut PackageData,
) -> Result<(), String> {
    use std::io::Read;

    let mut tar_archive = tar::Archive::new(reader);

    for tar_entry_result in tar_archive
        .entries()
        .map_err(|e| format!("Failed to read data tar entries: {}", e))?
    {
        let mut tar_entry =
            tar_entry_result.map_err(|e| format!("Failed to read data tar entry: {}", e))?;

        let tar_path = tar_entry
            .path()
            .map_err(|e| format!("Failed to get data tar path: {}", e))?;
        let tar_path_str = tar_path.to_string_lossy();

        if tar_path_str.ends_with(&format!(
            "/usr/share/doc/{}/copyright",
            package.name.as_deref().unwrap_or_default()
        )) || tar_path_str.ends_with(&format!(
            "usr/share/doc/{}/copyright",
            package.name.as_deref().unwrap_or_default()
        )) {
            let mut copyright_content = String::new();
            tar_entry
                .read_to_string(&mut copyright_content)
                .map_err(|e| format!("Failed to read copyright file from data tar: {}", e))?;

            let copyright_pkg = parse_copyright_file(&copyright_content, package.name.as_deref());
            merge_debian_copyright_into_package(package, &copyright_pkg);
            break;
        }
    }

    Ok(())
}

fn merge_debian_copyright_into_package(target: &mut PackageData, copyright: &PackageData) {
    if target.extracted_license_statement.is_none() {
        target.extracted_license_statement = copyright.extracted_license_statement.clone();
    }

    for party in &copyright.parties {
        if !target.parties.iter().any(|existing| {
            existing.r#type == party.r#type
                && existing.role == party.role
                && existing.name == party.name
                && existing.email == party.email
                && existing.url == party.url
                && existing.organization == party.organization
                && existing.organization_url == party.organization_url
                && existing.timezone == party.timezone
        }) {
            target.parties.push(party.clone());
        }
    }
}

fn parse_deb_filename(filename: &str) -> PackageData {
    let without_ext = filename.trim_end_matches(".deb");

    let parts: Vec<&str> = without_ext.split('_').collect();
    if parts.len() < 2 {
        return default_package_data(DatasourceId::DebianDeb);
    }

    let name = parts[0].to_string();
    let version = parts[1].to_string();
    let architecture = if parts.len() >= 3 {
        Some(parts[2].to_string())
    } else {
        None
    };

    let namespace = Some("debian".to_string());

    PackageData {
        datasource_id: Some(DatasourceId::DebianDeb),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name.clone()),
        version: Some(version.clone()),
        purl: build_debian_purl(
            &name,
            Some(&version),
            namespace.as_deref(),
            architecture.as_deref(),
        ),
        ..Default::default()
    }
}

/// Parser for control files inside extracted .deb control tarballs.
///
/// Matches paths like `*/control.tar.gz-extract/control` and
/// `*/control.tar.xz-extract/control` which are created by ExtractCode
/// when extracting .deb archives.
pub struct DebianControlInExtractedDebParser;

impl PackageParser for DebianControlInExtractedDebParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "control")
            && path
                .to_str()
                .map(|p| {
                    p.ends_with("control.tar.gz-extract/control")
                        || p.ends_with("control.tar.xz-extract/control")
                })
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "Failed to read control file in extracted deb {:?}: {}",
                    path, e
                );
                return vec![default_package_data(
                    DatasourceId::DebianControlExtractedDeb,
                )];
            }
        };

        // A control file inside an extracted .deb has a single paragraph
        // (unlike debian/control which has source + binary paragraphs)
        let paragraphs = rfc822::parse_rfc822_paragraphs(&content);
        if paragraphs.is_empty() {
            return vec![default_package_data(
                DatasourceId::DebianControlExtractedDeb,
            )];
        }

        if let Some(pkg) = build_package_from_paragraph(
            &paragraphs[0],
            None,
            DatasourceId::DebianControlExtractedDeb,
        ) {
            vec![pkg]
        } else {
            vec![default_package_data(
                DatasourceId::DebianControlExtractedDeb,
            )]
        }
    }
}

/// Parser for MD5 checksum files inside extracted .deb control tarballs
pub struct DebianMd5sumInPackageParser;

impl PackageParser for DebianMd5sumInPackageParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "md5sums")
            && path
                .to_str()
                .map(|p| {
                    p.ends_with("control.tar.gz-extract/md5sums")
                        || p.ends_with("control.tar.xz-extract/md5sums")
                })
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read md5sums file {:?}: {}", path, e);
                return vec![default_package_data(
                    DatasourceId::DebianMd5SumsInExtractedDeb,
                )];
            }
        };

        let package_name = extract_package_name_from_deb_path(path);

        vec![parse_md5sums_in_package(&content, package_name.as_deref())]
    }
}

pub(crate) fn extract_package_name_from_deb_path(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let grandparent = parent.parent()?;
    let dirname = grandparent.file_name()?.to_str()?;
    let without_extract = dirname.strip_suffix("-extract")?;
    let without_deb = without_extract.strip_suffix(".deb")?;
    let name = without_deb.split('_').next()?;

    Some(name.to_string())
}

fn parse_md5sums_in_package(content: &str, package_name: Option<&str>) -> PackageData {
    let mut file_references = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (md5sum, filepath): (Option<Md5Digest>, &str) = if let Some(idx) = line.find("  ") {
            (
                Md5Digest::from_hex(line[..idx].trim()).ok(),
                line[idx + 2..].trim(),
            )
        } else if let Some((hash, path)) = line.split_once(' ') {
            (Md5Digest::from_hex(hash.trim()).ok(), path.trim())
        } else {
            (None, line)
        };

        if IGNORED_ROOT_DIRS.contains(&filepath) {
            continue;
        }

        file_references.push(FileReference {
            path: filepath.to_string(),
            size: None,
            sha1: None,
            md5: md5sum,
            sha256: None,
            sha512: None,
            extra_data: None,
        });
    }

    if file_references.is_empty() {
        return default_package_data(DatasourceId::DebianMd5SumsInExtractedDeb);
    }

    let namespace = Some("debian".to_string());
    let mut package = PackageData {
        datasource_id: Some(DatasourceId::DebianMd5SumsInExtractedDeb),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: package_name.map(|s| s.to_string()),
        file_references,
        ..Default::default()
    };

    if let Some(n) = &package.name {
        package.purl = build_debian_purl(n, None, namespace.as_deref(), None);
    }

    package
}

crate::register_parser!(
    "Debian control file in extracted .deb control tarball",
    &[
        "**/control.tar.gz-extract/control",
        "**/control.tar.xz-extract/control"
    ],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

crate::register_parser!(
    "Debian MD5 checksums in extracted .deb control tarball",
    &[
        "**/control.tar.gz-extract/md5sums",
        "**/control.tar.xz-extract/md5sums"
    ],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use ar::{Builder as ArBuilder, Header as ArHeader};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use liblzma::write::XzEncoder;
    use std::io::Cursor;
    use std::path::PathBuf;
    use tar::{Builder as TarBuilder, Header as TarHeader};
    use tempfile::NamedTempFile;

    fn create_synthetic_deb_with_control_tar_xz() -> NamedTempFile {
        let mut control_tar = Vec::new();
        {
            let encoder = XzEncoder::new(&mut control_tar, 6);
            let mut tar_builder = TarBuilder::new(encoder);

            let control_content = b"Package: synthetic\nVersion: 1.2.3\nArchitecture: amd64\nDescription: Synthetic deb\nHomepage: https://example.com\n";
            let mut header = TarHeader::new_gnu();
            header
                .set_path("control")
                .expect("control tar path should be valid");
            header.set_size(control_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, Cursor::new(control_content))
                .expect("control file should be appended to tar.xz");
            tar_builder.finish().expect("control tar.xz should finish");
        }

        let deb = NamedTempFile::new().expect("temp deb file should be created");
        {
            let mut builder = ArBuilder::new(
                deb.reopen()
                    .expect("temporary deb file should reopen for writing"),
            );

            let debian_binary = b"2.0\n";
            let mut debian_binary_header =
                ArHeader::new(b"debian-binary".to_vec(), debian_binary.len() as u64);
            debian_binary_header.set_mode(0o100644);
            builder
                .append(&debian_binary_header, Cursor::new(debian_binary))
                .expect("debian-binary entry should be appended");

            let mut control_header =
                ArHeader::new(b"control.tar.xz".to_vec(), control_tar.len() as u64);
            control_header.set_mode(0o100644);
            builder
                .append(&control_header, Cursor::new(control_tar))
                .expect("control.tar.xz entry should be appended");
        }

        deb
    }

    fn create_synthetic_deb_with_copyright() -> NamedTempFile {
        let mut control_tar = Vec::new();
        {
            let encoder = GzEncoder::new(&mut control_tar, Compression::default());
            let mut tar_builder = TarBuilder::new(encoder);

            let control_content = b"Package: synthetic\nVersion: 9.9.9\nArchitecture: all\nDescription: Synthetic deb with copyright\n";
            let mut header = TarHeader::new_gnu();
            header
                .set_path("control")
                .expect("control tar path should be valid");
            header.set_size(control_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, Cursor::new(control_content))
                .expect("control file should be appended to tar.gz");
            tar_builder.finish().expect("control tar.gz should finish");
        }

        let mut data_tar = Vec::new();
        {
            let encoder = GzEncoder::new(&mut data_tar, Compression::default());
            let mut tar_builder = TarBuilder::new(encoder);

            let copyright = b"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nFiles: *\nCopyright: 2024 Example Org\nLicense: Apache-2.0\n Licensed under the Apache License, Version 2.0.\n";
            let mut header = TarHeader::new_gnu();
            header
                .set_path("./usr/share/doc/synthetic/copyright")
                .expect("copyright path should be valid");
            header.set_size(copyright.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, Cursor::new(copyright))
                .expect("copyright file should be appended to data tar");
            tar_builder.finish().expect("data tar.gz should finish");
        }

        let deb = NamedTempFile::new().expect("temp deb file should be created");
        {
            let mut builder = ArBuilder::new(
                deb.reopen()
                    .expect("temporary deb file should reopen for writing"),
            );

            let debian_binary = b"2.0\n";
            let mut debian_binary_header =
                ArHeader::new(b"debian-binary".to_vec(), debian_binary.len() as u64);
            debian_binary_header.set_mode(0o100644);
            builder
                .append(&debian_binary_header, Cursor::new(debian_binary))
                .expect("debian-binary entry should be appended");

            let mut control_header =
                ArHeader::new(b"control.tar.gz".to_vec(), control_tar.len() as u64);
            control_header.set_mode(0o100644);
            builder
                .append(&control_header, Cursor::new(control_tar))
                .expect("control.tar.gz entry should be appended");

            let mut data_header = ArHeader::new(b"data.tar.gz".to_vec(), data_tar.len() as u64);
            data_header.set_mode(0o100644);
            builder
                .append(&data_header, Cursor::new(data_tar))
                .expect("data.tar.gz entry should be appended");
        }

        deb
    }

    // ====== Namespace detection ======

    #[test]
    fn test_detect_namespace_from_ubuntu_version() {
        assert_eq!(
            detect_namespace(Some("1.0-1ubuntu1"), None),
            Some("ubuntu".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_from_debian_version() {
        assert_eq!(
            detect_namespace(Some("1.0-1+deb11u1"), None),
            Some("debian".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_from_ubuntu_maintainer() {
        assert_eq!(
            detect_namespace(
                None,
                Some("Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>")
            ),
            Some("ubuntu".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_from_debian_maintainer() {
        assert_eq!(
            detect_namespace(None, Some("John Doe <john@debian.org>")),
            Some("debian".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_default() {
        assert_eq!(
            detect_namespace(None, Some("Unknown <unknown@example.com>")),
            Some("debian".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_version_takes_priority() {
        // Version clue should be checked before maintainer
        assert_eq!(
            detect_namespace(Some("1.0ubuntu1"), Some("maintainer@debian.org")),
            Some("ubuntu".to_string())
        );
    }

    // ====== PURL generation ======

    #[test]
    fn test_build_purl_basic() {
        let purl = build_debian_purl("curl", Some("7.68.0-1"), Some("debian"), Some("amd64"));
        assert_eq!(
            purl,
            Some("pkg:deb/debian/curl@7.68.0-1?arch=amd64".to_string())
        );
    }

    #[test]
    fn test_build_purl_no_version() {
        let purl = build_debian_purl("curl", None, Some("debian"), Some("any"));
        assert_eq!(purl, Some("pkg:deb/debian/curl?arch=any".to_string()));
    }

    #[test]
    fn test_build_purl_no_arch() {
        let purl = build_debian_purl("curl", Some("7.68.0"), Some("ubuntu"), None);
        assert_eq!(purl, Some("pkg:deb/ubuntu/curl@7.68.0".to_string()));
    }

    #[test]
    fn test_build_purl_no_namespace() {
        let purl = build_debian_purl("curl", Some("7.68.0"), None, None);
        assert_eq!(purl, Some("pkg:deb/curl@7.68.0".to_string()));
    }

    // ====== Dependency parsing ======

    #[test]
    fn test_parse_simple_dependency() {
        let deps = parse_dependency_field("libc6", "depends", true, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
        assert_eq!(deps[0].extracted_requirement, None);
        assert_eq!(deps[0].scope, Some("depends".to_string()));
    }

    #[test]
    fn test_parse_dependency_with_version() {
        let deps =
            parse_dependency_field("libc6 (>= 2.17)", "depends", true, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some(">= 2.17".to_string()));
    }

    #[test]
    fn test_parse_dependency_exact_version() {
        let deps = parse_dependency_field(
            "libc6 (= 2.31-13+deb11u5)",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].is_pinned, Some(true));
    }

    #[test]
    fn test_parse_dependency_strict_less() {
        let deps =
            parse_dependency_field("libgcc-s1 (<< 12)", "breaks", false, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].extracted_requirement, Some("<< 12".to_string()));
        assert_eq!(deps[0].scope, Some("breaks".to_string()));
    }

    #[test]
    fn test_parse_multiple_dependencies() {
        let deps = parse_dependency_field(
            "libc6 (>= 2.17), libssl1.1 (>= 1.1.0), zlib1g (>= 1:1.2.0)",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 3);
    }

    #[test]
    fn test_parse_dependency_alternatives() {
        let deps = parse_dependency_field(
            "libssl1.1 | libssl3",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 2);
        // Alternatives are marked as optional
        assert_eq!(deps[0].is_optional, Some(true));
        assert_eq!(deps[1].is_optional, Some(true));
    }

    #[test]
    fn test_parse_dependency_skips_substitutions() {
        let deps = parse_dependency_field(
            "${shlibs:Depends}, ${misc:Depends}, libc6",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
    }

    #[test]
    fn test_parse_dependency_with_arch_qualifier() {
        // Dependencies can have [arch] qualifiers which we ignore
        let deps = parse_dependency_field(
            "libc6 (>= 2.17) [amd64]",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
    }

    #[test]
    fn test_parse_empty_dependency() {
        let deps = parse_dependency_field("", "depends", true, false, Some("debian"));
        assert!(deps.is_empty());
    }

    // ====== Source field parsing ======

    #[test]
    fn test_parse_source_field_name_only() {
        let sources = parse_source_field(Some("util-linux"), Some("debian"));
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], "pkg:deb/debian/util-linux");
    }

    #[test]
    fn test_parse_source_field_with_version() {
        let sources = parse_source_field(Some("util-linux (2.36.1-8+deb11u1)"), Some("debian"));
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], "pkg:deb/debian/util-linux@2.36.1-8%2Bdeb11u1");
    }

    #[test]
    fn test_parse_source_field_empty() {
        let sources = parse_source_field(None, Some("debian"));
        assert!(sources.is_empty());
    }

    // ====== Control file parsing ======

    #[test]
    fn test_parse_debian_control_source_and_binary() {
        let content = "\
Source: curl
Section: web
Priority: optional
Maintainer: Alessandro Ghedini <ghedo@debian.org>
Homepage: https://curl.se/
Vcs-Browser: https://salsa.debian.org/debian/curl
Vcs-Git: https://salsa.debian.org/debian/curl.git
Build-Depends: debhelper (>= 12), libssl-dev

Package: curl
Architecture: amd64
Depends: libc6 (>= 2.17), libcurl4 (= ${binary:Version})
Description: command line tool for transferring data with URL syntax";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name, Some("curl".to_string()));
        assert_eq!(pkg.package_type, Some(PackageType::Deb));
        assert_eq!(pkg.homepage_url, Some("https://curl.se/".to_string()));
        assert_eq!(
            pkg.vcs_url,
            Some("https://salsa.debian.org/debian/curl.git".to_string())
        );
        assert_eq!(
            pkg.code_view_url,
            Some("https://salsa.debian.org/debian/curl".to_string())
        );

        // Maintainer from source paragraph
        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(pkg.parties[0].name, Some("Alessandro Ghedini".to_string()));
        assert_eq!(pkg.parties[0].email, Some("ghedo@debian.org".to_string()));

        // Dependencies parsed
        assert!(!pkg.dependencies.is_empty());
    }

    #[test]
    fn test_parse_debian_control_multiple_binary() {
        let content = "\
Source: gzip
Maintainer: Debian Developer <dev@debian.org>

Package: gzip
Architecture: any
Depends: libc6 (>= 2.17)
Description: GNU file compression

Package: gzip-win32
Architecture: all
Description: gzip for Windows";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, Some("gzip".to_string()));
        assert_eq!(packages[1].name, Some("gzip-win32".to_string()));

        // Both inherit source maintainer
        assert_eq!(packages[0].parties.len(), 1);
        assert_eq!(packages[1].parties.len(), 1);
    }

    #[test]
    fn test_parse_debian_control_source_only() {
        let content = "\
Source: my-package
Maintainer: Test User <test@debian.org>
Build-Depends: debhelper (>= 13)";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, Some("my-package".to_string()));
        // Build-Depends parsed
        assert!(!packages[0].dependencies.is_empty());
        assert_eq!(
            packages[0].dependencies[0].scope,
            Some("build-depends".to_string())
        );
    }

    #[test]
    fn test_parse_debian_control_with_uploaders() {
        let content = "\
Source: example
Maintainer: Main Dev <main@debian.org>
Uploaders: Alice <alice@example.com>, Bob <bob@example.com>

Package: example
Architecture: any
Description: test package";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        // 1 maintainer + 2 uploaders
        assert_eq!(packages[0].parties.len(), 3);
        assert_eq!(packages[0].parties[0].role, Some("maintainer".to_string()));
        assert_eq!(packages[0].parties[1].role, Some("uploader".to_string()));
        assert_eq!(packages[0].parties[2].role, Some("uploader".to_string()));
    }

    #[test]
    fn test_parse_debian_control_vcs_git_with_branch() {
        let content = "\
Source: example
Maintainer: Dev <dev@debian.org>
Vcs-Git: https://salsa.debian.org/example.git -b main

Package: example
Architecture: any
Description: test";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        // Should only take the URL, not the branch
        assert_eq!(
            packages[0].vcs_url,
            Some("https://salsa.debian.org/example.git".to_string())
        );
    }

    #[test]
    fn test_parse_debian_control_multi_arch() {
        let content = "\
Source: example
Maintainer: Dev <dev@debian.org>

Package: libexample
Architecture: any
Multi-Arch: same
Description: shared library";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        let extra = packages[0].extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("multi_arch"),
            Some(&serde_json::Value::String("same".to_string()))
        );
    }

    // ====== dpkg/status parsing ======

    #[test]
    fn test_parse_dpkg_status_basic() {
        let content = "\
Package: base-files
Status: install ok installed
Priority: required
Section: admin
Installed-Size: 391
Maintainer: Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>
Architecture: amd64
Version: 11ubuntu5.6
Description: Debian base system miscellaneous files
Homepage: https://tracker.debian.org/pkg/base-files

Package: not-installed
Status: deinstall ok config-files
Architecture: amd64
Version: 1.0
Description: This should be skipped";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name, Some("base-files".to_string()));
        assert_eq!(pkg.version, Some("11ubuntu5.6".to_string()));
        assert_eq!(pkg.namespace, Some("ubuntu".to_string()));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianInstalledStatusDb)
        );

        // Installed-Size in extra_data
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("installed_size"),
            Some(&serde_json::Value::Number(serde_json::Number::from(391)))
        );
    }

    #[test]
    fn test_parse_dpkg_status_multiple_installed() {
        let content = "\
Package: libc6
Status: install ok installed
Architecture: amd64
Version: 2.31-13+deb11u5
Maintainer: GNU Libc Maintainers <debian-glibc@lists.debian.org>
Description: GNU C Library

Package: zlib1g
Status: install ok installed
Architecture: amd64
Version: 1:1.2.11.dfsg-2+deb11u2
Maintainer: Mark Brown <broonie@debian.org>
Description: compression library";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, Some("libc6".to_string()));
        assert_eq!(packages[1].name, Some("zlib1g".to_string()));
    }

    #[test]
    fn test_parse_dpkg_status_with_dependencies() {
        let content = "\
Package: curl
Status: install ok installed
Architecture: amd64
Version: 7.74.0-1.3+deb11u7
Maintainer: Alessandro Ghedini <ghedo@debian.org>
Depends: libc6 (>= 2.17), libcurl4 (= 7.74.0-1.3+deb11u7)
Recommends: ca-certificates
Description: command line tool for transferring data with URL syntax";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);

        let deps = &packages[0].dependencies;
        // 2 from Depends + 1 from Recommends
        assert_eq!(deps.len(), 3);

        // Check first dependency
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
        assert_eq!(deps[0].scope, Some("depends".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some(">= 2.17".to_string()));

        // Check recommends
        assert_eq!(
            deps[2].purl,
            Some("pkg:deb/debian/ca-certificates".to_string())
        );
        assert_eq!(deps[2].scope, Some("recommends".to_string()));
        assert_eq!(deps[2].is_optional, Some(true));
    }

    #[test]
    fn test_parse_dpkg_status_with_source() {
        let content = "\
Package: libncurses6
Status: install ok installed
Architecture: amd64
Source: ncurses (6.2+20201114-2+deb11u1)
Version: 6.2+20201114-2+deb11u1
Maintainer: Craig Small <csmall@debian.org>
Description: shared libraries for terminal handling";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);
        assert!(!packages[0].source_packages.is_empty());
        // Source PURL should include version from parentheses
        assert!(packages[0].source_packages[0].contains("ncurses"));
    }

    #[test]
    fn test_parse_dpkg_status_filters_not_installed() {
        let content = "\
Package: installed-pkg
Status: install ok installed
Version: 1.0
Architecture: amd64
Description: installed

Package: half-installed
Status: install ok half-installed
Version: 2.0
Architecture: amd64
Description: half installed

Package: deinstall-pkg
Status: deinstall ok config-files
Version: 3.0
Architecture: amd64
Description: deinstalled

Package: purge-pkg
Status: purge ok not-installed
Version: 4.0
Architecture: amd64
Description: purged";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, Some("installed-pkg".to_string()));
    }

    #[test]
    fn test_parse_dpkg_status_empty() {
        let packages = parse_dpkg_status("");
        assert!(packages.is_empty());
    }

    // ====== is_match tests ======

    #[test]
    fn test_debian_control_is_match() {
        assert!(DebianControlParser::is_match(Path::new(
            "/path/to/debian/control"
        )));
        assert!(DebianControlParser::is_match(Path::new("debian/control")));
        assert!(!DebianControlParser::is_match(Path::new(
            "/path/to/control"
        )));
        assert!(!DebianControlParser::is_match(Path::new(
            "/path/to/debian/changelog"
        )));
    }

    #[test]
    fn test_debian_installed_is_match() {
        assert!(DebianInstalledParser::is_match(Path::new(
            "/var/lib/dpkg/status"
        )));
        assert!(DebianInstalledParser::is_match(Path::new(
            "some/root/var/lib/dpkg/status"
        )));
        assert!(!DebianInstalledParser::is_match(Path::new(
            "/var/lib/dpkg/status.d/something"
        )));
        assert!(!DebianInstalledParser::is_match(Path::new(
            "/var/lib/dpkg/available"
        )));
    }

    // ====== Edge cases ======

    #[test]
    fn test_parse_debian_control_empty_input() {
        let packages = parse_debian_control("");
        assert!(packages.is_empty());
    }

    #[test]
    fn test_parse_debian_control_malformed_input() {
        let content = "this is not a valid control file\nwith random text";
        let packages = parse_debian_control(content);
        // Should not panic, may return empty or partial results
        assert!(packages.is_empty());
    }

    #[test]
    fn test_dependency_with_epoch_version() {
        // Debian versions can have epochs like 1:2.3.4
        let deps = parse_dependency_field(
            "zlib1g (>= 1:1.2.11)",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].extracted_requirement,
            Some(">= 1:1.2.11".to_string())
        );
    }

    #[test]
    fn test_dependency_with_plus_in_name() {
        let deps =
            parse_dependency_field("libstdc++6 (>= 10)", "depends", true, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert!(deps[0].purl.as_ref().unwrap().contains("libstdc%2B%2B6"));
    }

    #[test]
    fn test_dsc_parser_is_match() {
        assert!(DebianDscParser::is_match(&PathBuf::from("package.dsc")));
        assert!(DebianDscParser::is_match(&PathBuf::from(
            "adduser_3.118+deb11u1.dsc"
        )));
        assert!(!DebianDscParser::is_match(&PathBuf::from("control")));
        assert!(!DebianDscParser::is_match(&PathBuf::from("package.txt")));
    }

    #[test]
    fn test_dsc_parser_adduser() {
        let path = PathBuf::from("testdata/debian/dsc_files/adduser_3.118+deb11u1.dsc");
        let package = DebianDscParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PACKAGE_TYPE));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert_eq!(package.name, Some("adduser".to_string()));
        assert_eq!(package.version, Some("3.118+deb11u1".to_string()));
        assert_eq!(
            package.purl,
            Some("pkg:deb/debian/adduser@3.118%2Bdeb11u1?arch=all".to_string())
        );
        assert_eq!(
            package.vcs_url,
            Some("https://salsa.debian.org/debian/adduser.git".to_string())
        );
        assert_eq!(
            package.code_view_url,
            Some("https://salsa.debian.org/debian/adduser".to_string())
        );
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::DebianSourceControlDsc)
        );

        assert_eq!(package.parties.len(), 2);
        assert_eq!(package.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(
            package.parties[0].name,
            Some("Debian Adduser Developers".to_string())
        );
        assert_eq!(
            package.parties[0].email,
            Some("adduser@packages.debian.org".to_string())
        );
        assert_eq!(package.parties[0].r#type, None);

        assert_eq!(package.parties[1].role, Some("uploader".to_string()));
        assert_eq!(package.parties[1].name, Some("Marc Haber".to_string()));
        assert_eq!(
            package.parties[1].email,
            Some("mh+debian-packages@zugschlus.de".to_string())
        );
        assert_eq!(package.parties[1].r#type, None);

        assert_eq!(package.source_packages.len(), 1);
        assert_eq!(
            package.source_packages[0],
            "pkg:deb/debian/adduser".to_string()
        );

        assert!(!package.dependencies.is_empty());
        let build_dep_names: Vec<String> = package
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_ref())
            .filter(|p| p.contains("po-debconf") || p.contains("debhelper"))
            .map(|p| p.to_string())
            .collect();
        assert!(build_dep_names.len() >= 2);
    }

    #[test]
    fn test_dsc_parser_zsh() {
        let path = PathBuf::from("testdata/debian/dsc_files/zsh_5.7.1-1+deb10u1.dsc");
        let package = DebianDscParser::extract_first_package(&path);

        assert_eq!(package.name, Some("zsh".to_string()));
        assert_eq!(package.version, Some("5.7.1-1+deb10u1".to_string()));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert!(package.purl.is_some());
        assert!(package.purl.as_ref().unwrap().contains("zsh"));
        assert!(package.purl.as_ref().unwrap().contains("5.7.1"));
    }

    #[test]
    fn test_parse_dsc_content_basic() {
        let content = "Format: 3.0 (native)
Source: testpkg
Binary: testpkg
Architecture: amd64
Version: 1.0.0
Maintainer: Test User <test@example.com>
Standards-Version: 4.5.0
Build-Depends: debhelper (>= 12)
Files:
 abc123 1024 testpkg_1.0.0.tar.xz
";

        let package = parse_dsc_content(content);
        assert_eq!(package.name, Some("testpkg".to_string()));
        assert_eq!(package.version, Some("1.0.0".to_string()));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert_eq!(package.parties.len(), 1);
        assert_eq!(package.parties[0].name, Some("Test User".to_string()));
        assert_eq!(
            package.parties[0].email,
            Some("test@example.com".to_string())
        );
        assert_eq!(package.dependencies.len(), 1);
        assert!(package.purl.as_ref().unwrap().contains("arch=amd64"));
    }

    #[test]
    fn test_parse_dsc_content_with_uploaders() {
        let content = "Source: mypkg
Version: 2.0
Architecture: all
Maintainer: Main Dev <main@example.com>
Uploaders: Dev One <dev1@example.com>, Dev Two <dev2@example.com>
";

        let package = parse_dsc_content(content);
        assert_eq!(package.parties.len(), 3);
        assert_eq!(package.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(package.parties[1].role, Some("uploader".to_string()));
        assert_eq!(package.parties[2].role, Some("uploader".to_string()));
    }

    #[test]
    fn test_orig_tar_parser_is_match() {
        assert!(DebianOrigTarParser::is_match(&PathBuf::from(
            "package_1.0.orig.tar.gz"
        )));
        assert!(DebianOrigTarParser::is_match(&PathBuf::from(
            "abseil_0~20200923.3.orig.tar.xz"
        )));
        assert!(!DebianOrigTarParser::is_match(&PathBuf::from(
            "package.debian.tar.gz"
        )));
        assert!(!DebianOrigTarParser::is_match(&PathBuf::from("control")));
    }

    #[test]
    fn test_debian_tar_parser_is_match() {
        assert!(DebianDebianTarParser::is_match(&PathBuf::from(
            "package_1.0-1.debian.tar.xz"
        )));
        assert!(DebianDebianTarParser::is_match(&PathBuf::from(
            "abseil_20220623.1-1.debian.tar.gz"
        )));
        assert!(!DebianDebianTarParser::is_match(&PathBuf::from(
            "package.orig.tar.gz"
        )));
        assert!(!DebianDebianTarParser::is_match(&PathBuf::from("control")));
    }

    #[test]
    fn test_parse_orig_tar_filename() {
        let pkg = parse_source_tarball_filename(
            "abseil_0~20200923.3.orig.tar.gz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        assert_eq!(pkg.name, Some("abseil".to_string()));
        assert_eq!(pkg.version, Some("0~20200923.3".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(
            pkg.purl,
            Some("pkg:deb/debian/abseil@0~20200923.3".to_string())
        );
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianOriginalSourceTarball)
        );
    }

    #[test]
    fn test_parse_debian_tar_filename() {
        let pkg = parse_source_tarball_filename(
            "abseil_20220623.1-1.debian.tar.xz",
            DatasourceId::DebianSourceMetadataTarball,
        );
        assert_eq!(pkg.name, Some("abseil".to_string()));
        assert_eq!(pkg.version, Some("20220623.1-1".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(
            pkg.purl,
            Some("pkg:deb/debian/abseil@20220623.1-1".to_string())
        );
    }

    #[test]
    fn test_parse_deb_filename() {
        let pkg = parse_deb_filename("nginx_1.18.0-1_amd64.deb");
        assert_eq!(pkg.name, Some("nginx".to_string()));
        assert_eq!(pkg.version, Some("1.18.0-1".to_string()));

        let pkg = parse_deb_filename("invalid.deb");
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());
    }

    #[test]
    fn test_parse_source_tarball_various_compressions() {
        let pkg_gz = parse_source_tarball_filename(
            "test_1.0.orig.tar.gz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        let pkg_xz = parse_source_tarball_filename(
            "test_1.0.orig.tar.xz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        let pkg_bz2 = parse_source_tarball_filename(
            "test_1.0.orig.tar.bz2",
            DatasourceId::DebianOriginalSourceTarball,
        );

        assert_eq!(pkg_gz.version, Some("1.0".to_string()));
        assert_eq!(pkg_xz.version, Some("1.0".to_string()));
        assert_eq!(pkg_bz2.version, Some("1.0".to_string()));
    }

    #[test]
    fn test_parse_source_tarball_invalid_format() {
        let pkg = parse_source_tarball_filename(
            "invalid-no-underscore.tar.gz",
            DatasourceId::DebianOriginalSourceTarball,
        );
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());
    }

    #[test]
    fn test_list_parser_is_match() {
        assert!(DebianInstalledListParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.list"
        )));
        assert!(DebianInstalledListParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/package:amd64.list"
        )));
        assert!(!DebianInstalledListParser::is_match(&PathBuf::from(
            "bash.list"
        )));
        assert!(!DebianInstalledListParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.md5sums"
        )));
    }

    #[test]
    fn test_md5sums_parser_is_match() {
        assert!(DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.md5sums"
        )));
        assert!(DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/package:amd64.md5sums"
        )));
        assert!(!DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "bash.md5sums"
        )));
        assert!(!DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.list"
        )));
    }

    #[test]
    fn test_parse_debian_file_list_plain_list() {
        let content = "/.
/bin
/bin/bash
/usr/bin/bashbug
/usr/share/doc/bash/README
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.name, Some("bash".to_string()));
        assert_eq!(pkg.file_references.len(), 3);
        assert_eq!(pkg.file_references[0].path, "/bin/bash");
        assert_eq!(pkg.file_references[0].md5, None);
        assert_eq!(pkg.file_references[1].path, "/usr/bin/bashbug");
        assert_eq!(pkg.file_references[2].path, "/usr/share/doc/bash/README");
    }

    #[test]
    fn test_parse_debian_file_list_md5sums() {
        let content = "77506afebd3b7e19e937a678a185b62e  bin/bash
1c77d2031971b4e4c512ac952102cd85  usr/bin/bashbug
f55e3a16959b0bb8915cb5f219521c80  usr/share/doc/bash/COMPAT.gz
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.name, Some("bash".to_string()));
        assert_eq!(pkg.file_references.len(), 3);
        assert_eq!(pkg.file_references[0].path, "bin/bash");
        assert_eq!(
            pkg.file_references[0].md5,
            Some(Md5Digest::from_hex("77506afebd3b7e19e937a678a185b62e").unwrap())
        );
        assert_eq!(pkg.file_references[1].path, "usr/bin/bashbug");
        assert_eq!(
            pkg.file_references[1].md5,
            Some(Md5Digest::from_hex("1c77d2031971b4e4c512ac952102cd85").unwrap())
        );
    }

    #[test]
    fn test_parse_debian_file_list_with_arch() {
        let content = "/usr/bin/foo
/usr/lib/x86_64-linux-gnu/libfoo.so
";
        let pkg = parse_debian_file_list(
            content,
            "libfoo:amd64",
            DatasourceId::DebianInstalledFilesList,
        );
        assert_eq!(pkg.name, Some("libfoo".to_string()));
        assert!(pkg.purl.is_some());
        assert!(pkg.purl.as_ref().unwrap().contains("arch=amd64"));
        assert_eq!(pkg.file_references.len(), 2);
    }

    #[test]
    fn test_parse_debian_file_list_skips_comments_and_empty() {
        let content = "# This is a comment
/bin/bash

/usr/bin/bashbug
  
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.file_references.len(), 2);
    }

    #[test]
    fn test_parse_debian_file_list_md5sums_only() {
        let content = "abc123  usr/bin/tool
";
        let pkg =
            parse_debian_file_list(content, "md5sums", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.name, None);
        assert_eq!(pkg.file_references.len(), 1);
    }

    #[test]
    fn test_parse_debian_file_list_ignores_root_dirs() {
        let content = "/.
/bin
/bin/bash
/etc
/usr
/var
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.file_references.len(), 1);
        assert_eq!(pkg.file_references[0].path, "/bin/bash");
    }

    #[test]
    fn test_copyright_parser_is_match() {
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "/usr/share/doc/bash/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "debian/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "src/third_party/gperftools/dist/packages/deb/copyright"
        )));
        assert!(!DebianCopyrightParser::is_match(&PathBuf::from(
            "copyright.txt"
        )));
        assert!(!DebianCopyrightParser::is_match(&PathBuf::from(
            "/etc/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "/tmp/sample_copyright"
        )));
    }

    #[test]
    fn test_detect_debian_copyright_datasource() {
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("debian/copyright")),
            DatasourceId::DebianCopyrightInSource
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from(
                "src/third_party/gperftools/dist/packages/deb/copyright"
            )),
            DatasourceId::DebianCopyrightStandalone
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("/usr/share/doc/bash/copyright")),
            DatasourceId::DebianCopyrightInPackage
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("stable_copyright")),
            DatasourceId::DebianCopyrightStandalone
        );
    }

    #[test]
    fn test_extract_package_name_from_path() {
        assert_eq!(
            extract_package_name_from_path(&PathBuf::from("/usr/share/doc/bash/copyright")),
            Some("bash".to_string())
        );
        assert_eq!(
            extract_package_name_from_path(&PathBuf::from("/usr/share/doc/libseccomp2/copyright")),
            Some("libseccomp2".to_string())
        );
        assert_eq!(
            extract_package_name_from_path(&PathBuf::from("debian/copyright")),
            None
        );
    }

    #[test]
    fn test_parse_copyright_dep5_format() {
        let content = "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: libseccomp
Source: https://sourceforge.net/projects/libseccomp/

Files: *
Copyright: 2012 Paul Moore <pmoore@redhat.com>
 2012 Ashley Lai <adlai@us.ibm.com>
License: LGPL-2.1

License: LGPL-2.1
 This library is free software
";
        let pkg = parse_copyright_file(content, Some("libseccomp"));
        assert_eq!(pkg.name, Some("libseccomp".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::DebianCopyright));
        assert_eq!(
            pkg.extracted_license_statement,
            Some("LGPL-2.1".to_string())
        );
        assert!(pkg.parties.len() >= 2);
        assert_eq!(pkg.parties[0].role, Some("copyright-holder".to_string()));
        assert!(pkg.parties[0].name.as_ref().unwrap().contains("Paul Moore"));
    }

    #[test]
    fn test_parse_copyright_primary_license_detection_from_bsdutils_fixture() {
        let path = PathBuf::from(
            "testdata/debian-fixtures/debian-slim-2021-04-07/usr/share/doc/bsdutils/copyright",
        );
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("bsdutils".to_string()));
        let extracted = pkg
            .extracted_license_statement
            .as_deref()
            .expect("license statement should exist");
        assert!(extracted.contains("GPL-2+"));
        assert!(!pkg.license_detections.is_empty());

        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: GPL-2+")
        );
        assert_eq!(primary.matches[0].start_line, 47);
        assert_eq!(primary.matches[0].end_line, 47);
    }

    #[test]
    fn test_parse_copyright_emits_ordered_absolute_case_preserved_detections() {
        let path = PathBuf::from("testdata/debian/copyright/copyright");
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        assert_eq!(pkg.license_detections.len(), 1);
        assert_eq!(pkg.other_license_detections.len(), 4);

        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: LGPL-2.1")
        );
        assert_eq!(primary.matches[0].start_line, 11);

        let ordered_lines: Vec<usize> = pkg
            .other_license_detections
            .iter()
            .map(|detection| detection.matches[0].start_line)
            .collect();
        assert_eq!(ordered_lines, vec![15, 19, 23, 25]);

        let ordered_texts: Vec<&str> = pkg
            .other_license_detections
            .iter()
            .map(|detection| detection.matches[0].matched_text.as_deref().unwrap())
            .collect();
        assert_eq!(
            ordered_texts,
            vec![
                "License: LGPL-2.1",
                "License: LGPL-2.1",
                "License: LGPL-2.1",
                "License: LGPL-2.1",
            ]
        );
    }

    #[test]
    fn test_parse_copyright_detects_bottom_standalone_license_paragraph() {
        let path = PathBuf::from(
            "testdata/debian-fixtures/debian-2019-11-15/main/c/clamav/stable_copyright",
        );
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        let zlib = pkg
            .other_license_detections
            .iter()
            .find(|detection| detection.matches[0].matched_text.as_deref() == Some("License: Zlib"))
            .expect("at least one Zlib license paragraph should be detected");
        assert_eq!(
            zlib.matches[0].matched_text.as_deref(),
            Some("License: Zlib")
        );

        let last_zlib = pkg
            .other_license_detections
            .iter()
            .rev()
            .find(|detection| detection.matches[0].matched_text.as_deref() == Some("License: Zlib"))
            .expect("bottom standalone Zlib license paragraph should be detected");
        assert_eq!(last_zlib.matches[0].start_line, 732);
        assert_eq!(last_zlib.matches[0].end_line, 732);
    }

    #[test]
    fn test_parse_copyright_uses_header_paragraph_as_primary_when_files_star_is_blank() {
        let path =
            PathBuf::from("testdata/debian-fixtures/crafted_for_tests/test_license_nameless");
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        assert_eq!(pkg.license_detections.len(), 1);
        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: LGPL-3+ or GPL-2+")
        );
        assert_eq!(primary.matches[0].start_line, 8);
        assert_eq!(primary.matches[0].end_line, 8);

        assert!(pkg.other_license_detections.iter().any(|detection| {
            detection.matches[0].matched_text.as_deref() == Some("License: GPL-2+")
        }));
    }

    #[test]
    fn test_parse_copyright_prefers_files_star_primary_over_header_paragraph() {
        let content = "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nUpstream-Name: foo\nLicense: MIT\n\nFiles: *\nCopyright: 2024 Example\nLicense: GPL-2+\n";
        let pkg = parse_copyright_file(content, Some("foo"));

        assert_eq!(pkg.license_detections.len(), 1);
        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: GPL-2+")
        );
        assert_eq!(primary.matches[0].start_line, 7);
    }

    #[test]
    fn test_finalize_copyright_paragraph_matches_rfc822_headers_and_license_line() {
        let raw_lines = vec![
            "Files: *".to_string(),
            "Copyright: 2024 Example Org".to_string(),
            "License: Apache-2.0".to_string(),
            " Licensed under the Apache License, Version 2.0.".to_string(),
        ];

        let paragraph = finalize_copyright_paragraph(raw_lines.clone(), 10);
        let expected = rfc822::parse_rfc822_paragraphs(&raw_lines.join("\n"))
            .into_iter()
            .next()
            .expect("reference RFC822 paragraph should parse");

        assert_eq!(paragraph.metadata.headers, expected.headers);
        assert_eq!(paragraph.metadata.body, expected.body);
        assert_eq!(
            paragraph.license_header_line,
            Some(("License: Apache-2.0".to_string(), 12))
        );
    }

    #[test]
    fn test_parse_copyright_unstructured() {
        let content = "This package was debianized by John Doe.

Upstream Authors:
    Jane Smith

Copyright:
    2009 10gen

License:
    SSPL
";
        let pkg = parse_copyright_file(content, Some("mongodb"));
        assert_eq!(pkg.name, Some("mongodb".to_string()));
        assert_eq!(pkg.extracted_license_statement, Some("SSPL".to_string()));
        assert!(!pkg.parties.is_empty());
    }

    #[test]
    fn test_parse_copyright_holders() {
        let text = "2012 Paul Moore <pmoore@redhat.com>
2012 Ashley Lai <adlai@us.ibm.com>
Copyright (C) 2015-2018 Example Corp";
        let holders = parse_copyright_holders(text);
        assert!(holders.len() >= 3);
        assert!(holders.iter().any(|h| h.contains("Paul Moore")));
        assert!(holders.iter().any(|h| h.contains("Example Corp")));
    }

    #[test]
    fn test_parse_copyright_empty() {
        let content = "This is just some text without proper copyright info.";
        let pkg = parse_copyright_file(content, Some("test"));
        assert_eq!(pkg.name, Some("test".to_string()));
        assert!(pkg.parties.is_empty());
        assert!(pkg.extracted_license_statement.is_none());
    }

    #[test]
    fn test_deb_parser_is_match() {
        assert!(DebianDebParser::is_match(&PathBuf::from("package.deb")));
        assert!(DebianDebParser::is_match(&PathBuf::from(
            "libapache2-mod-md_2.4.38-3+deb10u10_amd64.deb"
        )));
        assert!(!DebianDebParser::is_match(&PathBuf::from("package.tar.gz")));
        assert!(!DebianDebParser::is_match(&PathBuf::from("control")));
    }

    #[test]
    fn test_parse_deb_filename_with_arch() {
        let pkg = parse_deb_filename("libapache2-mod-md_2.4.38-3+deb10u10_amd64.deb");
        assert_eq!(pkg.name, Some("libapache2-mod-md".to_string()));
        assert_eq!(pkg.version, Some("2.4.38-3+deb10u10".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(
            pkg.purl,
            Some("pkg:deb/debian/libapache2-mod-md@2.4.38-3%2Bdeb10u10?arch=amd64".to_string())
        );
        assert_eq!(pkg.datasource_id, Some(DatasourceId::DebianDeb));
    }

    #[test]
    fn test_parse_deb_filename_without_arch() {
        let pkg = parse_deb_filename("package_1.0-1_all.deb");
        assert_eq!(pkg.name, Some("package".to_string()));
        assert_eq!(pkg.version, Some("1.0-1".to_string()));
        assert!(pkg.purl.as_ref().unwrap().contains("arch=all"));
    }

    #[test]
    fn test_extract_deb_archive() {
        let test_path = PathBuf::from("testdata/debian/deb/adduser_3.112ubuntu1_all.deb");
        if !test_path.exists() {
            return;
        }

        let pkg = DebianDebParser::extract_first_package(&test_path);

        assert_eq!(pkg.name, Some("adduser".to_string()));
        assert_eq!(pkg.version, Some("3.112ubuntu1".to_string()));
        assert_eq!(pkg.namespace, Some("ubuntu".to_string()));
        assert!(pkg.description.is_some());
        assert!(!pkg.parties.is_empty());

        assert!(pkg.purl.as_ref().unwrap().contains("adduser"));
        assert!(pkg.purl.as_ref().unwrap().contains("3.112ubuntu1"));
    }

    #[test]
    fn test_extract_deb_archive_with_control_tar_xz() {
        let deb = create_synthetic_deb_with_control_tar_xz();

        let pkg = DebianDebParser::extract_first_package(deb.path());

        assert_eq!(pkg.name, Some("synthetic".to_string()));
        assert_eq!(pkg.version, Some("1.2.3".to_string()));
        assert_eq!(pkg.description, Some("Synthetic deb".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_extract_deb_archive_collects_embedded_copyright_metadata() {
        let deb = create_synthetic_deb_with_copyright();

        let pkg = DebianDebParser::extract_first_package(deb.path());

        assert_eq!(pkg.name, Some("synthetic".to_string()));
        assert_eq!(
            pkg.extracted_license_statement,
            Some("Apache-2.0".to_string())
        );
        assert!(pkg.parties.iter().any(|party| {
            party.role.as_deref() == Some("copyright-holder")
                && party.name.as_deref() == Some("Example Org")
        }));
    }

    #[test]
    fn test_parse_deb_filename_simple() {
        let pkg = parse_deb_filename("adduser_3.112ubuntu1_all.deb");
        assert_eq!(pkg.name, Some("adduser".to_string()));
        assert_eq!(pkg.version, Some("3.112ubuntu1".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
    }

    #[test]
    fn test_parse_deb_filename_invalid() {
        let pkg = parse_deb_filename("invalid.deb");
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());
    }

    #[test]
    fn test_distroless_parser() {
        let test_file = PathBuf::from("testdata/debian/var/lib/dpkg/status.d/base-files");

        assert!(DebianDistrolessInstalledParser::is_match(&test_file));

        if !test_file.exists() {
            eprintln!("Warning: Test file not found, skipping test");
            return;
        }

        let pkg = DebianDistrolessInstalledParser::extract_first_package(&test_file);

        assert_eq!(pkg.package_type, Some(PackageType::Deb));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianDistrolessInstalledDb)
        );
        assert_eq!(pkg.name, Some("base-files".to_string()));
        assert_eq!(pkg.version, Some("11.1+deb11u8".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert!(pkg.purl.is_some());
        assert!(
            pkg.purl
                .as_ref()
                .unwrap()
                .contains("pkg:deb/debian/base-files")
        );
    }
}
