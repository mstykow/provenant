// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{PackageParser, YarnPnpParser};

    fn create_temp_pnp(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("tempdir");
        let path = temp_dir.path().join(".pnp.cjs");
        fs::write(&path, content).expect("write .pnp.cjs");
        (temp_dir, path)
    }

    #[test]
    fn test_is_match() {
        assert!(YarnPnpParser::is_match(PathBuf::from(".pnp.cjs").as_path()));
        assert!(!YarnPnpParser::is_match(
            PathBuf::from("yarn.lock").as_path()
        ));
    }

    #[test]
    fn test_extract_dependencies_from_raw_runtime_state() {
        let (_temp_dir, path) = create_temp_pnp(
            "const RAW_RUNTIME_STATE = {\n  \"packageRegistryData\": [\n    [null, {\n      \"packageDependencies\": [[\"left-pad\", \"npm:1.3.0\"], [\"@scope/demo\", \"npm:2.0.0\"]]\n    }],\n    [\"left-pad@npm:1.3.0\", {\n      \"packageDependencies\": []\n    }],\n    [\"@scope/demo@npm:2.0.0\", {\n      \"packageDependencies\": []\n    }]\n  ]\n};\n",
        );

        let package = YarnPnpParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Npm));
        assert_eq!(package.datasource_id, Some(DatasourceId::YarnPnpCjs));
        assert_eq!(package.dependencies.len(), 2);
        assert!(package.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:npm/left-pad@1.3.0") && dep.is_direct == Some(true)
        }));
        assert!(package.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:npm/%40scope/demo@2.0.0")
                && dep.is_direct == Some(true)
        }));
    }

    #[test]
    fn test_invalid_pnp_returns_default_package() {
        let (_temp_dir, path) = create_temp_pnp("module.exports = {};\n");
        let package = YarnPnpParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Npm));
        assert_eq!(package.datasource_id, Some(DatasourceId::YarnPnpCjs));
        assert!(package.dependencies.is_empty());
    }
}
