// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::{self, BufWriter, Write};

use crate::output_schema::Output;

mod cyclonedx;
mod debian;
mod html;
mod jsonl;
mod public_serialize;
mod shared;
mod spdx;
mod template;

pub(crate) const SPDX_DOCUMENT_NOTICE: &str = "Generated with Provenant and provided on an \"AS IS\" BASIS, WITHOUT WARRANTIES\nOR CONDITIONS OF ANY KIND, either express or implied. No content created from\nProvenant should be considered or used as legal advice. Consult an attorney\nfor legal advice.\nProvenant is a free software code scanning tool.\nVisit https://github.com/mstykow/provenant/ for support and download.\nSPDX License List: 3.27";
const OUTPUT_BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    JsonPretty,
    Yaml,
    JsonLines,
    Debian,
    Html,
    CustomTemplate,
    SpdxTv,
    SpdxRdf,
    CycloneDxJson,
    CycloneDxXml,
}

#[derive(Debug, Clone, Default)]
pub struct OutputWriteConfig {
    pub format: OutputFormat,
    pub custom_template: Option<String>,
    pub scanned_path: Option<String>,
}

pub trait OutputWriter {
    fn write(
        &self,
        output: &Output,
        writer: &mut dyn Write,
        config: &OutputWriteConfig,
    ) -> io::Result<()>;
}

pub struct FormatWriter {
    format: OutputFormat,
}

pub fn writer_for_format(format: OutputFormat) -> FormatWriter {
    FormatWriter { format }
}

impl OutputWriter for FormatWriter {
    fn write(
        &self,
        output: &Output,
        writer: &mut dyn Write,
        config: &OutputWriteConfig,
    ) -> io::Result<()> {
        match self.format {
            OutputFormat::Json => {
                serde_json::to_writer(&mut *writer, &public_serialize::PublicOutput(output))
                    .map_err(shared::io_other)?;
                writer.write_all(b"\n")
            }
            OutputFormat::JsonPretty => {
                serde_json::to_writer_pretty(&mut *writer, &public_serialize::PublicOutput(output))
                    .map_err(shared::io_other)?;
                writer.write_all(b"\n")
            }
            OutputFormat::Yaml => write_yaml(output, writer),
            OutputFormat::JsonLines => jsonl::write_json_lines(output, writer),
            OutputFormat::Debian => debian::write_debian_copyright(output, writer),
            OutputFormat::Html => html::write_html_report(output, writer),
            OutputFormat::CustomTemplate => template::write_custom_template(output, writer, config),
            OutputFormat::SpdxTv => spdx::write_spdx_tag_value(output, writer, config),
            OutputFormat::SpdxRdf => spdx::write_spdx_rdf_xml(output, writer, config),
            OutputFormat::CycloneDxJson => cyclonedx::write_cyclonedx_json(output, writer),
            OutputFormat::CycloneDxXml => cyclonedx::write_cyclonedx_xml(output, writer),
        }
    }
}

pub fn write_output_file(
    output_file: &str,
    output: &Output,
    config: &OutputWriteConfig,
) -> io::Result<()> {
    if output_file == "-" {
        let stdout = io::stdout();
        let handle = stdout.lock();
        let mut writer = BufWriter::with_capacity(OUTPUT_BUFFER_SIZE, handle);
        writer_for_format(config.format).write(output, &mut writer, config)?;
        return writer.flush();
    }

    let file = File::create(output_file)?;
    let mut writer = BufWriter::with_capacity(OUTPUT_BUFFER_SIZE, file);
    writer_for_format(config.format).write(output, &mut writer, config)?;
    writer.flush()
}

fn write_yaml(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    yaml_serde::to_writer(&mut *writer, &public_serialize::PublicOutput(output))
        .map_err(shared::io_other)?;
    writer.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;

    use crate::models::{
        Author, Copyright, ExtraData, FileInfo, FileType, GitSha1, Header, Holder,
        LicenseDetection, LineNumber, Match, MatchScore, Md5Digest, OutputEmail, OutputURL,
        Package, PackageData, PackageUid, Sha1Digest, Sha256Digest, SystemEnvironment,
    };
    use crate::output_schema::OutputFileInfo;

    #[test]
    fn test_yaml_writer_outputs_yaml() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Yaml)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("yaml write should succeed");
        let rendered = String::from_utf8(bytes).expect("yaml should be utf-8");
        assert!(rendered.contains("headers:"));
        assert!(rendered.contains("files:"));
    }

    #[test]
    fn test_json_lines_writer_outputs_parseable_lines() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");

        let rendered = String::from_utf8(bytes).expect("json-lines should be utf-8");
        let lines = rendered.lines().collect::<Vec<_>>();
        assert!(lines.len() >= 2);
        for line in lines {
            serde_json::from_str::<Value>(line).expect("each line should be valid json");
        }
    }

    #[test]
    fn test_yaml_writer_emits_license_index_provenance_in_headers() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Yaml)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("yaml write should succeed");

        let rendered = String::from_utf8(bytes).expect("yaml should be utf-8");
        assert!(rendered.contains("license_index_provenance:"));
        assert!(rendered.contains("dataset_fingerprint: test-fingerprint"));
        assert!(rendered.contains("source: embedded-artifact"));
    }

    #[test]
    fn test_debian_writer_outputs_dep5_style_document() {
        let mut internal = sample_internal_output();
        internal.files[0].license_expression = Some("mit".to_string());
        internal.files[0].license_detections[0].matches[0].matched_text = Some(
            "Permission is hereby granted, free of charge, to any person obtaining a copy"
                .to_string(),
        );
        let output = Output::from(&internal);

        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Debian)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("debian write should succeed");

        let rendered = String::from_utf8(bytes).expect("debian output should be utf-8");
        assert!(rendered.contains(
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
        ));
        assert!(rendered.contains("Comment: Generated with Provenant"));
        assert!(rendered.contains("Files: src/main.rs"));
        assert!(rendered.contains("Copyright: Example Org"));
        assert!(rendered.contains("License: mit"));
        assert!(rendered.contains(" Permission is hereby granted, free of charge"));
    }

    #[test]
    fn test_debian_writer_skips_directories_and_deduplicates_license_texts() {
        let mut internal = sample_internal_output();
        internal.files.insert(
            0,
            FileInfo::new(
                "src".to_string(),
                "src".to_string(),
                String::new(),
                "src".to_string(),
                FileType::Directory,
                None,
                None,
                0,
                None,
                None,
                None,
                None,
                None,
                vec![],
                None,
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ),
        );
        internal.files[1].license_expression = Some("mit".to_string());
        internal.files[1].license_detections[0].matches[0].matched_text =
            Some("Same text".to_string());
        internal.files[1].license_detections[0].matches.push(Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("src/main.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("2-aho".to_string()),
            score: MatchScore::MAX,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_rule".to_string()),
            rule_url: None,
            matched_text: Some("Same text again".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        });
        let output = Output::from(&internal);

        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Debian)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("debian write should succeed");

        let rendered = String::from_utf8(bytes).expect("debian output should be utf-8");
        assert!(!rendered.contains("Files: src\n"));
        assert_eq!(rendered.matches(" Same text").count(), 1);
    }

    #[test]
    fn test_file_info_serialization_omits_info_fields_when_unset() {
        let file = FileInfo::new(
            "main.rs".to_string(),
            "main".to_string(),
            "rs".to_string(),
            "src/main.rs".to_string(),
            FileType::File,
            None,
            None,
            42,
            None,
            None,
            None,
            None,
            None,
            vec![],
            None,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );

        let schema_file = OutputFileInfo::from(&file);
        let value = serde_json::to_value(&schema_file).expect("file info serializes");
        let object = value.as_object().expect("file info object");

        assert!(!object.contains_key("date"));
        assert!(!object.contains_key("sha1"));
        assert!(!object.contains_key("md5"));
        assert!(!object.contains_key("sha256"));
        assert!(!object.contains_key("sha1_git"));
        assert!(!object.contains_key("mime_type"));
        assert!(!object.contains_key("file_type"));
        assert!(!object.contains_key("programming_language"));
        assert!(!object.contains_key("is_binary"));
        assert!(!object.contains_key("is_text"));
        assert!(!object.contains_key("is_archive"));
        assert!(!object.contains_key("is_media"));
        assert!(!object.contains_key("is_source"));
        assert!(!object.contains_key("is_script"));
        assert!(!object.contains_key("files_count"));
        assert!(!object.contains_key("dirs_count"));
        assert!(!object.contains_key("size_count"));
        assert!(!object.contains_key("license_policy"));
    }

    #[test]
    fn test_file_info_serialization_keeps_license_policy_when_enabled() {
        let mut file = FileInfo::new(
            "main.rs".to_string(),
            "main".to_string(),
            "rs".to_string(),
            "src/main.rs".to_string(),
            FileType::File,
            Some("text/plain".to_string()),
            Some("text".to_string()),
            42,
            Some("2026-01-01T00:00:00Z".to_string()),
            Some(Sha1Digest::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap()),
            Some(Md5Digest::from_hex("d41d8cd98f00b204e9800998ecf8427e").unwrap()),
            Some(
                Sha256Digest::from_hex(
                    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                )
                .unwrap(),
            ),
            Some("Rust".to_string()),
            vec![],
            None,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        file.license_policy = Some(vec![]);
        file.sha1_git =
            Some(GitSha1::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap());
        file.is_binary = Some(false);
        file.is_text = Some(true);
        file.is_archive = Some(false);
        file.is_media = Some(false);
        file.is_source = Some(true);
        file.is_script = Some(false);
        file.files_count = Some(0);
        file.dirs_count = Some(0);
        file.size_count = Some(0);

        let schema_file = OutputFileInfo::from(&file);
        let value = serde_json::to_value(&schema_file).expect("file info serializes");
        let object = value.as_object().expect("file info object");

        assert_eq!(object.get("license_policy"), Some(&serde_json::json!([])));
        assert_eq!(object.get("file_type"), Some(&serde_json::json!("text")));
        assert_eq!(object.get("is_binary"), Some(&serde_json::json!(false)));
        assert_eq!(object.get("is_text"), Some(&serde_json::json!(true)));
        assert_eq!(object.get("files_count"), Some(&serde_json::json!(0)));
        assert_eq!(object.get("dirs_count"), Some(&serde_json::json!(0)));
        assert_eq!(object.get("size_count"), Some(&serde_json::json!(0)));
    }

    #[test]
    fn test_detected_license_expression_spdx_prefers_detection_spdx_values() {
        let mut internal = sample_internal_output();
        internal.files[0].license_expression = Some("mit".to_string());

        let schema_file = OutputFileInfo::from(&internal.files[0]);
        let schema_value = serde_json::to_value(&schema_file).expect("file info serializes");
        assert_eq!(schema_value["detected_license_expression_spdx"], "MIT");

        let output = Output::from(&internal);
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");

        let rendered: Value = serde_json::from_slice(&bytes).expect("json output should parse");
        assert_eq!(
            rendered["files"][0]["detected_license_expression_spdx"],
            "MIT"
        );
    }

    #[test]
    fn test_json_lines_writer_sorts_files_by_path_for_reproducibility() {
        let mut internal = sample_internal_output();
        internal.files.reverse();
        let output = Output::from(&internal);
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");

        let rendered = String::from_utf8(bytes).expect("json-lines should be utf-8");
        let file_lines = rendered
            .lines()
            .filter_map(|line| {
                let value: Value = serde_json::from_str(line).ok()?;
                let files = value.get("files")?.as_array()?;
                files.first()?.get("path")?.as_str().map(str::to_string)
            })
            .collect::<Vec<_>>();

        let mut sorted = file_lines.clone();
        sorted.sort();
        assert_eq!(file_lines, sorted);
    }

    #[test]
    fn test_spdx_tag_value_writer_contains_required_fields() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxTv)
            .write(
                &output,
                &mut bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxTv,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx tv write should succeed");

        let rendered = String::from_utf8(bytes).expect("spdx should be utf-8");
        assert!(rendered.contains("SPDXVersion: SPDX-2.2"));
        assert!(rendered.contains("FileName: ./src/main.rs"));
    }

    #[test]
    fn test_spdx_rdf_writer_outputs_xml() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxRdf)
            .write(
                &output,
                &mut bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxRdf,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx rdf write should succeed");

        let rendered = String::from_utf8(bytes).expect("rdf should be utf-8");
        assert!(rendered.contains("<rdf:RDF"));
        assert!(rendered.contains("<spdx:SpdxDocument"));
        assert!(rendered.contains("<spdx:created>2026-01-01T00:00:00Z</spdx:created>"));
    }

    #[test]
    fn test_cyclonedx_writers_keep_iso_timestamps_when_headers_use_scancode_format() {
        let mut internal = sample_internal_output();
        internal.packages.push(Package::from_package_data(
            &PackageData {
                name: Some("demo".to_string()),
                version: Some("1.0.0".to_string()),
                ..PackageData::default()
            },
            "scan/package.json".to_string(),
        ));
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxJson)
            .write(
                &output,
                &mut json_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::CycloneDxJson,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("cyclonedx json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("cyclonedx json should parse");
        assert_eq!(
            json_value["metadata"]["timestamp"].as_str(),
            Some("2026-01-01T00:00:01Z")
        );

        let mut xml_bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxXml)
            .write(
                &output,
                &mut xml_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::CycloneDxXml,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("cyclonedx xml write should succeed");
        let xml = String::from_utf8(xml_bytes).expect("cyclonedx xml should be utf-8");
        assert!(xml.contains("<timestamp>2026-01-01T00:00:01Z</timestamp>"));
    }

    #[test]
    fn test_spdx_writers_emit_real_file_and_package_license_info() {
        let output = Output::from(&sample_internal_output());

        let mut tv_bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxTv)
            .write(
                &output,
                &mut tv_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxTv,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx tv write should succeed");
        let tv_rendered = String::from_utf8(tv_bytes).expect("spdx tv should be utf-8");
        assert!(tv_rendered.contains("PackageLicenseConcluded: NOASSERTION"));
        assert!(tv_rendered.contains("PackageLicenseInfoFromFiles: MIT"));
        assert!(tv_rendered.contains("LicenseConcluded: NOASSERTION"));
        assert!(tv_rendered.contains("LicenseInfoInFile: MIT"));
        assert!(tv_rendered.contains("PackageCopyrightText: Copyright (c) Example"));

        let mut rdf_bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxRdf)
            .write(
                &output,
                &mut rdf_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxRdf,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx rdf write should succeed");
        let rdf_rendered = String::from_utf8(rdf_bytes).expect("spdx rdf should be utf-8");
        assert!(rdf_rendered.contains(
            "<spdx:licenseInfoFromFiles rdf:resource=\"http://spdx.org/licenses/MIT\"/>"
        ));
        assert!(
            rdf_rendered.contains(
                "<spdx:licenseInfoInFile rdf:resource=\"http://spdx.org/licenses/MIT\"/>"
            )
        );
        assert!(rdf_rendered.contains(
            "<spdx:licenseConcluded rdf:resource=\"http://spdx.org/rdf/terms#noassertion\"/>"
        ));
    }

    #[test]
    fn test_spdx_writers_emit_license_ref_metadata_and_matched_text() {
        let mut internal = sample_internal_output();
        internal.files[0].license_detections = vec![LicenseDetection {
            license_expression: "unknown-license-reference".to_string(),
            license_expression_spdx: "LicenseRef-scancode-unknown-license-reference".to_string(),
            matches: vec![Match {
                license_expression: "unknown-license-reference".to_string(),
                license_expression_spdx: "LicenseRef-scancode-unknown-license-reference"
                    .to_string(),
                from_file: Some("src/main.rs".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(2).unwrap(),
                matcher: Some("2-aho".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(4),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("unknown-license-reference.RULE".to_string()),
                rule_url: Some("https://example.com/unknown-license-reference.LICENSE".to_string()),
                matched_text: Some("Custom license text".to_string()),
                referenced_filenames: Some(vec!["LICENSE".to_string()]),
                matched_text_diagnostics: None,
            }],
            detection_log: vec![],
            identifier: Some("unknown-ref-id".to_string()),
        }];
        internal.license_references = vec![crate::models::LicenseReference {
            key: Some("unknown-license-reference".to_string()),
            language: Some("en".to_string()),
            name: "Unknown License Reference".to_string(),
            short_name: "Unknown License Reference".to_string(),
            owner: None,
            homepage_url: None,
            spdx_license_key: "LicenseRef-scancode-unknown-license-reference".to_string(),
            other_spdx_license_keys: vec![],
            osi_license_key: None,
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            category: None,
            is_exception: false,
            is_unknown: true,
            is_generic: false,
            notes: None,
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: vec![],
            ignorable_holders: vec![],
            ignorable_authors: vec![],
            ignorable_urls: vec![],
            ignorable_emails: vec![],
            scancode_url: None,
            licensedb_url: None,
            spdx_url: None,
            text: "Unused fallback text".to_string(),
        }];
        let output = Output::from(&internal);

        let mut tv_bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxTv)
            .write(
                &output,
                &mut tv_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxTv,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx tv write should succeed");
        let tv_rendered = String::from_utf8(tv_bytes).expect("spdx tv should be utf-8");
        assert!(
            tv_rendered
                .contains("LicenseInfoInFile: LicenseRef-scancode-unknown-license-reference")
        );
        assert!(tv_rendered.contains(
            "PackageLicenseInfoFromFiles: LicenseRef-scancode-unknown-license-reference"
        ));
        assert!(tv_rendered.contains("LicenseID: LicenseRef-scancode-unknown-license-reference"));
        assert!(tv_rendered.contains("ExtractedText: <text>Custom license text"));
        assert!(tv_rendered.contains("LicenseName: Unknown License Reference"));
        assert!(tv_rendered.contains(
            "LicenseComment: <text>See details at https://example.com/unknown-license-reference.LICENSE"
        ));

        let mut rdf_bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxRdf)
            .write(
                &output,
                &mut rdf_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxRdf,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx rdf write should succeed");
        let rdf_rendered = String::from_utf8(rdf_bytes).expect("spdx rdf should be utf-8");
        assert!(rdf_rendered.contains(
            "<spdx:licenseInfoInFile rdf:resource=\"http://spdx.org/licenses/LicenseRef-scancode-unknown-license-reference\"/>"
        ));
        assert!(rdf_rendered.contains(
            "<spdx:hasExtractedLicensingInfo><spdx:ExtractedLicensingInfo rdf:about=\"#LicenseRef-scancode-unknown-license-reference\">"
        ));
        assert!(
            rdf_rendered.contains("<spdx:extractedText>Custom license text</spdx:extractedText>")
        );
    }

    #[test]
    fn test_cyclonedx_json_writer_outputs_bom() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxJson)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("cyclonedx json write should succeed");

        let rendered = String::from_utf8(bytes).expect("cyclonedx json should be utf-8");
        let value: Value = serde_json::from_str(&rendered).expect("valid json");
        assert_eq!(value["bomFormat"], "CycloneDX");
        assert_eq!(value["specVersion"], "1.3");
    }

    #[test]
    fn test_json_writer_includes_summary_and_key_file_flags() {
        let mut internal = sample_internal_output();
        internal.summary = Some(crate::models::Summary {
            declared_license_expression: Some("apache-2.0".to_string()),
            license_clarity_score: Some(crate::models::LicenseClarityScore {
                score: 100,
                declared_license: true,
                identification_precision: true,
                has_license_text: true,
                declared_copyrights: true,
                conflicting_license_categories: false,
                ambiguous_compound_licensing: false,
            }),
            declared_holder: Some("Example Corp.".to_string()),
            primary_language: Some("Ruby".to_string()),
            other_license_expressions: vec![crate::models::TallyEntry {
                value: Some("mit".to_string()),
                count: 1,
            }],
            other_holders: vec![
                crate::models::TallyEntry {
                    value: None,
                    count: 2,
                },
                crate::models::TallyEntry {
                    value: Some("Other Corp.".to_string()),
                    count: 1,
                },
            ],
            other_languages: vec![crate::models::TallyEntry {
                value: Some("Python".to_string()),
                count: 2,
            }],
        });
        internal.files[0].is_legal = true;
        internal.files[0].is_top_level = true;
        internal.files[0].is_key_file = true;
        let output = Output::from(&internal);

        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");

        let rendered = String::from_utf8(bytes).expect("json should be utf-8");
        let value: Value = serde_json::from_str(&rendered).expect("valid json");

        assert_eq!(
            value["summary"]["declared_license_expression"],
            "apache-2.0"
        );
        assert_eq!(value["summary"]["license_clarity_score"]["score"], 100);
        assert_eq!(value["summary"]["declared_holder"], "Example Corp.");
        assert_eq!(value["summary"]["primary_language"], "Ruby");
        assert_eq!(
            value["summary"]["other_license_expressions"][0]["value"],
            "mit"
        );
        assert!(value["summary"]["other_holders"][0]["value"].is_null());
        assert_eq!(value["summary"]["other_holders"][1]["value"], "Other Corp.");
        assert_eq!(value["summary"]["other_languages"][0]["value"], "Python");
        assert_eq!(value["files"][0]["is_key_file"], true);
    }

    #[test]
    fn test_json_and_json_lines_writers_include_top_level_tallies() {
        let mut internal = sample_internal_output();
        internal.tallies = Some(crate::models::Tallies {
            detected_license_expression: vec![crate::models::TallyEntry {
                value: Some("mit".to_string()),
                count: 2,
            }],
            copyrights: vec![crate::models::TallyEntry {
                value: Some("Copyright (c) Example Org".to_string()),
                count: 1,
            }],
            holders: vec![crate::models::TallyEntry {
                value: Some("Example Org".to_string()),
                count: 1,
            }],
            authors: vec![crate::models::TallyEntry {
                value: Some("Jane Doe".to_string()),
                count: 1,
            }],
            programming_language: vec![crate::models::TallyEntry {
                value: Some("Rust".to_string()),
                count: 1,
            }],
        });
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(
            json_value["tallies"]["detected_license_expression"][0]["value"],
            "mit"
        );
        assert_eq!(
            json_value["tallies"]["programming_language"][0]["value"],
            "Rust"
        );

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(rendered.lines().any(|line| line.contains("\"tallies\"")));
    }

    #[test]
    fn test_json_and_json_lines_writers_include_key_file_tallies() {
        let mut internal = sample_internal_output();
        internal.tallies_of_key_files = Some(crate::models::Tallies {
            detected_license_expression: vec![crate::models::TallyEntry {
                value: Some("apache-2.0".to_string()),
                count: 1,
            }],
            copyrights: vec![],
            holders: vec![],
            authors: vec![],
            programming_language: vec![crate::models::TallyEntry {
                value: Some("Markdown".to_string()),
                count: 1,
            }],
        });
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(
            json_value["tallies_of_key_files"]["detected_license_expression"][0]["value"],
            "apache-2.0"
        );

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("\"tallies_of_key_files\""))
        );
    }

    #[test]
    fn test_json_and_json_lines_writers_include_file_tallies() {
        let mut internal = sample_internal_output();
        internal.files[0].tallies = Some(crate::models::Tallies {
            detected_license_expression: vec![crate::models::TallyEntry {
                value: Some("mit".to_string()),
                count: 1,
            }],
            copyrights: vec![crate::models::TallyEntry {
                value: None,
                count: 1,
            }],
            holders: vec![],
            authors: vec![],
            programming_language: vec![crate::models::TallyEntry {
                value: Some("Rust".to_string()),
                count: 1,
            }],
        });
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(
            json_value["files"][0]["tallies"]["detected_license_expression"][0]["value"],
            "mit"
        );

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(rendered.lines().any(|line| line.contains("\"tallies\"")));
    }

    #[test]
    fn test_json_and_json_lines_writers_include_facets_and_tallies_by_facet() {
        let mut internal = sample_internal_output();
        internal.files[0].facets = vec!["core".to_string(), "docs".to_string()];
        internal.tallies_by_facet = Some(vec![crate::models::FacetTallies {
            facet: "core".to_string(),
            tallies: crate::models::Tallies {
                detected_license_expression: vec![crate::models::TallyEntry {
                    value: Some("mit".to_string()),
                    count: 1,
                }],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                programming_language: vec![],
            },
        }]);
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(json_value["files"][0]["facets"][0], "core");
        assert_eq!(json_value["tallies_by_facet"][0]["facet"], "core");

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("\"tallies_by_facet\""))
        );
    }

    #[test]
    fn test_json_and_json_lines_writers_include_top_level_license_references() {
        let mut internal = sample_internal_output();
        internal.license_references = vec![crate::models::LicenseReference {
            key: Some("mit".to_string()),
            language: Some("en".to_string()),
            name: "MIT License".to_string(),
            short_name: "MIT".to_string(),
            owner: Some("Example Owner".to_string()),
            homepage_url: Some("https://example.com/license".to_string()),
            spdx_license_key: "MIT".to_string(),
            other_spdx_license_keys: vec![],
            osi_license_key: Some("MIT".to_string()),
            text_urls: vec!["https://example.com/license.txt".to_string()],
            osi_url: Some("https://opensource.org/licenses/MIT".to_string()),
            faq_url: None,
            other_urls: vec![],
            category: None,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            notes: None,
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: vec![],
            ignorable_holders: vec![],
            ignorable_authors: vec![],
            ignorable_urls: vec![],
            ignorable_emails: vec![],
            scancode_url: None,
            licensedb_url: None,
            spdx_url: None,
            text: "MIT text".to_string(),
        }];
        internal.license_rule_references = vec![crate::models::LicenseRuleReference {
            identifier: "license-clue_1.RULE".to_string(),
            license_expression: "unknown-license-reference".to_string(),
            is_license_text: false,
            is_license_notice: false,
            is_license_reference: false,
            is_license_tag: false,
            is_license_clue: true,
            is_license_intro: false,
            language: None,
            rule_url: None,
            is_required_phrase: false,
            skip_for_required_phrase_generation: false,
            replaced_by: vec![],
            is_continuous: false,
            is_synthetic: false,
            is_from_license: false,
            length: 0,
            relevance: None,
            minimum_coverage: None,
            referenced_filenames: vec![],
            notes: None,
            ignorable_copyrights: vec![],
            ignorable_holders: vec![],
            ignorable_authors: vec![],
            ignorable_urls: vec![],
            ignorable_emails: vec![],
            text: None,
        }];
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(
            json_value["license_references"][0]["spdx_license_key"],
            "MIT"
        );
        assert_eq!(json_value["license_references"][0]["key"], "mit");
        assert_eq!(json_value["license_references"][0]["language"], "en");
        assert_eq!(
            json_value["license_references"][0]["owner"],
            "Example Owner"
        );
        assert_eq!(
            json_value["license_references"][0]["homepage_url"],
            "https://example.com/license"
        );
        assert_eq!(
            json_value["license_references"][0]["osi_license_key"],
            "MIT"
        );
        assert_eq!(
            json_value["license_references"][0]["text_urls"][0],
            "https://example.com/license.txt"
        );
        assert_eq!(
            json_value["license_rule_references"][0]["identifier"],
            "license-clue_1.RULE"
        );
        assert_eq!(
            json_value["license_rule_references"][0]["relevance"],
            Value::Null
        );
        assert_eq!(
            json_value["license_rule_references"][0]["length"],
            Value::from(0)
        );

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("\"license_references\""))
        );
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("\"license_rule_references\""))
        );
    }

    #[test]
    fn test_json_and_json_lines_writers_include_top_level_license_detections() {
        let mut internal = sample_internal_output();
        internal.license_detections = vec![crate::models::TopLevelLicenseDetection {
            identifier: "mit-id".to_string(),
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            detection_count: 2,
            detection_log: vec![],
            reference_matches: vec![crate::models::Match {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                from_file: Some("src/main.rs".to_string()),
                start_line: LineNumber::ONE,
                end_line: LineNumber::new(3).unwrap(),
                matcher: Some("1-hash".to_string()),
                score: MatchScore::MAX,
                matched_length: Some(10),
                match_coverage: Some(100.0),
                rule_relevance: Some(100),
                rule_identifier: Some("mit.LICENSE".to_string()),
                rule_url: None,
                matched_text: None,
                referenced_filenames: None,
                matched_text_diagnostics: None,
            }],
        }];
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(json_value["license_detections"][0]["identifier"], "mit-id");
        assert_eq!(json_value["license_detections"][0]["detection_count"], 2);

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("\"license_detections\""))
        );
    }

    #[test]
    fn test_json_and_json_lines_writers_keep_empty_top_level_license_detections() {
        let output = Output::from(&sample_internal_output());

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let json_value: Value =
            serde_json::from_slice(&json_bytes).expect("json output should parse");
        assert_eq!(json_value["license_detections"], Value::Array(vec![]));

        let mut jsonl_bytes = Vec::new();
        writer_for_format(OutputFormat::JsonLines)
            .write(&output, &mut jsonl_bytes, &OutputWriteConfig::default())
            .expect("json-lines write should succeed");
        let rendered = String::from_utf8(jsonl_bytes).expect("json-lines should be utf-8");
        assert!(
            rendered
                .lines()
                .any(|line| line == r#"{"license_detections":[]}"#)
        );
    }

    #[test]
    fn test_public_writer_normalizes_empty_package_maps_without_changing_schema_output() {
        let mut internal = sample_internal_output();
        internal.packages.push(Package::from_package_data(
            &PackageData {
                package_type: Some(crate::models::PackageType::Npm),
                name: Some("demo".to_string()),
                version: Some("1.0.0".to_string()),
                ..PackageData::default()
            },
            "scan/package.json".to_string(),
        ));

        let output = Output::from(&internal);
        let raw_schema = serde_json::to_value(&output).expect("schema output should serialize");
        assert_eq!(
            raw_schema["packages"][0]["qualifiers"],
            serde_json::json!({})
        );
        assert_eq!(
            raw_schema["packages"][0]["extra_data"],
            serde_json::json!({})
        );

        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Json)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("json write should succeed");
        let public_value: Value = serde_json::from_slice(&bytes).expect("public json should parse");

        assert!(public_value["packages"][0]["qualifiers"].is_null());
        assert!(public_value["packages"][0]["extra_data"].is_null());
    }

    #[test]
    fn test_cyclonedx_xml_writer_outputs_xml() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxXml)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("cyclonedx xml write should succeed");

        let rendered = String::from_utf8(bytes).expect("cyclonedx xml should be utf-8");
        assert!(rendered.contains("<bom xmlns=\"http://cyclonedx.org/schema/bom/1.3\""));
        assert!(rendered.contains("<components>"));
    }

    #[test]
    fn test_cyclonedx_json_includes_component_license_expression() {
        let mut internal = sample_internal_output();
        internal.packages = vec![crate::models::Package {
            package_type: Some(crate::models::PackageType::Maven),
            namespace: Some("example".to_string()),
            name: Some("gradle-project".to_string()),
            version: Some("1.0.0".to_string()),
            qualifiers: None,
            subpath: None,
            primary_language: Some("Java".to_string()),
            description: None,
            release_date: None,
            parties: vec![],
            keywords: vec![],
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression: Some("Apache-2.0".to_string()),
            declared_license_expression_spdx: Some("Apache-2.0".to_string()),
            license_detections: vec![],
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: vec![],
            extracted_license_statement: Some("Apache-2.0".to_string()),
            notice_text: None,
            source_packages: vec![],
            is_private: false,
            is_virtual: false,
            extra_data: None,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_ids: vec![],
            purl: Some("pkg:maven/example/gradle-project@1.0.0".to_string()),
            package_uid: PackageUid::from_raw(
                "pkg:maven/example/gradle-project@1.0.0?uuid=test".to_string(),
            ),
            datafile_paths: vec![],
        }];
        let output = Output::from(&internal);

        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxJson)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("cyclonedx json write should succeed");

        let rendered = String::from_utf8(bytes).expect("cyclonedx json should be utf-8");
        let value: Value = serde_json::from_str(&rendered).expect("valid json");

        assert_eq!(
            value["components"][0]["licenses"][0]["expression"],
            "Apache-2.0"
        );
    }

    #[test]
    fn test_cyclonedx_external_references_are_deduplicated() {
        let mut internal = sample_internal_output();
        internal.packages = vec![Package::from_package_data(
            &PackageData {
                package_type: Some(crate::models::PackageType::Npm),
                name: Some("demo".to_string()),
                version: Some("1.0.0".to_string()),
                download_url: Some("https://example.com/download.tgz".to_string()),
                repository_download_url: Some("https://example.com/download.tgz".to_string()),
                homepage_url: Some("https://example.com".to_string()),
                repository_homepage_url: Some("https://example.com".to_string()),
                ..PackageData::default()
            },
            "scan/package.json".to_string(),
        )];
        let output = Output::from(&internal);

        let mut json_bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxJson)
            .write(&output, &mut json_bytes, &OutputWriteConfig::default())
            .expect("cyclonedx json write should succeed");
        let value: Value = serde_json::from_slice(&json_bytes).expect("valid cyclonedx json");
        let refs = value["components"][0]["externalReferences"]
            .as_array()
            .expect("external references should be an array");
        assert_eq!(refs.len(), 2);

        let mut xml_bytes = Vec::new();
        writer_for_format(OutputFormat::CycloneDxXml)
            .write(&output, &mut xml_bytes, &OutputWriteConfig::default())
            .expect("cyclonedx xml write should succeed");
        let xml = String::from_utf8(xml_bytes).expect("cyclonedx xml should be utf-8");
        assert_eq!(xml.matches("https://example.com/download.tgz").count(), 1);
        assert_eq!(xml.matches("https://example.com</url>").count(), 1);
    }

    #[test]
    fn test_spdx_prefers_single_detected_package_name_over_scan_root() {
        let mut internal = sample_internal_output();
        internal.packages = vec![Package::from_package_data(
            &PackageData {
                package_type: Some(crate::models::PackageType::Npm),
                name: Some("detected-package".to_string()),
                version: Some("1.0.0".to_string()),
                ..PackageData::default()
            },
            "scan/package.json".to_string(),
        )];
        let output = Output::from(&internal);

        let mut tv_bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxTv)
            .write(
                &output,
                &mut tv_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxTv,
                    custom_template: None,
                    scanned_path: Some("scan-root".to_string()),
                },
            )
            .expect("spdx tv write should succeed");
        let tv = String::from_utf8(tv_bytes).expect("spdx tv should be utf-8");
        assert!(tv.contains("PackageName: detected-package"));
        assert!(tv.contains("DocumentNamespace: http://spdx.org/spdxdocs/detected-package"));

        let mut rdf_bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxRdf)
            .write(
                &output,
                &mut rdf_bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxRdf,
                    custom_template: None,
                    scanned_path: Some("scan-root".to_string()),
                },
            )
            .expect("spdx rdf write should succeed");
        let rdf = String::from_utf8(rdf_bytes).expect("spdx rdf should be utf-8");
        assert!(rdf.contains("<spdx:name>detected-package</spdx:name>"));
    }

    #[test]
    fn test_spdx_empty_scan_tag_value_matches_python_sentinel() {
        let output = Output {
            summary: None,
            tallies: None,
            tallies_of_key_files: None,
            tallies_by_facet: None,
            headers: vec![],
            packages: vec![],
            dependencies: vec![],
            license_detections: vec![],
            files: vec![],
            license_references: vec![],
            license_rule_references: vec![],
        };
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxTv)
            .write(
                &output,
                &mut bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxTv,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx tv write should succeed");

        let rendered = String::from_utf8(bytes).expect("spdx should be utf-8");
        assert_eq!(rendered, "# No results for package 'scan'.\n");
    }

    #[test]
    fn test_spdx_empty_scan_rdf_matches_python_sentinel() {
        let output = Output {
            summary: None,
            tallies: None,
            tallies_of_key_files: None,
            tallies_by_facet: None,
            headers: vec![],
            packages: vec![],
            dependencies: vec![],
            license_detections: vec![],
            files: vec![],
            license_references: vec![],
            license_rule_references: vec![],
        };
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::SpdxRdf)
            .write(
                &output,
                &mut bytes,
                &OutputWriteConfig {
                    format: OutputFormat::SpdxRdf,
                    custom_template: None,
                    scanned_path: Some("scan".to_string()),
                },
            )
            .expect("spdx rdf write should succeed");

        let rendered = String::from_utf8(bytes).expect("rdf should be utf-8");
        assert_eq!(rendered, "<!-- No results for package 'scan'. -->\n");
    }

    #[test]
    fn test_html_writer_outputs_html_document() {
        let output = Output::from(&sample_internal_output());
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Html)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("html write should succeed");
        let rendered = String::from_utf8(bytes).expect("html should be utf-8");
        assert!(rendered.contains("<!doctype html>"));
        assert!(rendered.contains("Provenant HTML Report"));
    }

    #[test]
    fn test_custom_template_writer_renders_output_context() {
        let output = Output::from(&sample_internal_output());
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        let template_path = temp_dir.path().join("template.tera");
        fs::write(
            &template_path,
            "version={{ output.headers[0].output_format_version }} files={{ files | length }}",
        )
        .expect("template should be written");

        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::CustomTemplate)
            .write(
                &output,
                &mut bytes,
                &OutputWriteConfig {
                    format: OutputFormat::CustomTemplate,
                    custom_template: Some(template_path.to_string_lossy().to_string()),
                    scanned_path: None,
                },
            )
            .expect("custom template write should succeed");

        let rendered = String::from_utf8(bytes).expect("template output should be utf-8");
        assert!(rendered.contains("version=4.1.0"));
        assert!(rendered.contains("files=1"));
    }

    fn sample_internal_output() -> crate::models::Output {
        crate::models::Output {
            summary: None,
            tallies: None,
            tallies_of_key_files: None,
            tallies_by_facet: None,
            headers: vec![Header {
                tool_name: "provenant".to_string(),
                tool_version: crate::version::BUILD_VERSION.to_string(),
                options: serde_json::Map::new(),
                notice: crate::models::HEADER_NOTICE.to_string(),
                start_timestamp: "2026-01-01T000000.000000".to_string(),
                end_timestamp: "2026-01-01T000001.000000".to_string(),
                output_format_version: "4.1.0".to_string(),
                duration: 1.0,
                errors: vec![],
                warnings: vec![],
                extra_data: ExtraData {
                    system_environment: SystemEnvironment {
                        operating_system: "darwin".to_string(),
                        cpu_architecture: "aarch64".to_string(),
                        platform: "darwin".to_string(),
                        platform_version: "26.3.1".to_string(),
                        rust_version: "1.93.0".to_string(),
                    },
                    spdx_license_list_version: "3.27".to_string(),
                    files_count: 1,
                    directories_count: 1,
                    excluded_count: 0,
                    license_index_provenance: Some(crate::models::LicenseIndexProvenance {
                        source: "embedded-artifact".to_string(),
                        dataset_fingerprint: "test-fingerprint".to_string(),
                        ignored_rules: vec![
                            "gpl-2.0_and-unknown-license-reference_1.RULE".to_string(),
                        ],
                        ignored_licenses: vec![],
                        ignored_rules_due_to_licenses: vec![],
                        added_rules: vec![],
                        replaced_rules: vec![],
                        added_licenses: vec![],
                        replaced_licenses: vec![],
                    }),
                },
            }],
            packages: vec![],
            dependencies: vec![],
            license_detections: vec![],
            files: vec![FileInfo::new(
                "main.rs".to_string(),
                "main".to_string(),
                "rs".to_string(),
                "src/main.rs".to_string(),
                FileType::File,
                Some("text/plain".to_string()),
                None,
                42,
                None,
                Some(Sha1Digest::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap()),
                Some(Md5Digest::from_hex("d41d8cd98f00b204e9800998ecf8427e").unwrap()),
                Some(
                    Sha256Digest::from_hex(
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    )
                    .unwrap(),
                ),
                Some("Rust".to_string()),
                vec![PackageData::default()],
                None,
                vec![LicenseDetection {
                    license_expression: "mit".to_string(),
                    license_expression_spdx: "MIT".to_string(),
                    matches: vec![Match {
                        license_expression: "mit".to_string(),
                        license_expression_spdx: "MIT".to_string(),
                        from_file: None,
                        start_line: LineNumber::ONE,
                        end_line: LineNumber::ONE,
                        matcher: None,
                        score: MatchScore::MAX,
                        matched_length: None,
                        match_coverage: None,
                        rule_relevance: None,
                        rule_identifier: Some("mit_rule".to_string()),
                        rule_url: None,
                        matched_text: None,
                        referenced_filenames: None,
                        matched_text_diagnostics: None,
                    }],
                    detection_log: vec![],
                    identifier: None,
                }],
                vec![],
                vec![Copyright {
                    copyright: "Copyright (c) Example".to_string(),
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                }],
                vec![Holder {
                    holder: "Example Org".to_string(),
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                }],
                vec![Author {
                    author: "Jane Doe".to_string(),
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                }],
                vec![OutputEmail {
                    email: "jane@example.com".to_string(),
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                }],
                vec![OutputURL {
                    url: "https://example.com".to_string(),
                    start_line: LineNumber::ONE,
                    end_line: LineNumber::ONE,
                }],
                vec![],
                vec![],
            )],
            license_references: vec![],
            license_rule_references: vec![],
        }
    }
}
