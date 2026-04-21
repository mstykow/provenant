// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::Path as StdPath;

use object::endian::LittleEndian as LE;
use object::pe;
use object::read::FileKind;
use packageurl::PackageUrl;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::register_parser;

use super::ParsePackagesResult;
use super::license_normalization::{
    detect_declared_license_from_text, normalize_spdx_declared_license,
};
use super::utils::{MAX_ITERATION_COUNT, truncate_field};

register_parser!(
    "Windows PE executable with VERSIONINFO package metadata",
    &["<windows executable and DLL files with VERSIONINFO resources>"],
    "winexe",
    "",
    Some("https://learn.microsoft.com/en-us/windows/win32/menurc/versioninfo-resource"),
);

const VS_FIXEDFILEINFO_SIGNATURE: u32 = 0xFEEF04BD;
const MAX_SIBLING_LICENSE_BYTES: u64 = 256 * 1024;
const WINDOWS_VERSION_FALLBACK_KEYS: &[&str] = &[
    "ProductName",
    "FileDescription",
    "CompanyName",
    "LegalCopyright",
    "ProductVersion",
    "FileVersion",
    "OriginalFilename",
    "InternalName",
    "URL",
    "WWW",
    "License",
];

#[derive(Debug, Clone)]
struct FixedVersionInfo {
    product_version: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ParsedVersionInfo {
    string_tables: Vec<HashMap<String, String>>,
    fixed: Option<FixedVersionInfo>,
}

type VersionStrings<'a> = &'a HashMap<String, String>;
type PreferredVersionStringSources<'a> = (Option<VersionStrings<'a>>, Option<VersionStrings<'a>>);

#[derive(Debug, Clone)]
struct VersionBlock<'a> {
    key: String,
    value_type: u16,
    value: &'a [u8],
    children: &'a [u8],
}

pub(crate) fn try_parse_windows_executable_bytes(
    path: &Path,
    bytes: &[u8],
) -> Option<ParsePackagesResult> {
    let packages = parse_windows_executable_bytes(path, bytes);

    (!packages.is_empty()).then_some(ParsePackagesResult {
        packages,
        scan_diagnostics: Vec::new(),
        scan_errors: Vec::new(),
    })
}

pub(crate) fn extract_windows_executable_metadata_text(bytes: &[u8]) -> Option<String> {
    let parsed = match FileKind::parse(bytes) {
        Ok(FileKind::Pe32) => parse_pe_version_info::<pe::ImageNtHeaders32>(bytes),
        Ok(FileKind::Pe64) => parse_pe_version_info::<pe::ImageNtHeaders64>(bytes),
        _ => return None,
    }?;
    let fallback_strings = extract_utf16_version_string_fallback(bytes);

    let mut lines = Vec::new();
    for string_table in &parsed.string_tables {
        for key in [
            "ProductName",
            "FileDescription",
            "CompanyName",
            "LegalCopyright",
            "License",
            "LegalTrademarks",
            "LegalTrademarks1",
            "LegalTrademarks2",
            "LegalTrademarks3",
            "Comments",
            "URL",
            "WWW",
        ] {
            if let Some(value) = string_table.get(key).map(|value| value.trim())
                && !value.is_empty()
            {
                let line = format!("{key}: {value}");
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
    }

    if !fallback_strings.is_empty() {
        for key in [
            "ProductName",
            "FileDescription",
            "CompanyName",
            "LegalCopyright",
            "License",
            "LegalTrademarks",
            "LegalTrademarks1",
            "LegalTrademarks2",
            "LegalTrademarks3",
            "Comments",
            "URL",
            "WWW",
        ] {
            if let Some(value) = fallback_strings.get(key).map(|value| value.trim())
                && !value.is_empty()
            {
                let line = format!("{key}: {value}");
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
    }

    if let Some(version) = parsed.fixed.and_then(|fixed| fixed.product_version) {
        let line = format!("ProductVersion: {version}");
        if !lines.contains(&line) {
            lines.push(line);
        }
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn parse_windows_executable_bytes(path: &Path, bytes: &[u8]) -> Vec<PackageData> {
    let fallback_strings = extract_utf16_version_string_fallback(bytes);
    let parsed = match FileKind::parse(bytes) {
        Ok(FileKind::Pe32) => parse_pe_version_info::<pe::ImageNtHeaders32>(bytes),
        Ok(FileKind::Pe64) => parse_pe_version_info::<pe::ImageNtHeaders64>(bytes),
        _ => return Vec::new(),
    };

    match parsed {
        Some(version_info) => build_windows_executable_package(
            path,
            version_info,
            (!fallback_strings.is_empty()).then_some(&fallback_strings),
        )
        .into_iter()
        .collect(),
        None if !fallback_strings.is_empty() => build_windows_executable_package(
            path,
            ParsedVersionInfo::default(),
            Some(&fallback_strings),
        )
        .into_iter()
        .collect(),
        None => build_windows_executable_fallback(path)
            .into_iter()
            .collect(),
    }
}

fn parse_pe_version_info<Pe: object::read::pe::ImageNtHeaders>(
    bytes: &[u8],
) -> Option<ParsedVersionInfo> {
    let pe = object::read::pe::PeFile::<Pe>::parse(bytes).ok()?;
    let resource_directory = pe
        .data_directories()
        .resource_directory(bytes, &pe.section_table())
        .ok()??;
    let root = resource_directory.root().ok()?;
    let version_entry = root.entries.iter().find(|entry| {
        matches!(entry.name_or_id(), object::read::pe::ResourceNameOrId::Id(id) if id == pe::RT_VERSION)
    })?;
    let name_table = version_entry.data(resource_directory).ok()?.table()?;

    let parsed_infos = name_table
        .entries
        .iter()
        .filter_map(|name_entry| name_entry.data(resource_directory).ok()?.table())
        .flat_map(|language_table| {
            language_table.entries.iter().filter_map(|language_entry| {
                let data_entry = language_entry.data(resource_directory).ok()?.data()?;
                let version_bytes = resource_data_bytes(&pe, bytes, data_entry)?;
                parse_version_info_bytes(version_bytes)
            })
        });

    merge_parsed_version_infos(parsed_infos)
}

fn resource_data_bytes<'a, Pe: object::read::pe::ImageNtHeaders>(
    pe_file: &object::read::pe::PeFile<'a, Pe>,
    bytes: &'a [u8],
    data_entry: &pe::ImageResourceDataEntry,
) -> Option<&'a [u8]> {
    let data_rva = data_entry.offset_to_data.get(LE);
    let size = data_entry.size.get(LE) as usize;
    pe_file
        .section_table()
        .pe_data_at(bytes, data_rva)
        .and_then(|data| data.get(..size))
}

fn parse_version_info_bytes(bytes: &[u8]) -> Option<ParsedVersionInfo> {
    let root = parse_version_block(bytes)?;
    if root.key != "VS_VERSION_INFO" {
        return None;
    }

    let mut parsed = ParsedVersionInfo {
        fixed: parse_fixed_version_info(root.value),
        ..ParsedVersionInfo::default()
    };

    for child in iter_version_blocks(root.children) {
        let Some(child) = child else {
            continue;
        };
        if child.key != "StringFileInfo" {
            continue;
        }

        for string_table in iter_version_blocks(child.children) {
            let Some(string_table) = string_table else {
                continue;
            };
            let mut strings = HashMap::new();
            for string_entry in iter_version_blocks(string_table.children) {
                let Some(string_entry) = string_entry else {
                    continue;
                };
                let Some(value) = decode_version_value(&string_entry) else {
                    continue;
                };
                if !string_entry.key.is_empty() && !value.is_empty() {
                    strings.insert(string_entry.key.clone(), value);
                }
            }
            if !strings.is_empty() {
                parsed.string_tables.push(strings);
            }
        }
    }

    Some(parsed)
}

fn parse_version_block(bytes: &[u8]) -> Option<VersionBlock<'_>> {
    if bytes.len() < 6 {
        return None;
    }

    let total_len = read_u16_le(bytes, 0)? as usize;
    let value_len = read_u16_le(bytes, 2)? as usize;
    let value_type = read_u16_le(bytes, 4)?;
    if total_len == 0 || total_len > bytes.len() {
        return None;
    }

    let block_bytes = &bytes[..total_len];
    let mut cursor = 6;
    let key_end = find_utf16_nul(block_bytes.get(cursor..)?)?;
    let key_bytes = &block_bytes[cursor..cursor + key_end];
    let key = decode_utf16_bytes(key_bytes)?;
    cursor += key_end + 2;
    cursor = align_to_4(cursor);
    if cursor > block_bytes.len() {
        return None;
    }

    let value_byte_len = if value_type == 1 {
        value_len.checked_mul(2)?
    } else {
        value_len
    };
    let value_end = cursor.checked_add(value_byte_len)?;
    if value_end > block_bytes.len() {
        return None;
    }
    let value = &block_bytes[cursor..value_end];
    let children_start = align_to_4(value_end);
    let children = block_bytes.get(children_start..).unwrap_or(&[]);

    Some(VersionBlock {
        key,
        value_type,
        value,
        children,
    })
}

fn merge_parsed_version_infos(
    parsed_infos: impl IntoIterator<Item = ParsedVersionInfo>,
) -> Option<ParsedVersionInfo> {
    let mut merged = ParsedVersionInfo::default();
    let mut saw_any = false;

    for parsed in parsed_infos {
        saw_any = true;
        if merged.fixed.is_none() {
            merged.fixed = parsed.fixed;
        }
        merged.string_tables.extend(parsed.string_tables);
    }

    saw_any.then_some(merged)
}

fn extract_utf16_version_string_fallback(bytes: &[u8]) -> HashMap<String, String> {
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let text = String::from_utf16_lossy(&units);

    WINDOWS_VERSION_FALLBACK_KEYS
        .iter()
        .filter_map(|key| {
            find_utf16_version_value(&text, key).map(|value| ((*key).to_string(), value))
        })
        .collect()
}

fn find_utf16_version_value(text: &str, key: &str) -> Option<String> {
    let needle = format!("{key}\0");
    let start = text.find(&needle)? + needle.len();
    let rest = text.get(start..)?.trim_start_matches('\0');
    let value_end = rest.find('\0')?;
    let value = rest[..value_end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn iter_version_blocks(mut bytes: &[u8]) -> impl Iterator<Item = Option<VersionBlock<'_>>> + '_ {
    let mut count = 0usize;
    std::iter::from_fn(move || {
        if bytes.is_empty() {
            return None;
        }

        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!(
                "iter_version_blocks exceeded MAX_ITERATION_COUNT ({MAX_ITERATION_COUNT}), stopping iteration"
            );
            return None;
        }

        let block_len = read_u16_le(bytes, 0)? as usize;
        if block_len == 0 {
            return None;
        }
        let current = bytes.get(..block_len)?;
        let next_offset = align_to_4(block_len);
        bytes = bytes.get(next_offset..).unwrap_or(&[]);
        Some(parse_version_block(current))
    })
}

fn parse_fixed_version_info(value: &[u8]) -> Option<FixedVersionInfo> {
    if value.len() < 13 * 4 {
        return None;
    }
    let signature = read_u32_le(value, 0)?;
    if signature != VS_FIXEDFILEINFO_SIGNATURE {
        return None;
    }

    let product_version_ms = read_u32_le(value, 16)?;
    let product_version_ls = read_u32_le(value, 20)?;
    let product_version = version_components_to_string(product_version_ms, product_version_ls);

    Some(FixedVersionInfo { product_version })
}

fn version_components_to_string(ms: u32, ls: u32) -> Option<String> {
    let major = (ms >> 16) & 0xFFFF;
    let minor = ms & 0xFFFF;
    let patch = (ls >> 16) & 0xFFFF;
    let build = ls & 0xFFFF;
    let version = format!("{major}.{minor}.{patch}.{build}");
    (version != "0.0.0.0").then_some(version)
}

fn decode_version_value(block: &VersionBlock<'_>) -> Option<String> {
    if block.value_type != 1 {
        return None;
    }
    let mut value = decode_utf16_bytes(block.value)?;
    while value.ends_with(' ') {
        value.pop();
    }
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let bytes = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let bytes = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn find_utf16_nul(bytes: &[u8]) -> Option<usize> {
    let mut offset = 0;
    while offset + 1 < bytes.len() {
        if bytes[offset] == 0 && bytes[offset + 1] == 0 {
            return Some(offset);
        }
        offset += 2;
    }
    None
}

fn decode_utf16_bytes(bytes: &[u8]) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    match String::from_utf16(&units) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("decode_utf16_bytes: invalid UTF-16 sequence, using lossy conversion: {e}");
            Some(
                char::decode_utf16(units)
                    .map(|r| r.unwrap_or('\u{FFFD}'))
                    .collect(),
            )
        }
    }
}

fn align_to_4(offset: usize) -> usize {
    (offset + 3) & !3
}

fn build_windows_executable_package(
    path: &Path,
    version_info: ParsedVersionInfo,
    fallback_strings: Option<&HashMap<String, String>>,
) -> Option<PackageData> {
    let (strings, fallback) =
        preferred_windows_version_strings_with_fallback(path, &version_info, fallback_strings);

    let name = preferred_string_with_fallback(
        strings,
        fallback,
        &["ProductName", "OriginalFilename", "InternalName"],
    )
    .map(|v| truncate_field(trim_windows_executable_name(v)));
    let version = preferred_string_with_fallback(
        strings,
        fallback,
        &[
            "Full Version",
            "ProductVersion",
            "FileVersion",
            "Assembly Version",
        ],
    )
    .or_else(|| {
        version_info
            .fixed
            .as_ref()
            .and_then(|fixed| fixed.product_version.clone())
    })
    .map(truncate_field);

    let name = name
        .filter(|value| !value.is_empty())
        .or_else(|| fallback_windows_executable_name(path).map(truncate_field))?;

    let mut package = PackageData {
        package_type: Some(PackageType::Winexe),
        datasource_id: Some(DatasourceId::WindowsExecutable),
        name: Some(name.clone()),
        version: version.clone(),
        description: combined_string_with_fallback(
            strings,
            fallback,
            &["FileDescription", "Comments"],
        )
        .map(truncate_field),
        homepage_url: preferred_string_with_fallback(strings, fallback, &["URL", "WWW"])
            .map(truncate_field),
        purl: create_winexe_purl(&name, version.as_deref()).map(truncate_field),
        ..Default::default()
    };

    if let Some(company_name) =
        preferred_string_with_fallback(strings, fallback, &["CompanyName", "Company"])
            .map(truncate_field)
    {
        package.parties.push(Party {
            r#type: Some("organization".to_string()),
            role: Some("author".to_string()),
            name: Some(company_name),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    let license_statement =
        windows_license_statement_with_fallback(strings, fallback).map(truncate_field);
    let normalizable_license = windows_normalizable_license_text_with_fallback(strings, fallback);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(normalizable_license.as_deref());
    package.extracted_license_statement = license_statement;
    package.declared_license_expression = declared_license_expression;
    package.declared_license_expression_spdx = declared_license_expression_spdx;
    package.license_detections = license_detections;
    package.copyright =
        preferred_string_with_fallback(strings, fallback, &["LegalCopyright"]).map(truncate_field);
    package.holder = package
        .copyright
        .as_deref()
        .map(extract_windows_holder)
        .filter(|value| !value.is_empty())
        .map(truncate_field);

    merge_sibling_license_text(path, &mut package);

    Some(package)
}

fn build_windows_executable_fallback(path: &Path) -> Option<PackageData> {
    let name = fallback_windows_executable_name(path)?;

    Some(PackageData {
        package_type: Some(PackageType::Winexe),
        datasource_id: Some(DatasourceId::WindowsExecutable),
        name: Some(name.clone()),
        purl: create_winexe_purl(&name, None).map(truncate_field),
        ..Default::default()
    })
}

fn fallback_windows_executable_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| trim_windows_executable_name(name.to_string()))
        .filter(|name| !name.is_empty())
}

fn preferred_windows_version_strings<'a>(
    path: &Path,
    version_info: &'a ParsedVersionInfo,
) -> Option<VersionStrings<'a>> {
    let normalized_filename =
        fallback_windows_executable_name(path).map(|name| normalize_windows_name_for_match(&name));

    version_info.string_tables.iter().max_by_key(|strings| {
        score_windows_version_strings(strings, normalized_filename.as_deref())
    })
}

fn preferred_windows_version_strings_with_fallback<'a>(
    path: &Path,
    version_info: &'a ParsedVersionInfo,
    fallback_strings: Option<VersionStrings<'a>>,
) -> PreferredVersionStringSources<'a> {
    let primary = preferred_windows_version_strings(path, version_info);
    let normalized_filename =
        fallback_windows_executable_name(path).map(|name| normalize_windows_name_for_match(&name));

    let primary_score = primary
        .map(|strings| score_windows_version_strings(strings, normalized_filename.as_deref()))
        .unwrap_or(0);
    let fallback_score = fallback_strings
        .map(|strings| score_windows_version_strings(strings, normalized_filename.as_deref()))
        .unwrap_or(0);

    if fallback_score > primary_score {
        (fallback_strings, primary)
    } else {
        (
            primary.or(fallback_strings),
            fallback_strings.filter(|_| primary.is_some()),
        )
    }
}

fn score_windows_version_strings(
    strings: &HashMap<String, String>,
    normalized_filename: Option<&str>,
) -> usize {
    let candidate = preferred_string_from_table(
        strings,
        &["ProductName", "OriginalFilename", "InternalName"],
    );
    let product_name = preferred_string_from_table(strings, &["ProductName"]);
    let original_filename = preferred_string_from_table(strings, &["OriginalFilename"]);
    let internal_name = preferred_string_from_table(strings, &["InternalName"]);

    let mut score = 0;
    if product_name.is_some() {
        score += 40;
    }
    if original_filename.is_some() {
        score += 20;
    }
    if internal_name.is_some() {
        score += 10;
    }
    if preferred_string_from_table(strings, &["FileDescription"]).is_some() {
        score += 5;
    }
    if preferred_string_from_table(strings, &["CompanyName", "Company"]).is_some() {
        score += 5;
    }
    if preferred_string_from_table(
        strings,
        &[
            "Full Version",
            "ProductVersion",
            "FileVersion",
            "Assembly Version",
        ],
    )
    .is_some()
    {
        score += 5;
    }

    if let Some(candidate) = candidate.as_deref() {
        score += score_windows_name_match(candidate, normalized_filename);
    }

    score
}

fn score_windows_name_match(candidate: &str, normalized_filename: Option<&str>) -> usize {
    let Some(normalized_filename) = normalized_filename else {
        return 0;
    };
    let normalized_candidate = normalize_windows_name_for_match(candidate);
    if normalized_candidate.is_empty() {
        return 0;
    }

    if normalized_filename == normalized_candidate {
        200
    } else if normalized_filename.starts_with(&normalized_candidate)
        || normalized_filename.contains(&normalized_candidate)
    {
        120
    } else {
        0
    }
}

fn normalize_windows_name_for_match(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn preferred_string_from_table(strings: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        strings
            .get(*key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn preferred_string(strings: Option<&HashMap<String, String>>, keys: &[&str]) -> Option<String> {
    preferred_string_from_table(strings?, keys)
}

fn preferred_string_with_fallback(
    strings: Option<&HashMap<String, String>>,
    fallback: Option<&HashMap<String, String>>,
    keys: &[&str],
) -> Option<String> {
    preferred_string(strings, keys).or_else(|| preferred_string(fallback, keys))
}

fn combined_string_with_fallback(
    strings: Option<&HashMap<String, String>>,
    fallback: Option<&HashMap<String, String>>,
    keys: &[&str],
) -> Option<String> {
    let mut parts = Vec::new();
    for table in [strings, fallback].into_iter().flatten() {
        for key in keys {
            if let Some(value) = table.get(*key).map(|value| value.trim())
                && !value.is_empty()
                && !parts.iter().any(|existing| existing == value)
            {
                parts.push(value.to_string());
            }
        }
    }

    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn windows_license_statement_with_fallback(
    strings: Option<&HashMap<String, String>>,
    fallback: Option<&HashMap<String, String>>,
) -> Option<String> {
    let keys = [
        "License",
        "LegalCopyright",
        "LegalTrademarks",
        "LegalTrademarks1",
        "LegalTrademarks2",
        "LegalTrademarks3",
    ];
    let mut parts = Vec::new();

    for table in [strings, fallback].into_iter().flatten() {
        for key in keys {
            if let Some(value) = table.get(key).map(|value| value.trim())
                && !value.is_empty()
                && !parts
                    .iter()
                    .any(|existing| existing == &format!("{key}: {value}"))
            {
                parts.push(format!("{key}: {value}"));
            }
        }
    }

    (!parts.is_empty()).then(|| parts.join("\n") + "\n")
}

fn windows_normalizable_license_text(strings: Option<&HashMap<String, String>>) -> Option<String> {
    preferred_string(strings, &["License", "LegalTrademarks", "LegalTrademarks1"])
}

fn windows_normalizable_license_text_with_fallback(
    strings: Option<&HashMap<String, String>>,
    fallback: Option<&HashMap<String, String>>,
) -> Option<String> {
    windows_normalizable_license_text(strings)
        .or_else(|| windows_normalizable_license_text(fallback))
}

fn merge_sibling_license_text(path: &Path, package: &mut PackageData) {
    if package.declared_license_expression_spdx.is_some() {
        return;
    }

    let Some((license_path, license_text)) = read_sibling_license_text(path) else {
        return;
    };

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        detect_declared_license_from_text(&license_text, &license_path);

    if declared_license_expression_spdx.is_some() {
        package.declared_license_expression = declared_license_expression;
        package.declared_license_expression_spdx = declared_license_expression_spdx;
        package.license_detections = license_detections;
    }
}

fn read_sibling_license_text(path: &Path) -> Option<(String, String)> {
    let parent = path.parent()?;
    for name in [
        "LICENSE",
        "LICENSE.txt",
        "LICENSE.md",
        "COPYING",
        "COPYING.txt",
        "COPYING.md",
    ] {
        let sibling = parent.join(name);
        let Ok(metadata) = sibling.metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_SIBLING_LICENSE_BYTES {
            continue;
        }
        let Ok(content) = fs::read_to_string(&sibling) else {
            continue;
        };
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Some((name.to_string(), trimmed.to_string()));
        }
    }
    None
}

fn trim_windows_executable_name(name: String) -> String {
    let trimmed = name.trim();
    let lowercase = trimmed.to_ascii_lowercase();
    for suffix in [
        ".dll", ".exe", ".mui", ".mun", ".sys", ".com", ".pyd", ".winmd", ".tlb", ".ocx",
    ] {
        if lowercase.ends_with(suffix) {
            let stem = StdPath::new(trimmed)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(trimmed);
            return stem.trim().to_string();
        }
    }

    trimmed.to_string()
}

fn extract_windows_holder(copyright: &str) -> String {
    let trimmed = copyright.trim().trim_start_matches('©').trim();
    let trimmed = trimmed
        .strip_prefix("Copyright")
        .or_else(|| trimmed.strip_prefix("copyright"))
        .unwrap_or(trimmed)
        .trim_start();
    let trimmed = trimmed
        .strip_prefix("(c)")
        .or_else(|| trimmed.strip_prefix("(C)"))
        .unwrap_or(trimmed)
        .trim_start();
    let start = trimmed
        .char_indices()
        .find(|(_, ch)| ch.is_alphabetic())
        .map(|(index, _)| index)
        .unwrap_or(0);
    let holder = trimmed[start..].trim();
    holder
        .split_once('<')
        .map(|(name, _)| name.trim_end().to_string())
        .unwrap_or_else(|| holder.to_string())
}

fn create_winexe_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("winexe", name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use object::pe;
    use object::read::FileKind;

    use super::{
        FixedVersionInfo, ParsedVersionInfo, build_windows_executable_package, decode_utf16_bytes,
        extract_utf16_version_string_fallback, extract_windows_executable_metadata_text,
        merge_parsed_version_infos, parse_fixed_version_info, parse_version_info_bytes,
        preferred_windows_version_strings_with_fallback, read_u32_le,
    };

    fn is_supported_pe_format(bytes: &[u8]) -> bool {
        matches!(FileKind::parse(bytes), Ok(FileKind::Pe32 | FileKind::Pe64))
    }

    #[test]
    fn parses_version_info_from_real_pe_fixture() {
        let bytes = std::fs::read("testdata/compiled-binary-golden/win_pe/libiconv2.dll")
            .expect("read PE fixture");
        assert!(is_supported_pe_format(&bytes));
        let parsed = super::parse_pe_version_info::<pe::ImageNtHeaders64>(&bytes)
            .or_else(|| super::parse_pe_version_info::<pe::ImageNtHeaders32>(&bytes))
            .expect("version info");

        let strings = parsed.string_tables.first().expect("string table");
        assert_eq!(
            strings.get("ProductName").map(String::as_str),
            Some("LibIconv")
        );
        assert_eq!(
            strings.get("CompanyName").map(String::as_str),
            Some("GNU <www.gnu.org>")
        );
        assert_eq!(
            parsed
                .fixed
                .and_then(|fixed| fixed.product_version)
                .as_deref(),
            Some("1.9.2.1519")
        );
    }

    #[test]
    fn decodes_utf16le_bytes() {
        let bytes = b"L\0i\0b\0\0\0";
        assert_eq!(decode_utf16_bytes(bytes).as_deref(), Some("Lib\0"));
    }

    #[test]
    fn parses_fixed_file_info_product_version() {
        let mut bytes = vec![0u8; 16 * 4];
        bytes[0..4].copy_from_slice(&0xFEEF04BDu32.to_le_bytes());
        bytes[16..20].copy_from_slice(&0x00010009u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&0x000205efu32.to_le_bytes());
        let fixed = parse_fixed_version_info(&bytes).expect("fixed info");
        assert_eq!(fixed.product_version.as_deref(), Some("1.9.2.1519"));
        assert_eq!(read_u32_le(&bytes, 16), Some(0x00010009));
    }

    #[test]
    fn returns_none_for_truncated_version_info_blob() {
        assert!(parse_version_info_bytes(&[1, 2, 3]).is_none());
    }

    #[test]
    fn extracts_windows_executable_metadata_text_from_real_fixture() {
        let bytes = std::fs::read("testdata/compiled-binary-golden/win_pe/libiconv2.dll")
            .expect("read PE fixture");

        let text = extract_windows_executable_metadata_text(&bytes).expect("metadata text");

        assert!(text.contains("ProductName: LibIconv"), "{text}");
        assert!(
            text.contains("License: This program is free software"),
            "{text}"
        );
    }

    #[test]
    fn prefers_version_table_matching_executable_name() {
        let mut bootstrapper_strings = HashMap::new();
        bootstrapper_strings.insert("InternalName".to_string(), "burn".to_string());
        bootstrapper_strings.insert("ProductVersion".to_string(), "3.10.1.0".to_string());

        let mut app_strings = HashMap::new();
        app_strings.insert("ProductName".to_string(), "GlazeWM".to_string());
        app_strings.insert("ProductVersion".to_string(), "3.10.1".to_string());
        app_strings.insert("FileDescription".to_string(), "GlazeWM".to_string());
        app_strings.insert(
            "CompanyName".to_string(),
            "Glzr Software Pte. Ltd.".to_string(),
        );

        let package = build_windows_executable_package(
            Path::new("glazewm-v3.10.1.exe"),
            ParsedVersionInfo {
                string_tables: vec![bootstrapper_strings, app_strings],
                fixed: Some(FixedVersionInfo {
                    product_version: Some("3.10.1.0".to_string()),
                }),
            },
            None,
        )
        .expect("package");

        assert_eq!(package.name.as_deref(), Some("GlazeWM"));
        assert_eq!(package.version.as_deref(), Some("3.10.1"));
        assert_eq!(package.purl.as_deref(), Some("pkg:winexe/GlazeWM@3.10.1"));
    }

    #[test]
    fn merges_multiple_version_resources_before_selection() {
        let mut bootstrapper_strings = HashMap::new();
        bootstrapper_strings.insert("InternalName".to_string(), "burn".to_string());

        let mut app_strings = HashMap::new();
        app_strings.insert("ProductName".to_string(), "GlazeWM".to_string());

        let merged = merge_parsed_version_infos([
            ParsedVersionInfo {
                string_tables: vec![bootstrapper_strings],
                fixed: Some(FixedVersionInfo {
                    product_version: Some("3.10.1.0".to_string()),
                }),
            },
            ParsedVersionInfo {
                string_tables: vec![app_strings],
                fixed: None,
            },
        ])
        .expect("merged version info");

        let package =
            build_windows_executable_package(Path::new("glazewm-v3.10.1.exe"), merged, None)
                .expect("package");

        assert_eq!(package.name.as_deref(), Some("GlazeWM"));
        assert_eq!(package.version.as_deref(), Some("3.10.1.0"));
    }

    #[test]
    fn extracts_utf16_version_fallback_strings() {
        let blob = concat!(
            "ProductName\0\0GlazeWM\0",
            "ProductVersion\03.10.1\0",
            "CompanyName\0\0Glzr Software Pte. Ltd.\0"
        );
        let bytes = blob
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();

        let strings = extract_utf16_version_string_fallback(&bytes);

        assert_eq!(
            strings.get("ProductName").map(String::as_str),
            Some("GlazeWM")
        );
        assert_eq!(
            strings.get("ProductVersion").map(String::as_str),
            Some("3.10.1")
        );
        assert_eq!(
            strings.get("CompanyName").map(String::as_str),
            Some("Glzr Software Pte. Ltd.")
        );
    }

    #[test]
    fn fallback_does_not_override_selected_parsed_table() {
        let parsed = ParsedVersionInfo {
            string_tables: vec![HashMap::from([
                ("ProductName".to_string(), "LibIconv".to_string()),
                (
                    "LegalTrademarks".to_string(),
                    "GNU®, LibIconv®, libiconv2®".to_string(),
                ),
            ])],
            fixed: None,
        };
        let fallback_bytes = concat!(
            "ProductName\0\0GlazeWM\0",
            "CompanyName\0\0Glzr Software Pte. Ltd.\0"
        )
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
        let fallback = extract_utf16_version_string_fallback(&fallback_bytes);

        let (primary, secondary) = preferred_windows_version_strings_with_fallback(
            Path::new("libiconv2.dll"),
            &parsed,
            Some(&fallback),
        );

        assert_eq!(
            primary
                .and_then(|s| s.get("ProductName"))
                .map(String::as_str),
            Some("LibIconv")
        );
        assert_eq!(
            secondary
                .and_then(|s| s.get("CompanyName"))
                .map(String::as_str),
            Some("Glzr Software Pte. Ltd.")
        );
    }

    #[test]
    fn fallback_can_win_without_mutating_parsed_tables() {
        let parsed = ParsedVersionInfo {
            string_tables: vec![HashMap::from([
                ("InternalName".to_string(), "burn".to_string()),
                ("ProductVersion".to_string(), "3.10.1.0".to_string()),
            ])],
            fixed: None,
        };
        let fallback_bytes = concat!(
            "ProductName\0\0GlazeWM\0",
            "ProductVersion\03.10.1\0",
            "CompanyName\0\0Glzr Software Pte. Ltd.\0"
        )
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
        let fallback = extract_utf16_version_string_fallback(&fallback_bytes);

        let package = build_windows_executable_package(
            Path::new("glazewm-v3.10.1.exe"),
            parsed,
            Some(&fallback),
        )
        .expect("package");

        assert_eq!(package.name.as_deref(), Some("GlazeWM"));
        assert_eq!(package.version.as_deref(), Some("3.10.1"));
    }

    #[test]
    fn extracts_windows_holder_without_copyright_prefix() {
        assert_eq!(
            super::extract_windows_holder(
                "Copyright (c) Glzr Software Pte. Ltd.. All rights reserved."
            ),
            "Glzr Software Pte. Ltd.. All rights reserved."
        );
    }

    #[test]
    fn reads_sibling_license_text_when_version_info_has_no_declared_license() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let exe_path = temp_dir.path().join("glazewm.exe");
        std::fs::write(&exe_path, b"MZ").expect("write exe placeholder");
        std::fs::write(temp_dir.path().join("LICENSE.md"), "GPL-3.0-only\n")
            .expect("write license text");

        let package = build_windows_executable_package(
            &exe_path,
            ParsedVersionInfo {
                string_tables: vec![HashMap::from([
                    ("ProductName".to_string(), "GlazeWM".to_string()),
                    (
                        "CompanyName".to_string(),
                        "Glzr Software Pte. Ltd.".to_string(),
                    ),
                ])],
                fixed: None,
            },
            None,
        )
        .expect("package");

        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("GPL-3.0-only")
        );
        assert!(!package.license_detections.is_empty());
        assert_eq!(package.license_detections.len(), 1);
    }
}
