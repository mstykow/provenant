// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::models::{DatasourceId, FileInfo, Package, PackageType, PackageUid, TopLevelDependency};

const PYTHON_PROJECT_ROOT_FILENAMES: &[&str] = &[
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "Pipfile",
    "Pipfile.lock",
    "poetry.lock",
    "pylock.toml",
    "uv.lock",
];

struct PythonProjectRoot {
    root: PathBuf,
    package_index: usize,
    package_uid: PackageUid,
}

pub fn assign_python_requirements_to_projects(
    files: &mut [FileInfo],
    packages: &mut [Package],
    dependencies: &mut Vec<TopLevelDependency>,
) {
    let project_roots = collect_python_project_roots(packages);
    if project_roots.is_empty() {
        return;
    }

    for file in files.iter_mut() {
        if !is_requirements_subdir_file(file) || !file.for_packages.is_empty() {
            continue;
        }

        let path = Path::new(&file.path);
        let Some(project_root) = find_nearest_project_root(path, &project_roots) else {
            continue;
        };

        if !file.for_packages.contains(&project_root.package_uid) {
            file.for_packages.push(project_root.package_uid.clone());
        }

        let package = &mut packages[project_root.package_index];
        if !package.datafile_paths.contains(&file.path) {
            package.datafile_paths.push(file.path.clone());
        }
        if !package
            .datasource_ids
            .contains(&DatasourceId::PipRequirements)
        {
            package.datasource_ids.push(DatasourceId::PipRequirements);
        }

        for dependency in dependencies.iter_mut() {
            if dependency.datasource_id == DatasourceId::PipRequirements
                && dependency.datafile_path == file.path
                && dependency.for_package_uid.is_none()
            {
                dependency.for_package_uid = Some(project_root.package_uid.clone());
            }
        }

        if !dependencies.iter().any(|dependency| {
            dependency.datasource_id == DatasourceId::PipRequirements
                && dependency.datafile_path == file.path
        }) {
            for pkg_data in &file.package_data {
                if pkg_data.datasource_id != Some(DatasourceId::PipRequirements) {
                    continue;
                }

                dependencies.extend(
                    pkg_data
                        .dependencies
                        .iter()
                        .filter(|dep| dep.purl.is_some())
                        .map(|dep| {
                            TopLevelDependency::from_dependency(
                                dep,
                                file.path.clone(),
                                DatasourceId::PipRequirements,
                                Some(project_root.package_uid.clone()),
                            )
                        }),
                );
            }
        }
    }
}

fn collect_python_project_roots(packages: &[Package]) -> Vec<PythonProjectRoot> {
    packages
        .iter()
        .enumerate()
        .filter(|(_, package)| package.package_type == Some(PackageType::Pypi))
        .filter_map(|(package_index, package)| {
            if package.package_uid.is_empty() {
                return None;
            }

            let root = package
                .datafile_paths
                .iter()
                .find(|path| is_python_project_root_path(path))
                .and_then(|path| Path::new(path).parent())?
                .to_path_buf();

            Some(PythonProjectRoot {
                root,
                package_index,
                package_uid: package.package_uid.clone(),
            })
        })
        .collect()
}

fn is_python_project_root_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| PYTHON_PROJECT_ROOT_FILENAMES.contains(&name))
}

fn is_requirements_subdir_file(file: &FileInfo) -> bool {
    if !file
        .package_data
        .iter()
        .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::PipRequirements))
    {
        return false;
    }

    let path = Path::new(&file.path);
    path.parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .filter_map(|ancestor| ancestor.file_name())
        .filter_map(|name| name.to_str())
        .any(|name| name == "requirements")
}

fn find_nearest_project_root<'a>(
    path: &Path,
    project_roots: &'a [PythonProjectRoot],
) -> Option<&'a PythonProjectRoot> {
    let mut current_dir = path.parent().and_then(|parent| parent.parent());

    while let Some(dir) = current_dir {
        let mut matches = project_roots.iter().filter(|root| root.root == dir);
        let first = matches.next();
        let second = matches.next();

        match (first, second) {
            (Some(root), None) => return Some(root),
            (Some(_), Some(_)) => return None,
            (None, _) => {
                current_dir = dir.parent();
            }
        }
    }

    None
}
