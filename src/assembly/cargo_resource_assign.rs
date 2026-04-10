use std::path::{Path, PathBuf};

use crate::models::{FileInfo, Package, PackageType, PackageUid};

pub fn assign_cargo_package_resources(files: &mut [FileInfo], packages: &[Package]) {
    let cargo_roots: Vec<(PathBuf, PackageUid)> = packages
        .iter()
        .filter(|package| package.package_type == Some(PackageType::Cargo))
        .filter_map(|package| {
            let root = package
                .datafile_paths
                .iter()
                .find(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.eq_ignore_ascii_case("cargo.toml"))
                })
                .and_then(|path| Path::new(path).parent())?
                .to_path_buf();

            Some((root, package.package_uid.clone()))
        })
        .collect();

    if cargo_roots.is_empty() {
        return;
    }

    for file in files.iter_mut() {
        let path = Path::new(&file.path);

        for (root, package_uid) in &cargo_roots {
            if !path.starts_with(root) || is_target_path(path, root) {
                continue;
            }

            if !file.for_packages.contains(package_uid) {
                file.for_packages.push(package_uid.clone());
            }
        }
    }
}

fn is_target_path(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .is_some_and(|component| component.as_os_str() == "target")
}
