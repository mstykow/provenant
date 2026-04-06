use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::models::{FacetTallies, FileInfo, FileType, Tallies, TallyEntry};

use super::FACETS;
use super::classification::{is_legal_file, is_readme_file};
use super::summary_helpers::{
    canonicalize_summary_expression, package_other_detected_license_values,
    package_primary_detected_license_values,
};

pub(super) fn compute_tallies(files: &[FileInfo]) -> Option<Tallies> {
    let detected_license_expression = tally_file_values(files, detected_license_values, true);
    let copyrights = tally_file_values(files, copyright_values, true);
    let holders = tally_file_values(files, holder_values, true);
    let authors = tally_file_values(files, author_values, true);
    let programming_language = tally_file_values(files, programming_language_values, false);

    let tallies = Tallies {
        detected_license_expression,
        copyrights,
        holders,
        authors,
        programming_language,
    };

    (!tallies.is_empty()).then_some(tallies)
}

pub(super) fn compute_key_file_tallies(files: &[FileInfo]) -> Option<Tallies> {
    if !files
        .iter()
        .any(|file| file.file_type == FileType::File && file.is_key_file)
    {
        return None;
    }

    let tallies = Tallies {
        detected_license_expression: tally_file_values_filtered(
            files,
            |file| file.is_key_file,
            detected_license_values,
            false,
        ),
        copyrights: tally_file_values_filtered(
            files,
            |file| file.is_key_file,
            copyright_values,
            false,
        ),
        holders: tally_file_values_filtered(files, |file| file.is_key_file, holder_values, false),
        authors: tally_file_values_filtered(files, |file| file.is_key_file, author_values, false),
        programming_language: tally_file_values_filtered(
            files,
            |file| file.is_key_file,
            programming_language_values,
            false,
        ),
    };

    (!tallies.is_empty()).then_some(tallies)
}

pub(super) fn compute_tallies_by_facet(files: &[FileInfo]) -> Option<Vec<FacetTallies>> {
    let mut buckets: HashMap<&'static str, TallyAccumulator> = FACETS
        .iter()
        .map(|facet| (*facet, TallyAccumulator::default()))
        .collect();

    for file in files.iter().filter(|file| file.file_type == FileType::File) {
        if file.facets.is_empty() {
            continue;
        }

        let Some(file_tallies) = file.tallies.as_ref() else {
            continue;
        };

        for facet in &file.facets {
            let Some(bucket) = buckets.get_mut(facet.as_str()) else {
                continue;
            };
            bucket.merge_license_expressions(&file_tallies.detected_license_expression);
            bucket.merge_copyrights(&file_tallies.copyrights);
            bucket.merge_holders(&file_tallies.holders);
            bucket.merge_authors(&file_tallies.authors);
            bucket.merge_programming_languages(&file_tallies.programming_language);
        }
    }

    Some(
        FACETS
            .iter()
            .map(|facet| FacetTallies {
                facet: (*facet).to_string(),
                tallies: buckets.remove(facet).unwrap_or_default().into_tallies(),
            })
            .collect(),
    )
}

pub(super) fn compute_detailed_tallies(files: &mut [FileInfo]) {
    let mut children_by_parent: HashMap<String, Vec<usize>> = HashMap::new();
    let known_paths: HashSet<String> = files.iter().map(|file| file.path.clone()).collect();

    for (idx, file) in files.iter().enumerate() {
        let Some(parent) = parent_path(&file.path) else {
            continue;
        };
        if known_paths.contains(parent.as_str()) {
            children_by_parent.entry(parent).or_default().push(idx);
        }
    }

    let mut indices: Vec<usize> = (0..files.len()).collect();
    indices.sort_by_key(|&idx| std::cmp::Reverse(path_depth(&files[idx].path)));

    for idx in indices {
        let tallies = if files[idx].file_type == FileType::File {
            compute_direct_file_tallies(&files[idx])
        } else {
            aggregate_child_tallies(
                children_by_parent
                    .get(files[idx].path.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                files,
            )
        };
        files[idx].tallies = Some(tallies);
    }
}

pub(super) fn compute_file_tallies(files: &mut [FileInfo]) {
    for file in files.iter_mut() {
        if file.file_type == FileType::File {
            file.tallies = Some(compute_direct_file_tallies(file));
        } else {
            file.tallies = None;
        }
    }
}

pub(super) fn author_values(file: &FileInfo) -> Vec<String> {
    if is_legal_file(file)
        || is_readme_file(file)
        || file.programming_language.as_deref() == Some("C/C++ Header")
    {
        return Vec::new();
    }

    file.authors
        .iter()
        .filter(|author| author.author.chars().any(|ch| ch.is_ascii_uppercase()))
        .map(|author| author.author.clone())
        .collect()
}

pub(super) fn copyright_values(file: &FileInfo) -> Vec<String> {
    if is_legal_file(file) {
        return Vec::new();
    }

    file.copyrights
        .iter()
        .map(|copyright| normalize_tally_copyright_value(&copyright.copyright))
        .collect()
}

pub(super) fn holder_values(file: &FileInfo) -> Vec<String> {
    if is_legal_file(file) {
        return Vec::new();
    }

    file.holders
        .iter()
        .map(|holder| normalize_tally_holder_value(&holder.holder))
        .collect()
}

pub(super) fn programming_language_values(file: &FileInfo) -> Vec<String> {
    file.programming_language
        .as_deref()
        .filter(|language| !matches!(*language, "Text" | "JSON"))
        .map(str::to_string)
        .into_iter()
        .collect()
}

pub(super) fn summary_detected_license_values(file: &FileInfo) -> Vec<String> {
    let mut detection_expressions: Vec<String> = file
        .license_detections
        .iter()
        .map(|detection| canonicalize_summary_expression(&detection.license_expression))
        .filter(|expression| expression != "unknown-license-reference")
        .collect();
    detection_expressions.extend(
        file.license_clues
            .iter()
            .map(|detection_match| {
                canonicalize_summary_expression(&detection_match.license_expression)
            })
            .filter(|expression| expression != "unknown-license-reference"),
    );
    detection_expressions.extend(package_primary_detected_license_values(file, true));
    detection_expressions.extend(package_other_detected_license_values(file, true));

    if detection_expressions.is_empty() {
        return file
            .license_expression
            .as_deref()
            .map(canonicalize_summary_expression)
            .into_iter()
            .collect();
    }

    detection_expressions
}

pub(super) fn tally_file_values<F>(
    files: &[FileInfo],
    values_for_file: F,
    count_missing_files: bool,
) -> Vec<TallyEntry>
where
    F: Fn(&FileInfo) -> Vec<String>,
{
    tally_file_values_filtered(files, |_| true, values_for_file, count_missing_files)
}

pub(super) fn tally_file_values_filtered<P, F>(
    files: &[FileInfo],
    predicate: P,
    values_for_file: F,
    count_missing_files: bool,
) -> Vec<TallyEntry>
where
    P: Fn(&FileInfo) -> bool,
    F: Fn(&FileInfo) -> Vec<String>,
{
    let mut counts: HashMap<Option<String>, usize> = HashMap::new();

    for file in files
        .iter()
        .filter(|file| file.file_type == FileType::File && predicate(file))
    {
        let values = values_for_file(file);
        if values.is_empty() {
            if count_missing_files {
                *counts.entry(None).or_insert(0) += 1;
            }
            continue;
        }

        for value in values {
            *counts.entry(Some(value)).or_insert(0) += 1;
        }
    }

    build_tally_entries(counts)
}

pub(super) fn build_tally_entries(counts: HashMap<Option<String>, usize>) -> Vec<TallyEntry> {
    let mut tallies: Vec<TallyEntry> = counts
        .into_iter()
        .map(|(value, count)| TallyEntry { value, count })
        .collect();

    tallies.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    tallies
}

#[derive(Default)]
struct TallyAccumulator {
    detected_license_expression: HashMap<Option<String>, usize>,
    copyrights: HashMap<Option<String>, usize>,
    holders: HashMap<Option<String>, usize>,
    authors: HashMap<Option<String>, usize>,
    programming_language: HashMap<Option<String>, usize>,
}

impl TallyAccumulator {
    fn merge_license_expressions(&mut self, entries: &[TallyEntry]) {
        merge_non_null_entries_into_counts(&mut self.detected_license_expression, entries);
    }

    fn merge_copyrights(&mut self, entries: &[TallyEntry]) {
        merge_non_null_entries_into_counts(&mut self.copyrights, entries);
    }

    fn merge_holders(&mut self, entries: &[TallyEntry]) {
        merge_non_null_entries_into_counts(&mut self.holders, entries);
    }

    fn merge_authors(&mut self, entries: &[TallyEntry]) {
        merge_non_null_entries_into_counts(&mut self.authors, entries);
    }

    fn merge_programming_languages(&mut self, entries: &[TallyEntry]) {
        merge_non_null_entries_into_counts(&mut self.programming_language, entries);
    }

    fn into_tallies(self) -> Tallies {
        Tallies {
            detected_license_expression: build_tally_entries(self.detected_license_expression),
            copyrights: build_tally_entries(self.copyrights),
            holders: build_tally_entries(self.holders),
            authors: build_tally_entries(self.authors),
            programming_language: build_tally_entries(self.programming_language),
        }
    }
}

fn compute_direct_file_tallies(file: &FileInfo) -> Tallies {
    Tallies {
        detected_license_expression: build_direct_tally_entries(
            detected_license_values(file),
            true,
        ),
        copyrights: build_direct_tally_entries(copyright_values(file), true),
        holders: build_direct_tally_entries(holder_values(file), true),
        authors: build_direct_tally_entries(author_values(file), true),
        programming_language: build_direct_tally_entries(programming_language_values(file), true),
    }
}

fn aggregate_child_tallies(child_indices: &[usize], files: &[FileInfo]) -> Tallies {
    let mut detected_license_expression = HashMap::new();
    let mut copyrights = HashMap::new();
    let mut holders = HashMap::new();
    let mut authors = HashMap::new();
    let mut programming_language = HashMap::new();

    for &child_idx in child_indices {
        let Some(child_tallies) = files[child_idx].tallies.as_ref() else {
            continue;
        };

        merge_tally_entries(
            &mut detected_license_expression,
            &child_tallies.detected_license_expression,
        );
        merge_tally_entries(&mut copyrights, &child_tallies.copyrights);
        merge_tally_entries(&mut holders, &child_tallies.holders);
        merge_tally_entries(&mut authors, &child_tallies.authors);
        merge_non_null_entries_into_counts(
            &mut programming_language,
            &child_tallies.programming_language,
        );
    }

    Tallies {
        detected_license_expression: build_tally_entries(detected_license_expression),
        copyrights: build_tally_entries(copyrights),
        holders: build_tally_entries(holders),
        authors: build_tally_entries(authors),
        programming_language: build_tally_entries(programming_language),
    }
}

fn build_direct_tally_entries(values: Vec<String>, count_missing: bool) -> Vec<TallyEntry> {
    let mut counts: HashMap<Option<String>, usize> = HashMap::new();

    if values.is_empty() {
        if count_missing {
            counts.insert(None, 1);
        }
    } else {
        for value in values {
            *counts.entry(Some(value)).or_insert(0) += 1;
        }
    }

    build_tally_entries(counts)
}

fn merge_tally_entries(counts: &mut HashMap<Option<String>, usize>, entries: &[TallyEntry]) {
    for entry in entries {
        *counts.entry(entry.value.clone()).or_insert(0) += entry.count;
    }
}

fn merge_non_null_entries_into_counts(
    destination: &mut HashMap<Option<String>, usize>,
    entries: &[TallyEntry],
) {
    for entry in entries.iter().filter(|entry| entry.value.is_some()) {
        *destination.entry(entry.value.clone()).or_insert(0) += entry.count;
    }
}

fn detected_license_values(file: &FileInfo) -> Vec<String> {
    let mut detection_expressions: Vec<String> = file
        .license_detections
        .iter()
        .map(|detection| canonicalize_summary_expression(&detection.license_expression))
        .collect();
    detection_expressions.extend(file.license_clues.iter().map(|detection_match| {
        canonicalize_summary_expression(&detection_match.license_expression)
    }));
    detection_expressions.extend(package_primary_detected_license_values(file, false));
    detection_expressions.extend(package_other_detected_license_values(file, false));

    if detection_expressions.is_empty() {
        return file
            .license_expression
            .as_deref()
            .map(canonicalize_summary_expression)
            .into_iter()
            .collect();
    }

    detection_expressions
}

fn normalize_tally_copyright_value(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_end_matches(" as indicated by the @authors tag");

    if let Some(rest) = trimmed.strip_prefix("Copyright (c) ") {
        let normalized_rest = rest.trim_start_matches(|ch: char| {
            ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-'
        });

        if !normalized_rest.is_empty() && normalized_rest != rest {
            return format!("Copyright (c) {}", normalized_rest.trim());
        }
    }

    if let Some(rest) = trimmed.strip_prefix("Copyright ")
        && let Some((yearish, remainder)) = rest.split_once(',')
        && !yearish.is_empty()
        && yearish
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-')
    {
        return format!("Copyright {}", remainder.trim());
    }

    if let Some(rest) = trimmed.strip_prefix("Copyright ") {
        let mut parts = rest.rsplitn(2, ' ');
        let trailing = parts.next().unwrap_or_default();
        let leading = parts.next().unwrap_or_default();
        if !leading.is_empty()
            && trailing
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == ',' || ch == '-')
        {
            return format!("Copyright {}", leading.trim());
        }
    }

    trimmed.to_string()
}

fn normalize_tally_holder_value(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(" as indicated by the @authors tag")
        .to_string()
}

fn parent_path(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .and_then(|parent| parent.to_str())
        .filter(|parent| !parent.is_empty())
        .map(str::to_string)
}

fn path_depth(path: &str) -> usize {
    Path::new(path).components().count()
}
