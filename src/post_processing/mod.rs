use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use chrono::Utc;
use glob::Pattern;

use crate::assembly;
use crate::license_detection::detection::{
    DETECTION_LOG_UNKNOWN_REFERENCE_TO_LOCAL_FILE, FileRegion as InternalFileRegion,
    determine_license_expression, determine_spdx_expression, get_unique_detections,
    select_matches_for_expression,
};
use crate::license_detection::expression::{
    LicenseExpression, combine_expressions_and, expression_to_string, parse_expression,
    simplify_expression,
};
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::spdx_mapping::build_spdx_mapping;
use crate::models::{
    DatasourceId, ExtraData, FacetTallies, FileInfo, FileType, Header, LicenseClarityScore,
    LicenseDetection, LicenseReference, LicenseRuleReference, Match, OUTPUT_FORMAT_VERSION, Output,
    Package, PackageData, Summary, SystemEnvironment, Tallies, TallyEntry,
    TopLevelLicenseDetection,
};

const SCANCODE_LICENSE_URL_BASE: &str =
    "https://github.com/nexB/scancode-toolkit/tree/develop/src/licensedcode/data/licenses";
pub(crate) use crate::license_detection::DEFAULT_LICENSEDB_URL_TEMPLATE;
const SPDX_LICENSE_URL_BASE: &str = "https://spdx.org/licenses";
const INHERIT_LICENSE_FROM_PACKAGE_REFERENCE: &str = "INHERIT_LICENSE_FROM_PACKAGE";
const DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_PACKAGE: &str =
    "unknown-reference-in-file-to-package";
const DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_NONEXISTENT_PACKAGE: &str =
    "unknown-reference-in-file-to-nonexistent-package";
use crate::scanner;
#[cfg(test)]
use crate::utils::generated::generated_code_hints;
use crate::utils::spdx::combine_license_expressions;

#[cfg(test)]
mod classify_test;
#[cfg(test)]
mod facet_test;
#[cfg(test)]
mod generated_test;
#[cfg(all(test, feature = "golden-tests"))]
mod golden_test;
pub(crate) mod license_policy;
#[cfg(test)]
mod output_test;
#[cfg(test)]
mod summary_test;
#[cfg(test)]
mod tallies_test;
#[cfg(test)]
mod test_utils;

pub(crate) struct CreateOutputOptions<'a> {
    pub(crate) facet_rules: &'a [FacetRule],
    pub(crate) include_classify: bool,
    pub(crate) include_summary: bool,
    pub(crate) include_license_clarity_score: bool,
    pub(crate) include_tallies: bool,
    pub(crate) include_tallies_of_key_files: bool,
    pub(crate) include_tallies_with_details: bool,
    pub(crate) include_tallies_by_facet: bool,
    pub(crate) include_generated: bool,
}

pub(crate) struct CreateOutputContext<'a> {
    pub(crate) total_dirs: usize,
    pub(crate) assembly_result: assembly::AssemblyResult,
    pub(crate) license_detections: Vec<TopLevelLicenseDetection>,
    pub(crate) license_references: Vec<crate::models::LicenseReference>,
    pub(crate) license_rule_references: Vec<crate::models::LicenseRuleReference>,
    pub(crate) extra_errors: Vec<String>,
    pub(crate) options: CreateOutputOptions<'a>,
}

#[derive(Debug, Clone)]
struct ResolvedReferenceTarget {
    path: String,
    detections: Vec<LicenseDetection>,
    preserve_match_from_file: bool,
}

struct ClassificationContext {
    package_roots: HashMap<String, PathBuf>,
    package_file_references: HashMap<String, HashSet<String>>,
    scan_roots: Vec<PathBuf>,
    package_data_top_level_dirs: HashSet<String>,
}

#[derive(Default)]
struct OutputIndexes {
    first_file_index_by_path: HashMap<String, usize>,
    key_file_indices_by_package_uid: HashMap<String, Vec<usize>>,
}

#[derive(Clone, Copy)]
struct FileClassification {
    is_legal: bool,
    is_manifest: bool,
    is_readme: bool,
    is_top_level: bool,
    is_key_file: bool,
    is_community: bool,
}

pub(crate) fn create_output(
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
    scan_result: scanner::ProcessResult,
    context: CreateOutputContext<'_>,
) -> Output {
    let duration = (end_time - start_time).num_nanoseconds().unwrap_or(0) as f64 / 1_000_000_000.0;

    let extra_data = ExtraData {
        files_count: scan_result
            .files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .count(),
        directories_count: context.total_dirs,
        excluded_count: scan_result.excluded_count,
        system_environment: SystemEnvironment {
            operating_system: sys_info::os_type().ok(),
            cpu_architecture: env::consts::ARCH.to_string(),
            platform: format!(
                "{}-{}-{}",
                sys_info::os_type().unwrap_or_else(|_| "unknown".to_string()),
                sys_info::os_release().unwrap_or_else(|_| "unknown".to_string()),
                env::consts::ARCH
            ),
            rust_version: rustc_version_runtime::version().to_string(),
        },
    };

    let mut errors: Vec<String> = scan_result
        .files
        .iter()
        .filter_map(|file| {
            if file.scan_errors.is_empty() {
                None
            } else {
                Some(
                    file.scan_errors
                        .iter()
                        .map(|error| format!("{}: {}", file.path, error))
                        .collect::<Vec<String>>(),
                )
            }
        })
        .flatten()
        .collect();
    errors.extend(context.extra_errors);

    let mut files = scan_result.files;
    let assembly::AssemblyResult {
        mut packages,
        dependencies,
    } = context.assembly_result;
    let needs_classification = context.options.include_classify
        || context.options.include_summary
        || context.options.include_license_clarity_score
        || context.options.include_tallies_of_key_files;
    let classification_context = (needs_classification || !packages.is_empty())
        .then(|| build_classification_context(&files, &packages));

    if context.options.include_generated {
        materialize_generated_flags(&mut files);
    } else {
        clear_generated_flags(&mut files);
    }
    if needs_classification && let Some(classification_context) = classification_context.as_ref() {
        apply_file_classification(&mut files, classification_context);
    }
    let output_indexes = build_output_indexes(
        &files,
        classification_context.as_ref(),
        !needs_classification,
    );

    promote_package_metadata_from_key_files(&files, &mut packages, &output_indexes);
    assign_facets(&mut files, context.options.facet_rules);
    if context.options.include_tallies_with_details {
        compute_detailed_tallies(&mut files);
    } else if context.options.include_tallies_by_facet {
        compute_file_tallies(&mut files);
    } else {
        clear_resource_tallies(&mut files);
    }
    let summary =
        if context.options.include_summary || context.options.include_license_clarity_score {
            compute_summary_with_options(
                &files,
                &packages,
                &output_indexes,
                context.options.include_summary,
                context.options.include_license_clarity_score || context.options.include_summary,
            )
        } else {
            None
        };
    let tallies = if context.options.include_tallies || context.options.include_tallies_with_details
    {
        compute_tallies(&files)
    } else {
        None
    };
    let tallies_of_key_files = if context.options.include_tallies_of_key_files {
        compute_key_file_tallies(&files)
    } else {
        None
    };
    let tallies_by_facet = if context.options.include_tallies_by_facet {
        compute_tallies_by_facet(&files)
    } else {
        None
    };
    if !context.options.include_tallies_with_details {
        clear_resource_tallies(&mut files);
    }

    Output {
        summary,
        tallies,
        tallies_of_key_files,
        tallies_by_facet,
        headers: vec![Header {
            start_timestamp: start_time.to_rfc3339(),
            end_timestamp: end_time.to_rfc3339(),
            duration,
            extra_data,
            errors,
            output_format_version: OUTPUT_FORMAT_VERSION.to_string(),
        }],
        packages,
        dependencies,
        license_detections: context.license_detections,
        files,
        license_references: context.license_references,
        license_rule_references: context.license_rule_references,
    }
}

pub(crate) fn collect_top_level_license_detections(
    files: &[FileInfo],
) -> Vec<TopLevelLicenseDetection> {
    let mut internal_detections = Vec::new();

    for file in files {
        let mut file_detections = file.license_detections.iter().collect::<Vec<_>>();
        for package_data in &file.package_data {
            file_detections.extend(package_data.license_detections.iter());
            file_detections.extend(package_data.other_license_detections.iter());
        }

        for detection in file_detections {
            internal_detections.push(public_detection_to_internal(detection, &file.path));
        }
    }

    let representative_detections: HashMap<_, _> =
        internal_detections
            .iter()
            .fold(HashMap::new(), |mut acc, detection| {
                if let Some(identifier) = detection.identifier.as_ref() {
                    acc.entry(identifier.clone())
                        .and_modify(
                            |existing: &mut &crate::license_detection::LicenseDetection| {
                                if existing.detection_log.is_empty()
                                    && !detection.detection_log.is_empty()
                                {
                                    *existing = detection;
                                }
                            },
                        )
                        .or_insert(detection);
                }
                acc
            });
    let mut unique_detections: Vec<_> = get_unique_detections(&internal_detections)
        .into_iter()
        .filter_map(|unique| {
            representative_detections
                .get(&unique.identifier)
                .map(|detection| {
                    let license_expression = detection
                        .license_expression
                        .clone()
                        .filter(|expression| !expression.is_empty())
                        .or_else(|| determine_license_expression(&detection.matches).ok())
                        .unwrap_or_default();
                    let license_expression_spdx = detection
                        .license_expression_spdx
                        .clone()
                        .filter(|expression| !expression.is_empty())
                        .or_else(|| determine_spdx_expression(&detection.matches).ok())
                        .unwrap_or_default();

                    TopLevelLicenseDetection {
                        identifier: unique.identifier.clone(),
                        license_expression,
                        license_expression_spdx,
                        detection_count: unique.file_regions.len(),
                        detection_log: detection.detection_log.clone(),
                        reference_matches: detection
                            .matches
                            .iter()
                            .cloned()
                            .map(internal_match_to_public)
                            .collect(),
                    }
                })
        })
        .collect();
    unique_detections.sort_by(|left, right| {
        left.license_expression
            .cmp(&right.license_expression)
            .then_with(|| right.detection_count.cmp(&left.detection_count))
            .then_with(|| left.identifier.cmp(&right.identifier))
    });
    unique_detections
}

#[derive(Debug, Clone)]
struct ReferenceFollowSnapshot {
    files_by_path: HashMap<String, ResolvedReferenceTarget>,
    package_targets_by_uid: HashMap<String, ResolvedReferenceTarget>,
    package_manifest_dirs_by_uid: HashMap<String, Vec<String>>,
    root_license_targets_by_root: HashMap<String, Vec<ResolvedReferenceTarget>>,
    root_paths: Vec<String>,
}

fn build_reference_follow_snapshot(
    files: &[FileInfo],
    packages: &[Package],
) -> ReferenceFollowSnapshot {
    let files_by_path = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .map(|file| {
            (
                file.path.clone(),
                ResolvedReferenceTarget {
                    path: file.path.clone(),
                    detections: file.license_detections.clone(),
                    preserve_match_from_file: false,
                },
            )
        })
        .collect();

    let package_targets_by_uid = packages
        .iter()
        .filter_map(|package| {
            let package_expression = combine_detection_expressions(&package.license_detections)?;
            if !is_resolved_package_context_expression(&package_expression) {
                return None;
            }

            let path = package
                .datafile_paths
                .first()
                .cloned()
                .unwrap_or_else(|| package.package_uid.clone());

            Some((
                package.package_uid.clone(),
                ResolvedReferenceTarget {
                    path,
                    detections: package.license_detections.clone(),
                    preserve_match_from_file: true,
                },
            ))
        })
        .collect();

    let package_manifest_dirs_by_uid = packages
        .iter()
        .map(|package| {
            let dirs = package
                .datafile_paths
                .iter()
                .filter_map(|path| Path::new(path).parent())
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (package.package_uid.clone(), dirs)
        })
        .collect();

    let root_paths = top_level_root_paths(files);
    let root_license_targets_by_root = build_root_license_targets(files, &root_paths);

    ReferenceFollowSnapshot {
        files_by_path,
        package_targets_by_uid,
        package_manifest_dirs_by_uid,
        root_license_targets_by_root,
        root_paths,
    }
}

fn build_root_license_targets(
    files: &[FileInfo],
    root_paths: &[String],
) -> HashMap<String, Vec<ResolvedReferenceTarget>> {
    let mut targets_by_root = HashMap::new();

    for root in root_paths {
        let mut targets: Vec<_> = files
            .iter()
            .filter(|file| is_root_license_target(file, root))
            .filter_map(|file| {
                let expression = combine_detection_expressions(&file.license_detections)?;
                if !is_resolved_package_context_expression(&expression) {
                    return None;
                }

                Some(ResolvedReferenceTarget {
                    path: file.path.clone(),
                    detections: file.license_detections.clone(),
                    preserve_match_from_file: false,
                })
            })
            .collect();

        targets.sort_by(|left, right| {
            root_license_candidate_priority(&left.path)
                .cmp(&root_license_candidate_priority(&right.path))
                .then_with(|| left.path.cmp(&right.path))
        });

        if !targets.is_empty() {
            targets_by_root.insert(root.clone(), targets);
        }
    }

    targets_by_root
}

fn is_root_license_target(file: &FileInfo, root: &str) -> bool {
    if file.file_type != FileType::File
        || file.license_detections.is_empty()
        || !is_legal_file(file)
    {
        return false;
    }

    let path = Path::new(&file.path);
    let relative = if root.is_empty() {
        path
    } else {
        match path.strip_prefix(root) {
            Ok(relative) => relative,
            Err(_) => return false,
        }
    };

    relative.components().count() == 1
}

fn root_license_candidate_priority(path: &str) -> usize {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if name.starts_with("license") || name.starts_with("licence") {
        0
    } else if name.starts_with("copying") {
        1
    } else if name.starts_with("notice") {
        2
    } else if name.starts_with("copyright") {
        3
    } else {
        4
    }
}

fn combine_detection_expressions(detections: &[LicenseDetection]) -> Option<String> {
    combine_license_expressions(
        detections
            .iter()
            .map(|detection| detection.license_expression.clone()),
    )
}

fn is_resolved_package_context_expression(expression: &str) -> bool {
    !expression.contains("unknown-license-reference") && !expression.contains("free-unknown")
}

fn top_level_root_paths(files: &[FileInfo]) -> Vec<String> {
    let directories: HashSet<String> = files
        .iter()
        .filter(|file| file.file_type == FileType::Directory)
        .map(|file| file.path.clone())
        .collect();

    let mut roots: Vec<String> = directories
        .iter()
        .filter(|path| {
            Path::new(path)
                .parent()
                .and_then(|parent| {
                    let parent = parent.to_string_lossy().replace('\\', "/");
                    (!parent.is_empty()).then_some(parent)
                })
                .is_none_or(|parent| !directories.contains(&parent))
        })
        .cloned()
        .collect();

    if roots.is_empty()
        && files
            .iter()
            .any(|file| file.file_type == FileType::File && !file.path.contains('/'))
    {
        roots.push(String::new());
    }

    roots.sort();
    roots
}

fn follow_references_for_file(file: &mut FileInfo, snapshot: &ReferenceFollowSnapshot) -> bool {
    let mut modified = false;
    let current_path = file.path.clone();
    let package_uids = file.for_packages.clone();

    for detection in &mut file.license_detections {
        if apply_reference_following_to_detection(detection, &current_path, &package_uids, snapshot)
        {
            modified = true;
        }
    }

    for package_data in &mut file.package_data {
        for detection in &mut package_data.license_detections {
            if apply_reference_following_to_detection(
                detection,
                &current_path,
                &package_uids,
                snapshot,
            ) {
                modified = true;
            }
        }
        for detection in &mut package_data.other_license_detections {
            if apply_reference_following_to_detection(
                detection,
                &current_path,
                &package_uids,
                snapshot,
            ) {
                modified = true;
            }
        }

        if modified {
            package_data.declared_license_expression = combine_license_expressions(
                package_data
                    .license_detections
                    .iter()
                    .map(|detection| detection.license_expression.clone()),
            );
            package_data.declared_license_expression_spdx = combine_license_expressions(
                package_data
                    .license_detections
                    .iter()
                    .filter(|detection| !detection.license_expression_spdx.is_empty())
                    .map(|detection| detection.license_expression_spdx.clone()),
            );
            package_data.other_license_expression = combine_license_expressions(
                package_data
                    .other_license_detections
                    .iter()
                    .map(|detection| detection.license_expression.clone()),
            );
            package_data.other_license_expression_spdx = combine_license_expressions(
                package_data
                    .other_license_detections
                    .iter()
                    .filter(|detection| !detection.license_expression_spdx.is_empty())
                    .map(|detection| detection.license_expression_spdx.clone()),
            );
        }
    }

    if modified {
        file.license_expression = combine_license_expressions(
            file.license_detections
                .iter()
                .map(|detection| detection.license_expression.clone()),
        );
    }

    modified
}

pub(crate) fn apply_package_reference_following(files: &mut [FileInfo], packages: &mut [Package]) {
    for _ in 0..5 {
        let snapshot = build_reference_follow_snapshot(files, packages);
        let mut modified = false;

        for file in files
            .iter_mut()
            .filter(|file| file.file_type == FileType::File)
        {
            if follow_references_for_file(file, &snapshot) {
                modified = true;
            }
        }

        if sync_packages_from_followed_package_data(files, packages) {
            modified = true;
        }

        if !modified {
            break;
        }
    }
}

pub(crate) fn sync_packages_from_followed_package_data(
    files: &[FileInfo],
    packages: &mut [Package],
) -> bool {
    let package_data_by_path: HashMap<_, _> = files
        .iter()
        .filter(|file| !file.package_data.is_empty())
        .map(|file| (file.path.clone(), file.package_data.clone()))
        .collect();
    let files_by_path: HashMap<_, _> = files.iter().map(|file| (file.path.clone(), file)).collect();

    let mut modified = false;

    for package in packages {
        for datafile_path in &package.datafile_paths {
            let matched_package_data =
                package_data_by_path
                    .get(datafile_path)
                    .and_then(|package_datas| {
                        package_datas.iter().find(|package_data| {
                            package_data.purl.as_ref().is_some_and(|purl| {
                                package
                                    .purl
                                    .as_ref()
                                    .is_some_and(|pkg_purl| pkg_purl == purl)
                            }) || (package_data.name == package.name
                                && package_data.version == package.version)
                                || package_datas.len() == 1
                        })
                    });

            let manifest_file = files_by_path.get(datafile_path).copied();

            let mut next_license_detections = matched_package_data
                .map(|package_data| package_data.license_detections.clone())
                .unwrap_or_default();
            let next_other_license_detections = matched_package_data
                .map(|package_data| package_data.other_license_detections.clone())
                .unwrap_or_default();
            let mut next_declared_license_expression = matched_package_data
                .and_then(|package_data| package_data.declared_license_expression.clone());
            let mut next_declared_license_expression_spdx = matched_package_data
                .and_then(|package_data| package_data.declared_license_expression_spdx.clone());
            let next_other_license_expression = matched_package_data
                .and_then(|package_data| package_data.other_license_expression.clone());
            let next_other_license_expression_spdx = matched_package_data
                .and_then(|package_data| package_data.other_license_expression_spdx.clone());

            if next_license_detections.is_empty()
                && let Some(manifest_file) =
                    manifest_file.filter(|file| !file.license_detections.is_empty())
            {
                next_license_detections = manifest_file.license_detections.clone();
                if next_declared_license_expression.is_none() {
                    next_declared_license_expression = combine_license_expressions(
                        manifest_file
                            .license_detections
                            .iter()
                            .map(|detection| detection.license_expression.clone()),
                    )
                    .or_else(|| manifest_file.license_expression.clone());
                }
                if next_declared_license_expression_spdx.is_none() {
                    next_declared_license_expression_spdx = combine_license_expressions(
                        manifest_file
                            .license_detections
                            .iter()
                            .filter(|detection| !detection.license_expression_spdx.is_empty())
                            .map(|detection| detection.license_expression_spdx.clone()),
                    );
                }
            }

            let changed = package.license_detections != next_license_detections
                || package.other_license_detections != next_other_license_detections
                || package.declared_license_expression != next_declared_license_expression
                || package.declared_license_expression_spdx
                    != next_declared_license_expression_spdx
                || package.other_license_expression != next_other_license_expression
                || package.other_license_expression_spdx != next_other_license_expression_spdx;
            if changed {
                package.license_detections = next_license_detections;
                package.other_license_detections = next_other_license_detections;
                package.declared_license_expression = next_declared_license_expression;
                package.declared_license_expression_spdx = next_declared_license_expression_spdx;
                package.other_license_expression = next_other_license_expression;
                package.other_license_expression_spdx = next_other_license_expression_spdx;
                modified = true;
            }
            if matched_package_data.is_some() || manifest_file.is_some() {
                break;
            }
        }
    }

    modified
}

fn apply_reference_following_to_detection(
    detection: &mut LicenseDetection,
    current_path: &str,
    package_uids: &[String],
    snapshot: &ReferenceFollowSnapshot,
) -> bool {
    if has_resolved_referenced_file(detection, current_path) {
        return false;
    }

    let referenced_filenames = referenced_filenames_from_detection(detection);
    if !referenced_filenames.is_empty() {
        let referenced_targets: Vec<_> = referenced_filenames
            .iter()
            .filter_map(|referenced_filename| {
                resolve_referenced_resource(
                    referenced_filename,
                    current_path,
                    package_uids,
                    snapshot,
                )
            })
            .collect();
        if referenced_targets.is_empty() {
            return false;
        }

        return apply_resolved_reference_targets(
            detection,
            current_path,
            referenced_targets,
            DETECTION_LOG_UNKNOWN_REFERENCE_TO_LOCAL_FILE,
        );
    }

    if !inherits_license_from_package(detection) {
        return false;
    }

    let Some((referenced_targets, detection_log)) =
        resolve_package_reference_targets(current_path, package_uids, snapshot)
    else {
        return false;
    };

    apply_resolved_reference_targets(detection, current_path, referenced_targets, detection_log)
}

fn apply_resolved_reference_targets(
    detection: &mut LicenseDetection,
    current_path: &str,
    referenced_targets: Vec<ResolvedReferenceTarget>,
    detection_log: &str,
) -> bool {
    let referenced_license_expression =
        combine_license_expressions(referenced_targets.iter().flat_map(|target| {
            target
                .detections
                .iter()
                .map(|detection| detection.license_expression.clone())
        }));
    if !use_referenced_license_expression(referenced_license_expression.as_deref(), detection) {
        return false;
    }

    let mut internal_detection = public_detection_to_internal(detection, current_path);
    for target in &referenced_targets {
        for referenced_detection in &target.detections {
            let mut internal = public_detection_to_internal(referenced_detection, &target.path);
            for match_item in &mut internal.matches {
                if target.preserve_match_from_file {
                    match_item
                        .from_file
                        .get_or_insert_with(|| target.path.clone());
                } else {
                    match_item.from_file = Some(target.path.clone());
                }
            }
            internal_detection.matches.extend(internal.matches);
        }
    }
    let matches_for_expression = select_matches_for_expression(
        &internal_detection.matches,
        DETECTION_LOG_UNKNOWN_REFERENCE_TO_LOCAL_FILE,
    );
    internal_detection.license_expression =
        determine_license_expression(&matches_for_expression).ok();
    internal_detection.license_expression_spdx =
        determine_spdx_expression(&matches_for_expression).ok();
    internal_detection.detection_log = vec![detection_log.to_string()];
    let mut public_detection = internal_detection_to_public(internal_detection);
    public_detection.identifier = None;
    crate::models::file_info::enrich_license_detection_provenance(
        &mut public_detection,
        current_path,
    );
    *detection = public_detection;
    true
}

fn referenced_filenames_from_detection(detection: &LicenseDetection) -> Vec<String> {
    detection
        .matches
        .iter()
        .flat_map(|detection_match| {
            detection_match
                .referenced_filenames
                .clone()
                .unwrap_or_default()
        })
        .map(|name| normalize_referenced_filename(&name))
        .filter(|name| !name.is_empty() && name != INHERIT_LICENSE_FROM_PACKAGE_REFERENCE)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn inherits_license_from_package(detection: &LicenseDetection) -> bool {
    detection.matches.iter().any(|detection_match| {
        detection_match
            .referenced_filenames
            .as_ref()
            .is_some_and(|filenames| {
                filenames.iter().any(|filename| {
                    normalize_referenced_filename(filename)
                        == INHERIT_LICENSE_FROM_PACKAGE_REFERENCE
                })
            })
    })
}

fn has_resolved_referenced_file(detection: &LicenseDetection, current_path: &str) -> bool {
    detection.matches.iter().any(|detection_match| {
        detection_match
            .from_file
            .as_deref()
            .is_some_and(|path| path != current_path)
    })
}

fn normalize_referenced_filename(name: &str) -> String {
    name.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string()
}

fn resolve_referenced_resource(
    referenced_filename: &str,
    current_path: &str,
    package_uids: &[String],
    snapshot: &ReferenceFollowSnapshot,
) -> Option<ResolvedReferenceTarget> {
    let referenced_filename = normalize_referenced_filename(referenced_filename);
    if referenced_filename.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();
    if let Some(parent) = Path::new(current_path).parent() {
        let parent = parent.to_string_lossy();
        candidates.push(join_reference_candidate(
            parent.as_ref(),
            &referenced_filename,
        ));
    }

    for package_uid in package_uids {
        if let Some(dirs) = snapshot.package_manifest_dirs_by_uid.get(package_uid) {
            for dir in dirs {
                candidates.push(join_reference_candidate(dir, &referenced_filename));
            }
        }
    }

    for root in &snapshot.root_paths {
        candidates.push(join_reference_candidate(root, &referenced_filename));
    }

    candidates
        .into_iter()
        .find_map(|candidate| snapshot.files_by_path.get(&candidate).cloned())
}

fn resolve_package_reference_targets(
    current_path: &str,
    package_uids: &[String],
    snapshot: &ReferenceFollowSnapshot,
) -> Option<(Vec<ResolvedReferenceTarget>, &'static str)> {
    if let Some(targets) = resolve_package_context_target(package_uids, snapshot) {
        return Some((targets, DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_PACKAGE));
    }

    resolve_root_package_context_target(current_path, snapshot).map(|targets| {
        (
            targets,
            DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_NONEXISTENT_PACKAGE,
        )
    })
}

fn resolve_package_context_target(
    package_uids: &[String],
    snapshot: &ReferenceFollowSnapshot,
) -> Option<Vec<ResolvedReferenceTarget>> {
    let mut targets = Vec::new();

    for package_uid in package_uids {
        if let Some(target) = snapshot.package_targets_by_uid.get(package_uid) {
            targets.push(target.clone());
        }
    }

    collapse_equivalent_reference_targets(targets)
}

fn resolve_root_package_context_target(
    current_path: &str,
    snapshot: &ReferenceFollowSnapshot,
) -> Option<Vec<ResolvedReferenceTarget>> {
    let root = snapshot
        .root_paths
        .iter()
        .filter(|root| path_is_within_root(current_path, root))
        .max_by_key(|root| root.len())?;

    let targets = snapshot.root_license_targets_by_root.get(root)?.clone();
    collapse_equivalent_reference_targets(targets)
}

fn collapse_equivalent_reference_targets(
    targets: Vec<ResolvedReferenceTarget>,
) -> Option<Vec<ResolvedReferenceTarget>> {
    if targets.is_empty() {
        return None;
    }

    let expressions: HashSet<_> = targets
        .iter()
        .filter_map(|target| combine_detection_expressions(&target.detections))
        .collect();

    if expressions.len() != 1 {
        return None;
    }

    targets.into_iter().next().map(|target| vec![target])
}

fn path_is_within_root(path: &str, root: &str) -> bool {
    root.is_empty() || path == root || path.starts_with(&format!("{root}/"))
}

fn join_reference_candidate(base: &str, referenced_filename: &str) -> String {
    if base.is_empty() {
        referenced_filename.to_string()
    } else {
        Path::new(base)
            .join(referenced_filename)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

fn use_referenced_license_expression(
    referenced_license_expression: Option<&str>,
    detection: &LicenseDetection,
) -> bool {
    let Some(referenced_license_expression) = referenced_license_expression else {
        return false;
    };

    if detection.license_expression == "unknown-license-reference" {
        return true;
    }

    if referenced_license_expression == detection.license_expression {
        return true;
    }

    let current_keys = parse_expression(&detection.license_expression)
        .ok()
        .map(|expr| expr.license_keys())
        .unwrap_or_default();
    let referenced_keys = parse_expression(referenced_license_expression)
        .ok()
        .map(|expr| expr.license_keys())
        .unwrap_or_default();

    if current_keys == referenced_keys
        && detection.license_expression != referenced_license_expression
    {
        return false;
    }

    if referenced_keys.len() > 5 {
        return false;
    }

    true
}

fn public_detection_to_internal(
    detection: &LicenseDetection,
    owning_path: &str,
) -> crate::license_detection::LicenseDetection {
    let matches: Vec<_> = detection
        .matches
        .iter()
        .map(public_match_to_internal)
        .collect();
    let file_regions = if matches.is_empty() {
        Vec::new()
    } else {
        let start_line = matches.iter().map(|match_item| match_item.start_line).min();
        let end_line = matches.iter().map(|match_item| match_item.end_line).max();
        match (start_line, end_line) {
            (Some(start_line), Some(end_line)) => vec![InternalFileRegion {
                path: owning_path.to_string(),
                start_line,
                end_line,
            }],
            _ => Vec::new(),
        }
    };
    crate::license_detection::LicenseDetection {
        license_expression: (!detection.license_expression.is_empty())
            .then(|| detection.license_expression.clone()),
        license_expression_spdx: (!detection.license_expression_spdx.is_empty())
            .then(|| detection.license_expression_spdx.clone()),
        matches: matches.clone(),
        detection_log: detection.detection_log.clone(),
        identifier: detection.identifier.clone(),
        file_regions,
    }
}

fn internal_detection_to_public(
    detection: crate::license_detection::LicenseDetection,
) -> LicenseDetection {
    LicenseDetection {
        license_expression: detection.license_expression.unwrap_or_default(),
        license_expression_spdx: detection.license_expression_spdx.unwrap_or_default(),
        matches: detection
            .matches
            .into_iter()
            .map(internal_match_to_public)
            .collect(),
        detection_log: detection.detection_log,
        identifier: detection.identifier,
    }
}

fn public_match_to_internal(
    detection_match: &Match,
) -> crate::license_detection::models::LicenseMatch {
    crate::license_detection::models::LicenseMatch {
        rid: 0,
        license_expression: detection_match.license_expression.clone(),
        license_expression_spdx: (!detection_match.license_expression_spdx.is_empty())
            .then(|| detection_match.license_expression_spdx.clone()),
        from_file: detection_match.from_file.clone(),
        start_line: detection_match.start_line,
        end_line: detection_match.end_line,
        start_token: 0,
        end_token: 0,
        matcher: detection_match
            .matcher
            .as_deref()
            .and_then(|matcher| matcher.parse().ok())
            .unwrap_or(crate::license_detection::models::MatcherKind::Hash),
        score: detection_match.score as f32,
        matched_length: detection_match.matched_length.unwrap_or_default(),
        rule_length: detection_match.matched_length.unwrap_or_default(),
        match_coverage: detection_match.match_coverage.unwrap_or_default() as f32,
        rule_relevance: detection_match.rule_relevance.unwrap_or_default() as u8,
        rule_identifier: detection_match.rule_identifier.clone().unwrap_or_default(),
        rule_url: detection_match.rule_url.clone().unwrap_or_default(),
        matched_text: detection_match.matched_text.clone(),
        referenced_filenames: detection_match.referenced_filenames.clone(),
        rule_kind: crate::license_detection::models::RuleKind::None,
        is_from_license: false,
        matched_token_positions: None,
        hilen: 0,
        rule_start_token: 0,
        qspan_positions: None,
        ispan_positions: None,
        hispan_positions: None,
        candidate_resemblance: 0.0,
        candidate_containment: 0.0,
    }
}

fn internal_match_to_public(
    detection_match: crate::license_detection::models::LicenseMatch,
) -> Match {
    let output_metric = |value: f32| ((value as f64) * 100.0).round() / 100.0;
    let score = output_metric(detection_match.score);
    let match_coverage = output_metric(detection_match.coverage());

    Match {
        license_expression: detection_match.license_expression,
        license_expression_spdx: detection_match.license_expression_spdx.unwrap_or_default(),
        from_file: detection_match.from_file,
        start_line: detection_match.start_line,
        end_line: detection_match.end_line,
        matcher: Some(detection_match.matcher.to_string()),
        score,
        matched_length: Some(detection_match.matched_length),
        match_coverage: Some(match_coverage),
        rule_relevance: Some(detection_match.rule_relevance as usize),
        rule_identifier: Some(detection_match.rule_identifier),
        rule_url: (!detection_match.rule_url.is_empty()).then_some(detection_match.rule_url),
        matched_text: detection_match.matched_text,
        referenced_filenames: detection_match.referenced_filenames,
        matched_text_diagnostics: None,
    }
}

pub(crate) fn collect_top_level_license_references(
    files: &[FileInfo],
    packages: &[Package],
    license_index: &LicenseIndex,
    license_url_template: &str,
) -> (Vec<LicenseReference>, Vec<LicenseRuleReference>) {
    let licenses: Vec<_> = license_index.licenses_by_key.values().cloned().collect();
    let spdx_mapping = build_spdx_mapping(&licenses);
    let mut license_keys = BTreeSet::new();
    let mut rule_identifiers = BTreeSet::new();

    for file in files {
        collect_license_keys_from_expression(file.license_expression.as_deref(), &mut license_keys);
        collect_rule_identifiers_from_detections(&file.license_detections, &mut rule_identifiers);
        collect_rule_identifiers_from_matches(&file.license_clues, &mut rule_identifiers);

        for package_data in &file.package_data {
            collect_license_keys_from_package_data(package_data, &mut license_keys);
        }
    }

    for package in packages {
        collect_license_keys_from_expression(
            package.declared_license_expression.as_deref(),
            &mut license_keys,
        );
        collect_license_keys_from_expression(
            package.other_license_expression.as_deref(),
            &mut license_keys,
        );
        collect_license_keys_from_detections(&package.license_detections, &mut license_keys);
        collect_license_keys_from_detections(&package.other_license_detections, &mut license_keys);
        collect_rule_identifiers_from_detections(
            &package.license_detections,
            &mut rule_identifiers,
        );
        collect_rule_identifiers_from_detections(
            &package.other_license_detections,
            &mut rule_identifiers,
        );
    }

    let rules_by_identifier: HashMap<&str, &crate::license_detection::models::Rule> = license_index
        .rules_by_rid
        .iter()
        .map(|rule| (rule.identifier.as_str(), rule))
        .collect();

    for identifier in &rule_identifiers {
        if let Some(rule) = rules_by_identifier.get(identifier.as_str()) {
            collect_license_keys_from_expression(Some(&rule.license_expression), &mut license_keys);
        }
    }

    let license_references = license_keys
        .into_iter()
        .filter_map(|key| {
            license_index.licenses_by_key.get(&key).map(|license| {
                let spdx_license_key = spdx_mapping.scancode_to_spdx(&key).unwrap_or_default();
                let short_name = license.short_name.clone().unwrap_or_else(|| {
                    if spdx_license_key.is_empty()
                        || spdx_license_key.starts_with("LicenseRef-scancode-")
                    {
                        license.name.clone()
                    } else {
                        spdx_license_key.clone()
                    }
                });

                LicenseReference {
                    key: Some(license.key.clone()),
                    language: license.language.clone(),
                    name: license.name.clone(),
                    short_name,
                    owner: license.owner.clone(),
                    homepage_url: license.homepage_url.clone(),
                    spdx_license_key: spdx_license_key.clone(),
                    other_spdx_license_keys: license.other_spdx_license_keys.clone(),
                    osi_license_key: license.osi_license_key.clone(),
                    text_urls: license.text_urls.clone(),
                    osi_url: license.osi_url.clone(),
                    faq_url: license.faq_url.clone(),
                    other_urls: license.other_urls.clone(),
                    category: license.category.clone(),
                    is_exception: license.is_exception,
                    is_unknown: license.is_unknown,
                    is_generic: license.is_generic,
                    notes: license.notes.clone(),
                    minimum_coverage: license.minimum_coverage,
                    standard_notice: license.standard_notice.clone(),
                    ignorable_copyrights: license.ignorable_copyrights.clone().unwrap_or_default(),
                    ignorable_holders: license.ignorable_holders.clone().unwrap_or_default(),
                    ignorable_authors: license.ignorable_authors.clone().unwrap_or_default(),
                    ignorable_urls: license.ignorable_urls.clone().unwrap_or_default(),
                    ignorable_emails: license.ignorable_emails.clone().unwrap_or_default(),
                    scancode_url: Some(format!(
                        "{SCANCODE_LICENSE_URL_BASE}/{}.LICENSE",
                        license.key
                    )),
                    licensedb_url: Some(format_license_reference_url(
                        license_url_template,
                        &license.key,
                    )),
                    spdx_url: (!spdx_license_key.is_empty()
                        && !spdx_license_key.starts_with("LicenseRef-scancode-"))
                    .then(|| format!("{SPDX_LICENSE_URL_BASE}/{}", spdx_license_key)),
                    text: license.text.clone(),
                }
            })
        })
        .collect();

    let license_rule_references = rule_identifiers
        .into_iter()
        .filter_map(|identifier| {
            rules_by_identifier.get(identifier.as_str()).map(|rule| {
                let is_synthetic = is_synthetic_rule(rule);
                let metadata = license_index
                    .rule_metadata_by_identifier
                    .get(identifier.as_str());
                LicenseRuleReference {
                    identifier: rule.identifier.clone(),
                    license_expression: rule.license_expression.clone(),
                    is_license_text: rule.is_license_text(),
                    is_license_notice: rule.is_license_notice(),
                    is_license_reference: rule.is_license_reference(),
                    is_license_tag: rule.is_license_tag(),
                    is_license_clue: rule.is_license_clue(),
                    is_license_intro: rule.is_license_intro(),
                    language: rule.language.clone(),
                    rule_url: (!is_synthetic).then(|| rule.rule_url()).flatten(),
                    is_required_phrase: rule.is_required_phrase,
                    skip_for_required_phrase_generation: metadata
                        .map(|metadata| metadata.skip_for_required_phrase_generation)
                        .unwrap_or(false),
                    replaced_by: metadata
                        .map(|metadata| metadata.replaced_by.clone())
                        .unwrap_or_default(),
                    is_continuous: rule.is_continuous,
                    is_synthetic,
                    is_from_license: rule.is_from_license,
                    length: rule.tokens.len(),
                    relevance: Some(rule.relevance),
                    minimum_coverage: rule.minimum_coverage,
                    referenced_filenames: rule.referenced_filenames.clone().unwrap_or_default(),
                    notes: rule.notes.clone(),
                    ignorable_copyrights: rule.ignorable_copyrights.clone().unwrap_or_default(),
                    ignorable_holders: rule.ignorable_holders.clone().unwrap_or_default(),
                    ignorable_authors: rule.ignorable_authors.clone().unwrap_or_default(),
                    ignorable_urls: rule.ignorable_urls.clone().unwrap_or_default(),
                    ignorable_emails: rule.ignorable_emails.clone().unwrap_or_default(),
                    text: Some(rule.text.clone()),
                }
            })
        })
        .collect();

    (license_references, license_rule_references)
}

fn format_license_reference_url(template: &str, license_key: &str) -> String {
    template.replacen("{}", license_key, 1)
}

fn collect_license_keys_from_package_data(
    package_data: &PackageData,
    license_keys: &mut BTreeSet<String>,
) {
    collect_license_keys_from_expression(
        package_data.declared_license_expression.as_deref(),
        license_keys,
    );
    collect_license_keys_from_expression(
        package_data.other_license_expression.as_deref(),
        license_keys,
    );
    collect_license_keys_from_detections(&package_data.license_detections, license_keys);
    collect_license_keys_from_detections(&package_data.other_license_detections, license_keys);
}

fn collect_license_keys_from_detections(
    detections: &[LicenseDetection],
    license_keys: &mut BTreeSet<String>,
) {
    for detection in detections {
        collect_license_keys_from_expression(Some(&detection.license_expression), license_keys);
    }
}

fn collect_license_keys_from_expression(
    expression: Option<&str>,
    license_keys: &mut BTreeSet<String>,
) {
    let Some(expression) = expression else {
        return;
    };

    if let Ok(parsed) = parse_expression(expression) {
        for key in parsed.license_keys() {
            license_keys.insert(key);
        }
    }
}

fn collect_rule_identifiers_from_detections(
    detections: &[LicenseDetection],
    rule_identifiers: &mut BTreeSet<String>,
) {
    for detection in detections {
        collect_rule_identifiers_from_matches(&detection.matches, rule_identifiers);
    }
}

fn collect_rule_identifiers_from_matches(
    matches: &[Match],
    rule_identifiers: &mut BTreeSet<String>,
) {
    for license_match in matches {
        if let Some(rule_identifier) = license_match.rule_identifier.as_ref() {
            rule_identifiers.insert(rule_identifier.clone());
        }
    }
}

fn is_synthetic_rule(rule: &crate::license_detection::models::Rule) -> bool {
    rule.identifier.starts_with("spdx-license-identifier-")
        || rule.identifier.starts_with("spdx_license_id_")
}

fn build_classification_context(files: &[FileInfo], packages: &[Package]) -> ClassificationContext {
    ClassificationContext {
        package_roots: build_package_roots(packages),
        package_file_references: build_package_file_reference_map(files),
        scan_roots: build_scan_roots(files),
        package_data_top_level_dirs: build_package_data_top_level_dirs(files),
    }
}

fn classify_file(
    file: &FileInfo,
    classification_context: &ClassificationContext,
) -> FileClassification {
    let path = Path::new(&file.path);
    let is_manifest = file.file_type == FileType::File
        && (!file.package_data.is_empty() || is_manifest_file(&file.path));
    let is_scan_root_top_level = is_scan_top_level(path, &classification_context.scan_roots);
    let is_referenced = file.for_packages.iter().any(|uid| {
        classification_context
            .package_file_references
            .get(uid)
            .is_some_and(|refs| refs.contains(file.path.as_str()))
    });
    let is_root_top_level = file.for_packages.iter().any(|uid| {
        if file.file_type == FileType::File && !file.package_data.is_empty() {
            return false;
        }

        classification_context
            .package_roots
            .get(uid)
            .and_then(|root| path.strip_prefix(root).ok())
            .is_some_and(|relative| relative.components().count() == 1)
    });
    let is_package_data_top_level = if file.file_type == FileType::Directory {
        classification_context
            .package_data_top_level_dirs
            .contains(file.path.as_str())
    } else {
        (!file.package_data.is_empty() && !file.for_packages.is_empty() && is_manifest)
            || path
                .parent()
                .and_then(|parent| parent.to_str())
                .is_some_and(|parent| {
                    classification_context
                        .package_data_top_level_dirs
                        .contains(parent)
                })
    };
    let is_top_level =
        is_scan_root_top_level || is_referenced || is_root_top_level || is_package_data_top_level;
    let is_legal = file.file_type == FileType::File && is_legal_file(file);
    let is_readme = file.file_type == FileType::File && is_readme_file(file);
    let is_community = file.file_type == FileType::File && is_community_file(file);
    let is_key_file =
        file.file_type == FileType::File && is_top_level && (is_legal || is_manifest || is_readme);

    FileClassification {
        is_legal,
        is_manifest,
        is_readme,
        is_top_level,
        is_key_file,
        is_community,
    }
}

fn apply_file_classification(
    files: &mut [FileInfo],
    classification_context: &ClassificationContext,
) {
    for file in files.iter_mut() {
        let classification = classify_file(file, classification_context);
        file.is_legal = classification.is_legal;
        file.is_manifest = classification.is_manifest;
        file.is_readme = classification.is_readme;
        file.is_top_level = classification.is_top_level;
        file.is_key_file = classification.is_key_file;
        file.is_community = classification.is_community;
    }
}

#[cfg(test)]
fn classify_key_files(files: &mut [FileInfo], packages: &[Package]) {
    let classification_context = build_classification_context(files, packages);
    apply_file_classification(files, &classification_context);
}

fn build_output_indexes(
    files: &[FileInfo],
    classification_context: Option<&ClassificationContext>,
    use_fallback_key_classification: bool,
) -> OutputIndexes {
    let mut indexes = OutputIndexes::default();

    for (idx, file) in files.iter().enumerate() {
        indexes
            .first_file_index_by_path
            .entry(file.path.clone())
            .or_insert(idx);

        let is_key_file = if use_fallback_key_classification {
            classification_context.is_some_and(|context| classify_file(file, context).is_key_file)
        } else {
            file.is_key_file
        };

        if is_key_file {
            for package_uid in &file.for_packages {
                indexes
                    .key_file_indices_by_package_uid
                    .entry(package_uid.clone())
                    .or_default()
                    .push(idx);
            }
        }
    }

    indexes
}

fn build_package_data_top_level_dirs(files: &[FileInfo]) -> HashSet<String> {
    let mut top_level_dirs = HashSet::new();

    for file in files.iter().filter(|file| {
        file.file_type == FileType::File
            && !file.package_data.is_empty()
            && !file.for_packages.is_empty()
    }) {
        let path = Path::new(&file.path);
        if path.components().count() <= 2 {
            continue;
        }
        for ancestor in path.ancestors().skip(1) {
            let Some(ancestor_str) = ancestor.to_str() else {
                continue;
            };
            if ancestor_str.is_empty() {
                continue;
            }
            top_level_dirs.insert(ancestor_str.to_string());
        }
    }

    top_level_dirs
}

fn build_package_roots(packages: &[Package]) -> HashMap<String, PathBuf> {
    let mut roots = HashMap::new();
    for package in packages {
        if let Some(root) = package_root(package) {
            roots.insert(package.package_uid.clone(), root);
        }
    }
    roots
}

fn package_root(package: &Package) -> Option<PathBuf> {
    for datafile_path in &package.datafile_paths {
        let path = Path::new(datafile_path);

        if path.file_name().and_then(|n| n.to_str()) == Some("metadata.gz-extract") {
            return path.parent().map(|p| p.to_path_buf());
        }

        if path
            .components()
            .any(|c| c.as_os_str() == "data.gz-extract")
        {
            let mut current = path;
            while let Some(parent) = current.parent() {
                if parent.file_name().and_then(|n| n.to_str()) == Some("data.gz-extract") {
                    return parent.parent().map(|p| p.to_path_buf());
                }
                current = parent;
            }
        }

        if let Some(parent) = path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    None
}

fn build_scan_roots(files: &[FileInfo]) -> Vec<PathBuf> {
    let parent_dirs: Vec<PathBuf> = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .map(|file| {
            Path::new(&file.path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
        })
        .map(Path::to_path_buf)
        .collect();

    let mut roots: Vec<PathBuf> = if parent_dirs.iter().any(|path| path.as_os_str().is_empty()) {
        vec![PathBuf::new()]
    } else {
        lowest_common_parent_path(&parent_dirs)
            .into_iter()
            .collect()
    };

    if roots.is_empty() {
        for file in files {
            let mut components = Path::new(&file.path).components();
            let Some(first) = components.next() else {
                continue;
            };

            let root = PathBuf::from(first.as_os_str());
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }

    roots
}

fn lowest_common_parent_path(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut paths_iter = paths.iter();
    let first = paths_iter.next()?;
    let mut common_components: Vec<_> = first.components().collect();

    for path in paths_iter {
        let current_components: Vec<_> = path.components().collect();
        let shared_len = common_components
            .iter()
            .zip(current_components.iter())
            .take_while(|(left, right)| left == right)
            .count();
        common_components.truncate(shared_len);
        if common_components.is_empty() {
            break;
        }
    }

    (!common_components.is_empty()).then(|| {
        let mut common_path = PathBuf::new();
        for component in common_components {
            common_path.push(component.as_os_str());
        }
        common_path
    })
}

fn is_scan_top_level(path: &Path, scan_roots: &[PathBuf]) -> bool {
    if path.components().count() == 1 {
        return true;
    }

    scan_roots.iter().any(|root| {
        path == root
            || root.starts_with(path)
            || path
                .strip_prefix(root)
                .ok()
                .is_some_and(|relative| relative.components().count() == 1)
    })
}

fn build_package_file_reference_map(files: &[FileInfo]) -> HashMap<String, HashSet<String>> {
    let mut mapping: HashMap<String, HashSet<String>> = HashMap::new();

    for file in files {
        if file.package_data.is_empty() || file.for_packages.is_empty() {
            continue;
        }

        for package_uid in &file.for_packages {
            let refs = mapping.entry(package_uid.clone()).or_default();
            for pkg_data in &file.package_data {
                for file_ref in &pkg_data.file_references {
                    refs.insert(file_ref.path.clone());
                }
            }
        }
    }

    mapping
}

const LEGAL_STARTS_ENDS: &[&str] = &[
    "copying",
    "copyright",
    "copyrights",
    "copyleft",
    "notice",
    "license",
    "licenses",
    "licence",
    "licences",
    "licensing",
    "licencing",
    "legal",
    "eula",
    "agreement",
    "patent",
    "patents",
];

const MANIFEST_ENDS: &[&str] = &[
    ".about",
    "/bower.json",
    "/project.clj",
    ".podspec",
    "/composer.json",
    "/description",
    "/elm-package.json",
    "/+compact_manifest",
    "+manifest",
    ".gemspec",
    "/metadata",
    "/metadata.gz-extract",
    "/build.gradle",
    ".cabal",
    "/haxelib.json",
    "/package.json",
    ".nuspec",
    ".pod",
    "/meta.yml",
    "/dist.ini",
    "/pipfile",
    "/setup.cfg",
    "/setup.py",
    "/pkg-info",
    "/pyproject.toml",
    ".spec",
    "/cargo.toml",
    ".spdx",
    "/dependencies",
    "debian/copyright",
    "meta-inf/manifest.mf",
];

fn name_or_base_name_matches(file: &FileInfo, patterns: &[&str]) -> bool {
    let name = file.name.to_ascii_lowercase();
    let base_name = file.base_name.to_ascii_lowercase();

    patterns.iter().any(|pattern| {
        name.starts_with(pattern)
            || name.ends_with(pattern)
            || base_name.starts_with(pattern)
            || base_name.ends_with(pattern)
    })
}

fn is_legal_file(file: &FileInfo) -> bool {
    name_or_base_name_matches(file, LEGAL_STARTS_ENDS)
}

fn is_manifest_file(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    MANIFEST_ENDS.iter().any(|ending| lowered.ends_with(ending))
}

fn is_readme_file(file: &FileInfo) -> bool {
    name_or_base_name_matches(file, &["readme"])
}

fn is_community_file(file: &FileInfo) -> bool {
    let clean = |s: &str| s.replace(['_', '-'], "").to_ascii_lowercase();
    let candidates = [clean(&file.name), clean(&file.base_name)];
    [
        "changelog",
        "roadmap",
        "contributing",
        "codeofconduct",
        "authors",
        "security",
        "funding",
    ]
    .iter()
    .any(|prefix| {
        candidates
            .iter()
            .any(|candidate| candidate.starts_with(prefix) || candidate.ends_with(prefix))
    })
}

const FACETS: [&str; 6] = ["core", "dev", "tests", "docs", "data", "examples"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum FacetMatchTarget {
    Path,
    NameOrPath,
}

#[derive(Clone)]
pub(crate) struct FacetRule {
    facet_index: usize,
    target: FacetMatchTarget,
    pattern: Pattern,
}

pub(crate) fn build_facet_rules(facets: &[String]) -> Result<Vec<FacetRule>> {
    let mut rules = Vec::new();

    for facet_def in facets {
        let Some((raw_facet, raw_pattern)) = facet_def.split_once('=') else {
            return Err(anyhow!(
                "Invalid --facet option: missing <pattern> in \"{}\"",
                facet_def
            ));
        };

        let facet = raw_facet.trim().to_ascii_lowercase();
        let pattern_text = raw_pattern.trim();

        if facet.is_empty() {
            return Err(anyhow!(
                "Invalid --facet option: missing <facet> in \"{}\"",
                facet_def
            ));
        }

        if pattern_text.is_empty() {
            return Err(anyhow!(
                "Invalid --facet option: missing <pattern> in \"{}\"",
                facet_def
            ));
        }

        let Some(facet_index) = FACETS.iter().position(|candidate| *candidate == facet) else {
            return Err(anyhow!(
                "Invalid --facet option: unknown <facet> in \"{}\". Valid values are: {}",
                facet_def,
                FACETS.join(", ")
            ));
        };

        let pattern = Pattern::new(pattern_text).map_err(|err| {
            anyhow!(
                "Invalid --facet option: bad glob pattern in \"{}\": {}",
                facet_def,
                err
            )
        })?;

        let target = if pattern_text.contains('/') || pattern_text.contains('\\') {
            FacetMatchTarget::Path
        } else {
            FacetMatchTarget::NameOrPath
        };

        if !rules.iter().any(|rule: &FacetRule| {
            rule.facet_index == facet_index && rule.pattern.as_str() == pattern_text
        }) {
            rules.push(FacetRule {
                facet_index,
                target,
                pattern,
            });
        }
    }

    Ok(rules)
}

fn assign_facets(files: &mut [FileInfo], facet_rules: &[FacetRule]) {
    if facet_rules.is_empty() {
        return;
    }

    for file in files.iter_mut() {
        if file.file_type != FileType::File {
            file.facets.clear();
            continue;
        }

        const FACET_SORT_ORDER: [usize; FACETS.len()] = [0, 4, 1, 3, 5, 2];
        let mut matched_facets = [false; FACETS.len()];
        for rule in facet_rules {
            let is_match = match rule.target {
                FacetMatchTarget::Path => rule.pattern.matches(&file.path),
                FacetMatchTarget::NameOrPath => {
                    rule.pattern.matches(&file.name) || rule.pattern.matches(&file.path)
                }
            };

            if is_match {
                matched_facets[rule.facet_index] = true;
            }
        }

        let facets: Vec<String> = FACET_SORT_ORDER
            .into_iter()
            .filter(|&index| matched_facets[index])
            .map(|index| FACETS[index].to_string())
            .collect();

        file.facets = if facets.is_empty() {
            vec![FACETS[0].to_string()]
        } else {
            facets
        };
    }
}

fn promote_package_metadata_from_key_files(
    files: &[FileInfo],
    packages: &mut [Package],
    indexes: &OutputIndexes,
) {
    for package in packages.iter_mut() {
        let Some(key_file_indices) = indexes
            .key_file_indices_by_package_uid
            .get(&package.package_uid)
        else {
            continue;
        };

        if package.copyright.is_none() {
            package.copyright = key_file_indices
                .iter()
                .filter_map(|index| files.get(*index))
                .flat_map(|file| file.copyrights.iter())
                .map(|copyright| copyright.copyright.clone())
                .next();
        }

        if package.holder.is_none() {
            let promoted_holders = unique(
                &key_file_indices
                    .iter()
                    .filter_map(|index| files.get(*index))
                    .flat_map(|file| file.holders.iter())
                    .map(|holder| holder.holder.clone())
                    .collect::<Vec<_>>(),
            );
            if promoted_holders.len() == 1 {
                package.holder = promoted_holders.into_iter().next();
            }
        }
    }
}

#[cfg(test)]
fn compute_summary(files: &[FileInfo], packages: &[Package]) -> Option<Summary> {
    let indexes = build_output_indexes(files, None, false);
    compute_summary_with_options(files, packages, &indexes, true, true)
}

fn compute_summary_with_options(
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

fn materialize_generated_flags(files: &mut [FileInfo]) {
    for file in files.iter_mut() {
        if file.file_type != FileType::File {
            file.is_generated = Some(false);
            continue;
        }

        if file.is_generated.is_none() {
            file.is_generated = Some(false);
        }
    }
}

#[cfg(test)]
fn mark_generated_files(files: &mut [FileInfo], scanned_root: Option<&Path>) {
    for file in files.iter_mut() {
        if file.file_type != FileType::File {
            file.is_generated = Some(false);
            continue;
        }

        if file.is_generated.is_none() {
            file.is_generated =
                Some(generated_file_hint_exists(&file.path, scanned_root).unwrap_or(false));
        }
    }
}

fn clear_generated_flags(files: &mut [FileInfo]) {
    for file in files {
        file.is_generated = None;
    }
}

fn clear_resource_tallies(files: &mut [FileInfo]) {
    for file in files {
        file.tallies = None;
    }
}

#[cfg(test)]
fn generated_file_hint_exists(path: &str, scanned_root: Option<&Path>) -> Result<bool> {
    let path = resolve_generated_scan_path(path, scanned_root)?;
    Ok(!generated_code_hints(&path)?.is_empty())
}

#[cfg(test)]
fn resolve_generated_scan_path(path: &str, scanned_root: Option<&Path>) -> Result<PathBuf> {
    let candidate = PathBuf::from(path);

    if candidate.is_absolute() {
        return candidate
            .is_file()
            .then_some(candidate)
            .ok_or_else(|| anyhow!("Generated detection path not found: {}", path));
    }

    let Some(scanned_root) = scanned_root else {
        return Err(anyhow!(
            "Generated detection fallback requires an absolute path or scanned root: {}",
            path
        ));
    };

    let anchored = scanned_root.join(&candidate);
    if anchored.is_file() {
        return Ok(anchored);
    }

    Err(anyhow!("Generated detection path not found: {}", path))
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
                            .first_file_index_by_path
                            .get(datafile_path)
                            .and_then(|index| files.get(*index))
                            .and_then(|file| file.license_expression.clone())
                    })
                })
            }),
    ))
    .map(|expr| canonicalize_summary_expression(&expr))
}

fn compute_file_tallies(files: &mut [FileInfo]) {
    for file in files.iter_mut() {
        if file.file_type == FileType::File {
            file.tallies = Some(compute_direct_file_tallies(file));
        } else {
            file.tallies = None;
        }
    }
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

fn is_good_match(license_match: &Match) -> bool {
    match (license_match.match_coverage, license_match.rule_relevance) {
        (Some(coverage), Some(relevance)) => {
            license_match.score >= 80.0 && coverage >= 80.0 && relevance >= 80
        }
        _ => license_match.score >= 80.0,
    }
}

#[cfg(test)]
mod tests {
    use super::is_good_match;
    use crate::models::file_info::Match;

    fn make_match(score: f64, coverage: Option<f64>, relevance: Option<usize>) -> Match {
        Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: 1,
            end_line: 1,
            matcher: Some("1-hash".to_string()),
            score,
            matched_length: Some(3),
            match_coverage: coverage,
            rule_relevance: relevance,
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }
    }

    #[test]
    fn test_is_good_match_does_not_upscale_low_percent_scores() {
        let license_match = make_match(1.0, Some(100.0), Some(100));

        assert!(!is_good_match(&license_match));
    }

    #[test]
    fn test_is_good_match_accepts_percent_scores() {
        let license_match = make_match(81.82, Some(100.0), Some(100));

        assert!(is_good_match(&license_match));
    }
}

fn is_score_key_file(file: &FileInfo) -> bool {
    if !file.is_key_file {
        return false;
    }

    if file.is_manifest {
        return is_score_manifest(file);
    }

    true
}

fn is_score_manifest(file: &FileInfo) -> bool {
    let path = file.path.to_ascii_lowercase();
    path == "cargo.toml"
        || path.ends_with("/cargo.toml")
        || path.ends_with("/pom.xml")
        || path.ends_with("/pom.properties")
        || path == "manifest.mf"
        || path.ends_with("/manifest.mf")
        || path == "metadata.gz-extract"
        || path.ends_with("/metadata.gz-extract")
        || path.ends_with(".gemspec")
}

fn unique(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique_values = Vec::new();

    for value in values {
        if seen.insert(value.clone()) {
            unique_values.push(value.clone());
        }
    }

    unique_values
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

fn get_primary_license(declared_license_expressions: &[String]) -> Option<String> {
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

fn remove_tally_value(value: Option<&str>, tallies: &[TallyEntry]) -> Vec<TallyEntry> {
    tallies
        .iter()
        .filter(|entry| {
            !entry
                .value
                .as_deref()
                .is_some_and(|entry_value| is_redundant_declared_license_tally(entry_value, value))
        })
        .cloned()
        .collect()
}

fn is_redundant_declared_license_tally(entry_value: &str, declared_value: Option<&str>) -> bool {
    let Some(declared_value) = declared_value else {
        return false;
    };

    if entry_value == declared_value {
        return true;
    }

    if declared_value.contains(" AND ")
        || declared_value.contains(" OR ")
        || declared_value.contains(" WITH ")
    {
        return false;
    }

    let normalized_declared = declared_value.trim().to_ascii_lowercase();
    let parts: Vec<String> = entry_value
        .replace(['(', ')'], " ")
        .split_whitespace()
        .filter(|part| !matches!(part.to_ascii_uppercase().as_str(), "AND" | "OR" | "WITH"))
        .map(|part| part.to_ascii_lowercase())
        .collect();

    !parts.is_empty() && parts.iter().all(|part| part == &normalized_declared)
}

fn remove_tally_values(values: &[String], tallies: &[TallyEntry]) -> Vec<TallyEntry> {
    let normalized_values: HashSet<String> = values
        .iter()
        .map(|value| normalize_summary_holder_value(value))
        .collect();

    tallies
        .iter()
        .filter(|entry| {
            !entry.value.as_ref().is_some_and(|value| {
                values.contains(value)
                    || normalized_values.contains(&normalize_summary_holder_value(value))
            })
        })
        .cloned()
        .collect()
}

fn canonicalize_summary_expression(expression: &str) -> String {
    let canonical = parse_expression(expression)
        .map(|parsed| expression_to_string(&simplify_expression(&parsed)))
        .or_else(|_| combine_expressions_and(&[expression], true))
        .unwrap_or_else(|_| expression.to_ascii_lowercase());

    if canonical.contains(" AND ") && !canonical.contains(" OR ") && !canonical.contains(" WITH ") {
        canonical
            .replace(['(', ')'], "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        canonical
    }
}

fn normalize_summary_holder_value(value: &str) -> String {
    let normalized = canonicalize_summary_holder_display(value)
        .trim_end_matches(['.', ',', ';', ':'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    let key: String = normalized
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();

    match key.as_str() {
        "google" | "googlellc" | "googleinc" => "google".to_string(),
        "microsoft" | "microsoftcorp" | "microsoftinc" | "microsoftcorporation" => {
            "microsoft".to_string()
        }
        _ => normalized,
    }
}

fn canonicalize_summary_holder_display(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");

    let key: String = normalized
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();

    match key.as_str() {
        "google" | "googlellc" | "googleinc" => "Google".to_string(),
        "microsoft" | "microsoftcorp" | "microsoftinc" | "microsoftcorporation" => {
            "Microsoft".to_string()
        }
        "sunmicrosystems" | "sunmicrosystemsinc" => "Sun Microsystems".to_string(),
        _ => normalized,
    }
}

fn summary_holder_from_copyright(copyright: &str) -> Option<String> {
    let mut value = copyright.trim();
    if value.is_empty() {
        return None;
    }

    if value.len() >= "copyright".len()
        && value[.."copyright".len()].eq_ignore_ascii_case("copyright")
    {
        value = value["copyright".len()..].trim_start();
    }

    if let Some(stripped) = value.strip_prefix("(c)") {
        value = stripped.trim_start();
    }
    if let Some(stripped) = value.strip_prefix('©') {
        value = stripped.trim_start();
    }

    let cleaned = value.trim_matches(|ch: char| ch.is_whitespace() || ch == ',');
    if cleaned.is_empty() {
        return None;
    }

    if cleaned.starts_with("Holders ") || cleaned.contains("option either") {
        return None;
    }

    let cleaned = cleaned
        .strip_suffix(". Individual")
        .unwrap_or(cleaned)
        .trim();

    let cleaned = if cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        cleaned
            .trim_start_matches(|ch: char| {
                ch.is_ascii_digit() || ch == ' ' || ch == ',' || ch == '-'
            })
            .trim()
    } else {
        cleaned
    };

    let cleaned_without_email = cleaned
        .split_whitespace()
        .take_while(|token| !token.contains('@'))
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = if cleaned_without_email.is_empty() {
        cleaned
    } else {
        cleaned_without_email.as_str()
    };

    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn clean_legal_holder_candidate(holder: &str) -> Option<String> {
    let cleaned = holder.trim();
    if cleaned.is_empty()
        || cleaned.contains("option either")
        || cleaned.starts_with("messages,")
        || cleaned.starts_with("together with instructions")
    {
        return None;
    }

    let cleaned = cleaned
        .strip_suffix(". Individual")
        .unwrap_or(cleaned)
        .trim();

    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn summary_license_expression(file: &FileInfo) -> Option<String> {
    let mut detection_expressions: Vec<_> = file
        .license_detections
        .iter()
        .map(|detection| detection.license_expression.clone())
        .collect();

    if detection_expressions.is_empty() {
        detection_expressions.extend(
            file.package_data
                .iter()
                .flat_map(|package_data| package_data.license_detections.iter())
                .map(|detection| detection.license_expression.clone()),
        );
    }

    let detection_expressions = unique(&detection_expressions);

    if !detection_expressions.is_empty() {
        return if detection_expressions.len() == 1 {
            detection_expressions
                .into_iter()
                .next()
                .map(|expr| canonicalize_summary_expression(&expr))
        } else {
            combine_license_expressions(detection_expressions)
                .map(|expr| canonicalize_summary_expression(&expr))
        };
    }

    file.license_expression
        .as_deref()
        .map(canonicalize_summary_expression)
}

fn package_primary_detected_license_values(file: &FileInfo, skip_unknown: bool) -> Vec<String> {
    if !file.license_detections.is_empty() {
        return Vec::new();
    }

    let mut values = file
        .package_data
        .iter()
        .flat_map(|package_data| {
            package_data
                .license_detections
                .iter()
                .map(|detection| canonicalize_summary_expression(&detection.license_expression))
                .chain(
                    package_data
                        .declared_license_expression
                        .as_deref()
                        .map(canonicalize_summary_expression),
                )
        })
        .collect::<Vec<_>>();

    if skip_unknown {
        values.retain(|expression| expression != "unknown-license-reference");
    }

    unique(&values)
}

fn package_other_detected_license_values(file: &FileInfo, skip_unknown: bool) -> Vec<String> {
    let mut values = file
        .package_data
        .iter()
        .flat_map(|package_data| {
            package_data
                .other_license_detections
                .iter()
                .map(|detection| canonicalize_summary_expression(&detection.license_expression))
                .chain(
                    package_data
                        .other_license_expression
                        .as_deref()
                        .map(canonicalize_summary_expression),
                )
        })
        .collect::<Vec<_>>();

    if skip_unknown {
        values.retain(|expression| expression != "unknown-license-reference");
    }

    unique(&values)
}

fn key_file_has_license_text(file: &FileInfo) -> bool {
    file.license_detections
        .iter()
        .chain(
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
        .flat_map(|detection| detection.matches.iter())
        .any(|m| {
            m.matched_length.unwrap_or_default() > 1 || m.match_coverage.unwrap_or_default() > 1.0
        })
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

fn compute_tallies(files: &[FileInfo]) -> Option<Tallies> {
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

fn compute_key_file_tallies(files: &[FileInfo]) -> Option<Tallies> {
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

fn compute_tallies_by_facet(files: &[FileInfo]) -> Option<Vec<FacetTallies>> {
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

fn compute_detailed_tallies(files: &mut [FileInfo]) {
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

fn tally_file_values<F>(
    files: &[FileInfo],
    values_for_file: F,
    count_missing_files: bool,
) -> Vec<TallyEntry>
where
    F: Fn(&FileInfo) -> Vec<String>,
{
    tally_file_values_filtered(files, |_| true, values_for_file, count_missing_files)
}

fn tally_file_values_filtered<P, F>(
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

fn summary_detected_license_values(file: &FileInfo) -> Vec<String> {
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

fn copyright_values(file: &FileInfo) -> Vec<String> {
    if is_legal_file(file) {
        return Vec::new();
    }

    file.copyrights
        .iter()
        .map(|copyright| normalize_tally_copyright_value(&copyright.copyright))
        .collect()
}

fn holder_values(file: &FileInfo) -> Vec<String> {
    if is_legal_file(file) {
        return Vec::new();
    }

    file.holders
        .iter()
        .map(|holder| normalize_tally_holder_value(&holder.holder))
        .collect()
}

fn author_values(file: &FileInfo) -> Vec<String> {
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

fn programming_language_values(file: &FileInfo) -> Vec<String> {
    file.programming_language
        .as_deref()
        .filter(|language| !matches!(*language, "Text" | "JSON"))
        .map(str::to_string)
        .into_iter()
        .collect()
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

fn build_tally_entries(counts: HashMap<Option<String>, usize>) -> Vec<TallyEntry> {
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

fn compute_declared_holders(
    files: &[FileInfo],
    packages: &[Package],
    indexes: &OutputIndexes,
) -> Vec<String> {
    let mut package_datafile_holders = Vec::new();
    for package in packages {
        for datafile_path in &package.datafile_paths {
            if let Some(file) = indexes
                .first_file_index_by_path
                .get(datafile_path)
                .and_then(|index| files.get(*index))
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
                    .first_file_index_by_path
                    .get(datafile_path)
                    .and_then(|index| files.get(*index))
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

fn top_level_summary_package_roots(packages: &[Package]) -> Vec<PathBuf> {
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

fn nested_summary_package_roots(packages: &[Package], files: &[FileInfo]) -> Vec<PathBuf> {
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
