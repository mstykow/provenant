mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    use super::super::materialize_generated_flags;
    use super::super::test_utils::{
        FixtureOutputOptions, assert_classify_fixture_matches_expected,
        assert_facet_fixture_matches_expected, assert_file_info_fixture_matches_expected,
        assert_package_fixture_matches_expected, assert_reference_follow_fixture_matches_expected,
        assert_summary_fixture_matches_expected, assert_tally_fixture_matches_expected,
        compare_scan_json_values, fixture_exclude_patterns, normalize_paths_for_test,
        normalize_scan_json,
    };
    use crate::models::FileType;
    use crate::progress::{ProgressMode, ScanProgress};
    use crate::scanner::{
        LicenseScanOptions, TextDetectionOptions, collect_paths, process_collected,
    };

    #[test]
    fn test_golden_summary_fixtures_match_expected_summary_blocks() {
        let fixtures = [
            (
                "testdata/summarycode-golden/summary/without_package_data",
                "testdata/summarycode-golden/summary/without_package_data/without_package_data.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/with_package_data",
                "testdata/summarycode-golden/summary/with_package_data/with_package_data.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/use_holder_from_package_resource",
                "testdata/summarycode-golden/summary/use_holder_from_package_resource/use_holder_from_package_resource.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/summary_without_holder",
                "testdata/summarycode-golden/summary/summary_without_holder/summary-without-holder-pypi.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/single_file",
                "testdata/summarycode-golden/summary/single_file/single_file.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/multiple_package_data",
                "testdata/summarycode-golden/summary/multiple_package_data/multiple_package_data.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/license_ambiguity/unambiguous",
                "testdata/summarycode-golden/summary/license_ambiguity/unambiguous.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/license_ambiguity/ambiguous",
                "testdata/summarycode-golden/summary/license_ambiguity/ambiguous.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/holders/combined_holders",
                "testdata/summarycode-golden/summary/holders/combined_holders.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/holders/clear_holder",
                "testdata/summarycode-golden/summary/holders/clear_holder.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/conflicting_license_categories",
                "testdata/summarycode-golden/summary/conflicting_license_categories/conflicting_license_categories.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/end-2-end/bug-1141",
                "testdata/summarycode-golden/summary/end-2-end/bug-1141.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/embedded_packages/bunkerweb",
                "testdata/summarycode-golden/summary/embedded_packages/bunkerweb.expected.json",
            ),
            (
                "testdata/summarycode-golden/summary/package_copyright_precedence",
                "testdata/summarycode-golden/summary/package_copyright_precedence/package_copyright_precedence.expected.json",
            ),
        ];

        for (fixture_dir, expected_file) in fixtures {
            assert_summary_fixture_matches_expected(fixture_dir, expected_file, true, true);
        }
    }

    #[test]
    fn test_golden_score_fixtures_match_expected_summary_blocks() {
        let fixtures = [
            (
                "testdata/summarycode-golden/score/basic",
                "testdata/summarycode-golden/score/basic-expected.json",
            ),
            (
                "testdata/summarycode-golden/score/no_license_text",
                "testdata/summarycode-golden/score/no_license_text-expected.json",
            ),
            (
                "testdata/summarycode-golden/score/no_license_or_copyright",
                "testdata/summarycode-golden/score/no_license_or_copyright-expected.json",
            ),
            (
                "testdata/summarycode-golden/score/no_license_ambiguity",
                "testdata/summarycode-golden/score/no_license_ambiguity-expected.json",
            ),
            (
                "testdata/summarycode-golden/score/inconsistent_licenses_copyleft",
                "testdata/summarycode-golden/score/inconsistent_licenses_copyleft-expected.json",
            ),
            (
                "testdata/summarycode-golden/score/jar",
                "testdata/summarycode-golden/score/jar-expected.json",
            ),
        ];

        for (fixture_dir, expected_file) in fixtures {
            assert_summary_fixture_matches_expected(fixture_dir, expected_file, false, true);
        }
    }

    #[test]
    fn test_golden_classify_cli_fixture_matches_expected_output() {
        assert_classify_fixture_matches_expected(
            "testdata/summarycode-golden/classify/cli",
            "testdata/summarycode-golden/classify/cli.expected.json",
            true,
        );
    }

    #[test]
    fn test_golden_classify_with_package_data_fixture_matches_expected_output() {
        assert_classify_fixture_matches_expected(
            "testdata/summarycode-golden/score/jar",
            "testdata/summarycode-golden/classify/with_package_data.expected.json",
            false,
        );
    }

    #[test]
    fn test_golden_file_info_cli_fixture_matches_expected_output() {
        assert_file_info_fixture_matches_expected(
            "testdata/summarycode-golden/classify/cli",
            "testdata/summarycode-golden/classify/cli.expected.json",
            true,
        );
    }

    #[test]
    fn test_golden_file_info_with_package_data_fixture_matches_expected_output() {
        assert_file_info_fixture_matches_expected(
            "testdata/summarycode-golden/score/jar",
            "testdata/summarycode-golden/classify/with_package_data.file_info.expected.json",
            false,
        );
    }

    #[test]
    fn test_golden_generated_cli_fixture_matches_expected_file_flags() {
        let generated_root = Path::new("testdata/summarycode-golden/generated");
        let fixture_root = generated_root.join("simple");
        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(&fixture_root, 0, &fixture_exclude_patterns(&fixture_root));
        let mut files = process_collected(
            &collected,
            progress,
            None,
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: false,
                detect_generated: true,
                ..TextDetectionOptions::default()
            },
        )
        .files;

        normalize_paths_for_test(
            &mut files,
            generated_root
                .to_str()
                .expect("fixture path should be UTF-8"),
        );
        materialize_generated_flags(&mut files);
        let actual = serde_json::json!({
            "files": files
                .into_iter()
                .map(|file| serde_json::json!({
                    "path": file.path,
                    "type": file.file_type,
                    "is_generated": file.is_generated,
                    "scan_errors": file.scan_errors,
                }))
                .collect::<Vec<_>>()
        });
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string("testdata/summarycode-golden/generated/cli.expected.json")
                .expect("expected generated cli fixture should be readable"),
        )
        .expect("expected generated cli fixture should parse");

        let mut actual_normalized = actual;
        let mut expected_normalized = expected;
        normalize_scan_json(&mut actual_normalized, None);
        normalize_scan_json(&mut expected_normalized, None);

        if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
            panic!(
                "Generated CLI fixture mismatch: {}\nactual={}\nexpected={}",
                error,
                serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
                serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
            );
        }
    }

    #[test]
    fn test_golden_fedora_binary_rootfs_contact_fixture_matches_expected_output() {
        fn sorted(mut values: Vec<String>) -> Vec<String> {
            values.sort();
            values
        }

        let temp_dir = tempdir().expect("temporary fixture dir should be created");
        let fixture_root = temp_dir.path().join("binary_rootfs_contacts");
        fs::create_dir_all(&fixture_root).expect("fixture root should be created");

        let mut bytes = b"abcd\0\xff".repeat(525_000);
        bytes.extend_from_slice(
            b"Patch by Andreas Schneider <asn@redhat.com>\0\xff\
original work done by Mr. Sam <sam@email-scan.com>\0\xff\
same for both OpenSSL and NSS by Rob Crittenden (rcritten@redhat.com)\0\xff\
jakub@redhat.com\0\xffjakub@redhat.com\0\xffcontyk@redhat.com\0\xff\
http://tukaani.org/xz/\0\xffhttps://publicsuffix.org/\0\xffhttp://gmail.com/\0\xff\
Copyright - split out libs\0\xff",
        );
        fs::write(fixture_root.join("fedora.bin"), bytes).expect("fixture file should be written");

        let progress = Arc::new(ScanProgress::new(ProgressMode::Quiet));
        let collected = collect_paths(&fixture_root, 0, &fixture_exclude_patterns(&fixture_root));
        let mut files = process_collected(
            &collected,
            progress,
            Some(super::super::test_utils::test_license_engine()),
            LicenseScanOptions::default(),
            &TextDetectionOptions {
                collect_info: true,
                detect_packages: true,
                detect_application_packages: true,
                detect_system_packages: true,
                detect_packages_in_compiled: false,
                detect_copyrights: true,
                detect_emails: true,
                detect_urls: true,
                ..TextDetectionOptions::default()
            },
        )
        .files;

        normalize_paths_for_test(
            &mut files,
            temp_dir.path().to_str().expect("temp path should be UTF-8"),
        );

        let actual = json!({
            "files": files
                .into_iter()
                .filter(|file| file.file_type == FileType::File)
                .map(|file| json!({
                    "path": file.path,
                    "type": file.file_type,
                    "authors": sorted(file.authors.into_iter().map(|author| author.author).collect::<Vec<_>>()),
                    "emails": sorted(file.emails.into_iter().map(|email| email.email).collect::<Vec<_>>()),
                    "urls": sorted(file.urls.into_iter().map(|url| url.url).collect::<Vec<_>>()),
                    "copyrights": sorted(file.copyrights.into_iter().map(|copyright| copyright.copyright).collect::<Vec<_>>()),
                    "holders": sorted(file.holders.into_iter().map(|holder| holder.holder).collect::<Vec<_>>()),
                    "scan_errors": file.scan_errors,
                }))
                .collect::<Vec<_>>()
        });

        let expected: Value = serde_json::from_str(
            &fs::read_to_string(
                "testdata/summarycode-golden/file-info/fedora_binary_rootfs_contacts.expected.json",
            )
            .expect("expected file should be readable"),
        )
        .expect("expected file should parse");

        let mut actual_normalized = actual;
        let mut expected_normalized = expected;
        normalize_scan_json(&mut actual_normalized, None);
        normalize_scan_json(&mut expected_normalized, None);

        if let Err(error) = compare_scan_json_values(&actual_normalized, &expected_normalized, "") {
            panic!(
                "Fedora binary rootfs contact fixture mismatch: {}\nactual={}\nexpected={}",
                error,
                serde_json::to_string_pretty(&actual_normalized).unwrap_or_default(),
                serde_json::to_string_pretty(&expected_normalized).unwrap_or_default()
            );
        }
    }

    #[test]
    fn test_golden_tallies_full_fixture_matches_expected_output() {
        assert_tally_fixture_matches_expected(
            "testdata/summarycode-golden/tallies/full_tallies",
            "testdata/summarycode-golden/tallies/full_tallies/tallies.expected.json",
            FixtureOutputOptions {
                facet_defs: &[],
                include_classify: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: true,
                include_tallies_of_key_files: false,
                include_tallies_with_details: false,
                include_tallies_by_facet: false,
                include_generated: false,
                include_top_level_license_data: false,
            },
        );
    }

    #[test]
    fn test_golden_tallies_with_details_fixture_matches_expected_output() {
        assert_tally_fixture_matches_expected(
            "testdata/summarycode-golden/tallies/full_tallies",
            "testdata/summarycode-golden/tallies/full_tallies/tallies_details.expected.json",
            FixtureOutputOptions {
                facet_defs: &[],
                include_classify: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: false,
                include_tallies_of_key_files: false,
                include_tallies_with_details: true,
                include_tallies_by_facet: false,
                include_generated: false,
                include_top_level_license_data: false,
            },
        );
    }

    #[test]
    fn test_golden_tallies_key_files_fixture_matches_expected_output() {
        assert_tally_fixture_matches_expected(
            "testdata/summarycode-golden/tallies/full_tallies",
            "testdata/summarycode-golden/tallies/full_tallies/tallies_key_files.expected.json",
            FixtureOutputOptions {
                facet_defs: &[],
                include_classify: true,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: true,
                include_tallies_of_key_files: true,
                include_tallies_with_details: false,
                include_tallies_by_facet: false,
                include_generated: false,
                include_top_level_license_data: false,
            },
        );
    }

    #[test]
    fn test_golden_tallies_by_facet_fixture_matches_expected_output() {
        let facet_defs = vec![
            "dev=*.java".to_string(),
            "dev=*.cs".to_string(),
            "dev=*ada*".to_string(),
            "data=*.S".to_string(),
            "tests=*infback9*".to_string(),
            "docs=*README".to_string(),
        ];

        assert_tally_fixture_matches_expected(
            "testdata/summarycode-golden/tallies/full_tallies",
            "testdata/summarycode-golden/tallies/full_tallies/tallies_by_facet.expected.json",
            FixtureOutputOptions {
                facet_defs: &facet_defs,
                include_classify: false,
                include_summary: false,
                include_license_clarity_score: false,
                include_tallies: true,
                include_tallies_of_key_files: false,
                include_tallies_with_details: false,
                include_tallies_by_facet: true,
                include_generated: false,
                include_top_level_license_data: false,
            },
        );
    }

    #[test]
    fn test_golden_facet_cli_fixture_matches_expected_output() {
        let facet_defs = vec![
            "dev=*.c".to_string(),
            "tests=*/tests/*".to_string(),
            "data=*.json".to_string(),
            "docs=*/docs/*".to_string(),
        ];

        assert_facet_fixture_matches_expected(
            "testdata/summarycode-golden/facet",
            "testdata/summarycode-golden/facet/cli.expected.json",
            &facet_defs,
        );
    }

    #[test]
    fn test_golden_package_fixture_matches_expected_output() {
        assert_package_fixture_matches_expected(
            "testdata/summarycode-golden/tallies/packages",
            "testdata/summarycode-golden/tallies/packages/expected.json",
        );
    }

    #[test]
    fn test_golden_reference_follow_manifest_origin_local_file() {
        assert_reference_follow_fixture_matches_expected(
            "testdata/summarycode-golden/reference_following/manifest_origin_local_file",
            "testdata/summarycode-golden/reference_following/manifest_origin_local_file/expected.json",
        );
    }

    #[test]
    fn test_golden_reference_follow_license_beside_manifest() {
        assert_reference_follow_fixture_matches_expected(
            "testdata/summarycode-golden/reference_following/license_beside_manifest",
            "testdata/summarycode-golden/reference_following/license_beside_manifest/expected.json",
        );
    }

    #[test]
    fn test_golden_reference_follow_readme_mit_see_license() {
        assert_reference_follow_fixture_matches_expected(
            "testdata/summarycode-golden/reference_following/readme_mit_see_license",
            "testdata/summarycode-golden/reference_following/readme_mit_see_license/expected.json",
        );
    }

    #[test]
    fn test_golden_reference_follow_file_to_package_inheritance() {
        assert_reference_follow_fixture_matches_expected(
            "testdata/summarycode-golden/reference_following/file_to_package_inheritance",
            "testdata/summarycode-golden/reference_following/file_to_package_inheritance/expected.json",
        );
    }

    #[test]
    fn test_golden_reference_follow_root_fallback_no_package() {
        assert_reference_follow_fixture_matches_expected(
            "testdata/summarycode-golden/reference_following/root_fallback_no_package",
            "testdata/summarycode-golden/reference_following/root_fallback_no_package/expected.json",
        );
    }
}
