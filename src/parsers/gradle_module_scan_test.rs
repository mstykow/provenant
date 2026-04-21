// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::DatasourceId;

    #[test]
    fn test_gradle_module_scan_assigns_primary_artifact_but_not_documentation_artifact() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let module_path = temp_dir.path().join("converter-moshi-2.11.0.module");
        let jar_path = temp_dir.path().join("converter-moshi-2.11.0.jar");
        let sources_path = temp_dir.path().join("converter-moshi-2.11.0-sources.jar");

        fs::copy(
            Path::new("testdata/gradle-golden/module/converter-moshi-2.11.0.module"),
            &module_path,
        )
        .expect("copy gradle module fixture");
        fs::write(&jar_path, "binary").expect("write main jar");
        fs::write(&sources_path, "sources").expect("write sources jar");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("converter-moshi"))
            .expect("converter-moshi package should be assembled");
        let module_file = files
            .iter()
            .find(|file| file.path.ends_with("/converter-moshi-2.11.0.module"))
            .expect("module file should be scanned");
        let jar_file = files
            .iter()
            .find(|file| file.path.ends_with("/converter-moshi-2.11.0.jar"))
            .expect("main jar should be scanned");
        let sources_file = files
            .iter()
            .find(|file| file.path.ends_with("/converter-moshi-2.11.0-sources.jar"))
            .expect("sources jar should be scanned");

        assert!(
            module_file
                .package_data
                .iter()
                .any(|pkg_data| { pkg_data.datasource_id == Some(DatasourceId::GradleModule) })
        );
        assert!(jar_file.for_packages.contains(&package.package_uid));
        assert!(sources_file.for_packages.is_empty());
    }
}
