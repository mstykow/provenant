// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use crate::models::PackageData;
    use crate::parsers::PackageParser;
    use crate::parsers::conan::{ConanFilePyParser, ConanLockParser, ConanfileTxtParser};
    use crate::parsers::conan_data::ConanDataParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;

    fn sort_packages_by_version(packages: &mut [PackageData]) {
        packages.sort_by(|left, right| left.version.cmp(&right.version));
    }

    fn compare_values(actual: &Value, expected: &Value, path: &str) -> Result<(), String> {
        const SKIP_FIELDS: &[&str] = &[
            "identifier",
            "matched_text",
            "matcher",
            "matched_length",
            "match_coverage",
            "rule_relevance",
            "rule_identifier",
            "rule_url",
            "start_line",
            "end_line",
            "extra_data",
        ];

        if SKIP_FIELDS.iter().any(|field| path.ends_with(field)) {
            return Ok(());
        }

        match (actual, expected) {
            (Value::Null, Value::Null) => Ok(()),
            (Value::Null, Value::Object(obj)) if obj.is_empty() => Ok(()),
            (Value::Object(obj), Value::Null) if obj.is_empty() => Ok(()),
            (Value::Bool(a), Value::Bool(e)) if a == e => Ok(()),
            (Value::Number(a), Value::Number(e)) if a == e => Ok(()),
            (Value::String(a), Value::String(e)) if a == e => Ok(()),
            (Value::Array(actual_items), Value::Array(expected_items)) => {
                if actual_items.len() != expected_items.len() {
                    return Err(format!(
                        "Array length mismatch at {}: actual={}, expected={}",
                        path,
                        actual_items.len(),
                        expected_items.len()
                    ));
                }

                for (index, (actual_item, expected_item)) in
                    actual_items.iter().zip(expected_items.iter()).enumerate()
                {
                    let item_path = format!("{}[{}]", path, index);
                    compare_values(actual_item, expected_item, &item_path)?;
                }

                Ok(())
            }
            (Value::Object(actual_obj), Value::Object(expected_obj)) => {
                let all_keys: std::collections::HashSet<_> =
                    actual_obj.keys().chain(expected_obj.keys()).collect();

                for key in all_keys {
                    if SKIP_FIELDS.contains(&key.as_str()) {
                        continue;
                    }

                    let field_path = if path.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}.{}", path, key)
                    };

                    match (actual_obj.get(key), expected_obj.get(key)) {
                        (Some(actual_val), Some(expected_val)) => {
                            compare_values(actual_val, expected_val, &field_path)?;
                        }
                        (None, Some(expected_val)) => match expected_val {
                            Value::Null => continue,
                            Value::Bool(false) => continue,
                            Value::Array(values) if values.is_empty() => continue,
                            Value::Object(values) if values.is_empty() => continue,
                            _ => return Err(format!("Missing field in actual: {}", field_path)),
                        },
                        (Some(actual_val), None) => match actual_val {
                            Value::Null => continue,
                            Value::Bool(false) => continue,
                            Value::Array(values) if values.is_empty() => continue,
                            Value::Object(values) if values.is_empty() => continue,
                            _ => return Err(format!("Extra field in actual: {}", field_path)),
                        },
                        (None, None) => unreachable!(),
                    }
                }

                Ok(())
            }
            _ => Err(format!(
                "Type mismatch at {}: actual={:?}, expected={:?}",
                path, actual, expected
            )),
        }
    }

    fn run_conandata_golden(test_file: &str, expected_file: &str) {
        let test_path = PathBuf::from(test_file);
        let mut actual_packages = ConanDataParser::extract_packages(&test_path);
        sort_packages_by_version(&mut actual_packages);

        let expected_content =
            std::fs::read_to_string(expected_file).expect("expected Conan golden should exist");
        let mut expected_packages: Vec<PackageData> =
            serde_json::from_str(&expected_content).expect("expected Conan golden should parse");
        sort_packages_by_version(&mut expected_packages);

        let actual_value =
            serde_json::to_value(actual_packages).expect("actual Conan packages should serialize");
        let expected_value = serde_json::to_value(expected_packages)
            .expect("expected Conan packages should serialize");

        if let Err(error) = compare_values(&actual_value, &expected_value, "packages") {
            panic!(
                "Conan golden test failed for {} vs {}: {}",
                test_file, expected_file, error
            );
        }
    }

    fn run_single_golden(parser_type: &str, test_file: &str, expected_file: &str) {
        let test_path = PathBuf::from(test_file);
        let package_data = match parser_type {
            "conanfile-py" => ConanFilePyParser::extract_first_package(&test_path),
            "conanfile-txt" => ConanfileTxtParser::extract_first_package(&test_path),
            "conan-lock" => ConanLockParser::extract_first_package(&test_path),
            _ => panic!("Unknown Conan parser type: {}", parser_type),
        };

        match compare_package_data_parser_only(&package_data, &PathBuf::from(expected_file)) {
            Ok(()) => {}
            Err(error) => panic!("Conan golden test failed for {}: {}", test_file, error),
        }
    }

    #[test]
    fn test_golden_conandata_boost() {
        run_conandata_golden(
            "testdata/conan/recipes/boost/manifest/conandata.yml",
            "testdata/conan/recipes/boost/conandata.yml-expected.json",
        );
    }

    #[test]
    fn test_golden_conandata_libgettext() {
        run_conandata_golden(
            "testdata/conan/recipes/libgettext/manifest/conandata.yml",
            "testdata/conan/recipes/libgettext/conandata.yml-expected.json",
        );
    }

    #[test]
    fn test_golden_conandata_libzip() {
        run_conandata_golden(
            "testdata/conan/recipes/libzip/manifest/conandata.yml",
            "testdata/conan/recipes/libzip/conandata.yml-expected.json",
        );
    }

    #[test]
    fn test_golden_conanfile_py() {
        run_single_golden(
            "conanfile-py",
            "testdata/conan/recipes/libgettext/manifest/conanfile.py",
            "testdata/conan/recipes/libgettext/manifest/conanfile.py-expected.json",
        );
    }

    #[test]
    fn test_golden_conanfile_txt() {
        run_single_golden(
            "conanfile-txt",
            "testdata/conan/conanfile.txt",
            "testdata/conan/conanfile.txt-expected.json",
        );
    }

    #[test]
    fn test_golden_conan_lock() {
        run_single_golden(
            "conan-lock",
            "testdata/conan/conan.lock",
            "testdata/conan/conan.lock-expected.json",
        );
    }
}
