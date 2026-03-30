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
//! - Direct parsing of RPM database files (not via rpm CLI)
//! - Database location detection (/var/lib/rpm/Packages or variants)
//! - Graceful error handling for unreadable or corrupted databases
//! - Returns package data for each installed package entry
//! - Currently limited to Unix targets because the underlying `rpmdb` dependency
//!   uses Unix-specific file-locking support in its NDB reader
//!
//! This restriction applies to installed RPM database parsing only. Standalone
//! `.rpm` archives are parsed separately by `rpm_parser.rs` and do not share this
//! Unix-only limitation.

use std::path::Path;

use crate::parser_warn as warn;

use crate::models::{DatasourceId, PackageData, PackageType};

#[cfg(unix)]
use crate::models::{Dependency, FileReference};

use super::PackageParser;

#[cfg(unix)]
use super::rpm_parser::infer_rpm_namespace;

const PACKAGE_TYPE: PackageType = PackageType::Rpm;

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
        if cfg!(target_os = "windows") {
            return false;
        }

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
        if cfg!(target_os = "windows") {
            return false;
        }

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
        if cfg!(target_os = "windows") {
            return false;
        }

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
    #[cfg(unix)]
    match rpmdb::read_packages(path.to_path_buf()) {
        Ok(packages) => Ok(packages
            .into_iter()
            .map(|pkg| {
                let name = if pkg.name.is_empty() {
                    None
                } else {
                    Some(pkg.name.clone())
                };

                let version = build_evr_version(pkg.epoch, &pkg.version, &pkg.release);

                let namespace = infer_rpm_namespace(
                    None,
                    (!pkg.vendor.is_empty()).then_some(pkg.vendor.as_str()),
                    Some(pkg.release.as_str()),
                    None,
                );

                let architecture = if pkg.arch.is_empty() {
                    None
                } else {
                    Some(pkg.arch.clone())
                };

                let dependencies = pkg
                    .requires
                    .iter()
                    .filter(|r| {
                        !r.is_empty() && !r.starts_with("rpmlib(") && !r.starts_with("config(")
                    })
                    .map(|require| {
                        use packageurl::PackageUrl;
                        let purl = PackageUrl::new(PACKAGE_TYPE.as_str(), require)
                            .ok()
                            .map(|p| p.to_string());

                        Dependency {
                            purl,
                            extracted_requirement: None,
                            scope: Some("requires".to_string()),
                            is_runtime: Some(true),
                            is_optional: Some(false),
                            is_pinned: Some(false),
                            is_direct: Some(true),
                            resolved_package: None,
                            extra_data: None,
                        }
                    })
                    .collect();

                let extracted_license_statement = if pkg.license.is_empty() {
                    None
                } else {
                    Some(pkg.license)
                };

                let purl = name.as_ref().and_then(|n| {
                    use packageurl::PackageUrl;
                    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), n).ok()?;

                    if let Some(ns) = &namespace {
                        purl.with_namespace(ns).ok()?;
                    }

                    if let Some(ver) = &version {
                        purl.with_version(ver).ok()?;
                    }

                    if let Some(arch) = &architecture {
                        purl.add_qualifier("arch", arch).ok()?;
                    }

                    Some(purl.to_string())
                });

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
                    size: if pkg.size > 0 {
                        Some(pkg.size as u64)
                    } else {
                        None
                    },
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
                    source_packages: if pkg.source_rpm.is_empty() {
                        Vec::new()
                    } else {
                        vec![pkg.source_rpm]
                    },
                    file_references: build_file_references(
                        &pkg.base_names,
                        &pkg.dir_indexes,
                        &pkg.dir_names,
                    ),
                    is_private: false,
                    is_virtual: false,
                    extra_data: None,
                    dependencies,
                    repository_homepage_url: None,
                    repository_download_url: None,
                    api_data_url: None,
                    purl,
                }
            })
            .collect()),
        Err(e) => Err(format!("Failed to read RPM database: {:?}", e)),
    }

    #[cfg(not(unix))]
    {
        let _ = (path, datasource_id);
        Err(format!(
            "RPM database parsing is only supported on Unix targets because the current rpmdb dependency uses Unix-specific locking in its NDB backend (current target: {})",
            std::env::consts::OS
        ))
    }
}

#[cfg(unix)]
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

#[cfg(unix)]
fn build_file_references(
    base_names: &[String],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn test_build_evr_version_full() {
        assert_eq!(
            build_evr_version(2, "1.0.0", "1.el7"),
            Some("2:1.0.0-1.el7".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_build_evr_version_no_epoch() {
        assert_eq!(
            build_evr_version(0, "1.0.0", "1.el7"),
            Some("1.0.0-1.el7".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_build_evr_version_no_release() {
        assert_eq!(build_evr_version(0, "1.0.0", ""), Some("1.0.0".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn test_build_evr_version_empty() {
        assert_eq!(build_evr_version(0, "", ""), None);
    }

    #[cfg(unix)]
    #[test]
    fn test_parse_rpm_database_sqlite() {
        let test_file = PathBuf::from("testdata/rpm/rpmdb.sqlite");

        let pkg = RpmSqliteDatabaseParser::extract_first_package(&test_file);

        assert_eq!(pkg.package_type, Some(PackageType::Rpm));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::RpmInstalledDatabaseSqlite)
        );
        assert!(pkg.name.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn test_parse_rpm_database_sqlite_preserves_release_in_version() {
        let test_file = PathBuf::from("testdata/rpm/rpmdb.sqlite");

        let pkg = RpmSqliteDatabaseParser::extract_first_package(&test_file);

        assert!(
            pkg.version
                .as_ref()
                .is_some_and(|version| version.contains('-'))
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_build_file_references_skips_invalid_entries() {
        let file_refs = build_file_references(
            &["valid".to_string(), "".to_string(), "ignored".to_string()],
            &[0, 0, -1],
            &["/usr/bin/".to_string()],
        );

        assert_eq!(file_refs.len(), 2);
        assert_eq!(file_refs[0].path, "/usr/bin/valid");
        assert_eq!(file_refs[1].path, "/usr/bin/");
    }
}

#[cfg(unix)]
crate::register_parser!(
    "RPM installed package database",
    &[
        "**/var/lib/rpm/Packages",
        "**/var/lib/rpm/Packages.db",
        "**/var/lib/rpm/rpmdb.sqlite"
    ],
    "rpm",
    "",
    Some("https://rpm.org/"),
);
