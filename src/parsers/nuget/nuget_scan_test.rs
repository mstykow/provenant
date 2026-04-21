// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_nuget_basic_scan_assembles_csproj_and_packages_config() {
        let (files, result) = scan_and_assemble(Path::new("testdata/assembly-golden/nuget-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("Contoso.Utility"))
            .expect("nuget package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Nuget));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:nuget/Contoso.Utility@1.0.0")
        );
        assert_dependency_present(&result.dependencies, "pkg:nuget/NUnit", "packages.config");
        assert_file_links_to_package(
            &files,
            "/Contoso.Utility.csproj",
            &package.package_uid,
            DatasourceId::NugetCsproj,
        );
        assert_file_links_to_package(
            &files,
            "/packages.config",
            &package.package_uid,
            DatasourceId::NugetPackagesConfig,
        );
    }

    #[test]
    fn test_nuget_named_packages_lock_scan_assembles_with_project() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("Contoso.Utility.csproj"),
            include_str!("../../../testdata/assembly-golden/nuget-basic/Contoso.Utility.csproj"),
        )
        .expect("write csproj");
        fs::write(
            temp_dir.path().join("Contoso.Utility.packages.lock.json"),
            r#"{
  "version": 1,
  "dependencies": {
    ".NETFramework,Version=v4.7.2": {
      "NUnit": {
        "type": "Direct",
        "requested": "[3.13.2, )",
        "resolved": "3.13.2",
        "contentHash": "sha512-example"
      }
    }
  }
}
"#,
        )
        .expect("write named packages.lock.json");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("Contoso.Utility"))
            .expect("nuget package should be assembled with named packages lock");

        assert_eq!(package.package_type, Some(PackageType::Nuget));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_dependency_present(
            &result.dependencies,
            "pkg:nuget/NUnit@3.13.2",
            "Contoso.Utility.packages.lock.json",
        );
        assert_file_links_to_package(
            &files,
            "/Contoso.Utility.packages.lock.json",
            &package.package_uid,
            DatasourceId::NugetPackagesLock,
        );
    }
}
