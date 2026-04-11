mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{PackageParser, PixiLockParser, PixiTomlParser};

    fn create_temp_file(file_name: &str, content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("Failed to write temp file");
        (temp_dir, file_path)
    }

    #[test]
    fn test_pixi_toml_is_match() {
        assert!(PixiTomlParser::is_match(&PathBuf::from("pixi.toml")));
        assert!(!PixiTomlParser::is_match(&PathBuf::from("pyproject.toml")));
        assert!(!PixiTomlParser::is_match(&PathBuf::from("pixi.lock")));
    }

    #[test]
    fn test_pixi_lock_is_match() {
        assert!(PixiLockParser::is_match(&PathBuf::from("pixi.lock")));
        assert!(!PixiLockParser::is_match(&PathBuf::from("pixi.toml")));
        assert!(!PixiLockParser::is_match(&PathBuf::from("poetry.lock")));
    }

    #[test]
    fn test_extract_from_pixi_toml_workspace_metadata_and_dependencies() {
        let content = r#"
[workspace]
name = "pixi-demo"
version = "1.2.3"
authors = ["Jane Doe <jane@example.com>"]
description = "Example Pixi workspace"
license = "MIT"
license-file = "LICENSE"
readme = "README.md"
homepage = "https://example.com/pixi-demo"
repository = "https://github.com/example/pixi-demo"
documentation = "https://docs.example.com/pixi-demo"
channels = ["conda-forge", "https://repo.prefix.dev/example"]
platforms = ["linux-64", "osx-arm64"]
requires-pixi = ">=0.40"
exclude-newer = "2025-01-01"

[dependencies]
python = "3.12.*"
numpy = { version = "2.1.0", channel = "conda-forge" }
exactpkg = "=1.5.0"

[pypi-dependencies]
requests = ">=2.32,<3"
localpkg = { path = ".", editable = true }

[feature.docs.dependencies]
sphinx = "8.2.3"

[feature.test.pypi-dependencies]
pytest = { version = "8.4.0", extras = ["dev"] }

[environments]
default = ["docs"]
test = { features = ["test"], solve-group = "default" }

[tasks]
test = "pytest"

[pypi-options]
index-url = "https://pypi.org/simple"
        "#;

        let (_temp_dir, path) = create_temp_file("pixi.toml", content);
        let package_data = PixiTomlParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Pixi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PixiToml));
        assert_eq!(package_data.primary_language.as_deref(), Some("TOML"));
        assert_eq!(package_data.name.as_deref(), Some("pixi-demo"));
        assert_eq!(package_data.version.as_deref(), Some("1.2.3"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:pixi/pixi-demo@1.2.3")
        );
        assert_eq!(
            package_data.description.as_deref(),
            Some("Example Pixi workspace")
        );
        assert_eq!(
            package_data.homepage_url.as_deref(),
            Some("https://example.com/pixi-demo")
        );
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("https://github.com/example/pixi-demo")
        );
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT")
        );
        assert_eq!(
            package_data
                .file_references
                .iter()
                .map(|reference| reference.path.as_str())
                .collect::<Vec<_>>(),
            vec!["LICENSE", "README.md"]
        );
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(package_data.parties[0].name.as_deref(), Some("Jane Doe"));
        assert_eq!(
            package_data.parties[0].email.as_deref(),
            Some("jane@example.com")
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing extra_data");
        assert!(extra_data.get("channels").is_some());
        assert!(extra_data.get("platforms").is_some());
        assert!(extra_data.get("environments").is_some());
        assert!(extra_data.get("tasks").is_some());
        assert!(extra_data.get("pypi_options").is_some());
        assert_eq!(
            extra_data
                .get("requires_pixi")
                .and_then(|value| value.as_str()),
            Some(">=0.40")
        );

        assert_eq!(package_data.dependencies.len(), 7);
        let numpy = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/numpy@2.1.0"))
            .expect("numpy dependency missing");
        assert_eq!(numpy.is_pinned, Some(true));

        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests"))
            .expect("requests dependency missing");
        assert_eq!(requests.is_pinned, Some(false));

        let exactpkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/exactpkg@1.5.0"))
            .expect("exactpkg dependency missing");
        assert_eq!(exactpkg.is_pinned, Some(true));

        let localpkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/localpkg"))
            .expect("localpkg dependency missing");
        assert_eq!(localpkg.is_pinned, Some(false));
        assert_eq!(
            localpkg
                .extra_data
                .as_ref()
                .and_then(|value| value.get("editable"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );

        let sphinx = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/sphinx@8.2.3"))
            .expect("sphinx dependency missing");
        assert_eq!(sphinx.scope.as_deref(), Some("docs"));
        assert_eq!(sphinx.is_optional, Some(true));

        let pytest = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/pytest@8.4.0"))
            .expect("pytest dependency missing");
        assert_eq!(pytest.scope.as_deref(), Some("test"));
        assert_eq!(pytest.is_optional, Some(true));
    }

    #[test]
    fn test_extract_from_pixi_toml_project_fallback() {
        let content = r#"
[project]
name = "project-fallback"
version = "0.1.0"
description = "Project table fallback"

[dependencies]
python = "3.11.*"
        "#;

        let (_temp_dir, path) = create_temp_file("pixi.toml", content);
        let package_data = PixiTomlParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("project-fallback"));
        assert_eq!(package_data.version.as_deref(), Some("0.1.0"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:pixi/project-fallback@0.1.0")
        );
    }

    #[test]
    fn test_extract_from_pixi_toml_preserves_license_file_reference() {
        let content = r#"
[workspace]
name = "pixi-demo"
version = "1.2.3"
license = "MIT"
license-file = "LICENSE"
"#;

        let (_temp_dir, path) = create_temp_file("pixi.toml", content);
        let package_data = PixiTomlParser::extract_first_package(&path);

        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(
            package_data.license_detections[0].matches[0]
                .referenced_filenames
                .as_ref(),
            Some(&vec!["LICENSE".to_string()])
        );
    }

    #[test]
    fn test_extract_from_pixi_lock_v6() {
        let content = r#"
version = 6

[environments.default]
channels = [{ url = "https://conda.anaconda.org/conda-forge/" }]
indexes = ["https://pypi.org/simple"]

[environments.default.packages]
linux-64 = [
  { conda = "https://conda.anaconda.org/conda-forge/linux-64/python-3.12.7-h2628c8c_0_cpython.conda" },
  { pypi = "https://files.pythonhosted.org/packages/example/requests-2.32.5-py3-none-any.whl" },
]

[environments.test.packages]
osx-arm64 = [
  { pypi = "https://files.pythonhosted.org/packages/example/requests-2.32.5-py3-none-any.whl" },
]

[[packages]]
conda = "https://conda.anaconda.org/conda-forge/linux-64/python-3.12.7-h2628c8c_0_cpython.conda"
version = "3.12.7"
sha256 = "conda-hash"
depends = ["openssl >=3.0"]

[[packages]]
pypi = "https://files.pythonhosted.org/packages/example/requests-2.32.5-py3-none-any.whl"
name = "requests"
version = "2.32.5"
requires_python = ">=3.9"
requires_dist = ["urllib3>=2"]
sha256 = "pypi-hash"
        "#;

        let (_temp_dir, path) = create_temp_file("pixi.lock", content);
        let package_data = PixiLockParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Pixi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PixiLock));
        assert_eq!(package_data.primary_language.as_deref(), Some("TOML"));
        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_version"))
                .and_then(|value| value.as_i64()),
            Some(6)
        );
        assert!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_environments"))
                .is_some()
        );

        let python = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/python@3.12.7"))
            .expect("python conda dep missing");
        assert_eq!(python.is_pinned, Some(true));
        assert_eq!(python.is_direct, None);
        assert!(
            python
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_references"))
                .is_some()
        );

        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests@2.32.5"))
            .expect("requests pypi dep missing");
        assert_eq!(requests.is_pinned, Some(true));
        assert_eq!(requests.is_direct, None);
        assert_eq!(
            requests
                .extra_data
                .as_ref()
                .and_then(|value| value.get("sha256"))
                .and_then(|value| value.as_str()),
            Some("pypi-hash")
        );
        assert_eq!(
            requests
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_references"))
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn test_extract_from_pixi_lock_yaml_v6() {
        let content = r#"
version: 6

environments:
  default:
    channels:
      - url: https://conda.anaconda.org/conda-forge/
    indexes:
      - https://pypi.org/simple
    packages:
      linux-64:
        - conda: https://conda.anaconda.org/conda-forge/linux-64/python-3.12.7-h2628c8c_0_cpython.conda
        - pypi: https://files.pythonhosted.org/packages/example/requests-2.32.5-py3-none-any.whl
  test:
    packages:
      osx-arm64:
        - pypi: https://files.pythonhosted.org/packages/example/requests-2.32.5-py3-none-any.whl

packages:
  - conda: https://conda.anaconda.org/conda-forge/linux-64/python-3.12.7-h2628c8c_0_cpython.conda
    version: 3.12.7
    sha256: conda-hash
    depends:
      - openssl >=3.0
  - pypi: https://files.pythonhosted.org/packages/example/requests-2.32.5-py3-none-any.whl
    name: requests
    version: 2.32.5
    requires_python: '>=3.9'
    requires_dist:
      - urllib3>=2
    sha256: pypi-hash
        "#;

        let (_temp_dir, path) = create_temp_file("pixi.lock", content);
        let package_data = PixiLockParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Pixi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PixiLock));
        assert_eq!(package_data.primary_language.as_deref(), Some("YAML"));
        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_version"))
                .and_then(|value| value.as_i64()),
            Some(6)
        );

        let python = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/python@3.12.7"))
            .expect("python conda dep missing");
        assert_eq!(python.is_pinned, Some(true));
        assert_eq!(python.is_direct, None);
        assert!(
            python
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_references"))
                .is_some()
        );

        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests@2.32.5"))
            .expect("requests pypi dep missing");
        assert_eq!(requests.is_pinned, Some(true));
        assert_eq!(requests.is_direct, None);
        assert_eq!(
            requests
                .extra_data
                .as_ref()
                .and_then(|value| value.get("sha256"))
                .and_then(|value| value.as_str()),
            Some("pypi-hash")
        );
        assert_eq!(
            requests
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_references"))
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn test_extract_from_pixi_lock_v4() {
        let content = r#"
version = 4

[environments.default]
channels = [{ url = "https://conda.anaconda.org/conda-forge/" }]

[environments.default.packages]
win-64 = [
  { conda = "https://conda.anaconda.org/conda-forge/win-64/python-3.12.3-h2628c8c_0_cpython.conda" },
  { pypi = "./foo" },
]

[[packages]]
kind = "conda"
name = "python"
version = "3.12.3"
url = "https://conda.anaconda.org/conda-forge/win-64/python-3.12.3-h2628c8c_0_cpython.conda"
sha256 = "conda-v4-hash"

[[packages]]
kind = "pypi"
name = "foo"
version = "0.1.0"
path = "./foo"
editable = true
sha256 = "pypi-v4-hash"
        "#;

        let (_temp_dir, path) = create_temp_file("pixi.lock", content);
        let package_data = PixiLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 2);
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:conda/python@3.12.3"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:pypi/foo@0.1.0"))
        );
        assert!(
            package_data
                .dependencies
                .iter()
                .all(|dep| dep.is_direct.is_none())
        );
    }

    #[test]
    fn test_extract_from_pixi_lock_unsupported_version_returns_default_lock_package() {
        let content = r#"
version = 99

[environments.default]
channels = ["conda-forge"]
        "#;

        let (_temp_dir, path) = create_temp_file("pixi.lock", content);
        let package_data = PixiLockParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Pixi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PixiLock));
        assert!(package_data.dependencies.is_empty());
        assert_eq!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("lock_version"))
                .and_then(|value| value.as_i64()),
            Some(99)
        );
    }

    #[test]
    fn test_graceful_error_handling_for_invalid_toml() {
        let (_temp_dir, path) = create_temp_file("pixi.toml", "[workspace");
        let package_data = PixiTomlParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Pixi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PixiToml));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }
}
