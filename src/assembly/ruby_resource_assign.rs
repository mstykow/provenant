use std::path::{Path, PathBuf};

use crate::cache::DEFAULT_CACHE_DIR_NAME;
use crate::models::{FileInfo, Package, PackageType, PackageUid};

pub fn assign_ruby_package_resources(files: &mut [FileInfo], packages: &[Package]) {
    let ruby_roots: Vec<(PathBuf, PackageUid)> = packages
        .iter()
        .filter(|package| package.package_type == Some(PackageType::Gem))
        .filter_map(|package| {
            ruby_package_root(package).map(|root| (root, package.package_uid.clone()))
        })
        .collect();

    if ruby_roots.is_empty() {
        return;
    }

    for file in files.iter_mut() {
        let path = Path::new(&file.path);

        for (root, package_uid) in &ruby_roots {
            if !path.starts_with(root) || is_internal_cache_path(path, root) {
                continue;
            }

            if ruby_roots.iter().any(|(other_root, _)| {
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

fn ruby_package_root(package: &Package) -> Option<PathBuf> {
    if package
        .datasource_ids
        .contains(&crate::models::DatasourceId::GemArchive)
    {
        return None;
    }

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

fn is_internal_cache_path(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .is_some_and(|component| component.as_os_str() == DEFAULT_CACHE_DIR_NAME)
}
