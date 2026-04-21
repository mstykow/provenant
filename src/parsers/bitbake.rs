use std::collections::HashMap;
use std::path::Path;

use crate::models::{
    DatasourceId, Dependency, FileReference, Md5Digest, PackageData, PackageType, Sha1Digest,
    Sha256Digest, Sha512Digest,
};
use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value;

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;
use super::utils::{read_file_to_string, truncate_field};

pub struct BitbakeRecipeParser;

impl PackageParser for BitbakeRecipeParser {
    const PACKAGE_TYPE: PackageType = PackageType::Bitbake;

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "bb" | "bbappend"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let datasource_id = datasource_id_for_path(path);
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read BitBake recipe at {:?}: {}", path, error);
                return vec![default_package_data(datasource_id)];
            }
        };

        vec![parse_recipe(&content, path, datasource_id)]
    }
}

fn datasource_id_for_path(path: &Path) -> DatasourceId {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("bbappend") => DatasourceId::BitbakeRecipeAppend,
        _ => DatasourceId::BitbakeRecipe,
    }
}

fn parse_recipe(content: &str, path: &Path, datasource_id: DatasourceId) -> PackageData {
    let vars = extract_variables(content);
    let (filename_name, filename_version) = parse_recipe_filename(path);

    let mut package = default_package_data(datasource_id);
    let mut extra_data: HashMap<String, Value> = HashMap::new();

    let name = vars
        .get("PN")
        .cloned()
        .or(filename_name)
        .map(truncate_field);
    let version = vars
        .get("PV")
        .cloned()
        .or(filename_version)
        .map(truncate_field);

    package.name = name.clone();
    package.version = version.clone();

    if let Some(summary) = vars.get("SUMMARY") {
        package.description = Some(truncate_field(summary.clone()));
    } else if let Some(description) = vars.get("DESCRIPTION") {
        package.description = Some(truncate_field(description.clone()));
    }

    if let Some(homepage) = vars.get("HOMEPAGE") {
        package.homepage_url = Some(truncate_field(homepage.clone()));
    }

    if let Some(bugtracker) = vars.get("BUGTRACKER") {
        package.bug_tracking_url = Some(truncate_field(bugtracker.clone()));
    }

    if let Some(license) = select_license_value(&vars, name.as_deref()) {
        package.extracted_license_statement = Some(truncate_field(license.clone()));

        let normalized = normalize_bitbake_license(&license);
        let (declared, spdx, detections) =
            normalize_spdx_declared_license(Some(normalized.as_str()));
        package.declared_license_expression = declared;
        package.declared_license_expression_spdx = spdx;
        package.license_detections = detections;
    }

    if let Some(section) = vars.get("SECTION") {
        extra_data.insert("section".to_string(), Value::String(section.clone()));
    }

    let mut file_references = Vec::new();
    if let Some(lic_files) = vars.get("LIC_FILES_CHKSUM") {
        merge_file_references(
            &mut file_references,
            extract_lic_files_chksum_references(lic_files),
        );
    }

    if let Some(src_uri) = vars.get("SRC_URI") {
        let (remote_entries, local_references) = extract_src_uri_data(src_uri);
        let uris: Vec<String> = remote_entries
            .iter()
            .map(|entry| entry.uri.clone())
            .collect();
        if !uris.is_empty() {
            extra_data.insert(
                "src_uri".to_string(),
                Value::Array(uris.into_iter().map(Value::String).collect()),
            );
        }
        merge_file_references(&mut file_references, local_references);
        apply_src_uri_package_metadata(&mut package, &vars, &remote_entries);
    }

    let inherits = extract_inherits(content);
    if !inherits.is_empty() {
        extra_data.insert(
            "inherit".to_string(),
            Value::Array(inherits.into_iter().map(Value::String).collect()),
        );
    }

    let mut dependencies = Vec::new();

    if let Some(depends) = vars.get("DEPENDS") {
        dependencies.extend(
            parse_dependency_list(depends)
                .into_iter()
                .map(|dependency| Dependency {
                    purl: build_dependency_purl(&dependency.name),
                    extracted_requirement: dependency.requirement,
                    scope: Some("build".to_string()),
                    is_runtime: Some(false),
                    is_optional: None,
                    is_pinned: None,
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }),
        );
    }

    for (key, value) in &vars {
        if is_rdepends_key(key) {
            dependencies.extend(parse_dependency_list(value).into_iter().map(|dependency| {
                Dependency {
                    purl: build_dependency_purl(&dependency.name),
                    extracted_requirement: dependency.requirement,
                    scope: Some("runtime".to_string()),
                    is_runtime: Some(true),
                    is_optional: None,
                    is_pinned: None,
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }
            }));
        }
    }

    package.dependencies = dependencies;
    package.file_references = file_references;
    package.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    package.purl = name
        .as_deref()
        .and_then(|n| build_package_purl(n, version.as_deref()));

    package
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PackageType::Bitbake),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

fn parse_recipe_filename(path: &Path) -> (Option<String>, Option<String>) {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return (None, None),
    };

    match stem.split_once('_') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            let version = (!version.contains('%')).then_some(version.to_string());
            (Some(name.to_string()), version)
        }
        _ => {
            let trimmed_stem = stem.trim_end_matches('%');
            let name = if trimmed_stem.is_empty() {
                stem.to_string()
            } else {
                trimmed_stem.to_string()
            };
            (Some(name), None)
        }
    }
}

fn select_license_value(
    vars: &HashMap<String, String>,
    package_name: Option<&str>,
) -> Option<String> {
    let mut candidate_keys = Vec::new();

    if let Some(package_name) = package_name {
        candidate_keys.push(format!("LICENSE:{package_name}"));
        candidate_keys.push(format!("LICENSE_{package_name}"));
    }

    candidate_keys.extend([
        "LICENSE:${PN}".to_string(),
        "LICENSE_${PN}".to_string(),
        "LICENSE".to_string(),
    ]);

    candidate_keys
        .into_iter()
        .find_map(|candidate| vars.get(&candidate).cloned())
}

fn apply_src_uri_package_metadata(
    package: &mut PackageData,
    vars: &HashMap<String, String>,
    remote_entries: &[SrcUriEntry],
) {
    if remote_entries.len() != 1 {
        return;
    }

    let entry = &remote_entries[0];
    package.download_url = Some(entry.uri.clone());
    package.sha1 = parse_sha1_digest(
        entry
            .sha1sum
            .as_deref()
            .or_else(|| src_uri_varflag_value(vars, entry.name.as_deref(), "sha1sum")),
    );
    package.md5 = parse_md5_digest(
        entry
            .md5sum
            .as_deref()
            .or_else(|| src_uri_varflag_value(vars, entry.name.as_deref(), "md5sum")),
    );
    package.sha256 = parse_sha256_digest(
        entry
            .sha256sum
            .as_deref()
            .or_else(|| src_uri_varflag_value(vars, entry.name.as_deref(), "sha256sum")),
    );
    package.sha512 = parse_sha512_digest(
        entry
            .sha512sum
            .as_deref()
            .or_else(|| src_uri_varflag_value(vars, entry.name.as_deref(), "sha512sum")),
    );
}

fn src_uri_varflag_value<'a>(
    vars: &'a HashMap<String, String>,
    name: Option<&str>,
    algorithm: &str,
) -> Option<&'a str> {
    name.and_then(|name| vars.get(&format!("SRC_URI[{name}.{algorithm}]")))
        .or_else(|| vars.get(&format!("SRC_URI[{algorithm}]")))
        .map(String::as_str)
}

fn parse_sha1_digest(value: Option<&str>) -> Option<Sha1Digest> {
    value.and_then(|value| Sha1Digest::from_hex(value).ok())
}

fn parse_md5_digest(value: Option<&str>) -> Option<Md5Digest> {
    value.and_then(|value| Md5Digest::from_hex(value).ok())
}

fn parse_sha256_digest(value: Option<&str>) -> Option<Sha256Digest> {
    value.and_then(|value| Sha256Digest::from_hex(value).ok())
}

fn parse_sha512_digest(value: Option<&str>) -> Option<Sha512Digest> {
    value.and_then(|value| Sha512Digest::from_hex(value).ok())
}

#[derive(Default)]
struct OverrideMutations {
    appends: Vec<String>,
    prepends: Vec<String>,
    removes: Vec<String>,
}

fn extract_variables(content: &str) -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = HashMap::new();
    let mut override_mutations: HashMap<String, OverrideMutations> = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut full_line = trimmed.to_string();
        while full_line.ends_with('\\') {
            full_line.truncate(full_line.len() - 1);
            if let Some(next) = lines.next() {
                full_line.push(' ');
                full_line.push_str(next.trim());
            } else {
                break;
            }
        }

        if let Some((var_name, value, op)) = parse_assignment(&full_line) {
            let cleaned = strip_quotes(&value);
            match op {
                AssignOp::Set | AssignOp::Immediate => {
                    vars.insert(var_name, cleaned);
                }
                AssignOp::WeakSet | AssignOp::WeakDefault => {
                    vars.entry(var_name).or_insert(cleaned);
                }
                AssignOp::Append => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| {
                            v.push(' ');
                            v.push_str(&cleaned);
                        })
                        .or_insert(cleaned);
                }
                AssignOp::Prepend => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| {
                            let mut new = cleaned.clone();
                            new.push(' ');
                            new.push_str(v);
                            *v = new;
                        })
                        .or_insert(cleaned);
                }
                AssignOp::AppendNoSpace => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| v.push_str(&cleaned))
                        .or_insert(cleaned);
                }
                AssignOp::PrependNoSpace => {
                    vars.entry(var_name.clone())
                        .and_modify(|v| {
                            let mut new = cleaned.clone();
                            new.push_str(v);
                            *v = new;
                        })
                        .or_insert(cleaned);
                }
                AssignOp::OverrideAppend => {
                    override_mutations
                        .entry(var_name)
                        .or_default()
                        .appends
                        .push(cleaned);
                }
                AssignOp::OverridePrepend => {
                    override_mutations
                        .entry(var_name)
                        .or_default()
                        .prepends
                        .push(cleaned);
                }
                AssignOp::OverrideRemove => {
                    override_mutations
                        .entry(var_name)
                        .or_default()
                        .removes
                        .push(cleaned);
                }
            }
        }
    }

    apply_override_mutations(&mut vars, override_mutations);

    vars
}

fn apply_override_mutations(
    vars: &mut HashMap<String, String>,
    override_mutations: HashMap<String, OverrideMutations>,
) {
    for (var_name, mutations) in override_mutations {
        let value = vars.entry(var_name).or_default();

        for append in mutations.appends {
            value.push_str(&append);
        }

        if !mutations.prepends.is_empty() {
            let mut prefix = String::new();
            for prepend in mutations.prepends {
                prefix.push_str(&prepend);
            }
            value.insert_str(0, &prefix);
        }

        for remove in mutations.removes {
            *value = remove_override_tokens(value, &remove);
        }
    }
}

fn remove_override_tokens(current: &str, remove: &str) -> String {
    let removal_tokens: Vec<&str> = remove.split_whitespace().collect();
    if removal_tokens.is_empty() {
        return current.to_string();
    }

    current
        .split_whitespace()
        .filter(|token| !removal_tokens.contains(token))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AssignOp {
    Set,
    Immediate,
    WeakSet,
    WeakDefault,
    Append,
    Prepend,
    AppendNoSpace,
    PrependNoSpace,
    OverrideAppend,
    OverridePrepend,
    OverrideRemove,
}

fn parse_assignment(line: &str) -> Option<(String, String, AssignOp)> {
    let operators: &[(&str, AssignOp)] = &[
        ("??=", AssignOp::WeakDefault),
        ("?=", AssignOp::WeakSet),
        (":=", AssignOp::Immediate),
        ("+=", AssignOp::Append),
        ("=+", AssignOp::Prepend),
        (".=", AssignOp::AppendNoSpace),
        ("=.", AssignOp::PrependNoSpace),
        ("=", AssignOp::Set),
    ];

    for (op_str, op) in operators {
        if let Some(pos) = line.find(op_str) {
            let raw_var_name = line[..pos].trim();
            if raw_var_name.is_empty() || !is_valid_var_name(raw_var_name) {
                continue;
            }

            let (var_name, op) = parse_override_var_name(raw_var_name)
                .unwrap_or_else(|| (raw_var_name.to_string(), *op));
            let value = line[pos + op_str.len()..].trim().to_string();

            return Some((var_name, value, op));
        }
    }

    None
}

fn parse_override_var_name(var_name: &str) -> Option<(String, AssignOp)> {
    let colon_segments: Vec<&str> = var_name.split(':').collect();
    if colon_segments.len() > 1 {
        for (index, segment) in colon_segments.iter().enumerate() {
            let op = match *segment {
                "append" => AssignOp::OverrideAppend,
                "prepend" => AssignOp::OverridePrepend,
                "remove" => AssignOp::OverrideRemove,
                _ => continue,
            };

            let canonical = colon_segments
                .iter()
                .enumerate()
                .filter_map(|(current, segment)| (current != index).then_some(*segment))
                .collect::<Vec<_>>()
                .join(":");

            return Some((canonical, op));
        }
    }

    for (suffix, op) in [
        ("_append", AssignOp::OverrideAppend),
        ("_prepend", AssignOp::OverridePrepend),
        ("_remove", AssignOp::OverrideRemove),
    ] {
        if let Some(base) = var_name.strip_suffix(suffix) {
            return Some((base.to_string(), op));
        }
    }

    None
}

fn is_valid_var_name(s: &str) -> bool {
    let base = s.split([':', '[']).next().unwrap_or(s);
    !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '{' || c == '}')
}

fn strip_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn extract_inherits(content: &str) -> Vec<String> {
    let mut inherits = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("inherit ") {
            for class in rest.split_whitespace() {
                if !class.starts_with('#') {
                    inherits.push(class.to_string());
                } else {
                    break;
                }
            }
        }
    }
    inherits
}

fn is_rdepends_key(key: &str) -> bool {
    key == "RDEPENDS"
        || key.starts_with("RDEPENDS:")
        || key.starts_with("RDEPENDS_")
        || key.starts_with("RDEPENDS[")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDependency {
    name: String,
    requirement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SrcUriEntry {
    uri: String,
    name: Option<String>,
    sha1sum: Option<String>,
    md5sum: Option<String>,
    sha256sum: Option<String>,
    sha512sum: Option<String>,
}

fn parse_dependency_list(value: &str) -> Vec<ParsedDependency> {
    let cleaned_value = strip_bitbake_expansions(value);
    let tokens: Vec<&str> = cleaned_value.split_whitespace().collect();
    let mut dependencies = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = tokens[index];
        let Some(name) = normalize_dependency_name_token(token) else {
            index += 1;
            continue;
        };

        let mut requirement = None;

        if tokens
            .get(index + 1)
            .is_some_and(|next| next.starts_with('('))
        {
            let mut pieces = Vec::new();
            index += 1;

            while index < tokens.len() {
                let piece = tokens[index];
                pieces.push(piece);
                if piece.ends_with(')') {
                    break;
                }
                index += 1;
            }

            let joined = pieces.join(" ");
            let cleaned = joined
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim()
                .to_string();
            if !cleaned.is_empty() {
                requirement = Some(cleaned);
            }
        }

        dependencies.push(ParsedDependency { name, requirement });
        index += 1;
    }

    dependencies
}

fn strip_bitbake_expansions(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            index += 2;
            let mut depth = 1;
            while index < chars.len() && depth > 0 {
                match chars[index] {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                index += 1;
            }
            result.push(' ');
            continue;
        }

        result.push(chars[index]);
        index += 1;
    }

    result
}

fn normalize_dependency_name_token(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|c| matches!(c, '"' | '\'' | ','));
    if trimmed.is_empty() || trimmed.contains('$') {
        return None;
    }

    let first = trimmed.chars().next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }

    if trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '.' | '/'))
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn extract_src_uri_data(src_uri: &str) -> (Vec<SrcUriEntry>, Vec<FileReference>) {
    let mut remote_entries = Vec::new();
    let mut local_references = Vec::new();

    for entry in src_uri.split_whitespace() {
        if entry.is_empty() {
            continue;
        }

        let mut parts = entry.split(';');
        let base = parts.next().unwrap_or(entry);

        let mut remote_entry = SrcUriEntry {
            uri: truncate_field(base.to_string()),
            name: None,
            sha1sum: None,
            md5sum: None,
            sha256sum: None,
            sha512sum: None,
        };

        for parameter in parts {
            let Some((key, value)) = parameter.split_once('=') else {
                continue;
            };

            match key {
                "name" => remote_entry.name = Some(value.to_string()),
                "sha1sum" => remote_entry.sha1sum = Some(value.to_string()),
                "md5sum" => remote_entry.md5sum = Some(value.to_string()),
                "sha256sum" => remote_entry.sha256sum = Some(value.to_string()),
                "sha512sum" => remote_entry.sha512sum = Some(value.to_string()),
                _ => {}
            }
        }

        if let Some(path) = base.strip_prefix("file://") {
            if !path.is_empty() {
                local_references.push(file_reference_from_path(path, "SRC_URI"));
            }
            continue;
        }

        remote_entries.push(remote_entry);
    }

    (remote_entries, local_references)
}

fn extract_lic_files_chksum_references(value: &str) -> Vec<FileReference> {
    let mut references = Vec::new();

    for entry in value.split_whitespace() {
        let Some(path) = entry
            .split(';')
            .next()
            .and_then(|item| item.strip_prefix("file://"))
        else {
            continue;
        };

        if path.is_empty() {
            continue;
        }

        let mut reference = file_reference_from_path(path, "LIC_FILES_CHKSUM");
        let mut extra_data = reference.extra_data.take().unwrap_or_default();

        for parameter in entry.split(';').skip(1) {
            let Some((key, raw_value)) = parameter.split_once('=') else {
                continue;
            };

            match key {
                "md5" => {
                    reference.md5 = Md5Digest::from_hex(raw_value).ok();
                }
                _ => {
                    extra_data.insert(key.to_string(), Value::String(raw_value.to_string()));
                }
            }
        }

        reference.extra_data = (!extra_data.is_empty()).then_some(extra_data);
        references.push(reference);
    }

    references
}

fn file_reference_from_path(path: &str, source_variable: &str) -> FileReference {
    let mut reference = FileReference::from_path(truncate_field(path.to_string()));
    let mut extra_data = HashMap::new();
    extra_data.insert(
        "source_variable".to_string(),
        Value::String(source_variable.to_string()),
    );
    reference.extra_data = Some(extra_data);
    reference
}

fn merge_file_references(target: &mut Vec<FileReference>, additions: Vec<FileReference>) {
    for addition in additions {
        if let Some(existing) = target
            .iter_mut()
            .find(|reference| reference.path == addition.path)
        {
            if existing.md5.is_none() {
                existing.md5 = addition.md5;
            }
            if existing.sha1.is_none() {
                existing.sha1 = addition.sha1;
            }
            if existing.sha256.is_none() {
                existing.sha256 = addition.sha256;
            }
            if existing.sha512.is_none() {
                existing.sha512 = addition.sha512;
            }
            if existing.extra_data.is_none() {
                existing.extra_data = addition.extra_data;
            } else if let (Some(existing_extra), Some(addition_extra)) =
                (&mut existing.extra_data, addition.extra_data)
            {
                existing_extra.extend(addition_extra);
            }
            continue;
        }

        target.push(addition);
    }
}

fn normalize_bitbake_license(license: &str) -> String {
    let mut result = String::with_capacity(license.len());
    let mut chars = license.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let trimmed = result.trim_end();
            result.truncate(trimmed.len());
            result.push_str(" AND ");
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        } else if ch == '|' {
            let trimmed = result.trim_end();
            result.truncate(trimmed.len());
            result.push_str(" OR ");
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn build_package_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Bitbake.as_str(), name).ok()?;
    if let Some(v) = version {
        purl.with_version(v).ok()?;
    }
    Some(truncate_field(purl.to_string()))
}

fn build_dependency_purl(name: &str) -> Option<String> {
    PackageUrl::new(PackageType::Bitbake.as_str(), name)
        .ok()
        .map(|purl| truncate_field(purl.to_string()))
}

crate::register_parser!(
    "Yocto BitBake recipe",
    &["**/*.bb"],
    "bitbake",
    "Shell",
    Some(
        "https://docs.yoctoproject.org/bitbake/bitbake-user-manual/bitbake-user-manual-metadata.html"
    ),
);

crate::register_parser!(
    "Yocto BitBake append file",
    &["**/*.bbappend"],
    "bitbake",
    "Shell",
    Some(
        "https://docs.yoctoproject.org/bitbake/bitbake-user-manual/bitbake-user-manual-metadata.html"
    ),
);
