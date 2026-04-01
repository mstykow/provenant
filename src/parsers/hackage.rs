use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use regex::Regex;
use serde_json::Value as JsonValue;
use yaml_serde::{Mapping, Value as YamlValue};

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::utils::split_name_email;

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Hackage;
const PRIMARY_LANGUAGE: &str = "Haskell";

pub struct HackageCabalParser;

pub struct HackageCabalProjectParser;

pub struct HackageStackYamlParser;

impl PackageParser for HackageCabalParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().is_some_and(|ext| ext == "cabal")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read cabal file {:?}: {}", path, error);
                return vec![default_package_data(DatasourceId::HackageCabal)];
            }
        };

        vec![parse_cabal_manifest(&content)]
    }
}

impl PackageParser for HackageCabalProjectParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "cabal.project")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read cabal.project {:?}: {}", path, error);
                return vec![default_package_data(DatasourceId::HackageCabalProject)];
            }
        };

        vec![parse_cabal_project(&content)]
    }
}

impl PackageParser for HackageStackYamlParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "stack.yaml")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read stack.yaml {:?}: {}", path, error);
                return vec![default_package_data(DatasourceId::HackageStackYaml)];
            }
        };

        let yaml: YamlValue = match yaml_serde::from_str(&content) {
            Ok(yaml) => yaml,
            Err(error) => {
                warn!("Failed to parse stack.yaml {:?}: {}", path, error);
                return vec![default_package_data(DatasourceId::HackageStackYaml)];
            }
        };

        vec![parse_stack_yaml(&yaml)]
    }
}

#[derive(Clone, Debug, Default)]
struct ComponentContext {
    component_type: String,
    component_name: Option<String>,
}

#[derive(Debug, Default)]
struct CabalData {
    name: Option<String>,
    version: Option<String>,
    synopsis: Option<String>,
    description: Option<String>,
    license: Option<String>,
    homepage_url: Option<String>,
    bug_tracking_url: Option<String>,
    vcs_url: Option<String>,
    authors: Vec<String>,
    maintainers: Vec<String>,
    category_keywords: Vec<String>,
    explicit_keywords: Vec<String>,
    dependencies: Vec<Dependency>,
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some(PRIMARY_LANGUAGE.to_string()),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

fn parse_cabal_manifest(content: &str) -> PackageData {
    let parsed = parse_cabal_data(content);
    let keywords = merge_keywords(&parsed.category_keywords, &parsed.explicit_keywords);
    let description = combine_summary_and_description(&parsed.synopsis, &parsed.description);
    let parties = build_parties(&parsed.authors, &parsed.maintainers);
    let purl = build_hackage_purl(parsed.name.as_deref(), parsed.version.as_deref());
    let repository_homepage_url = parsed
        .name
        .as_ref()
        .map(|name| match parsed.version.as_ref() {
            Some(version) => format!("https://hackage.haskell.org/package/{}-{}", name, version),
            None => format!("https://hackage.haskell.org/package/{}", name),
        });

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: None,
        name: parsed.name,
        version: parsed.version,
        qualifiers: None,
        subpath: None,
        primary_language: Some(PRIMARY_LANGUAGE.to_string()),
        description,
        release_date: None,
        parties,
        keywords,
        homepage_url: parsed.homepage_url,
        download_url: None,
        size: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: parsed.bug_tracking_url,
        code_view_url: None,
        vcs_url: parsed.vcs_url,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement: parsed.license,
        notice_text: None,
        source_packages: Vec::new(),
        file_references: Vec::new(),
        is_private: false,
        is_virtual: false,
        extra_data: None,
        dependencies: parsed.dependencies,
        repository_homepage_url,
        repository_download_url: None,
        api_data_url: None,
        datasource_id: Some(DatasourceId::HackageCabal),
        purl,
    }
}

fn parse_cabal_project(content: &str) -> PackageData {
    let mut package_data = default_package_data(DatasourceId::HackageCabalProject);
    let lines: Vec<&str> = content.lines().collect();
    let mut dependencies = Vec::new();
    let mut extra_data = HashMap::new();
    let mut source_repo_entries: Vec<HashMap<String, JsonValue>> = Vec::new();
    let mut current_source_repo: Option<HashMap<String, JsonValue>> = None;
    let mut index = 0;

    while index < lines.len() {
        let cleaned = strip_cabal_comment(lines[index]);
        let trimmed = cleaned.trim();
        let indent = indentation(cleaned);

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        if indent == 0 && trimmed == "source-repository-package" {
            if let Some(entry) = current_source_repo.take() {
                source_repo_entries.push(entry);
            }
            current_source_repo = Some(HashMap::new());
            index += 1;
            continue;
        }

        let Some((key, value, next_index)) = collect_indented_field(&lines, index) else {
            if indent == 0
                && let Some(entry) = current_source_repo.take()
            {
                source_repo_entries.push(entry);
            }
            index += 1;
            continue;
        };

        if current_source_repo.is_some() && indent > 0 {
            if let Some(source_repo) = current_source_repo.as_mut() {
                source_repo.insert(
                    project_extra_key(&key),
                    parse_multiline_scalar_or_list(&value),
                );
            }
            index = next_index + 1;
            continue;
        }

        if current_source_repo.is_some()
            && indent == 0
            && key != "source-repository-package"
            && let Some(entry) = current_source_repo.take()
        {
            source_repo_entries.push(entry);
        }

        match key.as_str() {
            "packages" => {
                dependencies.extend(parse_path_like_entries(&value, "packages", false));
            }
            "optional-packages" => {
                dependencies.extend(parse_path_like_entries(&value, "optional-packages", true));
            }
            "extra-packages" => {
                dependencies.extend(parse_hackage_spec_entries(&value, "extra-packages", None));
            }
            "import" => {
                dependencies.extend(parse_import_entries(&value));
            }
            _ => {
                extra_data.insert(
                    project_extra_key(&key),
                    parse_multiline_scalar_or_list(&value),
                );
            }
        }

        index = next_index + 1;
    }

    if let Some(entry) = current_source_repo.take() {
        source_repo_entries.push(entry);
    }

    for entry in source_repo_entries {
        dependencies.push(build_source_repository_dependency(entry));
    }

    package_data.dependencies = dependencies;
    package_data.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    package_data
}

fn parse_stack_yaml(yaml: &YamlValue) -> PackageData {
    let mut package_data = default_package_data(DatasourceId::HackageStackYaml);
    let Some(mapping) = yaml.as_mapping() else {
        return package_data;
    };

    let mut dependencies = Vec::new();
    let mut extra_data = HashMap::new();

    if let Some(resolver) = mapping_get(mapping, "resolver")
        && let Ok(value) = serde_json::to_value(resolver)
    {
        extra_data.insert("resolver".to_string(), value);
    }

    if let Some(snapshot) = mapping_get(mapping, "snapshot")
        && let Ok(value) = serde_json::to_value(snapshot)
    {
        extra_data.insert("snapshot".to_string(), value);
    }

    if let Some(packages) = mapping_get(mapping, "packages") {
        dependencies.extend(parse_stack_package_entries(packages));
    }

    if let Some(extra_deps) = mapping_get(mapping, "extra-deps") {
        dependencies.extend(parse_stack_extra_dep_entries(extra_deps));
    }

    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };

        if matches!(key, "resolver" | "snapshot" | "packages" | "extra-deps") {
            continue;
        }

        if let Ok(json_value) = serde_json::to_value(value) {
            extra_data.insert(key.to_string(), json_value);
        }
    }

    package_data.dependencies = dependencies;
    package_data.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    package_data
}

fn parse_cabal_data(content: &str) -> CabalData {
    let mut data = CabalData::default();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_component: Option<ComponentContext> = None;
    let mut in_source_repository = false;
    let mut index = 0;

    while index < lines.len() {
        let cleaned = strip_cabal_comment(lines[index]);
        let trimmed = cleaned.trim();
        let indent = indentation(cleaned);

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        if indent == 0 && !trimmed.contains(':') {
            current_component = parse_component_header(trimmed);
            in_source_repository = trimmed.starts_with("source-repository");
            index += 1;
            continue;
        }

        let Some((key, value, next_index)) = collect_indented_field(&lines, index) else {
            index += 1;
            continue;
        };

        match key.as_str() {
            "name" if indent == 0 => data.name = clean_single_line(&value),
            "version" if indent == 0 => data.version = clean_single_line(&value),
            "synopsis" if indent == 0 => data.synopsis = clean_single_line(&value),
            "description" if indent == 0 => {
                data.description = normalize_cabal_multiline(&value);
            }
            "license" if indent == 0 => data.license = clean_single_line(&value),
            "homepage" if indent == 0 => data.homepage_url = clean_single_line(&value),
            "bug-reports" if indent == 0 => data.bug_tracking_url = clean_single_line(&value),
            "author" if indent == 0 => data.authors.extend(split_comma_separated(&value)),
            "maintainer" if indent == 0 => {
                data.maintainers.extend(split_comma_separated(&value));
            }
            "category" if indent == 0 => {
                data.category_keywords.extend(split_keywords(&value));
            }
            "keywords" if indent == 0 => {
                data.explicit_keywords.extend(split_keywords(&value));
            }
            "location" if in_source_repository && data.vcs_url.is_none() => {
                data.vcs_url = clean_single_line(&value);
            }
            "build-depends" => {
                data.dependencies
                    .extend(parse_build_depends(&value, current_component.as_ref()));
            }
            _ => {}
        }

        index = next_index + 1;
    }

    data
}

fn parse_build_depends(value: &str, component: Option<&ComponentContext>) -> Vec<Dependency> {
    if component.is_some_and(|component| component.component_type == "common") {
        return Vec::new();
    }

    split_dependency_entries(value)
        .into_iter()
        .filter_map(|entry| {
            parse_hackage_spec_dependency(&entry, Some("build-depends"), component, None)
        })
        .collect()
}

fn parse_path_like_entries(value: &str, scope: &str, optional: bool) -> Vec<Dependency> {
    split_multiline_entries(value)
        .into_iter()
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let mut extra_data = HashMap::new();
            extra_data.insert("path".to_string(), JsonValue::String(entry.clone()));

            Dependency {
                purl: None,
                extracted_requirement: Some(entry),
                scope: Some(scope.to_string()),
                is_runtime: None,
                is_optional: Some(optional),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: Some(extra_data),
            }
        })
        .collect()
}

fn parse_import_entries(value: &str) -> Vec<Dependency> {
    split_multiline_entries(value)
        .into_iter()
        .filter(|entry| !entry.is_empty())
        .map(|entry| Dependency {
            purl: None,
            extracted_requirement: Some(entry),
            scope: Some("import".to_string()),
            is_runtime: None,
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        })
        .collect()
}

fn parse_hackage_spec_entries(
    value: &str,
    scope: &str,
    is_runtime: Option<bool>,
) -> Vec<Dependency> {
    split_multiline_entries(value)
        .into_iter()
        .filter_map(|entry| parse_hackage_spec_dependency(&entry, Some(scope), None, is_runtime))
        .collect()
}

fn parse_stack_package_entries(value: &YamlValue) -> Vec<Dependency> {
    let Some(sequence) = value.as_sequence() else {
        return Vec::new();
    };

    sequence
        .iter()
        .filter_map(|entry| match entry {
            YamlValue::String(path) => {
                let mut extra_data = HashMap::new();
                extra_data.insert("path".to_string(), JsonValue::String(path.clone()));

                Some(Dependency {
                    purl: None,
                    extracted_requirement: Some(path.clone()),
                    scope: Some("packages".to_string()),
                    is_runtime: None,
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: Some(extra_data),
                })
            }
            YamlValue::Mapping(map) => {
                let extracted_requirement = mapping_string(map, "location")
                    .or_else(|| mapping_string(map, "git"))
                    .or_else(|| mapping_string(map, "url"));
                let extra_data = serde_json::to_value(entry)
                    .ok()
                    .and_then(|value| value.as_object().cloned())
                    .map(|map| map.into_iter().collect::<HashMap<_, _>>());

                Some(Dependency {
                    purl: None,
                    extracted_requirement,
                    scope: Some("packages".to_string()),
                    is_runtime: None,
                    is_optional: Some(false),
                    is_pinned: Some(mapping_string(map, "commit").is_some()),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data,
                })
            }
            _ => None,
        })
        .collect()
}

fn parse_stack_extra_dep_entries(value: &YamlValue) -> Vec<Dependency> {
    let Some(sequence) = value.as_sequence() else {
        return Vec::new();
    };

    sequence
        .iter()
        .filter_map(|entry| match entry {
            YamlValue::String(spec) => parse_stack_extra_dep_string(spec),
            YamlValue::Mapping(map) => Some(parse_stack_extra_dep_mapping(map, entry)),
            _ => None,
        })
        .collect()
}

fn parse_stack_extra_dep_string(spec: &str) -> Option<Dependency> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (package_spec, pantry_suffix) = trimmed
        .split_once('@')
        .map_or((trimmed, None), |(package_spec, suffix)| {
            (package_spec, Some(suffix))
        });

    let mut dependency =
        parse_hackage_spec_dependency(package_spec, Some("extra-deps"), None, None).unwrap_or(
            Dependency {
                purl: None,
                extracted_requirement: Some(package_spec.to_string()),
                scope: Some("extra-deps".to_string()),
                is_runtime: None,
                is_optional: Some(false),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            },
        );

    if let Some(suffix) = pantry_suffix {
        let mut extra_data = dependency.extra_data.take().unwrap_or_default();
        extra_data.insert("pantry".to_string(), JsonValue::String(suffix.to_string()));
        dependency.extra_data = Some(extra_data);
        dependency.is_pinned = Some(true);
        if dependency.extracted_requirement.is_none() {
            dependency.extracted_requirement = Some(package_spec.to_string());
        }
    }

    dependency.scope = Some("extra-deps".to_string());
    Some(dependency)
}

fn parse_stack_extra_dep_mapping(map: &Mapping, raw_value: &YamlValue) -> Dependency {
    let name = mapping_string(map, "name");
    let version = mapping_string(map, "version");
    let purl = build_hackage_purl(name.as_deref(), version.as_deref());
    let extracted_requirement = version
        .clone()
        .or_else(|| mapping_string(map, "git"))
        .or_else(|| mapping_string(map, "url"));
    let extra_data = serde_json::to_value(raw_value)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|map| map.into_iter().collect::<HashMap<_, _>>());

    Dependency {
        purl,
        extracted_requirement,
        scope: Some("extra-deps".to_string()),
        is_runtime: None,
        is_optional: Some(false),
        is_pinned: Some(version.is_some() || mapping_string(map, "commit").is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data,
    }
}

fn build_source_repository_dependency(extra_data: HashMap<String, JsonValue>) -> Dependency {
    let extracted_requirement = extra_data
        .get("location")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .or_else(|| {
            extra_data
                .get("tag")
                .and_then(JsonValue::as_str)
                .map(str::to_string)
        });

    Dependency {
        purl: None,
        extracted_requirement,
        scope: Some("source-repository-package".to_string()),
        is_runtime: None,
        is_optional: Some(false),
        is_pinned: Some(
            extra_data.contains_key("tag")
                || extra_data.contains_key("commit")
                || extra_data.contains_key("sha256"),
        ),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: Some(extra_data),
    }
}

fn parse_hackage_spec_dependency(
    spec: &str,
    scope: Option<&str>,
    component: Option<&ComponentContext>,
    is_runtime: Option<bool>,
) -> Option<Dependency> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }

    let can_split_name_version = matches!(scope, Some("extra-packages" | "extra-deps"));

    if can_split_name_version && let Some((name, version)) = split_hackage_name_version(trimmed) {
        let mut extra_data = HashMap::new();
        if let Some(component) = component {
            extra_data.insert(
                "component_type".to_string(),
                JsonValue::String(component.component_type.clone()),
            );
            if let Some(component_name) = &component.component_name {
                extra_data.insert(
                    "component_name".to_string(),
                    JsonValue::String(component_name.clone()),
                );
            }
        }

        return Some(Dependency {
            purl: Some(format!("pkg:hackage/{}@{}", name, version)),
            extracted_requirement: Some(version),
            scope: scope.map(str::to_string),
            is_runtime: component.map(component_is_runtime).or(is_runtime),
            is_optional: Some(false),
            is_pinned: Some(true),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: (!extra_data.is_empty()).then_some(extra_data),
        });
    }

    let name_re = Regex::new(r"^(?P<name>[A-Za-z0-9][A-Za-z0-9_\.-]*)").ok()?;
    let captures = name_re.captures(trimmed)?;
    let name = captures.name("name")?.as_str().to_string();
    let requirement = trimmed[name.len()..].trim();
    let implicit_name_version = if can_split_name_version && requirement.is_empty() {
        split_hackage_name_version(trimmed)
    } else {
        None
    };
    let resolved_name = implicit_name_version
        .as_ref()
        .map(|(resolved_name, _)| resolved_name.as_str())
        .unwrap_or(name.as_str());
    let exact_version = exact_version_requirement(requirement).or_else(|| {
        implicit_name_version
            .as_ref()
            .map(|(_, version)| version.clone())
    });
    let purl = if let Some(version) = exact_version.as_deref() {
        Some(format!("pkg:hackage/{}@{}", resolved_name, version))
    } else {
        Some(format!("pkg:hackage/{}", resolved_name))
    };

    let mut extra_data = HashMap::new();
    if let Some(component) = component {
        extra_data.insert(
            "component_type".to_string(),
            JsonValue::String(component.component_type.clone()),
        );
        if let Some(component_name) = &component.component_name {
            extra_data.insert(
                "component_name".to_string(),
                JsonValue::String(component_name.clone()),
            );
        }
    }

    let extracted_requirement = if let Some((_, version)) = implicit_name_version {
        Some(version)
    } else {
        (!requirement.is_empty()).then_some(requirement.to_string())
    };

    Some(Dependency {
        purl,
        extracted_requirement,
        scope: scope.map(str::to_string),
        is_runtime: component.map(component_is_runtime).or(is_runtime),
        is_optional: Some(false),
        is_pinned: Some(exact_version.is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn component_is_runtime(component: &ComponentContext) -> bool {
    !matches!(
        component.component_type.as_str(),
        "test-suite" | "benchmark"
    )
}

fn parse_component_header(trimmed: &str) -> Option<ComponentContext> {
    const COMPONENT_PREFIXES: &[&str] = &[
        "library",
        "foreign-library",
        "executable",
        "test-suite",
        "benchmark",
        "common",
    ];

    COMPONENT_PREFIXES.iter().find_map(|prefix| {
        trimmed
            .strip_prefix(prefix)
            .map(|remainder| ComponentContext {
                component_type: (*prefix).to_string(),
                component_name: clean_single_line(remainder),
            })
    })
}

fn collect_indented_field(lines: &[&str], start_index: usize) -> Option<(String, String, usize)> {
    let current = strip_cabal_comment(lines[start_index]);
    let trimmed = current.trim();
    let indent = indentation(current);
    let colon_index = trimmed.find(':')?;
    let key = trimmed[..colon_index].trim().to_ascii_lowercase();
    let mut values = vec![trimmed[colon_index + 1..].trim().to_string()];
    let mut last_index = start_index;

    for (next_index, line) in lines.iter().enumerate().skip(start_index + 1) {
        let next = strip_cabal_comment(line);
        let next_trimmed = next.trim();
        if next_trimmed.is_empty() {
            break;
        }

        if indentation(next) <= indent {
            break;
        }

        values.push(next_trimmed.to_string());
        last_index = next_index;
    }

    Some((key, values.join("\n"), last_index))
}

fn split_dependency_entries(value: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut bracket_depth = 0usize;

    for character in value.chars() {
        match character {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    entries.push(trimmed.to_string());
                }
                current.clear();
                continue;
            }
            _ => {}
        }

        current.push(character);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        entries.push(trimmed.to_string());
    }

    entries
}

fn split_multiline_entries(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.strip_prefix("-").unwrap_or(line).trim().to_string())
        .collect()
}

fn parse_multiline_scalar_or_list(value: &str) -> JsonValue {
    let entries = split_multiline_entries(value);
    if entries.len() <= 1 {
        clean_single_line(value)
            .map(JsonValue::String)
            .unwrap_or(JsonValue::Null)
    } else {
        JsonValue::Array(entries.into_iter().map(JsonValue::String).collect())
    }
}

fn normalize_cabal_multiline(value: &str) -> Option<String> {
    let lines: Vec<String> = value
        .lines()
        .map(str::trim)
        .map(|line| {
            if line == "." {
                "".to_string()
            } else {
                line.to_string()
            }
        })
        .collect();

    let combined = lines.join("\n").trim().to_string();
    (!combined.is_empty()).then_some(combined)
}

fn clean_single_line(value: &str) -> Option<String> {
    let cleaned = value.trim();
    (!cleaned.is_empty()).then_some(cleaned.to_string())
}

fn split_comma_separated(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn split_keywords(value: &str) -> Vec<String> {
    split_comma_separated(value)
}

fn merge_keywords(categories: &[String], keywords: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    categories
        .iter()
        .chain(keywords.iter())
        .filter_map(|keyword| {
            let normalized = keyword.trim();
            if normalized.is_empty() || !seen.insert(normalized.to_ascii_lowercase()) {
                None
            } else {
                Some(normalized.to_string())
            }
        })
        .collect()
}

fn combine_summary_and_description(
    synopsis: &Option<String>,
    description: &Option<String>,
) -> Option<String> {
    match (synopsis, description) {
        (Some(synopsis), Some(description)) if synopsis == description => Some(synopsis.clone()),
        (Some(synopsis), Some(description)) => Some(format!("{}\n\n{}", synopsis, description)),
        (Some(synopsis), None) => Some(synopsis.clone()),
        (None, Some(description)) => Some(description.clone()),
        (None, None) => None,
    }
}

fn build_parties(authors: &[String], maintainers: &[String]) -> Vec<Party> {
    let author_parties = authors
        .iter()
        .filter_map(|author| build_party(author, "author"));
    let maintainer_parties = maintainers
        .iter()
        .filter_map(|maintainer| build_party(maintainer, "maintainer"));

    author_parties.chain(maintainer_parties).collect()
}

fn build_party(value: &str, role: &str) -> Option<Party> {
    let (name, email) = split_name_email(value.trim());
    if name.is_none() && email.is_none() {
        return None;
    }

    Some(Party {
        r#type: Some("person".to_string()),
        role: Some(role.to_string()),
        name,
        email,
        url: None,
        organization: None,
        organization_url: None,
        timezone: None,
    })
}

fn build_hackage_purl(name: Option<&str>, version: Option<&str>) -> Option<String> {
    match (name, version) {
        (Some(name), Some(version)) => Some(format!("pkg:hackage/{}@{}", name, version)),
        (Some(name), None) => Some(format!("pkg:hackage/{}", name)),
        _ => None,
    }
}

fn split_hackage_name_version(spec: &str) -> Option<(String, String)> {
    if spec.chars().any(|character| {
        character.is_whitespace() || matches!(character, '<' | '>' | '=' | '&' | '|' | '(' | ')')
    }) {
        return None;
    }

    for (index, character) in spec.char_indices().rev() {
        if character != '-' {
            continue;
        }

        let name = &spec[..index];
        let version = &spec[index + 1..];

        if name.is_empty()
            || version.is_empty()
            || !version
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
        {
            continue;
        }

        return Some((name.to_string(), version.to_string()));
    }

    None
}

fn exact_version_requirement(requirement: &str) -> Option<String> {
    let trimmed = requirement.trim();
    if trimmed.is_empty() {
        return None;
    }

    let exact_re = Regex::new(r"^==\s*([A-Za-z0-9][A-Za-z0-9\.\-_+]*)$").ok()?;
    exact_re.captures(trimmed).and_then(|captures| {
        let version = captures.get(1)?.as_str();
        (!version.contains('*')).then_some(version.to_string())
    })
}

fn project_extra_key(key: &str) -> String {
    key.replace('-', "_")
}

fn strip_cabal_comment(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("--") {
        return "";
    }

    let bytes = line.as_bytes();
    for index in 0..bytes.len().saturating_sub(1) {
        if bytes[index] == b'-'
            && bytes[index + 1] == b'-'
            && (index == 0 || bytes[index - 1].is_ascii_whitespace())
        {
            return line[..index].trim_end();
        }
    }

    line
}

fn indentation(line: &str) -> usize {
    line.chars()
        .take_while(|character| character.is_whitespace())
        .count()
}

fn mapping_get<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a YamlValue> {
    mapping.get(YamlValue::String(key.to_string()))
}

fn mapping_string(mapping: &Mapping, key: &str) -> Option<String> {
    mapping_get(mapping, key)
        .and_then(YamlValue::as_str)
        .map(str::to_string)
}

crate::register_parser!(
    "Hackage Cabal package manifest",
    &["**/*.cabal"],
    "hackage",
    "Haskell",
    Some("https://cabal.readthedocs.io/en/stable/cabal-package-description-file.html"),
);

crate::register_parser!(
    "Hackage cabal.project workspace file",
    &["**/cabal.project"],
    "hackage",
    "Haskell",
    Some("https://cabal.readthedocs.io/en/stable/cabal-project-description-file.html"),
);

crate::register_parser!(
    "Hackage Stack project manifest",
    &["**/stack.yaml"],
    "hackage",
    "Haskell",
    Some("https://docs.haskellstack.org/en/stable/configure/yaml/"),
);
