use std::collections::HashMap;
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::rfc822;
use crate::parsers::utils::{MAX_ITERATION_COUNT, split_name_email, truncate_field};

use super::utils::{build_debian_purl, make_party, parse_dependency_field};
use super::{PACKAGE_TYPE, default_package_data, read_or_default};
use crate::parsers::PackageParser;

/// Parser for Debian Source Control (.dsc) files
pub struct DebianDscParser;

impl PackageParser for DebianDscParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("dsc")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = read_or_default!(path, ".dsc file", DatasourceId::DebianSourceControlDsc);

        vec![parse_dsc_content(&content)]
    }
}

crate::register_parser!(
    "Debian source control file (.dsc)",
    &["**/*.dsc"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

fn strip_pgp_signature(content: &str) -> String {
    let mut result = String::new();
    let mut in_pgp_block = false;
    let mut in_signature = false;
    let mut count = 0usize;

    for line in content.lines() {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!("strip_pgp_signature: exceeded MAX_ITERATION_COUNT lines, stopping");
            break;
        }
        if line.starts_with("-----BEGIN PGP SIGNED MESSAGE-----") {
            in_pgp_block = true;
            continue;
        }
        if line.starts_with("-----BEGIN PGP SIGNATURE-----") {
            in_signature = true;
            continue;
        }
        if line.starts_with("-----END PGP SIGNATURE-----") {
            in_signature = false;
            continue;
        }
        if in_pgp_block && line.starts_with("Hash:") {
            continue;
        }
        if in_pgp_block && line.is_empty() && result.is_empty() {
            in_pgp_block = false;
            continue;
        }
        if !in_signature {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn parse_dsc_content(content: &str) -> PackageData {
    let clean_content = strip_pgp_signature(content);
    let metadata = rfc822::parse_rfc822_content(&clean_content);
    let headers = &metadata.headers;

    let name = rfc822::get_header_first(headers, "source").map(truncate_field);
    let version = rfc822::get_header_first(headers, "version").map(truncate_field);
    let architecture = rfc822::get_header_first(headers, "architecture").map(truncate_field);
    let namespace = Some("debian".to_string());

    let mut package = PackageData {
        datasource_id: Some(DatasourceId::DebianSourceControlDsc),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        description: rfc822::get_header_first(headers, "description").map(truncate_field),
        homepage_url: rfc822::get_header_first(headers, "homepage").map(truncate_field),
        vcs_url: rfc822::get_header_first(headers, "vcs-git").map(truncate_field),
        code_view_url: rfc822::get_header_first(headers, "vcs-browser").map(truncate_field),
        ..Default::default()
    };

    // Build PURL with architecture qualifier
    if let (Some(n), Some(v)) = (&name, &version) {
        package.purl = build_debian_purl(n, Some(v), namespace.as_deref(), architecture.as_deref());
    }

    // Set source_packages to point to the source itself (without version)
    if let Some(n) = &name
        && let Some(source_purl) = build_debian_purl(n, None, namespace.as_deref(), None)
    {
        package.source_packages.push(source_purl);
    }

    if let Some(maintainer) = rfc822::get_header_first(headers, "maintainer") {
        let (name_opt, email_opt) = split_name_email(&maintainer);
        package
            .parties
            .push(make_party(None, "maintainer", name_opt, email_opt));
    }

    if let Some(uploaders_str) = rfc822::get_header_first(headers, "uploaders") {
        for uploader in uploaders_str.split(',') {
            let uploader = uploader.trim();
            if uploader.is_empty() {
                continue;
            }
            let (name_opt, email_opt) = split_name_email(uploader);
            package
                .parties
                .push(make_party(None, "uploader", name_opt, email_opt));
        }
    }

    // Parse Build-Depends
    if let Some(build_deps) = rfc822::get_header_first(headers, "build-depends") {
        package.dependencies.extend(parse_dependency_field(
            &build_deps,
            "build",
            false,
            false,
            namespace.as_deref(),
        ));
    }

    // Store Standards-Version in extra_data
    if let Some(standards) = rfc822::get_header_first(headers, "standards-version") {
        let map = package.extra_data.get_or_insert_with(HashMap::new);
        map.insert("standards_version".to_string(), standards.into());
    }

    package
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use std::path::PathBuf;

    #[test]
    fn test_dsc_parser_is_match() {
        assert!(DebianDscParser::is_match(&PathBuf::from("package.dsc")));
        assert!(DebianDscParser::is_match(&PathBuf::from(
            "adduser_3.118+deb11u1.dsc"
        )));
        assert!(!DebianDscParser::is_match(&PathBuf::from("control")));
        assert!(!DebianDscParser::is_match(&PathBuf::from("package.txt")));
    }

    #[test]
    fn test_dsc_parser_adduser() {
        let path = PathBuf::from("testdata/debian/dsc_files/adduser_3.118+deb11u1.dsc");
        let package = DebianDscParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PACKAGE_TYPE));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert_eq!(package.name, Some("adduser".to_string()));
        assert_eq!(package.version, Some("3.118+deb11u1".to_string()));
        assert_eq!(
            package.purl,
            Some("pkg:deb/debian/adduser@3.118%2Bdeb11u1?arch=all".to_string())
        );
        assert_eq!(
            package.vcs_url,
            Some("https://salsa.debian.org/debian/adduser.git".to_string())
        );
        assert_eq!(
            package.code_view_url,
            Some("https://salsa.debian.org/debian/adduser".to_string())
        );
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::DebianSourceControlDsc)
        );

        assert_eq!(package.parties.len(), 2);
        assert_eq!(package.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(
            package.parties[0].name,
            Some("Debian Adduser Developers".to_string())
        );
        assert_eq!(
            package.parties[0].email,
            Some("adduser@packages.debian.org".to_string())
        );
        assert_eq!(package.parties[0].r#type, None);

        assert_eq!(package.parties[1].role, Some("uploader".to_string()));
        assert_eq!(package.parties[1].name, Some("Marc Haber".to_string()));
        assert_eq!(
            package.parties[1].email,
            Some("mh+debian-packages@zugschlus.de".to_string())
        );
        assert_eq!(package.parties[1].r#type, None);

        assert_eq!(package.source_packages.len(), 1);
        assert_eq!(
            package.source_packages[0],
            "pkg:deb/debian/adduser".to_string()
        );

        assert!(!package.dependencies.is_empty());
        let build_dep_names: Vec<String> = package
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_ref())
            .filter(|p| p.contains("po-debconf") || p.contains("debhelper"))
            .map(|p| p.to_string())
            .collect();
        assert!(build_dep_names.len() >= 2);
    }

    #[test]
    fn test_dsc_parser_zsh() {
        let path = PathBuf::from("testdata/debian/dsc_files/zsh_5.7.1-1+deb10u1.dsc");
        let package = DebianDscParser::extract_first_package(&path);

        assert_eq!(package.name, Some("zsh".to_string()));
        assert_eq!(package.version, Some("5.7.1-1+deb10u1".to_string()));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert!(package.purl.is_some());
        assert!(package.purl.as_ref().unwrap().contains("zsh"));
        assert!(package.purl.as_ref().unwrap().contains("5.7.1"));
    }

    #[test]
    fn test_parse_dsc_content_basic() {
        let content = "Format: 3.0 (native)
Source: testpkg
Binary: testpkg
Architecture: amd64
Version: 1.0.0
Maintainer: Test User <test@example.com>
Standards-Version: 4.5.0
Build-Depends: debhelper (>= 12)
Files:
 abc123 1024 testpkg_1.0.0.tar.xz
";

        let package = parse_dsc_content(content);
        assert_eq!(package.name, Some("testpkg".to_string()));
        assert_eq!(package.version, Some("1.0.0".to_string()));
        assert_eq!(package.namespace, Some("debian".to_string()));
        assert_eq!(package.parties.len(), 1);
        assert_eq!(package.parties[0].name, Some("Test User".to_string()));
        assert_eq!(
            package.parties[0].email,
            Some("test@example.com".to_string())
        );
        assert_eq!(package.dependencies.len(), 1);
        assert!(package.purl.as_ref().unwrap().contains("arch=amd64"));
    }

    #[test]
    fn test_parse_dsc_content_with_uploaders() {
        let content = "Source: mypkg
Version: 2.0
Architecture: all
Maintainer: Main Dev <main@example.com>
Uploaders: Dev One <dev1@example.com>, Dev Two <dev2@example.com>
";

        let package = parse_dsc_content(content);
        assert_eq!(package.parties.len(), 3);
        assert_eq!(package.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(package.parties[1].role, Some("uploader".to_string()));
        assert_eq!(package.parties[2].role, Some("uploader".to_string()));
    }
}
