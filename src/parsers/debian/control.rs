use std::collections::HashMap;
use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::rfc822::{self, Rfc822Metadata};
use crate::parsers::utils::{MAX_ITERATION_COUNT, split_name_email, truncate_field};

use super::utils::{
    build_debian_purl, detect_namespace, make_party, parse_all_dependencies, parse_source_field,
};
use super::{PACKAGE_TYPE, default_package_data, read_or_default};
use crate::parsers::PackageParser;

// ---------------------------------------------------------------------------
// DebianControlParser: debian/control files (source + binary paragraphs)
// ---------------------------------------------------------------------------

pub struct DebianControlParser;

impl PackageParser for DebianControlParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        if let Some(name) = path.file_name()
            && name == "control"
            && let Some(parent) = path.parent()
            && let Some(parent_name) = parent.file_name()
        {
            return parent_name == "debian";
        }
        false
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = read_or_default!(path, "debian/control", DatasourceId::DebianControlInSource);

        let packages = parse_debian_control(&content);
        if packages.is_empty() {
            vec![default_package_data(DatasourceId::DebianControlInSource)]
        } else {
            packages
        }
    }
}

// ---------------------------------------------------------------------------
// DebianInstalledParser: /var/lib/dpkg/status
// ---------------------------------------------------------------------------

pub struct DebianInstalledParser;

impl PackageParser for DebianInstalledParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.ends_with("var/lib/dpkg/status")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = read_or_default!(path, "dpkg/status", DatasourceId::DebianInstalledStatusDb);

        let packages = parse_dpkg_status(&content);
        if packages.is_empty() {
            vec![default_package_data(DatasourceId::DebianInstalledStatusDb)]
        } else {
            packages
        }
    }
}

pub struct DebianDistrolessInstalledParser;

impl PackageParser for DebianDistrolessInstalledParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.contains("var/lib/dpkg/status.d/")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = read_or_default!(
            path,
            "distroless status file",
            DatasourceId::DebianDistrolessInstalledDb
        );

        vec![parse_distroless_status(&content)]
    }
}

fn parse_distroless_status(content: &str) -> PackageData {
    let paragraphs = rfc822::parse_rfc822_paragraphs(content);

    if paragraphs.is_empty() {
        return default_package_data(DatasourceId::DebianDistrolessInstalledDb);
    }

    build_package_from_paragraph(
        &paragraphs[0],
        None,
        DatasourceId::DebianDistrolessInstalledDb,
    )
    .unwrap_or_else(|| default_package_data(DatasourceId::DebianDistrolessInstalledDb))
}

// ---------------------------------------------------------------------------
// Parsing logic
// ---------------------------------------------------------------------------

/// Parses a debian/control file into PackageData entries.
///
/// A debian/control file has a Source paragraph followed by one or more Binary
/// paragraphs. Source-level metadata (maintainer, homepage, VCS URLs) is merged
/// into each binary package.
fn parse_debian_control(content: &str) -> Vec<PackageData> {
    let paragraphs = rfc822::parse_rfc822_paragraphs(content);
    if paragraphs.is_empty() {
        return Vec::new();
    }

    let has_source = rfc822::get_header_first(&paragraphs[0].headers, "source").is_some();

    let (source_paragraph, binary_start) = if has_source {
        (Some(&paragraphs[0]), 1)
    } else {
        (None, 0)
    };

    let source_meta = source_paragraph.map(extract_source_meta);

    let mut packages = Vec::new();
    let mut count = 0usize;

    for para in &paragraphs[binary_start..] {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!("parse_debian_control: exceeded MAX_ITERATION_COUNT paragraphs, stopping");
            break;
        }
        if let Some(pkg) = build_package_from_paragraph(
            para,
            source_meta.as_ref(),
            DatasourceId::DebianControlInSource,
        ) {
            packages.push(pkg);
        }
    }

    if packages.is_empty()
        && let Some(source_para) = source_paragraph
        && let Some(pkg) = build_package_from_source_paragraph(source_para)
    {
        packages.push(pkg);
    }

    packages
}

/// Parses a dpkg/status file into PackageData entries.
///
/// Each paragraph represents an installed package. Only packages with
/// `Status: install ok installed` are included.
fn parse_dpkg_status(content: &str) -> Vec<PackageData> {
    let paragraphs = rfc822::parse_rfc822_paragraphs(content);
    let mut packages = Vec::new();
    let mut count = 0usize;

    for para in &paragraphs {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!("parse_dpkg_status: exceeded MAX_ITERATION_COUNT paragraphs, stopping");
            break;
        }
        let status = rfc822::get_header_first(&para.headers, "status");
        if status.as_deref() != Some("install ok installed") {
            continue;
        }

        if let Some(pkg) =
            build_package_from_paragraph(para, None, DatasourceId::DebianInstalledStatusDb)
        {
            packages.push(pkg);
        }
    }

    packages
}

// ---------------------------------------------------------------------------
// Source paragraph metadata (shared across binary packages)
// ---------------------------------------------------------------------------

pub(super) struct SourceMeta {
    parties: Vec<Party>,
    homepage_url: Option<String>,
    vcs_url: Option<String>,
    code_view_url: Option<String>,
    bug_tracking_url: Option<String>,
}

fn extract_source_meta(paragraph: &Rfc822Metadata) -> SourceMeta {
    let mut parties = Vec::new();

    // Maintainer
    if let Some(maintainer) = rfc822::get_header_first(&paragraph.headers, "maintainer") {
        let (name, email) = split_name_email(&maintainer);
        parties.push(make_party(Some("person"), "maintainer", name, email));
    }

    // Original-Maintainer
    if let Some(orig_maintainer) =
        rfc822::get_header_first(&paragraph.headers, "original-maintainer")
    {
        let (name, email) = split_name_email(&orig_maintainer);
        parties.push(make_party(Some("person"), "maintainer", name, email));
    }

    // Uploaders (comma-separated)
    if let Some(uploaders_str) = rfc822::get_header_first(&paragraph.headers, "uploaders") {
        for uploader in uploaders_str.split(',') {
            let trimmed = uploader.trim();
            if !trimmed.is_empty() {
                let (name, email) = split_name_email(trimmed);
                parties.push(make_party(Some("person"), "uploader", name, email));
            }
        }
    }

    let homepage_url = rfc822::get_header_first(&paragraph.headers, "homepage").map(truncate_field);

    let vcs_url = rfc822::get_header_first(&paragraph.headers, "vcs-git")
        .map(|url| truncate_field(url.split_whitespace().next().unwrap_or(&url).to_string()));

    let code_view_url =
        rfc822::get_header_first(&paragraph.headers, "vcs-browser").map(truncate_field);

    let bug_tracking_url = rfc822::get_header_first(&paragraph.headers, "bugs").map(truncate_field);

    SourceMeta {
        parties,
        homepage_url,
        vcs_url,
        code_view_url,
        bug_tracking_url,
    }
}

// ---------------------------------------------------------------------------
// Package building
// ---------------------------------------------------------------------------

pub(super) fn build_package_from_paragraph(
    paragraph: &Rfc822Metadata,
    source_meta: Option<&SourceMeta>,
    datasource_id: DatasourceId,
) -> Option<PackageData> {
    let name = rfc822::get_header_first(&paragraph.headers, "package").map(truncate_field)?;
    let version = rfc822::get_header_first(&paragraph.headers, "version").map(truncate_field);
    let architecture =
        rfc822::get_header_first(&paragraph.headers, "architecture").map(truncate_field);
    let description =
        rfc822::get_header_first(&paragraph.headers, "description").map(truncate_field);
    let maintainer_str = rfc822::get_header_first(&paragraph.headers, "maintainer");
    let homepage = rfc822::get_header_first(&paragraph.headers, "homepage").map(truncate_field);
    let source_field = rfc822::get_header_first(&paragraph.headers, "source");
    let section = rfc822::get_header_first(&paragraph.headers, "section");
    let installed_size = rfc822::get_header_first(&paragraph.headers, "installed-size");
    let multi_arch = rfc822::get_header_first(&paragraph.headers, "multi-arch");

    let namespace = detect_namespace(version.as_deref(), maintainer_str.as_deref());

    // Build parties: use source_meta parties if available, otherwise parse from paragraph
    let parties = if let Some(meta) = source_meta {
        meta.parties.clone()
    } else {
        let mut p = Vec::new();
        if let Some(m) = &maintainer_str {
            let (n, e) = split_name_email(m);
            p.push(make_party(Some("person"), "maintainer", n, e));
        }
        p
    };

    // Resolve homepage: paragraph's own, or from source metadata
    let homepage_url = homepage.or_else(|| source_meta.and_then(|m| m.homepage_url.clone()));
    let vcs_url = source_meta.and_then(|m| m.vcs_url.clone());
    let code_view_url = source_meta.and_then(|m| m.code_view_url.clone());
    let bug_tracking_url = source_meta.and_then(|m| m.bug_tracking_url.clone());

    // Build PURL
    let purl = build_debian_purl(
        &name,
        version.as_deref(),
        namespace.as_deref(),
        architecture.as_deref(),
    );

    // Parse dependencies from all dependency fields
    let dependencies = parse_all_dependencies(&paragraph.headers, namespace.as_deref());

    // Keywords from section
    let keywords = section.into_iter().collect();

    // Source packages
    let source_packages = parse_source_field(source_field.as_deref(), namespace.as_deref());

    // Extra data
    let mut extra_data: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(ma) = &multi_arch
        && !ma.is_empty()
    {
        extra_data.insert(
            "multi_arch".to_string(),
            serde_json::Value::String(ma.clone()),
        );
    }
    if let Some(size_str) = &installed_size
        && let Ok(size) = size_str.parse::<u64>()
    {
        extra_data.insert(
            "installed_size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(size)),
        );
    }

    // Qualifiers for architecture
    let qualifiers = architecture.as_ref().map(|arch| {
        let mut q = HashMap::new();
        q.insert("arch".to_string(), arch.clone());
        q
    });

    Some(PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name),
        version,
        qualifiers,
        description,
        parties,
        keywords,
        homepage_url,
        bug_tracking_url,
        code_view_url,
        vcs_url,
        source_packages,
        file_references: Vec::new(),
        extra_data: if extra_data.is_empty() {
            None
        } else {
            Some(extra_data)
        },
        dependencies,
        datasource_id: Some(datasource_id),
        purl,
        ..Default::default()
    })
}

fn build_package_from_source_paragraph(paragraph: &Rfc822Metadata) -> Option<PackageData> {
    let name = rfc822::get_header_first(&paragraph.headers, "source").map(truncate_field)?;
    let version = rfc822::get_header_first(&paragraph.headers, "version").map(truncate_field);
    let maintainer_str = rfc822::get_header_first(&paragraph.headers, "maintainer");

    let namespace = detect_namespace(version.as_deref(), maintainer_str.as_deref());
    let source_meta = extract_source_meta(paragraph);

    let purl = build_debian_purl(&name, version.as_deref(), namespace.as_deref(), None);
    let dependencies = parse_all_dependencies(&paragraph.headers, namespace.as_deref());

    let section = rfc822::get_header_first(&paragraph.headers, "section");
    let keywords = section.into_iter().collect();

    Some(PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name),
        version,
        parties: source_meta.parties,
        keywords,
        homepage_url: source_meta.homepage_url,
        bug_tracking_url: source_meta.bug_tracking_url,
        code_view_url: source_meta.code_view_url,
        vcs_url: source_meta.vcs_url,
        dependencies,
        datasource_id: Some(DatasourceId::DebianControlInSource),
        purl,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Parser registration macros
// ---------------------------------------------------------------------------

crate::register_parser!(
    "Debian source package control file (debian/control)",
    &["**/debian/control"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

crate::register_parser!(
    "Debian installed package database (dpkg status)",
    &["**/var/lib/dpkg/status"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

crate::register_parser!(
    "Debian distroless package database (status.d)",
    &["**/var/lib/dpkg/status.d/*"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn test_parse_debian_control_source_and_binary() {
        let content = "\
Source: curl
Section: web
Priority: optional
Maintainer: Alessandro Ghedini <ghedo@debian.org>
Homepage: https://curl.se/
Vcs-Browser: https://salsa.debian.org/debian/curl
Vcs-Git: https://salsa.debian.org/debian/curl.git
Build-Depends: debhelper (>= 12), libssl-dev

Package: curl
Architecture: amd64
Depends: libc6 (>= 2.17), libcurl4 (= ${binary:Version})
Description: command line tool for transferring data with URL syntax";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name, Some("curl".to_string()));
        assert_eq!(pkg.package_type, Some(PackageType::Deb));
        assert_eq!(pkg.homepage_url, Some("https://curl.se/".to_string()));
        assert_eq!(
            pkg.vcs_url,
            Some("https://salsa.debian.org/debian/curl.git".to_string())
        );
        assert_eq!(
            pkg.code_view_url,
            Some("https://salsa.debian.org/debian/curl".to_string())
        );

        assert_eq!(pkg.parties.len(), 1);
        assert_eq!(pkg.parties[0].role, Some("maintainer".to_string()));
        assert_eq!(pkg.parties[0].name, Some("Alessandro Ghedini".to_string()));
        assert_eq!(pkg.parties[0].email, Some("ghedo@debian.org".to_string()));

        assert!(!pkg.dependencies.is_empty());
    }

    #[test]
    fn test_parse_debian_control_multiple_binary() {
        let content = "\
Source: gzip
Maintainer: Debian Developer <dev@debian.org>

Package: gzip
Architecture: any
Depends: libc6 (>= 2.17)
Description: GNU file compression

Package: gzip-win32
Architecture: all
Description: gzip for Windows";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, Some("gzip".to_string()));
        assert_eq!(packages[1].name, Some("gzip-win32".to_string()));

        assert_eq!(packages[0].parties.len(), 1);
        assert_eq!(packages[1].parties.len(), 1);
    }

    #[test]
    fn test_parse_debian_control_source_only() {
        let content = "\
Source: my-package
Maintainer: Test User <test@debian.org>
Build-Depends: debhelper (>= 13)";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, Some("my-package".to_string()));
        assert!(!packages[0].dependencies.is_empty());
        assert_eq!(
            packages[0].dependencies[0].scope,
            Some("build-depends".to_string())
        );
    }

    #[test]
    fn test_parse_debian_control_with_uploaders() {
        let content = "\
Source: example
Maintainer: Main Dev <main@debian.org>
Uploaders: Alice <alice@example.com>, Bob <bob@example.com>

Package: example
Architecture: any
Description: test package";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].parties.len(), 3);
        assert_eq!(packages[0].parties[0].role, Some("maintainer".to_string()));
        assert_eq!(packages[0].parties[1].role, Some("uploader".to_string()));
        assert_eq!(packages[0].parties[2].role, Some("uploader".to_string()));
    }

    #[test]
    fn test_parse_debian_control_vcs_git_with_branch() {
        let content = "\
Source: example
Maintainer: Dev <dev@debian.org>
Vcs-Git: https://salsa.debian.org/example.git -b main

Package: example
Architecture: any
Description: test";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(
            packages[0].vcs_url,
            Some("https://salsa.debian.org/example.git".to_string())
        );
    }

    #[test]
    fn test_parse_debian_control_multi_arch() {
        let content = "\
Source: example
Maintainer: Dev <dev@debian.org>

Package: libexample
Architecture: any
Multi-Arch: same
Description: shared library";

        let packages = parse_debian_control(content);
        assert_eq!(packages.len(), 1);
        let extra = packages[0].extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("multi_arch"),
            Some(&serde_json::Value::String("same".to_string()))
        );
    }

    #[test]
    fn test_parse_dpkg_status_basic() {
        let content = "\
Package: base-files
Status: install ok installed
Priority: required
Section: admin
Installed-Size: 391
Maintainer: Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>
Architecture: amd64
Version: 11ubuntu5.6
Description: Debian base system miscellaneous files
Homepage: https://tracker.debian.org/pkg/base-files

Package: not-installed
Status: deinstall ok config-files
Architecture: amd64
Version: 1.0
Description: This should be skipped";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);

        let pkg = &packages[0];
        assert_eq!(pkg.name, Some("base-files".to_string()));
        assert_eq!(pkg.version, Some("11ubuntu5.6".to_string()));
        assert_eq!(pkg.namespace, Some("ubuntu".to_string()));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianInstalledStatusDb)
        );

        let extra = pkg.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("installed_size"),
            Some(&serde_json::Value::Number(serde_json::Number::from(391)))
        );
    }

    #[test]
    fn test_parse_dpkg_status_multiple_installed() {
        let content = "\
Package: libc6
Status: install ok installed
Architecture: amd64
Version: 2.31-13+deb11u5
Maintainer: GNU Libc Maintainers <debian-glibc@lists.debian.org>
Description: GNU C Library

Package: zlib1g
Status: install ok installed
Architecture: amd64
Version: 1:1.2.11.dfsg-2+deb11u2
Maintainer: Mark Brown <broonie@debian.org>
Description: compression library";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, Some("libc6".to_string()));
        assert_eq!(packages[1].name, Some("zlib1g".to_string()));
    }

    #[test]
    fn test_parse_dpkg_status_with_dependencies() {
        let content = "\
Package: curl
Status: install ok installed
Architecture: amd64
Version: 7.74.0-1.3+deb11u7
Maintainer: Alessandro Ghedini <ghedo@debian.org>
Depends: libc6 (>= 2.17), libcurl4 (= 7.74.0-1.3+deb11u7)
Recommends: ca-certificates
Description: command line tool for transferring data with URL syntax";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);

        let deps = &packages[0].dependencies;
        assert_eq!(deps.len(), 3);

        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
        assert_eq!(deps[0].scope, Some("depends".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some(">= 2.17".to_string()));

        assert_eq!(
            deps[2].purl,
            Some("pkg:deb/debian/ca-certificates".to_string())
        );
        assert_eq!(deps[2].scope, Some("recommends".to_string()));
        assert_eq!(deps[2].is_optional, Some(true));
    }

    #[test]
    fn test_parse_dpkg_status_with_source() {
        let content = "\
Package: libncurses6
Status: install ok installed
Architecture: amd64
Source: ncurses (6.2+20201114-2+deb11u1)
Version: 6.2+20201114-2+deb11u1
Maintainer: Craig Small <csmall@debian.org>
Description: shared libraries for terminal handling";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);
        assert!(!packages[0].source_packages.is_empty());
        assert!(packages[0].source_packages[0].contains("ncurses"));
    }

    #[test]
    fn test_parse_dpkg_status_filters_not_installed() {
        let content = "\
Package: installed-pkg
Status: install ok installed
Version: 1.0
Architecture: amd64
Description: installed

Package: half-installed
Status: install ok half-installed
Version: 2.0
Architecture: amd64
Description: half installed

Package: deinstall-pkg
Status: deinstall ok config-files
Version: 3.0
Architecture: amd64
Description: deinstalled

Package: purge-pkg
Status: purge ok not-installed
Version: 4.0
Architecture: amd64
Description: purged";

        let packages = parse_dpkg_status(content);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, Some("installed-pkg".to_string()));
    }

    #[test]
    fn test_parse_dpkg_status_empty() {
        let packages = parse_dpkg_status("");
        assert!(packages.is_empty());
    }

    #[test]
    fn test_debian_control_is_match() {
        assert!(DebianControlParser::is_match(Path::new(
            "/path/to/debian/control"
        )));
        assert!(DebianControlParser::is_match(Path::new("debian/control")));
        assert!(!DebianControlParser::is_match(Path::new(
            "/path/to/control"
        )));
        assert!(!DebianControlParser::is_match(Path::new(
            "/path/to/debian/changelog"
        )));
    }

    #[test]
    fn test_debian_installed_is_match() {
        assert!(DebianInstalledParser::is_match(Path::new(
            "/var/lib/dpkg/status"
        )));
        assert!(DebianInstalledParser::is_match(Path::new(
            "some/root/var/lib/dpkg/status"
        )));
        assert!(!DebianInstalledParser::is_match(Path::new(
            "/var/lib/dpkg/status.d/something"
        )));
        assert!(!DebianInstalledParser::is_match(Path::new(
            "/var/lib/dpkg/available"
        )));
    }

    #[test]
    fn test_parse_debian_control_empty_input() {
        let packages = parse_debian_control("");
        assert!(packages.is_empty());
    }

    #[test]
    fn test_parse_debian_control_malformed_input() {
        let content = "this is not a valid control file\nwith random text";
        let packages = parse_debian_control(content);
        assert!(packages.is_empty());
    }

    #[test]
    fn test_distroless_parser() {
        let test_file = PathBuf::from("testdata/debian/var/lib/dpkg/status.d/base-files");

        assert!(DebianDistrolessInstalledParser::is_match(&test_file));

        if !test_file.exists() {
            eprintln!("Warning: Test file not found, skipping test");
            return;
        }

        let pkg = DebianDistrolessInstalledParser::extract_first_package(&test_file);

        assert_eq!(pkg.package_type, Some(PackageType::Deb));
        assert_eq!(
            pkg.datasource_id,
            Some(DatasourceId::DebianDistrolessInstalledDb)
        );
        assert_eq!(pkg.name, Some("base-files".to_string()));
        assert_eq!(pkg.version, Some("11.1+deb11u8".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert!(pkg.purl.is_some());
        assert!(
            pkg.purl
                .as_ref()
                .unwrap()
                .contains("pkg:deb/debian/base-files")
        );
    }
}
