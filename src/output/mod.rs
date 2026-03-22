use std::fs::File;
use std::io::{self, Write};

use crate::models::Output;

mod csv;
mod cyclonedx;
mod html;
mod html_app;
mod jsonl;
mod shared;
mod spdx;
mod template;

pub(crate) const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
pub(crate) const SPDX_DOCUMENT_NOTICE: &str = "Generated with Provenant and provided on an \"AS IS\" BASIS, WITHOUT WARRANTIES\nOR CONDITIONS OF ANY KIND, either express or implied. No content created from\nProvenant should be considered or used as legal advice. Consult an attorney\nfor legal advice.\nProvenant is a free software code scanning tool.\nVisit https://github.com/mstykow/provenant/ for support and download.\nSPDX License List: 3.27";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    JsonPretty,
    Yaml,
    Csv,
    JsonLines,
    Html,
    HtmlApp,
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
                serde_json::to_writer(&mut *writer, output).map_err(shared::io_other)?;
                writer.write_all(b"\n")
            }
            OutputFormat::JsonPretty => {
                serde_json::to_writer_pretty(&mut *writer, output).map_err(shared::io_other)?;
                writer.write_all(b"\n")
            }
            OutputFormat::Yaml => write_yaml(output, writer),
            OutputFormat::Csv => csv::write_csv(output, writer),
            OutputFormat::JsonLines => jsonl::write_json_lines(output, writer),
            OutputFormat::Html => html::write_html_report(output, writer),
            OutputFormat::CustomTemplate => template::write_custom_template(output, writer, config),
            OutputFormat::SpdxTv => spdx::write_spdx_tag_value(output, writer, config),
            OutputFormat::SpdxRdf => spdx::write_spdx_rdf_xml(output, writer, config),
            OutputFormat::CycloneDxJson => cyclonedx::write_cyclonedx_json(output, writer),
            OutputFormat::CycloneDxXml => cyclonedx::write_cyclonedx_xml(output, writer),
            OutputFormat::HtmlApp => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "html-app requires write_output_file() to create companion assets",
            )),
        }
    }
}

pub fn write_output_file(
    output_file: &str,
    output: &Output,
    config: &OutputWriteConfig,
) -> io::Result<()> {
    if output_file == "-" {
        if config.format == OutputFormat::HtmlApp {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "html-app output cannot be written to stdout",
            ));
        }

        let stdout = io::stdout();
        let mut handle = stdout.lock();
        return writer_for_format(config.format).write(output, &mut handle, config);
    }

    if config.format == OutputFormat::HtmlApp {
        return html_app::write_html_app(output_file, output, config);
    }

    let mut file = File::create(output_file)?;
    writer_for_format(config.format).write(output, &mut file, config)
}

fn write_yaml(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    serde_yaml::to_writer(&mut *writer, output).map_err(shared::io_other)?;
    writer.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;

    use crate::models::{
        Author, Copyright, ExtraData, FileInfo, FileType, Header, Holder, LicenseDetection, Match,
        OutputEmail, OutputURL, PackageData, SystemEnvironment,
    };

    #[test]
    fn test_yaml_writer_outputs_yaml() {
        let output = sample_output();
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
        let output = sample_output();
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
    fn test_json_lines_writer_sorts_files_by_path_for_reproducibility() {
        let mut output = sample_output();
        output.files.reverse();
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
    fn test_csv_writer_outputs_headers_and_rows() {
        let output = sample_output();
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Csv)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("csv write should succeed");

        let rendered = String::from_utf8(bytes).expect("csv should be utf-8");
        assert!(rendered.contains("kind,path"));
        assert!(rendered.contains("info"));
    }

    #[test]
    fn test_spdx_tag_value_writer_contains_required_fields() {
        let output = sample_output();
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
        let output = sample_output();
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
    }

    #[test]
    fn test_cyclonedx_json_writer_outputs_bom() {
        let output = sample_output();
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
        let mut output = sample_output();
        output.summary = Some(crate::models::Summary {
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
            other_languages: vec![crate::models::TallyEntry {
                value: Some("Python".to_string()),
                count: 2,
            }],
        });
        output.files[0].is_legal = true;
        output.files[0].is_top_level = true;
        output.files[0].is_key_file = true;

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
        assert_eq!(value["summary"]["other_languages"][0]["value"], "Python");
        assert_eq!(value["files"][0]["is_key_file"], true);
    }

    #[test]
    fn test_json_and_json_lines_writers_include_top_level_tallies() {
        let mut output = sample_output();
        output.tallies = Some(crate::models::Tallies {
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
    fn test_cyclonedx_xml_writer_outputs_xml() {
        let output = sample_output();
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
        let mut output = sample_output();
        output.packages = vec![crate::models::Package {
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
            package_uid: "pkg:maven/example/gradle-project@1.0.0?uuid=test".to_string(),
            datafile_paths: vec![],
        }];

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
    fn test_spdx_empty_scan_tag_value_matches_python_sentinel() {
        let output = Output {
            summary: None,
            tallies: None,
            headers: vec![],
            packages: vec![],
            dependencies: vec![],
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
            headers: vec![],
            packages: vec![],
            dependencies: vec![],
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
        let output = sample_output();
        let mut bytes = Vec::new();
        writer_for_format(OutputFormat::Html)
            .write(&output, &mut bytes, &OutputWriteConfig::default())
            .expect("html write should succeed");
        let rendered = String::from_utf8(bytes).expect("html should be utf-8");
        assert!(rendered.contains("<!doctype html>"));
        assert!(rendered.contains("Custom Template"));
    }

    #[test]
    fn test_custom_template_writer_renders_output_context() {
        let output = sample_output();
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
        assert!(rendered.contains("version=4.0.0"));
        assert!(rendered.contains("files=1"));
    }

    #[test]
    fn test_html_app_writer_creates_assets() {
        let output = sample_output();
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        let output_path = temp_dir.path().join("report.html");

        write_output_file(
            output_path
                .to_str()
                .expect("output path should be valid utf-8"),
            &output,
            &OutputWriteConfig {
                format: OutputFormat::HtmlApp,
                custom_template: None,
                scanned_path: Some("/tmp/project".to_string()),
            },
        )
        .expect("html app write should succeed");

        let assets_dir = temp_dir.path().join("report_files");
        assert!(output_path.exists());
        assert!(assets_dir.join("data.js").exists());
        assert!(assets_dir.join("app.css").exists());
        assert!(assets_dir.join("app.js").exists());
    }

    fn sample_output() -> Output {
        Output {
            summary: None,
            tallies: None,
            headers: vec![Header {
                start_timestamp: "2026-01-01T00:00:00Z".to_string(),
                end_timestamp: "2026-01-01T00:00:01Z".to_string(),
                duration: 1.0,
                extra_data: ExtraData {
                    files_count: 1,
                    directories_count: 1,
                    excluded_count: 0,
                    system_environment: SystemEnvironment {
                        operating_system: Some("darwin".to_string()),
                        cpu_architecture: "aarch64".to_string(),
                        platform: "darwin".to_string(),
                        rust_version: "1.93.0".to_string(),
                    },
                },
                errors: vec![],
                output_format_version: "4.0.0".to_string(),
            }],
            packages: vec![],
            dependencies: vec![],
            files: vec![FileInfo::new(
                "main.rs".to_string(),
                "main".to_string(),
                "rs".to_string(),
                "src/main.rs".to_string(),
                FileType::File,
                Some("text/plain".to_string()),
                42,
                None,
                Some(EMPTY_SHA1.to_string()),
                Some("d41d8cd98f00b204e9800998ecf8427e".to_string()),
                Some("e3b0c44298fc1c149afbf4c8996fb924".to_string()),
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
                        start_line: 1,
                        end_line: 1,
                        matcher: None,
                        score: 100.0,
                        matched_length: None,
                        match_coverage: None,
                        rule_relevance: None,
                        rule_identifier: Some("mit_rule".to_string()),
                        rule_url: None,
                        matched_text: None,
                    }],
                    identifier: None,
                }],
                vec![Copyright {
                    copyright: "Copyright (c) Example".to_string(),
                    start_line: 1,
                    end_line: 1,
                }],
                vec![Holder {
                    holder: "Example Org".to_string(),
                    start_line: 1,
                    end_line: 1,
                }],
                vec![Author {
                    author: "Jane Doe".to_string(),
                    start_line: 1,
                    end_line: 1,
                }],
                vec![OutputEmail {
                    email: "jane@example.com".to_string(),
                    start_line: 1,
                    end_line: 1,
                }],
                vec![OutputURL {
                    url: "https://example.com".to_string(),
                    start_line: 1,
                    end_line: 1,
                }],
                vec![],
                vec![],
            )],
            license_references: vec![],
            license_rule_references: vec![],
        }
    }
}
