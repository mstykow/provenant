mod assemblers;
#[cfg(test)]
mod assembly_golden_test;
#[cfg(test)]
mod assembly_test;
mod bazel_merge;
mod bazel_prune;
mod cargo_resource_assign;
mod cargo_workspace_merge;
mod composer_resource_assign;
mod conda_rootfs_merge;
pub mod file_ref_resolve;
mod hackage_merge;
mod nested_merge;
mod npm_resource_assign;
mod npm_workspace_merge;
mod nuget_cpm_resolve;
mod python_requirements_assign;
mod ruby_resource_assign;
mod sibling_merge;
mod swift_merge;
mod topology;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::models::{DatasourceId, FileInfo, Package, TopLevelDependency};

pub use assemblers::ASSEMBLERS;

type DirectoryMergeOutput = (Option<Package>, Vec<TopLevelDependency>, Vec<usize>);

/// Pre-computed lookup: DatasourceId → config key (first DatasourceId in config).
/// Built once on first use, avoiding HashMap allocation on every `assemble()` call.
static ASSEMBLER_LOOKUP: LazyLock<HashMap<DatasourceId, DatasourceId>> = LazyLock::new(|| {
    let mut lookup = HashMap::new();
    for config in ASSEMBLERS {
        let key = *config
            .datasource_ids
            .first()
            .expect("assembler must have at least one datasource_id");
        for &dsid in config.datasource_ids {
            lookup.insert(dsid, key);
        }
    }
    lookup
});

static ASSEMBLER_CONFIG_LOOKUP: LazyLock<HashMap<DatasourceId, &'static AssemblerConfig>> =
    LazyLock::new(|| {
        let mut lookup = HashMap::new();
        for config in ASSEMBLERS {
            let key = *config
                .datasource_ids
                .first()
                .expect("assembler must have at least one datasource_id");
            lookup.insert(key, config);
        }
        lookup
    });

/// Result of the assembly phase: top-level packages and dependencies,
/// plus updated file-to-package associations.
pub struct AssemblyResult {
    pub packages: Vec<Package>,
    pub dependencies: Vec<TopLevelDependency>,
}

/// How an assembler groups PackageData into Packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyMode {
    /// Merge related files in the same directory (or nested) into one Package.
    SiblingMerge,
    /// Each PackageData becomes its own independent Package (e.g., database files
    /// containing many installed packages like Alpine DB, RPM DB, Debian status).
    OnePerPackageData,
}

pub struct AssemblerConfig {
    pub datasource_ids: &'static [DatasourceId],
    pub sibling_file_patterns: &'static [&'static str],
    pub mode: AssemblyMode,
}

/// Run the assembly phase over all scanned files.
///
/// Groups files by parent directory, finds related manifests/lockfiles,
/// merges them into top-level `Package` objects, and hoists dependencies.
/// Updates each `FileInfo.for_packages` with the UIDs of packages it belongs to.
pub fn assemble(files: &mut [FileInfo]) -> AssemblyResult {
    let assembler_lookup = &*ASSEMBLER_LOOKUP;
    let assembler_config_lookup = &*ASSEMBLER_CONFIG_LOOKUP;
    let mut packages = Vec::new();
    let mut dependencies = Vec::new();

    let dir_files = group_files_by_directory(files);
    let topology_plan = topology::TopologyPlan::build(files, &dir_files);

    for file_indices in dir_files.values() {
        let mut groups: HashSet<DatasourceId> = HashSet::new();

        for &idx in file_indices {
            for pkg_data in &files[idx].package_data {
                if let Some(dsid) = pkg_data.datasource_id
                    && let Some(&config_key) = assembler_lookup.get(&dsid)
                {
                    groups.insert(config_key);
                }
            }
        }

        for &config_key in &groups {
            let config = assembler_config_lookup
                .get(&config_key)
                .copied()
                .expect("assembler config must exist");

            if topology_plan.claims_directory_assembly(config, file_indices, files) {
                continue;
            }

            if let Some(special_merger) = assemblers::special_directory_merger_for(config_key) {
                let results = special_merger.run(config, files, file_indices);
                apply_directory_merge_results(files, &mut packages, &mut dependencies, results);
                continue;
            }

            match config.mode {
                AssemblyMode::SiblingMerge => {
                    let results = sibling_merge::assemble_siblings(config, files, file_indices)
                        .into_iter()
                        .collect();
                    apply_directory_merge_results(files, &mut packages, &mut dependencies, results);
                }
                AssemblyMode::OnePerPackageData => {
                    let results = assemble_one_per_package_data(config, files, file_indices)
                        .into_iter()
                        .map(|(pkg, deps, affected_idx)| (Some(pkg), deps, vec![affected_idx]))
                        .collect();
                    apply_directory_merge_results(files, &mut packages, &mut dependencies, results);
                }
            }
        }
    }

    topology_plan.apply_directory_scoped_domains(files, &mut packages, &mut dependencies);

    for config in ASSEMBLERS {
        if config.mode != AssemblyMode::SiblingMerge {
            continue;
        }
        if let Some((pkg, deps, affected_indices)) =
            nested_merge::assemble_nested_patterns(files, config)
        {
            let package_uid = pkg.package_uid.clone();
            let purl = pkg.purl.clone();
            let removed_package_uids: Vec<String> = packages
                .iter()
                .filter(|p| p.purl == purl)
                .map(|p| p.package_uid.clone())
                .collect();

            packages.retain(|p| p.purl != purl);
            dependencies.retain(|d| {
                d.for_package_uid.as_ref() != Some(&package_uid)
                    && !removed_package_uids
                        .iter()
                        .any(|old_uid| d.for_package_uid.as_ref() == Some(old_uid))
            });

            for idx in &affected_indices {
                files[*idx].for_packages.clear();
                files[*idx].for_packages.push(package_uid.clone());
            }

            packages.push(pkg);
            dependencies.extend(deps);
        }
    }

    assemblers::run_post_assembly_passes(files, &mut packages, &mut dependencies, &topology_plan);
    hoist_unassembled_file_dependencies(files, &mut dependencies);

    for package in &mut packages {
        package.datafile_paths.sort();
        package.datafile_paths.dedup();
        package.datasource_ids.sort_by_key(|left| left.to_string());
        package.datasource_ids.dedup();
    }

    for file in files.iter_mut() {
        file.for_packages
            .sort_by(|left, right| stable_uid_key(left).cmp(stable_uid_key(right)));
        file.for_packages.dedup();
    }

    packages
        .sort_by(|left, right| stable_package_sort_key(left).cmp(&stable_package_sort_key(right)));
    dependencies.sort_by(|left, right| {
        left.purl
            .as_deref()
            .cmp(&right.purl.as_deref())
            .then_with(|| {
                left.extracted_requirement
                    .as_deref()
                    .cmp(&right.extracted_requirement.as_deref())
            })
            .then_with(|| left.scope.as_deref().cmp(&right.scope.as_deref()))
            .then_with(|| left.datafile_path.cmp(&right.datafile_path))
            .then_with(|| {
                left.datasource_id
                    .to_string()
                    .cmp(&right.datasource_id.to_string())
            })
            .then_with(|| {
                left.for_package_uid
                    .as_deref()
                    .map(stable_uid_key)
                    .cmp(&right.for_package_uid.as_deref().map(stable_uid_key))
            })
    });

    AssemblyResult {
        packages,
        dependencies,
    }
}

fn apply_directory_merge_results(
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
    results: Vec<DirectoryMergeOutput>,
) {
    for (package, deps, affected_indices) in results {
        if let Some(package) = package {
            let package_uid = package.package_uid.clone();
            for idx in &affected_indices {
                if !files[*idx].for_packages.contains(&package_uid) {
                    files[*idx].for_packages.push(package_uid.clone());
                }
            }
            packages.push(package);
        }
        dependencies.extend(deps);
    }
}

fn hoist_unassembled_file_dependencies(
    files: &[FileInfo],
    dependencies: &mut Vec<TopLevelDependency>,
) {
    for file in files {
        if !file.for_packages.is_empty() {
            continue;
        }

        for pkg_data in &file.package_data {
            let Some(datasource_id) = pkg_data.datasource_id else {
                continue;
            };

            if !should_hoist_unassembled_dependencies(datasource_id) {
                continue;
            }

            dependencies.extend(pkg_data.dependencies.iter().map(|dep| {
                TopLevelDependency::from_dependency(dep, file.path.clone(), datasource_id, None)
            }));
        }
    }
}

fn should_hoist_unassembled_dependencies(datasource_id: DatasourceId) -> bool {
    if !assemblers::UNASSEMBLED_DATASOURCE_IDS.contains(&datasource_id) {
        return false;
    }

    !matches!(
        datasource_id,
        DatasourceId::NugetDirectoryBuildProps | DatasourceId::NugetDirectoryPackagesProps
    )
}

fn stable_package_sort_key(package: &Package) -> (Option<&str>, Option<&str>, Option<&str>, &str) {
    (
        package.purl.as_deref(),
        package.name.as_deref(),
        package.version.as_deref(),
        package
            .datafile_paths
            .first()
            .map(String::as_str)
            .unwrap_or(""),
    )
}

fn stable_uid_key(uid: &str) -> &str {
    uid.split_once("?uuid=")
        .map(|(prefix, _)| prefix)
        .or_else(|| uid.split_once("&uuid=").map(|(prefix, _)| prefix))
        .unwrap_or(uid)
}

fn assemble_one_per_package_data(
    config: &AssemblerConfig,
    files: &[FileInfo],
    file_indices: &[usize],
) -> Vec<(Package, Vec<TopLevelDependency>, usize)> {
    let mut results = Vec::new();

    for &idx in file_indices {
        let file = &files[idx];
        for pkg_data in &file.package_data {
            let dsid_matches = pkg_data
                .datasource_id
                .is_some_and(|dsid| config.datasource_ids.contains(&dsid));

            if !dsid_matches || pkg_data.purl.is_none() {
                continue;
            }

            let datafile_path = file.path.clone();
            let datasource_id = pkg_data.datasource_id.expect("datasource_id must be Some");
            let pkg = Package::from_package_data(pkg_data, datafile_path.clone());
            let for_package_uid = Some(pkg.package_uid.clone());

            let deps: Vec<TopLevelDependency> = pkg_data
                .dependencies
                .iter()
                .filter(|dep| dep.purl.is_some())
                .map(|dep| {
                    TopLevelDependency::from_dependency(
                        dep,
                        datafile_path.clone(),
                        datasource_id,
                        for_package_uid.clone(),
                    )
                })
                .collect();

            results.push((pkg, deps, idx));
        }
    }

    results
}

/// Group file indices by their parent directory path.
fn group_files_by_directory(files: &[FileInfo]) -> HashMap<PathBuf, Vec<usize>> {
    let mut groups: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (idx, file) in files.iter().enumerate() {
        if let Some(parent) = std::path::Path::new(&file.path).parent() {
            groups.entry(parent.to_path_buf()).or_default().push(idx);
        }
    }
    groups
}
