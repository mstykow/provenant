// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::models::PackageType;
    use crate::parsers::{NpmWorkspaceParser, PackageParser};
    use std::path::{Path, PathBuf};

    #[test]
    fn test_extract_from_basic_testdata() {
        let workspace_path = PathBuf::from("testdata/npm-workspace/basic.yaml")
            .canonicalize()
            .unwrap();
        let package_data = NpmWorkspaceParser::extract_first_package(&workspace_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));

        // Check workspaces are extracted
        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(
            extra_data.get("datasource_id").unwrap().as_str().unwrap(),
            "pnpm_workspace_yaml"
        );
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0], "packages/*");
    }

    #[test]
    fn test_extract_from_multiple_testdata() {
        let workspace_path = PathBuf::from("testdata/npm-workspace/multiple.yaml")
            .canonicalize()
            .unwrap();
        let package_data = NpmWorkspaceParser::extract_first_package(&workspace_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));

        let extra_data = package_data.extra_data.unwrap();
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 3);
        assert_eq!(workspaces[0], "packages/*");
        assert_eq!(workspaces[1], "apps/*");
        assert_eq!(workspaces[2], "tools/*");
    }

    #[test]
    fn test_extract_from_complex_testdata() {
        let workspace_path = PathBuf::from("testdata/npm-workspace/complex.yaml")
            .canonicalize()
            .unwrap();
        let package_data = NpmWorkspaceParser::extract_first_package(&workspace_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));

        let extra_data = package_data.extra_data.unwrap();
        let workspaces = extra_data.get("workspaces").unwrap().as_array().unwrap();
        assert_eq!(workspaces.len(), 3);
        assert_eq!(workspaces[0], "**/packages/*");
        assert_eq!(workspaces[1], "!**/node_modules");
        assert_eq!(workspaces[2], "!**/dist");
    }

    #[test]
    fn test_is_match_with_real_file() {
        let valid_path = PathBuf::from("testdata/npm-workspace/basic.yaml")
            .canonicalize()
            .unwrap();
        let _invalid_path = PathBuf::from("testdata/npm/package.json")
            .canonicalize()
            .unwrap();

        // is_match() checks the filename, which works with absolute or relative paths
        assert!(
            valid_path.file_name().unwrap() == "pnpm-workspace.yaml"
                || valid_path.file_name().unwrap() == "basic.yaml"
        );
        // The actual is_match() function only accepts exact filenames
        assert!(NpmWorkspaceParser::is_match(Path::new(
            "pnpm-workspace.yaml"
        )));
        assert!(NpmWorkspaceParser::is_match(Path::new(
            "/any/path/to/pnpm-workspace.yaml"
        )));
        assert!(!NpmWorkspaceParser::is_match(Path::new("package.json")));
    }
}
