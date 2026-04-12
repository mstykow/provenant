#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::ruby::{
        GemArchiveParser, GemMetadataExtractedParser, GemfileLockParser, GemfileParser,
        GemspecParser,
    };
    use std::path::PathBuf;

    #[test]
    fn test_golden_arel_gemspec() {
        let test_file = PathBuf::from("testdata/ruby-golden/arel-gemspec/arel.gemspec");
        let expected_file =
            PathBuf::from("testdata/ruby-golden/arel-gemspec/arel.gemspec.expected");

        let package_data = GemspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_cat_gemspec() {
        let test_file = PathBuf::from("testdata/ruby-golden/cat-gemspec/cat.gemspec");
        let expected_file = PathBuf::from("testdata/ruby-golden/cat-gemspec/cat.gemspec.expected");

        let package_data = GemspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_oj_gemspec() {
        let test_file = PathBuf::from("testdata/ruby-golden/oj-gemspec/oj.gemspec");
        let expected_file = PathBuf::from("testdata/ruby-golden/oj-gemspec/oj.gemspec.expected");

        let package_data = GemspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_rubocop_gemspec() {
        let test_file = PathBuf::from("testdata/ruby-golden/rubocop-gemspec/rubocop.gemspec");
        let expected_file =
            PathBuf::from("testdata/ruby-golden/rubocop-gemspec/rubocop.gemspec.expected");

        let package_data = GemspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_with_variables_gemspec() {
        let test_file = PathBuf::from("testdata/ruby-golden/with-variables/with_variables.gemspec");
        let expected_file =
            PathBuf::from("testdata/ruby-golden/with-variables/with_variables.gemspec.expected");

        let package_data = GemspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gemfile_lock_git() {
        let test_file = PathBuf::from("testdata/ruby-golden/gemfile-lock-git/Gemfile.lock");
        let expected_file =
            PathBuf::from("testdata/ruby-golden/gemfile-lock-git/Gemfile.lock.expected");

        let package_data = GemfileLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gemfile_lock_path() {
        let test_file = PathBuf::from("testdata/ruby-golden/gemfile-lock-path/Gemfile.lock");
        let expected_file =
            PathBuf::from("testdata/ruby-golden/gemfile-lock-path/Gemfile.lock.expected");

        let package_data = GemfileLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gemfile_source_options() {
        let test_file = PathBuf::from("testdata/ruby-golden/gemfile-source-options/Gemfile");
        let expected_file =
            PathBuf::from("testdata/ruby-golden/gemfile-source-options/Gemfile.expected");

        let package_data = GemfileParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gem_archive() {
        let test_file = PathBuf::from("testdata/ruby/example-gem-1.2.3.gem");
        let expected_file = PathBuf::from("testdata/ruby/example-gem-1.2.3.gem.expected.json");

        let package_data = GemArchiveParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_gem_metadata_extracted() {
        let test_file = PathBuf::from("testdata/gem/extracted/metadata.gz-extract");
        let expected_file =
            PathBuf::from("testdata/gem/extracted/metadata.gz-extract.expected.json");

        let package_data = GemMetadataExtractedParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
