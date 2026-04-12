#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use crate::models::{DatasourceId, PackageType};
    use crate::parsers::{NixDefaultParser, NixFlakeLockParser, NixFlakeParser, PackageParser};

    fn create_named_manifest(
        dir_name: &str,
        file_name: &str,
        content: &str,
    ) -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let manifest_dir = temp_dir.path().join(dir_name);
        fs::create_dir_all(&manifest_dir).expect("Failed to create manifest dir");
        let manifest_path = manifest_dir.join(file_name);
        fs::write(&manifest_path, content).expect("Failed to write manifest");
        (temp_dir, manifest_path)
    }

    fn create_manifest_tree(
        dir_name: &str,
        files: &[(&str, &str)],
    ) -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let manifest_dir = temp_dir.path().join(dir_name);
        fs::create_dir_all(&manifest_dir).expect("Failed to create manifest dir");

        for (relative_path, content) in files {
            let file_path = manifest_dir.join(relative_path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).expect("Failed to create parent dir");
            }
            fs::write(&file_path, content).expect("Failed to write manifest tree file");
        }

        (temp_dir, manifest_dir.join("default.nix"))
    }

    #[test]
    fn test_nix_parsers_match_expected_file_names() {
        assert!(NixFlakeParser::is_match(std::path::Path::new("flake.nix")));
        assert!(NixFlakeLockParser::is_match(std::path::Path::new(
            "flake.lock"
        )));
        assert!(!NixFlakeParser::is_match(std::path::Path::new(
            "flake.lock"
        )));
        assert!(NixDefaultParser::is_match(std::path::Path::new(
            "default.nix"
        )));
        assert!(!NixDefaultParser::is_match(std::path::Path::new(
            "shell.nix"
        )));
    }

    #[test]
    fn test_extract_flake_lock_dependency_graph() {
        let (_temp_dir, path) = create_named_manifest(
            "demo-lock",
            "flake.lock",
            r#"{
  "version": 7,
  "root": "root",
  "nodes": {
    "root": {
      "inputs": {
        "nixpkgs": "nixpkgs",
        "flake-utils": "flake-utils",
        "crate2nix": "crate2nix"
      }
    },
    "nixpkgs": {
      "locked": {
        "type": "github",
        "owner": "NixOS",
        "repo": "nixpkgs",
        "rev": "abc123",
        "narHash": "sha256-nixpkgs",
        "lastModified": 1710000000
      },
      "original": {
        "type": "indirect",
        "id": "nixpkgs"
      }
    },
    "flake-utils": {
      "locked": {
        "type": "github",
        "owner": "numtide",
        "repo": "flake-utils",
        "rev": "def456",
        "narHash": "sha256-flake-utils",
        "lastModified": 1710000001
      },
      "original": {
        "type": "github",
        "owner": "numtide",
        "repo": "flake-utils"
      }
    },
    "crate2nix": {
      "locked": {
        "type": "github",
        "owner": "nix-community",
        "repo": "crate2nix",
        "rev": "ghi789",
        "narHash": "sha256-crate2nix",
        "lastModified": 1710000002
      },
      "original": {
        "type": "github",
        "owner": "nix-community",
        "repo": "crate2nix"
      },
      "flake": false
    }
  }
}
"#,
        );

        let package = NixFlakeLockParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixFlakeLock));
        assert_eq!(package.primary_language.as_deref(), Some("JSON"));
        assert_eq!(package.name.as_deref(), Some("demo-lock"));
        assert_eq!(package.dependencies.len(), 3);
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|data| data.get("lock_version"))
                .and_then(|value| value.as_i64()),
            Some(7)
        );
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|data| data.get("root"))
                .and_then(|value| value.as_str()),
            Some("root")
        );

        let nixpkgs = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nix/nixpkgs@abc123"))
            .expect("missing nixpkgs dependency");
        assert_eq!(nixpkgs.scope.as_deref(), Some("inputs"));
        assert_eq!(nixpkgs.is_pinned, Some(true));
        assert_eq!(
            nixpkgs
                .extra_data
                .as_ref()
                .and_then(|data| data.get("source_type"))
                .and_then(|value| value.as_str()),
            Some("github")
        );

        let crate2nix = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nix/crate2nix@ghi789"))
            .expect("missing crate2nix dependency");
        assert_eq!(
            crate2nix
                .extra_data
                .as_ref()
                .and_then(|data| data.get("flake"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn test_extract_flake_metadata_and_inputs() {
        let (_temp_dir, path) = create_named_manifest(
            "demo-flake",
            "flake.nix",
            r#"{
  description = "Demo flake package";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils = {
      url = "github:numtide/flake-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }: {};
}
"#,
        );

        let package = NixFlakeParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixFlakeNix));
        assert_eq!(package.primary_language.as_deref(), Some("Nix"));
        assert_eq!(package.name.as_deref(), Some("demo-flake"));
        assert_eq!(package.description.as_deref(), Some("Demo flake package"));
        assert_eq!(package.purl.as_deref(), Some("pkg:nix/demo-flake"));
        assert_eq!(package.dependencies.len(), 3);

        let nixpkgs = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nix/nixpkgs"))
            .expect("missing nixpkgs dependency");
        assert_eq!(
            nixpkgs.extracted_requirement.as_deref(),
            Some("github:NixOS/nixpkgs/nixos-24.11")
        );
        assert_eq!(nixpkgs.scope.as_deref(), Some("inputs"));
        assert_eq!(nixpkgs.is_runtime, Some(false));
        assert_eq!(nixpkgs.is_optional, Some(false));
        assert_eq!(nixpkgs.is_direct, Some(true));

        let flake_utils = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nix/flake-utils"))
            .expect("missing flake-utils dependency");
        assert_eq!(
            flake_utils.extracted_requirement.as_deref(),
            Some("github:numtide/flake-utils")
        );
        assert_eq!(
            flake_utils
                .extra_data
                .as_ref()
                .and_then(|data| data.get("follows"))
                .and_then(|value| value.as_str()),
            Some("nixpkgs")
        );
    }

    #[test]
    fn test_extract_flake_metadata_from_chromium_style_flake() {
        let (_temp_dir, path) = create_named_manifest(
            "chromium-flake",
            "flake.nix",
            r#"{
  inputs = { };
  outputs =
    { self }:
    let
      devShell = system: {
        "${system}".default = import ./make-shell-for-system.nix {
          inherit system;
        } { };
      };
    in
    {
      devShells = devShell "aarch64-linux" // devShell "x86_64-linux";
    };
}
"#,
        );

        let package = NixFlakeParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixFlakeNix));
        assert_eq!(package.name.as_deref(), Some("chromium-flake"));
        assert!(package.dependencies.is_empty());
    }

    #[test]
    fn test_extract_flake_metadata_with_let_bound_literals() {
        let (_temp_dir, path) = create_named_manifest(
            "bound-flake",
            "flake.nix",
            r#"let
  descriptionText = "Bound demo flake";
  nixpkgsUrl = "github:NixOS/nixpkgs/nixos-24.11";
  utilsUrl = "github:numtide/flake-utils";
in {
  description = descriptionText;
  inputs = {
    nixpkgs.url = nixpkgsUrl;
    flake-utils = {
      url = utilsUrl;
      flake = false;
    };
  };
  outputs = { self, nixpkgs, flake-utils }: { };
}
"#,
        );

        let package = NixFlakeParser::extract_first_package(&path);

        assert_eq!(package.description.as_deref(), Some("Bound demo flake"));
        assert_eq!(package.dependencies.len(), 2);
        assert!(package.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:nix/nixpkgs")
                && dep.extracted_requirement.as_deref() == Some("github:NixOS/nixpkgs/nixos-24.11")
        }));
        assert!(package.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:nix/flake-utils")
                && dep.extracted_requirement.as_deref() == Some("github:numtide/flake-utils")
                && dep
                    .extra_data
                    .as_ref()
                    .and_then(|data| data.get("flake"))
                    .and_then(|value| value.as_bool())
                    == Some(false)
        }));
    }

    #[test]
    fn test_extract_default_nix_derivation_metadata_and_dependencies() {
        let (_temp_dir, path) = create_named_manifest(
            "demo-derivation",
            "default.nix",
            r#"{ lib, stdenv, fetchFromGitHub, pkg-config, openssl, zlib }:
stdenv.mkDerivation rec {
  pname = "demo";
  version = "1.2.3";
  homepage = "https://example.com/demo";

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ openssl zlib ];

  meta = with lib; {
    description = "Demo package";
    homepage = "https://example.com/demo";
    license = licenses.mit;
  };
}
"#,
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixDefaultNix));
        assert_eq!(package.primary_language.as_deref(), Some("Nix"));
        assert_eq!(package.name.as_deref(), Some("demo"));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.description.as_deref(), Some("Demo package"));
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://example.com/demo")
        );
        assert_eq!(
            package.extracted_license_statement.as_deref(),
            Some("licenses.mit")
        );
        assert_eq!(package.purl.as_deref(), Some("pkg:nix/demo@1.2.3"));
        assert_eq!(package.dependencies.len(), 3);

        let native = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nix/pkg-config"))
            .expect("missing native build input");
        assert_eq!(native.scope.as_deref(), Some("nativeBuildInputs"));
        assert_eq!(native.is_runtime, Some(false));

        let openssl = package
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:nix/openssl"))
            .expect("missing openssl input");
        assert_eq!(openssl.scope.as_deref(), Some("buildInputs"));
        assert_eq!(openssl.is_runtime, Some(true));
    }

    #[test]
    fn test_extract_flake_with_relative_paths_in_outputs() {
        let (_temp_dir, path) = create_named_manifest(
            "rnp-flake",
            "flake.nix",
            r#"{
  description = "High performance C++ OpenPGP library, fully compliant to RFC 4880";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        thePackage = pkgs.callPackage ./default.nix { };
      in
      rec {
        defaultApp = flake-utils.lib.mkApp {
          drv = defaultPackage;
        };
        defaultPackage = thePackage;
      });
}
"#,
        );

        let package = NixFlakeParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixFlakeNix));
        assert_eq!(package.name.as_deref(), Some("rnp-flake"));
        assert_eq!(
            package.description.as_deref(),
            Some("High performance C++ OpenPGP library, fully compliant to RFC 4880")
        );
        assert_eq!(package.dependencies.len(), 2);
    }

    #[test]
    fn test_extract_default_nix_with_string_interpolation() {
        let (_temp_dir, path) = create_named_manifest(
            "rnp-default",
            "default.nix",
            r#"{ pkgs ? import <nixpkgs> { }
, lib ? pkgs.lib
, stdenv ? pkgs.stdenv
}:

stdenv.mkDerivation rec {
  pname = "rnp";
  version = "unstable";

  src = ./.;

  buildInputs = with pkgs; [ zlib bzip2 json_c botan2 ];

  cmakeFlags = [
    "-DCMAKE_INSTALL_PREFIX=${placeholder "out"}"
    "-DBUILD_SHARED_LIBS=on"
  ];

  nativeBuildInputs = with pkgs; [ asciidoctor cmake pkg-config python3 ];

  meta = with lib; {
    homepage = "https://github.com/rnpgp/rnp";
    description = "High performance C++ OpenPGP library, fully compliant to RFC 4880";
    license = licenses.bsd2;
  };
}
"#,
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixDefaultNix));
        assert_eq!(package.name.as_deref(), Some("rnp"));
        assert_eq!(package.version.as_deref(), Some("unstable"));
        assert_eq!(
            package.description.as_deref(),
            Some("High performance C++ OpenPGP library, fully compliant to RFC 4880")
        );
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://github.com/rnpgp/rnp")
        );
        assert_eq!(
            package.extracted_license_statement.as_deref(),
            Some("licenses.bsd2")
        );
        assert!(
            package
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:nix/zlib"))
        );
        assert!(
            package
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:nix/asciidoctor"))
        );
    }

    #[test]
    fn test_extract_default_nix_with_let_bound_aliases_and_inherit() {
        let (_temp_dir, path) = create_named_manifest(
            "bound-default",
            "default.nix",
            r#"let
  pname = "bound-demo";
  version = "2.0.0";
  commonInputs = [ zlib openssl ];
  metadata = {
    homepage = "https://example.com/bound-demo";
  };
in stdenv.mkDerivation {
  inherit pname version;
  name = "${pname}-${version}";
  buildInputs = commonInputs;
  homepage = metadata.homepage;
}
"#,
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("bound-demo"));
        assert_eq!(package.version.as_deref(), Some("2.0.0"));
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://example.com/bound-demo")
        );
        assert!(
            package
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:nix/zlib"))
        );
        assert!(
            package
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:nix/openssl"))
        );
    }

    #[test]
    fn test_extract_default_nix_from_local_import_wrapper() {
        let (_temp_dir, path) = create_manifest_tree(
            "import-wrapper",
            &[
                (
                    "default.nix",
                    r#"import ./package.nix { }
"#,
                ),
                (
                    "package.nix",
                    r#"{ }:
stdenv.mkDerivation {
  pname = "imported-demo";
  version = "1.4.0";
  buildInputs = [ zlib ];
}
"#,
                ),
            ],
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("imported-demo"));
        assert_eq!(package.version.as_deref(), Some("1.4.0"));
        assert!(
            package
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:nix/zlib"))
        );
    }

    #[test]
    fn test_extract_default_nix_from_call_package_wrapper_and_selected_default() {
        let (_temp_dir, path) = create_manifest_tree(
            "call-package-wrapper",
            &[
                (
                    "default.nix",
                    r#"(callPackage ./release.nix { }).defaultNix
"#,
                ),
                (
                    "release.nix",
                    r#"{
  defaultNix = stdenv.mkDerivation {
    pname = "selected-demo";
    version = "3.1.0";
    nativeBuildInputs = [ pkg-config ];
  };
}
"#,
                ),
            ],
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("selected-demo"));
        assert_eq!(package.version.as_deref(), Some("3.1.0"));
        assert!(package.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:nix/pkg-config")
                && dep.scope.as_deref() == Some("nativeBuildInputs")
        }));
    }

    #[test]
    fn test_extract_default_nix_from_local_flake_compat_wrapper() {
        let (_temp_dir, path) = create_manifest_tree(
            "flake-compat-wrapper",
            &[
                (
                    "default.nix",
                    r#"let
  flake = import ./flake-compat.nix { src = ./.; };
in flake.defaultNix // {
  __functor = defaultNix: inputs: defaultNix.overrideInputs inputs;
}
"#,
                ),
                (
                    "flake-compat.nix",
                    r#"{ src }:
{
  defaultNix = src;
}
"#,
                ),
            ],
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("flake-compat-wrapper"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:nix/flake-compat-wrapper")
        );
    }

    #[test]
    fn test_extract_default_nix_from_local_flake_compat_selected_default() {
        let (_temp_dir, path) = create_manifest_tree(
            "outer-wrapper",
            &[
                (
                    "default.nix",
                    r#"(import ./flake-compat.nix { src = ./inner; }).defaultNix.default
"#,
                ),
                (
                    "flake-compat.nix",
                    r#"{ src }:
{
  defaultNix = src;
}
"#,
                ),
                (
                    "inner/placeholder.nix",
                    "# marker file for local source root\n",
                ),
            ],
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("inner"));
        assert_eq!(package.purl.as_deref(), Some("pkg:nix/inner"));
    }

    #[test]
    fn test_extract_default_nix_from_real_world_dream2nix_root_wrapper_shape() {
        let (_temp_dir, path) = create_manifest_tree(
            "dream2nix",
            &[
                (
                    "default.nix",
                    r#"# This file provides backward compatibility to nix < 2.4 clients
let
  flake =
    import
    ./dev-flake/flake-compat.nix
    {src = ./.;};
in
  flake.defaultNix
  // {
    __functor = defaultNix: inputs: defaultNix.overrideInputs inputs;
  }
"#,
                ),
                (
                    "dev-flake/flake-compat.nix",
                    r#"{ src }:
{
  defaultNix = src;
}
"#,
                ),
            ],
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("dream2nix"));
        assert_eq!(package.purl.as_deref(), Some("pkg:nix/dream2nix"));
    }

    #[test]
    fn test_extract_default_nix_from_real_world_dream2nix_dev_flake_shape() {
        let (temp_dir, _path) = create_manifest_tree(
            "dream2nix",
            &[
                (
                    "dev-flake/default.nix",
                    r#"# This file provides backward compatibility to nix < 2.4 clients
let
  flake =
    import
    ./flake-compat.nix
    {src = ./.;};
in
  flake.defaultNix
"#,
                ),
                (
                    "dev-flake/flake-compat.nix",
                    r#"{ src }:
{
  defaultNix = src;
}
"#,
                ),
            ],
        );
        let path = temp_dir.path().join("dream2nix/dev-flake/default.nix");

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.name.as_deref(), Some("dev-flake"));
        assert_eq!(package.purl.as_deref(), Some("pkg:nix/dev-flake"));
    }

    #[test]
    fn test_invalid_nix_inputs_preserve_datasource_identity() {
        let (_flake_dir, flake_path) =
            create_named_manifest("broken-flake", "flake.nix", "{ description = ; }");
        let (_lock_dir, lock_path) = create_named_manifest(
            "broken-lock",
            "flake.lock",
            "{ \"version\": 7, \"root\": \"root\" }",
        );
        let (_default_dir, default_path) =
            create_named_manifest("broken-default", "default.nix", "stdenv.mkDerivation {");

        let flake = NixFlakeParser::extract_first_package(&flake_path);
        let lock = NixFlakeLockParser::extract_first_package(&lock_path);
        let default_pkg = NixDefaultParser::extract_first_package(&default_path);

        assert_eq!(flake.package_type, Some(PackageType::Nix));
        assert_eq!(flake.datasource_id, Some(DatasourceId::NixFlakeNix));
        assert!(flake.name.is_none());
        assert!(flake.dependencies.is_empty());

        assert_eq!(lock.package_type, Some(PackageType::Nix));
        assert_eq!(lock.datasource_id, Some(DatasourceId::NixFlakeLock));
        assert!(lock.name.is_none());
        assert!(lock.dependencies.is_empty());

        assert_eq!(default_pkg.package_type, Some(PackageType::Nix));
        assert_eq!(default_pkg.datasource_id, Some(DatasourceId::NixDefaultNix));
        assert!(default_pkg.name.is_none());
        assert!(default_pkg.dependencies.is_empty());
    }

    #[test]
    fn test_default_nix_ignores_nested_non_root_mk_derivation() {
        let (_temp_dir, path) = create_named_manifest(
            "nested-helper",
            "default.nix",
            r#"{
  helper = stdenv.mkDerivation {
    pname = "nested";
    version = "1.0.0";
  };
}
"#,
        );

        let package = NixDefaultParser::extract_first_package(&path);

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.datasource_id, Some(DatasourceId::NixDefaultNix));
        assert!(package.name.is_none());
        assert!(package.version.is_none());
        assert!(package.dependencies.is_empty());
    }
}
