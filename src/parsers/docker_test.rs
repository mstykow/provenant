// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::docker::{DockerfileParser, parse_dockerfile};
    use crate::models::{DatasourceId, PackageType};
    use std::path::PathBuf;

    #[test]
    fn test_is_match_dockerfile_and_containerfile_variants() {
        assert!(DockerfileParser::is_match(&PathBuf::from("Dockerfile")));
        assert!(DockerfileParser::is_match(&PathBuf::from("dockerfile")));
        assert!(DockerfileParser::is_match(&PathBuf::from("Containerfile")));
        assert!(DockerfileParser::is_match(&PathBuf::from(
            "containerfile.core"
        )));

        assert!(!DockerfileParser::is_match(&PathBuf::from(
            "Dockerfile.dev"
        )));
        assert!(!DockerfileParser::is_match(&PathBuf::from(
            "docker-compose.yml"
        )));
        assert!(!DockerfileParser::is_match(&PathBuf::from(
            "my-Containerfile.txt"
        )));
    }

    #[test]
    fn test_parse_oci_labels_from_dockerfile() {
        let content = r#"
FROM docker.io/library/debian:bookworm-slim

LABEL org.opencontainers.image.title="Jitsi Broadcasting Infrastructure (jibri)" \
      org.opencontainers.image.description="Components for recording and/or streaming a conference." \
      org.opencontainers.image.url="https://github.com/jitsi/jibri" \
      org.opencontainers.image.source="https://github.com/jitsi/docker-jitsi-meet" \
      org.opencontainers.image.documentation="https://jitsi.github.io/handbook/" \
      org.opencontainers.image.version="stable-8960-1" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.revision="abcdef123456"
"#;

        let package = parse_dockerfile(content);

        assert_eq!(package.package_type, Some(PackageType::Docker));
        assert_eq!(package.primary_language, Some("Dockerfile".to_string()));
        assert_eq!(package.datasource_id, Some(DatasourceId::Dockerfile));
        assert_eq!(
            package.name.as_deref(),
            Some("Jitsi Broadcasting Infrastructure (jibri)")
        );
        assert_eq!(
            package.description.as_deref(),
            Some("Components for recording and/or streaming a conference.")
        );
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://github.com/jitsi/jibri")
        );
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://github.com/jitsi/docker-jitsi-meet")
        );
        assert_eq!(package.version.as_deref(), Some("stable-8960-1"));
        assert_eq!(
            package.extracted_license_statement.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            package.declared_license_expression.as_deref(),
            Some("apache-2.0")
        );
        assert_eq!(
            package.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(package.license_detections.len(), 1);
        assert_eq!(
            package.license_detections[0].license_expression_spdx,
            "Apache-2.0"
        );

        let oci_labels = package
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("oci_labels"))
            .and_then(|value| value.as_object())
            .expect("oci_labels should be collected");

        assert_eq!(
            oci_labels
                .get("org.opencontainers.image.documentation")
                .and_then(|value| value.as_str()),
            Some("https://jitsi.github.io/handbook/")
        );
        assert_eq!(
            oci_labels
                .get("org.opencontainers.image.revision")
                .and_then(|value| value.as_str()),
            Some("abcdef123456")
        );
    }

    #[test]
    fn test_parse_old_style_label_value() {
        let package =
            parse_dockerfile("LABEL org.opencontainers.image.title \"Example Container\"\n");

        assert_eq!(package.name.as_deref(), Some("Example Container"));
    }

    #[test]
    fn test_parse_old_style_label_value_with_equals_sign() {
        let package = parse_dockerfile(
            "LABEL org.opencontainers.image.description \"mode=a=b compatibility\"\n",
        );

        assert_eq!(
            package.description.as_deref(),
            Some("mode=a=b compatibility")
        );
    }

    #[test]
    fn test_parse_repeated_labels_override_previous_values() {
        let package = parse_dockerfile(
            "LABEL org.opencontainers.image.title=\"First\"\nLABEL org.opencontainers.image.title=\"Second\"\n",
        );

        assert_eq!(package.name.as_deref(), Some("Second"));
    }

    #[test]
    fn test_parse_dockerfile_without_oci_labels_still_returns_package_data() {
        let package = parse_dockerfile("FROM scratch\nRUN echo hello\n");

        assert_eq!(package.package_type, Some(PackageType::Docker));
        assert_eq!(package.primary_language, Some("Dockerfile".to_string()));
        assert_eq!(package.datasource_id, Some(DatasourceId::Dockerfile));
        assert!(package.extra_data.is_none());
    }
}
