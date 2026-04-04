use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::{DatasourceId, FileInfo, Package, TopLevelDependency};

use super::AssemblerConfig;
use super::cargo_workspace_merge::{
    CargoWorkspaceDomain, CargoWorkspaceRootHint, apply_cargo_workspace_domain,
    collect_cargo_workspace_hints, plan_cargo_workspace_domains,
};
use super::npm_workspace_merge::{
    NpmWorkspaceDomain, NpmWorkspaceRootHint, apply_npm_workspace_domain,
    collect_npm_workspace_hints, plan_npm_workspace_domains,
};

pub(super) enum TopologyHint {
    CargoWorkspaceRoot(CargoWorkspaceRootHint),
    NpmWorkspaceRoot(NpmWorkspaceRootHint),
}

pub(super) enum TopologyDomain {
    CargoWorkspace(CargoWorkspaceDomain),
    NpmWorkspace(NpmWorkspaceDomain),
}

pub(super) struct TopologyPlan {
    domains: Vec<TopologyDomain>,
    claimed_cargo_dirs: HashSet<PathBuf>,
    claimed_npm_dirs: HashSet<PathBuf>,
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
            collect_npm_workspace_hints(files)
                .into_iter()
                .map(TopologyHint::NpmWorkspaceRoot),
        );

        let mut domains = Vec::new();
        let mut claimed_cargo_dirs = HashSet::new();
        let mut claimed_npm_dirs = HashSet::new();

        let cargo_workspace_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(hint) => Some(hint),
                TopologyHint::NpmWorkspaceRoot(_) => None,
            })
            .collect();

        for domain in plan_cargo_workspace_domains(files, dir_files, &cargo_workspace_hints) {
            claimed_cargo_dirs.insert(domain.root_dir.clone());
            claimed_cargo_dirs.extend(domain.members.iter().map(|member| member.dir_path.clone()));
            domains.push(TopologyDomain::CargoWorkspace(domain));
        }

        let npm_workspace_hints: Vec<_> = hints
            .iter()
            .filter_map(|hint| match hint {
                TopologyHint::CargoWorkspaceRoot(_) => None,
                TopologyHint::NpmWorkspaceRoot(hint) => Some(hint),
            })
            .collect();

        for domain in plan_npm_workspace_domains(files, dir_files, &npm_workspace_hints) {
            claimed_npm_dirs.insert(domain.root_dir.clone());
            claimed_npm_dirs.extend(domain.members.iter().map(|member| member.dir_path.clone()));
            domains.push(TopologyDomain::NpmWorkspace(domain));
        }

        Self {
            domains,
            claimed_cargo_dirs,
            claimed_npm_dirs,
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

        if !config
            .datasource_ids
            .contains(&DatasourceId::NpmPackageJson)
        {
            return false;
        }

        self.claimed_npm_dirs.contains(parent_dir)
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
                TopologyDomain::NpmWorkspace(_) => {}
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
                TopologyDomain::CargoWorkspace(_) => {}
                TopologyDomain::NpmWorkspace(domain) => {
                    apply_npm_workspace_domain(domain, files, packages, dependencies);
                }
            }
        }
    }
}
