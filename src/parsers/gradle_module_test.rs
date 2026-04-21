// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::models::{DatasourceId, Md5Digest, PackageType, Sha1Digest, Sha256Digest};
    use crate::parsers::{GradleModuleParser, PackageParser};

    #[test]
    fn test_is_match_gradle_module() {
        assert!(GradleModuleParser::is_match(Path::new(
            "testdata/gradle-golden/module/simple.module"
        )));
        assert!(GradleModuleParser::is_match(Path::new(
            "testdata/gradle-golden/module/material-1.9.0.module"
        )));
        assert!(!GradleModuleParser::is_match(Path::new("build.gradle")));
        assert!(!GradleModuleParser::is_match(Path::new(
            "testdata/gradle-golden/module/not-a-gradle-module.module"
        )));
    }

    #[test]
    fn test_extract_simple_module() {
        let package_data = GradleModuleParser::extract_first_package(&PathBuf::from(
            "testdata/gradle-golden/module/simple.module",
        ));

        assert_eq!(package_data.package_type, Some(PackageType::Maven));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::GradleModule));
        assert_eq!(package_data.namespace.as_deref(), Some("com.example"));
        assert_eq!(package_data.name.as_deref(), Some("mylib"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:maven/com.example/mylib@1.0.0")
        );
        assert_eq!(package_data.primary_language.as_deref(), Some("Java"));

        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present");
        assert_eq!(
            extra_data
                .get("format_version")
                .and_then(|value| value.as_str()),
            Some("1.1")
        );
        assert_eq!(package_data.dependencies.len(), 0);
        assert_eq!(package_data.file_references.len(), 0);
    }

    #[test]
    fn test_extract_material_module_dedupes_and_classifies_dependencies() {
        let package_data = GradleModuleParser::extract_first_package(&PathBuf::from(
            "testdata/gradle-golden/module/material-1.9.0.module",
        ));

        assert_eq!(package_data.dependencies.len(), 3);
        assert_eq!(
            package_data.sha1,
            Some(Sha1Digest::from_hex("08f4a93a381be223a5bbaacd46eaab92381ab6a8").unwrap())
        );
        assert_eq!(
            package_data.md5,
            Some(Md5Digest::from_hex("3287103cfb083fb998a35ef8a1983c58").unwrap())
        );
        assert_eq!(
            package_data.sha256,
            Some(
                Sha256Digest::from_hex(
                    "6cc2359979269e4d9eddce7d84682d2bb06a35a14edce806bf0da6e8d4d31806"
                )
                .unwrap()
            )
        );
        assert_eq!(package_data.sha512, None);
        assert_eq!(package_data.file_references.len(), 1);

        let annotation = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref() == Some("pkg:maven/androidx.annotation/annotation@1.2.0")
            })
            .expect("annotation dependency missing");
        assert_eq!(annotation.scope.as_deref(), Some("compile"));
        assert_eq!(annotation.is_runtime, Some(true));
        assert_eq!(annotation.is_optional, None);
        let annotation_extra = annotation
            .extra_data
            .as_ref()
            .expect("annotation extra_data missing");
        let variant_entries = annotation_extra
            .get("variant_dependency_entries")
            .and_then(|value| value.as_array())
            .expect("variant_dependency_entries should be present");
        assert_eq!(variant_entries.len(), 2);
        assert!(variant_entries.iter().any(|entry| {
            entry
                .as_object()
                .and_then(|obj| obj.get("variant_name"))
                .and_then(|value| value.as_str())
                == Some("releaseApiElements")
        }));
        assert!(variant_entries.iter().any(|entry| {
            entry
                .as_object()
                .and_then(|obj| obj.get("variant_name"))
                .and_then(|value| value.as_str())
                == Some("releaseRuntimeElements")
        }));

        let appcompat = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:maven/androidx.appcompat/appcompat@1.5.0"))
            .expect("appcompat dependency missing");
        assert_eq!(appcompat.scope.as_deref(), Some("compile"));

        let error_prone = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.purl.as_deref()
                    == Some("pkg:maven/com.google.errorprone/error_prone_annotations@2.15.0")
            })
            .expect("error_prone dependency missing");
        assert_eq!(error_prone.scope.as_deref(), Some("runtime"));
        assert_eq!(error_prone.is_runtime, Some(true));
        assert_eq!(error_prone.is_optional, None);
    }

    #[test]
    fn test_variant_usage_beats_test_like_variant_name() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("usage-wins.module");
        let content = r#"{
  "formatVersion": "1.1",
  "component": {
    "group": "com.example",
    "module": "usage-wins",
    "version": "1.0.0"
  },
  "variants": [
    {
      "name": "testFixturesRuntimeElements",
      "attributes": {
        "org.gradle.usage": "java-runtime"
      },
      "dependencies": [
        {
          "group": "org.sample",
          "module": "dep",
          "version": { "requires": "1.0.0" }
        }
      ]
    }
  ]
}"#;
        fs::write(&file_path, content).unwrap();

        let package_data = GradleModuleParser::extract_first_package(&file_path);
        let dependency = &package_data.dependencies[0];

        assert_eq!(dependency.scope.as_deref(), Some("runtime"));
        assert_eq!(dependency.is_runtime, Some(true));
        assert_eq!(dependency.is_optional, None);
        assert_eq!(dependency.is_direct, Some(true));
    }

    #[test]
    fn test_extract_converter_module_skips_documentation_variant() {
        let package_data = GradleModuleParser::extract_first_package(&PathBuf::from(
            "testdata/gradle-golden/module/converter-moshi-2.11.0.module",
        ));

        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(package_data.size, Some(5292));
        assert_eq!(package_data.file_references.len(), 1);
        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present");
        assert_eq!(
            extra_data
                .get("gradle_version")
                .and_then(|value| value.as_str()),
            Some("8.7")
        );
    }

    #[test]
    fn test_extract_variant_metadata_with_dependency_constraints_and_available_at() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("delegated.module");
        let content = r#"{
  "formatVersion": "1.1",
  "component": {
    "group": "com.example",
    "module": "delegated",
    "version": "1.2.3"
  },
  "variants": [
    {
      "name": "apiElements",
      "attributes": {
        "org.gradle.usage": "java-api"
      },
      "dependencyConstraints": [
        {
          "group": "org.sample",
          "module": "platform",
          "version": { "strictly": "2.0.0" }
        }
      ],
      "available-at": {
        "url": "delegated.module",
        "group": "com.example",
        "module": "delegated-api",
        "version": "1.2.3"
      }
    }
  ]
}"#;
        fs::write(&file_path, content).unwrap();

        let package_data = GradleModuleParser::extract_first_package(&file_path);
        let extra_data = package_data
            .extra_data
            .expect("extra_data should be present");
        let variants = extra_data
            .get("variants")
            .and_then(|value| value.as_array())
            .expect("variants metadata should be present");
        assert_eq!(variants.len(), 1);
        let variant = variants[0].as_object().unwrap();
        assert!(variant.contains_key("dependency_constraints"));
        assert!(variant.contains_key("available_at"));
    }

    #[test]
    fn test_extract_preserves_conflicting_variant_dependency_metadata() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("conflicting.module");
        let content = r#"{
  "formatVersion": "1.1",
  "component": {
    "group": "com.example",
    "module": "conflicting",
    "version": "1.0.0"
  },
  "variants": [
    {
      "name": "apiElements",
      "attributes": {
        "org.gradle.usage": "java-api"
      },
      "dependencies": [
        {
          "group": "org.sample",
          "module": "dep",
          "version": { "requires": "1.0.0" },
          "reason": "api path"
        }
      ]
    },
    {
      "name": "runtimeElements",
      "attributes": {
        "org.gradle.usage": "java-runtime"
      },
      "dependencies": [
        {
          "group": "org.sample",
          "module": "dep",
          "version": { "requires": "1.0.0" },
          "reason": "runtime path",
          "requestedCapabilities": [
            {
              "group": "org.sample",
              "name": "cap",
              "version": "1.0"
            }
          ]
        }
      ]
    }
  ]
}"#;
        fs::write(&file_path, content).unwrap();

        let package_data = GradleModuleParser::extract_first_package(&file_path);
        assert_eq!(package_data.dependencies.len(), 1);

        let dependency = &package_data.dependencies[0];
        let extra_data = dependency
            .extra_data
            .as_ref()
            .expect("dependency extra_data missing");
        let variant_entries = extra_data
            .get("variant_dependency_entries")
            .and_then(|value| value.as_array())
            .expect("variant_dependency_entries should be present");
        assert_eq!(variant_entries.len(), 2);
        assert!(variant_entries.iter().any(|entry| {
            let entry = entry.as_object().unwrap();
            entry.get("variant_name").and_then(|value| value.as_str()) == Some("apiElements")
                && entry
                    .get("dependency")
                    .and_then(|value| value.get("reason"))
                    .and_then(|value| value.as_str())
                    == Some("api path")
        }));
        assert!(variant_entries.iter().any(|entry| {
            let entry = entry.as_object().unwrap();
            entry.get("variant_name").and_then(|value| value.as_str()) == Some("runtimeElements")
                && entry
                    .get("dependency")
                    .and_then(|value| value.get("requestedCapabilities"))
                    .and_then(|value| value.as_array())
                    .is_some()
        }));
    }

    #[test]
    fn test_extract_invalid_module_returns_default() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("broken.module");
        fs::write(&file_path, "{not-json").unwrap();

        let package_data = GradleModuleParser::extract_first_package(&file_path);
        assert_eq!(package_data.datasource_id, Some(DatasourceId::GradleModule));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }
}
