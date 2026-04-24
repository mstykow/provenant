// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for RPM package archives.
//!
//! Extracts package metadata and dependencies from binary RPM package (.rpm) files
//! by reading the embedded header metadata.
//!
//! # Supported Formats
//! - *.rpm (binary RPM package archives)
//!
//! # Key Features
//! - Metadata extraction from RPM headers (name, version, release, architecture)
//! - Dependency extraction (requires, provides, obsoletes)
//! - License and distribution information parsing
//! - Package URL (purl) generation for installed packages
//! - Graceful handling of malformed or corrupted RPM files
//!
//! # Implementation Notes
//! - Uses `rpm` crate for low-level RPM format parsing
//! - RPM architecture is captured as namespace in metadata
//! - Direct dependency tracking (all requires are direct)
//! - Error handling with `warn!()` logs on parse failures

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::LazyLock;

use crate::parser_warn as warn;
use regex::Regex;
use rpm::{
    HEADER_MAGIC, INDEX_ENTRY_SIZE, INDEX_HEADER_SIZE, IndexTag, LEAD_SIZE, PackageMetadata,
    RPM_MAGIC,
};

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::utils::{MAX_ITERATION_COUNT, MAX_MANIFEST_SIZE, truncate_field};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_data,
    empty_declared_license_data, normalize_declared_license_key, normalize_spdx_expression,
};

const PACKAGE_TYPE: PackageType = PackageType::Rpm;
const RPM_HEADER_PARSE_LIMIT_BYTES: u64 = MAX_MANIFEST_SIZE.saturating_add(1);

static RE_RPM_LICENSE_AND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+and\s+").expect("valid RPM license AND regex"));
static RE_RPM_LICENSE_OR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+or\s+").expect("valid RPM license OR regex"));
static RE_RPM_LICENSE_COMMA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*,\s*").expect("valid RPM license comma regex"));
static RE_RPM_LICENSE_WITH_EXCEPTIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+with\s+exceptions\b").expect("valid RPM license exceptions regex")
});

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::RpmArchive),
        ..Default::default()
    }
}

pub(crate) fn infer_rpm_namespace(
    distribution: Option<&str>,
    vendor: Option<&str>,
    release: Option<&str>,
    dist_url: Option<&str>,
) -> Option<String> {
    for candidate in [distribution, vendor, dist_url].into_iter().flatten() {
        let lower = candidate.to_ascii_lowercase();
        if lower.contains("fedora") || lower.contains("koji") {
            return Some("fedora".to_string());
        }
        if lower.contains("centos") {
            return Some("centos".to_string());
        }
        if lower.contains("red hat") || lower.contains("redhat") || lower.contains("ubi") {
            return Some("rhel".to_string());
        }
        if lower.contains("opensuse") {
            return Some("opensuse".to_string());
        }
        if lower.contains("suse") {
            return Some("suse".to_string());
        }
        if lower.contains("openmandriva") || lower.contains("mandriva") {
            return Some("openmandriva".to_string());
        }
        if lower.contains("mariner") {
            return Some("mariner".to_string());
        }
    }

    if let Some(release) = release {
        let lower = release.to_ascii_lowercase();
        if lower.contains(".fc") {
            return Some("fedora".to_string());
        }
        if lower.contains(".el") {
            return Some("rhel".to_string());
        }
        if lower.contains("mdv") || lower.contains("mnb") {
            return Some("openmandriva".to_string());
        }
        if lower.contains("suse") {
            return Some("suse".to_string());
        }
    }

    None
}

fn rpm_header_string(metadata: &PackageMetadata, tag: IndexTag) -> Option<String> {
    metadata
        .header
        .get_entry_data_as_string(tag)
        .ok()
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed == "(none)" {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
}

fn rpm_header_string_array(metadata: &PackageMetadata, tag: IndexTag) -> Option<Vec<String>> {
    metadata
        .header
        .get_entry_data_as_string_array(tag)
        .ok()
        .map(|items| {
            items
                .iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty() && item != "(none)")
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
}

fn infer_vcs_url(metadata: &PackageMetadata, source_urls: &[String]) -> Option<String> {
    if let Ok(vcs) = metadata.get_vcs()
        && !vcs.trim().is_empty()
    {
        return Some(vcs.to_string());
    }

    source_urls
        .iter()
        .find(|url| url.starts_with("git+") || url.contains("src.fedoraproject.org"))
        .cloned()
}

fn build_rpm_qualifiers(
    architecture: Option<&str>,
    is_source: bool,
) -> Option<std::collections::HashMap<String, String>> {
    let mut qualifiers = std::collections::HashMap::new();

    if let Some(arch) = architecture.filter(|arch| !arch.is_empty()) {
        qualifiers.insert("arch".to_string(), arch.to_string());
    }

    if is_source {
        qualifiers.insert("source".to_string(), "true".to_string());
    }

    (!qualifiers.is_empty()).then_some(qualifiers)
}

pub(crate) fn is_rpm_archive_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| matches!(ext, "rpm" | "srpm"))
}

pub(crate) fn path_looks_like_rpm_archive(path: &Path) -> bool {
    if is_rpm_archive_extension(path) {
        return true;
    }

    if fs::metadata(path).is_err() {
        return false;
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic).is_ok() && magic == RPM_MAGIC
}

fn parse_rpm_metadata_only(path: &Path) -> Result<PackageMetadata, String> {
    let file =
        File::open(path).map_err(|e| format!("Failed to open RPM file {:?}: {}", path, e))?;
    let limited_file = file.take(RPM_HEADER_PARSE_LIMIT_BYTES);
    let mut reader = BufReader::new(limited_file);

    PackageMetadata::parse(&mut reader)
        .map_err(|e| format!("Failed to parse RPM file {:?}: {}", path, e))
}

#[derive(Debug, Clone, Copy)]
struct RpmHeaderEntryView {
    tag: u32,
    data_type: u32,
    offset: usize,
    num_items: usize,
}

struct ParsedRpmHeader<'a> {
    entries: Vec<RpmHeaderEntryView>,
    store: &'a [u8],
}

#[derive(Default)]
struct SalvagedRpmFields {
    name: Option<String>,
    version: Option<String>,
    release: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    distribution: Option<String>,
    vendor: Option<String>,
    license: Option<String>,
    packager: Option<String>,
    group: Option<String>,
    url: Option<String>,
    arch: Option<String>,
    source_rpm: Option<String>,
    dist_url: Option<String>,
}

fn read_rpm_header_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file =
        File::open(path).map_err(|e| format!("Failed to open RPM file {:?}: {}", path, e))?;
    let mut limited_file = file.take(RPM_HEADER_PARSE_LIMIT_BYTES);
    let mut bytes = Vec::new();
    limited_file
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read RPM file {:?}: {}", path, e))?;
    Ok(bytes)
}

fn parse_index_header(bytes: &[u8], offset: usize) -> Option<(usize, usize)> {
    let header = bytes.get(offset..offset + INDEX_HEADER_SIZE as usize)?;
    if header.get(..3)? != HEADER_MAGIC {
        return None;
    }
    if header.get(3).copied()? != 1 {
        return None;
    }

    let num_entries = u32::from_be_bytes(header.get(8..12)?.try_into().ok()?) as usize;
    let data_section_size = u32::from_be_bytes(header.get(12..16)?.try_into().ok()?) as usize;
    Some((num_entries, data_section_size))
}

fn parse_header_entries<'a>(
    bytes: &'a [u8],
    offset: usize,
    allow_truncated_store: bool,
) -> Option<(ParsedRpmHeader<'a>, usize)> {
    let (num_entries, data_section_size) = parse_index_header(bytes, offset)?;
    let entries_offset = offset.checked_add(INDEX_HEADER_SIZE as usize)?;
    let entries_size = num_entries.checked_mul(INDEX_ENTRY_SIZE as usize)?;
    let store_offset = entries_offset.checked_add(entries_size)?;
    bytes.get(entries_offset..store_offset)?;
    let store_end = store_offset.checked_add(data_section_size)?;
    let store = if allow_truncated_store {
        bytes.get(store_offset..).unwrap_or(&[])
    } else {
        bytes.get(store_offset..store_end)?
    };

    let mut entries = Vec::with_capacity(num_entries);
    for index in 0..num_entries {
        let entry_offset =
            entries_offset.checked_add(index.checked_mul(INDEX_ENTRY_SIZE as usize)?)?;
        let entry = bytes.get(entry_offset..entry_offset + INDEX_ENTRY_SIZE as usize)?;
        entries.push(RpmHeaderEntryView {
            tag: u32::from_be_bytes(entry.get(0..4)?.try_into().ok()?),
            data_type: u32::from_be_bytes(entry.get(4..8)?.try_into().ok()?),
            offset: u32::from_be_bytes(entry.get(8..12)?.try_into().ok()?) as usize,
            num_items: u32::from_be_bytes(entry.get(12..16)?.try_into().ok()?) as usize,
        });
    }

    Some((ParsedRpmHeader { entries, store }, store_end))
}

fn parse_main_rpm_header(bytes: &[u8]) -> Option<ParsedRpmHeader<'_>> {
    if bytes.get(..RPM_MAGIC.len())? != RPM_MAGIC {
        return None;
    }

    let (_, signature_end) = parse_header_entries(bytes, LEAD_SIZE as usize, false)?;
    let signature_padding = (8 - (signature_end - (LEAD_SIZE as usize)) % 8) % 8;
    let main_header_offset = signature_end.checked_add(signature_padding)?;
    let (header, _) = parse_header_entries(bytes, main_header_offset, true)?;
    Some(header)
}

fn read_header_string(store: &[u8], offset: usize) -> Option<(String, usize)> {
    let remaining = store.get(offset..)?;
    let nul = remaining.iter().position(|byte| *byte == 0)?;
    let text = String::from_utf8_lossy(&remaining[..nul])
        .trim()
        .to_string();
    let next_offset = offset.checked_add(nul)?.checked_add(1)?;
    if text.is_empty() || text == "(none)" {
        None
    } else {
        Some((text, next_offset))
    }
}

fn read_entry_first_string(header: &ParsedRpmHeader<'_>, tag: u32) -> Option<String> {
    let entry = header.entries.iter().find(|entry| entry.tag == tag)?;
    match entry.data_type {
        6 => read_header_string(header.store, entry.offset).map(|(value, _)| value),
        8 | 9 => {
            let mut offset = entry.offset;
            let mut first_value = None;
            for _ in 0..entry.num_items {
                let (value, next_offset) = read_header_string(header.store, offset)?;
                first_value.get_or_insert(value);
                offset = next_offset;
            }
            first_value
        }
        _ => None,
    }
}

fn salvage_rpm_header_fields(path: &Path) -> Option<SalvagedRpmFields> {
    let bytes = read_rpm_header_bytes(path).ok()?;
    let header = parse_main_rpm_header(&bytes)?;

    Some(SalvagedRpmFields {
        name: read_entry_first_string(&header, IndexTag::RPMTAG_NAME as u32).map(truncate_field),
        version: read_entry_first_string(&header, IndexTag::RPMTAG_VERSION as u32)
            .map(truncate_field),
        release: read_entry_first_string(&header, IndexTag::RPMTAG_RELEASE as u32)
            .map(truncate_field),
        summary: read_entry_first_string(&header, IndexTag::RPMTAG_SUMMARY as u32)
            .map(truncate_field),
        description: read_entry_first_string(&header, IndexTag::RPMTAG_DESCRIPTION as u32)
            .map(truncate_field),
        distribution: read_entry_first_string(&header, IndexTag::RPMTAG_DISTRIBUTION as u32)
            .map(truncate_field),
        vendor: read_entry_first_string(&header, IndexTag::RPMTAG_VENDOR as u32)
            .map(truncate_field),
        license: read_entry_first_string(&header, IndexTag::RPMTAG_LICENSE as u32)
            .map(truncate_field),
        packager: read_entry_first_string(&header, IndexTag::RPMTAG_PACKAGER as u32)
            .map(truncate_field),
        group: read_entry_first_string(&header, IndexTag::RPMTAG_GROUP as u32).map(truncate_field),
        url: read_entry_first_string(&header, IndexTag::RPMTAG_URL as u32).map(truncate_field),
        arch: read_entry_first_string(&header, IndexTag::RPMTAG_ARCH as u32).map(truncate_field),
        source_rpm: read_entry_first_string(&header, IndexTag::RPMTAG_SOURCERPM as u32)
            .map(truncate_field),
        dist_url: read_entry_first_string(&header, IndexTag::RPMTAG_DISTURL as u32)
            .map(truncate_field),
    })
}

fn build_salvaged_rpm_package(path: &Path, fields: SalvagedRpmFields) -> Option<PackageData> {
    let name = fields.name?;
    let mut version = fields.version;
    if let Some(release) = fields.release.as_deref() {
        let mut evr = version.take().unwrap_or_default();
        if !evr.is_empty() {
            evr.push('-');
        }
        evr.push_str(release);
        version = Some(truncate_field(evr));
    }

    let namespace = infer_rpm_namespace(
        fields.distribution.as_deref(),
        fields.vendor.as_deref(),
        fields.release.as_deref(),
        fields.dist_url.as_deref(),
    )
    .or_else(|| infer_rpm_namespace_from_filename(path))
    .map(truncate_field);
    let is_source =
        path.to_string_lossy().ends_with(".src.rpm") || path.to_string_lossy().ends_with(".srpm");
    let qualifiers = build_rpm_qualifiers(fields.arch.as_deref(), is_source);

    let mut parties = Vec::new();
    if let Some(vendor) = fields.vendor.clone() {
        parties.push(Party {
            r#type: Some("organization".to_string()),
            role: Some("vendor".to_string()),
            name: Some(vendor),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }
    if let Some(distribution) = fields.distribution.clone() {
        parties.push(Party {
            r#type: Some("organization".to_string()),
            role: Some("distributor".to_string()),
            name: Some(distribution),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }
    if let Some(packager) = fields.packager.as_deref() {
        let (name_opt, email_opt) = parse_packager(packager);
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("packager".to_string()),
            name: name_opt.map(truncate_field),
            email: email_opt.map(truncate_field),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let extracted_license_statement = fields.license.map(truncate_field);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        extracted_license_statement
            .as_deref()
            .and_then(normalize_rpm_declared_license)
            .map(|normalized| {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(
                        extracted_license_statement.as_deref().unwrap_or_default(),
                    ),
                )
            })
            .map(|(expr, spdx, detections)| {
                (
                    expr.map(truncate_field),
                    spdx.map(truncate_field),
                    detections,
                )
            })
            .unwrap_or_else(empty_declared_license_data);

    let mut extra_data = std::collections::HashMap::new();
    if let Some(distribution) = fields.distribution.clone() {
        extra_data.insert(
            "distribution".to_string(),
            serde_json::Value::String(distribution),
        );
    }
    if let Some(dist_url) = fields.dist_url.clone() {
        extra_data.insert("dist_url".to_string(), serde_json::Value::String(dist_url));
    }

    Some(PackageData {
        datasource_id: Some(DatasourceId::RpmArchive),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name.clone()),
        version: version.clone(),
        qualifiers,
        description: fields.description.or(fields.summary),
        homepage_url: fields.url,
        parties,
        keywords: fields.group.into_iter().collect(),
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        source_packages: fields.source_rpm.into_iter().collect(),
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
        purl: build_rpm_purl(
            &name,
            version.as_deref(),
            namespace.as_deref(),
            fields.arch.as_deref(),
            is_source,
        )
        .map(truncate_field),
        ..Default::default()
    })
}

pub(crate) fn extract_rpm_packages(path: &Path) -> Vec<PackageData> {
    if let Err(e) = fs::metadata(path) {
        warn!("Cannot stat RPM file {:?}: {}", path, e);
        return vec![default_package_data()];
    }

    let metadata = match parse_rpm_metadata_only(path) {
        Ok(metadata) => metadata,
        Err(message) => {
            if let Some(package) = salvage_rpm_header_fields(path)
                .and_then(|fields| build_salvaged_rpm_package(path, fields))
            {
                return vec![package];
            }
            warn!("{}", message);
            return vec![default_package_data()];
        }
    };

    vec![parse_rpm_package(&metadata, path)]
}

/// Parser for RPM package archives
pub struct RpmParser;

impl PackageParser for RpmParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path_looks_like_rpm_archive(path)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        extract_rpm_packages(path)
    }
}

pub(crate) fn infer_rpm_namespace_from_filename(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_str()?.to_ascii_lowercase();

    if filename.contains(".fc") {
        return Some("fedora".to_string());
    }
    if filename.contains(".el") {
        return Some("rhel".to_string());
    }
    if filename.contains("mdv") || filename.contains("mnb") {
        return Some("openmandriva".to_string());
    }
    if filename.contains("opensuse") {
        return Some("opensuse".to_string());
    }
    if filename.contains("suse") {
        return Some("suse".to_string());
    }

    None
}

fn parse_rpm_package(metadata: &PackageMetadata, path: &Path) -> PackageData {
    let name = metadata
        .get_name()
        .ok()
        .map(|s| truncate_field(s.to_string()));
    let version = build_evr_version(metadata).map(truncate_field);
    let description = metadata
        .get_description()
        .ok()
        .map(|s| truncate_field(s.to_string()));
    let homepage_url = metadata
        .get_url()
        .ok()
        .map(|s| truncate_field(s.to_string()));
    let architecture = metadata
        .get_arch()
        .ok()
        .map(|s| truncate_field(s.to_string()));
    let path_str = path.to_string_lossy();
    let is_source = metadata.is_source_package()
        || path_str.ends_with(".src.rpm")
        || path_str.ends_with(".srpm");
    let distribution =
        rpm_header_string(metadata, IndexTag::RPMTAG_DISTRIBUTION).map(truncate_field);
    let dist_url = rpm_header_string(metadata, IndexTag::RPMTAG_DISTURL).map(truncate_field);
    let bug_tracking_url = rpm_header_string(metadata, IndexTag::RPMTAG_BUGURL).map(truncate_field);
    let source_urls =
        rpm_header_string_array(metadata, IndexTag::RPMTAG_SOURCE).unwrap_or_default();
    let source_rpm = metadata
        .get_source_rpm()
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| truncate_field(value.to_string()));
    let namespace = infer_rpm_namespace(
        distribution.as_deref(),
        metadata.get_vendor().ok(),
        metadata.get_release().ok(),
        dist_url.as_deref(),
    )
    .or_else(|| infer_rpm_namespace_from_filename(path))
    .map(truncate_field);

    let mut parties = Vec::new();

    if let Ok(vendor) = metadata.get_vendor()
        && !vendor.is_empty()
    {
        parties.push(Party {
            r#type: Some("organization".to_string()),
            role: Some("vendor".to_string()),
            name: Some(truncate_field(vendor.to_string())),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    if let Some(distribution_name) = distribution.as_ref() {
        parties.push(Party {
            r#type: Some("organization".to_string()),
            role: Some("distributor".to_string()),
            name: Some(distribution_name.clone()),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    if let Ok(packager) = metadata.get_packager()
        && !packager.is_empty()
    {
        let (name_opt, email_opt) = parse_packager(packager);
        parties.push(Party {
            r#type: Some("person".to_string()),
            role: Some("packager".to_string()),
            name: name_opt.map(truncate_field),
            email: email_opt.map(truncate_field),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let extracted_license_statement = metadata
        .get_license()
        .ok()
        .map(|s| truncate_field(s.to_string()));
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        extracted_license_statement
            .as_deref()
            .and_then(normalize_rpm_declared_license)
            .map(|normalized| {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(
                        extracted_license_statement.as_deref().unwrap_or_default(),
                    ),
                )
            })
            .map(|(expr, spdx, detections)| {
                (
                    expr.map(truncate_field),
                    spdx.map(truncate_field),
                    detections,
                )
            })
            .unwrap_or_else(empty_declared_license_data);

    let dependencies = extract_rpm_dependencies(metadata, namespace.as_deref());

    let qualifiers = build_rpm_qualifiers(architecture.as_deref(), is_source);

    let mut keywords = Vec::new();
    if let Ok(group) = metadata.get_group()
        && !group.is_empty()
    {
        keywords.push(truncate_field(group.to_string()));
    }

    let mut extra_data = std::collections::HashMap::new();
    if let Some(distribution) = distribution.clone() {
        extra_data.insert(
            "distribution".to_string(),
            serde_json::Value::String(distribution),
        );
    }
    if let Some(dist_url) = dist_url.clone() {
        extra_data.insert("dist_url".to_string(), serde_json::Value::String(dist_url));
    }
    if let Ok(build_host) = metadata.get_build_host()
        && !build_host.is_empty()
    {
        extra_data.insert(
            "build_host".to_string(),
            serde_json::Value::String(build_host.to_string()),
        );
    }
    if let Ok(build_time) = metadata.get_build_time() {
        extra_data.insert(
            "build_time".to_string(),
            serde_json::Value::Number(serde_json::Number::from(build_time)),
        );
    }
    if !source_urls.is_empty() {
        extra_data.insert(
            "source_urls".to_string(),
            serde_json::Value::Array(
                source_urls
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    if let Some(provides) = extract_rpm_relationships(metadata, RpmRelationshipKind::Provides)
        && !provides.is_empty()
    {
        extra_data.insert(
            "provides".to_string(),
            serde_json::Value::Array(
                provides
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    if let Some(obsoletes) = extract_rpm_relationships(metadata, RpmRelationshipKind::Obsoletes)
        && !obsoletes.is_empty()
    {
        extra_data.insert(
            "obsoletes".to_string(),
            serde_json::Value::Array(
                obsoletes
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    let vcs_url = infer_vcs_url(metadata, &source_urls).map(truncate_field);

    PackageData {
        datasource_id: Some(DatasourceId::RpmArchive),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        qualifiers,
        description,
        homepage_url,
        size: metadata.get_installed_size().ok(),
        parties,
        keywords,
        bug_tracking_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        dependencies,
        source_packages: source_rpm.into_iter().collect(),
        vcs_url,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
        purl: name.as_ref().and_then(|n| {
            build_rpm_purl(
                n,
                version.as_deref(),
                namespace.as_deref(),
                architecture.as_deref(),
                is_source,
            )
            .map(truncate_field)
        }),
        ..Default::default()
    }
}

pub(crate) fn normalize_rpm_declared_license(statement: &str) -> Option<NormalizedDeclaredLicense> {
    let trimmed = statement.trim();
    if trimmed.is_empty() {
        return None;
    }

    let rewritten = canonicalize_rpm_license_statement(trimmed);
    if let Some(normalized) = normalize_spdx_expression(&rewritten) {
        return Some(normalized);
    }

    let is_simple_key = !trimmed.contains(' ')
        && !trimmed.contains(',')
        && !trimmed.contains('(')
        && !trimmed.contains(')');
    if is_simple_key {
        return normalize_declared_license_key(trimmed);
    }

    None
}

fn canonicalize_rpm_license_statement(statement: &str) -> String {
    let mut rewritten = statement.trim().to_string();

    for (from, to) in [
        ("LGPLv2.1+", "LGPL-2.1-or-later"),
        ("LGPLv2.1", "LGPL-2.1-only"),
        ("LGPLv2+", "LGPL-2.0-or-later"),
        ("LGPLv2", "LGPL-2.0-only"),
        ("LGPLv3+", "LGPL-3.0-or-later"),
        ("LGPLv3", "LGPL-3.0-only"),
        ("GPLv2+", "GPL-2.0-or-later"),
        ("GPLv2", "GPL-2.0-only"),
        ("GPLv3+", "GPL-3.0-or-later"),
        ("GPLv3", "GPL-3.0-only"),
        ("GPLV2+", "GPL-2.0-or-later"),
        ("MPLv2.0", "MPL-2.0"),
        ("MPLv1.1", "MPL-1.1"),
        ("BSD with advertising", "BSD-4-Clause-UC"),
        ("Public Domain", "LicenseRef-provenant-public-domain"),
        ("public domain", "LicenseRef-provenant-public-domain"),
        ("OpenLDAP", "OLDAP-2.8"),
        ("OpenSSL", "OpenSSL"),
        ("Sleepycat", "Sleepycat"),
        ("zlib", "Zlib"),
        ("Boost", "BSL-1.0"),
        ("BSD", "BSD-3-Clause"),
    ] {
        rewritten = rewritten.replace(from, to);
    }

    rewritten = RE_RPM_LICENSE_WITH_EXCEPTIONS
        .replace_all(&rewritten, "")
        .into_owned();
    rewritten = RE_RPM_LICENSE_COMMA
        .replace_all(&rewritten, " AND ")
        .into_owned();
    rewritten = RE_RPM_LICENSE_AND
        .replace_all(&rewritten, " AND ")
        .into_owned();
    rewritten = RE_RPM_LICENSE_OR
        .replace_all(&rewritten, " OR ")
        .into_owned();

    rewritten.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_rpm_dependencies(
    metadata: &PackageMetadata,
    namespace: Option<&str>,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Ok(requires) = metadata.get_requires() {
        for rpm_dep in requires {
            if dependencies.len() >= MAX_ITERATION_COUNT {
                warn!(
                    "RPM dependency iteration capped at {} items",
                    MAX_ITERATION_COUNT
                );
                break;
            }
            let purl = build_rpm_purl(
                &rpm_dep.name,
                if rpm_dep.version.is_empty() {
                    None
                } else {
                    Some(&rpm_dep.version)
                },
                namespace,
                None,
                false,
            )
            .map(truncate_field);

            let extracted_requirement = if !rpm_dep.version.is_empty() {
                Some(truncate_field(format_rpm_requirement(&rpm_dep)))
            } else {
                None
            };

            dependencies.push(Dependency {
                purl,
                extracted_requirement,
                scope: Some("install".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
                is_pinned: Some(!rpm_dep.version.is_empty()),
            });
        }
    }

    dependencies
}

enum RpmRelationshipKind {
    Provides,
    Obsoletes,
}

fn extract_rpm_relationships(
    metadata: &PackageMetadata,
    kind: RpmRelationshipKind,
) -> Option<Vec<String>> {
    let relationships = match kind {
        RpmRelationshipKind::Provides => metadata.get_provides().ok()?,
        RpmRelationshipKind::Obsoletes => metadata.get_obsoletes().ok()?,
    };

    let mut count = 0usize;
    let values: Vec<String> = relationships
        .into_iter()
        .take(MAX_ITERATION_COUNT)
        .map(|dep| format_rpm_requirement(&dep))
        .filter(|value| !value.is_empty() && value != "(none)")
        .inspect(|_| count += 1)
        .collect();

    if count >= MAX_ITERATION_COUNT {
        warn!(
            "RPM relationship iteration capped at {} items",
            MAX_ITERATION_COUNT
        );
    }

    (!values.is_empty()).then_some(values)
}

fn format_rpm_requirement(dep: &rpm::Dependency) -> String {
    use rpm::DependencyFlags;

    if dep.version.is_empty() {
        return dep.name.clone();
    }

    let operator = if dep.flags.contains(DependencyFlags::EQUAL)
        && dep.flags.contains(DependencyFlags::LESS)
    {
        "<="
    } else if dep.flags.contains(DependencyFlags::EQUAL)
        && dep.flags.contains(DependencyFlags::GREATER)
    {
        ">="
    } else if dep.flags.contains(DependencyFlags::EQUAL) {
        "="
    } else if dep.flags.contains(DependencyFlags::LESS) {
        "<"
    } else if dep.flags.contains(DependencyFlags::GREATER) {
        ">"
    } else {
        ""
    };

    if operator.is_empty() {
        dep.name.clone()
    } else {
        format!("{} {} {}", dep.name, operator, dep.version)
    }
}

fn build_evr_version(metadata: &PackageMetadata) -> Option<String> {
    let version = metadata.get_version().ok()?;
    let release = metadata.get_release().ok();

    let mut evr = String::from(version);

    if let Some(r) = release {
        evr.push('-');
        evr.push_str(r);
    }

    Some(evr)
}

fn parse_packager(packager: &str) -> (Option<String>, Option<String>) {
    if let Some(email_start) = packager.find('<') {
        let name = packager[..email_start].trim();
        if let Some(email_end) = packager.find('>') {
            let email = &packager[email_start + 1..email_end];
            return (Some(name.to_string()), Some(email.to_string()));
        }
    }
    (Some(packager.to_string()), None)
}

fn build_rpm_purl(
    name: &str,
    version: Option<&str>,
    namespace: Option<&str>,
    architecture: Option<&str>,
    is_source: bool,
) -> Option<String> {
    use packageurl::PackageUrl;

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

    if is_source {
        purl.add_qualifier("source", "true").ok()?;
    }

    Some(purl.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn build_sparse_oversized_rpm(name: &str) -> PathBuf {
        let package = rpm::PackageBuilder::new(name, "1.0", "MIT", "x86_64", "Demo RPM package")
            .release("1")
            .build()
            .unwrap();

        let temp_file = NamedTempFile::new().unwrap();
        package.write_file(temp_file.path()).unwrap();
        let oversized_len = MAX_MANIFEST_SIZE + 1_048_576;
        fs::OpenOptions::new()
            .write(true)
            .open(temp_file.path())
            .unwrap()
            .set_len(oversized_len)
            .unwrap();

        temp_file.into_temp_path().keep().unwrap()
    }

    #[test]
    fn test_rpm_parser_is_match() {
        assert!(RpmParser::is_match(&PathBuf::from("package.rpm")));
        assert!(RpmParser::is_match(&PathBuf::from("package.srpm")));
        assert!(RpmParser::is_match(&PathBuf::from(
            "test-1.0-1.el7.x86_64.rpm"
        )));
        assert!(!RpmParser::is_match(&PathBuf::from("package.deb")));
        assert!(!RpmParser::is_match(&PathBuf::from("package.tar.gz")));
    }

    #[test]
    fn test_rpm_parser_matches_hash_named_source_rpm_by_magic() {
        let source_fixture = PathBuf::from("testdata/rpm/setup-2.5.49-b1.src.rpm");
        if !source_fixture.exists() {
            return;
        }

        let temp_file = NamedTempFile::new().unwrap();
        fs::copy(&source_fixture, temp_file.path()).unwrap();

        assert!(RpmParser::is_match(temp_file.path()));
    }

    #[test]
    fn test_rpm_parser_matches_pack_named_rpm_by_magic() {
        let source_fixture = PathBuf::from("testdata/rpm/setup-2.5.49-b1.src.rpm");
        if !source_fixture.exists() {
            return;
        }

        let temp_dir = tempfile::TempDir::new().unwrap();
        let pack_path = temp_dir.path().join("setup-2.5.49-b1.src.pack");
        fs::copy(&source_fixture, &pack_path).unwrap();

        assert!(RpmParser::is_match(&pack_path));
        assert!(path_looks_like_rpm_archive(&pack_path));
    }

    #[test]
    fn test_build_evr_version_simple() {
        let evr = "1.0-1";
        assert_eq!(evr, "1.0-1");
    }

    #[test]
    fn test_build_evr_version_with_epoch() {
        let evr = "2:1.0-1";
        assert!(evr.starts_with("2:"));
    }

    #[test]
    fn test_parse_packager() {
        let (name, email) = parse_packager("John Doe <john@example.com>");
        assert_eq!(name, Some("John Doe".to_string()));
        assert_eq!(email, Some("john@example.com".to_string()));

        let (name2, email2) = parse_packager("Plain Name");
        assert_eq!(name2, Some("Plain Name".to_string()));
        assert_eq!(email2, None);
    }

    #[test]
    fn test_build_rpm_purl() {
        let purl = build_rpm_purl(
            "bash",
            Some("4.4.19-1.el7"),
            Some("fedora"),
            Some("x86_64"),
            false,
        );
        assert!(purl.is_some());
        let purl_str = purl.unwrap();
        assert!(purl_str.contains("pkg:rpm/fedora/bash"));
        assert!(purl_str.contains("4.4.19-1.el7"));
        assert!(purl_str.contains("arch=x86_64"));
    }

    #[test]
    fn test_parse_real_rpm() {
        let test_file = PathBuf::from("testdata/rpm/Eterm-0.9.3-5mdv2007.0.rpm");
        if !test_file.exists() {
            eprintln!("Warning: Test file not found, skipping test");
            return;
        }

        let pkg = RpmParser::extract_first_package(&test_file);

        assert_eq!(pkg.package_type, Some(PackageType::Rpm));
        assert_eq!(pkg.name, Some("Eterm".to_string()));
        assert_eq!(pkg.version, Some("0.9.3-5mdv2007.0".to_string()));
    }

    #[test]
    fn test_parse_oversized_rpm_from_headers_only() {
        let test_file = build_sparse_oversized_rpm("oversized-demo");

        assert!(RpmParser::is_match(&test_file));

        let pkg = RpmParser::extract_first_package(&test_file);

        assert_eq!(pkg.datasource_id, Some(DatasourceId::RpmArchive));
        assert_eq!(pkg.package_type, Some(PackageType::Rpm));
        assert_eq!(pkg.name.as_deref(), Some("oversized-demo"));
        assert_eq!(pkg.version.as_deref(), Some("1.0-1"));

        fs::remove_file(test_file).unwrap();
    }

    #[test]
    fn test_build_rpm_purl_no_namespace() {
        let purl = build_rpm_purl("package", Some("1.0-1"), None, Some("x86_64"), false);
        assert!(purl.is_some());
        let purl_str = purl.unwrap();
        assert!(purl_str.starts_with("pkg:rpm/package@"));
        assert!(purl_str.contains("arch=x86_64"));
    }

    #[test]
    fn test_rpm_dependency_extraction() {
        use rpm::{Dependency as RpmDependency, DependencyFlags};

        let rpm_dep = RpmDependency {
            name: "libc.so.6".to_string(),
            flags: DependencyFlags::GREATER | DependencyFlags::EQUAL,
            version: "2.2.5".to_string(),
        };

        let formatted = format_rpm_requirement(&rpm_dep);
        assert_eq!(formatted, "libc.so.6 >= 2.2.5");

        let rpm_dep_no_version = RpmDependency {
            name: "bash".to_string(),
            flags: DependencyFlags::ANY,
            version: String::new(),
        };

        let formatted_no_ver = format_rpm_requirement(&rpm_dep_no_version);
        assert_eq!(formatted_no_ver, "bash");
    }

    #[test]
    fn test_parse_packager_with_parentheses() {
        let (name, email) = parse_packager("John Doe (Company) <john@example.com>");
        assert_eq!(name, Some("John Doe (Company)".to_string()));
        assert_eq!(email, Some("john@example.com".to_string()));
    }

    #[test]
    fn test_parse_packager_email_only() {
        let (name, email) = parse_packager("<noreply@example.com>");
        assert!(name.is_none() || name == Some(String::new()));
        assert_eq!(email, Some("noreply@example.com".to_string()));
    }

    #[test]
    fn test_rpm_fping_package() {
        let test_file = PathBuf::from("testdata/rpm/fping-2.4b2-10.fc12.x86_64.rpm");
        if !test_file.exists() {
            return;
        }

        let pkg = RpmParser::extract_first_package(&test_file);
        assert_eq!(pkg.name, Some("fping".to_string()));
        assert_eq!(pkg.version, Some("2.4b2-10.fc12".to_string()));
    }

    #[test]
    fn test_rpm_archive_extracts_additional_metadata_fields() {
        let test_file = PathBuf::from("testdata/rpm/setup-2.5.49-b1.src.rpm");
        if !test_file.exists() {
            return;
        }

        let pkg = RpmParser::extract_first_package(&test_file);

        assert_eq!(pkg.name.as_deref(), Some("setup"));
        assert_eq!(
            pkg.qualifiers
                .as_ref()
                .and_then(|q| q.get("arch"))
                .map(String::as_str),
            Some("noarch")
        );
        assert!(!pkg.keywords.is_empty());
        assert!(pkg.size.is_some());
        assert!(
            pkg.parties
                .iter()
                .any(|party| party.role.as_deref() == Some("packager"))
        );
        assert!(
            pkg.qualifiers
                .as_ref()
                .is_some_and(|q| q.get("source") == Some(&"true".to_string()))
        );
    }

    #[test]
    fn test_source_rpm_sets_source_qualifier() {
        let test_file = PathBuf::from("testdata/rpm/setup-2.5.49-b1.src.rpm");
        if !test_file.exists() {
            return;
        }

        let pkg = RpmParser::extract_first_package(&test_file);

        assert!(
            pkg.qualifiers
                .as_ref()
                .is_some_and(|q| q.get("source") == Some(&"true".to_string()))
        );
        assert!(
            pkg.purl
                .as_ref()
                .is_some_and(|purl| purl.contains("source=true"))
        );
    }

    #[test]
    fn test_rpm_archive_extracts_vcs_and_source_metadata() {
        let package = rpm::PackageBuilder::new(
            "thunar-sendto-clamtk",
            "0.08",
            "GPL-2.0-or-later",
            "noarch",
            "Simple virus scanning extension for Thunar",
        )
        .release("2.fc40")
        .vendor("Fedora Project")
        .packager("Fedora Release Engineering <releng@fedoraproject.org>")
        .group("Applications/System")
        .vcs("git+https://src.fedoraproject.org/rpms/thunar-sendto-clamtk.git#5a3f8e92b45f46b464e6924c79d4bf3e11bb1f0e")
        .build()
        .unwrap();

        let temp_file = NamedTempFile::new().unwrap();
        package.write_file(temp_file.path()).unwrap();

        let pkg = RpmParser::extract_first_package(temp_file.path());

        assert_eq!(pkg.namespace.as_deref(), Some("fedora"));
        assert_eq!(
            pkg.vcs_url.as_deref(),
            Some(
                "git+https://src.fedoraproject.org/rpms/thunar-sendto-clamtk.git#5a3f8e92b45f46b464e6924c79d4bf3e11bb1f0e",
            )
        );
        assert!(
            pkg.extra_data
                .as_ref()
                .is_some_and(|extra| extra.contains_key("build_time"))
        );
        assert!(!pkg.keywords.is_empty());
    }

    #[test]
    fn test_rpm_archive_preserves_provides_and_obsoletes_relationships() {
        use rpm::{Dependency as RpmDependency, DependencyFlags};

        let package = rpm::PackageBuilder::new(
            "demo-rpm",
            "1.0.0",
            "MIT",
            "noarch",
            "RPM relationship metadata fixture",
        )
        .release("1")
        .provides(RpmDependency {
            name: "demo-rpm-virtual".to_string(),
            flags: DependencyFlags::GREATER | DependencyFlags::EQUAL,
            version: "1.0.0".to_string(),
        })
        .obsoletes(RpmDependency {
            name: "old-demo-rpm".to_string(),
            flags: DependencyFlags::LESS,
            version: "0.9.0".to_string(),
        })
        .build()
        .unwrap();

        let temp_file = NamedTempFile::new().unwrap();
        package.write_file(temp_file.path()).unwrap();

        let pkg = RpmParser::extract_first_package(temp_file.path());
        let extra = pkg.extra_data.as_ref().expect("extra_data should exist");

        let provides = extra
            .get("provides")
            .and_then(|value| value.as_array())
            .expect("provides should be present");
        assert!(
            provides
                .iter()
                .any(|value| value.as_str() == Some("demo-rpm-virtual >= 1.0.0"))
        );

        let obsoletes = extra
            .get("obsoletes")
            .and_then(|value| value.as_array())
            .expect("obsoletes should be present");
        assert!(
            obsoletes
                .iter()
                .any(|value| value.as_str() == Some("old-demo-rpm < 0.9.0"))
        );
    }

    #[test]
    fn test_rpm_archive_normalizes_declared_license_expression() {
        let package = rpm::PackageBuilder::new(
            "demo-license",
            "1.0.0",
            "LGPLv2",
            "noarch",
            "RPM declared license normalization fixture",
        )
        .release("1")
        .build()
        .unwrap();

        let temp_file = NamedTempFile::new().unwrap();
        package.write_file(temp_file.path()).unwrap();

        let pkg = RpmParser::extract_first_package(temp_file.path());

        assert_eq!(pkg.extracted_license_statement.as_deref(), Some("LGPLv2"));
        assert_eq!(
            pkg.declared_license_expression.as_deref(),
            Some("lgpl-2.0-only")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("LGPL-2.0-only")
        );
        assert_eq!(pkg.license_detections.len(), 1);
        assert_eq!(
            pkg.license_detections[0].license_expression_spdx,
            "LGPL-2.0-only"
        );
        assert_eq!(
            pkg.license_detections[0].matches[0].matched_text.as_deref(),
            Some("LGPLv2")
        );
    }

    #[test]
    fn test_rpm_archive_normalizes_public_domain_declared_license_expression() {
        let package = rpm::PackageBuilder::new(
            "demo-public-domain",
            "1.0.0",
            "public domain",
            "noarch",
            "RPM public domain normalization fixture",
        )
        .release("1")
        .build()
        .unwrap();

        let temp_file = NamedTempFile::new().unwrap();
        package.write_file(temp_file.path()).unwrap();

        let pkg = RpmParser::extract_first_package(temp_file.path());

        assert_eq!(
            pkg.extracted_license_statement.as_deref(),
            Some("public domain")
        );
        assert_eq!(
            pkg.declared_license_expression.as_deref(),
            Some("licenseref-provenant-public-domain")
        );
        assert_eq!(
            pkg.declared_license_expression_spdx.as_deref(),
            Some("LicenseRef-provenant-public-domain")
        );
        assert_eq!(pkg.license_detections.len(), 1);
    }

    #[test]
    fn test_normalize_rpm_declared_license_rewrites_compound_aliases() {
        let normalized = normalize_rpm_declared_license("BSD and GPLv2+")
            .expect("compound RPM license should normalize");

        assert_eq!(
            normalized.declared_license_expression_spdx,
            "BSD-3-Clause AND GPL-2.0-or-later"
        );
    }
}

crate::register_parser!(
    "RPM package archive",
    &["**/*.rpm", "**/*.srpm"],
    "rpm",
    "",
    Some("https://rpm.org/"),
);
