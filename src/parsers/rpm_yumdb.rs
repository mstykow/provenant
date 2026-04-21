// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Rpm;

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::RpmYumdb),
        ..Default::default()
    }
}

fn parse_yumdb_dir_name(dir_name: &str) -> Option<(String, String, String)> {
    let (_, package_part) = dir_name.split_once('-')?;
    let (name_version_release, arch) = package_part.rsplit_once('.')?;

    let mut parts = name_version_release.rsplitn(3, '-');
    let release = parts.next()?;
    let version = parts.next()?;
    let name = parts.next()?;

    Some((
        truncate_field(name.to_string()),
        truncate_field(format!("{}-{}", version, release)),
        truncate_field(arch.to_string()),
    ))
}

fn build_yumdb_purl(name: &str, version: &str, arch: &str) -> Option<String> {
    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;
    purl.with_version(version).ok()?;
    purl.add_qualifier("arch", arch).ok()?;
    Some(truncate_field(purl.to_string()))
}

pub struct RpmYumdbParser;

impl PackageParser for RpmYumdbParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("from_repo")
            && path.to_string_lossy().contains("/var/lib/yum/yumdb/")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let Some(package_dir) = path.parent() else {
            return vec![default_package_data()];
        };

        let Some(dir_name) = package_dir.file_name().and_then(|name| name.to_str()) else {
            return vec![default_package_data()];
        };

        let Some((name, version, arch)) = parse_yumdb_dir_name(dir_name) else {
            warn!(
                "Failed to parse yumdb package directory name {:?}",
                package_dir
            );
            return vec![default_package_data()];
        };

        let mut extra_data = std::collections::HashMap::new();
        let entries = match fs::read_dir(package_dir) {
            Ok(entries) => entries,
            Err(e) => {
                warn!(
                    "Failed to read yumdb package directory {:?}: {}",
                    package_dir, e
                );
                return vec![default_package_data()];
            }
        };

        let entries: Vec<_> = entries.flatten().take(MAX_ITERATION_COUNT).collect();

        for entry in entries {
            let key_path = entry.path();
            if !key_path.is_file() {
                continue;
            }

            let Some(key) = key_path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            match read_file_to_string(&key_path, None) {
                Ok(value) => {
                    let value = value.trim();
                    if !value.is_empty() {
                        extra_data.insert(
                            key.to_string(),
                            serde_json::Value::String(truncate_field(value.to_string())),
                        );
                    }
                }
                Err(e) => warn!("Failed to read yumdb key {:?}: {}", key_path, e),
            }
        }

        let qualifiers = std::iter::once(("arch".to_string(), arch.clone())).collect();

        vec![PackageData {
            datasource_id: Some(DatasourceId::RpmYumdb),
            package_type: Some(PACKAGE_TYPE),
            name: Some(name.clone()),
            version: Some(version.clone()),
            qualifiers: Some(qualifiers),
            purl: build_yumdb_purl(&name, &version, &arch),
            extra_data: (!extra_data.is_empty()).then_some(extra_data),
            is_virtual: true,
            ..Default::default()
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_yumdb_dir_name() {
        let parsed = parse_yumdb_dir_name("p/bash-5.0-1.el8.x86_64");
        assert!(parsed.is_none());

        let parsed = parse_yumdb_dir_name("abc123-bash-5.0-1.el8.x86_64").unwrap();
        assert_eq!(parsed.0, "bash");
        assert_eq!(parsed.1, "5.0-1.el8");
        assert_eq!(parsed.2, "x86_64");
    }

    #[test]
    fn test_is_match() {
        assert!(RpmYumdbParser::is_match(Path::new(
            "/rootfs/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64/from_repo"
        )));
        assert!(!RpmYumdbParser::is_match(Path::new(
            "/rootfs/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64/reason"
        )));
    }

    #[test]
    fn test_extract_packages_reads_sibling_metadata() {
        let tempdir = tempdir().unwrap();
        let package_dir = tempdir
            .path()
            .join("rootfs/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64");
        fs::create_dir_all(&package_dir).unwrap();
        fs::write(package_dir.join("from_repo"), "baseos\n").unwrap();
        fs::write(package_dir.join("reason"), "dep\n").unwrap();
        fs::write(package_dir.join("releasever"), "8\n").unwrap();

        let packages = RpmYumdbParser::extract_packages(&package_dir.join("from_repo"));
        let pkg = &packages[0];

        assert_eq!(pkg.datasource_id, Some(DatasourceId::RpmYumdb));
        assert_eq!(pkg.name.as_deref(), Some("bash"));
        assert_eq!(pkg.version.as_deref(), Some("5.0-1.el8"));
        assert_eq!(
            pkg.qualifiers.as_ref().and_then(|q| q.get("arch")),
            Some(&"x86_64".to_string())
        );
        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(extra["from_repo"], "baseos");
        assert_eq!(extra["reason"], "dep");
        assert_eq!(extra["releasever"], "8");
    }
}

crate::register_parser!(
    "RPM yumdb metadata",
    &["**/var/lib/yum/yumdb/*/*/from_repo"],
    "rpm",
    "",
    Some("http://yum.baseurl.org/wiki/YumDB.html"),
);
