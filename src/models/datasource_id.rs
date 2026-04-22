// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Datasource identifiers for package parsers.
//!
//! Each variant uniquely identifies the type of package data source (file format)
//! that was parsed. These IDs enable the assembly system to intelligently merge
//! related package files.

use serde::{Deserialize, Serialize};
use std::fmt;
use strum::{EnumCount, EnumIter};

/// Unique identifier for the type of package data source (file format).
///
/// Datasource IDs distinguish between different file types within the same ecosystem
/// (e.g., `NpmPackageJson` vs `NpmPackageLockJson`). The assembly system uses these
/// IDs to match packages from related files for merging into a single logical package.
///
/// # Serialization
///
/// Variants serialize to snake_case strings matching the Python reference values.
/// The JSON output is identical to the Python ScanCode Toolkit.
///
/// # Examples
///
/// ```ignore
/// use provenant::models::DatasourceId;
///
/// let id = DatasourceId::NpmPackageJson;
/// assert_eq!(id.as_ref(), "npm_package_json");
/// assert_eq!(id.to_string(), "npm_package_json");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum DatasourceId {
    // ── About/README/OS ──
    AboutFile,
    Readme,
    EtcOsRelease,

    // ── Alpine ──
    AlpineApkArchive,
    AlpineApkbuild,
    AlpineInstalledDb,

    // ── Arch Linux ──
    ArchAurinfo,
    ArchPkginfo,
    ArchSrcinfo,

    // ── Android ──
    AndroidAab,
    AndroidAarLibrary,
    AndroidApk,
    AndroidManifestXml,
    AndroidSoongMetadata,

    // ── Apache Axis2 ──
    Axis2Mar,
    Axis2ModuleXml,

    // ── Autotools ──
    AutotoolsConfigure,

    // ── Bazel ──
    BazelBuild,
    BazelModule,

    // ── Bitbake ──
    BitbakeRecipe,
    BitbakeRecipeAppend,

    // ── Bower ──
    BowerJson,

    // ── Buck ──
    /// Matches Python reference value. More consistent name would be `buck_file`.
    #[serde(rename = "buck_file")]
    BuckFile,
    /// Matches Python reference value. More consistent name would be `buck_metadata`.
    #[serde(rename = "buck_metadata")]
    BuckMetadata,

    // ── Bun ──
    BunLock,
    BunLockb,

    // ── Carthage ──
    CarthageCartfile,
    CarthageCartfileResolved,

    // ── Cargo/Rust ──
    CargoLock,
    CargoToml,
    RustBinary,
    WindowsExecutable,

    // ── Chef ──
    /// Matches Python reference value.
    #[serde(rename = "chef_cookbook_metadata_json")]
    ChefCookbookMetadataJson,
    /// Matches Python reference value.
    #[serde(rename = "chef_cookbook_metadata_rb")]
    ChefCookbookMetadataRb,

    CitationCff,

    // ── CocoaPods ──
    CocoapodsPodfile,
    CocoapodsPodfileLock,
    CocoapodsPodspec,
    CocoapodsPodspecJson,

    // ── Conan ──
    #[serde(rename = "conan_conandata_yml")]
    ConanConanDataYml,
    #[serde(rename = "conan_conanfile_py")]
    ConanConanFilePy,
    #[serde(rename = "conan_conanfile_txt")]
    ConanConanFileTxt,
    ConanLock,

    // ── Conda ──
    /// Matches Python reference value.
    #[serde(rename = "conda_yaml")]
    CondaYaml,
    CondaMetaJson,
    CondaMetaYaml,

    // ── Clojure ──
    ClojureDepsEdn,
    ClojureProjectClj,

    // ── CPAN/Perl ──
    CpanDistIni,
    /// Matches Python reference value.
    #[serde(rename = "cpan_makefile")]
    CpanMakefile,
    CpanManifest,
    CpanMetaJson,
    CpanMetaYml,

    // ── CRAN/R ──
    CranDescription,

    // ── Dart/Flutter ──
    PubspecLock,
    PubspecYaml,

    // ── Debian ──
    DebianControlExtractedDeb,
    DebianControlInSource,
    DebianCopyright,
    DebianCopyrightInSource,
    DebianCopyrightInPackage,
    DebianCopyrightStandalone,
    DebianDeb,
    /// Matches Python reference value.
    #[serde(rename = "debian_source_metadata_tarball")]
    DebianSourceMetadataTarball,
    DebianDistrolessInstalledDb,
    /// Matches Python reference value.
    #[serde(rename = "debian_installed_files_list")]
    DebianInstalledFilesList,
    #[serde(rename = "debian_installed_md5sums")]
    DebianInstalledMd5Sums,
    DebianInstalledStatusDb,
    #[serde(rename = "debian_md5sums_in_extracted_deb")]
    DebianMd5SumsInExtractedDeb,
    /// Matches Python reference value.
    #[serde(rename = "debian_original_source_tarball")]
    DebianOriginalSourceTarball,
    DebianSourceControlDsc,

    // ── Deno ──
    DenoJson,
    DenoLock,

    // ── Docker ──
    Dockerfile,

    // ── Erlang / OTP ──
    ErlangOtpAppSrc,
    RebarConfig,
    RebarLock,

    // ── FreeBSD ──
    FreebsdCompactManifest,

    // ── Go ──
    Godeps,
    GoBinary,
    GoMod,
    GoModGraph,
    GoSum,
    GoWork,

    // ── Haskell / Hackage ──
    HackageCabal,
    HackageCabalProject,
    HackageStackYaml,

    // ── Gradle ──
    BuildGradle,
    GradleLockfile,
    GradleModule,

    // ── Haxe ──
    HaxelibJson,

    // ── Helm ──
    HelmChartLock,
    HelmChartYaml,

    // ── Hex/Elixir ──
    HexMixLock,

    // ── Julia ──
    JuliaProjectToml,
    JuliaManifestToml,

    // ── Java ──
    AntIvyXml,
    JavaEarApplicationXml,
    JavaEarArchive,
    JavaJar,
    JavaJarManifest,
    JavaOsgiManifest,
    JavaWarArchive,
    JavaWarWebXml,
    JbossSar,
    JbossServiceXml,

    // ── Maven ──
    MavenPom,
    MavenPomProperties,
    MesonBuild,

    SbtBuildSbt,

    // ── Microsoft ──
    MicrosoftCabinet,
    MicrosoftUpdateManifestMum,

    // ── Mobile/Browser ──
    AppleDmg,
    ChromeCrx,
    IosIpa,
    MozillaXpi,

    // ── Meteor ──
    MeteorPackage,

    NixDefaultNix,
    NixFlakeLock,
    NixFlakeNix,

    // ── npm ──
    NpmPackageJson,
    NpmPackageLockJson,

    // ── NuGet ──
    NugetCsproj,
    NugetDepsJson,
    NugetDirectoryBuildProps,
    NugetDirectoryPackagesProps,
    NugetNupkg,
    NugetProjectJson,
    NugetProjectLockJson,
    NugetPackagesConfig,
    NugetPackagesLock,
    /// Serializes to `"nuget_nuspec"` (corrected from typo `"nuget_nupsec"` in Python reference).
    /// Legacy alias `"nuget_nupsec"` is supported for backward compatibility with `--from-json`.
    #[serde(rename = "nuget_nuspec", alias = "nuget_nupsec")]
    NugetNuspec,
    NugetVbproj,
    NugetFsproj,

    // ── OCaml/opam ──
    OpamFile,

    // ── PHP/Composer ──
    PhpComposerJson,
    PhpComposerLock,

    // ── pnpm ──
    PnpmLockYaml,
    PnpmWorkspaceYaml,

    // ── Python/PyPI ──
    Pipfile,
    PipfileLock,
    PipRequirements,
    PixiLock,
    PixiToml,
    PypiPipOriginJson,
    PypiEgg,
    PypiEggPkginfo,
    PypiEditableEggPkginfo,
    PypiInspectDeplock,
    PypiJson,
    PypiPoetryLock,
    PypiPoetryPyprojectToml,
    PypiSdist,
    PypiPylockToml,
    PypiPyprojectToml,
    PypiSdistPkginfo,
    PypiSetupCfg,
    PypiSetupPy,
    PypiUvLock,
    PypiWheel,
    PypiWheelMetadata,

    // ── RPM ──
    RpmArchive,
    RpmInstalledDatabaseBdb,
    RpmInstalledDatabaseNdb,
    RpmInstalledDatabaseSqlite,
    RpmMarinerManifest,
    RpmPackageLicenses,
    /// Serializes to `"rpm_specfile"` (corrected from typo `"rpm_spefile"` in Python reference).
    /// Legacy alias `"rpm_spefile"` is supported for backward compatibility with `--from-json`.
    #[serde(rename = "rpm_specfile", alias = "rpm_spefile")]
    RpmSpecfile,
    RpmYumdb,

    // ── Ruby/RubyGems ──
    Gemfile,
    GemfileExtracted,
    GemfileLock,
    GemfileLockExtracted,
    GemArchive,
    /// Matches Python reference value.
    #[serde(rename = "gem_archive_extracted")]
    GemArchiveExtracted,
    Gemspec,
    GemspecExtracted,
    GemGemspecInstalledSpecifications,

    // ── Disk Images/Installers ──
    InstallshieldInstaller,
    IsoDiskImage,
    NsisInstaller,
    SharShellArchive,
    SquashfsDiskImage,

    // ── Swift ──
    SwiftPackageManifestJson,
    SwiftPackageResolved,
    SwiftPackageShowDependencies,

    PubliccodeYaml,

    // ── vcpkg ──
    VcpkgJson,

    // ── Yarn ──
    YarnLock,
    YarnLockV1,
    YarnLockV2,
    YarnPnpCjs,

    // ── Git ──
    Gitmodules,
}

impl DatasourceId {
    /// Returns the string representation of this datasource ID.
    ///
    /// This matches the serialized form used in JSON output.
    pub fn as_str(&self) -> &'static str {
        match self {
            // About/README/OS
            Self::AboutFile => "about_file",
            Self::Readme => "readme",
            Self::EtcOsRelease => "etc_os_release",

            // Alpine
            Self::AlpineApkArchive => "alpine_apk_archive",
            Self::AlpineApkbuild => "alpine_apkbuild",
            Self::AlpineInstalledDb => "alpine_installed_db",

            // Arch Linux
            Self::ArchAurinfo => "arch_aurinfo",
            Self::ArchPkginfo => "arch_pkginfo",
            Self::ArchSrcinfo => "arch_srcinfo",

            // Android
            Self::AndroidAab => "android_aab",
            Self::AndroidAarLibrary => "android_aar_library",
            Self::AndroidApk => "android_apk",
            Self::AndroidManifestXml => "android_manifest_xml",
            Self::AndroidSoongMetadata => "android_soong_metadata",

            // Apache Axis2
            Self::Axis2Mar => "axis2_mar",
            Self::Axis2ModuleXml => "axis2_module_xml",

            // Autotools
            Self::AutotoolsConfigure => "autotools_configure",

            // Bazel
            Self::BazelBuild => "bazel_build",

            // Bitbake
            Self::BitbakeRecipe => "bitbake_recipe",
            Self::BitbakeRecipeAppend => "bitbake_recipe_append",

            // Bower
            Self::BowerJson => "bower_json",

            // Buck
            Self::BuckFile => "buck_file",
            Self::BuckMetadata => "buck_metadata",

            // Carthage
            Self::CarthageCartfile => "carthage_cartfile",
            Self::CarthageCartfileResolved => "carthage_cartfile_resolved",

            // Cargo/Rust
            Self::CargoLock => "cargo_lock",
            Self::CargoToml => "cargo_toml",
            Self::RustBinary => "rust_binary",
            Self::WindowsExecutable => "windows_executable",

            // Chef
            Self::ChefCookbookMetadataJson => "chef_cookbook_metadata_json",
            Self::ChefCookbookMetadataRb => "chef_cookbook_metadata_rb",

            Self::CitationCff => "citation_cff",

            // CocoaPods
            Self::CocoapodsPodfile => "cocoapods_podfile",
            Self::CocoapodsPodfileLock => "cocoapods_podfile_lock",
            Self::CocoapodsPodspec => "cocoapods_podspec",
            Self::CocoapodsPodspecJson => "cocoapods_podspec_json",

            // Conan
            Self::ConanConanDataYml => "conan_conandata_yml",
            Self::ConanConanFilePy => "conan_conanfile_py",
            Self::ConanConanFileTxt => "conan_conanfile_txt",
            Self::ConanLock => "conan_lock",

            // Conda
            Self::CondaYaml => "conda_yaml",
            Self::CondaMetaJson => "conda_meta_json",
            Self::CondaMetaYaml => "conda_meta_yaml",

            // Clojure
            Self::ClojureDepsEdn => "clojure_deps_edn",
            Self::ClojureProjectClj => "clojure_project_clj",

            // CPAN/Perl
            Self::CpanDistIni => "cpan_dist_ini",
            Self::CpanMakefile => "cpan_makefile",
            Self::CpanManifest => "cpan_manifest",
            Self::CpanMetaJson => "cpan_meta_json",
            Self::CpanMetaYml => "cpan_meta_yml",

            // CRAN/R
            Self::CranDescription => "cran_description",

            // Dart/Flutter
            Self::PubspecLock => "pubspec_lock",
            Self::PubspecYaml => "pubspec_yaml",

            // Debian
            Self::DebianControlExtractedDeb => "debian_control_extracted_deb",
            Self::DebianControlInSource => "debian_control_in_source",
            Self::DebianCopyright => "debian_copyright",
            Self::DebianCopyrightInSource => "debian_copyright_in_source",
            Self::DebianCopyrightInPackage => "debian_copyright_in_package",
            Self::DebianCopyrightStandalone => "debian_copyright_standalone",
            Self::DebianDeb => "debian_deb",
            Self::DebianSourceMetadataTarball => "debian_source_metadata_tarball",
            Self::DebianDistrolessInstalledDb => "debian_distroless_installed_db",
            Self::DebianInstalledFilesList => "debian_installed_files_list",
            Self::DebianInstalledMd5Sums => "debian_installed_md5sums",
            Self::DebianInstalledStatusDb => "debian_installed_status_db",
            Self::DebianMd5SumsInExtractedDeb => "debian_md5sums_in_extracted_deb",
            Self::DebianOriginalSourceTarball => "debian_original_source_tarball",
            Self::DebianSourceControlDsc => "debian_source_control_dsc",
            Self::DenoJson => "deno_json",
            Self::DenoLock => "deno_lock",
            Self::Dockerfile => "dockerfile",
            Self::ErlangOtpAppSrc => "erlang_otp_app_src",
            Self::RebarConfig => "rebar_config",
            Self::RebarLock => "rebar_lock",
            Self::BazelModule => "bazel_module",

            // FreeBSD
            Self::FreebsdCompactManifest => "freebsd_compact_manifest",

            // Go
            Self::Godeps => "godeps",
            Self::GoBinary => "go_binary",
            Self::GoMod => "go_mod",
            Self::GoModGraph => "go_mod_graph",
            Self::GoSum => "go_sum",
            Self::GoWork => "go_work",

            // Haskell / Hackage
            Self::HackageCabal => "hackage_cabal",
            Self::HackageCabalProject => "hackage_cabal_project",
            Self::HackageStackYaml => "hackage_stack_yaml",

            // Gradle
            Self::BuildGradle => "build_gradle",
            Self::GradleLockfile => "gradle_lockfile",
            Self::GradleModule => "gradle_module",

            // Haxe
            Self::HaxelibJson => "haxelib_json",

            // Helm
            Self::HelmChartLock => "helm_chart_lock",
            Self::HelmChartYaml => "helm_chart_yaml",

            // Hex/Elixir
            Self::HexMixLock => "hex_mix_lock",

            // Julia
            Self::JuliaProjectToml => "julia_project_toml",
            Self::JuliaManifestToml => "julia_manifest_toml",

            // Java
            Self::AntIvyXml => "ant_ivy_xml",
            Self::JavaEarApplicationXml => "java_ear_application_xml",
            Self::JavaEarArchive => "java_ear_archive",
            Self::JavaJar => "java_jar",
            Self::JavaJarManifest => "java_jar_manifest",
            Self::JavaOsgiManifest => "java_osgi_manifest",
            Self::JavaWarArchive => "java_war_archive",
            Self::JavaWarWebXml => "java_war_web_xml",
            Self::JbossSar => "jboss_sar",
            Self::JbossServiceXml => "jboss_service_xml",

            // Maven
            Self::MavenPom => "maven_pom",
            Self::MavenPomProperties => "maven_pom_properties",
            Self::MesonBuild => "meson_build",
            Self::SbtBuildSbt => "sbt_build_sbt",

            // Microsoft
            Self::MicrosoftCabinet => "microsoft_cabinet",
            Self::MicrosoftUpdateManifestMum => "microsoft_update_manifest_mum",

            // Mobile/Browser
            Self::AppleDmg => "apple_dmg",
            Self::ChromeCrx => "chrome_crx",
            Self::IosIpa => "ios_ipa",
            Self::MozillaXpi => "mozilla_xpi",

            // Meteor
            Self::MeteorPackage => "meteor_package",

            Self::NixDefaultNix => "nix_default_nix",
            Self::NixFlakeLock => "nix_flake_lock",
            Self::NixFlakeNix => "nix_flake_nix",

            // npm
            Self::BunLock => "bun_lock",
            Self::BunLockb => "bun_lockb",
            Self::NpmPackageJson => "npm_package_json",
            Self::NpmPackageLockJson => "npm_package_lock_json",

            // NuGet
            Self::NugetCsproj => "nuget_csproj",
            Self::NugetDepsJson => "nuget_deps_json",
            Self::NugetDirectoryBuildProps => "nuget_directory_build_props",
            Self::NugetDirectoryPackagesProps => "nuget_directory_packages_props",
            Self::NugetNupkg => "nuget_nupkg",
            Self::NugetProjectJson => "nuget_project_json",
            Self::NugetProjectLockJson => "nuget_project_lock_json",
            Self::NugetPackagesConfig => "nuget_packages_config",
            Self::NugetPackagesLock => "nuget_packages_lock",
            Self::NugetNuspec => "nuget_nuspec",
            Self::NugetVbproj => "nuget_vbproj",
            Self::NugetFsproj => "nuget_fsproj",

            // OCaml/opam
            Self::OpamFile => "opam_file",

            // PHP/Composer
            Self::PhpComposerJson => "php_composer_json",
            Self::PhpComposerLock => "php_composer_lock",

            // pnpm
            Self::PnpmLockYaml => "pnpm_lock_yaml",
            Self::PnpmWorkspaceYaml => "pnpm_workspace_yaml",

            // Python/PyPI
            Self::Pipfile => "pipfile",
            Self::PipfileLock => "pipfile_lock",
            Self::PipRequirements => "pip_requirements",
            Self::PixiLock => "pixi_lock",
            Self::PixiToml => "pixi_toml",
            Self::PypiPipOriginJson => "pypi_pip_origin_json",
            Self::PypiEgg => "pypi_egg",
            Self::PypiEggPkginfo => "pypi_egg_pkginfo",
            Self::PypiEditableEggPkginfo => "pypi_editable_egg_pkginfo",
            Self::PypiInspectDeplock => "pypi_inspect_deplock",
            Self::PypiJson => "pypi_json",
            Self::PypiPoetryLock => "pypi_poetry_lock",
            Self::PypiPoetryPyprojectToml => "pypi_poetry_pyproject_toml",
            Self::PypiSdist => "pypi_sdist",
            Self::PypiPylockToml => "pypi_pylock_toml",
            Self::PypiPyprojectToml => "pypi_pyproject_toml",
            Self::PypiSdistPkginfo => "pypi_sdist_pkginfo",
            Self::PypiSetupCfg => "pypi_setup_cfg",
            Self::PypiSetupPy => "pypi_setup_py",
            Self::PypiUvLock => "pypi_uv_lock",
            Self::PypiWheel => "pypi_wheel",
            Self::PypiWheelMetadata => "pypi_wheel_metadata",

            // RPM
            Self::RpmArchive => "rpm_archive",
            Self::RpmInstalledDatabaseBdb => "rpm_installed_database_bdb",
            Self::RpmInstalledDatabaseNdb => "rpm_installed_database_ndb",
            Self::RpmInstalledDatabaseSqlite => "rpm_installed_database_sqlite",
            Self::RpmMarinerManifest => "rpm_mariner_manifest",
            Self::RpmPackageLicenses => "rpm_package_licenses",
            Self::RpmSpecfile => "rpm_specfile",
            Self::RpmYumdb => "rpm_yumdb",

            // Ruby/RubyGems
            Self::Gemfile => "gemfile",
            Self::GemfileExtracted => "gemfile_extracted",
            Self::GemfileLock => "gemfile_lock",
            Self::GemfileLockExtracted => "gemfile_lock_extracted",
            Self::GemArchive => "gem_archive",
            Self::GemArchiveExtracted => "gem_archive_extracted",
            Self::Gemspec => "gemspec",
            Self::GemspecExtracted => "gemspec_extracted",
            Self::GemGemspecInstalledSpecifications => "gem_gemspec_installed_specifications",

            // Disk Images/Installers
            Self::InstallshieldInstaller => "installshield_installer",
            Self::IsoDiskImage => "iso_disk_image",
            Self::NsisInstaller => "nsis_installer",
            Self::SharShellArchive => "shar_shell_archive",
            Self::SquashfsDiskImage => "squashfs_disk_image",

            // Swift
            Self::SwiftPackageManifestJson => "swift_package_manifest_json",
            Self::SwiftPackageResolved => "swift_package_resolved",
            Self::SwiftPackageShowDependencies => "swift_package_show_dependencies",

            Self::PubliccodeYaml => "publiccode_yaml",

            // vcpkg
            Self::VcpkgJson => "vcpkg_json",

            // Yarn
            Self::YarnLock => "yarn_lock",
            Self::YarnLockV1 => "yarn_lock_v1",
            Self::YarnLockV2 => "yarn_lock_v2",
            Self::YarnPnpCjs => "yarn_pnp_cjs",

            // Git
            Self::Gitmodules => "gitmodules",
        }
    }
}

impl AsRef<str> for DatasourceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for DatasourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() {
        let id = DatasourceId::NpmPackageJson;
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""npm_package_json""#);
    }

    #[test]
    fn test_deserialization() {
        let json = r#""npm_package_json""#;
        let id: DatasourceId = serde_json::from_str(json).unwrap();
        assert_eq!(id, DatasourceId::NpmPackageJson);
    }

    #[test]
    fn test_as_str() {
        assert_eq!(DatasourceId::NpmPackageJson.as_str(), "npm_package_json");
        assert_eq!(DatasourceId::CargoLock.as_str(), "cargo_lock");
        assert_eq!(
            DatasourceId::PypiPyprojectToml.as_str(),
            "pypi_pyproject_toml"
        );
        assert_eq!(DatasourceId::HackageCabal.as_str(), "hackage_cabal");
        assert_eq!(DatasourceId::CitationCff.as_str(), "citation_cff");
        assert_eq!(DatasourceId::PubliccodeYaml.as_str(), "publiccode_yaml");
        assert_eq!(DatasourceId::YarnPnpCjs.as_str(), "yarn_pnp_cjs");
    }

    #[test]
    fn test_display() {
        assert_eq!(DatasourceId::NpmPackageJson.to_string(), "npm_package_json");
    }

    #[test]
    fn test_as_ref() {
        let id = DatasourceId::NpmPackageJson;
        let s: &str = id.as_ref();
        assert_eq!(s, "npm_package_json");
    }

    #[test]
    fn test_python_rename_mappings() {
        // Test the ~12 IDs that changed from our old values to match Python
        assert_eq!(DatasourceId::BuckFile.as_str(), "buck_file");
        assert_eq!(DatasourceId::BuckMetadata.as_str(), "buck_metadata");
        assert_eq!(
            DatasourceId::ChefCookbookMetadataJson.as_str(),
            "chef_cookbook_metadata_json"
        );
        assert_eq!(
            DatasourceId::ChefCookbookMetadataRb.as_str(),
            "chef_cookbook_metadata_rb"
        );
        assert_eq!(DatasourceId::CondaYaml.as_str(), "conda_yaml");
        assert_eq!(DatasourceId::CpanMakefile.as_str(), "cpan_makefile");
        assert_eq!(
            DatasourceId::DebianInstalledFilesList.as_str(),
            "debian_installed_files_list"
        );
        assert_eq!(
            DatasourceId::DebianOriginalSourceTarball.as_str(),
            "debian_original_source_tarball"
        );
        assert_eq!(
            DatasourceId::DebianSourceMetadataTarball.as_str(),
            "debian_source_metadata_tarball"
        );
        assert_eq!(
            DatasourceId::GemArchiveExtracted.as_str(),
            "gem_archive_extracted"
        );
        // Corrected from typos in Python reference
        assert_eq!(DatasourceId::NugetNuspec.as_str(), "nuget_nuspec");
        assert_eq!(DatasourceId::RpmSpecfile.as_str(), "rpm_specfile");
    }

    #[test]
    fn test_legacy_deserialization_aliases() {
        // Test that legacy typo spellings are accepted for backward compatibility
        let legacy_nuget: DatasourceId = serde_json::from_str(r#""nuget_nupsec""#).unwrap();
        assert_eq!(legacy_nuget, DatasourceId::NugetNuspec);

        let legacy_rpm: DatasourceId = serde_json::from_str(r#""rpm_spefile""#).unwrap();
        assert_eq!(legacy_rpm, DatasourceId::RpmSpecfile);

        // Canonical spellings should also work
        let canonical_nuget: DatasourceId = serde_json::from_str(r#""nuget_nuspec""#).unwrap();
        assert_eq!(canonical_nuget, DatasourceId::NugetNuspec);

        let canonical_rpm: DatasourceId = serde_json::from_str(r#""rpm_specfile""#).unwrap();
        assert_eq!(canonical_rpm, DatasourceId::RpmSpecfile);
    }

    #[test]
    fn test_canonical_serialization() {
        // Test that canonical spellings are used for serialization
        let nuget_json = serde_json::to_string(&DatasourceId::NugetNuspec).unwrap();
        assert_eq!(nuget_json, r#""nuget_nuspec""#);

        let rpm_json = serde_json::to_string(&DatasourceId::RpmSpecfile).unwrap();
        assert_eq!(rpm_json, r#""rpm_specfile""#);
    }
}
