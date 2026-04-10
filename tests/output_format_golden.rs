use provenant::models::{
    Copyright, DatasourceId, ExtraData, FacetTallies, FileInfo, FileType, Header, Holder,
    LineNumber, Md5Digest, Output, Package, PackageData, PackageType, Party, ResolvedPackage,
    Sha1Digest, SystemEnvironment, Tallies, TallyEntry, TopLevelDependency,
};
use provenant::output_schema::Output as OutputSchemaOutput;
use provenant::{OutputFormat, OutputWriteConfig, OutputWriter, writer_for_format};
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;

const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

#[test]
fn test_spdx_empty_matches_local_python_golden() {
    let output = empty_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::SpdxTv)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::SpdxTv,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("spdx output should be generated");

    let actual = String::from_utf8(bytes).expect("spdx output should be utf-8");
    let expected = fs::read_to_string("testdata/output-formats/spdx-empty-expected.tv")
        .expect("golden fixture should be readable");
    assert_eq!(actual, expected);
}

#[test]
fn test_cyclonedx_empty_matches_local_python_golden_core_fields() {
    let output = empty_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::CycloneDxJson)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxJson,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx output should be generated");

    let actual: Value = serde_json::from_slice(&bytes).expect("cyclonedx output should be json");
    let expected_text =
        fs::read_to_string("testdata/output-formats/cyclonedx-expected-without-packages.json")
            .expect("golden fixture should be readable");
    let expected: Value =
        serde_json::from_str(&expected_text).expect("golden fixture should be json");

    let actual = normalize_cyclonedx(actual);
    let expected = normalize_cyclonedx(expected);
    assert_eq!(actual, expected);
}

#[test]
fn test_json_lines_contract_shape_matches_python_fixture_structure() {
    let fixture = fs::read_to_string("testdata/output-formats/json-simple-expected.jsonlines")
        .expect("json-lines fixture should be readable");
    let expected: Value = serde_json::from_str(&fixture).expect("fixture should be valid json");
    let expected_array = expected.as_array().expect("fixture should be an array");

    let output = sample_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::JsonLines)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::JsonLines,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("json-lines output should be generated");

    let lines = String::from_utf8(bytes)
        .expect("output should be utf-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("line should be json"))
        .collect::<Vec<_>>();

    assert!(!lines.is_empty());
    assert!(lines[0].get("headers").is_some());
    assert!(expected_array[0].get("headers").is_some());
    assert!(lines.iter().any(|line| line.get("files").is_some()));
}

#[test]
fn test_debian_output_matches_local_expected_fixture() {
    let mut file = sample_plain_text_file(
        "main.rs",
        "main",
        ".rs",
        "scan/src/main.rs",
        42,
        EMPTY_SHA1,
        vec![],
    );
    file.holders = vec![Holder {
        holder: "Example Org".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];
    file.license_expression = Some("MIT".to_string());
    file.license_detections = vec![provenant::models::LicenseDetection {
        license_expression: "mit".to_string(),
        license_expression_spdx: "MIT".to_string(),
        matches: vec![provenant::models::Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: Some("scan/src/main.rs".to_string()),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: 100.0,
            matched_length: Some(1),
            match_coverage: Some(100.0),
            rule_relevance: Some(100),
            rule_identifier: Some("mit_1.RULE".to_string()),
            rule_url: None,
            matched_text: Some("Permission is hereby granted.".to_string()),
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }],
        detection_log: vec![],
        identifier: None,
    }];

    let output = sample_output_with_sections(1, 0, vec![], vec![], vec![file]);
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::Debian)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::Debian,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("debian output should be generated");

    let actual = String::from_utf8(bytes).expect("debian output should be utf-8");
    let expected = fs::read_to_string("testdata/output-formats/debian-basic-expected.copyright")
        .expect("debian expected fixture should be readable");
    assert_eq!(actual, expected);
}

#[test]
fn test_json_contract_includes_detailed_tallies_for_files_and_directories() {
    let mut root = sample_directory_file("scan");
    root.tallies = Some(Tallies {
        detected_license_expression: vec![
            TallyEntry {
                value: None,
                count: 1,
            },
            TallyEntry {
                value: Some("mit".to_string()),
                count: 1,
            },
        ],
        copyrights: vec![],
        holders: vec![],
        authors: vec![],
        programming_language: vec![
            TallyEntry {
                value: Some("Markdown".to_string()),
                count: 1,
            },
            TallyEntry {
                value: Some("Rust".to_string()),
                count: 1,
            },
        ],
    });

    let mut src = sample_directory_file("scan/src");
    src.tallies = Some(Tallies {
        detected_license_expression: vec![TallyEntry {
            value: Some("mit".to_string()),
            count: 1,
        }],
        copyrights: vec![],
        holders: vec![],
        authors: vec![],
        programming_language: vec![TallyEntry {
            value: Some("Rust".to_string()),
            count: 1,
        }],
    });

    let mut empty = sample_directory_file("scan/empty");
    empty.tallies = Some(Tallies::default());

    let mut file = sample_plain_text_file(
        "main.rs",
        "main",
        ".rs",
        "scan/src/main.rs",
        10,
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
        vec![],
    );
    file.programming_language = Some("Rust".to_string());
    file.tallies = Some(Tallies {
        detected_license_expression: vec![TallyEntry {
            value: Some("mit".to_string()),
            count: 1,
        }],
        copyrights: vec![],
        holders: vec![],
        authors: vec![],
        programming_language: vec![TallyEntry {
            value: Some("Rust".to_string()),
            count: 1,
        }],
    });

    let output = sample_output_with_sections(1, 3, vec![], vec![], vec![root, src, empty, file]);
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::Json)
        .write(&schema_output, &mut bytes, &OutputWriteConfig::default())
        .expect("json output should be generated");

    let value: Value = serde_json::from_slice(&bytes).expect("json output should parse");
    let files = value["files"].as_array().expect("files should be an array");

    let root = files
        .iter()
        .find(|file| file["path"] == "scan")
        .expect("root directory present");
    let src = files
        .iter()
        .find(|file| file["path"] == "scan/src")
        .expect("src directory present");
    let empty = files
        .iter()
        .find(|file| file["path"] == "scan/empty")
        .expect("empty directory present");
    let file = files
        .iter()
        .find(|file| file["path"] == "scan/src/main.rs")
        .expect("source file present");

    assert!(root.get("tallies").is_some());
    assert!(src.get("tallies").is_some());
    assert!(empty.get("tallies").is_some());
    assert!(file.get("tallies").is_some());

    assert_eq!(
        root["tallies"]["detected_license_expression"][0]["value"],
        Value::Null
    );
    assert_eq!(src["tallies"]["programming_language"][0]["value"], "Rust");
    assert_eq!(empty["tallies"], serde_json::json!({}));
    assert_eq!(
        file["tallies"]["detected_license_expression"][0]["value"],
        "mit"
    );
}

#[test]
fn test_json_contract_includes_facets_and_tallies_by_facet() {
    let mut core_file = sample_plain_text_file(
        "README",
        "README",
        "",
        "scan/README",
        10,
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
        vec![],
    );
    core_file.facets = vec!["docs".to_string()];
    core_file.tallies = Some(Tallies {
        detected_license_expression: vec![TallyEntry {
            value: Some("mit".to_string()),
            count: 1,
        }],
        copyrights: vec![],
        holders: vec![],
        authors: vec![],
        programming_language: vec![],
    });

    let output = Output {
        summary: None,
        tallies: None,
        tallies_of_key_files: None,
        tallies_by_facet: Some(vec![FacetTallies {
            facet: "docs".to_string(),
            tallies: Tallies {
                detected_license_expression: vec![TallyEntry {
                    value: Some("mit".to_string()),
                    count: 1,
                }],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                programming_language: vec![],
            },
        }]),
        headers: vec![sample_header(1, 0)],
        packages: vec![],
        dependencies: vec![],
        license_detections: vec![],
        files: vec![core_file],
        license_references: vec![],
        license_rule_references: vec![],
    };

    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::Json)
        .write(&schema_output, &mut bytes, &OutputWriteConfig::default())
        .expect("json output should be generated");
    let value: Value = serde_json::from_slice(&bytes).expect("json should parse");

    assert_eq!(value["files"][0]["facets"][0], "docs");
    assert_eq!(value["tallies_by_facet"][0]["facet"], "docs");
    assert_eq!(
        value["tallies_by_facet"][0]["tallies"]["detected_license_expression"][0]["value"],
        "mit"
    );
}

#[test]
fn test_json_lines_matches_local_fixture_file_semantics() {
    let fixture = fs::read_to_string("testdata/output-formats/json-simple-expected.jsonlines")
        .expect("json-lines fixture should be readable");
    let expected_entries: Vec<Value> =
        serde_json::from_str(&fixture).expect("fixture should be valid json array");

    let output = sample_html_simple_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::JsonLines)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::JsonLines,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("json-lines output should be generated");

    let actual_entries = String::from_utf8(bytes)
        .expect("output should be utf-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("line should be valid json"))
        .collect::<Vec<_>>();

    let expected_files = extract_jsonlines_file_semantics(&expected_entries);
    let actual_files = extract_jsonlines_file_semantics(&actual_entries);
    assert_eq!(actual_files, expected_files);

    let expected_header = jsonlines_header(&expected_entries);
    let actual_header = jsonlines_header(&actual_entries);
    assert_eq!(
        header_output_format_version(actual_header),
        header_output_format_version(expected_header)
    );
}

#[test]
fn test_yaml_matches_local_fixture_file_semantics() {
    let output = sample_html_simple_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::Yaml)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::Yaml,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("yaml output should be generated");

    let actual_yaml = String::from_utf8(bytes).expect("yaml output should be utf-8");
    let expected_yaml = fs::read_to_string("testdata/output-formats/yaml-simple-expected.yaml")
        .expect("yaml fixture should be readable");

    let actual_yaml_value: yaml_serde::Value =
        yaml_serde::from_str(&actual_yaml).expect("actual yaml should parse");
    let expected_yaml_value: yaml_serde::Value =
        yaml_serde::from_str(&expected_yaml).expect("fixture yaml should parse");

    let actual = serde_json::to_value(actual_yaml_value).expect("actual yaml should convert");
    let expected =
        serde_json::to_value(expected_yaml_value).expect("fixture yaml should convert to json");

    let actual_files = extract_top_level_files_semantics(&actual);
    let expected_files = extract_top_level_files_semantics(&expected);
    assert_eq!(actual_files, expected_files);

    assert_eq!(
        header_output_format_version(single_document_header(&actual)),
        header_output_format_version(single_document_header(&expected))
    );
}

#[test]
fn test_html_report_contract_contains_python_style_sections() {
    let output = sample_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::Html)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::Html,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("html output should be generated");

    let rendered = String::from_utf8(bytes).expect("html output should be utf-8");
    assert!(rendered.contains("<!doctype html>"));
    assert!(rendered.contains("Copyrights and Licenses Information"));
    assert!(rendered.contains("File Information"));
    assert!(rendered.contains("Package Information"));
    assert!(rendered.contains("<th>is_script</th>"));
    assert!(rendered.contains("<td>scan</td>"));
}

#[test]
fn test_html_report_matches_local_fixture_after_normalization() {
    let output = sample_html_simple_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::Html)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::Html,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("html output should be generated");

    let actual = String::from_utf8(bytes).expect("html output should be utf-8");
    let expected =
        fs::read_to_string("testdata/output-formats/html-templated-simple-expected.html")
            .expect("html fixture should be readable");

    assert_eq!(normalize_html(&actual), normalize_html(&expected));
}

#[test]
fn test_spdx_simple_contract_matches_local_python_fixture_after_normalization() {
    let output = sample_spdx_simple_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::SpdxTv)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::SpdxTv,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("spdx output should be generated");

    let actual = String::from_utf8(bytes).expect("spdx should be utf-8");
    let expected = fs::read_to_string("testdata/output-formats/spdx-simple-expected.tv")
        .expect("fixture should be readable");

    assert_eq!(normalize_spdx_tv(&actual), normalize_spdx_tv(&expected));
}

#[test]
fn test_spdx_rdf_contract_contains_python_semantic_markers() {
    let output = sample_spdx_simple_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::SpdxRdf)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::SpdxRdf,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("spdx rdf output should be generated");

    let rendered = String::from_utf8(bytes).expect("spdx rdf should be utf-8");
    let fixture = fs::read_to_string("testdata/output-formats/spdx-simple-expected.rdf")
        .expect("spdx rdf fixture should be readable");
    let expected: Value = serde_json::from_str(&fixture).expect("spdx rdf fixture should be json");

    let package = &expected["rdf:RDF"]["spdx:Package"];
    let relationship = &package["spdx:relationship"]["spdx:Relationship"];
    let file = &relationship["spdx:relatedSpdxElement"]["spdx:File"];
    let doc = &expected["rdf:RDF"]["spdx:SpdxDocument"];

    let expected_verification_code = package["spdx:packageVerificationCode"]
        ["spdx:PackageVerificationCode"]["spdx:packageVerificationCodeValue"]
        .as_str()
        .expect("fixture must contain package verification code");
    let expected_files_analyzed_datatype = package["spdx:filesAnalyzed"]["@rdf:datatype"]
        .as_str()
        .expect("fixture must contain filesAnalyzed datatype");
    let expected_files_analyzed_text = package["spdx:filesAnalyzed"]["#text"]
        .as_str()
        .expect("fixture must contain filesAnalyzed text");
    let expected_package_about = package["@rdf:about"]
        .as_str()
        .expect("fixture must contain package about id");
    let expected_download_location = package["spdx:downloadLocation"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain download location");
    let expected_package_license_concluded = package["spdx:licenseConcluded"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain package license concluded");
    let expected_package_license_declared = package["spdx:licenseDeclared"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain package license declared");
    let expected_package_license_info_from_files =
        package["spdx:licenseInfoFromFiles"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain package license info from files");
    let expected_relationship_type = relationship["spdx:relationshipType"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain relationship type");
    let expected_file_about = file["@rdf:about"]
        .as_str()
        .expect("fixture must contain file about id");
    let expected_file_license_concluded = file["spdx:licenseConcluded"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain file license concluded");
    let expected_file_license_info_in_file = file["spdx:licenseInfoInFile"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain file license info in file");
    let expected_checksum_algorithm =
        file["spdx:checksum"]["spdx:Checksum"]["spdx:algorithm"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain checksum algorithm");
    let expected_file_sha1 = file["spdx:checksum"]["spdx:Checksum"]["spdx:checksumValue"]
        .as_str()
        .expect("fixture must contain file sha1");
    let expected_file_name = file["spdx:fileName"]
        .as_str()
        .expect("fixture must contain file name");
    let expected_file_copyright = file["spdx:copyrightText"]
        .as_str()
        .expect("fixture must contain file copyright");
    let expected_package_copyright = package["spdx:copyrightText"]
        .as_str()
        .expect("fixture must contain package copyright");
    let expected_package_name = package["spdx:name"]
        .as_str()
        .expect("fixture must contain package name");
    let expected_document_about = doc["@rdf:about"]
        .as_str()
        .expect("fixture must contain document about id");
    let expected_data_license = doc["spdx:dataLicense"]["@rdf:resource"]
        .as_str()
        .expect("fixture must contain data license");
    let expected_document_name = doc["spdx:name"]
        .as_str()
        .expect("fixture must contain document name");
    let expected_comment_prefix = doc["rdfs:comment"]
        .as_str()
        .expect("fixture must contain document comment")
        .lines()
        .next()
        .expect("fixture comment must have at least one line");
    let expected_xmlns_rdf = expected["rdf:RDF"]["@xmlns:rdf"]
        .as_str()
        .expect("fixture must contain rdf namespace");
    let expected_xmlns_rdfs = expected["rdf:RDF"]["@xmlns:rdfs"]
        .as_str()
        .expect("fixture must contain rdfs namespace");
    let expected_xmlns_spdx = expected["rdf:RDF"]["@xmlns:spdx"]
        .as_str()
        .expect("fixture must contain spdx namespace");
    let expected_spec_version = doc["spdx:specVersion"]
        .as_str()
        .expect("fixture must contain SPDX version");

    assert!(rendered.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(rendered.contains(&format!("xmlns:rdf=\"{}\"", expected_xmlns_rdf)));
    assert!(rendered.contains(&format!("xmlns:rdfs=\"{}\"", expected_xmlns_rdfs)));
    assert!(rendered.contains(&format!("xmlns:spdx=\"{}\"", expected_xmlns_spdx)));
    assert!(rendered.contains(&format!(
        "<spdx:Package rdf:about=\"{}\">",
        expected_package_about
    )));
    assert!(rendered.contains(&format!(
        "<spdx:filesAnalyzed rdf:datatype=\"{}\">{}</spdx:filesAnalyzed>",
        expected_files_analyzed_datatype, expected_files_analyzed_text
    )));
    assert!(rendered.contains(&format!(
        "<spdx:downloadLocation rdf:resource=\"{}\"/>",
        expected_download_location
    )));
    assert!(rendered.contains(&format!(
        "<spdx:licenseConcluded rdf:resource=\"{}\"/>",
        expected_package_license_concluded
    )));
    assert!(rendered.contains(&format!(
        "<spdx:licenseDeclared rdf:resource=\"{}\"/>",
        expected_package_license_declared
    )));
    assert!(rendered.contains(&format!(
        "<spdx:licenseInfoFromFiles rdf:resource=\"{}\"/>",
        expected_package_license_info_from_files
    )));
    assert!(rendered.contains(&format!(
        "<spdx:relationshipType rdf:resource=\"{}\"/>",
        expected_relationship_type
    )));
    assert!(rendered.contains(&format!(
        "<spdx:File rdf:about=\"{}\">",
        expected_file_about
    )));
    assert!(rendered.contains(&format!(
        "<spdx:licenseConcluded rdf:resource=\"{}\"/>",
        expected_file_license_concluded
    )));
    assert!(rendered.contains(&format!(
        "<spdx:licenseInfoInFile rdf:resource=\"{}\"/>",
        expected_file_license_info_in_file
    )));
    assert!(rendered.contains(&format!(
        "<spdx:algorithm rdf:resource=\"{}\"/>",
        expected_checksum_algorithm
    )));
    assert!(rendered.contains(expected_verification_code));
    assert!(rendered.contains(expected_file_sha1));
    assert!(rendered.contains(expected_file_name));
    assert!(rendered.contains(&format!(
        "<spdx:copyrightText>{}</spdx:copyrightText>",
        expected_file_copyright
    )));
    assert!(rendered.contains(&format!("<spdx:name>{}</spdx:name>", expected_package_name)));
    assert!(rendered.contains(&format!(
        "<spdx:copyrightText>{}</spdx:copyrightText>",
        expected_package_copyright
    )));
    assert!(rendered.contains(&format!(
        "<spdx:SpdxDocument rdf:about=\"{}\">",
        expected_document_about
    )));
    assert!(rendered.contains(expected_data_license));
    assert!(rendered.contains(&xml_escape_for_assert(expected_comment_prefix)));
    assert!(rendered.contains(expected_document_name));
    assert!(rendered.contains(expected_spec_version));
    assert!(rendered.contains("<spdx:creationInfo><spdx:CreationInfo><spdx:created>"));
}

#[test]
fn test_spdx_rdf_semantics_match_fixture_map() {
    let output = sample_spdx_simple_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::SpdxRdf)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::SpdxRdf,
                custom_template: None,
                scanned_path: Some("simple".to_string()),
            },
        )
        .expect("spdx rdf output should be generated");

    let rendered = String::from_utf8(bytes).expect("spdx rdf should be utf-8");
    let fixture = fs::read_to_string("testdata/output-formats/spdx-simple-expected.rdf")
        .expect("spdx rdf fixture should be readable");
    let expected: Value = serde_json::from_str(&fixture).expect("spdx rdf fixture should be json");

    assert_eq!(
        extract_spdx_rdf_semantics(&rendered),
        expected_spdx_rdf_semantics(&expected)
    );
}

#[test]
fn test_cyclonedx_rich_output_contains_enriched_fields_json_and_xml() {
    let output = sample_cyclonedx_rich_output();
    let schema_output = OutputSchemaOutput::from(&output);

    let mut json_bytes = Vec::new();
    writer_for_format(OutputFormat::CycloneDxJson)
        .write(
            &schema_output,
            &mut json_bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxJson,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx json output should be generated");
    let json_value: Value = serde_json::from_slice(&json_bytes).expect("json must parse");
    assert_eq!(json_value["bomFormat"], "CycloneDX");
    assert_eq!(json_value["specVersion"], "1.3");
    assert_eq!(json_value["version"], 1);
    assert!(
        json_value["serialNumber"]
            .as_str()
            .is_some_and(|s| s.starts_with("urn:uuid:"))
    );
    assert_eq!(json_value["metadata"]["tools"][0]["name"], "Provenant");
    let component = &json_value["components"][0];
    assert_eq!(component["name"], "npm");
    assert_eq!(component["description"], "a package manager for JavaScript");
    assert_eq!(component["author"], "Isaac Z. Schlueter");
    assert_eq!(component["scope"], "required");
    assert!(component["hashes"].is_array());
    assert!(component["externalReferences"].is_array());

    let mut xml_bytes = Vec::new();
    writer_for_format(OutputFormat::CycloneDxXml)
        .write(
            &schema_output,
            &mut xml_bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxXml,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx xml output should be generated");
    let xml = String::from_utf8(xml_bytes).expect("xml must be utf-8");
    assert!(xml.contains("<vendor>Provenant</vendor>"));
    assert!(xml.contains("<description>a package manager for JavaScript</description>"));
    assert!(xml.contains("<author>Isaac Z. Schlueter</author>"));
    assert!(xml.contains("<scope>required</scope>"));
    assert!(xml.contains("<hashes>"));
    assert!(xml.contains("<externalReferences>"));
}

#[test]
fn test_cyclonedx_json_matches_local_fixture_after_normalization() {
    let output = sample_cyclonedx_rich_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::CycloneDxJson)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxJson,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx json output should be generated");

    let actual: Value = serde_json::from_slice(&bytes).expect("cyclonedx json should be valid");
    let expected_text = fs::read_to_string("testdata/output-formats/cyclonedx-expected.json")
        .expect("cyclonedx json fixture should be readable");
    let expected: Value =
        serde_json::from_str(&expected_text).expect("cyclonedx json fixture should be valid");

    assert_eq!(normalize_cyclonedx(actual), normalize_cyclonedx(expected));
}

#[test]
fn test_cyclonedx_json_dependency_graph_matches_local_fixture_after_normalization() {
    let output = sample_cyclonedx_dependency_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::CycloneDxJson)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxJson,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx json output should be generated");

    let actual: Value = serde_json::from_slice(&bytes).expect("cyclonedx json should be valid");
    let expected_text =
        fs::read_to_string("testdata/output-formats/cyclonedx-dependencies-expected.json")
            .expect("cyclonedx dependency fixture should be readable");
    let expected: Value = serde_json::from_str(&expected_text)
        .expect("cyclonedx dependency fixture should be valid json");

    assert_eq!(normalize_cyclonedx(actual), normalize_cyclonedx(expected));
}

#[test]
fn test_cyclonedx_xml_dependency_graph_matches_local_fixture_after_normalization() {
    let output = sample_cyclonedx_dependency_output();
    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::CycloneDxXml)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxXml,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx xml output should be generated");

    let actual = String::from_utf8(bytes).expect("cyclonedx xml should be utf-8");
    let expected =
        fs::read_to_string("testdata/output-formats/cyclonedx-dependencies-expected.xml")
            .expect("cyclonedx dependency xml fixture should be readable");

    assert_eq!(
        normalize_cyclonedx_xml(&actual),
        normalize_cyclonedx_xml(&expected)
    );
}

#[test]
fn test_cyclonedx_xml_matches_local_fixture_after_normalization() {
    let mut output = sample_cyclonedx_rich_output();
    let package = output
        .packages
        .first_mut()
        .expect("sample must include one package");
    package.bug_tracking_url = None;
    package.download_url = None;
    package.repository_download_url = None;
    package.homepage_url = None;
    package.repository_homepage_url = None;
    package.vcs_url = None;

    let mut bytes = Vec::new();
    let schema_output = OutputSchemaOutput::from(&output);
    writer_for_format(OutputFormat::CycloneDxXml)
        .write(
            &schema_output,
            &mut bytes,
            &OutputWriteConfig {
                format: OutputFormat::CycloneDxXml,
                custom_template: None,
                scanned_path: Some("scan".to_string()),
            },
        )
        .expect("cyclonedx xml output should be generated");

    let actual = String::from_utf8(bytes).expect("cyclonedx xml should be utf-8");
    let expected = fs::read_to_string("testdata/output-formats/cyclonedx-expected.xml")
        .expect("cyclonedx xml fixture should be readable");

    assert_eq!(
        normalize_cyclonedx_xml(&actual),
        normalize_cyclonedx_xml(&expected)
    );
}

fn normalize_cyclonedx(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("serialNumber");
        if let Some(metadata) = obj.get_mut("metadata").and_then(Value::as_object_mut) {
            metadata.remove("timestamp");
            if let Some(tools) = metadata.get_mut("tools").and_then(Value::as_array_mut) {
                for tool in tools {
                    if let Some(tool_obj) = tool.as_object_mut() {
                        tool_obj.remove("version");
                    }
                }
            }
        }
    }
    value
}

fn normalize_cyclonedx_xml(xml: &str) -> String {
    let bom_open_re = Regex::new(r#"<bom\s+[^>]*>"#).expect("bom regex should compile");
    let serial_re =
        Regex::new(r#"serialNumber=\"urn:uuid:[^\"]+\""#).expect("serial regex should compile");
    let timestamp_re =
        Regex::new(r"<timestamp>[^<]+</timestamp>").expect("timestamp regex should compile");
    let tool_version_re =
        Regex::new(r"(?s)(<metadata>.*?<tools>.*?<tool>.*?<version>)[^<]+(</version>)")
            .expect("tool version regex should compile");
    let inter_tag_ws = Regex::new(r">\s+<").expect("whitespace regex should compile");

    let normalized = bom_open_re.replace_all(
        xml,
        "<bom xmlns=\"http://cyclonedx.org/schema/bom/1.3\" version=\"1\" serialNumber=\"urn:uuid:NORMALIZED\">",
    );
    let normalized = serial_re.replace_all(&normalized, "serialNumber=\"urn:uuid:NORMALIZED\"");
    let normalized = timestamp_re.replace_all(&normalized, "<timestamp>NORMALIZED</timestamp>");
    let normalized = tool_version_re.replace_all(&normalized, "$1NORMALIZED$2");
    inter_tag_ws
        .replace_all(&normalized, "><")
        .trim()
        .to_string()
}

fn normalize_html(html: &str) -> String {
    let caption_re = Regex::new(r"<caption>\s*([^<]+?)\s*</caption>")
        .expect("html caption regex should compile");
    let multi_ws = Regex::new(r"\s+").expect("html collapse whitespace regex should compile");
    let inter_tag_ws =
        Regex::new(r">\s+<").expect("html inter-tag whitespace regex should compile");

    let normalized = html.replace("&#x2F;", "/");
    let normalized = multi_ws.replace_all(&normalized, " ");
    let normalized = inter_tag_ws.replace_all(&normalized, "><").to_string();
    let normalized = caption_re
        .replace_all(&normalized, "<caption>$1</caption>")
        .to_string();
    normalized
        .replace(
            "<td>text/plain</td><td>None</td><td>C</td>",
            "<td>text/plain</td><td>UTF-8 Unicode text, with no line terminators</td><td>C</td>",
        )
        .replace(
            "<td>None</td><td>C</td>",
            "<td>UTF-8 Unicode text, with no line terminators</td><td>C</td>",
        )
        .trim()
        .to_string()
}

fn normalize_spdx_tv(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("Creator:"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn jsonlines_header(entries: &[Value]) -> &Value {
    entries
        .iter()
        .find_map(|entry| {
            entry
                .get("headers")
                .and_then(Value::as_array)
                .and_then(|h| h.first())
        })
        .expect("json-lines entries must contain one headers object")
}

fn single_document_header(document: &Value) -> &Value {
    document
        .get("headers")
        .and_then(Value::as_array)
        .and_then(|h| h.first())
        .expect("yaml/json document must contain one headers object")
}

fn header_output_format_version(header: &Value) -> String {
    header
        .get("output_format_version")
        .and_then(Value::as_str)
        .expect("header must contain output_format_version")
        .to_string()
}

fn extract_jsonlines_file_semantics(entries: &[Value]) -> Vec<BTreeMap<String, String>> {
    let mut rows = Vec::new();
    for entry in entries {
        if let Some(files) = entry.get("files").and_then(Value::as_array) {
            for file in files {
                let mut row = extract_file_semantics(file);
                row.remove("copyright");
                row.remove("holder");
                rows.push(row);
            }
        }
    }
    rows.sort_by(|a, b| a["path"].cmp(&b["path"]));
    rows
}

fn extract_top_level_files_semantics(document: &Value) -> Vec<BTreeMap<String, String>> {
    let mut rows = document
        .get("files")
        .and_then(Value::as_array)
        .expect("document must contain files array")
        .iter()
        .map(extract_file_semantics)
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a["path"].cmp(&b["path"]));
    rows
}

fn extract_file_semantics(file: &Value) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    for key in [
        "path",
        "type",
        "name",
        "base_name",
        "extension",
        "size",
        "mime_type",
        "programming_language",
    ] {
        row.insert(key.to_string(), test_value_to_string(file.get(key)));
    }

    let first_copyright = file
        .get("copyrights")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|v| v.get("copyright").and_then(Value::as_str))
        .unwrap_or("");
    let first_holder = file
        .get("holders")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|v| v.get("holder").and_then(Value::as_str))
        .unwrap_or("");
    row.insert("copyright".to_string(), first_copyright.to_string());
    row.insert("holder".to_string(), first_holder.to_string());

    row
}

fn test_value_to_string(value: Option<&Value>) -> String {
    match value.unwrap_or(&Value::Null) {
        Value::Null => String::new(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        _ => String::new(),
    }
}

fn xml_escape_for_assert(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn extract_spdx_rdf_semantics(xml: &str) -> BTreeMap<String, String> {
    let rdf_root_re = Regex::new(r#"<rdf:RDF\s+([^>]+)>"#).expect("rdf root regex should compile");
    let files_analyzed_re =
        Regex::new(r#"<spdx:filesAnalyzed rdf:datatype=\"([^\"]+)\">([^<]+)</spdx:filesAnalyzed>"#)
            .expect("filesAnalyzed regex should compile");
    let package_about_re =
        Regex::new(r#"<spdx:Package rdf:about=\"([^\"]+)\">"#).expect("package about regex");
    let download_location_re = Regex::new(r#"<spdx:downloadLocation rdf:resource=\"([^\"]+)\"/>"#)
        .expect("download location regex should compile");
    let relationship_type_re = Regex::new(r#"<spdx:relationshipType rdf:resource=\"([^\"]+)\"/>"#)
        .expect("relationship type regex should compile");
    let file_about_re =
        Regex::new(r#"<spdx:File rdf:about=\"([^\"]+)\">"#).expect("file about regex");
    let package_verification_code_re = Regex::new(
        r#"<spdx:packageVerificationCodeValue>([^<]+)</spdx:packageVerificationCodeValue>"#,
    )
    .expect("verification code regex should compile");
    let checksum_algorithm_re = Regex::new(r#"<spdx:algorithm rdf:resource=\"([^\"]+)\"/>"#)
        .expect("checksum algorithm regex should compile");
    let checksum_value_re = Regex::new(r#"<spdx:checksumValue>([^<]+)</spdx:checksumValue>"#)
        .expect("checksum value regex should compile");
    let file_name_re =
        Regex::new(r#"<spdx:fileName>([^<]+)</spdx:fileName>"#).expect("file name regex");
    let package_name_re = Regex::new(r#"(?s)<spdx:Package[^>]*>.*?<spdx:name>([^<]+)</spdx:name>"#)
        .expect("package name regex should compile");
    let document_about_re = Regex::new(r#"<spdx:SpdxDocument rdf:about=\"([^\"]+)\">"#)
        .expect("document about regex should compile");
    let data_license_re = Regex::new(r#"<spdx:dataLicense rdf:resource=\"([^\"]+)\"/>"#)
        .expect("data license regex should compile");
    let document_name_re =
        Regex::new(r#"(?s)<spdx:SpdxDocument[^>]*>.*?<spdx:name>([^<]+)</spdx:name>"#)
            .expect("document name regex should compile");
    let spec_version_re =
        Regex::new(r#"<spdx:specVersion>([^<]+)</spdx:specVersion>"#).expect("spec version regex");
    let comment_re = Regex::new(r#"(?s)<rdfs:comment>(.*?)</rdfs:comment>"#)
        .expect("comment regex should compile");
    let creation_info_re = Regex::new(r#"<spdx:creationInfo><spdx:CreationInfo><spdx:created>[^<]+</spdx:created></spdx:CreationInfo></spdx:creationInfo>"#)
        .expect("creation info regex should compile");
    let package_license_values_re = Regex::new(
        r#"(?s)<spdx:Package[^>]*>.*?<spdx:licenseConcluded rdf:resource=\"([^\"]+)\"/>.*?<spdx:licenseDeclared rdf:resource=\"([^\"]+)\"/>.*?<spdx:licenseInfoFromFiles rdf:resource=\"([^\"]+)\"/>.*?<spdx:copyrightText>([^<]+)</spdx:copyrightText>.*?<spdx:name>[^<]+</spdx:name>"#,
    )
    .expect("package license values regex should compile");
    let file_license_values_re = Regex::new(
        r#"(?s)<spdx:File[^>]*>.*?<spdx:licenseConcluded rdf:resource=\"([^\"]+)\"/>.*?<spdx:licenseInfoInFile rdf:resource=\"([^\"]+)\"/>.*?<spdx:checksum>.*?</spdx:checksum>.*?<spdx:fileName>[^<]+</spdx:fileName><spdx:copyrightText>([^<]+)</spdx:copyrightText>"#,
    )
    .expect("file license values regex should compile");

    let mut semantics = BTreeMap::new();

    let root_attrs = capture_single(&rdf_root_re, xml, 1, "rdf root attrs");
    semantics.insert(
        "xmlns:rdf".to_string(),
        extract_attr(&root_attrs, "xmlns:rdf").to_string(),
    );
    semantics.insert(
        "xmlns:rdfs".to_string(),
        extract_attr(&root_attrs, "xmlns:rdfs").to_string(),
    );
    semantics.insert(
        "xmlns:spdx".to_string(),
        extract_attr(&root_attrs, "xmlns:spdx").to_string(),
    );

    semantics.insert(
        "package_about".to_string(),
        capture_single(&package_about_re, xml, 1, "package about"),
    );

    let files_analyzed_caps = files_analyzed_re
        .captures(xml)
        .expect("must capture files analyzed semantics");
    semantics.insert(
        "files_analyzed_datatype".to_string(),
        files_analyzed_caps[1].to_string(),
    );
    semantics.insert(
        "files_analyzed_text".to_string(),
        files_analyzed_caps[2].to_string(),
    );

    semantics.insert(
        "download_location".to_string(),
        capture_single(&download_location_re, xml, 1, "download location"),
    );
    semantics.insert(
        "relationship_type".to_string(),
        capture_single(&relationship_type_re, xml, 1, "relationship type"),
    );
    semantics.insert(
        "file_about".to_string(),
        capture_single(&file_about_re, xml, 1, "file about"),
    );
    semantics.insert(
        "package_verification_code".to_string(),
        capture_single(
            &package_verification_code_re,
            xml,
            1,
            "package verification code",
        ),
    );
    semantics.insert(
        "checksum_algorithm".to_string(),
        capture_single(&checksum_algorithm_re, xml, 1, "checksum algorithm"),
    );
    semantics.insert(
        "checksum_value".to_string(),
        capture_single(&checksum_value_re, xml, 1, "checksum value"),
    );
    semantics.insert(
        "file_name".to_string(),
        capture_single(&file_name_re, xml, 1, "file name"),
    );
    semantics.insert(
        "package_name".to_string(),
        capture_single(&package_name_re, xml, 1, "package name"),
    );
    semantics.insert(
        "document_about".to_string(),
        capture_single(&document_about_re, xml, 1, "document about"),
    );
    semantics.insert(
        "data_license".to_string(),
        capture_single(&data_license_re, xml, 1, "data license"),
    );
    semantics.insert(
        "document_name".to_string(),
        capture_single(&document_name_re, xml, 1, "document name"),
    );
    semantics.insert(
        "spec_version".to_string(),
        capture_single(&spec_version_re, xml, 1, "spec version"),
    );

    let package_license_values = package_license_values_re
        .captures(xml)
        .expect("must capture package license values");
    semantics.insert(
        "package_license_concluded".to_string(),
        package_license_values[1].to_string(),
    );
    semantics.insert(
        "package_license_declared".to_string(),
        package_license_values[2].to_string(),
    );
    semantics.insert(
        "package_license_info_from_files".to_string(),
        package_license_values[3].to_string(),
    );
    semantics.insert(
        "package_copyright".to_string(),
        package_license_values[4].to_string(),
    );

    let file_license_values = file_license_values_re
        .captures(xml)
        .expect("must capture file license values");
    semantics.insert(
        "file_license_concluded".to_string(),
        file_license_values[1].to_string(),
    );
    semantics.insert(
        "file_license_info_in_file".to_string(),
        file_license_values[2].to_string(),
    );
    semantics.insert(
        "file_copyright".to_string(),
        file_license_values[3].to_string(),
    );

    let comment_escaped = capture_single(&comment_re, xml, 1, "rdfs comment");
    semantics.insert(
        "document_comment".to_string(),
        xml_unescape_for_assert(&comment_escaped),
    );
    semantics.insert(
        "creation_info_present".to_string(),
        creation_info_re.is_match(xml).to_string(),
    );

    semantics
}

fn expected_spdx_rdf_semantics(expected: &Value) -> BTreeMap<String, String> {
    let package = &expected["rdf:RDF"]["spdx:Package"];
    let relationship = &package["spdx:relationship"]["spdx:Relationship"];
    let file = &relationship["spdx:relatedSpdxElement"]["spdx:File"];
    let doc = &expected["rdf:RDF"]["spdx:SpdxDocument"];

    let mut semantics = BTreeMap::new();
    semantics.insert(
        "xmlns:rdf".to_string(),
        expected["rdf:RDF"]["@xmlns:rdf"]
            .as_str()
            .expect("fixture must contain rdf namespace")
            .to_string(),
    );
    semantics.insert(
        "xmlns:rdfs".to_string(),
        expected["rdf:RDF"]["@xmlns:rdfs"]
            .as_str()
            .expect("fixture must contain rdfs namespace")
            .to_string(),
    );
    semantics.insert(
        "xmlns:spdx".to_string(),
        expected["rdf:RDF"]["@xmlns:spdx"]
            .as_str()
            .expect("fixture must contain spdx namespace")
            .to_string(),
    );
    semantics.insert(
        "package_about".to_string(),
        package["@rdf:about"]
            .as_str()
            .expect("fixture must contain package about")
            .to_string(),
    );
    semantics.insert(
        "files_analyzed_datatype".to_string(),
        package["spdx:filesAnalyzed"]["@rdf:datatype"]
            .as_str()
            .expect("fixture must contain filesAnalyzed datatype")
            .to_string(),
    );
    semantics.insert(
        "files_analyzed_text".to_string(),
        package["spdx:filesAnalyzed"]["#text"]
            .as_str()
            .expect("fixture must contain filesAnalyzed text")
            .to_string(),
    );
    semantics.insert(
        "download_location".to_string(),
        package["spdx:downloadLocation"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain download location")
            .to_string(),
    );
    semantics.insert(
        "package_license_concluded".to_string(),
        package["spdx:licenseConcluded"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain package license concluded")
            .to_string(),
    );
    semantics.insert(
        "package_license_declared".to_string(),
        package["spdx:licenseDeclared"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain package license declared")
            .to_string(),
    );
    semantics.insert(
        "package_license_info_from_files".to_string(),
        package["spdx:licenseInfoFromFiles"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain package license info from files")
            .to_string(),
    );
    semantics.insert(
        "package_verification_code".to_string(),
        package["spdx:packageVerificationCode"]["spdx:PackageVerificationCode"]
            ["spdx:packageVerificationCodeValue"]
            .as_str()
            .expect("fixture must contain package verification code")
            .to_string(),
    );
    semantics.insert(
        "relationship_type".to_string(),
        relationship["spdx:relationshipType"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain relationship type")
            .to_string(),
    );
    semantics.insert(
        "file_about".to_string(),
        file["@rdf:about"]
            .as_str()
            .expect("fixture must contain file about")
            .to_string(),
    );
    semantics.insert(
        "file_license_concluded".to_string(),
        file["spdx:licenseConcluded"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain file license concluded")
            .to_string(),
    );
    semantics.insert(
        "file_license_info_in_file".to_string(),
        file["spdx:licenseInfoInFile"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain file license in file")
            .to_string(),
    );
    semantics.insert(
        "checksum_algorithm".to_string(),
        file["spdx:checksum"]["spdx:Checksum"]["spdx:algorithm"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain checksum algorithm")
            .to_string(),
    );
    semantics.insert(
        "checksum_value".to_string(),
        file["spdx:checksum"]["spdx:Checksum"]["spdx:checksumValue"]
            .as_str()
            .expect("fixture must contain checksum value")
            .to_string(),
    );
    semantics.insert(
        "file_name".to_string(),
        file["spdx:fileName"]
            .as_str()
            .expect("fixture must contain file name")
            .to_string(),
    );
    semantics.insert(
        "file_copyright".to_string(),
        file["spdx:copyrightText"]
            .as_str()
            .expect("fixture must contain file copyright")
            .to_string(),
    );
    semantics.insert(
        "package_copyright".to_string(),
        package["spdx:copyrightText"]
            .as_str()
            .expect("fixture must contain package copyright")
            .to_string(),
    );
    semantics.insert(
        "package_name".to_string(),
        package["spdx:name"]
            .as_str()
            .expect("fixture must contain package name")
            .to_string(),
    );
    semantics.insert(
        "document_about".to_string(),
        doc["@rdf:about"]
            .as_str()
            .expect("fixture must contain document about")
            .to_string(),
    );
    semantics.insert(
        "data_license".to_string(),
        doc["spdx:dataLicense"]["@rdf:resource"]
            .as_str()
            .expect("fixture must contain data license")
            .to_string(),
    );
    semantics.insert(
        "document_comment".to_string(),
        doc["rdfs:comment"]
            .as_str()
            .expect("fixture must contain document comment")
            .to_string(),
    );
    semantics.insert(
        "document_name".to_string(),
        doc["spdx:name"]
            .as_str()
            .expect("fixture must contain document name")
            .to_string(),
    );
    semantics.insert(
        "spec_version".to_string(),
        doc["spdx:specVersion"]
            .as_str()
            .expect("fixture must contain spec version")
            .to_string(),
    );
    semantics.insert("creation_info_present".to_string(), "true".to_string());
    semantics
}

fn capture_single(re: &Regex, text: &str, group: usize, label: &str) -> String {
    re.captures(text)
        .and_then(|caps| caps.get(group).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| panic!("missing {} in rendered xml", label))
}

fn extract_attr<'a>(attrs: &'a str, name: &str) -> &'a str {
    let pattern = format!("{}=\"", name);
    let start = attrs
        .find(&pattern)
        .unwrap_or_else(|| panic!("missing attribute {}", name))
        + pattern.len();
    let end_rel = attrs[start..]
        .find('"')
        .unwrap_or_else(|| panic!("unterminated attribute {}", name));
    &attrs[start..start + end_rel]
}

fn xml_unescape_for_assert(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn sample_header(files_count: usize, directories_count: usize) -> Header {
    Header {
        start_timestamp: "2026-01-01T00:00:00Z".to_string(),
        end_timestamp: "2026-01-01T00:00:01Z".to_string(),
        duration: 1.0,
        errors: vec![],
        output_format_version: "4.0.0".to_string(),
        extra_data: ExtraData {
            files_count,
            directories_count,
            excluded_count: 0,
            system_environment: SystemEnvironment {
                operating_system: Some("linux".to_string()),
                cpu_architecture: "64".to_string(),
                platform: "linux".to_string(),
                rust_version: "1.93.0".to_string(),
            },
        },
    }
}

fn sample_output_with_sections(
    files_count: usize,
    directories_count: usize,
    packages: Vec<Package>,
    dependencies: Vec<TopLevelDependency>,
    files: Vec<FileInfo>,
) -> Output {
    Output {
        summary: None,
        tallies: None,
        tallies_of_key_files: None,
        tallies_by_facet: None,
        headers: vec![sample_header(files_count, directories_count)],
        packages,
        dependencies,
        license_detections: vec![],
        files,
        license_references: vec![],
        license_rule_references: vec![],
    }
}

fn sample_directory_file(path: &str) -> FileInfo {
    FileInfo::new(
        path.to_string(),
        path.to_string(),
        "".to_string(),
        path.to_string(),
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
    )
}

fn sample_plain_text_file(
    name: &str,
    base_name: &str,
    extension: &str,
    path: &str,
    size: u64,
    sha1: &str,
    package_data: Vec<PackageData>,
) -> FileInfo {
    FileInfo::new(
        name.to_string(),
        base_name.to_string(),
        extension.to_string(),
        path.to_string(),
        FileType::File,
        Some("text/plain".to_string()),
        None,
        size,
        None,
        Some(Sha1Digest::from_hex(sha1).unwrap()),
        None,
        None,
        None,
        package_data,
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
    )
}

fn sample_output() -> Output {
    sample_output_with_sections(
        1,
        1,
        vec![],
        vec![],
        vec![
            sample_directory_file("scan"),
            sample_plain_text_file(
                "test.txt",
                "test",
                ".txt",
                "scan/test.txt",
                10,
                "b8a793cce3c3a4cd3a4646ddbe86edd542ed0cd8",
                vec![PackageData::default()],
            ),
        ],
    )
}

fn sample_html_simple_output() -> Output {
    let mut file = FileInfo::new(
        "copyright_acme_c-c.c".to_string(),
        "copyright_acme_c-c".to_string(),
        ".c".to_string(),
        "simple/copyright_acme_c-c.c".to_string(),
        FileType::File,
        Some("text/plain".to_string()),
        None,
        55,
        None,
        Some(Sha1Digest::from_hex("e2466d5b764d27fb301ceb439ffb5da22e43ab1d").unwrap()),
        Some(Md5Digest::from_hex("bdf7c572beb4094c2059508fa73c05a4").unwrap()),
        None,
        Some("C".to_string()),
        vec![],
        None,
        vec![],
        vec![],
        vec![Copyright {
            copyright: "Copyright (c) 2000 ACME, Inc.".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        }],
        vec![Holder {
            holder: "ACME, Inc.".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        }],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    file.is_binary = Some(false);
    file.is_text = Some(true);
    file.is_archive = Some(false);
    file.is_media = Some(false);
    file.is_source = Some(true);
    file.is_script = Some(false);

    sample_output_with_sections(
        1,
        1,
        vec![],
        vec![],
        vec![sample_directory_file("simple"), file],
    )
}

fn sample_spdx_simple_output() -> Output {
    sample_output_with_sections(
        1,
        0,
        vec![],
        vec![],
        vec![sample_plain_text_file(
            "test.txt",
            "test",
            ".txt",
            "test.txt",
            10,
            "b8a793cce3c3a4cd3a4646ddbe86edd542ed0cd8",
            vec![],
        )],
    )
}

fn sample_cyclonedx_rich_output() -> Output {
    let mut pkg_data = PackageData {
        package_type: Some(PackageType::Npm),
        name: Some("npm".to_string()),
        version: Some("2.13.5".to_string()),
        description: Some("a package manager for JavaScript".to_string()),
        purl: Some("pkg:npm/npm@2.13.5".to_string()),
        sha1: Some(Sha1Digest::from_hex("a124386bce4a90506f28ad4b1d1a804a17baaf32").unwrap()),
        declared_license_expression_spdx: Some("Artistic-2.0".to_string()),
        homepage_url: Some("https://docs.npmjs.com/".to_string()),
        repository_homepage_url: Some("https://www.npmjs.com/package/npm".to_string()),
        download_url: Some("https://registry.npmjs.org/npm/-/npm-2.13.5.tgz".to_string()),
        repository_download_url: Some(
            "https://registry.npmjs.org/npm/-/npm-2.13.5.tgz".to_string(),
        ),
        api_data_url: Some("https://registry.npmjs.org/npm/2.13.5".to_string()),
        vcs_url: Some(
            "git+https://github.com/npm/npm.git@fc7bbf03e39cc48a8924b90696d28345a6a90f3c"
                .to_string(),
        ),
        bug_tracking_url: Some("http://github.com/npm/npm/issues".to_string()),
        ..Default::default()
    };
    pkg_data.parties = vec![Party {
        r#type: None,
        role: Some("author".to_string()),
        name: Some("Isaac Z. Schlueter".to_string()),
        email: None,
        url: None,
        organization: None,
        organization_url: None,
        timezone: None,
    }];

    let package = Package::from_package_data(&pkg_data, "package.json".to_string());

    sample_output_with_sections(1, 0, vec![package], vec![], vec![])
}

fn sample_cyclonedx_dependency_output() -> Output {
    let root_dep = TopLevelDependency {
        purl: Some("pkg:npm/root@1.0.0".to_string()),
        extracted_requirement: None,
        scope: Some("dependencies".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(true),
        resolved_package: Some(Box::new(ResolvedPackage {
            primary_language: None,
            download_url: None,
            sha1: None,
            sha256: None,
            sha512: None,
            md5: None,
            is_virtual: false,
            extra_data: None,
            dependencies: vec![],
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: None,
            purl: Some("pkg:npm/dep@2.0.0".to_string()),
            ..ResolvedPackage::new(
                PackageType::Npm,
                String::new(),
                "dep".to_string(),
                "2.0.0".to_string(),
            )
        })),
        extra_data: None,
        dependency_uid: "pkg:npm/root@1.0.0?uuid=00000000-0000-0000-0000-000000000001".to_string(),
        for_package_uid: Some(
            "pkg:npm/root-package@1.0.0?uuid=00000000-0000-0000-0000-000000000000".to_string(),
        ),
        datafile_path: "scan/package-lock.json".to_string(),
        datasource_id: DatasourceId::NpmPackageLockJson,
        namespace: None,
    };

    let fallback_dep = TopLevelDependency {
        purl: None,
        extracted_requirement: Some("^3.0.0".to_string()),
        scope: Some("devDependencies".to_string()),
        is_runtime: Some(false),
        is_optional: Some(false),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
        dependency_uid: String::new(),
        for_package_uid: Some(
            "pkg:npm/root-package@1.0.0?uuid=00000000-0000-0000-0000-000000000000".to_string(),
        ),
        datafile_path: "scan/package-lock.json".to_string(),
        datasource_id: DatasourceId::NpmPackageLockJson,
        namespace: None,
    };

    sample_output_with_sections(0, 0, vec![], vec![root_dep, fallback_dep], vec![])
}

fn empty_output() -> Output {
    Output {
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
    }
}
