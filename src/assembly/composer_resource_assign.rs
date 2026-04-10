use std::path::{Path, PathBuf};

use crate::cache::DEFAULT_CACHE_DIR_NAME;
use crate::models::{FileInfo, Package, PackageType, PackageUid};

pub fn assign_composer_package_resources(files: &mut [FileInfo], packages: &[Package]) {
    let composer_roots: Vec<(PathBuf, PackageUid)> = packages
        .iter()
        .filter(|package| package.package_type == Some(PackageType::Composer))
        .filter_map(|package| {
            let root = package
                .datafile_paths
                .iter()
                .find(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(is_composer_manifest_filename)
                })
                .and_then(|path| Path::new(path).parent())?
                .to_path_buf();

            Some((root, package.package_uid.clone()))
        })
        .collect();

    if composer_roots.is_empty() {
        return;
    }

    for file in files.iter_mut() {
        let path = Path::new(&file.path);

        for (root, package_uid) in &composer_roots {
            if !path.starts_with(root)
                || is_vendor_path(path, root)
                || is_internal_cache_path(path, root)
            {
                continue;
            }

            if composer_roots.iter().any(|(other_root, _)| {
                other_root != root && other_root.starts_with(root) && path.starts_with(other_root)
            }) {
                continue;
            }

            if !file.for_packages.contains(package_uid) {
                file.for_packages.push(package_uid.clone());
            }
        }
    }
}

fn is_vendor_path(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .is_some_and(|component| component.as_os_str() == "vendor")
}

fn is_composer_manifest_filename(name: &str) -> bool {
    name == "composer.json"
        || name.ends_with(".composer.json")
        || (name.starts_with("composer.") && name.ends_with(".json"))
}

fn is_internal_cache_path(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .is_some_and(|component| component.as_os_str() == DEFAULT_CACHE_DIR_NAME)
}
