// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::nuget::{
        CentralPackageManagementPropsParser, DirectoryBuildPropsParser, DotNetDepsJsonParser,
        NupkgParser, NuspecParser, PackageReferenceProjectParser, PackagesConfigParser,
        PackagesLockParser, ProjectJsonParser, ProjectLockJsonParser,
    };
    use std::path::PathBuf;

    #[test]
    fn test_golden_bootstrap() {
        let test_file = PathBuf::from("testdata/nuget-golden/bootstrap/bootstrap.nuspec");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/bootstrap/bootstrap.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_castle_core() {
        let test_file = PathBuf::from("testdata/nuget-golden/castle-core/Castle.Core.nuspec");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/castle-core/Castle.Core.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_entity_framework() {
        let test_file =
            PathBuf::from("testdata/nuget-golden/entity-framework/EntityFramework.nuspec");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/entity-framework/EntityFramework.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_jquery_ui() {
        let test_file = PathBuf::from("testdata/nuget-golden/jquery-ui/jQuery.UI.Combined.nuspec");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/jquery-ui/jQuery.UI.Combined.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_aspnet_mvc() {
        let test_file =
            PathBuf::from("testdata/nuget-golden/aspnet-mvc/Microsoft.AspNet.Mvc.nuspec");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/aspnet-mvc/Microsoft.AspNet.Mvc.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_net_http() {
        let test_file = PathBuf::from("testdata/nuget-golden/net-http/Microsoft.Net.Http.nuspec");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/net-http/Microsoft.Net.Http.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_fizzler() {
        let test_file = PathBuf::from("testdata/nuget-golden/fizzler/Fizzler.nuspec");
        let expected_file = PathBuf::from("testdata/nuget-golden/fizzler/Fizzler.nuspec.expected");

        let package_data = NuspecParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_project_json() {
        let test_file = PathBuf::from("testdata/nuget-golden/legacy-project-json/project.json");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/legacy-project-json/project.json.expected");

        let package_data = ProjectJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_package_reference_project() {
        let test_file =
            PathBuf::from("testdata/nuget-golden/package-reference/Contoso.Utility.csproj");
        let expected_file = PathBuf::from(
            "testdata/nuget-golden/package-reference/Contoso.Utility.csproj.expected",
        );

        let package_data = PackageReferenceProjectParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_dotnet_deps_json() {
        let test_file = PathBuf::from("testdata/nuget-golden/deps-json/ExampleApp.deps.json");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/deps-json/ExampleApp.deps.json.expected.json");

        let package_data = DotNetDepsJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_nupkg() {
        let test_file = PathBuf::from("testdata/nuget-golden/nupkg/Fizzler.1.3.0.nupkg");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/nupkg/Fizzler.1.3.0.nupkg.expected.json");

        let package_data = NupkgParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_packages_config() {
        let test_file = PathBuf::from("testdata/nuget-golden/packages-config/packages.config");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/packages-config/packages.config.expected.json");

        let package_data = PackagesConfigParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_packages_lock() {
        let test_file = PathBuf::from("testdata/nuget-golden/packages-lock/packages.lock.json");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/packages-lock/packages.lock.json.expected.json");

        let package_data = PackagesLockParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_project_lock_json() {
        let test_file = PathBuf::from("testdata/nuget-golden/project-lock/project.lock.json");
        let expected_file =
            PathBuf::from("testdata/nuget-golden/project-lock/project.lock.json.expected.json");

        let package_data = ProjectLockJsonParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_directory_packages_props() {
        let test_file = PathBuf::from(
            "testdata/nuget-golden/central-package-management/Directory.Packages.props",
        );
        let expected_file = PathBuf::from(
            "testdata/nuget-golden/central-package-management/Directory.Packages.props.expected.json",
        );

        let package_data = CentralPackageManagementPropsParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }

    #[test]
    fn test_golden_directory_build_props() {
        let test_file =
            PathBuf::from("testdata/nuget-golden/directory-build-props/src/Directory.Build.props");
        let expected_file = PathBuf::from(
            "testdata/nuget-golden/directory-build-props/src/Directory.Build.props.expected.json",
        );

        let package_data = DirectoryBuildPropsParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed: {}", e),
        }
    }
}
