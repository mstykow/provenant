use std::path::Path;
use std::sync::Arc;

use crate::assembly;
use crate::cache::{DEFAULT_CACHE_DIR_NAME, build_collection_exclude_patterns};
use crate::models::{DatasourceId, FileInfo, TopLevelDependency};
use crate::progress::{ProgressMode, ScanProgress};
use crate::scanner::{LicenseScanOptions, TextDetectionOptions, collect_paths, process_collected};

pub(crate) fn scan_and_assemble(path: &Path) -> (Vec<FileInfo>, assembly::AssemblyResult) {
    let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
    let collected = collect_paths(
        path,
        0,
        &build_collection_exclude_patterns(path, &path.join(DEFAULT_CACHE_DIR_NAME)),
    );
    let result = process_collected(
        &collected,
        progress,
        None,
        LicenseScanOptions::default(),
        &TextDetectionOptions {
            collect_info: true,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            ..TextDetectionOptions::default()
        },
    );

    let mut files = result.files;
    let assembly_result = assembly::assemble(&mut files);
    (files, assembly_result)
}

pub(crate) fn scan_and_assemble_with_stripped_root(
    path: &Path,
) -> (Vec<FileInfo>, assembly::AssemblyResult) {
    let (mut files, mut assembly_result) = scan_and_assemble(path);
    strip_root_paths(&mut files, path);
    strip_assembly_root_paths(&mut assembly_result, path);
    (files, assembly_result)
}

fn strip_assembly_root_paths(assembly_result: &mut assembly::AssemblyResult, root: &Path) {
    for package in &mut assembly_result.packages {
        for datafile_path in &mut package.datafile_paths {
            let current_path = Path::new(datafile_path);
            if let Some(stripped) = strip_root_prefix(current_path, root) {
                *datafile_path = stripped.to_string_lossy().to_string();
            }
        }
    }

    for dependency in &mut assembly_result.dependencies {
        let current_path = Path::new(&dependency.datafile_path);
        if let Some(stripped) = strip_root_prefix(current_path, root) {
            dependency.datafile_path = stripped.to_string_lossy().to_string();
        }
    }
}

pub(crate) fn strip_root_paths(files: &mut [FileInfo], scan_root: &Path) {
    for entry in files {
        let current_path = Path::new(&entry.path);

        if current_path == scan_root
            || current_path.canonicalize().ok().as_deref()
                == scan_root.canonicalize().ok().as_deref()
        {
            entry.path.clear();
            continue;
        }

        if let Some(stripped) = strip_root_prefix(current_path, scan_root) {
            entry.path = stripped.to_string_lossy().to_string();
        }
    }
}

pub(crate) fn assert_dependency_present(
    dependencies: &[TopLevelDependency],
    purl: &str,
    datafile_suffix: &str,
) {
    assert!(
        dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some(purl) && dep.datafile_path.ends_with(datafile_suffix)
        }),
        "expected dependency {purl} from {datafile_suffix}, found: {:?}",
        dependencies
            .iter()
            .map(|dep| (dep.purl.clone(), dep.datafile_path.clone()))
            .collect::<Vec<_>>()
    );
}

pub(crate) fn assert_file_links_to_package(
    files: &[FileInfo],
    suffix: &str,
    package_uid: &str,
    datasource_id: DatasourceId,
) {
    let file = files
        .iter()
        .find(|file| file.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("{suffix} should be scanned"));

    assert!(file.for_packages.iter().any(|uid| uid == package_uid));
    assert!(
        file.package_data
            .iter()
            .any(|pkg_data| { pkg_data.datasource_id == Some(datasource_id) })
    );
}

fn strip_root_prefix(path: &Path, root: &Path) -> Option<std::path::PathBuf> {
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
