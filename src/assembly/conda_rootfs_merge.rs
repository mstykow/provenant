use std::collections::HashMap;

use crate::models::{DatasourceId, FileInfo, Package, PackageData, PackageUid, TopLevelDependency};

pub fn merge_conda_rootfs_metadata(
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut [TopLevelDependency],
) {
    let conda_json_data: Vec<(String, PackageData, String)> = files
        .iter()
        .flat_map(|file| {
            file.package_data.iter().filter_map(|pkg_data| {
                if pkg_data.datasource_id != Some(DatasourceId::CondaMetaJson) {
                    return None;
                }

                Some((
                    file.path.clone(),
                    pkg_data.clone(),
                    pkg_data
                        .extra_data
                        .as_ref()?
                        .get("extracted_package_dir")?
                        .as_str()?
                        .replace('\\', "/")
                        .split("/pkgs/")
                        .nth(1)?
                        .to_string(),
                ))
            })
        })
        .collect();

    let json_package_uids: HashMap<Option<String>, PackageUid> = packages
        .iter()
        .filter(|package| {
            package
                .datasource_ids
                .contains(&DatasourceId::CondaMetaJson)
        })
        .map(|package| (package.purl.clone(), package.package_uid.clone()))
        .collect();

    let mut removal_indices = Vec::new();

    for (json_path, pkg_data, package_dir_name) in conda_json_data {
        let Some(target_idx) = packages.iter().enumerate().find_map(|(idx, package)| {
            if !package
                .datasource_ids
                .contains(&DatasourceId::CondaMetaYaml)
            {
                return None;
            }

            let matches_recipe = package.datafile_paths.iter().any(|path| {
                path.contains(&format!("pkgs/{package_dir_name}/info/recipe/meta.yaml"))
                    || path.contains(&format!("pkgs/{package_dir_name}/info/recipe/meta.yml"))
                    || path.contains(&format!(
                        "pkgs/{package_dir_name}/info/recipe.tar-extract/recipe/meta.yaml"
                    ))
                    || path.contains(&format!(
                        "pkgs/{package_dir_name}/info/recipe.tar-extract/recipe/meta.yml"
                    ))
            });

            (matches_recipe && package.purl == pkg_data.purl).then_some(idx)
        }) else {
            continue;
        };

        let old_uid = json_package_uids.get(&pkg_data.purl).cloned();
        packages[target_idx].update(&pkg_data, json_path);
        let new_uid = packages[target_idx].package_uid.clone();

        if let Some(old_uid) = old_uid {
            for file in files.iter_mut() {
                for package_uid in &mut file.for_packages {
                    if *package_uid == old_uid {
                        *package_uid = new_uid.clone();
                    }
                }
            }

            for dep in dependencies.iter_mut() {
                if dep.for_package_uid.as_ref() == Some(&old_uid) {
                    dep.for_package_uid = Some(new_uid.clone());
                }
            }

            if let Some(idx) = packages
                .iter()
                .position(|package| package.package_uid == old_uid)
            {
                removal_indices.push(idx);
            }
        }
    }

    removal_indices.sort_unstable();
    removal_indices.dedup();
    for idx in removal_indices.into_iter().rev() {
        packages.remove(idx);
    }
}
