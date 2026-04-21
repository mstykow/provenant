// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use regex::Regex;
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

use crate::models::{
    DatasourceId, Dependency, Md5Digest, PackageData, PackageType, ResolvedPackage, Sha256Digest,
    Sha512Digest,
};
use crate::parsers::python::read_toml_file;
use crate::parsers::utils::{MAX_ITERATION_COUNT, RecursionGuard, truncate_field};

use super::PackageParser;

const FIELD_LOCK_VERSION: &str = "lock-version";
const FIELD_CREATED_BY: &str = "created-by";
const SUPPORTED_LOCK_VERSION: &str = "1.0";
const FIELD_REQUIRES_PYTHON: &str = "requires-python";
const FIELD_ENVIRONMENTS: &str = "environments";
const FIELD_EXTRAS: &str = "extras";
const FIELD_DEPENDENCY_GROUPS: &str = "dependency-groups";
const FIELD_DEFAULT_GROUPS: &str = "default-groups";
const FIELD_PACKAGES: &str = "packages";
const FIELD_NAME: &str = "name";
const FIELD_VERSION: &str = "version";
const FIELD_MARKER: &str = "marker";
const FIELD_DEPENDENCIES: &str = "dependencies";
const FIELD_INDEX: &str = "index";
const FIELD_VCS: &str = "vcs";
const FIELD_DIRECTORY: &str = "directory";
const FIELD_ARCHIVE: &str = "archive";
const FIELD_SDIST: &str = "sdist";
const FIELD_WHEELS: &str = "wheels";
const FIELD_HASHES: &str = "hashes";
const FIELD_TOOL: &str = "tool";
const FIELD_ATTESTATION_IDENTITIES: &str = "attestation-identities";

pub struct PylockTomlParser;

#[derive(Clone, Debug, Default)]
struct MarkerClassification {
    is_runtime: bool,
    is_optional: bool,
    scope: Option<String>,
}

struct DependencyAnalysisContext<'a> {
    package_tables: &'a [&'a TomlMap<String, TomlValue>],
    dependency_indices: &'a [Vec<usize>],
    incoming_counts: &'a [usize],
    root_classifications: &'a [MarkerClassification],
    runtime_reachable: &'a HashSet<usize>,
    optional_reachable: &'a HashSet<usize>,
    scope_sets: &'a HashMap<String, HashSet<usize>>,
}

impl PackageParser for PylockTomlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Pypi;

    fn is_match(path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };

        file_name == "pylock.toml"
            || file_name
                .strip_prefix("pylock.")
                .and_then(|suffix| suffix.strip_suffix(".toml"))
                .is_some_and(|middle| !middle.is_empty() && !middle.contains('.'))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let toml_content = match read_toml_file(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read pylock.toml at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_pylock_toml(&toml_content)]
    }
}

fn parse_pylock_toml(toml_content: &TomlValue) -> PackageData {
    let lock_version = toml_content
        .get(FIELD_LOCK_VERSION)
        .and_then(TomlValue::as_str);
    if lock_version != Some(SUPPORTED_LOCK_VERSION) {
        warn!(
            "Invalid pylock.toml: missing or unsupported lock-version {:?}",
            lock_version
        );
        return default_package_data();
    }

    let created_by = toml_content
        .get(FIELD_CREATED_BY)
        .and_then(TomlValue::as_str);
    if created_by.is_none() {
        warn!("Invalid pylock.toml: missing required created-by field");
        return default_package_data();
    }

    let Some(package_values) = toml_content
        .get(FIELD_PACKAGES)
        .and_then(TomlValue::as_array)
    else {
        warn!("Invalid pylock.toml: missing required packages array");
        return default_package_data();
    };

    let package_tables: Vec<&TomlMap<String, TomlValue>> = package_values
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(TomlValue::as_table)
        .collect();
    if package_tables.is_empty() {
        warn!("Invalid pylock.toml: packages array does not contain package tables");
        return default_package_data();
    }

    let dependency_indices = build_dependency_indices(&package_tables);
    let incoming_counts = build_incoming_counts(package_tables.len(), &dependency_indices);
    let default_groups = extract_string_set(toml_content, FIELD_DEFAULT_GROUPS);

    let root_classifications: Vec<MarkerClassification> = package_tables
        .iter()
        .enumerate()
        .map(|(index, table)| {
            if incoming_counts[index] == 0 {
                classify_marker(
                    table.get(FIELD_MARKER).and_then(TomlValue::as_str),
                    &default_groups,
                )
            } else {
                MarkerClassification::default()
            }
        })
        .collect();

    let runtime_roots: Vec<usize> = root_classifications
        .iter()
        .enumerate()
        .filter_map(|(index, info)| {
            (incoming_counts[index] == 0 && info.is_runtime).then_some(index)
        })
        .collect();
    let optional_roots: Vec<usize> = root_classifications
        .iter()
        .enumerate()
        .filter_map(|(index, info)| {
            (incoming_counts[index] == 0 && info.is_optional).then_some(index)
        })
        .collect();

    let runtime_reachable = collect_reachable_indices(&dependency_indices, &runtime_roots);
    let optional_reachable = collect_reachable_indices(&dependency_indices, &optional_roots);

    let mut scope_sets: HashMap<String, HashSet<usize>> = HashMap::new();
    for (index, info) in root_classifications.iter().enumerate() {
        if incoming_counts[index] != 0 {
            continue;
        }

        if let Some(scope) = info.scope.as_ref() {
            scope_sets.insert(
                scope.clone(),
                collect_reachable_indices(&dependency_indices, &[index]),
            );
        }
    }

    let analysis = DependencyAnalysisContext {
        package_tables: &package_tables,
        dependency_indices: &dependency_indices,
        incoming_counts: &incoming_counts,
        root_classifications: &root_classifications,
        runtime_reachable: &runtime_reachable,
        optional_reachable: &optional_reachable,
        scope_sets: &scope_sets,
    };

    let mut package_data = default_package_data();
    package_data.extra_data = build_lock_extra_data(toml_content);
    package_data.dependencies = package_tables
        .iter()
        .enumerate()
        .filter_map(|(index, package_table)| {
            build_top_level_dependency(index, package_table, &analysis)
        })
        .collect();

    package_data
}

fn build_top_level_dependency(
    index: usize,
    package_table: &TomlMap<String, TomlValue>,
    analysis: &DependencyAnalysisContext<'_>,
) -> Option<Dependency> {
    let name = normalized_package_name(package_table)?;
    let version = package_version(package_table);
    let direct = analysis
        .incoming_counts
        .get(index)
        .copied()
        .unwrap_or_default()
        == 0;

    let (is_runtime, is_optional, scope) = if direct {
        let classification = analysis
            .root_classifications
            .get(index)
            .cloned()
            .unwrap_or_default();
        (
            classification.is_runtime,
            classification.is_optional,
            classification.scope,
        )
    } else {
        let is_runtime = analysis.runtime_reachable.contains(&index);
        let is_optional = !is_runtime && analysis.optional_reachable.contains(&index);
        let scope = scope_for_index(analysis.scope_sets, index);
        (is_runtime, is_optional, scope)
    };

    Some(Dependency {
        purl: create_pypi_purl(&name, version.as_deref()),
        extracted_requirement: None,
        scope: scope.map(truncate_field),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(is_package_pinned(package_table)),
        is_direct: Some(direct),
        resolved_package: Some(Box::new(build_resolved_package(
            package_table,
            analysis.package_tables,
            analysis
                .dependency_indices
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        ))),
        extra_data: build_package_extra_data(package_table),
    })
}

fn build_resolved_package(
    package_table: &TomlMap<String, TomlValue>,
    package_tables: &[&TomlMap<String, TomlValue>],
    dependency_indices: &[usize],
) -> ResolvedPackage {
    let name = normalized_package_name(package_table).unwrap_or_default();
    let version = package_version(package_table).unwrap_or_default();
    let (_, repository_download_url, api_data_url, purl) = build_pypi_urls(
        Some(&name),
        (!version.is_empty()).then_some(version.as_str()),
    );
    let repository_homepage_url = Some(format!("https://pypi.org/project/{}", name));
    let (download_url, sha256, sha512, md5) = extract_artifact_metadata(package_table);

    ResolvedPackage {
        primary_language: Some("Python".to_string()),
        download_url,
        sha1: None,
        sha256: sha256.and_then(|h| Sha256Digest::from_hex(&h).ok()),
        sha512: sha512.and_then(|h| Sha512Digest::from_hex(&h).ok()),
        md5: md5.and_then(|h| Md5Digest::from_hex(&h).ok()),
        is_virtual: false,
        extra_data: build_package_extra_data(package_table),
        dependencies: dependency_indices
            .iter()
            .filter_map(|child_index| package_tables.get(*child_index))
            .filter_map(|child| build_resolved_dependency(child))
            .collect(),
        repository_homepage_url,
        repository_download_url,
        api_data_url,
        datasource_id: Some(DatasourceId::PypiPylockToml),
        purl,
        ..ResolvedPackage::new(PylockTomlParser::PACKAGE_TYPE, String::new(), name, version)
    }
}

fn build_resolved_dependency(package_table: &TomlMap<String, TomlValue>) -> Option<Dependency> {
    let name = normalized_package_name(package_table)?;
    let version = package_version(package_table);

    Some(Dependency {
        purl: create_pypi_purl(&name, version.as_deref()),
        extracted_requirement: None,
        scope: None,
        is_runtime: None,
        is_optional: None,
        is_pinned: Some(is_package_pinned(package_table)),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: build_package_extra_data(package_table),
    })
}

fn build_dependency_indices(package_tables: &[&TomlMap<String, TomlValue>]) -> Vec<Vec<usize>> {
    let mut total_iterations: usize = 0;
    package_tables
        .iter()
        .map(|package_table| {
            package_table
                .get(FIELD_DEPENDENCIES)
                .and_then(TomlValue::as_array)
                .into_iter()
                .flatten()
                .filter_map(TomlValue::as_table)
                .flat_map(|reference| {
                    if total_iterations >= MAX_ITERATION_COUNT {
                        return Vec::new();
                    }
                    total_iterations += 1;
                    resolve_dependency_reference_indices(package_tables, reference)
                })
                .collect()
        })
        .collect()
}

fn resolve_dependency_reference_indices(
    package_tables: &[&TomlMap<String, TomlValue>],
    reference: &TomlMap<String, TomlValue>,
) -> Vec<usize> {
    let matches: Vec<usize> = package_tables
        .iter()
        .enumerate()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|(index, package_table)| {
            package_reference_matches(package_table, reference).then_some(index)
        })
        .collect();

    if matches.len() == 1 {
        matches
    } else {
        Vec::new()
    }
}

fn package_reference_matches(
    package_table: &TomlMap<String, TomlValue>,
    reference: &TomlMap<String, TomlValue>,
) -> bool {
    let mut guard = RecursionGuard::depth_only();
    reference.iter().all(|(key, ref_value)| {
        package_table
            .get(key)
            .is_some_and(|pkg_value| toml_values_match(pkg_value, ref_value, &mut guard))
    })
}

fn toml_values_match(left: &TomlValue, right: &TomlValue, guard: &mut RecursionGuard<()>) -> bool {
    if guard.descend() {
        warn!("toml_values_match: recursion depth limit exceeded");
        return false;
    }
    let result = match (left, right) {
        (TomlValue::String(left), TomlValue::String(right)) => left == right,
        (TomlValue::Integer(left), TomlValue::Integer(right)) => left == right,
        (TomlValue::Float(left), TomlValue::Float(right)) => left == right,
        (TomlValue::Boolean(left), TomlValue::Boolean(right)) => left == right,
        (TomlValue::Datetime(left), TomlValue::Datetime(right)) => left == right,
        (TomlValue::Array(left), TomlValue::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| toml_values_match(left, right, guard))
        }
        (TomlValue::Table(left), TomlValue::Table(right)) => {
            right.iter().all(|(key, right_value)| {
                left.get(key)
                    .is_some_and(|left_value| toml_values_match(left_value, right_value, guard))
            })
        }
        _ => false,
    };
    guard.ascend();
    result
}

fn build_incoming_counts(package_count: usize, dependency_indices: &[Vec<usize>]) -> Vec<usize> {
    let mut incoming = vec![0; package_count];
    for dependency_list in dependency_indices {
        for &child_index in dependency_list {
            if let Some(count) = incoming.get_mut(child_index) {
                *count += 1;
            }
        }
    }
    incoming
}

fn collect_reachable_indices(dependency_indices: &[Vec<usize>], roots: &[usize]) -> HashSet<usize> {
    let mut visited = HashSet::new();
    let mut queue: VecDeque<usize> = roots.iter().copied().collect();

    while let Some(index) = queue.pop_front() {
        if visited.len() >= MAX_ITERATION_COUNT {
            break;
        }
        if !visited.insert(index) {
            continue;
        }

        for &child_index in dependency_indices.get(index).into_iter().flatten() {
            queue.push_back(child_index);
        }
    }

    visited
}

fn classify_marker(marker: Option<&str>, default_groups: &HashSet<String>) -> MarkerClassification {
    let Some(marker) = marker else {
        return MarkerClassification {
            is_runtime: true,
            is_optional: false,
            scope: None,
        };
    };

    let extras = extract_marker_memberships(marker, "extras");
    if !extras.is_empty() {
        return MarkerClassification {
            is_runtime: false,
            is_optional: true,
            scope: single_scope(extras),
        };
    }

    let groups = extract_marker_memberships(marker, "dependency_groups");
    let non_default_groups: Vec<String> = groups
        .into_iter()
        .filter(|group| !default_groups.contains(group))
        .collect();
    if !non_default_groups.is_empty() {
        return MarkerClassification {
            is_runtime: false,
            is_optional: false,
            scope: single_scope(non_default_groups),
        };
    }

    MarkerClassification {
        is_runtime: true,
        is_optional: false,
        scope: None,
    }
}

fn extract_marker_memberships(marker: &str, variable_name: &str) -> Vec<String> {
    let pattern = format!(
        r#"['\"]([^'\"]+)['\"]\s+in\s+{}\b"#,
        regex::escape(variable_name)
    );
    let Ok(regex) = Regex::new(&pattern) else {
        return Vec::new();
    };

    let mut memberships: Vec<String> = regex
        .captures_iter(marker)
        .filter_map(|captures| {
            captures
                .get(1)
                .map(|value| truncate_field(value.as_str().trim().to_string()))
        })
        .filter(|value| !value.is_empty())
        .collect();
    memberships.sort();
    memberships.dedup();
    memberships
}

fn single_scope(values: Vec<String>) -> Option<String> {
    (values.len() == 1).then(|| values[0].clone())
}

fn scope_for_index(scope_sets: &HashMap<String, HashSet<usize>>, index: usize) -> Option<String> {
    let matches: Vec<String> = scope_sets
        .iter()
        .filter_map(|(scope, indices)| indices.contains(&index).then_some(scope.clone()))
        .collect();
    single_scope(matches)
}

fn normalized_package_name(package_table: &TomlMap<String, TomlValue>) -> Option<String> {
    package_table
        .get(FIELD_NAME)
        .and_then(TomlValue::as_str)
        .map(|value| truncate_field(value.trim().to_ascii_lowercase()))
}

fn package_version(package_table: &TomlMap<String, TomlValue>) -> Option<String> {
    package_table
        .get(FIELD_VERSION)
        .and_then(TomlValue::as_str)
        .map(|value| truncate_field(value.to_string()))
}

fn is_package_pinned(package_table: &TomlMap<String, TomlValue>) -> bool {
    package_table.contains_key(FIELD_VERSION)
        || package_table
            .get(FIELD_VCS)
            .and_then(TomlValue::as_table)
            .is_some_and(|table| table.contains_key("commit-id"))
        || has_hashes(package_table.get(FIELD_ARCHIVE))
        || has_hashes(package_table.get(FIELD_SDIST))
        || package_table
            .get(FIELD_WHEELS)
            .and_then(TomlValue::as_array)
            .into_iter()
            .flatten()
            .filter_map(TomlValue::as_table)
            .any(|wheel| wheel.contains_key(FIELD_HASHES))
}

fn has_hashes(value: Option<&TomlValue>) -> bool {
    value
        .and_then(TomlValue::as_table)
        .is_some_and(|table| table.contains_key(FIELD_HASHES))
}

fn build_lock_extra_data(toml_content: &TomlValue) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    for (source_key, target_key) in [
        (FIELD_LOCK_VERSION, "lock_version"),
        (FIELD_CREATED_BY, "created_by"),
        (FIELD_REQUIRES_PYTHON, "requires_python"),
        (FIELD_ENVIRONMENTS, FIELD_ENVIRONMENTS),
        (FIELD_EXTRAS, FIELD_EXTRAS),
        (FIELD_DEPENDENCY_GROUPS, FIELD_DEPENDENCY_GROUPS),
        (FIELD_DEFAULT_GROUPS, FIELD_DEFAULT_GROUPS),
    ] {
        if let Some(value) = toml_content.get(source_key) {
            extra_data.insert(target_key.to_string(), toml_value_to_json(value));
        }
    }

    if let Some(tool) = toml_content.get(FIELD_TOOL) {
        extra_data.insert(FIELD_TOOL.to_string(), toml_value_to_json(tool));
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn build_package_extra_data(
    package_table: &TomlMap<String, TomlValue>,
) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    for key in [
        FIELD_MARKER,
        FIELD_REQUIRES_PYTHON,
        FIELD_INDEX,
        FIELD_VCS,
        FIELD_DIRECTORY,
        FIELD_ARCHIVE,
        FIELD_SDIST,
        FIELD_WHEELS,
        FIELD_TOOL,
        FIELD_ATTESTATION_IDENTITIES,
    ] {
        if let Some(value) = package_table.get(key) {
            extra_data.insert(key.to_string(), toml_value_to_json(value));
        }
    }

    (!extra_data.is_empty()).then_some(extra_data)
}

fn extract_artifact_metadata(
    package_table: &TomlMap<String, TomlValue>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if let Some(archive_table) = package_table
        .get(FIELD_ARCHIVE)
        .and_then(TomlValue::as_table)
    {
        return (
            archive_table
                .get("url")
                .and_then(TomlValue::as_str)
                .map(|value| truncate_field(value.to_string()))
                .or_else(|| {
                    archive_table
                        .get("path")
                        .and_then(TomlValue::as_str)
                        .map(|value| truncate_field(value.to_string()))
                }),
            extract_hash_by_name(archive_table, "sha256"),
            extract_hash_by_name(archive_table, "sha512"),
            extract_hash_by_name(archive_table, "md5"),
        );
    }

    if let Some(sdist_table) = package_table.get(FIELD_SDIST).and_then(TomlValue::as_table) {
        return (
            sdist_table
                .get("url")
                .and_then(TomlValue::as_str)
                .map(|value| truncate_field(value.to_string()))
                .or_else(|| {
                    sdist_table
                        .get("path")
                        .and_then(TomlValue::as_str)
                        .map(|value| truncate_field(value.to_string()))
                }),
            extract_hash_by_name(sdist_table, "sha256"),
            extract_hash_by_name(sdist_table, "sha512"),
            extract_hash_by_name(sdist_table, "md5"),
        );
    }

    let wheel_table = package_table
        .get(FIELD_WHEELS)
        .and_then(TomlValue::as_array)
        .and_then(|wheels| wheels.first())
        .and_then(TomlValue::as_table);

    (
        wheel_table
            .and_then(|table| table.get("url"))
            .and_then(TomlValue::as_str)
            .map(|value| truncate_field(value.to_string()))
            .or_else(|| {
                wheel_table
                    .and_then(|table| table.get("path"))
                    .and_then(TomlValue::as_str)
                    .map(|value| truncate_field(value.to_string()))
            }),
        wheel_table.and_then(|table| extract_hash_by_name(table, "sha256")),
        wheel_table.and_then(|table| extract_hash_by_name(table, "sha512")),
        wheel_table.and_then(|table| extract_hash_by_name(table, "md5")),
    )
}

fn extract_hash_by_name(table: &TomlMap<String, TomlValue>, name: &str) -> Option<String> {
    table
        .get(FIELD_HASHES)
        .and_then(TomlValue::as_table)
        .and_then(|hashes| hashes.get(name))
        .and_then(TomlValue::as_str)
        .map(|value| truncate_field(value.to_string()))
}

fn extract_string_set(toml_content: &TomlValue, key: &str) -> HashSet<String> {
    toml_content
        .get(key)
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(TomlValue::as_str)
        .map(|value| value.to_string())
        .collect()
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
    let repository_homepage_url = name.map(|value| format!("https://pypi.org/project/{}", value));
    let repository_download_url = name.and_then(|value| {
        version.map(|ver| {
            format!(
                "https://pypi.org/packages/source/{}/{}/{}-{}.tar.gz",
                &value[..1.min(value.len())],
                value,
                value,
                ver
            )
        })
    });
    let api_data_url = name.map(|value| {
        if let Some(ver) = version {
            format!("https://pypi.org/pypi/{}/{}/json", value, ver)
        } else {
            format!("https://pypi.org/pypi/{}/json", value)
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

fn create_pypi_purl(name: &str, version: Option<&str>) -> Option<String> {
    if let Ok(mut purl) = PackageUrl::new(PylockTomlParser::PACKAGE_TYPE.as_str(), name) {
        if let Some(version) = version
            && purl.with_version(version).is_err()
        {
            return None;
        }
        return Some(truncate_field(purl.to_string()));
    }

    let mut purl = format!("pkg:pypi/{}", name);
    if let Some(version) = version
        && !version.is_empty()
    {
        purl.push('@');
        purl.push_str(version);
    }
    Some(truncate_field(purl))
}

fn toml_value_to_json(value: &TomlValue) -> JsonValue {
    match value {
        TomlValue::String(value) => JsonValue::String(value.clone()),
        TomlValue::Integer(value) => JsonValue::String(value.to_string()),
        TomlValue::Float(value) => JsonValue::String(value.to_string()),
        TomlValue::Boolean(value) => JsonValue::Bool(*value),
        TomlValue::Datetime(value) => JsonValue::String(value.to_string()),
        TomlValue::Array(values) => {
            JsonValue::Array(values.iter().map(toml_value_to_json).collect())
        }
        TomlValue::Table(values) => JsonValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_value_to_json(value)))
                .collect::<JsonMap<String, JsonValue>>(),
        ),
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PylockTomlParser::PACKAGE_TYPE),
        primary_language: Some("Python".to_string()),
        datasource_id: Some(DatasourceId::PypiPylockToml),
        ..Default::default()
    }
}

crate::register_parser!(
    "pylock.toml lockfile",
    &["**/pylock.toml", "**/pylock.*.toml"],
    "pypi",
    "Python",
    Some("https://packaging.python.org/en/latest/specifications/pylock-toml/"),
);
