use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::{DatasourceId, FileInfo, Package, TopLevelDependency};

use super::AssemblerConfig;
use super::cargo_workspace_merge::{
    CargoWorkspaceDomain, CargoWorkspaceRootHint, apply_cargo_workspace_domain,
    collect_cargo_workspace_hints, plan_cargo_workspace_domains,
};
use super::hackage_merge;
use super::npm_workspace_merge::{
    NpmWorkspaceDomain, NpmWorkspaceRootHint, apply_npm_workspace_domain,
    collect_npm_workspace_hints, plan_npm_workspace_domains,
};
use super::{ASSEMBLERS, DirectoryMergeOutput, sibling_merge};

pub(super) struct GoWorkspaceRootHint {
    root_dir: PathBuf,
}

pub(super) struct GoWorkspaceDomain {
    root_dir: PathBuf,
    root_dir_file_indices: Vec<usize>,
}

pub(super) struct PixiRootHint {
    root_dir: PathBuf,
}

pub(super) struct PixiDomain {
    root_dir: PathBuf,
    root_dir_file_indices: Vec<usize>,
}

pub(super) struct HackageProjectHint {
    root_dir: PathBuf,
}

pub(super) struct HackageProjectDomain {
    root_dir: PathBuf,
    root_dir_file_indices: Vec<usize>,
}

pub(super) enum TopologyHint {
    CargoWorkspaceRoot(CargoWorkspaceRootHint),
    GoWorkspaceRoot(GoWorkspaceRootHint),
    HackageProject(HackageProjectHint),
    NpmWorkspaceRoot(NpmWorkspaceRootHint),
    PixiRoot(PixiRootHint),
}

pub(super) enum TopologyDomain {
    CargoWorkspace(CargoWorkspaceDomain),
    GoWorkspace(GoWorkspaceDomain),
    HackageProject(HackageProjectDomain),
    NpmWorkspace(NpmWorkspaceDomain),
    Pixi(PixiDomain),
}

pub(super) struct TopologyPlan {
    domains: Vec<TopologyDomain>,
    claimed_cargo_dirs: HashSet<PathBuf>,
    claimed_go_dirs: HashSet<PathBuf>,
    claimed_hackage_dirs: HashSet<PathBuf>,
    claimed_npm_dirs: HashSet<PathBuf>,
    claimed_pixi_dirs: HashSet<PathBuf>,
}

impl TopologyPlan {
    pub(super) fn build(files: &[FileInfo], dir_files: &HashMap<PathBuf, Vec<usize>>) -> Self {
        let mut hints = Vec::new();
        hints.extend(
            collect_cargo_workspace_hints(files)
                .into_iter()
                .map(TopologyHint::CargoWorkspaceRoot),
        );
        hints.extend(
            collect_go_workspace_hints(files)
                .into_iter()
                .map(TopologyHint::GoWorkspaceRoot),
        );
        hints.extend(
            collect_hackage_project_hints(files)
                .into_iter()
                .map(TopologyHint::HackageProject),
        );
        hints.extend(
            collect_npm_workspace_hints(files)
                .into_iter()
                .map(TopologyHint::NpmWorkspaceRoot),
        );
        hints.extend(
            collect_pixi_root_hints(files)
                .into_iter()
                .map(TopologyHint::PixiRoot),
        );

        let mut domains = Vec::new();
        let mut claimed_cargo_dirs = HashSet::new();
        let mut claimed_go_dirs = HashSet::new();
        let mut claimed_hackage_dirs = HashSet::new();
        let mut claimed_npm_dirs = HashSet::new();
        let mut claimed_pixi_dirs = HashSet::new();

        let cargo_workspace_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(hint) => Some(hint),
                TopologyHint::GoWorkspaceRoot(_) => None,
                TopologyHint::HackageProject(_) => None,
                TopologyHint::NpmWorkspaceRoot(_) => None,
                TopologyHint::PixiRoot(_) => None,
            })
            .collect();

        for domain in plan_cargo_workspace_domains(files, dir_files, &cargo_workspace_hints) {
            claimed_cargo_dirs.insert(domain.root_dir.clone());
            claimed_cargo_dirs.extend(domain.members.iter().map(|member| member.dir_path.clone()));
            domains.push(TopologyDomain::CargoWorkspace(domain));
        }

        let go_workspace_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(_) => None,
                TopologyHint::GoWorkspaceRoot(hint) => Some(hint),
                TopologyHint::HackageProject(_) => None,
                TopologyHint::NpmWorkspaceRoot(_) => None,
                TopologyHint::PixiRoot(_) => None,
            })
            .collect();

        for domain in plan_go_workspace_domains(dir_files, &go_workspace_hints) {
            claimed_go_dirs.insert(domain.root_dir.clone());
            domains.push(TopologyDomain::GoWorkspace(domain));
        }

        let hackage_project_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(_) => None,
                TopologyHint::GoWorkspaceRoot(_) => None,
                TopologyHint::HackageProject(hint) => Some(hint),
                TopologyHint::NpmWorkspaceRoot(_) => None,
                TopologyHint::PixiRoot(_) => None,
            })
            .collect();

        for domain in plan_hackage_project_domains(dir_files, &hackage_project_hints) {
            claimed_hackage_dirs.insert(domain.root_dir.clone());
            domains.push(TopologyDomain::HackageProject(domain));
        }

        let npm_workspace_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(_) => None,
                TopologyHint::GoWorkspaceRoot(_) => None,
                TopologyHint::HackageProject(_) => None,
                TopologyHint::NpmWorkspaceRoot(hint) => Some(hint),
                TopologyHint::PixiRoot(_) => None,
            })
            .collect();

        for domain in plan_npm_workspace_domains(files, dir_files, &npm_workspace_hints) {
            claimed_npm_dirs.insert(domain.root_dir.clone());
            claimed_npm_dirs.extend(domain.members.iter().map(|member| member.dir_path.clone()));
            domains.push(TopologyDomain::NpmWorkspace(domain));
        }

        let pixi_root_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(_) => None,
                TopologyHint::GoWorkspaceRoot(_) => None,
                TopologyHint::HackageProject(_) => None,
                TopologyHint::NpmWorkspaceRoot(_) => None,
                TopologyHint::PixiRoot(hint) => Some(hint),
            })
            .collect();

        for domain in plan_pixi_domains(dir_files, &pixi_root_hints) {
            claimed_pixi_dirs.insert(domain.root_dir.clone());
            domains.push(TopologyDomain::Pixi(domain));
        }

        Self {
            domains,
            claimed_cargo_dirs,
            claimed_go_dirs,
            claimed_hackage_dirs,
            claimed_npm_dirs,
            claimed_pixi_dirs,
        }
    }

    pub(super) fn claims_directory_assembly(
        &self,
        config: &AssemblerConfig,
        file_indices: &[usize],
        files: &[FileInfo],
    ) -> bool {
        let Some(&first_idx) = file_indices.first() else {
            return false;
        };
        let Some(parent_dir) = Path::new(&files[first_idx].path).parent() else {
            return false;
        };

        if config.datasource_ids.contains(&DatasourceId::CargoToml) {
            return self.claimed_cargo_dirs.contains(parent_dir);
        }

        if config.datasource_ids.contains(&DatasourceId::GoWork) {
            return self.claimed_go_dirs.contains(parent_dir);
        }

        if config.datasource_ids.contains(&DatasourceId::PixiToml) {
            return self.claimed_pixi_dirs.contains(parent_dir);
        }

        if config.datasource_ids.contains(&DatasourceId::HackageCabal) {
            return self.claimed_hackage_dirs.contains(parent_dir);
        }

        if !config
            .datasource_ids
            .contains(&DatasourceId::NpmPackageJson)
        {
            return false;
        }

        self.claimed_npm_dirs.contains(parent_dir)
    }

    pub(super) fn apply_directory_scoped_domains(
        &self,
        files: &mut [FileInfo],
        packages: &mut Vec<Package>,
        dependencies: &mut Vec<TopLevelDependency>,
    ) {
        for domain in &self.domains {
            match domain {
                TopologyDomain::GoWorkspace(domain) => {
                    let Some(result) = sibling_merge::assemble_siblings(
                        go_assembler_config(),
                        files,
                        &domain.root_dir_file_indices,
                    )
                    .into_iter()
                    .next() else {
                        continue;
                    };

                    apply_directory_merge_result(files, packages, dependencies, result);
                }
                TopologyDomain::HackageProject(domain) => {
                    let results = hackage_merge::assemble_hackage_packages(
                        files,
                        &domain.root_dir_file_indices,
                    );
                    for result in results {
                        apply_directory_merge_result(files, packages, dependencies, result);
                    }
                }
                TopologyDomain::Pixi(domain) => {
                    let Some(result) = sibling_merge::assemble_siblings(
                        pixi_assembler_config(),
                        files,
                        &domain.root_dir_file_indices,
                    )
                    .into_iter()
                    .next() else {
                        continue;
                    };

                    apply_directory_merge_result(files, packages, dependencies, result);
                }
                TopologyDomain::CargoWorkspace(_) | TopologyDomain::NpmWorkspace(_) => {}
            }
        }
    }

    pub(super) fn apply_cargo_workspace_domains(
        &self,
        files: &mut [FileInfo],
        packages: &mut Vec<Package>,
        dependencies: &mut Vec<TopLevelDependency>,
    ) {
        for domain in &self.domains {
            match domain {
                TopologyDomain::CargoWorkspace(domain) => {
                    apply_cargo_workspace_domain(domain, files, packages, dependencies);
                }
                TopologyDomain::GoWorkspace(_)
                | TopologyDomain::HackageProject(_)
                | TopologyDomain::NpmWorkspace(_)
                | TopologyDomain::Pixi(_) => {}
            }
        }
    }

    pub(super) fn apply_npm_workspace_domains(
        &self,
        files: &mut [FileInfo],
        packages: &mut Vec<Package>,
        dependencies: &mut Vec<TopLevelDependency>,
    ) {
        for domain in &self.domains {
            match domain {
                TopologyDomain::CargoWorkspace(_)
                | TopologyDomain::GoWorkspace(_)
                | TopologyDomain::HackageProject(_)
                | TopologyDomain::Pixi(_) => {}
                TopologyDomain::NpmWorkspace(domain) => {
                    apply_npm_workspace_domain(domain, files, packages, dependencies);
                }
            }
        }
    }
}

fn collect_go_workspace_hints(files: &[FileInfo]) -> Vec<GoWorkspaceRootHint> {
    let mut seen = HashSet::new();
    let mut hints = Vec::new();

    for file in files {
        let path = Path::new(&file.path);
        if path.file_name().and_then(|name| name.to_str()) != Some("go.work") {
            continue;
        }

        let has_go_work_data = file
            .package_data
            .iter()
            .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::GoWork));
        if !has_go_work_data {
            continue;
        }

        let Some(parent) = path.parent() else {
            continue;
        };
        let root_dir = parent.to_path_buf();
        if seen.insert(root_dir.clone()) {
            hints.push(GoWorkspaceRootHint { root_dir });
        }
    }

    hints.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    hints
}

fn collect_pixi_root_hints(files: &[FileInfo]) -> Vec<PixiRootHint> {
    let mut seen = HashSet::new();
    let mut hints = Vec::new();

    for file in files {
        let path = Path::new(&file.path);
        if path.file_name().and_then(|name| name.to_str()) != Some("pixi.toml") {
            continue;
        }

        let has_pixi_manifest = file
            .package_data
            .iter()
            .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::PixiToml));
        if !has_pixi_manifest {
            continue;
        }

        let Some(parent) = path.parent() else {
            continue;
        };
        let root_dir = parent.to_path_buf();
        if seen.insert(root_dir.clone()) {
            hints.push(PixiRootHint { root_dir });
        }
    }

    hints.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    hints
}

fn collect_hackage_project_hints(files: &[FileInfo]) -> Vec<HackageProjectHint> {
    let mut seen = HashSet::new();
    let mut hints = Vec::new();

    for file in files {
        let path = Path::new(&file.path);
        let file_name = path.file_name().and_then(|name| name.to_str());
        if !matches!(file_name, Some("cabal.project" | "stack.yaml")) {
            continue;
        }

        let has_project_surface = file.package_data.iter().any(|pkg_data| {
            matches!(
                pkg_data.datasource_id,
                Some(DatasourceId::HackageCabalProject | DatasourceId::HackageStackYaml)
            )
        });
        if !has_project_surface {
            continue;
        }

        let Some(parent) = path.parent() else {
            continue;
        };
        let root_dir = parent.to_path_buf();
        if seen.insert(root_dir.clone()) {
            hints.push(HackageProjectHint { root_dir });
        }
    }

    hints.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    hints
}

fn plan_go_workspace_domains(
    dir_files: &HashMap<PathBuf, Vec<usize>>,
    workspace_hints: &[&GoWorkspaceRootHint],
) -> Vec<GoWorkspaceDomain> {
    let mut domains = Vec::new();

    for hint in workspace_hints {
        let root_dir_file_indices = dir_files.get(&hint.root_dir).cloned().unwrap_or_default();
        if root_dir_file_indices.is_empty() {
            continue;
        }

        domains.push(GoWorkspaceDomain {
            root_dir: hint.root_dir.clone(),
            root_dir_file_indices,
        });
    }

    domains.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    domains
}

fn plan_pixi_domains(
    dir_files: &HashMap<PathBuf, Vec<usize>>,
    workspace_hints: &[&PixiRootHint],
) -> Vec<PixiDomain> {
    let mut domains = Vec::new();

    for hint in workspace_hints {
        let root_dir_file_indices = dir_files.get(&hint.root_dir).cloned().unwrap_or_default();
        if root_dir_file_indices.is_empty() {
            continue;
        }

        domains.push(PixiDomain {
            root_dir: hint.root_dir.clone(),
            root_dir_file_indices,
        });
    }

    domains.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    domains
}

fn plan_hackage_project_domains(
    dir_files: &HashMap<PathBuf, Vec<usize>>,
    workspace_hints: &[&HackageProjectHint],
) -> Vec<HackageProjectDomain> {
    let mut domains = Vec::new();

    for hint in workspace_hints {
        let root_dir_file_indices = dir_files.get(&hint.root_dir).cloned().unwrap_or_default();
        if root_dir_file_indices.is_empty() {
            continue;
        }

        domains.push(HackageProjectDomain {
            root_dir: hint.root_dir.clone(),
            root_dir_file_indices,
        });
    }

    domains.sort_by(|left, right| left.root_dir.cmp(&right.root_dir));
    domains
}

fn apply_directory_merge_result(
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
    result: DirectoryMergeOutput,
) {
    let (package, deps, affected_indices) = result;

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

fn go_assembler_config() -> &'static AssemblerConfig {
    ASSEMBLERS
        .iter()
        .find(|config| config.datasource_ids.contains(&DatasourceId::GoWork))
        .expect("Go assembler config must exist")
}

fn pixi_assembler_config() -> &'static AssemblerConfig {
    ASSEMBLERS
        .iter()
        .find(|config| config.datasource_ids.contains(&DatasourceId::PixiToml))
        .expect("Pixi assembler config must exist")
}
