// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha256Digest,
};
use crate::parsers::python::read_toml_file;
use crate::parsers::utils::{MAX_ITERATION_COUNT, RecursionGuard, truncate_field};

use super::PackageParser;

const FIELD_PACKAGE: &str = "package";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_SOURCE: &str = "source";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_OPTIONAL_DEPENDENCIES: &str = "optional-dependencies";
const FIELD_DEV_DEPENDENCIES: &str = "dev-dependencies";
const FIELD_METADATA: &str = "metadata";
const FIELD_REQUIRES_DIST: &str = "requires-dist";
const FIELD_REQUIRES_DEV: &str = "requires-dev";
const FIELD_METADATA_OPTIONAL_DEPENDENCIES: &str = "optional-dependencies";
const FIELD_MARKER: &str = "marker";
const FIELD_EXTRA: &str = "extra";
const FIELD_SPECIFIER: &str = "specifier";
const FIELD_REVISION: &str = "revision";
const FIELD_REQUIRES_PYTHON: &str = "requires-python";
const FIELD_RESOLUTION_MARKERS: &str = "resolution-markers";
const FIELD_MANIFEST: &str = "manifest";

pub struct UvLockParser;

#[derive(Clone, Debug, Default)]
struct DirectDependencyInfo {
    extracted_requirement: Option<String>,
    scope: Option<String>,
    is_runtime: bool,
    is_optional: bool,
    extra_data: Option<HashMap<String, JsonValue>>,
    source_key: Option<String>,
}

#[derive(Clone, Debug)]
struct DependencyEdge {
    name: String,
    extracted_requirement: Option<String>,
    scope: Option<String>,
    is_runtime: bool,
    is_optional: bool,
    source_key: Option<String>,
    extra_data: Option<HashMap<String, JsonValue>>,
}

impl PackageParser for UvLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "uv.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_toml_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read uv.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_uv_lock(&toml_content)]
    }
}

fn parse_uv_lock(toml_content: &TomlValue) -> PackageData {
    let packages = toml_content
        .get(FIELD_PACKAGE)
        .and_then(TomlValue::as_array)
        .cloned()
        .unwrap_or_default();

    if packages.is_empty() {
        return default_package_data();
    }

    let package_tables: Vec<&TomlMap<String, TomlValue>> = packages
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(TomlValue::as_table)
        .collect();

    if package_tables.is_empty() {
        return default_package_data();
    }

    let root_index = find_root_package_index(&package_tables);
    let package_lookup = build_package_lookup(&package_tables);

    let direct_infos = root_index
        .and_then(|index| package_tables.get(index).copied())
        .map(collect_root_direct_dependencies)
        .unwrap_or_default();

    let runtime_roots: Vec<(String, Option<String>)> = direct_infos
        .iter()
        .filter(|(_, info)| info.is_runtime)
        .map(|(name, info)| (name.clone(), info.source_key.clone()))
        .collect();
    let dev_roots: Vec<(String, Option<String>)> = direct_infos
        .iter()
        .filter(|(_, info)| !info.is_runtime && !info.is_optional)
        .map(|(name, info)| (name.clone(), info.source_key.clone()))
        .collect();
    let optional_roots: Vec<(String, Option<String>)> = direct_infos
        .iter()
        .filter(|(_, info)| info.is_optional)
        .map(|(name, info)| (name.clone(), info.source_key.clone()))
        .collect();

    let runtime_reachable =
        collect_reachable_packages(&package_tables, &package_lookup, &runtime_roots, false);
    let dev_reachable =
        collect_reachable_packages(&package_tables, &package_lookup, &dev_roots, true);
    let optional_reachable =
        collect_reachable_packages(&package_tables, &package_lookup, &optional_roots, true);

    let mut package_data = default_package_data();
    package_data.extra_data = build_lock_extra_data(toml_content);

    if let Some(index) = root_index
        && let Some(root_table) = package_tables.get(index)
    {
        package_data.name = root_table
            .get(FIELD_NAME)
            .and_then(TomlValue::as_str)
            .map(normalize_pypi_name);
        package_data.version = root_table
            .get(FIELD_VERSION)
            .and_then(TomlValue::as_str)
            .map(|value| truncate_field(value.to_string()));
        package_data.is_virtual =
            package_source_table(root_table).is_some_and(|source| source.contains_key("virtual"));
        package_data.purl = package_data
            .name
            .as_deref()
            .and_then(|name| create_pypi_purl(name, package_data.version.as_deref()));
    }

    package_data.dependencies = package_tables
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != root_index)
        .filter_map(|(_, package_table)| {
            build_top_level_dependency(
                package_table,
                root_index.is_none(),
                &direct_infos,
                &runtime_reachable,
                &dev_reachable,
                &optional_reachable,
                &package_lookup,
            )
        })
        .collect();

    package_data
}

fn build_top_level_dependency(
    package_table: &TomlMap<String, TomlValue>,
    no_root_package: bool,
    direct_infos: &HashMap<String, DirectDependencyInfo>,
    runtime_reachable: &HashSet<String>,
    dev_reachable: &HashSet<String>,
    optional_reachable: &HashSet<String>,
    package_lookup: &HashMap<String, Vec<usize>>,
) -> Option<Dependency> {
    let name = package_table
        .get(FIELD_NAME)
        .and_then(TomlValue::as_str)
        .map(normalize_pypi_name)?;
    let version = package_table
        .get(FIELD_VERSION)
        .and_then(TomlValue::as_str)
        .map(|value| truncate_field(value.to_string()))?;

    let direct_info = direct_infos.get(&name);
    let is_direct = direct_info.is_some();
    let is_runtime = if no_root_package {
        true
    } else if let Some(info) = direct_info {
        info.is_runtime
    } else if runtime_reachable.contains(&name) {
        true
    } else {
        !dev_reachable.contains(&name) && !optional_reachable.contains(&name)
    };
    let is_optional = direct_info.is_some_and(|info| info.is_optional)
        || (!is_direct && optional_reachable.contains(&name) && !runtime_reachable.contains(&name));

    Some(Dependency {
        purl: create_pypi_purl(&name, Some(&version)).map(truncate_field),
        extracted_requirement: direct_info
            .and_then(|info| info.extracted_requirement.clone())
            .map(truncate_field),
        scope: direct_info
            .and_then(|info| info.scope.clone())
            .map(truncate_field),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(true),
        is_direct: Some(is_direct),
        resolved_package: Some(Box::new(build_resolved_package(
            package_table,
            package_lookup,
        ))),
        extra_data: direct_info.and_then(|info| info.extra_data.clone()),
    })
}

fn build_resolved_package(
    package_table: &TomlMap<String, TomlValue>,
    package_lookup: &HashMap<String, Vec<usize>>,
) -> ResolvedPackage {
    let name = package_table
        .get(FIELD_NAME)
        .and_then(TomlValue::as_str)
        .map(normalize_pypi_name)
        .unwrap_or_default();
    let version = package_table
        .get(FIELD_VERSION)
        .and_then(TomlValue::as_str)
        .map(|value| truncate_field(value.to_string()))
        .unwrap_or_default();

    let (_, repository_download_url, api_data_url, purl) =
        build_pypi_urls(Some(&name), Some(&version));
    let repository_homepage_url =
        Some(truncate_field(format!("https://pypi.org/project/{}", name)));
    let (download_url, sha256) = extract_artifact_metadata(package_table);

    let download_url = download_url.map(truncate_field);

    ResolvedPackage {
        primary_language: Some("Python".to_string()),
        download_url,
        sha1: None,
        sha256: sha256.and_then(|h| Sha256Digest::from_hex(&h).ok()),
        sha512: None,
        md5: None,
        is_virtual: true,
        extra_data: build_package_extra_data(package_table),
        dependencies: collect_package_dependency_edges(package_table)
            .into_iter()
            .map(|edge| edge_to_dependency(edge, package_lookup))
            .collect(),
        repository_homepage_url,
        repository_download_url: repository_download_url.map(truncate_field),
        api_data_url: api_data_url.map(truncate_field),
        datasource_id: Some(DatasourceId::PypiUvLock),
        purl: purl.map(truncate_field),
        ..ResolvedPackage::new(UvLockParser::PACKAGE_TYPE, String::new(), name, version)
    }
}

fn edge_to_dependency(
    edge: DependencyEdge,
    package_lookup: &HashMap<String, Vec<usize>>,
) -> Dependency {
    let is_pinned = edge
        .source_key
        .as_ref()
        .map(|_| !package_lookup.contains_key(&edge.name))
        .unwrap_or(false);

    Dependency {
        purl: create_pypi_purl(&edge.name, None).map(truncate_field),
        extracted_requirement: edge.extracted_requirement.map(truncate_field),
        scope: edge.scope.map(truncate_field),
        is_runtime: Some(edge.is_runtime),
        is_optional: Some(edge.is_optional),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: edge.extra_data,
    }
}

fn collect_root_direct_dependencies(
    root_table: &TomlMap<String, TomlValue>,
) -> HashMap<String, DirectDependencyInfo> {
    let mut infos = HashMap::new();
    let metadata = root_table.get(FIELD_METADATA).and_then(TomlValue::as_table);
    let runtime_requirements = metadata
        .and_then(|metadata| metadata.get(FIELD_REQUIRES_DIST))
        .map(parse_requirement_metadata_array)
        .unwrap_or_default();
    let dev_requirements = metadata
        .and_then(|metadata| metadata.get(FIELD_REQUIRES_DEV))
        .and_then(TomlValue::as_table)
        .map(parse_requirement_metadata_table)
        .unwrap_or_default();
    let optional_requirements = metadata
        .and_then(|metadata| metadata.get(FIELD_METADATA_OPTIONAL_DEPENDENCIES))
        .and_then(TomlValue::as_table)
        .map(parse_requirement_metadata_table)
        .unwrap_or_default();

    for edge in collect_dependency_edges_from_array(
        root_table
            .get(FIELD_DEPENDENCIES)
            .and_then(TomlValue::as_array),
        None,
        true,
        false,
        runtime_requirements.get("__runtime__"),
    ) {
        merge_direct_dependency_info(&mut infos, edge);
    }

    if let Some(optional_table) = root_table
        .get(FIELD_OPTIONAL_DEPENDENCIES)
        .and_then(TomlValue::as_table)
    {
        for (group, value) in optional_table.iter().take(MAX_ITERATION_COUNT) {
            let requirement_map = optional_requirements.get(group);
            for edge in collect_dependency_edges_from_array(
                value.as_array(),
                Some(group.to_string()),
                false,
                true,
                requirement_map,
            )
            .into_iter()
            .take(MAX_ITERATION_COUNT)
            {
                merge_direct_dependency_info(&mut infos, edge);
            }
        }
    }

    if let Some(dev_table) = root_table
        .get(FIELD_DEV_DEPENDENCIES)
        .and_then(TomlValue::as_table)
    {
        for (group, value) in dev_table.iter().take(MAX_ITERATION_COUNT) {
            let requirement_map = dev_requirements.get(group);
            for edge in collect_dependency_edges_from_array(
                value.as_array(),
                Some(group.to_string()),
                false,
                false,
                requirement_map,
            )
            .into_iter()
            .take(MAX_ITERATION_COUNT)
            {
                merge_direct_dependency_info(&mut infos, edge);
            }
        }
    }

    infos
}

fn merge_direct_dependency_info(
    infos: &mut HashMap<String, DirectDependencyInfo>,
    edge: DependencyEdge,
) {
    let name = edge.name.clone();
    let new_info = direct_info_from_edge(edge);

    if let Some(existing) = infos.get_mut(&name) {
        existing.is_runtime |= new_info.is_runtime;
        existing.is_optional &= new_info.is_optional;

        if existing.extracted_requirement.is_none() {
            existing.extracted_requirement = new_info.extracted_requirement.clone();
        }

        existing.scope = merge_scope(existing.scope.as_ref(), new_info.scope.as_ref());
        existing.extra_data =
            merge_optional_json_maps(existing.extra_data.take(), new_info.extra_data);

        if existing.source_key != new_info.source_key {
            existing.source_key = None;
        }
    } else {
        infos.insert(name, new_info);
    }
}

fn merge_scope(current: Option<&String>, new: Option<&String>) -> Option<String> {
    match (current, new) {
        (None, None) => None,
        (None, Some(_)) | (Some(_), None) => None,
        (Some(left), Some(right)) if left == right => Some(left.clone()),
        _ => None,
    }
}

fn merge_optional_json_maps(
    current: Option<HashMap<String, JsonValue>>,
    new: Option<HashMap<String, JsonValue>>,
) -> Option<HashMap<String, JsonValue>> {
    match (current, new) {
        (None, None) => None,
        (Some(map), None) | (None, Some(map)) => Some(map),
        (Some(mut current), Some(new)) => {
            for (key, value) in new {
                current.entry(key).or_insert(value);
            }
            Some(current)
        }
    }
}

fn direct_info_from_edge(edge: DependencyEdge) -> DirectDependencyInfo {
    DirectDependencyInfo {
        extracted_requirement: edge.extracted_requirement,
        scope: edge.scope,
        is_runtime: edge.is_runtime,
        is_optional: edge.is_optional,
        extra_data: edge.extra_data,
        source_key: edge.source_key,
    }
}

fn collect_package_dependency_edges(
    package_table: &TomlMap<String, TomlValue>,
) -> Vec<DependencyEdge> {
    let mut edges = Vec::new();

    edges.extend(collect_dependency_edges_from_array(
        package_table
            .get(FIELD_DEPENDENCIES)
            .and_then(TomlValue::as_array),
        None,
        true,
        false,
        None,
    ));

    if let Some(optional_table) = package_table
        .get(FIELD_OPTIONAL_DEPENDENCIES)
        .and_then(TomlValue::as_table)
    {
        for (group, value) in optional_table.iter().take(MAX_ITERATION_COUNT) {
            edges.extend(
                collect_dependency_edges_from_array(
                    value.as_array(),
                    Some(group.to_string()),
                    false,
                    true,
                    None,
                )
                .into_iter()
                .take(MAX_ITERATION_COUNT),
            );
        }
    }

    if let Some(dev_table) = package_table
        .get(FIELD_DEV_DEPENDENCIES)
        .and_then(TomlValue::as_table)
    {
        for (group, value) in dev_table.iter().take(MAX_ITERATION_COUNT) {
            edges.extend(
                collect_dependency_edges_from_array(
                    value.as_array(),
                    Some(group.to_string()),
                    false,
                    false,
                    None,
                )
                .into_iter()
                .take(MAX_ITERATION_COUNT),
            );
        }
    }

    edges
}

fn collect_dependency_edges_from_array(
    values: Option<&Vec<TomlValue>>,
    scope: Option<String>,
    is_runtime: bool,
    is_optional: bool,
    requirement_map: Option<&HashMap<String, String>>,
) -> Vec<DependencyEdge> {
    values
        .into_iter()
        .flatten()
        .filter_map(|value| {
            build_dependency_edge(
                value,
                scope.clone(),
                is_runtime,
                is_optional,
                requirement_map,
            )
        })
        .collect()
}

fn build_dependency_edge(
    value: &TomlValue,
    scope: Option<String>,
    is_runtime: bool,
    is_optional: bool,
    requirement_map: Option<&HashMap<String, String>>,
) -> Option<DependencyEdge> {
    let table = value.as_table()?;
    let name = table
        .get(FIELD_NAME)
        .and_then(TomlValue::as_str)
        .map(normalize_pypi_name)?;

    let mut extra_data = HashMap::new();
    if let Some(marker) = table.get(FIELD_MARKER).and_then(TomlValue::as_str) {
        extra_data.insert(
            FIELD_MARKER.to_string(),
            JsonValue::String(marker.to_string()),
        );
    }
    if let Some(extra_value) = table.get(FIELD_EXTRA) {
        let json_value = toml_value_to_json(extra_value);
        extra_data.insert(FIELD_EXTRA.to_string(), json_value);
    }

    let source_key = table
        .get(FIELD_SOURCE)
        .and_then(TomlValue::as_table)
        .and_then(source_table_key);
    if let Some(source) = table.get(FIELD_SOURCE) {
        extra_data.insert(FIELD_SOURCE.to_string(), toml_value_to_json(source));
    }

    let extracted_requirement = requirement_map
        .and_then(|map| map.get(&name).cloned().map(truncate_field))
        .or_else(|| {
            table
                .get(FIELD_SPECIFIER)
                .and_then(TomlValue::as_str)
                .map(|value| truncate_field(value.to_string()))
        });

    Some(DependencyEdge {
        name,
        extracted_requirement,
        scope,
        is_runtime,
        is_optional,
        source_key,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn parse_requirement_metadata_array(value: &TomlValue) -> HashMap<String, HashMap<String, String>> {
    let mut grouped = HashMap::new();
    let runtime = value
        .as_array()
        .map(|values| parse_requirement_entries(values))
        .unwrap_or_default();
    grouped.insert("__runtime__".to_string(), runtime);
    grouped
}

fn parse_requirement_metadata_table(
    table: &TomlMap<String, TomlValue>,
) -> HashMap<String, HashMap<String, String>> {
    table
        .iter()
        .map(|(group, value)| {
            (
                group.to_string(),
                value
                    .as_array()
                    .map(|values| parse_requirement_entries(values))
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn parse_requirement_entries(values: &[TomlValue]) -> HashMap<String, String> {
    values
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|value| {
            let table = value.as_table()?;
            let name = table
                .get(FIELD_NAME)
                .and_then(TomlValue::as_str)
                .map(normalize_pypi_name)?;
            let specifier = table
                .get(FIELD_SPECIFIER)
                .and_then(TomlValue::as_str)
                .map(|value| truncate_field(value.to_string()))?;
            Some((name, specifier))
        })
        .collect()
}

fn collect_reachable_packages(
    package_tables: &[&TomlMap<String, TomlValue>],
    package_lookup: &HashMap<String, Vec<usize>>,
    roots: &[(String, Option<String>)],
    include_non_runtime_edges: bool,
) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue: VecDeque<(String, Option<String>)> = roots.iter().cloned().collect();
    let mut iterations: usize = 0;

    while let Some((name, source_key)) = queue.pop_front() {
        iterations += 1;
        if iterations > MAX_ITERATION_COUNT {
            warn!(
                "collect_reachable_packages exceeded MAX_ITERATION_COUNT ({})",
                MAX_ITERATION_COUNT
            );
            break;
        }
        let Some(index) =
            match_package_index(package_tables, package_lookup, &name, source_key.as_deref())
        else {
            continue;
        };

        let Some(package_table) = package_tables.get(index) else {
            continue;
        };

        let package_name = package_table
            .get(FIELD_NAME)
            .and_then(TomlValue::as_str)
            .map(normalize_pypi_name)
            .unwrap_or(name);

        if !visited.insert(package_name.clone()) {
            continue;
        }

        let edges = if include_non_runtime_edges {
            collect_package_dependency_edges(package_table)
        } else {
            collect_dependency_edges_from_array(
                package_table
                    .get(FIELD_DEPENDENCIES)
                    .and_then(TomlValue::as_array),
                None,
                true,
                false,
                None,
            )
        };

        for edge in edges {
            queue.push_back((edge.name, edge.source_key));
        }
    }

    visited
}

fn build_package_lookup(
    package_tables: &[&TomlMap<String, TomlValue>],
) -> HashMap<String, Vec<usize>> {
    let mut lookup: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, package_table) in package_tables.iter().enumerate() {
        if let Some(name) = package_table
            .get(FIELD_NAME)
            .and_then(TomlValue::as_str)
            .map(normalize_pypi_name)
        {
            lookup.entry(name).or_default().push(index);
        }
    }
    lookup
}

fn match_package_index(
    package_tables: &[&TomlMap<String, TomlValue>],
    package_lookup: &HashMap<String, Vec<usize>>,
    name: &str,
    source_key: Option<&str>,
) -> Option<usize> {
    let candidates = package_lookup.get(name)?;
    if candidates.len() == 1 {
        return candidates.first().copied();
    }

    let source_key = source_key?;
    candidates.iter().copied().find(|index| {
        package_tables
            .get(*index)
            .and_then(|table| package_source_table(table))
            .and_then(source_table_key)
            .as_deref()
            == Some(source_key)
    })
}

fn find_root_package_index(package_tables: &[&TomlMap<String, TomlValue>]) -> Option<usize> {
    if let Some(index) = package_tables.iter().position(|table| {
        package_source_table(table)
            .and_then(local_source_path)
            .is_some_and(|path| path == ".")
    }) {
        return Some(index);
    }

    package_tables.iter().position(|table| {
        package_source_table(table)
            .is_some_and(|source| source.contains_key("editable") || source.contains_key("virtual"))
    })
}

fn local_source_path(source_table: &TomlMap<String, TomlValue>) -> Option<&str> {
    source_table
        .get("virtual")
        .and_then(TomlValue::as_str)
        .or_else(|| source_table.get("editable").and_then(TomlValue::as_str))
}

fn build_lock_extra_data(toml_content: &TomlValue) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    if let Some(version) = toml_content
        .get(FIELD_VERSION)
        .and_then(TomlValue::as_integer)
    {
        extra_data.insert(
            "lockfile_version".to_string(),
            JsonValue::String(version.to_string()),
        );
    }

    if let Some(revision) = toml_content
        .get(FIELD_REVISION)
        .and_then(TomlValue::as_integer)
    {
        extra_data.insert(
            FIELD_REVISION.to_string(),
            JsonValue::String(revision.to_string()),
        );
    }

    if let Some(requires_python) = toml_content
        .get(FIELD_REQUIRES_PYTHON)
        .and_then(TomlValue::as_str)
    {
        extra_data.insert(
            "requires_python".to_string(),
            JsonValue::String(requires_python.to_string()),
        );
    }

    if let Some(markers) = toml_content.get(FIELD_RESOLUTION_MARKERS) {
        extra_data.insert(
            FIELD_RESOLUTION_MARKERS.to_string(),
            toml_value_to_json(markers),
        );
    }

    if let Some(manifest) = toml_content.get(FIELD_MANIFEST) {
        extra_data.insert(FIELD_MANIFEST.to_string(), toml_value_to_json(manifest));
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn build_package_extra_data(
    package_table: &TomlMap<String, TomlValue>,
) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    if let Some(source) = package_table.get(FIELD_SOURCE) {
        extra_data.insert(FIELD_SOURCE.to_string(), toml_value_to_json(source));
    }

    if let Some(metadata) = package_table.get(FIELD_METADATA) {
        extra_data.insert(FIELD_METADATA.to_string(), toml_value_to_json(metadata));
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn extract_artifact_metadata(
    package_table: &TomlMap<String, TomlValue>,
) -> (Option<String>, Option<String>) {
    if let Some(sdist_table) = package_table.get("sdist").and_then(TomlValue::as_table) {
        let download_url = sdist_table
            .get("url")
            .and_then(TomlValue::as_str)
            .map(|value| truncate_field(value.to_string()));
        let sha256 = sdist_table
            .get("hash")
            .and_then(TomlValue::as_str)
            .and_then(strip_sha256_prefix);
        if download_url.is_some() || sha256.is_some() {
            return (download_url, sha256);
        }
    }

    let wheel_table = package_table
        .get("wheels")
        .and_then(TomlValue::as_array)
        .and_then(|wheels| wheels.first())
        .and_then(TomlValue::as_table);

    let download_url = wheel_table
        .and_then(|table| table.get("url"))
        .and_then(TomlValue::as_str)
        .map(|value| truncate_field(value.to_string()));
    let sha256 = wheel_table
        .and_then(|table| table.get("hash"))
        .and_then(TomlValue::as_str)
        .and_then(strip_sha256_prefix);

    (download_url, sha256)
}

fn strip_sha256_prefix(value: &str) -> Option<String> {
    value.strip_prefix("sha256:").map(|hash| hash.to_string())
}

fn package_source_table(
    package_table: &TomlMap<String, TomlValue>,
) -> Option<&TomlMap<String, TomlValue>> {
    package_table
        .get(FIELD_SOURCE)
        .and_then(TomlValue::as_table)
}

fn source_table_key(source_table: &TomlMap<String, TomlValue>) -> Option<String> {
    ["registry", "editable", "virtual", "git"]
        .into_iter()
        .find_map(|key| {
            source_table
                .get(key)
                .and_then(TomlValue::as_str)
                .map(|value| format!("{}:{}", key, value))
        })
}

fn build_pypi_urls(
    name: Option<&str>,
    version: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let repository_homepage_url =
        name.map(|value| truncate_field(format!("https://pypi.org/project/{}", value)));

    let repository_download_url = name.and_then(|value| {
        version.map(|ver| {
            truncate_field(format!(
                "https://pypi.org/packages/source/{}/{}/{}-{}.tar.gz",
                &value[..1.min(value.len())],
                value,
                value,
                ver
            ))
        })
    });

    let api_data_url = name.map(|value| {
        if let Some(ver) = version {
            truncate_field(format!("https://pypi.org/pypi/{}/{}/json", value, ver))
        } else {
            truncate_field(format!("https://pypi.org/pypi/{}/json", value))
        }
    });

    let purl = name.and_then(|value| create_pypi_purl(value, version));

    (
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        purl,
    )
}

fn normalize_pypi_name(name: &str) -> String {
    truncate_field(name.trim().to_ascii_lowercase())
}

fn create_pypi_purl(name: &str, version: Option<&str>) -> Option<String> {
    if name.contains('[') || name.contains(']') {
        return Some(truncate_field(build_manual_pypi_purl(name, version)));
    }

    if let Ok(mut purl) = PackageUrl::new(UvLockParser::PACKAGE_TYPE.as_str(), name) {
        if let Some(version) = version
            && purl.with_version(version).is_err()
        {
            return None;
        }
        return Some(truncate_field(purl.to_string()));
    }

    Some(truncate_field(build_manual_pypi_purl(name, version)))
}

fn build_manual_pypi_purl(name: &str, version: Option<&str>) -> String {
    let encoded_name = name.replace('[', "%5b").replace(']', "%5d");
    let mut purl = format!("pkg:pypi/{}", encoded_name);
    if let Some(version) = version
        && !version.is_empty()
    {
        purl.push('@');
        purl.push_str(version);
    }
    purl
}

fn toml_value_to_json(value: &TomlValue) -> JsonValue {
    toml_value_to_json_recursive(value, &mut RecursionGuard::depth_only())
}

fn toml_value_to_json_recursive(value: &TomlValue, guard: &mut RecursionGuard<()>) -> JsonValue {
    if guard.descend() {
        warn!("toml_value_to_json exceeded recursion depth limit");
        return JsonValue::Null;
    }

    let result = match value {
        TomlValue::String(value) => JsonValue::String(value.clone()),
        TomlValue::Integer(value) => JsonValue::String(value.to_string()),
        TomlValue::Float(value) => JsonValue::String(value.to_string()),
        TomlValue::Boolean(value) => JsonValue::Bool(*value),
        TomlValue::Datetime(value) => JsonValue::String(value.to_string()),
        TomlValue::Array(values) => JsonValue::Array(
            values
                .iter()
                .map(|v| toml_value_to_json_recursive(v, guard))
                .collect(),
        ),
        TomlValue::Table(values) => JsonValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_value_to_json_recursive(value, guard)))
                .collect(),
        ),
    };
    guard.ascend();
    result
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(UvLockParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        datasource_id: Some(DatasourceId::PypiUvLock),
        ..Default::default()
    }
}

crate::register_parser!(
    "uv lockfile",
    &["**/uv.lock"],
    "pypi",
    "Python",
    Some("https://docs.astral.sh/uv/concepts/projects/layout/"),
);
