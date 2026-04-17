use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::super::PackageParser;
use super::super::utils::{MAX_ITERATION_COUNT, RecursionGuard};
use super::utils::{resolve_bool_property_reference, resolve_optional_property_value};
use super::{build_nuget_purl, check_file_size, default_package_data, insert_extra_string};

pub struct CentralPackageManagementPropsParser;

pub struct DirectoryBuildPropsParser;

impl PackageParser for DirectoryBuildPropsParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("Directory.Build.props")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match (
            resolve_directory_build_props(path, &mut RecursionGuard::new()),
            parse_directory_build_props_file(path),
        ) {
            (Ok(data), Ok(raw)) => build_directory_build_props_package_data(data, raw),
            (Err(e), _) | (_, Err(e)) => {
                warn!("Error parsing Directory.Build.props at {:?}: {}", path, e);
                default_package_data(Some(DatasourceId::NugetDirectoryBuildProps))
            }
        }]
    }
}

impl PackageParser for CentralPackageManagementPropsParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nuget;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("Directory.Packages.props")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match (
            resolve_directory_packages_props(path, &mut RecursionGuard::new()),
            parse_directory_packages_props_file(path),
        ) {
            (Ok(data), Ok(raw)) => build_directory_packages_package_data(data, raw),
            (Err(e), _) | (_, Err(e)) => {
                warn!(
                    "Error parsing Directory.Packages.props at {:?}: {}",
                    path, e
                );
                default_package_data(Some(DatasourceId::NugetDirectoryPackagesProps))
            }
        }]
    }
}

#[derive(Default)]
struct CentralPackageVersionData {
    name: Option<String>,
    version: Option<String>,
    condition: Option<String>,
}

#[derive(Default)]
struct RawCentralPackagePropsData {
    package_versions: Vec<CentralPackageVersionData>,
    property_values: HashMap<String, String>,
    import_projects: Vec<String>,
    manage_package_versions_centrally: Option<String>,
    central_package_transitive_pinning_enabled: Option<String>,
    central_package_version_override_enabled: Option<String>,
}

#[derive(Default)]
struct RawBuildPropsData {
    property_values: HashMap<String, String>,
    import_projects: Vec<String>,
    manage_package_versions_centrally: Option<String>,
    central_package_transitive_pinning_enabled: Option<String>,
    central_package_version_override_enabled: Option<String>,
}

#[derive(Default)]
struct BuildPropsData {
    property_values: HashMap<String, String>,
    import_projects: Vec<String>,
    manage_package_versions_centrally: Option<bool>,
    central_package_transitive_pinning_enabled: Option<bool>,
    central_package_version_override_enabled: Option<bool>,
}

#[derive(Default)]
pub(super) struct CentralPackagePropsData {
    dependencies: Vec<Dependency>,
    properties: HashMap<String, String>,
    import_projects: Vec<String>,
    manage_package_versions_centrally: Option<bool>,
    central_package_transitive_pinning_enabled: Option<bool>,
    central_package_version_override_enabled: Option<bool>,
}

fn build_directory_packages_dependency(
    name: Option<String>,
    version: Option<String>,
    raw_version: Option<String>,
    condition: Option<String>,
) -> Option<Dependency> {
    let name = name?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let version = version
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;

    let mut extra_data = serde_json::Map::new();
    insert_extra_string(&mut extra_data, "condition", condition);
    insert_extra_string(&mut extra_data, "version_expression", raw_version);

    Some(Dependency {
        purl: build_nuget_purl(Some(&name), None),
        extracted_requirement: Some(version),
        scope: Some("package_version".to_string()),
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

fn resolve_directory_packages_props(
    path: &Path,
    guard: &mut RecursionGuard<PathBuf>,
) -> Result<CentralPackagePropsData, String> {
    if guard.exceeded() {
        return Err(format!(
            "Recursion depth exceeded resolving Directory.Packages.props at {:?}",
            path
        ));
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if guard.enter(canonical.clone()) {
        return Ok(CentralPackagePropsData::default());
    }

    let raw = parse_directory_packages_props_file(path)?;
    let mut merged = CentralPackagePropsData::default();

    for import_project in &raw.import_projects {
        let Some(import_path) =
            resolve_import_project_for_directory_packages(path, import_project, &HashMap::new())
        else {
            continue;
        };
        let imported = resolve_directory_packages_props(&import_path, guard)?;
        merge_central_package_props(&mut merged, imported);
    }

    merged.import_projects.extend(raw.import_projects.clone());
    merged.properties.extend(raw.property_values.clone());

    if let Some(value) = resolve_bool_property_reference(
        raw.manage_package_versions_centrally.as_deref(),
        &merged.properties,
    ) {
        merged.manage_package_versions_centrally = Some(value);
    }
    if let Some(value) = resolve_bool_property_reference(
        raw.central_package_transitive_pinning_enabled.as_deref(),
        &merged.properties,
    ) {
        merged.central_package_transitive_pinning_enabled = Some(value);
    }
    if let Some(value) = resolve_bool_property_reference(
        raw.central_package_version_override_enabled.as_deref(),
        &merged.properties,
    ) {
        merged.central_package_version_override_enabled = Some(value);
    }

    for entry in raw.package_versions {
        let resolved_version =
            resolve_optional_property_value(entry.version.as_deref(), &merged.properties);
        if let Some(dependency) = build_directory_packages_dependency(
            entry.name,
            resolved_version,
            entry.version,
            entry.condition,
        ) {
            replace_matching_dependency_group(
                &mut merged.dependencies,
                std::slice::from_ref(&dependency),
            );
            merged.dependencies.push(dependency);
        }
    }

    guard.leave(canonical);
    Ok(merged)
}

fn resolve_directory_build_props(
    path: &Path,
    guard: &mut RecursionGuard<PathBuf>,
) -> Result<BuildPropsData, String> {
    if guard.exceeded() {
        return Err(format!(
            "Recursion depth exceeded resolving Directory.Build.props at {:?}",
            path
        ));
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if guard.enter(canonical.clone()) {
        return Ok(BuildPropsData::default());
    }

    let raw = parse_directory_build_props_file(path)?;
    let mut merged = BuildPropsData::default();

    for import_project in &raw.import_projects {
        let Some(import_path) =
            resolve_import_project_for_directory_build(path, import_project, &HashMap::new())
        else {
            continue;
        };
        let imported = resolve_directory_build_props(&import_path, guard)?;
        merge_build_props_data(&mut merged, imported);
    }

    merged.import_projects.extend(raw.import_projects.clone());
    merged.property_values.extend(raw.property_values.clone());

    if let Some(value) = resolve_bool_property_reference(
        raw.manage_package_versions_centrally.as_deref(),
        &merged.property_values,
    ) {
        merged.manage_package_versions_centrally = Some(value);
    }
    if let Some(value) = resolve_bool_property_reference(
        raw.central_package_transitive_pinning_enabled.as_deref(),
        &merged.property_values,
    ) {
        merged.central_package_transitive_pinning_enabled = Some(value);
    }
    if let Some(value) = resolve_bool_property_reference(
        raw.central_package_version_override_enabled.as_deref(),
        &merged.property_values,
    ) {
        merged.central_package_version_override_enabled = Some(value);
    }

    guard.leave(canonical);
    Ok(merged)
}

fn parse_directory_packages_props_file(path: &Path) -> Result<RawCentralPackagePropsData, String> {
    check_file_size(path)?;

    let file = File::open(path).map_err(|e| {
        format!(
            "Failed to open Directory.Packages.props at {:?}: {}",
            path, e
        )
    })?;

    let reader = BufReader::new(file);
    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.config_mut().trim_text(true);

    let mut raw = RawCentralPackagePropsData::default();
    let mut buf = Vec::new();
    let mut current_element = String::new();
    let mut current_property_group_condition = None;
    let mut current_item_group_condition = None;
    let mut current_package_version: Option<CentralPackageVersionData> = None;
    let mut iteration_count: usize = 0;

    loop {
        iteration_count += 1;
        if iteration_count > MAX_ITERATION_COUNT {
            return Err(format!(
                "Iteration limit exceeded in Directory.Packages.props at {:?}; stopping at {} items",
                path, MAX_ITERATION_COUNT
            ));
        }
        match xml_reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = tag_name.clone();

                match tag_name.as_str() {
                    "ItemGroup" => {
                        current_item_group_condition = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"Condition")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                    }
                    "PackageVersion" => {
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
                        let condition = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"Condition")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
                            .or_else(|| current_item_group_condition.clone());

                        current_package_version = Some(CentralPackageVersionData {
                            name,
                            version,
                            condition,
                        });
                    }
                    "PropertyGroup" => {
                        current_property_group_condition = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|attr| attr.key.as_ref() == b"Condition")
                            .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag_name == "PackageVersion" {
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
                    let condition = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|attr| attr.key.as_ref() == b"Condition")
                        .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
                        .or_else(|| current_item_group_condition.clone());

                    raw.package_versions.push(CentralPackageVersionData {
                        name,
                        version,
                        condition,
                    });
                } else if tag_name == "Import"
                    && let Some(project) = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|attr| attr.key.as_ref() == b"Project")
                        .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
                    && !e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .any(|attr| attr.key.as_ref() == b"Condition")
                    && is_supported_directory_packages_import(&project)
                {
                    raw.import_projects.push(project.trim().to_string());
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.decode().ok().map(|s| s.trim().to_string());
                let Some(text) = text.filter(|value| !value.is_empty()) else {
                    buf.clear();
                    continue;
                };

                if current_package_version.is_some() {
                    if current_element.as_str() == "Version"
                        && let Some(entry) = &mut current_package_version
                    {
                        entry.version = Some(text);
                    }
                } else if current_property_group_condition.is_none() {
                    raw.property_values
                        .insert(current_element.clone(), text.clone());
                    match current_element.as_str() {
                        "ManagePackageVersionsCentrally" => {
                            raw.manage_package_versions_centrally = Some(text)
                        }
                        "CentralPackageTransitivePinningEnabled" => {
                            raw.central_package_transitive_pinning_enabled = Some(text)
                        }
                        "CentralPackageVersionOverrideEnabled" => {
                            raw.central_package_version_override_enabled = Some(text)
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match tag_name.as_str() {
                    "PropertyGroup" => current_property_group_condition = None,
                    "ItemGroup" => current_item_group_condition = None,
                    "PackageVersion" => {
                        if let Some(entry) = current_package_version.take() {
                            raw.package_versions.push(entry);
                        }
                    }
                    _ => {}
                }

                current_element.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!(
                    "Error parsing Directory.Packages.props at {:?}: {}",
                    path, e
                ));
            }
            _ => {}
        }

        buf.clear();
    }

    Ok(raw)
}

fn parse_directory_build_props_file(path: &Path) -> Result<RawBuildPropsData, String> {
    check_file_size(path)?;

    let file = File::open(path)
        .map_err(|e| format!("Failed to open Directory.Build.props at {:?}: {}", path, e))?;

    let reader = BufReader::new(file);
    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.config_mut().trim_text(true);

    let mut raw = RawBuildPropsData::default();
    let mut buf = Vec::new();
    let mut current_element = String::new();
    let mut in_property_group = false;
    let mut current_property_group_condition = None;
    let mut iteration_count: usize = 0;

    loop {
        iteration_count += 1;
        if iteration_count > MAX_ITERATION_COUNT {
            return Err(format!(
                "Iteration limit exceeded in Directory.Build.props at {:?}; stopping at {} items",
                path, MAX_ITERATION_COUNT
            ));
        }
        match xml_reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = tag_name.clone();
                if tag_name == "PropertyGroup" {
                    in_property_group = true;
                    current_property_group_condition = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|attr| attr.key.as_ref() == b"Condition")
                        .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok());
                }
            }
            Ok(Event::Empty(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag_name == "Import"
                    && let Some(project) = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|attr| attr.key.as_ref() == b"Project")
                        .and_then(|attr| String::from_utf8(attr.value.to_vec()).ok())
                    && !e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .any(|attr| attr.key.as_ref() == b"Condition")
                    && is_supported_directory_build_import(&project)
                {
                    raw.import_projects.push(project.trim().to_string());
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.decode().ok().map(|s| s.trim().to_string());
                let Some(text) = text.filter(|value| !value.is_empty()) else {
                    buf.clear();
                    continue;
                };

                if in_property_group && current_property_group_condition.is_none() {
                    raw.property_values
                        .insert(current_element.clone(), text.clone());
                    match current_element.as_str() {
                        "ManagePackageVersionsCentrally" => {
                            raw.manage_package_versions_centrally = Some(text)
                        }
                        "CentralPackageTransitivePinningEnabled" => {
                            raw.central_package_transitive_pinning_enabled = Some(text)
                        }
                        "CentralPackageVersionOverrideEnabled" => {
                            raw.central_package_version_override_enabled = Some(text)
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag_name == "PropertyGroup" {
                    in_property_group = false;
                    current_property_group_condition = None;
                }
                current_element.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!(
                    "Error parsing Directory.Build.props at {:?}: {}",
                    path, e
                ));
            }
            _ => {}
        }

        buf.clear();
    }

    Ok(raw)
}

fn build_directory_packages_package_data(
    data: CentralPackagePropsData,
    raw: RawCentralPackagePropsData,
) -> PackageData {
    let mut extra_data = serde_json::Map::new();
    if !data.properties.is_empty() {
        extra_data.insert(
            "property_values".to_string(),
            serde_json::Value::Object(
                data.properties
                    .iter()
                    .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                    .collect(),
            ),
        );
    }
    if let Some(value) = data.manage_package_versions_centrally {
        extra_data.insert(
            "manage_package_versions_centrally".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if let Some(value) = data.central_package_transitive_pinning_enabled {
        extra_data.insert(
            "central_package_transitive_pinning_enabled".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if let Some(value) = data.central_package_version_override_enabled {
        extra_data.insert(
            "central_package_version_override_enabled".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if !data.import_projects.is_empty() {
        extra_data.insert(
            "import_projects".to_string(),
            serde_json::Value::Array(
                data.import_projects
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    extra_data.insert(
        "package_versions".to_string(),
        serde_json::Value::Array(
            raw.package_versions
                .into_iter()
                .map(|entry| {
                    serde_json::json!({
                        "name": entry.name,
                        "version": entry.version,
                        "condition": entry.condition,
                    })
                })
                .collect(),
        ),
    );

    PackageData {
        datasource_id: Some(DatasourceId::NugetDirectoryPackagesProps),
        package_type: Some(PackageType::Nuget),
        dependencies: data.dependencies,
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data.into_iter().collect())
        },
        ..default_package_data(Some(DatasourceId::NugetDirectoryPackagesProps))
    }
}

fn build_directory_build_props_package_data(
    data: BuildPropsData,
    _raw: RawBuildPropsData,
) -> PackageData {
    let mut extra_data = serde_json::Map::new();
    if !data.property_values.is_empty() {
        extra_data.insert(
            "property_values".to_string(),
            serde_json::Value::Object(
                data.property_values
                    .iter()
                    .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                    .collect(),
            ),
        );
    }
    if let Some(value) = data.manage_package_versions_centrally {
        extra_data.insert(
            "manage_package_versions_centrally".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if let Some(value) = data.central_package_transitive_pinning_enabled {
        extra_data.insert(
            "central_package_transitive_pinning_enabled".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if let Some(value) = data.central_package_version_override_enabled {
        extra_data.insert(
            "central_package_version_override_enabled".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if !data.import_projects.is_empty() {
        extra_data.insert(
            "import_projects".to_string(),
            serde_json::Value::Array(
                data.import_projects
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    PackageData {
        datasource_id: Some(DatasourceId::NugetDirectoryBuildProps),
        package_type: Some(PackageType::Nuget),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data.into_iter().collect())
        },
        ..default_package_data(Some(DatasourceId::NugetDirectoryBuildProps))
    }
}

fn merge_central_package_props(
    target: &mut CentralPackagePropsData,
    source: CentralPackagePropsData,
) {
    target.import_projects.extend(source.import_projects);
    target.properties.extend(source.properties);
    if target.manage_package_versions_centrally.is_none() {
        target.manage_package_versions_centrally = source.manage_package_versions_centrally;
    }
    if target.central_package_transitive_pinning_enabled.is_none() {
        target.central_package_transitive_pinning_enabled =
            source.central_package_transitive_pinning_enabled;
    }
    if target.central_package_version_override_enabled.is_none() {
        target.central_package_version_override_enabled =
            source.central_package_version_override_enabled;
    }
    replace_matching_dependency_group(&mut target.dependencies, &source.dependencies);
    target.dependencies.extend(source.dependencies);
}

fn replace_matching_dependency_group(target: &mut Vec<Dependency>, source: &[Dependency]) {
    if source.is_empty() {
        return;
    }

    let source_keys = source.iter().map(dependency_key).collect::<Vec<_>>();
    target.retain(|candidate| {
        !source_keys
            .iter()
            .any(|key| *key == dependency_key(candidate))
    });
}

fn dependency_key(dependency: &Dependency) -> (Option<String>, Option<String>, Option<String>) {
    (
        dependency.purl.clone(),
        dependency.scope.clone(),
        dependency
            .extra_data
            .as_ref()
            .and_then(|data| data.get("condition"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
    )
}

fn is_supported_directory_packages_import(project: &str) -> bool {
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return false;
    }

    if is_get_path_of_file_above_import(trimmed) {
        return true;
    }

    let candidate = PathBuf::from(trimmed);
    candidate.file_name().and_then(|name| name.to_str()) == Some("Directory.Packages.props")
}

fn is_supported_directory_build_import(project: &str) -> bool {
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return false;
    }

    if is_get_path_of_file_above_build_import(trimmed) {
        return true;
    }

    let candidate = PathBuf::from(trimmed);
    candidate.file_name().and_then(|name| name.to_str()) == Some("Directory.Build.props")
}

fn is_get_path_of_file_above_import(project: &str) -> bool {
    let normalized = project.replace(' ', "");
    normalized
        == "$([MSBuild]::GetPathOfFileAbove(Directory.Packages.props,$(MSBuildThisFileDirectory)..))"
}

fn is_get_path_of_file_above_build_import(project: &str) -> bool {
    let normalized = project.replace(' ', "");
    normalized
        == "$([MSBuild]::GetPathOfFileAbove(Directory.Build.props,$(MSBuildThisFileDirectory)..))"
}

fn resolve_import_project_for_directory_build(
    current_path: &Path,
    project: &str,
    known_props_paths: &HashMap<PathBuf, &PackageData>,
) -> Option<PathBuf> {
    let trimmed = project.trim();
    if is_get_path_of_file_above_build_import(trimmed) {
        let start_dir = current_path.parent()?.parent()?;
        for ancestor in start_dir.ancestors() {
            let candidate = ancestor.join("Directory.Build.props");
            if known_props_paths.is_empty() {
                if candidate.exists() {
                    return Some(candidate);
                }
            } else if known_props_paths.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        return None;
    }

    if !is_supported_directory_build_import(trimmed) {
        return None;
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        if known_props_paths.is_empty() {
            candidate.exists().then_some(candidate)
        } else {
            known_props_paths
                .contains_key(&candidate)
                .then_some(candidate)
        }
    } else {
        let resolved = current_path.parent()?.join(candidate);
        if known_props_paths.is_empty() {
            resolved.exists().then_some(resolved)
        } else {
            known_props_paths
                .contains_key(&resolved)
                .then_some(resolved)
        }
    }
}

fn merge_build_props_data(target: &mut BuildPropsData, source: BuildPropsData) {
    target.import_projects.extend(source.import_projects);
    target.property_values.extend(source.property_values);
    if target.manage_package_versions_centrally.is_none() {
        target.manage_package_versions_centrally = source.manage_package_versions_centrally;
    }
    if target.central_package_transitive_pinning_enabled.is_none() {
        target.central_package_transitive_pinning_enabled =
            source.central_package_transitive_pinning_enabled;
    }
    if target.central_package_version_override_enabled.is_none() {
        target.central_package_version_override_enabled =
            source.central_package_version_override_enabled;
    }
}

fn resolve_import_project_for_directory_packages(
    current_path: &Path,
    project: &str,
    known_props_paths: &HashMap<PathBuf, &PackageData>,
) -> Option<PathBuf> {
    let trimmed = project.trim();
    if is_get_path_of_file_above_import(trimmed) {
        let start_dir = current_path.parent()?.parent()?;
        for ancestor in start_dir.ancestors() {
            let candidate = ancestor.join("Directory.Packages.props");
            if known_props_paths.is_empty() {
                if candidate.exists() {
                    return Some(candidate);
                }
            } else if known_props_paths.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        return None;
    }

    if !is_supported_directory_packages_import(trimmed) {
        return None;
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        if known_props_paths.is_empty() {
            candidate.exists().then_some(candidate)
        } else {
            known_props_paths
                .contains_key(&candidate)
                .then_some(candidate)
        }
    } else {
        let resolved = current_path.parent()?.join(candidate);
        if known_props_paths.is_empty() {
            resolved.exists().then_some(resolved)
        } else {
            known_props_paths
                .contains_key(&resolved)
                .then_some(resolved)
        }
    }
}

crate::register_parser!(
    ".NET Directory.Build.props property source",
    &["**/Directory.Build.props"],
    "nuget",
    "C#",
    Some(
        "https://learn.microsoft.com/en-us/visualstudio/msbuild/customize-by-directory?view=vs-2022"
    ),
);

crate::register_parser!(
    ".NET Directory.Packages.props central package management manifest",
    &["**/Directory.Packages.props"],
    "nuget",
    "C#",
    Some("https://learn.microsoft.com/en-us/nuget/consume-packages/central-package-management"),
);
