// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use crate::models::{DatasourceId, Md5Digest, PackageType, Sha256Digest};
    use crate::parsers::{PackageParser, PylockTomlParser};

    #[test]
    fn test_is_match() {
        assert!(PylockTomlParser::is_match(&PathBuf::from(
            "/tmp/project/pylock.toml"
        )));
        assert!(PylockTomlParser::is_match(&PathBuf::from(
            "/tmp/project/pylock.spam.toml"
        )));
        assert!(!PylockTomlParser::is_match(&PathBuf::from(
            "/tmp/project/pylock.spam.web.toml"
        )));
        assert!(!PylockTomlParser::is_match(&PathBuf::from(
            "/tmp/project/poetry.lock"
        )));
    }

    #[test]
    fn test_extract_from_pylock_toml_with_groups_and_local_package() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        fs::write(&file_path, sample_pylock_toml()).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.primary_language.as_deref(), Some("Python"));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert_eq!(
            extra_data
                .get("lock_version")
                .and_then(|value| value.as_str()),
            Some("1.0")
        );
        assert_eq!(
            extra_data
                .get("created_by")
                .and_then(|value| value.as_str()),
            Some("mousebender")
        );
        assert_eq!(
            extra_data
                .get("requires_python")
                .and_then(|value| value.as_str()),
            Some(">=3.12")
        );

        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests@2.32.3"))
            .expect("requests dependency should be present");
        assert_eq!(requests.is_direct, Some(true));
        assert_eq!(requests.is_runtime, Some(true));
        assert_eq!(requests.is_optional, Some(false));
        assert_eq!(requests.is_pinned, Some(true));
        let requests_resolved = requests
            .resolved_package
            .as_ref()
            .expect("requests should have resolved package");
        assert_eq!(
            requests_resolved.sha256,
            Some(
                Sha256Digest::from_hex(
                    "aabb0000000000000000000000000000000000000000000000000000000000aa"
                )
                .unwrap()
            )
        );
        assert!(requests_resolved.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:pypi/urllib3@2.2.3") && dep.is_direct == Some(true)
        }));

        let pytest = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/pytest@8.3.5"))
            .expect("pytest dependency should be present");
        assert_eq!(pytest.is_direct, Some(true));
        assert_eq!(pytest.is_runtime, Some(false));
        assert_eq!(pytest.is_optional, Some(false));
        assert_eq!(pytest.scope.as_deref(), Some("dev"));

        let pluggy = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/pluggy@1.5.0"))
            .expect("pluggy dependency should be present");
        assert_eq!(pluggy.is_direct, Some(false));
        assert_eq!(pluggy.is_runtime, Some(false));
        assert_eq!(pluggy.is_optional, Some(false));
        assert_eq!(pluggy.scope.as_deref(), Some("dev"));

        let sphinx = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/sphinx@8.2.3"))
            .expect("sphinx dependency should be present");
        assert_eq!(sphinx.is_direct, Some(true));
        assert_eq!(sphinx.is_runtime, Some(false));
        assert_eq!(sphinx.is_optional, Some(true));
        assert_eq!(sphinx.scope.as_deref(), Some("docs"));

        let jinja2 = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/jinja2@3.1.6"))
            .expect("jinja2 dependency should be present");
        assert_eq!(jinja2.is_direct, Some(false));
        assert_eq!(jinja2.is_runtime, Some(false));
        assert_eq!(jinja2.is_optional, Some(true));
        assert_eq!(jinja2.scope.as_deref(), Some("docs"));

        let local_editable = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/local-editable"))
            .expect("local-editable dependency should be present");
        assert_eq!(local_editable.is_direct, Some(true));
        assert_eq!(local_editable.is_runtime, Some(true));
        assert_eq!(local_editable.is_optional, Some(false));
        assert_eq!(local_editable.is_pinned, Some(false));
    }

    #[test]
    fn test_extract_from_pylock_toml_invalid_toml() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        fs::write(&file_path, "not valid toml = [").expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_from_pylock_toml_missing_lock_version_returns_default() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
created-by = "mousebender"

[[packages]]
name = "requests"
version = "2.32.3"
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.dependencies.is_empty());
        assert!(package_data.extra_data.is_none());
    }

    #[test]
    fn test_extract_from_pylock_toml_unsupported_lock_version_returns_default() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "2.0"
created-by = "mousebender"

[[packages]]
name = "requests"
version = "2.32.3"
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.dependencies.is_empty());
        assert!(package_data.extra_data.is_none());
    }

    #[test]
    fn test_extract_from_pylock_toml_missing_created_by_returns_default() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "1.0"

[[packages]]
name = "requests"
version = "2.32.3"
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.dependencies.is_empty());
        assert!(package_data.extra_data.is_none());
    }

    #[test]
    fn test_extract_from_pylock_toml_missing_packages_returns_default() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "1.0"
created-by = "mousebender"
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.dependencies.is_empty());
        assert!(package_data.extra_data.is_none());
    }

    #[test]
    fn test_extract_from_pylock_toml_empty_packages_array_returns_default() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "1.0"
created-by = "mousebender"
packages = []
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPylockToml)
        );
        assert!(package_data.dependencies.is_empty());
        assert!(package_data.extra_data.is_none());
    }

    #[test]
    fn test_extract_from_pylock_toml_shared_root_dependency_is_classified_conservatively() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "1.0"
created-by = "mousebender"

[[packages]]
name = "requests"
version = "2.32.3"

[[packages.wheels]]
name = "requests-2.32.3-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/requests-2.32.3-py3-none-any.whl"
hashes = { sha256 = "reqwheelhash" }

[[packages]]
name = "local-app"

[packages.directory]
path = "."

[[packages.dependencies]]
name = "requests"
version = "2.32.3"
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);
        let requests = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/requests@2.32.3"))
            .expect("requests dependency should be present");

        assert_eq!(requests.is_direct, Some(false));
        assert_eq!(requests.is_runtime, Some(true));
    }

    #[test]
    fn test_extract_from_pylock_toml_skips_ambiguous_dependency_reference() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "1.0"
created-by = "mousebender"

[[packages]]
name = "spam"
version = "1.0.0"

[[packages.wheels]]
name = "spam-1.0.0-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/spam-1.0.0-py3-none-any.whl"
hashes = { sha256 = "spam1hash" }

[[packages]]
name = "spam"
version = "2.0.0"

[[packages.wheels]]
name = "spam-2.0.0-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/spam-2.0.0-py3-none-any.whl"
hashes = { sha256 = "spam2hash" }

[[packages]]
name = "root"
version = "0.1.0"

[[packages.wheels]]
name = "root-0.1.0-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/root-0.1.0-py3-none-any.whl"
hashes = { sha256 = "roothash" }

[[packages.dependencies]]
name = "spam"
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);
        let root = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/root@0.1.0"))
            .expect("root dependency should be present");
        let resolved = root
            .resolved_package
            .as_ref()
            .expect("root should have resolved package");

        assert!(resolved.dependencies.is_empty());
    }

    #[test]
    fn test_extract_from_pylock_toml_preserves_provenance_sources() {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("pylock.toml");
        let content = r#"
lock-version = "1.0"
created-by = "mousebender"

[[packages]]
name = "gitpkg"

[packages.vcs]
type = "git"
url = "https://github.com/example/gitpkg.git"
commit-id = "abc123"
requested-revision = "main"

[[packages]]
name = "archivepkg"
version = "1.0.0"

[packages.archive]
url = "https://example.com/archivepkg-1.0.0.zip"
size = 1234
hashes = { sha256 = "aaa10000000000000000000000000000000000000000000000000000000000a1", md5 = "bb11bb22bb33bb44bb55bb66bb77bb88" }

[[packages]]
name = "sdistpkg"
version = "2.0.0"

[packages.sdist]
name = "sdistpkg-2.0.0.tar.gz"
url = "https://files.pythonhosted.org/packages/sdistpkg-2.0.0.tar.gz"
hashes = { sha256 = "aaa20000000000000000000000000000000000000000000000000000000000a2" }

[[packages]]
name = "wheelpkg"
version = "3.0.0"

[[packages.wheels]]
name = "wheelpkg-3.0.0-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/wheelpkg-3.0.0-py3-none-any.whl"
hashes = { sha256 = "aaa30000000000000000000000000000000000000000000000000000000000a3" }

[[packages.wheels]]
name = "wheelpkg-3.0.0-py3-none-any-manylinux.whl"
url = "https://files.pythonhosted.org/packages/wheelpkg-3.0.0-py3-none-any-manylinux.whl"
hashes = { sha256 = "aaa40000000000000000000000000000000000000000000000000000000000a4" }

[[packages]]
name = "dirpkg"

[packages.directory]
path = "./dirpkg"
editable = true
"#;
        fs::write(&file_path, content).expect("failed to write pylock.toml");

        let package_data = PylockTomlParser::extract_first_package(&file_path);

        let gitpkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/gitpkg"))
            .expect("gitpkg dependency should be present");
        let git_extra = gitpkg
            .resolved_package
            .as_ref()
            .and_then(|pkg| pkg.extra_data.as_ref())
            .expect("gitpkg should preserve vcs provenance");
        assert!(git_extra.contains_key("vcs"));
        assert_eq!(gitpkg.is_pinned, Some(true));

        let archivepkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/archivepkg@1.0.0"))
            .expect("archivepkg dependency should be present");
        let archive_resolved = archivepkg
            .resolved_package
            .as_ref()
            .expect("archivepkg should have resolved package");
        assert_eq!(
            archive_resolved.download_url.as_deref(),
            Some("https://example.com/archivepkg-1.0.0.zip")
        );
        assert_eq!(
            archive_resolved.sha256,
            Some(
                Sha256Digest::from_hex(
                    "aaa10000000000000000000000000000000000000000000000000000000000a1"
                )
                .unwrap()
            )
        );
        assert_eq!(
            archive_resolved.md5,
            Some(Md5Digest::from_hex("bb11bb22bb33bb44bb55bb66bb77bb88").unwrap())
        );

        let sdistpkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/sdistpkg@2.0.0"))
            .expect("sdistpkg dependency should be present");
        let sdist_resolved = sdistpkg
            .resolved_package
            .as_ref()
            .expect("sdistpkg should have resolved package");
        assert_eq!(
            sdist_resolved.sha256,
            Some(
                Sha256Digest::from_hex(
                    "aaa20000000000000000000000000000000000000000000000000000000000a2"
                )
                .unwrap()
            )
        );

        let wheelpkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/wheelpkg@3.0.0"))
            .expect("wheelpkg dependency should be present");
        let wheel_resolved = wheelpkg
            .resolved_package
            .as_ref()
            .expect("wheelpkg should have resolved package");
        assert_eq!(
            wheel_resolved.sha256,
            Some(
                Sha256Digest::from_hex(
                    "aaa30000000000000000000000000000000000000000000000000000000000a3"
                )
                .unwrap()
            )
        );
        let wheel_extra = wheel_resolved
            .extra_data
            .as_ref()
            .expect("wheelpkg should preserve wheels metadata");
        assert!(wheel_extra.contains_key("wheels"));

        let dirpkg = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/dirpkg"))
            .expect("dirpkg dependency should be present");
        assert_eq!(dirpkg.is_pinned, Some(false));
        let dir_extra = dirpkg
            .resolved_package
            .as_ref()
            .and_then(|pkg| pkg.extra_data.as_ref())
            .expect("dirpkg should preserve directory provenance");
        assert!(dir_extra.contains_key("directory"));
    }

    fn sample_pylock_toml() -> &'static str {
        r#"
lock-version = "1.0"
created-by = "mousebender"
requires-python = ">=3.12"
environments = ["sys_platform == 'linux'"]
extras = ["docs"]
dependency-groups = ["dev"]
default-groups = ["default"]

[[packages]]
name = "requests"
version = "2.32.3"
requires-python = ">=3.8"
dependencies = [
    { name = "urllib3", version = "2.2.3" },
]

[[packages.wheels]]
name = "requests-2.32.3-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/requests-2.32.3-py3-none-any.whl"
size = 62574
hashes = { sha256 = "aabb0000000000000000000000000000000000000000000000000000000000aa" }

[[packages]]
name = "urllib3"
version = "2.2.3"

[[packages.wheels]]
name = "urllib3-2.2.3-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/urllib3-2.2.3-py3-none-any.whl"
size = 12345
hashes = { sha256 = "bbcc0000000000000000000000000000000000000000000000000000000000bb" }

[[packages]]
name = "pytest"
version = "8.3.5"
marker = "'dev' in dependency_groups"
dependencies = [
    { name = "pluggy", version = "1.5.0" },
]

[[packages.wheels]]
name = "pytest-8.3.5-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/pytest-8.3.5-py3-none-any.whl"
size = 55555
hashes = { sha256 = "ccdd0000000000000000000000000000000000000000000000000000000000cc" }

[[packages]]
name = "pluggy"
version = "1.5.0"
marker = "'dev' in dependency_groups"

[[packages.wheels]]
name = "pluggy-1.5.0-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/pluggy-1.5.0-py3-none-any.whl"
size = 44444
hashes = { sha256 = "ddee0000000000000000000000000000000000000000000000000000000000dd" }

[[packages]]
name = "sphinx"
version = "8.2.3"
marker = "'docs' in extras"
dependencies = [
    { name = "jinja2", version = "3.1.6" },
]

[[packages.wheels]]
name = "sphinx-8.2.3-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/sphinx-8.2.3-py3-none-any.whl"
size = 33333
hashes = { sha256 = "eeff0000000000000000000000000000000000000000000000000000000000ee" }

[[packages]]
name = "jinja2"
version = "3.1.6"
marker = "'docs' in extras"

[[packages.wheels]]
name = "jinja2-3.1.6-py3-none-any.whl"
url = "https://files.pythonhosted.org/packages/jinja2-3.1.6-py3-none-any.whl"
size = 22222
hashes = { sha256 = "ff110000000000000000000000000000000000000000000000000000000000ff" }

[[packages]]
name = "local-editable"
requires-python = ">=3.12"

[packages.directory]
path = "./local-editable"
editable = true
"#
    }
}
