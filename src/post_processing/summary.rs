use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::license_detection::expression::{
    LicenseExpression, parse_expression, simplify_expression,
};
use crate::models::{
    DatasourceId, FileInfo, FileType, LicenseClarityScore, Package, Summary, Tallies, TallyEntry,
};
use crate::utils::spdx::combine_license_expressions;

use super::output_indexes::OutputIndexes;
use super::summary_helpers::{
    canonicalize_summary_expression, canonicalize_summary_holder_display,
    clean_legal_holder_candidate, key_file_has_license_text, remove_tally_value,
    remove_tally_values, summary_holder_from_copyright, summary_license_expression, unique,
};
use super::tallies::{
    author_values, copyright_values, programming_language_values, summary_detected_license_values,
    tally_file_values, tally_file_values_filtered,
};
use super::{is_score_key_file, package_root};

#[cfg(test)]
pub(super) fn compute_summary(files: &[FileInfo], packages: &[Package]) -> Option<Summary> {
    let package_file_index = super::package_file_index::PackageFileIndex::build(files, packages);
    let indexes = super::output_indexes::OutputIndexes::build(
        files,
        Some(&package_file_index),
        false,
        super::output_indexes::OutputIndexMode::Full,
    );
    compute_summary_with_options(files, packages, &indexes, true, true)
}

pub(super) fn compute_summary_with_options(
    files: &[FileInfo],
    packages: &[Package],
    indexes: &OutputIndexes,
    include_summary_fields: bool,
    include_license_clarity_score: bool,
) -> Option<Summary> {
    let top_level_package_uids = top_level_package_uids(packages, files, indexes);
    let declared_holders = compute_declared_holders(files, packages, indexes);
    let (score_declared_license_expression, score_clarity) =
        compute_license_score(files, packages, &top_level_package_uids);

    let declared_holder = if include_summary_fields && !declared_holders.is_empty() {
        Some(declared_holders.join(", "))
    } else {
        None
    };
    let primary_language = if include_summary_fields {
        compute_primary_language(files, packages)
    } else {
        None
    };
    let other_languages = if include_summary_fields {
        compute_other_languages(files, primary_language.as_deref())
    } else {
        Vec::new()
    };
    let tallies = if include_summary_fields {
        compute_summary_tallies(files, packages).unwrap_or_default()
    } else {
        Tallies::default()
    };

    if !include_summary_fields
        && !include_license_clarity_score
        && score_declared_license_expression.is_none()
        && declared_holder.is_none()
        && primary_language.is_none()
        && other_languages.is_empty()
    {
        return None;
    }

    let package_declared_license_expression = if include_summary_fields {
        package_declared_license_expression(packages, files, indexes, &top_level_package_uids)
    } else {
        None
    };
    let declared_license_expression = package_declared_license_expression
        .clone()
        .or_else(|| score_declared_license_expression.clone());
    let other_license_expressions = remove_tally_value(
        declared_license_expression.as_deref(),
        &tallies.detected_license_expression,
    );
    let mut other_holders = if declared_holders.is_empty() {
        tallies.holders.clone()
    } else {
        remove_tally_values(&declared_holders, &tallies.holders)
    };
    if packages.is_empty()
        && !declared_holders.is_empty()
        && files.iter().any(|file| {
            file.is_top_level && file.is_key_file && file.is_legal && !file.copyrights.is_empty()
        })
    {
        other_holders.retain(|entry| entry.value.is_some());
        if files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .all(|file| !file.is_key_file || file.is_legal || file.holders.is_empty())
        {
            other_holders.clear();
        }
    }
    if declared_holders.is_empty() && other_holders.iter().all(|entry| entry.value.is_none()) {
        other_holders.clear();
    }
    if !packages.is_empty() && declared_holders.is_empty() {
        other_holders.clear();
    }

    let license_clarity_score = if include_license_clarity_score {
        let mut score_clarity = score_clarity;
        if !score_clarity.declared_copyrights
            && ((!declared_holders.is_empty()
                && files.iter().any(|file| {
                    file.is_top_level
                        && file.is_key_file
                        && file.is_legal
                        && !file.copyrights.is_empty()
                }))
                || (packages.is_empty()
                    && files.iter().any(|file| {
                        file.is_key_file && file.is_legal && !file.copyrights.is_empty()
                    })))
        {
            score_clarity.declared_copyrights = true;
            score_clarity.score += 10;
        }
        Some(score_clarity)
    } else {
        None
    };

    Some(Summary {
        declared_license_expression,
        license_clarity_score,
        declared_holder: include_summary_fields.then(|| declared_holder.unwrap_or_default()),
        primary_language: include_summary_fields.then_some(primary_language).flatten(),
        other_license_expressions: if include_summary_fields {
            other_license_expressions
        } else {
            vec![]
        },
        other_holders: if include_summary_fields {
            other_holders
        } else {
            vec![]
        },
        other_languages: if include_summary_fields {
            other_languages
        } else {
            vec![]
        },
    })
}

fn package_declared_license_expression(
    packages: &[Package],
    files: &[FileInfo],
    indexes: &OutputIndexes,
    top_level_package_uids: &HashSet<String>,
) -> Option<String> {
    combine_license_expressions(stable_summary_expressions(
        packages
            .iter()
            .filter(|package| top_level_package_uids.contains(&package.package_uid))
            .filter_map(|package| {
                package.declared_license_expression.clone().or_else(|| {
                    package.datafile_paths.iter().find_map(|datafile_path| {
                        indexes
                            .file_ix_by_path(datafile_path)
                            .and_then(|index| files.get(index.0))
                            .and_then(|file| file.license_expression.clone())
                    })
                })
            }),
    ))
    .map(|expr| canonicalize_summary_expression(&expr))
}

fn compute_license_score(
    files: &[FileInfo],
    packages: &[Package],
    top_level_package_uids: &HashSet<String>,
) -> (Option<String>, LicenseClarityScore) {
    let nested_package_roots = nested_summary_package_roots(packages, files);
    let key_files: Vec<&FileInfo> = files
        .iter()
        .filter(|file| is_summary_score_key_file(file, &nested_package_roots))
        .filter(|file| {
            file.for_packages.is_empty()
                || top_level_package_uids.is_empty()
                || file
                    .for_packages
                    .iter()
                    .any(|uid| top_level_package_uids.contains(uid))
        })
        .collect();
    let non_key_files: Vec<&FileInfo> = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .filter(|file| !is_summary_score_key_file(file, &nested_package_roots))
        .collect();

    let key_file_expressions = stable_summary_expressions(
        key_files
            .iter()
            .filter_map(|file| summary_license_expression(file)),
    );
    let primary_declared_license = get_primary_license(&key_file_expressions);

    let mut scoring = LicenseClarityScore {
        score: 0,
        declared_license: key_files.iter().any(|file| {
            !file.license_detections.is_empty()
                || (file.license_detections.is_empty()
                    && file
                        .package_data
                        .iter()
                        .any(|package_data| !package_data.license_detections.is_empty()))
        }),
        identification_precision: key_files
            .iter()
            .flat_map(|file| {
                file.license_detections.iter().chain(
                    file.license_detections
                        .is_empty()
                        .then_some(())
                        .into_iter()
                        .flat_map(|_| {
                            file.package_data
                                .iter()
                                .flat_map(|package_data| package_data.license_detections.iter())
                        }),
                )
            })
            .flat_map(|detection| detection.matches.iter())
            .any(is_good_match),
        has_license_text: key_files.iter().any(|file| key_file_has_license_text(file)),
        declared_copyrights: key_files
            .iter()
            .any(|file| !file.is_legal && !file.copyrights.is_empty()),
        conflicting_license_categories: false,
        ambiguous_compound_licensing: primary_declared_license.is_none(),
    };

    if scoring.declared_license {
        scoring.score += 40;
    }
    if scoring.identification_precision {
        scoring.score += 40;
    }
    if scoring.has_license_text {
        scoring.score += 10;
    }
    if scoring.declared_copyrights {
        scoring.score += 10;
    }

    let declared_license_expression = primary_declared_license
        .map(|expr| canonicalize_summary_expression(&expr))
        .or_else(|| {
            combine_license_expressions(key_file_expressions)
                .map(|expr| canonicalize_summary_expression(&expr))
        });

    scoring.conflicting_license_categories = declared_license_expression
        .as_deref()
        .is_some_and(is_permissive_expression)
        && non_key_files
            .iter()
            .filter_map(|file| summary_license_expression(file))
            .map(|expr| expr.to_ascii_lowercase())
            .any(|expr| is_conflicting_expression(&expr));

    if scoring.conflicting_license_categories {
        scoring.score = scoring.score.saturating_sub(20);
    }
    if scoring.ambiguous_compound_licensing {
        scoring.score = scoring.score.saturating_sub(10);
    }

    (declared_license_expression, scoring)
}

fn is_good_match(license_match: &crate::models::Match) -> bool {
    match (license_match.match_coverage, license_match.rule_relevance) {
        (Some(coverage), Some(relevance)) => {
            license_match.score >= 80.0 && coverage >= 80.0 && relevance >= 80
        }
        _ => license_match.score >= 80.0,
    }
}

fn is_permissive_expression(expression: &str) -> bool {
    ["apache", "mit", "bsd", "zlib", "isc", "cc0", "boost"]
        .iter()
        .any(|needle| expression.contains(needle))
}

fn is_conflicting_expression(expression: &str) -> bool {
    ["gpl", "agpl", "lgpl", "copyleft", "proprietary"]
        .iter()
        .any(|needle| expression.contains(needle))
}

fn stable_summary_expressions<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut expressions: Vec<String> = values
        .into_iter()
        .map(|value| canonicalize_summary_expression(&value))
        .collect();
    expressions.sort_unstable();
    expressions.dedup();
    expressions
}

pub(super) fn get_primary_license(declared_license_expressions: &[String]) -> Option<String> {
    let unique_declared_license_expressions = unique(declared_license_expressions);
    if unique_declared_license_expressions.len() == 1 {
        return unique_declared_license_expressions.into_iter().next();
    }

    let (unique_joined_expressions, single_expressions) =
        group_license_expressions(&unique_declared_license_expressions);

    if unique_joined_expressions.len() == 1 {
        let joined_expression = unique_joined_expressions[0].clone();
        let all_other_expressions_accounted_for = unique_declared_license_expressions
            .iter()
            .filter(|expression| *expression != &joined_expression)
            .all(|expression| summary_expression_covers(&joined_expression, expression));

        if all_other_expressions_accounted_for {
            return Some(joined_expression);
        }
    }

    if unique_joined_expressions.is_empty() {
        return (single_expressions.len() == 1).then(|| single_expressions[0].clone());
    }

    None
}

fn summary_expression_covers(container: &str, contained: &str) -> bool {
    let Ok(parsed_container) = parse_expression(container) else {
        return false;
    };
    let Ok(parsed_contained) = parse_expression(contained) else {
        return false;
    };

    let simplified_container = simplify_expression(&parsed_container);
    let simplified_contained = simplify_expression(&parsed_contained);

    summary_expression_covers_ast(&simplified_container, &simplified_contained)
}

fn summary_expression_covers_ast(
    container: &LicenseExpression,
    contained: &LicenseExpression,
) -> bool {
    if summary_expressions_equal(container, contained) {
        return true;
    }

    match (container, contained) {
        (LicenseExpression::And { .. }, LicenseExpression::And { .. }) => {
            let container_args = summary_flat_and_args(container);
            let contained_args = summary_flat_and_args(contained);
            contained_args.iter().all(|contained_arg| {
                container_args
                    .iter()
                    .any(|container_arg| summary_expressions_equal(container_arg, contained_arg))
            })
        }
        (LicenseExpression::Or { .. }, LicenseExpression::Or { .. }) => {
            let container_args = summary_flat_or_args(container);
            let contained_args = summary_flat_or_args(contained);
            contained_args.iter().all(|contained_arg| {
                container_args
                    .iter()
                    .any(|container_arg| summary_expressions_equal(container_arg, contained_arg))
            })
        }
        (LicenseExpression::And { .. }, _) => summary_flat_and_args(container)
            .iter()
            .any(|container_arg| summary_expressions_equal(container_arg, contained)),
        (LicenseExpression::Or { .. }, _) => summary_flat_or_args(container)
            .iter()
            .any(|container_arg| summary_expressions_equal(container_arg, contained)),
        _ => false,
    }
}

fn summary_expressions_equal(a: &LicenseExpression, b: &LicenseExpression) -> bool {
    match (a, b) {
        (LicenseExpression::License(left), LicenseExpression::License(right)) => left == right,
        (LicenseExpression::LicenseRef(left), LicenseExpression::LicenseRef(right)) => {
            left == right
        }
        (
            LicenseExpression::With {
                left: left_license,
                right: left_exception,
            },
            LicenseExpression::With {
                left: right_license,
                right: right_exception,
            },
        ) => {
            summary_expressions_equal(left_license, right_license)
                && summary_expressions_equal(left_exception, right_exception)
        }
        (LicenseExpression::And { .. }, LicenseExpression::And { .. }) => {
            let left_args = summary_flat_and_args(a);
            let right_args = summary_flat_and_args(b);
            left_args.len() == right_args.len()
                && right_args.iter().all(|right_arg| {
                    left_args
                        .iter()
                        .any(|left_arg| summary_expressions_equal(left_arg, right_arg))
                })
        }
        (LicenseExpression::Or { .. }, LicenseExpression::Or { .. }) => {
            let left_args = summary_flat_or_args(a);
            let right_args = summary_flat_or_args(b);
            left_args.len() == right_args.len()
                && right_args.iter().all(|right_arg| {
                    left_args
                        .iter()
                        .any(|left_arg| summary_expressions_equal(left_arg, right_arg))
                })
        }
        _ => false,
    }
}

fn summary_flat_and_args(expr: &LicenseExpression) -> Vec<LicenseExpression> {
    let mut args = Vec::new();
    collect_summary_flat_and_args(expr, &mut args);
    args
}

fn collect_summary_flat_and_args(expr: &LicenseExpression, args: &mut Vec<LicenseExpression>) {
    match expr {
        LicenseExpression::And { left, right } => {
            collect_summary_flat_and_args(left, args);
            collect_summary_flat_and_args(right, args);
        }
        _ => args.push(expr.clone()),
    }
}

fn summary_flat_or_args(expr: &LicenseExpression) -> Vec<LicenseExpression> {
    let mut args = Vec::new();
    collect_summary_flat_or_args(expr, &mut args);
    args
}

fn collect_summary_flat_or_args(expr: &LicenseExpression, args: &mut Vec<LicenseExpression>) {
    match expr {
        LicenseExpression::Or { left, right } => {
            collect_summary_flat_or_args(left, args);
            collect_summary_flat_or_args(right, args);
        }
        _ => args.push(expr.clone()),
    }
}

fn group_license_expressions(expressions: &[String]) -> (Vec<String>, Vec<String>) {
    let mut joined = Vec::new();
    let mut single = Vec::new();

    for expression in expressions {
        let upper = expression.to_ascii_uppercase();
        if upper.contains(" AND ") || upper.contains(" OR ") || upper.contains(" WITH ") {
            joined.push(expression.clone());
        } else {
            single.push(expression.clone());
        }
    }

    if joined.len() <= 1 {
        return (joined, single);
    }

    let mut unique_joined = Vec::new();
    for expression in joined {
        if !unique_joined.contains(&expression) {
            unique_joined.push(expression);
        }
    }

    (unique_joined, single)
}

fn compute_summary_tallies(files: &[FileInfo], packages: &[Package]) -> Option<Tallies> {
    let summary_origin_package_uids: HashSet<String> = summary_origin_packages(packages, files)
        .into_iter()
        .map(|package| package.package_uid.clone())
        .collect();
    let nested_package_roots = nested_summary_package_roots(packages, files);
    let detected_license_expression = tally_file_values_filtered(
        files,
        |file| {
            !file
                .package_data
                .iter()
                .any(|package_data| package_data.datasource_id == Some(DatasourceId::PypiSetupCfg))
        },
        summary_detected_license_values,
        true,
    );
    let copyrights = tally_file_values(files, copyright_values, true);
    let holders = if packages.is_empty() {
        tally_file_values(
            files,
            |file| {
                file.holders
                    .iter()
                    .map(|holder| holder.holder.clone())
                    .collect()
            },
            true,
        )
    } else {
        tally_file_values_filtered(
            files,
            |file| {
                file.is_community
                    || (file.is_top_level
                        && file.is_key_file
                        && !nested_package_roots
                            .iter()
                            .any(|root| Path::new(&file.path).starts_with(root))
                        && (file.for_packages.is_empty()
                            || summary_origin_package_uids.is_empty()
                            || file
                                .for_packages
                                .iter()
                                .any(|uid| summary_origin_package_uids.contains(uid))))
            },
            |file| {
                file.holders
                    .iter()
                    .map(|holder| holder.holder.clone())
                    .collect()
            },
            true,
        )
    };
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

fn compute_declared_holders(
    files: &[FileInfo],
    packages: &[Package],
    indexes: &OutputIndexes,
) -> Vec<String> {
    let mut package_datafile_holders = Vec::new();
    for package in packages {
        for datafile_path in &package.datafile_paths {
            if let Some(file) = indexes
                .file_ix_by_path(datafile_path)
                .and_then(|index| files.get(index.0))
            {
                if file.is_legal {
                    continue;
                }
                for holder in &file.holders {
                    let canonical_holder = canonicalize_summary_holder_display(&holder.holder);
                    if !package_datafile_holders.contains(&canonical_holder) {
                        package_datafile_holders.push(canonical_holder);
                    }
                }
            }
        }
    }

    let package_copyright_holders = unique(
        &packages
            .iter()
            .filter_map(|package| package.copyright.as_deref())
            .filter_map(summary_holder_from_copyright)
            .map(|holder| canonicalize_summary_holder_display(&holder))
            .collect::<Vec<_>>(),
    );
    if !package_copyright_holders.is_empty() {
        if !package_datafile_holders.is_empty()
            && package_copyright_holders
                .iter()
                .all(|holder| package_datafile_holders.contains(holder))
        {
            return package_datafile_holders;
        }
        return package_copyright_holders;
    }

    let mut counts: HashMap<String, usize> = HashMap::new();

    for holder in packages
        .iter()
        .filter_map(|package| package.holder.as_ref())
    {
        *counts
            .entry(canonicalize_summary_holder_display(holder))
            .or_insert(0) += 1;
    }

    if counts.is_empty() && !package_datafile_holders.is_empty() {
        return package_datafile_holders;
    }

    if counts.is_empty() {
        let mut key_file_holders = Vec::new();
        for holder in files
            .iter()
            .filter(|file| file.is_key_file && !file.is_legal)
            .flat_map(|file| file.holders.iter())
            .map(|holder| canonicalize_summary_holder_display(&holder.holder))
        {
            if !key_file_holders.contains(&holder) {
                key_file_holders.push(holder);
            }
        }

        let mut codebase_holder_counts: HashMap<String, usize> = HashMap::new();
        for holder in files
            .iter()
            .flat_map(|file| file.holders.iter())
            .map(|holder| canonicalize_summary_holder_display(&holder.holder))
        {
            *codebase_holder_counts.entry(holder).or_insert(0) += 1;
        }

        let highest_count = key_file_holders
            .iter()
            .filter_map(|holder| codebase_holder_counts.get(holder).copied())
            .max();

        if let Some(highest_count) = highest_count {
            let highest_key_file_holders: Vec<String> = key_file_holders
                .iter()
                .filter(|holder| codebase_holder_counts.get(*holder) == Some(&highest_count))
                .cloned()
                .collect();
            if !highest_key_file_holders.is_empty() {
                return highest_key_file_holders;
            }
        }

        if !key_file_holders.is_empty() {
            return key_file_holders;
        }

        if packages.is_empty() {
            let mut legal_key_file_holders = Vec::new();
            for holder in files
                .iter()
                .filter(|file| file.is_key_file && file.is_legal)
                .flat_map(|file| {
                    let explicit_holders: Vec<String> = file
                        .holders
                        .iter()
                        .filter_map(|holder| clean_legal_holder_candidate(&holder.holder))
                        .map(|holder| canonicalize_summary_holder_display(&holder))
                        .collect();
                    if explicit_holders.is_empty() {
                        file.copyrights
                            .iter()
                            .filter_map(|copyright| {
                                summary_holder_from_copyright(&copyright.copyright)
                                    .map(|holder| canonicalize_summary_holder_display(&holder))
                            })
                            .collect::<Vec<_>>()
                    } else {
                        explicit_holders
                    }
                })
            {
                if !legal_key_file_holders.contains(&holder) {
                    legal_key_file_holders.push(holder);
                }
            }

            if !legal_key_file_holders.is_empty() {
                return legal_key_file_holders;
            }
        }
    }

    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(holder, _)| holder)
        .into_iter()
        .collect()
}

fn compute_primary_language(files: &[FileInfo], packages: &[Package]) -> Option<String> {
    let package_languages = unique(
        &summary_origin_packages(packages, files)
            .into_iter()
            .filter_map(summary_origin_package_primary_language)
            .collect::<Vec<_>>(),
    );

    if package_languages.len() == 1 {
        return package_languages.into_iter().next();
    }

    let mut counts: HashMap<String, usize> = HashMap::new();

    for language in files
        .iter()
        .filter_map(|file| file.programming_language.as_ref())
        .filter(|language| language.as_str() != "Text")
    {
        *counts.entry(language.clone()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(language, _)| language)
}

fn summary_origin_package_primary_language(package: &Package) -> Option<String> {
    package
        .primary_language
        .clone()
        .or_else(|| match package.package_type {
            Some(crate::models::PackageType::Pypi) => Some("Python".to_string()),
            _ => None,
        })
}

fn summary_origin_packages<'a>(packages: &'a [Package], files: &[FileInfo]) -> Vec<&'a Package> {
    if packages.is_empty() {
        return Vec::new();
    }

    let top_level_roots = top_level_summary_package_roots(packages);
    if top_level_roots.is_empty() {
        return packages.iter().collect();
    }

    let top_level_packages: Vec<&Package> = packages
        .iter()
        .filter(|package| {
            package_root(package)
                .as_ref()
                .is_some_and(|root| top_level_roots.iter().any(|top_level| top_level == root))
        })
        .collect();

    if top_level_packages.is_empty() && !files.is_empty() {
        return packages.iter().collect();
    }

    top_level_packages
}

fn top_level_package_uids(
    packages: &[Package],
    files: &[FileInfo],
    indexes: &OutputIndexes,
) -> HashSet<String> {
    let top_level_packages = summary_origin_packages(packages, files);
    let key_package_uids: HashSet<String> = top_level_packages
        .iter()
        .filter(|package| {
            package.datafile_paths.iter().any(|datafile_path| {
                indexes
                    .file_ix_by_path(datafile_path)
                    .and_then(|index| files.get(index.0))
                    .is_some_and(|file| file.file_type == FileType::File)
            })
        })
        .map(|package| package.package_uid.clone())
        .collect();

    if key_package_uids.is_empty() {
        top_level_packages
            .into_iter()
            .map(|package| package.package_uid.clone())
            .collect()
    } else {
        key_package_uids
    }
}

pub(super) fn top_level_summary_package_roots(packages: &[Package]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = packages.iter().filter_map(package_root).collect();
    roots.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    roots.dedup();

    let mut top_level_roots = Vec::new();
    for root in roots {
        if top_level_roots
            .iter()
            .any(|top_level| root.starts_with(top_level))
        {
            continue;
        }
        top_level_roots.push(root);
    }

    top_level_roots
}

pub(super) fn nested_summary_package_roots(
    packages: &[Package],
    files: &[FileInfo],
) -> Vec<PathBuf> {
    let top_level_roots = top_level_summary_package_roots(packages);
    let mut nested_roots: Vec<PathBuf> = packages
        .iter()
        .filter_map(package_root)
        .filter(|root| {
            top_level_roots
                .iter()
                .any(|top_level| root != top_level && root.starts_with(top_level))
        })
        .collect();

    nested_roots.extend(
        files
            .iter()
            .filter(|file| {
                file.file_type == FileType::File && file.is_manifest && !file.is_top_level
            })
            .map(|file| {
                Path::new(&file.path)
                    .parent()
                    .unwrap_or_else(|| Path::new(&file.path))
            })
            .map(Path::to_path_buf),
    );

    nested_roots.sort();
    nested_roots.dedup();
    nested_roots
}

fn is_summary_score_key_file(file: &FileInfo, nested_package_roots: &[PathBuf]) -> bool {
    file.file_type == FileType::File
        && file.is_top_level
        && is_score_key_file(file)
        && !nested_package_roots
            .iter()
            .any(|root| Path::new(&file.path).starts_with(root))
}

fn compute_other_languages(files: &[FileInfo], primary_language: Option<&str>) -> Vec<TallyEntry> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for language in files
        .iter()
        .filter(|file| file.file_type == FileType::File && !file.is_key_file)
        .filter_map(|file| file.programming_language.as_ref())
        .filter(|language| language.as_str() != "Text")
    {
        *counts.entry(language.clone()).or_insert(0) += 1;
    }

    let mut tallies: Vec<TallyEntry> = counts
        .into_iter()
        .filter(|(language, _)| Some(language.as_str()) != primary_language)
        .map(|(language, count)| TallyEntry {
            value: Some(language),
            count,
        })
        .collect();

    tallies.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    tallies
}
