use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::font_policy::{is_font_asset_path, is_font_license_file_name};
use crate::license_detection::detection::{
    FileRegion as InternalFileRegion, determine_license_expression, determine_spdx_expression,
    select_matches_for_expression,
};
use crate::license_detection::expression::parse_expression;
use crate::models::{
    FileInfo, FileType, LicenseDetection, Match, Package, PackageUid, TopLevelLicenseDetection,
};
use crate::utils::spdx::combine_license_expressions;

use super::classification::is_legal_file;
use super::package_file_index::PackageFileIndex;

const INHERIT_LICENSE_FROM_PACKAGE_REFERENCE: &str = "INHERIT_LICENSE_FROM_PACKAGE";
const DETECTION_LOG_UNKNOWN_REFERENCE_TO_LOCAL_FILE: &str = "unknown-reference-to-local-file";
const DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_PACKAGE: &str =
    "unknown-reference-in-file-to-package";
const DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_NONEXISTENT_PACKAGE: &str =
    "unknown-reference-in-file-to-nonexistent-package";

#[derive(Debug, Clone)]
pub(super) struct ResolvedReferenceTarget {
    pub(super) path: String,
    detections: Vec<LicenseDetection>,
    preserve_match_from_file: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ReferenceFollowSnapshot {
    all_file_paths: HashSet<String>,
    files_by_path: HashMap<String, ResolvedReferenceTarget>,
    package_targets_by_uid: HashMap<PackageUid, ResolvedReferenceTarget>,
    package_manifest_dirs_by_uid: HashMap<PackageUid, Vec<String>>,
    same_directory_legal_targets_by_dir: HashMap<String, Vec<ResolvedReferenceTarget>>,
    root_license_targets_by_root: HashMap<String, Vec<ResolvedReferenceTarget>>,
    root_paths: Vec<String>,
}

pub(crate) fn apply_package_reference_following(files: &mut [FileInfo], packages: &mut [Package]) {
    for _ in 0..5 {
        let snapshot = build_reference_follow_snapshot(files, packages);
        let package_file_index = PackageFileIndex::build(files, packages);
        let mut modified = false;

        for file in files
            .iter_mut()
            .filter(|file| file.file_type == FileType::File)
        {
            if follow_references_for_file(file, &snapshot) {
                modified = true;
            }
            if inherit_same_directory_legal_detections_for_file(file, &snapshot) {
                modified = true;
            }
        }

        if sync_packages_from_followed_package_data(files, packages, &package_file_index) {
            modified = true;
        }

        if !modified {
            break;
        }
    }
}

pub(crate) fn collect_top_level_license_detections(
    files: &[FileInfo],
) -> Vec<TopLevelLicenseDetection> {
    #[derive(Clone, Copy)]
    struct RepresentativeDetection<'a> {
        detection: &'a LicenseDetection,
    }

    struct AggregatedDetection<'a> {
        representative: RepresentativeDetection<'a>,
        seen_regions: HashSet<(String, usize, usize)>,
        detection_count: usize,
    }

    let mut detections_by_identifier: HashMap<String, AggregatedDetection<'_>> = HashMap::new();

    for file in files {
        let mut file_detections = file.license_detections.iter().collect::<Vec<_>>();
        for package_data in &file.package_data {
            file_detections.extend(package_data.license_detections.iter());
            file_detections.extend(package_data.other_license_detections.iter());
        }

        for detection in file_detections {
            let Some(identifier) = detection.identifier.as_ref() else {
                continue;
            };

            let entry = detections_by_identifier
                .entry(identifier.clone())
                .or_insert_with(|| AggregatedDetection {
                    representative: RepresentativeDetection { detection },
                    seen_regions: HashSet::new(),
                    detection_count: 0,
                });

            if entry.representative.detection.detection_log.is_empty()
                && !detection.detection_log.is_empty()
            {
                entry.representative = RepresentativeDetection { detection };
            }

            if let Some(region_key) = public_detection_region_key(detection, &file.path)
                && entry.seen_regions.insert(region_key)
            {
                entry.detection_count += 1;
            }
        }
    }

    let mut unique_detections: Vec<_> = detections_by_identifier
        .into_iter()
        .map(|(identifier, aggregated)| {
            let representative = aggregated.representative.detection;
            let reference_matches = representative
                .matches
                .iter()
                .map(public_match_to_internal)
                .map(internal_match_to_public)
                .collect::<Vec<_>>();
            let representative_internal_matches = representative
                .matches
                .iter()
                .map(public_match_to_internal)
                .collect::<Vec<_>>();
            let license_expression = if representative.license_expression.is_empty() {
                determine_license_expression(&representative_internal_matches, None)
                    .unwrap_or_default()
            } else {
                representative.license_expression.clone()
            };
            let license_expression_spdx = if representative.license_expression_spdx.is_empty() {
                determine_spdx_expression(&representative_internal_matches, None)
                    .unwrap_or_default()
            } else {
                representative.license_expression_spdx.clone()
            };

            TopLevelLicenseDetection {
                identifier,
                license_expression,
                license_expression_spdx,
                detection_count: aggregated.detection_count,
                detection_log: representative.detection_log.clone(),
                reference_matches,
            }
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

fn public_detection_region_key(
    detection: &LicenseDetection,
    owning_path: &str,
) -> Option<(String, usize, usize)> {
    let start_line = detection
        .matches
        .iter()
        .map(|match_item| match_item.start_line)
        .min()?;
    let end_line = detection
        .matches
        .iter()
        .map(|match_item| match_item.end_line)
        .max()?;
    Some((owning_path.to_string(), start_line.get(), end_line.get()))
}

pub(super) fn build_reference_follow_snapshot(
    files: &[FileInfo],
    packages: &[Package],
) -> ReferenceFollowSnapshot {
    let all_file_paths = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .map(|file| file.path.clone())
        .collect();

    let files_by_path = files
        .iter()
        .filter(|file| file.file_type == FileType::File)
        .filter(|file| can_be_reference_source(&file.license_detections))
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
            if !can_be_reference_source(&package.license_detections) {
                return None;
            }

            let package_expression = combine_detection_expressions(&package.license_detections)?;
            if !is_resolved_package_context_expression(&package_expression) {
                return None;
            }

            let path = package
                .datafile_paths
                .first()
                .cloned()
                .unwrap_or_else(|| package.package_uid.to_string());

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
    let same_directory_legal_targets_by_dir = build_same_directory_legal_targets(files);
    let root_license_targets_by_root = build_root_license_targets(files, &root_paths);

    ReferenceFollowSnapshot {
        all_file_paths,
        files_by_path,
        package_targets_by_uid,
        package_manifest_dirs_by_uid,
        same_directory_legal_targets_by_dir,
        root_license_targets_by_root,
        root_paths,
    }
}

fn build_same_directory_legal_targets(
    files: &[FileInfo],
) -> HashMap<String, Vec<ResolvedReferenceTarget>> {
    let mut targets_by_dir: HashMap<String, Vec<ResolvedReferenceTarget>> = HashMap::new();

    for file in files {
        if file.file_type != FileType::File
            || file.license_detections.is_empty()
            || !is_same_directory_legal_target(file)
            || !can_be_reference_source(&file.license_detections)
        {
            continue;
        }

        let Some(expression) = combine_detection_expressions(&file.license_detections) else {
            continue;
        };
        if !is_resolved_package_context_expression(&expression) {
            continue;
        }

        let directory = parent_directory(&file.path);
        targets_by_dir
            .entry(directory)
            .or_default()
            .push(ResolvedReferenceTarget {
                path: file.path.clone(),
                detections: file.license_detections.clone(),
                preserve_match_from_file: false,
            });
    }

    for targets in targets_by_dir.values_mut() {
        targets.sort_by(|left, right| {
            root_license_candidate_priority(&left.path)
                .cmp(&root_license_candidate_priority(&right.path))
                .then_with(|| left.path.cmp(&right.path))
        });
    }

    targets_by_dir
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
            .filter(|file| can_be_reference_source(&file.license_detections))
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

fn parent_directory(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
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

fn can_be_reference_source(detections: &[LicenseDetection]) -> bool {
    !detections.iter().any(detection_was_followed_from_reference)
}

fn detection_was_followed_from_reference(detection: &LicenseDetection) -> bool {
    detection.detection_log.iter().any(|entry| {
        matches!(
            entry.as_str(),
            DETECTION_LOG_UNKNOWN_REFERENCE_TO_LOCAL_FILE
                | DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_PACKAGE
                | DETECTION_LOG_UNKNOWN_REFERENCE_IN_FILE_TO_NONEXISTENT_PACKAGE
        )
    })
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

    if files
        .iter()
        .any(|file| file.file_type == FileType::File && !file.path.contains('/'))
        && !roots.iter().any(String::is_empty)
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

fn inherit_same_directory_legal_detections_for_file(
    file: &mut FileInfo,
    snapshot: &ReferenceFollowSnapshot,
) -> bool {
    if !is_same_directory_legal_inheritance_candidate(file) {
        return false;
    }

    let directory = parent_directory(&file.path);
    let Some(targets) = snapshot.same_directory_legal_targets_by_dir.get(&directory) else {
        return false;
    };

    let inherited_detections: Vec<_> = targets
        .iter()
        .flat_map(|target| {
            target
                .detections
                .iter()
                .cloned()
                .map(|detection| detection_with_match_source(detection, &target.path))
        })
        .collect();
    if inherited_detections.is_empty() {
        return false;
    }

    file.license_detections = inherited_detections;
    file.license_expression = combine_license_expressions(
        file.license_detections
            .iter()
            .map(|detection| detection.license_expression.clone()),
    );
    true
}

fn is_same_directory_legal_inheritance_candidate(file: &FileInfo) -> bool {
    file.file_type == FileType::File
        && file.license_detections.is_empty()
        && file.for_packages.is_empty()
        && is_font_asset_path(Path::new(&file.path))
}

fn is_same_directory_legal_target(file: &FileInfo) -> bool {
    is_legal_file(file) || is_font_license_file(file)
}

fn is_font_license_file(file: &FileInfo) -> bool {
    is_font_license_file_name(&file.name, &file.base_name)
}

fn detection_with_match_source(
    mut detection: LicenseDetection,
    source_path: &str,
) -> LicenseDetection {
    for detection_match in &mut detection.matches {
        detection_match.from_file = Some(source_path.to_string());
    }
    detection
}

fn sync_packages_from_followed_package_data(
    files: &[FileInfo],
    packages: &mut [Package],
    package_file_index: &PackageFileIndex,
) -> bool {
    let package_data_by_path: HashMap<_, _> = files
        .iter()
        .filter(|file| !file.package_data.is_empty())
        .map(|file| (file.path.as_str(), file.package_data.as_slice()))
        .collect();

    let mut modified = false;

    for package in packages {
        for datafile_path in &package.datafile_paths {
            let matched_package_data =
                package_data_by_path
                    .get(datafile_path.as_str())
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

            let manifest_file = package_file_index
                .file_ix_by_path(datafile_path)
                .and_then(|index| files.get(index.0));

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
    package_uids: &[PackageUid],
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
    let referenced_targets: Vec<_> = referenced_targets
        .into_iter()
        .map(|mut target| {
            target.detections = filter_unknown_reference_detections(&target.detections);
            target
        })
        .collect();
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

    let is_placeholder_reference = matches!(
        detection.license_expression.as_str(),
        "unknown-license-reference" | "free-unknown"
    );
    let mut internal_detection = public_detection_to_internal(detection, current_path);
    let mut placeholder_matches = Vec::new();
    if is_placeholder_reference {
        placeholder_matches = internal_detection.matches.clone();
        internal_detection.matches.clear();
    }
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
        true,
    );
    internal_detection.license_expression =
        determine_license_expression(&matches_for_expression, None).ok();
    internal_detection.license_expression_spdx =
        determine_spdx_expression(&matches_for_expression, None).ok();
    internal_detection.detection_log = vec![detection_log.to_string()];
    if !placeholder_matches.is_empty() {
        let mut combined_matches = placeholder_matches;
        combined_matches.extend(internal_detection.matches);
        internal_detection.matches = combined_matches;
    }
    let mut public_detection = internal_detection_to_public(internal_detection);
    public_detection.identifier = None;
    crate::models::file_info::enrich_license_detection_provenance(
        &mut public_detection,
        current_path,
    );
    *detection = public_detection;
    true
}

fn filter_unknown_reference_detections(detections: &[LicenseDetection]) -> Vec<LicenseDetection> {
    let has_concrete_detection = detections.iter().any(|detection| {
        detection.license_expression != "unknown-license-reference"
            && detection.license_expression != "free-unknown"
    });
    if !has_concrete_detection {
        return detections.to_vec();
    }

    detections
        .iter()
        .filter(|detection| {
            detection.license_expression != "unknown-license-reference"
                && detection.license_expression != "free-unknown"
        })
        .map(strip_unknown_reference_matches_from_detection)
        .collect()
}

fn strip_unknown_reference_matches_from_detection(
    detection: &LicenseDetection,
) -> LicenseDetection {
    let has_concrete_match = detection.matches.iter().any(|match_item| {
        match_item.license_expression != "unknown-license-reference"
            && match_item.license_expression != "free-unknown"
    });
    if !has_concrete_match {
        return detection.clone();
    }

    let mut filtered = detection.clone();
    filtered.matches.retain(|match_item| {
        match_item.license_expression != "unknown-license-reference"
            && match_item.license_expression != "free-unknown"
    });
    filtered
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
        .map(|name| sanitize_referenced_filename(&name))
        .filter(|name| {
            !name.is_empty()
                && normalize_referenced_filename(name) != INHERIT_LICENSE_FROM_PACKAGE_REFERENCE
        })
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

fn sanitize_referenced_filename(name: &str) -> String {
    name.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn resolve_referenced_resource(
    referenced_filename: &str,
    current_path: &str,
    package_uids: &[PackageUid],
    snapshot: &ReferenceFollowSnapshot,
) -> Option<ResolvedReferenceTarget> {
    let is_absolute = referenced_filename.trim_start().starts_with('/');
    let referenced_filename = normalize_referenced_filename(referenced_filename);
    if referenced_filename.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();
    if is_absolute {
        candidates.push(referenced_filename.clone());
    }
    if let Some(base) = current_reference_base(current_path) {
        candidates.push(join_reference_candidate(&base, &referenced_filename));
    }

    for package_uid in package_uids {
        if let Some(dirs) = snapshot.package_manifest_dirs_by_uid.get(package_uid) {
            for dir in dirs {
                candidates.push(join_reference_candidate(dir, &referenced_filename));
            }
        }
    }

    if let Some(root) = explicit_reference_root(snapshot) {
        candidates.push(join_reference_candidate(root, &referenced_filename));
    }

    let mut seen = HashSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }

        if let Some(target) = snapshot.files_by_path.get(&candidate) {
            return Some(target.clone());
        }

        if snapshot.all_file_paths.contains(&candidate) {
            return None;
        }
    }

    None
}

fn current_reference_base(current_path: &str) -> Option<String> {
    Path::new(current_path)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn explicit_reference_root(snapshot: &ReferenceFollowSnapshot) -> Option<&str> {
    match snapshot.root_paths.as_slice() {
        [] => None,
        [single_root] => Some(single_root.as_str()),
        _ => Some(""),
    }
}

fn resolve_package_reference_targets(
    current_path: &str,
    package_uids: &[PackageUid],
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
    package_uids: &[PackageUid],
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
    let mut candidate_roots = snapshot
        .root_paths
        .iter()
        .filter(|root| path_is_within_root(current_path, root))
        .collect::<Vec<_>>();
    candidate_roots.sort_by_key(|root| std::cmp::Reverse(root.len()));

    for root in candidate_roots {
        if let Some(targets) = snapshot.root_license_targets_by_root.get(root)
            && let Some(collapsed) = collapse_equivalent_reference_targets(targets.clone())
        {
            return Some(collapsed);
        }
    }

    None
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

pub(super) fn use_referenced_license_expression(
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
        score: detection_match.score,
        matched_length: detection_match.matched_length.unwrap_or_default(),
        rule_length: detection_match.matched_length.unwrap_or_default(),
        match_coverage: detection_match.match_coverage.unwrap_or_default() as f32,
        rule_relevance: detection_match.rule_relevance.unwrap_or_default(),
        rule_identifier: detection_match
            .rule_identifier
            .clone()
            .or_else(|| detection_match.matcher.clone())
            .unwrap_or_default(),
        rule_url: detection_match.rule_url.clone().unwrap_or_default(),
        matched_text: detection_match.matched_text.clone(),
        referenced_filenames: detection_match.referenced_filenames.clone(),
        rule_kind: crate::license_detection::models::RuleKind::None,
        is_from_license: false,
        rule_start_token: 0,
        coordinates: crate::license_detection::models::MatchCoordinates::query_region(
            crate::license_detection::models::PositionSpan::empty(),
        ),
        candidate_resemblance: 0.0,
        candidate_containment: 0.0,
    }
}

fn internal_match_to_public(
    detection_match: crate::license_detection::models::LicenseMatch,
) -> Match {
    let score = detection_match.score;
    let match_coverage = (f64::from(detection_match.coverage()) * 100.0).round() / 100.0;

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
        rule_relevance: Some(detection_match.rule_relevance),
        rule_identifier: (!detection_match.rule_identifier.is_empty())
            .then_some(detection_match.rule_identifier),
        rule_url: (!detection_match.rule_url.is_empty()).then_some(detection_match.rule_url),
        matched_text: detection_match.matched_text,
        referenced_filenames: detection_match.referenced_filenames,
        matched_text_diagnostics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_package_reference_following, collect_top_level_license_detections};
    use crate::models::{LineNumber, Match, MatchScore};
    use crate::post_processing::test_utils::file;

    #[test]
    fn collect_top_level_license_detections_prefers_later_logged_representative() {
        let mut first = file("project/src/lib.rs");
        first.license_detections = vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/lib.rs".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(3).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("mit-shared-id".to_string()),
        }];

        let mut second = file("project/src/other.rs");
        second.license_detections = vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("project/src/other.rs".to_string()),
                start_line: LineNumber::new(4).unwrap(),
                end_line: LineNumber::new(6).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec!["imperfect-match-coverage".to_string()],
            identifier: Some("mit-shared-id".to_string()),
        }];

        let detections = collect_top_level_license_detections(&[first, second]);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].detection_count, 2);
        assert_eq!(
            detections[0].reference_matches[0].from_file.as_deref(),
            Some("project/src/other.rs")
        );
        assert_eq!(
            detections[0].detection_log,
            vec!["imperfect-match-coverage".to_string()]
        );
    }

    #[test]
    fn collect_top_level_license_detections_keeps_identifier_with_zero_match_detection() {
        let mut file = file("project/src/lib.rs");
        file.license_detections = vec![crate::models::LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![],
            detection_log: vec![],
            identifier: Some("mit-empty".to_string()),
        }];

        let detections = collect_top_level_license_detections(&[file]);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].identifier, "mit-empty");
        assert_eq!(detections[0].detection_count, 0);
        assert!(detections[0].reference_matches.is_empty());
    }

    #[test]
    fn same_directory_legal_file_inheritance_applies_to_font_assets() {
        let mut font = file("fonts/Scheherazade-Bold.ttf");
        let mut legal = file("fonts/OFL.txt");
        legal.license_detections = vec![crate::models::LicenseDetection {
            license_expression: "ofl-1.1".to_string(),
            license_expression_spdx: "OFL-1.1".to_string(),
            matches: vec![Match {
                license_expression: "ofl-1.1".to_string(),
                license_expression_spdx: "OFL-1.1".to_string(),
                from_file: Some("fonts/OFL.txt".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(3).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("ofl-1.1_0.RULE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("ofl-1.1-font".to_string()),
        }];
        legal.license_expression = Some("ofl-1.1".to_string());

        let mut files = vec![font.clone(), legal];
        apply_package_reference_following(&mut files, &mut []);
        font = files.remove(0);

        assert_eq!(font.license_expression.as_deref(), Some("ofl-1.1"));
        assert_eq!(font.license_detections.len(), 1);
        assert_eq!(
            font.license_detections[0].matches[0].from_file.as_deref(),
            Some("fonts/OFL.txt")
        );
    }
}
