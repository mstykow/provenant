use crate::models::{DatasourceId, FileInfo, Package, TopLevelDependency};
use strum::EnumIter;

use super::{
    AssemblerConfig, AssemblyMode, DirectoryMergeOutput, cargo_resource_assign,
    cargo_workspace_merge, composer_resource_assign, conda_rootfs_merge, file_ref_resolve,
    hackage_merge, npm_resource_assign, npm_workspace_merge, nuget_cpm_resolve,
    python_requirements_assign, ruby_resource_assign, swift_merge,
};

#[derive(Clone, Copy)]
pub(super) enum SpecialDirectoryMergerKind {
    Skip,
    Hackage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter)]
pub(super) enum PostAssemblyPassKind {
    SwiftMerge,
    CondaRootfsMerge,
    NpmResourceAssign,
    PythonRequirementsAssign,
    FileReferenceResolve,
    RpmYumdbMerge,
    NpmWorkspaceMerge,
    CargoWorkspaceMerge,
    NugetCpmResolve,
    CargoResourceAssign,
    ComposerResourceAssign,
    RubyResourceAssign,
}

pub(super) fn special_directory_merger_for(
    config_key: DatasourceId,
) -> Option<SpecialDirectoryMergerKind> {
    match config_key {
        DatasourceId::HackageCabal => Some(SpecialDirectoryMergerKind::Hackage),
        DatasourceId::SwiftPackageManifestJson => Some(SpecialDirectoryMergerKind::Skip),
        _ => None,
    }
}

pub(super) static POST_ASSEMBLY_PASSES: &[PostAssemblyPassKind] = &[
    PostAssemblyPassKind::SwiftMerge,
    PostAssemblyPassKind::CondaRootfsMerge,
    PostAssemblyPassKind::NpmResourceAssign,
    PostAssemblyPassKind::PythonRequirementsAssign,
    PostAssemblyPassKind::FileReferenceResolve,
    PostAssemblyPassKind::RpmYumdbMerge,
    PostAssemblyPassKind::NpmWorkspaceMerge,
    PostAssemblyPassKind::CargoWorkspaceMerge,
    PostAssemblyPassKind::NugetCpmResolve,
    PostAssemblyPassKind::CargoResourceAssign,
    PostAssemblyPassKind::ComposerResourceAssign,
    PostAssemblyPassKind::RubyResourceAssign,
];

pub(super) fn run_post_assembly_passes(
    files: &mut [FileInfo],
    packages: &mut Vec<Package>,
    dependencies: &mut Vec<TopLevelDependency>,
) {
    for pass in POST_ASSEMBLY_PASSES {
        pass.run(files, packages, dependencies);
    }
}

impl SpecialDirectoryMergerKind {
    pub(super) fn run(
        self,
        files: &[FileInfo],
        file_indices: &[usize],
    ) -> Vec<DirectoryMergeOutput> {
        match self {
            Self::Skip => Vec::new(),
            Self::Hackage => hackage_merge::assemble_hackage_packages(files, file_indices),
        }
    }
}

impl PostAssemblyPassKind {
    fn run(
        self,
        files: &mut [FileInfo],
        packages: &mut Vec<Package>,
        dependencies: &mut Vec<TopLevelDependency>,
    ) {
        match self {
            Self::SwiftMerge => swift_merge::assemble_swift_packages(files, packages, dependencies),
            Self::CondaRootfsMerge => {
                conda_rootfs_merge::merge_conda_rootfs_metadata(files, packages, dependencies)
            }
            Self::NpmResourceAssign => {
                npm_resource_assign::assign_npm_package_resources(files, packages)
            }
            Self::PythonRequirementsAssign => {
                python_requirements_assign::assign_python_requirements_to_projects(
                    files,
                    packages,
                    dependencies,
                )
            }
            Self::FileReferenceResolve => {
                file_ref_resolve::resolve_file_references(files, packages, dependencies)
            }
            Self::RpmYumdbMerge => file_ref_resolve::merge_rpm_yumdb_metadata(files, packages),
            Self::NpmWorkspaceMerge => {
                npm_workspace_merge::assemble_npm_workspaces(files, packages, dependencies)
            }
            Self::CargoWorkspaceMerge => {
                cargo_workspace_merge::assemble_cargo_workspaces(files, packages, dependencies)
            }
            Self::NugetCpmResolve => {
                nuget_cpm_resolve::resolve_nuget_cpm_versions(files, dependencies)
            }
            Self::CargoResourceAssign => {
                cargo_resource_assign::assign_cargo_package_resources(files, packages)
            }
            Self::ComposerResourceAssign => {
                composer_resource_assign::assign_composer_package_resources(files, packages)
            }
            Self::RubyResourceAssign => {
                ruby_resource_assign::assign_ruby_package_resources(files, packages)
            }
        }
    }
}

pub static ASSEMBLERS: &[AssemblerConfig] = &[
    // ── Sibling-merge assemblers ──
    //
    // npm ecosystem: package.json + lockfiles in same directory.
    // NOTE: npm-shrinkwrap.json emits "npm_package_lock_json" as its datasource_id,
    // so "npm_shrinkwrap_json" is NOT a real datasource_id.
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::BunLock,
            DatasourceId::BunLockb,
            DatasourceId::NpmPackageJson,
            DatasourceId::NpmPackageLockJson,
            DatasourceId::YarnLock,
            DatasourceId::YarnLockV1,
            DatasourceId::YarnLockV2,
            DatasourceId::PnpmLockYaml,
            DatasourceId::PnpmWorkspaceYaml,
        ],
        sibling_file_patterns: &[
            "package.json",
            "bun.lock",
            "bun.lockb",
            ".package-lock.json",
            "package-lock.json",
            ".npm-shrinkwrap.json",
            "npm-shrinkwrap.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "shrinkwrap.yaml",
            "pnpm-workspace.yaml",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // Rust/Cargo ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::CargoToml, DatasourceId::CargoLock],
        sibling_file_patterns: &["Cargo.toml", "Cargo.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    // CocoaPods ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::CocoapodsPodspec,
            DatasourceId::CocoapodsPodspecJson,
            DatasourceId::CocoapodsPodfile,
            DatasourceId::CocoapodsPodfileLock,
        ],
        sibling_file_patterns: &["*.podspec", "*.podspec.json", "Podfile", "Podfile.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    // PHP Composer ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::PhpComposerJson, DatasourceId::PhpComposerLock],
        sibling_file_patterns: &[
            "*composer.json",
            "composer.*.json",
            "*composer.lock",
            "composer.*.lock",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // Go ecosystem (includes legacy Godeps)
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::GoMod,
            DatasourceId::GoModGraph,
            DatasourceId::GoSum,
            DatasourceId::GoWork,
            DatasourceId::Godeps,
        ],
        sibling_file_patterns: &[
            "go.mod",
            "go.work",
            "go.mod.graph",
            "go.modgraph",
            "go.sum",
            "Godeps.json",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // Dart/Flutter ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::PubspecYaml, DatasourceId::PubspecLock],
        sibling_file_patterns: &["pubspec.yaml", "pubspec.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Pixi ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::PixiToml, DatasourceId::PixiLock],
        sibling_file_patterns: &["pixi.toml", "pixi.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::NixFlakeNix, DatasourceId::NixFlakeLock],
        sibling_file_patterns: &["flake.nix", "flake.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::NixDefaultNix],
        sibling_file_patterns: &["default.nix"],
        mode: AssemblyMode::OnePerPackageData,
    },
    // Helm chart ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::HelmChartYaml, DatasourceId::HelmChartLock],
        sibling_file_patterns: &["Chart.yaml", "Chart.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::HackageCabal,
            DatasourceId::HackageCabalProject,
            DatasourceId::HackageStackYaml,
        ],
        sibling_file_patterns: &["*.cabal", "cabal.project", "stack.yaml"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Chef ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::ChefCookbookMetadataJson,
            DatasourceId::ChefCookbookMetadataRb,
        ],
        sibling_file_patterns: &["metadata.json", "metadata.rb"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Conan (C/C++) ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::ConanConanFilePy,
            DatasourceId::ConanConanFileTxt,
            DatasourceId::ConanLock,
            DatasourceId::ConanConanDataYml,
        ],
        sibling_file_patterns: &[
            "conanfile.py",
            "conanfile.txt",
            "conan.lock",
            "conandata.yml",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // Maven/Java ecosystem (nested merge via META-INF)
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::MavenPom,
            DatasourceId::MavenPomProperties,
            DatasourceId::JavaJarManifest,
            DatasourceId::JavaOsgiManifest,
        ],
        sibling_file_patterns: &[
            "pom.xml",
            "*.pom",
            "pom.properties",
            "**/META-INF/MANIFEST.MF",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::PypiWheel, DatasourceId::PypiPipOriginJson],
        sibling_file_patterns: &["*.whl", "origin.json"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Python/PyPI ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::PypiPyprojectToml,
            DatasourceId::PypiPoetryPyprojectToml,
            DatasourceId::PypiSetupPy,
            DatasourceId::PypiSetupCfg,
            DatasourceId::PypiWheel,
            DatasourceId::PypiWheelMetadata,
            DatasourceId::PypiEgg,
            DatasourceId::PypiEggPkginfo,
            DatasourceId::PypiEditableEggPkginfo,
            DatasourceId::PypiJson,
            DatasourceId::PypiSdist,
            DatasourceId::PypiSdistPkginfo,
            DatasourceId::PypiInspectDeplock,
            DatasourceId::PipRequirements,
            DatasourceId::PypiPoetryLock,
            DatasourceId::PypiPylockToml,
            DatasourceId::PypiUvLock,
            DatasourceId::Pipfile,
            DatasourceId::PipfileLock,
        ],
        sibling_file_patterns: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "PKG-INFO",
            "METADATA",
            "pypi.json",
            "pip-inspect.deplock",
            "*.tar.gz",
            "*.tgz",
            "*.tar.bz2",
            "*.tar.xz",
            "*.zip",
            "requirements*.txt",
            "Pipfile",
            "Pipfile.lock",
            "poetry.lock",
            "pylock.toml",
            "pylock.*.toml",
            "uv.lock",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::DenoJson, DatasourceId::DenoLock],
        sibling_file_patterns: &["deno.json", "deno.jsonc", "deno.lock"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Ruby/RubyGems ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::GemArchiveExtracted,
            DatasourceId::Gemspec,
            DatasourceId::GemspecExtracted,
            DatasourceId::Gemfile,
            DatasourceId::GemfileExtracted,
            DatasourceId::GemfileLock,
            DatasourceId::GemfileLockExtracted,
            DatasourceId::GemArchive,
        ],
        sibling_file_patterns: &[
            "metadata.gz-extract",
            "**/data.gz-extract/*.gemspec",
            "**/data.gz-extract/Gemfile",
            "**/data.gz-extract/Gemfile.lock",
            "*.gemspec",
            "Gemfile",
            "Gemfile.lock",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // Conda ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::CondaMetaYaml,
            DatasourceId::CondaYaml,
            DatasourceId::CondaMetaJson,
        ],
        sibling_file_patterns: &[
            "meta.yaml",
            "meta.yml",
            "environment.yml",
            "environment.yaml",
            "conda.yaml",
            "conda.yml",
            "*conda*.yaml",
            "*conda*.yml",
            "env.yaml",
            "env.yml",
            "*env*.yaml",
            "*env*.yml",
            "*environment*.yaml",
            "*environment*.yml",
            "*.json",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // RPM specfile (source packages)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::RpmSpecfile],
        sibling_file_patterns: &["*.spec"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Debian source packages (nested merge via debian/ directory)
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::DebianControlInSource,
            DatasourceId::DebianCopyrightInSource,
        ],
        sibling_file_patterns: &["**/debian/control", "**/debian/copyright"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Gradle/Android ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::BuildGradle, DatasourceId::GradleLockfile],
        sibling_file_patterns: &["build.gradle", "build.gradle.kts", "gradle.lockfile"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::GradleModule],
        sibling_file_patterns: &["*.module"],
        mode: AssemblyMode::OnePerPackageData,
    },
    // CPAN/Perl ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::CpanMetaJson,
            DatasourceId::CpanMetaYml,
            DatasourceId::CpanManifest,
            DatasourceId::CpanDistIni,
            DatasourceId::CpanMakefile,
        ],
        sibling_file_patterns: &[
            "META.json",
            "META.yml",
            "MANIFEST",
            "dist.ini",
            "Makefile.PL",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // NuGet/.NET ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::NugetCsproj,
            DatasourceId::NugetFsproj,
            DatasourceId::NugetNuspec,
            DatasourceId::NugetNupkg,
            DatasourceId::NugetProjectJson,
            DatasourceId::NugetProjectLockJson,
            DatasourceId::NugetPackagesConfig,
            DatasourceId::NugetPackagesLock,
            DatasourceId::NugetVbproj,
        ],
        sibling_file_patterns: &[
            "*.csproj",
            "*.fsproj",
            "*.nuspec",
            "*.nupkg",
            "project.json",
            "project.lock.json",
            "packages.config",
            "packages.lock.json",
            "*.packages.lock.json",
            "*.vbproj",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::NugetDepsJson],
        sibling_file_patterns: &["*.deps.json"],
        mode: AssemblyMode::OnePerPackageData,
    },
    // Swift/SPM ecosystem
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::SwiftPackageManifestJson,
            DatasourceId::SwiftPackageResolved,
            DatasourceId::SwiftPackageShowDependencies,
        ],
        sibling_file_patterns: &[
            "Package.swift.json",
            "Package.swift.deplock",
            "Package.resolved",
            ".package.resolved",
            "swift-show-dependencies.deplock",
        ],
        mode: AssemblyMode::SiblingMerge,
    },
    // ── Standalone assemblers (single file → single package) ──
    //
    // These ecosystems have only one manifest file type with no sibling merging.
    // They still need configs so their datasource_ids are recognized by the assembler.
    //
    // Bower (JavaScript)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::BowerJson],
        sibling_file_patterns: &["bower.json"],
        mode: AssemblyMode::SiblingMerge,
    },
    // CRAN (R language)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::CranDescription],
        sibling_file_patterns: &["DESCRIPTION"],
        mode: AssemblyMode::SiblingMerge,
    },
    // FreeBSD packages
    AssemblerConfig {
        datasource_ids: &[DatasourceId::FreebsdCompactManifest],
        sibling_file_patterns: &["+COMPACT_MANIFEST"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Haxe ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::HaxelibJson],
        sibling_file_patterns: &["haxelib.json"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::Gitmodules],
        sibling_file_patterns: &[".gitmodules"],
        mode: AssemblyMode::SiblingMerge,
    },
    // OCaml/opam ecosystem
    AssemblerConfig {
        datasource_ids: &[DatasourceId::OpamFile],
        sibling_file_patterns: &["opam", "*.opam"],
        mode: AssemblyMode::SiblingMerge,
    },
    // RPM Mariner manifest
    AssemblerConfig {
        datasource_ids: &[DatasourceId::RpmMarinerManifest],
        sibling_file_patterns: &["*.rpm.manifest"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::RpmYumdb],
        sibling_file_patterns: &["**/var/lib/yum/yumdb/*/*/from_repo"],
        mode: AssemblyMode::OnePerPackageData,
    },
    // Microsoft Update Manifest
    AssemblerConfig {
        datasource_ids: &[DatasourceId::MicrosoftUpdateManifestMum],
        sibling_file_patterns: &["*.mum"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Autotools (C/C++ build system)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::AutotoolsConfigure],
        sibling_file_patterns: &["configure", "configure.ac"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Bazel (build system)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::BazelBuild],
        sibling_file_patterns: &["BUILD"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::BazelModule],
        sibling_file_patterns: &["MODULE.bazel"],
        mode: AssemblyMode::OnePerPackageData,
    },
    // Buck (build system)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::BuckFile, DatasourceId::BuckMetadata],
        sibling_file_patterns: &["BUCK", "METADATA.bzl", ".buckconfig"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Ant/Ivy (Java dependency management)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::AntIvyXml],
        sibling_file_patterns: &["ivy.xml"],
        mode: AssemblyMode::SiblingMerge,
    },
    // Meteor (JavaScript platform)
    AssemblerConfig {
        datasource_ids: &[DatasourceId::MeteorPackage],
        sibling_file_patterns: &["package.js"],
        mode: AssemblyMode::SiblingMerge,
    },
    // ── One-per-PackageData assemblers (database files with many packages) ──
    //
    // Alpine installed package database
    AssemblerConfig {
        datasource_ids: &[DatasourceId::AlpineInstalledDb],
        sibling_file_patterns: &["installed"],
        mode: AssemblyMode::OnePerPackageData,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::AlpineApkbuild],
        sibling_file_patterns: &["APKBUILD"],
        mode: AssemblyMode::SiblingMerge,
    },
    // RPM installed package databases (BDB, NDB, SQLite)
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::RpmInstalledDatabaseBdb,
            DatasourceId::RpmInstalledDatabaseNdb,
            DatasourceId::RpmInstalledDatabaseSqlite,
        ],
        sibling_file_patterns: &["Packages", "Packages.db", "rpmdb.sqlite"],
        mode: AssemblyMode::OnePerPackageData,
    },
    // Debian installed package databases
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::DebianInstalledStatusDb,
            DatasourceId::DebianDistrolessInstalledDb,
        ],
        sibling_file_patterns: &["status"],
        mode: AssemblyMode::OnePerPackageData,
    },
    AssemblerConfig {
        datasource_ids: &[
            DatasourceId::DebianControlExtractedDeb,
            DatasourceId::DebianMd5SumsInExtractedDeb,
        ],
        sibling_file_patterns: &["control", "md5sums"],
        mode: AssemblyMode::SiblingMerge,
    },
    AssemblerConfig {
        datasource_ids: &[DatasourceId::AboutFile],
        sibling_file_patterns: &["*.ABOUT"],
        mode: AssemblyMode::OnePerPackageData,
    },
];

// Datasource IDs intentionally excluded from package assembly.
//
// This list is runtime-significant: files with these datasource IDs may remain
// unowned by any Package, while their dependencies are still eligible for
// top-level hoisting. Tests also use it to enforce explicit assembly accounting.
pub static UNASSEMBLED_DATASOURCE_IDS: &[DatasourceId] = &[
    // Non-package metadata
    DatasourceId::Readme,
    DatasourceId::EtcOsRelease,
    // Binary archives (require external extraction via ExtractCode before scanning)
    DatasourceId::AlpineApkArchive,
    DatasourceId::AndroidAarLibrary,
    DatasourceId::AndroidApk,
    DatasourceId::AppleDmg,
    DatasourceId::Axis2Mar,
    DatasourceId::ChromeCrx,
    DatasourceId::DebianDeb,
    DatasourceId::DebianOriginalSourceTarball,
    DatasourceId::DebianSourceMetadataTarball,
    DatasourceId::InstallshieldInstaller,
    DatasourceId::IosIpa,
    DatasourceId::IsoDiskImage,
    DatasourceId::JavaEarArchive,
    DatasourceId::JavaJar,
    DatasourceId::JavaWarArchive,
    DatasourceId::JbossSar,
    DatasourceId::MicrosoftCabinet,
    DatasourceId::MozillaXpi,
    DatasourceId::NsisInstaller,
    DatasourceId::RpmArchive,
    DatasourceId::SharShellArchive,
    DatasourceId::SquashfsDiskImage,
    // Supplementary metadata (not primary package definitions)
    DatasourceId::ArchAurinfo,
    DatasourceId::ArchPkginfo,
    DatasourceId::ArchSrcinfo,
    DatasourceId::Axis2ModuleXml,
    DatasourceId::ClojureDepsEdn,
    DatasourceId::ClojureProjectClj,
    DatasourceId::DebianInstalledFilesList,
    DatasourceId::DebianInstalledMd5Sums,
    DatasourceId::DebianCopyright,
    DatasourceId::DebianCopyrightInPackage,
    DatasourceId::DebianCopyrightStandalone,
    DatasourceId::GoBinary,
    DatasourceId::DebianSourceControlDsc,
    DatasourceId::Dockerfile,
    DatasourceId::HexMixLock,
    DatasourceId::JavaEarApplicationXml,
    DatasourceId::JavaWarWebXml,
    DatasourceId::JbossServiceXml,
    DatasourceId::MesonBuild,
    DatasourceId::GemGemspecInstalledSpecifications,
    DatasourceId::NugetDirectoryBuildProps,
    DatasourceId::NugetDirectoryPackagesProps,
    DatasourceId::RpmPackageLicenses,
    DatasourceId::RustBinary,
    DatasourceId::SbtBuildSbt,
    DatasourceId::VcpkgJson,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use strum::IntoEnumIterator;

    #[test]
    fn test_every_datasource_id_is_accounted_for() {
        let mut assembled: HashSet<DatasourceId> = HashSet::new();
        for config in ASSEMBLERS {
            for &dsid in config.datasource_ids {
                assembled.insert(dsid);
            }
        }

        let unassembled: HashSet<DatasourceId> =
            UNASSEMBLED_DATASOURCE_IDS.iter().copied().collect();

        let overlap: Vec<_> = assembled.intersection(&unassembled).collect();
        assert!(
            overlap.is_empty(),
            "Datasource IDs in BOTH ASSEMBLERS and UNASSEMBLED: {overlap:?}"
        );

        let missing: Vec<_> = DatasourceId::iter()
            .filter(|dsid| !assembled.contains(dsid) && !unassembled.contains(dsid))
            .collect();

        assert!(
            missing.is_empty(),
            "Datasource IDs in NEITHER ASSEMBLERS nor UNASSEMBLED: {missing:?}\n\
             Add each to an AssemblerConfig in ASSEMBLERS, or to UNASSEMBLED_DATASOURCE_IDS."
        );
    }

    #[test]
    fn test_post_assembly_passes_are_unique() {
        let unique: HashSet<PostAssemblyPassKind> = POST_ASSEMBLY_PASSES.iter().copied().collect();

        assert_eq!(
            unique.len(),
            POST_ASSEMBLY_PASSES.len(),
            "POST_ASSEMBLY_PASSES contains duplicate entries"
        );
    }

    #[test]
    fn test_every_post_assembly_pass_kind_is_registered_once() {
        let registered: HashSet<PostAssemblyPassKind> =
            POST_ASSEMBLY_PASSES.iter().copied().collect();

        let missing: Vec<_> = PostAssemblyPassKind::iter()
            .filter(|pass| !registered.contains(pass))
            .collect();

        assert!(
            missing.is_empty(),
            "Post-assembly pass variants not registered in POST_ASSEMBLY_PASSES: {missing:?}"
        );

        for pass in PostAssemblyPassKind::iter() {
            let count = POST_ASSEMBLY_PASSES
                .iter()
                .filter(|registered| **registered == pass)
                .count();
            assert_eq!(
                count, 1,
                "Post-assembly pass {pass:?} should be registered exactly once"
            );
        }
    }
}
