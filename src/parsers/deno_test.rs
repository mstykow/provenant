// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{DenoParser, PackageParser};

    #[test]
    fn test_is_match() {
        assert!(DenoParser::is_match(Path::new("deno.json")));
        assert!(DenoParser::is_match(Path::new("deno.jsonc")));
        assert!(!DenoParser::is_match(Path::new("package.json")));
    }

    #[test]
    fn test_extract_from_deno_json_manifest() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("deno.json");
        let content = r#"{
  "name": "@provenant/deno-sample",
  "version": "1.0.0",
  "exports": "./mod.ts",
  "imports": {
    "@std/assert": "jsr:@std/assert@^1.0.0",
    "chalk": "npm:chalk@5",
    "oak": "https://deno.land/x/oak/mod.ts"
  },
  "links": ["../local-package"],
  "scopes": {
    "https://deno.land/x/oak/": {
      "https://deno.land/x/dep/mod.ts": "./patched.ts"
    }
  },
  "tasks": { "test": "deno test" },
  "lock": { "path": "./deno.lock", "frozen": false }
}"#;
        fs::write(&file_path, content).unwrap();

        let package_data = DenoParser::extract_first_package(&file_path);

        assert_eq!(package_data.package_type, Some(PackageType::Deno));
        assert_eq!(package_data.primary_language.as_deref(), Some("TypeScript"));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::DenoJson));
        assert_eq!(package_data.namespace.as_deref(), Some("@provenant"));
        assert_eq!(package_data.name.as_deref(), Some("deno-sample"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:generic/%40provenant/deno-sample@1.0.0")
        );
        assert_eq!(package_data.dependencies.len(), 3);

        let jsr_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.extracted_requirement.as_deref() == Some("jsr:@std/assert@^1.0.0"))
            .unwrap();
        assert_eq!(jsr_dep.is_direct, Some(true));
        assert_eq!(jsr_dep.is_runtime, Some(true));
        assert_eq!(
            jsr_dep.purl.as_deref(),
            Some("pkg:generic/jsr.io/%40std/assert")
        );

        let npm_dep = package_data
            .dependencies
            .iter()
            .find(|dep| dep.extracted_requirement.as_deref() == Some("npm:chalk@5"))
            .unwrap();
        assert_eq!(npm_dep.purl.as_deref(), Some("pkg:npm/chalk"));

        let remote_dep = package_data
            .dependencies
            .iter()
            .find(|dep| {
                dep.extracted_requirement.as_deref() == Some("https://deno.land/x/oak/mod.ts")
            })
            .unwrap();
        assert!(remote_dep.purl.is_some());

        let extra_data = package_data.extra_data.unwrap();
        assert!(extra_data.contains_key("exports"));
        assert!(extra_data.contains_key("scopes"));
        assert!(extra_data.contains_key("links"));
        assert!(extra_data.contains_key("tasks"));
        assert!(extra_data.contains_key("lock"));
    }

    #[test]
    fn test_extract_from_deno_jsonc_manifest() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("deno.jsonc");
        let content = r#"{
  // package identity
  "name": "@std/jsonc",
  "version": "1.0.2",
  "exports": {
    ".": "./mod.ts",
    "./parse": "./parse.ts",
  },
  "imports": {
    "@std/assert": "jsr:@std/assert@1",
  },
}"#;
        fs::write(&file_path, content).unwrap();

        let package_data = DenoParser::extract_first_package(&file_path);

        assert_eq!(package_data.datasource_id, Some(DatasourceId::DenoJson));
        assert_eq!(package_data.namespace.as_deref(), Some("@std"));
        assert_eq!(package_data.name.as_deref(), Some("jsonc"));
        assert_eq!(package_data.version.as_deref(), Some("1.0.2"));
        assert_eq!(package_data.dependencies.len(), 1);
    }
}
