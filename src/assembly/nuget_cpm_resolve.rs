// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, FileInfo, PackageData, TopLevelDependency};

#[derive(Default)]
struct ResolvedCpmData {
    dependencies: Vec<Dependency>,
    properties: HashMap<String, String>,
    manage_package_versions_centrally: Option<bool>,
    central_package_version_override_enabled: Option<bool>,
}

#[derive(Default)]
struct ResolvedBuildPropsData {
    properties: HashMap<String, String>,
    manage_package_versions_centrally: Option<bool>,
    central_package_transitive_pinning_enabled: Option<bool>,
    central_package_version_override_enabled: Option<bool>,
}

pub(super) fn resolve_nuget_cpm_versions(
    files: &[FileInfo],
    dependencies: &mut [TopLevelDependency],
) {
    let props_by_path = collect_directory_packages_props(files);
    let build_props_by_path = collect_directory_build_props(files);
    let projects_by_path = collect_nuget_project_packages(files);

    for dependency in dependencies {
        if !is_nuget_project_dependency(dependency.datasource_id)
            || dependency.extracted_requirement.is_some()
        {
            continue;
        }

        let project_path = PathBuf::from(&dependency.datafile_path);
        let Some(project_dir) = project_path.parent() else {
            continue;
        };

        let build_props_data = resolve_effective_directory_build_props_for_dir(
            project_dir,
            &build_props_by_path,
            &mut HashSet::new(),
        );

        let project_property_map = build_props_data.properties.clone();

        let Some(props_path) =
            find_nearest_directory_packages_props_path(project_dir, &props_by_path)
        else {
            continue;
        };

        let props_data = resolve_effective_directory_packages_props(
            &props_path,
            &props_by_path,
            &build_props_data.properties,
            &mut HashSet::new(),
        );

        if !is_central_package_management_enabled(&props_data) {
            continue;
        }

        let project_override_enabled = projects_by_path
            .get(&project_path)
            .and_then(|package| {
                project_central_package_version_override_enabled(package, &project_property_map)
            })
            .or(build_props_data.central_package_version_override_enabled)
            .unwrap_or(false);

        if let Some(version_override) = resolve_package_version_override(
            dependency,
            &props_data,
            project_override_enabled,
            &project_property_map,
        ) {
            dependency.extracted_requirement = Some(version_override);
            continue;
        }

        if let Some(version) = resolve_central_package_version(dependency, &props_data) {
            dependency.extracted_requirement = Some(version);
        }
    }
}

fn collect_directory_packages_props(files: &[FileInfo]) -> HashMap<PathBuf, &PackageData> {
    let mut props_by_path = HashMap::new();

    for file in files {
        let Some(package_data) = file.package_data.iter().find(|package_data| {
            package_data.datasource_id == Some(DatasourceId::NugetDirectoryPackagesProps)
        }) else {
            continue;
        };

        props_by_path.insert(PathBuf::from(&file.path), package_data);
    }

    props_by_path
}

fn collect_directory_build_props(files: &[FileInfo]) -> HashMap<PathBuf, &PackageData> {
    let mut props_by_path = HashMap::new();

    for file in files {
        let Some(package_data) = file.package_data.iter().find(|package_data| {
            package_data.datasource_id == Some(DatasourceId::NugetDirectoryBuildProps)
        }) else {
            continue;
        };

        props_by_path.insert(PathBuf::from(&file.path), package_data);
    }

    props_by_path
}

fn collect_nuget_project_packages(files: &[FileInfo]) -> HashMap<PathBuf, &PackageData> {
    let mut projects = HashMap::new();

    for file in files {
        let Some(package_data) = file.package_data.iter().find(|package_data| {
            package_data
                .datasource_id
                .is_some_and(is_nuget_project_dependency)
        }) else {
            continue;
        };

        projects.insert(PathBuf::from(&file.path), package_data);
    }

    projects
}

fn find_nearest_directory_packages_props_path(
    project_dir: &Path,
    props_by_path: &HashMap<PathBuf, &PackageData>,
) -> Option<PathBuf> {
    for ancestor in project_dir.ancestors() {
        let candidate = ancestor.join("Directory.Packages.props");
        if props_by_path.contains_key(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn find_nearest_directory_build_props_path(
    project_dir: &Path,
    props_by_path: &HashMap<PathBuf, &PackageData>,
) -> Option<PathBuf> {
    for ancestor in project_dir.ancestors() {
        let candidate = ancestor.join("Directory.Build.props");
        if props_by_path.contains_key(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn is_nuget_project_dependency(datasource_id: DatasourceId) -> bool {
    matches!(
        datasource_id,
        DatasourceId::NugetCsproj | DatasourceId::NugetFsproj | DatasourceId::NugetVbproj
    )
}

fn resolve_effective_directory_packages_props(
    props_path: &Path,
    props_by_path: &HashMap<PathBuf, &PackageData>,
    base_properties: &HashMap<String, String>,
    visited: &mut HashSet<PathBuf>,
) -> ResolvedCpmData {
    let canonical = props_path
        .canonicalize()
        .unwrap_or_else(|_| props_path.to_path_buf());
    if !visited.insert(canonical) {
        return ResolvedCpmData::default();
    }

    let Some(package_data) = props_by_path.get(props_path) else {
        return ResolvedCpmData::default();
    };

    let mut resolved = ResolvedCpmData {
        properties: base_properties.clone(),
        ..ResolvedCpmData::default()
    };

    for import_path in raw_import_projects(package_data) {
        let Some(imported_path) =
            resolve_recorded_directory_packages_import(props_path, import_path, props_by_path)
        else {
            continue;
        };
        let imported = resolve_effective_directory_packages_props(
            &imported_path,
            props_by_path,
            base_properties,
            visited,
        );
        merge_resolved_cpm_data(&mut resolved, imported);
    }

    resolved
        .properties
        .extend(property_values_map(package_data));

    if let Some(value) =
        extra_bool(package_data, "manage_package_versions_centrally").or_else(|| {
            resolve_bool_property_reference(
                raw_extra_string(package_data, "manage_package_versions_centrally")
                    .or_else(|| raw_property_value(package_data, "ManagePackageVersionsCentrally")),
                &resolved.properties,
            )
        })
    {
        resolved.manage_package_versions_centrally = Some(value);
    }
    if let Some(value) = extra_bool(package_data, "central_package_version_override_enabled")
        .or_else(|| {
            resolve_bool_property_reference(
                raw_extra_string(package_data, "central_package_version_override_enabled").or_else(
                    || raw_property_value(package_data, "CentralPackageVersionOverrideEnabled"),
                ),
                &resolved.properties,
            )
        })
    {
        resolved.central_package_version_override_enabled = Some(value);
    }

    let raw_versions = raw_package_versions(package_data);
    if raw_versions.is_empty() {
        replace_matching_dependencies(&mut resolved.dependencies, &package_data.dependencies);
        resolved
            .dependencies
            .extend(package_data.dependencies.iter().cloned());
    } else {
        let dependencies = raw_versions
            .into_iter()
            .filter_map(|(name, version, condition)| {
                let resolved_version =
                    resolve_optional_property_value(version.as_deref(), &resolved.properties);
                build_central_dependency(name, resolved_version, version, condition)
            })
            .collect::<Vec<_>>();
        replace_matching_dependencies(&mut resolved.dependencies, &dependencies);
        resolved.dependencies.extend(dependencies);
    }

    resolved
}

fn resolve_effective_directory_build_props_for_dir(
    project_dir: &Path,
    props_by_path: &HashMap<PathBuf, &PackageData>,
    visited: &mut HashSet<PathBuf>,
) -> ResolvedBuildPropsData {
    let Some(props_path) = find_nearest_directory_build_props_path(project_dir, props_by_path)
    else {
        return ResolvedBuildPropsData::default();
    };

    resolve_effective_directory_build_props(&props_path, props_by_path, visited)
}

fn resolve_effective_directory_build_props(
    props_path: &Path,
    props_by_path: &HashMap<PathBuf, &PackageData>,
    visited: &mut HashSet<PathBuf>,
) -> ResolvedBuildPropsData {
    let canonical = props_path
        .canonicalize()
        .unwrap_or_else(|_| props_path.to_path_buf());
    if !visited.insert(canonical) {
        return ResolvedBuildPropsData::default();
    }

    let Some(package_data) = props_by_path.get(props_path) else {
        return ResolvedBuildPropsData::default();
    };

    let mut resolved = ResolvedBuildPropsData::default();

    for import_path in raw_import_projects(package_data) {
        let Some(imported_path) =
            resolve_recorded_directory_build_import(props_path, import_path, props_by_path)
        else {
            continue;
        };
        let imported =
            resolve_effective_directory_build_props(&imported_path, props_by_path, visited);
        merge_resolved_build_props_data(&mut resolved, imported);
    }

    resolved
        .properties
        .extend(property_values_map(package_data));

    if let Some(value) =
        extra_bool(package_data, "manage_package_versions_centrally").or_else(|| {
            resolve_bool_property_reference(
                raw_extra_string(package_data, "manage_package_versions_centrally")
                    .or_else(|| raw_property_value(package_data, "ManagePackageVersionsCentrally")),
                &resolved.properties,
            )
        })
    {
        resolved.manage_package_versions_centrally = Some(value);
    }
    if let Some(value) = extra_bool(package_data, "central_package_transitive_pinning_enabled")
        .or_else(|| {
            resolve_bool_property_reference(
                raw_extra_string(package_data, "central_package_transitive_pinning_enabled")
                    .or_else(|| {
                        raw_property_value(package_data, "CentralPackageTransitivePinningEnabled")
                    }),
                &resolved.properties,
            )
        })
    {
        resolved.central_package_transitive_pinning_enabled = Some(value);
    }
    if let Some(value) = extra_bool(package_data, "central_package_version_override_enabled")
        .or_else(|| {
            resolve_bool_property_reference(
                raw_extra_string(package_data, "central_package_version_override_enabled").or_else(
                    || raw_property_value(package_data, "CentralPackageVersionOverrideEnabled"),
                ),
                &resolved.properties,
            )
        })
    {
        resolved.central_package_version_override_enabled = Some(value);
    }

    resolved
}

fn merge_resolved_cpm_data(target: &mut ResolvedCpmData, source: ResolvedCpmData) {
    target.properties.extend(source.properties);
    if target.manage_package_versions_centrally.is_none() {
        target.manage_package_versions_centrally = source.manage_package_versions_centrally;
    }
    if target.central_package_version_override_enabled.is_none() {
        target.central_package_version_override_enabled =
            source.central_package_version_override_enabled;
    }
    replace_matching_dependencies(&mut target.dependencies, &source.dependencies);
    target.dependencies.extend(source.dependencies);
}

fn merge_resolved_build_props_data(
    target: &mut ResolvedBuildPropsData,
    source: ResolvedBuildPropsData,
) {
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
}

fn replace_matching_dependencies(target: &mut Vec<Dependency>, source: &[Dependency]) {
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
        candidate_condition(dependency).map(ToOwned::to_owned),
    )
}

fn resolve_recorded_directory_packages_import(
    current_path: &Path,
    import_project: &str,
    props_by_path: &HashMap<PathBuf, &PackageData>,
) -> Option<PathBuf> {
    let trimmed = import_project.trim();
    if trimmed.is_empty() {
        return None;
    }

    if is_get_path_of_file_above_directory_packages_import(trimmed) {
        let start_dir = current_path.parent()?.parent()?;
        for ancestor in start_dir.ancestors() {
            let candidate = ancestor.join("Directory.Packages.props");
            if props_by_path.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        return None;
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.file_name().and_then(|name| name.to_str()) != Some("Directory.Packages.props") {
        return None;
    }

    if candidate.is_absolute() {
        props_by_path.contains_key(&candidate).then_some(candidate)
    } else {
        let resolved = current_path.parent()?.join(candidate);
        props_by_path.contains_key(&resolved).then_some(resolved)
    }
}

fn resolve_recorded_directory_build_import(
    current_path: &Path,
    import_project: &str,
    props_by_path: &HashMap<PathBuf, &PackageData>,
) -> Option<PathBuf> {
    let trimmed = import_project.trim();
    if trimmed.is_empty() {
        return None;
    }

    if is_get_path_of_file_above_directory_build_import(trimmed) {
        let start_dir = current_path.parent()?.parent()?;
        for ancestor in start_dir.ancestors() {
            let candidate = ancestor.join("Directory.Build.props");
            if props_by_path.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        return None;
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.file_name().and_then(|name| name.to_str()) != Some("Directory.Build.props") {
        return None;
    }

    if candidate.is_absolute() {
        props_by_path.contains_key(&candidate).then_some(candidate)
    } else {
        let resolved = current_path.parent()?.join(candidate);
        props_by_path.contains_key(&resolved).then_some(resolved)
    }
}

fn is_get_path_of_file_above_directory_packages_import(project: &str) -> bool {
    project.replace(' ', "")
        == "$([MSBuild]::GetPathOfFileAbove(Directory.Packages.props,$(MSBuildThisFileDirectory)..))"
}

fn is_get_path_of_file_above_directory_build_import(project: &str) -> bool {
    project.replace(' ', "")
        == "$([MSBuild]::GetPathOfFileAbove(Directory.Build.props,$(MSBuildThisFileDirectory)..))"
}

fn is_central_package_management_enabled(package_data: &ResolvedCpmData) -> bool {
    package_data.manage_package_versions_centrally == Some(true)
}

fn is_central_package_version_override_enabled(package_data: &ResolvedCpmData) -> bool {
    package_data.central_package_version_override_enabled == Some(true)
}

fn resolve_package_version_override(
    dependency: &TopLevelDependency,
    package_data: &ResolvedCpmData,
    project_override_enabled: bool,
    project_properties: &HashMap<String, String>,
) -> Option<String> {
    if !project_override_enabled && !is_central_package_version_override_enabled(package_data) {
        return None;
    }

    let version_override = dependency_version_override(dependency, project_properties)?;
    if !is_literal_version_override(&version_override) {
        return None;
    }

    let dependency_purl = dependency.purl.as_deref()?;
    let dependency_condition = dependency_condition(dependency);

    let matching_central_versions = package_data
        .dependencies
        .iter()
        .filter(|candidate| candidate.scope.as_deref() == Some("package_version"))
        .filter(|candidate| candidate.purl.as_deref() == Some(dependency_purl))
        .filter(|candidate| condition_matches(dependency_condition, candidate_condition(candidate)))
        .count();

    (matching_central_versions == 1).then_some(version_override)
}

fn resolve_central_package_version(
    dependency: &TopLevelDependency,
    package_data: &ResolvedCpmData,
) -> Option<String> {
    let dependency_purl = dependency.purl.as_deref()?;
    let dependency_condition = dependency_condition(dependency);

    let matches: Vec<&str> = package_data
        .dependencies
        .iter()
        .filter(|candidate| candidate.scope.as_deref() == Some("package_version"))
        .filter(|candidate| candidate.purl.as_deref() == Some(dependency_purl))
        .filter_map(|candidate| {
            let version = candidate.extracted_requirement.as_deref()?;
            condition_matches(dependency_condition, candidate_condition(candidate))
                .then_some(version)
        })
        .collect();

    match matches.as_slice() {
        [version] => Some((*version).to_string()),
        _ => None,
    }
}

fn dependency_condition(dependency: &TopLevelDependency) -> Option<&str> {
    dependency
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("condition"))
        .and_then(|value| value.as_str())
}

fn dependency_version_override(
    dependency: &TopLevelDependency,
    project_properties: &HashMap<String, String>,
) -> Option<String> {
    if let Some(resolved) = dependency
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("version_override_resolved"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(resolved.to_string());
    }

    resolve_optional_property_value(
        dependency
            .extra_data
            .as_ref()
            .and_then(|extra_data| extra_data.get("version_override"))
            .and_then(|value| value.as_str()),
        project_properties,
    )
}

fn project_central_package_version_override_enabled(
    package_data: &&PackageData,
    project_properties: &HashMap<String, String>,
) -> Option<bool> {
    extra_bool(package_data, "central_package_version_override_enabled").or_else(|| {
        resolve_bool_property_reference(
            raw_extra_string(package_data, "central_package_version_override_enabled_raw"),
            project_properties,
        )
    })
}

fn is_literal_version_override(value: &str) -> bool {
    !value.contains("$(")
}

fn candidate_condition(candidate: &Dependency) -> Option<&str> {
    candidate
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("condition"))
        .and_then(|value| value.as_str())
}

fn condition_matches(
    dependency_condition: Option<&str>,
    candidate_condition: Option<&str>,
) -> bool {
    match candidate_condition {
        None => true,
        Some(candidate_condition) => dependency_condition == Some(candidate_condition),
    }
}

fn build_central_dependency(
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

    let mut extra_data = HashMap::new();
    if let Some(condition) = condition {
        extra_data.insert("condition".to_string(), JsonValue::String(condition));
    }
    if let Some(raw_version) = raw_version {
        extra_data.insert(
            "version_expression".to_string(),
            JsonValue::String(raw_version),
        );
    }

    Some(Dependency {
        purl: Some(format!("pkg:nuget/{name}")),
        extracted_requirement: Some(version),
        scope: Some("package_version".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn property_values_map(package_data: &PackageData) -> HashMap<String, String> {
    package_data
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("property_values"))
        .and_then(|value| value.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn raw_property_value<'a>(package_data: &'a PackageData, key: &str) -> Option<&'a str> {
    package_data
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("property_values"))
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
}

fn raw_extra_string<'a>(package_data: &'a PackageData, key: &str) -> Option<&'a str> {
    package_data
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get(key))
        .and_then(|value| value.as_str())
}

fn extra_bool(package_data: &PackageData, key: &str) -> Option<bool> {
    package_data
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get(key))
        .and_then(|value| value.as_bool())
}

fn raw_import_projects(package_data: &PackageData) -> Vec<&str> {
    package_data
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("import_projects"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .collect()
}

fn raw_package_versions(
    package_data: &PackageData,
) -> Vec<(Option<String>, Option<String>, Option<String>)> {
    package_data
        .extra_data
        .as_ref()
        .and_then(|extra_data| extra_data.get("package_versions"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .map(|entry| {
            (
                entry
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                entry
                    .get("version")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                entry
                    .get("condition")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
            )
        })
        .collect()
}

fn resolve_string_property_reference(
    value: &str,
    properties: &HashMap<String, String>,
) -> Option<String> {
    let trimmed = value.trim();
    if let Some(property_name) = trimmed
        .strip_prefix("$(")
        .and_then(|value| value.strip_suffix(')'))
    {
        properties.get(property_name).cloned()
    } else {
        Some(trimmed.to_string())
    }
}

fn resolve_bool_property_reference(
    value: Option<&str>,
    properties: &HashMap<String, String>,
) -> Option<bool> {
    let resolved = resolve_string_property_reference(value?, properties)?;
    Some(resolved.eq_ignore_ascii_case("true"))
}

fn resolve_optional_property_value(
    value: Option<&str>,
    properties: &HashMap<String, String>,
) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    if value.starts_with("$(") && value.ends_with(')') {
        resolve_string_property_reference(value, properties)
    } else {
        Some(value.to_string())
    }
}
