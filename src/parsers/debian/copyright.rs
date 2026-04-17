use std::collections::HashMap;
use std::path::Path;

use crate::models::{DatasourceId, LicenseDetection, LineNumber, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::rfc822::{self, Rfc822Metadata};
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use crate::utils::spdx::combine_license_expressions;

use super::utils::{build_debian_purl, make_party};
use super::{PACKAGE_TYPE, default_package_data};
use crate::parsers::PackageParser;
use crate::parsers::license_normalization::{
    DeclaredLicenseMatchMetadata, NormalizedDeclaredLicense, build_declared_license_detection,
    normalize_declared_license_key,
};

/// Parser for Debian machine-readable copyright files (DEP-5 format)
pub struct DebianCopyrightParser;

impl PackageParser for DebianCopyrightParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename != "copyright" {
                return filename.ends_with("_copyright");
            }
            let path_str = path.to_string_lossy();
            path_str.contains("/debian/")
                || path_str.contains("/ports/")
                || path_str.starts_with("ports/")
                || path_str.contains("/packages/deb/")
                || path_str.contains("/usr/share/doc/")
                || path_str.ends_with("debian/copyright")
        } else {
            false
        }
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let datasource_id = detect_debian_copyright_datasource(path);
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read copyright file {:?}: {}", path, e);
                return vec![default_package_data(datasource_id)];
            }
        };

        let package_name = extract_package_name_from_path(path)
            .or_else(|| extract_standalone_package_name_from_path(path, datasource_id));
        let mut package_data = parse_copyright_file(&content, package_name.as_deref());
        package_data.datasource_id = Some(datasource_id);
        vec![package_data]
    }
}

crate::register_parser!(
    "Debian machine-readable copyright file",
    &[
        "**/debian/copyright",
        "**/ports/*/copyright",
        "**/packages/deb/copyright",
        "**/usr/share/doc/*/copyright",
        "**/*_copyright"
    ],
    "deb",
    "",
    Some("https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"),
);

fn detect_debian_copyright_datasource(path: &Path) -> DatasourceId {
    let path_str = path.to_string_lossy();
    if path_str.contains("/debian/") || path_str.ends_with("debian/copyright") {
        DatasourceId::DebianCopyrightInSource
    } else if path_str.contains("/usr/share/doc/") {
        DatasourceId::DebianCopyrightInPackage
    } else {
        DatasourceId::DebianCopyrightStandalone
    }
}

fn extract_package_name_from_path(path: &Path) -> Option<String> {
    let components: Vec<_> = path.components().collect();

    for (i, component) in components.iter().enumerate() {
        if let std::path::Component::Normal(os_str) = component
            && os_str.to_str() == Some("doc")
            && i + 1 < components.len()
            && let std::path::Component::Normal(next) = components[i + 1]
        {
            return next.to_str().map(|s| s.to_string());
        }
    }
    None
}

fn extract_standalone_package_name_from_path(
    path: &Path,
    datasource_id: DatasourceId,
) -> Option<String> {
    if datasource_id != DatasourceId::DebianCopyrightStandalone {
        return None;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name == "copyright")?;

    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

pub(super) fn parse_copyright_file(content: &str, package_name: Option<&str>) -> PackageData {
    let paragraphs = parse_copyright_paragraphs_with_lines(content);

    let is_dep5 = paragraphs
        .first()
        .and_then(|p| rfc822::get_header_first(&p.metadata.headers, "format"))
        .is_some();

    let namespace = Some("debian".to_string());
    let mut parties = Vec::new();
    let mut license_statements = Vec::new();
    let mut primary_license_detection = None;
    let mut header_license_detection = None;
    let mut other_license_detections = Vec::new();

    if is_dep5 {
        let mut para_count = 0usize;
        for para in &paragraphs {
            para_count += 1;
            if para_count > MAX_ITERATION_COUNT {
                warn!("parse_copyright_file: exceeded MAX_ITERATION_COUNT paragraphs, stopping");
                break;
            }
            if let Some(copyright_text) =
                rfc822::get_header_first(&para.metadata.headers, "copyright")
            {
                for holder in parse_copyright_holders(&copyright_text) {
                    if !holder.is_empty() {
                        parties.push(make_party(None, "copyright-holder", Some(holder), None));
                    }
                }
            }

            if let Some(license) = rfc822::get_header_first(&para.metadata.headers, "license") {
                let license_name = license.lines().next().unwrap_or(&license).trim();
                if !license_name.is_empty()
                    && !license_statements.contains(&license_name.to_string())
                {
                    license_statements.push(license_name.to_string());
                }

                if let Some((matched_text, line_no)) = para.license_header_line.clone() {
                    let detection =
                        build_primary_license_detection(license_name, matched_text, line_no);
                    let is_header_paragraph =
                        rfc822::get_header_first(&para.metadata.headers, "format").is_some();
                    if rfc822::get_header_first(&para.metadata.headers, "files").as_deref()
                        == Some("*")
                    {
                        primary_license_detection = Some(detection);
                    } else if is_header_paragraph {
                        header_license_detection.get_or_insert(detection);
                    } else {
                        other_license_detections.push(detection);
                    }
                }
            }
        }

        if primary_license_detection.is_none() && header_license_detection.is_some() {
            primary_license_detection = header_license_detection;
        }
    } else {
        let copyright_block = extract_unstructured_field(content, "Copyright:");
        if let Some(text) = copyright_block {
            for holder in parse_copyright_holders(&text) {
                if !holder.is_empty() {
                    parties.push(make_party(None, "copyright-holder", Some(holder), None));
                }
            }
        }

        let license_block = extract_unstructured_field(content, "License:");
        if let Some(text) = license_block {
            license_statements.push(text.lines().next().unwrap_or(&text).trim().to_string());
        }
    }

    let extracted_license_statement = if license_statements.is_empty() {
        None
    } else {
        Some(truncate_field(license_statements.join(" AND ")))
    };

    let license_detections = primary_license_detection.into_iter().collect::<Vec<_>>();
    let declared_license_expression = license_detections
        .first()
        .map(|detection| detection.license_expression.clone());
    let declared_license_expression_spdx = license_detections
        .first()
        .map(|detection| detection.license_expression_spdx.clone());
    let other_license_expression = combine_license_expressions(
        other_license_detections
            .iter()
            .map(|detection| detection.license_expression.clone()),
    );
    let other_license_expression_spdx = combine_license_expressions(
        other_license_detections
            .iter()
            .map(|detection| detection.license_expression_spdx.clone()),
    );

    PackageData {
        datasource_id: Some(DatasourceId::DebianCopyright),
        package_type: Some(PACKAGE_TYPE),
        namespace: namespace.clone(),
        name: package_name.map(|s| truncate_field(s.to_string())),
        parties,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        other_license_expression,
        other_license_expression_spdx,
        other_license_detections,
        extracted_license_statement,
        purl: package_name.and_then(|n| build_debian_purl(n, None, namespace.as_deref(), None)),
        ..Default::default()
    }
}

#[derive(Debug)]
struct CopyrightParagraph {
    metadata: Rfc822Metadata,
    license_header_line: Option<(String, usize)>,
}

fn parse_copyright_paragraphs_with_lines(content: &str) -> Vec<CopyrightParagraph> {
    let mut paragraphs = Vec::new();
    let mut current_lines = Vec::new();
    let mut current_start_line = 1usize;
    let mut count = 0usize;

    for (idx, line) in content.lines().enumerate() {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!(
                "parse_copyright_paragraphs_with_lines: exceeded MAX_ITERATION_COUNT lines, stopping"
            );
            break;
        }
        let line_no = idx + 1;
        if line.is_empty() {
            if !current_lines.is_empty() {
                paragraphs.push(finalize_copyright_paragraph(
                    std::mem::take(&mut current_lines),
                    current_start_line,
                ));
            }
            current_start_line = line_no + 1;
        } else {
            if current_lines.is_empty() {
                current_start_line = line_no;
            }
            current_lines.push(line.to_string());
        }
    }

    if !current_lines.is_empty() {
        paragraphs.push(finalize_copyright_paragraph(
            current_lines,
            current_start_line,
        ));
    }

    paragraphs
}

fn finalize_copyright_paragraph(raw_lines: Vec<String>, start_line: usize) -> CopyrightParagraph {
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_value = String::new();
    let mut license_header_line = None;

    for (idx, line) in raw_lines.iter().enumerate() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_name.is_some() {
                current_value.push('\n');
                current_value.push_str(line);
            }
            continue;
        }

        if let Some(name) = current_name.take() {
            add_copyright_header_value(&mut headers, &name, &current_value);
            current_value.clear();
        }

        if let Some((name, value)) = line.split_once(':') {
            let normalized_name = name.trim().to_ascii_lowercase();
            if normalized_name == "license" && license_header_line.is_none() {
                license_header_line = Some((line.trim_end().to_string(), start_line + idx));
            }
            current_name = Some(normalized_name);
            current_value = value.trim_start().to_string();
        }
    }

    if let Some(name) = current_name.take() {
        add_copyright_header_value(&mut headers, &name, &current_value);
    }

    CopyrightParagraph {
        metadata: Rfc822Metadata {
            headers,
            body: String::new(),
        },
        license_header_line,
    }
}

fn add_copyright_header_value(headers: &mut HashMap<String, Vec<String>>, name: &str, value: &str) {
    let entry = headers.entry(name.to_string()).or_default();
    let trimmed = value.trim_end();
    if !trimmed.is_empty() {
        entry.push(trimmed.to_string());
    }
}

fn build_primary_license_detection(
    license_name: &str,
    matched_text: String,
    line_no: usize,
) -> LicenseDetection {
    let normalized = normalize_debian_license_name(license_name);
    let line = match LineNumber::new(line_no) {
        Some(l) => l,
        None => {
            warn!(
                "build_primary_license_detection: line number {} out of range, clamping to 1",
                line_no
            );
            LineNumber::new(1).expect("1 is a valid line number")
        }
    };

    build_declared_license_detection(
        &normalized,
        DeclaredLicenseMatchMetadata::new(&matched_text, line, line),
    )
}

fn normalize_debian_license_name(license_name: &str) -> NormalizedDeclaredLicense {
    match license_name.trim() {
        "GPL-2+" => NormalizedDeclaredLicense::new("gpl-2.0-plus", "GPL-2.0-or-later"),
        "GPL-2" => NormalizedDeclaredLicense::new("gpl-2.0", "GPL-2.0-only"),
        "LGPL-2+" => NormalizedDeclaredLicense::new("lgpl-2.0-plus", "LGPL-2.0-or-later"),
        "LGPL-2.1" => NormalizedDeclaredLicense::new("lgpl-2.1", "LGPL-2.1-only"),
        "LGPL-2.1+" => NormalizedDeclaredLicense::new("lgpl-2.1-plus", "LGPL-2.1-or-later"),
        "LGPL-3+" => NormalizedDeclaredLicense::new("lgpl-3.0-plus", "LGPL-3.0-or-later"),
        "BSD-4-clause" => NormalizedDeclaredLicense::new("bsd-original-uc", "BSD-4-Clause-UC"),
        "public-domain" => {
            NormalizedDeclaredLicense::new("public-domain", "LicenseRef-provenant-public-domain")
        }
        other => normalize_declared_license_key(other)
            .unwrap_or_else(|| NormalizedDeclaredLicense::new(other.to_ascii_lowercase(), other)),
    }
}

fn parse_copyright_holders(text: &str) -> Vec<String> {
    let mut holders = Vec::new();
    let mut count = 0usize;

    for line in text.lines() {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!("parse_copyright_holders: exceeded MAX_ITERATION_COUNT lines, stopping");
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let cleaned = line
            .trim_start_matches("Copyright")
            .trim_start_matches("copyright")
            .trim_start_matches("(C)")
            .trim_start_matches("(c)")
            .trim_start_matches("©")
            .trim();

        if let Some(year_end) = cleaned.find(char::is_alphabetic) {
            let without_years = &cleaned[year_end..];
            let holder = without_years
                .trim_start_matches(',')
                .trim_start_matches('-')
                .trim();

            if !holder.is_empty() && holder.len() > 2 {
                holders.push(holder.to_string());
            }
        }
    }

    holders
}

fn extract_unstructured_field(content: &str, field_name: &str) -> Option<String> {
    let mut in_field = false;
    let mut field_content = String::new();
    let mut count = 0usize;

    for line in content.lines() {
        count += 1;
        if count > MAX_ITERATION_COUNT {
            warn!("extract_unstructured_field: exceeded MAX_ITERATION_COUNT lines, stopping");
            break;
        }
        if line.starts_with(field_name) {
            in_field = true;
            field_content.push_str(line.trim_start_matches(field_name).trim());
            field_content.push('\n');
        } else if in_field {
            if line.starts_with(char::is_whitespace) {
                field_content.push_str(line.trim());
                field_content.push('\n');
            } else if !line.trim().is_empty() {
                break;
            }
        }
    }

    let trimmed = field_content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(truncate_field(trimmed.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::deb::merge_debian_copyright_into_package;
    use super::super::default_package_data;
    use super::*;
    use crate::models::DatasourceId;
    use crate::models::LineNumber;
    use std::path::PathBuf;

    #[test]
    fn test_copyright_parser_is_match() {
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "/usr/share/doc/bash/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "debian/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "src/third_party/gperftools/dist/packages/deb/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "ports/zlib/copyright"
        )));
        assert!(!DebianCopyrightParser::is_match(&PathBuf::from(
            "copyright.txt"
        )));
        assert!(!DebianCopyrightParser::is_match(&PathBuf::from(
            "/etc/copyright"
        )));
        assert!(DebianCopyrightParser::is_match(&PathBuf::from(
            "/tmp/sample_copyright"
        )));
    }

    #[test]
    fn test_detect_debian_copyright_datasource() {
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("debian/copyright")),
            DatasourceId::DebianCopyrightInSource
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from(
                "src/third_party/gperftools/dist/packages/deb/copyright"
            )),
            DatasourceId::DebianCopyrightStandalone
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("ports/zlib/copyright")),
            DatasourceId::DebianCopyrightStandalone
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("/usr/share/doc/bash/copyright")),
            DatasourceId::DebianCopyrightInPackage
        );
        assert_eq!(
            detect_debian_copyright_datasource(&PathBuf::from("stable_copyright")),
            DatasourceId::DebianCopyrightStandalone
        );
    }

    #[test]
    fn test_extract_package_name_from_path() {
        assert_eq!(
            extract_package_name_from_path(&PathBuf::from("/usr/share/doc/bash/copyright")),
            Some("bash".to_string())
        );
        assert_eq!(
            extract_package_name_from_path(&PathBuf::from("/usr/share/doc/libseccomp2/copyright")),
            Some("libseccomp2".to_string())
        );
        assert_eq!(
            extract_package_name_from_path(&PathBuf::from("debian/copyright")),
            None
        );
        assert_eq!(
            extract_standalone_package_name_from_path(
                &PathBuf::from("ports/zlib/copyright"),
                DatasourceId::DebianCopyrightStandalone,
            ),
            Some("zlib".to_string())
        );
    }

    #[test]
    fn test_parse_copyright_dep5_format() {
        let content = "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: libseccomp
Source: https://sourceforge.net/projects/libseccomp/

Files: *
Copyright: 2012 Paul Moore <pmoore@redhat.com>
 2012 Ashley Lai <adlai@us.ibm.com>
License: LGPL-2.1

License: LGPL-2.1
 This library is free software
";
        let pkg = parse_copyright_file(content, Some("libseccomp"));
        assert_eq!(pkg.name, Some("libseccomp".to_string()));
        assert_eq!(pkg.namespace, Some("debian".to_string()));
        assert_eq!(pkg.datasource_id, Some(DatasourceId::DebianCopyright));
        assert_eq!(
            pkg.extracted_license_statement,
            Some("LGPL-2.1".to_string())
        );
        assert!(pkg.parties.len() >= 2);
        assert_eq!(pkg.parties[0].role, Some("copyright-holder".to_string()));
        assert!(pkg.parties[0].name.as_ref().unwrap().contains("Paul Moore"));
    }

    #[test]
    fn test_parse_copyright_primary_license_detection_from_bsdutils_fixture() {
        let path = PathBuf::from(
            "testdata/debian-fixtures/debian-slim-2021-04-07/usr/share/doc/bsdutils/copyright",
        );
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        assert_eq!(pkg.name, Some("bsdutils".to_string()));
        let extracted = pkg
            .extracted_license_statement
            .as_deref()
            .expect("license statement should exist");
        assert!(extracted.contains("GPL-2+"));
        assert!(!pkg.license_detections.is_empty());

        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: GPL-2+")
        );
        assert_eq!(primary.matches[0].start_line, LineNumber::new(47).unwrap());
        assert_eq!(primary.matches[0].end_line, LineNumber::new(47).unwrap());
    }

    #[test]
    fn test_parse_copyright_emits_ordered_absolute_case_preserved_detections() {
        let path = PathBuf::from("testdata/debian/copyright/copyright");
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        assert_eq!(pkg.license_detections.len(), 1);
        assert_eq!(pkg.other_license_detections.len(), 4);

        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: LGPL-2.1")
        );
        assert_eq!(primary.matches[0].start_line, LineNumber::new(11).unwrap());

        let ordered_lines: Vec<usize> = pkg
            .other_license_detections
            .iter()
            .map(|detection| detection.matches[0].start_line.get())
            .collect();
        assert_eq!(ordered_lines, vec![15, 19, 23, 25]);

        let ordered_texts: Vec<&str> = pkg
            .other_license_detections
            .iter()
            .map(|detection| detection.matches[0].matched_text.as_deref().unwrap())
            .collect();
        assert_eq!(
            ordered_texts,
            vec![
                "License: LGPL-2.1",
                "License: LGPL-2.1",
                "License: LGPL-2.1",
                "License: LGPL-2.1",
            ]
        );
    }

    #[test]
    fn test_parse_copyright_detects_bottom_standalone_license_paragraph() {
        let path = PathBuf::from(
            "testdata/debian-fixtures/debian-2019-11-15/main/c/clamav/stable_copyright",
        );
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        let zlib = pkg
            .other_license_detections
            .iter()
            .find(|detection| detection.matches[0].matched_text.as_deref() == Some("License: Zlib"))
            .expect("at least one Zlib license paragraph should be detected");
        assert_eq!(
            zlib.matches[0].matched_text.as_deref(),
            Some("License: Zlib")
        );

        let last_zlib = pkg
            .other_license_detections
            .iter()
            .rev()
            .find(|detection| detection.matches[0].matched_text.as_deref() == Some("License: Zlib"))
            .expect("bottom standalone Zlib license paragraph should be detected");
        assert_eq!(
            last_zlib.matches[0].start_line,
            LineNumber::new(732).unwrap()
        );
        assert_eq!(last_zlib.matches[0].end_line, LineNumber::new(732).unwrap());
    }

    #[test]
    fn test_parse_copyright_uses_header_paragraph_as_primary_when_files_star_is_blank() {
        let path =
            PathBuf::from("testdata/debian-fixtures/crafted_for_tests/test_license_nameless");
        let pkg = DebianCopyrightParser::extract_first_package(&path);

        assert_eq!(pkg.license_detections.len(), 1);
        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: LGPL-3+ or GPL-2+")
        );
        assert_eq!(primary.matches[0].start_line, LineNumber::new(8).unwrap());
        assert_eq!(primary.matches[0].end_line, LineNumber::new(8).unwrap());

        assert!(pkg.other_license_detections.iter().any(|detection| {
            detection.matches[0].matched_text.as_deref() == Some("License: GPL-2+")
        }));
    }

    #[test]
    fn test_parse_copyright_prefers_files_star_primary_over_header_paragraph() {
        let content = "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\nUpstream-Name: foo\nLicense: MIT\n\nFiles: *\nCopyright: 2024 Example\nLicense: GPL-2+\n";
        let pkg = parse_copyright_file(content, Some("foo"));

        assert_eq!(pkg.license_detections.len(), 1);
        let primary = &pkg.license_detections[0];
        assert_eq!(
            primary.matches[0].matched_text.as_deref(),
            Some("License: GPL-2+")
        );
        assert_eq!(primary.matches[0].start_line, LineNumber::new(7).unwrap());
    }

    #[test]
    fn test_finalize_copyright_paragraph_matches_rfc822_headers_and_license_line() {
        let raw_lines = vec![
            "Files: *".to_string(),
            "Copyright: 2024 Example Org".to_string(),
            "License: Apache-2.0".to_string(),
            " Licensed under the Apache License, Version 2.0.".to_string(),
        ];

        let paragraph = finalize_copyright_paragraph(raw_lines.clone(), 10);
        let expected = rfc822::parse_rfc822_paragraphs(&raw_lines.join("\n"))
            .into_iter()
            .next()
            .expect("reference RFC822 paragraph should parse");

        assert_eq!(paragraph.metadata.headers, expected.headers);
        assert_eq!(paragraph.metadata.body, expected.body);
        assert_eq!(
            paragraph.license_header_line,
            Some(("License: Apache-2.0".to_string(), 12))
        );
    }

    #[test]
    fn test_parse_copyright_unstructured() {
        let content = "This package was debianized by John Doe.

Upstream Authors:
    Jane Smith

Copyright:
    2009 10gen

License:
    SSPL
";
        let pkg = parse_copyright_file(content, Some("mongodb"));
        assert_eq!(pkg.name, Some("mongodb".to_string()));
        assert_eq!(pkg.extracted_license_statement, Some("SSPL".to_string()));
        assert!(!pkg.parties.is_empty());
    }

    #[test]
    fn test_parse_copyright_holders() {
        let text = "2012 Paul Moore <pmoore@redhat.com>
2012 Ashley Lai <adlai@us.ibm.com>
Copyright (C) 2015-2018 Example Corp";
        let holders = parse_copyright_holders(text);
        assert!(holders.len() >= 3);
        assert!(holders.iter().any(|h| h.contains("Paul Moore")));
        assert!(holders.iter().any(|h| h.contains("Example Corp")));
    }

    #[test]
    fn test_parse_copyright_empty() {
        let content = "This is just some text without proper copyright info.";
        let pkg = parse_copyright_file(content, Some("test"));
        assert_eq!(pkg.name, Some("test".to_string()));
        assert!(pkg.parties.is_empty());
        assert!(pkg.extracted_license_statement.is_none());
    }

    #[test]
    fn test_merge_debian_copyright_into_package_preserves_license_fields() {
        let copyright = parse_copyright_file(
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\
             Upstream-Name: demo\n\n\
             Files: *\n\
             Copyright: 2024 Example\n\
             License: MIT\n\n\
             Files: debian/*\n\
             Copyright: 2024 Debian Example\n\
             License: Apache-2.0\n",
            Some("demo"),
        );
        let mut target = default_package_data(DatasourceId::DebianDeb);

        merge_debian_copyright_into_package(&mut target, &copyright);

        assert_eq!(target.declared_license_expression.as_deref(), Some("mit"));
        assert_eq!(
            target.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(
            target.other_license_expression.as_deref(),
            Some("apache-2.0")
        );
        assert_eq!(
            target.other_license_expression_spdx.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(target.license_detections.len(), 1);
        assert_eq!(target.other_license_detections.len(), 1);
    }
}
