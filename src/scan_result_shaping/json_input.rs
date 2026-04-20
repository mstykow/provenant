use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::assembly;
use crate::models::{
    DiagnosticSeverity, FileInfo, FileType, LicenseIndexProvenance, LicenseReference,
    LicenseRuleReference, Package, TopLevelDependency, TopLevelLicenseDetection,
};
use crate::output_schema::{
    OutputFileInfo, OutputLicenseReference, OutputLicenseRuleReference, OutputMatch, OutputPackage,
    OutputTopLevelDependency, OutputTopLevelLicenseDetection,
};
use crate::scanner::ProcessResult;

type JsonInputParts = (
    ProcessResult,
    assembly::AssemblyResult,
    Vec<TopLevelLicenseDetection>,
    Vec<LicenseReference>,
    Vec<LicenseRuleReference>,
    Vec<String>,
    Option<String>,
    Option<LicenseIndexProvenance>,
);

#[cfg(test)]
#[path = "json_input_test.rs"]
mod json_input_test;

#[derive(Deserialize, Clone)]
pub(crate) struct JsonHeaderExtraDataInput {
    #[serde(default)]
    spdx_license_list_version: Option<String>,
    #[serde(default)]
    license_index_provenance: Option<LicenseIndexProvenance>,
}

#[derive(Deserialize)]
pub(crate) struct JsonHeaderInput {
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default)]
    extra_data: Option<JsonHeaderExtraDataInput>,
}

#[derive(Deserialize)]
pub(crate) struct JsonScanInput {
    #[serde(default)]
    pub(crate) headers: Vec<JsonHeaderInput>,
    #[serde(default)]
    pub(crate) files: Vec<OutputFileInfo>,
    #[serde(default)]
    pub(crate) packages: Vec<OutputPackage>,
    #[serde(default)]
    pub(crate) dependencies: Vec<OutputTopLevelDependency>,
    #[serde(default)]
    pub(crate) license_detections: Vec<OutputTopLevelLicenseDetection>,
    #[serde(default)]
    pub(crate) license_references: Vec<OutputLicenseReference>,
    #[serde(default)]
    pub(crate) license_rule_references: Vec<OutputLicenseRuleReference>,
    #[serde(default)]
    pub(crate) excluded_count: usize,
}

impl JsonScanInput {
    pub(crate) fn directory_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.file_type == FileType::Directory)
            .count()
    }

    pub(crate) fn file_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .count()
    }

    pub(crate) fn file_size_count(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .map(|file| file.size)
            .sum()
    }

    pub(crate) fn into_parts(self) -> Result<JsonInputParts> {
        let imported_header_warnings: Vec<String> = self
            .headers
            .iter()
            .flat_map(|header| header.warnings.iter().cloned())
            .collect();
        let imported_header_errors: Vec<String> = self
            .headers
            .iter()
            .flat_map(|header| header.errors.iter().cloned())
            .collect();
        let _discarded_warning_count: usize = self
            .headers
            .iter()
            .map(|header| header.warnings.len())
            .sum();
        let mut files = self
            .files
            .iter()
            .map(FileInfo::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to convert file from JSON: {}", e))?;
        restore_imported_warning_severities(
            &mut files,
            &imported_header_warnings,
            &imported_header_errors,
        );
        let packages = self
            .packages
            .iter()
            .map(Package::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to convert package from JSON: {}", e))?;
        let dependencies = self
            .dependencies
            .iter()
            .map(TopLevelDependency::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to convert dependency from JSON: {}", e))?;
        let license_detections = self
            .license_detections
            .iter()
            .map(TopLevelLicenseDetection::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to convert license detection from JSON: {}", e))?;
        let license_references = self
            .license_references
            .iter()
            .map(LicenseReference::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to convert license reference from JSON: {}", e))?;
        let license_rule_references = self
            .license_rule_references
            .iter()
            .map(LicenseRuleReference::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to convert license rule reference from JSON: {}", e))?;

        let imported_spdx_license_list_version =
            consistent_header_value(self.headers.iter().filter_map(|header| {
                header
                    .extra_data
                    .as_ref()
                    .and_then(|extra_data| extra_data.spdx_license_list_version.clone())
            }));
        let imported_license_index_provenance =
            consistent_header_value(self.headers.iter().filter_map(|header| {
                header
                    .extra_data
                    .as_ref()
                    .and_then(|extra_data| extra_data.license_index_provenance.clone())
            }));

        let preserved_header_errors = self
            .headers
            .into_iter()
            .flat_map(
                |JsonHeaderInput {
                     errors,
                     warnings: _,
                     extra_data: _,
                 }| errors,
            )
            .filter(|error| !is_imported_file_summary_error(error, &files))
            .collect();

        Ok((
            ProcessResult {
                files,
                excluded_count: self.excluded_count,
            },
            assembly::AssemblyResult {
                packages,
                dependencies,
            },
            license_detections,
            license_references,
            license_rule_references,
            preserved_header_errors,
            imported_spdx_license_list_version,
            imported_license_index_provenance,
        ))
    }
}

fn consistent_header_value<T>(mut values: impl Iterator<Item = T>) -> Option<T>
where
    T: PartialEq,
{
    let first = values.next()?;
    values.all(|value| value == first).then_some(first)
}

fn is_imported_file_summary_error(error: &str, files: &[FileInfo]) -> bool {
    files
        .iter()
        .filter(|file| !file.scan_errors.is_empty())
        .any(|file| header_error_matches_file_summary(error, &file.path))
}

fn header_error_matches_file_summary(error: &str, path: &str) -> bool {
    let first_line = error.lines().next().unwrap_or(error).trim();

    first_line == format!("Path: {path}") || first_line.ends_with(&format!(": {path}"))
}

fn restore_imported_warning_severities(
    files: &mut [FileInfo],
    imported_header_warnings: &[String],
    imported_header_errors: &[String],
) {
    for file in files {
        if file.scan_errors.is_empty() {
            continue;
        }

        let warning_summary = imported_header_warnings
            .iter()
            .any(|warning| header_error_matches_file_summary(warning, &file.path));
        let error_summary = imported_header_errors
            .iter()
            .any(|error| header_error_matches_file_summary(error, &file.path));

        if warning_summary && !error_summary {
            for diagnostic in &mut file.scan_diagnostics {
                diagnostic.severity = DiagnosticSeverity::Warning;
            }
        }
    }
}

pub(crate) fn load_and_merge_json_inputs(
    input_paths: &[String],
    strip_root: bool,
    full_root: bool,
) -> Result<JsonScanInput> {
    let mut merged: Option<JsonScanInput> = None;
    let input_count = input_paths.len();
    for (index, input_path) in input_paths.iter().enumerate() {
        let mut loaded = load_scan_from_json(input_path)?;
        normalize_loaded_json_scan(&mut loaded, strip_root, full_root);
        if input_count > 1 {
            namespace_loaded_input(&mut loaded, index + 1);
        }

        if let Some(acc) = &mut merged {
            acc.files.append(&mut loaded.files);
            acc.packages.append(&mut loaded.packages);
            acc.dependencies.append(&mut loaded.dependencies);
            acc.license_detections
                .append(&mut loaded.license_detections);
            acc.license_references
                .append(&mut loaded.license_references);
            acc.license_rule_references
                .append(&mut loaded.license_rule_references);
            acc.headers.append(&mut loaded.headers);
            acc.excluded_count += loaded.excluded_count;
        } else {
            merged = Some(loaded);
        }
    }

    merged.ok_or_else(|| anyhow!("No input paths provided"))
}

fn namespace_loaded_input(loaded: &mut JsonScanInput, input_index: usize) {
    let original_paths: Vec<String> = loaded.files.iter().map(|file| file.path.clone()).collect();

    for file in &mut loaded.files {
        file.path = namespace_loaded_resource_path(&file.path, input_index);

        for package_data in &mut file.package_data {
            for file_reference in &mut package_data.file_references {
                file_reference.path =
                    namespace_loaded_resource_path(&file_reference.path, input_index);
            }
        }
    }

    for package in &mut loaded.packages {
        for datafile_path in &mut package.datafile_paths {
            *datafile_path = namespace_loaded_resource_path(datafile_path, input_index);
        }
    }

    for dependency in &mut loaded.dependencies {
        dependency.datafile_path =
            namespace_loaded_resource_path(&dependency.datafile_path, input_index);
    }

    normalize_loaded_header_paths(loaded, &original_paths);
}

fn namespace_loaded_resource_path(path: &str, input_index: usize) -> String {
    let trimmed = path.trim_matches('/');
    let relative = trimmed
        .strip_prefix("virtual_root/")
        .or_else(|| (trimmed == "virtual_root").then_some(""))
        .unwrap_or(trimmed);
    let base = format!("virtual_root/codebase-{input_index}");
    if relative.is_empty() {
        base
    } else {
        format!("{base}/{relative}")
    }
}

pub(crate) fn load_scan_from_json(path: &str) -> Result<JsonScanInput> {
    let input_path = Path::new(path);
    if !input_path.is_file() {
        return Err(anyhow!("--from-json input must be a valid file: {}", path));
    }

    let content = fs::read_to_string(input_path)?;
    let mut parsed: JsonScanInput = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Input JSON scan file is not valid JSON: {path}: {e}"))?;

    backfill_imported_file_identity_fields(&mut parsed);
    synthesize_missing_directory_entries(&mut parsed);

    Ok(parsed)
}

fn backfill_imported_file_identity_fields(scan: &mut JsonScanInput) {
    for file in &mut scan.files {
        let path = Path::new(&file.path);

        if file.name.is_empty() {
            file.name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
        }

        if file.base_name.is_empty() {
            file.base_name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
        }

        if file.extension.is_empty() {
            file.extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| format!(".{ext}"))
                .unwrap_or_default();
        }
    }
}

fn synthesize_missing_directory_entries(scan: &mut JsonScanInput) {
    let mut seen_paths: HashSet<String> = scan.files.iter().map(|file| file.path.clone()).collect();
    let mut missing_dirs = Vec::new();

    for file in &scan.files {
        let mut current = Path::new(&file.path).parent();
        while let Some(parent) = current.and_then(|parent| parent.to_str()) {
            if parent.is_empty() || parent == "." {
                break;
            }
            if seen_paths.insert(parent.to_string()) {
                missing_dirs.push(parent.to_string());
            }
            current = Path::new(parent).parent();
        }
    }

    missing_dirs.sort_by_key(|path| path.matches('/').count());
    for dir_path in missing_dirs {
        scan.files.push(directory_output_json_file(&dir_path));
    }

    ensure_replay_root_directories(scan);
}

fn ensure_replay_root_directories(scan: &mut JsonScanInput) {
    let mut seen_paths: HashSet<String> = scan.files.iter().map(|file| file.path.clone()).collect();
    let mut replay_roots = HashSet::new();

    for file in &scan.files {
        let path = file.path.as_str();
        if path == "virtual_root" || path.starts_with("virtual_root/") {
            replay_roots.insert("virtual_root".to_string());
            if let Some(rest) = path.strip_prefix("virtual_root/")
                && let Some((first_component, _)) = rest.split_once('/')
                && first_component.starts_with("codebase-")
            {
                replay_roots.insert(format!("virtual_root/{first_component}"));
            }
        }
    }

    let mut replay_roots: Vec<_> = replay_roots.into_iter().collect();
    replay_roots.sort_by_key(|path| path.matches('/').count());
    for root in replay_roots {
        if seen_paths.insert(root.clone()) {
            scan.files.push(directory_output_json_file(&root));
        }
    }
}

fn directory_output_json_file(path: &str) -> OutputFileInfo {
    let path_obj = Path::new(path);
    let name = path_obj
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string();

    OutputFileInfo {
        name: name.clone(),
        base_name: name,
        extension: String::new(),
        path: path.to_string(),
        file_type: FileType::Directory,
        mime_type: None,
        file_type_label: None,
        size: 0,
        date: None,
        sha1: None,
        md5: None,
        sha256: None,
        sha1_git: None,
        programming_language: None,
        package_data: Vec::new(),
        license_expression: None,
        license_detections: Vec::new(),
        license_clues: Vec::new(),
        percentage_of_license_text: None,
        copyrights: Vec::new(),
        holders: Vec::new(),
        authors: Vec::new(),
        emails: Vec::new(),
        urls: Vec::new(),
        for_packages: Vec::new(),
        scan_errors: Vec::new(),
        license_policy: None,
        is_generated: None,
        is_binary: None,
        is_text: None,
        is_archive: None,
        is_media: None,
        is_source: None,
        is_script: None,
        files_count: None,
        dirs_count: None,
        size_count: None,
        source_count: None,
        is_legal: false,
        is_manifest: false,
        is_readme: false,
        is_top_level: false,
        is_key_file: false,
        is_community: false,
        facets: Vec::new(),
        tallies: None,
    }
}

pub(crate) fn normalize_loaded_json_scan(
    loaded: &mut JsonScanInput,
    strip_root: bool,
    full_root: bool,
) {
    let initial_paths: Vec<String> = loaded.files.iter().map(|file| file.path.clone()).collect();

    if should_prefix_loaded_paths_with_virtual_root(loaded, strip_root, full_root) {
        prefix_loaded_resource_paths_with_virtual_root(loaded);
        normalize_loaded_header_paths(loaded, &initial_paths);
    }

    let original_paths: Vec<String> = loaded.files.iter().map(|file| file.path.clone()).collect();

    if let Some(scan_root) = derive_json_scan_root(&loaded.files)
        && strip_root
    {
        normalize_output_paths(&mut loaded.files, &scan_root, true, false);
        normalize_loaded_top_level_detection_paths(loaded, &scan_root, true, false);
        normalize_output_top_level_paths(
            &mut loaded.packages,
            &mut loaded.dependencies,
            &scan_root,
            true,
        );
    }

    if full_root {
        trim_loaded_json_full_root_paths(loaded);
    }

    synthesize_missing_directory_entries(loaded);

    normalize_loaded_header_paths(loaded, &original_paths);
}

fn should_prefix_loaded_paths_with_virtual_root(
    loaded: &JsonScanInput,
    strip_root: bool,
    full_root: bool,
) -> bool {
    if strip_root || full_root || loaded.files.len() <= 1 {
        return false;
    }

    let mut saw_relative = false;
    for file in &loaded.files {
        let path = Path::new(&file.path);
        if path.is_absolute() {
            return false;
        }
        if file.path == "virtual_root" || file.path.starts_with("virtual_root/") {
            return false;
        }
        saw_relative = true;
    }

    saw_relative
}

fn prefix_loaded_resource_paths_with_virtual_root(loaded: &mut JsonScanInput) {
    for file in &mut loaded.files {
        file.path = prefix_virtual_root(&file.path);

        for package_data in &mut file.package_data {
            for file_reference in &mut package_data.file_references {
                file_reference.path = prefix_virtual_root(&file_reference.path);
            }
        }
    }

    for package in &mut loaded.packages {
        for datafile_path in &mut package.datafile_paths {
            *datafile_path = prefix_virtual_root(datafile_path);
        }
    }

    for dependency in &mut loaded.dependencies {
        dependency.datafile_path = prefix_virtual_root(&dependency.datafile_path);
    }
}

fn prefix_virtual_root(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        "virtual_root".to_string()
    } else {
        format!("virtual_root/{trimmed}")
    }
}

fn derive_json_scan_root(files: &[OutputFileInfo]) -> Option<String> {
    let mut directories: Vec<&str> = files
        .iter()
        .filter(|file| file.file_type == FileType::Directory)
        .map(|file| file.path.as_str())
        .collect();
    directories.sort_by_key(|path| (path.matches('/').count(), path.len()));
    if let Some(root_dir) = directories.first() {
        return Some((*root_dir).to_string());
    }

    if files.len() == 1 {
        return files.first().map(|file| file.path.clone());
    }

    let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    super::selection::common_path_prefix(&paths).map(|path| path.to_string_lossy().to_string())
}

fn trim_loaded_json_full_root_paths(loaded: &mut JsonScanInput) {
    for file in &mut loaded.files {
        trim_full_root_display_value(&mut file.path);
        for detection_match in &mut file.license_clues {
            if let Some(from_file) = detection_match.from_file.as_mut() {
                trim_full_root_display_value(from_file);
            }
        }
        for detection in &mut file.license_detections {
            for detection_match in &mut detection.matches {
                if let Some(from_file) = detection_match.from_file.as_mut() {
                    trim_full_root_display_value(from_file);
                }
            }
        }
        for package_data in &mut file.package_data {
            for file_reference in &mut package_data.file_references {
                trim_full_root_display_value(&mut file_reference.path);
            }
            for detection in &mut package_data.license_detections {
                for detection_match in &mut detection.matches {
                    if let Some(from_file) = detection_match.from_file.as_mut() {
                        trim_full_root_display_value(from_file);
                    }
                }
            }
            for detection in &mut package_data.other_license_detections {
                for detection_match in &mut detection.matches {
                    if let Some(from_file) = detection_match.from_file.as_mut() {
                        trim_full_root_display_value(from_file);
                    }
                }
            }
        }
    }

    for package in &mut loaded.packages {
        for datafile_path in &mut package.datafile_paths {
            trim_full_root_display_value(datafile_path);
        }
    }
    for dependency in &mut loaded.dependencies {
        trim_full_root_display_value(&mut dependency.datafile_path);
    }

    normalize_loaded_top_level_detection_paths(loaded, "", false, true);
}

fn trim_full_root_display_value(path: &mut String) {
    *path = path.replace('\\', "/").trim_matches('/').to_string();
}

fn normalize_loaded_top_level_detection_paths(
    loaded: &mut JsonScanInput,
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) {
    for detection in &mut loaded.license_detections {
        for detection_match in &mut detection.reference_matches {
            if let Some(from_file) = detection_match.from_file.as_mut() {
                if strip_root
                    && let Some(normalized) =
                        normalize_loaded_detection_path(from_file, scan_root, true, false)
                {
                    *from_file = normalized;
                }
                if full_root
                    && let Some(normalized) =
                        normalize_loaded_detection_path(from_file, scan_root, false, true)
                {
                    *from_file = normalized;
                }
            }
        }
    }
}

fn normalize_loaded_header_paths(loaded: &mut JsonScanInput, original_paths: &[String]) {
    let mut replacements: Vec<_> = original_paths
        .iter()
        .zip(loaded.files.iter().map(|file| file.path.as_str()))
        .filter(|(before, after)| before.as_str() != *after)
        .map(|(before, after)| (before.as_str(), after))
        .collect();
    replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()));

    for header in &mut loaded.headers {
        for error in &mut header.errors {
            normalize_loaded_header_message(error, &replacements);
        }
        for warning in &mut header.warnings {
            normalize_loaded_header_message(warning, &replacements);
        }
    }
}

fn normalize_loaded_header_message(message: &mut String, replacements: &[(&str, &str)]) {
    for (before, after) in replacements {
        if let Some(remainder) = message.strip_prefix(&format!("Path: {before}")) {
            *message = format!("Path: {after}{remainder}");
            break;
        }
        if let Some((first_line, remainder)) = message.split_once('\n')
            && first_line.ends_with(&format!(": {before}"))
        {
            *message = format!(
                "{}: {after}\n{remainder}",
                &first_line[..first_line.len() - before.len() - 2]
            );
            break;
        }
        if message.ends_with(before) {
            let prefix_len = message.len() - before.len();
            message.replace_range(prefix_len.., after);
            break;
        }
    }
}

fn normalize_loaded_detection_path(
    path: &str,
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) -> Option<String> {
    let current_path = PathBuf::from(path);

    if full_root {
        let absolute_candidate = if current_path.is_absolute() {
            current_path.clone()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(&current_path))
                .unwrap_or(current_path.clone())
        };
        let absolute = absolute_candidate
            .canonicalize()
            .unwrap_or(absolute_candidate);
        return Some(
            absolute
                .to_string_lossy()
                .replace('\\', "/")
                .trim_matches('/')
                .to_string(),
        );
    }

    if strip_root {
        let scan_root_path = Path::new(scan_root);
        let strip_base = if scan_root_path.is_file() {
            scan_root_path.parent().unwrap_or_else(|| Path::new(""))
        } else {
            scan_root_path
        };

        if current_path == scan_root_path
            && let Some(file_name) = scan_root_path.file_name().and_then(|name| name.to_str())
        {
            return Some(file_name.to_string());
        }

        if let Ok(stripped) = current_path.strip_prefix(strip_base)
            && !stripped.as_os_str().is_empty()
        {
            return Some(stripped.to_string_lossy().to_string());
        }
    }

    None
}

fn normalize_output_paths(
    files: &mut [OutputFileInfo],
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) {
    for entry in files.iter_mut() {
        if let Some(normalized_path) =
            normalize_path_value(&entry.path, scan_root, strip_root, full_root)
        {
            entry.path = normalized_path;
        }

        normalize_output_match_paths(&mut entry.license_clues, scan_root, strip_root, full_root);

        for detection in &mut entry.license_detections {
            normalize_output_match_paths(&mut detection.matches, scan_root, strip_root, full_root);
        }

        for package_data in &mut entry.package_data {
            for file_reference in &mut package_data.file_references {
                if let Some(normalized_path) =
                    normalize_path_value(&file_reference.path, scan_root, strip_root, full_root)
                {
                    file_reference.path = normalized_path;
                }
            }

            for detection in &mut package_data.license_detections {
                normalize_output_match_paths(
                    &mut detection.matches,
                    scan_root,
                    strip_root,
                    full_root,
                );
            }

            for detection in &mut package_data.other_license_detections {
                normalize_output_match_paths(
                    &mut detection.matches,
                    scan_root,
                    strip_root,
                    full_root,
                );
            }
        }
    }
}

fn normalize_output_match_paths(
    matches: &mut [OutputMatch],
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) {
    for detection_match in matches {
        if let Some(from_file) = detection_match.from_file.as_mut()
            && let Some(normalized_path) =
                normalize_path_value(from_file.as_str(), scan_root, strip_root, full_root)
        {
            *from_file = normalized_path;
        }
    }
}

fn normalize_path_value(
    path: &str,
    scan_root: &str,
    strip_root: bool,
    full_root: bool,
) -> Option<String> {
    let current_path = PathBuf::from(path);

    if full_root {
        let absolute_candidate = if current_path.is_absolute() {
            current_path.clone()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(&current_path))
                .unwrap_or(current_path.clone())
        };
        let absolute = absolute_candidate
            .canonicalize()
            .unwrap_or(absolute_candidate);
        return Some(
            absolute
                .to_string_lossy()
                .replace('\\', "/")
                .trim_matches('/')
                .to_string(),
        );
    }

    if strip_root {
        let scan_root_path = Path::new(scan_root);
        let strip_base = if scan_root_path.is_file() {
            scan_root_path.parent().unwrap_or_else(|| Path::new(""))
        } else {
            scan_root_path
        };

        if current_path == scan_root_path
            && let Some(file_name) = scan_root_path.file_name().and_then(|name| name.to_str())
        {
            return Some(file_name.to_string());
        }

        if let Some(stripped) = strip_root_prefix(&current_path, strip_base) {
            return Some(stripped.to_string_lossy().to_string());
        }
    }

    None
}

fn strip_root_prefix(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(stripped) = path.strip_prefix(root)
        && !stripped.as_os_str().is_empty()
    {
        return Some(stripped.to_path_buf());
    }

    let canonical_path = path.canonicalize().ok()?;
    let canonical_root = root.canonicalize().ok()?;
    let stripped = canonical_path.strip_prefix(canonical_root).ok()?;
    if stripped.as_os_str().is_empty() {
        None
    } else {
        Some(stripped.to_path_buf())
    }
}

fn normalize_output_top_level_paths(
    packages: &mut [OutputPackage],
    dependencies: &mut [OutputTopLevelDependency],
    scan_root: &str,
    strip_root: bool,
) {
    if !strip_root {
        return;
    }

    for package in packages {
        for datafile_path in &mut package.datafile_paths {
            if let Some(normalized_path) =
                normalize_path_value(datafile_path, scan_root, true, false)
            {
                *datafile_path = normalized_path;
            }
        }
    }

    for dependency in dependencies {
        if let Some(normalized_path) =
            normalize_path_value(&dependency.datafile_path, scan_root, true, false)
        {
            dependency.datafile_path = normalized_path;
        }
    }
}
