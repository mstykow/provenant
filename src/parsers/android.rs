use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::Path;

use prost::Message;
use quick_xml::Reader;
use quick_xml::events::Event;
use rusty_axml::{find_nodes_by_type, get_requested_permissions, parse_from_reader};
use zip::ZipArchive;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, MAX_MANIFEST_SIZE, truncate_field};
use crate::utils::magic;

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Android;
const MAX_ARCHIVE_SIZE: u64 = 100 * 1024 * 1024;
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: f64 = 100.0;
const ANDROID_XML_NAMESPACE: &str = "http://schemas.android.com/apk/res/android";

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

pub struct AndroidSoongMetadataParser;
pub struct AndroidManifestParser;
pub struct AndroidApkParser;
pub struct AndroidAabParser;

impl PackageParser for AndroidSoongMetadataParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("METADATA")
            && !path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".dist-info"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match crate::parsers::utils::read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!(
                    "Failed to read Android Soong METADATA {:?}: {}",
                    path, error
                );
                return vec![default_package_data(DatasourceId::AndroidSoongMetadata)];
            }
        };

        vec![parse_soong_metadata(&content)]
    }
}

impl PackageParser for AndroidManifestParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("AndroidManifest.xml")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let bytes = match read_file_bytes(path, None) {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!("Failed to read AndroidManifest.xml {:?}: {}", path, error);
                return vec![default_package_data(DatasourceId::AndroidManifestXml)];
            }
        };

        vec![parse_manifest_bytes(
            &bytes,
            DatasourceId::AndroidManifestXml,
            "AndroidManifest.xml",
        )]
    }
}

impl PackageParser for AndroidApkParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|ext| ext.to_str()) == Some("apk") && magic::is_zip(path)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let package_data = match read_best_zip_entry(path, |entry_name| {
            if entry_name == "AndroidManifest.xml" {
                Some(0)
            } else {
                None
            }
        }) {
            Ok(Some((_, bytes))) => parse_binary_manifest_bytes(&bytes, DatasourceId::AndroidApk)
                .unwrap_or_else(|error| {
                    warn!("Failed to parse APK manifest {:?}: {}", path, error);
                    default_package_data(DatasourceId::AndroidApk)
                }),
            Ok(None) => {
                warn!("No AndroidManifest.xml found in APK {:?}", path);
                default_package_data(DatasourceId::AndroidApk)
            }
            Err(error) => {
                warn!("Failed to read APK archive {:?}: {}", path, error);
                default_package_data(DatasourceId::AndroidApk)
            }
        };

        vec![package_data]
    }
}

impl PackageParser for AndroidAabParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|ext| ext.to_str()) == Some("aab") && magic::is_zip(path)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let package_data = match read_best_zip_entry(path, |entry_name| {
            if entry_name == "base/manifest/AndroidManifest.xml" {
                Some(0)
            } else if entry_name.ends_with("/manifest/AndroidManifest.xml") {
                Some(1)
            } else {
                None
            }
        }) {
            Ok(Some((entry_name, bytes))) => {
                parse_proto_manifest_bytes(&bytes).unwrap_or_else(|error| {
                    warn!(
                        "Failed to parse AAB manifest {:?} ({}): {}",
                        path, entry_name, error
                    );
                    default_package_data(DatasourceId::AndroidAab)
                })
            }
            Ok(None) => {
                warn!("No proto AndroidManifest.xml found in AAB {:?}", path);
                default_package_data(DatasourceId::AndroidAab)
            }
            Err(error) => {
                warn!("Failed to read AAB archive {:?}: {}", path, error);
                default_package_data(DatasourceId::AndroidAab)
            }
        };

        vec![package_data]
    }
}

fn read_file_bytes(path: &Path, max_size: Option<u64>) -> Result<Vec<u8>, String> {
    let limit = max_size.unwrap_or(MAX_MANIFEST_SIZE);
    let metadata =
        fs::metadata(path).map_err(|error| format!("Cannot stat file {:?}: {}", path, error))?;

    if metadata.len() > limit {
        return Err(format!(
            "File {:?} is {} bytes, exceeding the {} byte limit",
            path,
            metadata.len(),
            limit
        ));
    }

    let mut file =
        File::open(path).map_err(|error| format!("Failed to open {:?}: {}", path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read {:?}: {}", path, error))?;
    Ok(bytes)
}

fn parse_soong_metadata(content: &str) -> PackageData {
    let parsed = parse_textproto_map(content).unwrap_or_else(|error| {
        warn!("Failed to parse Android Soong METADATA: {}", error);
        ProtoMap::default()
    });

    let mut package = default_package_data(DatasourceId::AndroidSoongMetadata);
    package.name = parsed.get_first_string("name").map(truncate_field);
    package.description = parsed.get_first_string("description").map(truncate_field);

    if let Some(third_party) = parsed.get_first_map("third_party") {
        package.version = third_party.get_first_string("version").map(truncate_field);

        let url_entries = third_party
            .get_all_maps("url")
            .into_iter()
            .map(|entry| {
                let type_ = entry.get_first_string("type").map(truncate_field);
                let value = entry.get_first_string("value").map(truncate_field);
                (type_, value)
            })
            .collect::<Vec<_>>();

        let homepage_url = third_party.get_first_string("homepage").or_else(|| {
            url_entries
                .iter()
                .find(|(type_, _)| {
                    type_
                        .as_deref()
                        .is_some_and(|type_| type_.eq_ignore_ascii_case("homepage"))
                })
                .and_then(|(_, value)| value.clone())
        });
        package.homepage_url = homepage_url.map(truncate_field);

        let license_types = third_party
            .get_all_strings("license_type")
            .into_iter()
            .map(truncate_field)
            .collect::<Vec<_>>();
        if !license_types.is_empty() {
            package.extracted_license_statement = Some(license_types.join(", "));
        }

        let identifiers = third_party
            .get_all_maps("identifier")
            .into_iter()
            .map(|identifier| {
                let type_ = identifier.get_first_string("type").map(truncate_field);
                let value = identifier.get_first_string("value").map(truncate_field);
                let mut object = serde_json::Map::new();
                if let Some(type_) = type_ {
                    object.insert("type".to_string(), type_.into());
                }
                if let Some(value) = &value {
                    object.insert("value".to_string(), value.clone().into());
                }

                if package.vcs_url.is_none()
                    && let (Some(type_), Some(value)) = (
                        identifier.get_first_string("type"),
                        identifier.get_first_string("value"),
                    )
                {
                    let lower_type = type_.to_ascii_lowercase();
                    if lower_type.contains("git") {
                        package.vcs_url = Some(truncate_field(value));
                    } else if lower_type.contains("archive")
                        || lower_type.contains("tar")
                        || lower_type.contains("zip")
                    {
                        package.download_url = Some(truncate_field(value));
                    }
                }

                serde_json::Value::Object(object)
            })
            .collect::<Vec<_>>();

        for (type_, value) in &url_entries {
            let Some(value) = value else {
                continue;
            };

            match type_.as_deref().map(str::to_ascii_lowercase).as_deref() {
                Some("git") if package.vcs_url.is_none() => {
                    package.vcs_url = Some(value.clone());
                }
                Some("archive") if package.download_url.is_none() => {
                    package.download_url = Some(value.clone());
                }
                Some("homepage") if package.homepage_url.is_none() => {
                    package.homepage_url = Some(value.clone());
                }
                _ => {}
            }
        }

        let mut extra_data = HashMap::new();
        if !identifiers.is_empty() {
            extra_data.insert("identifiers".to_string(), identifiers.into());
        }
        if !url_entries.is_empty() {
            extra_data.insert(
                "urls".to_string(),
                url_entries
                    .iter()
                    .map(|(type_, value)| {
                        let mut object = serde_json::Map::new();
                        if let Some(type_) = type_ {
                            object.insert("type".to_string(), type_.clone().into());
                        }
                        if let Some(value) = value {
                            object.insert("value".to_string(), value.clone().into());
                        }
                        serde_json::Value::Object(object)
                    })
                    .collect::<Vec<_>>()
                    .into(),
            );
        }

        if let Some(last_upgrade_date) = third_party.get_first_map("last_upgrade_date") {
            let year = last_upgrade_date.get_first_string("year");
            let month = last_upgrade_date.get_first_string("month");
            let day = last_upgrade_date.get_first_string("day");
            if let (Some(year), Some(month), Some(day)) = (year, month, day) {
                let formatted = format!(
                    "{:04}-{:02}-{:02}",
                    year.parse::<u32>().unwrap_or_default(),
                    month.parse::<u32>().unwrap_or_default(),
                    day.parse::<u32>().unwrap_or_default()
                );
                extra_data.insert(
                    "last_upgrade_date".to_string(),
                    truncate_field(formatted).into(),
                );
            }
        }

        if let Some(upstream_url) = third_party.get_first_string("url") {
            extra_data.insert(
                "upstream_url".to_string(),
                truncate_field(upstream_url).into(),
            );
        }

        if !extra_data.is_empty() {
            package.extra_data = Some(extra_data);
        }
    }

    package
}

fn parse_manifest_bytes(bytes: &[u8], datasource_id: DatasourceId, context: &str) -> PackageData {
    if looks_like_text_xml(bytes) {
        match parse_text_manifest_bytes(bytes, datasource_id) {
            Ok(package) => return package,
            Err(error) => warn!("Failed to parse {} as text XML: {}", context, error),
        }
    }

    parse_binary_manifest_bytes(bytes, datasource_id).unwrap_or_else(|error| {
        warn!(
            "Failed to parse {} as binary Android XML: {}",
            context, error
        );
        default_package_data(datasource_id)
    })
}

fn looks_like_text_xml(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'<')
}

fn parse_text_manifest_bytes(
    bytes: &[u8],
    datasource_id: DatasourceId,
) -> Result<PackageData, String> {
    let content = String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("Invalid UTF-8 in AndroidManifest.xml: {}", error))?;

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut manifest_attributes = HashMap::new();
    let mut uses_sdk_attributes = HashMap::new();
    let mut application_attributes = HashMap::new();
    let mut requested_permissions = Vec::new();
    let mut uses_libraries = Vec::new();
    let mut iteration_count = 0usize;

    loop {
        iteration_count += 1;
        if iteration_count > MAX_ITERATION_COUNT {
            return Err(format!(
                "Exceeded MAX_ITERATION_COUNT ({}) while parsing AndroidManifest.xml",
                MAX_ITERATION_COUNT
            ));
        }

        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                let attributes = xml_attributes_to_map(&reader, &event)?;
                match name.as_str() {
                    "manifest" if manifest_attributes.is_empty() => {
                        manifest_attributes = attributes
                    }
                    "uses-sdk" => uses_sdk_attributes = attributes,
                    "application" if application_attributes.is_empty() => {
                        application_attributes = attributes;
                    }
                    "uses-permission" | "uses-permission-sdk-23" => {
                        if let Some(permission) = attributes.get("android:name") {
                            requested_permissions.push(permission.clone());
                        }
                    }
                    "uses-library" => {
                        if let Some(library_name) = attributes.get("android:name") {
                            uses_libraries.push(library_name.clone());
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(format!(
                    "XML parse error at position {}: {}",
                    reader.buffer_position(),
                    error
                ));
            }
            _ => {}
        }

        buf.clear();
    }

    Ok(build_manifest_package_data(
        datasource_id,
        &manifest_attributes,
        &uses_sdk_attributes,
        &application_attributes,
        requested_permissions,
        uses_libraries,
    ))
}

fn xml_attributes_to_map(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<HashMap<String, String>, String> {
    let mut attributes = HashMap::new();

    for attribute in event.attributes().flatten().take(MAX_ITERATION_COUNT) {
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = attribute
            .decode_and_unescape_value(reader.decoder())
            .map_err(|error| format!("Failed to decode XML attribute {}: {}", key, error))?
            .into_owned();
        attributes.insert(key, truncate_field(value));
    }

    Ok(attributes)
}

fn parse_binary_manifest_bytes(
    bytes: &[u8],
    datasource_id: DatasourceId,
) -> Result<PackageData, String> {
    let axml = std::panic::catch_unwind(|| parse_from_reader(Cursor::new(bytes.to_vec())))
        .map_err(|_| "rusty-axml panicked while parsing binary Android XML".to_string())?
        .map_err(|error| format!("rusty-axml parse failure: {}", error))?;

    let manifest_attributes =
        normalize_binary_attributes(axml.root().borrow().attributes().clone());
    let uses_sdk_attributes = find_nodes_by_type(&axml, "uses-sdk")
        .into_iter()
        .next()
        .map(|node| normalize_binary_attributes(node.borrow().attributes().clone()))
        .unwrap_or_default();
    let application_attributes = find_nodes_by_type(&axml, "application")
        .into_iter()
        .next()
        .map(|node| normalize_binary_attributes(node.borrow().attributes().clone()))
        .unwrap_or_default();

    let requested_permissions = get_requested_permissions(&axml)
        .into_iter()
        .map(truncate_field)
        .collect::<Vec<_>>();
    let uses_libraries = find_nodes_by_type(&axml, "uses-library")
        .into_iter()
        .filter_map(|node| node.borrow().get_attr("android:name").map(str::to_string))
        .map(truncate_field)
        .collect::<Vec<_>>();

    Ok(build_manifest_package_data(
        datasource_id,
        &manifest_attributes,
        &uses_sdk_attributes,
        &application_attributes,
        requested_permissions,
        uses_libraries,
    ))
}

fn build_manifest_package_data(
    datasource_id: DatasourceId,
    manifest_attributes: &HashMap<String, String>,
    uses_sdk_attributes: &HashMap<String, String>,
    application_attributes: &HashMap<String, String>,
    requested_permissions: Vec<String>,
    uses_libraries: Vec<String>,
) -> PackageData {
    let mut package = default_package_data(datasource_id);
    package.name = manifest_attributes.get("package").cloned();
    package.version = manifest_attributes
        .get("android:versionName")
        .cloned()
        .or_else(|| manifest_attributes.get("android:versionCode").cloned());

    package.description = application_attributes
        .get("android:label")
        .filter(|label| {
            !label.starts_with('@') && !label.chars().all(|character| character.is_ascii_digit())
        })
        .cloned();

    let mut extra_data = HashMap::new();
    insert_extra(
        &mut extra_data,
        "version_code",
        manifest_attributes.get("android:versionCode"),
    );
    insert_extra(
        &mut extra_data,
        "compile_sdk_version",
        manifest_attributes.get("android:compileSdkVersion"),
    );
    insert_extra(
        &mut extra_data,
        "compile_sdk_version_codename",
        manifest_attributes.get("android:compileSdkVersionCodename"),
    );
    insert_extra(
        &mut extra_data,
        "platform_build_version_code",
        manifest_attributes.get("platformBuildVersionCode"),
    );
    insert_extra(
        &mut extra_data,
        "platform_build_version_name",
        manifest_attributes.get("platformBuildVersionName"),
    );
    insert_extra(
        &mut extra_data,
        "min_sdk_version",
        uses_sdk_attributes.get("android:minSdkVersion"),
    );
    insert_extra(
        &mut extra_data,
        "target_sdk_version",
        uses_sdk_attributes.get("android:targetSdkVersion"),
    );
    insert_extra(
        &mut extra_data,
        "max_sdk_version",
        uses_sdk_attributes.get("android:maxSdkVersion"),
    );

    if !requested_permissions.is_empty() {
        extra_data.insert(
            "requested_permissions".to_string(),
            requested_permissions
                .into_iter()
                .map(serde_json::Value::from)
                .collect::<Vec<_>>()
                .into(),
        );
    }
    if !uses_libraries.is_empty() {
        extra_data.insert(
            "uses_libraries".to_string(),
            uses_libraries
                .into_iter()
                .map(serde_json::Value::from)
                .collect::<Vec<_>>()
                .into(),
        );
    }

    if !extra_data.is_empty() {
        package.extra_data = Some(extra_data);
    }

    package
}

fn normalize_binary_attributes(attributes: HashMap<String, String>) -> HashMap<String, String> {
    attributes
        .into_iter()
        .map(|(key, value)| (key, normalize_binary_attribute_value(&value)))
        .collect()
}

fn normalize_binary_attribute_value(value: &str) -> String {
    let hex_value = value
        .strip_prefix("(type 0x10) 0x")
        .or_else(|| value.strip_prefix("0x"));

    if let Some(hex_value) = hex_value
        && let Ok(parsed) = u64::from_str_radix(hex_value, 16)
    {
        return parsed.to_string();
    }

    value.to_string()
}

fn insert_extra(
    extra_data: &mut HashMap<String, serde_json::Value>,
    key: &str,
    value: Option<&String>,
) {
    if let Some(value) = value {
        extra_data.insert(key.to_string(), truncate_field(value.clone()).into());
    }
}

fn read_best_zip_entry<F>(
    path: &Path,
    mut rank_entry: F,
) -> Result<Option<(String, Vec<u8>)>, String>
where
    F: FnMut(&str) -> Option<u8>,
{
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Failed to stat archive {:?}: {}", path, error))?;
    if metadata.len() > MAX_ARCHIVE_SIZE {
        return Err(format!(
            "Archive {:?} is {} bytes, exceeding the {} byte limit",
            path,
            metadata.len(),
            MAX_ARCHIVE_SIZE
        ));
    }

    let file = File::open(path)
        .map_err(|error| format!("Failed to open archive {:?}: {}", path, error))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("Failed to parse ZIP archive {:?}: {}", path, error))?;

    let mut total_uncompressed = 0u64;
    let mut best: Option<(u8, String, Vec<u8>)> = None;
    let entry_count = archive.len().min(MAX_ITERATION_COUNT);

    if archive.len() > MAX_ITERATION_COUNT {
        warn!(
            "Archive {:?} has more than MAX_ITERATION_COUNT ({}) entries; truncating scan",
            path, MAX_ITERATION_COUNT
        );
    }

    for index in 0..entry_count {
        let mut entry = archive.by_index(index).map_err(|error| {
            format!(
                "Failed to read ZIP entry {} in {:?}: {}",
                index, path, error
            )
        })?;

        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED_SIZE {
            return Err(format!(
                "Archive {:?} exceeds total uncompressed size limit of {} bytes",
                path, MAX_TOTAL_UNCOMPRESSED_SIZE
            ));
        }

        let entry_name = entry.name().replace('\\', "/");
        if entry_name.starts_with('/') || entry_name.split('/').any(|segment| segment == "..") {
            return Err(format!(
                "Archive entry {} contains a disallowed path",
                entry_name
            ));
        }
        let Some(rank) = rank_entry(&entry_name) else {
            continue;
        };

        if entry.size() > MAX_FILE_SIZE {
            return Err(format!(
                "Archive entry {} is {} bytes, exceeding the {} byte limit",
                entry_name,
                entry.size(),
                MAX_FILE_SIZE
            ));
        }

        let compressed_size = entry.compressed_size();
        if compressed_size > 0 {
            let ratio = entry.size() as f64 / compressed_size as f64;
            if ratio > MAX_COMPRESSION_RATIO {
                return Err(format!(
                    "Archive entry {} has suspicious compression ratio {:.2}:1",
                    entry_name, ratio
                ));
            }
        }

        let should_replace = match &best {
            Some((best_rank, _, _)) => rank < *best_rank,
            None => true,
        };

        if should_replace {
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes).map_err(|error| {
                format!("Failed to read archive entry {}: {}", entry_name, error)
            })?;
            best = Some((rank, entry_name, bytes));
        }
    }

    Ok(best.map(|(_, entry_name, bytes)| (entry_name, bytes)))
}

fn parse_proto_manifest_bytes(bytes: &[u8]) -> Result<PackageData, String> {
    let node =
        ProtoXmlNode::decode(bytes).map_err(|error| format!("prost decode failure: {}", error))?;
    let root_element = node
        .element()
        .ok_or_else(|| "Proto manifest root is not an element".to_string())?;
    if root_element.name != "manifest" {
        return Err(format!(
            "Unexpected proto XML root element: {}",
            root_element.name
        ));
    }

    let manifest_attributes = proto_attributes_to_map(&root_element.attribute);
    let uses_sdk_attributes = root_element
        .child_elements_named("uses-sdk")
        .next()
        .map(|element| proto_attributes_to_map(&element.attribute))
        .unwrap_or_default();
    let application_attributes = root_element
        .child_elements_named("application")
        .next()
        .map(|element| proto_attributes_to_map(&element.attribute))
        .unwrap_or_default();
    let requested_permissions = root_element
        .child_elements_named_any(&["uses-permission", "uses-permission-sdk-23"])
        .filter_map(|element| proto_attributes_to_map(&element.attribute).remove("android:name"))
        .collect::<Vec<_>>();
    let uses_libraries = root_element
        .child_elements_named("uses-library")
        .filter_map(|element| proto_attributes_to_map(&element.attribute).remove("android:name"))
        .collect::<Vec<_>>();

    let mut package = build_manifest_package_data(
        DatasourceId::AndroidAab,
        &manifest_attributes,
        &uses_sdk_attributes,
        &application_attributes,
        requested_permissions,
        uses_libraries,
    );

    if let Some(extra_data) = package.extra_data.as_mut() {
        extra_data.insert("manifest_encoding".to_string(), "proto".into());
    } else {
        package.extra_data = Some(HashMap::from([(
            "manifest_encoding".to_string(),
            serde_json::Value::String("proto".to_string()),
        )]));
    }

    Ok(package)
}

fn proto_attributes_to_map(attributes: &[ProtoXmlAttribute]) -> HashMap<String, String> {
    attributes
        .iter()
        .filter_map(|attribute| {
            let key = proto_attribute_key(attribute)?;
            let value = proto_attribute_value(attribute)?;
            Some((key, truncate_field(value)))
        })
        .collect()
}

fn proto_attribute_key(attribute: &ProtoXmlAttribute) -> Option<String> {
    if attribute.name.is_empty() {
        return None;
    }

    if attribute.namespace_uri == ANDROID_XML_NAMESPACE {
        return Some(format!("android:{}", attribute.name));
    }

    Some(attribute.name.clone())
}

fn proto_attribute_value(attribute: &ProtoXmlAttribute) -> Option<String> {
    if !attribute.value.is_empty() {
        return Some(attribute.value.clone());
    }

    attribute
        .compiled_item
        .as_ref()
        .and_then(proto_item_to_string)
}

fn proto_item_to_string(item: &ProtoItem) -> Option<String> {
    match &item.value {
        Some(proto_item::Value::Str(value)) => Some(value.value.clone()),
        Some(proto_item::Value::RawStr(value)) => Some(value.value.clone()),
        Some(proto_item::Value::Prim(value)) => proto_primitive_to_string(value),
        _ => None,
    }
}

fn proto_primitive_to_string(primitive: &ProtoPrimitive) -> Option<String> {
    match &primitive.value {
        Some(proto_primitive::Value::IntDecimal(value)) => Some(value.to_string()),
        Some(proto_primitive::Value::IntHexadecimal(value)) => Some(format!("0x{value:x}")),
        Some(proto_primitive::Value::Boolean(value)) => Some(value.to_string()),
        Some(proto_primitive::Value::Float(value)) => Some(value.to_string()),
        Some(proto_primitive::Value::Dimension(value)) => Some(value.to_string()),
        Some(proto_primitive::Value::Fraction(value)) => Some(value.to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
struct ProtoMap {
    fields: HashMap<String, Vec<ProtoValue>>,
}

#[derive(Debug, Clone)]
enum ProtoValue {
    Scalar(String),
    Map(ProtoMap),
}

impl ProtoMap {
    fn get_first_string(&self, key: &str) -> Option<String> {
        self.fields.get(key).and_then(|values| {
            values.iter().find_map(|value| match value {
                ProtoValue::Scalar(value) => Some(value.clone()),
                ProtoValue::Map(_) => None,
            })
        })
    }

    fn get_all_strings(&self, key: &str) -> Vec<String> {
        self.fields
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|value| match value {
                ProtoValue::Scalar(value) => Some(value.clone()),
                ProtoValue::Map(_) => None,
            })
            .collect()
    }

    fn get_first_map(&self, key: &str) -> Option<ProtoMap> {
        self.fields.get(key).and_then(|values| {
            values.iter().find_map(|value| match value {
                ProtoValue::Map(value) => Some(value.clone()),
                ProtoValue::Scalar(_) => None,
            })
        })
    }

    fn get_all_maps(&self, key: &str) -> Vec<ProtoMap> {
        self.fields
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|value| match value {
                ProtoValue::Map(value) => Some(value.clone()),
                ProtoValue::Scalar(_) => None,
            })
            .collect()
    }
}

fn parse_textproto_map(content: &str) -> Result<ProtoMap, String> {
    let mut parser = TextProtoParser::new(content)?;
    parser.parse_map(false)
}

struct TextProtoParser {
    tokens: Vec<TextProtoToken>,
    position: usize,
}

#[derive(Debug, Clone)]
enum TextProtoToken {
    Identifier(String),
    String(String),
    Colon,
    LBrace,
    RBrace,
}

impl TextProtoParser {
    fn new(content: &str) -> Result<Self, String> {
        Ok(Self {
            tokens: tokenize_textproto(content)?,
            position: 0,
        })
    }

    fn parse_map(&mut self, stop_on_rbrace: bool) -> Result<ProtoMap, String> {
        let mut map = ProtoMap::default();

        while let Some(token) = self.peek() {
            match token {
                TextProtoToken::RBrace if stop_on_rbrace => {
                    self.position += 1;
                    break;
                }
                TextProtoToken::RBrace => return Err("Unexpected closing brace".to_string()),
                TextProtoToken::Identifier(_) => {
                    let key = self.expect_identifier()?;
                    match self.peek() {
                        Some(TextProtoToken::Colon) => {
                            self.position += 1;
                            let value = self.expect_scalar()?;
                            map.fields
                                .entry(key)
                                .or_default()
                                .push(ProtoValue::Scalar(truncate_field(value)));
                        }
                        Some(TextProtoToken::LBrace) => {
                            self.position += 1;
                            let value = self.parse_map(true)?;
                            map.fields
                                .entry(key)
                                .or_default()
                                .push(ProtoValue::Map(value));
                        }
                        Some(other) => {
                            return Err(format!("Unexpected token after key: {:?}", other));
                        }
                        None => return Err("Unexpected end of input after key".to_string()),
                    }
                }
                other => return Err(format!("Unexpected token in textproto: {:?}", other)),
            }
        }

        Ok(map)
    }

    fn expect_identifier(&mut self) -> Result<String, String> {
        match self.next() {
            Some(TextProtoToken::Identifier(value)) => Ok(value),
            other => Err(format!("Expected identifier, found {:?}", other)),
        }
    }

    fn expect_scalar(&mut self) -> Result<String, String> {
        match self.next() {
            Some(TextProtoToken::Identifier(value)) | Some(TextProtoToken::String(value)) => {
                Ok(value)
            }
            other => Err(format!("Expected scalar value, found {:?}", other)),
        }
    }

    fn peek(&self) -> Option<&TextProtoToken> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<TextProtoToken> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }
}

fn tokenize_textproto(content: &str) -> Result<Vec<TextProtoToken>, String> {
    let mut tokens = Vec::new();
    let chars = content.chars().collect::<Vec<_>>();
    let mut index = 0usize;

    while index < chars.len() {
        match chars[index] {
            '{' => {
                tokens.push(TextProtoToken::LBrace);
                index += 1;
            }
            '}' => {
                tokens.push(TextProtoToken::RBrace);
                index += 1;
            }
            ':' => {
                tokens.push(TextProtoToken::Colon);
                index += 1;
            }
            '"' => {
                index += 1;
                let mut value = String::new();
                while index < chars.len() {
                    match chars[index] {
                        '\\' if index + 1 < chars.len() => {
                            index += 1;
                            value.push(chars[index]);
                            index += 1;
                        }
                        '"' => {
                            index += 1;
                            break;
                        }
                        character => {
                            value.push(character);
                            index += 1;
                        }
                    }
                }
                tokens.push(TextProtoToken::String(value));
            }
            '#' => {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
            }
            '/' if index + 1 < chars.len() && chars[index + 1] == '/' => {
                index += 2;
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
            }
            character if character.is_ascii_whitespace() => index += 1,
            _ => {
                let start = index;
                while index < chars.len() {
                    let character = chars[index];
                    let starts_comment =
                        character == '/' && index + 1 < chars.len() && chars[index + 1] == '/';

                    if character.is_ascii_whitespace()
                        || matches!(character, '{' | '}' | ':' | '#')
                        || starts_comment
                    {
                        break;
                    }

                    index += 1;
                }

                let token = chars[start..index].iter().collect::<String>();
                if token.is_empty() {
                    return Err("Encountered empty textproto token".to_string());
                }
                tokens.push(TextProtoToken::Identifier(token));
            }
        }
    }

    Ok(tokens)
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoSourcePosition {
    #[prost(uint32, tag = "1")]
    pub line_number: u32,
    #[prost(uint32, tag = "2")]
    pub column_number: u32,
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoXmlNode {
    #[prost(oneof = "proto_xml_node::Node", tags = "1, 2")]
    pub node: Option<proto_xml_node::Node>,
    #[prost(message, optional, tag = "3")]
    pub source: Option<ProtoSourcePosition>,
}

impl ProtoXmlNode {
    fn element(&self) -> Option<&ProtoXmlElement> {
        match &self.node {
            Some(proto_xml_node::Node::Element(element)) => Some(element),
            _ => None,
        }
    }
}

pub(crate) mod proto_xml_node {
    use super::ProtoXmlElement;
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Node {
        #[prost(message, tag = "1")]
        Element(ProtoXmlElement),
        #[prost(string, tag = "2")]
        Text(String),
    }
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoXmlElement {
    #[prost(message, repeated, tag = "1")]
    pub namespace_declaration: Vec<ProtoXmlNamespace>,
    #[prost(string, tag = "2")]
    pub namespace_uri: String,
    #[prost(string, tag = "3")]
    pub name: String,
    #[prost(message, repeated, tag = "4")]
    pub attribute: Vec<ProtoXmlAttribute>,
    #[prost(message, repeated, tag = "5")]
    pub child: Vec<ProtoXmlNode>,
}

impl ProtoXmlElement {
    fn child_elements_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a ProtoXmlElement> {
        self.child
            .iter()
            .filter_map(ProtoXmlNode::element)
            .filter(move |element| element.name == name)
    }

    fn child_elements_named_any<'a>(
        &'a self,
        names: &'a [&'a str],
    ) -> impl Iterator<Item = &'a ProtoXmlElement> {
        self.child
            .iter()
            .filter_map(ProtoXmlNode::element)
            .filter(move |element| names.contains(&element.name.as_str()))
    }
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoXmlNamespace {
    #[prost(string, tag = "1")]
    pub prefix: String,
    #[prost(string, tag = "2")]
    pub uri: String,
    #[prost(message, optional, tag = "3")]
    pub source: Option<ProtoSourcePosition>,
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoXmlAttribute {
    #[prost(string, tag = "1")]
    pub namespace_uri: String,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, tag = "3")]
    pub value: String,
    #[prost(message, optional, tag = "4")]
    pub source: Option<ProtoSourcePosition>,
    #[prost(uint32, tag = "5")]
    pub resource_id: u32,
    #[prost(message, optional, tag = "6")]
    pub compiled_item: Option<ProtoItem>,
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoItem {
    #[prost(oneof = "proto_item::Value", tags = "2, 3, 7")]
    pub value: Option<proto_item::Value>,
    #[prost(uint32, tag = "8")]
    pub flag_status: u32,
    #[prost(bool, tag = "9")]
    pub flag_negated: bool,
    #[prost(string, tag = "10")]
    pub flag_name: String,
}

pub(crate) mod proto_item {
    use super::{ProtoPrimitive, ProtoRawStringValue, ProtoStringValue};
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Value {
        #[prost(message, tag = "2")]
        Str(ProtoStringValue),
        #[prost(message, tag = "3")]
        RawStr(ProtoRawStringValue),
        #[prost(message, tag = "7")]
        Prim(ProtoPrimitive),
    }
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoStringValue {
    #[prost(string, tag = "1")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoRawStringValue {
    #[prost(string, tag = "1")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct ProtoPrimitive {
    #[prost(oneof = "proto_primitive::Value", tags = "3, 6, 7, 8, 13, 14")]
    pub value: Option<proto_primitive::Value>,
}

pub(crate) mod proto_primitive {
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Value {
        #[prost(float, tag = "3")]
        Float(f32),
        #[prost(int32, tag = "6")]
        IntDecimal(i32),
        #[prost(uint32, tag = "7")]
        IntHexadecimal(u32),
        #[prost(bool, tag = "8")]
        Boolean(bool),
        #[prost(uint32, tag = "13")]
        Dimension(u32),
        #[prost(uint32, tag = "14")]
        Fraction(u32),
    }
}

crate::register_parser!(
    "Android Soong METADATA textproto",
    &["**/METADATA"],
    "android",
    "",
    Some(
        "https://android.googlesource.com/platform/build/soong/+/refs/heads/main/licenses/metadata/metadata_file.proto"
    ),
);

crate::register_parser!(
    "AndroidManifest.xml metadata (text XML or binary AXML)",
    &["**/AndroidManifest.xml"],
    "android",
    "XML",
    Some("https://developer.android.com/guide/topics/manifest/manifest-intro"),
);

crate::register_parser!(
    "Android APK archive manifest metadata",
    &["**/*.apk"],
    "android",
    "",
    Some("https://developer.android.com/build/build-for-release"),
);

crate::register_parser!(
    "Android App Bundle (.aab) proto manifest metadata",
    &["**/*.aab"],
    "android",
    "",
    Some("https://developer.android.com/guide/app-bundle"),
);
