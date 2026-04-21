// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use serde_json::Value as JsonValue;

    #[test]
    fn test_python_metadata_scan_assigns_referenced_site_packages_files() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let site_packages = temp_dir.path().join("venv/lib/python3.11/site-packages");
        let dist_info = site_packages.join("click-8.0.4.dist-info");
        let package_dir = site_packages.join("click");

        std::fs::create_dir_all(&dist_info).expect("create dist-info dir");
        std::fs::create_dir_all(&package_dir).expect("create package dir");
        std::fs::write(
            dist_info.join("METADATA"),
            "Metadata-Version: 2.1\nName: click\nVersion: 8.0.4\n",
        )
        .unwrap();
        std::fs::write(
            dist_info.join("RECORD"),
            "click/__init__.py,,0\nclick/core.py,,10\nclick-8.0.4.dist-info/LICENSE.rst,,20\n",
        )
        .unwrap();
        std::fs::write(dist_info.join("LICENSE.rst"), "license text").unwrap();
        std::fs::write(package_dir.join("__init__.py"), "").unwrap();
        std::fs::write(package_dir.join("core.py"), "def click():\n    pass\n").unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("click"))
            .unwrap();
        let core_file = files
            .iter()
            .find(|file| file.path.ends_with("site-packages/click/core.py"))
            .unwrap();
        let license_file = files
            .iter()
            .find(|file| {
                file.path
                    .ends_with("site-packages/click-8.0.4.dist-info/LICENSE.rst")
            })
            .unwrap();
        assert!(core_file.for_packages.contains(&package.package_uid));
        assert!(license_file.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_python_pkg_info_scan_assigns_installed_files_entries() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let site_packages = temp_dir.path().join("venv/lib/python3.11/site-packages");
        let egg_info = site_packages.join("examplepkg.egg-info");
        let package_dir = site_packages.join("examplepkg");

        std::fs::create_dir_all(&egg_info).unwrap();
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            egg_info.join("PKG-INFO"),
            "Metadata-Version: 1.2\nName: examplepkg\nVersion: 1.0.0\n",
        )
        .unwrap();
        std::fs::write(
            egg_info.join("installed-files.txt"),
            "../examplepkg/__init__.py\n../examplepkg/core.py\n",
        )
        .unwrap();
        std::fs::write(package_dir.join("__init__.py"), "").unwrap();
        std::fs::write(package_dir.join("core.py"), "VALUE = 1\n").unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("examplepkg"))
            .unwrap();
        let core_file = files
            .iter()
            .find(|file| file.path.ends_with("site-packages/examplepkg/core.py"))
            .unwrap();
        assert!(core_file.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_python_pkg_info_scan_assigns_sources_entries() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let egg_info = temp_dir.path().join("PyJPString.egg-info");
        let package_dir = temp_dir.path().join("jpstring");

        std::fs::create_dir_all(&egg_info).unwrap();
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            egg_info.join("PKG-INFO"),
            "Metadata-Version: 1.0\nName: PyJPString\nVersion: 0.0.3\n",
        )
        .unwrap();
        std::fs::write(
            egg_info.join("SOURCES.txt"),
            "setup.py\nPyJPString.egg-info/PKG-INFO\nPyJPString.egg-info/top_level.txt\njpstring/__init__.py\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("setup.py"),
            "from setuptools import setup\n",
        )
        .unwrap();
        std::fs::write(egg_info.join("top_level.txt"), "jpstring\n").unwrap();
        std::fs::write(package_dir.join("__init__.py"), "").unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("PyJPString"))
            .unwrap();
        let setup_file = files
            .iter()
            .find(|file| file.path.ends_with("setup.py"))
            .unwrap();
        let module_init = files
            .iter()
            .find(|file| file.path.ends_with("jpstring/__init__.py"))
            .unwrap();
        let top_level = files
            .iter()
            .find(|file| file.path.ends_with("PyJPString.egg-info/top_level.txt"))
            .unwrap();
        assert!(setup_file.for_packages.contains(&package.package_uid));
        assert!(module_init.for_packages.contains(&package.package_uid));
        assert!(top_level.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_python_wheel_origin_scan_assembles_distribution_and_origin_metadata() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let cache_dir = temp_dir.path().join(".cache/pip/wheels/eb/60/37/cachehash");
        std::fs::create_dir_all(&cache_dir).expect("create pip cache dir");
        std::fs::copy(
            "testdata/python/golden/pip_cache/wheels/construct/construct-2.10.68-py3-none-any.whl",
            cache_dir.join("construct-2.10.68-py3-none-any.whl"),
        )
        .expect("copy wheel fixture");
        std::fs::copy(
            "testdata/python/golden/pip_cache/wheels/construct/origin.json",
            cache_dir.join("origin.json"),
        )
        .expect("copy origin fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("construct"))
            .expect("construct package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Pypi));
        assert_eq!(package.version.as_deref(), Some("2.10.68"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:pypi/construct@2.10.68?extension=py3-none-any")
        );
        assert_file_links_to_package(
            &files,
            "/construct-2.10.68-py3-none-any.whl",
            &package.package_uid,
            DatasourceId::PypiWheel,
        );
        assert_file_links_to_package(
            &files,
            "/origin.json",
            &package.package_uid,
            DatasourceId::PypiPipOriginJson,
        );
    }

    #[test]
    fn test_python_pip_inspect_scan_assembles_with_pyproject() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"[project]
name = "univers"
version = "0.0.0"
"#,
        )
        .expect("write pyproject.toml");
        fs::copy(
            "testdata/python/pip-inspect/pip-inspect.deplock",
            temp_dir.path().join("pip-inspect.deplock"),
        )
        .expect("copy pip-inspect fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("univers"))
            .expect("pyproject + pip-inspect should assemble univers package");

        assert_eq!(package.package_type, Some(PackageType::Pypi));
        assert_eq!(package.version.as_deref(), Some("0.0.0"));
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PypiInspectDeplock)
        );
        assert_file_links_to_package(
            &files,
            "/pip-inspect.deplock",
            &package.package_uid,
            DatasourceId::PypiInspectDeplock,
        );
    }

    #[test]
    fn test_python_requirements_subdir_scan_assigns_to_project_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let requirements_dir = temp_dir.path().join("requirements");
        fs::create_dir_all(&requirements_dir).expect("create requirements dir");

        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"[project]
name = "req-demo"
version = "1.0.0"
"#,
        )
        .expect("write pyproject.toml");
        fs::write(requirements_dir.join("dev.txt"), "pytest==8.3.5\n")
            .expect("write requirements/dev.txt");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("req-demo"))
            .expect("pyproject should assemble into a Python package");

        assert_eq!(package.package_type, Some(PackageType::Pypi));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::PipRequirements)
        );
        assert!(
            package
                .datafile_paths
                .iter()
                .any(|path| path.ends_with("requirements/dev.txt"))
        );
        assert_file_links_to_package(
            &files,
            "/requirements/dev.txt",
            &package.package_uid,
            DatasourceId::PipRequirements,
        );
        assert!(result.dependencies.iter().any(|dep| {
            dep.datafile_path.ends_with("requirements/dev.txt")
                && dep.purl.is_some()
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
    }

    #[test]
    fn test_python_nested_requirements_subdir_scan_assigns_to_project_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let requirements_dir = temp_dir.path().join("requirements/compiled");
        fs::create_dir_all(&requirements_dir).expect("create nested requirements dir");

        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"[project]
name = "req-demo"
version = "1.0.0"
"#,
        )
        .expect("write pyproject.toml");
        fs::write(requirements_dir.join("black.txt"), "black==24.10.0\n")
            .expect("write requirements/compiled/black.txt");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("req-demo"))
            .expect("pyproject should assemble into a Python package");

        assert!(
            package
                .datafile_paths
                .iter()
                .any(|path| path.ends_with("requirements/compiled/black.txt"))
        );
        assert_file_links_to_package(
            &files,
            "/requirements/compiled/black.txt",
            &package.package_uid,
            DatasourceId::PipRequirements,
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:pypi/black@24.10.0",
            "requirements/compiled/black.txt",
        );
        assert!(result.dependencies.iter().any(|dep| {
            dep.datafile_path
                .ends_with("requirements/compiled/black.txt")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
    }

    #[test]
    fn test_python_requirements_named_subdir_scan_discovers_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let requirements_dir = temp_dir.path().join("test-data/requirements-txt");
        fs::create_dir_all(&requirements_dir).expect("create requirements-txt dir");
        fs::write(requirements_dir.join("basic.txt"), "httpx==0.27.0\n")
            .expect("write requirements-txt/basic.txt");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let requirements_file = files
            .iter()
            .find(|file| file.path.ends_with("test-data/requirements-txt/basic.txt"))
            .expect("requirements-txt/basic.txt should be scanned");

        let package_data = requirements_file
            .package_data
            .iter()
            .find(|package_data| package_data.datasource_id == Some(DatasourceId::PipRequirements))
            .expect("requirements-txt/basic.txt should have PipRequirements package data");

        assert!(
            package_data
                .dependencies
                .iter()
                .any(|dependency| dependency.purl.as_deref() == Some("pkg:pypi/httpx@0.27.0"))
        );
        assert!(result.packages.is_empty());
        assert_dependency_present(
            &result.dependencies,
            "pkg:pypi/httpx@0.27.0",
            "requirements-txt/basic.txt",
        );
        assert!(result.dependencies.iter().any(|dependency| {
            dependency
                .datafile_path
                .ends_with("requirements-txt/basic.txt")
                && dependency.for_package_uid.is_none()
        }));
    }

    #[test]
    fn test_python_standalone_min_requirements_scan_hoists_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("min_requirements.txt"),
            "Sphinx==3.4.3\njsonschema==4.*\n",
        )
        .expect("write min_requirements.txt");

        let (_files, result) = scan_and_assemble(temp_dir.path());

        assert!(result.packages.is_empty());
        assert_dependency_present(
            &result.dependencies,
            "pkg:pypi/sphinx@3.4.3",
            "min_requirements.txt",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:pypi/jsonschema@4.%2A",
            "min_requirements.txt",
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|dependency| dependency.for_package_uid.is_none())
        );
    }

    #[test]
    fn test_python_standalone_reqs_filename_scan_hoists_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("mkdocs-reqs.txt"),
            "mkdocs-material~=8.2\nmkdocs~=1.3\n",
        )
        .expect("write mkdocs-reqs.txt");

        let (_files, result) = scan_and_assemble(temp_dir.path());

        assert!(result.packages.is_empty());
        assert_dependency_present(
            &result.dependencies,
            "pkg:pypi/mkdocs-material",
            "mkdocs-reqs.txt",
        );
        assert_dependency_present(&result.dependencies, "pkg:pypi/mkdocs", "mkdocs-reqs.txt");
        assert!(
            result
                .dependencies
                .iter()
                .all(|dependency| dependency.for_package_uid.is_none())
        );
    }

    #[test]
    fn test_python_standalone_minreqs_filename_scan_hoists_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("minreqs.txt"),
            "pytest==6.0.2\nqemu.qmp==0.0.5\n",
        )
        .expect("write minreqs.txt");

        let (_files, result) = scan_and_assemble(temp_dir.path());

        assert!(result.packages.is_empty());
        assert_dependency_present(&result.dependencies, "pkg:pypi/pytest@6.0.2", "minreqs.txt");
        assert_dependency_present(
            &result.dependencies,
            "pkg:pypi/qemu-qmp@0.0.5",
            "minreqs.txt",
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|dependency| dependency.for_package_uid.is_none())
        );
    }

    #[test]
    fn test_python_pyproject_scan_preserves_dependency_requirement_shapes() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"[project]
name = "array-demo"
version = "1.0.0"
dependencies = [
    "requests>=2.32",
    "mypy==1.19.1",
    'ninja; sys_platform != "emscripten"',
]

[project.optional-dependencies]
dev = ["typing_extensions", "helper[cli]==1.2.3"]
"#,
        )
        .expect("write pyproject.toml");

        let (_files, result) = scan_and_assemble(temp_dir.path());
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("array-demo"))
            .expect("pyproject should assemble into a Python package");

        let requests = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/requests")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("requests dependency");
        assert_eq!(requests.extracted_requirement.as_deref(), Some(">=2.32"));
        assert_eq!(requests.is_pinned, Some(false));

        let mypy = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/mypy@1.19.1")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("mypy dependency");
        assert_eq!(mypy.extracted_requirement.as_deref(), Some("==1.19.1"));
        assert_eq!(mypy.is_pinned, Some(true));

        let ninja = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/ninja")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("ninja dependency");
        let ninja_extra = ninja.extra_data.as_ref().expect("ninja marker data");
        assert_eq!(ninja.extracted_requirement, None);
        assert_eq!(
            ninja_extra.get("marker"),
            Some(&JsonValue::String(
                "sys_platform != \"emscripten\"".to_string()
            ))
        );

        let helper = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/helper@1.2.3")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("helper dependency");
        let helper_extra = helper.extra_data.as_ref().expect("helper extras data");
        assert_eq!(helper.scope.as_deref(), Some("dev"));
        assert_eq!(helper.is_runtime, Some(false));
        assert_eq!(helper.is_optional, Some(true));
        assert_eq!(helper.extracted_requirement.as_deref(), Some("==1.2.3"));
        assert_eq!(helper.is_pinned, Some(true));
        assert_eq!(
            helper_extra.get("extras"),
            Some(&JsonValue::Array(vec![JsonValue::String(
                "cli".to_string()
            )]))
        );
    }

    #[test]
    fn test_python_pyproject_scan_attaches_poetry_group_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"[project]
name = "poetry-groups-demo"
version = "1.0.0"
dependencies = ["requests>=2.32"]

[tool.poetry]
requires-poetry = ">=2.0"

[tool.poetry.group.dev.dependencies]
pre-commit = ">=3.0"

[tool.poetry.group.test.dependencies]
pytest = ">=8.0"

[tool.poetry.group.github-actions]
optional = true

[tool.poetry.group.github-actions.dependencies]
pytest-github-actions-annotate-failures = "^0.1.7"
"#,
        )
        .expect("write pyproject.toml");

        let (_files, result) = scan_and_assemble(temp_dir.path());
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("poetry-groups-demo"))
            .expect("pyproject should assemble into a Python package");

        let runtime = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/requests")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("runtime dependency");
        assert_eq!(runtime.scope, None);
        assert_eq!(runtime.is_runtime, Some(true));
        assert_eq!(runtime.is_optional, Some(false));

        let pre_commit = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/pre-commit")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("dev dependency");
        assert_eq!(pre_commit.scope.as_deref(), Some("dev"));
        assert_eq!(pre_commit.is_runtime, Some(false));
        assert_eq!(pre_commit.is_optional, Some(true));

        let pytest = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/pytest")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("test dependency");
        assert_eq!(pytest.scope.as_deref(), Some("test"));
        assert_eq!(pytest.is_runtime, Some(false));
        assert_eq!(pytest.is_optional, Some(true));

        let gha = result
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:pypi/pytest-github-actions-annotate-failures")
                    && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
            })
            .expect("github-actions dependency");
        assert_eq!(gha.scope.as_deref(), Some("github-actions"));
        assert_eq!(gha.is_runtime, Some(false));
        assert_eq!(gha.is_optional, Some(true));
    }
}
