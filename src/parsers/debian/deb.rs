// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::models::{DatasourceId, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::rfc822;
use crate::parsers::utils::truncate_field;

use super::control::build_package_from_paragraph;
use super::copyright::parse_copyright_file;
use super::file_list::parse_file_entries;
use super::utils::build_debian_purl;
use super::{
    MAX_ARCHIVE_SIZE, MAX_COMPRESSION_RATIO, MAX_FILE_SIZE, PACKAGE_TYPE, default_package_data,
    read_or_default,
};
use crate::parsers::PackageParser;

/// Parser for Debian binary package archives (.deb files)
pub struct DebianDebParser;

impl PackageParser for DebianDebParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("deb")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        // Try to extract metadata from archive contents first
        if let Ok(data) = extract_deb_archive(path) {
            return vec![data];
        }

        // Fallback to filename parsing
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => {
                return vec![default_package_data(DatasourceId::DebianDeb)];
            }
        };

        vec![parse_deb_filename(filename)]
    }
}

crate::register_parser!(
    "Debian binary package archive (.deb)",
    &["**/*.deb"],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-binary.html"),
);

fn is_path_traversal(path: &std::path::Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

#[derive(PartialEq)]
enum ExtractionLimit {
    Ok,
    Exceeded,
}

fn check_extraction_limits(
    total_extracted: &mut usize,
    new_bytes: usize,
    compressed_size: usize,
    context: &str,
) -> ExtractionLimit {
    *total_extracted += new_bytes;
    if compressed_size > 0 && *total_extracted / compressed_size > MAX_COMPRESSION_RATIO {
        warn!("{context}: compression ratio exceeded MAX_COMPRESSION_RATIO, stopping");
        ExtractionLimit::Exceeded
    } else if *total_extracted > MAX_ARCHIVE_SIZE as usize {
        warn!("{context}: cumulative extracted size exceeded MAX_ARCHIVE_SIZE, stopping");
        ExtractionLimit::Exceeded
    } else {
        ExtractionLimit::Ok
    }
}

fn extract_deb_archive(path: &Path) -> Result<PackageData, String> {
    use flate2::read::GzDecoder;
    use liblzma::read::XzDecoder;
    use std::io::{Cursor, Read};

    let file_metadata =
        std::fs::metadata(path).map_err(|e| format!("Failed to stat .deb file: {}", e))?;
    if file_metadata.len() > MAX_ARCHIVE_SIZE {
        return Err(format!(
            ".deb file exceeds MAX_ARCHIVE_SIZE ({} bytes)",
            file_metadata.len()
        ));
    }
    let compressed_size = file_metadata.len() as usize;

    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open .deb file: {}", e))?;

    let mut archive = ar::Archive::new(file);
    let mut package: Option<PackageData> = None;
    let mut total_extracted: usize = 0;

    while let Some(entry_result) = archive.next_entry() {
        let entry = entry_result.map_err(|e| format!("Failed to read ar entry: {}", e))?;

        let entry_name_raw = entry.header().identifier();
        let entry_name = String::from_utf8_lossy(entry_name_raw);
        let had_replacement = entry_name_raw.iter().any(|&b| b > 127);
        if had_replacement {
            warn!(
                "extract_deb_archive: non-UTF-8 bytes in entry name replaced with lossy conversion"
            );
        }
        let entry_name = entry_name.trim().to_string();

        if entry_name == "control.tar.gz" || entry_name.starts_with("control.tar") {
            let entry_size = entry.header().size();
            if entry_size > MAX_FILE_SIZE {
                warn!(
                    "extract_deb_archive: control tar entry exceeds MAX_FILE_SIZE ({} bytes), skipping",
                    entry_size
                );
                continue;
            }
            let mut control_data = Vec::new();
            entry
                .take(MAX_FILE_SIZE)
                .read_to_end(&mut control_data)
                .map_err(|e| format!("Failed to read control.tar.gz: {}", e))?;

            if check_extraction_limits(
                &mut total_extracted,
                control_data.len(),
                compressed_size,
                "extract_deb_archive",
            ) == ExtractionLimit::Exceeded
            {
                break;
            }

            if entry_name.ends_with(".gz") {
                let decoder = GzDecoder::new(Cursor::new(control_data));
                if let Some(parsed_package) =
                    parse_control_tar_archive(decoder, &mut total_extracted, compressed_size)?
                {
                    package = Some(parsed_package);
                }
            } else if entry_name.ends_with(".xz") {
                let decoder = XzDecoder::new(Cursor::new(control_data));
                if let Some(parsed_package) =
                    parse_control_tar_archive(decoder, &mut total_extracted, compressed_size)?
                {
                    package = Some(parsed_package);
                }
            }
        } else if entry_name.starts_with("data.tar") {
            let entry_size = entry.header().size();
            if entry_size > MAX_FILE_SIZE {
                warn!(
                    "extract_deb_archive: data tar entry exceeds MAX_FILE_SIZE ({} bytes), skipping",
                    entry_size
                );
                continue;
            }
            let mut data = Vec::new();
            entry
                .take(MAX_FILE_SIZE)
                .read_to_end(&mut data)
                .map_err(|e| format!("Failed to read data archive: {}", e))?;

            if check_extraction_limits(
                &mut total_extracted,
                data.len(),
                compressed_size,
                "extract_deb_archive",
            ) == ExtractionLimit::Exceeded
            {
                break;
            }

            let Some(current_package) = package.as_mut() else {
                continue;
            };

            if entry_name.ends_with(".gz") {
                let decoder = GzDecoder::new(Cursor::new(data));
                merge_deb_data_archive(
                    decoder,
                    current_package,
                    &mut total_extracted,
                    compressed_size,
                )?;
            } else if entry_name.ends_with(".xz") {
                let decoder = XzDecoder::new(Cursor::new(data));
                merge_deb_data_archive(
                    decoder,
                    current_package,
                    &mut total_extracted,
                    compressed_size,
                )?;
            }
        }
    }

    package.ok_or_else(|| ".deb archive does not contain control.tar.* metadata".to_string())
}

fn parse_control_tar_archive<R: std::io::Read>(
    reader: R,
    total_extracted: &mut usize,
    compressed_size: usize,
) -> Result<Option<PackageData>, String> {
    use std::io::Read;

    let mut tar_archive = tar::Archive::new(reader);

    for tar_entry_result in tar_archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?
    {
        let tar_entry = tar_entry_result.map_err(|e| format!("Failed to read tar entry: {}", e))?;

        let tar_path = tar_entry
            .path()
            .map_err(|e| format!("Failed to get tar path: {}", e))?;

        if is_path_traversal(&tar_path) {
            warn!(
                "parse_control_tar_archive: skipping tar entry with path traversal: {:?}",
                tar_path
            );
            continue;
        }

        if tar_entry.size() > MAX_FILE_SIZE {
            warn!(
                "parse_control_tar_archive: tar entry exceeds MAX_FILE_SIZE ({} bytes), skipping",
                tar_entry.size()
            );
            continue;
        }

        if tar_path.ends_with("control") {
            let mut control_content = String::new();
            tar_entry
                .take(MAX_FILE_SIZE)
                .read_to_string(&mut control_content)
                .map_err(|e| format!("Failed to read control file: {}", e))?;

            if check_extraction_limits(
                total_extracted,
                control_content.len(),
                compressed_size,
                "parse_control_tar_archive",
            ) == ExtractionLimit::Exceeded
            {
                return Ok(None);
            }

            let paragraphs = rfc822::parse_rfc822_paragraphs(&control_content);
            if paragraphs.is_empty() {
                return Err("No paragraphs in control file".to_string());
            }

            if let Some(package) =
                build_package_from_paragraph(&paragraphs[0], None, DatasourceId::DebianDeb)
            {
                return Ok(Some(package));
            }

            return Err("Failed to parse control file".to_string());
        }
    }

    Ok(None)
}

fn merge_deb_data_archive<R: std::io::Read>(
    reader: R,
    package: &mut PackageData,
    total_extracted: &mut usize,
    compressed_size: usize,
) -> Result<(), String> {
    use std::io::Read;

    let mut tar_archive = tar::Archive::new(reader);

    for tar_entry_result in tar_archive
        .entries()
        .map_err(|e| format!("Failed to read data tar entries: {}", e))?
    {
        let tar_entry =
            tar_entry_result.map_err(|e| format!("Failed to read data tar entry: {}", e))?;

        let tar_path = tar_entry
            .path()
            .map_err(|e| format!("Failed to get data tar path: {}", e))?;

        if is_path_traversal(&tar_path) {
            warn!(
                "merge_deb_data_archive: skipping tar entry with path traversal: {:?}",
                tar_path
            );
            continue;
        }

        if tar_entry.size() > MAX_FILE_SIZE {
            warn!(
                "merge_deb_data_archive: tar entry exceeds MAX_FILE_SIZE ({} bytes), skipping",
                tar_entry.size()
            );
            continue;
        }

        let tar_path_str = tar_path.to_string_lossy();

        if tar_path_str.ends_with(&format!(
            "/usr/share/doc/{}/copyright",
            package.name.as_deref().unwrap_or_default()
        )) || tar_path_str.ends_with(&format!(
            "usr/share/doc/{}/copyright",
            package.name.as_deref().unwrap_or_default()
        )) {
            let mut copyright_content = String::new();
            tar_entry
                .take(MAX_FILE_SIZE)
                .read_to_string(&mut copyright_content)
                .map_err(|e| format!("Failed to read copyright file from data tar: {}", e))?;

            if check_extraction_limits(
                total_extracted,
                copyright_content.len(),
                compressed_size,
                "merge_deb_data_archive",
            ) == ExtractionLimit::Exceeded
            {
                return Ok(());
            }

            let copyright_pkg = parse_copyright_file(&copyright_content, package.name.as_deref());
            merge_debian_copyright_into_package(package, &copyright_pkg);
            break;
        }
    }

    Ok(())
}

pub(super) fn merge_debian_copyright_into_package(
    target: &mut PackageData,
    copyright: &PackageData,
) {
    if target.extracted_license_statement.is_none() {
        target.extracted_license_statement = copyright.extracted_license_statement.clone();
    }

    if target.declared_license_expression.is_none() {
        target.declared_license_expression = copyright.declared_license_expression.clone();
    }
    if target.declared_license_expression_spdx.is_none() {
        target.declared_license_expression_spdx =
            copyright.declared_license_expression_spdx.clone();
    }
    if target.license_detections.is_empty() {
        target.license_detections = copyright.license_detections.clone();
    }
    if target.other_license_expression.is_none() {
        target.other_license_expression = copyright.other_license_expression.clone();
    }
    if target.other_license_expression_spdx.is_none() {
        target.other_license_expression_spdx = copyright.other_license_expression_spdx.clone();
    }
    if target.other_license_detections.is_empty() {
        target.other_license_detections = copyright.other_license_detections.clone();
    }

    for party in &copyright.parties {
        if !target.parties.iter().any(|existing| existing == party) {
            target.parties.push(party.clone());
        }
    }
}

fn parse_deb_filename(filename: &str) -> PackageData {
    let without_ext = filename.trim_end_matches(".deb");

    let parts: Vec<&str> = without_ext.split('_').collect();
    if parts.len() < 2 {
        return default_package_data(DatasourceId::DebianDeb);
    }

    let name = truncate_field(parts[0].to_string());
    let version = truncate_field(parts[1].to_string());
    let architecture = if parts.len() >= 3 {
        Some(truncate_field(parts[2].to_string()))
    } else {
        None
    };

    let namespace = Some("debian".to_string());

    PackageData {
        datasource_id: Some(DatasourceId::DebianDeb),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: Some(name.clone()),
        version: Some(version.clone()),
        purl: build_debian_purl(
            &name,
            Some(&version),
            namespace.as_deref(),
            architecture.as_deref(),
        ),
        ..Default::default()
    }
}

/// Parser for control files inside extracted .deb control tarballs.
///
/// Matches paths like `*/control.tar.gz-extract/control` and
/// `*/control.tar.xz-extract/control` which are created by ExtractCode
/// when extracting .deb archives.
pub struct DebianControlInExtractedDebParser;

impl PackageParser for DebianControlInExtractedDebParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "control")
            && path
                .to_str()
                .map(|p| {
                    p.ends_with("control.tar.gz-extract/control")
                        || p.ends_with("control.tar.xz-extract/control")
                })
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = read_or_default!(
            path,
            "control file in extracted deb",
            DatasourceId::DebianControlExtractedDeb
        );

        // A control file inside an extracted .deb has a single paragraph
        // (unlike debian/control which has source + binary paragraphs)
        let paragraphs = rfc822::parse_rfc822_paragraphs(&content);
        if paragraphs.is_empty() {
            return vec![default_package_data(
                DatasourceId::DebianControlExtractedDeb,
            )];
        }

        if let Some(pkg) = build_package_from_paragraph(
            &paragraphs[0],
            None,
            DatasourceId::DebianControlExtractedDeb,
        ) {
            vec![pkg]
        } else {
            vec![default_package_data(
                DatasourceId::DebianControlExtractedDeb,
            )]
        }
    }
}

/// Parser for MD5 checksum files inside extracted .deb control tarballs
pub struct DebianMd5sumInPackageParser;

impl PackageParser for DebianMd5sumInPackageParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "md5sums")
            && path
                .to_str()
                .map(|p| {
                    p.ends_with("control.tar.gz-extract/md5sums")
                        || p.ends_with("control.tar.xz-extract/md5sums")
                })
                .unwrap_or(false)
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = read_or_default!(
            path,
            "md5sums file",
            DatasourceId::DebianMd5SumsInExtractedDeb
        );

        let package_name = extract_package_name_from_deb_path(path);

        vec![parse_md5sums_in_package(&content, package_name.as_deref())]
    }
}

pub(crate) fn extract_package_name_from_deb_path(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let grandparent = parent.parent()?;
    let dirname = grandparent.file_name()?.to_str()?;
    let without_extract = dirname.strip_suffix("-extract")?;
    let without_deb = without_extract.strip_suffix(".deb")?;
    let name = without_deb.split('_').next()?;

    Some(name.to_string())
}

fn parse_md5sums_in_package(content: &str, package_name: Option<&str>) -> PackageData {
    let file_references = parse_file_entries(content, "parse_md5sums_in_package");

    if file_references.is_empty() {
        return default_package_data(DatasourceId::DebianMd5SumsInExtractedDeb);
    }

    let namespace = Some("debian".to_string());
    let mut package = PackageData {
        datasource_id: Some(DatasourceId::DebianMd5SumsInExtractedDeb),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: package_name.map(|s| truncate_field(s.to_string())),
        file_references,
        ..Default::default()
    };

    if let Some(n) = &package.name {
        package.purl = build_debian_purl(n, None, namespace.as_deref(), None);
    }

    package
}

crate::register_parser!(
    "Debian control file in extracted .deb control tarball",
    &[
        "**/control.tar.gz-extract/control",
        "**/control.tar.xz-extract/control"
    ],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

crate::register_parser!(
    "Debian MD5 checksums in extracted .deb control tarball",
    &[
        "**/control.tar.gz-extract/md5sums",
        "**/control.tar.xz-extract/md5sums"
    ],
    "deb",
    "",
    Some("https://www.debian.org/doc/debian-policy/ch-controlfields.html"),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use ar::{Builder as ArBuilder, Header as ArHeader};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use liblzma::write::XzEncoder;
    use std::io::Cursor;
    use std::path::PathBuf;
    use tar::{Builder as TarBuilder, Header as TarHeader};
    use tempfile::NamedTempFile;

    fn create_synthetic_deb_with_control_tar_xz() -> NamedTempFile {
        let mut control_tar = Vec::new();
        {
            let encoder = XzEncoder::new(&mut control_tar, 6);
            let mut tar_builder = TarBuilder::new(encoder);

            let control_content = b"Package: synthetic\nVersion: 1.2.3\nArchitecture: amd64\nDescription: Synthetic deb\nHomepage: https://example.com\n";
            let mut header = TarHeader::new_gnu();
            header
                .set_path("control")
                .expect("control tar path should be valid");
            header.set_size(control_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, Cursor::new(control_content))
                .expect("control file should be appended to tar.xz");
            tar_builder.finish().expect("control tar.xz should finish");
        }

        let deb = NamedTempFile::new().expect("temp deb file should be created");
        {
            let mut builder = ArBuilder::new(
                deb.reopen()
                    .expect("temporary deb file should reopen for writing"),
            );

            let debian_binary = b"2.0\n";
            let mut debian_binary_header =
                ArHeader::new(b"debian-binary".to_vec(), debian_binary.len() as u64);
            debian_binary_header.set_mode(0o100644);
            builder
                .append(&debian_binary_header, Cursor::new(debian_binary))
                .expect("debian-binary entry should be appended");

            let mut control_header =
                ArHeader::new(b"control.tar.xz".to_vec(), control_tar.len() as u64);
            control_header.set_mode(0o100644);
            builder
                .append(&control_header, Cursor::new(control_tar))
                .expect("control.tar.xz entry should be appended");
        }

        deb
    }

    fn create_synthetic_deb_with_copyright() -> NamedTempFile {
        let mut control_tar = Vec::new();
        {
            let encoder = GzEncoder::new(&mut control_tar, Compression::default());
            let mut tar_builder = TarBuilder::new(encoder);

            let control_content = b"Package: synthetic\nVersion: 9.9.9\nArchitecture: all\nDescription: Synthetic deb with copyright\n";
            let mut header = TarHeader::new_gnu();
            header
                .set_path("control")
                .expect("control tar path should be valid");
            header.set_size(control_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, Cursor::new(control_content))
                .expect("control file should be appended to tar.gz");
            tar_builder.finish().expect("control tar.gz should finish");
        }

        let mut data_tar = Vec::new();
        {
            let encoder = GzEncoder::new(&mut data_tar, Compression::default());
            let mut tar_builder = TarBuilder::new(encoder);

            let copyright = b"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nFiles: *\nCopyright: 2024 Example Org\nLicense: Apache-2.0\n Licensed under the Apache License, Version 2.0.\n";
            let mut header = TarHeader::new_gnu();
            header
                .set_path("./usr/share/doc/synthetic/copyright")
                .expect("copyright path should be valid");
            header.set_size(copyright.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, Cursor::new(copyright))
                .expect("copyright file should be appended to data tar");
            tar_builder.finish().expect("data tar.gz should finish");
        }

        let deb = NamedTempFile::new().expect("temp deb file should be created");
        {
            let mut builder = ArBuilder::new(
                deb.reopen()
                    .expect("temporary deb file should reopen for writing"),
            );

            let debian_binary = b"2.0\n";
            let mut debian_binary_header =
                ArHeader::new(b"debian-binary".to_vec(), debian_binary.len() as u64);
            debian_binary_header.set_mode(0o100644);
            builder
                .append(&debian_binary_header, Cursor::new(debian_binary))
                .expect("debian-binary entry should be appended");

            let mut control_header =
                ArHeader::new(b"control.tar.gz".to_vec(), control_tar.len() as u64);
            control_header.set_mode(0o100644);
            builder
                .append(&control_header, Cursor::new(control_tar))
                .expect("control.tar.gz entry should be appended");

            let mut data_header = ArHeader::new(b"data.tar.gz".to_vec(), data_tar.len() as u64);
            data_header.set_mode(0o100644);
            builder
                .append(&data_header, Cursor::new(data_tar))
                .expect("data.tar.gz entry should be appended");
        }

        deb
    }

    #[test]
    fn test_deb_parser_is_match() {
        assert!(DebianDebParser::is_match(&PathBuf::from("package.deb")));
        assert!(DebianDebParser::is_match(&PathBuf::from(
            "libapache2-mod-md_2.4.38-3+deb10u10_amd64.deb"
        )));
        assert!(!DebianDebParser::is_match(&PathBuf::from("package.tar.gz")));
        assert!(!DebianDebParser::is_match(&PathBuf::from("control")));
    }

    #[test]
    fn test_parse_deb_filename() {
        let pkg = parse_deb_filename("nginx_1.18.0-1_amd64.deb");
        assert_eq!(pkg.name, Some("nginx".to_string()));
        assert_eq!(pkg.version, Some("1.18.0-1".to_string()));

        let pkg = parse_deb_filename("invalid.deb");
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());
    }

    #[test]
    fn test_parse_deb_filename_with_arch() {
        let pkg = parse_deb_filename("libapache2-mod-md_2.4.38-3+deb10u10_amd64.deb");
        assert_eq!(pkg.name, Some("libapache2-mod-md".to_string()));
        assert_eq!(pkg.version, Some("2.4.38-3+deb10u10".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(
            pkg.purl,
            Some("pkg:deb/debian/libapache2-mod-md@2.4.38-3%2Bdeb10u10?arch=amd64".to_string())
        );
        assert_eq!(pkg.datasource_id, Some(DatasourceId::DebianDeb));
    }

    #[test]
    fn test_parse_deb_filename_without_arch() {
        let pkg = parse_deb_filename("package_1.0-1_all.deb");
        assert_eq!(pkg.name, Some("package".to_string()));
        assert_eq!(pkg.version, Some("1.0-1".to_string()));
        assert!(pkg.purl.as_ref().unwrap().contains("arch=all"));
    }

    #[test]
    fn test_extract_deb_archive() {
        let test_path = PathBuf::from("testdata/debian/deb/adduser_3.112ubuntu1_all.deb");
        if !test_path.exists() {
            return;
        }

        let pkg = DebianDebParser::extract_first_package(&test_path);

        assert_eq!(pkg.name, Some("adduser".to_string()));
        assert_eq!(pkg.version, Some("3.112ubuntu1".to_string()));
        assert_eq!(pkg.namespace, Some("ubuntu".to_string()));
        assert!(pkg.description.is_some());
        assert!(!pkg.parties.is_empty());

        assert!(pkg.purl.as_ref().unwrap().contains("adduser"));
        assert!(pkg.purl.as_ref().unwrap().contains("3.112ubuntu1"));
    }

    #[test]
    fn test_deb_parser_xz_control() {
        let deb = create_synthetic_deb_with_control_tar_xz();

        let pkg = DebianDebParser::extract_first_package(deb.path());

        assert_eq!(pkg.name, Some("synthetic".to_string()));
        assert_eq!(pkg.version, Some("1.2.3".to_string()));
        assert_eq!(pkg.description, Some("Synthetic deb".to_string()));
        assert_eq!(pkg.homepage_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_deb_parser_with_copyright() {
        let deb = create_synthetic_deb_with_copyright();

        let pkg = DebianDebParser::extract_first_package(deb.path());

        assert_eq!(pkg.name, Some("synthetic".to_string()));
        assert_eq!(
            pkg.extracted_license_statement,
            Some("Apache-2.0".to_string())
        );
        assert!(pkg.parties.iter().any(|party| {
            party.role.as_deref() == Some("copyright-holder")
                && party.name.as_deref() == Some("Example Org")
        }));
    }

    #[test]
    fn test_parse_deb_filename_simple() {
        let pkg = parse_deb_filename("adduser_3.112ubuntu1_all.deb");
        assert_eq!(pkg.name, Some("adduser".to_string()));
        assert_eq!(pkg.version, Some("3.112ubuntu1".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
    }

    #[test]
    fn test_parse_deb_filename_invalid() {
        let pkg = parse_deb_filename("invalid.deb");
        assert!(pkg.name.is_none());
        assert!(pkg.version.is_none());
    }
}
