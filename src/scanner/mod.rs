// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

mod collect;
mod process;

use crate::license_detection::LicenseDetectionEngine;
use crate::models::FileInfo;

pub struct ProcessResult {
    pub files: Vec<FileInfo>,
    pub excluded_count: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LicenseScanOptions {
    pub include_text: bool,
    pub include_text_diagnostics: bool,
    pub include_diagnostics: bool,
    pub unknown_licenses: bool,
    pub min_score: u8,
}

#[derive(Debug, Clone)]
pub struct TextDetectionOptions {
    pub collect_info: bool,
    pub detect_packages: bool,
    pub detect_application_packages: bool,
    pub detect_system_packages: bool,
    pub detect_packages_in_compiled: bool,
    pub detect_copyrights: bool,
    pub detect_generated: bool,
    pub detect_emails: bool,
    pub detect_urls: bool,
    pub max_emails: usize,
    pub max_urls: usize,
    pub timeout_seconds: f64,
}

impl Default for TextDetectionOptions {
    fn default() -> Self {
        Self {
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
        }
    }
}

pub fn scan_options_fingerprint(
    text_options: &TextDetectionOptions,
    license_options: LicenseScanOptions,
    license_engine: Option<&LicenseDetectionEngine>,
) -> String {
    let (license_enabled, rules_count, first_rule_id, last_rule_id) = match license_engine {
        Some(engine) => {
            let rules = &engine.index().rules_by_rid;
            (
                true,
                rules.len(),
                rules
                    .first()
                    .map(|rule| rule.identifier.as_str())
                    .unwrap_or(""),
                rules
                    .last()
                    .map(|rule| rule.identifier.as_str())
                    .unwrap_or(""),
            )
        }
        None => (false, 0, "", ""),
    };

    format!(
        "tool_version={};info={};packages={};app_packages={};system_packages={};compiled_packages={};copyrights={};generated={};emails={};urls={};max_emails={};max_urls={};timeout={:.6};license_enabled={};rules_count={};first_rule_id={};last_rule_id={};license_text={};license_text_diagnostics={};license_diagnostics={};unknown_licenses={};license_score={}",
        crate::version::BUILD_VERSION,
        text_options.collect_info,
        text_options.detect_packages,
        text_options.detect_application_packages,
        text_options.detect_system_packages,
        text_options.detect_packages_in_compiled,
        text_options.detect_copyrights,
        text_options.detect_generated,
        text_options.detect_emails,
        text_options.detect_urls,
        text_options.max_emails,
        text_options.max_urls,
        text_options.timeout_seconds,
        license_enabled,
        rules_count,
        first_rule_id,
        last_rule_id,
        license_options.include_text,
        license_options.include_text_diagnostics,
        license_options.include_diagnostics,
        license_options.unknown_licenses,
        license_options.min_score,
    )
}

pub use self::collect::{
    CollectedPaths, CollectionFrontier, collect_paths, collect_selected_paths,
};
#[allow(unused_imports)]
pub use self::process::{
    MemoryMode, process_collected, process_collected_sequential,
    process_collected_with_memory_limit, process_collected_with_memory_limit_sequential,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use tempfile::TempDir;

    use crate::cache::build_collection_exclude_patterns;
    use crate::license_detection::LicenseDetectionEngine;
    use crate::models::{DatasourceId, FileType, PackageType as FilePackageType};
    use crate::progress::{ProgressMode, ScanProgress};

    use super::{
        CollectionFrontier, LicenseScanOptions, MemoryMode, TextDetectionOptions, collect_paths,
        collect_selected_paths, process_collected, process_collected_with_memory_limit,
        scan_options_fingerprint,
    };

    fn build_sparse_oversized_rpm_with_filename(
        temp_dir: &TempDir,
        package_name: &str,
        filename: &str,
    ) -> PathBuf {
        let file_path = temp_dir.path().join(filename);
        rpm::PackageBuilder::new(package_name, "1.0", "MIT", "x86_64", "Demo RPM package")
            .release("1")
            .build()
            .expect("build rpm fixture")
            .write_file(&file_path)
            .expect("write rpm fixture");
        fs::OpenOptions::new()
            .write(true)
            .open(&file_path)
            .expect("open rpm fixture for sparse extension")
            .set_len(100 * 1024 * 1024 + 1_048_576)
            .expect("extend rpm fixture");
        file_path
    }

    fn build_sparse_oversized_rpm(temp_dir: &TempDir, name: &str) -> PathBuf {
        build_sparse_oversized_rpm_with_filename(
            temp_dir,
            name,
            &format!("{name}-1.0-1.x86_64.rpm"),
        )
    }

    fn build_sparse_oversized_pack_rpm(temp_dir: &TempDir, name: &str) -> PathBuf {
        build_sparse_oversized_rpm_with_filename(
            temp_dir,
            name,
            &format!("{name}-1.0-1.x86_64.pack"),
        )
    }

    #[test]
    fn default_options_keep_copyright_detection_enabled() {
        let options = TextDetectionOptions::default();
        assert!(!options.detect_packages);
        assert!(options.detect_copyrights);
    }

    #[test]
    fn test_scan_options_fingerprint_changes_with_license_score() {
        let text_options = TextDetectionOptions::default();
        let default_fingerprint = scan_options_fingerprint(
            &text_options,
            LicenseScanOptions {
                min_score: 0,
                ..LicenseScanOptions::default()
            },
            None,
        );
        let filtered_fingerprint = scan_options_fingerprint(
            &text_options,
            LicenseScanOptions {
                min_score: 70,
                ..LicenseScanOptions::default()
            },
            None,
        );

        assert_ne!(default_fingerprint, filtered_fingerprint);
    }

    fn scan_single_file(
        file_name: &str,
        content: &str,
        options: &TextDetectionOptions,
    ) -> crate::models::FileInfo {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("write test file");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            options,
        );

        result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry")
    }

    fn scan_file_at_relative_path(
        relative_path: &str,
        content: &[u8],
        options: &TextDetectionOptions,
    ) -> crate::models::FileInfo {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&file_path, content).expect("write test file");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            options,
        );

        result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry")
    }

    fn scan_single_file_with_license_engine(
        file_name: &str,
        content: &str,
        options: &TextDetectionOptions,
    ) -> crate::models::FileInfo {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("write test file");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let engine =
            Arc::new(LicenseDetectionEngine::from_embedded().expect("initialize license engine"));
        let result = process_collected(
            &collected,
            progress,
            Some(engine),
            LicenseScanOptions::default(),
            options,
        );

        result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry")
    }

    #[test]
    fn scanner_reports_repeated_email_occurrences() {
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
        };
        let scanned = scan_single_file(
            "contacts.txt",
            "linux@3ware.com\nlinux@3ware.com\nandre@suse.com\nlinux@3ware.com\n",
            &options,
        );

        let emails: Vec<(&str, usize)> = scanned
            .emails
            .iter()
            .map(|email| (email.email.as_str(), email.start_line.get()))
            .collect();

        assert_eq!(emails.len(), 4, "emails: {emails:#?}");
        assert_eq!(
            emails,
            vec![
                ("linux@3ware.com", 1),
                ("linux@3ware.com", 2),
                ("andre@suse.com", 3),
                ("linux@3ware.com", 4),
            ]
        );
    }

    #[test]
    fn scanner_skips_pem_certificate_text_detection() {
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
        };
        let pem_fixture = concat!(
            "-----BEGIN CERTIFICATE-----\n",
            "MIID8TCCAtmgAwIBAgIQQT1yx/RrH4FDffHSKFTfmjANBgkqhkiG9w0BAQUFADCB\n",
            "ijELMAkGA1UEBhMCQ0gxEDAOBgNVBAoTB1dJU2VLZXkxGzAZBgNVBAsTEkNvcHly\n",
            "-----END CERTIFICATE-----\n",
            "Certificate:\n",
            "    Data:\n",
            "        Signature Algorithm: sha1WithRSAEncryption\n",
            "        Issuer: C=CH, O=WISeKey, OU=Copyright (c) 2005, OU=OISTE Foundation Endorsed\n",
            "        Subject: C=CH, O=WISeKey, OU=Copyright (c) 2005, OU=OISTE Foundation Endorsed\n",
            "        Contact: cert-owner@example.com\n",
        );
        let scanned = scan_single_file("cert.pem", pem_fixture, &options);

        assert!(
            scanned.copyrights.is_empty(),
            "copyrights: {:#?}",
            scanned.copyrights
        );
        assert!(
            scanned.holders.is_empty(),
            "holders: {:#?}",
            scanned.holders
        );
        assert!(
            scanned.authors.is_empty(),
            "authors: {:#?}",
            scanned.authors
        );
        assert!(scanned.emails.is_empty(), "emails: {:#?}", scanned.emails);
        assert!(scanned.urls.is_empty(), "urls: {:#?}", scanned.urls);
        assert!(
            scanned.license_detections.is_empty(),
            "licenses: {:#?}",
            scanned.license_detections
        );
        assert!(
            scanned.license_clues.is_empty(),
            "license clues: {:#?}",
            scanned.license_clues
        );
    }

    #[test]
    fn scanner_keeps_source_headers_when_pem_blocks_are_embedded() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: true,
            detect_generated: false,
            detect_emails: false,
            detect_urls: true,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let fixture = concat!(
            "/*\n",
            "Copyright 2022 The Kubernetes Authors.\n\n",
            "Licensed under the Apache License, Version 2.0 (the \"License\");\n",
            "you may not use this file except in compliance with the License.\n",
            "You may obtain a copy of the License at\n\n",
            "    http://www.apache.org/licenses/LICENSE-2.0\n",
            "*/\n\n",
            "package storage\n\n",
            "const validCert = `\n",
            "-----BEGIN CERTIFICATE-----\n",
            "MIIDmTCCAoGgAwIBAgIUWQ==\n",
            "-----END CERTIFICATE-----\n",
            "`\n",
        );
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("storage_test.go");
        fs::write(&file_path, fixture).expect("write fixture");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let engine =
            Arc::new(LicenseDetectionEngine::from_embedded().expect("initialize license engine"));
        let result = process_collected(
            &collected,
            progress,
            Some(engine),
            LicenseScanOptions::default(),
            &options,
        );
        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned
                .copyrights
                .iter()
                .any(|c| c.copyright == "Copyright 2022 The Kubernetes Authors"),
            "copyrights: {:#?}",
            scanned.copyrights
        );
        assert!(
            scanned
                .holders
                .iter()
                .any(|h| h.holder == "The Kubernetes Authors"),
            "holders: {:#?}",
            scanned.holders
        );
        assert!(
            scanned
                .urls
                .iter()
                .any(|u| u.url == "http://www.apache.org/licenses/LICENSE-2.0"),
            "urls: {:#?}",
            scanned.urls
        );
        assert_eq!(scanned.license_expression.as_deref(), Some("Apache-2.0"));
    }

    #[test]
    fn scanner_detects_structured_credits_authors() {
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
        };
        let credits_fixture = concat!(
            "N: Jack Lloyd\n",
            "E: lloyd@randombit.net\n",
            "W: http://www.randombit.net/\n",
        );
        let scanned = scan_single_file("CREDITS", credits_fixture, &options);

        let authors: Vec<(&str, usize, usize)> = scanned
            .authors
            .iter()
            .map(|author| {
                (
                    author.author.as_str(),
                    author.start_line.get(),
                    author.end_line.get(),
                )
            })
            .collect();

        assert_eq!(
            authors,
            vec![(
                "Jack Lloyd lloyd@randombit.net http://www.randombit.net/",
                1,
                3,
            )]
        );
        assert!(scanned.copyrights.is_empty());
        assert!(scanned.holders.is_empty());
    }

    #[test]
    fn scanner_uses_or_for_alternative_license_header() {
        let fixture =
            include_str!("../../testdata/license-golden/datadriven/external/boost-json-d2s.ipp");
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("d2s.ipp");
        fs::write(&file_path, fixture).expect("write fixture");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let engine =
            Arc::new(LicenseDetectionEngine::from_embedded().expect("initialize license engine"));
        let result = process_collected(
            &collected,
            progress,
            Some(engine),
            LicenseScanOptions::default(),
            &TextDetectionOptions::default(),
        );
        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert_eq!(
            scanned.license_expression.as_deref(),
            Some("Apache-2.0 OR BSL-1.0")
        );
        assert!(
            scanned.license_clues.is_empty(),
            "license clues: {:#?}",
            scanned.license_clues
        );
        assert_eq!(
            scanned.license_detections.len(),
            1,
            "detections: {:#?}",
            scanned.license_detections
        );

        let detection = &scanned.license_detections[0];
        assert_eq!(detection.license_expression_spdx, "Apache-2.0 OR BSL-1.0");

        let match_expressions: Vec<_> = detection
            .matches
            .iter()
            .map(|m| m.license_expression_spdx.as_str())
            .collect();
        assert_eq!(match_expressions, vec!["Apache-2.0", "BSL-1.0"]);
    }

    #[test]
    fn scanner_sets_generated_flag_when_enabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: true,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "generated.c",
            "/* DO NOT EDIT THIS FILE - it is machine generated */\n",
            &options,
        );

        assert_eq!(scanned.is_generated, Some(true));
    }

    #[test]
    fn scanner_leaves_generated_flag_unset_when_disabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "generated.c",
            "/* DO NOT EDIT THIS FILE - it is machine generated */\n",
            &options,
        );

        assert_eq!(scanned.is_generated, None);
    }

    #[test]
    fn scanner_populates_info_surface_when_enabled() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "script.py",
            "#!/usr/bin/env python3\nprint(\"hello\")\n",
            &options,
        );

        assert!(scanned.sha1.is_some());
        assert!(scanned.md5.is_some());
        assert!(scanned.sha256.is_some());
        assert!(scanned.sha1_git.is_some());
        assert!(scanned.mime_type.is_some());
        assert!(scanned.date.is_some());
        assert_eq!(scanned.programming_language.as_deref(), Some("Python"));
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_script, Some(true));
        assert_eq!(scanned.is_source, Some(true));
    }

    #[test]
    fn scanner_treats_latin1_python_sources_as_textual_scripts() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let latin1_python = b"# coding: latin-1\nprint(\"caf\xe9\")\n# comment padding\n";
        let scanned = scan_file_at_relative_path("script.py", latin1_python, &options);

        assert_eq!(scanned.programming_language.as_deref(), Some("Python"));
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("python script, text executable")
        );
        assert_eq!(scanned.is_binary, Some(false));
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_script, Some(true));
        assert_eq!(scanned.is_source, Some(true));
    }

    #[test]
    fn scanner_skips_findings_for_zip_like_archives() {
        let options = TextDetectionOptions {
            collect_info: true,
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
        };
        let archive_like = b"PK\x03\x04\x14\x00\x00\x00\x08\x00MIT License\ncontact@example.com\nhttps://example.com\n";
        let scanned = scan_file_at_relative_path("demo.whl", archive_like, &options);

        assert_eq!(scanned.mime_type.as_deref(), Some("application/zip"));
        assert_eq!(scanned.is_archive, Some(true));
        assert!(scanned.license_detections.is_empty());
        assert!(scanned.copyrights.is_empty());
        assert!(scanned.emails.is_empty());
        assert!(scanned.urls.is_empty());
    }

    #[test]
    fn scanner_treats_typescript_sources_as_text_not_video_media() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file("main.ts", "export const answer: number = 42;\n", &options);

        assert_eq!(scanned.programming_language.as_deref(), Some("TypeScript"));
        assert_eq!(scanned.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("TypeScript source, UTF-8 Unicode text")
        );
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_media, Some(false));
        assert_eq!(scanned.is_script, Some(false));
        assert_eq!(scanned.is_source, Some(true));
    }

    #[test]
    fn scanner_normalizes_sparse_ts_files_away_from_video_mime() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file("main.ts", "// comment-only TypeScript fixture\n", &options);

        assert_eq!(scanned.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("TypeScript source, UTF-8 Unicode text")
        );
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_media, Some(false));
        assert_eq!(scanned.is_script, Some(false));
        assert_eq!(scanned.is_source, Some(true));
    }

    #[test]
    fn scanner_treats_empty_files_like_scancode_info_surface() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file("test.txt", "", &options);

        assert_eq!(scanned.mime_type.as_deref(), Some("inode/x-empty"));
        assert_eq!(scanned.file_type_label.as_deref(), Some("empty"));
        assert_eq!(scanned.programming_language, None);
        assert_eq!(scanned.is_binary, Some(false));
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_archive, Some(false));
        assert_eq!(scanned.is_media, Some(false));
        assert_eq!(scanned.is_source, Some(false));
        assert_eq!(scanned.is_script, Some(false));
    }

    #[test]
    fn scanner_treats_package_json_as_text_not_source() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file("package.json", r#"{"name":"demo"}"#, &options);

        assert_eq!(scanned.mime_type.as_deref(), Some("application/json"));
        assert_eq!(scanned.file_type_label.as_deref(), Some("JSON text data"));
        assert_eq!(scanned.programming_language, None);
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_source, Some(false));
        assert_eq!(scanned.is_script, Some(false));
    }

    #[test]
    fn scanner_classifies_gradle_and_nix_manifests_as_source() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };

        let gradle = scan_single_file("build.gradle", "plugins { id 'java' }\n", &options);
        let nix = scan_single_file("flake.nix", "{ inputs, ... }: {}\n", &options);

        assert_eq!(gradle.programming_language.as_deref(), Some("Groovy"));
        assert_eq!(gradle.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(gradle.is_source, Some(true));
        assert_eq!(gradle.is_script, Some(false));

        assert_eq!(nix.programming_language.as_deref(), Some("Nix"));
        assert_eq!(nix.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(nix.is_source, Some(true));
        assert_eq!(nix.is_script, Some(false));
    }

    #[test]
    fn scanner_treats_gitmodules_as_text_not_source() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_file_at_relative_path(
            ".gitmodules",
            b"[submodule \"demo\"]\n\tpath = vendor/demo\n",
            &options,
        );

        assert_eq!(scanned.programming_language, None);
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("Git configuration text")
        );
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_source, Some(false));
        assert_eq!(scanned.is_script, Some(false));
    }

    #[test]
    fn scanner_treats_javascript_shebang_files_as_scripts() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_file_at_relative_path(
            "bin/run",
            b"#!/usr/bin/env node\nconsole.log('hello');\n",
            &options,
        );

        assert_eq!(scanned.programming_language.as_deref(), Some("JavaScript"));
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("javascript script, UTF-8 Unicode text executable")
        );
        assert_eq!(scanned.is_script, Some(true));
        assert_eq!(scanned.is_source, Some(true));
    }

    #[test]
    fn scanner_treats_dockerfile_as_source() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file("Dockerfile", "FROM scratch\n", &options);

        assert_eq!(scanned.programming_language.as_deref(), Some("Dockerfile"));
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("Dockerfile source, UTF-8 Unicode text")
        );
        assert_eq!(scanned.is_source, Some(true));
        assert_eq!(scanned.is_script, Some(false));
    }

    #[test]
    fn scanner_treats_makefile_as_text_not_source() {
        let options = TextDetectionOptions {
            collect_info: true,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file("Makefile", "all:\n\techo hi\n", &options);

        assert_eq!(scanned.programming_language, None);
        assert_eq!(
            scanned.file_type_label.as_deref(),
            Some("UTF-8 Unicode text")
        );
        assert_eq!(scanned.is_text, Some(true));
        assert_eq!(scanned.is_source, Some(false));
        assert_eq!(scanned.is_script, Some(false));
    }

    #[test]
    fn scanner_omits_info_surface_when_disabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "script.py",
            "#!/usr/bin/env python3\nprint(\"hello\")\n",
            &options,
        );

        assert!(scanned.sha1.is_none());
        assert!(scanned.md5.is_none());
        assert!(scanned.sha256.is_none());
        assert!(scanned.sha1_git.is_none());
        assert!(scanned.mime_type.is_none());
        assert!(scanned.date.is_none());
        assert!(scanned.programming_language.is_none());
        assert!(scanned.is_binary.is_none());
        assert!(scanned.is_text.is_none());
        assert!(scanned.is_archive.is_none());
        assert!(scanned.is_media.is_none());
        assert!(scanned.is_script.is_none());
        assert!(scanned.is_source.is_none());
    }

    #[test]
    fn scanner_skips_package_parsing_when_disabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "package.json",
            r#"{"name":"demo","version":"1.0.0"}"#,
            &options,
        );

        assert!(
            scanned.package_data.is_empty(),
            "package_data: {:#?}",
            scanned.package_data
        );
    }

    #[test]
    fn scanner_parses_package_manifests_when_enabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: true,
            detect_application_packages: true,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "package.json",
            r#"{"name":"demo","version":"1.0.0"}"#,
            &options,
        );

        assert_eq!(
            scanned.package_data.len(),
            1,
            "package_data: {:#?}",
            scanned.package_data
        );
    }

    #[test]
    fn scanner_parses_oversized_rpm_in_package_only_mode_without_size_warning() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = build_sparse_oversized_rpm(&temp_dir, "oversized-demo");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned.scan_errors.is_empty(),
            "scan_errors: {:#?}",
            scanned.scan_errors
        );
        assert_eq!(
            scanned.package_data.len(),
            1,
            "package_data: {:#?}",
            scanned.package_data
        );
        assert_eq!(
            scanned.package_data[0].datasource_id,
            Some(DatasourceId::RpmArchive)
        );
        assert_eq!(
            scanned.package_data[0].name.as_deref(),
            Some("oversized-demo")
        );
        assert_eq!(scanned.package_data[0].version.as_deref(), Some("1.0-1"));
    }

    #[test]
    fn scanner_parses_oversized_rpm_with_info_without_timeout_or_size_warning() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = build_sparse_oversized_rpm(&temp_dir, "oversized-info-demo");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: true,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned.scan_errors.is_empty(),
            "scan_errors: {:#?}",
            scanned.scan_errors
        );
        assert_eq!(
            scanned.package_data.len(),
            1,
            "package_data: {:#?}",
            scanned.package_data
        );
        assert_eq!(
            scanned.package_data[0].datasource_id,
            Some(DatasourceId::RpmArchive)
        );
        assert_eq!(
            scanned.package_data[0].name.as_deref(),
            Some("oversized-info-demo")
        );
        assert!(scanned.sha1.is_some());
        assert!(scanned.md5.is_some());
        assert!(scanned.sha256.is_some());
        assert!(scanned.sha1_git.is_some());
        assert_eq!(scanned.mime_type.as_deref(), Some("application/x-rpm"));
        assert_eq!(scanned.file_type_label.as_deref(), Some("RPM package"));
        assert_eq!(scanned.is_binary, Some(true));
        assert_eq!(scanned.is_text, Some(false));
        assert_eq!(scanned.is_archive, Some(true));
    }

    #[test]
    fn scanner_parses_oversized_pack_rpm_in_package_only_mode_without_size_warning() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = build_sparse_oversized_pack_rpm(&temp_dir, "oversized-pack-demo");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned.scan_errors.is_empty(),
            "scan_errors: {:#?}",
            scanned.scan_errors
        );
        assert_eq!(
            scanned.package_data.len(),
            1,
            "package_data: {:#?}",
            scanned.package_data
        );
        assert_eq!(
            scanned.package_data[0].datasource_id,
            Some(DatasourceId::RpmArchive)
        );
        assert_eq!(
            scanned.package_data[0].name.as_deref(),
            Some("oversized-pack-demo")
        );
    }

    #[test]
    fn scanner_parses_oversized_pack_rpm_with_info_without_timeout_or_size_warning() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = build_sparse_oversized_pack_rpm(&temp_dir, "oversized-pack-info-demo");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: true,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned.scan_errors.is_empty(),
            "scan_errors: {:#?}",
            scanned.scan_errors
        );
        assert_eq!(
            scanned.package_data.len(),
            1,
            "package_data: {:#?}",
            scanned.package_data
        );
        assert_eq!(
            scanned.package_data[0].datasource_id,
            Some(DatasourceId::RpmArchive)
        );
        assert_eq!(
            scanned.package_data[0].name.as_deref(),
            Some("oversized-pack-info-demo")
        );
        assert!(scanned.sha1.is_some());
        assert!(scanned.md5.is_some());
        assert!(scanned.sha256.is_some());
        assert!(scanned.sha1_git.is_some());
        assert_eq!(scanned.mime_type.as_deref(), Some("application/x-rpm"));
        assert_eq!(scanned.file_type_label.as_deref(), Some("RPM package"));
        assert_eq!(scanned.is_binary, Some(true));
        assert_eq!(scanned.is_text, Some(false));
        assert_eq!(scanned.is_archive, Some(true));
    }

    #[test]
    fn scanner_skips_application_packages_when_only_system_packages_enabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: true,
            detect_application_packages: false,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_single_file(
            "package.json",
            r#"{"name":"demo","version":"1.0.0"}"#,
            &options,
        );

        assert!(
            scanned.package_data.is_empty(),
            "package_data: {:#?}",
            scanned.package_data
        );
    }

    #[test]
    fn scanner_parses_system_package_files_when_enabled() {
        let options = TextDetectionOptions {
            collect_info: false,
            detect_packages: true,
            detect_application_packages: false,
            detect_system_packages: true,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let scanned = scan_file_at_relative_path(
            "var/lib/dpkg/status",
            b"Package: demo\nVersion: 1.0\nArchitecture: all\nDescription: demo package\n\n",
            &options,
        );

        assert!(
            !scanned.package_data.is_empty(),
            "package_data: {:#?}",
            scanned.package_data
        );
    }

    #[test]
    fn scanner_only_parses_compiled_packages_when_package_in_compiled_is_enabled() {
        if std::process::Command::new("go")
            .arg("version")
            .status()
            .is_err()
        {
            return;
        }

        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("go.mod"),
            "module example.com/demo\n\ngo 1.23.0\n",
        )
        .expect("write go.mod");
        fs::write(
            temp_dir.path().join("main.go"),
            "package main\nfunc main() {}\n",
        )
        .expect("write main.go");
        let file_path = temp_dir.path().join("demo");
        let status = std::process::Command::new("go")
            .current_dir(temp_dir.path())
            .args(["build", "-o"])
            .arg(&file_path)
            .status()
            .expect("run go build");
        assert!(status.success());

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);

        let without_compiled = process_collected(
            &collected,
            Arc::clone(&progress),
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );
        let with_compiled = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: true,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let without_compiled = without_compiled
            .files
            .into_iter()
            .find(|entry| entry.file_type == FileType::File && entry.path.ends_with("/demo"))
            .expect("compiled artifact present");
        let with_compiled = with_compiled
            .files
            .into_iter()
            .find(|entry| entry.file_type == FileType::File && entry.path.ends_with("/demo"))
            .expect("compiled artifact present");

        assert!(
            without_compiled.package_data.is_empty(),
            "package_data: {:#?}",
            without_compiled.package_data
        );
        assert!(!with_compiled.package_data.is_empty());
    }

    #[test]
    fn scanner_parses_windows_executable_packages_under_normal_package_scan() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("libiconv2.dll");
        let fixture = fs::read("testdata/compiled-binary-golden/win_pe/libiconv2.dll")
            .expect("read PE fixture");
        fs::write(&file_path, fixture).expect("write PE fixture");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);

        let without_package = process_collected(
            &collected,
            Arc::clone(&progress),
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: false,
                detect_application_packages: false,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );
        let with_package = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let without_package = without_package
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path.ends_with("/libiconv2.dll")
            })
            .expect("compiled artifact present");
        let with_package = with_package
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path.ends_with("/libiconv2.dll")
            })
            .expect("compiled artifact present");

        assert!(without_package.package_data.is_empty());
        assert_eq!(with_package.package_data.len(), 1);
        assert_eq!(
            with_package.package_data[0].package_type,
            Some(FilePackageType::Winexe)
        );
        assert_eq!(
            with_package.package_data[0].datasource_id,
            Some(DatasourceId::WindowsExecutable)
        );
    }

    #[test]
    fn scanner_keeps_nsis_and_windows_executable_package_data_together() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("nsis-with-version.exe");
        let mut fixture = fs::read("testdata/compiled-binary-golden/win_pe/libiconv2.dll")
            .expect("read PE fixture");
        if fixture.len() < 70_000 {
            fixture.resize(70_000, 0);
        }
        fixture.extend_from_slice(b"Nullsoft.NSIS.exehead");
        fs::write(&file_path, fixture).expect("write synthetic NSIS PE fixture");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path.ends_with("/nsis-with-version.exe")
            })
            .expect("compiled artifact present");

        assert_eq!(
            scanned.package_data.len(),
            2,
            "package_data: {:#?}",
            scanned.package_data
        );
        assert!(
            scanned
                .package_data
                .iter()
                .any(|pkg| pkg.datasource_id == Some(DatasourceId::NsisInstaller))
        );
        assert!(
            scanned
                .package_data
                .iter()
                .any(|pkg| pkg.datasource_id == Some(DatasourceId::WindowsExecutable))
        );
    }

    #[test]
    fn scanner_detects_license_from_font_metadata() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("Lato-Bold.ttf");
        let fixture = fs::read("testdata/font-fixtures/Lato-Bold.ttf").expect("read font fixture");
        fs::write(&file_path, fixture).expect("write font fixture");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let engine =
            Arc::new(LicenseDetectionEngine::from_embedded().expect("initialize license engine"));
        let result = process_collected(
            &collected,
            progress,
            Some(engine),
            LicenseScanOptions::default(),
            &TextDetectionOptions::default(),
        );
        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned.license_expression.is_some(),
            "license detections: {:#?}",
            scanned.license_detections
        );
        assert!(
            scanned
                .license_expression
                .as_deref()
                .is_some_and(
                    |expression| expression.contains("OFL-1.1") || expression.contains("ofl-1.1")
                ),
            "license expression: {:?}",
            scanned.license_expression
        );
    }

    #[test]
    fn scanner_detects_license_from_windows_executable_metadata() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("libiconv2.dll");
        let fixture = fs::read("testdata/compiled-binary-golden/win_pe/libiconv2.dll")
            .expect("read PE fixture");
        fs::write(&file_path, fixture).expect("write PE fixture");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let engine =
            Arc::new(LicenseDetectionEngine::from_embedded().expect("initialize license engine"));
        let result = process_collected(
            &collected,
            progress,
            Some(engine),
            LicenseScanOptions::default(),
            &TextDetectionOptions::default(),
        );
        let scanned = result
            .files
            .into_iter()
            .find(|entry| {
                entry.file_type == FileType::File && entry.path == file_path.to_string_lossy()
            })
            .expect("scanned file entry");

        assert!(
            scanned.license_expression.is_some(),
            "license detections: {:#?}",
            scanned.license_detections
        );
        assert!(
            scanned
                .license_expression
                .as_deref()
                .is_some_and(|expression| {
                    expression.contains("lgpl") || expression.contains("LGPL")
                }),
            "license expression: {:?}",
            scanned.license_expression
        );
    }

    #[test]
    fn scanner_detects_cc_by_license_from_markdown_comment_banner() {
        let scanned = scan_single_file_with_license_engine(
            "navbar.md",
            "<!-- Documentation licensed under CC BY 4.0 -->\n<!-- License available at https://creativecommons.org/licenses/by/4.0/ -->\n",
            &TextDetectionOptions::default(),
        );

        assert!(
            scanned
                .license_expression
                .as_deref()
                .is_some_and(|expression| {
                    expression.contains("cc-by-4.0") || expression.contains("CC-BY-4.0")
                }),
            "license expression: {:?}",
            scanned.license_expression
        );
    }

    #[test]
    fn scanner_detects_mit_license_from_shields_badge_markdown() {
        let scanned = scan_single_file_with_license_engine(
            "README.md",
            "[![](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)\n",
            &TextDetectionOptions::default(),
        );

        assert!(
            scanned
                .license_expression
                .as_deref()
                .is_some_and(|expression| {
                    expression.contains("mit") || expression.contains("MIT")
                }),
            "license expression: {:?}",
            scanned.license_expression
        );
    }

    #[test]
    fn scanner_detects_apache_license_from_markdown_readme_phrase() {
        let scanned = scan_single_file_with_license_engine(
            "README.md",
            "This crate is distributed under the terms of the Apache License (Version 2.0).\n",
            &TextDetectionOptions::default(),
        );

        assert!(
            scanned
                .license_expression
                .as_deref()
                .is_some_and(|expression| {
                    expression.contains("apache-2.0") || expression.contains("Apache-2.0")
                }),
            "license expression: {:?}",
            scanned.license_expression
        );
    }

    #[test]
    fn scanner_sets_is_source_only_when_info_enabled() {
        let without_info = TextDetectionOptions {
            collect_info: false,
            detect_packages: false,
            detect_application_packages: false,
            detect_system_packages: false,
            detect_packages_in_compiled: false,
            detect_copyrights: false,
            detect_generated: false,
            detect_emails: false,
            detect_urls: false,
            max_emails: 50,
            max_urls: 50,
            timeout_seconds: 120.0,
        };
        let with_info = TextDetectionOptions {
            collect_info: true,
            ..without_info.clone()
        };

        let scanned_without_info = scan_single_file("main.rs", "fn main() {}\n", &without_info);
        let scanned_with_info = scan_single_file("main.rs", "fn main() {}\n", &with_info);

        assert_eq!(scanned_without_info.is_source, None);
        assert_eq!(scanned_with_info.is_source, Some(true));
    }

    #[test]
    fn directory_omits_info_fields_when_info_disabled() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::create_dir_all(temp_dir.path().join("nested")).expect("create nested dir");

        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            Arc::new(ScanProgress::new(ProgressMode::Quiet)),
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: false,
                detect_application_packages: false,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let directory = result
            .files
            .into_iter()
            .find(|entry| entry.file_type == FileType::Directory && entry.path.ends_with("nested"))
            .expect("directory entry");

        assert!(directory.date.is_none());
        assert!(directory.file_type_label.is_none());
        assert!(directory.is_binary.is_none());
        assert!(directory.is_text.is_none());
        assert!(directory.is_archive.is_none());
        assert!(directory.is_media.is_none());
        assert!(directory.is_source.is_none());
        assert!(directory.is_script.is_none());
    }

    #[test]
    fn directory_includes_info_fields_when_info_enabled() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::create_dir_all(temp_dir.path().join("nested")).expect("create nested dir");

        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected(
            &collected,
            Arc::new(ScanProgress::new(ProgressMode::Quiet)),
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: true,
                detect_packages: false,
                detect_application_packages: false,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
        );

        let directory = result
            .files
            .into_iter()
            .find(|entry| entry.file_type == FileType::Directory && entry.path.ends_with("nested"))
            .expect("directory entry");

        assert!(directory.date.is_none());
        assert!(directory.file_type_label.is_none());
        assert_eq!(directory.is_binary, Some(false));
        assert_eq!(directory.is_text, Some(false));
        assert_eq!(directory.is_archive, Some(false));
        assert_eq!(directory.is_media, Some(false));
        assert_eq!(directory.is_source, Some(false));
        assert_eq!(directory.is_script, Some(false));
        assert_eq!(directory.files_count, Some(0));
        assert_eq!(directory.dirs_count, Some(0));
        assert_eq!(directory.size_count, Some(0));
    }

    #[test]
    fn collect_paths_includes_root_directory_entry() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::create_dir_all(temp_dir.path().join("src")).expect("create nested dir");
        fs::write(temp_dir.path().join("src").join("main.rs"), "fn main() {}")
            .expect("write nested file");

        let collected = collect_paths(temp_dir.path(), 0, &[]);

        assert!(
            collected
                .directories
                .iter()
                .any(|(path, _)| path == temp_dir.path())
        );
    }

    #[test]
    fn collect_paths_supports_single_file_input() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("main.rs");
        fs::write(&file_path, "fn main() {}\n").expect("write file");

        let collected = collect_paths(&file_path, 0, &[]);

        assert_eq!(collected.files.len(), 1);
        assert!(collected.directories.is_empty());
        assert_eq!(collected.files[0].0, file_path);
    }

    #[cfg(unix)]
    #[test]
    fn collect_selected_paths_does_not_walk_unselected_siblings() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path();
        fs::create_dir_all(root.join("selected/docs")).expect("create selected dir");
        fs::create_dir_all(root.join("blocked/secret")).expect("create blocked dir");
        fs::write(root.join("selected/docs/guide.md"), "# guide\n").expect("write guide");

        let blocked = root.join("blocked");
        let mut perms = fs::metadata(&blocked)
            .expect("blocked metadata")
            .permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&blocked, perms).expect("remove blocked permissions");

        let collected = collect_selected_paths(
            root,
            &[CollectionFrontier {
                path: PathBuf::from("selected"),
                recurse: true,
            }],
            0,
            &[],
        );

        let mut restore = fs::metadata(&blocked)
            .expect("blocked metadata")
            .permissions();
        restore.set_mode(0o755);
        fs::set_permissions(&blocked, restore).expect("restore blocked permissions");

        assert!(
            collected.collection_errors.is_empty(),
            "{:#?}",
            collected.collection_errors
        );
        assert!(
            collected
                .files
                .iter()
                .any(|(path, _)| path == &root.join("selected/docs/guide.md"))
        );
        assert!(
            collected
                .files
                .iter()
                .all(|(path, _): &(PathBuf, fs::Metadata)| !path.starts_with(&blocked))
        );
    }

    #[test]
    fn collect_selected_paths_respects_excluded_ancestor_directories() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let root = temp_dir.path();
        fs::create_dir_all(root.join(".git")).expect("create git dir");
        fs::write(
            root.join(".git/config"),
            "[core]\nrepositoryformatversion = 0\n",
        )
        .expect("write git config");

        let exclude_patterns =
            build_collection_exclude_patterns(root, &root.join(".provenant-cache"));
        let collected = collect_selected_paths(
            root,
            &[CollectionFrontier {
                path: PathBuf::from(".git/config"),
                recurse: false,
            }],
            0,
            &exclude_patterns,
        );

        assert!(collected.files.is_empty());
        assert!(collected.directories.iter().all(|(path, _)| path == root));
        assert_eq!(collected.excluded_count, 1);
    }

    #[test]
    fn process_collected_with_memory_limit_preserves_results_when_spilling() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(temp_dir.path().join("a.txt"), "hello").expect("write first file");
        fs::write(temp_dir.path().join("b.txt"), "world").expect("write second file");

        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected_with_memory_limit(
            &collected,
            Arc::new(ScanProgress::new(ProgressMode::Quiet)),
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: false,
                detect_application_packages: false,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
            MemoryMode::Limit(1),
        );

        assert_eq!(result.files.len(), 3);
    }

    #[test]
    fn process_collected_with_negative_one_uses_disk_only_mode() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(temp_dir.path().join("a.txt"), "hello").expect("write first file");

        let collected = collect_paths(temp_dir.path(), 0, &[]);
        let result = process_collected_with_memory_limit(
            &collected,
            Arc::new(ScanProgress::new(ProgressMode::Quiet)),
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_packages: false,
                detect_application_packages: false,
                detect_system_packages: false,
                detect_packages_in_compiled: false,
                detect_copyrights: false,
                detect_generated: false,
                detect_emails: false,
                detect_urls: false,
                max_emails: 50,
                max_urls: 50,
                timeout_seconds: 120.0,
            },
            MemoryMode::StreamUnlimited,
        );

        assert_eq!(result.files.len(), 2);
    }
}
