#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{DenoLockParser, PackageParser};

    #[test]
    fn test_is_match() {
        assert!(DenoLockParser::is_match(Path::new("deno.lock")));
        assert!(!DenoLockParser::is_match(Path::new("package-lock.json")));
    }

    #[test]
    fn test_extract_from_deno_lock_v5() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("deno.lock");
        fs::write(&file_path, sample_deno_lock()).unwrap();

        let package_data = DenoLockParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Deno));
        assert_eq!(package_data.primary_language.as_deref(), Some("TypeScript"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::DenoLock));
        assert_eq!(
            package_data
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("version"))
                .and_then(|value| value.as_str()),
            Some("5")
        );

        let direct_assert = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:generic/jsr.io/%40std/assert@1.0.19"))
            .unwrap();
        assert_eq!(direct_assert.is_direct, Some(true));
        assert_eq!(direct_assert.is_pinned, Some(true));

        let transitive_internal = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:generic/jsr.io/%40std/internal@1.0.12"))
            .unwrap();
        assert_eq!(transitive_internal.is_direct, Some(false));

        let chalk = package_data
            .dependencies
            .iter()
            .find(|dep| dep.extracted_requirement.as_deref() == Some("npm:chalk@5"))
            .unwrap();
        assert_eq!(chalk.is_direct, Some(true));
        assert_eq!(chalk.purl.as_deref(), Some("pkg:npm/chalk@5.6.2"));

        let remote = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.extracted_requirement.as_deref() == Some("https://deno.land/x/oak/mod.ts")
            })
            .unwrap();
        assert_eq!(remote.is_direct, Some(true));
        assert!(remote.resolved_package.is_some());
    }

    #[test]
    fn test_extract_from_deno_lock_unsupported_version() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("deno.lock");
        fs::write(&file_path, r#"{"version":"4"}"#).unwrap();

        let package_data = DenoLockParser::extract_first_package(&file_path);

        assert_eq!(package_data.datasource_id, Some(DatasourceId::DenoLock));
        assert!(package_data.dependencies.is_empty());
    }

    fn sample_deno_lock() -> &'static str {
        r#"{
  "version": "5",
  "specifiers": {
    "jsr:@std/assert@1": "1.0.19",
    "npm:chalk@5": "5.6.2"
  },
  "jsr": {
    "@std/assert@1.0.19": {
      "integrity": "sha256-qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo=",
      "dependencies": ["jsr:@std/internal"]
    },
    "@std/internal@1.0.12": {
      "integrity": "sha256-u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7s="
    }
  },
  "npm": {
    "chalk@5.6.2": {
      "integrity": "sha512-zMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzA=="
    }
  },
  "redirects": {
    "https://deno.land/x/oak/mod.ts": "https://deno.land/x/oak@v17.2.0/mod.ts"
  },
  "remote": {
    "https://deno.land/x/oak@v17.2.0/mod.ts": "ddd00000000000000000000000000000000000000000000000000000000000dd"
  },
  "workspace": {
    "dependencies": [
      "jsr:@std/assert@1",
      "npm:chalk@5"
    ]
  }
}"#
    }
}
