#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_nix_flake_scan_assembles_manifest_and_lockfile() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let root = temp_dir.path().join("flake-demo");
        fs::create_dir_all(&root).expect("create nix fixture dir");
        fs::copy(
            "testdata/nix-golden/flake-demo/flake.nix",
            root.join("flake.nix"),
        )
        .expect("copy flake.nix fixture");
        fs::copy(
            "testdata/nix-golden/lock-demo/flake.lock",
            root.join("flake.lock"),
        )
        .expect("copy flake.lock fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("flake-demo"))
            .expect("nix flake package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert_eq!(package.purl.as_deref(), Some("pkg:nix/flake-demo"));
        assert_dependency_present(
            &result.dependencies,
            "pkg:nix/crate2nix@ghi789",
            "flake.lock",
        );
        assert_file_links_to_package(
            &files,
            "/flake.nix",
            &package.package_uid,
            DatasourceId::NixFlakeNix,
        );
        assert_file_links_to_package(
            &files,
            "/flake.lock",
            &package.package_uid,
            DatasourceId::NixFlakeLock,
        );
    }

    #[test]
    fn test_nix_scan_handles_mongodb_style_vendored_nix_files_without_scan_errors() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let root = temp_dir.path().join("vendor-nix");
        fs::create_dir_all(&root).expect("create nix fixture dir");

        fs::write(
            root.join("flake.nix"),
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
        )
        .expect("write flake.nix");

        fs::write(
            root.join("default.nix"),
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
        )
        .expect("write default.nix");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let flake_file = files
            .iter()
            .find(|file| file.path.ends_with("/flake.nix"))
            .expect("flake.nix should be scanned");
        assert!(
            flake_file.scan_errors.is_empty(),
            "{:?}",
            flake_file.scan_errors
        );

        let default_file = files
            .iter()
            .find(|file| file.path.ends_with("/default.nix"))
            .expect("default.nix should be scanned");
        assert!(
            default_file.scan_errors.is_empty(),
            "{:?}",
            default_file.scan_errors
        );

        let package = result
            .packages
            .iter()
            .find(|package| package.package_type == Some(PackageType::Nix))
            .expect("nix package should be assembled");
        assert_eq!(package.name.as_deref(), Some("rnp"));
    }

    #[test]
    fn test_nix_scan_silently_falls_back_for_unsupported_repo_style_files() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");

        fs::write(
            temp_dir.path().join("flake.nix"),
            r#"{
  description = "The purely functional package manager";

  inputs.nixpkgs.url = "https://channels.nixos.org/nixos-25.11/nixexprs.tar.xz";

  outputs = inputs@{ self, nixpkgs, ... }:
    let
      inherit (nixpkgs) lib;
    in
    {
      checks = { };
    };
}
"#,
        )
        .expect("write flake.nix");

        fs::write(
            temp_dir.path().join("default.nix"),
            r#"(import (
  let
    lock = builtins.fromJSON (builtins.readFile ./flake.lock);
  in
  fetchTarball {
    url = "https://github.com/edolstra/flake-compat/archive/${lock.nodes.flake-compat.locked.rev}.tar.gz";
    sha256 = lock.nodes.flake-compat.locked.narHash;
  }
) { src = ./.; }).defaultNix
"#,
        )
        .expect("write default.nix");

        let (files, _result) = scan_and_assemble(temp_dir.path());

        let flake_file = files
            .iter()
            .find(|file| file.path.ends_with("/flake.nix"))
            .expect("flake.nix should be scanned");
        assert!(
            flake_file.scan_errors.is_empty(),
            "{:?}",
            flake_file.scan_errors
        );
        assert!(flake_file.package_data.iter().any(|package| {
            package.datasource_id == Some(DatasourceId::NixFlakeNix)
                && package.description.as_deref() == Some("The purely functional package manager")
        }));

        let default_file = files
            .iter()
            .find(|file| file.path.ends_with("/default.nix"))
            .expect("default.nix should be scanned");
        assert!(
            default_file.scan_errors.is_empty(),
            "{:?}",
            default_file.scan_errors
        );
        assert!(
            default_file
                .package_data
                .iter()
                .any(|package| package.datasource_id == Some(DatasourceId::NixDefaultNix))
        );
    }

    #[test]
    fn test_nix_scan_extracts_local_import_wrapped_default_nix() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let root = temp_dir.path().join("import-wrapper");
        fs::create_dir_all(&root).expect("create nix fixture dir");

        fs::write(
            root.join("default.nix"),
            r#"let
  pkgFile = ./package.nix;
in import pkgFile { }
"#,
        )
        .expect("write default.nix");

        fs::write(
            root.join("package.nix"),
            r#"{ }:
let
  pname = "scan-wrapper-demo";
  version = "0.9.0";
  deps = [ zlib ];
in stdenv.mkDerivation {
  inherit pname version;
  buildInputs = deps;
}
"#,
        )
        .expect("write package.nix");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let default_file = files
            .iter()
            .find(|file| file.path.ends_with("/default.nix"))
            .expect("default.nix should be scanned");
        assert!(
            default_file.scan_errors.is_empty(),
            "{:?}",
            default_file.scan_errors
        );

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("scan-wrapper-demo"))
            .expect("wrapped nix package should be assembled");
        assert_eq!(package.package_type, Some(PackageType::Nix));
        assert!(
            result
                .dependencies
                .iter()
                .any(|dep| dep.purl.as_deref() == Some("pkg:nix/zlib"))
        );
    }

    #[test]
    fn test_nix_scan_links_local_flake_compat_default_wrapper_to_flake_lock_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let root = temp_dir.path().join("flake-compat-scan");
        fs::create_dir_all(&root).expect("create nix fixture dir");

        fs::write(
            root.join("default.nix"),
            r#"let
  flake = import ./flake-compat.nix { src = ./.; };
in flake.defaultNix
"#,
        )
        .expect("write default.nix");

        fs::write(
            root.join("flake-compat.nix"),
            r#"{ src }:
{
  defaultNix = src;
}
"#,
        )
        .expect("write flake-compat.nix");

        fs::copy(
            "testdata/nix-golden/lock-demo/flake.lock",
            root.join("flake.lock"),
        )
        .expect("copy flake.lock fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let default_file = files
            .iter()
            .find(|file| file.path.ends_with("/default.nix"))
            .expect("default.nix should be scanned");
        assert!(
            default_file.scan_errors.is_empty(),
            "{:?}",
            default_file.scan_errors
        );
        assert!(
            default_file
                .package_data
                .iter()
                .any(|package| package.datasource_id == Some(DatasourceId::NixDefaultNix))
        );

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("flake-compat-scan"))
            .expect("flake lock package should be assembled");
        assert_file_links_to_package(
            &files,
            "/default.nix",
            &package.package_uid,
            DatasourceId::NixDefaultNix,
        );
    }
}
