// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for Microsoft Update Manifest (.mum) files.
//!
//! Extracts Windows Update package metadata from .mum XML manifest files.
//!
//! # Supported Formats
//! - `*.mum` - Microsoft Update Manifest XML files
//!
//! # Implementation Notes
//! - Format: XML with assembly and package metadata
//! - Spec: Windows Update manifests

use crate::models::{DatasourceId, PackageType};
use std::path::Path;

use crate::parser_warn as warn;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::models::PackageData;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::WindowsUpdate;

pub struct MicrosoftUpdateManifestParser;

impl PackageParser for MicrosoftUpdateManifestParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().is_some_and(|ext| ext == "mum")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .mum file {:?}: {}", path, e);
                return vec![PackageData {
                    package_type: Some(PACKAGE_TYPE),
                    datasource_id: Some(DatasourceId::MicrosoftUpdateManifestMum),
                    ..Default::default()
                }];
            }
        };

        vec![parse_mum_xml(&content)]
    }
}

pub(crate) fn parse_mum_xml(content: &str) -> PackageData {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut name = None;
    let mut version = None;
    let mut description = None;
    let mut copyright = None;
    let mut homepage_url = None;

    let mut buf = Vec::new();
    let mut iteration_count: usize = 0;

    loop {
        iteration_count += 1;
        if iteration_count > MAX_ITERATION_COUNT {
            warn!(
                "Exceeded MAX_ITERATION_COUNT ({}) parsing .mum XML, stopping",
                MAX_ITERATION_COUNT
            );
            break;
        }
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) if e.name().as_ref() == b"assemblyIdentity" => {
                for attr in e.attributes().filter_map(|a| a.ok()) {
                    match attr.key.as_ref() {
                        b"name" => {
                            let raw = attr.value.to_vec();
                            let has_invalid = String::from_utf8(raw.clone()).is_err();
                            let val = String::from_utf8_lossy(&raw).into_owned();
                            if has_invalid {
                                warn!("Invalid UTF-8 in 'name' attribute, using lossy conversion");
                            }
                            name = Some(truncate_field(val));
                        }
                        b"version" => {
                            let raw = attr.value.to_vec();
                            let has_invalid = String::from_utf8(raw.clone()).is_err();
                            let val = String::from_utf8_lossy(&raw).into_owned();
                            if has_invalid {
                                warn!(
                                    "Invalid UTF-8 in 'version' attribute, using lossy conversion"
                                );
                            }
                            version = Some(truncate_field(val));
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Start(e)) if e.name().as_ref() == b"assembly" => {
                for attr in e.attributes().filter_map(|a| a.ok()) {
                    match attr.key.as_ref() {
                        b"description" => {
                            let raw = attr.value.to_vec();
                            let has_invalid = String::from_utf8(raw.clone()).is_err();
                            let val = String::from_utf8_lossy(&raw).into_owned();
                            if has_invalid {
                                warn!(
                                    "Invalid UTF-8 in 'description' attribute, using lossy conversion"
                                );
                            }
                            description = Some(truncate_field(val));
                        }
                        b"copyright" => {
                            let raw = attr.value.to_vec();
                            let has_invalid = String::from_utf8(raw.clone()).is_err();
                            let val = String::from_utf8_lossy(&raw).into_owned();
                            if has_invalid {
                                warn!(
                                    "Invalid UTF-8 in 'copyright' attribute, using lossy conversion"
                                );
                            }
                            copyright = Some(truncate_field(val));
                        }
                        b"supportInformation" => {
                            let raw = attr.value.to_vec();
                            let has_invalid = String::from_utf8(raw.clone()).is_err();
                            let val = String::from_utf8_lossy(&raw).into_owned();
                            if has_invalid {
                                warn!(
                                    "Invalid UTF-8 in 'supportInformation' attribute, using lossy conversion"
                                );
                            }
                            homepage_url = Some(truncate_field(val));
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!(
                    "Error parsing XML at position {}: {}",
                    reader.buffer_position(),
                    e
                );
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        name,
        version,
        description,
        homepage_url,
        copyright,
        datasource_id: Some(DatasourceId::MicrosoftUpdateManifestMum),
        ..Default::default()
    }
}

crate::register_parser!(
    "Microsoft Update Manifest .mum file",
    &["*.mum"],
    "windows-update",
    "",
    None,
);
