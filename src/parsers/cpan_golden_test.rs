// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::cpan::{CpanManifestParser, CpanMetaJsonParser, CpanMetaYmlParser};
    use crate::parsers::cpan_dist_ini::CpanDistIniParser;
    use crate::parsers::cpan_makefile_pl::CpanMakefilePlParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use std::path::PathBuf;

    fn run_golden(parser_type: &str, test_file: &str, expected_file: &str) {
        let test_path = PathBuf::from(test_file);
        let package_data = match parser_type {
            "manifest" => CpanManifestParser::extract_first_package(&test_path),
            "meta-json" => CpanMetaJsonParser::extract_first_package(&test_path),
            "meta-yml" => CpanMetaYmlParser::extract_first_package(&test_path),
            "dist-ini" => CpanDistIniParser::extract_first_package(&test_path),
            "makefile" => CpanMakefilePlParser::extract_first_package(&test_path),
            _ => panic!("Unknown parser type: {}", parser_type),
        };

        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("CPAN golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_cpan_manifest() {
        run_golden(
            "manifest",
            "testdata/cpan/manifest/MANIFEST",
            "testdata/cpan/manifest/MANIFEST.expected.json",
        );
    }

    #[test]
    fn test_golden_cpan_meta_json() {
        run_golden(
            "meta-json",
            "testdata/cpan/meta_json/META.json",
            "testdata/cpan/meta_json/META.json.expected.json",
        );
    }

    #[test]
    fn test_golden_cpan_meta_yml() {
        run_golden(
            "meta-yml",
            "testdata/cpan/meta_yml/META.yml",
            "testdata/cpan/meta_yml/META.yml.expected.json",
        );
    }

    #[test]
    fn test_golden_cpan_dist_ini() {
        run_golden(
            "dist-ini",
            "testdata/cpan/dist-ini/basic/dist.ini",
            "testdata/cpan/dist-ini/basic/dist.ini.expected.json",
        );
    }

    #[test]
    fn test_golden_cpan_makefile_pl() {
        run_golden(
            "makefile",
            "testdata/cpan/makefile-pl/basic/Makefile.PL",
            "testdata/cpan/makefile-pl/basic/Makefile.PL.expected.json",
        );
    }

    #[test]
    fn test_golden_cpan_makefile_pl_version_abstract_from() {
        run_golden(
            "makefile",
            "testdata/cpan/makefile-pl/version-abstract-from/Makefile.PL",
            "testdata/cpan/makefile-pl/version-abstract-from/Makefile.PL.expected.json",
        );
    }
}
