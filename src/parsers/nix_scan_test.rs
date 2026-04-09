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
}
