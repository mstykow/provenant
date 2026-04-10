#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use regex::Regex;
    use serde_json::{json, Value};
    use tempfile::TempDir;

    use super::super::scan_test_utils::scan_and_assemble_with_stripped_root;

    fn normalize_test_uuids(json_str: &str) -> String {
        let re = Regex::new(r"uuid=[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
            .expect("uuid regex should compile");
        re.replace_all(json_str, "uuid=fixed-uid-done-for-testing-5642512d1758")
            .to_string()
    }

    fn compare_scan_json_values(
        actual: &Value,
        expected: &Value,
        path: &str,
    ) -> Result<(), String> {
        if path.ends_with("package_data") {
            return Ok(());
        }

        match (actual, expected) {
            (Value::Null, Value::Null) => Ok(()),
            (Value::Null, Value::Object(obj))
                if path.ends_with("resolved_package") && obj.is_empty() =>
            {
                Ok(())
            }
            (Value::Object(obj), Value::Null)
                if path.ends_with("resolved_package") && obj.is_empty() =>
            {
                Ok(())
            }
            (Value::Bool(a), Value::Bool(e)) if a == e => Ok(()),
            (Value::Number(a), Value::Number(e)) if a == e => Ok(()),
            (Value::String(a), Value::String(e)) if a == e => Ok(()),
            (Value::Array(a), Value::Array(e)) => {
                if a.len() != e.len() {
                    return Err(format!(
                        "Array length mismatch at {}: actual={}, expected={}",
                        path,
                        a.len(),
                        e.len()
                    ));
                }

                for (index, (actual_item, expected_item)) in a.iter().zip(e.iter()).enumerate() {
                    let item_path = if path.is_empty() {
                        format!("[{}]", index)
                    } else {
                        format!("{}[{}]", path, index)
                    };
                    compare_scan_json_values(actual_item, expected_item, &item_path)?;
                }

                Ok(())
            }
            (Value::Object(a), Value::Object(e)) => {
                if path.ends_with("resolved_package") && e.is_empty() {
                    return Ok(());
                }

                for key in e.keys() {
                    if !a.contains_key(key) {
                        match e.get(key) {
                            Some(Value::Null) => continue,
                            Some(Value::Bool(false)) => continue,
                            Some(Value::Array(values)) if values.is_empty() => continue,
                            Some(Value::Object(values)) if values.is_empty() => continue,
                            _ => {
                                let field_path = if path.is_empty() {
                                    key.to_string()
                                } else {
                                    format!("{}.{}", path, key)
                                };
                                return Err(format!("Missing key in actual: {}", field_path));
                            }
                        }
                    }
                }

                for key in a.keys() {
                    if !e.contains_key(key) {
                        if path.ends_with("extra_data") {
                            continue;
                        }

                        match a.get(key) {
                            Some(Value::Null) => continue,
                            Some(Value::Bool(false)) => continue,
                            Some(Value::Array(values)) if values.is_empty() => continue,
                            Some(Value::Object(values)) if values.is_empty() => continue,
                            _ => {
                                let field_path = if path.is_empty() {
                                    key.to_string()
                                } else {
                                    format!("{}.{}", path, key)
                                };
                                return Err(format!("Extra key in actual: {}", field_path));
                            }
                        }
                    }
                }

                for key in a.keys() {
                    if let (Some(actual_val), Some(expected_val)) = (a.get(key), e.get(key)) {
                        let field_path = if path.is_empty() {
                            key.to_string()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        compare_scan_json_values(actual_val, expected_val, &field_path)?;
                    }
                }

                Ok(())
            }
            _ => Err(format!(
                "Type or value mismatch at {}: actual={}, expected={}",
                path,
                serde_json::to_string(actual).unwrap_or_default(),
                serde_json::to_string(expected).unwrap_or_default()
            )),
        }
    }

    fn normalize_scan_json(value: &mut Value, parent_key: Option<&str>) {
        match value {
            Value::Array(values) => {
                for item in values.iter_mut() {
                    normalize_scan_json(item, parent_key);
                }

                if parent_key.is_some_and(|key| {
                    matches!(
                        key,
                        "packages"
                            | "dependencies"
                            | "files"
                            | "package_data"
                            | "datafile_paths"
                            | "datasource_ids"
                            | "for_packages"
                    )
                }) {
                    values
                        .sort_by_cached_key(|item| serde_json::to_string(item).unwrap_or_default());
                }
            }
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    normalize_scan_json(item, Some(key));
                }
            }
            _ => {}
        }
    }

    fn swift_scan_and_assemble(path: &Path) -> Value {
        let (mut files, assembly_result) = scan_and_assemble_with_stripped_root(path);

        files.sort_by(|left, right| left.path.cmp(&right.path));
        let files_json: Vec<Value> = files
            .into_iter()
            .filter(|file| !file.path.is_empty())
            .map(|file| {
                json!({
                    "path": file.path,
                    "type": file.file_type,
                    "package_data": file.package_data,
                    "for_packages": file.for_packages,
                    "scan_errors": file.scan_errors,
                })
            })
            .collect();

        let output_packages: Vec<crate::output_schema::OutputPackage> = assembly_result
            .packages
            .iter()
            .map(crate::output_schema::OutputPackage::from)
            .collect();
        let output_deps: Vec<crate::output_schema::OutputTopLevelDependency> = assembly_result
            .dependencies
            .iter()
            .map(crate::output_schema::OutputTopLevelDependency::from)
            .collect();
        json!({
            "packages": output_packages,
            "dependencies": output_deps,
            "files": files_json,
        })
    }

    fn assert_swift_scan_matches_expected(fixture_dir: &str, expected_file: &str) {
        let actual = swift_scan_and_assemble(Path::new(fixture_dir));
        let actual_str =
            serde_json::to_string_pretty(&actual).expect("actual scan JSON should serialize");
        let expected_str =
            fs::read_to_string(expected_file).expect("expected scan JSON should be readable");

        let actual_normalized = normalize_test_uuids(&actual_str);
        let expected_normalized = normalize_test_uuids(&expected_str);

        let mut actual_value: Value =
            serde_json::from_str(&actual_normalized).expect("normalized actual JSON should parse");
        let mut expected_value: Value = serde_json::from_str(&expected_normalized)
            .expect("normalized expected JSON should parse");

        normalize_scan_json(&mut actual_value, None);
        normalize_scan_json(&mut expected_value, None);

        if let Err(error) = compare_scan_json_values(&actual_value, &expected_value, "") {
            panic!(
                "Swift scan golden mismatch for fixture {} vs {}: {}",
                fixture_dir, expected_file, error
            );
        }
    }

    #[test]
    fn test_swift_scan_uses_show_dependencies_only_fixture() {
        assert_swift_scan_matches_expected(
            "testdata/swift-golden/packages/vercelui_show_dependencies",
            "testdata/swift-golden/swift-vercelui-show-dependencies-expected.json",
        );
    }

    #[test]
    fn test_swift_scan_uses_resolved_only_fixture() {
        assert_swift_scan_matches_expected(
            "testdata/swift-golden/packages/fastlane_resolved_v1",
            "testdata/swift-golden/swift-fastlane-resolved-v1-package-expected.json",
        );
    }

    #[test]
    fn test_swift_scan_prefers_show_dependencies_over_manifest_dependencies() {
        assert_swift_scan_matches_expected(
            "testdata/swift-golden/packages/vercelui",
            "testdata/swift-golden/swift-vercelui-expected.json",
        );
    }

    #[test]
    fn test_swift_scan_falls_back_to_resolved_when_show_dependencies_missing() {
        assert_swift_scan_matches_expected(
            "testdata/swift-golden/packages/mapboxmaps_manifest_and_resolved",
            "testdata/swift-golden/swift-mapboxmaps-manifest-and-resolved-package-expected.json",
        );
    }

    #[test]
    fn test_swift_scan_supports_manifest_and_resolved_alias_surfaces() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/swift/Package.swift.deplock",
            temp_dir.path().join("Package.swift.deplock"),
        )
        .expect("copy manifest alias fixture");
        fs::copy(
            "testdata/swift/Package-v1.resolved",
            temp_dir.path().join(".package.resolved"),
        )
        .expect("copy resolved alias fixture");

        let scanned = swift_scan_and_assemble(temp_dir.path());
        let packages = scanned
            .get("packages")
            .and_then(|value| value.as_array())
            .expect("packages should be present");

        assert_eq!(packages.len(), 1);
        let package = &packages[0];
        let datafile_paths = package
            .get("datafile_paths")
            .and_then(|value| value.as_array())
            .expect("datafile_paths should be present");
        assert!(datafile_paths
            .iter()
            .any(|path| path.as_str() == Some("Package.swift.deplock")));
        assert!(datafile_paths
            .iter()
            .any(|path| path.as_str() == Some(".package.resolved")));
    }
}
