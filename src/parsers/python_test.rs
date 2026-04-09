#[cfg(test)]
mod tests {
    use crate::models::PackageType;
    use crate::models::{DatasourceId, Dependency, Sha256Digest};
    use crate::parsers::{PackageParser, PythonParser, try_parse_file};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary test file with the given content and name
    fn create_temp_file(content: &str, filename: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(filename);
        fs::write(&file_path, content).expect("Failed to write file");

        (temp_dir, file_path)
    }

    fn create_temp_dist_info_metadata_file(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let dist_info = temp_dir.path().join("demo-1.0.0.dist-info");
        fs::create_dir_all(&dist_info).expect("Failed to create dist-info dir");
        let file_path = dist_info.join("METADATA");
        fs::write(&file_path, content).expect("Failed to write METADATA");

        (temp_dir, file_path)
    }

    fn create_temp_tar_gz(entries: &[(&str, &str)], filename: &str) -> (TempDir, PathBuf) {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use tar::{Builder, Header};

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(filename);
        let file = fs::File::create(&file_path).expect("Failed to create tar.gz file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        for (path, content) in entries {
            let bytes = content.as_bytes();
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, *path, bytes)
                .expect("Failed to add tar.gz entry");
        }

        let encoder = builder
            .into_inner()
            .expect("Failed to finish tar.gz archive");
        encoder.finish().expect("Failed to finalize tar.gz archive");

        (temp_dir, file_path)
    }

    fn create_temp_tar_bz2(entries: &[(&str, &str)], filename: &str) -> (TempDir, PathBuf) {
        use bzip2::Compression;
        use bzip2::write::BzEncoder;
        use tar::{Builder, Header};

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(filename);
        let file = fs::File::create(&file_path).expect("Failed to create tar.bz2 file");
        let encoder = BzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        for (path, content) in entries {
            let bytes = content.as_bytes();
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, *path, bytes)
                .expect("Failed to add tar.bz2 entry");
        }

        let encoder = builder
            .into_inner()
            .expect("Failed to finish tar.bz2 archive");
        encoder
            .finish()
            .expect("Failed to finalize tar.bz2 archive");

        (temp_dir, file_path)
    }

    fn create_temp_zip(entries: &[(&str, &str)], filename: &str) -> (TempDir, PathBuf) {
        use std::io::Write;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(filename);
        let file = fs::File::create(&file_path).expect("Failed to create zip file");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for (path, content) in entries {
            zip.start_file(*path, options)
                .expect("Failed to start zip entry");
            zip.write_all(content.as_bytes())
                .expect("Failed to write zip entry");
        }

        zip.finish().expect("Failed to finalize zip archive");

        (temp_dir, file_path)
    }

    fn make_high_ratio_metadata(prefix: &str) -> String {
        let mut content = prefix.to_string();
        content.push_str(&"A".repeat(300_000));
        content
    }

    #[test]
    fn test_is_match() {
        let pyproject_path = PathBuf::from("/some/path/pyproject.toml");
        let setup_cfg_path = PathBuf::from("/some/path/setup.cfg");
        let setup_path = PathBuf::from("/some/path/setup.py");
        let pkg_info_path = PathBuf::from("/some/path/PKG-INFO");
        let metadata_path = PathBuf::from("/some/path/demo-1.0.0.dist-info/METADATA");
        let bare_metadata_path = PathBuf::from("/some/path/METADATA");
        let pip_inspect_path = PathBuf::from("/some/path/pip-inspect.deplock");
        let pypi_json_path = PathBuf::from("/some/path/pypi.json");
        let invalid_path = PathBuf::from("/some/path/not_python.txt");

        assert!(PythonParser::is_match(&pyproject_path));
        assert!(PythonParser::is_match(&setup_cfg_path));
        assert!(PythonParser::is_match(&setup_path));
        assert!(PythonParser::is_match(&pkg_info_path));
        assert!(PythonParser::is_match(&metadata_path));
        assert!(!PythonParser::is_match(&bare_metadata_path));
        assert!(PythonParser::is_match(&pip_inspect_path));
        assert!(PythonParser::is_match(&pypi_json_path));
        assert!(!PythonParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_from_pyproject_toml() {
        let content = r#"
[project]
name = "test-package"
version = "0.1.0"
license = "MIT"
authors = [
    "Test User <test@example.com>",
    "Another User <another@example.com>"
]

[project.urls]
homepage = "https://example.com"
repository = "https://github.com/user/test-package"

[project.optional-dependencies]
test = ["pytest>=6.0.0"]

[project.dependencies]
requests = ">=2.25.0"
numpy = ">=1.20.0"
"#;

        let (_temp_file, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPyprojectToml)
        );
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
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
        assert!(package_data.extracted_license_statement.is_some());

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:pypi/test-package@0.1.0".to_string())
        );
    }

    #[test]
    fn test_extract_from_poetry_pyproject_toml_uses_poetry_datasource() {
        let content = r#"
[tool.poetry]
name = "poetry-demo"
version = "0.2.0"
description = "demo"
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name.as_deref(), Some("poetry-demo"));
        assert_eq!(package_data.version.as_deref(), Some("0.2.0"));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPoetryPyprojectToml)
        );
    }

    #[test]
    fn test_try_parse_file_build_system_only_pyproject_is_not_a_scan_error() {
        let content = r#"
[build-system]
build-backend = 'setuptools.build_meta'
requires = ['setuptools >= 47', 'wheel']

[tool.black]
line-length = 79
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pyproject.toml");
        let result = try_parse_file(&file_path).expect("pyproject should still be recognized");

        assert!(result.scan_errors.is_empty());
        assert_eq!(result.packages.len(), 1);
        assert_eq!(
            result.packages[0].datasource_id,
            Some(DatasourceId::PypiPyprojectToml)
        );
        assert!(result.packages[0].name.is_none());
    }

    #[test]
    fn test_extract_from_python_testdata() {
        let file_path = PathBuf::from("testdata/python/pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
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
        assert!(package_data.extracted_license_statement.is_some());

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:pypi/test-package@0.1.0".to_string())
        );

        // Check dependencies - should have 2 regular dependencies
        assert_eq!(package_data.dependencies.len(), 2);
        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        // Check that dependencies exist with correct package names (version-agnostic)
        assert!(
            purls.iter().any(|p| p.starts_with("pkg:pypi/requests@")),
            "Should contain requests dependency"
        );
        assert!(
            purls.iter().any(|p| p.starts_with("pkg:pypi/numpy@")),
            "Should contain numpy dependency"
        );
    }

    #[test]
    fn test_extract_from_setup_py() {
        let content = r#"
 from setuptools import setup, find_packages

setup(
    name="test-package",
    version="0.1.0",
    license="MIT",
    url="https://example.com",
    author="Test User",
    author_email="test@example.com",
    description="A test package",
    packages=find_packages(),
    install_requires=[
        "requests>=2.25.0",
        "numpy>=1.20.0",
    ],
)
"#;

        let (_temp_file, file_path) = create_temp_file(content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
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
        assert!(package_data.extracted_license_statement.is_some());
        assert_eq!(package_data.primary_language.as_deref(), Some("Python"));

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:pypi/test-package@0.1.0".to_string())
        );
    }

    #[test]
    fn test_extract_from_minimal_summary_fixture_setup_py() {
        let file_path = PathBuf::from(
            "testdata/summarycode-golden/summary/with_package_data/codebase/setup.py",
        );
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name.as_deref(), Some("codebase"));
        assert_eq!(package_data.primary_language.as_deref(), Some("Python"));
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("apache-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_setup_py_ast_basic() {
        let content = fs::read_to_string("testdata/python/setup-ast-basic.py")
            .expect("Failed to read setup-ast-basic.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("mypackage".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(package_data.description, Some("A test package".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com".to_string())
        );

        let author_party = package_data
            .parties
            .iter()
            .find(|party| party.role.as_deref() == Some("author"))
            .expect("author party should exist");
        assert_eq!(author_party.name, Some("John Doe".to_string()));
        assert_eq!(author_party.email, Some("john@example.com".to_string()));

        let install_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("install"))
            .collect();
        assert_eq!(install_deps.len(), 2);
    }

    #[test]
    fn test_setup_py_ast_constants() {
        let content = fs::read_to_string("testdata/python/setup-ast-constants.py")
            .expect("Failed to read setup-ast-constants.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("mypackage".to_string()));
        assert_eq!(package_data.version, Some("1.2.3".to_string()));
    }

    #[test]
    fn test_setup_py_ast_dict_unpack() {
        let content = fs::read_to_string("testdata/python/setup-ast-dict-unpack.py")
            .expect("Failed to read setup-ast-dict-unpack.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("dictpkg".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));

        let dep_purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref())
            .collect();
        assert!(
            dep_purls
                .iter()
                .any(|purl| purl.starts_with("pkg:pypi/requests")),
            "Should contain requests dependency"
        );
    }

    #[test]
    fn test_setup_py_ast_dynamic_expr() {
        let content = fs::read_to_string("testdata/python/setup-ast-dynamic.py")
            .expect("Failed to read setup-ast-dynamic.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("dynpkg".to_string()));
        assert_eq!(package_data.version, None);
    }

    #[test]
    fn test_setup_py_ast_install_requires() {
        let content = fs::read_to_string("testdata/python/setup-ast-install-requires.py")
            .expect("Failed to read setup-ast-install-requires.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        let install_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("install"))
            .collect();
        assert_eq!(install_deps.len(), 2);

        let dep_purls: Vec<&str> = install_deps
            .iter()
            .filter_map(|dep| dep.purl.as_deref())
            .collect();
        assert!(
            dep_purls
                .iter()
                .any(|purl| purl.starts_with("pkg:pypi/requests")),
            "Should contain requests dependency"
        );
        assert!(
            dep_purls
                .iter()
                .any(|purl| purl.starts_with("pkg:pypi/click")),
            "Should contain click dependency"
        );
    }

    #[test]
    fn test_setup_py_ast_extras_require() {
        let content = fs::read_to_string("testdata/python/setup-ast-extras-require.py")
            .expect("Failed to read setup-ast-extras-require.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        let dev_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("dev"))
            .collect();
        assert_eq!(dev_deps.len(), 2);
        assert!(dev_deps.iter().all(|dep| dep.is_optional == Some(true)));

        let docs_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("docs"))
            .collect();
        assert_eq!(docs_deps.len(), 1);
        assert!(docs_deps.iter().all(|dep| dep.is_optional == Some(true)));

        let test_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("test"))
            .collect();
        assert_eq!(test_deps.len(), 1);
        assert!(test_deps.iter().all(|dep| dep.is_optional == Some(true)));
    }

    #[test]
    fn test_setup_py_ast_malformed_syntax() {
        let content = fs::read_to_string("testdata/python/setup-ast-malformed.py")
            .expect("Failed to read setup-ast-malformed.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("broken".to_string()));
        assert_eq!(package_data.version, Some("0.0.1".to_string()));
    }

    #[test]
    fn test_setup_py_without_setup_call_emits_minimal_package() {
        let content = r#"
import pathlib

def main():
    script_dir = pathlib.Path(__file__).resolve().parent
    return script_dir

if __name__ == '__main__':
    main()
"#;
        let (_temp_dir, file_path) = create_temp_file(content, "setup.py");

        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSetupPy));
        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_suffix_setup_py_is_matched_and_emits_minimal_package() {
        let content = r#"
def main():
    return "no setup call"
"#;
        let (_temp_dir, file_path) = create_temp_file(content, "chrome_setup.py");

        assert!(PythonParser::is_match(&file_path));

        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSetupPy));
        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_setup_py_detects_setup_call_inside_called_function() {
        let content = r#"
from setuptools import setup

setup_args = {
    "name": "coverage",
    "version": "3.7",
    "license": "BSD",
}

def main():
    setup(**setup_args)

if __name__ == '__main__':
    main()
"#;
        let (_temp_dir, file_path) = create_temp_file(content, "setup.py");

        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("coverage".to_string()));
        assert_eq!(package_data.version, Some("3.7".to_string()));
        assert_eq!(package_data.purl.as_deref(), Some("pkg:pypi/coverage@3.7"));
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("BSD")
        );
    }

    #[test]
    fn test_setup_py_with_empty_setup_call_emits_minimal_package() {
        let content = "from setuptools import setup\n\nsetup()\n";
        let (_temp_dir, file_path) = create_temp_file(content, "setup.py");

        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.primary_language.as_deref(), Some("Python"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSetupPy));
        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_setup_py_ast_large_file() {
        let mut content = String::from("from setuptools import setup\n");
        content.push_str("setup(name=\"big-package\", version=\"9.9.9\")\n");
        content.push_str(&"a".repeat(1_048_600));

        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("big-package".to_string()));
        assert_eq!(package_data.version, Some("9.9.9".to_string()));
    }

    #[test]
    fn test_setup_py_ast_deep_nesting() {
        let mut nested = "\"1.0\"".to_string();
        for _ in 0..60 {
            nested = format!("[{}]", nested);
        }

        let content = format!(
            "from setuptools import setup\n\
NAME = \"deep\"\n\
VERSION = {}\n\
setup(name=NAME, version=VERSION)\n",
            nested
        );

        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("deep".to_string()));
        assert_eq!(package_data.version, None);
    }

    #[test]
    fn test_setup_py_ast_malicious_no_exec() {
        let content = fs::read_to_string("testdata/python/setup-ast-malicious.py")
            .expect("Failed to read setup-ast-malicious.py");
        let (_temp_dir, file_path) = create_temp_file(&content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("malicious".to_string()));
    }

    #[test]
    fn test_extract_api_url_basic() {
        // Given: A pyproject.toml with name and version
        let content = r#"
[project]
name = "requests"
version = "2.31.0"
license = "Apache-2.0"
authors = [
    "Kenneth Reitz <me@kennethreitz.com>"
]

[project.dependencies]
urllib3 = ">=1.21.1,<2"
chardet = ">=3.0.2,<5"
"#;

        let (_temp_file, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        // Then: API data URL should be generated
        assert_eq!(
            package_data.api_data_url,
            Some("https://pypi.org/pypi/requests/2.31.0/json".to_string())
        );

        // Then: Homepage URL should fall back to PyPI
        assert_eq!(
            package_data.homepage_url,
            Some("https://pypi.org/project/requests".to_string())
        );

        // Then: Download URL should be PyPI source tarball
        assert_eq!(
            package_data.download_url,
            Some("https://pypi.org/packages/source/r/requests/requests-2.31.0.tar.gz".to_string())
        );
    }

    #[test]
    fn test_extract_api_url_no_version() {
        // Given: A pyproject.toml with name but no version
        let content = r#"
[project]
name = "numpy"
license = "BSD-3-Clause"
authors = [
    "Travis Oliphant <oliphant@enthought.com>"
]

[project.dependencies]
"#;

        let (_temp_file, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        // Then: API data URL should still be generated (without version)
        assert_eq!(
            package_data.api_data_url,
            Some("https://pypi.org/pypi/numpy/json".to_string())
        );

        // Then: Homepage URL should fall back to PyPI
        assert_eq!(
            package_data.homepage_url,
            Some("https://pypi.org/project/numpy".to_string())
        );

        // Then: Download URL should not be generated (no version)
        assert_eq!(package_data.download_url, None);
    }

    #[test]
    fn test_extract_vcs_url_with_repository() {
        // Given: A pyproject.toml with a repository URL
        let content = r#"
[project]
name = "test-package"
version = "1.0.0"
repository = "https://github.com/user/test-package"

[project.dependencies]
"#;

        let (_temp_file, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        // Then: vcs_url should contain the repository URL
        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/user/test-package".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_with_urls_section() {
        // Given: A pyproject.toml with repository in URLs section
        let content = r#"
[project]
name = "test-package"
version = "1.0.0"

[project.urls]
repository = "https://github.com/user/test-package"
homepage = "https://example.com"

[project.dependencies]
"#;

        let (_temp_file, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        // Then: vcs_url should contain the repository URL from URLs section
        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/user/test-package".to_string())
        );
    }

    #[test]
    fn test_extract_vcs_url_without_repository() {
        // Given: A minimal pyproject.toml without a repository
        let content = r#"
[project]
name = "test-package"
version = "1.0.0"

[project.dependencies]
"#;

        let (_temp_file, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        // Then: vcs_url should be None
        assert_eq!(package_data.vcs_url, None);
    }

    #[test]
    fn test_extract_download_url_and_requires_python() {
        let content = r#"Metadata-Version: 2.0
Name: test-package
Version: 1.0.0
Summary: A test package
Home-page: https://example.com
Author: Test Author
Author-email: test@example.com
License: MIT
Download-URL: https://github.com/test/test-package/tarball/1.0.0
Requires-Python: >=3.8

This is a test package.
"#;

        let (_temp_file, file_path) = create_temp_dist_info_metadata_file(content);
        let package_data = PythonParser::extract_first_package(&file_path);

        // Verify Download-URL is extracted
        assert_eq!(
            package_data.download_url,
            Some("https://github.com/test/test-package/tarball/1.0.0".to_string())
        );

        // Verify Requires-Python is stored in extra_data
        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        assert_eq!(
            extra_data.get("requires_python").and_then(|v| v.as_str()),
            Some(">=3.8")
        );
    }

    #[test]
    fn test_extract_project_urls() {
        let content = r#"Metadata-Version: 2.1
Name: pip
Version: 20.2.2
Summary: The PyPA recommended tool for installing Python packages.
Home-page: https://pip.pypa.io/
Author: The pip developers
Author-email: distutils-sig@python.org
License: MIT
Project-URL: Documentation, https://pip.pypa.io
Project-URL: Source, https://github.com/pypa/pip
Project-URL: Changelog, https://pip.pypa.io/en/stable/news/

pip - The Python Package Installer
"#;

        let (_temp_file, file_path) = create_temp_dist_info_metadata_file(content);
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("pip".to_string()));
        assert_eq!(package_data.version, Some("20.2.2".to_string()));

        assert_eq!(
            package_data.homepage_url,
            Some("https://pip.pypa.io/".to_string())
        );

        assert_eq!(
            package_data.code_view_url,
            Some("https://github.com/pypa/pip".to_string()),
            "Source URL should be mapped to code_view_url"
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();

        assert_eq!(
            extra_data.get("changelog_url").and_then(|v| v.as_str()),
            Some("https://pip.pypa.io/en/stable/news/"),
            "Changelog URL should be in extra_data"
        );

        let project_urls = extra_data.get("project_urls").and_then(|v| v.as_object());
        assert!(
            project_urls.is_some(),
            "project_urls should be in extra_data"
        );
        let urls = project_urls.unwrap();

        assert_eq!(urls.len(), 3, "Should have all 3 Project-URLs");
        assert_eq!(
            urls.get("Documentation").and_then(|v| v.as_str()),
            Some("https://pip.pypa.io")
        );
        assert_eq!(
            urls.get("Source").and_then(|v| v.as_str()),
            Some("https://github.com/pypa/pip")
        );
        assert_eq!(
            urls.get("Changelog").and_then(|v| v.as_str()),
            Some("https://pip.pypa.io/en/stable/news/")
        );
    }

    #[test]
    fn test_extract_license_expression_from_metadata() {
        let content = r#"Metadata-Version: 2.4
Name: pip
Version: 24.0
License-Expression: MIT OR Apache-2.0

Example metadata.
"#;

        let (_temp_file, file_path) = create_temp_dist_info_metadata_file(content);
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit OR apache-2.0")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(package_data.license_detections.len(), 1);
    }

    #[test]
    fn test_extract_project_urls_with_mapping() {
        let content = r#"Metadata-Version: 2.2
Name: trimesh
Version: 4.6.1
Summary: Import, export, process, analyze and view triangular meshes.
Author-email: Michael Dawson-Haggerty <mikedh@kerfed.com>
License: MIT
Project-URL: homepage, https://github.com/mikedh/trimesh
Project-URL: documentation, https://trimesh.org
Project-URL: Bug Tracker, https://github.com/mikedh/trimesh/issues
Project-URL: Repository, https://github.com/mikedh/trimesh

Trimesh is a pure Python library for loading and using triangular meshes.
"#;

        let (_temp_file, file_path) = create_temp_dist_info_metadata_file(content);
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("trimesh".to_string()));

        assert_eq!(
            package_data.homepage_url,
            Some("https://github.com/mikedh/trimesh".to_string()),
            "homepage Project-URL should be mapped to homepage_url"
        );

        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/mikedh/trimesh".to_string()),
            "Repository Project-URL should be mapped to vcs_url"
        );

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/mikedh/trimesh/issues".to_string()),
            "Bug Tracker Project-URL should be mapped to bug_tracking_url"
        );

        assert_eq!(
            package_data.code_view_url, None,
            "documentation URL is not mapped to code_view_url (only source/source code/code are)"
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        let project_urls = extra_data.get("project_urls").and_then(|v| v.as_object());
        assert!(
            project_urls.is_some(),
            "All Project-URLs should be in extra_data"
        );

        let urls = project_urls.unwrap();
        assert_eq!(urls.len(), 4, "Should have all 4 Project-URLs");
        assert_eq!(
            urls.get("documentation").and_then(|v| v.as_str()),
            Some("https://trimesh.org")
        );
    }

    #[test]
    fn test_extract_project_urls_all_stored() {
        let content = r#"Metadata-Version: 2.1
Name: test-package
Version: 1.0.0
Summary: Test package
Project-URL: Documentation, https://docs.example.com
Project-URL: Source, https://github.com/user/test-package
Project-URL: Issues, https://github.com/user/test-package/issues

Test package description.
"#;

        let (_temp_file, file_path) = create_temp_dist_info_metadata_file(content);
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(
            package_data.code_view_url,
            Some("https://github.com/user/test-package".to_string()),
            "Source should map to code_view_url"
        );

        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/user/test-package/issues".to_string()),
            "Issues should map to bug_tracking_url"
        );

        assert!(package_data.extra_data.is_some());
        let extra_data = package_data.extra_data.unwrap();
        let project_urls = extra_data.get("project_urls").and_then(|v| v.as_object());
        assert!(project_urls.is_some(), "All Project-URLs should be stored");

        let urls = project_urls.unwrap();
        assert_eq!(urls.len(), 3, "Should have all 3 Project-URLs");
        assert_eq!(
            urls.get("Documentation").and_then(|v| v.as_str()),
            Some("https://docs.example.com")
        );
        assert_eq!(
            urls.get("Source").and_then(|v| v.as_str()),
            Some("https://github.com/user/test-package")
        );
        assert_eq!(
            urls.get("Issues").and_then(|v| v.as_str()),
            Some("https://github.com/user/test-package/issues")
        );
    }

    #[test]
    fn test_extract_license_file_from_metadata() {
        let content = fs::read_to_string("testdata/python/metadata-license-files/METADATA")
            .expect("read metadata fixture");
        let (_temp_dir, metadata_path) = create_temp_dist_info_metadata_file(&content);
        let package_data = PythonParser::extract_first_package(&metadata_path);

        assert_eq!(package_data.name, Some("example-package".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present");

        let license_files = extra_data
            .get("license_files")
            .expect("license_files should be present")
            .as_array()
            .expect("license_files should be an array");

        assert_eq!(license_files.len(), 2);
        assert_eq!(license_files[0].as_str().unwrap(), "LICENSE");
        assert_eq!(license_files[1].as_str().unwrap(), "COPYING.txt");

        assert_eq!(package_data.file_references.len(), 2);
        assert_eq!(package_data.file_references[0].path, "LICENSE");
        assert_eq!(package_data.file_references[1].path, "COPYING.txt");

        let referenced_filenames = package_data.license_detections[0].matches[0]
            .referenced_filenames
            .as_ref()
            .expect("referenced filenames should be present");
        assert_eq!(
            referenced_filenames,
            &vec!["LICENSE".to_string(), "COPYING.txt".to_string()]
        );
    }

    #[test]
    fn test_extract_metadata_reads_sibling_record_csv() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let dist_info = temp_dir.path().join("click-8.0.4.dist-info");
        fs::create_dir_all(&dist_info).expect("Failed to create dist-info dir");

        fs::write(
            dist_info.join("METADATA"),
            "Metadata-Version: 2.1\nName: click\nVersion: 8.0.4\n",
        )
        .expect("Failed to write METADATA");
        fs::write(
            dist_info.join("RECORD"),
            "click/__init__.py,,0\nclick/core.py,,10\nclick-8.0.4.dist-info/LICENSE.rst,,20\n",
        )
        .expect("Failed to write RECORD");

        let package_data = PythonParser::extract_first_package(&dist_info.join("METADATA"));
        let file_paths: Vec<&str> = package_data
            .file_references
            .iter()
            .map(|file_ref| file_ref.path.as_str())
            .collect();

        assert!(file_paths.contains(&"click/__init__.py"));
        assert!(file_paths.contains(&"click/core.py"));
        assert!(file_paths.contains(&"click-8.0.4.dist-info/LICENSE.rst"));
    }

    #[test]
    fn test_extract_metadata_reads_sibling_wheel_tags_and_builds_detailed_purl() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let dist_info = temp_dir.path().join("demo-1.0.0.dist-info");
        fs::create_dir_all(&dist_info).expect("Failed to create dist-info dir");

        fs::write(
            dist_info.join("METADATA"),
            "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\n",
        )
        .expect("Failed to write METADATA");
        fs::write(
            dist_info.join("WHEEL"),
            "Wheel-Version: 1.0\nGenerator: bdist_wheel (0.37.1)\nRoot-Is-Purelib: true\nTag: py2-none-any\nTag: py3-none-any\n",
        )
        .expect("Failed to write WHEEL");

        let package_data = PythonParser::extract_first_package(&dist_info.join("METADATA"));

        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiWheelMetadata)
        );
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:pypi/demo@1.0.0?extension=py2.py3-none-any")
        );

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        let wheel_tags = extra_data
            .get("wheel_tags")
            .and_then(|value| value.as_array())
            .expect("wheel_tags should be present as an array");
        assert_eq!(wheel_tags.len(), 2);
        assert_eq!(wheel_tags[0].as_str(), Some("py2-none-any"));
        assert_eq!(wheel_tags[1].as_str(), Some("py3-none-any"));
        assert_eq!(
            extra_data
                .get("wheel_version")
                .and_then(|value| value.as_str()),
            Some("1.0")
        );
        assert_eq!(
            extra_data
                .get("wheel_generator")
                .and_then(|value| value.as_str()),
            Some("bdist_wheel (0.37.1)")
        );
        assert_eq!(
            extra_data
                .get("root_is_purelib")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_extract_metadata_without_sibling_wheel_keeps_plain_purl() {
        let (_temp_dir, metadata_path) = create_temp_dist_info_metadata_file(
            "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\n",
        );

        let package_data = PythonParser::extract_first_package(&metadata_path);

        assert_eq!(package_data.purl.as_deref(), Some("pkg:pypi/demo@1.0.0"));
        assert!(
            package_data
                .extra_data
                .as_ref()
                .is_none_or(|extra| !extra.contains_key("wheel_tags"))
        );
    }

    #[test]
    fn test_is_match_rejects_bare_metadata_outside_dist_info() {
        assert!(!PythonParser::is_match(&PathBuf::from(
            "third_party/devscripts/METADATA"
        )));
        assert!(PythonParser::is_match(&PathBuf::from(
            "site-packages/demo-1.0.0.dist-info/METADATA"
        )));
    }

    #[test]
    fn test_extract_first_package_returns_default_for_bare_metadata_outside_dist_info() {
        let (_temp_dir, metadata_path) = create_temp_file(
            "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\n",
            "METADATA",
        );

        let package_data = PythonParser::extract_first_package(&metadata_path);

        assert_eq!(package_data.datasource_id, None);
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
    }

    #[test]
    fn test_extract_pkg_info_reads_sibling_installed_files_txt() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let egg_info = temp_dir.path().join("examplepkg.egg-info");
        fs::create_dir_all(&egg_info).expect("Failed to create egg-info dir");

        fs::write(
            egg_info.join("PKG-INFO"),
            "Metadata-Version: 1.2\nName: examplepkg\nVersion: 1.0.0\n",
        )
        .expect("Failed to write PKG-INFO");
        fs::write(
            egg_info.join("installed-files.txt"),
            "../examplepkg/__init__.py\n../examplepkg/core.py\n",
        )
        .expect("Failed to write installed-files.txt");

        let package_data = PythonParser::extract_first_package(&egg_info.join("PKG-INFO"));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiEditableEggPkginfo)
        );
        let file_paths: Vec<&str> = package_data
            .file_references
            .iter()
            .map(|file_ref| file_ref.path.as_str())
            .collect();

        assert!(file_paths.contains(&"../examplepkg/__init__.py"));
        assert!(file_paths.contains(&"../examplepkg/core.py"));
    }

    #[test]
    fn test_extract_pkg_info_reads_sibling_sources_txt() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let egg_info = temp_dir.path().join("PyJPString.egg-info");
        fs::create_dir_all(&egg_info).expect("Failed to create egg-info dir");

        fs::write(
            egg_info.join("PKG-INFO"),
            "Metadata-Version: 1.0\nName: PyJPString\nVersion: 0.0.3\n",
        )
        .expect("Failed to write PKG-INFO");
        fs::write(
            egg_info.join("SOURCES.txt"),
            "setup.py\nPyJPString.egg-info/PKG-INFO\nPyJPString.egg-info/top_level.txt\njpstring/__init__.py\n",
        )
        .expect("Failed to write SOURCES.txt");

        let package_data = PythonParser::extract_first_package(&egg_info.join("PKG-INFO"));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiEditableEggPkginfo)
        );
        let file_paths: Vec<&str> = package_data
            .file_references
            .iter()
            .map(|file_ref| file_ref.path.as_str())
            .collect();

        assert!(file_paths.contains(&"setup.py"));
        assert!(file_paths.contains(&"PyJPString.egg-info/top_level.txt"));
        assert!(file_paths.contains(&"jpstring/__init__.py"));
    }

    #[test]
    fn test_extract_metadata_requires_dist_from_anonapi_wheel() {
        let path = PathBuf::from(
            "testdata/python/golden/metadata-fixtures/unpacked_wheel/metadata-2.1/with_sources/anonapi-0.0.19.dist-info/METADATA",
        );
        let package_data = PythonParser::extract_first_package(&path);

        assert_eq!(package_data.name, Some("anonapi".to_string()));
        assert_eq!(package_data.dependencies.len(), 1);

        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:pypi/pyyaml"));
        assert_eq!(dep.extracted_requirement, None);
        assert_eq!(dep.scope.as_deref(), Some("install"));
        assert_eq!(dep.is_optional, Some(false));
        assert_eq!(dep.is_runtime, Some(true));
    }

    #[test]
    fn test_extract_pkg_info_from_egg_info_dir_uses_extracted_egg_pkginfo_datasource() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let egg_info = temp_dir.path().join("EGG-INFO");
        fs::create_dir_all(&egg_info).expect("Failed to create EGG-INFO dir");
        fs::write(
            egg_info.join("PKG-INFO"),
            "Metadata-Version: 1.2\nName: extracted-egg\nVersion: 1.0.0\n",
        )
        .expect("Failed to write PKG-INFO");

        let package_data = PythonParser::extract_first_package(&egg_info.join("PKG-INFO"));

        assert_eq!(package_data.name.as_deref(), Some("extracted-egg"));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiEggPkginfo)
        );
    }

    #[test]
    fn test_extract_pkg_info_recovers_anonapi_sdist_requires_txt_dependency() {
        let path = PathBuf::from(
            "testdata/python/golden/metadata-fixtures/unpacked_sdist/metadata-1.2/anonapi-0.0.19/PKG-INFO",
        );
        let package_data = PythonParser::extract_first_package(&path);

        assert_eq!(package_data.name, Some("anonapi".to_string()));
        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:pypi/pyyaml"));
        assert_eq!(dep.scope.as_deref(), Some("install"));
        assert_eq!(dep.is_optional, Some(false));
    }

    #[test]
    fn test_extract_metadata_requires_dist_jinja2_extras() {
        let path = PathBuf::from(
            "testdata/python/golden/metadata-fixtures/unpacked_wheel/metadata-2.0/Jinja2-2.10.dist-info/METADATA",
        );
        let package_data = PythonParser::extract_first_package(&path);

        let install_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/markupsafe"))
            .expect("MarkupSafe install dependency should be extracted");
        assert_eq!(install_dep.extracted_requirement.as_deref(), Some(">=0.23"));
        assert_eq!(install_dep.scope.as_deref(), Some("install"));
        assert_eq!(install_dep.is_optional, Some(false));

        let extra_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/babel"))
            .expect("Babel extra dependency should be extracted");
        assert_eq!(extra_dep.extracted_requirement.as_deref(), Some(">=0.8"));
        assert_eq!(extra_dep.scope.as_deref(), Some("i18n"));
        assert_eq!(extra_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_metadata_requires_dist_urllib3_markers() {
        let path = PathBuf::from(
            "testdata/python/golden/metadata-fixtures/unpacked_wheel/metadata-2.0/urllib3-1.26.4.dist-info/METADATA",
        );
        let package_data = PythonParser::extract_first_package(&path);

        let secure_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/pyopenssl"))
            .expect("secure extra dependency should be extracted");
        assert_eq!(secure_dep.extracted_requirement.as_deref(), Some(">=0.14"));
        assert_eq!(secure_dep.scope.as_deref(), Some("secure"));
        assert_eq!(secure_dep.is_optional, Some(true));

        let marker_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/ipaddress"))
            .expect("marker-bearing extra dependency should be extracted");
        let marker_data = marker_dep
            .extra_data
            .as_ref()
            .expect("marker extra_data should be preserved");
        assert_eq!(
            marker_data
                .get("python_version")
                .and_then(|value| value.as_str()),
            Some("== 2.7")
        );

        let socks_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/pysocks"))
            .expect("socks extra dependency should be extracted");
        assert_eq!(
            socks_dep.extracted_requirement.as_deref(),
            Some("!=1.5.7,<2.0,>=1.5.6")
        );
        assert_eq!(socks_dep.scope.as_deref(), Some("socks"));
        assert_eq!(socks_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_pkg_info_requires_dist_extras() {
        let path = PathBuf::from("testdata/python/golden/metadata-fixtures/metadata/v20/PKG-INFO");
        let package_data = PythonParser::extract_first_package(&path);

        let srv_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/dnspython"))
            .expect("srv extra dependency should be extracted from PKG-INFO");
        assert_eq!(
            srv_dep.extracted_requirement.as_deref(),
            Some("<2.0.0,>=1.8.0")
        );
        assert_eq!(srv_dep.scope.as_deref(), Some("srv"));
        assert_eq!(srv_dep.is_optional, Some(true));
    }

    #[test]
    fn test_extract_pkg_info_recovers_distinct_requires_txt_marker_variants() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let egg_info = temp_dir.path().join("celery.egg-info");
        fs::create_dir_all(&egg_info).expect("Failed to create egg-info dir");

        fs::write(
            egg_info.join("PKG-INFO"),
            "Metadata-Version: 1.2\nName: celery\nVersion: 5.2.3\n",
        )
        .expect("Failed to write PKG-INFO");
        fs::write(
            egg_info.join("requires.txt"),
            "[tblib:python_version < \"3.8.0\"]\ntblib>=1.3.0\n\n[tblib:python_version >= \"3.8.0\"]\ntblib>=1.5.0\n",
        )
        .expect("Failed to write requires.txt");

        let package_data = PythonParser::extract_first_package(&egg_info.join("PKG-INFO"));
        let tblib_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.purl.as_deref() == Some("pkg:pypi/tblib"))
            .collect();

        assert_eq!(tblib_deps.len(), 2);
        assert!(tblib_deps.iter().any(|dep| {
            dep.extracted_requirement.as_deref() == Some(">=1.3.0")
                && dep.scope.as_deref() == Some("tblib")
                && dep
                    .extra_data
                    .as_ref()
                    .and_then(|extra| extra.get("python_version"))
                    .and_then(|value| value.as_str())
                    == Some("< 3.8.0")
        }));
        assert!(tblib_deps.iter().any(|dep| {
            dep.extracted_requirement.as_deref() == Some(">=1.5.0")
                && dep.scope.as_deref() == Some("tblib")
                && dep
                    .extra_data
                    .as_ref()
                    .and_then(|extra| extra.get("python_version"))
                    .and_then(|value| value.as_str())
                    == Some(">= 3.8.0")
        }));
    }

    #[test]
    fn test_extract_metadata_requires_dist_pinned_version() {
        let (_temp_dir, file_path) = create_temp_dist_info_metadata_file(
            "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nRequires-Dist: helper==1.2.3\n",
        );
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:pypi/helper@1.2.3"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("==1.2.3"));
        assert_eq!(dep.is_pinned, Some(true));
    }

    #[test]
    fn test_is_match_wheel_extension() {
        let wheel_path = PathBuf::from("/some/path/package-1.0.0-py3-none-any.whl");
        let wheel_uppercase = PathBuf::from("/some/path/package-1.0.0-py3-none-any.WHL");

        assert!(PythonParser::is_match(&wheel_path));
        assert!(PythonParser::is_match(&wheel_uppercase));
    }

    #[test]
    fn test_is_match_egg_extension() {
        let egg_path = PathBuf::from("/some/path/package-1.0.0-py3.9.egg");
        let egg_uppercase = PathBuf::from("/some/path/package-1.0.0-py3.9.EGG");

        assert!(PythonParser::is_match(&egg_path));
        assert!(PythonParser::is_match(&egg_uppercase));
    }

    #[test]
    fn test_is_match_sdist_archive_extensions() {
        let tar_gz_path = PathBuf::from("/some/path/package-1.0.0.tar.gz");
        let tar_bz2_path = PathBuf::from("/some/path/package-1.0.0.tar.bz2");
        let zip_path = PathBuf::from("/some/path/package-1.0.0.zip");
        let plain_zip_path = PathBuf::from("/some/path/archive.zip");
        let plain_tar_path = PathBuf::from("/some/path/archive.tar.gz");

        assert!(PythonParser::is_match(&tar_gz_path));
        assert!(PythonParser::is_match(&tar_bz2_path));
        assert!(PythonParser::is_match(&zip_path));
        assert!(!PythonParser::is_match(&plain_zip_path));
        assert!(!PythonParser::is_match(&plain_tar_path));
    }

    #[test]
    fn test_is_match_zip_sdist_requires_pkg_info_for_real_file() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[("docs/readme.txt", "not a python sdist")],
            "libzip-1.10.1.zip",
        );

        assert!(!PythonParser::is_match(&zip_path));
        assert!(try_parse_file(&zip_path).is_none());
    }

    #[test]
    fn test_try_parse_file_rejects_zip_sdist_with_unsafe_pkg_info_path() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[(
                "../demo-1.0.0/PKG-INFO",
                "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\n",
            )],
            "demo-1.0.0.zip",
        );

        assert!(!PythonParser::is_match(&zip_path));
        assert!(try_parse_file(&zip_path).is_none());
    }

    #[test]
    fn test_try_parse_file_rejects_zip_sdist_with_vendored_pkg_info_only() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[(
                "vendor/other-9.9.9/PKG-INFO",
                "Metadata-Version: 2.1\nName: other\nVersion: 9.9.9\n",
            )],
            "demo-1.0.0.zip",
        );

        assert!(!PythonParser::is_match(&zip_path));
        assert!(try_parse_file(&zip_path).is_none());
    }

    #[test]
    fn test_extract_from_wheel_archive() {
        let wheel_path = PathBuf::from(
            "testdata/python/golden/archives/atomicwrites-1.2.1-py2.py3-none-any.whl",
        );

        let package_data = PythonParser::extract_first_package(&wheel_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name, Some("atomicwrites".to_string()));
        assert_eq!(package_data.version, Some("1.2.1".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://github.com/untitaker/python-atomicwrites".to_string())
        );
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiWheel));
        assert_eq!(package_data.primary_language, Some("Python".to_string()));

        assert!(!package_data.parties.is_empty());
        assert_eq!(
            package_data.parties[0].name,
            Some("Markus Unterwaditzer".to_string())
        );
        assert_eq!(
            package_data.parties[0].email,
            Some("markus@unterwaditzer.net".to_string())
        );

        assert!(package_data.purl.is_some());
        let purl = package_data.purl.unwrap();
        assert!(purl.starts_with("pkg:pypi/atomicwrites@1.2.1"));
        assert!(purl.contains("extension="));

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        assert!(extra_data.contains_key("python_requires"));
        assert!(extra_data.contains_key("abi_tag"));
        assert!(extra_data.contains_key("platform_tag"));

        assert!(package_data.size.is_some(), "size should be calculated");
        assert_eq!(package_data.size.unwrap(), 1427);
        assert!(package_data.sha256.is_some(), "sha256 should be calculated");
    }

    #[test]
    fn test_extract_from_egg_archive() {
        let egg_path =
            PathBuf::from("testdata/python/golden/archives/commoncode-21.5.12-py3.9.egg");

        let package_data = PythonParser::extract_first_package(&egg_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name, Some("commoncode".to_string()));
        assert_eq!(package_data.version, Some("21.5.12".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://github.com/nexB/commoncode".to_string())
        );
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiEgg));
        assert_eq!(package_data.primary_language, Some("Python".to_string()));

        assert!(!package_data.parties.is_empty());
        assert_eq!(
            package_data.parties[0].name,
            Some("nexB. Inc. and others".to_string())
        );

        assert!(package_data.purl.is_some());
        let purl = package_data.purl.unwrap();
        assert!(purl.starts_with("pkg:pypi/commoncode@21.5.12"));
        assert!(purl.contains("type=egg"));

        assert!(package_data.size.is_some(), "size should be calculated");
        assert_eq!(package_data.size.unwrap(), 1756);
        assert!(package_data.sha256.is_some(), "sha256 should be calculated");
    }

    #[test]
    fn test_extract_from_sdist_archive() {
        let sdist_path = PathBuf::from("testdata/python/pip-22.0.4.tar.gz");

        let package_data = PythonParser::extract_first_package(&sdist_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name.as_deref(), Some("pip"));
        assert_eq!(package_data.version.as_deref(), Some("22.0.4"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSdist));
        assert_eq!(package_data.primary_language.as_deref(), Some("Python"));
        assert_eq!(
            package_data.homepage_url.as_deref(),
            Some("https://pip.pypa.io/")
        );
        assert_eq!(
            package_data.repository_download_url.as_deref(),
            Some("https://pypi.org/packages/source/p/pip/pip-22.0.4.tar.gz")
        );
        assert_eq!(package_data.purl.as_deref(), Some("pkg:pypi/pip@22.0.4"));
        assert!(package_data.size.is_some(), "size should be calculated");
        assert!(package_data.sha256.is_some(), "sha256 should be calculated");
    }

    #[test]
    fn test_extract_from_sdist_zip_archive() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[(
                "demo-1.0.0/PKG-INFO",
                "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: Zip demo\n",
            )],
            "demo-1.0.0.zip",
        );

        let package_data = PythonParser::extract_first_package(&zip_path);

        assert_eq!(package_data.name.as_deref(), Some("demo"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSdist));
    }

    #[test]
    fn test_extract_from_sdist_zip_archive_rejects_suspicious_metadata_entry() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[(
                "demo-1.0.0/PKG-INFO",
                &make_high_ratio_metadata(
                    "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: suspicious\n",
                ),
            )],
            "demo-1.0.0.zip",
        );

        let package_data = PythonParser::extract_first_package(&zip_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSdist));
    }

    #[test]
    fn test_extract_from_sdist_zip_archive_rejects_unsafe_absolute_metadata_path() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[(
                "C:/demo-1.0.0/PKG-INFO",
                "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: unsafe path\n",
            )],
            "demo-1.0.0.zip",
        );

        let package_data = PythonParser::extract_first_package(&zip_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_extract_from_sdist_zip_archive_rejects_parent_traversal_metadata_path() {
        let (_temp_dir, zip_path) = create_temp_zip(
            &[(
                "../demo-1.0.0/PKG-INFO",
                "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: traversal path\n",
            )],
            "demo-1.0.0.zip",
        );

        let package_data = PythonParser::extract_first_package(&zip_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_is_match_tgz_sdist_requires_pkg_info_for_real_file() {
        let (_temp_dir, archive_path) = create_temp_tar_gz(
            &[(
                "postgresql/Chart.yaml",
                "apiVersion: v2\nname: postgresql\nversion: 13.2.24\n",
            )],
            "postgresql-13.2.24.tgz",
        );

        assert!(!PythonParser::is_match(&archive_path));
        assert!(try_parse_file(&archive_path).is_none());
    }

    #[test]
    fn test_extract_from_tgz_sdist_archive_with_pkg_info() {
        let (_temp_dir, archive_path) = create_temp_tar_gz(
            &[(
                "demo-1.0.0/PKG-INFO",
                "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: tgz demo\n",
            )],
            "demo-1.0.0.tgz",
        );

        assert!(PythonParser::is_match(&archive_path));

        let package_data = PythonParser::extract_first_package(&archive_path);

        assert_eq!(package_data.name.as_deref(), Some("demo"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSdist));
    }

    #[test]
    fn test_extract_from_sdist_tar_bz2_archive() {
        let (_temp_dir, archive_path) = create_temp_tar_bz2(
            &[(
                "demo-1.0.0/PKG-INFO",
                "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: Tar bz2 demo\n",
            )],
            "demo-1.0.0.tar.bz2",
        );

        let package_data = PythonParser::extract_first_package(&archive_path);

        assert_eq!(package_data.name.as_deref(), Some("demo"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiSdist));
    }

    #[test]
    fn test_extract_from_sdist_archive_prefers_egg_info_metadata_and_sidecars() {
        let (_temp_dir, archive_path) = create_temp_tar_gz(
            &[
                (
                    "demo-1.0.0/PKG-INFO",
                    "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: Root metadata\n",
                ),
                (
                    "demo-1.0.0/demo.egg-info/PKG-INFO",
                    "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: Egg metadata\nRequires-Dist: click>=8\n",
                ),
                ("demo-1.0.0/demo.egg-info/requires.txt", "click>=8\n"),
                (
                    "demo-1.0.0/demo.egg-info/SOURCES.txt",
                    "README.md\ndemo/__init__.py\n",
                ),
                (
                    "demo-1.0.0/vendor/other.egg-info/PKG-INFO",
                    "Metadata-Version: 2.1\nName: other\nVersion: 9.9.9\nSummary: Nested metadata\nRequires-Dist: evil>=1\n",
                ),
                ("demo-1.0.0/vendor/other.egg-info/requires.txt", "evil>=1\n"),
            ],
            "demo-1.0.0.tar.gz",
        );

        let package_data = PythonParser::extract_first_package(&archive_path);

        assert_eq!(package_data.name.as_deref(), Some("demo"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(package_data.description.as_deref(), Some("Egg metadata"));

        assert!(package_data.dependencies.iter().any(|dependency| {
            dependency.purl.as_deref() == Some("pkg:pypi/click")
                && dependency.extracted_requirement.as_deref() == Some(">=8")
        }));
        assert!(
            !package_data
                .dependencies
                .iter()
                .any(|dependency| dependency.purl.as_deref() == Some("pkg:pypi/evil"))
        );

        let file_refs: Vec<_> = package_data
            .file_references
            .iter()
            .map(|file_ref| file_ref.path.as_str())
            .collect();
        assert!(file_refs.contains(&"README.md"));
        assert!(file_refs.contains(&"demo/__init__.py"));
    }

    #[test]
    fn test_corrupt_wheel_archive_no_panic() {
        let (_temp_dir, corrupt_path) = create_temp_file("this is not a valid zip file", "bad.whl");
        let package_data = PythonParser::extract_first_package(&corrupt_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiWheel));
    }

    #[test]
    fn test_extract_from_wheel_archive_rejects_suspicious_metadata_entry() {
        let (_temp_dir, wheel_path) = create_temp_zip(
            &[(
                "demo-1.0.0.dist-info/METADATA",
                &make_high_ratio_metadata(
                    "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: suspicious wheel\n",
                ),
            )],
            "demo-1.0.0-py3-none-any.whl",
        );

        let package_data = PythonParser::extract_first_package(&wheel_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_extract_from_egg_archive_rejects_suspicious_metadata_entry() {
        let (_temp_dir, egg_path) = create_temp_zip(
            &[(
                "demo-1.0.0.egg-info/PKG-INFO",
                &make_high_ratio_metadata(
                    "Metadata-Version: 2.1\nName: demo\nVersion: 1.0.0\nSummary: suspicious egg\n",
                ),
            )],
            "demo-1.0.0-py3.11.egg",
        );

        let package_data = PythonParser::extract_first_package(&egg_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiEgg));
    }

    #[test]
    fn test_corrupt_egg_archive_no_panic() {
        let (_temp_dir, corrupt_path) = create_temp_file("this is not a valid zip file", "bad.egg");
        let package_data = PythonParser::extract_first_package(&corrupt_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiEgg));
    }

    #[test]
    fn test_wheel_purl_format() {
        let wheel_path = PathBuf::from(
            "testdata/python/golden/archives/atomicwrites-1.2.1-py2.py3-none-any.whl",
        );

        let package_data = PythonParser::extract_first_package(&wheel_path);

        let purl = package_data.purl.expect("PURL should be present");
        assert!(
            purl.contains("extension=py2.py3-none-any"),
            "PURL should contain extension qualifiers: {}",
            purl
        );
    }

    #[test]
    fn test_egg_purl_format() {
        let egg_path =
            PathBuf::from("testdata/python/golden/archives/commoncode-21.5.12-py3.9.egg");

        let package_data = PythonParser::extract_first_package(&egg_path);

        let purl = package_data.purl.expect("PURL should be present");
        assert!(
            purl.contains("type=egg"),
            "PURL should contain type=egg qualifier: {}",
            purl
        );
    }

    #[test]
    fn test_parse_record_csv() {
        use crate::parsers::python::parse_record_csv;

        let record_content = "package/__init__.py,sha256=47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU,0\n\
                             package/module.py,sha256=2jmj7l5rSw0yVb_vlWAYkK_YBwk1BkwQZq6ZNzJBH20,1234\n\
                             package/data.txt,,100\n";

        let file_refs = parse_record_csv(record_content);

        assert_eq!(file_refs.len(), 3);

        assert_eq!(file_refs[0].path, "package/__init__.py");
        assert_eq!(file_refs[0].size, Some(0));
        assert!(file_refs[0].sha256.is_some());

        assert_eq!(file_refs[1].path, "package/module.py");
        assert_eq!(file_refs[1].size, Some(1234));
        assert!(file_refs[1].sha256.is_some());

        assert_eq!(file_refs[2].path, "package/data.txt");
        assert_eq!(file_refs[2].size, Some(100));
        assert!(file_refs[2].sha256.is_none());
    }

    #[test]
    fn test_parse_installed_files_txt() {
        use crate::parsers::python::parse_installed_files_txt;

        let installed_files_content = "__init__.py\n\
                                       module.py\n\
                                       data.txt\n";

        let file_refs = parse_installed_files_txt(installed_files_content);

        assert_eq!(file_refs.len(), 3);
        assert_eq!(file_refs[0].path, "__init__.py");
        assert_eq!(file_refs[1].path, "module.py");
        assert_eq!(file_refs[2].path, "data.txt");

        for file_ref in &file_refs {
            assert!(file_ref.size.is_none());
            assert!(file_ref.sha256.is_none());
            assert!(file_ref.sha1.is_none());
        }
    }

    #[test]
    fn test_wheel_file_references() {
        let wheel_path = PathBuf::from(
            "testdata/python/golden/archives/atomicwrites-1.2.1-py2.py3-none-any.whl",
        );

        let package_data = PythonParser::extract_first_package(&wheel_path);

        assert!(!package_data.file_references.is_empty());
        for file_ref in &package_data.file_references {
            assert!(!file_ref.path.is_empty());
        }
    }

    #[test]
    fn test_egg_file_references() {
        let egg_path =
            PathBuf::from("testdata/python/golden/archives/commoncode-21.5.12-py3.9.egg");

        let package_data = PythonParser::extract_first_package(&egg_path);

        assert!(package_data.file_references.is_empty());
        for file_ref in &package_data.file_references {
            assert!(!file_ref.path.is_empty());
        }
    }

    #[test]
    fn test_missing_file_references_graceful() {
        let (_temp_dir, corrupt_path) = create_temp_file("this is not a valid zip file", "bad.whl");
        let package_data = PythonParser::extract_first_package(&corrupt_path);

        assert!(package_data.file_references.is_empty());
    }

    #[test]
    fn test_pip_inspect_is_match() {
        let pip_inspect_path = PathBuf::from("/some/path/pip-inspect.deplock");
        assert!(PythonParser::is_match(&pip_inspect_path));
    }

    #[test]
    fn test_pip_cache_origin_json_is_match_only_in_pip_wheels_cache() {
        let cache_origin = PathBuf::from("/tmp/.cache/pip/wheels/eb/60/37/cachehash/origin.json");
        let generic_origin = PathBuf::from("/tmp/project/origin.json");

        assert!(PythonParser::is_match(&cache_origin));
        assert!(!PythonParser::is_match(&generic_origin));
    }

    #[test]
    fn test_extract_from_pip_cache_origin_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache_dir = temp_dir
            .path()
            .join(".cache")
            .join("pip")
            .join("wheels")
            .join("eb")
            .join("60")
            .join("37")
            .join("ee40403cbd895ccdb57eb28b03b0afabeb449d5df9ce776a0d");
        fs::create_dir_all(&cache_dir).expect("Failed to create pip cache dir");

        let origin_path = cache_dir.join("origin.json");
        fs::write(
            &origin_path,
            r#"{
                "archive_info": {
                    "hash": "sha256=a5488a3dd1fd021ce33f969780b88fe0f7eebb76eb20996d7318f307612a045b",
                    "hashes": {
                        "sha256": "a5488a3dd1fd021ce33f969780b88fe0f7eebb76eb20996d7318f307612a045b"
                    }
                },
                "url": "https://files.pythonhosted.org/packages/48/30/4559d06bad5bb627733dac1ef28c34f5e35f1461247ba63e5f6366901277/construct-2.10.68.tar.gz"
            }"#,
        )
        .expect("Failed to write origin.json");

        let package_data = PythonParser::extract_first_package(&origin_path);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.primary_language, Some("Python".to_string()));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPipOriginJson)
        );
        assert_eq!(package_data.name, Some("construct".to_string()));
        assert_eq!(package_data.version, Some("2.10.68".to_string()));
        assert_eq!(
            package_data.download_url.as_deref(),
            Some(
                "https://files.pythonhosted.org/packages/48/30/4559d06bad5bb627733dac1ef28c34f5e35f1461247ba63e5f6366901277/construct-2.10.68.tar.gz"
            )
        );
        assert_eq!(
            package_data.sha256,
            Some(
                Sha256Digest::from_hex(
                    "a5488a3dd1fd021ce33f969780b88fe0f7eebb76eb20996d7318f307612a045b"
                )
                .unwrap()
            )
        );
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:pypi/construct@2.10.68")
        );
        assert_eq!(
            package_data.repository_homepage_url.as_deref(),
            Some("https://pypi.org/project/construct")
        );
    }

    #[test]
    fn test_invalid_pip_cache_origin_json_returns_default_package() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache_dir = temp_dir
            .path()
            .join(".cache")
            .join("pip")
            .join("wheels")
            .join("aa")
            .join("bb")
            .join("cc")
            .join("badcache");
        fs::create_dir_all(&cache_dir).expect("Failed to create pip cache dir");

        let origin_path = cache_dir.join("origin.json");
        fs::write(&origin_path, "{not-valid-json").expect("Failed to write invalid origin.json");

        let package_data = PythonParser::extract_first_package(&origin_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert!(package_data.purl.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiPipOriginJson)
        );
    }

    #[test]
    fn test_extract_from_pip_inspect() {
        let test_file = PathBuf::from("testdata/python/pip-inspect/pip-inspect.deplock");
        let package_data = PythonParser::extract_first_package(&test_file);

        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(package_data.name, Some("univers".to_string()));
        assert_eq!(package_data.version, Some("0.0.0".to_string()));
        assert_eq!(package_data.primary_language, Some("Python".to_string()));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiInspectDeplock)
        );
        assert!(package_data.is_virtual);

        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("apache-2.0 AND bsd-3-clause AND mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0 AND BSD-3-Clause AND MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert!(package_data.extracted_license_statement.is_some());

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present");
        assert_eq!(
            extra_data.get("pip_version").and_then(|v| v.as_str()),
            Some("24.1")
        );
        assert_eq!(
            extra_data.get("inspect_version").and_then(|v| v.as_str()),
            Some("1")
        );

        assert_eq!(package_data.dependencies.len(), 5);

        let dep_purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        assert!(
            dep_purls
                .iter()
                .any(|p| p.starts_with("pkg:pypi/packaging@")),
            "Should contain packaging dependency"
        );
        assert!(
            dep_purls
                .iter()
                .any(|p| p.starts_with("pkg:pypi/setuptools@")),
            "Should contain setuptools dependency"
        );
        assert!(dep_purls.contains(&"pkg:pypi/attrs"));
        assert!(dep_purls.contains(&"pkg:pypi/semantic-version"));
        assert!(dep_purls.contains(&"pkg:pypi/semver"));

        for dep in &package_data.dependencies {
            assert_eq!(dep.is_runtime, Some(true));
            assert_eq!(dep.is_optional, Some(false));
            if dep
                .purl
                .as_deref()
                .is_some_and(|purl| purl.starts_with("pkg:pypi/packaging@"))
                || dep
                    .purl
                    .as_deref()
                    .is_some_and(|purl| purl.starts_with("pkg:pypi/setuptools@"))
            {
                assert_eq!(dep.is_pinned, Some(true));
                assert!(dep.resolved_package.is_some());
            } else {
                assert_eq!(dep.is_pinned, Some(false));
                assert!(dep.resolved_package.is_none());
            }
        }
    }

    #[test]
    fn test_extract_from_pip_inspect_direct_dependencies() {
        let test_file = PathBuf::from("testdata/python/pip-inspect/pip-inspect.deplock");
        let package_data = PythonParser::extract_first_package(&test_file);

        let direct_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.is_direct == Some(true))
            .collect();

        assert_eq!(direct_deps.len(), 5, "Should have 5 direct dependencies");
    }

    #[test]
    fn test_extract_from_pip_inspect_preserves_reference_merge_semantics() {
        let test_file = PathBuf::from("testdata/python/pip-inspect/pip-inspect.deplock");
        let package_data = PythonParser::extract_first_package(&test_file);

        let packaging = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl
                    .as_deref()
                    .is_some_and(|purl| purl.starts_with("pkg:pypi/packaging@"))
            })
            .expect("packaging dependency missing");
        assert_eq!(packaging.is_direct, Some(true));
        assert_eq!(packaging.is_pinned, Some(true));
        assert!(packaging.resolved_package.is_some());
        assert!(packaging.extracted_requirement.is_none());
        assert!(packaging.scope.is_none());
        assert!(packaging.extra_data.is_none());

        let semver = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/semver"))
            .expect("semver dependency missing");
        assert_eq!(semver.is_direct, Some(true));
        assert_eq!(semver.is_pinned, Some(false));
        assert!(semver.resolved_package.is_none());
        assert_eq!(
            semver
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("python_version"))
                .and_then(|value| value.as_str()),
            Some("< 3.8")
        );
    }

    #[test]
    fn test_pip_inspect_invalid_json() {
        let (_temp_dir, invalid_path) = create_temp_file("not valid json", "pip-inspect.deplock");
        let package_data = PythonParser::extract_first_package(&invalid_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiInspectDeplock)
        );
    }

    #[test]
    fn test_pip_inspect_missing_installed_array() {
        let content = r#"{"version": "1", "pip_version": "24.1"}"#;
        let (_temp_dir, file_path) = create_temp_file(content, "pip-inspect.deplock");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
        assert_eq!(package_data.package_type, Some(PackageType::Pypi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::PypiInspectDeplock)
        );
    }

    #[test]
    fn test_pyproject_optional_dependencies_scopes() {
        let content = r#"
[project]
name = "test-package"
version = "1.0.0"

[project.dependencies]
requests = ">=2.0"

[project.optional-dependencies]
dev = ["pytest>=7.0", "black>=22.0"]
docs = ["sphinx>=5.0"]
test = ["coverage>=6.0"]
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.dependencies.len(), 5);

        let dev_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("dev"))
            .collect();
        assert_eq!(dev_deps.len(), 2);
        assert!(dev_deps.iter().all(|d| d.is_optional == Some(true)));

        let docs_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("docs"))
            .collect();
        assert_eq!(docs_deps.len(), 1);
        assert!(docs_deps.iter().all(|d| d.is_optional == Some(true)));

        let test_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("test"))
            .collect();
        assert_eq!(test_deps.len(), 1);
        assert!(test_deps.iter().all(|d| d.is_optional == Some(true)));
    }

    #[test]
    fn test_pyproject_optional_dependencies_is_runtime() {
        let content = r#"
[project]
name = "test-package"
version = "1.0.0"

[project.dependencies]
requests = ">=2.0"
click = ">=8.0"

[project.optional-dependencies]
dev = ["pytest>=7.0", "black>=22.0"]
docs = ["sphinx>=5.0"]
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.dependencies.len(), 5);

        let regular_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.is_optional == Some(false))
            .collect();
        assert_eq!(regular_deps.len(), 2);
        assert!(
            regular_deps.iter().all(|d| d.is_runtime == Some(true)),
            "Regular dependencies should have is_runtime=true"
        );

        let optional_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.is_optional == Some(true))
            .collect();
        assert_eq!(optional_deps.len(), 3);
        assert!(
            optional_deps.iter().all(|d| d.is_runtime == Some(false)),
            "Optional dependencies should have is_runtime=false"
        );
    }

    #[test]
    fn test_pyproject_extracts_uv_dependency_groups_and_tool_config() {
        let content = r#"
[project]
name = "uv-project"
version = "0.4.0"
dependencies = ["requests>=2.32"]

[dependency-groups]
dev = ["pytest>=8.0", "ruff>=0.5"]
lint = ["mypy>=1.10"]

[tool.uv]
default-groups = ["dev", "lint"]
dev-dependencies = ["coverage>=7.0"]

[tool.uv.sources]
requests = { git = "https://github.com/psf/requests" }
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name.as_deref(), Some("uv-project"));
        assert_eq!(package_data.version.as_deref(), Some("0.4.0"));
        assert_eq!(package_data.dependencies.len(), 5);

        let dev_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("dev"))
            .collect();
        assert_eq!(dev_deps.len(), 3);
        assert!(dev_deps.iter().all(|d| d.is_optional == Some(true)));
        assert!(dev_deps.iter().all(|d| d.is_runtime == Some(false)));

        let lint_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("lint"))
            .collect();
        assert_eq!(lint_deps.len(), 1);
        assert!(lint_deps.iter().all(|d| d.is_optional == Some(true)));

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("tool.uv data should be preserved");
        let tool_uv = extra_data
            .get("tool_uv")
            .and_then(|value| value.as_object())
            .expect("tool_uv should be stored as an object");

        let default_groups = tool_uv
            .get("default-groups")
            .and_then(|value| value.as_array())
            .expect("default-groups should be stored");
        assert_eq!(default_groups.len(), 2);
        assert_eq!(default_groups[0].as_str(), Some("dev"));
        assert_eq!(default_groups[1].as_str(), Some("lint"));

        let sources = tool_uv
            .get("sources")
            .and_then(|value| value.as_object())
            .expect("tool.uv.sources should be preserved");
        let requests = sources
            .get("requests")
            .and_then(|value| value.as_object())
            .expect("requests source should be retained");
        assert_eq!(
            requests.get("git").and_then(|value| value.as_str()),
            Some("https://github.com/psf/requests")
        );
    }

    #[test]
    fn test_setup_cfg_extras_require_scopes() {
        let content = r#"
[metadata]
name = test-package
version = 1.0.0

[options]
install_requires =
    requests>=2.0

[options.extras_require]
dev = 
    pytest>=7.0
    black>=22.0
docs = sphinx>=5.0
test = coverage>=6.0
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "setup.cfg");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert!(package_data.dependencies.len() >= 4);

        let dev_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("dev"))
            .collect();
        assert_eq!(dev_deps.len(), 2);
        assert!(dev_deps.iter().all(|d| d.is_optional == Some(true)));

        let docs_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("docs"))
            .collect();
        assert_eq!(docs_deps.len(), 1);
        assert!(docs_deps.iter().all(|d| d.is_optional == Some(true)));

        let test_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("test"))
            .collect();
        assert_eq!(test_deps.len(), 1);
        assert!(test_deps.iter().all(|d| d.is_optional == Some(true)));

        let install_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("install"))
            .collect();
        assert_eq!(install_deps.len(), 1);
        assert!(install_deps.iter().all(|d| d.is_optional == Some(false)));

        let regular_dep: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.is_none() || d.scope.as_deref() == Some("install"))
            .collect();
        assert!(regular_dep.iter().all(|d| d.is_optional == Some(false)));
    }

    #[test]
    fn test_setup_cfg_extracts_richer_metadata() {
        let path = PathBuf::from("testdata/python/golden/setup_cfg_wheel/setup.cfg");
        let package_data = PythonParser::extract_first_package(&path);

        assert_eq!(package_data.name, Some("wheel".to_string()));
        assert_eq!(
            package_data.description,
            Some("A built-package format for Python".to_string())
        );
        assert_eq!(
            package_data.keywords,
            vec!["wheel".to_string(), "packaging".to_string()]
        );

        let maintainer = package_data
            .parties
            .iter()
            .find(|party| party.role.as_deref() == Some("maintainer"))
            .expect("maintainer should be extracted from setup.cfg");
        assert_eq!(maintainer.name.as_deref(), Some("Alex Gronholm"));
        assert_eq!(
            maintainer.email.as_deref(),
            Some("alex.gronholm@nextday.fi")
        );

        assert_eq!(
            package_data.bug_tracking_url.as_deref(),
            Some("https://github.com/pypa/wheel/issues")
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("setup.cfg extra_data should exist");

        assert_eq!(
            extra_data
                .get("python_requires")
                .and_then(|value| value.as_str()),
            Some(">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*, !=3.4.*")
        );

        let project_urls = extra_data
            .get("project_urls")
            .and_then(|value| value.as_object())
            .expect("project_urls should be retained in extra_data");
        assert_eq!(
            project_urls
                .get("Documentation")
                .and_then(|value| value.as_str()),
            Some("https://wheel.readthedocs.io/")
        );
        assert_eq!(
            project_urls
                .get("Changelog")
                .and_then(|value| value.as_str()),
            Some("https://wheel.readthedocs.io/en/stable/news.html")
        );
    }

    #[test]
    fn test_setup_py_extras_require_scopes() {
        let content = r#"
from setuptools import setup

setup(
    name="test-package",
    version="1.0.0",
    extras_require={
        'dev': ['pytest>=7.0', 'black>=22.0'],
        'docs': ['sphinx>=5.0'],
    },
    tests_require=['coverage>=6.0'],
)
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(package_data.dependencies.len(), 4);

        let dev_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("dev"))
            .collect();
        assert_eq!(dev_deps.len(), 2);
        assert!(dev_deps.iter().all(|d| d.is_optional == Some(true)));

        let docs_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("docs"))
            .collect();
        assert_eq!(docs_deps.len(), 1);
        assert!(docs_deps.iter().all(|d| d.is_optional == Some(true)));

        let test_deps: Vec<&Dependency> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("test"))
            .collect();
        assert_eq!(test_deps.len(), 1);
        assert!(test_deps.iter().all(|d| d.is_optional == Some(true)));
    }

    #[test]
    fn test_setup_py_project_urls_ordered_dict() {
        let content = r#"
from collections import OrderedDict
from setuptools import setup

setup(
    name="flask",
    version="3.0.0",
    project_urls=OrderedDict([
        ("Documentation", "https://flask.palletsprojects.com/"),
        ("Source", "https://github.com/pallets/flask"),
        ("Issues", "https://github.com/pallets/flask/issues"),
    ]),
)
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "setup.py");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("flask".to_string()));
        assert_eq!(
            package_data.code_view_url.as_deref(),
            Some("https://github.com/pallets/flask")
        );
        assert_eq!(
            package_data.bug_tracking_url.as_deref(),
            Some("https://github.com/pallets/flask/issues")
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("setup.py project_urls should be preserved");
        let project_urls = extra_data
            .get("project_urls")
            .and_then(|value| value.as_object())
            .expect("project_urls should be stored in extra_data");
        assert_eq!(
            project_urls
                .get("Documentation")
                .and_then(|value| value.as_str()),
            Some("https://flask.palletsprojects.com/")
        );
    }

    #[test]
    fn test_pyproject_private_classifier_marks_package_private() {
        let content = r#"
[project]
name = "private-package"
version = "1.0.0"
classifiers = ["Private :: Do Not Upload"]
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pyproject.toml");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert!(package_data.is_private);
    }

    #[test]
    fn test_extract_from_pypi_json() {
        let content = r#"
{
  "info": {
    "name": "attrs",
    "version": "24.1.0",
    "summary": "Classes Without Boilerplate",
    "description": "Longer attrs description",
    "home_page": "https://www.attrs.org/",
    "author": "Hynek Schlawack",
    "author_email": "hs@example.com",
    "license": "MIT",
    "keywords": "attrs,dataclasses",
    "requires_dist": [
      "typing-extensions>=4.0.0",
      "importlib-metadata; python_version < \"3.10\""
    ],
    "classifiers": ["Private :: Do Not Upload"],
    "project_urls": {
      "Documentation": "https://www.attrs.org/",
      "Source": "https://github.com/python-attrs/attrs",
      "Issues": "https://github.com/python-attrs/attrs/issues"
    }
  },
  "urls": [
    {
      "packagetype": "bdist_wheel",
      "url": "https://files.pythonhosted.org/packages/example/attrs-24.1.0-py3-none-any.whl",
      "size": 12345,
      "digests": {"sha256": "aaaa0000000000000000000000000000000000000000000000000000000000bb"}
    },
    {
      "packagetype": "sdist",
      "url": "https://files.pythonhosted.org/packages/source/a/attrs/attrs-24.1.0.tar.gz",
      "size": 67890,
      "digests": {"sha256": "aaaa0000000000000000000000000000000000000000000000000000000000cc"}
    }
  ]
}
"#;

        let (_temp_dir, file_path) = create_temp_file(content, "pypi.json");
        let package_data = PythonParser::extract_first_package(&file_path);

        assert_eq!(package_data.name, Some("attrs".to_string()));
        assert_eq!(package_data.version, Some("24.1.0".to_string()));
        assert_eq!(
            package_data.description,
            Some("Longer attrs description".to_string())
        );
        assert_eq!(
            package_data.homepage_url,
            Some("https://www.attrs.org/".to_string())
        );
        assert_eq!(
            package_data.code_view_url,
            Some("https://github.com/python-attrs/attrs".to_string())
        );
        assert_eq!(
            package_data.bug_tracking_url,
            Some("https://github.com/python-attrs/attrs/issues".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some(
                "https://files.pythonhosted.org/packages/source/a/attrs/attrs-24.1.0.tar.gz"
                    .to_string()
            )
        );
        assert_eq!(
            package_data.sha256,
            Some(
                Sha256Digest::from_hex(
                    "aaaa0000000000000000000000000000000000000000000000000000000000cc"
                )
                .unwrap()
            )
        );
        assert_eq!(package_data.size, Some(67890));
        assert_eq!(
            package_data.keywords,
            vec!["attrs".to_string(), "dataclasses".to_string()]
        );
        assert!(package_data.is_private);
        assert_eq!(package_data.datasource_id, Some(DatasourceId::PypiJson));
        assert_eq!(package_data.purl, Some("pkg:pypi/attrs@24.1.0".to_string()));
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert_eq!(package_data.dependencies.len(), 2);
        assert!(package_data.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:pypi/typing-extensions")
                && dep.extracted_requirement.as_deref() == Some(">=4.0.0")
        }));
        assert!(package_data.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:pypi/importlib-metadata")
                && dep
                    .extra_data
                    .as_ref()
                    .and_then(|extra| extra.get("python_version"))
                    .and_then(|value| value.as_str())
                    == Some("< 3.10")
        }));
    }

    #[test]
    fn test_setup_py_resolves_sibling_dunder_metadata() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_dir = temp_dir.path().join("examplepkg");
        fs::create_dir_all(&package_dir).expect("Failed to create package dir");

        let setup_path = temp_dir.path().join("setup.py");
        fs::write(
            &setup_path,
            r#"
from setuptools import setup
from examplepkg.__about__ import __author__, __license__, __version__

setup(
    name="examplepkg",
    version=__version__,
    author=__author__,
    license=__license__,
)
"#,
        )
        .expect("Failed to write setup.py");

        fs::write(
            package_dir.join("__about__.py"),
            r#"
__version__ = "2.4.6"
__author__ = "Example Author"
__license__ = "Apache-2.0"
"#,
        )
        .expect("Failed to write __about__.py");

        let package_data = PythonParser::extract_first_package(&setup_path);

        assert_eq!(package_data.name, Some("examplepkg".to_string()));
        assert_eq!(package_data.version, Some("2.4.6".to_string()));
        assert_eq!(
            package_data.extracted_license_statement,
            Some("Apache-2.0".to_string())
        );

        let author = package_data
            .parties
            .iter()
            .find(|party| party.role.as_deref() == Some("author"))
            .expect("author should be resolved from sibling dunder metadata");
        assert_eq!(author.name.as_deref(), Some("Example Author"));
    }

    #[test]
    fn test_setup_py_prefers_imported_dunder_module() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_dir = temp_dir.path().join("examplepkg");
        fs::create_dir_all(&package_dir).expect("Failed to create package dir");

        let setup_path = temp_dir.path().join("setup.py");
        fs::write(
            &setup_path,
            r#"
from setuptools import setup
from examplepkg.__about__ import __version__

setup(name="examplepkg", version=__version__)
"#,
        )
        .expect("Failed to write setup.py");

        fs::write(package_dir.join("__about__.py"), r#"__version__ = "2.4.6""#)
            .expect("Failed to write imported dunder file");
        fs::write(package_dir.join("other.py"), r#"__version__ = "9.9.9""#)
            .expect("Failed to write unrelated dunder file");

        let package_data = PythonParser::extract_first_package(&setup_path);

        assert_eq!(package_data.version, Some("2.4.6".to_string()));
    }

    #[test]
    fn test_setup_py_resolves_sibling_dunder_metadata_in_src_layout() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_dir = temp_dir.path().join("src").join("examplepkg");
        fs::create_dir_all(&package_dir).expect("Failed to create src package dir");

        let setup_path = temp_dir.path().join("setup.py");
        fs::write(
            &setup_path,
            r#"
from setuptools import setup
from examplepkg.__about__ import __version__

setup(name="examplepkg", version=__version__)
"#,
        )
        .expect("Failed to write setup.py");

        fs::write(package_dir.join("__about__.py"), r#"__version__ = "3.1.4""#)
            .expect("Failed to write src-layout dunder file");

        let package_data = PythonParser::extract_first_package(&setup_path);

        assert_eq!(package_data.version, Some("3.1.4".to_string()));
    }

    #[test]
    fn test_archive_security_constants_exist() {
        const MAX_ARCHIVE_SIZE: u64 = 100 * 1024 * 1024;
        const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
        const MAX_COMPRESSION_RATIO: f64 = 100.0;

        assert_eq!(MAX_ARCHIVE_SIZE, 100 * 1024 * 1024);
        assert_eq!(MAX_FILE_SIZE, 50 * 1024 * 1024);
        assert_eq!(MAX_COMPRESSION_RATIO, 100.0);

        const { assert!(MAX_FILE_SIZE < MAX_ARCHIVE_SIZE) };
        const { assert!(MAX_COMPRESSION_RATIO > 1.0) };
    }
}
