// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::PackageParser;
use super::maven::*;
use crate::models::DatasourceId;
use crate::models::PackageType;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_osgi_basic_bundle_detection() {
    let path = PathBuf::from("testdata/osgi/basic/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Osgi));
    assert_eq!(package.datasource_id, Some(DatasourceId::JavaOsgiManifest));
    assert_eq!(package.name, Some("org.example.mybundle".to_string()));
    assert_eq!(package.version, Some("1.2.3".to_string()));
}

#[test]
fn test_osgi_basic_bundle_metadata() {
    let path = PathBuf::from("testdata/osgi/basic/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert_eq!(
        package.description,
        Some("A comprehensive example OSGi bundle".to_string())
    );
    assert_eq!(
        package.homepage_url,
        Some("https://example.org/mybundle".to_string())
    );
    assert_eq!(
        package.extracted_license_statement,
        Some("https://www.apache.org/licenses/LICENSE-2.0".to_string())
    );

    assert_eq!(package.parties.len(), 1);
    assert_eq!(package.parties[0].name, Some("Example Corp".to_string()));
    assert_eq!(package.parties[0].role, Some("vendor".to_string()));
}

#[test]
fn test_osgi_basic_bundle_purl() {
    let path = PathBuf::from("testdata/osgi/basic/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert_eq!(
        package.purl,
        Some("pkg:osgi/org.example.mybundle@1.2.3".to_string())
    );
}

#[test]
fn test_osgi_import_package_dependencies() {
    let path = PathBuf::from("testdata/osgi/basic/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    let import_deps: Vec<_> = package
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("import"))
        .collect();

    assert_eq!(import_deps.len(), 2);

    let osgi_dep = import_deps
        .iter()
        .find(|d| d.purl.as_deref() == Some("pkg:osgi/org.osgi.framework"));
    assert!(osgi_dep.is_some());
    let osgi_dep = osgi_dep.unwrap();
    assert_eq!(osgi_dep.extracted_requirement, Some("[1.6,2)".to_string()));
    assert_eq!(osgi_dep.is_runtime, Some(true));
    assert_eq!(osgi_dep.is_optional, Some(false));

    let servlet_dep = import_deps
        .iter()
        .find(|d| d.purl.as_deref() == Some("pkg:osgi/javax.servlet"));
    assert!(servlet_dep.is_some());
    let servlet_dep = servlet_dep.unwrap();
    assert_eq!(
        servlet_dep.extracted_requirement,
        Some("[3.0,4)".to_string())
    );
}

#[test]
fn test_osgi_require_bundle_dependencies() {
    let path = PathBuf::from("testdata/osgi/basic/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    let require_deps: Vec<_> = package
        .dependencies
        .iter()
        .filter(|d| d.scope.as_deref() == Some("require-bundle"))
        .collect();

    assert_eq!(require_deps.len(), 1);

    let runtime_dep = &require_deps[0];
    assert_eq!(
        runtime_dep.purl,
        Some("pkg:osgi/org.eclipse.core.runtime".to_string())
    );
    assert_eq!(runtime_dep.extracted_requirement, Some("3.7.0".to_string()));
    assert_eq!(runtime_dep.is_runtime, Some(true));
    assert_eq!(runtime_dep.is_optional, Some(false));
}

#[test]
fn test_osgi_export_package_extra_data() {
    let path = PathBuf::from("testdata/osgi/basic/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert!(package.extra_data.is_some());
    let extra_data = package.extra_data.unwrap();
    assert!(extra_data.contains_key("export_packages"));
    assert_eq!(
        extra_data.get("export_packages"),
        Some(&serde_json::Value::String(
            "org.example.mybundle;version=\"1.2.3\"".to_string()
        ))
    );
}

#[test]
fn test_osgi_minimal_bundle() {
    let path = PathBuf::from("testdata/osgi/minimal/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Osgi));
    assert_eq!(package.name, Some("com.simple.bundle".to_string()));
    assert_eq!(package.version, Some("0.1.0".to_string()));
    assert_eq!(
        package.purl,
        Some("pkg:osgi/com.simple.bundle@0.1.0".to_string())
    );
}

#[test]
fn test_osgi_bundle_symbolic_name_with_directives() {
    let path = PathBuf::from("testdata/osgi/directive/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Osgi));
    assert_eq!(package.name, Some("com.example.mybundle".to_string()));
    assert_eq!(package.version, Some("2.1.0".to_string()));
}

#[test]
fn test_non_osgi_manifest_stays_maven() {
    let path = PathBuf::from("testdata/osgi/non-osgi/META-INF/MANIFEST.MF");
    let package = MavenParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Maven));
    assert_eq!(package.datasource_id, Some(DatasourceId::JavaJarManifest));
    assert_eq!(package.name, Some("spring-web".to_string()));
    assert_eq!(package.version, Some("5.3.20".to_string()));

    assert_eq!(package.parties.len(), 1);
    assert_eq!(
        package.parties[0].name,
        Some("Spring Framework".to_string())
    );
}

#[test]
fn test_split_osgi_list_simple() {
    let list = "org.osgi.framework,javax.servlet,javax.sql";
    let result = split_osgi_list(list);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], "org.osgi.framework");
    assert_eq!(result[1], "javax.servlet");
    assert_eq!(result[2], "javax.sql");
}

#[test]
fn test_split_osgi_list_with_quoted_commas() {
    let list = "org.osgi.framework;version=\"[1.6,2)\",javax.servlet;version=\"[3.0,4)\"";
    let result = split_osgi_list(list);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "org.osgi.framework;version=\"[1.6,2)\"");
    assert_eq!(result[1], "javax.servlet;version=\"[3.0,4)\"");
}

#[test]
fn test_extract_osgi_version_quoted() {
    let entry = "org.osgi.framework;version=\"[1.6,2)\"";
    let version = extract_osgi_version(entry);
    assert_eq!(version, Some("[1.6,2)".to_string()));
}

#[test]
fn test_extract_osgi_version_unquoted() {
    let entry = "org.osgi.framework;version=1.6.0";
    let version = extract_osgi_version(entry);
    assert_eq!(version, Some("1.6.0".to_string()));
}

#[test]
fn test_extract_osgi_bundle_version() {
    let entry = "org.eclipse.core.runtime;bundle-version=\"3.7.0\"";
    let version = extract_osgi_bundle_version(entry);
    assert_eq!(version, Some("3.7.0".to_string()));
}

#[test]
fn test_parse_osgi_package_list() {
    let list = "org.osgi.framework;version=\"[1.6,2)\",javax.servlet;version=\"3.0\"";
    let deps = parse_osgi_package_list(list, "import");

    assert_eq!(deps.len(), 2);
    assert_eq!(
        deps[0].purl,
        Some("pkg:osgi/org.osgi.framework".to_string())
    );
    assert_eq!(deps[0].extracted_requirement, Some("[1.6,2)".to_string()));
    assert_eq!(deps[0].scope, Some("import".to_string()));

    assert_eq!(deps[1].purl, Some("pkg:osgi/javax.servlet".to_string()));
    assert_eq!(deps[1].extracted_requirement, Some("3.0".to_string()));
}

#[test]
fn test_parse_osgi_package_list_with_optional() {
    let list = "org.osgi.framework;version=\"[1.6,2)\",javax.servlet;resolution:=optional";
    let deps = parse_osgi_package_list(list, "import");

    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].is_optional, Some(false));
    assert_eq!(deps[0].is_runtime, Some(true));
    assert_eq!(deps[1].purl, Some("pkg:osgi/javax.servlet".to_string()));
    assert_eq!(deps[1].is_optional, Some(true));
    assert_eq!(deps[1].is_runtime, Some(true));
}

#[test]
fn test_parse_osgi_bundle_list_with_optional() {
    let list =
        "org.eclipse.core.runtime;bundle-version=\"3.7.0\",org.eclipse.ui;resolution:=optional";
    let deps = parse_osgi_bundle_list(list, "require-bundle");

    assert_eq!(deps.len(), 2);

    assert_eq!(
        deps[0].purl,
        Some("pkg:osgi/org.eclipse.core.runtime".to_string())
    );
    assert_eq!(deps[0].extracted_requirement, Some("3.7.0".to_string()));
    assert_eq!(deps[0].is_optional, Some(false));
    assert_eq!(deps[0].is_runtime, Some(true));

    assert_eq!(deps[1].purl, Some("pkg:osgi/org.eclipse.ui".to_string()));
    assert_eq!(deps[1].is_optional, Some(true));
    assert_eq!(deps[1].is_runtime, Some(false));
}

#[test]
fn test_osgi_manifest_optional_import_package_dependency() {
    let temp_dir = TempDir::new().expect("temp dir");
    let manifest_dir = temp_dir.path().join("META-INF");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let manifest_path = manifest_dir.join("MANIFEST.MF");

    fs::write(
        &manifest_path,
        "Manifest-Version: 1.0\nBundle-ManifestVersion: 2\nBundle-SymbolicName: com.example.optional\nBundle-Version: 1.0.0\nImport-Package: org.osgi.framework;version=\"[1.6,2)\",javax.servlet;resolution:=optional\n",
    )
    .expect("write manifest");

    let package = MavenParser::extract_first_package(&manifest_path);

    let import_deps: Vec<_> = package
        .dependencies
        .iter()
        .filter(|dep| dep.scope.as_deref() == Some("import"))
        .collect();
    assert_eq!(import_deps.len(), 2);

    let optional_dep = import_deps
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:osgi/javax.servlet"))
        .expect("optional import missing");
    assert_eq!(optional_dep.is_optional, Some(true));
    assert_eq!(optional_dep.is_runtime, Some(true));
}
