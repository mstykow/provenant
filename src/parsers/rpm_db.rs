//! Parser for RPM database files.
//!
//! Extracts installed package metadata from the RPM database maintained by the
//! system package manager, typically located in /var/lib/rpm/.
//!
//! # Supported Formats
//! - /var/lib/rpm/Packages (BerkleyDB format or SQLite - raw database file)
//! - Other RPM database index files
//!
//! # Key Features
//! - Installed package metadata extraction from system RPM database
//! - Database format detection (BDB vs SQLite)
//! - Multi-version package support
//! - Package URL (purl) generation with architecture namespace
//!
//! # Implementation Notes
//! - Database location detection (/var/lib/rpm/Packages or variants)
//! - Graceful error handling for unreadable or corrupted databases
//! - Returns package data for each installed package entry

use std::path::Path;
use std::process::Command;

use crate::parser_warn as warn;
use serde::Deserialize;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::models::{Dependency, FileReference};

use super::PackageParser;
use super::rpm_parser::infer_rpm_namespace;
use super::rpm_parser::infer_rpm_namespace_from_filename;

const PACKAGE_TYPE: PackageType = PackageType::Rpm;

#[derive(Debug, Deserialize)]
struct RpmQueryPackage {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Epoch")]
    epoch: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "Release")]
    release: Option<String>,
    #[serde(rename = "Vendor")]
    vendor: Option<String>,
    #[serde(rename = "Distribution")]
    distribution: Option<String>,
    #[serde(rename = "Arch")]
    arch: Option<String>,
    #[serde(rename = "Platform")]
    platform: Option<String>,
    #[serde(rename = "Size")]
    size: Option<u64>,
    #[serde(rename = "License")]
    license: Option<String>,
    #[serde(rename = "Sourcerpm")]
    source_rpm: Option<String>,
    #[serde(default, rename = "Requirename")]
    requires: Vec<String>,
    #[serde(default, rename = "Filenames")]
    file_names: Vec<Option<String>>,
    #[serde(default, rename = "Dirindexes")]
    dir_indexes: Vec<i32>,
    #[serde(default, rename = "Basenames")]
    base_names: Vec<Option<String>>,
    #[serde(default, rename = "Dirnames")]
    dir_names: Vec<String>,
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

pub struct RpmBdbDatabaseParser;

impl PackageParser for RpmBdbDatabaseParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        (path_str.ends_with("/Packages") || path_str.contains("/var/lib/rpm/Packages"))
            && !path_str.ends_with(".db")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match parse_rpm_database(path, DatasourceId::RpmInstalledDatabaseBdb) {
            Ok(pkgs) if !pkgs.is_empty() => pkgs,
            Ok(_) => vec![default_package_data(DatasourceId::RpmInstalledDatabaseBdb)],
            Err(e) => {
                warn!("Failed to parse RPM BDB database {:?}: {}", path, e);
                vec![default_package_data(DatasourceId::RpmInstalledDatabaseBdb)]
            }
        }
    }
}

pub struct RpmNdbDatabaseParser;

impl PackageParser for RpmNdbDatabaseParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.ends_with("/Packages.db") || path_str.contains("usr/lib/sysimage/rpm/Packages.db")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match parse_rpm_database(path, DatasourceId::RpmInstalledDatabaseNdb) {
            Ok(pkgs) if !pkgs.is_empty() => pkgs,
            Ok(_) => vec![default_package_data(DatasourceId::RpmInstalledDatabaseNdb)],
            Err(e) => {
                warn!("Failed to parse RPM NDB database {:?}: {}", path, e);
                vec![default_package_data(DatasourceId::RpmInstalledDatabaseNdb)]
            }
        }
    }
}

pub struct RpmSqliteDatabaseParser;

impl PackageParser for RpmSqliteDatabaseParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.ends_with("/rpmdb.sqlite") || path_str.contains("rpm/rpmdb.sqlite")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match parse_rpm_database(path, DatasourceId::RpmInstalledDatabaseSqlite) {
            Ok(pkgs) if !pkgs.is_empty() => pkgs,
            Ok(_) => vec![default_package_data(
                DatasourceId::RpmInstalledDatabaseSqlite,
            )],
            Err(e) => {
                warn!("Failed to parse RPM SQLite database {:?}: {}", path, e);
                vec![default_package_data(
                    DatasourceId::RpmInstalledDatabaseSqlite,
                )]
            }
        }
    }
}

fn parse_rpm_database(
    path: &Path,
    datasource_id: DatasourceId,
) -> Result<Vec<PackageData>, String> {
    let rpmdb_dir = path
        .parent()
        .ok_or_else(|| format!("RPM database path {:?} has no parent directory", path))?;

    query_rpm_database(rpmdb_dir).map(|packages| {
        packages
            .into_iter()
            .map(|pkg| build_package_data(pkg, datasource_id))
            .collect()
    })
}

fn build_evr_version(epoch: i32, version: &str, release: &str) -> Option<String> {
    if version.is_empty() {
        return None;
    }

    let mut evr = String::new();

    if epoch > 0 {
        evr.push_str(&format!("{}:", epoch));
    }

    evr.push_str(version);

    if !release.is_empty() {
        evr.push('-');
        evr.push_str(release);
    }

    Some(evr)
}

fn build_file_references(
    base_names: &[Option<String>],
    dir_indexes: &[i32],
    dir_names: &[String],
) -> Vec<FileReference> {
    if base_names.is_empty() || dir_names.is_empty() {
        return Vec::new();
    }

    base_names
        .iter()
        .zip(dir_indexes.iter())
        .filter_map(|(basename, &dir_idx)| {
            let dirname = dir_names.get(dir_idx as usize)?;
            let basename = basename.as_deref().unwrap_or_default();
            let path = format!("{}{}", dirname, basename);
            if path.is_empty() || path == "/" {
                return None;
            }
            Some(FileReference {
                path,
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                extra_data: None,
            })
        })
        .collect()
}

fn build_file_references_from_paths(paths: &[Option<String>]) -> Vec<FileReference> {
    paths
        .iter()
        .filter_map(|path| {
            let path = path.as_deref()?.trim();
            if path.is_empty() || path == "/" {
                return None;
            }

            Some(FileReference {
                path: path.to_string(),
                size: None,
                sha1: None,
                md5: None,
                sha256: None,
                sha512: None,
                extra_data: None,
            })
        })
        .collect()
}

fn query_rpm_database(rpmdb_dir: &Path) -> Result<Vec<RpmQueryPackage>, String> {
    let output = Command::new("rpm")
        .args(["--dbpath"])
        .arg(rpmdb_dir)
        .args(["--query", "--all", "--json"])
        .output()
        .map_err(|e| format!("Failed to execute rpm for {:?}: {}", rpmdb_dir, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        return Err(format!(
            "rpm query failed for {:?} (status: {}): {}",
            rpmdb_dir, output.status, details
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| format!("rpm output for {:?} was not valid UTF-8: {}", rpmdb_dir, e))?;

    parse_rpm_query_output(&stdout)
}

fn parse_rpm_query_output(stdout: &str) -> Result<Vec<RpmQueryPackage>, String> {
    serde_json::Deserializer::from_str(stdout)
        .into_iter::<RpmQueryPackage>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to parse rpm JSON output: {}", e))
}

fn build_package_data(pkg: RpmQueryPackage, datasource_id: DatasourceId) -> PackageData {
    let name = normalize_optional_string(pkg.name);
    let version_raw = normalize_optional_string(pkg.version);
    let release = normalize_optional_string(pkg.release);
    let version = build_evr_version(
        parse_epoch(pkg.epoch),
        version_raw.as_deref().unwrap_or_default(),
        release.as_deref().unwrap_or_default(),
    );

    let vendor = normalize_optional_string(pkg.vendor)
        .or_else(|| normalize_optional_string(pkg.distribution));
    let source_rpm = normalize_optional_string(pkg.source_rpm);
    let namespace =
        infer_rpm_namespace(None, vendor.as_deref(), release.as_deref(), None).or_else(|| {
            source_rpm
                .as_deref()
                .and_then(|source_rpm| infer_rpm_namespace_from_filename(Path::new(source_rpm)))
        });

    let architecture = normalize_optional_string(pkg.arch)
        .or_else(|| infer_platform_architecture(pkg.platform.as_deref()));
    let dependencies = pkg
        .requires
        .into_iter()
        .filter_map(|require| build_dependency(&require))
        .collect();
    let extracted_license_statement = normalize_optional_string(pkg.license);
    let source_packages = source_rpm.clone().into_iter().collect();
    let file_references = {
        let from_dir_components =
            build_file_references(&pkg.base_names, &pkg.dir_indexes, &pkg.dir_names);
        if from_dir_components.is_empty() {
            build_file_references_from_paths(&pkg.file_names)
        } else {
            from_dir_components
        }
    };
    let purl = build_package_purl(
        name.as_deref(),
        namespace.as_deref(),
        version.as_deref(),
        architecture.as_deref(),
    );

    PackageData {
        datasource_id: Some(datasource_id),
        package_type: Some(PACKAGE_TYPE),
        namespace,
        name,
        version,
        qualifiers: architecture.as_ref().map(|arch| {
            let mut q = std::collections::HashMap::new();
            q.insert("arch".to_string(), arch.clone());
            q
        }),
        subpath: None,
        primary_language: None,
        description: None,
        release_date: None,
        parties: Vec::new(),
        keywords: Vec::new(),
        homepage_url: None,
        download_url: None,
        size: pkg.size.filter(|size| *size > 0),
        sha1: None,
        md5: None,
        sha256: None,
        sha512: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        copyright: None,
        holder: None,
        declared_license_expression: None,
        declared_license_expression_spdx: None,
        license_detections: Vec::new(),
        other_license_expression: None,
        other_license_expression_spdx: None,
        other_license_detections: Vec::new(),
        extracted_license_statement,
        notice_text: None,
        source_packages,
        file_references,
        is_private: false,
        is_virtual: false,
        extra_data: None,
        dependencies,
        repository_homepage_url: None,
        repository_download_url: None,
        api_data_url: None,
        purl,
    }
}

fn build_dependency(require: &str) -> Option<Dependency> {
    let require = require.trim();
    if require.is_empty() || require.starts_with("rpmlib(") || require.starts_with("config(") {
        return None;
    }

    let purl = packageurl::PackageUrl::new(PACKAGE_TYPE.as_str(), require)
        .ok()
        .map(|p| p.to_string());

    Some(Dependency {
        purl,
        extracted_requirement: None,
        scope: Some("requires".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn build_package_purl(
    name: Option<&str>,
    namespace: Option<&str>,
    version: Option<&str>,
    arch: Option<&str>,
) -> Option<String> {
    let name = name?;
    let mut purl = packageurl::PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(namespace) = namespace {
        purl.with_namespace(namespace).ok()?;
    }

    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }

    if let Some(arch) = arch {
        purl.add_qualifier("arch", arch).ok()?;
    }

    Some(purl.to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "(none)" || trimmed == "[]" {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_epoch(value: Option<String>) -> i32 {
    normalize_optional_string(value)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

fn infer_platform_architecture(platform: Option<&str>) -> Option<String> {
    let platform = platform?.trim();
    if platform.is_empty() {
        return None;
    }

    platform
        .split_once('-')
        .map(|(arch, _)| arch)
        .filter(|arch| !arch.is_empty())
        .map(|arch| arch.to_string())
}

#[cfg(test)]
fn rpm_command_available() -> bool {
    Command::new("rpm").arg("--version").output().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::models::DatasourceId;
    use std::path::PathBuf;

    #[test]
    fn test_bdb_parser_is_match() {
        assert!(RpmBdbDatabaseParser::is_match(&PathBuf::from(
            "/var/lib/rpm/Packages"
        )));
        assert!(RpmBdbDatabaseParser::is_match(&PathBuf::from(
            "rootfs/var/lib/rpm/Packages"
        )));
        assert!(!RpmBdbDatabaseParser::is_match(&PathBuf::from(
            "/var/lib/rpm/Packages.db"
        )));
    }

    #[test]
    fn test_ndb_parser_is_match() {
        assert!(RpmNdbDatabaseParser::is_match(&PathBuf::from(
            "usr/lib/sysimage/rpm/Packages.db"
        )));
        assert!(RpmNdbDatabaseParser::is_match(&PathBuf::from(
            "/rootfs/usr/lib/sysimage/rpm/Packages.db"
        )));
        assert!(!RpmNdbDatabaseParser::is_match(&PathBuf::from(
            "usr/lib/rpm/Packages"
        )));
    }

    #[test]
    fn test_sqlite_parser_is_match() {
        assert!(RpmSqliteDatabaseParser::is_match(&PathBuf::from(
            "var/lib/rpm/rpmdb.sqlite"
        )));
        assert!(RpmSqliteDatabaseParser::is_match(&PathBuf::from(
            "/rootfs/var/lib/rpm/rpmdb.sqlite"
        )));
        assert!(!RpmSqliteDatabaseParser::is_match(&PathBuf::from(
            "/var/lib/rpm/Packages"
        )));
    }

    #[test]
    fn test_build_evr_version_full() {
        assert_eq!(
            build_evr_version(2, "1.0.0", "1.el7"),
            Some("2:1.0.0-1.el7".to_string())
        );
    }

    #[test]
    fn test_build_evr_version_no_epoch() {
        assert_eq!(
            build_evr_version(0, "1.0.0", "1.el7"),
            Some("1.0.0-1.el7".to_string())
        );
    }

    #[test]
    fn test_build_evr_version_no_release() {
        assert_eq!(build_evr_version(0, "1.0.0", ""), Some("1.0.0".to_string()));
    }

    #[test]
    fn test_build_evr_version_empty() {
        assert_eq!(build_evr_version(0, "", ""), None);
    }

    #[test]
    fn test_parse_rpm_database_sqlite() {
        if !rpm_command_available() {
            return;
        }

        let test_file = PathBuf::from("testdata/rpm/rpmdb.sqlite");

        let pkg = RpmSqliteDatabaseParser::extract_first_package(&test_file);

        assert_eq!(pkg.package_type, Some(PackageType::Rpm));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::RpmInstalledDatabaseSqlite)
        );
        assert!(pkg.name.is_some());
    }

    #[test]
    fn test_parse_rpm_database_sqlite_preserves_release_in_version() {
        if !rpm_command_available() {
            return;
        }

        let test_file = PathBuf::from("testdata/rpm/rpmdb.sqlite");

        let pkg = RpmSqliteDatabaseParser::extract_first_package(&test_file);

        assert!(
            pkg.version
                .as_ref()
                .is_some_and(|version| version.contains('-'))
        );
    }

    #[test]
    fn test_build_file_references_skips_invalid_entries() {
        let file_refs = build_file_references(
            &[
                Some("valid".to_string()),
                Some("".to_string()),
                Some("ignored".to_string()),
            ],
            &[0, 0, -1],
            &["/usr/bin/".to_string()],
        );

        assert_eq!(file_refs.len(), 2);
        assert_eq!(file_refs[0].path, "/usr/bin/valid");
        assert_eq!(file_refs[1].path, "/usr/bin/");
    }

    #[test]
    fn test_build_package_data_falls_back_to_file_names() {
        let package = build_package_data(
            RpmQueryPackage {
                name: Some("libgcc".to_string()),
                epoch: None,
                version: Some("13.1.1".to_string()),
                release: Some("2.fc38".to_string()),
                vendor: Some("Fedora Project".to_string()),
                distribution: None,
                arch: Some("x86_64".to_string()),
                platform: None,
                size: Some(235748),
                license: Some("GPLv3+".to_string()),
                source_rpm: Some("gcc-13.1.1-2.fc38.src.rpm".to_string()),
                requires: Vec::new(),
                file_names: vec![
                    Some("/usr/share/licenses/libgcc/COPYING".to_string()),
                    Some("/usr/share/licenses/libgcc/COPYING.RUNTIME".to_string()),
                ],
                dir_indexes: Vec::new(),
                base_names: Vec::new(),
                dir_names: Vec::new(),
            },
            DatasourceId::RpmInstalledDatabaseSqlite,
        );

        assert_eq!(package.file_references.len(), 2);
        assert_eq!(
            package.file_references[0].path,
            "/usr/share/licenses/libgcc/COPYING"
        );
        assert_eq!(
            package.file_references[1].path,
            "/usr/share/licenses/libgcc/COPYING.RUNTIME"
        );
    }

    #[test]
    fn test_build_package_data_uses_distribution_for_namespace() {
        let package = build_package_data(
            RpmQueryPackage {
                name: Some("libgcc".to_string()),
                epoch: None,
                version: Some("13.1.1".to_string()),
                release: Some("2.fc38".to_string()),
                vendor: None,
                distribution: Some("Fedora Project".to_string()),
                arch: Some("x86_64".to_string()),
                platform: None,
                size: Some(235748),
                license: Some("GPLv3+".to_string()),
                source_rpm: Some("gcc-13.1.1-2.fc38.src.rpm".to_string()),
                requires: Vec::new(),
                file_names: vec![Some("/usr/share/licenses/libgcc/COPYING".to_string())],
                dir_indexes: Vec::new(),
                base_names: Vec::new(),
                dir_names: Vec::new(),
            },
            DatasourceId::RpmInstalledDatabaseSqlite,
        );

        assert_eq!(package.namespace.as_deref(), Some("fedora"));
    }

    #[test]
    fn test_build_package_data_uses_source_rpm_for_namespace() {
        let package = build_package_data(
            RpmQueryPackage {
                name: Some("libgcc".to_string()),
                epoch: None,
                version: Some("13.1.1".to_string()),
                release: None,
                vendor: None,
                distribution: None,
                arch: Some("x86_64".to_string()),
                platform: None,
                size: Some(235748),
                license: Some("GPLv3+".to_string()),
                source_rpm: Some("gcc-13.1.1-2.fc38.src.rpm".to_string()),
                requires: Vec::new(),
                file_names: vec![Some("/usr/share/licenses/libgcc/COPYING".to_string())],
                dir_indexes: Vec::new(),
                base_names: Vec::new(),
                dir_names: Vec::new(),
            },
            DatasourceId::RpmInstalledDatabaseSqlite,
        );

        assert_eq!(package.namespace.as_deref(), Some("fedora"));
    }

    #[test]
    fn test_build_package_data_uses_platform_for_architecture() {
        let package = build_package_data(
            RpmQueryPackage {
                name: Some("libgcc".to_string()),
                epoch: None,
                version: Some("13.1.1".to_string()),
                release: None,
                vendor: None,
                distribution: None,
                arch: None,
                platform: Some("x86_64-redhat-linux".to_string()),
                size: Some(235748),
                license: Some("GPLv3+".to_string()),
                source_rpm: Some("gcc-13.1.1-2.fc38.src.rpm".to_string()),
                requires: Vec::new(),
                file_names: vec![Some("/usr/share/licenses/libgcc/COPYING".to_string())],
                dir_indexes: Vec::new(),
                base_names: Vec::new(),
                dir_names: Vec::new(),
            },
            DatasourceId::RpmInstalledDatabaseSqlite,
        );

        assert_eq!(
            package.qualifiers.as_ref().and_then(|q| q.get("arch")),
            Some(&"x86_64".to_string())
        );
    }

    #[test]
    fn test_parse_rpm_query_output_parses_multiple_json_objects() {
        let stdout = r#"
        {
            "Name": "libgcc",
            "Version": "13.1.1",
            "Release": "2.fc38",
            "Vendor": "Fedora Project",
            "Arch": "x86_64",
            "Size": 235748,
            "License": "GPLv3+",
            "Sourcerpm": "gcc-13.1.1-2.fc38.src.rpm",
            "Requirename": ["rpmlib(PayloadIsZstd)", "glibc"],
            "Filenames": ["/usr/share/licenses/libgcc/COPYING"],
            "Dirindexes": [0],
            "Basenames": ["COPYING"],
            "Dirnames": ["/usr/share/licenses/libgcc/"]
        }
        {
            "Name": "coreutils",
            "Version": "9.1",
            "Release": "12.fc38",
            "Vendor": "Fedora Project",
            "Arch": "x86_64",
            "Requirename": ["glibc"],
            "Filenames": ["/usr/bin/cat"]
        }
        "#;

        let packages = parse_rpm_query_output(stdout).expect("rpm JSON stream should parse");

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name.as_deref(), Some("libgcc"));
        assert_eq!(packages[0].file_names.len(), 1);
        assert_eq!(packages[1].name.as_deref(), Some("coreutils"));
        assert_eq!(packages[1].requires, vec!["glibc".to_string()]);
    }
}

crate::register_parser!(
    "RPM installed package database (requires `rpm` CLI at runtime)",
    &[
        "**/var/lib/rpm/Packages",
        "**/var/lib/rpm/Packages.db",
        "**/var/lib/rpm/rpmdb.sqlite"
    ],
    "rpm",
    "",
    Some("https://rpm.org/"),
);
