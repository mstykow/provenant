use std::path::Path;

use crate::models::{DatasourceId, FileReference, Md5Digest, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::utils::build_debian_purl;
use super::{IGNORED_ROOT_DIRS, PACKAGE_TYPE, default_package_data};
use crate::parsers::PackageParser;

/// Parser for Debian installed file lists (*.list)
pub struct DebianInstalledListParser;

impl PackageParser for DebianInstalledListParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("list")
            && path
                .to_str()
                .map(|p| p.contains("/var/lib/dpkg/info/"))
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_stem().and_then(|s| s.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(DatasourceId::DebianInstalledFilesList)];
            }
        };

        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .list file {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianInstalledFilesList)];
            }
        };

        vec![parse_debian_file_list(
            &content,
            filename,
            DatasourceId::DebianInstalledFilesList,
        )]
    }
}

crate::register_parser!(
    "Debian installed files list",
    &["**/var/lib/dpkg/info/*.list"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-files.html"),
);

/// Parser for Debian installed MD5 checksum files (*.md5sums)
pub struct DebianInstalledMd5sumsParser;

impl PackageParser for DebianInstalledMd5sumsParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("md5sums")
            && path
                .to_str()
                .map(|p| p.contains("/var/lib/dpkg/info/"))
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let filename = match path.file_stem().and_then(|s| s.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(DatasourceId::DebianInstalledMd5Sums)];
            }
        };

        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .md5sums file {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::DebianInstalledMd5Sums)];
            }
        };

        vec![parse_debian_file_list(
            &content,
            filename,
            DatasourceId::DebianInstalledMd5Sums,
        )]
    }
}

crate::register_parser!(
    "Debian installed package md5sums",
    &["**/var/lib/dpkg/info/*.md5sums"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-files.html"),
);

fn parse_debian_file_list(
    content: &str,
    filename: &str,
    datasource_id: DatasourceId,
) -> PackageData {
    let (name, arch_qualifier) = if let Some((pkg, arch)) = filename.split_once(':') {
        (
            Some(truncate_field(pkg.to_string())),
            Some(arch.to_string()),
        )
    } else if filename == "md5sums" {
        (None, None)
    } else {
        (Some(truncate_field(filename.to_string())), None)
    };

    let mut file_references = Vec::new();
    let mut count = 0usize;

    for line in content.lines() {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!("parse_debian_file_list: exceeded MAX_ITERATION_COUNT lines, stopping");
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (md5sum, path) = if let Some((hash, p)) = line.split_once(' ') {
            (Md5Digest::from_hex(hash.trim()).ok(), p.trim())
        } else {
            (None, line)
        };

        if IGNORED_ROOT_DIRS.contains(&path) {
            continue;
        }

        file_references.push(FileReference {
            path: path.to_string(),
            size: None,
            sha1: None,
            md5: md5sum,
            sha256: None,
            sha512: None,
            extra_data: None,
        });
    }

    if file_references.is_empty() {
        return default_package_data(datasource_id);
    }

    let namespace = Some("debian".to_string());
    let mut package = PackageData {
        datasource_id: Some(datasource_id),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: name.clone(),
        file_references,
        ..Default::default()
    };

    if let Some(n) = &name {
        package.purl = build_debian_purl(n, None, namespace.as_deref(), arch_qualifier.as_deref());
    }

    package
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use std::path::PathBuf;

    #[test]
    fn test_list_parser_is_match() {
        assert!(DebianInstalledListParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.list"
        )));
        assert!(DebianInstalledListParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/package:amd64.list"
        )));
        assert!(!DebianInstalledListParser::is_match(&PathBuf::from(
            "bash.list"
        )));
        assert!(!DebianInstalledListParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.md5sums"
        )));
    }

    #[test]
    fn test_md5sums_parser_is_match() {
        assert!(DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.md5sums"
        )));
        assert!(DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/package:amd64.md5sums"
        )));
        assert!(!DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "bash.md5sums"
        )));
        assert!(!DebianInstalledMd5sumsParser::is_match(&PathBuf::from(
            "/var/lib/dpkg/info/bash.list"
        )));
    }

    #[test]
    fn test_parse_debian_file_list_plain_list() {
        let content = "/.
/bin
/bin/bash
/usr/bin/bashbug
/usr/share/doc/bash/README
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.name, Some("bash".to_string()));
        assert_eq!(pkg.file_references.len(), 3);
        assert_eq!(pkg.file_references[0].path, "/bin/bash");
        assert_eq!(pkg.file_references[0].md5, None);
        assert_eq!(pkg.file_references[1].path, "/usr/bin/bashbug");
        assert_eq!(pkg.file_references[2].path, "/usr/share/doc/bash/README");
    }

    #[test]
    fn test_parse_debian_file_list_md5sums() {
        let content = "77506afebd3b7e19e937a678a185b62e  bin/bash
1c77d2031971b4e4c512ac952102cd85  usr/bin/bashbug
f55e3a16959b0bb8915cb5f219521c80  usr/share/doc/bash/COMPAT.gz
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.name, Some("bash".to_string()));
        assert_eq!(pkg.file_references.len(), 3);
        assert_eq!(pkg.file_references[0].path, "bin/bash");
        assert_eq!(
            pkg.file_references[0].md5,
            Some(Md5Digest::from_hex("77506afebd3b7e19e937a678a185b62e").unwrap())
        );
        assert_eq!(pkg.file_references[1].path, "usr/bin/bashbug");
        assert_eq!(
            pkg.file_references[1].md5,
            Some(Md5Digest::from_hex("1c77d2031971b4e4c512ac952102cd85").unwrap())
        );
    }

    #[test]
    fn test_parse_debian_file_list_with_arch() {
        let content = "/usr/bin/foo
/usr/lib/x86_64-linux-gnu/libfoo.so
";
        let pkg = parse_debian_file_list(
            content,
            "libfoo:amd64",
            DatasourceId::DebianInstalledFilesList,
        );
        assert_eq!(pkg.name, Some("libfoo".to_string()));
        assert!(pkg.purl.is_some());
        assert!(pkg.purl.as_ref().unwrap().contains("arch=amd64"));
        assert_eq!(pkg.file_references.len(), 2);
    }

    #[test]
    fn test_parse_debian_file_list_skips_comments_and_empty() {
        let content = "# This is a comment
/bin/bash

/usr/bin/bashbug
  
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.file_references.len(), 2);
    }

    #[test]
    fn test_parse_debian_file_list_md5sums_only() {
        let content = "abc123  usr/bin/tool
";
        let pkg =
            parse_debian_file_list(content, "md5sums", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.name, None);
        assert_eq!(pkg.file_references.len(), 1);
    }

    #[test]
    fn test_parse_debian_file_list_ignores_root_dirs() {
        let content = "/.
/bin
/bin/bash
/etc
/usr
/var
";
        let pkg = parse_debian_file_list(content, "bash", DatasourceId::DebianInstalledFilesList);
        assert_eq!(pkg.file_references.len(), 1);
        assert_eq!(pkg.file_references[0].path, "/bin/bash");
    }
}
