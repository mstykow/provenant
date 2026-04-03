use std::collections::HashMap;
use std::path::Path;

use crate::models::{FileInfo, Package, PackageType};
use crate::utils::path::parent_dir_for_lookup;

pub fn assign_npm_package_resources(files: &mut [FileInfo], packages: &[Package]) {
    let package_roots: HashMap<String, String> = packages
        .iter()
        .filter(|package| package.package_type == Some(PackageType::Npm))
        .filter_map(|package| {
            package
                .datafile_paths
                .first()
                .and_then(|path| Path::new(path).parent())
                .map(|root| {
                    (
                        root.to_string_lossy().into_owned(),
                        package.package_uid.clone(),
                    )
                })
        })
        .collect();

    for file in files.iter_mut() {
        if let Some(package_uid) = find_nearest_package_owner(&file.path, &package_roots) {
            file.for_packages.clear();
            file.for_packages.push(package_uid);
        }
    }
}

fn find_nearest_package_owner(
    path: &str,
    package_roots: &HashMap<String, String>,
) -> Option<String> {
    let mut current = Some(path);

    while let Some(candidate) = current {
        if let Some(package_uid) = package_roots.get(candidate)
            && !is_first_level_node_modules_str(path, candidate)
        {
            return Some(package_uid.clone());
        }

        current = parent_dir_for_lookup(candidate);
    }

    None
}

fn is_first_level_node_modules_str(path: &str, root: &str) -> bool {
    strip_root_prefix(path, root)
        .and_then(|relative| relative.split('/').next())
        .is_some_and(|component| component == "node_modules")
}

fn strip_root_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    if path == root {
        return Some("");
    }

    path.strip_prefix(root)
        .and_then(|suffix| suffix.strip_prefix('/'))
}
