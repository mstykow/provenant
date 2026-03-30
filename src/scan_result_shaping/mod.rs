#[cfg(test)]
mod core_test;
pub(crate) mod json_input;
pub(crate) mod selection;
#[cfg(test)]
mod test_fixtures;

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use crate::license_detection::LicenseDetectionEngine;
use crate::license_detection::index::LicenseIndex;
use crate::models::{FileInfo, Match, Package, TopLevelDependency};
use anyhow::Result;

pub(crate) use json_input::load_and_merge_json_inputs;
pub(crate) use selection::{
    apply_cli_path_selection_filter, apply_user_path_filters_to_collected,
    resolve_native_scan_inputs,
};

fn retain_matching_files_with_ancestor_dirs<F>(files: &mut Vec<FileInfo>, mut keep_file: F)
where
    F: FnMut(&FileInfo) -> bool,
{
    let kept_file_paths: HashSet<String> = files
        .iter()
        .filter(|entry| entry.file_type == crate::models::FileType::File && keep_file(entry))
        .map(|entry| entry.path.clone())
        .collect();

    files.retain(|entry| match entry.file_type {
        crate::models::FileType::File => kept_file_paths.contains(&entry.path),
        crate::models::FileType::Directory => kept_file_paths
            .iter()
            .any(|path| Path::new(path).starts_with(Path::new(&entry.path))),
    });
}

pub(crate) fn apply_path_selection_filter<F>(files: &mut Vec<FileInfo>, keep_file: F)
where
    F: FnMut(&FileInfo) -> bool,
{
    retain_matching_files_with_ancestor_dirs(files, keep_file);
}

fn has_findings(file: &FileInfo) -> bool {
    file.license_expression.is_some()
        || !file.license_detections.is_empty()
        || !file.license_clues.is_empty()
        || !file.copyrights.is_empty()
        || !file.holders.is_empty()
        || !file.authors.is_empty()
        || !file.emails.is_empty()
        || !file.urls.is_empty()
        || !file.package_data.is_empty()
        || !file.scan_errors.is_empty()
        || file.is_generated == Some(true)
}

pub(crate) fn apply_only_findings_filter(files: &mut Vec<FileInfo>) {
    retain_matching_files_with_ancestor_dirs(files, has_findings);
}

pub(crate) fn apply_package_only_filter(files: &mut Vec<FileInfo>) {
    retain_matching_files_with_ancestor_dirs(files, |file| {
        file.file_type == crate::models::FileType::File && !file.package_data.is_empty()
    });
}

fn matches_any_regex<'a, I>(patterns: &[Regex], values: I) -> bool
where
    I: IntoIterator<Item = &'a str>,
{
    values
        .into_iter()
        .any(|value| patterns.iter().any(|pattern| pattern.is_match(value)))
}

pub(crate) fn apply_ignore_resource_filter(
    files: &mut Vec<FileInfo>,
    ignored_holders: &[Regex],
    ignored_authors: &[Regex],
) {
    if ignored_holders.is_empty() && ignored_authors.is_empty() {
        return;
    }

    retain_matching_files_with_ancestor_dirs(files, |file| {
        let holder_match = !ignored_holders.is_empty()
            && matches_any_regex(
                ignored_holders,
                file.holders.iter().map(|holder| holder.holder.as_str()),
            );
        let author_match = !ignored_authors.is_empty()
            && matches_any_regex(
                ignored_authors,
                file.authors.iter().map(|author| author.author.as_str()),
            );

        !(holder_match || author_match)
    });
}

fn dedupe_vec_by_key<T, K, F>(items: &mut Vec<T>, mut key_fn: F)
where
    K: std::hash::Hash + Eq,
    F: FnMut(&T) -> K,
{
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(key_fn(item)));
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ClueRuleData {
    ignorable_copyrights: Vec<String>,
    ignorable_holders: Vec<String>,
    ignorable_authors: Vec<String>,
    ignorable_urls: Vec<String>,
    ignorable_emails: Vec<String>,
}

pub(crate) type ClueRuleLookup = HashMap<String, ClueRuleData>;

#[derive(Debug, Clone)]
struct IgnorableSpan {
    start_line: usize,
    end_line: usize,
    values: Vec<String>,
}

pub(crate) fn build_clue_rule_lookup(index: &LicenseIndex) -> ClueRuleLookup {
    index
        .rules_by_rid
        .iter()
        .map(|rule| {
            (
                rule.identifier.clone(),
                ClueRuleData {
                    ignorable_copyrights: rule.ignorable_copyrights.clone().unwrap_or_default(),
                    ignorable_holders: rule.ignorable_holders.clone().unwrap_or_default(),
                    ignorable_authors: rule.ignorable_authors.clone().unwrap_or_default(),
                    ignorable_urls: rule.ignorable_urls.clone().unwrap_or_default(),
                    ignorable_emails: rule.ignorable_emails.clone().unwrap_or_default(),
                },
            )
        })
        .collect()
}

pub(crate) fn prepare_filter_clue_rule_lookup(
    files: &[crate::models::FileInfo],
    active_license_engine: Option<&LicenseDetectionEngine>,
    rules_path: Option<&str>,
) -> Result<Option<ClueRuleLookup>> {
    let needs_rule_lookup = files.iter().any(|file| {
        file.license_detections
            .iter()
            .chain(
                file.package_data
                    .iter()
                    .flat_map(|package_data| package_data.license_detections.iter()),
            )
            .chain(
                file.package_data
                    .iter()
                    .flat_map(|package_data| package_data.other_license_detections.iter()),
            )
            .any(|detection| !detection.matches.is_empty())
    });
    if !needs_rule_lookup {
        return Ok(None);
    }

    if let Some(active_license_engine) = active_license_engine {
        return Ok(Some(build_clue_rule_lookup(active_license_engine.index())));
    }

    let fallback_engine = match rules_path {
        Some(path) => LicenseDetectionEngine::from_directory(Path::new(path))?,
        None => LicenseDetectionEngine::from_embedded()?,
    };
    Ok(Some(build_clue_rule_lookup(fallback_engine.index())))
}

pub(crate) fn filter_redundant_clues(files: &mut [FileInfo]) {
    filter_redundant_clues_with_rules(files, None);
}

pub(crate) fn filter_redundant_clues_with_rules(
    files: &mut [FileInfo],
    clue_rule_lookup: Option<&ClueRuleLookup>,
) {
    for file in files.iter_mut() {
        dedupe_vec_by_key(&mut file.copyrights, |c| {
            (c.copyright.clone(), c.start_line, c.end_line)
        });
        dedupe_vec_by_key(&mut file.holders, |h| {
            (h.holder.clone(), h.start_line, h.end_line)
        });
        dedupe_vec_by_key(&mut file.authors, |a| {
            (a.author.clone(), a.start_line, a.end_line)
        });
        dedupe_vec_by_key(&mut file.emails, |e| {
            (e.email.clone(), e.start_line, e.end_line)
        });
        dedupe_vec_by_key(&mut file.urls, |u| {
            (u.url.clone(), u.start_line, u.end_line)
        });

        filter_license_aware_clues(file, clue_rule_lookup);
    }
}

fn filter_license_aware_clues(file: &mut FileInfo, clue_rule_lookup: Option<&ClueRuleLookup>) {
    let rule_ignorables = collect_rule_ignorables(file, clue_rule_lookup);
    let copyrights_as_ignorable = file
        .copyrights
        .iter()
        .map(|copyright| IgnorableSpan {
            start_line: copyright.start_line,
            end_line: copyright.end_line,
            values: vec![copyright.copyright.clone()],
        })
        .collect::<Vec<_>>();
    let holders_as_ignorable = file
        .holders
        .iter()
        .map(|holder| IgnorableSpan {
            start_line: holder.start_line,
            end_line: holder.end_line,
            values: vec![holder.holder.clone()],
        })
        .collect::<Vec<_>>();
    let authors_as_ignorable = file
        .authors
        .iter()
        .map(|author| IgnorableSpan {
            start_line: author.start_line,
            end_line: author.end_line,
            values: vec![author.author.clone()],
        })
        .collect::<Vec<_>>();

    file.emails.retain(|email| {
        !matches_ignorable(
            &rule_ignorables
                .ignorable_emails
                .iter()
                .chain(copyrights_as_ignorable.iter())
                .chain(authors_as_ignorable.iter())
                .cloned()
                .collect::<Vec<_>>(),
            email.start_line,
            email.end_line,
            email.email.as_str(),
            false,
        )
    });
    file.urls.retain(|url| {
        !matches_ignorable(
            &rule_ignorables
                .ignorable_urls
                .iter()
                .chain(copyrights_as_ignorable.iter())
                .chain(authors_as_ignorable.iter())
                .cloned()
                .collect::<Vec<_>>(),
            url.start_line,
            url.end_line,
            url.url.as_str(),
            true,
        )
    });
    file.authors.retain(|author| {
        !matches_ignorable(
            &rule_ignorables
                .ignorable_authors
                .iter()
                .chain(copyrights_as_ignorable.iter())
                .chain(holders_as_ignorable.iter())
                .cloned()
                .collect::<Vec<_>>(),
            author.start_line,
            author.end_line,
            author.author.as_str(),
            false,
        )
    });
    file.holders.retain(|holder| {
        !matches_ignorable(
            &rule_ignorables.ignorable_holders,
            holder.start_line,
            holder.end_line,
            holder.holder.as_str(),
            false,
        )
    });
    file.copyrights.retain(|copyright| {
        !matches_ignorable(
            &rule_ignorables.ignorable_copyrights,
            copyright.start_line,
            copyright.end_line,
            copyright.copyright.as_str(),
            false,
        )
    });
}

#[derive(Debug, Default)]
struct ResourceIgnorables {
    ignorable_copyrights: Vec<IgnorableSpan>,
    ignorable_holders: Vec<IgnorableSpan>,
    ignorable_authors: Vec<IgnorableSpan>,
    ignorable_urls: Vec<IgnorableSpan>,
    ignorable_emails: Vec<IgnorableSpan>,
}

fn collect_rule_ignorables(
    file: &FileInfo,
    clue_rule_lookup: Option<&ClueRuleLookup>,
) -> ResourceIgnorables {
    let mut ignorables = ResourceIgnorables::default();
    let Some(clue_rule_lookup) = clue_rule_lookup else {
        return ignorables;
    };

    for detection in file
        .license_detections
        .iter()
        .chain(
            file.package_data
                .iter()
                .flat_map(|package_data| package_data.license_detections.iter()),
        )
        .chain(
            file.package_data
                .iter()
                .flat_map(|package_data| package_data.other_license_detections.iter()),
        )
    {
        for detection_match in &detection.matches {
            let Some(rule_identifier) = detection_match.rule_identifier.as_deref() else {
                continue;
            };
            let Some(match_coverage) = detection_match.match_coverage else {
                continue;
            };
            if match_coverage < 90.0 {
                continue;
            }
            let Some(rule_data) = clue_rule_lookup.get(rule_identifier) else {
                continue;
            };

            push_ignorable_values(
                &mut ignorables.ignorable_copyrights,
                detection_match.start_line,
                detection_match.end_line,
                &rule_data.ignorable_copyrights,
                false,
            );
            push_ignorable_values(
                &mut ignorables.ignorable_holders,
                detection_match.start_line,
                detection_match.end_line,
                &rule_data.ignorable_holders,
                false,
            );
            push_ignorable_values(
                &mut ignorables.ignorable_authors,
                detection_match.start_line,
                detection_match.end_line,
                &rule_data.ignorable_authors,
                false,
            );
            push_ignorable_values(
                &mut ignorables.ignorable_urls,
                detection_match.start_line,
                detection_match.end_line,
                &rule_data.ignorable_urls,
                true,
            );
            push_ignorable_values(
                &mut ignorables.ignorable_emails,
                detection_match.start_line,
                detection_match.end_line,
                &rule_data.ignorable_emails,
                false,
            );
        }
    }

    ignorables
}

fn push_ignorable_values(
    target: &mut Vec<IgnorableSpan>,
    start_line: usize,
    end_line: usize,
    values: &[String],
    trim_slashes: bool,
) {
    if values.is_empty() {
        return;
    }

    let normalized_values = values
        .iter()
        .map(|value| normalize_ignorable_value(value, trim_slashes))
        .collect::<Vec<_>>();
    target.push(IgnorableSpan {
        start_line,
        end_line,
        values: normalized_values,
    });
}

fn matches_ignorable(
    ignorables: &[IgnorableSpan],
    start_line: usize,
    end_line: usize,
    value: &str,
    trim_slashes: bool,
) -> bool {
    let normalized_value = normalize_ignorable_value(value, trim_slashes);

    ignorables.iter().any(|ignorable| {
        ((start_line >= ignorable.start_line && start_line <= ignorable.end_line)
            || (end_line >= ignorable.start_line && end_line <= ignorable.end_line))
            && ignorable
                .values
                .iter()
                .any(|candidate| candidate == &normalized_value)
    })
}

fn normalize_ignorable_value(value: &str, trim_slashes: bool) -> String {
    if trim_slashes {
        value.trim_matches('/').to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn normalize_paths(
    files: &mut [FileInfo],
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

        normalize_match_paths(&mut entry.license_clues, scan_root, strip_root, full_root);

        for detection in &mut entry.license_detections {
            normalize_match_paths(&mut detection.matches, scan_root, strip_root, full_root);
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
                normalize_match_paths(&mut detection.matches, scan_root, strip_root, full_root);
            }

            for detection in &mut package_data.other_license_detections {
                normalize_match_paths(&mut detection.matches, scan_root, strip_root, full_root);
            }
        }
    }
}

fn normalize_match_paths(
    matches: &mut [Match],
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

pub(crate) fn apply_mark_source(files: &mut [FileInfo]) {
    let mut index_by_path = HashMap::<String, usize>::new();
    for (idx, entry) in files.iter().enumerate() {
        index_by_path.insert(entry.path.clone(), idx);
    }

    for entry in files.iter_mut() {
        entry.is_source = Some(entry.is_source.unwrap_or(false));
        entry.source_count = Some(0);
    }

    let mut dir_paths = files
        .iter()
        .filter(|entry| entry.file_type == crate::models::FileType::Directory)
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    dir_paths.sort_by_key(|path| usize::MAX - Path::new(path).components().count());

    let mut direct_file_count = HashMap::<String, usize>::new();
    let mut direct_source_file_count = HashMap::<String, usize>::new();
    let mut child_dirs = HashMap::<String, Vec<String>>::new();

    for entry in files.iter() {
        if let Some(parent) = Path::new(&entry.path).parent().and_then(|p| p.to_str()) {
            let parent_key = parent.to_string();
            if entry.file_type == crate::models::FileType::File {
                let excluded_go_non_production = entry.programming_language.as_deref()
                    == Some("Go")
                    && entry.is_source == Some(false);
                if excluded_go_non_production {
                    continue;
                }
                *direct_file_count.entry(parent_key.clone()).or_insert(0) += 1;
                if entry.is_source.unwrap_or(false) {
                    *direct_source_file_count.entry(parent_key).or_insert(0) += 1;
                }
            } else {
                child_dirs
                    .entry(parent_key)
                    .or_default()
                    .push(entry.path.clone());
            }
        }
    }

    let mut descendant_file_count = HashMap::<String, usize>::new();
    let mut descendant_source_count = HashMap::<String, usize>::new();

    for dir_path in dir_paths {
        let mut total_files = *direct_file_count.get(&dir_path).unwrap_or(&0);
        let mut source_files = *direct_source_file_count.get(&dir_path).unwrap_or(&0);

        if let Some(children) = child_dirs.get(&dir_path) {
            for child in children {
                total_files += descendant_file_count.get(child).copied().unwrap_or(0);
                source_files += descendant_source_count.get(child).copied().unwrap_or(0);
            }
        }

        let qualifies = total_files > 0 && (source_files as f64 / total_files as f64) >= 0.9;

        if let Some(idx) = index_by_path.get(&dir_path)
            && let Some(entry) = files.get_mut(*idx)
        {
            if qualifies && source_files > 0 {
                entry.is_source = Some(true);
                entry.source_count = Some(source_files);
            } else {
                entry.is_source = Some(false);
                entry.source_count = Some(0);
            }
        }

        descendant_file_count.insert(dir_path.clone(), total_files);
        descendant_source_count.insert(dir_path, if qualifies { source_files } else { 0 });
    }
}

pub(crate) fn trim_preloaded_assembly_to_files(
    files: &[FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let kept_file_paths: HashSet<&str> = files
        .iter()
        .filter(|entry| entry.file_type == crate::models::FileType::File)
        .map(|entry| entry.path.as_str())
        .collect();

    packages.retain_mut(|package| {
        package
            .datafile_paths
            .retain(|path| kept_file_paths.contains(path.as_str()));
        !package.datafile_paths.is_empty()
    });

    let kept_package_uids: HashSet<&str> = packages
        .iter()
        .map(|package| package.package_uid.as_str())
        .collect();
    dependencies.retain(|dependency| {
        kept_file_paths.contains(dependency.datafile_path.as_str())
            && dependency
                .for_package_uid
                .as_deref()
                .is_none_or(|uid| kept_package_uids.contains(uid))
    });
}

pub(crate) fn normalize_top_level_output_paths(
    packages: &mut [Package],
    dependencies: &mut [TopLevelDependency],
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
