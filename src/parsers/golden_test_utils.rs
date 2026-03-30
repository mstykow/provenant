#[cfg(all(test, feature = "golden-tests"))]
use crate::models::PackageData;
#[cfg(all(test, feature = "golden-tests"))]
use serde_json::Value;
#[cfg(all(test, feature = "golden-tests"))]
use std::fs;
#[cfg(all(test, feature = "golden-tests"))]
use std::path::Path;

#[cfg(all(test, feature = "golden-tests"))]
pub fn compare_package_data_parser_only(
    actual: &PackageData,
    expected_path: &Path,
) -> Result<(), String> {
    let expected_content = fs::read_to_string(expected_path)
        .map_err(|e| format!("Failed to read expected file: {}", e))?;

    let expected_value: Value = serde_json::from_str(&expected_content)
        .map_err(|e| format!("Failed to parse expected JSON: {}", e))?;

    let expected_json = unwrap_expected_parser_package(&expected_value)?;

    let actual_json = serde_json::to_value(actual)
        .map_err(|e| format!("Failed to serialize actual PackageData: {}", e))?;

    compare_json_values_parser_only(&actual_json, expected_json, "")
}

#[cfg(all(test, feature = "golden-tests"))]
pub fn compare_package_data_collection_parser_only(
    actual: &[PackageData],
    expected_path: &Path,
) -> Result<(), String> {
    let expected_content = fs::read_to_string(expected_path)
        .map_err(|e| format!("Failed to read expected file: {}", e))?;

    let expected_value: Value = serde_json::from_str(&expected_content)
        .map_err(|e| format!("Failed to parse expected JSON: {}", e))?;

    let expected_json = unwrap_expected_parser_package_collection(&expected_value)?;

    let actual_json = serde_json::to_value(actual)
        .map_err(|e| format!("Failed to serialize actual PackageData collection: {}", e))?;

    compare_json_values_parser_only(&actual_json, expected_json, "")
}

#[cfg(all(test, feature = "golden-tests"))]
fn unwrap_expected_parser_package(expected_value: &Value) -> Result<&Value, String> {
    if let Some(expected_array) = expected_value.as_array() {
        if expected_array.is_empty() {
            return Err("Expected file contains empty array".to_string());
        }
        return Ok(&expected_array[0]);
    }

    if let Some(package_data) = expected_value
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| file.get("package_data"))
        .and_then(Value::as_array)
    {
        if package_data.is_empty() {
            return Err("Expected file contains empty files[0].package_data array".to_string());
        }
        return Ok(&package_data[0]);
    }

    Ok(expected_value)
}

#[cfg(all(test, feature = "golden-tests"))]
fn unwrap_expected_parser_package_collection(expected_value: &Value) -> Result<&Value, String> {
    if expected_value.is_array() {
        return Ok(expected_value);
    }

    if let Some(packages) = expected_value.get("packages") {
        return Ok(packages);
    }

    if let Some(package_data) = expected_value
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| file.get("package_data"))
    {
        return Ok(package_data);
    }

    Err("Expected file does not contain a package collection".to_string())
}

#[cfg(all(test, feature = "golden-tests"))]
fn compare_json_values_parser_only(
    actual: &Value,
    expected: &Value,
    path: &str,
) -> Result<(), String> {
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
        "package_uid",
        "datafile_paths",
        "datasource_ids",
    ];

    if SKIP_FIELDS.iter().any(|&field| path.ends_with(field)) {
        return Ok(());
    }

    match (actual, expected) {
        (Value::Null, Value::Null) => Ok(()),
        (Value::Null, Value::Object(obj)) if obj.is_empty() => Ok(()),
        (Value::Object(obj), Value::Null) if obj.is_empty() => Ok(()),
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
            for (i, (actual_item, expected_item)) in a.iter().zip(e.iter()).enumerate() {
                let item_path = format!("{}[{}]", path, i);
                compare_json_values_parser_only(actual_item, expected_item, &item_path)?;
            }
            Ok(())
        }

        (Value::Object(a), Value::Object(e)) => {
            if e.is_empty() && path.ends_with("resolved_package") {
                return Ok(());
            }

            let all_keys: std::collections::HashSet<_> = a.keys().chain(e.keys()).collect();

            for key in all_keys {
                let field_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", path, key)
                };

                if SKIP_FIELDS.contains(&key.as_str()) {
                    continue;
                }

                match (a.get(key), e.get(key)) {
                    (Some(actual_val), Some(expected_val)) => {
                        compare_json_values_parser_only(actual_val, expected_val, &field_path)?;
                    }
                    (None, Some(expected_val)) => match expected_val {
                        Value::Null => continue,
                        Value::Bool(false) => continue,
                        Value::Array(arr) if arr.is_empty() => continue,
                        Value::Object(obj) if obj.is_empty() => continue,
                        _ => {
                            if key == "license_detections"
                                || key == "declared_license_expression"
                                || key == "declared_license_expression_spdx"
                                || key == "other_license_detections"
                                || key == "other_license_expression"
                                || key == "other_license_expression_spdx"
                            {
                                continue;
                            }
                            if !SKIP_FIELDS.contains(&key.as_str()) {
                                return Err(format!("Missing field in actual: {}", field_path));
                            }
                        }
                    },
                    (Some(_), None) => {
                        if (key == "license_detections" || key == "dependencies")
                            && a.get(key)
                                .and_then(Value::as_array)
                                .is_some_and(|arr| arr.is_empty())
                        {
                            continue;
                        }
                        return Err(format!("Extra field in actual: {}", field_path));
                    }
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
