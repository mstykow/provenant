use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RepoManifest {
    pub url: Option<String>,
    pub requested_ref: Option<String>,
    pub resolved_sha: Option<String>,
    pub cache_dir: Option<PathBuf>,
}

impl RepoManifest {
    pub fn new(
        url: Option<String>,
        requested_ref: Option<String>,
        resolved_sha: Option<String>,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            url,
            requested_ref,
            resolved_sha,
            cache_dir,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetManifest {
    pub source_label: String,
    pub label: String,
    pub revision: String,
    pub resolved_path: Option<PathBuf>,
    pub checkout_path_during_run: PathBuf,
    pub checkout_retained_after_run: bool,
}

impl TargetManifest {
    pub fn new(
        source_label: String,
        label: String,
        revision: String,
        resolved_path: Option<PathBuf>,
        checkout_path_during_run: PathBuf,
        checkout_retained_after_run: bool,
    ) -> Self {
        Self {
            source_label,
            label,
            revision,
            resolved_path,
            checkout_path_during_run,
            checkout_retained_after_run,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareArtifactsManifest {
    pub raw_dir: PathBuf,
    pub comparison_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandInvocation {
    pub command: String,
    pub working_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandsManifest {
    pub scancode: CommandInvocation,
    pub provenant: CommandInvocation,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScancodeManifest {
    pub image: String,
    pub submodule_path: PathBuf,
    pub runtime_revision: String,
    pub runtime_dirty: bool,
    pub runtime_diff_hash: Option<String>,
    pub cache_key: Option<String>,
    pub cache_dir: Option<PathBuf>,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareRunManifest {
    pub run_id: String,
    pub target: TargetManifest,
    pub repo: RepoManifest,
    pub scan_profile: Option<String>,
    pub scan_args: Vec<String>,
    pub artifacts: CompareArtifactsManifest,
    pub commands: CommandsManifest,
    pub scancode: ScancodeManifest,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkRunManifest {
    pub target: TargetManifest,
    pub repo: RepoManifest,
    pub scan_profile: Option<String>,
    pub scan_args: Vec<String>,
}
