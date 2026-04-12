#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::parsers::{MavenParser, PackageParser, try_parse_file};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_temp_maven_file(content: &str, file_name: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let pom_path = temp_dir.path().join(file_name);

        fs::write(&pom_path, content).expect("Failed to write pom.xml");

        (temp_dir, pom_path)
    }

    // Helper function to create a temporary pom.xml file with the given content
    // Returns a TempDir (which must be kept alive) and the path to pom.xml
    fn create_temp_pom_xml(content: &str) -> (TempDir, PathBuf) {
        create_temp_maven_file(content, "pom.xml")
    }

    #[test]
    fn test_is_match() {
        let valid_path = PathBuf::from("/some/path/pom.xml");
        let valid_suffix_path = PathBuf::from("/some/path/canonical-pom.xml");
        let valid_dot_suffix_path = PathBuf::from("/some/path/plugin-level-dep.pom.xml");
        let repo_pom_path = PathBuf::from("/some/path/aopalliance-1.0.pom");
        let invalid_path = PathBuf::from("/some/path/not_pom.xml");

        assert!(MavenParser::is_match(&valid_path));
        assert!(MavenParser::is_match(&valid_suffix_path));
        assert!(MavenParser::is_match(&valid_dot_suffix_path));
        assert!(MavenParser::is_match(&repo_pom_path));
        assert!(!MavenParser::is_match(&invalid_path));
    }

    #[test]
    fn test_extract_from_testdata() {
        let pom_path = PathBuf::from("testdata/maven/pom.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(package_data.namespace, Some("com.example".to_string()));
        assert_eq!(package_data.name, Some("demo-app".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com/demo".to_string())
        );

        assert_eq!(package_data.declared_license_expression, None);
        assert_eq!(package_data.declared_license_expression_spdx, None);
        assert_eq!(package_data.license_detections.len(), 0);
        assert_eq!(
            package_data.description,
            Some("Demo Application\nA sample Maven project".to_string())
        );
        assert_eq!(
            package_data.extracted_license_statement,
            Some(
                "- license:\n    name: Apache License, Version 2.0\n    url: https://www.apache.org/licenses/LICENSE-2.0.txt\n".to_string()
            )
        );

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:maven/com.example/demo-app@1.0.0".to_string())
        );
        assert_eq!(
            package_data.source_packages,
            vec!["pkg:maven/com.example/demo-app@1.0.0?classifier=sources".to_string()]
        );

        // Check dependencies
        assert_eq!(package_data.dependencies.len(), 2);
        let purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.purl.as_deref())
            .collect();

        // Check that junit dependency exists with correct group/artifact
        assert!(
            purls
                .iter()
                .any(|p| p.starts_with("pkg:maven/junit/junit@")),
            "Should contain junit dependency"
        );

        // Check that commons-lang3 dependency exists with correct group/artifact
        assert!(
            purls
                .iter()
                .any(|p| p.starts_with("pkg:maven/org.apache.commons/commons-lang3@")),
            "Should contain commons-lang3 dependency"
        );
    }

    #[test]
    fn test_extract_basic_package_info() {
        let content = r#"
<project xmlns="http://maven.apache.org/POM/4.0.0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.test</groupId>
    <artifactId>test-project</artifactId>
    <version>1.0.0</version>
    <name>Test Project</name>
    <url>https://test.example.com</url>
    <licenses>
        <license>
            <name>MIT License</name>
        </license>
    </licenses>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(package_data.namespace, Some("com.test".to_string()));
        assert_eq!(package_data.name, Some("test-project".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://test.example.com".to_string())
        );

        assert_eq!(package_data.declared_license_expression, None);
        assert_eq!(package_data.declared_license_expression_spdx, None);
        assert_eq!(package_data.license_detections.len(), 0);
        assert_eq!(package_data.description, Some("Test Project".to_string()));
        assert_eq!(
            package_data.extracted_license_statement,
            Some("- license:\n    name: MIT License\n".to_string())
        );

        // Check purl
        assert_eq!(
            package_data.purl,
            Some("pkg:maven/com.test/test-project@1.0.0".to_string())
        );
    }

    #[test]
    fn test_skip_template_placeholder_pom_coordinates() {
        let content = r#"
<project>
    <groupId>${groupId}</groupId>
    <artifactId>${artifactId}</artifactId>
    <version>${version}</version>
    <description>Template pom</description>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::MavenPom));
        assert_eq!(package_data.namespace, None);
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.purl, None);
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_dependencies() {
        let content = r#"
<project>
    <groupId>com.example</groupId>
    <artifactId>test-deps</artifactId>
    <version>1.0.0</version>
    
    <dependencies>
        <dependency>
            <groupId>org.junit</groupId>
            <artifactId>junit</artifactId>
            <version>5.9.2</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>com.fasterxml.jackson.core</groupId>
            <artifactId>jackson-databind</artifactId>
            <version>2.15.2</version>
        </dependency>
    </dependencies>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.dependencies.len(), 2);

        // Verify junit dependency (test scope)
        let junit_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("junit"))
            .expect("Should find junit dependency");
        assert_eq!(
            junit_dep.purl,
            Some("pkg:maven/org.junit/junit@5.9.2".to_string())
        );
        assert_eq!(junit_dep.scope, Some("test".to_string()));
        assert_eq!(junit_dep.is_optional, Some(true));
        assert_eq!(junit_dep.is_runtime, Some(false));

        // Verify jackson dependency (no scope = compile/runtime)
        let jackson_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("jackson-databind"))
            .expect("Should find jackson-databind dependency");
        assert_eq!(
            jackson_dep.purl,
            Some("pkg:maven/com.fasterxml.jackson.core/jackson-databind@2.15.2".to_string())
        );
        assert_eq!(jackson_dep.scope, None);
        assert_eq!(jackson_dep.is_optional, Some(false));
        assert_eq!(jackson_dep.is_runtime, None);
    }

    #[test]
    fn test_maven_scope_types() {
        let content = r#"
<project>
    <groupId>com.example</groupId>
    <artifactId>scope-test</artifactId>
    <version>1.0.0</version>
    
    <dependencies>
        <dependency>
            <groupId>org.example</groupId>
            <artifactId>compile-dep</artifactId>
            <version>1.0</version>
            <scope>compile</scope>
        </dependency>
        <dependency>
            <groupId>org.example</groupId>
            <artifactId>test-dep</artifactId>
            <version>1.0</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>org.example</groupId>
            <artifactId>provided-dep</artifactId>
            <version>1.0</version>
            <scope>provided</scope>
        </dependency>
        <dependency>
            <groupId>org.example</groupId>
            <artifactId>runtime-dep</artifactId>
            <version>1.0</version>
            <scope>runtime</scope>
        </dependency>
        <dependency>
            <groupId>org.example</groupId>
            <artifactId>system-dep</artifactId>
            <version>1.0</version>
            <scope>system</scope>
            <systemPath>/path/to/system.jar</systemPath>
        </dependency>
    </dependencies>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.dependencies.len(), 5);

        let compile_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("compile-dep"))
            .expect("Should find compile-dep");
        assert_eq!(compile_dep.scope, Some("compile".to_string()));
        assert_eq!(compile_dep.is_optional, Some(false));
        assert_eq!(compile_dep.is_runtime, Some(true));

        let test_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("test-dep"))
            .expect("Should find test-dep");
        assert_eq!(test_dep.scope, Some("test".to_string()));
        assert_eq!(test_dep.is_optional, Some(true));
        assert_eq!(test_dep.is_runtime, Some(false));

        let provided_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("provided-dep"))
            .expect("Should find provided-dep");
        assert_eq!(provided_dep.scope, Some("provided".to_string()));
        assert_eq!(provided_dep.is_optional, Some(true));
        assert_eq!(provided_dep.is_runtime, Some(false));

        let runtime_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("runtime-dep"))
            .expect("Should find runtime-dep");
        assert_eq!(runtime_dep.scope, Some("runtime".to_string()));
        assert_eq!(runtime_dep.is_optional, Some(false));
        assert_eq!(runtime_dep.is_runtime, Some(true));

        let system_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_ref().unwrap().contains("system-dep"))
            .expect("Should find system-dep");
        assert_eq!(system_dep.scope, Some("system".to_string()));
        assert_eq!(system_dep.is_optional, Some(false));
        assert_eq!(system_dep.is_runtime, Some(true));
    }

    #[test]
    fn test_empty_or_invalid_pom_xml() {
        // Test with empty content
        let content = "";
        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        // Should return default/empty package data
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert!(package_data.dependencies.is_empty());

        // Test with invalid XML
        let content = "this is not valid XML";
        let (_temp_dir2, pom_path2) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path2);

        // Should return default/empty package data
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_api_url_basic() {
        // Given: A pom.xml with groupId, artifactId, and version
        let pom_path = PathBuf::from("testdata/maven/pom-api-url-basic.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        // Then: API data URL should point to the POM file
        assert_eq!(
            package_data.api_data_url,
            Some("https://repo1.maven.org/maven2/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.pom".to_string())
        );

        // Then: Repository homepage URL should be the Maven directory listing
        assert_eq!(
            package_data.repository_homepage_url,
            Some(
                "https://repo1.maven.org/maven2/org/apache/commons/commons-lang3/3.12.0/"
                    .to_string()
            )
        );

        // Then: Repository download URL should be the JAR file download URL
        assert_eq!(
            package_data.repository_download_url,
            Some("https://repo1.maven.org/maven2/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.jar".to_string())
        );
    }

    #[test]
    fn test_extract_api_url_no_version() {
        // Given: A pom.xml with groupId and artifactId but no version
        let pom_path = PathBuf::from("testdata/maven/pom-api-url-no-version.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        // Then: API data URL should be None (no version, can't construct POM filename)
        assert_eq!(package_data.api_data_url, None);

        // Then: Repository homepage URL should still be the Maven directory listing
        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://repo1.maven.org/maven2/junit/junit/".to_string())
        );

        // Then: Repository download URL should not be generated (no version)
        assert_eq!(package_data.repository_download_url, None);
    }

    #[test]
    fn test_extract_vcs_url_with_scm_connection() {
        // Given: A pom.xml with scm.connection
        let pom_path = PathBuf::from("testdata/maven/pom-scm.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        // Then: vcs_url should contain the scm.connection
        assert_eq!(
            package_data.vcs_url,
            Some("git+https://github.com/junit-team/junit5.git".to_string())
        );
    }

    #[test]
    fn test_parse_pom_properties() {
        let pom_props_path = PathBuf::from("testdata/maven/test1/pom.properties");
        let package_data = MavenParser::extract_first_package(&pom_props_path);

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::MavenPomProperties)
        );
        assert_eq!(package_data.namespace, Some("com.example.test".to_string()));
        assert_eq!(package_data.name, Some("test-library".to_string()));
        assert_eq!(package_data.version, Some("1.2.3".to_string()));
        assert_eq!(
            package_data.purl,
            Some("pkg:maven/com.example.test/test-library@1.2.3".to_string())
        );
    }

    #[test]
    fn test_parse_manifest_mf_implementation() {
        let manifest_path = PathBuf::from("testdata/maven/test2/MANIFEST.MF");
        let package_data = MavenParser::extract_first_package(&manifest_path);

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::JavaJarManifest)
        );
        assert_eq!(package_data.name, Some("spring-web".to_string()));
        assert_eq!(package_data.version, Some("5.3.20".to_string()));

        assert_eq!(package_data.parties.len(), 1);
        let vendor = &package_data.parties[0];
        assert_eq!(vendor.r#type, Some("organization".to_string()));
        assert_eq!(vendor.role, Some("vendor".to_string()));
        assert_eq!(vendor.name, Some("Spring Framework".to_string()));
    }

    #[test]
    fn test_parse_manifest_mf_bundle() {
        let manifest_path = PathBuf::from("testdata/maven/test3/MANIFEST.MF");
        let package_data = MavenParser::extract_first_package(&manifest_path);

        // This file has Bundle-SymbolicName, so it's detected as OSGi
        assert_eq!(package_data.package_type, Some(PackageType::Osgi));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::JavaOsgiManifest)
        );
        assert_eq!(package_data.name, Some("com.example.mybundle".to_string()));
        assert_eq!(package_data.version, Some("2.1.0".to_string()));

        assert_eq!(package_data.parties.len(), 1);
        let vendor = &package_data.parties[0];
        assert_eq!(vendor.name, Some("Example Corp".to_string()));
    }

    #[test]
    fn test_missing_manifest_mf_preserves_manifest_datasource() {
        let manifest_path = PathBuf::from("/nonexistent/MANIFEST.MF");
        let package_data = MavenParser::extract_first_package(&manifest_path);

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::JavaJarManifest)
        );
    }

    #[test]
    fn test_minimal_manifest_mf_stays_generic_jar() {
        let content = "Manifest-Version: 1.0\nStart-Class: ${foo.main}\n";
        let (_temp_dir, manifest_path) = create_temp_maven_file(content, "MANIFEST.MF");
        let package_data = MavenParser::extract_first_package(&manifest_path);

        assert_eq!(package_data.package_type, Some(PackageType::Jar));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::JavaJarManifest)
        );
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.purl, None);
    }

    #[test]
    fn test_pom_properties_purl_generation() {
        let pom_props_path = PathBuf::from("testdata/maven/test4/pom.properties");
        let package_data = MavenParser::extract_first_package(&pom_props_path);

        assert_eq!(
            package_data.purl,
            Some("pkg:maven/org.apache.commons/commons-lang3@3.12.0".to_string())
        );
        assert_eq!(
            package_data.namespace,
            Some("org.apache.commons".to_string())
        );
        assert_eq!(package_data.name, Some("commons-lang3".to_string()));
        assert_eq!(package_data.version, Some("3.12.0".to_string()));
    }

    #[test]
    fn test_extract_repositories() {
        let pom_path = PathBuf::from("testdata/maven/repositories-test.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.namespace, Some("com.example".to_string()));
        assert_eq!(package_data.name, Some("test-repo".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        let extra_data = package_data.extra_data.expect("extra_data should exist");

        let repositories = extra_data
            .get("repositories")
            .expect("repositories should exist")
            .as_array()
            .expect("repositories should be array");
        assert_eq!(repositories.len(), 2);

        let repo1 = repositories[0].as_object().expect("repo should be object");
        assert_eq!(repo1.get("id").unwrap().as_str().unwrap(), "central");
        assert_eq!(
            repo1.get("name").unwrap().as_str().unwrap(),
            "Maven Central Repository"
        );
        assert_eq!(
            repo1.get("url").unwrap().as_str().unwrap(),
            "https://repo1.maven.org/maven2"
        );

        let repo2 = repositories[1].as_object().expect("repo should be object");
        assert_eq!(
            repo2.get("id").unwrap().as_str().unwrap(),
            "spring-releases"
        );
        assert_eq!(
            repo2.get("name").unwrap().as_str().unwrap(),
            "Spring Releases"
        );
        assert_eq!(
            repo2.get("url").unwrap().as_str().unwrap(),
            "https://repo.spring.io/release"
        );

        let plugin_repositories = extra_data
            .get("plugin_repositories")
            .expect("plugin_repositories should exist")
            .as_array()
            .expect("plugin_repositories should be array");
        assert_eq!(plugin_repositories.len(), 1);

        let plugin_repo = plugin_repositories[0]
            .as_object()
            .expect("plugin_repo should be object");
        assert_eq!(plugin_repo.get("id").unwrap().as_str().unwrap(), "central");
        assert_eq!(
            plugin_repo.get("name").unwrap().as_str().unwrap(),
            "Maven Plugin Repository"
        );
        assert_eq!(
            plugin_repo.get("url").unwrap().as_str().unwrap(),
            "https://repo1.maven.org/maven2"
        );
    }

    #[test]
    fn test_extract_modules() {
        let pom_path = PathBuf::from("testdata/maven/modules-test.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.namespace, Some("com.example".to_string()));
        assert_eq!(package_data.name, Some("multi-module-parent".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        let extra_data = package_data.extra_data.expect("extra_data should exist");

        let modules = extra_data
            .get("modules")
            .expect("modules should exist")
            .as_array()
            .expect("modules should be array");
        assert_eq!(modules.len(), 3);

        assert_eq!(modules[0].as_str().unwrap(), "module-core");
        assert_eq!(modules[1].as_str().unwrap(), "module-api");
        assert_eq!(modules[2].as_str().unwrap(), "module-web");
    }

    #[test]
    fn test_extract_mailing_lists() {
        let pom_path = PathBuf::from("testdata/maven/mailing-lists-test.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.namespace,
            Some("org.apache.commons".to_string())
        );
        assert_eq!(package_data.name, Some("commons-lang3".to_string()));
        assert_eq!(package_data.version, Some("3.12.0".to_string()));

        let extra_data = package_data.extra_data.expect("extra_data should exist");

        let mailing_lists = extra_data
            .get("mailing_lists")
            .expect("mailing_lists should exist")
            .as_array()
            .expect("mailing_lists should be array");
        assert_eq!(mailing_lists.len(), 2);

        let ml1 = mailing_lists[0]
            .as_object()
            .expect("mailing list should be object");
        assert_eq!(
            ml1.get("name").unwrap().as_str().unwrap(),
            "Commons User List"
        );
        assert_eq!(
            ml1.get("subscribe").unwrap().as_str().unwrap(),
            "user-subscribe@commons.apache.org"
        );
        assert_eq!(
            ml1.get("unsubscribe").unwrap().as_str().unwrap(),
            "user-unsubscribe@commons.apache.org"
        );
        assert_eq!(
            ml1.get("post").unwrap().as_str().unwrap(),
            "user@commons.apache.org"
        );
        assert_eq!(
            ml1.get("archive").unwrap().as_str().unwrap(),
            "https://lists.apache.org/list.html?user@commons.apache.org"
        );

        let ml2 = mailing_lists[1]
            .as_object()
            .expect("mailing list should be object");
        assert_eq!(
            ml2.get("name").unwrap().as_str().unwrap(),
            "Commons Dev List"
        );
        assert_eq!(
            ml2.get("subscribe").unwrap().as_str().unwrap(),
            "dev-subscribe@commons.apache.org"
        );
    }

    #[test]
    fn test_extract_dependency_management() {
        let pom_path = PathBuf::from("testdata/maven/dependency-management-test.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.namespace, Some("com.example".to_string()));
        assert_eq!(package_data.name, Some("parent-project".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        assert_eq!(package_data.dependencies.len(), 4);
        let slf4j_dep = &package_data.dependencies[0];
        assert!(
            slf4j_dep
                .purl
                .as_ref()
                .unwrap()
                .contains("org.slf4j/slf4j-api")
        );

        let dep_mgmt_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|dep| dep.scope.as_deref() == Some("dependencymanagement"))
            .collect();
        assert_eq!(dep_mgmt_deps.len(), 3);
        assert!(dep_mgmt_deps.iter().any(|dep| {
            dep.purl.as_deref()
                == Some("pkg:maven/org.springframework.boot/spring-boot-dependencies@2.7.0")
        }));

        let extra_data = package_data.extra_data.expect("extra_data should exist");

        let dep_mgmt = extra_data
            .get("dependency_management")
            .expect("dependency_management should exist")
            .as_array()
            .expect("dependency_management should be array");
        assert_eq!(dep_mgmt.len(), 3);

        let spring_dep = dep_mgmt[0].as_object().expect("dep should be object");
        assert_eq!(
            spring_dep.get("groupId").unwrap().as_str().unwrap(),
            "org.springframework.boot"
        );
        assert_eq!(
            spring_dep.get("artifactId").unwrap().as_str().unwrap(),
            "spring-boot-dependencies"
        );
        assert_eq!(
            spring_dep.get("version").unwrap().as_str().unwrap(),
            "2.7.0"
        );

        let jackson_dep = dep_mgmt[1].as_object().expect("dep should be object");
        assert_eq!(
            jackson_dep.get("groupId").unwrap().as_str().unwrap(),
            "com.fasterxml.jackson.core"
        );

        let junit_dep = dep_mgmt[2].as_object().expect("dep should be object");
        assert_eq!(junit_dep.get("groupId").unwrap().as_str().unwrap(), "junit");
    }

    #[test]
    fn test_dependency_management_with_exclusions_preserves_main_coordinates() {
        let pom_path = PathBuf::from("testdata/maven/dependency-management-exclusions-test.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        let dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("dependencymanagement"))
            .expect("dependency management entry should be present");

        assert_eq!(
            dep.purl.as_deref(),
            Some("pkg:maven/com.google.guava/guava@33.5.0-jre")
        );
    }

    #[test]
    fn test_extract_parent_pom() {
        let pom_path = PathBuf::from("testdata/maven/parent-pom-test.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.namespace,
            Some("org.springframework.boot".to_string())
        );
        assert_eq!(package_data.name, Some("my-service".to_string()));
        assert_eq!(package_data.version, Some("2.7.0".to_string()));

        let extra_data = package_data.extra_data.expect("extra_data should exist");

        let parent = extra_data
            .get("parent")
            .expect("parent should exist")
            .as_object()
            .expect("parent should be object");

        assert_eq!(
            parent.get("groupId").unwrap().as_str().unwrap(),
            "org.springframework.boot"
        );
        assert_eq!(
            parent.get("artifactId").unwrap().as_str().unwrap(),
            "spring-boot-starter-parent"
        );
        assert_eq!(parent.get("version").unwrap().as_str().unwrap(), "2.7.0");

        if let Some(relative_path) = parent.get("relativePath") {
            assert_eq!(relative_path.as_str().unwrap(), "");
        }
    }

    #[test]
    fn test_is_match_pom_properties() {
        let valid_path = PathBuf::from("/some/path/pom.properties");
        let invalid_path = PathBuf::from("/some/path/not_pom.properties");

        assert!(MavenParser::is_match(&valid_path));
        assert!(!MavenParser::is_match(&invalid_path));
    }

    #[test]
    fn test_is_match_manifest_mf() {
        let valid_path = PathBuf::from("/some/path/MANIFEST.MF");
        let invalid_path = PathBuf::from("/some/path/manifest.mf");

        assert!(MavenParser::is_match(&valid_path));
        assert!(!MavenParser::is_match(&invalid_path));
    }

    #[test]
    fn test_basic_property_chain() {
        let pom_path = PathBuf::from("testdata/maven/test-properties-basic.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("1.0.0".to_string()));
    }

    #[test]
    fn test_extract_declared_license_from_public_domain_name() {
        let content = r#"
            <project>
              <modelVersion>4.0.0</modelVersion>
              <groupId>aopalliance</groupId>
              <artifactId>aopalliance</artifactId>
              <version>1.0</version>
              <licenses>
                <license>
                  <name>Public Domain</name>
                </license>
              </licenses>
            </project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("public-domain")
        );
    }

    #[test]
    fn test_multiple_placeholders() {
        let content = r#"
<project>
  <groupId>com.test</groupId>
  <artifactId>multi</artifactId>
  <properties>
    <a>alpha</a>
    <b>beta</b>
    <c>gamma</c>
  </properties>
  <version>${a}-${b}-${c}</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("alpha-beta-gamma".to_string()));
    }

    #[test]
    fn test_missing_key() {
        let content = r#"
<project>
  <groupId>com.test</groupId>
  <artifactId>missing</artifactId>
  <version>${does.not.exist}</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("${does.not.exist}".to_string()));
    }

    #[test]
    fn test_try_parse_file_missing_key_is_not_a_scan_error() {
        let content = r#"
<project>
  <groupId>com.test</groupId>
  <artifactId>missing</artifactId>
  <version>${does.not.exist}</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_maven_file(content, "missing-version-pom.xml");
        let result = try_parse_file(&pom_path).expect("suffix pom.xml should be recognized");

        assert!(result.scan_errors.is_empty(), "{:?}", result.scan_errors);
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name.as_deref(), Some("missing"));
        assert_eq!(
            result.packages[0].version.as_deref(),
            Some("${does.not.exist}")
        );
    }

    #[test]
    fn test_missing_dependency_version_keeps_versionless_dependency_purl() {
        let pom_path = PathBuf::from("testdata/maven/missing-dependency-version-pom.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dependency = &package_data.dependencies[0];
        assert_eq!(
            dependency.purl.as_deref(),
            Some("pkg:maven/groupId/artifactId")
        );
        assert_eq!(dependency.extracted_requirement, None);
    }

    #[test]
    fn test_extract_from_suffix_pom_xml_filename() {
        let content = r#"
<project>
  <groupId>org.example</groupId>
  <artifactId>parent</artifactId>
  <version>0.0.1-SNAPSHOT</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_maven_file(content, "parent-pom.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.namespace.as_deref(), Some("org.example"));
        assert_eq!(package_data.name.as_deref(), Some("parent"));
        assert_eq!(package_data.version.as_deref(), Some("0.0.1-SNAPSHOT"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::MavenPom));
    }

    #[test]
    fn test_self_cycle() {
        let content = r#"
<project>
  <groupId>com.test</groupId>
  <artifactId>cycle</artifactId>
  <properties>
    <a>${a}</a>
  </properties>
  <version>${a}</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("${a}".to_string()));
    }

    #[test]
    fn test_try_parse_file_direct_self_cycle_is_not_a_scan_error() {
        let content = r#"
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>org.apache</groupId>
  <artifactId>child</artifactId>
  <version>${project.version}</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let result = try_parse_file(&pom_path).expect("pom.xml should still be recognized");

        assert!(result.scan_errors.is_empty(), "{:?}", result.scan_errors);
        assert_eq!(
            result.packages[0].version.as_deref(),
            Some("${project.version}")
        );
    }

    #[test]
    fn test_mutual_cycle() {
        let pom_path = PathBuf::from("testdata/maven/test-properties-cycle.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("${a}".to_string()));
    }

    #[test]
    fn test_nested_placeholder() {
        let pom_path = PathBuf::from("testdata/maven/test-properties-nested.xml");
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("SUCCESS".to_string()));
    }

    #[test]
    fn test_malformed_placeholders() {
        let content = r#"
<project>
  <groupId>com.test</groupId>
  <artifactId>malformed</artifactId>
  <version>${a</version>
  <url>${}</url>
  <scm>
    <url>${a}}</url>
  </scm>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("${a".to_string()));
        assert_eq!(package_data.homepage_url, Some("${}".to_string()));
        assert_eq!(package_data.code_view_url, Some("${a}}".to_string()));
    }

    #[test]
    fn test_depth_limit() {
        let mut properties = String::new();
        for index in 1..=11 {
            if index < 11 {
                properties.push_str(&format!(
                    "<a{index}>${{a{next}}}</a{index}>",
                    index = index,
                    next = index + 1
                ));
            } else {
                properties.push_str(&format!("<a{index}>final</a{index}>", index = index));
            }
        }

        let content = format!(
            "<project><groupId>com.test</groupId><artifactId>depth</artifactId><properties>{}</properties><version>${{a1}}</version></project>",
            properties
        );

        let (_temp_dir, pom_path) = create_temp_pom_xml(&content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.version, Some("${a11}".to_string()));
    }

    #[test]
    fn test_standard_properties() {
        let content = r#"
<project>
  <groupId>com.test</groupId>
  <artifactId>standard</artifactId>
  <version>2.5.0</version>
  <properties>
    <resolved.version>${project.version}</resolved.version>
  </properties>
  <url>https://example.com/${resolved.version}</url>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com/2.5.0".to_string())
        );
    }

    #[test]
    fn test_extract_structured_license_owner_and_source_package() {
        let content = r#"
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>rich-metadata</artifactId>
    <version>1.2.3</version>
    <name>Rich Metadata</name>
    <description>Detailed metadata package</description>
    <organization>
        <name>Example Org</name>
        <url>https://example.org</url>
    </organization>
    <licenses>
        <license>
            <name>Apache-2.0</name>
            <url>https://www.apache.org/licenses/LICENSE-2.0.txt</url>
            <comments>Main distribution license</comments>
        </license>
    </licenses>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.description,
            Some("Rich Metadata\nDetailed metadata package".to_string())
        );
        assert_eq!(
            package_data.extracted_license_statement,
            Some(
                "- license:\n    name: Apache-2.0\n    url: https://www.apache.org/licenses/LICENSE-2.0.txt\n    comments: Main distribution license\n".to_string()
            )
        );
        assert!(package_data.parties.iter().any(|party| {
            party.role.as_deref() == Some("owner")
                && party.name.as_deref() == Some("Example Org")
                && party.url.as_deref() == Some("https://example.org")
        }));
        assert_eq!(
            package_data.source_packages,
            vec!["pkg:maven/com.example/rich-metadata@1.2.3?classifier=sources".to_string()]
        );
    }

    #[test]
    fn test_extract_license_statement_includes_xml_license_comments() {
        let content = r#"
<!-- Licensed under the Apache License, Version 2.0 -->
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>commented-license</artifactId>
    <version>1.0.0</version>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(
            package_data.extracted_license_statement,
            Some(
                "- license:\n    comments: Licensed under the Apache License, Version 2.0\n"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_nested_meta_inf_pom_path_supplies_namespace() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nested_dir = temp_dir
            .path()
            .join("META-INF/maven/org.example/nested-lib");
        fs::create_dir_all(&nested_dir).expect("Failed to create nested META-INF directory");

        let pom_path = nested_dir.join("pom.xml");
        fs::write(
            &pom_path,
            r#"
<project>
    <modelVersion>4.0.0</modelVersion>
    <artifactId>nested-lib</artifactId>
    <version>1.0.0</version>
</project>
            "#,
        )
        .expect("Failed to write nested pom.xml");

        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.datasource_id, Some(DatasourceId::MavenPom));
        assert_eq!(package_data.namespace, Some("org.example".to_string()));
        assert_eq!(package_data.name, Some("nested-lib".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));
        assert_eq!(
            package_data.purl,
            Some("pkg:maven/org.example/nested-lib@1.0.0".to_string())
        );
    }

    #[test]
    fn test_developer_role_spelling_matches_python_reference() {
        let content = r#"
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>role-spelling</artifactId>
    <version>1.0.0</version>
    <developers>
        <developer>
            <name>Example Dev</name>
        </developer>
    </developers>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert!(
            package_data
                .parties
                .iter()
                .any(|party| party.role.as_deref() == Some("developer"))
        );
        assert!(
            package_data
                .parties
                .iter()
                .all(|party| party.role.as_deref() != Some("developper"))
        );
    }

    #[test]
    fn test_extract_model_4_1_qualifiers_dependency_management_and_relocation() {
        let content = r#"
<project>
    <modelVersion>4.1.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>web-app</artifactId>
    <version>2.0.0</version>
    <packaging>war</packaging>
    <classifier>jakarta</classifier>
    <name>web-app</name>
    <description>web-app</description>
    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-dependencies</artifactId>
                <version>3.2.0</version>
                <type>pom</type>
                <scope>import</scope>
            </dependency>
            <dependency>
                <groupId>com.example.libs</groupId>
                <artifactId>managed-war</artifactId>
                <version>4.5.6</version>
                <type>war</type>
                <classifier>tests</classifier>
                <optional>true</optional>
            </dependency>
        </dependencies>
    </dependencyManagement>
    <distributionManagement>
        <relocation>
            <groupId>org.example.new</groupId>
            <artifactId>new-web-app</artifactId>
            <version>3.0.0</version>
            <message>Use the relocated coordinates</message>
        </relocation>
    </distributionManagement>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        let qualifiers = package_data.qualifiers.expect("qualifiers should exist");
        assert_eq!(
            qualifiers.get("classifier").map(String::as_str),
            Some("jakarta")
        );
        assert_eq!(qualifiers.get("type").map(String::as_str), Some("war"));
        assert_eq!(package_data.description, Some("web-app".to_string()));
        assert_eq!(
            package_data.purl,
            Some("pkg:maven/com.example/web-app@2.0.0?classifier=jakarta&type=war".to_string())
        );
        assert_eq!(
            package_data.repository_download_url,
            Some(
                "https://repo1.maven.org/maven2/com/example/web-app/2.0.0/web-app-2.0.0-jakarta.war"
                    .to_string()
            )
        );
        assert!(package_data.source_packages.is_empty());

        let import_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("import"))
            .expect("import dependency should exist");
        assert_eq!(
            import_dep.purl,
            Some("pkg:maven/org.springframework.boot/spring-boot-dependencies@3.2.0".to_string())
        );
        assert_eq!(import_dep.is_runtime, Some(false));

        let managed_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("dependencymanagement"))
            .expect("dependencyManagement dependency should exist");
        assert_eq!(
            managed_dep.purl,
            Some(
                "pkg:maven/com.example.libs/managed-war@4.5.6?classifier=tests&type=war"
                    .to_string()
            )
        );
        assert_eq!(managed_dep.is_optional, Some(true));

        let relocation_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.scope.as_deref() == Some("relocation"))
            .expect("relocation dependency should exist");
        assert_eq!(
            relocation_dep.purl,
            Some("pkg:maven/org.example.new/new-web-app@3.0.0".to_string())
        );
        assert_eq!(
            relocation_dep
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("message"))
                .and_then(|value| value.as_str()),
            Some("Use the relocated coordinates")
        );

        let extra_data = package_data.extra_data.expect("extra_data should exist");
        let relocation = extra_data
            .get("relocation")
            .and_then(|value| value.as_object())
            .expect("relocation extra data should exist");
        assert_eq!(
            relocation.get("message").and_then(|value| value.as_str()),
            Some("Use the relocated coordinates")
        );
    }

    #[test]
    fn test_packaging_alias_normalizes_to_jar_outputs() {
        let content = r#"
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>org.example</groupId>
    <artifactId>plugin-artifact</artifactId>
    <version>1.0.0</version>
    <packaging>maven-plugin</packaging>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);

        assert_eq!(package_data.qualifiers, None);
        assert_eq!(
            package_data.purl,
            Some("pkg:maven/org.example/plugin-artifact@1.0.0".to_string())
        );
        assert_eq!(
            package_data.repository_download_url,
            Some(
                "https://repo1.maven.org/maven2/org/example/plugin-artifact/1.0.0/plugin-artifact-1.0.0.jar"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_dependency_scope_and_optional_resolve_from_properties() {
        let content = r#"
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>org.example</groupId>
    <artifactId>property-scopes</artifactId>
    <version>1.0.0</version>
    <properties>
        <dep.scope>provided</dep.scope>
        <dep.optional>true</dep.optional>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.example.libs</groupId>
            <artifactId>runtime-helper</artifactId>
            <version>2.0.0</version>
            <scope>${dep.scope}</scope>
            <optional>${dep.optional}</optional>
        </dependency>
    </dependencies>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);
        let dependency = package_data
            .dependencies
            .first()
            .expect("dependency should exist");

        assert_eq!(dependency.scope, Some("provided".to_string()));
        assert_eq!(dependency.is_runtime, Some(false));
        assert_eq!(dependency.is_optional, Some(true));
    }

    #[test]
    fn test_relocation_message_is_preserved_without_coordinates() {
        let content = r#"
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>org.example</groupId>
    <artifactId>relocation-message-only</artifactId>
    <version>1.0.0</version>
    <distributionManagement>
        <relocation>
            <message>Moved to a private repository</message>
        </relocation>
    </distributionManagement>
</project>
        "#;

        let (_temp_dir, pom_path) = create_temp_pom_xml(content);
        let package_data = MavenParser::extract_first_package(&pom_path);
        let extra_data = package_data.extra_data.expect("extra_data should exist");
        let relocation = extra_data
            .get("relocation")
            .and_then(|value| value.as_object())
            .expect("relocation extra data should exist");

        assert_eq!(package_data.dependencies.len(), 0);
        assert_eq!(
            relocation.get("message").and_then(|value| value.as_str()),
            Some("Moved to a private repository")
        );
    }
}
