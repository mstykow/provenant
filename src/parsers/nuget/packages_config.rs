// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::super::PackageParser;
use super::super::utils::MAX_ITERATION_COUNT;
use super::{check_file_size, default_package_data};

pub struct PackagesConfigParser;

impl PackageParser for PackagesConfigParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "packages.config")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        if let Err(e) = check_file_size(path) {
            warn!("{}", e);
            return vec![default_package_data(Some(
                DatasourceId::NugetPackagesConfig,
            ))];
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open packages.config at {:?}: {}", path, e);
                return vec![default_package_data(Some(
                    DatasourceId::NugetPackagesConfig,
                ))];
            }
        };

        let reader = BufReader::new(file);
        let mut xml_reader = Reader::from_reader(reader);
        xml_reader.config_mut().trim_text(true);

        let mut dependencies = Vec::new();
        let mut buf = Vec::new();
        let mut iteration_count: usize = 0;

        loop {
            iteration_count += 1;
            if iteration_count > MAX_ITERATION_COUNT {
                warn!(
                    "Iteration limit exceeded in packages.config at {:?}; stopping at {} items",
                    path, MAX_ITERATION_COUNT
                );
                break;
            }
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Empty(e)) if e.name().as_ref() == b"package" => {
                    if let Some(dep) = parse_packages_config_package(&e) {
                        dependencies.push(dep);
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    warn!("Error parsing packages.config at {:?}: {}", path, e);
                    return vec![default_package_data(Some(
                        DatasourceId::NugetPackagesConfig,
                    ))];
                }
                _ => {}
            }
            buf.clear();
        }

        vec![PackageData {
            datasource_id: Some(DatasourceId::NugetPackagesConfig),
            package_type: Some(Self::PACKAGE_TYPE),
            dependencies,
            ..default_package_data(Some(DatasourceId::NugetPackagesConfig))
        }]
    }
}

fn parse_packages_config_package(element: &quick_xml::events::BytesStart) -> Option<Dependency> {
    let mut id = None;
    let mut version = None;
    let mut target_framework = None;

    for attr in element.attributes().filter_map(|a| a.ok()) {
        match attr.key.as_ref() {
            b"id" => id = String::from_utf8(attr.value.to_vec()).ok(),
            b"version" => version = String::from_utf8(attr.value.to_vec()).ok(),
            b"targetFramework" => target_framework = String::from_utf8(attr.value.to_vec()).ok(),
            _ => {}
        }
    }

    let name = id?;
    let purl = PackageUrl::new("nuget", &name).ok().map(|p| p.to_string());

    Some(Dependency {
        purl,
        extracted_requirement: version,
        scope: target_framework,
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

crate::register_parser!(
    ".NET packages.config manifest",
    &["**/packages.config"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/nuget/reference/packages-config"),
);
