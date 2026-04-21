// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{PackageParser, UvLockParser};

    #[test]
    fn test_is_match() {
        assert!(UvLockParser::is_match(&PathBuf::from(
            "/tmp/project/uv.lock"
        )));
        assert!(!UvLockParser::is_match(&PathBuf::from(
            "/tmp/project/poetry.lock"
        )));
    }

    #[test]
    fn test_extract_from_uv_lock_with_root_package_and_dev_group() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("uv.lock");
        fs::write(&file_path, sample_uv_lock()).expect("failed to write uv.lock");

        let package_data = UvLockParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.primary_language.as_deref(), Some("Python"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiUvLock));
        assert_eq!(package_data.name.as_deref(), Some("uv-demo"));
        assert_eq!(package_data.version.as_deref(), Some("0.1.0"));
        assert!(package_data.is_virtual);
        assert_eq!(package_data.purl.as_deref(), Some("pkg:pypi/uv-demo@0.1.0"));

        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests@2.32.5"))
            .expect("requests dependency should be present");
        assert_eq!(requests.extracted_requirement.as_deref(), Some(">=2.32.5"));
        assert_eq!(requests.scope, None);
        assert_eq!(requests.is_runtime, Some(true));
        assert_eq!(requests.is_optional, Some(false));
        assert_eq!(requests.is_direct, Some(true));
        assert!(requests.resolved_package.is_some());

        let pytest = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/pytest@9.0.2"))
            .expect("pytest dependency should be present");
        assert_eq!(pytest.extracted_requirement.as_deref(), Some(">=9.0.0"));
        assert_eq!(pytest.scope.as_deref(), Some("dev"));
        assert_eq!(pytest.is_runtime, Some(false));
        assert_eq!(pytest.is_optional, Some(false));
        assert_eq!(pytest.is_direct, Some(true));

        let certifi = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/certifi@2026.2.25"))
            .expect("certifi dependency should be present");
        assert_eq!(certifi.is_runtime, Some(true));
        assert_eq!(certifi.is_direct, Some(false));

        let colorama = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/colorama@0.4.6"))
            .expect("colorama dependency should be present");
        assert_eq!(colorama.is_runtime, Some(false));
        assert_eq!(colorama.is_direct, Some(false));
    }

    #[test]
    fn test_extract_from_uv_lock_editable_root_is_not_virtual() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("uv.lock");
        let content = r#"
version = 1
revision = 3

[[package]]
name = "requests"
version = "2.32.5"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "editable-root"
version = "0.2.0"
source = { editable = "." }
dependencies = [
    { name = "requests" },
]

[package.metadata]
requires-dist = [{ name = "requests", specifier = ">=2.32.5" }]
"#;
        fs::write(&file_path, content).expect("failed to write uv.lock");

        let package_data = UvLockParser::extract_first_package(&file_path);

        assert_eq!(package_data.name.as_deref(), Some("editable-root"));
        assert_eq!(package_data.version.as_deref(), Some("0.2.0"));
        assert!(!package_data.is_virtual);
    }

    #[test]
    fn test_extract_from_uv_lock_prefers_dot_root_when_multiple_local_packages_exist() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("uv.lock");
        let content = r#"
version = 1
revision = 3

[[package]]
name = "workspace-member"
version = "0.5.0"
source = { editable = "packages/member" }

[[package]]
name = "workspace-root"
version = "1.0.0"
source = { virtual = "." }
dependencies = [
    { name = "requests" },
]

[package.metadata]
requires-dist = [{ name = "requests", specifier = ">=2.32.5" }]

[[package]]
name = "requests"
version = "2.32.5"
source = { registry = "https://pypi.org/simple" }
"#;
        fs::write(&file_path, content).expect("failed to write uv.lock");

        let package_data = UvLockParser::extract_first_package(&file_path);

        assert_eq!(package_data.name.as_deref(), Some("workspace-root"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert!(package_data.is_virtual);
    }

    #[test]
    fn test_extract_from_uv_lock_merges_overlapping_direct_groups() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("uv.lock");
        let content = r#"
version = 1
revision = 3

[[package]]
name = "requests"
version = "2.32.5"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "uv-demo"
version = "0.1.0"
source = { virtual = "." }
dependencies = [
    { name = "requests" },
]

[package.dev-dependencies]
dev = [
    { name = "requests" },
]

[package.metadata]
requires-dist = [{ name = "requests", specifier = ">=2.32.5" }]

[package.metadata.requires-dev]
dev = [{ name = "requests", specifier = ">=2.31.0" }]
"#;
        fs::write(&file_path, content).expect("failed to write uv.lock");

        let package_data = UvLockParser::extract_first_package(&file_path);
        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests@2.32.5"))
            .expect("requests dependency should be present");

        assert_eq!(requests.is_direct, Some(true));
        assert_eq!(requests.is_runtime, Some(true));
        assert_eq!(requests.is_optional, Some(false));
        assert_eq!(requests.extracted_requirement.as_deref(), Some(">=2.32.5"));
    }

    #[test]
    fn test_extract_from_uv_lock_marks_transitive_optional_dependencies_non_runtime() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("uv.lock");
        let content = r#"
version = 1
revision = 3

[[package]]
name = "sphinx"
version = "7.4.0"
source = { registry = "https://pypi.org/simple" }
optional-dependencies = { docs = [{ name = "jinja2" }] }

[[package]]
name = "jinja2"
version = "3.1.4"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "uv-demo"
version = "0.1.0"
source = { virtual = "." }

[package.optional-dependencies]
docs = [
    { name = "sphinx" },
]

[package.metadata]
requires-dist = []

[package.metadata.optional-dependencies]
docs = [{ name = "sphinx", specifier = ">=7.4.0" }]
"#;
        fs::write(&file_path, content).expect("failed to write uv.lock");

        let package_data = UvLockParser::extract_first_package(&file_path);
        let jinja2 = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/jinja2@3.1.4"))
            .expect("jinja2 dependency should be present");

        assert_eq!(jinja2.is_direct, Some(false));
        assert_eq!(jinja2.is_runtime, Some(false));
        assert_eq!(jinja2.is_optional, Some(true));
    }

    #[test]
    fn test_extract_from_uv_lock_invalid_toml() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("uv.lock");
        fs::write(&file_path, "not valid toml = [").expect("failed to write uv.lock");

        let package_data = UvLockParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiUvLock));
        assert!(package_data.name.is_none());
    }

    fn sample_uv_lock() -> &'static str {
        r#"
version = 1
revision = 3
requires-python = ">=3.12"
resolution-markers = ["python_full_version >= '3.12'"]

[[package]]
name = "requests"
version = "2.32.5"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "certifi" },
]
sdist = { url = "https://files.pythonhosted.org/packages/source/r/requests/requests-2.32.5.tar.gz", hash = "sha256:requestssdist" }
wheels = [
    { url = "https://files.pythonhosted.org/packages/requests-2.32.5-py3-none-any.whl", hash = "sha256:requestswheel" },
]

[[package]]
name = "pytest"
version = "9.0.2"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "colorama", marker = "sys_platform == 'win32'" },
]
wheels = [
    { url = "https://files.pythonhosted.org/packages/pytest-9.0.2-py3-none-any.whl", hash = "sha256:pytestwheel" },
]

[[package]]
name = "certifi"
version = "2026.2.25"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://files.pythonhosted.org/packages/certifi-2026.2.25-py3-none-any.whl", hash = "sha256:certifiwheel" },
]

[[package]]
name = "colorama"
version = "0.4.6"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://files.pythonhosted.org/packages/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:coloramawheel" },
]

[[package]]
name = "uv-demo"
version = "0.1.0"
source = { virtual = "." }
dependencies = [
    { name = "requests" },
]

[package.dev-dependencies]
dev = [
    { name = "pytest" },
]

[package.metadata]
requires-dist = [{ name = "requests", specifier = ">=2.32.5" }]

[package.metadata.requires-dev]
dev = [{ name = "pytest", specifier = ">=9.0.0" }]
"#
    }
}
