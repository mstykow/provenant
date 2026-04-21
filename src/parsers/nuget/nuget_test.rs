// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::super::PackageParser;
    use super::super::{
        CentralPackageManagementPropsParser, DirectoryBuildPropsParser, DotNetDepsJsonParser,
        NupkgParser, NuspecParser, PackageReferenceProjectParser, PackagesConfigParser,
        PackagesLockParser, ProjectJsonParser, ProjectLockJsonParser,
    };
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::{Builder, NamedTempFile, TempDir};

    fn write_directory_packages_props(contents: &str) -> (TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("Directory.Packages.props");
        std::fs::write(&path, contents).unwrap();
        (temp_dir, path)
    }

    fn write_directory_build_props(contents: &str) -> (TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("Directory.Build.props");
        std::fs::write(&path, contents).unwrap();
        (temp_dir, path)
    }

    #[test]
    fn test_packages_config_is_match() {
        assert!(PackagesConfigParser::is_match(&PathBuf::from(
            "packages.config"
        )));
        assert!(!PackagesConfigParser::is_match(&PathBuf::from("other.xml")));
        assert!(!PackagesConfigParser::is_match(&PathBuf::from(
            "packages.config.bak"
        )));
    }

    #[test]
    fn test_nuspec_is_match() {
        assert!(NuspecParser::is_match(&PathBuf::from("example.nuspec")));
        assert!(NuspecParser::is_match(&PathBuf::from("MyPackage.nuspec")));
        assert!(!NuspecParser::is_match(&PathBuf::from("example.xml")));
        assert!(!NuspecParser::is_match(&PathBuf::from("example.nupkg")));
    }

    #[test]
    fn test_packages_config_simple() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<packages>
  <package id="Newtonsoft.Json" version="13.0.1" targetFramework="net472" />
  <package id="NUnit" version="3.13.2" targetFramework="net472" />
</packages>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = PackagesConfigParser::extract_first_package(path);

        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetPackagesConfig)
        );

        let deps = &package_data.dependencies;
        assert_eq!(deps.len(), 2);

        let dep1 = &deps[0];
        assert_eq!(dep1.purl, Some("pkg:nuget/Newtonsoft.Json".to_string()));
        assert_eq!(dep1.extracted_requirement, Some("13.0.1".to_string()));
        assert_eq!(dep1.scope, Some("net472".to_string()));
        assert_eq!(dep1.is_pinned, Some(true));
        assert_eq!(dep1.is_direct, Some(true));

        let dep2 = &deps[1];
        assert_eq!(dep2.purl, Some("pkg:nuget/NUnit".to_string()));
        assert_eq!(dep2.extracted_requirement, Some("3.13.2".to_string()));
    }

    #[test]
    fn test_packages_config_no_target_framework() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<packages>
  <package id="jQuery" version="3.6.0" />
</packages>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = PackagesConfigParser::extract_first_package(path);

        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(package_data.dependencies[0].scope, None);
    }

    #[test]
    fn test_nuspec_basic() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>Newtonsoft.Json</id>
    <version>13.0.1</version>
    <description>Json.NET is a popular high-performance JSON framework for .NET</description>
    <authors>James Newton-King</authors>
    <owners>James Newton-King</owners>
    <license>MIT</license>
    <projectUrl>https://www.newtonsoft.com/json</projectUrl>
    <copyright>Copyright © James Newton-King 2008</copyright>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(package_data.name, Some("Newtonsoft.Json".to_string()));
        assert_eq!(package_data.version, Some("13.0.1".to_string()));
        assert!(package_data.description.is_some());
        assert_eq!(
            package_data.homepage_url,
            Some("https://www.newtonsoft.com/json".to_string())
        );
        assert_eq!(
            package_data.copyright,
            Some("Copyright © James Newton-King 2008".to_string())
        );

        let parties = &package_data.parties;
        assert_eq!(parties.len(), 2);
        assert_eq!(parties[0].name, Some("James Newton-King".to_string()));
        assert_eq!(parties[0].role, Some("author".to_string()));
        assert_eq!(parties[1].role, Some("owner".to_string()));
    }

    #[test]
    fn test_nuspec_parties_have_person_type() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Dapper</id>
    <version>2.1.0</version>
    <authors>Sam Saffron,Marc Gravell,Nick Craver</authors>
    <owners>Sam Saffron,Marc Gravell,Nick Craver</owners>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = NuspecParser::extract_first_package(temp_file.path());

        assert_eq!(package_data.parties.len(), 2);
        assert_eq!(package_data.parties[0].r#type.as_deref(), Some("person"));
        assert_eq!(package_data.parties[1].r#type.as_deref(), Some("person"));
    }

    #[test]
    fn test_nuspec_with_dependencies() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>MyPackage</id>
    <version>1.0.0</version>
    <dependencies>
      <dependency id="Newtonsoft.Json" version="13.0.1" />
      <dependency id="NUnit" version="[3.0,4.0)" />
    </dependencies>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);
        let deps = &package_data.dependencies;

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].purl, Some("pkg:nuget/Newtonsoft.Json".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some("13.0.1".to_string()));
        assert_eq!(deps[0].scope, Some("dependency".to_string()));
        assert_eq!(deps[0].is_pinned, Some(false));

        assert_eq!(deps[1].purl, Some("pkg:nuget/NUnit".to_string()));
        assert_eq!(deps[1].extracted_requirement, Some("[3.0,4.0)".to_string()));
    }

    #[test]
    fn test_nuspec_with_dependency_groups() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>MyPackage</id>
    <version>1.0.0</version>
    <dependencies>
      <group targetFramework="net45">
        <dependency id="Newtonsoft.Json" version="12.0.0" />
      </group>
      <group targetFramework="netstandard2.0">
        <dependency id="Newtonsoft.Json" version="13.0.1" />
        <dependency id="System.Text.Json" version="6.0.0" />
      </group>
    </dependencies>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);
        let deps = &package_data.dependencies;

        assert_eq!(deps.len(), 3);

        // First group (net45)
        assert_eq!(deps[0].purl, Some("pkg:nuget/Newtonsoft.Json".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some("12.0.0".to_string()));
        let extra = deps[0].extra_data.as_ref().unwrap();
        assert_eq!(extra["framework"], "net45");

        // Second group (netstandard2.0)
        assert_eq!(deps[1].purl, Some("pkg:nuget/Newtonsoft.Json".to_string()));
        let extra = deps[1].extra_data.as_ref().unwrap();
        assert_eq!(extra["framework"], "netstandard2.0");

        assert_eq!(deps[2].purl, Some("pkg:nuget/System.Text.Json".to_string()));
        let extra = deps[2].extra_data.as_ref().unwrap();
        assert_eq!(extra["framework"], "netstandard2.0");
    }

    #[test]
    fn test_nuspec_with_repository() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>MyPackage</id>
    <version>1.0.0</version>
    <repository type="git" url="https://github.com/user/repo.git" />
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(
            package_data.vcs_url,
            Some("git+https://github.com/user/repo.git".to_string())
        );
    }

    #[test]
    fn test_nuspec_license_url_fallback() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>OldPackage</id>
    <version>1.0.0</version>
    <licenseUrl>https://opensource.org/licenses/MIT</licenseUrl>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(
            package_data.extracted_license_statement,
            Some("https://opensource.org/licenses/MIT".to_string())
        );
    }

    #[test]
    fn test_nuspec_modern_license_expression_tracks_license_type() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Microsoft.WindowsPackageManager.Utils</id>
    <version>1.0.0</version>
    <authors>Microsoft</authors>
    <projectUrl>https://github.com/microsoft/winget-cli</projectUrl>
    <license type="expression">MIT</license>
    <description>The utility binary for use with the WinGet CLI.</description>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = NuspecParser::extract_first_package(temp_file.path());
        let extra = package_data.extra_data.unwrap();

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(extra["license_type"], "expression");
    }

    #[test]
    fn test_nuspec_file_license_prefers_license_over_placeholder_url() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Fizzler</id>
    <version>1.3.0</version>
    <license type="file">COPYING.txt</license>
    <licenseUrl>https://aka.ms/deprecateLicenseUrl</licenseUrl>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = NuspecParser::extract_first_package(temp_file.path());
        let extra = package_data.extra_data.unwrap();

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("COPYING.txt")
        );
        assert_eq!(extra["license_type"], "file");
        assert_eq!(extra["license_file"], "COPYING.txt");
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.license_detections[0].license_expression,
            "unknown-license-reference"
        );
        assert_eq!(
            package_data.license_detections[0].matches[0]
                .referenced_filenames
                .as_ref(),
            Some(&vec!["COPYING.txt".to_string()])
        );
    }

    #[test]
    fn test_nuspec_repository_branch_and_commit_are_preserved() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Fizzler</id>
    <version>1.3.0</version>
    <repository type="Git" url="https://github.com/atifaziz/Fizzler" commit="8323ec7a49ce5dff579b1aa146492ee7aa0ab10d" branch="main" />
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = NuspecParser::extract_first_package(temp_file.path());
        let extra = package_data.extra_data.unwrap();

        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("Git+https://github.com/atifaziz/Fizzler")
        );
        assert_eq!(extra["repository_branch"], "main");
        assert_eq!(
            extra["repository_commit"],
            "8323ec7a49ce5dff579b1aa146492ee7aa0ab10d"
        );
    }

    #[test]
    fn test_nuspec_malformed_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Broken</id>
    <version>1.0.0
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        // Should return default package data on error
        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::NugetNuspec));
        assert!(package_data.name.is_none());
    }

    #[test]
    fn test_packages_config_empty() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<packages>
</packages>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = PackagesConfigParser::extract_first_package(path);

        assert_eq!(package_data.dependencies.len(), 0);
    }

    #[test]
    fn test_nuspec_repository_urls() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>TestPackage</id>
    <version>2.5.0</version>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://www.nuget.org/packages/TestPackage/2.5.0".to_string())
        );
        assert_eq!(
            package_data.repository_download_url,
            Some("https://www.nuget.org/api/v2/package/TestPackage/2.5.0".to_string())
        );
        assert_eq!(
            package_data.api_data_url,
            Some("https://api.nuget.org/v3/registration3/testpackage/2.5.0.json".to_string())
        );
    }

    #[test]
    fn test_packages_lock_is_match() {
        assert!(PackagesLockParser::is_match(&PathBuf::from(
            "packages.lock.json"
        )));
        assert!(PackagesLockParser::is_match(&PathBuf::from(
            "MyProject.packages.lock.json"
        )));
        assert!(!PackagesLockParser::is_match(&PathBuf::from(
            "package.json"
        )));
    }

    #[test]
    fn test_nupkg_is_match() {
        assert!(NupkgParser::is_match(&PathBuf::from("Example.nupkg")));
        assert!(NupkgParser::is_match(&PathBuf::from(
            "Newtonsoft.Json.13.0.1.nupkg"
        )));
        assert!(!NupkgParser::is_match(&PathBuf::from("example.zip")));
    }

    #[test]
    fn test_packages_lock_simple() {
        let json = r#"{
  "version": 1,
  "dependencies": {
    "net5.0": {
      "Newtonsoft.Json": {
        "type": "Direct",
        "requested": "[13.0.1, )",
        "resolved": "13.0.1",
        "contentHash": "sha512-ppPFpBcvxdsfUonNcvITKqLl3bqxWbDCZIzDWHzjpdAHRFfZe0Dw9HmA0+za13IdyrgJwpkDTDA9fHaxOrt20A=="
      }
    }
  }
}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = PackagesLockParser::extract_first_package(path);

        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetPackagesLock)
        );

        let deps = &package_data.dependencies;
        assert_eq!(deps.len(), 1);

        let dep = &deps[0];
        assert_eq!(
            dep.purl,
            Some("pkg:nuget/Newtonsoft.Json@13.0.1".to_string())
        );
        assert_eq!(dep.extracted_requirement, Some("[13.0.1, )".to_string()));
        assert_eq!(dep.scope, Some("net5.0".to_string()));
        assert_eq!(dep.is_direct, Some(true));
        assert_eq!(dep.is_pinned, Some(true));
    }

    #[test]
    fn test_packages_lock_multiple_frameworks() {
        let json = r#"{
  "version": 1,
  "dependencies": {
    "net5.0": {
      "Newtonsoft.Json": {
        "type": "Direct",
        "resolved": "13.0.1"
      }
    },
    "netstandard2.0": {
      "System.Text.Json": {
        "type": "Transitive",
        "resolved": "6.0.0"
      }
    }
  }
}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = PackagesLockParser::extract_first_package(path);
        let deps = &package_data.dependencies;

        assert_eq!(deps.len(), 2);

        assert_eq!(deps[0].scope, Some("net5.0".to_string()));
        assert_eq!(deps[0].is_direct, Some(true));

        assert_eq!(deps[1].scope, Some("netstandard2.0".to_string()));
        assert_eq!(deps[1].is_direct, Some(false));
    }

    #[test]
    fn test_project_json_is_match() {
        assert!(ProjectJsonParser::is_match(&PathBuf::from("project.json")));
        assert!(!ProjectJsonParser::is_match(&PathBuf::from(
            "project.lock.json"
        )));
    }

    #[test]
    fn test_project_json_extracts_dependencies() {
        let json = r#"{
  "name": "Legacy.Project",
  "version": "1.2.3",
  "description": "Legacy project.json manifest",
  "projectUrl": "https://example.test/legacy",
  "dependencies": {
    "Newtonsoft.Json": "13.0.1",
    "Native.Package": {
      "version": "2.0.0",
      "include": "build, native",
      "exclude": "contentFiles"
    }
  },
  "frameworks": {
    "net46": {
      "dependencies": {
        "Framework.Package": {
          "version": "3.1.4",
          "type": "build"
        }
      }
    }
  }
}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let package_data = ProjectJsonParser::extract_first_package(temp_file.path());

        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetProjectJson)
        );
        assert_eq!(package_data.name.as_deref(), Some("Legacy.Project"));
        assert_eq!(package_data.version.as_deref(), Some("1.2.3"));
        assert_eq!(package_data.dependencies.len(), 3);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:nuget/Newtonsoft.Json")
        );
        assert_eq!(
            package_data.dependencies[1].extra_data.as_ref().unwrap()["include"],
            "build, native"
        );
        assert_eq!(package_data.dependencies[2].scope.as_deref(), Some("net46"));
        assert_eq!(
            package_data.dependencies[2].extra_data.as_ref().unwrap()["type"],
            "build"
        );
    }

    #[test]
    fn test_project_lock_json_is_match() {
        assert!(ProjectLockJsonParser::is_match(&PathBuf::from(
            "project.lock.json"
        )));
        assert!(!ProjectLockJsonParser::is_match(&PathBuf::from(
            "packages.lock.json"
        )));
    }

    #[test]
    fn test_project_lock_json_extracts_dependency_groups() {
        let json = r#"{
  "version": 2,
  "projectFileDependencyGroups": {
    "": [
      "Newtonsoft.Json >= 13.0.1"
    ],
    ".NETCoreApp,Version=v1.0": [
      "Microsoft.NETCore.App >= 1.0.0"
    ]
  },
  "libraries": {}
}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let package_data = ProjectLockJsonParser::extract_first_package(temp_file.path());

        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetProjectLockJson)
        );
        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data.dependencies[0]
                .extracted_requirement
                .as_deref(),
            Some(">= 13.0.1")
        );
        assert_eq!(package_data.dependencies[0].scope, None);
        assert_eq!(
            package_data.dependencies[1].scope.as_deref(),
            Some(".NETCoreApp,Version=v1.0")
        );
    }

    #[test]
    fn test_dotnet_deps_json_is_match() {
        assert!(DotNetDepsJsonParser::is_match(&PathBuf::from(
            "ExampleApp.deps.json"
        )));
        assert!(!DotNetDepsJsonParser::is_match(&PathBuf::from(
            "ExampleApp.runtimeconfig.json"
        )));
    }

    #[test]
    fn test_dotnet_deps_json_extracts_root_and_dependencies() {
        let package_data = DotNetDepsJsonParser::extract_first_package(&PathBuf::from(
            "testdata/nuget-golden/deps-json/ExampleApp.deps.json",
        ));

        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetDepsJson)
        );
        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(package_data.name.as_deref(), Some("ExampleApp"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:nuget/ExampleApp@1.0.0")
        );

        let extra = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert_eq!(
            extra
                .get("runtime_target_name")
                .and_then(|value| value.as_str()),
            Some(".NETCoreApp,Version=v8.0/win-x64")
        );
        assert_eq!(
            extra
                .get("target_framework")
                .and_then(|value| value.as_str()),
            Some(".NETCoreApp,Version=v8.0")
        );
        assert_eq!(
            extra
                .get("runtime_identifier")
                .and_then(|value| value.as_str()),
            Some("win-x64")
        );
        assert_eq!(
            extra
                .get("runtime_signature")
                .and_then(|value| value.as_str()),
            Some("signature-value")
        );

        assert_eq!(package_data.dependencies.len(), 4);

        let newtonsoft = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nuget/Newtonsoft.Json@13.0.1"))
            .expect("Newtonsoft.Json dependency missing");
        assert_eq!(newtonsoft.is_direct, Some(true));
        let newtonsoft_extra = newtonsoft.extra_data.as_ref().expect("extra_data missing");
        assert_eq!(newtonsoft_extra["type"], "package");
        assert_eq!(newtonsoft_extra["sha512"], "newton-hash");

        let transitive = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nuget/System.Text.Json@8.0.0"))
            .expect("System.Text.Json dependency missing");
        assert_eq!(transitive.is_direct, Some(false));
        assert_eq!(transitive.is_runtime, Some(false));
        assert_eq!(transitive.is_optional, Some(true));

        let project_ref = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nuget/Project.Ref@1.0.0"))
            .expect("Project.Ref dependency missing");
        let project_ref_extra = project_ref.extra_data.as_ref().expect("extra_data missing");
        assert_eq!(project_ref_extra["type"], "project");
    }

    #[test]
    fn test_dotnet_deps_json_fallback_target_selection() {
        let json = r#"{
  "targets": {
    ".NETCoreApp,Version=v9.0": {
      "FallbackApp/2.0.0": {
        "dependencies": {
          "NUnit": "3.14.0"
        }
      },
      "NUnit/3.14.0": {}
    }
  },
  "libraries": {
    "FallbackApp/2.0.0": { "type": "project" },
    "NUnit/3.14.0": { "type": "package" }
  }
}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let package_data = DotNetDepsJsonParser::extract_first_package(temp_file.path());
        assert_eq!(package_data.name.as_deref(), Some("FallbackApp"));
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0].scope.as_deref(),
            Some(".NETCoreApp,Version=v9.0")
        );
    }

    #[test]
    fn test_dotnet_deps_json_without_project_root_returns_dependency_only_package() {
        let json = r#"{
  "runtimeTarget": {
    "name": ".NETCoreApp,Version=v8.0/linux-x64"
  },
  "targets": {
    ".NETCoreApp,Version=v8.0/linux-x64": {
      "Newtonsoft.Json/13.0.1": {},
      "Serilog/2.12.0": {}
    }
  },
  "libraries": {
    "Newtonsoft.Json/13.0.1": { "type": "package" },
    "Serilog/2.12.0": { "type": "package" }
  }
}"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("ExampleApp.deps.json");
        std::fs::write(&file_path, json).unwrap();

        let package_data = DotNetDepsJsonParser::extract_first_package(&file_path);
        assert_eq!(package_data.name.as_deref(), Some("ExampleApp"));
        assert_eq!(package_data.purl.as_deref(), Some("pkg:nuget/ExampleApp"));
        assert_eq!(package_data.dependencies.len(), 2);
        assert!(
            package_data
                .dependencies
                .iter()
                .all(|dep| dep.is_direct.is_none())
        );
    }

    #[test]
    fn test_dotnet_deps_json_without_project_root_and_without_named_path_stays_anonymous() {
        let json = r#"{
  "targets": {
    ".NETCoreApp,Version=v8.0": {
      "Newtonsoft.Json/13.0.1": {},
      "Serilog/2.12.0": {}
    }
  },
  "libraries": {
    "Newtonsoft.Json/13.0.1": { "type": "package" },
    "Serilog/2.12.0": { "type": "package" }
  }
}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let package_data = DotNetDepsJsonParser::extract_first_package(temp_file.path());
        assert!(package_data.name.is_none());
        assert!(package_data.purl.is_none());
        assert_eq!(package_data.dependencies.len(), 2);
    }

    #[test]
    fn test_dotnet_deps_json_prefers_project_root_matching_filename() {
        let json = r#"{
  "runtimeTarget": {
    "name": ".NETCoreApp,Version=v8.0/win-x64"
  },
  "targets": {
    ".NETCoreApp,Version=v8.0/win-x64": {
      "ExampleApp/1.0.0": {
        "dependencies": {
          "Newtonsoft.Json": "13.0.1"
        }
      },
      "SupportProject/1.1.0": {
        "dependencies": {
          "Serilog": "2.12.0"
        }
      },
      "Newtonsoft.Json/13.0.1": {},
      "Serilog/2.12.0": {}
    }
  },
  "libraries": {
    "ExampleApp/1.0.0": { "type": "project" },
    "SupportProject/1.1.0": { "type": "project" },
    "Newtonsoft.Json/13.0.1": { "type": "package" },
    "Serilog/2.12.0": { "type": "package" }
  }
}"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("ExampleApp.deps.json");
        std::fs::write(&file_path, json).unwrap();

        let package_data = DotNetDepsJsonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name.as_deref(), Some("ExampleApp"));
        let direct_names: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.is_direct == Some(true))
            .filter_map(|dep| dep.purl.as_deref())
            .collect();
        assert!(direct_names.contains(&"pkg:nuget/Newtonsoft.Json@13.0.1"));
        assert!(!direct_names.contains(&"pkg:nuget/Serilog@2.12.0"));
    }

    #[test]
    fn test_dotnet_deps_json_malformed_returns_default() {
        let json = r#"{"runtimeTarget": "broken""#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let package_data = DotNetDepsJsonParser::extract_first_package(temp_file.path());
        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetDepsJson)
        );
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_package_reference_project_is_match() {
        assert!(PackageReferenceProjectParser::is_match(&PathBuf::from(
            "example.csproj"
        )));
        assert!(PackageReferenceProjectParser::is_match(&PathBuf::from(
            "example.vbproj"
        )));
        assert!(PackageReferenceProjectParser::is_match(&PathBuf::from(
            "example.fsproj"
        )));
        assert!(!PackageReferenceProjectParser::is_match(&PathBuf::from(
            "example.sln"
        )));
    }

    #[test]
    fn test_directory_packages_props_is_match() {
        assert!(CentralPackageManagementPropsParser::is_match(
            &PathBuf::from("Directory.Packages.props")
        ));
        assert!(!CentralPackageManagementPropsParser::is_match(
            &PathBuf::from("Directory.Build.props")
        ));
        assert!(!CentralPackageManagementPropsParser::is_match(
            &PathBuf::from("packages.props")
        ));
    }

    #[test]
    fn test_directory_build_props_is_match() {
        assert!(DirectoryBuildPropsParser::is_match(&PathBuf::from(
            "Directory.Build.props"
        )));
        assert!(!DirectoryBuildPropsParser::is_match(&PathBuf::from(
            "Directory.Packages.props"
        )));
        assert!(!DirectoryBuildPropsParser::is_match(&PathBuf::from(
            "Directory.Build.targets"
        )));
    }

    #[test]
    fn test_directory_packages_props_extracts_package_versions() {
        let xml = r#"<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
    <CentralPackageTransitivePinningEnabled>true</CentralPackageTransitivePinningEnabled>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageVersion Include="Serilog" Version="3.1.1" Condition="'$(TargetFramework)' == 'net8.0'" />
  </ItemGroup>
</Project>"#;

        let (_temp_dir, path) = write_directory_packages_props(xml);

        let package_data = CentralPackageManagementPropsParser::extract_first_package(&path);
        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetDirectoryPackagesProps)
        );
        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.dependencies.len(), 2);

        let dep1 = &package_data.dependencies[0];
        assert_eq!(dep1.purl.as_deref(), Some("pkg:nuget/Newtonsoft.Json"));
        assert_eq!(dep1.extracted_requirement.as_deref(), Some("13.0.3"));
        assert_eq!(dep1.scope.as_deref(), Some("package_version"));
        assert_eq!(dep1.is_direct, Some(true));

        let dep2 = &package_data.dependencies[1];
        assert_eq!(dep2.purl.as_deref(), Some("pkg:nuget/Serilog"));
        assert_eq!(dep2.extracted_requirement.as_deref(), Some("3.1.1"));
        let extra = dep2.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("condition").and_then(|v| v.as_str()),
            Some("'$(TargetFramework)' == 'net8.0'")
        );

        let package_extra = package_data.extra_data.as_ref().unwrap();
        assert_eq!(
            package_extra
                .get("manage_package_versions_centrally")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            package_extra
                .get("central_package_transitive_pinning_enabled")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_directory_packages_props_extracts_update_entries() {
        let xml = r#"<Project>
  <ItemGroup Condition="'$(TargetFramework)' == 'net472'">
    <PackageVersion Update="NUnit" Version="4.0.1" />
  </ItemGroup>
</Project>"#;

        let (_temp_dir, path) = write_directory_packages_props(xml);

        let package_data = CentralPackageManagementPropsParser::extract_first_package(&path);
        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:nuget/NUnit"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("4.0.1"));
        let extra = dep.extra_data.as_ref().unwrap();
        assert_eq!(
            extra.get("condition").and_then(|v| v.as_str()),
            Some("'$(TargetFramework)' == 'net472'")
        );
    }

    #[test]
    fn test_directory_packages_props_extracts_imported_parent_metadata_and_property_backed_versions()
     {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_props = temp_dir.path().join("Directory.Packages.props");
        std::fs::write(
            &root_props,
            r#"<Project>
  <PropertyGroup>
    <ManageVersions>true</ManageVersions>
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
  </PropertyGroup>
</Project>"#,
        )
        .unwrap();

        let child_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&child_dir).unwrap();
        let child_props = child_dir.join("Directory.Packages.props");
        std::fs::write(
            &child_props,
            r#"<Project>
  <Import Project="$([MSBuild]::GetPathOfFileAbove(Directory.Packages.props, $(MSBuildThisFileDirectory)..))" />
  <PropertyGroup>
    <ManagePackageVersionsCentrally>$(ManageVersions)</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="$(NewtonsoftJsonVersion)" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();

        let package_data = CentralPackageManagementPropsParser::extract_first_package(&child_props);
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0]
                .extracted_requirement
                .as_deref(),
            Some("13.0.3")
        );
        let package_extra = package_data.extra_data.as_ref().unwrap();
        assert_eq!(
            package_extra
                .get("manage_package_versions_centrally")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            package_extra
                .get("import_projects")
                .and_then(|v| v.as_array())
                .and_then(|v| v.first())
                .and_then(|v| v.as_str()),
            Some(
                "$([MSBuild]::GetPathOfFileAbove(Directory.Packages.props, $(MSBuildThisFileDirectory)..))"
            )
        );
    }

    #[test]
    fn test_directory_build_props_extracts_properties_and_imported_parent_values() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root_props = temp_dir.path().join("Directory.Build.props");
        std::fs::write(
            &root_props,
            r#"<Project>
  <PropertyGroup>
    <ManageVersions>true</ManageVersions>
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
  </PropertyGroup>
</Project>"#,
        )
        .unwrap();

        let child_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&child_dir).unwrap();
        let child_props = child_dir.join("Directory.Build.props");
        std::fs::write(
            &child_props,
            r#"<Project>
  <Import Project="$([MSBuild]::GetPathOfFileAbove(Directory.Build.props, $(MSBuildThisFileDirectory)..))" />
  <PropertyGroup>
    <ManagePackageVersionsCentrally>$(ManageVersions)</ManagePackageVersionsCentrally>
  </PropertyGroup>
</Project>"#,
        )
        .unwrap();

        let package_data = DirectoryBuildPropsParser::extract_first_package(&child_props);
        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetDirectoryBuildProps)
        );
        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing extra_data");
        assert_eq!(
            extra_data
                .get("manage_package_versions_centrally")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            extra_data
                .get("property_values")
                .and_then(|v| v.get("NewtonsoftJsonVersion"))
                .and_then(|v| v.as_str()),
            Some("13.0.3")
        );
        assert_eq!(
            extra_data
                .get("import_projects")
                .and_then(|v| v.as_array())
                .and_then(|v| v.first())
                .and_then(|v| v.as_str()),
            Some(
                "$([MSBuild]::GetPathOfFileAbove(Directory.Build.props, $(MSBuildThisFileDirectory)..))"
            )
        );
    }

    #[test]
    fn test_directory_build_props_ignores_unsupported_import_targets() {
        let xml = r#"<Project>
  <Import Project="../Directory.Build.targets" />
  <PropertyGroup>
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
  </PropertyGroup>
</Project>"#;

        let (_temp_dir, path) = write_directory_build_props(xml);
        let package_data = DirectoryBuildPropsParser::extract_first_package(&path);
        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing extra_data");
        assert!(
            extra_data
                .get("import_projects")
                .and_then(|value| value.as_array())
                .is_none_or(|values| values.is_empty())
        );
        assert_eq!(
            extra_data
                .get("property_values")
                .and_then(|v| v.get("NewtonsoftJsonVersion"))
                .and_then(|v| v.as_str()),
            Some("13.0.3")
        );
    }

    #[test]
    fn test_directory_build_props_ignores_conditioned_imports_and_property_groups() {
        let xml = r#"<Project>
  <Import Project="$([MSBuild]::GetPathOfFileAbove(Directory.Build.props, $(MSBuildThisFileDirectory)..))" Condition="'$(TargetFramework)' == 'net8.0'" />
  <PropertyGroup Condition="'$(TargetFramework)' == 'net8.0'">
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
  </PropertyGroup>
  <PropertyGroup>
    <ManageVersions>true</ManageVersions>
  </PropertyGroup>
</Project>"#;

        let (_temp_dir, path) = write_directory_build_props(xml);
        let package_data = DirectoryBuildPropsParser::extract_first_package(&path);
        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing extra_data");
        assert!(
            extra_data
                .get("import_projects")
                .and_then(|value| value.as_array())
                .is_none_or(|values| values.is_empty())
        );
        assert!(
            extra_data
                .get("property_values")
                .and_then(|v| v.get("NewtonsoftJsonVersion"))
                .is_none()
        );
        assert_eq!(
            extra_data
                .get("property_values")
                .and_then(|v| v.get("ManageVersions"))
                .and_then(|v| v.as_str()),
            Some("true")
        );
    }

    #[test]
    fn test_directory_packages_props_ignores_non_cpm_import_targets() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_props = temp_dir.path().join("Directory.Build.props");
        std::fs::write(
            &build_props,
            r#"<Project>
  <PropertyGroup>
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
  </PropertyGroup>
</Project>"#,
        )
        .unwrap();

        let child_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&child_dir).unwrap();
        let child_props = child_dir.join("Directory.Packages.props");
        std::fs::write(
            &child_props,
            r#"<Project>
  <Import Project="../Directory.Build.props" />
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="$(NewtonsoftJsonVersion)" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();

        let package_data = CentralPackageManagementPropsParser::extract_first_package(&child_props);
        assert_eq!(package_data.dependencies.len(), 0);
        assert!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|data| data.get("import_projects"))
                .is_none()
        );
    }

    #[test]
    fn test_csproj_versionless_package_reference_remains_unresolved_in_this_slice() {
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <PackageId>Contoso.Utility</PackageId>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" />
    <PackageReference Include="Serilog">
      <Version>2.10.0</Version>
    </PackageReference>
  </ItemGroup>
</Project>"#;

        let mut temp_file = Builder::new().suffix(".csproj").tempfile().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = PackageReferenceProjectParser::extract_first_package(temp_file.path());
        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:nuget/Newtonsoft.Json")
        );
        assert!(package_data.dependencies[0].extracted_requirement.is_none());
        assert_eq!(
            package_data.dependencies[1]
                .extracted_requirement
                .as_deref(),
            Some("2.10.0")
        );
    }

    #[test]
    fn test_csproj_package_reference_preserves_literal_version_override_metadata() {
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" VersionOverride="13.0.3" />
    <PackageReference Include="Serilog">
      <VersionOverride>2.12.0</VersionOverride>
    </PackageReference>
  </ItemGroup>
</Project>"#;

        let mut temp_file = Builder::new().suffix(".csproj").tempfile().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = PackageReferenceProjectParser::extract_first_package(temp_file.path());
        assert_eq!(package_data.dependencies.len(), 2);

        let first_extra = package_data.dependencies[0]
            .extra_data
            .as_ref()
            .expect("first PackageReference extra_data missing");
        assert_eq!(
            first_extra
                .get("version_override")
                .and_then(|value| value.as_str()),
            Some("13.0.3")
        );
        assert!(package_data.dependencies[0].extracted_requirement.is_none());

        let second_extra = package_data.dependencies[1]
            .extra_data
            .as_ref()
            .expect("second PackageReference extra_data missing");
        assert_eq!(
            second_extra
                .get("version_override")
                .and_then(|value| value.as_str()),
            Some("2.12.0")
        );
        assert!(package_data.dependencies[1].extracted_requirement.is_none());
    }

    #[test]
    fn test_csproj_package_reference_preserves_property_backed_version_override_metadata() {
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <CentralOverridesEnabled>true</CentralOverridesEnabled>
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
    <CentralPackageVersionOverrideEnabled>$(CentralOverridesEnabled)</CentralPackageVersionOverrideEnabled>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" VersionOverride="$(NewtonsoftJsonVersion)" />
  </ItemGroup>
</Project>"#;

        let mut temp_file = Builder::new().suffix(".csproj").tempfile().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = PackageReferenceProjectParser::extract_first_package(temp_file.path());
        let dep_extra = package_data.dependencies[0].extra_data.as_ref().unwrap();
        assert_eq!(
            dep_extra
                .get("version_override")
                .and_then(|value| value.as_str()),
            Some("$(NewtonsoftJsonVersion)")
        );
        assert_eq!(
            dep_extra
                .get("version_override_resolved")
                .and_then(|value| value.as_str()),
            Some("13.0.3")
        );
        let package_extra = package_data.extra_data.as_ref().unwrap();
        assert_eq!(
            package_extra
                .get("central_package_version_override_enabled")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_csproj_conditioned_property_group_does_not_enable_version_override() {
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup Condition="'$(TargetFramework)' == 'net8.0'">
    <CentralPackageVersionOverrideEnabled>true</CentralPackageVersionOverrideEnabled>
    <NewtonsoftJsonVersion>13.0.3</NewtonsoftJsonVersion>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" VersionOverride="$(NewtonsoftJsonVersion)" />
  </ItemGroup>
</Project>"#;

        let mut temp_file = Builder::new().suffix(".csproj").tempfile().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = PackageReferenceProjectParser::extract_first_package(temp_file.path());
        assert!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("central_package_version_override_enabled"))
                .is_none()
        );
        let dep_extra = package_data.dependencies[0].extra_data.as_ref().unwrap();
        assert_eq!(
            dep_extra
                .get("version_override")
                .and_then(|value| value.as_str()),
            Some("$(NewtonsoftJsonVersion)")
        );
        assert!(dep_extra.get("version_override_resolved").is_none());
    }

    #[test]
    fn test_directory_packages_props_malformed_returns_default() {
        let xml = r#"<Project><ItemGroup><PackageVersion Include="Newtonsoft.Json""#;

        let (_temp_dir, path) = write_directory_packages_props(xml);

        let package_data = CentralPackageManagementPropsParser::extract_first_package(&path);
        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::NugetDirectoryPackagesProps)
        );
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_csproj_package_reference_extracts_metadata_and_dependencies() {
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <PackageId>Contoso.Utility</PackageId>
    <Version>1.0.0</Version>
    <Description>Useful utilities</Description>
    <Authors>Jane Doe;John Doe</Authors>
    <PackageProjectUrl>https://example.test/contoso</PackageProjectUrl>
    <PackageLicenseExpression>MIT</PackageLicenseExpression>
    <RepositoryUrl>https://github.com/example/contoso</RepositoryUrl>
    <RepositoryType>git</RepositoryType>
    <RepositoryBranch>main</RepositoryBranch>
    <RepositoryCommit>abc123</RepositoryCommit>
    <PackageReadmeFile>README.md</PackageReadmeFile>
    <PackageIcon>icon.png</PackageIcon>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.1" />
    <PackageReference Include="Serilog">
      <Version>2.10.0</Version>
    </PackageReference>
  </ItemGroup>
</Project>"#;

        let mut temp_file = Builder::new().suffix(".csproj").tempfile().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let package_data = PackageReferenceProjectParser::extract_first_package(temp_file.path());
        let extra = package_data.extra_data.unwrap();

        assert_eq!(package_data.datasource_id, Some(DatasourceId::NugetCsproj));
        assert_eq!(package_data.name.as_deref(), Some("Contoso.Utility"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(package_data.parties[0].r#type.as_deref(), Some("person"));
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("git+https://github.com/example/contoso")
        );
        assert_eq!(extra["license_type"], "expression");
        assert_eq!(extra["repository_branch"], "main");
        assert_eq!(extra["repository_commit"], "abc123");
        assert_eq!(extra["readme_file"], "README.md");
        assert_eq!(extra["icon_file"], "icon.png");
        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:nuget/Newtonsoft.Json")
        );
        assert_eq!(
            package_data.dependencies[1]
                .extracted_requirement
                .as_deref(),
            Some("2.10.0")
        );
    }

    #[test]
    fn test_project_file_datasource_matches_extension() {
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><PackageId>Visual.Basic.Package</PackageId></PropertyGroup></Project>"#;

        let mut vbproj = Builder::new().suffix(".vbproj").tempfile().unwrap();
        vbproj.write_all(xml.as_bytes()).unwrap();
        let vb_package = PackageReferenceProjectParser::extract_first_package(vbproj.path());
        assert_eq!(vb_package.datasource_id, Some(DatasourceId::NugetVbproj));

        let mut fsproj = Builder::new().suffix(".fsproj").tempfile().unwrap();
        fsproj.write_all(xml.as_bytes()).unwrap();
        let fs_package = PackageReferenceProjectParser::extract_first_package(fsproj.path());
        assert_eq!(fs_package.datasource_id, Some(DatasourceId::NugetFsproj));
    }

    #[test]
    fn test_nupkg_extracts_embedded_license_file_contents() {
        let nuspec = r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Fizzler</id>
    <version>1.3.0</version>
    <license type="file">COPYING.txt</license>
    <licenseUrl>https://aka.ms/deprecateLicenseUrl</licenseUrl>
  </metadata>
</package>"#;
        let license_text = "GNU GENERAL PUBLIC LICENSE\nVersion 2\n";

        let temp_file = Builder::new().suffix(".nupkg").tempfile().unwrap();
        let file = temp_file.reopen().unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("Fizzler.nuspec", options).unwrap();
        zip.write_all(nuspec.as_bytes()).unwrap();
        zip.start_file("COPYING.txt", options).unwrap();
        zip.write_all(license_text.as_bytes()).unwrap();
        zip.finish().unwrap();

        let package_data = NupkgParser::extract_first_package(temp_file.path());

        assert_eq!(package_data.datasource_id, Some(DatasourceId::NugetNupkg));
        assert_eq!(package_data.name.as_deref(), Some("Fizzler"));
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some(license_text)
        );
        assert_eq!(
            package_data.extra_data.as_ref().unwrap()["license_file"],
            "COPYING.txt"
        );
    }

    #[test]
    fn test_packages_lock_malformed() {
        let json = r#"{"this is": "not valid json"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = PackagesLockParser::extract_first_package(path);

        assert_eq!(package_data.package_type, Some(PackageType::Nuget));
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_nuspec_description_building() {
        let xml = r#"<?xml version="1.0"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>TestPackage</id>
    <version>1.0.0</version>
    <title>Test Package Title</title>
    <summary>A short summary of the package</summary>
    <description>This is the full description of the package with more details.</description>
    <authors>Test Author</authors>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(package_data.name, Some("TestPackage".to_string()));
        assert!(package_data.description.is_some());

        let desc = package_data.description.unwrap();
        assert!(
            desc.contains("Test Package Title"),
            "Description should contain title"
        );
        assert!(
            desc.contains("A short summary"),
            "Description should contain summary"
        );
        assert!(
            desc.contains("full description"),
            "Description should contain description"
        );

        let lines: Vec<&str> = desc.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "Description should have 3 lines (title, summary, description)"
        );
        assert_eq!(lines[0], "Test Package Title");
        assert_eq!(lines[1], "A short summary of the package");
        assert_eq!(
            lines[2],
            "This is the full description of the package with more details."
        );
    }

    #[test]
    fn test_nuspec_description_summary_only() {
        let xml = r#"<?xml version="1.0"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>TestPackage</id>
    <version>1.0.0</version>
    <summary>Just a summary</summary>
    <authors>Test Author</authors>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(package_data.description, Some("Just a summary".to_string()));
    }

    #[test]
    fn test_nuspec_description_title_same_as_name() {
        let xml = r#"<?xml version="1.0"?>
<package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">
  <metadata>
    <id>TestPackage</id>
    <version>1.0.0</version>
    <title>TestPackage</title>
    <description>Description text</description>
    <authors>Test Author</authors>
  </metadata>
</package>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();
        let path = temp_file.path();

        let package_data = NuspecParser::extract_first_package(path);

        assert_eq!(
            package_data.description,
            Some("Description text".to_string())
        );
        assert!(
            !package_data
                .description
                .as_ref()
                .unwrap()
                .contains("TestPackage\n"),
            "Title should not be prepended when it matches the package name"
        );
    }
}
