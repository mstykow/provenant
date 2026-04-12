#[cfg(all(test, feature = "golden-tests"))]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use regex::Regex;
    use serde_json::{Value, json};

    use crate::assembly::{AssemblyResult, assemble};
    use crate::models::{FileInfo, FileType};
    use crate::parsers::try_parse_file;

    /// Normalize all UUID v4 values to a fixed placeholder for deterministic testing.
    ///
    /// Replaces `uuid=<any-uuid-v4>` with `uuid=fixed-uid-done-for-testing-5642512d1758`
    /// to match the format used in cocoapods golden tests.
    fn normalize_uuids(json_str: &str) -> String {
        // UUID v4 pattern: 8-4-4-4-12 hex chars
        let re = Regex::new(r"uuid=[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
            .unwrap();
        re.replace_all(json_str, "uuid=fixed-uid-done-for-testing-5642512d1758")
            .to_string()
    }

    /// Build FileInfo objects from real files in a test directory.
    ///
    /// This discovers all parseable files in the directory (recursively), runs the appropriate parser,
    /// and constructs FileInfo objects with relative paths (required for proper assembly grouping).
    fn build_file_infos_from_directory(test_dir: &Path) -> Result<Vec<FileInfo>, String> {
        let mut file_infos = Vec::new();

        visit_dir_recursive(test_dir, test_dir, &mut file_infos)?;

        if file_infos.is_empty() {
            return Err(format!("No files found in directory: {:?}", test_dir));
        }

        // At least one file must have package data for assembly to be meaningful
        let has_package_data = file_infos.iter().any(|f| !f.package_data.is_empty());
        if !has_package_data {
            return Err(format!(
                "No parseable files found in directory: {:?}",
                test_dir
            ));
        }

        // Sort by path for deterministic order across platforms
        // (fs::read_dir order is OS-dependent)
        file_infos.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(file_infos)
    }

    fn visit_dir_recursive(
        dir: &Path,
        base_dir: &Path,
        file_infos: &mut Vec<FileInfo>,
    ) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                visit_dir_recursive(&path, base_dir, file_infos)?;
                continue;
            }

            if !path.is_file() {
                continue;
            }

            if path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .contains("expected.json")
            {
                continue;
            }

            let package_data_vec = try_parse_file(&path)
                .map(|result| result.packages)
                .unwrap_or_default();

            let relative_path = path
                .strip_prefix(base_dir)
                .map_err(|e| format!("Failed to strip prefix: {}", e))?
                .to_str()
                .ok_or_else(|| format!("Invalid path: {:?}", path))?
                .to_string();

            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();

            let metadata =
                fs::metadata(&path).map_err(|e| format!("Failed to read file metadata: {}", e))?;
            let size = metadata.len();

            let file_info = FileInfo {
                name: file_name.clone(),
                base_name: file_name.clone(),
                extension,
                path: relative_path,
                file_type: FileType::File,
                mime_type: Some("text/plain".to_string()),
                file_type_label: None,
                size,
                date: None,
                sha1: None,
                md5: None,
                sha256: None,
                programming_language: None,
                package_data: package_data_vec,
                license_expression: None,
                license_detections: vec![],
                license_clues: vec![],
                percentage_of_license_text: None,
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                emails: vec![],
                urls: vec![],
                for_packages: vec![],
                scan_errors: vec![],
                license_policy: None,
                is_source: None,
                files_count: None,
                dirs_count: None,
                size_count: None,
                source_count: None,
                is_legal: false,
                is_manifest: false,
                is_readme: false,
                is_top_level: false,
                is_key_file: false,
                is_community: false,
                is_generated: None,
                facets: vec![],
                tallies: None,
                sha1_git: None,
                is_binary: None,
                is_text: None,
                is_archive: None,
                is_media: None,
                is_script: None,
            };

            file_infos.push(file_info);
        }

        Ok(())
    }

    fn compare_assembly_output(
        actual: &AssemblyResult,
        file_infos: &[FileInfo],
        expected_path: &Path,
    ) -> Result<(), String> {
        let expected_str = fs::read_to_string(expected_path)
            .map_err(|e| format!("Failed to read expected file: {}", e))?;

        let file_for_packages: Vec<Value> = file_infos
            .iter()
            .filter(|f| !f.for_packages.is_empty())
            .map(|f| {
                json!({
                    "path": f.path,
                    "for_packages": f.for_packages,
                })
            })
            .collect();

        let output_packages: Vec<crate::output_schema::OutputPackage> = actual
            .packages
            .iter()
            .map(crate::output_schema::OutputPackage::from)
            .collect();
        let output_deps: Vec<crate::output_schema::OutputTopLevelDependency> = actual
            .dependencies
            .iter()
            .map(crate::output_schema::OutputTopLevelDependency::from)
            .collect();
        let actual_json = json!({
            "packages": output_packages,
            "dependencies": output_deps,
            "files_with_packages": file_for_packages,
        });
        let actual_str = serde_json::to_string_pretty(&actual_json)
            .map_err(|e| format!("Failed to serialize actual result: {}", e))?;

        let actual_normalized = normalize_uuids(&actual_str);
        let expected_normalized = normalize_uuids(&expected_str);

        let actual_value: Value = serde_json::from_str(&actual_normalized)
            .map_err(|e| format!("Failed to parse normalized actual JSON: {}", e))?;
        let expected_value: Value = serde_json::from_str(&expected_normalized)
            .map_err(|e| format!("Failed to parse normalized expected JSON: {}", e))?;

        compare_json_values(&actual_value, &expected_value, "")
    }

    fn assert_npm_nested_fixture_inputs(file_infos: &[FileInfo]) -> Result<(), String> {
        let expected_paths = vec![
            ".pnp.cjs".to_string(),
            "index.js".to_string(),
            "node_modules/child/index.js".to_string(),
            "node_modules/child/node_modules/grand/index.js".to_string(),
            "node_modules/child/node_modules/grand/package.json".to_string(),
            "node_modules/child/package.json".to_string(),
            "package.json".to_string(),
        ];
        let actual_paths: Vec<String> = file_infos.iter().map(|file| file.path.clone()).collect();

        if actual_paths != expected_paths {
            return Err(format!(
                "Unexpected fixture inputs for npm-nested-packages: actual={:?}, expected={:?}",
                actual_paths, expected_paths
            ));
        }

        Ok(())
    }

    /// Recursively compare two JSON values with helpful error messages.
    fn compare_json_values(actual: &Value, expected: &Value, path: &str) -> Result<(), String> {
        fn is_tolerable_default_field(key: &str, value: &Value) -> bool {
            match value {
                Value::Null => true,
                Value::Bool(false) => true,
                Value::Array(arr) if arr.is_empty() => true,
                Value::Object(obj) if obj.is_empty() => true,
                Value::String(s) if key == "namespace" && s.is_empty() => true,
                _ => false,
            }
        }

        match (actual, expected) {
            (Value::Null, Value::Null) => Ok(()),
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
                    let item_path = if path.is_empty() {
                        format!("[{}]", i)
                    } else {
                        format!("{}[{}]", path, i)
                    };
                    compare_json_values(actual_item, expected_item, &item_path)?;
                }
                Ok(())
            }

            (Value::Object(a), Value::Object(e)) => {
                // Check all keys exist in both objects
                for key in e.keys() {
                    if !a.contains_key(key) {
                        // Allow missing empty values
                        match e.get(key) {
                            Some(expected_val) if is_tolerable_default_field(key, expected_val) => {
                                continue;
                            }
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

                // Check no extra keys in actual
                for key in a.keys() {
                    if !e.contains_key(key) {
                        // Allow extra empty values
                        match a.get(key) {
                            Some(actual_val) if is_tolerable_default_field(key, actual_val) => {
                                continue;
                            }
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

                // Compare common keys
                for key in a.keys() {
                    if let (Some(actual_val), Some(expected_val)) = (a.get(key), e.get(key)) {
                        let field_path = if path.is_empty() {
                            key.to_string()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        compare_json_values(actual_val, expected_val, &field_path)?;
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

    /// Run assembly on a test directory and compare against expected output.
    fn run_assembly_golden_test(test_dir_name: &str) -> Result<(), String> {
        let test_dir = PathBuf::from("testdata/assembly-golden").join(test_dir_name);
        let expected_file = test_dir.join("expected.json");

        if !test_dir.exists() {
            return Err(format!("Test directory does not exist: {:?}", test_dir));
        }

        if !expected_file.exists() {
            eprintln!("Expected file not found, generating: {:?}", expected_file);

            let mut file_infos = build_file_infos_from_directory(&test_dir)?;
            let result = assemble(&mut file_infos);

            let file_for_packages: Vec<Value> = file_infos
                .iter()
                .filter(|f| !f.for_packages.is_empty())
                .map(|f| {
                    json!({
                        "path": f.path,
                        "for_packages": f.for_packages,
                    })
                })
                .collect();

            let output_packages: Vec<crate::output_schema::OutputPackage> = result
                .packages
                .iter()
                .map(crate::output_schema::OutputPackage::from)
                .collect();
            let output_deps: Vec<crate::output_schema::OutputTopLevelDependency> = result
                .dependencies
                .iter()
                .map(crate::output_schema::OutputTopLevelDependency::from)
                .collect();
            let output_json = json!({
                "packages": output_packages,
                "dependencies": output_deps,
                "files_with_packages": file_for_packages,
            });
            let output_str = serde_json::to_string_pretty(&output_json)
                .map_err(|e| format!("Failed to serialize output: {}", e))?;

            let normalized = normalize_uuids(&output_str);

            fs::write(&expected_file, normalized)
                .map_err(|e| format!("Failed to write expected file: {}", e))?;

            return Err(format!(
                "Expected file generated at {:?}. Please review and re-run test.",
                expected_file
            ));
        }

        let mut file_infos = build_file_infos_from_directory(&test_dir)?;

        if test_dir_name == "npm-nested-packages" {
            assert_npm_nested_fixture_inputs(&file_infos)?;
        }

        let result = assemble(&mut file_infos);

        compare_assembly_output(&result, &file_infos, &expected_file)
    }

    #[test]
    fn test_assembly_npm_basic() {
        match run_assembly_golden_test("npm-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for npm-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_bun_basic() {
        match run_assembly_golden_test("bun-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for bun-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_cargo_basic() {
        match run_assembly_golden_test("cargo-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for cargo-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_go_basic() {
        match run_assembly_golden_test("go-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for go-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_go_graph_basic() {
        match run_assembly_golden_test("go-graph-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for go-graph-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_ruby_extracted_basic() {
        match run_assembly_golden_test("ruby-extracted-basic") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for ruby-extracted-basic: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_composer_basic() {
        match run_assembly_golden_test("composer-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for composer-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_composer_nested() {
        match run_assembly_golden_test("composer-nested") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for composer-nested: {}", e),
        }
    }

    #[test]
    fn test_assembly_conda_rootfs_assigns_meta_json_files() {
        let test_dir = PathBuf::from("testdata/conda/assembly/opt/conda");
        let mut file_infos = build_file_infos_from_directory(&test_dir)
            .expect("should build file infos from Conda rootfs fixture");
        let result = assemble(&mut file_infos);

        let conda_pkg = result
            .packages
            .iter()
            .find(|pkg| pkg.package_type == Some(crate::models::PackageType::Conda))
            .expect("expected assembled conda package");

        assert_eq!(conda_pkg.name.as_deref(), Some("requests"));
        assert!(
            conda_pkg
                .datasource_ids
                .contains(&crate::models::DatasourceId::CondaMetaJson)
        );
        assert!(
            conda_pkg
                .datasource_ids
                .contains(&crate::models::DatasourceId::CondaMetaYaml)
        );
        assert!(
            conda_pkg
                .datafile_paths
                .iter()
                .any(|path| path.contains("conda-meta/requests-2.32.3-py312h06a4308_1.json"))
        );
        assert!(conda_pkg.datafile_paths.iter().any(|path| {
            path.contains("pkgs/requests-2.32.3-py312h06a4308_1/info/recipe/meta.yaml")
        }));

        let assigned_paths = [
            "lib/python3.12/site-packages/requests/__init__.py",
            "lib/python3.12/site-packages/requests-2.32.3.dist-info/METADATA",
            "lib/python3.12/site-packages/requests-2.32.3.dist-info/LICENSE",
        ];

        for expected_path in assigned_paths {
            let file = file_infos
                .iter()
                .find(|file| file.path == expected_path)
                .unwrap_or_else(|| panic!("missing file fixture: {expected_path}"));
            assert!(
                file.for_packages.contains(&conda_pkg.package_uid),
                "expected {expected_path} to be assigned to {}",
                conda_pkg.package_uid
            );
        }
    }

    #[test]
    fn test_assembly_maven_basic() {
        match run_assembly_golden_test("maven-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for maven-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_maven_meta_inf_basic() {
        match run_assembly_golden_test("maven-meta-inf-basic") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for maven-meta-inf-basic: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_deno_basic() {
        match run_assembly_golden_test("deno-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for deno-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_go_workspace_basic() {
        match run_assembly_golden_test("go-workspace-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for go-workspace-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_hackage_basic() {
        match run_assembly_golden_test("hackage-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for hackage-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_nix_flake_compat_basic() {
        match run_assembly_golden_test("nix-flake-compat-basic") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nix-flake-compat-basic: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_helm_basic() {
        match run_assembly_golden_test("helm-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for helm-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_nuget_basic() {
        match run_assembly_golden_test("nuget-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for nuget-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_nuget_deps_json_basic() {
        match run_assembly_golden_test("nuget-deps-json-basic") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nuget-deps-json-basic: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_nuget_cpm_nearest_ancestor() {
        match run_assembly_golden_test("nuget-cpm-nearest-ancestor") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nuget-cpm-nearest-ancestor: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_nuget_cpm_version_override() {
        match run_assembly_golden_test("nuget-cpm-version-override") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nuget-cpm-version-override: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_nuget_cpm_imported_parent() {
        match run_assembly_golden_test("nuget-cpm-imported-parent") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nuget-cpm-imported-parent: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_nuget_cpm_directory_build_nearest_ancestor() {
        match run_assembly_golden_test("nuget-cpm-directory-build-nearest-ancestor") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nuget-cpm-directory-build-nearest-ancestor: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_nuget_cpm_directory_build_imported_parent() {
        match run_assembly_golden_test("nuget-cpm-directory-build-imported-parent") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for nuget-cpm-directory-build-imported-parent: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_npm_workspace() {
        match run_assembly_golden_test("npm-workspace") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for npm-workspace: {}", e),
        }
    }

    #[test]
    fn test_assembly_npm_nested_packages() {
        match run_assembly_golden_test("npm-nested-packages") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for npm-nested-packages: {}", e),
        }
    }

    #[test]
    fn test_assembly_pnpm_workspace() {
        match run_assembly_golden_test("pnpm-workspace") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for pnpm-workspace: {}", e),
        }
    }

    #[test]
    fn test_assembly_pixi_basic() {
        match run_assembly_golden_test("pixi-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for pixi-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_alpine_file_refs() {
        match run_assembly_golden_test("alpine-file-refs") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for alpine-file-refs: {}", e),
        }
    }

    #[test]
    fn test_assembly_bazel_module_basic() {
        match run_assembly_golden_test("bazel-module-basic") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for bazel-module-basic: {}", e),
        }
    }

    #[test]
    fn test_assembly_bazel_build_chromium() {
        match run_assembly_golden_test("bazel-build-chromium") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for bazel-build-chromium: {}",
                e
            ),
        }
    }

    #[test]
    fn test_assembly_cargo_workspace() {
        match run_assembly_golden_test("cargo-workspace") {
            Ok(_) => (),
            Err(e) => panic!("Assembly golden test failed for cargo-workspace: {}", e),
        }
    }

    #[test]
    fn test_assembly_cargo_workspace_lowercase() {
        match run_assembly_golden_test("cargo-workspace-lowercase") {
            Ok(_) => (),
            Err(e) => panic!(
                "Assembly golden test failed for cargo-workspace-lowercase: {}",
                e
            ),
        }
    }

    #[test]
    fn test_uuid_normalization() {
        let input =
            r#"{"package_uid": "pkg:npm/test@1.0.0?uuid=12345678-1234-1234-1234-123456789abc"}"#;
        let expected =
            r#"{"package_uid": "pkg:npm/test@1.0.0?uuid=fixed-uid-done-for-testing-5642512d1758"}"#;
        let actual = normalize_uuids(input);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_uuid_normalization_multiple() {
        let input = r#"{"pkg1": "pkg:npm/a@1.0.0?uuid=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", "pkg2": "pkg:npm/b@2.0.0?uuid=bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"}"#;
        let expected = r#"{"pkg1": "pkg:npm/a@1.0.0?uuid=fixed-uid-done-for-testing-5642512d1758", "pkg2": "pkg:npm/b@2.0.0?uuid=fixed-uid-done-for-testing-5642512d1758"}"#;
        let actual = normalize_uuids(input);
        assert_eq!(actual, expected);
    }
}
