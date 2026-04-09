#[cfg(test)]
mod tests {
    use crate::models::PackageType;
    use crate::parsers::{NpmLockParser, PackageParser};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary package-lock.json file with the given content
    fn create_temp_lock_file(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let lock_path = temp_dir.path().join("package-lock.json");
        fs::write(&lock_path, content).expect("Failed to write package-lock.json");
        (temp_dir, lock_path)
    }

    // Helper to load test data files
    fn load_testdata_file(name: &str) -> PathBuf {
        PathBuf::from(format!("testdata/npm/{}", name))
            .canonicalize()
            .expect("Failed to find test data file")
    }

    // ===== File Matching Tests =====

    #[test]
    fn test_is_match_package_lock() {
        let valid_path = PathBuf::from("/some/path/package-lock.json");
        assert!(NpmLockParser::is_match(&valid_path));
    }

    #[test]
    fn test_is_match_hidden_package_lock() {
        let valid_path = PathBuf::from("/some/path/.package-lock.json");
        assert!(NpmLockParser::is_match(&valid_path));
    }

    #[test]
    fn test_is_match_npm_shrinkwrap() {
        let valid_path = PathBuf::from("/some/path/npm-shrinkwrap.json");
        assert!(NpmLockParser::is_match(&valid_path));
    }

    #[test]
    fn test_is_match_hidden_npm_shrinkwrap() {
        let valid_path = PathBuf::from("/some/path/.npm-shrinkwrap.json");
        assert!(NpmLockParser::is_match(&valid_path));
    }

    #[test]
    fn test_is_not_match_package_json() {
        let invalid_path = PathBuf::from("/some/path/package.json");
        assert!(!NpmLockParser::is_match(&invalid_path));
    }

    // ===== Integration Tests =====

    #[test]
    fn test_parse_v1_from_testdata() {
        let lock_path = load_testdata_file("package-lock-v1.json");
        let package_data = NpmLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("babel-runtime".to_string()));
        assert_eq!(package_data.version, Some("6.23.0".to_string()));
        assert_eq!(package_data.namespace, Some("".to_string()));

        // Should have dependencies
        assert!(!package_data.dependencies.is_empty());

        // Check a specific dependency
        let ansi_regex_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("ansi-regex"))
                    .unwrap_or(false)
            })
            .expect("Should have ansi-regex dependency");

        assert_eq!(ansi_regex_dep.scope, Some("devDependencies".to_string()));
        assert_eq!(ansi_regex_dep.is_pinned, Some(true));
        assert_eq!(ansi_regex_dep.is_optional, Some(true));
        assert_eq!(ansi_regex_dep.is_runtime, Some(false));

        // Check resolved package
        assert!(ansi_regex_dep.resolved_package.is_some());
        let resolved = ansi_regex_dep.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.name, "ansi-regex");
        assert_eq!(resolved.version, "2.1.1");
        assert!(resolved.is_virtual);
        assert!(resolved.download_url.is_some());
        assert!(resolved.sha1.is_some());
    }

    #[test]
    fn test_parse_v2_from_testdata() {
        let lock_path = load_testdata_file("package-lock-v2.json");
        let package_data = NpmLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("megak".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        // Should have dependencies
        assert!(!package_data.dependencies.is_empty());

        // All dependencies should be pinned
        for dep in &package_data.dependencies {
            assert_eq!(dep.is_pinned, Some(true));
        }
    }

    #[test]
    fn test_parse_scoped_packages() {
        let lock_path = load_testdata_file("package-lock-scoped.json");
        let package_data = NpmLockParser::extract_first_package(&lock_path);

        // Root package should be scoped
        assert_eq!(package_data.namespace, Some("@example".to_string()));
        assert_eq!(package_data.name, Some("test-package".to_string()));
        assert_eq!(
            package_data.purl,
            Some("pkg:npm/%40example/test-package@1.0.0".to_string())
        );

        // Find @types/node dependency
        let types_node = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("%40types") && p.contains("node"))
                    .unwrap_or(false)
            })
            .expect("Should have @types/node dependency");

        // Check resolved package has correct namespace
        let resolved = types_node.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.namespace, "@types");
        assert_eq!(resolved.name, "node");
    }

    #[test]
    fn test_parse_npm_shrinkwrap() {
        let lock_path = load_testdata_file("npm-shrinkwrap.json");
        let package_data = NpmLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("shrinkwrap-test".to_string()));
        assert_eq!(package_data.version, Some("2.0.0".to_string()));

        // Should have lodash dependency
        assert!(!package_data.dependencies.is_empty());
        let lodash_dep = package_data
            .dependencies
            .iter()
            .find(|d| {
                d.purl
                    .as_ref()
                    .map(|p| p.contains("lodash"))
                    .unwrap_or(false)
            })
            .expect("Should have lodash dependency");

        assert_eq!(lodash_dep.is_pinned, Some(true));
        assert!(lodash_dep.resolved_package.is_some());

        let resolved = lodash_dep.resolved_package.as_ref().unwrap();
        assert_eq!(resolved.name, "lodash");
        assert_eq!(resolved.version, "4.17.21");
        assert!(resolved.download_url.is_some());
    }

    #[test]
    fn test_parse_minimal_file() {
        let lock_path = load_testdata_file("package-lock-minimal.json");
        let package_data = NpmLockParser::extract_first_package(&lock_path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, Some("minimal-test".to_string()));
        assert_eq!(package_data.version, Some("1.0.0".to_string()));

        // Should have no dependencies
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_v1_lockfile_without_root_name_keeps_dependencies_but_no_identity() {
        let content = r#"{
            "requires": true,
            "lockfileVersion": 1,
            "version": "9.9.9",
            "dependencies": {
                "minimist": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/minimist/-/minimist-1.0.0.tgz",
                    "integrity": "sha1-lCnE+D4NIlkKdq62EABtzD+Jxw8="
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert_eq!(package_data.name, None);
        assert_eq!(package_data.version, None);
        assert_eq!(package_data.purl, None);
        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0].purl.as_deref(),
            Some("pkg:npm/minimist@1.0.0")
        );
    }

    // ===== Integrity Parsing Tests =====

    #[test]
    fn test_parse_integrity_sha512() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/test-pkg": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/test-pkg/-/test-pkg-1.0.0.tgz",
                    "integrity": "sha512-9NET910DNaIPngYnLLPeg+Ogzqsi9uM4mSboU5y6p8S5DzMTVEsJZrawi+BoDNUVBa2DhJqQYUFvMDfgU062LQ=="
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        let resolved = dep.resolved_package.as_ref().unwrap();

        // Should have sha512 checksum
        assert!(resolved.sha512.is_some());
        let sha512 = resolved.sha512.as_ref().unwrap();
        assert_eq!(sha512.len(), 128); // sha512 hex is 128 characters
    }

    #[test]
    fn test_parse_integrity_sha1() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 1,
            "dependencies": {
                "test-pkg": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/test-pkg/-/test-pkg-1.0.0.tgz",
                    "integrity": "sha1-w7M6te42DYbg5ijwRorn7yfWVN8="
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        let resolved = dep.resolved_package.as_ref().unwrap();

        // Should have sha1 checksum
        assert!(resolved.sha1.is_some());
        let sha1 = resolved.sha1.as_ref().unwrap();
        assert_eq!(sha1.len(), 40); // sha1 hex is 40 characters
        assert_eq!(sha1, "c3b33ab5ee360d86e0e628f0468ae7ef27d654df");
    }

    #[test]
    fn test_parse_integrity_missing() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/test-pkg": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/test-pkg/-/test-pkg-1.0.0.tgz"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        let resolved = dep.resolved_package.as_ref().unwrap();

        // Should have no checksums
        assert!(resolved.sha1.is_none());
        assert!(resolved.sha512.is_none());
    }

    // ===== Namespace & PURL Tests =====

    #[test]
    fn test_extract_namespace_scoped() {
        // Implicitly tested through parsing scoped packages
        let content = r#"{
            "name": "@myorg/mypackage",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "@myorg/mypackage",
                    "version": "1.0.0"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.namespace, Some("@myorg".to_string()));
        assert_eq!(package_data.name, Some("mypackage".to_string()));
    }

    #[test]
    fn test_extract_namespace_regular() {
        let content = r#"{
            "name": "express",
            "version": "4.18.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "express",
                    "version": "4.18.0"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.namespace, Some("".to_string()));
        assert_eq!(package_data.name, Some("express".to_string()));
    }

    #[test]
    fn test_create_purl_scoped() {
        let content = r#"{
            "name": "@types/node",
            "version": "18.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "@types/node",
                    "version": "18.0.0"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        // PURL should encode @ as %40
        assert_eq!(
            package_data.purl,
            Some("pkg:npm/%40types/node@18.0.0".to_string())
        );
    }

    // ===== Dependency Flags Tests =====

    #[test]
    fn test_dev_dependencies_marked_correctly() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/jest": {
                    "version": "29.0.0",
                    "resolved": "https://registry.npmjs.org/jest/-/jest-29.0.0.tgz",
                    "dev": true
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        let jest_dep = &package_data.dependencies[0];
        assert_eq!(jest_dep.scope, Some("devDependencies".to_string()));
        assert_eq!(jest_dep.is_optional, Some(true));
        assert_eq!(jest_dep.is_runtime, Some(false));
    }

    #[test]
    fn test_optional_dependencies() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/fsevents": {
                    "version": "2.3.0",
                    "resolved": "https://registry.npmjs.org/fsevents/-/fsevents-2.3.0.tgz",
                    "optional": true
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        let fsevents_dep = &package_data.dependencies[0];
        assert_eq!(fsevents_dep.scope, Some("dependencies".to_string()));
        assert_eq!(fsevents_dep.is_optional, Some(true));
        assert_eq!(fsevents_dep.is_runtime, Some(true));
    }

    #[test]
    fn test_regular_dependencies() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/express": {
                    "version": "4.18.0",
                    "resolved": "https://registry.npmjs.org/express/-/express-4.18.0.tgz"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        let express_dep = &package_data.dependencies[0];
        assert_eq!(express_dep.is_pinned, Some(true));
        assert_eq!(express_dep.scope, Some("dependencies".to_string()));
        assert_eq!(express_dep.is_optional, Some(false));
        assert_eq!(express_dep.is_runtime, Some(true));
    }

    // ===== Error Handling Tests =====

    #[test]
    fn test_invalid_json() {
        let content = "{ invalid json }";
        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        // Should return default empty data
        assert_eq!(package_data.package_type, Some(PackageType::Npm));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }

    #[test]
    fn test_missing_version_field() {
        let content = r#"{
            "name": "test",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test"
                },
                "node_modules/no-version": {
                    "resolved": "https://registry.npmjs.org/no-version/-/no-version-1.0.0.tgz"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        // Should skip dependency without version
        assert_eq!(package_data.dependencies.len(), 0);
    }

    #[test]
    fn test_link_dependency_without_version_is_preserved() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/local-pkg": {
                    "resolved": "../packages/local-pkg",
                    "link": true
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl, Some("pkg:npm/local-pkg".to_string()));
        assert_eq!(
            dep.extracted_requirement,
            Some("../packages/local-pkg".to_string())
        );
        assert_eq!(dep.is_pinned, Some(false));
        assert_eq!(dep.is_direct, Some(false));
        assert!(dep.resolved_package.is_none());
        let extra_data = dep.extra_data.as_ref().expect("expected link extra_data");
        assert_eq!(extra_data.get("link"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(
            extra_data.get("resolved"),
            Some(&serde_json::Value::String(
                "../packages/local-pkg".to_string()
            ))
        );
    }

    #[test]
    fn test_root_package_falls_back_to_packages_entry_and_keeps_lockfile_version() {
        let content = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "hidden-lock-root",
                    "version": "2.0.0"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.name, Some("hidden-lock-root".to_string()));
        assert_eq!(package_data.version, Some("2.0.0".to_string()));
        assert_eq!(
            package_data.purl,
            Some("pkg:npm/hidden-lock-root@2.0.0".to_string())
        );
        let extra_data = package_data
            .extra_data
            .expect("expected lockfile extra_data");
        assert_eq!(
            extra_data.get("lockfileVersion"),
            Some(&serde_json::Value::from(3))
        );
    }

    // ===== Version Detection Tests =====

    #[test]
    fn test_detect_version_v1() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 1,
            "dependencies": {}
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        // Should successfully parse v1 format
        assert_eq!(package_data.name, Some("test".to_string()));
    }

    #[test]
    fn test_detect_version_v2() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        // Should successfully parse v2 format
        assert_eq!(package_data.name, Some("test".to_string()));
    }

    #[test]
    fn test_url_checksum_extraction() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 2,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/test-pkg": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/test-pkg/-/test-pkg-1.0.0.tgz#c3b33ab5ee360d86e0e628f0468ae7ef27d654df"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        let dep = &package_data.dependencies[0];
        let resolved = dep.resolved_package.as_ref().unwrap();

        // Should extract sha1 from URL
        assert_eq!(
            resolved.sha1,
            Some("c3b33ab5ee360d86e0e628f0468ae7ef27d654df".to_string())
        );
    }

    #[test]
    fn test_file_spec_dependency_is_unpinned_and_preserves_extra_data() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0",
                    "dependencies": {
                        "local-tarball": "file:vendor/local-tarball-1.2.3.tgz"
                    }
                },
                "node_modules/local-tarball": {
                    "version": "file:vendor/local-tarball-1.2.3.tgz",
                    "resolved": "file:vendor/local-tarball-1.2.3.tgz",
                    "from": "file:vendor/local-tarball-1.2.3.tgz",
                    "inBundle": true
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl, Some("pkg:npm/local-tarball".to_string()));
        assert_eq!(
            dep.extracted_requirement,
            Some("file:vendor/local-tarball-1.2.3.tgz".to_string())
        );
        assert_eq!(dep.is_pinned, Some(false));
        assert_eq!(dep.is_direct, Some(true));

        let resolved = dep
            .resolved_package
            .as_ref()
            .expect("expected resolved package");
        assert_eq!(resolved.version, "file:vendor/local-tarball-1.2.3.tgz");
        assert_eq!(
            resolved.download_url,
            Some("file:vendor/local-tarball-1.2.3.tgz".to_string())
        );

        let extra_data = dep
            .extra_data
            .as_ref()
            .expect("expected dependency extra_data");
        assert_eq!(
            extra_data.get("from"),
            Some(&serde_json::Value::String(
                "file:vendor/local-tarball-1.2.3.tgz".to_string()
            ))
        );
        assert_eq!(
            extra_data.get("inBundle"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn test_git_spec_dependency_is_unpinned() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0",
                    "dependencies": {
                        "git-dep": "git+https://github.com/npm/cli.git#deadbeef"
                    }
                },
                "node_modules/git-dep": {
                    "version": "git+https://github.com/npm/cli.git#deadbeef",
                    "resolved": "git+https://github.com/npm/cli.git#deadbeef"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl, Some("pkg:npm/git-dep".to_string()));
        assert_eq!(
            dep.extracted_requirement,
            Some("git+https://github.com/npm/cli.git#deadbeef".to_string())
        );
        assert_eq!(dep.is_pinned, Some(false));

        let resolved = dep
            .resolved_package
            .as_ref()
            .expect("expected resolved package");
        assert_eq!(
            resolved.version,
            "git+https://github.com/npm/cli.git#deadbeef"
        );
        assert_eq!(
            resolved.download_url,
            Some("git+https://github.com/npm/cli.git#deadbeef".to_string())
        );
    }

    #[test]
    fn test_tarball_url_dependency_uses_unpinned_purl_and_download_url() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0",
                    "dependencies": {
                        "remote-tarball": "https://registry.npmjs.org/remote-tarball/-/remote-tarball-1.2.3.tgz"
                    }
                },
                "node_modules/remote-tarball": {
                    "version": "https://registry.npmjs.org/remote-tarball/-/remote-tarball-1.2.3.tgz"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl, Some("pkg:npm/remote-tarball".to_string()));
        assert_eq!(dep.is_pinned, Some(false));

        let resolved = dep
            .resolved_package
            .as_ref()
            .expect("expected resolved package");
        assert_eq!(
            resolved.download_url,
            Some(
                "https://registry.npmjs.org/remote-tarball/-/remote-tarball-1.2.3.tgz".to_string()
            )
        );
    }

    #[test]
    fn test_npm_alias_range_dependency_is_not_marked_pinned() {
        let content = r#"{
            "name": "test",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0",
                    "dependencies": {
                        "wrap-ansi-cjs": "npm:wrap-ansi@^7.0.0"
                    }
                },
                "node_modules/wrap-ansi-cjs": {
                    "name": "wrap-ansi",
                    "version": "npm:wrap-ansi@^7.0.0",
                    "resolved": "https://registry.npmjs.org/wrap-ansi/-/wrap-ansi-7.0.0.tgz"
                }
            }
        }"#;

        let (_temp, path) = create_temp_lock_file(content);
        let package_data = NpmLockParser::extract_first_package(&path);

        assert_eq!(package_data.dependencies.len(), 1);
        let dep = &package_data.dependencies[0];
        assert_eq!(dep.purl, Some("pkg:npm/wrap-ansi".to_string()));
        assert_eq!(
            dep.extracted_requirement,
            Some("npm:wrap-ansi@^7.0.0".to_string())
        );
        assert_eq!(dep.is_pinned, Some(false));

        let resolved = dep
            .resolved_package
            .as_ref()
            .expect("expected resolved package");
        assert_eq!(resolved.name, "wrap-ansi");
        assert_eq!(resolved.version, "^7.0.0");
        assert_eq!(
            resolved.download_url,
            Some("https://registry.npmjs.org/wrap-ansi/-/wrap-ansi-7.0.0.tgz".to_string())
        );
    }

    #[test]
    fn test_npm_lock_v2_nested_duplicate_marks_only_root_entry_direct() {
        let lock_path = PathBuf::from("testdata/npm/lock-v2-nested-dups/package-lock.json");
        let package_data = NpmLockParser::extract_first_package(&lock_path);

        let deps = package_data.dependencies;
        assert!(deps.len() >= 2, "Should have at least 2 dependencies");

        let foo_deps: Vec<_> = deps
            .iter()
            .filter(|d| d.purl.as_ref().map(|p| p.contains("foo")).unwrap_or(false))
            .collect();

        assert_eq!(
            foo_deps.len(),
            2,
            "Should have exactly 2 'foo' dependencies (direct at root + nested under bar)"
        );

        let direct_count = foo_deps
            .iter()
            .filter(|d| d.is_direct == Some(true))
            .count();
        let nested_count = foo_deps
            .iter()
            .filter(|d| d.is_direct == Some(false))
            .count();

        assert_eq!(direct_count, 1, "Only the root-level foo should be direct");
        assert_eq!(
            nested_count, 1,
            "The nested duplicate foo should be transitive"
        );
    }
}
