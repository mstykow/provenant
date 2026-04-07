use std::collections::HashMap;
use std::path::Path;
use std::path::Path as StdPath;

use object::endian::LittleEndian as LE;
use object::pe;
use object::read::FileKind;
use object::read::pe::{ResourceDirectory, ResourceDirectoryTable};
use packageurl::PackageUrl;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use crate::register_parser;

use super::ParsePackagesResult;
use super::license_normalization::normalize_spdx_declared_license;

register_parser!(
    "Windows PE executable with VERSIONINFO package metadata",
    &["<windows executable and DLL files with VERSIONINFO resources>"],
    "winexe",
    "",
    Some("https://learn.microsoft.com/en-us/windows/win32/menurc/versioninfo-resource"),
);

const VS_FIXEDFILEINFO_SIGNATURE: u32 = 0xFEEF04BD;

#[derive(Debug, Clone)]
struct FixedVersionInfo {
    product_version: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ParsedVersionInfo {
    string_tables: Vec<HashMap<String, String>>,
    fixed: Option<FixedVersionInfo>,
}

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
        scan_errors: Vec::new(),
    })
}

pub(crate) fn extract_windows_executable_metadata_text(bytes: &[u8]) -> Option<String> {
    let parsed = match FileKind::parse(bytes) {
        Ok(FileKind::Pe32) => parse_pe_version_info::<pe::ImageNtHeaders32>(bytes),
        Ok(FileKind::Pe64) => parse_pe_version_info::<pe::ImageNtHeaders64>(bytes),
        _ => return None,
    }?;

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

    if let Some(version) = parsed.fixed.and_then(|fixed| fixed.product_version) {
        let line = format!("ProductVersion: {version}");
        if !lines.contains(&line) {
            lines.push(line);
        }
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn parse_windows_executable_bytes(path: &Path, bytes: &[u8]) -> Vec<PackageData> {
    let parsed = match FileKind::parse(bytes) {
        Ok(FileKind::Pe32) => parse_pe_version_info::<pe::ImageNtHeaders32>(bytes),
        Ok(FileKind::Pe64) => parse_pe_version_info::<pe::ImageNtHeaders64>(bytes),
        _ => return Vec::new(),
    };

    match parsed {
        Some(version_info) => build_windows_executable_package(path, version_info)
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
    let language_table = first_subtable(resource_directory, &name_table)?;
    let data_entry = first_data_entry(resource_directory, &language_table)?;
    let version_bytes = resource_data_bytes(&pe, bytes, data_entry)?;
    parse_version_info_bytes(version_bytes)
}

fn first_subtable<'a>(
    resource_directory: ResourceDirectory<'a>,
    table: &ResourceDirectoryTable<'a>,
) -> Option<ResourceDirectoryTable<'a>> {
    table
        .entries
        .iter()
        .find_map(|entry| entry.data(resource_directory).ok()?.table())
}

fn first_data_entry<'a>(
    resource_directory: ResourceDirectory<'a>,
    table: &ResourceDirectoryTable<'a>,
) -> Option<&'a pe::ImageResourceDataEntry> {
    table
        .entries
        .iter()
        .find_map(|entry| entry.data(resource_directory).ok()?.data())
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

fn iter_version_blocks(mut bytes: &[u8]) -> impl Iterator<Item = Option<VersionBlock<'_>>> + '_ {
    std::iter::from_fn(move || {
        if bytes.is_empty() {
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
    String::from_utf16(&units).ok()
}

fn align_to_4(offset: usize) -> usize {
    (offset + 3) & !3
}

fn build_windows_executable_package(
    path: &Path,
    version_info: ParsedVersionInfo,
) -> Option<PackageData> {
    let strings = preferred_windows_version_strings(&version_info);

    let name = preferred_string(
        strings,
        &["ProductName", "OriginalFilename", "InternalName"],
    )
    .map(trim_windows_executable_name);
    let version = preferred_string(
        strings,
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
    });

    let name = name
        .filter(|value| !value.is_empty())
        .or_else(|| fallback_windows_executable_name(path))?;

    let mut package = PackageData {
        package_type: Some(PackageType::Winexe),
        datasource_id: Some(DatasourceId::WindowsExecutable),
        name: Some(name.clone()),
        version: version.clone(),
        description: combined_string(strings, &["FileDescription", "Comments"]),
        homepage_url: preferred_string(strings, &["URL", "WWW"]),
        purl: create_winexe_purl(&name, version.as_deref()),
        ..Default::default()
    };

    if let Some(company_name) = preferred_string(strings, &["CompanyName", "Company"]) {
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

    let license_statement = windows_license_statement(strings);
    let normalizable_license = windows_normalizable_license_text(strings);
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(normalizable_license.as_deref());
    package.extracted_license_statement = license_statement;
    package.declared_license_expression = declared_license_expression;
    package.declared_license_expression_spdx = declared_license_expression_spdx;
    package.license_detections = license_detections;
    package.copyright = preferred_string(strings, &["LegalCopyright"]);
    package.holder = package
        .copyright
        .as_deref()
        .map(extract_windows_holder)
        .filter(|value| !value.is_empty());

    Some(package)
}

fn build_windows_executable_fallback(path: &Path) -> Option<PackageData> {
    let name = fallback_windows_executable_name(path)?;

    Some(PackageData {
        package_type: Some(PackageType::Winexe),
        datasource_id: Some(DatasourceId::WindowsExecutable),
        name: Some(name.clone()),
        purl: create_winexe_purl(&name, None),
        ..Default::default()
    })
}

fn fallback_windows_executable_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| trim_windows_executable_name(name.to_string()))
        .filter(|name| !name.is_empty())
}

fn preferred_windows_version_strings(
    version_info: &ParsedVersionInfo,
) -> Option<&HashMap<String, String>> {
    version_info.string_tables.first().or_else(|| {
        version_info
            .string_tables
            .iter()
            .max_by_key(|values| values.len())
    })
}

fn preferred_string(strings: Option<&HashMap<String, String>>, keys: &[&str]) -> Option<String> {
    let strings = strings?;
    keys.iter().find_map(|key| {
        strings
            .get(*key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn combined_string(strings: Option<&HashMap<String, String>>, keys: &[&str]) -> Option<String> {
    let strings = strings?;
    let mut parts = Vec::new();
    for key in keys {
        if let Some(value) = strings.get(*key).map(|value| value.trim())
            && !value.is_empty()
            && !parts.iter().any(|existing| existing == value)
        {
            parts.push(value.to_string());
        }
    }

    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn windows_license_statement(strings: Option<&HashMap<String, String>>) -> Option<String> {
    let strings = strings?;
    let keys = [
        "License",
        "LegalCopyright",
        "LegalTrademarks",
        "LegalTrademarks1",
        "LegalTrademarks2",
        "LegalTrademarks3",
    ];
    let mut parts = Vec::new();

    for key in keys {
        if let Some(value) = strings.get(key).map(|value| value.trim())
            && !value.is_empty()
            && !parts.iter().any(|existing| existing == value)
        {
            parts.push(format!("{key}: {value}"));
        }
    }

    (!parts.is_empty()).then(|| parts.join("\n") + "\n")
}

fn windows_normalizable_license_text(strings: Option<&HashMap<String, String>>) -> Option<String> {
    preferred_string(strings, &["License", "LegalTrademarks", "LegalTrademarks1"])
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
    use object::pe;
    use object::read::FileKind;

    use super::{
        decode_utf16_bytes, extract_windows_executable_metadata_text, parse_fixed_version_info,
        parse_version_info_bytes, read_u32_le,
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
}
