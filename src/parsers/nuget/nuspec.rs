use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::super::PackageParser;
use super::super::license_normalization::{
    empty_declared_license_data, normalize_spdx_declared_license,
};
use super::super::utils::{MAX_ITERATION_COUNT, truncate_field};
use super::{
    build_nuget_description, build_nuget_party, build_nuget_purl, build_nuget_urls,
    check_file_size, default_package_data, insert_extra_string, parse_repository_metadata,
};

pub struct NuspecParser;

impl PackageParser for NuspecParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "nuspec")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        if let Err(e) = check_file_size(path) {
            warn!("{}", e);
            return vec![default_package_data(Some(DatasourceId::NugetNuspec))];
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open .nuspec at {:?}: {}", path, e);
                return vec![default_package_data(Some(DatasourceId::NugetNuspec))];
            }
        };

        let reader = BufReader::new(file);
        let mut xml_reader = Reader::from_reader(reader);
        xml_reader.config_mut().trim_text(true);

        let mut name = None;
        let mut version = None;
        let mut summary = None;
        let mut description = None;
        let mut title = None;
        let mut homepage_url = None;
        let mut parties = Vec::new();
        let mut dependencies = Vec::new();
        let mut extracted_license_statement = None;
        let mut license_type = None;
        let mut copyright = None;
        let mut vcs_url = None;
        let mut repository_branch = None;
        let mut repository_commit = None;

        let mut buf = Vec::new();
        let mut current_element = String::new();
        let mut in_metadata = false;
        let mut in_dependencies = false;
        let mut current_group_framework = None;
        let mut iteration_count: usize = 0;

        loop {
            iteration_count += 1;
            if iteration_count > MAX_ITERATION_COUNT {
                warn!(
                    "Iteration limit exceeded in .nuspec at {:?}; stopping at {} items",
                    path, MAX_ITERATION_COUNT
                );
                break;
            }
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    current_element = tag_name.clone();

                    if tag_name == "metadata" {
                        in_metadata = true;
                    } else if tag_name == "dependencies" && in_metadata {
                        in_dependencies = true;
                    } else if tag_name == "group" && in_dependencies {
                        current_group_framework = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"targetFramework")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                    } else if tag_name == "repository" && in_metadata {
                        let repository = parse_repository_metadata(&e);
                        vcs_url = repository.vcs_url;
                        repository_branch = repository.branch;
                        repository_commit = repository.commit;
                    } else if tag_name == "license" && in_metadata {
                        license_type = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"type")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                    }
                }
                Ok(Event::Empty(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    if tag_name == "dependency" && in_dependencies {
                        if let Some(dep) =
                            parse_nuspec_dependency(&e, current_group_framework.as_deref())
                        {
                            dependencies.push(dep);
                        }
                    } else if tag_name == "repository" && in_metadata {
                        let repository = parse_repository_metadata(&e);
                        vcs_url = repository.vcs_url;
                        repository_branch = repository.branch;
                        repository_commit = repository.commit;
                    }
                }
                Ok(Event::Text(e)) => {
                    if !in_metadata {
                        continue;
                    }

                    let text = e.decode().ok().map(|s| s.trim().to_string());
                    if let Some(text) = text.filter(|s| !s.is_empty()) {
                        match current_element.as_str() {
                            "id" => name = Some(text),
                            "version" => version = Some(text),
                            "summary" => summary = Some(text),
                            "description" => description = Some(text),
                            "title" => title = Some(text),
                            "projectUrl" => homepage_url = Some(text),
                            "authors" => {
                                parties.push(build_nuget_party("author", text));
                            }
                            "owners" => {
                                parties.push(build_nuget_party("owner", text));
                            }
                            "license" => {
                                extracted_license_statement = Some(text);
                            }
                            "licenseUrl" => {
                                if extracted_license_statement.is_none() {
                                    extracted_license_statement = Some(text);
                                }
                            }
                            "copyright" => copyright = Some(text),
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    if tag_name == "metadata" {
                        in_metadata = false;
                    } else if tag_name == "dependencies" {
                        in_dependencies = false;
                    } else if tag_name == "group" {
                        current_group_framework = None;
                    }

                    current_element.clear();
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    warn!("Error parsing .nuspec at {:?}: {}", path, e);
                    return vec![default_package_data(Some(DatasourceId::NugetNuspec))];
                }
                _ => {}
            }
            buf.clear();
        }

        let final_description = build_nuget_description(
            summary.as_deref(),
            description.as_deref(),
            title.as_deref(),
            name.as_deref(),
        );

        let (repository_homepage_url, repository_download_url, api_data_url) =
            build_nuget_urls(name.as_deref(), version.as_deref());

        let purl = build_nuget_purl(name.as_deref(), version.as_deref());

        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            if license_type.as_deref() == Some("expression") {
                normalize_spdx_declared_license(extracted_license_statement.as_deref())
            } else {
                empty_declared_license_data()
            };

        let holder = None;

        let mut extra_data = serde_json::Map::new();
        insert_extra_string(&mut extra_data, "license_type", license_type.clone());
        if license_type.as_deref() == Some("file") {
            insert_extra_string(
                &mut extra_data,
                "license_file",
                extracted_license_statement.clone(),
            );
        }
        insert_extra_string(&mut extra_data, "repository_branch", repository_branch);
        insert_extra_string(&mut extra_data, "repository_commit", repository_commit);

        vec![PackageData {
            datasource_id: Some(DatasourceId::NugetNuspec),
            package_type: Some(Self::PACKAGE_TYPE),
            name: name.map(truncate_field),
            version: version.map(truncate_field),
            purl,
            description: final_description.map(truncate_field),
            homepage_url: homepage_url.map(truncate_field),
            parties,
            dependencies,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            extracted_license_statement: extracted_license_statement.map(truncate_field),
            copyright: copyright.map(truncate_field),
            holder,
            vcs_url: vcs_url.map(truncate_field),
            extra_data: if extra_data.is_empty() {
                None
            } else {
                Some(extra_data.into_iter().collect())
            },
            repository_homepage_url,
            repository_download_url,
            api_data_url,
            ..default_package_data(Some(DatasourceId::NugetNuspec))
        }]
    }
}

pub(super) fn parse_nuspec_dependency(
    element: &quick_xml::events::BytesStart,
    framework: Option<&str>,
) -> Option<Dependency> {
    let mut id = None;
    let mut version = None;
    let mut include = None;
    let mut exclude = None;

    for attr in element.attributes().filter_map(|a| a.ok()) {
        match attr.key.as_ref() {
            b"id" => id = String::from_utf8(attr.value.to_vec()).ok(),
            b"version" => version = String::from_utf8(attr.value.to_vec()).ok(),
            b"include" => include = String::from_utf8(attr.value.to_vec()).ok(),
            b"exclude" => exclude = String::from_utf8(attr.value.to_vec()).ok(),
            _ => {}
        }
    }

    let name = id?;
    let purl = PackageUrl::new("nuget", &name).ok().map(|p| p.to_string());

    let mut extra_data = serde_json::Map::new();
    if let Some(fw) = framework {
        extra_data.insert(
            "framework".to_string(),
            serde_json::Value::String(fw.to_string()),
        );
    }
    if let Some(inc) = include {
        extra_data.insert("include".to_string(), serde_json::Value::String(inc));
    }
    if let Some(exc) = exclude {
        extra_data.insert("exclude".to_string(), serde_json::Value::String(exc));
    }

    Some(Dependency {
        purl,
        extracted_requirement: version,
        scope: Some("dependency".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data.into_iter().collect())
        },
    })
}

pub(super) fn parse_nuspec_content(content: &str) -> Result<PackageData, String> {
    use quick_xml::Reader;

    let mut xml_reader = Reader::from_str(content);
    xml_reader.config_mut().trim_text(true);

    let mut name = None;
    let mut version = None;
    let mut description = None;
    let mut homepage_url = None;
    let mut parties = Vec::new();
    let mut dependencies = Vec::new();
    let mut extracted_license_statement = None;
    let mut license_type = None;
    let mut copyright = None;
    let mut vcs_url = None;
    let mut repository_branch = None;
    let mut repository_commit = None;

    let mut buf = Vec::new();
    let mut current_element = String::new();
    let mut in_metadata = false;
    let mut in_dependencies = false;
    let mut current_group_framework = None;
    let mut iteration_count: usize = 0;

    loop {
        iteration_count += 1;
        if iteration_count > MAX_ITERATION_COUNT {
            return Err(format!(
                "Iteration limit exceeded parsing .nuspec content; stopping at {} items",
                MAX_ITERATION_COUNT
            ));
        }
        match xml_reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = tag_name.clone();

                if tag_name == "metadata" {
                    in_metadata = true;
                } else if tag_name == "dependencies" && in_metadata {
                    in_dependencies = true;
                } else if tag_name == "group" && in_dependencies {
                    current_group_framework = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|attr| attr.key.as_ref() == b"targetFramework")
                        .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                } else if tag_name == "repository" && in_metadata {
                    let repository = parse_repository_metadata(&e);
                    vcs_url = repository.vcs_url;
                    repository_branch = repository.branch;
                    repository_commit = repository.commit;
                } else if tag_name == "license" && in_metadata {
                    license_type = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|attr| attr.key.as_ref() == b"type")
                        .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                }
            }
            Ok(Event::Empty(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if tag_name == "dependency" && in_dependencies {
                    if let Some(dep) =
                        parse_nuspec_dependency(&e, current_group_framework.as_deref())
                    {
                        dependencies.push(dep);
                    }
                } else if tag_name == "repository" && in_metadata {
                    let repository = parse_repository_metadata(&e);
                    vcs_url = repository.vcs_url;
                    repository_branch = repository.branch;
                    repository_commit = repository.commit;
                }
            }
            Ok(Event::Text(e)) => {
                if !in_metadata {
                    continue;
                }

                let text = e.decode().ok().map(|s| s.trim().to_string());
                if let Some(text) = text.filter(|s| !s.is_empty()) {
                    match current_element.as_str() {
                        "id" => name = Some(text),
                        "version" => version = Some(text),
                        "description" => description = Some(text),
                        "projectUrl" => homepage_url = Some(text),
                        "authors" => {
                            parties.push(build_nuget_party("author", text));
                        }
                        "owners" => {
                            parties.push(build_nuget_party("owner", text));
                        }
                        "license" => {
                            extracted_license_statement = Some(text);
                        }
                        "licenseUrl" => {
                            if extracted_license_statement.is_none() {
                                extracted_license_statement = Some(text);
                            }
                        }
                        "copyright" => copyright = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if tag_name == "metadata" {
                    in_metadata = false;
                } else if tag_name == "dependencies" {
                    in_dependencies = false;
                } else if tag_name == "group" {
                    current_group_framework = None;
                }

                current_element.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!("XML parsing error: {}", e));
            }
            _ => {}
        }
        buf.clear();
    }

    let (repository_homepage_url, repository_download_url, api_data_url) =
        build_nuget_urls(name.as_deref(), version.as_deref());

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        if license_type.as_deref() == Some("expression") {
            normalize_spdx_declared_license(extracted_license_statement.as_deref())
        } else {
            empty_declared_license_data()
        };

    let holder = None;

    let mut extra_data = serde_json::Map::new();
    insert_extra_string(&mut extra_data, "license_type", license_type.clone());
    if license_type.as_deref() == Some("file") {
        insert_extra_string(
            &mut extra_data,
            "license_file",
            extracted_license_statement.clone(),
        );
    }
    insert_extra_string(&mut extra_data, "repository_branch", repository_branch);
    insert_extra_string(&mut extra_data, "repository_commit", repository_commit);

    Ok(PackageData {
        datasource_id: Some(DatasourceId::NugetNupkg),
        package_type: Some(super::nupkg::NupkgParser::PACKAGE_TYPE),
        name: name.map(truncate_field),
        version: version.map(truncate_field),
        description: description.map(truncate_field),
        homepage_url: homepage_url.map(truncate_field),
        parties,
        dependencies,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement: extracted_license_statement.map(truncate_field),
        copyright: copyright.map(truncate_field),
        holder,
        vcs_url: vcs_url.map(truncate_field),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data.into_iter().collect())
        },
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        ..default_package_data(Some(DatasourceId::NugetNupkg))
    })
}

crate::register_parser!(
    ".NET .nuspec package specification",
    &["**/*.nuspec"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/nuget/reference/nuspec"),
);
