use std::collections::{HashMap, HashSet};
use std::path::Path;

use rayon::prelude::*;

use crate::license_detection::detection::identifier::compute_detection_identifier;
use crate::license_detection::detection::{
    FileRegion as InternalFileRegion, determine_license_expression, determine_spdx_expression,
    get_unique_detections, select_matches_for_expression,
};
use crate::license_detection::expression::parse_expression;
use crate::models::{
    FileInfo, FileType, LicenseDetection, Match, Package, TopLevelLicenseDetection,
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
pub(crate) struct ResolvedReferenceTarget {
    pub(crate) path: String,
    detections: Vec<LicenseDetection>,
    preserve_match_from_file: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ReferenceFollowSnapshot {
    files_by_path: HashMap<String, ResolvedReferenceTarget>,
    package_targets_by_uid: HashMap<String, ResolvedReferenceTarget>,
    package_manifest_dirs_by_uid: HashMap<String, Vec<String>>,
    root_license_targets_by_root: HashMap<String, Vec<ResolvedReferenceTarget>>,
    root_paths: Vec<String>,
}

pub(crate) fn apply_package_reference_following(files: &mut [FileInfo], packages: &mut [Package]) {
    for _ in 0..5 {
        let snapshot = build_reference_follow_snapshot(files, packages);
        let package_file_index = PackageFileIndex::build(files, packages);
        let mut modified = files
            .par_iter_mut()
            .filter(|file| file.file_type == FileType::File)
            .map(|file| follow_references_for_file(file, &snapshot))
            .reduce(|| false, |left, right| left || right);

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
    let internal_detections: Vec<_> = files
        .par_iter()
        .flat_map_iter(|file| {
            let mut detections = Vec::new();
            detections.extend(
                file.license_detections
                    .iter()
                    .map(|detection| public_detection_to_internal(detection, &file.path)),
            );
            for package_data in &file.package_data {
                detections.extend(
                    package_data
                        .license_detections
                        .iter()
                        .map(|detection| public_detection_to_internal(detection, &file.path)),
                );
                detections.extend(
                    package_data
                        .other_license_detections
                        .iter()
                        .map(|detection| public_detection_to_internal(detection, &file.path)),
                );
            }
            detections.into_iter()
        })
        .collect();

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
                        .or_else(|| determine_license_expression(&detection.matches, None).ok())
                        .unwrap_or_default();
                    let license_expression_spdx = detection
                        .license_expression_spdx
                        .clone()
                        .filter(|expression| !expression.is_empty())
                        .or_else(|| determine_spdx_expression(&detection.matches, None).ok())
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

pub(crate) fn build_reference_follow_snapshot(
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
    let referenced_license_expression = combine_license_expressions(
        referenced_targets
            .iter()
            .flat_map(|target| target.detections.iter())
            .map(|detection| detection.license_expression.clone()),
    );
    let referenced_license_expression_spdx = combine_license_expressions(
        referenced_targets
            .iter()
            .flat_map(|target| target.detections.iter())
            .filter(|detection| !detection.license_expression_spdx.is_empty())
            .map(|detection| detection.license_expression_spdx.clone()),
    );
    internal_detection.license_expression = if is_placeholder_reference {
        referenced_license_expression
            .or_else(|| determine_license_expression(&matches_for_expression, None).ok())
    } else {
        combine_license_expressions(std::iter::once(detection.license_expression.clone()).chain(
            referenced_targets.iter().flat_map(|target| {
                target
                    .detections
                    .iter()
                    .map(|detection| detection.license_expression.clone())
            }),
        ))
        .or_else(|| determine_license_expression(&matches_for_expression, None).ok())
    };
    internal_detection.license_expression_spdx = if is_placeholder_reference {
        referenced_license_expression_spdx
            .or_else(|| determine_spdx_expression(&matches_for_expression, None).ok())
    } else {
        combine_license_expressions(
            (!detection.license_expression_spdx.is_empty())
                .then(|| detection.license_expression_spdx.clone())
                .into_iter()
                .chain(referenced_targets.iter().flat_map(|target| {
                    target
                        .detections
                        .iter()
                        .filter(|detection| !detection.license_expression_spdx.is_empty())
                        .map(|detection| detection.license_expression_spdx.clone())
                })),
        )
        .or_else(|| determine_spdx_expression(&matches_for_expression, None).ok())
    };
    internal_detection.detection_log = vec![detection_log.to_string()];
    let identifier_matches = if is_placeholder_reference {
        let mut matches = placeholder_matches.clone();
        matches.extend(matches_for_expression.clone());
        matches
    } else {
        matches_for_expression.clone()
    };
    internal_detection.identifier =
        if is_placeholder_reference && inherits_license_from_package(detection) {
            referenced_targets
                .iter()
                .flat_map(|target| {
                    target
                        .detections
                        .iter()
                        .map(move |detection| (target, detection))
                })
                .next()
                .and_then(|(target, detection)| {
                    public_detection_to_internal(detection, &target.path).identifier
                })
        } else {
            internal_detection
                .license_expression
                .as_ref()
                .map(|license_expression| {
                    compute_detection_identifier(&crate::license_detection::LicenseDetection {
                        license_expression: Some(license_expression.clone()),
                        license_expression_spdx: internal_detection.license_expression_spdx.clone(),
                        matches: identifier_matches.clone(),
                        detection_log: internal_detection.detection_log.clone(),
                        identifier: None,
                        file_regions: internal_detection.file_regions.clone(),
                    })
                })
        };
    if !placeholder_matches.is_empty() {
        let mut combined_matches = placeholder_matches;
        combined_matches.extend(internal_detection.matches);
        internal_detection.matches = combined_matches;
    }
    let mut public_detection = internal_detection_to_public(internal_detection);
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

pub(crate) fn resolve_referenced_resource(
    referenced_filename: &str,
    current_path: &str,
    package_uids: &[String],
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

pub(crate) fn use_referenced_license_expression(
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
    let identifier = detection.identifier.clone().or_else(|| {
        (!matches.is_empty() && !detection.license_expression.is_empty()).then(|| {
            compute_detection_identifier(&crate::license_detection::LicenseDetection {
                license_expression: Some(detection.license_expression.clone()),
                license_expression_spdx: (!detection.license_expression_spdx.is_empty())
                    .then(|| detection.license_expression_spdx.clone()),
                matches: matches.clone(),
                detection_log: detection.detection_log.clone(),
                identifier: None,
                file_regions: file_regions.clone(),
            })
        })
    });
    crate::license_detection::LicenseDetection {
        license_expression: (!detection.license_expression.is_empty())
            .then(|| detection.license_expression.clone()),
        license_expression_spdx: (!detection.license_expression_spdx.is_empty())
            .then(|| detection.license_expression_spdx.clone()),
        matches: matches.clone(),
        detection_log: detection.detection_log.clone(),
        identifier,
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
