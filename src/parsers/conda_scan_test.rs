// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_conda_assembly_scan_keeps_conda_and_pypi_package_contracts() {
        let (files, result) = scan_and_assemble(Path::new("testdata/conda/assembly"));

        let conda_package = result
            .packages
            .iter()
            .find(|package| {
                package.package_type == Some(PackageType::Conda)
                    && package.name.as_deref() == Some("requests")
            })
            .expect("conda requests package should be assembled");
        let pypi_package = result
            .packages
            .iter()
            .find(|package| {
                package.package_type == Some(PackageType::Pypi)
                    && package.name.as_deref() == Some("requests")
            })
            .expect("embedded pypi requests package should be assembled");

        assert_eq!(conda_package.version.as_deref(), Some("2.32.3"));
        assert_eq!(
            conda_package.purl.as_deref(),
            Some("pkg:conda/requests@2.32.3")
        );
        assert_eq!(pypi_package.version.as_deref(), Some("2.32.3"));
        assert_eq!(
            pypi_package.purl.as_deref(),
            Some("pkg:pypi/requests@2.32.3")
        );
        assert_dependency_present(&result.dependencies, "pkg:conda/zlib", "meta.yaml");
        assert_file_links_to_package(
            &files,
            "/requests-2.32.3-py312h06a4308_1.json",
            &conda_package.package_uid,
            DatasourceId::CondaMetaJson,
        );
        assert_file_links_to_package(
            &files,
            "/site-packages/requests-2.32.3.dist-info/METADATA",
            &pypi_package.package_uid,
            DatasourceId::PypiWheelMetadata,
        );
    }

    #[test]
    fn test_conda_hyphenated_environment_alias_scans_and_assembles() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("conda-env.yaml"),
            "name: alias-env\ndependencies:\n  - requests=2.32.3\n",
        )
        .expect("write conda-env.yaml");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.package_type == Some(PackageType::Conda))
            .expect("conda alias environment should assemble a package");

        assert_eq!(package.name.as_deref(), Some("alias-env"));
        assert_eq!(package.datasource_ids, vec![DatasourceId::CondaYaml]);
        assert_dependency_present(
            &result.dependencies,
            "pkg:conda/requests@2.32.3",
            "conda-env.yaml",
        );
        assert_file_links_to_package(
            &files,
            "/conda-env.yaml",
            &package.package_uid,
            DatasourceId::CondaYaml,
        );
    }

    #[test]
    fn test_conda_recipe_yaml_feedstock_scans_and_assembles() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let recipe_dir = temp_dir.path().join("recipe");
        fs::create_dir_all(&recipe_dir).expect("create recipe dir");
        fs::write(
            recipe_dir.join("recipe.yaml"),
            r#"
schema_version: 1

context:
  version: "3.0.2"

package:
  name: pandas
  version: ${{ version }}

source:
  url: https://github.com/pandas-dev/pandas/releases/download/v${{ version }}/pandas-${{ version }}.tar.gz
  sha256: f4753e73e34c8d83221ba58f232433fca2748be8b18dbca02d242ed153945043

requirements:
  host:
    - python
    - numpy =2.3
  run:
    - python
    - numpy >=1.26.0

about:
  license: BSD-3-Clause
  license_file: LICENSE
  summary: >-
    Powerful data structures for data analysis, time series, and statistics
  homepage: http://pandas.pydata.org
  repository: https://github.com/pandas-dev/pandas
"#,
        )
        .expect("write recipe.yaml");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| {
                package.package_type == Some(PackageType::Conda)
                    && package.name.as_deref() == Some("pandas")
            })
            .expect("recipe.yaml feedstock should assemble a conda package");

        assert_eq!(package.version.as_deref(), Some("3.0.2"));
        assert_eq!(package.datasource_ids, vec![DatasourceId::CondaMetaYaml]);
        assert_dependency_present(
            &result.dependencies,
            "pkg:conda/numpy",
            "recipe/recipe.yaml",
        );
        assert_file_links_to_package(
            &files,
            "/recipe/recipe.yaml",
            &package.package_uid,
            DatasourceId::CondaMetaYaml,
        );
    }
}
