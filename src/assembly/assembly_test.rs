// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::assemble;
    use crate::models::{
        DatasourceId, Dependency, FileInfo, FileType, Package, PackageData, PackageType,
        Sha256Digest,
    };
    use serde_json::json;
    use std::collections::HashMap;

    fn create_test_file_info(
        path: &str,
        datasource_id: DatasourceId,
        purl: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        dependencies: Vec<Dependency>,
    ) -> FileInfo {
        let path_parts: Vec<&str> = path.split('/').collect();
        let file_name = path_parts.last().unwrap_or(&"");
        let extension = file_name.split('.').next_back().unwrap_or("");

        FileInfo {
            name: file_name.to_string(),
            base_name: file_name.to_string(),
            extension: extension.to_string(),
            path: path.to_string(),
            file_type: FileType::File,
            mime_type: Some("application/json".to_string()),
            file_type_label: None,
            size: 100,
            date: None,
            sha1: None,
            md5: None,
            sha256: None,
            programming_language: None,
            package_data: vec![PackageData {
                datasource_id: Some(datasource_id),
                purl: purl.map(|s| s.to_string()),
                name: name.map(|s| s.to_string()),
                version: version.map(|s| s.to_string()),
                dependencies,
                ..Default::default()
            }],
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
            scan_diagnostics: vec![],
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
            sha1_git: None,
            is_binary: None,
            is_text: None,
            is_archive: None,
            is_media: None,
            is_script: None,
            facets: vec![],
            tallies: None,
        }
    }

    #[test]
    fn test_windows_update_assembly_prefers_update_mum_as_primary_identity() {
        let mut hidden_pkg = create_test_file_info(
            "windows/package_41_for_kb5050109~31bf3856ad364e35~amd64~~14393.7692.1.1.mum",
            DatasourceId::MicrosoftUpdateManifestMum,
            None,
            Some("Package_41_for_KB5050109"),
            Some("14393.7692.1.1"),
            vec![],
        );
        hidden_pkg.package_data[0].package_type = Some(PackageType::WindowsUpdate);

        let mut update_root = create_test_file_info(
            "windows/update.mum",
            DatasourceId::MicrosoftUpdateManifestMum,
            None,
            Some("Package_for_KB5050109"),
            Some("14393.7692.1.1"),
            vec![],
        );
        update_root.package_data[0].package_type = Some(PackageType::WindowsUpdate);

        let mut files = vec![hidden_pkg, update_root];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1, "packages: {:#?}", result.packages);
        let package = &result.packages[0];
        assert_eq!(package.name.as_deref(), Some("Package_for_KB5050109"));
        assert_eq!(package.version.as_deref(), Some("14393.7692.1.1"));
        assert!(
            package
                .datafile_paths
                .iter()
                .any(|path| path == "windows/update.mum"),
            "datafile_paths: {:?}",
            package.datafile_paths
        );
    }

    fn create_test_dependency(
        purl: &str,
        extracted_requirement: Option<&str>,
        extra_data: Option<HashMap<String, serde_json::Value>>,
    ) -> Dependency {
        Dependency {
            purl: Some(purl.to_string()),
            extracted_requirement: extracted_requirement.map(str::to_string),
            scope: None,
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data,
        }
    }

    fn create_test_central_dependency(
        purl: &str,
        extracted_requirement: Option<&str>,
        extra_data: Option<HashMap<String, serde_json::Value>>,
    ) -> Dependency {
        let mut dependency = create_test_dependency(purl, extracted_requirement, extra_data);
        dependency.scope = Some("package_version".to_string());
        dependency
    }

    #[test]
    fn test_assemble_prunes_orphan_bazel_packages_after_npm_assignment() {
        let mut npm_file = create_test_file_info(
            "repo/package.json",
            DatasourceId::NpmPackageJson,
            Some("pkg:npm/demo@1.0.0"),
            Some("demo"),
            Some("1.0.0"),
            vec![],
        );
        npm_file.package_data[0].package_type = Some(PackageType::Npm);

        let mut bazel_file = create_test_file_info(
            "repo/closure/BUILD",
            DatasourceId::BazelBuild,
            Some("pkg:bazel/abstractspellchecker"),
            Some("abstractspellchecker"),
            None,
            vec![],
        );
        bazel_file.package_data[0].package_type = Some(PackageType::Bazel);

        let mut files = vec![npm_file, bazel_file];
        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.package_type, Some(PackageType::Npm));
        assert_eq!(package.name.as_deref(), Some("demo"));
        assert_eq!(files[0].for_packages, vec![package.package_uid.clone()]);
        assert_eq!(files[1].for_packages, vec![package.package_uid.clone()]);
    }

    #[test]
    fn test_assemble_keeps_bazel_package_when_it_owns_files() {
        let mut bazel_file = create_test_file_info(
            "repo/protobuf/BUILD",
            DatasourceId::BazelBuild,
            Some("pkg:bazel/parent_proto"),
            Some("parent_proto"),
            None,
            vec![],
        );
        bazel_file.package_data[0].package_type = Some(PackageType::Bazel);

        let mut files = vec![bazel_file];
        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.package_type, Some(PackageType::Bazel));
        assert_eq!(package.name.as_deref(), Some("parent_proto"));
        assert_eq!(files[0].for_packages, vec![package.package_uid.clone()]);
    }

    #[test]
    fn test_assemble_nuget_cpm_backfills_versionless_reference_from_nearest_ancestor() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_prefers_nearest_ancestor_props_file() {
        let mut root_props = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("12.0.1"),
                None,
            )],
        );
        root_props.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut nested_props = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        nested_props.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/app/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            root_props,
            nested_props,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_merges_imported_parent_props() {
        let mut parent_props = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        parent_props.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut child_props = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![],
        );
        child_props.package_data[0].extra_data = Some(HashMap::from([(
            "import_projects".to_string(),
            json!([
                "$([MSBuild]::GetPathOfFileAbove(Directory.Packages.props, $(MSBuildThisFileDirectory)..))"
            ]),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/app/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            parent_props,
            child_props,
        ];

        let result = assemble(&mut files);
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_prefers_child_update_over_imported_parent() {
        let mut parent_props = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("12.0.1"),
                None,
            )],
        );
        parent_props.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut child_props = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        child_props.package_data[0].extra_data = Some(HashMap::from([
            ("manage_package_versions_centrally".to_string(), json!(true)),
            (
                "import_projects".to_string(),
                json!(["repo/Directory.Packages.props"]),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/app/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            parent_props,
            child_props,
        ];

        let result = assemble(&mut files);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_ignores_non_directory_packages_imports() {
        let mut non_cpm_import = create_test_file_info(
            "repo/Directory.Build.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        non_cpm_import.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut child_props = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![],
        );
        child_props.package_data[0].extra_data = Some(HashMap::from([
            ("manage_package_versions_centrally".to_string(), json!(true)),
            (
                "import_projects".to_string(),
                json!(["../Directory.Build.props"]),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/app/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            non_cpm_import,
            child_props,
        ];

        let result = assemble(&mut files);
        assert!(result.dependencies[0].extracted_requirement.is_none());
    }

    #[test]
    fn test_assemble_nuget_cpm_resolves_property_backed_version_override() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut project_file = create_test_file_info(
            "repo/src/Contoso.Utility.csproj",
            DatasourceId::NugetCsproj,
            Some("pkg:nuget/Contoso.Utility@1.0.0"),
            Some("Contoso.Utility"),
            Some("1.0.0"),
            vec![create_test_dependency(
                "pkg:nuget/Newtonsoft.Json",
                None,
                Some(HashMap::from([
                    (
                        "version_override".to_string(),
                        json!("$(NewtonsoftJsonVersion)"),
                    ),
                    ("version_override_resolved".to_string(), json!("14.0.1")),
                ])),
            )],
        );
        project_file.package_data[0].extra_data = Some(HashMap::from([(
            "central_package_version_override_enabled".to_string(),
            json!(true),
        )]));

        let mut files = vec![project_file, props_file];
        let result = assemble(&mut files);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("14.0.1")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_uses_directory_build_props_for_central_versions() {
        let mut build_props = create_test_file_info(
            "repo/src/Directory.Build.props",
            DatasourceId::NugetDirectoryBuildProps,
            None,
            None,
            None,
            vec![],
        );
        build_props.package_data[0].extra_data = Some(HashMap::from([(
            "property_values".to_string(),
            json!({
                "ManageVersions": "true",
                "NewtonsoftJsonVersion": "13.0.3"
            }),
        )]));

        let mut props_file = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([
            (
                "property_values".to_string(),
                json!({
                    "ManagePackageVersionsCentrally": "$(ManageVersions)"
                }),
            ),
            (
                "package_versions".to_string(),
                json!([
                    {
                        "name": "Newtonsoft.Json",
                        "version": "$(NewtonsoftJsonVersion)",
                        "condition": null
                    }
                ]),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/app/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            build_props,
            props_file,
        ];

        let result = assemble(&mut files);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_uses_directory_build_props_for_version_override() {
        let mut build_props = create_test_file_info(
            "repo/src/Directory.Build.props",
            DatasourceId::NugetDirectoryBuildProps,
            None,
            None,
            None,
            vec![],
        );
        build_props.package_data[0].extra_data = Some(HashMap::from([(
            "property_values".to_string(),
            json!({
                "CentralOverridesEnabled": "true",
                "NewtonsoftJsonVersion": "14.0.1"
            }),
        )]));

        let mut props_file = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut project_file = create_test_file_info(
            "repo/src/app/Contoso.Utility.csproj",
            DatasourceId::NugetCsproj,
            Some("pkg:nuget/Contoso.Utility@1.0.0"),
            Some("Contoso.Utility"),
            Some("1.0.0"),
            vec![create_test_dependency(
                "pkg:nuget/Newtonsoft.Json",
                None,
                Some(HashMap::from([(
                    "version_override".to_string(),
                    json!("$(NewtonsoftJsonVersion)"),
                )])),
            )],
        );
        project_file.package_data[0].extra_data = Some(HashMap::from([(
            "central_package_version_override_enabled_raw".to_string(),
            json!("$(CentralOverridesEnabled)"),
        )]));

        let mut files = vec![project_file, build_props, props_file];
        let result = assemble(&mut files);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("14.0.1")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_ignores_conditioned_directory_build_props_data() {
        let mut build_props = create_test_file_info(
            "repo/src/Directory.Build.props",
            DatasourceId::NugetDirectoryBuildProps,
            None,
            None,
            None,
            vec![],
        );
        build_props.package_data[0].extra_data = Some(HashMap::from([
            (
                "property_values".to_string(),
                json!({
                    "ManageVersions": "true"
                }),
            ),
            ("import_projects".to_string(), json!([])),
        ]));

        let mut props_file = create_test_file_info(
            "repo/src/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([
            (
                "property_values".to_string(),
                json!({
                    "ManagePackageVersionsCentrally": "$(ManageVersions)"
                }),
            ),
            (
                "package_versions".to_string(),
                json!([
                    {
                        "name": "Newtonsoft.Json",
                        "version": "$(NewtonsoftJsonVersion)",
                        "condition": null
                    }
                ]),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/app/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            build_props,
            props_file,
        ];

        let result = assemble(&mut files);
        assert!(result.dependencies[0].extracted_requirement.is_none());
    }

    #[test]
    fn test_assemble_nuget_cpm_does_not_override_explicit_project_version() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    Some("12.0.1"),
                    None,
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("12.0.1")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_requires_matching_condition() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                Some(HashMap::from([(
                    "condition".to_string(),
                    json!("'$(TargetFramework)' == 'net472'"),
                )])),
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    Some(HashMap::from([(
                        "condition".to_string(),
                        json!("'$(TargetFramework)' == 'net8.0'"),
                    )])),
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert!(result.dependencies[0].extracted_requirement.is_none());
    }

    #[test]
    fn test_assemble_nuget_cpm_applies_exact_matching_condition() {
        let condition = "'$(TargetFramework)' == 'net8.0'";
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                Some(HashMap::from([("condition".to_string(), json!(condition))])),
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    Some(HashMap::from([("condition".to_string(), json!(condition))])),
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_requires_manage_package_versions_centrally_true() {
        let props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert!(result.dependencies[0].extracted_requirement.is_none());
    }

    #[test]
    fn test_assemble_hoists_unowned_standalone_pip_requirements_dependencies() {
        let mut files = vec![create_test_file_info(
            "docs/min_requirements.txt",
            DatasourceId::PipRequirements,
            None,
            None,
            None,
            vec![create_test_dependency(
                "pkg:pypi/sphinx@3.4.3",
                Some("==3.4.3"),
                None,
            )],
        )];

        let result = assemble(&mut files);

        assert!(result.packages.is_empty());
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl.as_deref(),
            Some("pkg:pypi/sphinx@3.4.3")
        );
        assert_eq!(result.dependencies[0].for_package_uid, None);
        assert_eq!(
            result.dependencies[0].datafile_path,
            "docs/min_requirements.txt"
        );
        assert!(files[0].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_does_not_hoist_unowned_nuget_cpm_metadata_dependencies() {
        let mut files = vec![create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        )];

        let result = assemble(&mut files);

        assert!(result.packages.is_empty());
        assert!(result.dependencies.is_empty());
        assert!(files[0].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_creates_package_for_buck_metadata_without_package_type() {
        let mut files = vec![create_test_file_info(
            "repo/METADATA.bzl",
            DatasourceId::BuckMetadata,
            None,
            Some("example"),
            Some("0.0.1"),
            vec![],
        )];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.package_type, None);
        assert_eq!(package.name.as_deref(), Some("example"));
        assert_eq!(package.version.as_deref(), Some("0.0.1"));
        assert!(!package.package_uid.is_empty());
        assert!(
            package
                .package_uid
                .starts_with("generated-package:buck_metadata/example@0.0.1?uuid=")
        );
        assert!(package.datasource_ids.contains(&DatasourceId::BuckMetadata));
        assert_eq!(files[0].for_packages, vec![package.package_uid.clone()]);
    }

    #[test]
    fn test_assemble_nuget_cpm_prefers_version_override_when_enabled() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([
            ("manage_package_versions_centrally".to_string(), json!(true)),
            (
                "central_package_version_override_enabled".to_string(),
                json!(true),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    Some(HashMap::from([(
                        "version_override".to_string(),
                        json!("14.0.1"),
                    )])),
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("14.0.1")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_ignores_version_override_when_not_enabled() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    Some(HashMap::from([(
                        "version_override".to_string(),
                        json!("14.0.1"),
                    )])),
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_ignores_version_override_without_matching_central_entry() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Serilog",
                Some("3.1.1"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([
            ("manage_package_versions_centrally".to_string(), json!(true)),
            (
                "central_package_version_override_enabled".to_string(),
                json!(true),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    Some(HashMap::from([(
                        "version_override".to_string(),
                        json!("14.0.1"),
                    )])),
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert!(result.dependencies[0].extracted_requirement.is_none());
    }

    #[test]
    fn test_assemble_nuget_cpm_ignores_non_literal_version_override() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![create_test_central_dependency(
                "pkg:nuget/Newtonsoft.Json",
                Some("13.0.3"),
                None,
            )],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([
            ("manage_package_versions_centrally".to_string(), json!(true)),
            (
                "central_package_version_override_enabled".to_string(),
                json!(true),
            ),
        ]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    Some(HashMap::from([(
                        "version_override".to_string(),
                        json!("$(NewtonsoftJsonVersion)"),
                    )])),
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].extracted_requirement.as_deref(),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_assemble_nuget_cpm_leaves_dependency_unresolved_when_matches_are_ambiguous() {
        let mut props_file = create_test_file_info(
            "repo/Directory.Packages.props",
            DatasourceId::NugetDirectoryPackagesProps,
            None,
            None,
            None,
            vec![
                create_test_central_dependency("pkg:nuget/Newtonsoft.Json", Some("13.0.3"), None),
                create_test_central_dependency("pkg:nuget/Newtonsoft.Json", Some("13.0.4"), None),
            ],
        );
        props_file.package_data[0].extra_data = Some(HashMap::from([(
            "manage_package_versions_centrally".to_string(),
            json!(true),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/src/Contoso.Utility.csproj",
                DatasourceId::NugetCsproj,
                Some("pkg:nuget/Contoso.Utility@1.0.0"),
                Some("Contoso.Utility"),
                Some("1.0.0"),
                vec![create_test_dependency(
                    "pkg:nuget/Newtonsoft.Json",
                    None,
                    None,
                )],
            ),
            props_file,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.dependencies.len(), 1);
        assert!(result.dependencies[0].extracted_requirement.is_none());
    }

    #[test]
    fn test_assemble_npm_package_json_with_lockfile() {
        let dep = Dependency {
            purl: Some("pkg:npm/express@4.18.0".to_string()),
            extracted_requirement: Some("^4.18.0".to_string()),
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        };

        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![dep],
            ),
            create_test_file_info(
                "project/package-lock.json",
                DatasourceId::NpmPackageLockJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1, "Expected exactly one package");
        let package = &result.packages[0];
        assert_eq!(package.name, Some("my-app".to_string()));
        assert!(
            package.package_uid.contains("uuid="),
            "Expected package_uid to contain uuid qualifier"
        );
        assert_eq!(
            package.datafile_paths.len(),
            2,
            "Expected both files in datafile_paths"
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/package.json".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/package-lock.json".to_string())
        );
        assert_eq!(
            package.datasource_ids.len(),
            2,
            "Expected both datasource IDs"
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::NpmPackageJson)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::NpmPackageLockJson)
        );

        assert_eq!(result.dependencies.len(), 1, "Expected one dependency");
        let dep = &result.dependencies[0];
        assert_eq!(dep.purl, Some("pkg:npm/express@4.18.0".to_string()));
        assert_eq!(dep.datafile_path, "project/package.json");
        assert_eq!(dep.datasource_id, DatasourceId::NpmPackageJson);
        assert!(
            dep.for_package_uid.is_some(),
            "Expected for_package_uid to be set"
        );
        assert!(
            dep.for_package_uid
                .as_ref()
                .expect("for_package_uid should be Some")
                .contains("uuid="),
            "Expected for_package_uid to contain uuid"
        );

        assert_eq!(
            files[0].for_packages.len(),
            1,
            "Expected package.json to have for_packages populated"
        );
        assert_eq!(
            files[1].for_packages.len(),
            1,
            "Expected package-lock.json to have for_packages populated"
        );
    }

    #[test]
    fn test_assemble_npm_package_json_with_yarn_pnp() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/root-app@1.0.0"),
                Some("root-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/.pnp.cjs",
                DatasourceId::YarnPnpCjs,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("npm:1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl.as_deref(),
            Some("pkg:npm/left-pad@1.3.0")
        );
        assert_eq!(
            result.dependencies[0].datasource_id,
            DatasourceId::YarnPnpCjs
        );
        assert_eq!(result.dependencies[0].datafile_path, "project/.pnp.cjs");
        assert!(
            files[1]
                .for_packages
                .iter()
                .any(|uid| uid == &result.packages[0].package_uid)
        );
        assert!(
            result.packages[0]
                .datasource_ids
                .contains(&DatasourceId::YarnPnpCjs)
        );
    }

    #[test]
    fn test_assemble_npm_package_json_skips_mismatched_lockfile() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/package-lock.json",
                DatasourceId::NpmPackageLockJson,
                Some("pkg:npm/other-app@2.0.0"),
                Some("other-app"),
                Some("2.0.0"),
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(false),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected only the manifest package"
        );
        let package = &result.packages[0];
        assert_eq!(package.name, Some("my-app".to_string()));
        assert_eq!(
            package.datafile_paths,
            vec!["project/package.json".to_string()]
        );
        assert!(
            result.dependencies.is_empty(),
            "Mismatched lockfile deps should not merge"
        );
        assert_eq!(files[0].for_packages.len(), 1);
        assert!(
            files[1].for_packages.is_empty(),
            "Mismatched lockfile should remain unassigned"
        );
    }

    #[test]
    fn test_assemble_nix_flake_merges_lockfile_while_default_nix_stays_standalone() {
        let mut files = vec![
            create_test_file_info(
                "repo/flake.nix",
                DatasourceId::NixFlakeNix,
                Some("pkg:nix/demo-flake"),
                Some("demo-flake"),
                None,
                vec![],
            ),
            create_test_file_info(
                "repo/flake.lock",
                DatasourceId::NixFlakeLock,
                Some("pkg:nix/demo-flake"),
                Some("demo-flake"),
                None,
                vec![create_test_dependency(
                    "pkg:nix/nixpkgs@abc123",
                    Some("github:NixOS/nixpkgs"),
                    None,
                )],
            ),
            create_test_file_info(
                "repo/default.nix",
                DatasourceId::NixDefaultNix,
                Some("pkg:nix/demo-derivation@1.0.0"),
                Some("demo-derivation"),
                Some("1.0.0"),
                vec![],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 2);

        let flake_package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("demo-flake"))
            .expect("missing flake package");
        assert_eq!(flake_package.datafile_paths.len(), 2);
        assert!(
            flake_package
                .datafile_paths
                .contains(&"repo/flake.nix".to_string())
        );
        assert!(
            flake_package
                .datafile_paths
                .contains(&"repo/flake.lock".to_string())
        );
        assert!(
            flake_package
                .datasource_ids
                .contains(&DatasourceId::NixFlakeNix)
        );
        assert!(
            flake_package
                .datasource_ids
                .contains(&DatasourceId::NixFlakeLock)
        );

        let default_package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("demo-derivation"))
            .expect("missing default.nix package");
        assert_eq!(default_package.datafile_paths, vec!["repo/default.nix"]);
        assert_eq!(
            default_package.datasource_ids,
            vec![DatasourceId::NixDefaultNix]
        );

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].datafile_path, "repo/flake.lock");
        assert_eq!(
            result.dependencies[0].for_package_uid.as_deref(),
            Some(flake_package.package_uid.as_str())
        );

        assert_eq!(files[0].for_packages.len(), 1);
        assert_eq!(files[1].for_packages.len(), 1);
        assert_eq!(files[2].for_packages.len(), 1);
        assert_eq!(files[0].for_packages[0], flake_package.package_uid);
        assert_eq!(files[1].for_packages[0], flake_package.package_uid);
        assert_eq!(files[2].for_packages[0], default_package.package_uid);
    }

    #[test]
    fn test_assemble_nix_flake_attaches_anonymous_flake_compat_default_nix() {
        let mut default_info = create_test_file_info(
            "repo/default.nix",
            DatasourceId::NixDefaultNix,
            None,
            None,
            None,
            vec![],
        );
        default_info.package_data[0].extra_data = Some(std::collections::HashMap::from([(
            "nix_wrapper_kind".to_string(),
            serde_json::Value::String("flake_compat".to_string()),
        )]));

        let mut files = vec![
            create_test_file_info(
                "repo/flake.nix",
                DatasourceId::NixFlakeNix,
                Some("pkg:nix/demo-flake"),
                Some("demo-flake"),
                None,
                vec![],
            ),
            create_test_file_info(
                "repo/flake.lock",
                DatasourceId::NixFlakeLock,
                Some("pkg:nix/demo-flake"),
                Some("demo-flake"),
                None,
                vec![],
            ),
            default_info,
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let flake_package = &result.packages[0];
        assert!(
            flake_package
                .datafile_paths
                .contains(&"repo/default.nix".to_string())
        );
        assert!(
            flake_package
                .datasource_ids
                .contains(&DatasourceId::NixDefaultNix)
        );
        assert_eq!(
            files[2].for_packages,
            vec![flake_package.package_uid.clone()]
        );
    }

    #[test]
    fn test_assemble_npm_package_json_skips_lockfile_with_same_name_different_version() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/package-lock.json",
                DatasourceId::NpmPackageLockJson,
                Some("pkg:npm/my-app@2.0.0"),
                Some("my-app"),
                Some("2.0.0"),
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(false),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, Some("my-app".to_string()));
        assert_eq!(result.packages[0].version, Some("1.0.0".to_string()));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/package.json".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_npm_package_json_skips_lockfile_with_same_version_different_name() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/package-lock.json",
                DatasourceId::NpmPackageLockJson,
                Some("pkg:npm/other-app@1.0.0"),
                Some("other-app"),
                Some("1.0.0"),
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(false),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, Some("my-app".to_string()));
        assert_eq!(result.packages[0].version, Some("1.0.0".to_string()));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/package.json".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_npm_package_json_skips_lockfile_with_missing_identity() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/package-lock.json",
                DatasourceId::NpmPackageLockJson,
                None,
                Some("my-app"),
                None,
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(false),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, Some("my-app".to_string()));
        assert_eq!(result.packages[0].version, Some("1.0.0".to_string()));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/package.json".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_npm_package_json_skips_mismatched_bun_lock() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/bun.lock",
                DatasourceId::BunLock,
                Some("pkg:npm/other-app@2.0.0"),
                Some("other-app"),
                Some("2.0.0"),
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name.as_deref(), Some("my-app"));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/package.json".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_npm_package_json_skips_mismatched_bun_lockb() {
        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/my-app@1.0.0"),
                Some("my-app"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/bun.lockb",
                DatasourceId::BunLockb,
                Some("pkg:npm/other-app"),
                Some("other-app"),
                None,
                vec![Dependency {
                    purl: Some("pkg:npm/left-pad@1.3.0".to_string()),
                    extracted_requirement: Some("1.3.0".to_string()),
                    scope: Some("dependencies".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name.as_deref(), Some("my-app"));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/package.json".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_cargo_toml_with_lock() {
        let mut files = vec![
            create_test_file_info(
                "project/Cargo.toml",
                DatasourceId::CargoToml,
                Some("pkg:cargo/my-crate@0.1.0"),
                Some("my-crate"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/Cargo.lock",
                DatasourceId::CargoLock,
                Some("pkg:cargo/my-crate@0.1.0"),
                Some("my-crate"),
                Some("0.1.0"),
                vec![],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1, "Expected exactly one package");
        let package = &result.packages[0];
        assert_eq!(package.name, Some("my-crate".to_string()));
        assert_eq!(package.version, Some("0.1.0".to_string()));
        assert!(
            package.package_uid.contains("uuid="),
            "Expected package_uid to contain uuid qualifier"
        );
        assert_eq!(
            package.datafile_paths.len(),
            2,
            "Expected both files in datafile_paths"
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/Cargo.toml".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/Cargo.lock".to_string())
        );
        assert_eq!(
            package.datasource_ids.len(),
            2,
            "Expected both datasource IDs"
        );
        assert!(package.datasource_ids.contains(&DatasourceId::CargoToml));
        assert!(package.datasource_ids.contains(&DatasourceId::CargoLock));
    }

    #[test]
    fn test_assemble_cargo_workspace_preserves_member_lock_dependencies() {
        let mut root = create_test_file_info(
            "workspace/Cargo.toml",
            DatasourceId::CargoToml,
            None,
            None,
            None,
            vec![],
        );
        root.package_data[0].package_type = Some(PackageType::Cargo);
        root.package_data[0].extra_data = Some(HashMap::from([(
            "workspace".to_string(),
            json!({ "members": ["crates/app"] }),
        )]));

        let mut member_manifest = create_test_file_info(
            "workspace/crates/app/Cargo.toml",
            DatasourceId::CargoToml,
            Some("pkg:cargo/app@0.1.0"),
            Some("app"),
            Some("0.1.0"),
            vec![create_test_dependency("pkg:cargo/log", Some("0.4"), None)],
        );
        member_manifest.package_data[0].package_type = Some(PackageType::Cargo);

        let mut member_lock = create_test_file_info(
            "workspace/crates/app/Cargo.lock",
            DatasourceId::CargoLock,
            Some("pkg:cargo/app@0.1.0"),
            Some("app"),
            Some("0.1.0"),
            vec![create_test_dependency(
                "pkg:cargo/log@0.4.22",
                Some("0.4.22"),
                None,
            )],
        );
        member_lock.package_data[0].package_type = Some(PackageType::Cargo);

        let mut files = vec![root, member_manifest, member_lock];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.purl.as_deref(), Some("pkg:cargo/app@0.1.0"));
        assert!(
            package
                .datafile_paths
                .contains(&"workspace/crates/app/Cargo.toml".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"workspace/crates/app/Cargo.lock".to_string())
        );

        assert_eq!(result.dependencies.len(), 2);
        assert!(result.dependencies.iter().any(|dep| {
            dep.datafile_path == "workspace/crates/app/Cargo.toml"
                && dep.purl.as_deref() == Some("pkg:cargo/log")
                && dep.for_package_uid.as_ref() == Some(&package.package_uid)
        }));
        assert!(result.dependencies.iter().any(|dep| {
            dep.datafile_path == "workspace/crates/app/Cargo.lock"
                && dep.purl.as_deref() == Some("pkg:cargo/log@0.4.22")
                && dep.for_package_uid.as_ref() == Some(&package.package_uid)
        }));
    }

    #[test]
    fn test_assemble_cargo_workspace_preserves_member_lock_dependencies_when_lock_root_differs() {
        let mut root = create_test_file_info(
            "workspace/Cargo.toml",
            DatasourceId::CargoToml,
            None,
            None,
            None,
            vec![],
        );
        root.package_data[0].package_type = Some(PackageType::Cargo);
        root.package_data[0].extra_data = Some(HashMap::from([(
            "workspace".to_string(),
            json!({ "members": ["crates/app"] }),
        )]));

        let mut member_manifest = create_test_file_info(
            "workspace/crates/app/Cargo.toml",
            DatasourceId::CargoToml,
            Some("pkg:cargo/app@0.1.0"),
            Some("app"),
            Some("0.1.0"),
            vec![create_test_dependency("pkg:cargo/log", Some("0.4"), None)],
        );
        member_manifest.package_data[0].package_type = Some(PackageType::Cargo);

        let mut member_lock = create_test_file_info(
            "workspace/crates/app/Cargo.lock",
            DatasourceId::CargoLock,
            Some("pkg:cargo/workspace-helper@0.1.0"),
            Some("workspace-helper"),
            Some("0.1.0"),
            vec![create_test_dependency(
                "pkg:cargo/log@0.4.22",
                Some("0.4.22"),
                None,
            )],
        );
        member_lock.package_data[0].package_type = Some(PackageType::Cargo);

        let mut files = vec![root, member_manifest, member_lock];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.purl.as_deref(), Some("pkg:cargo/app@0.1.0"));

        assert_eq!(result.dependencies.len(), 2);
        assert!(result.dependencies.iter().any(|dep| {
            dep.datafile_path == "workspace/crates/app/Cargo.toml"
                && dep.purl.as_deref() == Some("pkg:cargo/log")
                && dep.for_package_uid.as_ref() == Some(&package.package_uid)
        }));
        assert!(result.dependencies.iter().any(|dep| {
            dep.datafile_path == "workspace/crates/app/Cargo.lock"
                && dep.purl.as_deref() == Some("pkg:cargo/log@0.4.22")
                && dep.for_package_uid.as_ref() == Some(&package.package_uid)
        }));
    }

    #[test]
    fn test_assemble_cargo_workspace_hoists_root_lock_dependencies() {
        let mut root = create_test_file_info(
            "workspace/Cargo.toml",
            DatasourceId::CargoToml,
            None,
            None,
            None,
            vec![],
        );
        root.package_data[0].package_type = Some(PackageType::Cargo);
        root.package_data[0].extra_data = Some(HashMap::from([(
            "workspace".to_string(),
            json!({ "members": ["crates/app"] }),
        )]));

        let mut root_lock = create_test_file_info(
            "workspace/Cargo.lock",
            DatasourceId::CargoLock,
            Some("pkg:cargo/workspace-root@0.0.0"),
            Some("workspace-root"),
            Some("0.0.0"),
            vec![create_test_dependency(
                "pkg:cargo/serde@1.0.215",
                Some("1.0.215"),
                None,
            )],
        );
        root_lock.package_data[0].package_type = Some(PackageType::Cargo);

        let mut member_manifest = create_test_file_info(
            "workspace/crates/app/Cargo.toml",
            DatasourceId::CargoToml,
            Some("pkg:cargo/app@0.1.0"),
            Some("app"),
            Some("0.1.0"),
            vec![],
        );
        member_manifest.package_data[0].package_type = Some(PackageType::Cargo);

        let mut files = vec![root, root_lock, member_manifest];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.dependencies.len(), 1);

        let dep = &result.dependencies[0];
        assert_eq!(dep.datafile_path, "workspace/Cargo.lock");
        assert_eq!(dep.purl.as_deref(), Some("pkg:cargo/serde@1.0.215"));
        assert!(dep.for_package_uid.is_none());
    }

    #[test]
    fn test_assemble_cargo_workspace_keeps_root_package_when_root_is_real_member() {
        let mut root = create_test_file_info(
            "workspace/Cargo.toml",
            DatasourceId::CargoToml,
            Some("pkg:cargo/workspace-root@1.0.0"),
            Some("workspace-root"),
            Some("1.0.0"),
            vec![create_test_dependency("pkg:cargo/serde", Some("1.0"), None)],
        );
        root.package_data[0].package_type = Some(PackageType::Cargo);
        root.package_data[0].extra_data = Some(HashMap::from([(
            "workspace".to_string(),
            json!({ "members": ["crates/app"] }),
        )]));

        let mut member_manifest = create_test_file_info(
            "workspace/crates/app/Cargo.toml",
            DatasourceId::CargoToml,
            Some("pkg:cargo/app@0.1.0"),
            Some("app"),
            Some("0.1.0"),
            vec![],
        );
        member_manifest.package_data[0].package_type = Some(PackageType::Cargo);

        let mut files = vec![root, member_manifest];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 2);
        assert!(
            result
                .packages
                .iter()
                .any(|package| package.purl.as_deref() == Some("pkg:cargo/workspace-root@1.0.0"))
        );
        assert!(
            result
                .packages
                .iter()
                .any(|package| package.purl.as_deref() == Some("pkg:cargo/app@0.1.0"))
        );
    }

    #[test]
    fn test_assemble_python_pyproject_with_uv_lock() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPyprojectToml,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/uv.lock",
                DatasourceId::PypiUvLock,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![Dependency {
                    purl: Some("pkg:pypi/requests@2.32.5".to_string()),
                    extracted_requirement: Some(">=2.32.5".to_string()),
                    scope: None,
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        files[0].package_data[0].description = Some("Demo package".to_string());
        files[0].package_data[0].keywords = vec!["workflow".to_string(), "dag".to_string()];
        files[0].package_data[0].homepage_url = Some("https://example.com/home".to_string());
        files[0].package_data[0].bug_tracking_url = Some("https://example.com/issues".to_string());
        files[0].package_data[0].code_view_url = Some("https://example.com/source".to_string());
        files[0].package_data[0].parties = vec![crate::models::Party {
            r#type: None,
            role: Some("author".to_string()),
            name: Some("Example Author".to_string()),
            email: Some("author@example.com".to_string()),
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        }];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected exactly one merged Python package"
        );
        let package = &result.packages[0];
        assert_eq!(package.name, Some("uv-demo".to_string()));
        assert_eq!(package.description.as_deref(), Some("Demo package"));
        assert_eq!(
            package.keywords,
            vec!["workflow".to_string(), "dag".to_string()]
        );
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://example.com/home")
        );
        assert_eq!(
            package.bug_tracking_url.as_deref(),
            Some("https://example.com/issues")
        );
        assert_eq!(
            package.code_view_url.as_deref(),
            Some("https://example.com/source")
        );
        assert_eq!(package.parties.len(), 1);
        assert_eq!(package.parties[0].name.as_deref(), Some("Example Author"));
        assert!(
            package
                .datafile_paths
                .contains(&"project/pyproject.toml".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/uv.lock".to_string())
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPyprojectToml)
        );
        assert!(package.datasource_ids.contains(&DatasourceId::PypiUvLock));

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl.as_deref(),
            Some("pkg:pypi/requests@2.32.5")
        );
        assert_eq!(files[0].for_packages.len(), 1);
        assert_eq!(files[1].for_packages.len(), 1);
    }

    #[test]
    fn test_assemble_poetry_pyproject_with_uv_lock() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPoetryPyprojectToml,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/uv.lock",
                DatasourceId::PypiUvLock,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![Dependency {
                    purl: Some("pkg:pypi/requests@2.32.5".to_string()),
                    extracted_requirement: Some(">=2.32.5".to_string()),
                    scope: None,
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.name.as_deref(), Some("uv-demo"));
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPoetryPyprojectToml)
        );
        assert!(package.datasource_ids.contains(&DatasourceId::PypiUvLock));
        assert_eq!(result.dependencies.len(), 1);
    }

    #[test]
    fn test_assemble_poetry_pyproject_with_poetry_lock() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPoetryPyprojectToml,
                Some("pkg:pypi/poetry-demo@0.1.0"),
                Some("poetry-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/poetry.lock",
                DatasourceId::PypiPoetryLock,
                Some("pkg:pypi/poetry-demo@0.1.0"),
                Some("poetry-demo"),
                Some("0.1.0"),
                vec![Dependency {
                    purl: Some("pkg:pypi/requests@2.32.5".to_string()),
                    extracted_requirement: Some(">=2.32.5".to_string()),
                    scope: Some("main".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.name.as_deref(), Some("poetry-demo"));
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPoetryPyprojectToml)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPoetryLock)
        );
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl.as_deref(),
            Some("pkg:pypi/requests@2.32.5")
        );
    }

    #[test]
    fn test_assemble_python_pyproject_with_uv_lock_backfills_version_and_refreshes_uids() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPyprojectToml,
                Some("pkg:pypi/uv-demo"),
                Some("uv-demo"),
                None,
                vec![Dependency {
                    purl: Some("pkg:pypi/httpx@0.27.0".to_string()),
                    extracted_requirement: Some(">=0.27.0".to_string()),
                    scope: None,
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
            create_test_file_info(
                "project/uv.lock",
                DatasourceId::PypiUvLock,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![Dependency {
                    purl: Some("pkg:pypi/anyio@4.4.0".to_string()),
                    extracted_requirement: Some("==4.4.0".to_string()),
                    scope: Some("dev".to_string()),
                    is_runtime: Some(false),
                    is_optional: Some(true),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.version.as_deref(), Some("0.1.0"));
        assert_eq!(package.purl.as_deref(), Some("pkg:pypi/uv-demo@0.1.0"));
        assert!(
            package
                .package_uid
                .starts_with("pkg:pypi/uv-demo@0.1.0?uuid=")
        );
        assert_eq!(result.dependencies.len(), 2);
        assert!(
            result.dependencies.iter().all(|dep| {
                dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
        );
    }

    #[test]
    fn test_assemble_python_pyproject_skips_uv_lock_with_same_name_different_version() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPyprojectToml,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/uv.lock",
                DatasourceId::PypiUvLock,
                Some("pkg:pypi/uv-demo@0.2.0"),
                Some("uv-demo"),
                Some("0.2.0"),
                vec![Dependency {
                    purl: Some("pkg:pypi/requests@2.32.5".to_string()),
                    extracted_requirement: Some("==2.32.5".to_string()),
                    scope: None,
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].version.as_deref(), Some("0.1.0"));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/pyproject.toml".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_python_pyproject_skips_uv_lock_with_same_version_different_name() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPyprojectToml,
                Some("pkg:pypi/uv-demo@0.1.0"),
                Some("uv-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/uv.lock",
                DatasourceId::PypiUvLock,
                Some("pkg:pypi/other-demo@0.1.0"),
                Some("other-demo"),
                Some("0.1.0"),
                vec![Dependency {
                    purl: Some("pkg:pypi/requests@2.32.5".to_string()),
                    extracted_requirement: Some("==2.32.5".to_string()),
                    scope: None,
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name.as_deref(), Some("uv-demo"));
        assert_eq!(
            result.packages[0].datafile_paths,
            vec!["project/pyproject.toml".to_string()]
        );
        assert!(result.dependencies.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_python_pyproject_with_pylock_toml() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPyprojectToml,
                Some("pkg:pypi/pylock-demo@0.1.0"),
                Some("pylock-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/pylock.toml",
                DatasourceId::PypiPylockToml,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:pypi/requests@2.32.3".to_string()),
                    extracted_requirement: None,
                    scope: None,
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected exactly one merged Python package"
        );
        let package = &result.packages[0];
        assert_eq!(package.name, Some("pylock-demo".to_string()));
        assert!(
            package
                .datafile_paths
                .contains(&"project/pyproject.toml".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/pylock.toml".to_string())
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPyprojectToml)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPylockToml)
        );

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl.as_deref(),
            Some("pkg:pypi/requests@2.32.3")
        );
        assert_eq!(files[0].for_packages.len(), 1);
        assert_eq!(files[1].for_packages.len(), 1);
    }

    #[test]
    fn test_assemble_hackage_multiple_cabal_files_do_not_collapse_into_one_package() {
        let mut files = vec![
            create_test_file_info(
                "project/alpha.cabal",
                DatasourceId::HackageCabal,
                Some("pkg:hackage/alpha@1.0.0"),
                Some("alpha"),
                Some("1.0.0"),
                vec![Dependency {
                    purl: Some("pkg:hackage/base".to_string()),
                    extracted_requirement: Some(">=4.14 && <5".to_string()),
                    scope: Some("build-depends".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
            create_test_file_info(
                "project/beta.cabal",
                DatasourceId::HackageCabal,
                Some("pkg:hackage/beta@2.0.0"),
                Some("beta"),
                Some("2.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/cabal.project",
                DatasourceId::HackageCabalProject,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:hackage/lens@5.2.1".to_string()),
                    extracted_requirement: Some("5.2.1".to_string()),
                    scope: Some("extra-packages".to_string()),
                    is_runtime: None,
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 2);
        assert!(
            result
                .packages
                .iter()
                .any(|package| package.name.as_deref() == Some("alpha"))
        );
        assert!(
            result
                .packages
                .iter()
                .any(|package| package.name.as_deref() == Some("beta"))
        );
        assert!(
            result
                .packages
                .iter()
                .all(|package| package.datafile_paths.len() == 1)
        );
        assert!(
            result.dependencies.iter().any(|dependency| {
                dependency.purl.as_deref() == Some("pkg:hackage/lens@5.2.1")
                    && dependency.for_package_uid.is_none()
            }),
            "project-level Hackage dependency should stay unowned when multiple sibling manifests exist"
        );
        assert!(files[0].for_packages.len() == 1);
        assert!(files[1].for_packages.len() == 1);
        assert!(files[2].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_bun_workspace_hoists_root_lockfile_dependencies() {
        let mut root_file = create_test_file_info(
            "project/package.json",
            DatasourceId::NpmPackageJson,
            None,
            None,
            None,
            vec![Dependency {
                purl: Some("pkg:npm/typescript".to_string()),
                extracted_requirement: Some("^5.0.0".to_string()),
                scope: Some("devDependencies".to_string()),
                is_runtime: Some(false),
                is_optional: Some(true),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            }],
        );
        root_file.package_data[0].extra_data = Some(HashMap::from([(
            "workspaces".to_string(),
            serde_json::json!(["packages/*"]),
        )]));

        let mut files = vec![
            root_file,
            create_test_file_info(
                "project/bun.lock",
                DatasourceId::BunLock,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:npm/typescript@5.8.3".to_string()),
                    extracted_requirement: Some("5.8.3".to_string()),
                    scope: Some("devDependencies".to_string()),
                    is_runtime: Some(false),
                    is_optional: Some(true),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
            create_test_file_info(
                "project/packages/core/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/%40myorg/core@1.0.0"),
                Some("@myorg/core"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/packages/utils/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/%40myorg/utils@2.0.0"),
                Some("@myorg/utils"),
                Some("2.0.0"),
                vec![],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 2);
        assert!(
            result
                .packages
                .iter()
                .any(|pkg| pkg.name.as_deref() == Some("@myorg/core"))
        );
        assert!(
            result
                .packages
                .iter()
                .any(|pkg| pkg.name.as_deref() == Some("@myorg/utils"))
        );

        let bun_dep = result
            .dependencies
            .iter()
            .find(|dep| dep.datasource_id == DatasourceId::BunLock)
            .expect("expected bun.lock hoisted dependency");
        assert_eq!(bun_dep.purl.as_deref(), Some("pkg:npm/typescript@5.8.3"));
        assert_eq!(bun_dep.datafile_path, "project/bun.lock");
        assert!(bun_dep.for_package_uid.is_none());

        let bun_file = files
            .iter()
            .find(|file| file.path == "project/bun.lock")
            .expect("expected bun.lock file");
        assert_eq!(bun_file.for_packages.len(), 2);
    }

    #[test]
    fn test_assemble_bun_lockb_workspace_hoists_root_lockfile_dependencies() {
        let mut root_file = create_test_file_info(
            "project/package.json",
            DatasourceId::NpmPackageJson,
            None,
            None,
            None,
            vec![Dependency {
                purl: Some("pkg:npm/typescript".to_string()),
                extracted_requirement: Some("^5.0.0".to_string()),
                scope: Some("devDependencies".to_string()),
                is_runtime: Some(false),
                is_optional: Some(true),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            }],
        );
        root_file.package_data[0].extra_data = Some(HashMap::from([(
            "workspaces".to_string(),
            serde_json::json!(["packages/*"]),
        )]));

        let mut files = vec![
            root_file,
            create_test_file_info(
                "project/bun.lockb",
                DatasourceId::BunLockb,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:npm/typescript@5.8.3".to_string()),
                    extracted_requirement: Some("5.8.3".to_string()),
                    scope: Some("devDependencies".to_string()),
                    is_runtime: Some(false),
                    is_optional: Some(true),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
            create_test_file_info(
                "project/packages/core/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/%40myorg/core@1.0.0"),
                Some("@myorg/core"),
                Some("1.0.0"),
                vec![],
            ),
            create_test_file_info(
                "project/packages/utils/package.json",
                DatasourceId::NpmPackageJson,
                Some("pkg:npm/%40myorg/utils@2.0.0"),
                Some("@myorg/utils"),
                Some("2.0.0"),
                vec![],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 2);
        let bun_dep = result
            .dependencies
            .iter()
            .find(|dep| dep.datasource_id == DatasourceId::BunLockb)
            .expect("expected bun.lockb hoisted dependency");
        assert_eq!(bun_dep.purl.as_deref(), Some("pkg:npm/typescript@5.8.3"));
        assert_eq!(bun_dep.datafile_path, "project/bun.lockb");
        assert!(bun_dep.for_package_uid.is_none());

        let bun_file = files
            .iter()
            .find(|file| file.path == "project/bun.lockb")
            .expect("expected bun.lockb file");
        assert_eq!(bun_file.for_packages.len(), 2);
    }

    #[test]
    fn test_assemble_python_pyproject_with_named_pylock_toml() {
        let mut files = vec![
            create_test_file_info(
                "project/pyproject.toml",
                DatasourceId::PypiPyprojectToml,
                Some("pkg:pypi/pylock-demo@0.1.0"),
                Some("pylock-demo"),
                Some("0.1.0"),
                vec![],
            ),
            create_test_file_info(
                "project/pylock.dev.toml",
                DatasourceId::PypiPylockToml,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:pypi/pytest@8.3.5".to_string()),
                    extracted_requirement: None,
                    scope: Some("dev".to_string()),
                    is_runtime: Some(false),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected exactly one merged Python package"
        );
        let package = &result.packages[0];
        assert!(
            package
                .datafile_paths
                .contains(&"project/pylock.dev.toml".to_string())
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPylockToml)
        );
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl.as_deref(),
            Some("pkg:pypi/pytest@8.3.5")
        );
    }

    #[test]
    fn test_assemble_python_pip_cache_origin_with_wheel_archive() {
        let mut files = vec![
            create_test_file_info(
                ".cache/pip/wheels/eb/60/37/hash/construct-2.10.68-py3-none-any.whl",
                DatasourceId::PypiWheel,
                Some("pkg:pypi/construct@2.10.68?extension=py3-none-any"),
                Some("construct"),
                Some("2.10.68"),
                vec![],
            ),
            create_test_file_info(
                ".cache/pip/wheels/eb/60/37/hash/origin.json",
                DatasourceId::PypiPipOriginJson,
                Some("pkg:pypi/construct@2.10.68?extension=py3-none-any"),
                Some("construct"),
                Some("2.10.68"),
                vec![],
            ),
        ];

        files[1].package_data[0].download_url = Some(
            "https://files.pythonhosted.org/packages/source/c/construct/construct-2.10.68.tar.gz"
                .to_string(),
        );

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.name.as_deref(), Some("construct"));
        assert_eq!(package.version.as_deref(), Some("2.10.68"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:pypi/construct@2.10.68?extension=py3-none-any")
        );
        assert_eq!(
            package.download_url.as_deref(),
            Some(
                "https://files.pythonhosted.org/packages/source/c/construct/construct-2.10.68.tar.gz"
            )
        );
        assert!(package.datasource_ids.contains(&DatasourceId::PypiWheel));
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiPipOriginJson)
        );
        assert_eq!(package.datafile_paths.len(), 2);
        assert!(files.iter().all(|file| file.for_packages.len() == 1));
        assert_eq!(files[0].for_packages[0], package.package_uid);
        assert_eq!(files[1].for_packages[0], package.package_uid);
    }

    #[test]
    fn test_assemble_python_pip_cache_skips_mismatched_second_wheel() {
        let mut files = vec![
            create_test_file_info(
                ".cache/pip/wheels/eb/60/37/hash/construct-2.10.68-py3-none-any.whl",
                DatasourceId::PypiWheel,
                Some("pkg:pypi/construct@2.10.68?extension=py3-none-any"),
                Some("construct"),
                Some("2.10.68"),
                vec![],
            ),
            create_test_file_info(
                ".cache/pip/wheels/eb/60/37/hash/origin.json",
                DatasourceId::PypiPipOriginJson,
                Some("pkg:pypi/construct@2.10.68?extension=py3-none-any"),
                Some("construct"),
                Some("2.10.68"),
                vec![],
            ),
            create_test_file_info(
                ".cache/pip/wheels/eb/60/37/hash/otherpkg-9.9.9-py3-none-any.whl",
                DatasourceId::PypiWheel,
                Some("pkg:pypi/otherpkg@9.9.9?extension=py3-none-any"),
                Some("otherpkg"),
                Some("9.9.9"),
                vec![],
            ),
        ];

        files[1].package_data[0].download_url = Some(
            "https://files.pythonhosted.org/packages/source/c/construct/construct-2.10.68.tar.gz"
                .to_string(),
        );

        let result = assemble(&mut files);

        assert_eq!(result.packages.len(), 1);
        let package = &result.packages[0];
        assert_eq!(package.name.as_deref(), Some("construct"));
        assert_eq!(package.version.as_deref(), Some("2.10.68"));
        assert_eq!(package.datafile_paths.len(), 2);
        assert!(
            package
                .datafile_paths
                .iter()
                .all(|path| !path.contains("otherpkg"))
        );
        assert!(files[0].for_packages.contains(&package.package_uid));
        assert!(files[1].for_packages.contains(&package.package_uid));
        assert!(files[2].for_packages.is_empty());
    }

    #[test]
    fn test_assemble_deno_json_with_deno_lock() {
        let mut files = vec![
            create_test_file_info(
                "project/deno.json",
                DatasourceId::DenoJson,
                Some("pkg:generic/%40provenant/deno-sample@1.0.0"),
                Some("@provenant/deno-sample"),
                Some("1.0.0"),
                vec![Dependency {
                    purl: Some("pkg:npm/chalk".to_string()),
                    extracted_requirement: Some("npm:chalk@5".to_string()),
                    scope: Some("imports".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
            create_test_file_info(
                "project/deno.lock",
                DatasourceId::DenoLock,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:npm/chalk@5.6.2".to_string()),
                    extracted_requirement: Some("npm:chalk@5".to_string()),
                    scope: Some("imports".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(true),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected exactly one merged Deno package"
        );
        let package = &result.packages[0];
        assert_eq!(package.name, Some("@provenant/deno-sample".to_string()));
        assert!(
            package
                .datafile_paths
                .contains(&"project/deno.json".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/deno.lock".to_string())
        );
        assert!(package.datasource_ids.contains(&DatasourceId::DenoJson));
        assert!(package.datasource_ids.contains(&DatasourceId::DenoLock));

        assert_eq!(result.dependencies.len(), 2);
        assert_eq!(files[0].for_packages.len(), 1);
        assert_eq!(files[1].for_packages.len(), 1);
    }

    #[test]
    fn test_assemble_go_mod_with_go_work() {
        let mut files = vec![
            create_test_file_info(
                "project/go.mod",
                DatasourceId::GoMod,
                Some("pkg:golang/example.com/project"),
                Some("project"),
                None,
                vec![],
            ),
            create_test_file_info(
                "project/go.work",
                DatasourceId::GoWork,
                None,
                None,
                None,
                vec![Dependency {
                    purl: Some("pkg:golang/example.com/mymodule".to_string()),
                    extracted_requirement: Some("./mymodule".to_string()),
                    scope: Some("use".to_string()),
                    is_runtime: Some(true),
                    is_optional: Some(false),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                }],
            ),
        ];

        files[1].package_data[0].extra_data = Some(std::collections::HashMap::from([(
            "use_paths".to_string(),
            serde_json::json!(["./mymodule"]),
        )]));

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected exactly one merged Go package"
        );
        let package = &result.packages[0];
        assert_eq!(package.name, Some("project".to_string()));
        assert!(
            package
                .datafile_paths
                .contains(&"project/go.mod".to_string())
        );
        assert!(
            package
                .datafile_paths
                .contains(&"project/go.work".to_string())
        );
        assert!(package.datasource_ids.contains(&DatasourceId::GoMod));
        assert!(package.datasource_ids.contains(&DatasourceId::GoWork));
        let extra_data = package
            .extra_data
            .as_ref()
            .expect("merged extra_data missing");
        assert!(extra_data.contains_key("use_paths"));
        assert_eq!(result.dependencies.len(), 1);
    }

    #[test]
    fn test_assemble_no_matching_datasource() {
        let mut files = vec![create_test_file_info(
            "project/unknown.json",
            DatasourceId::Readme,
            Some("pkg:unknown/pkg@1.0.0"),
            Some("pkg"),
            Some("1.0.0"),
            vec![],
        )];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            0,
            "Expected no packages for unknown datasource"
        );
        assert_eq!(
            result.dependencies.len(),
            0,
            "Expected no dependencies for unknown datasource"
        );
    }

    #[test]
    fn test_assemble_single_file_no_sibling() {
        let dep = Dependency {
            purl: Some("pkg:npm/lodash@4.17.21".to_string()),
            extracted_requirement: Some("^4.17.0".to_string()),
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        };

        let mut files = vec![create_test_file_info(
            "project/package.json",
            DatasourceId::NpmPackageJson,
            Some("pkg:npm/solo-app@2.0.0"),
            Some("solo-app"),
            Some("2.0.0"),
            vec![dep],
        )];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            1,
            "Expected one package even without lockfile"
        );
        let package = &result.packages[0];
        assert_eq!(package.name, Some("solo-app".to_string()));
        assert_eq!(
            package.datafile_paths.len(),
            1,
            "Expected only one file in datafile_paths"
        );
        assert_eq!(package.datafile_paths[0], "project/package.json");
        assert_eq!(
            package.datasource_ids.len(),
            1,
            "Expected only one datasource ID"
        );

        assert_eq!(result.dependencies.len(), 1, "Expected one dependency");
    }

    #[test]
    fn test_assemble_no_purl_no_package() {
        let mut files = vec![create_test_file_info(
            "project/package.json",
            DatasourceId::NpmPackageJson,
            None,
            Some("no-purl-app"),
            None,
            vec![],
        )];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            0,
            "Expected no packages when PackageData has no purl"
        );
    }

    #[test]
    fn test_assemble_npm_lockfile_does_not_create_package_when_manifest_has_no_purl() {
        let dep = Dependency {
            purl: Some("pkg:npm/express@4.18.0".to_string()),
            extracted_requirement: Some("4.18.0".to_string()),
            scope: Some("dependencies".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(true),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        };

        let mut files = vec![
            create_test_file_info(
                "project/package.json",
                DatasourceId::NpmPackageJson,
                None,
                None,
                None,
                vec![],
            ),
            create_test_file_info(
                "project/package-lock.json",
                DatasourceId::NpmPackageLockJson,
                Some("pkg:npm/lock-only@1.0.0"),
                Some("lock-only"),
                Some("1.0.0"),
                vec![dep],
            ),
        ];

        let result = assemble(&mut files);

        assert!(result.packages.is_empty());
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].for_package_uid, None);
        assert!(files[0].for_packages.is_empty());
        assert!(files[1].for_packages.is_empty());
    }

    #[test]
    fn test_build_package_uid_format() {
        use crate::models::PackageUid;

        let purl = "pkg:npm/test@1.0.0";
        let uid = PackageUid::new(purl);

        assert!(
            uid.as_str().starts_with("pkg:npm/test@1.0.0?uuid="),
            "Expected UUID to be added as qualifier"
        );
        assert!(uid.as_str().contains("uuid="), "Expected uuid qualifier");

        let purl_with_qualifier = "pkg:npm/test@1.0.0?arch=x64";
        let uid2 = PackageUid::new(purl_with_qualifier);

        assert!(
            uid2.as_str().contains("&uuid="),
            "Expected UUID to be appended with & when qualifiers exist"
        );
        assert!(
            uid2.as_str()
                .starts_with("pkg:npm/test@1.0.0?arch=x64&uuid=")
        );
    }

    #[test]
    fn test_package_update_merges_fields() {
        let initial_pkg_data = PackageData {
            datasource_id: Some(DatasourceId::NpmPackageJson),
            purl: Some("pkg:npm/test@1.0.0".to_string()),
            name: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            description: Some("Initial description".to_string()),
            ..Default::default()
        };

        let mut package = Package::from_package_data(&initial_pkg_data, "file1.json".to_string());

        let update_pkg_data = PackageData {
            datasource_id: Some(DatasourceId::NpmPackageLockJson),
            purl: Some("pkg:npm/test@1.0.0".to_string()),
            name: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            homepage_url: Some("https://example.com".to_string()),
            sha256: Some(
                Sha256Digest::from_hex(
                    "abc1230000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
            ),
            ..Default::default()
        };

        package.update(&update_pkg_data, "file2.json".to_string());

        assert_eq!(package.datafile_paths.len(), 2);
        assert_eq!(package.datasource_ids.len(), 2);
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::NpmPackageLockJson)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::NpmPackageJson)
        );
        assert_eq!(
            package.description,
            Some("Initial description".to_string()),
            "Original description should be preserved"
        );
        assert_eq!(
            package.homepage_url,
            Some("https://example.com".to_string()),
            "New homepage should be filled"
        );
        assert_eq!(
            package.sha256,
            Some(
                Sha256Digest::from_hex(
                    "abc1230000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            ),
            "New sha256 should be filled"
        );
        assert_eq!(
            package.homepage_url,
            Some("https://example.com".to_string()),
            "New homepage should be filled"
        );
        assert_eq!(
            package.sha256,
            Some(
                Sha256Digest::from_hex(
                    "abc1230000000000000000000000000000000000000000000000000000000000"
                )
                .unwrap()
            ),
            "New sha256 should be filled"
        );
    }

    #[test]
    fn test_package_update_refreshes_purl_when_version_is_backfilled() {
        let initial_pkg_data = PackageData {
            datasource_id: Some(DatasourceId::PypiPyprojectToml),
            purl: Some("pkg:pypi/test-package".to_string()),
            name: Some("test-package".to_string()),
            version: None,
            ..Default::default()
        };

        let mut package =
            Package::from_package_data(&initial_pkg_data, "pyproject.toml".to_string());
        let original_uid = package.package_uid.clone();

        let update_pkg_data = PackageData {
            datasource_id: Some(DatasourceId::PypiUvLock),
            purl: Some("pkg:pypi/test-package@0.2.0".to_string()),
            name: Some("test-package".to_string()),
            version: Some("0.2.0".to_string()),
            ..Default::default()
        };

        package.update(&update_pkg_data, "uv.lock".to_string());

        assert_eq!(package.purl.as_deref(), Some("pkg:pypi/test-package@0.2.0"));
        assert_ne!(package.package_uid, original_uid);
        assert!(
            package
                .package_uid
                .starts_with("pkg:pypi/test-package@0.2.0?uuid=")
        );
    }

    #[test]
    fn test_matches_pattern_exact() {
        use crate::assembly::sibling_merge::matches_pattern;

        assert!(matches_pattern("package.json", "package.json"));
        assert!(!matches_pattern("package-lock.json", "package.json"));
    }

    #[test]
    fn test_matches_pattern_case_insensitive() {
        use crate::assembly::sibling_merge::matches_pattern;

        assert!(matches_pattern("Cargo.toml", "cargo.toml"));
        assert!(matches_pattern("cargo.toml", "Cargo.toml"));
        assert!(matches_pattern("CARGO.TOML", "cargo.toml"));
    }

    #[test]
    fn test_matches_pattern_glob() {
        use crate::assembly::sibling_merge::matches_pattern;

        assert!(matches_pattern("MyLib.podspec", "*.podspec"));
        assert!(matches_pattern("test.podspec", "*.podspec"));
        assert!(!matches_pattern("podspec", "*.podspec"));
        assert!(!matches_pattern("test.txt", "*.podspec"));

        assert!(matches_pattern("MyLib.podspec.json", "*.podspec.json"));
        assert!(!matches_pattern("MyLib.podspec", "*.podspec.json"));
    }

    #[test]
    fn test_assemble_one_per_package_data_mode() {
        let dep = Dependency {
            purl: Some("pkg:alpine/scanelf".to_string()),
            extracted_requirement: None,
            scope: Some("install".to_string()),
            is_runtime: Some(true),
            is_optional: Some(false),
            is_pinned: Some(false),
            is_direct: Some(true),
            resolved_package: None,
            extra_data: None,
        };

        let path = "rootfs/lib/apk/db/installed";
        let file_name = "installed";
        let extension = "";

        let mut files = vec![FileInfo {
            name: file_name.to_string(),
            base_name: file_name.to_string(),
            extension: extension.to_string(),
            path: path.to_string(),
            file_type: FileType::File,
            mime_type: Some("text/plain".to_string()),
            file_type_label: None,
            size: 5000,
            date: None,
            sha1: None,
            md5: None,
            sha256: None,
            programming_language: None,
            package_data: vec![
                PackageData {
                    datasource_id: Some(DatasourceId::AlpineInstalledDb),
                    purl: Some("pkg:alpine/musl@1.2.3-r0".to_string()),
                    name: Some("musl".to_string()),
                    version: Some("1.2.3-r0".to_string()),
                    dependencies: vec![dep],
                    ..Default::default()
                },
                PackageData {
                    datasource_id: Some(DatasourceId::AlpineInstalledDb),
                    purl: Some("pkg:alpine/busybox@1.35.0-r13".to_string()),
                    name: Some("busybox".to_string()),
                    version: Some("1.35.0-r13".to_string()),
                    dependencies: vec![],
                    ..Default::default()
                },
            ],
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
            scan_diagnostics: vec![],
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
            sha1_git: None,
            is_binary: None,
            is_text: None,
            is_archive: None,
            is_media: None,
            is_script: None,
            facets: vec![],
            tallies: None,
        }];

        let result = assemble(&mut files);

        assert_eq!(
            result.packages.len(),
            2,
            "Expected two independent packages from one database file"
        );

        let musl = result
            .packages
            .iter()
            .find(|p| p.name == Some("musl".to_string()));
        let busybox = result
            .packages
            .iter()
            .find(|p| p.name == Some("busybox".to_string()));

        assert!(musl.is_some(), "Expected musl package");
        assert!(busybox.is_some(), "Expected busybox package");

        let musl = musl.unwrap();
        assert_eq!(musl.version, Some("1.2.3-r0".to_string()));
        assert_eq!(musl.datafile_paths, vec![path.to_string()]);
        assert!(musl.package_uid.contains("uuid="));

        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(
            result.dependencies[0].purl,
            Some("pkg:alpine/scanelf".to_string())
        );

        assert_eq!(
            files[0].for_packages.len(),
            2,
            "Expected database file to reference both packages"
        );
    }
}
