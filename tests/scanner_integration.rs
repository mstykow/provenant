use glob::Pattern;
use provenant::license_detection::{LicenseDetectionEngine, SCANCODE_LICENSES_DATA_PATH};
use provenant::models::PackageType;
use provenant::parsers::list_parser_types;
use provenant::progress::{ProgressMode, ScanProgress};
use provenant::scanner::LicenseScanOptions;
use provenant::utils::file::{ExtractedTextKind, extract_text_for_detection};
use provenant::utils::hash::calculate_sha256;
use provenant::{FileType, TextDetectionOptions, collect_paths, process_collected};
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn create_license_detection_engine() -> Option<Arc<LicenseDetectionEngine>> {
    let data_path = Path::new(SCANCODE_LICENSES_DATA_PATH);
    if !data_path.exists() {
        eprintln!("Reference data not available at {:?}", data_path);
        return None;
    }
    match LicenseDetectionEngine::from_directory(data_path) {
        Ok(engine) => {
            eprintln!("License detection engine initialized for tests");
            Some(Arc::new(engine))
        }
        Err(e) => {
            eprintln!("Failed to create engine: {:?}", e);
            None
        }
    }
}

fn hidden_progress() -> Arc<ScanProgress> {
    Arc::new(ScanProgress::new(ProgressMode::Quiet))
}

fn package_scan_options() -> TextDetectionOptions {
    TextDetectionOptions {
        collect_info: false,
        detect_packages: true,
        detect_application_packages: true,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        ..TextDetectionOptions::default()
    }
}

fn scan<P: AsRef<Path>>(
    path: P,
    max_depth: usize,
    patterns: &[Pattern],
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    include_text: bool,
    options: Option<&TextDetectionOptions>,
) -> provenant::scanner::ProcessResult {
    let progress = hidden_progress();
    let collected = collect_paths(path.as_ref(), max_depth, patterns);
    let _ = include_text;
    process_collected(
        &collected,
        progress,
        license_engine,
        LicenseScanOptions::default(),
        options.unwrap_or(&TextDetectionOptions::default()),
    )
}

fn build_text_pdf(lines: &[&str]) -> Vec<u8> {
    fn escape_pdf_text(text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    }

    let mut content = String::from("BT\n/F1 12 Tf\n72 720 Td\n");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            content.push_str("0 -16 Td\n");
        }
        content.push_str(&format!("({}) Tj\n", escape_pdf_text(line)));
    }
    content.push_str("ET\n");

    let objects = [
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n".to_string(),
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
            content.len(),
            content
        ),
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
            .to_string(),
    ];

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for object in objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object.as_bytes());
    }

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }

    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_offset
        )
        .as_bytes(),
    );

    pdf
}

fn build_exif_block(field_tag: exif::Tag, value: &str) -> Vec<u8> {
    use exif::experimental::Writer;
    use exif::{Field, In, Value};
    use std::io::Cursor;

    let field = Field {
        tag: field_tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![value.as_bytes().to_vec()]),
    };

    let mut writer = Writer::new();
    writer.push_field(&field);

    let mut cursor = Cursor::new(Vec::new());
    writer
        .write(&mut cursor, false)
        .expect("Failed to encode EXIF metadata");
    cursor.into_inner()
}

fn build_png_with_metadata(exif: Option<Vec<u8>>, xmp: Option<&str>) -> Vec<u8> {
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    let mut png = Vec::new();
    {
        let mut encoder = PngEncoder::new(&mut png);
        if let Some(exif) = exif {
            encoder
                .set_exif_metadata(exif)
                .expect("PNG encoder should support EXIF metadata");
        }
        encoder
            .write_image(&[255, 255, 255], 1, 1, ExtendedColorType::Rgb8)
            .expect("Failed to encode PNG");
    }

    if let Some(xmp) = xmp {
        png = insert_png_chunk(&png, *b"iTXt", build_png_xmp_chunk_data(xmp.as_bytes()));
    }

    png
}

fn build_jpeg_with_exif(exif: Vec<u8>) -> Vec<u8> {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 100);
    encoder
        .set_exif_metadata(exif)
        .expect("JPEG encoder should support EXIF metadata");
    encoder
        .write_image(&[255, 255, 255], 1, 1, ExtendedColorType::Rgb8)
        .expect("Failed to encode JPEG");
    jpeg
}

fn build_webp_with_exif(exif: Vec<u8>) -> Vec<u8> {
    use image::codecs::webp::WebPEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    let mut webp = Vec::new();
    let mut encoder = WebPEncoder::new_lossless(&mut webp);
    encoder
        .set_exif_metadata(exif)
        .expect("WebP encoder should support EXIF metadata");
    encoder
        .write_image(&[255, 255, 255], 1, 1, ExtendedColorType::Rgb8)
        .expect("Failed to encode WebP");
    webp
}

fn build_tiff_with_exif(exif: Vec<u8>) -> Vec<u8> {
    exif
}

fn build_image_with_exif(extension: &str, exif: Vec<u8>) -> Vec<u8> {
    match extension {
        "png" => build_png_with_metadata(Some(exif), None),
        "jpg" => build_jpeg_with_exif(exif),
        "tiff" => build_tiff_with_exif(exif),
        "webp" => build_webp_with_exif(exif),
        other => panic!("unsupported image extension: {other}"),
    }
}

fn build_png_xmp_chunk_data(xmp: &[u8]) -> Vec<u8> {
    let mut data = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
    data.extend_from_slice(xmp);
    data
}

fn insert_png_chunk(png: &[u8], chunk_type: [u8; 4], data: Vec<u8>) -> Vec<u8> {
    const PNG_SIGNATURE_LEN: usize = 8;

    assert!(
        png.len() >= PNG_SIGNATURE_LEN,
        "PNG should contain signature"
    );
    let mut insert_at = png.len();
    let mut offset = PNG_SIGNATURE_LEN;

    while offset + 12 <= png.len() {
        let length = u32::from_be_bytes([
            png[offset],
            png[offset + 1],
            png[offset + 2],
            png[offset + 3],
        ]) as usize;
        let chunk_end = offset + 12 + length;
        assert!(chunk_end <= png.len(), "PNG chunk should be in bounds");

        if &png[offset + 4..offset + 8] == b"IEND" {
            insert_at = offset;
            break;
        }

        offset = chunk_end;
    }

    let mut out = Vec::with_capacity(png.len() + data.len() + 12);
    out.extend_from_slice(&png[..insert_at]);
    append_png_chunk(&mut out, chunk_type, &data);
    out.extend_from_slice(&png[insert_at..]);
    out
}

fn append_png_chunk(out: &mut Vec<u8>, chunk_type: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&chunk_type);
    out.extend_from_slice(data);

    let mut crc_input = Vec::with_capacity(chunk_type.len() + data.len());
    crc_input.extend_from_slice(&chunk_type);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&png_crc32(&crc_input).to_be_bytes());
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg() & 0xedb8_8320;
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

#[test]
fn test_scanner_discovers_all_registered_parsers() {
    let test_dir = "testdata/integration/multi-parser";
    let patterns: Vec<Pattern> = vec![];
    let options = package_scan_options();

    let result = scan(test_dir, 50, &patterns, None, false, Some(&options));

    // Should find 3 files with package data (npm, python, cargo)
    let package_files: Vec<_> = result
        .files
        .iter()
        .filter(|f| f.file_type == FileType::File && !f.package_data.is_empty())
        .collect();

    assert_eq!(
        package_files.len(),
        3,
        "Should find all 3 package manifests, found: {:?}",
        package_files.iter().map(|f| &f.name).collect::<Vec<_>>()
    );

    // Verify each parser was invoked
    let has_npm = package_files
        .iter()
        .any(|f| f.package_data[0].package_type == Some(PackageType::Npm));
    let has_pypi = package_files
        .iter()
        .any(|f| f.package_data[0].package_type == Some(PackageType::Pypi));
    let has_cargo = package_files
        .iter()
        .any(|f| f.package_data[0].package_type == Some(PackageType::Cargo));

    assert!(has_npm, "NpmParser should be invoked");
    assert!(has_pypi, "PythonParser should be invoked");
    assert!(has_cargo, "CargoParser should be invoked");
}

#[test]
fn test_full_output_format_structure() {
    let test_dir = "testdata/integration/multi-parser";
    let patterns: Vec<Pattern> = vec![];
    let options = package_scan_options();

    let result = scan(test_dir, 50, &patterns, None, false, Some(&options));

    // Verify basic structure
    assert!(!result.files.is_empty(), "Should have files in result");

    // Verify each file has required fields
    for file in &result.files {
        if file.file_type == FileType::File {
            assert!(!file.name.is_empty(), "File should have name");
            assert!(!file.path.is_empty(), "File should have path");
            assert!(file.sha1.is_some(), "File should have SHA1 hash");
            assert!(file.md5.is_some(), "File should have MD5 hash");
            assert!(file.sha256.is_some(), "File should have SHA256 hash");
            assert!(
                file.mime_type.is_some(),
                "File should have mime type for: {}",
                file.name
            );
            assert!(file.size > 0, "File should have size for: {}", file.name);
        }
    }

    // Verify package files have package_data
    let package_files: Vec<_> = result
        .files
        .iter()
        .filter(|f| {
            matches!(
                f.name.as_str(),
                "package.json" | "pyproject.toml" | "Cargo.toml"
            )
        })
        .collect();

    assert_eq!(package_files.len(), 3, "Should find all 3 manifest files");

    for file in package_files {
        assert!(
            !file.package_data.is_empty(),
            "Manifest file {} should have package_data",
            file.name
        );
        let pkg = &file.package_data[0];
        assert!(pkg.package_type.is_some(), "Should have package type");
        assert!(pkg.name.is_some(), "Should have package name");
        assert!(pkg.version.is_some(), "Should have package version");
    }
}

#[test]
fn test_scanner_handles_empty_directory() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();

    let patterns: Vec<Pattern> = vec![];

    let package_options = package_scan_options();
    let result = scan(
        test_path,
        50,
        &patterns,
        None,
        false,
        Some(&package_options),
    );

    // Should have no files (only the directory entry might be present)
    let file_count = result
        .files
        .iter()
        .filter(|f| f.file_type == FileType::File)
        .count();
    assert_eq!(file_count, 0, "Empty directory should have no files");
}

#[test]
fn test_scanner_handles_parse_errors_gracefully() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();

    let malformed_json = test_path.join("package.json");
    fs::write(&malformed_json, "{ this is not valid json }").expect("Failed to write test file");

    let patterns: Vec<Pattern> = vec![];

    // Scan should complete without crashing
    let package_options = package_scan_options();
    let result = scan(
        test_path,
        50,
        &patterns,
        None,
        false,
        Some(&package_options),
    );

    // Should find the file
    let json_file = result
        .files
        .iter()
        .find(|f| f.name == "package.json")
        .expect("Should find package.json file");

    assert!(
        !json_file.scan_errors.is_empty(),
        "Malformed file should surface parser failures in scan_errors"
    );
}

#[test]
fn test_exclusion_patterns_filter_correctly() {
    let test_dir = "testdata/integration/multi-parser";

    let patterns: Vec<Pattern> = vec![Pattern::new("*.toml").expect("Invalid pattern")];

    let result = scan(test_dir, 50, &patterns, None, false, None);

    // Should not find any .toml files
    let toml_files: Vec<_> = result
        .files
        .iter()
        .filter(|f| f.name.ends_with(".toml"))
        .collect();

    assert!(
        toml_files.is_empty(),
        "Should not find .toml files, but found: {:?}",
        toml_files.iter().map(|f| &f.name).collect::<Vec<_>>()
    );

    // Should still find .json file
    let has_json = result.files.iter().any(|f| f.name == "package.json");
    assert!(has_json, "Should still find package.json");

    // Check excluded count
    assert!(
        result.excluded_count > 0,
        "Should have excluded at least one file"
    );
}

#[test]
fn test_max_depth_limits_traversal() {
    use std::fs;
    use tempfile::TempDir;

    let engine = create_license_detection_engine();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();

    let level1 = test_path.join("level1");
    let level2 = level1.join("level2");
    fs::create_dir_all(&level2).expect("Failed to create nested dirs");

    let deep_file = level2.join("package.json");
    fs::write(&deep_file, r#"{"name": "deep", "version": "1.0.0"}"#)
        .expect("Failed to write test file");

    let patterns: Vec<Pattern> = vec![];

    // Scan with max_depth=1 (should not reach level2)
    let result = scan(test_path, 1, &patterns, None, false, None);

    // Should not find the deep package.json
    let has_deep_json = result.files.iter().any(|f| f.name == "package.json");
    assert!(!has_deep_json, "Should not find package.json at depth > 1");

    let unlimited_result = scan(test_path, 0, &patterns, engine, false, None);
    let has_deep_json_unlimited = unlimited_result
        .files
        .iter()
        .any(|f| f.name == "package.json");
    assert!(
        has_deep_json_unlimited,
        "max_depth=0 should scan recursively without depth limit"
    );
}

/// Regression test: Verify that all parsers in register_package_handlers! macro are actually
/// exported and accessible. This catches bugs where parsers are implemented but
/// not registered in the macro (like CargoLockParser was before being fixed).
#[test]
fn test_all_parsers_are_registered_and_exported() {
    // Get list of all parser types from the macro
    let parser_types = list_parser_types();

    // This test verifies that list_parser_types() returns a non-empty list
    // If a parser is implemented but not in register_package_handlers!, it won't appear here
    assert!(
        !parser_types.is_empty(),
        "Should have at least one parser registered"
    );

    // Known parsers that should be present (sample check)
    let expected_parsers = vec![
        "NpmParser",
        "NpmLockParser",
        "CargoParser",
        "CargoLockParser", // This was missing before the fix
        "PythonParser",
        "ComposerLockParser",
        "YarnLockParser",
        "PnpmLockParser",
        "PoetryLockParser",
    ];

    for expected in expected_parsers {
        assert!(
            parser_types.contains(&expected),
            "Parser '{}' should be registered in register_package_handlers! macro",
            expected
        );
    }

    // Verify we have a reasonable number of parsers (40+ formats supported)
    // If this number is suspiciously low, it indicates missing registrations
    assert!(
        parser_types.len() >= 40,
        "Expected at least 40 parsers, found {}. Some parsers may not be registered.",
        parser_types.len()
    );
}

#[test]
fn test_scanner_detects_emails_and_urls_when_enabled() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("contacts.txt");
    fs::write(
        &content_path,
        "mail us at support@many.org\nvisit www.acme.dev/docs\n",
    )
    .expect("Failed to write test file");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("contacts.txt"))
        .expect("Should find contacts file");

    assert_eq!(file.emails.len(), 1);
    assert_eq!(file.emails[0].email, "support@many.org");
    assert_eq!(file.emails[0].start_line, 1);
    assert_eq!(file.emails[0].end_line, 1);

    assert_eq!(file.urls.len(), 1);
    assert_eq!(file.urls[0].url, "http://www.acme.dev/docs");
    assert_eq!(file.urls[0].start_line, 2);
    assert_eq!(file.urls[0].end_line, 2);
}

#[test]
fn test_scanner_detects_copyrights_in_latin1_text() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("latin1_notice.txt");
    let content = b"Copyright 2024 Fran\xe7ois M\xfcller\n";
    fs::write(&content_path, content).expect("Failed to write Latin-1 test file");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("latin1_notice.txt"))
        .expect("Should find Latin-1 notice file");

    assert_eq!(file.copyrights.len(), 1);
    assert_eq!(
        file.copyrights[0].copyright,
        "Copyright 2024 François Müller"
    );
    assert_eq!(file.copyrights[0].start_line, 1);
    assert_eq!(file.copyrights[0].end_line, 1);

    assert_eq!(file.holders.len(), 1);
    assert_eq!(file.holders[0].holder, "François Müller");
    assert_eq!(file.holders[0].start_line, 1);
    assert_eq!(file.holders[0].end_line, 1);
}

#[test]
fn test_scanner_detects_copyrights_in_pdf_text() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("notice.pdf");
    let pdf = build_text_pdf(&["Copyright 2024 Example Corp.", "All rights reserved."]);
    fs::write(&content_path, pdf).expect("Failed to write PDF fixture");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("notice.pdf"))
        .expect("Should find PDF file");

    assert!(
        file.copyrights
            .iter()
            .any(|c| c.copyright == "Copyright 2024 Example Corp."),
        "copyrights: {:?}",
        file.copyrights
    );
    assert!(
        file.holders.iter().any(|h| h.holder == "Example Corp."),
        "holders: {:?}",
        file.holders
    );
}

#[test]
fn test_scanner_detects_emails_and_urls_in_pdf_text() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("contacts.pdf");
    let pdf = build_text_pdf(&["Reach us at legal@acme.org", "https://acme.org/support"]);
    let (extracted_text, _) =
        provenant::utils::file::extract_text_for_detection(&content_path, &pdf);
    fs::write(&content_path, pdf).expect("Failed to write PDF fixture");

    assert!(
        extracted_text.contains("legal@acme.org"),
        "extracted_text: {:?}",
        extracted_text
    );
    assert!(
        extracted_text.contains("https://acme.org/support"),
        "extracted_text: {:?}",
        extracted_text
    );

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("contacts.pdf"))
        .expect("Should find PDF file");

    assert!(
        file.emails
            .iter()
            .any(|email| email.email == "legal@acme.org"),
        "emails: {:?}",
        file.emails
    );
    assert!(
        file.urls
            .iter()
            .any(|url| url.url == "https://acme.org/support"),
        "urls: {:?}",
        file.urls
    );
}

#[test]
fn test_scanner_detects_copyrights_in_supported_image_exif_containers() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    for extension in ["png", "jpg", "tiff", "webp"] {
        let content_path = test_path.join(format!("photo.{extension}"));
        let image = build_image_with_exif(
            extension,
            build_exif_block(exif::Tag::Copyright, "Copyright 2026 Example Corp."),
        );
        let (extracted_text, kind) = extract_text_for_detection(&content_path, &image);
        fs::write(&content_path, image).expect("Failed to write image fixture");

        assert_eq!(kind, ExtractedTextKind::ImageMetadata);
        assert!(
            extracted_text.contains("Copyright 2026 Example Corp."),
            "{extension} extracted_text: {:?}",
            extracted_text
        );
    }

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    for extension in ["png", "jpg", "tiff", "webp"] {
        let suffix = format!("photo.{extension}");
        let file = result
            .files
            .iter()
            .find(|f| f.file_type == FileType::File && f.path.ends_with(&suffix))
            .unwrap_or_else(|| panic!("Should find {} file", suffix));

        assert!(
            file.copyrights
                .iter()
                .any(|c| c.copyright == "Copyright 2026 Example Corp"),
            "{extension} copyrights: {:?}",
            file.copyrights
        );
        assert!(
            file.holders.iter().any(|h| h.holder == "Example Corp"),
            "{extension} holders: {:?}",
            file.holders
        );
    }
}

#[test]
fn test_scanner_detects_emails_and_urls_in_xmp_metadata() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("contacts.png");
    let xmp = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/">
      <dc:description>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">Reach us at legal@acme.org and https://acme.org/legal</rdf:li>
        </rdf:Alt>
      </dc:description>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;
    let png = build_png_with_metadata(None, Some(xmp));
    let (extracted_text, kind) = extract_text_for_detection(&content_path, &png);
    fs::write(&content_path, png).expect("Failed to write PNG fixture");

    assert_eq!(kind, ExtractedTextKind::ImageMetadata);
    assert!(
        extracted_text.contains("legal@acme.org"),
        "extracted_text: {:?}",
        extracted_text
    );
    assert!(
        extracted_text.contains("https://acme.org/legal"),
        "extracted_text: {:?}",
        extracted_text
    );

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("contacts.png"))
        .expect("Should find PNG file");

    assert!(
        file.emails
            .iter()
            .any(|email| email.email == "legal@acme.org"),
        "emails: {:?}",
        file.emails
    );
    assert!(
        file.urls
            .iter()
            .any(|url| url.url == "https://acme.org/legal"),
        "urls: {:?}",
        file.urls
    );
}

#[test]
fn test_scanner_detects_urls_in_additional_xmp_fields() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("rights.png");
    let xmp = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:xmpRights="http://ns.adobe.com/xap/1.0/rights/">
      <dc:subject>
        <rdf:Bag>
          <rdf:li>legal@acme.org</rdf:li>
        </rdf:Bag>
      </dc:subject>
      <xmpRights:WebStatement>https://acme.org/rights</xmpRights:WebStatement>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;
    let png = build_png_with_metadata(None, Some(xmp));
    let (extracted_text, kind) = extract_text_for_detection(&content_path, &png);
    fs::write(&content_path, png).expect("Failed to write PNG fixture");

    assert_eq!(kind, ExtractedTextKind::ImageMetadata);
    assert!(
        extracted_text.contains("legal@acme.org"),
        "extracted_text: {:?}",
        extracted_text
    );
    assert!(
        extracted_text.contains("https://acme.org/rights"),
        "extracted_text: {:?}",
        extracted_text
    );

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("rights.png"))
        .expect("Should find PNG file");

    assert!(
        file.emails
            .iter()
            .any(|email| email.email == "legal@acme.org"),
        "emails: {:?}",
        file.emails
    );
    assert!(
        file.urls
            .iter()
            .any(|url| url.url == "https://acme.org/rights"),
        "urls: {:?}",
        file.urls
    );
}

#[test]
fn test_scanner_detects_emails_in_exif_user_comment() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("note.jpg");
    let jpeg = build_image_with_exif(
        "jpg",
        build_exif_block(
            exif::Tag::UserComment,
            "Contact legal@acme.org for support.",
        ),
    );
    let (extracted_text, kind) = extract_text_for_detection(&content_path, &jpeg);
    fs::write(&content_path, jpeg).expect("Failed to write JPEG fixture");

    assert_eq!(kind, ExtractedTextKind::ImageMetadata);
    assert!(
        extracted_text.contains("legal@acme.org"),
        "extracted_text: {:?}",
        extracted_text
    );

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("note.jpg"))
        .expect("Should find JPEG file");

    assert!(
        file.emails
            .iter()
            .any(|email| email.email == "legal@acme.org"),
        "emails: {:?}",
        file.emails
    );
}

#[test]
fn test_scanner_ignores_non_clue_image_metadata() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("camera.png");
    let png = build_png_with_metadata(
        Some(build_exif_block(exif::Tag::Software, "CameraTool 1.0")),
        None,
    );
    let (extracted_text, kind) = extract_text_for_detection(&content_path, &png);
    fs::write(&content_path, png).expect("Failed to write PNG fixture");

    assert_eq!(kind, ExtractedTextKind::None);
    assert!(
        extracted_text.is_empty(),
        "extracted_text: {:?}",
        extracted_text
    );

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("camera.png"))
        .expect("Should find PNG file");

    assert!(
        file.copyrights.is_empty(),
        "copyrights: {:?}",
        file.copyrights
    );
    assert!(file.holders.is_empty(), "holders: {:?}", file.holders);
    assert!(file.authors.is_empty(), "authors: {:?}", file.authors);
    assert!(file.emails.is_empty(), "emails: {:?}", file.emails);
    assert!(file.urls.is_empty(), "urls: {:?}", file.urls);
}

#[test]
fn test_scanner_ignores_xml_namespace_garbage_in_copyright_detection() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let fixture =
        Path::new("testdata/copyright-golden/copyrights/view_layout2_xml-view_layout_xml.xml");
    let content_path = test_path.join("view_layout.xml");
    fs::copy(fixture, &content_path).expect("Failed to copy XML fixture");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("view_layout.xml"))
        .expect("Should find XML fixture file");

    assert_eq!(
        file.copyrights.len(),
        1,
        "copyrights: {:?}",
        file.copyrights
    );
    assert_eq!(file.holders.len(), 1, "holders: {:?}", file.holders);
    assert_eq!(
        file.copyrights[0].copyright,
        "Copyright (c) 2008 Esmertec AG."
    );
    assert_eq!(file.holders[0].holder, "Esmertec AG.");
    assert!(
        file.holders.iter().all(|h| {
            let lower = h.holder.to_ascii_lowercase();
            !lower.contains("xmlns")
                && !lower.contains("android:")
                && !lower.contains("linearlayout")
        }),
        "Unexpected XML garbage holders: {:?}",
        file.holders
    );
}

#[test]
fn test_scanner_detects_copyrights_in_windows_dll_strings() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let fixture = Path::new("testdata/copyright-golden/copyrights/windows.dll");
    let content_path = test_path.join("windows.dll");
    fs::copy(fixture, &content_path).expect("Failed to copy DLL fixture");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("windows.dll"))
        .expect("Should find DLL fixture file");

    assert!(
        file.copyrights
            .iter()
            .any(|c| c.copyright == "Copyright nexB and others (c) 2012"),
        "copyrights: {:?}",
        file.copyrights
    );
    assert!(
        file.holders.iter().any(|h| h.holder == "nexB and others"),
        "holders: {:?}",
        file.holders
    );
}

#[test]
fn test_scanner_avoids_false_positive_copyrights_in_executable_strings() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let fixture = Path::new("testdata/misc/test_nsis.exe");
    let content_path = test_path.join("test_nsis.exe");
    fs::copy(fixture, &content_path).expect("Failed to copy executable fixture");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: false,
        detect_urls: false,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("test_nsis.exe"))
        .expect("Should find executable fixture file");

    assert!(
        file.copyrights.is_empty(),
        "copyrights: {:?}",
        file.copyrights
    );
    assert!(file.holders.is_empty(), "holders: {:?}", file.holders);
    assert!(file.authors.is_empty(), "authors: {:?}", file.authors);
}

#[test]
fn test_scanner_respects_email_url_thresholds() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let content_path = test_path.join("thresholds.txt");
    fs::write(
        &content_path,
        [
            "a@one.org",
            "b@two.org",
            "c@three.org",
            "http://one.org",
            "http://two.org",
            "http://three.org",
            "",
        ]
        .join("\n"),
    )
    .expect("Failed to write test file");

    let patterns: Vec<Pattern> = vec![];
    let engine = create_license_detection_engine();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 2,
        max_urls: 2,
        timeout_seconds: 120.0,
        scan_cache_dir: None,
    };

    let result = scan(test_path, 10, &patterns, engine, false, Some(&options));

    let file = result
        .files
        .iter()
        .find(|f| {
            f.file_type == FileType::File
                && Path::new(&f.path)
                    .file_name()
                    .is_some_and(|n| n == "thresholds.txt")
        })
        .expect("Should find thresholds file");

    assert_eq!(file.emails.len(), 2);
    assert_eq!(file.urls.len(), 2);
}

#[test]
fn test_scanner_persists_scan_result_cache_entries() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();
    let cache_dir = test_path.join("scan-cache");
    let content_path = test_path.join("contacts.txt");
    fs::write(
        &content_path,
        "copyright 2024 Acme Corp\nmail us at support@many.org\nvisit www.acme.dev/docs\n",
    )
    .expect("Failed to write test file");

    let patterns: Vec<Pattern> = vec![];
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: true,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
        scan_cache_dir: Some(cache_dir.clone()),
    };

    let first = scan(test_path, 10, &patterns, None, false, Some(&options));

    let first_file = first
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("contacts.txt"))
        .expect("Should find contacts file");
    assert_eq!(first_file.emails.len(), 1);
    assert_eq!(first_file.urls.len(), 1);

    let content = fs::read(&content_path).expect("Failed to read contacts file");
    let sha256 = calculate_sha256(&content);
    let cache_path = cache_dir
        .join(&sha256[0..2])
        .join(&sha256[2..4])
        .join(format!("{sha256}.msgpack.zst"));
    assert!(cache_path.exists(), "Expected scan cache entry to exist");

    let second = scan(test_path, 10, &patterns, None, false, Some(&options));
    let second_file = second
        .files
        .iter()
        .find(|f| f.file_type == FileType::File && f.path.ends_with("contacts.txt"))
        .expect("Should find contacts file on second scan");

    assert_eq!(second_file.emails.len(), 1);
    assert_eq!(second_file.urls.len(), 1);
}
