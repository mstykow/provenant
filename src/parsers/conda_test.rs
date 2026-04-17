#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::conda::{
        CondaEnvironmentYmlParser, CondaMetaYamlParser, apply_jinja2_substitutions,
        extract_jinja2_variables, parse_conda_requirement,
    };
    use crate::models::PackageType;
    use crate::models::Sha256Digest;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ==================== is_match() Tests ====================

    #[test]
    fn test_conda_meta_yaml_is_match_meta_yaml() {
        let valid_path = PathBuf::from("/some/path/meta.yaml");
        assert!(CondaMetaYamlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_meta_yaml_is_match_meta_yml() {
        let valid_path = PathBuf::from("/some/path/meta.yml");
        assert!(CondaMetaYamlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_meta_yaml_is_match_recipe_yaml() {
        let valid_path = PathBuf::from("/some/path/recipe/recipe.yaml");
        assert!(CondaMetaYamlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_meta_yaml_is_match_invalid() {
        let invalid_path = PathBuf::from("/some/path/metadata.yaml");
        assert!(!CondaMetaYamlParser::is_match(&invalid_path));

        let invalid_path2 = PathBuf::from("/some/path/meta.txt");
        assert!(!CondaMetaYamlParser::is_match(&invalid_path2));
    }

    #[test]
    fn test_conda_environment_yml_is_match_environment_yaml() {
        let valid_path = PathBuf::from("/some/path/environment.yaml");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_environment_yml_is_match_environment_yml() {
        let valid_path = PathBuf::from("/some/path/environment.yml");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_environment_yml_is_match_conda_yaml() {
        let valid_path = PathBuf::from("/some/path/conda.yaml");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_environment_yml_is_match_env_yaml() {
        let valid_path = PathBuf::from("/some/path/env.yaml");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path));
    }

    #[test]
    fn test_conda_environment_yml_is_match_case_insensitive() {
        let valid_path = PathBuf::from("/some/path/ENVIRONMENT.YAML");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path));

        let valid_path2 = PathBuf::from("/some/path/CONDA.YML");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path2));
    }

    #[test]
    fn test_conda_environment_yml_is_match_invalid() {
        let invalid_path = PathBuf::from("/some/path/environment.txt");
        assert!(!CondaEnvironmentYmlParser::is_match(&invalid_path));

        let invalid_path2 = PathBuf::from("/some/path/requirements.txt");
        assert!(!CondaEnvironmentYmlParser::is_match(&invalid_path2));
    }

    // ==================== Jinja2 Tests ====================

    #[test]
    fn test_extract_jinja2_variables_simple() {
        let content = r#"{% set version = "0.45.0" %}
{% set sha256 = "abc123" %}
package:
  name: test"#;

        let vars = extract_jinja2_variables(content);

        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("version"), Some(&"0.45.0".to_string()));
        assert_eq!(vars.get("sha256"), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_extract_jinja2_variables_single_quotes() {
        let content = "{% set version = '1.2.3' %}";

        let vars = extract_jinja2_variables(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("version"), Some(&"1.2.3".to_string()));
    }

    #[test]
    fn test_extract_jinja2_variables_empty_content() {
        let content = "package:\n  name: test\nversion: 1.0";

        let vars = extract_jinja2_variables(content);

        assert_eq!(vars.len(), 0);
    }

    #[test]
    fn test_apply_jinja2_substitutions_simple() {
        let mut variables = HashMap::new();
        variables.insert("version".to_string(), "0.45.0".to_string());
        variables.insert("sha256".to_string(), "abc123".to_string());

        let content = r#"{% set version = "0.45.0" %}
package:
  version: {{ version }}
source:
  sha256: {{ sha256 }}"#;

        let result = apply_jinja2_substitutions(content, &variables);

        assert!(result.contains("version: 0.45.0"));
        assert!(result.contains("sha256: abc123"));
        // Jinja2 set lines should be skipped
        assert!(!result.contains("{% set version"));
    }

    #[test]
    fn test_apply_jinja2_substitutions_with_lower_filter() {
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "MyPackage".to_string());

        let content = "url: https://example.com/packages/{{ name|lower }}/archive.tar.gz";

        let result = apply_jinja2_substitutions(content, &variables);

        assert!(result.contains("mypackage"));
        assert!(!result.contains("MyPackage"));
    }

    #[test]
    fn test_apply_jinja2_substitutions_unresolved_removed() {
        let variables = HashMap::new();

        let content = r#"{% set name = "test" %}
package:
  name: {{ unresolved_var }}
  version: 1.0"#;

        let result = apply_jinja2_substitutions(content, &variables);

        // Lines with unresolved variables should be skipped
        assert!(!result.contains("unresolved_var"));
    }

    // ==================== parse_conda_requirement() Tests ====================

    #[test]
    fn test_parse_conda_requirement_pinned_space_separated() {
        // Format: "package ==version" (note: expected as-per reference shows version in PURL but is_pinned=false)
        let dep = parse_conda_requirement("mccortex ==1.0", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.purl, Some("pkg:conda/mccortex@1.0".to_string()));
        assert_eq!(dep.extracted_requirement, Some("==1.0".to_string()));
        assert_eq!(dep.scope, Some("run".to_string()));
        assert_eq!(dep.is_runtime, Some(true));
        assert_eq!(dep.is_optional, Some(false));
        assert_eq!(dep.is_pinned, Some(false));
    }

    #[test]
    fn test_parse_conda_requirement_pinned_no_space() {
        // Format: "package=version"
        let dep = parse_conda_requirement("mccortex=1.0", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.purl, Some("pkg:conda/mccortex@1.0".to_string()));
        assert_eq!(dep.extracted_requirement, Some("=1.0".to_string()));
        assert_eq!(dep.scope, Some("run".to_string()));
        assert_eq!(dep.is_runtime, Some(true));
        assert_eq!(dep.is_pinned, Some(true));
    }

    #[test]
    fn test_parse_conda_requirement_version_constraint() {
        // Format: "package >=version"
        let dep = parse_conda_requirement("python >=3.6", "host");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.purl, Some("pkg:conda/python".to_string()));
        assert_eq!(dep.extracted_requirement, Some(">=3.6".to_string()));
        assert_eq!(dep.scope, Some("host".to_string()));
        assert_eq!(dep.is_pinned, Some(false));
    }

    #[test]
    fn test_parse_conda_requirement_with_namespace() {
        // Format: "namespace::package=version"
        let dep = parse_conda_requirement("conda-forge::numpy=1.15.4", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(
            dep.purl,
            Some("pkg:conda/conda-forge/numpy@1.15.4".to_string())
        );
        assert_eq!(dep.is_pinned, Some(true));
    }

    #[test]
    fn test_parse_conda_requirement_no_version() {
        // Format: "package"
        let dep = parse_conda_requirement("bwa", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.purl, Some("pkg:conda/bwa".to_string()));
        assert_eq!(dep.extracted_requirement, Some(String::new()));
        assert_eq!(dep.is_pinned, Some(false));
    }

    #[test]
    fn test_parse_conda_requirement_build_scope() {
        // Non-runtime scopes should be marked as optional
        let dep = parse_conda_requirement("cmake", "build");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.is_runtime, Some(false));
        assert_eq!(dep.is_optional, Some(true));
    }

    #[test]
    fn test_environment_top_level_dependencies_are_conda_unless_explicitly_pip() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join("environment.yml");
        fs::write(
            &env_path,
            r#"
name: test-env
channels:
  - conda-forge
dependencies:
  - conda-forge::numpy
  - pandas>=2
  - pip:
      - ray
"#,
        )
        .unwrap();

        let package_data = CondaEnvironmentYmlParser::extract_first_package(&env_path);

        let numpy = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/conda-forge/numpy"))
            .expect("conda numpy dependency missing");
        assert_eq!(numpy.scope.as_deref(), Some("dependencies"));

        let pandas = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/pandas"))
            .expect("conda pandas dependency missing");
        assert_eq!(pandas.extracted_requirement.as_deref(), Some(">=2"));

        let ray = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:pypi/ray"))
            .expect("pip ray dependency missing");
        assert_eq!(ray.scope.as_deref(), Some("dependencies"));
    }

    #[test]
    fn test_environment_top_level_dependencies_preserve_build_string_separately() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join("environment.yml");
        fs::write(
            &env_path,
            r#"
name: test-env
channels:
  - defaults
dependencies:
  - bzip2=1.0.8=h4bc722e_7
  - defaults::openssl=3.4.0=h7b32b05_1
"#,
        )
        .unwrap();

        let package_data = CondaEnvironmentYmlParser::extract_first_package(&env_path);

        let bzip2 = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/bzip2@1.0.8"))
            .expect("conda bzip2 dependency missing");
        assert_eq!(
            bzip2.extracted_requirement.as_deref(),
            Some("=1.0.8=h4bc722e_7")
        );
        assert_eq!(
            bzip2
                .extra_data
                .as_ref()
                .and_then(|m| m.get("build_string"))
                .and_then(|v| v.as_str()),
            Some("h4bc722e_7")
        );

        let openssl = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:conda/defaults/openssl@3.4.0"))
            .expect("conda openssl dependency missing");
        assert_eq!(
            openssl.extracted_requirement.as_deref(),
            Some("=3.4.0=h7b32b05_1")
        );
        assert_eq!(
            openssl
                .extra_data
                .as_ref()
                .and_then(|m| m.get("channel"))
                .and_then(|v| v.as_str()),
            Some("defaults")
        );
        assert_eq!(
            openssl
                .extra_data
                .as_ref()
                .and_then(|m| m.get("build_string"))
                .and_then(|v| v.as_str()),
            Some("h7b32b05_1")
        );
    }

    #[test]
    fn test_environment_yaml_without_conda_structure_returns_no_packages() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join("pod-with-api-env.yaml");
        fs::write(
            &env_path,
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: env-test-pod
spec:
  containers:
    - name: test-container
      image: registry.k8s.io/busybox
      env:
        - name: TEST_CMD_1
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
"#,
        )
        .unwrap();

        assert!(CondaEnvironmentYmlParser::extract_packages(&env_path).is_empty());
    }

    // ==================== extract_first_package() Tests ====================

    #[test]
    fn test_extract_meta_yaml_abeona() {
        let path = PathBuf::from("testdata/conda/meta-yaml/abeona/meta.yaml");
        let package_data = CondaMetaYamlParser::extract_first_package(&path);

        // Basic package info
        assert_eq!(package_data.package_type, Some(PackageType::Conda));
        assert_eq!(package_data.name, Some("abeona".to_string()));
        assert_eq!(package_data.version, Some("0.45.0".to_string()));

        // URLs
        assert_eq!(
            package_data.homepage_url,
            Some("https://github.com/winni2k/abeona".to_string())
        );
        assert_eq!(
            package_data.download_url,
            Some("https://pypi.io/packages/source/a/abeona/abeona-0.45.0.tar.gz".to_string())
        );

        // SHA256
        assert_eq!(
            package_data.sha256,
            Some(
                Sha256Digest::from_hex(
                    "bc7512f2eef785b037d836f4cc6faded457ac277f75c6e34eccd12da7c85258f"
                )
                .unwrap()
            )
        );

        // License
        assert_eq!(
            package_data.extracted_license_statement,
            Some("Apache Software".to_string())
        );

        // Description
        assert_eq!(
            package_data.description,
            Some(
                "A simple transcriptome assembler based on kallisto and Cortex graphs.".to_string()
            )
        );

        // VCS URL
        assert_eq!(
            package_data.vcs_url,
            Some("https://github.com/winni2k/abeona".to_string())
        );
    }

    #[test]
    fn test_extract_meta_yaml_abeona_dependencies() {
        let path = PathBuf::from("testdata/conda/meta-yaml/abeona/meta.yaml");
        let package_data = CondaMetaYamlParser::extract_first_package(&path);

        // Should have 7 dependencies (python >=3.6 in run scope)
        let deps = &package_data.dependencies;
        assert_eq!(deps.len(), 7);

        // Check mccortex ==1.0
        let mccortex = deps
            .iter()
            .find(|d| d.purl.as_deref().is_some_and(|p| p.contains("mccortex")));
        assert!(mccortex.is_some());
        let mccortex = mccortex.unwrap();
        assert_eq!(mccortex.extracted_requirement, Some("==1.0".to_string()));
        assert_eq!(mccortex.is_runtime, Some(true));
        assert_eq!(mccortex.scope, Some("run".to_string()));

        // Check nextflow ==19.01.0 (space-separated, so is_pinned=false but version in PURL)
        let nextflow = deps
            .iter()
            .find(|d| d.purl.as_deref().is_some_and(|p| p.contains("nextflow")));
        assert!(nextflow.is_some());
        let nextflow = nextflow.unwrap();
        assert_eq!(
            nextflow.purl,
            Some("pkg:conda/nextflow@19.01.0".to_string())
        );
        assert_eq!(nextflow.is_pinned, Some(false));

        // Check bwa (no version)
        let bwa = deps
            .iter()
            .find(|d| d.purl.as_deref().is_some_and(|p| p.contains("bwa")));
        assert!(bwa.is_some());
        let bwa = bwa.unwrap();
        assert_eq!(bwa.purl, Some("pkg:conda/bwa".to_string()));
        assert_eq!(bwa.extracted_requirement, Some(String::new()));
        assert_eq!(bwa.is_pinned, Some(false));

        // Check pandas (no version)
        let pandas = deps
            .iter()
            .find(|d| d.purl.as_deref().is_some_and(|p| p.contains("pandas")));
        assert!(pandas.is_some());
    }

    #[test]
    fn test_extract_meta_yaml_abeona_extra_data() {
        let path = PathBuf::from("testdata/conda/meta-yaml/abeona/meta.yaml");
        let package_data = CondaMetaYamlParser::extract_first_package(&path);

        // Check extra_data contains pip and python
        let extra_data = package_data.extra_data.unwrap_or_default();
        assert!(extra_data.contains_key("host"));
        assert!(extra_data.contains_key("run"));

        let host_deps = extra_data.get("host").unwrap().as_array();
        assert!(host_deps.is_some());
        let host_deps = host_deps.unwrap();
        assert!(host_deps.iter().any(|v| v.as_str() == Some("pip")));
        assert!(host_deps.iter().any(|v| v.as_str() == Some("python")));

        let run_deps = extra_data.get("run").unwrap().as_array();
        assert!(run_deps.is_some());
        let run_deps = run_deps.unwrap();
        assert!(run_deps.iter().any(|v| v.as_str() == Some("python >=3.6")));
    }

    #[test]
    fn test_extract_meta_yaml_abeona_preserves_license_file_reference() {
        let path = PathBuf::from("testdata/conda/meta-yaml/abeona/meta.yaml");
        let package_data = CondaMetaYamlParser::extract_first_package(&path);

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data should exist");
        assert_eq!(
            extra_data.get("license_file").and_then(|v| v.as_str()),
            Some("LICENSE")
        );

        let referenced_filenames = package_data.license_detections[0].matches[0]
            .referenced_filenames
            .as_ref()
            .expect("referenced_filenames should be present");
        assert_eq!(referenced_filenames, &vec!["LICENSE".to_string()]);
    }

    #[test]
    fn test_extract_environment_yaml_ringer() {
        let path = PathBuf::from("testdata/conda/conda-yaml/ringer/environment.yaml");
        let package_data = CondaEnvironmentYmlParser::extract_first_package(&path);

        // Basic info
        assert_eq!(package_data.package_type, Some(PackageType::Conda));
        assert_eq!(package_data.name, Some("ringer".to_string()));
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.primary_language, Some("Python".to_string()));

        // Check channels in extra_data
        let extra_data = package_data.extra_data.unwrap_or_default();
        assert!(extra_data.contains_key("channels"));
        let channels = extra_data.get("channels").unwrap().as_array();
        assert!(channels.is_some());
        let channels = channels.unwrap();
        assert_eq!(channels.len(), 3);
        assert!(channels.iter().any(|v| v.as_str() == Some("pytorch")));
        assert!(channels.iter().any(|v| v.as_str() == Some("conda-forge")));
        assert!(channels.iter().any(|v| v.as_str() == Some("huggingface")));
    }

    #[test]
    fn test_extract_environment_yaml_ringer_conda_dependencies() {
        let path = PathBuf::from("testdata/conda/conda-yaml/ringer/environment.yaml");
        let package_data = CondaEnvironmentYmlParser::extract_first_package(&path);

        let deps = &package_data.dependencies;

        // Should have conda dependencies with namespaces + conda-forge packages + pip (ray)
        // pytorch::pytorch, huggingface::transformers, conda-forge::* (5), pip:ray
        // Total conda: 8 (3 namespaced + 5 conda-forge) + pypi: 1 (ray) = 9 total

        // Check pytorch with namespace
        let pytorch = deps.iter().find(|d| {
            d.purl
                .as_deref()
                .is_some_and(|p| p.contains("pytorch/pytorch"))
        });
        assert!(pytorch.is_some());
        let pytorch = pytorch.unwrap();
        assert_eq!(
            pytorch.purl,
            Some("pkg:conda/pytorch/pytorch@1.12".to_string())
        );
        assert_eq!(pytorch.extracted_requirement, Some("=1.12".to_string()));
        assert_eq!(pytorch.is_pinned, Some(true));
        assert_eq!(pytorch.is_runtime, Some(true));

        // Check transformers with namespace
        let transformers = deps.iter().find(|d| {
            d.purl
                .as_deref()
                .is_some_and(|p| p.contains("transformers"))
        });
        assert!(transformers.is_some());
        let transformers = transformers.unwrap();
        assert_eq!(
            transformers.purl,
            Some("pkg:conda/huggingface/transformers@4.11.3".to_string())
        );

        // Check numpy (conda-forge namespace packages)
        let numpy = deps
            .iter()
            .find(|d| d.purl.as_deref().is_some_and(|p| p.contains("numpy")));
        assert!(numpy.is_some());
        let numpy = numpy.unwrap();
        assert_eq!(numpy.purl, Some("pkg:conda/conda-forge/numpy".to_string()));

        // Check ray (pip dependency)
        let ray = deps
            .iter()
            .find(|d| d.purl.as_deref().is_some_and(|p| p.contains("ray")));
        assert!(ray.is_some());
        let ray = ray.unwrap();
        assert_eq!(ray.purl, Some("pkg:pypi/ray".to_string()));
        assert_eq!(ray.scope, Some("dependencies".to_string()));
    }

    #[test]
    fn test_extract_environment_yaml_ringer_pip_filtering() {
        let path = PathBuf::from("testdata/conda/conda-yaml/ringer/environment.yaml");
        let package_data = CondaEnvironmentYmlParser::extract_first_package(&path);

        let deps = &package_data.dependencies;

        // python and pip should be filtered out
        assert!(
            !deps
                .iter()
                .any(|d| d.purl.as_deref().is_some_and(|p| p.contains("python")))
        );
        assert!(
            !deps
                .iter()
                .any(|d| d.purl.as_deref().is_some_and(|p| p.contains("pip")))
        );
    }

    #[test]
    fn test_parse_conda_requirement_with_namespace_pytorch() {
        let dep = parse_conda_requirement("pytorch::pytorch=1.12", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.purl, Some("pkg:conda/pytorch/pytorch@1.12".to_string()));
        assert_eq!(dep.scope, Some("run".to_string()));
        assert_eq!(dep.is_pinned, Some(true));
        assert_eq!(
            dep.extra_data
                .as_ref()
                .and_then(|m| m.get("channel"))
                .and_then(|v| v.as_str()),
            Some("pytorch")
        );
    }

    #[test]
    fn test_parse_conda_requirement_with_url_prefix_does_not_become_namespace() {
        let dep = parse_conda_requirement("https://104.17.92.24:443::flask=1.0.2", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.purl, Some("pkg:conda/flask@1.0.2".to_string()));
        assert_eq!(
            dep.extra_data
                .as_ref()
                .and_then(|m| m.get("channel_url"))
                .and_then(|v| v.as_str()),
            Some("https://104.17.92.24:443")
        );
    }

    #[test]
    fn test_parse_conda_requirement_trimmed() {
        let dep = parse_conda_requirement("  cortexpy  ==0.45.7  ", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert!(dep.purl.as_deref().is_some_and(|p| p.contains("cortexpy")));
        assert_eq!(dep.extracted_requirement, Some("==0.45.7".to_string()));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_extract_jinja2_variables_with_spaces() {
        let content = "{% set version =   \"1.2.3\"   %}";

        let vars = extract_jinja2_variables(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("version"), Some(&"1.2.3".to_string()));
    }

    #[test]
    fn test_extract_jinja2_variables_with_selector_comment() {
        let content = r#"{% set version = "2.0.1" %}  # [unix]"#;

        let vars = extract_jinja2_variables(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("version"), Some(&"2.0.1".to_string()));
    }

    #[test]
    fn test_apply_jinja2_substitutions_multiple_occurrences() {
        let mut variables = HashMap::new();
        variables.insert("version".to_string(), "1.0".to_string());

        let content = "url: https://example.com/{{ version }}/file-{{ version }}.tar.gz";

        let result = apply_jinja2_substitutions(content, &variables);

        assert_eq!(result.matches("1.0").count(), 2);
    }

    #[test]
    fn test_apply_jinja2_substitutions_quotes_numeric_version_scalars() {
        let mut variables = HashMap::new();
        variables.insert("version".to_string(), "0.80".to_string());

        let content = r#"{% set version = "0.80" %}
package:
  version: {{ version }}
outputs:
  - version: 1.10
"#;

        let result = apply_jinja2_substitutions(content, &variables);

        assert!(result.contains("version: \"0.80\""));
        assert!(result.contains("version: \"1.10\""));
    }

    #[test]
    fn test_apply_jinja2_substitutions_skips_selector_commented_jinja_statement() {
        let content = r#"{% set markers = ["not slow"] %}  # [linux]
test:
  commands:
    - python -V"#;

        let result = apply_jinja2_substitutions(content, &HashMap::new());

        assert!(!result.contains("{% set markers"));
        assert!(result.contains("test:"));
        assert!(result.contains("- python -V"));
    }

    #[test]
    fn test_extract_meta_yaml_with_selector_commented_jinja_statements() {
        let temp_dir = TempDir::new().unwrap();
        let meta_path = temp_dir.path().join("meta.yaml");
        fs::write(
            &meta_path,
            r#"{% set version = "2.0.1" %}

package:
  name: pandas
  version: {{ version }}

requirements:
  host:
    - python
    - numpy >=1.23

test:
  commands:
    {% set markers = ["not slow"] %}   # [linux]
    - python -V

about:
  license: BSD-3-Clause
  summary: Powerful data structures for data analysis"#,
        )
        .unwrap();

        let package_data = CondaMetaYamlParser::extract_first_package(&meta_path);

        assert_eq!(package_data.package_type, Some(PackageType::Conda));
        assert_eq!(package_data.name.as_deref(), Some("pandas"));
        assert_eq!(package_data.version.as_deref(), Some("2.0.1"));
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("BSD-3-Clause")
        );
    }

    #[test]
    fn test_extract_meta_yaml_preserves_float_like_version_strings() {
        let temp_dir = TempDir::new().unwrap();
        let meta_path = temp_dir.path().join("meta.yaml");
        fs::write(
            &meta_path,
            r#"
package:
  name: test_floating_point_version
  version: 1.10
"#,
        )
        .unwrap();

        let package_data = CondaMetaYamlParser::extract_first_package(&meta_path);

        assert_eq!(
            package_data.name.as_deref(),
            Some("test_floating_point_version")
        );
        assert_eq!(package_data.version.as_deref(), Some("1.10"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:conda/test_floating_point_version@1.10")
        );
    }

    #[test]
    fn test_parse_conda_requirement_empty_version() {
        let dep = parse_conda_requirement("package=", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert!(dep.purl.as_deref().is_some_and(|p| p.contains("package")));
    }

    #[test]
    fn test_conda_environment_yml_is_match_hyphenated() {
        // Test that "conda-env.yaml" matches (contains "conda")
        let valid_path = PathBuf::from("/some/path/conda-env.yaml");
        assert!(CondaEnvironmentYmlParser::is_match(&valid_path));
    }

    #[test]
    fn test_parse_conda_requirement_double_equals() {
        let dep = parse_conda_requirement("package ==1.2.3", "run");

        assert!(dep.is_some());
        let dep = dep.unwrap();
        assert_eq!(dep.extracted_requirement, Some("==1.2.3".to_string()));
        assert_eq!(dep.is_pinned, Some(false));
    }

    #[test]
    fn test_extract_recipe_yaml_schema_v1_feedstock() {
        let temp_dir = TempDir::new().unwrap();
        let recipe_dir = temp_dir.path().join("recipe");
        fs::create_dir_all(&recipe_dir).unwrap();
        let recipe_path = recipe_dir.join("recipe.yaml");
        fs::write(
            &recipe_path,
            r#"
schema_version: 1

context:
  version: "3.0.2"

package:
  name: pandas
  version: ${{ version }}

source:
  url: https://github.com/pandas-dev/pandas/releases/download/v${{ version }}/pandas-${{ version }}.tar.gz
  sha256: f4753e73e34c8d83221ba58f232433fca2748be8b18dbca02d242ed153945043

requirements:
  build:
    - ${{ compiler('c') }}
  host:
    - python
    - numpy =2.3
  run:
    - python
    - numpy >=1.26.0
    - python-dateutil >=2.8.2
    - if: win
      then: python-tzdata

about:
  license: BSD-3-Clause
  license_file: LICENSE
  summary: >-
    Powerful data structures for data analysis, time series, and statistics
  homepage: http://pandas.pydata.org
  repository: https://github.com/pandas-dev/pandas
  documentation: https://pandas.pydata.org/docs/
"#,
        )
        .unwrap();

        let package_data = CondaMetaYamlParser::extract_first_package(&recipe_path);

        assert_eq!(package_data.package_type, Some(PackageType::Conda));
        assert_eq!(package_data.name.as_deref(), Some("pandas"));
        assert_eq!(package_data.version.as_deref(), Some("3.0.2"));
        assert_eq!(
            package_data.download_url.as_deref(),
            Some(
                "https://github.com/pandas-dev/pandas/releases/download/v3.0.2/pandas-3.0.2.tar.gz"
            )
        );
        assert_eq!(
            package_data.homepage_url.as_deref(),
            Some("http://pandas.pydata.org")
        );
        assert_eq!(
            package_data.vcs_url.as_deref(),
            Some("https://github.com/pandas-dev/pandas")
        );
        assert_eq!(
            package_data.extracted_license_statement.as_deref(),
            Some("BSD-3-Clause")
        );
        assert_eq!(
            package_data.description.as_deref(),
            Some("Powerful data structures for data analysis, time series, and statistics")
        );

        let dependency_purls: Vec<&str> = package_data
            .dependencies
            .iter()
            .filter_map(|dep| dep.purl.as_deref())
            .collect();
        assert!(dependency_purls.contains(&"pkg:conda/numpy"));
        assert!(dependency_purls.contains(&"pkg:conda/python-dateutil"));
        assert!(dependency_purls.contains(&"pkg:conda/python-tzdata"));
        assert!(
            !dependency_purls
                .iter()
                .any(|purl| purl.contains("compiler"))
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("extra_data missing");
        assert_eq!(
            extra_data
                .get("schema_version")
                .and_then(|value| value.as_i64()),
            Some(1)
        );
        assert_eq!(
            extra_data
                .get("license_file")
                .and_then(|value| value.as_str()),
            Some("LICENSE")
        );
        assert_eq!(
            extra_data
                .get("documentation")
                .and_then(|value| value.as_str()),
            Some("https://pandas.pydata.org/docs/")
        );
        assert_eq!(
            extra_data
                .get("host")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
        assert_eq!(
            extra_data
                .get("run")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
    }
}
