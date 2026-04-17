use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::models::{Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::super::PackageParser;
use super::super::license_normalization::{
    empty_declared_license_data, normalize_spdx_declared_license,
};
use super::super::utils::{MAX_ITERATION_COUNT, truncate_field};
use super::utils::{resolve_bool_property_reference, resolve_string_property_reference};
use super::{
    PROJECT_FILE_EXTENSIONS, build_nuget_party, build_nuget_purl, build_nuget_urls,
    check_file_size, default_package_data, insert_extra_string, project_file_datasource_id,
};

#[derive(Default)]
struct ProjectReferenceData {
    name: Option<String>,
    version: Option<String>,
    version_override: Option<String>,
    condition: Option<String>,
}

pub struct PackageReferenceProjectParser;

impl PackageParser for PackageReferenceProjectParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| PROJECT_FILE_EXTENSIONS.contains(&ext))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let Some(datasource_id) = project_file_datasource_id(path) else {
            return vec![default_package_data(None)];
        };

        if let Err(e) = check_file_size(path) {
            warn!("{}", e);
            return vec![default_package_data(Some(datasource_id))];
        }

        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) => {
                warn!("Failed to open project file at {:?}: {}", path, e);
                return vec![default_package_data(Some(datasource_id))];
            }
        };

        let reader = BufReader::new(file);
        let mut xml_reader = Reader::from_reader(reader);
        xml_reader.config_mut().trim_text(true);

        let mut name = None;
        let mut fallback_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string());
        let mut version = None;
        let mut description = None;
        let mut homepage_url = None;
        let mut authors = None;
        let mut repository_url = None;
        let mut repository_type = None;
        let mut repository_branch = None;
        let mut repository_commit = None;
        let mut extracted_license_statement = None;
        let mut license_type = None;
        let mut copyright = None;
        let mut readme_file = None;
        let mut icon_file = None;
        let mut package_references = Vec::new();
        let mut project_properties = HashMap::new();

        let mut buf = Vec::new();
        let mut current_element = String::new();
        let mut in_property_group = false;
        let mut current_property_group_condition = None;
        let mut current_item_group_condition = None;
        let mut current_package_reference: Option<ProjectReferenceData> = None;
        let mut iteration_count: usize = 0;

        loop {
            iteration_count += 1;
            if iteration_count > MAX_ITERATION_COUNT {
                warn!(
                    "Iteration limit exceeded in project file at {:?}; stopping at {} items",
                    path, MAX_ITERATION_COUNT
                );
                break;
            }
            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    current_element = tag_name.clone();

                    match tag_name.as_str() {
                        "PropertyGroup" => {
                            in_property_group = true;
                            current_property_group_condition = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|attr| attr.key.as_ref() == b"Condition")
                                .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                        }
                        "ItemGroup" => {
                            current_item_group_condition = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|attr| attr.key.as_ref() == b"Condition")
                                .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                        }
                        "PackageReference" => {
                            let name = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|attr| matches!(attr.key.as_ref(), b"Include" | b"Update"))
                                .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                            let version = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|attr| attr.key.as_ref() == b"Version")
                                .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                            let version_override = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|attr| attr.key.as_ref() == b"VersionOverride")
                                .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                            let condition = e
                                .attributes()
                                .filter_map(|a| a.ok())
                                .find(|attr| attr.key.as_ref() == b"Condition")
                                .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
                                .or_else(|| current_item_group_condition.clone());

                            current_package_reference = Some(ProjectReferenceData {
                                name,
                                version,
                                version_override,
                                condition,
                            });
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    if tag_name == "PackageReference" {
                        let name = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| matches!(attr.key.as_ref(), b"Include" | b"Update"))
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                        let version = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"Version")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                        let version_override = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"VersionOverride")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                        let condition = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"Condition")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
                            .or_else(|| current_item_group_condition.clone());

                        package_references.push(ProjectReferenceData {
                            name,
                            version,
                            version_override,
                            condition,
                        });
                    }
                }
                Ok(Event::Text(e)) => {
                    let text = e.decode().ok().map(|s| s.trim().to_string());
                    let Some(text) = text.filter(|value| !value.is_empty()) else {
                        buf.clear();
                        continue;
                    };

                    if current_package_reference.is_some() {
                        if current_element.as_str() == "Version"
                            && let Some(reference) = &mut current_package_reference
                        {
                            reference.version = Some(text);
                        } else if current_element.as_str() == "VersionOverride"
                            && let Some(reference) = &mut current_package_reference
                        {
                            reference.version_override = Some(text);
                        }
                    } else if in_property_group && current_property_group_condition.is_none() {
                        project_properties.insert(current_element.clone(), text.clone());
                        match current_element.as_str() {
                            "PackageId" => name = Some(text),
                            "AssemblyName" if fallback_name.is_none() => fallback_name = Some(text),
                            "Version" if version.is_none() => version = Some(text),
                            "PackageVersion" => version = Some(text),
                            "Description" => description = Some(text),
                            "PackageProjectUrl" | "ProjectUrl" => homepage_url = Some(text),
                            "Authors" => authors = Some(text),
                            "RepositoryUrl" => repository_url = Some(text),
                            "RepositoryType" => repository_type = Some(text),
                            "RepositoryBranch" => repository_branch = Some(text),
                            "RepositoryCommit" => repository_commit = Some(text),
                            "PackageLicenseExpression" => {
                                extracted_license_statement = Some(text);
                                license_type = Some("expression".to_string());
                            }
                            "PackageLicenseFile" => {
                                extracted_license_statement = Some(text);
                                license_type = Some("file".to_string());
                            }
                            "PackageReadmeFile" => readme_file = Some(text),
                            "PackageIcon" => icon_file = Some(text),
                            "Copyright" => copyright = Some(text),
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    match tag_name.as_str() {
                        "PropertyGroup" => {
                            in_property_group = false;
                            current_property_group_condition = None;
                        }
                        "ItemGroup" => current_item_group_condition = None,
                        "PackageReference" => {
                            if let Some(reference) = current_package_reference.take() {
                                package_references.push(reference);
                            }
                        }
                        _ => {}
                    }

                    current_element.clear();
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    warn!("Error parsing project file at {:?}: {}", path, e);
                    return vec![default_package_data(Some(datasource_id))];
                }
                _ => {}
            }

            buf.clear();
        }

        let name = name.or(fallback_name);
        let vcs_url = repository_url.map(|url| match repository_type {
            Some(repo_type) if !repo_type.trim().is_empty() => format!("{}+{}", repo_type, url),
            _ => url,
        });
        let dependencies = package_references
            .into_iter()
            .filter_map(|reference| {
                build_project_file_dependency(
                    reference.name,
                    reference.version,
                    reference.version_override,
                    reference.condition,
                    &project_properties,
                )
            })
            .collect::<Vec<_>>();
        let (repository_homepage_url, repository_download_url, api_data_url) =
            build_nuget_urls(name.as_deref(), version.as_deref());

        let mut parties = Vec::new();
        if let Some(authors) = authors {
            parties.push(build_nuget_party("author", authors));
        }

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
        insert_extra_string(&mut extra_data, "readme_file", readme_file);
        insert_extra_string(&mut extra_data, "icon_file", icon_file);
        if let Some(value) = project_properties
            .get("CentralPackageVersionOverrideEnabled")
            .cloned()
        {
            extra_data.insert(
                "central_package_version_override_enabled_raw".to_string(),
                serde_json::Value::String(value),
            );
        }
        if let Some(value) = resolve_bool_property_reference(
            project_properties
                .get("CentralPackageVersionOverrideEnabled")
                .map(String::as_str),
            &project_properties,
        ) {
            extra_data.insert(
                "central_package_version_override_enabled".to_string(),
                serde_json::Value::Bool(value),
            );
        }

        let (declared_license_expression, declared_license_expression_spdx, license_detections) =
            if license_type.as_deref() == Some("expression") {
                normalize_spdx_declared_license(extracted_license_statement.as_deref())
            } else {
                empty_declared_license_data()
            };

        vec![PackageData {
            datasource_id: Some(datasource_id),
            package_type: Some(Self::PACKAGE_TYPE),
            name: name.clone().map(truncate_field),
            version: version.clone().map(truncate_field),
            purl: build_nuget_purl(name.as_deref(), version.as_deref()),
            description: description.map(truncate_field),
            homepage_url: homepage_url.map(truncate_field),
            parties,
            dependencies,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            extracted_license_statement: extracted_license_statement.map(truncate_field),
            copyright: copyright.map(truncate_field),
            vcs_url: vcs_url.map(truncate_field),
            extra_data: if extra_data.is_empty() {
                None
            } else {
                Some(extra_data.into_iter().collect())
            },
            repository_homepage_url,
            repository_download_url,
            api_data_url,
            ..default_package_data(Some(datasource_id))
        }]
    }
}

fn build_project_file_dependency(
    name: Option<String>,
    version: Option<String>,
    version_override: Option<String>,
    condition: Option<String>,
    project_properties: &HashMap<String, String>,
) -> Option<Dependency> {
    let name = name?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let mut extra_data = serde_json::Map::new();
    insert_extra_string(&mut extra_data, "condition", condition);
    insert_extra_string(
        &mut extra_data,
        "version_override",
        version_override.clone(),
    );
    insert_extra_string(
        &mut extra_data,
        "version_override_resolved",
        version_override
            .as_deref()
            .and_then(|value| resolve_string_property_reference(value, project_properties)),
    );

    Some(Dependency {
        purl: build_nuget_purl(Some(&name), None),
        extracted_requirement: version,
        scope: None,
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

crate::register_parser!(
    ".NET PackageReference C# project file",
    &["**/*.csproj"],
    "nuget",
    "C#",
    Some(
        "https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files"
    ),
);

crate::register_parser!(
    ".NET PackageReference Visual Basic project file",
    &["**/*.vbproj"],
    "nuget",
    "Visual Basic .NET",
    Some(
        "https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files"
    ),
);

crate::register_parser!(
    ".NET PackageReference F# project file",
    &["**/*.fsproj"],
    "nuget",
    "F#",
    Some(
        "https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files"
    ),
);
