// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use base64::Engine;
    use std::fs;
    use std::path::Path;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    fn decode_legacy_bun_lockb_fixture() -> Vec<u8> {
        let fixture = Path::new("testdata/bun/legacy/bun.lockb.v2-no-scripts.base64");
        base64::engine::general_purpose::STANDARD
            .decode(
                fs::read_to_string(fixture)
                    .expect("legacy bun.lockb fixture should be readable")
                    .trim(),
            )
            .expect("legacy bun.lockb fixture should decode")
    }

    #[test]
    fn test_npm_scoped_package_scan_preserves_namespace_and_leaf_name() {
        let (files, result) = scan_and_assemble(Path::new(
            "testdata/summarycode-golden/tallies/packages/scan/scoped1",
        ));

        let package = result
            .packages
            .iter()
            .find(|package| package.namespace.as_deref() == Some("@ionic"))
            .expect("scoped npm package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Npm));
        assert_eq!(package.name.as_deref(), Some("app-scripts"));
        assert_eq!(package.version.as_deref(), Some("3.0.1-201710301651"));

        let manifest = files
            .iter()
            .find(|file| file.path.ends_with("/package.json"))
            .expect("package.json should be scanned");
        assert!(manifest.for_packages.contains(&package.package_uid));
        assert!(
            manifest
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::NpmPackageJson))
        );
    }

    #[test]
    fn test_bun_basic_scan_assembles_package_and_bun_lock() {
        let (files, result) = scan_and_assemble(Path::new("testdata/assembly-golden/bun-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("test-package"))
            .expect("bun package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Npm));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_eq!(package.purl.as_deref(), Some("pkg:npm/test-package@1.0.0"));
        assert_dependency_present(&result.dependencies, "pkg:npm/express", "package.json");
        assert_dependency_present(&result.dependencies, "pkg:npm/express@4.18.0", "bun.lock");

        let package_json = files
            .iter()
            .find(|file| file.path.ends_with("/package.json"))
            .expect("package.json should be scanned");
        let bun_lock = files
            .iter()
            .find(|file| file.path.ends_with("/bun.lock"))
            .expect("bun.lock should be scanned");
        assert!(package_json.for_packages.contains(&package.package_uid));
        assert!(bun_lock.for_packages.contains(&package.package_uid));
        assert!(
            bun_lock
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::BunLock))
        );
    }

    #[test]
    fn test_bun_legacy_lockb_scan_assembles_package_and_dependency() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{
  "name": "bundle",
  "devDependencies": {
    "bun-types": "^0.5.0"
  }
}
"#,
        )
        .expect("write package.json");
        fs::write(
            temp_dir.path().join("bun.lockb"),
            decode_legacy_bun_lockb_fixture(),
        )
        .expect("write bun.lockb");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("bundle"))
            .expect("legacy bun.lockb package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Npm));
        assert_eq!(package.purl.as_deref(), Some("pkg:npm/bundle"));
        assert_dependency_present(&result.dependencies, "pkg:npm/bun-types", "package.json");
        assert_file_links_to_package(
            &files,
            "/package.json",
            &package.package_uid,
            DatasourceId::NpmPackageJson,
        );
        assert_file_links_to_package(
            &files,
            "/bun.lockb",
            &package.package_uid,
            DatasourceId::BunLockb,
        );
    }

    #[test]
    fn test_hidden_package_lock_scan_assembles_with_root_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("package.json"),
            include_str!("../../testdata/assembly-golden/npm-basic/package.json"),
        )
        .expect("write package.json");
        fs::write(
            temp_dir.path().join(".package-lock.json"),
            include_str!("../../testdata/assembly-golden/npm-basic/package-lock.json"),
        )
        .expect("write hidden package-lock");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("test-package"))
            .expect("package should be assembled with hidden package-lock");

        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::NpmPackageLockJson)
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/express@4.18.0",
            ".package-lock.json",
        );

        let hidden_lock = files
            .iter()
            .find(|file| file.path.ends_with("/.package-lock.json"))
            .expect("hidden package-lock should be scanned");
        assert!(hidden_lock.for_packages.contains(&package.package_uid));
        assert!(
            hidden_lock.package_data.iter().any(|pkg_data| {
                pkg_data.datasource_id == Some(DatasourceId::NpmPackageLockJson)
            })
        );
    }

    #[test]
    fn test_hidden_npm_shrinkwrap_scan_assembles_with_root_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("package.json"),
            include_str!("../../testdata/assembly-golden/npm-basic/package.json"),
        )
        .expect("write package.json");
        fs::write(
            temp_dir.path().join(".npm-shrinkwrap.json"),
            include_str!("../../testdata/assembly-golden/npm-basic/package-lock.json"),
        )
        .expect("write hidden shrinkwrap");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("test-package"))
            .expect("package should be assembled with hidden shrinkwrap");

        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::NpmPackageLockJson)
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/express@4.18.0",
            ".npm-shrinkwrap.json",
        );

        let hidden_lock = files
            .iter()
            .find(|file| file.path.ends_with("/.npm-shrinkwrap.json"))
            .expect("hidden shrinkwrap should be scanned");
        assert!(hidden_lock.for_packages.contains(&package.package_uid));
        assert!(
            hidden_lock.package_data.iter().any(|pkg_data| {
                pkg_data.datasource_id == Some(DatasourceId::NpmPackageLockJson)
            })
        );
    }

    #[test]
    fn test_pnpm_workspace_scan_keeps_root_package_with_shrinkwrap_yaml() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let packages_dir = temp_dir.path().join("packages");
        let app_dir = packages_dir.join("app");

        fs::create_dir_all(&app_dir).expect("create workspace member dir");
        fs::write(
            temp_dir.path().join("package.json"),
            include_str!("../../testdata/assembly-golden/pnpm-workspace/package.json"),
        )
        .expect("write root package.json");
        fs::write(
            temp_dir.path().join("pnpm-workspace.yaml"),
            include_str!("../../testdata/assembly-golden/pnpm-workspace/pnpm-workspace.yaml"),
        )
        .expect("write workspace yaml");
        fs::write(
            temp_dir.path().join("shrinkwrap.yaml"),
            include_str!("../../testdata/pnpm/pnpm-v5.yaml"),
        )
        .expect("write shrinkwrap.yaml");
        fs::write(
            app_dir.join("package.json"),
            r#"{
  "name": "workspace-app",
  "version": "0.2.0"
}
"#,
        )
        .expect("write member package.json");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let root_package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("my-pnpm-monorepo"))
            .expect("publishable pnpm root package should be kept");
        assert!(
            root_package
                .datasource_ids
                .contains(&DatasourceId::PnpmLockYaml)
        );
        assert!(
            root_package
                .datasource_ids
                .contains(&DatasourceId::PnpmWorkspaceYaml)
        );

        let shrinkwrap_file = files
            .iter()
            .find(|file| file.path.ends_with("/shrinkwrap.yaml"))
            .expect("shrinkwrap.yaml should be scanned");
        assert!(
            shrinkwrap_file
                .for_packages
                .contains(&root_package.package_uid)
        );
        assert!(
            shrinkwrap_file
                .package_data
                .iter()
                .any(|pkg_data| { pkg_data.datasource_id == Some(DatasourceId::PnpmLockYaml) })
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/%40babel/runtime@7.18.9",
            "shrinkwrap.yaml",
        );
    }

    #[test]
    fn test_pnpm_workspace_member_scan_keeps_member_lockfile_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let member_dir = temp_dir.path().join("packages").join("fixture");

        fs::create_dir_all(&member_dir).expect("create workspace member dir");
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{
  "name": "root",
  "private": true
}
"#,
        )
        .expect("write root package.json");
        fs::write(
            temp_dir.path().join("pnpm-workspace.yaml"),
            "packages:\n  - \"packages/*\"\n",
        )
        .expect("write pnpm-workspace.yaml");
        fs::write(
            member_dir.join("package.json"),
            r#"{
  "name": "fixture",
  "version": "1.0.0",
  "dependencies": {
    "write-json-file": "^2.2.0"
  },
  "optionalDependencies": {
    "is-negative": "^2.1.0"
  },
  "devDependencies": {
    "is-positive": "^3.1.0"
  }
}
"#,
        )
        .expect("write member package.json");
        fs::write(
            member_dir.join("pnpm-lock.yaml"),
            include_str!("../../testdata/pnpm/pnpm-v9.yaml"),
        )
        .expect("write member pnpm-lock.yaml");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("fixture"))
            .expect("workspace member should be assembled");

        assert!(package.datasource_ids.contains(&DatasourceId::PnpmLockYaml));
        assert!(
            package
                .datafile_paths
                .iter()
                .any(|path| path.ends_with("/packages/fixture/pnpm-lock.yaml"))
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/write-json-file",
            "package.json",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/%40babel/helper-string-parser@7.24.8",
            "pnpm-lock.yaml",
        );

        let lockfile = files
            .iter()
            .find(|file| file.path.ends_with("/packages/fixture/pnpm-lock.yaml"))
            .expect("pnpm-lock.yaml should be scanned");
        assert!(lockfile.for_packages.contains(&package.package_uid));
        assert!(
            lockfile
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::PnpmLockYaml))
        );
    }

    #[test]
    fn test_pnpm_workspace_roots_with_same_purl_do_not_clobber_each_other() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");

        for workspace_name in ["one", "two"] {
            let workspace_root = temp_dir.path().join(workspace_name);
            let member_dir = workspace_root.join("packages").join("a");

            fs::create_dir_all(&member_dir).expect("create workspace member dir");
            fs::write(
                workspace_root.join("package.json"),
                r#"{
  "name": "root",
  "version": "1.0.0",
  "dependencies": {
    "@scope/a": "workspace:*"
  }
}
"#,
            )
            .expect("write workspace root package.json");
            fs::write(
                workspace_root.join("pnpm-workspace.yaml"),
                "packages:\n  - \"packages/*\"\n",
            )
            .expect("write workspace config");
            fs::write(
                member_dir.join("package.json"),
                r#"{
  "name": "@scope/a",
  "version": "1.0.0"
}
"#,
            )
            .expect("write member package.json");
        }

        let (_files, result) = scan_and_assemble(temp_dir.path());

        assert_eq!(
            result
                .packages
                .iter()
                .filter(|package| package.purl.as_deref() == Some("pkg:npm/root@1.0.0"))
                .count(),
            2,
            "workspace roots with identical purls should both survive assembly"
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/%40scope/a",
            "one/package.json",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/%40scope/a",
            "two/package.json",
        );
    }

    #[test]
    fn test_pnpm_workspace_without_root_manifest_keeps_shared_lockfile_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let member_dir = temp_dir.path().join("pkg");

        fs::create_dir_all(&member_dir).expect("create workspace member dir");
        fs::write(
            temp_dir.path().join("pnpm-workspace.yaml"),
            "packages:\n  - \"pkg\"\n",
        )
        .expect("write workspace config");
        fs::write(
            temp_dir.path().join("pnpm-lock.yaml"),
            r#"lockfileVersion: '9.0'

importers:

  pkg:
    dependencies:
      is-positive:
        specifier: 1.0.0
        version: 1.0.0

packages:

  is-positive@1.0.0:
    resolution: {integrity: sha512-xxzPGZ4P2uN6rROUa5N9Z7zTX6ERuE0hs6GUOc/cKBLF2NqKc16UwqHMt3tFg4CO6EBTE5UecUasg+3jZx3Ckg==}

snapshots:

  is-positive@1.0.0: {}
"#,
        )
        .expect("write shared pnpm-lock.yaml");
        fs::write(
            member_dir.join("package.json"),
            r#"{
  "name": "pkg",
  "version": "1.0.0",
  "dependencies": {
    "is-positive": "1.0.0"
  }
}
"#,
        )
        .expect("write member package.json");

        let (files, result) = scan_and_assemble(temp_dir.path());

        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/is-positive@1.0.0",
            "pnpm-lock.yaml",
        );

        let lockfile = files
            .iter()
            .find(|file| file.path.ends_with("/pnpm-lock.yaml"))
            .expect("pnpm-lock.yaml should be scanned");
        assert!(
            lockfile
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::PnpmLockYaml))
        );
    }

    #[test]
    fn test_npm_workspace_without_root_manifest_keeps_shared_shrinkwrap_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let member_dir = temp_dir.path().join("packages").join("foo");

        fs::create_dir_all(&member_dir).expect("create workspace member dir");
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{
  "private": true,
  "name": "workspace-root",
  "version": "1.0.0",
  "workspaces": [
    "packages/**"
  ]
}
"#,
        )
        .expect("write workspace root package.json");
        fs::write(
            temp_dir.path().join("npm-shrinkwrap.json"),
            r#"{
  "name": "workspace-root",
  "version": "1.0.0",
  "lockfileVersion": 2,
  "packages": {
    "": {
      "name": "workspace-root",
      "version": "1.0.0",
      "workspaces": [
        "packages/**"
      ]
    },
    "node_modules/foo": {
      "resolved": "packages/foo",
      "link": true
    },
    "node_modules/is-positive": {
      "version": "1.0.0",
      "resolved": "https://registry.npmjs.org/is-positive/-/is-positive-1.0.0.tgz",
      "integrity": "sha1-iACYVrZKLx632LsBeUGEJK4EUss="
    },
    "packages/foo": {
      "version": "0.0.0",
      "dependencies": {
        "is-positive": "^1.0.0"
      }
    }
  },
  "dependencies": {
    "foo": {
      "version": "file:packages/foo",
      "requires": {
        "is-positive": "^1.0.0"
      }
    },
    "is-positive": {
      "version": "1.0.0",
      "resolved": "https://registry.npmjs.org/is-positive/-/is-positive-1.0.0.tgz",
      "integrity": "sha1-iACYVrZKLx632LsBeUGEJK4EUss="
    }
  }
}
"#,
        )
        .expect("write shared npm-shrinkwrap.json");
        fs::write(
            member_dir.join("package.json"),
            r#"{
  "name": "foo",
  "version": "0.0.0",
  "dependencies": {
    "is-positive": "^1.0.0"
  }
}
"#,
        )
        .expect("write member package.json");

        let (_files, result) = scan_and_assemble(temp_dir.path());

        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/is-positive@1.0.0",
            "npm-shrinkwrap.json",
        );
        assert_dependency_present(&result.dependencies, "pkg:npm/foo", "npm-shrinkwrap.json");
    }

    #[test]
    fn test_yarn_pnp_scan_assembles_dependency_source_into_root_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");

        fs::write(
            temp_dir.path().join("package.json"),
            r#"{
  "name": "root-app",
  "version": "1.0.0"
}
"#,
        )
        .expect("write package.json");
        fs::write(
            temp_dir.path().join(".pnp.cjs"),
            include_str!("../../testdata/yarn-pnp-golden/basic/.pnp.cjs"),
        )
        .expect("write .pnp.cjs");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("root-app"))
            .expect("root package should be assembled");
        assert!(package.datasource_ids.contains(&DatasourceId::YarnPnpCjs));
        assert_dependency_present(&result.dependencies, "pkg:npm/left-pad@1.3.0", ".pnp.cjs");
        assert_dependency_present(
            &result.dependencies,
            "pkg:npm/%40scope/demo@2.0.0",
            ".pnp.cjs",
        );

        let pnp_file = files
            .iter()
            .find(|file| file.path.ends_with("/.pnp.cjs"))
            .expect(".pnp.cjs should be scanned");
        assert!(pnp_file.for_packages.contains(&package.package_uid));
        assert!(
            pnp_file
                .package_data
                .iter()
                .any(|pkg_data| { pkg_data.datasource_id == Some(DatasourceId::YarnPnpCjs) })
        );
    }
}
