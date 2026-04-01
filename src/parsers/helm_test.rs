mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{HelmChartLockParser, HelmChartYamlParser, PackageParser};

    fn create_temp_file(file_name: &str, content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join(file_name);
        fs::write(&file_path, content).expect("Failed to write temp file");
        (temp_dir, file_path)
    }

    #[test]
    fn test_chart_yaml_is_match() {
        assert!(HelmChartYamlParser::is_match(&PathBuf::from("Chart.yaml")));
        assert!(!HelmChartYamlParser::is_match(&PathBuf::from("Chart.lock")));
        assert!(!HelmChartYamlParser::is_match(&PathBuf::from(
            "values.yaml"
        )));
    }

    #[test]
    fn test_chart_lock_is_match() {
        assert!(HelmChartLockParser::is_match(&PathBuf::from("Chart.lock")));
        assert!(!HelmChartLockParser::is_match(&PathBuf::from("Chart.yaml")));
        assert!(!HelmChartLockParser::is_match(&PathBuf::from(
            "requirements.lock"
        )));
    }

    #[test]
    fn test_extract_chart_yaml_metadata_and_dependencies() {
        let content = r#"
apiVersion: v2
name: nginx
version: 22.1.1
appVersion: 1.29.1
kubeVersion: ">=1.25.0-0"
description: Example Helm chart
type: application
home: https://example.com/nginx
icon: https://example.com/icon.png
keywords:
  - nginx
  - reverse-proxy
sources:
  - https://github.com/example/nginx-chart
maintainers:
  - name: Jane Doe
    email: jane@example.com
    url: https://example.com/jane
annotations:
  artifacthub.io/license: Apache-2.0
dependencies:
  - name: common
    version: 2.x.x
    repository: oci://registry-1.docker.io/bitnamicharts
    tags:
      - common
    alias: shared
  - name: metrics
    version: 1.2.3
    repository: https://charts.example.test
    condition: metrics.enabled
    import-values:
      - child: exports.data
        parent: metricsData
  - name: sidecar
    version: 1.2.3+linux.x86_64
    repository: https://charts.example.test
        "#;

        let (_temp_dir, path) = create_temp_file("Chart.yaml", content);
        let package_data = HelmChartYamlParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Helm));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::HelmChartYaml)
        );
        assert_eq!(package_data.name.as_deref(), Some("nginx"));
        assert_eq!(package_data.version.as_deref(), Some("22.1.1"));
        assert_eq!(package_data.purl.as_deref(), Some("pkg:helm/nginx@22.1.1"));
        assert_eq!(package_data.primary_language.as_deref(), Some("YAML"));
        assert_eq!(
            package_data.description.as_deref(),
            Some("Example Helm chart")
        );
        assert_eq!(
            package_data.homepage_url.as_deref(),
            Some("https://example.com/nginx")
        );
        assert_eq!(package_data.keywords, vec!["nginx", "reverse-proxy"]);
        assert_eq!(package_data.parties.len(), 1);
        assert_eq!(package_data.parties[0].role.as_deref(), Some("maintainer"));
        assert_eq!(package_data.parties[0].name.as_deref(), Some("Jane Doe"));
        assert_eq!(
            package_data.parties[0].email.as_deref(),
            Some("jane@example.com")
        );
        assert_eq!(
            package_data.parties[0].url.as_deref(),
            Some("https://example.com/jane")
        );

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing extra_data");
        assert_eq!(
            extra_data
                .get("api_version")
                .and_then(|value| value.as_str()),
            Some("v2")
        );
        assert_eq!(
            extra_data
                .get("app_version")
                .and_then(|value| value.as_str()),
            Some("1.29.1")
        );
        assert_eq!(
            extra_data
                .get("kube_version")
                .and_then(|value| value.as_str()),
            Some(">=1.25.0-0")
        );
        assert_eq!(
            extra_data
                .get("chart_type")
                .and_then(|value| value.as_str()),
            Some("application")
        );
        assert_eq!(
            extra_data.get("icon").and_then(|value| value.as_str()),
            Some("https://example.com/icon.png")
        );
        assert_eq!(
            extra_data
                .get("sources")
                .and_then(|value| value.as_array())
                .map(|values| values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .collect::<Vec<_>>()),
            Some(vec!["https://github.com/example/nginx-chart"])
        );
        assert_eq!(
            extra_data
                .get("annotations")
                .and_then(|value| value.get("artifacthub.io/license"))
                .and_then(|value| value.as_str()),
            Some("Apache-2.0")
        );

        assert_eq!(package_data.dependencies.len(), 3);
        let common = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:helm/common"))
            .expect("common dependency missing");
        assert_eq!(common.extracted_requirement.as_deref(), Some("2.x.x"));
        assert_eq!(common.is_pinned, Some(false));
        assert_eq!(common.is_runtime, Some(true));
        assert_eq!(common.is_optional, Some(true));
        let common_extra = common
            .extra_data
            .as_ref()
            .expect("common extra_data missing");
        assert_eq!(
            common_extra
                .get("repository")
                .and_then(|value| value.as_str()),
            Some("oci://registry-1.docker.io/bitnamicharts")
        );
        assert_eq!(
            common_extra.get("alias").and_then(|value| value.as_str()),
            Some("shared")
        );

        let metrics = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:helm/metrics@1.2.3"))
            .expect("metrics dependency missing");
        assert_eq!(metrics.extracted_requirement.as_deref(), Some("1.2.3"));
        assert_eq!(metrics.is_pinned, Some(true));
        assert_eq!(metrics.is_optional, Some(true));
        let metrics_extra = metrics
            .extra_data
            .as_ref()
            .expect("metrics extra_data missing");
        assert_eq!(
            metrics_extra
                .get("condition")
                .and_then(|value| value.as_str()),
            Some("metrics.enabled")
        );
        assert!(metrics_extra.get("import_values").is_some());

        let sidecar = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:helm/sidecar@1.2.3%2Blinux.x86_64"))
            .expect("sidecar dependency missing");
        assert_eq!(
            sidecar.extracted_requirement.as_deref(),
            Some("1.2.3+linux.x86_64")
        );
        assert_eq!(sidecar.is_pinned, Some(true));
        assert_eq!(sidecar.is_optional, Some(false));
    }

    #[test]
    fn test_extract_chart_lock_dependencies_and_metadata() {
        let content = r#"
dependencies:
  - name: common
    repository: oci://registry-1.docker.io/bitnamicharts
    version: 2.31.4
  - name: metrics
    repository: https://charts.example.test
    version: 1.2.3
digest: sha256:1234abcd
generated: "2025-08-13T19:31:58.340987883Z"
        "#;

        let (_temp_dir, path) = create_temp_file("Chart.lock", content);
        let package_data = HelmChartLockParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Helm));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::HelmChartLock)
        );
        assert_eq!(package_data.dependencies.len(), 2);

        let common = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:helm/common@2.31.4"))
            .expect("common lock dependency missing");
        assert_eq!(common.extracted_requirement.as_deref(), Some("2.31.4"));
        assert_eq!(common.is_pinned, Some(true));
        assert_eq!(common.is_runtime, Some(true));
        assert_eq!(common.is_optional, Some(false));

        let extra_data = package_data
            .extra_data
            .as_ref()
            .expect("missing lock extra_data");
        assert_eq!(
            extra_data.get("digest").and_then(|value| value.as_str()),
            Some("sha256:1234abcd")
        );
        assert_eq!(
            extra_data.get("generated").and_then(|value| value.as_str()),
            Some("2025-08-13T19:31:58.340987883Z")
        );
    }

    #[test]
    fn test_api_version_v1_chart_metadata_is_still_parsed() {
        let content = r#"
apiVersion: v1
name: legacy-chart
version: 0.9.0
description: Legacy Helm chart
home: https://example.com/legacy
        "#;

        let (_temp_dir, path) = create_temp_file("Chart.yaml", content);
        let package_data = HelmChartYamlParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("legacy-chart"));
        assert_eq!(package_data.version.as_deref(), Some("0.9.0"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:helm/legacy-chart@0.9.0")
        );
        assert!(package_data.dependencies.is_empty());
        assert_eq!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("api_version"))
                .and_then(|value| value.as_str()),
            Some("v1")
        );
    }

    #[test]
    fn test_api_version_v3_chart_metadata_is_parsed() {
        let content = r#"
apiVersion: v3
name: helm4-chart
version: 1.2.0
appVersion: 2.0.0
type: application
dependencies:
  - name: common
    version: 3.1.4
    repository: oci://registry-1.docker.io/bitnamicharts
        "#;

        let (_temp_dir, path) = create_temp_file("Chart.yaml", content);
        let package_data = HelmChartYamlParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("helm4-chart"));
        assert_eq!(package_data.version.as_deref(), Some("1.2.0"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:helm/helm4-chart@1.2.0")
        );
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|value| value.get("api_version"))
                .and_then(|value| value.as_str()),
            Some("v3")
        );

        let dependency = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:helm/common@3.1.4"))
            .expect("common dependency missing");
        assert_eq!(dependency.is_pinned, Some(true));
        assert_eq!(dependency.is_runtime, Some(true));
    }

    #[test]
    fn test_skips_malformed_dependency_entries_without_dropping_file() {
        let content = r#"
apiVersion: v2
name: resilient-chart
version: 1.0.0
dependencies:
  - name: valid-dep
    version: 1.0.0
    repository: https://charts.example.test
  - repository: https://charts.example.test
  - invalid-entry
        "#;

        let (_temp_dir, path) = create_temp_file("Chart.yaml", content);
        let package_data = HelmChartYamlParser::extract_first_package(&path);

        assert_eq!(package_data.name.as_deref(), Some("resilient-chart"));
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:helm/valid-dep@1.0.0")
        );
    }

    #[test]
    fn test_graceful_error_handling_for_invalid_yaml() {
        let (_temp_dir, chart_path) = create_temp_file("Chart.yaml", "[invalid_yaml");
        let package_data = HelmChartYamlParser::extract_first_package(&chart_path);

        assert_eq!(package_data.package_type, Some(PackageType::Helm));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::HelmChartYaml)
        );
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_exact_versions_with_x_in_build_metadata_stay_pinned() {
        let content = r#"
apiVersion: v2
name: exact-build-metadata
version: 1.0.0
dependencies:
  - name: sidecar
    version: 1.2.3+linux.x86_64
    repository: https://charts.example.test
  - name: wildcard
    version: 2.x.x
    repository: https://charts.example.test
        "#;

        let (_temp_dir, path) = create_temp_file("Chart.yaml", content);
        let package_data = HelmChartYamlParser::extract_first_package(&path);

        let sidecar = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:helm/sidecar@1.2.3%2Blinux.x86_64"))
            .expect("sidecar dependency missing");
        assert_eq!(sidecar.is_pinned, Some(true));

        let wildcard = package_data
            .dependencies
            .iter()
            .find(|dep| dep.extracted_requirement.as_deref() == Some("2.x.x"))
            .expect("wildcard dependency missing");
        assert_eq!(wildcard.purl.as_deref(), Some("pkg:helm/wildcard"));
        assert_eq!(wildcard.is_pinned, Some(false));
    }
}
