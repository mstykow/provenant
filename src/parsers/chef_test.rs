// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for Chef metadata.json and metadata.rb parsers.

use crate::models::{DatasourceId, PackageType};

use std::path::PathBuf;

use super::PackageParser;
use super::chef::{ChefMetadataJsonParser, ChefMetadataRbParser};

#[test]
fn test_is_match_metadata_json() {
    let path = PathBuf::from("cookbook/metadata.json");
    assert!(ChefMetadataJsonParser::is_match(&path));
}

#[test]
fn test_is_match_rejects_dist_info() {
    // Should reject metadata.json inside dist-info directories (Python wheels)
    let path = PathBuf::from("package-1.0.dist-info/metadata.json");
    assert!(!ChefMetadataJsonParser::is_match(&path));

    let path = PathBuf::from("something.dist-info/metadata.json");
    assert!(!ChefMetadataJsonParser::is_match(&path));
}

#[test]
fn test_is_match_other_filenames() {
    assert!(!ChefMetadataJsonParser::is_match(&PathBuf::from(
        "cookbook/metadata.rb"
    )));
    assert!(!ChefMetadataJsonParser::is_match(&PathBuf::from(
        "package.json"
    )));
    assert!(!ChefMetadataJsonParser::is_match(&PathBuf::from(
        "composer.json"
    )));
}

#[test]
fn test_basic_extraction() {
    let path = PathBuf::from("testdata/chef/basic/metadata.json");
    let package = ChefMetadataJsonParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Chef));
    assert_eq!(package.name, Some("301".to_string()));
    assert_eq!(package.version, Some("0.1.0".to_string()));
    assert_eq!(
        package.description,
        Some("Installs/Configures 301".to_string())
    );
    assert_eq!(package.extracted_license_statement, Some("MIT".to_string()));
    assert_eq!(package.primary_language, Some("Ruby".to_string()));

    // Check maintainer party
    assert_eq!(package.parties.len(), 1);
    let maintainer = &package.parties[0];
    assert_eq!(maintainer.name, Some("Mark Wilkerson".to_string()));
    assert_eq!(maintainer.email, Some("mark@segfawlt.net".to_string()));
    assert_eq!(maintainer.role, Some("maintainer".to_string()));

    // Check dependencies
    assert_eq!(package.dependencies.len(), 1);
    let dep = &package.dependencies[0];
    assert_eq!(dep.purl, Some("pkg:chef/nodejs".to_string()));
    assert_eq!(dep.extracted_requirement, Some(">= 0.0.0".to_string()));
    assert_eq!(dep.scope, Some("dependencies".to_string()));
    assert_eq!(dep.is_runtime, Some(true));
    assert_eq!(dep.is_optional, Some(false));

    // Check constructed URLs
    assert_eq!(
        package.download_url,
        Some("https://supermarket.chef.io/cookbooks/301/versions/0.1.0/download".to_string())
    );
    assert_eq!(
        package.repository_download_url,
        Some("https://supermarket.chef.io/cookbooks/301/versions/0.1.0/download".to_string())
    );
    assert_eq!(
        package.repository_homepage_url,
        Some("https://supermarket.chef.io/cookbooks/301/versions/0.1.0/".to_string())
    );
    assert_eq!(
        package.api_data_url,
        Some("https://supermarket.chef.io/api/v1/cookbooks/301/versions/0.1.0".to_string())
    );
}

#[test]
fn test_long_description_fallback() {
    // Test that long_description is used when description is not present
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "long_description": "This is a long description",
        "license": "Apache-2.0"
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_long_desc.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(
        package.description,
        Some("This is a long description".to_string())
    );

    // Cleanup
    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_description_preferred_over_long_description() {
    // Test that description is preferred when both are present
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "description": "Short description",
        "long_description": "This is a long description",
        "license": "Apache-2.0"
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_desc_priority.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.description, Some("Short description".to_string()));

    // Cleanup
    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_dependencies_merged_from_both_fields() {
    // Test that dependencies from both "dependencies" and "depends" are merged
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "dependencies": {
            "apt": ">= 0.0.0",
            "yum": ">= 3.0"
        },
        "depends": {
            "build-essential": ">= 2.0.0"
        }
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_deps_merge.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    // Should have 3 dependencies total
    assert_eq!(package.dependencies.len(), 3);

    // Find each dependency (they're sorted by purl)
    let apt = package
        .dependencies
        .iter()
        .find(|d| d.purl.as_ref().map(|p| p.contains("apt")).unwrap_or(false))
        .expect("apt dependency not found");
    assert_eq!(apt.extracted_requirement, Some(">= 0.0.0".to_string()));
    assert_eq!(apt.scope, Some("dependencies".to_string()));

    let build_essential = package
        .dependencies
        .iter()
        .find(|d| {
            d.purl
                .as_ref()
                .map(|p| p.contains("build-essential"))
                .unwrap_or(false)
        })
        .expect("build-essential dependency not found");
    assert_eq!(
        build_essential.extracted_requirement,
        Some(">= 2.0.0".to_string())
    );

    let yum = package
        .dependencies
        .iter()
        .find(|d| d.purl.as_ref().map(|p| p.contains("yum")).unwrap_or(false))
        .expect("yum dependency not found");
    assert_eq!(yum.extracted_requirement, Some(">= 3.0".to_string()));

    // Cleanup
    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_url_construction_requires_name_and_version() {
    // Test that URLs are not constructed without both name and version
    let json_content = r#"{
        "name": "test-cookbook"
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_no_urls.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.name, Some("test-cookbook".to_string()));
    assert_eq!(package.version, None);
    assert_eq!(package.download_url, None);
    assert_eq!(package.repository_download_url, None);
    assert_eq!(package.repository_homepage_url, None);
    assert_eq!(package.api_data_url, None);

    // Cleanup
    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_maintainer_party_extraction() {
    // Test with both name and email
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "maintainer": "John Doe",
        "maintainer_email": "john@example.com"
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_maintainer_both.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.parties.len(), 1);
    assert_eq!(package.parties[0].name, Some("John Doe".to_string()));
    assert_eq!(
        package.parties[0].email,
        Some("john@example.com".to_string())
    );

    std::fs::remove_file(&test_file).unwrap();

    // Test with only name
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "maintainer": "Jane Smith"
    }"#;

    let test_file = temp_dir.join("test_chef_maintainer_name.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.parties.len(), 1);
    assert_eq!(package.parties[0].name, Some("Jane Smith".to_string()));
    assert_eq!(package.parties[0].email, None);

    std::fs::remove_file(&test_file).unwrap();

    // Test with only email
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "maintainer_email": "contact@example.com"
    }"#;

    let test_file = temp_dir.join("test_chef_maintainer_email.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.parties.len(), 1);
    assert_eq!(package.parties[0].name, None);
    assert_eq!(
        package.parties[0].email,
        Some("contact@example.com".to_string())
    );

    std::fs::remove_file(&test_file).unwrap();

    // Test with neither - should have no parties
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0"
    }"#;

    let test_file = temp_dir.join("test_chef_maintainer_none.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.parties.len(), 0);

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_optional_urls() {
    // Test source_url and issues_url extraction
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0",
        "source_url": "https://github.com/example/cookbook",
        "issues_url": "https://github.com/example/cookbook/issues"
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_optional_urls.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(
        package.code_view_url,
        Some("https://github.com/example/cookbook".to_string())
    );
    assert_eq!(
        package.bug_tracking_url,
        Some("https://github.com/example/cookbook/issues".to_string())
    );

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_malformed_json() {
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "1.0.0"
        // Missing closing brace
    "#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_malformed.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    // Should return default package data with just package_type set
    assert_eq!(package.package_type, Some(PackageType::Chef));
    assert_eq!(
        package.datasource_id,
        Some(DatasourceId::ChefCookbookMetadataJson)
    );
    assert_eq!(package.name, None);
    assert_eq!(package.version, None);

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_empty_strings_are_filtered() {
    // Test that empty strings are filtered out
    let json_content = r#"{
        "name": "test-cookbook",
        "version": "",
        "description": "   ",
        "license": "",
        "maintainer": "",
        "maintainer_email": "  "
    }"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_empty_strings.json");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataJsonParser::extract_first_package(&test_file);

    assert_eq!(package.name, Some("test-cookbook".to_string()));
    assert_eq!(package.version, None);
    assert_eq!(package.description, None);
    assert_eq!(package.extracted_license_statement, None);
    assert_eq!(package.parties.len(), 0); // No parties since maintainer fields are empty

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_missing_metadata_rb_preserves_rb_datasource() {
    let path = PathBuf::from("/nonexistent/metadata.rb");
    let package = ChefMetadataRbParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Chef));
    assert_eq!(
        package.datasource_id,
        Some(DatasourceId::ChefCookbookMetadataRb)
    );
}

#[test]
fn test_is_match_rb() {
    let path = PathBuf::from("cookbook/metadata.rb");
    assert!(ChefMetadataRbParser::is_match(&path));
}

#[test]
fn test_is_match_rb_rejects_json() {
    assert!(!ChefMetadataRbParser::is_match(&PathBuf::from(
        "cookbook/metadata.json"
    )));
    assert!(!ChefMetadataRbParser::is_match(&PathBuf::from(
        "package.json"
    )));
}

#[test]
fn test_basic_rb_extraction() {
    let path = PathBuf::from("testdata/chef/basic/metadata.rb");
    let package = ChefMetadataRbParser::extract_first_package(&path);

    assert_eq!(package.package_type, Some(PackageType::Chef));
    assert_eq!(package.name, Some("301".to_string()));
    assert_eq!(package.version, Some("0.1.0".to_string()));
    assert_eq!(
        package.description,
        Some("Installs/Configures 301".to_string())
    );
    assert_eq!(package.extracted_license_statement, Some("MIT".to_string()));
    assert_eq!(package.primary_language, Some("Ruby".to_string()));

    assert_eq!(package.parties.len(), 1);
    let maintainer = &package.parties[0];
    assert_eq!(maintainer.name, Some("Mark Wilkerson".to_string()));
    assert_eq!(maintainer.email, Some("mark@segfawlt.net".to_string()));
    assert_eq!(maintainer.role, Some("maintainer".to_string()));

    assert_eq!(package.dependencies.len(), 1);
    let dep = &package.dependencies[0];
    assert_eq!(dep.purl, Some("pkg:chef/nodejs".to_string()));
    assert_eq!(dep.extracted_requirement, None);
    assert_eq!(dep.scope, Some("dependencies".to_string()));
    assert_eq!(dep.is_runtime, Some(true));
    assert_eq!(dep.is_optional, Some(false));

    assert_eq!(
        package.download_url,
        Some("https://supermarket.chef.io/cookbooks/301/versions/0.1.0/download".to_string())
    );
}

#[test]
fn test_rb_dependencies() {
    let path = PathBuf::from("testdata/chef/dependencies/metadata.rb");
    let package = ChefMetadataRbParser::extract_first_package(&path);

    assert_eq!(package.name, Some("build-essential".to_string()));
    assert_eq!(package.version, Some("8.2.1".to_string()));

    assert_eq!(package.dependencies.len(), 2);

    let mingw = package
        .dependencies
        .iter()
        .find(|d| {
            d.purl
                .as_ref()
                .map(|p| p.contains("mingw"))
                .unwrap_or(false)
        })
        .expect("mingw dependency not found");
    assert_eq!(mingw.extracted_requirement, Some(">= 1.1".to_string()));

    let seven_zip = package
        .dependencies
        .iter()
        .find(|d| {
            d.purl
                .as_ref()
                .map(|p| p.contains("seven_zip"))
                .unwrap_or(false)
        })
        .expect("seven_zip dependency not found");
    assert_eq!(seven_zip.extracted_requirement, None);
}

#[test]
fn test_rb_source_and_issues_urls() {
    let path = PathBuf::from("testdata/chef/dependencies/metadata.rb");
    let package = ChefMetadataRbParser::extract_first_package(&path);

    assert_eq!(
        package.code_view_url,
        Some("https://github.com/chef-cookbooks/build-essential".to_string())
    );
    assert_eq!(
        package.bug_tracking_url,
        Some("https://github.com/chef-cookbooks/build-essential/issues".to_string())
    );
}

#[test]
fn test_rb_io_read_skipped() {
    let path = PathBuf::from("testdata/chef/dependencies/metadata.rb");
    let package = ChefMetadataRbParser::extract_first_package(&path);

    assert_eq!(
        package.description,
        Some("Installs C compiler / build tools".to_string())
    );
}

#[test]
fn test_rb_vs_json_parity() {
    let rb_path = PathBuf::from("testdata/chef/basic/metadata.rb");
    let json_path = PathBuf::from("testdata/chef/basic/metadata.json");

    let rb_package = ChefMetadataRbParser::extract_first_package(&rb_path);
    let json_package = ChefMetadataJsonParser::extract_first_package(&json_path);

    assert_eq!(rb_package.name, json_package.name);
    assert_eq!(rb_package.version, json_package.version);
    assert_eq!(rb_package.description, json_package.description);
    assert_eq!(
        rb_package.extracted_license_statement,
        json_package.extracted_license_statement
    );
    assert_eq!(rb_package.parties.len(), json_package.parties.len());
    assert_eq!(
        rb_package.dependencies.len(),
        json_package.dependencies.len()
    );

    if !rb_package.parties.is_empty() && !json_package.parties.is_empty() {
        assert_eq!(rb_package.parties[0].name, json_package.parties[0].name);
        assert_eq!(rb_package.parties[0].email, json_package.parties[0].email);
    }
}

#[test]
fn test_rb_handles_empty_file() {
    let json_content = "";

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_empty.rb");
    std::fs::write(&test_file, json_content).unwrap();

    let package = ChefMetadataRbParser::extract_first_package(&test_file);

    assert_eq!(package.package_type, Some(PackageType::Chef));
    assert_eq!(package.name, None);
    assert_eq!(package.version, None);

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_rb_handles_comments() {
    let rb_content = r#"
# This is a comment
name             'test-cookbook'
# Another comment
version          '1.0.0'
description      'Test description'
"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_comments.rb");
    std::fs::write(&test_file, rb_content).unwrap();

    let package = ChefMetadataRbParser::extract_first_package(&test_file);

    assert_eq!(package.name, Some("test-cookbook".to_string()));
    assert_eq!(package.version, Some("1.0.0".to_string()));
    assert_eq!(package.description, Some("Test description".to_string()));

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_rb_depends_without_version() {
    let rb_content = r#"
name             'test-cookbook'
version          '1.0.0'
depends "apt"
depends "yum"
"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_depends_no_version.rb");
    std::fs::write(&test_file, rb_content).unwrap();

    let package = ChefMetadataRbParser::extract_first_package(&test_file);

    assert_eq!(package.dependencies.len(), 2);

    let apt = package
        .dependencies
        .iter()
        .find(|d| d.purl.as_ref().map(|p| p.contains("apt")).unwrap_or(false))
        .expect("apt dependency not found");
    assert_eq!(apt.extracted_requirement, None);

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_rb_depends_with_version() {
    let rb_content = r#"
name             'test-cookbook'
version          '1.0.0'
depends "apt", ">= 2.0"
depends "yum", "~> 3.0"
"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_depends_with_version.rb");
    std::fs::write(&test_file, rb_content).unwrap();

    let package = ChefMetadataRbParser::extract_first_package(&test_file);

    assert_eq!(package.dependencies.len(), 2);

    let apt = package
        .dependencies
        .iter()
        .find(|d| d.purl.as_ref().map(|p| p.contains("apt")).unwrap_or(false))
        .expect("apt dependency not found");
    assert_eq!(apt.extracted_requirement, Some(">= 2.0".to_string()));

    let yum = package
        .dependencies
        .iter()
        .find(|d| d.purl.as_ref().map(|p| p.contains("yum")).unwrap_or(false))
        .expect("yum dependency not found");
    assert_eq!(yum.extracted_requirement, Some("~> 3.0".to_string()));

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_rb_long_description_fallback() {
    let rb_content = r#"
name             'test-cookbook'
version          '1.0.0'
long_description 'This is a long description'
"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_long_desc.rb");
    std::fs::write(&test_file, rb_content).unwrap();

    let package = ChefMetadataRbParser::extract_first_package(&test_file);

    assert_eq!(
        package.description,
        Some("This is a long description".to_string())
    );

    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_rb_description_preferred_over_long() {
    let rb_content = r#"
name             'test-cookbook'
version          '1.0.0'
description      'Short description'
long_description 'This is a long description'
"#;

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chef_desc_priority.rb");
    std::fs::write(&test_file, rb_content).unwrap();

    let package = ChefMetadataRbParser::extract_first_package(&test_file);

    assert_eq!(package.description, Some("Short description".to_string()));

    std::fs::remove_file(&test_file).unwrap();
}
