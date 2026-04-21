// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{GoWorkParser, PackageParser};

    #[test]
    fn test_is_match_go_work() {
        assert!(GoWorkParser::is_match(Path::new("go.work")));
        assert!(!GoWorkParser::is_match(Path::new("go.mod")));
    }

    #[test]
    fn test_extract_go_work_with_use_block_and_toolchain() {
        let package_data = GoWorkParser::extract_first_package(&PathBuf::from(
            "testdata/go-golden/gowork-sample1/go.work",
        ));

        assert_eq!(package_data.package_type, Some(PackageType::Golang));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::GoWork));
        assert_eq!(package_data.primary_language.as_deref(), Some("Go"));
        assert!(package_data.name.is_none());
        assert!(package_data.purl.is_none());

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        assert_eq!(
            extra_data
                .get("go_version")
                .and_then(|value| value.as_str()),
            Some("1.21")
        );
        assert_eq!(
            extra_data.get("toolchain").and_then(|value| value.as_str()),
            Some("go1.21.5")
        );
        let use_paths = extra_data
            .get("use_paths")
            .and_then(|value| value.as_array())
            .expect("use_paths should exist");
        assert_eq!(use_paths.len(), 2);

        assert_eq!(package_data.dependencies.len(), 3);
        let mymodule = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:golang/example.com/mymodule"))
            .expect("workspace module dependency missing");
        assert_eq!(mymodule.scope.as_deref(), Some("use"));
        assert_eq!(mymodule.is_direct, Some(true));
        assert_eq!(mymodule.is_pinned, Some(false));
        assert_eq!(
            mymodule
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("workspace_path"))
                .and_then(|value| value.as_str()),
            Some("./mymodule")
        );

        let formatter = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:golang/example.com/formatter"))
            .expect("formatter workspace dependency missing");
        assert_eq!(formatter.scope.as_deref(), Some("use"));

        let replace_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("replace"))
            .expect("replace dependency missing");
        let replace_extra = replace_dep
            .extra_data
            .as_ref()
            .expect("replace extra data missing");
        assert_eq!(
            replace_extra
                .get("replace_old")
                .and_then(|value| value.as_str()),
            Some("golang.org/x/net")
        );
        assert_eq!(
            replace_extra
                .get("replace_new")
                .and_then(|value| value.as_str()),
            Some("../forks/net")
        );
    }

    #[test]
    fn test_extract_go_work_inline_use_without_toolchain() {
        let package_data = GoWorkParser::extract_first_package(&PathBuf::from(
            "testdata/go-golden/gowork-sample2/go.work",
        ));

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:golang/example.com/myapp"));
        assert_eq!(dep.scope.as_deref(), Some("use"));
    }

    #[test]
    fn test_extract_go_work_supports_quoted_use_and_versioned_replace() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("my app")).unwrap();
        fs::write(
            root.join("my app/go.mod"),
            "module example.com/myapp\n\ngo 1.21\n",
        )
        .unwrap();
        let file_path = root.join("go.work");
        let content = r#"
go 1.21

use "./my app"

replace (
	example.com/old v1.2.3 => example.com/new v1.4.5
	`example.com/raw` => `../raw-local`
)
"#;
        fs::write(&file_path, content).unwrap();

        let package_data = GoWorkParser::extract_first_package(&file_path);
        assert_eq!(package_data.dependencies.len(), 3);

        let workspace_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("use"))
            .unwrap();
        assert_eq!(
            workspace_dep.purl.as_deref(),
            Some("pkg:golang/example.com/myapp")
        );
        assert_eq!(
            workspace_dep.extracted_requirement.as_deref(),
            Some("./my app")
        );

        let versioned_replace = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.extra_data
                    .as_ref()
                    .and_then(|extra| extra.get("replace_old"))
                    .and_then(|value| value.as_str())
                    == Some("example.com/old")
            })
            .unwrap();
        assert_eq!(
            versioned_replace.extracted_requirement.as_deref(),
            Some("v1.4.5")
        );
        assert_eq!(
            versioned_replace.purl.as_deref(),
            Some("pkg:golang/example.com/new@v1.4.5")
        );

        let local_replace = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.extra_data
                    .as_ref()
                    .and_then(|extra| extra.get("replace_old"))
                    .and_then(|value| value.as_str())
                    == Some("example.com/raw")
            })
            .unwrap();
        assert!(local_replace.purl.is_none());
        assert_eq!(
            local_replace
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("replace_local_path"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_extract_go_work_tracks_unresolved_workspace_members() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("go.work");
        let content = r#"
go 1.21

use (
	./missing-module
	./broken-module
)
"#;
        fs::create_dir_all(temp_dir.path().join("broken-module")).unwrap();
        fs::write(temp_dir.path().join("broken-module/go.mod"), "not a go.mod").unwrap();
        fs::write(&file_path, content).unwrap();

        let package_data = GoWorkParser::extract_first_package(&file_path);
        assert!(package_data.dependencies.is_empty());

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        let unresolved = extra_data
            .get("unresolved_use_paths")
            .and_then(|value| value.as_array())
            .expect("unresolved_use_paths should exist");
        assert_eq!(unresolved.len(), 2);
    }

    #[test]
    fn test_extract_go_work_invalid_file_returns_default() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("go.work");
        fs::write(&file_path, "not a valid workspace").unwrap();

        let package_data = GoWorkParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Golang));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::GoWork));
        assert!(package_data.dependencies.is_empty());
    }
}
