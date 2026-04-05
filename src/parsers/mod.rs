mod about;
#[cfg(test)]
mod about_scan_test;
#[cfg(test)]
mod about_test;
mod alpine;
#[cfg(test)]
mod alpine_scan_test;
mod arch;
#[cfg(test)]
mod arch_scan_test;
#[cfg(test)]
mod arch_test;
mod autotools;
#[cfg(test)]
mod autotools_test;
mod bazel;
#[cfg(test)]
mod bazel_module_test;
#[cfg(test)]
mod bazel_test;
mod bower;
#[cfg(test)]
mod bower_scan_test;
#[cfg(test)]
mod bower_test;
mod buck;
#[cfg(test)]
mod buck_test;
mod bun_lock;
#[cfg(test)]
mod bun_lock_test;
mod bun_lockb;
#[cfg(test)]
mod bun_lockb_test;
mod cargo;
mod cargo_lock;
#[cfg(test)]
mod cargo_lock_test;
#[cfg(test)]
mod cargo_scan_test;
#[cfg(test)]
mod cargo_test;
mod chef;
#[cfg(test)]
mod chef_scan_test;
#[cfg(test)]
mod chef_test;
mod citation;
#[cfg(test)]
mod citation_test;
mod clojure;
#[cfg(test)]
mod clojure_test;
#[cfg(test)]
mod cocoapods_scan_test;
mod compiled_binary;
mod composer;
#[cfg(test)]
mod composer_scan_test;
#[cfg(test)]
mod composer_test;
mod conan;
mod conan_data;
#[cfg(test)]
mod conan_data_test;
#[cfg(test)]
mod conan_scan_test;
#[cfg(test)]
mod conan_test;
mod conda;
mod conda_meta_json;
#[cfg(test)]
mod conda_meta_json_test;
#[cfg(test)]
mod conda_scan_test;
#[cfg(test)]
mod conda_test;
mod cpan;
mod cpan_dist_ini;
#[cfg(test)]
mod cpan_dist_ini_test;
mod cpan_makefile_pl;
#[cfg(test)]
mod cpan_makefile_pl_test;
#[cfg(test)]
mod cpan_scan_test;
#[cfg(test)]
mod cpan_test;
mod cran;
#[cfg(test)]
mod cran_scan_test;
#[cfg(test)]
mod cran_test;
mod dart;
#[cfg(test)]
mod dart_scan_test;
#[cfg(test)]
mod dart_test;
mod debian;
#[cfg(test)]
mod debian_scan_test;
#[cfg(test)]
mod debian_test;
mod deno;
mod deno_lock;
#[cfg(test)]
mod deno_lock_test;
#[cfg(test)]
mod deno_scan_test;
#[cfg(test)]
mod deno_test;
mod docker;
#[cfg(test)]
mod docker_scan_test;
#[cfg(test)]
mod docker_test;
mod freebsd;
#[cfg(test)]
mod freebsd_scan_test;
#[cfg(test)]
mod freebsd_test;
mod gitmodules;
#[cfg(test)]
mod gitmodules_scan_test;
mod go;
mod go_mod_graph;
#[cfg(test)]
mod go_scan_test;
#[cfg(test)]
mod go_test;
#[cfg(test)]
mod go_work_test;
#[cfg(all(test, feature = "golden-tests"))]
pub(crate) mod golden_test_utils;
mod gradle;
mod gradle_lock;
#[cfg(test)]
mod gradle_lock_test;
mod gradle_module;
#[cfg(test)]
mod gradle_module_scan_test;
#[cfg(test)]
mod gradle_module_test;
#[cfg(test)]
mod gradle_scan_test;
mod hackage;
#[cfg(test)]
mod hackage_scan_test;
#[cfg(test)]
mod hackage_test;
mod haxe;
#[cfg(test)]
mod haxe_scan_test;
#[cfg(test)]
mod haxe_test;
mod helm;
#[cfg(test)]
mod helm_scan_test;
#[cfg(test)]
mod helm_test;
mod hex_lock;
#[cfg(test)]
mod hex_lock_test;
mod license_normalization;
mod maven;
#[cfg(test)]
mod maven_scan_test;
#[cfg(test)]
mod maven_test;
mod meson;
#[cfg(test)]
mod meson_test;
pub mod metadata;
mod microsoft_update_manifest;
#[cfg(test)]
mod microsoft_update_manifest_test;
mod misc;
#[cfg(test)]
mod misc_test;
mod nix;
#[cfg(test)]
mod nix_scan_test;
#[cfg(test)]
mod nix_test;
mod npm;
mod npm_lock;
#[cfg(test)]
mod npm_lock_test;
#[cfg(test)]
mod npm_scan_test;
#[cfg(test)]
mod npm_test;
mod npm_workspace;
#[cfg(test)]
mod npm_workspace_test;
mod nuget;
#[cfg(test)]
mod nuget_scan_test;
#[cfg(test)]
mod nuget_test;
mod opam;
#[cfg(test)]
mod opam_scan_test;
mod os_release;
#[cfg(test)]
mod os_release_test;
#[cfg(test)]
mod osgi_test;
mod pep508;
mod pip_inspect_deplock;
#[cfg(test)]
mod pip_inspect_deplock_test;
mod pipfile_lock;
#[cfg(test)]
mod pipfile_lock_test;
mod pixi;
#[cfg(test)]
mod pixi_scan_test;
#[cfg(test)]
mod pixi_test;
mod pnpm_lock;
#[cfg(test)]
mod pnpm_lock_test;
mod podfile;
mod podfile_lock;
#[cfg(test)]
mod podfile_lock_test;
mod podspec;
mod podspec_json;
#[cfg(test)]
mod podspec_json_test;
mod poetry_lock;
#[cfg(test)]
mod poetry_lock_test;
mod publiccode;
#[cfg(test)]
mod publiccode_test;
mod pylock_toml;
#[cfg(test)]
mod pylock_toml_test;
mod python;
#[cfg(test)]
mod python_scan_test;
#[cfg(test)]
mod python_test;
mod readme;
#[cfg(test)]
mod readme_test;
mod requirements_txt;
#[cfg(test)]
mod requirements_txt_test;
pub(crate) mod rfc822;
mod rpm_db;
mod rpm_db_native;
#[cfg(test)]
mod rpm_db_scan_test;
mod rpm_license_files;
#[cfg(test)]
mod rpm_license_files_test;
mod rpm_mariner_manifest;
#[cfg(test)]
mod rpm_mariner_manifest_test;
mod rpm_parser;
#[cfg(test)]
mod rpm_scan_test;
mod rpm_specfile;
#[cfg(test)]
mod rpm_specfile_test;
mod rpm_yumdb;
mod ruby;
#[cfg(test)]
mod ruby_scan_test;
#[cfg(test)]
mod ruby_test;
mod sbt;
#[cfg(test)]
mod sbt_test;
#[cfg(test)]
mod scan_test_utils;
mod swift_manifest_json;
#[cfg(test)]
mod swift_manifest_json_test;
mod swift_resolved;
#[cfg(test)]
mod swift_resolved_test;
#[cfg(test)]
mod swift_scan_test;
mod swift_show_dependencies;
#[cfg(test)]
mod swift_show_dependencies_test;
pub mod utils;
mod uv_lock;
#[cfg(test)]
mod uv_lock_test;
mod vcpkg;
#[cfg(test)]
mod vcpkg_scan_test;
#[cfg(test)]
mod vcpkg_test;
mod yarn_lock;
#[cfg(test)]
mod yarn_lock_test;
mod yarn_pnp;
#[cfg(test)]
mod yarn_pnp_test;

#[cfg(all(test, feature = "golden-tests"))]
mod golden_test;

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use crate::models::{PackageData, PackageType};
use crate::parsers::license_normalization::finalize_package_declared_license_references;

thread_local! {
    static PARSER_DIAGNOSTIC_STACK: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Default)]
pub struct ParsePackagesResult {
    pub packages: Vec<PackageData>,
    pub scan_errors: Vec<String>,
}

fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

pub(crate) fn capture_parser_diagnostics<F>(
    extract: F,
    handler_name: &str,
    path: &Path,
) -> ParsePackagesResult
where
    F: FnOnce() -> Vec<PackageData>,
{
    PARSER_DIAGNOSTIC_STACK.with(|stack| {
        stack.borrow_mut().push(Vec::new());
    });

    let extract_result = catch_unwind(AssertUnwindSafe(extract));
    let mut scan_errors =
        PARSER_DIAGNOSTIC_STACK.with(|stack| stack.borrow_mut().pop().unwrap_or_default());

    match extract_result {
        Ok(packages) => ParsePackagesResult {
            packages: packages
                .into_iter()
                .map(|mut package| {
                    finalize_package_declared_license_references(&mut package);
                    package
                })
                .collect(),
            scan_errors,
        },
        Err(payload) => {
            scan_errors.push(format!(
                "{} panicked while parsing {}: {}",
                handler_name,
                path.display(),
                panic_payload_to_string(payload.as_ref())
            ));
            ParsePackagesResult {
                packages: Vec::new(),
                scan_errors,
            }
        }
    }
}

pub(crate) fn record_parser_diagnostic(message: String) -> bool {
    PARSER_DIAGNOSTIC_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(active) = stack.last_mut() else {
            return false;
        };
        active.push(message);
        true
    })
}

#[macro_export]
macro_rules! parser_warn {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        if !$crate::parsers::record_parser_diagnostic(message.clone()) {
            log::warn!("{message}");
        }
    }};
}

/// Package parser trait for extracting metadata from package manifest files.
///
/// Each parser implementation handles a specific package manager/ecosystem
/// (npm, Maven, Python, Cargo, etc.) and extracts standardized metadata into
/// `PackageData` structures compatible with ScanCode Toolkit JSON output format.
///
/// # Implementation Guide
///
/// Implementors must provide:
/// - `PACKAGE_TYPE`: Package URL (purl) type identifier (e.g., "npm", "pypi", "maven")
/// - `is_match()`: Returns true if the given file path matches this parser's expected format
/// - `extract_packages()`: Parses the file and returns all extracted package metadata
///
/// # Error Handling
///
/// Parsers should handle errors gracefully by returning default/empty `PackageData`
/// and logging warnings with [`crate::parser_warn!`] rather than panicking. Scanner
/// dispatch captures those warnings and attaches them to `FileInfo.scan_errors` so
/// CI output and serialized scan results stay aligned.
/// This allows the scan to continue processing other files even when individual
/// files fail to parse.
///
/// # Example
///
/// ```ignore
/// use provenant::models::{PackageData, PackageType};
/// use provenant::parsers::PackageParser;
/// use std::path::Path;
///
/// pub struct MyParser;
///
/// impl PackageParser for MyParser {
///     const PACKAGE_TYPE: PackageType = PackageType::Npm;
///
///     fn is_match(path: &Path) -> bool {
///         path.file_name().is_some_and(|name| name == "package.json")
///     }
///
///     fn extract_packages(path: &Path) -> Vec<PackageData> {
///         vec![PackageData::default()]
///     }
/// }
/// ```
pub trait PackageParser {
    /// Package URL type identifier for this parser (e.g., PackageType::Npm, PackageType::Pypi).
    const PACKAGE_TYPE: PackageType;

    /// Extracts all packages from the given file path.
    ///
    /// Returns a vector of `PackageData` structures containing all extracted metadata
    /// including name, version, dependencies, licenses, etc. Most parsers return a
    /// single-element vector, but some (e.g., Bazel BUILD, Buck BUCK, Debian control)
    /// can contain multiple packages in a single file.
    ///
    /// On parse errors, returns a vector with a default `PackageData` with minimal or
    /// no fields populated.
    fn extract_packages(path: &Path) -> Vec<PackageData>;

    /// Checks if the given file path matches this parser's expected format.
    ///
    /// Returns true if the file should be handled by this parser based on filename,
    /// extension, or path patterns. Used by the scanner to route files to appropriate parsers.
    fn is_match(path: &Path) -> bool;

    /// Returns the first package from [`extract_packages()`](Self::extract_packages),
    /// or a default [`PackageData`] if the file contains no packages.
    fn extract_first_package(path: &Path) -> PackageData {
        Self::extract_packages(path)
            .into_iter()
            .map(|mut package| {
                finalize_package_declared_license_references(&mut package);
                package
            })
            .next()
            .unwrap_or_default()
    }
}

pub use self::about::AboutFileParser;
pub use self::alpine::{AlpineApkParser, AlpineApkbuildParser, AlpineInstalledParser};
pub use self::arch::{ArchPkginfoParser, ArchSrcinfoParser};
pub use self::autotools::AutotoolsConfigureParser;
pub use self::bazel::{BazelBuildParser, BazelModuleParser};
pub use self::bower::BowerJsonParser;
pub use self::buck::{BuckBuildParser, BuckMetadataBzlParser};
pub use self::bun_lock::BunLockParser;
pub use self::bun_lockb::BunLockbParser;
pub use self::cargo::CargoParser;
#[cfg_attr(not(test), allow(unused_imports))]
pub use self::cargo_lock::CargoLockParser;
pub use self::chef::{ChefMetadataJsonParser, ChefMetadataRbParser};
pub use self::citation::CitationCffParser;
pub use self::clojure::{ClojureDepsEdnParser, ClojureProjectCljParser};
pub(crate) use self::compiled_binary::try_parse_compiled_bytes;
pub use self::composer::{ComposerJsonParser, ComposerLockParser};
pub use self::conan::{ConanFilePyParser, ConanLockParser, ConanfileTxtParser};
pub use self::conan_data::ConanDataParser;
pub use self::conda::{CondaEnvironmentYmlParser, CondaMetaYamlParser};
pub use self::conda_meta_json::CondaMetaJsonParser;
pub use self::cpan::{CpanManifestParser, CpanMetaJsonParser, CpanMetaYmlParser};
pub use self::cpan_dist_ini::CpanDistIniParser;
pub use self::cpan_makefile_pl::CpanMakefilePlParser;
pub use self::cran::CranParser;
pub use self::dart::{PubspecLockParser, PubspecYamlParser};
pub use self::debian::{
    DebianControlInExtractedDebParser, DebianControlParser, DebianCopyrightParser, DebianDebParser,
    DebianDebianTarParser, DebianDistrolessInstalledParser, DebianDscParser,
    DebianInstalledListParser, DebianInstalledMd5sumsParser, DebianInstalledParser,
    DebianMd5sumInPackageParser, DebianOrigTarParser,
};
pub use self::deno::DenoParser;
pub use self::deno_lock::DenoLockParser;
pub use self::docker::DockerfileParser;
pub use self::freebsd::FreebsdCompactManifestParser;
pub use self::gitmodules::GitmodulesParser;
pub use self::go::{GoModParser, GoSumParser, GoWorkParser, GodepsParser};
pub use self::go_mod_graph::GoModGraphParser;
pub use self::gradle::GradleParser;
pub use self::gradle_lock::GradleLockfileParser;
pub use self::gradle_module::GradleModuleParser;
pub use self::hackage::{HackageCabalParser, HackageCabalProjectParser, HackageStackYamlParser};
pub use self::haxe::HaxeParser;
pub use self::helm::{HelmChartLockParser, HelmChartYamlParser};
pub use self::hex_lock::HexLockParser;
pub use self::maven::MavenParser;
pub use self::meson::MesonParser;
pub use self::microsoft_update_manifest::MicrosoftUpdateManifestParser;
pub use self::misc::{
    AndroidApkRecognizer, AndroidLibraryRecognizer, AppleDmgRecognizer, Axis2MarRecognizer,
    Axis2ModuleXmlRecognizer, CabArchiveRecognizer, ChromeCrxRecognizer, InstallShieldRecognizer,
    IosIpaRecognizer, IsoImageRecognizer, IvyXmlRecognizer, JBossSarRecognizer,
    JBossServiceXmlRecognizer, JavaEarAppXmlRecognizer, JavaEarRecognizer, JavaJarRecognizer,
    JavaWarRecognizer, JavaWarWebXmlRecognizer, MeteorPackageRecognizer, MozillaXpiRecognizer,
    NsisRecognizer, SharArchiveRecognizer, SquashfsRecognizer,
};
pub use self::nix::{NixDefaultParser, NixFlakeLockParser, NixFlakeParser};
pub use self::npm::NpmParser;
pub use self::npm_lock::NpmLockParser;
pub use self::npm_workspace::NpmWorkspaceParser;
pub use self::nuget::{
    CentralPackageManagementPropsParser, DirectoryBuildPropsParser, DotNetDepsJsonParser,
    NupkgParser, NuspecParser, PackageReferenceProjectParser, PackagesConfigParser,
    PackagesLockParser, ProjectJsonParser, ProjectLockJsonParser,
};
pub use self::opam::OpamParser;
pub use self::os_release::OsReleaseParser;
pub use self::pip_inspect_deplock::PipInspectDeplockParser;
pub use self::pipfile_lock::PipfileLockParser;
pub use self::pixi::{PixiLockParser, PixiTomlParser};
pub use self::pnpm_lock::PnpmLockParser;
pub use self::podfile::PodfileParser;
pub use self::podfile_lock::PodfileLockParser;
pub use self::podspec::PodspecParser;
pub use self::podspec_json::PodspecJsonParser;
pub use self::poetry_lock::PoetryLockParser;
pub use self::publiccode::PubliccodeParser;
pub use self::pylock_toml::PylockTomlParser;
pub use self::python::PythonParser;
pub use self::readme::ReadmeParser;
pub use self::requirements_txt::RequirementsTxtParser;
pub use self::rpm_db::{RpmBdbDatabaseParser, RpmNdbDatabaseParser, RpmSqliteDatabaseParser};
pub use self::rpm_license_files::RpmLicenseFilesParser;
pub use self::rpm_mariner_manifest::RpmMarinerManifestParser;
pub use self::rpm_parser::RpmParser;
pub use self::rpm_specfile::RpmSpecfileParser;
pub use self::rpm_yumdb::RpmYumdbParser;
pub use self::ruby::{
    GemArchiveParser, GemMetadataExtractedParser, GemfileLockParser, GemfileParser, GemspecParser,
};
pub use self::sbt::SbtParser;
pub use self::swift_manifest_json::SwiftManifestJsonParser;
pub use self::swift_resolved::SwiftPackageResolvedParser;
pub use self::swift_show_dependencies::SwiftShowDependenciesParser;
pub use self::uv_lock::UvLockParser;
pub use self::vcpkg::VcpkgManifestParser;
pub use self::yarn_lock::YarnLockParser;
pub use self::yarn_pnp::YarnPnpParser;

/// Registers all parsers and recognizers, generating dispatch functions.
///
/// Parsers are tried first, then recognizers. This ordering is important because
/// recognizers match broadly by file extension (e.g., `.jar`) and would shadow
/// more specific parsers if checked first.
macro_rules! register_package_handlers {
    (
        parsers: [$($parser:ty),* $(,)?],
        recognizers: [$($recognizer:ty),* $(,)?] $(,)?
    ) => {
        pub fn try_parse_file(path: &Path) -> Option<ParsePackagesResult> {
            $(
                if <$parser>::is_match(path) {
                    return Some(capture_parser_diagnostics(
                        || <$parser>::extract_packages(path),
                        stringify!($parser),
                        path,
                    ));
                }
            )*
            $(
                if <$recognizer>::is_match(path) {
                    return Some(capture_parser_diagnostics(
                        || <$recognizer>::extract_packages(path),
                        stringify!($recognizer),
                        path,
                    ));
                }
            )*
            None
        }

        // Used by the parser-golden maintenance tool in `xtask`.
        // Scanner runtime dispatch goes through `try_parse_file()`.
        #[allow(dead_code)]
        pub fn parse_by_type_name(type_name: &str, path: &Path) -> Option<PackageData> {
            match type_name {
                $(
                    stringify!($parser) => Some(<$parser>::extract_first_package(path)),
                )*
                $(
                    stringify!($recognizer) => Some(<$recognizer>::extract_first_package(path)),
                )*
                _ => None
            }
        }

        // Used by the parser-golden maintenance tool in `xtask` and by
        // `tests/scanner_integration.rs` to verify parser registration.
        #[allow(dead_code)]
        pub fn list_parser_types() -> Vec<&'static str> {
            vec![
                $(
                    stringify!($parser),
                )*
                $(
                    stringify!($recognizer),
                )*
            ]
        }
    };
}

register_package_handlers! {
    parsers: [
        AboutFileParser,
        AlpineApkParser,
        AlpineApkbuildParser,
        AlpineInstalledParser,
        ArchPkginfoParser,
        ArchSrcinfoParser,
        AutotoolsConfigureParser,
        BazelBuildParser,
        BazelModuleParser,
        BowerJsonParser,
        BunLockParser,
        BunLockbParser,
        BuckBuildParser,
        BuckMetadataBzlParser,
        CargoLockParser,
        CargoParser,
        ChefMetadataJsonParser,
        ChefMetadataRbParser,
        CitationCffParser,
        ClojureDepsEdnParser,
        ClojureProjectCljParser,
        ComposerJsonParser,
        ComposerLockParser,
        ConanDataParser,
        ConanFilePyParser,
        ConanfileTxtParser,
        ConanLockParser,
        CondaEnvironmentYmlParser,
        CondaMetaJsonParser,
        CondaMetaYamlParser,
        CpanDistIniParser,
        CpanMakefilePlParser,
        CpanManifestParser,
        CpanMetaJsonParser,
        CpanMetaYmlParser,
        CranParser,
        DebianControlInExtractedDebParser,
        DebianControlParser,
        DebianCopyrightParser,
        DebianDebianTarParser,
        DebianDebParser,
        DebianDistrolessInstalledParser,
        DebianDscParser,
        DebianInstalledListParser,
        DebianInstalledMd5sumsParser,
        DebianInstalledParser,
        DebianMd5sumInPackageParser,
        DebianOrigTarParser,
        DenoParser,
        DenoLockParser,
        DockerfileParser,
        FreebsdCompactManifestParser,
        GemArchiveParser,
        GemfileLockParser,
        GemfileParser,
        GemMetadataExtractedParser,
        GemspecParser,
        GitmodulesParser,
        GodepsParser,
        GoModParser,
        GoModGraphParser,
        GoSumParser,
        GoWorkParser,
        GradleLockfileParser,
        GradleParser,
        GradleModuleParser,
        HackageCabalParser,
        HackageCabalProjectParser,
        HackageStackYamlParser,
        HelmChartYamlParser,
        HelmChartLockParser,
        HaxeParser,
        HexLockParser,
        MavenParser,
        MesonParser,
        MicrosoftUpdateManifestParser,
        NixDefaultParser,
        NixFlakeLockParser,
        NixFlakeParser,
        NpmLockParser,
        NpmParser,
        NpmWorkspaceParser,
        DotNetDepsJsonParser,
        CentralPackageManagementPropsParser,
        DirectoryBuildPropsParser,
        NupkgParser,
        NuspecParser,
        PackageReferenceProjectParser,
        OpamParser,
        OsReleaseParser,
        PackagesConfigParser,
        PackagesLockParser,
        ProjectJsonParser,
        ProjectLockJsonParser,
        PipfileLockParser,
        PipInspectDeplockParser,
        PixiTomlParser,
        PixiLockParser,
        PnpmLockParser,
        PodfileLockParser,
        PodfileParser,
        PodspecJsonParser,
        PodspecParser,
        PoetryLockParser,
        PubliccodeParser,
        PylockTomlParser,
        PubspecLockParser,
        PubspecYamlParser,
        PythonParser,
        UvLockParser,
        VcpkgManifestParser,
        ReadmeParser,
        RequirementsTxtParser,
        RpmBdbDatabaseParser,
        RpmLicenseFilesParser,
        RpmMarinerManifestParser,
        RpmNdbDatabaseParser,
        RpmParser,
        RpmSpecfileParser,
        RpmSqliteDatabaseParser,
        RpmYumdbParser,
        SbtParser,
        SwiftManifestJsonParser,
        SwiftPackageResolvedParser,
        SwiftShowDependenciesParser,
        YarnLockParser,
        YarnPnpParser,
    ],
    recognizers: [
        AndroidApkRecognizer,
        AndroidLibraryRecognizer,
        AppleDmgRecognizer,
        Axis2MarRecognizer,
        Axis2ModuleXmlRecognizer,
        CabArchiveRecognizer,
        ChromeCrxRecognizer,
        InstallShieldRecognizer,
        IosIpaRecognizer,
        IsoImageRecognizer,
        IvyXmlRecognizer,
        JavaEarAppXmlRecognizer,
        JavaEarRecognizer,
        JavaJarRecognizer,
        JavaWarRecognizer,
        JavaWarWebXmlRecognizer,
        JBossSarRecognizer,
        JBossServiceXmlRecognizer,
        MeteorPackageRecognizer,
        MozillaXpiRecognizer,
        NsisRecognizer,
        SharArchiveRecognizer,
        SquashfsRecognizer,
    ],
}

#[cfg(test)]
mod panic_isolation_tests {
    use super::*;

    #[test]
    fn capture_parser_diagnostics_turns_panics_into_scan_errors() {
        let path = Path::new("fixtures/panic-package.json");
        let result = capture_parser_diagnostics(
            || -> Vec<PackageData> { panic!("panic boom") },
            "PanicParser",
            path,
        );

        assert!(result.packages.is_empty());
        assert_eq!(result.scan_errors.len(), 1);
        assert!(result.scan_errors[0].contains("PanicParser"));
        assert!(result.scan_errors[0].contains("fixtures/panic-package.json"));
        assert!(result.scan_errors[0].contains("panic boom"));
    }

    #[test]
    fn capture_parser_diagnostics_recovers_after_panic() {
        let panic_path = Path::new("fixtures/panic-package.json");
        let _ = capture_parser_diagnostics(
            || -> Vec<PackageData> { panic!("panic boom") },
            "PanicParser",
            panic_path,
        );

        let ok_path = Path::new("fixtures/recovered-package.json");
        let result = capture_parser_diagnostics(
            || {
                crate::parser_warn!("recoverable parser warning");
                vec![PackageData {
                    package_type: Some(PackageType::Npm),
                    ..Default::default()
                }]
            },
            "RecoveringParser",
            ok_path,
        );

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.scan_errors, vec!["recoverable parser warning"]);
    }
}
