#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use std::path::PathBuf;

    use crate::parsers::scan_test_utils::scan_and_assemble;

    #[test]
    fn test_debian_deb_scan_promotes_top_level_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/debian/deb/adduser_3.112ubuntu1_all.deb");
        let copied_fixture = temp_dir.path().join("adduser_3.112ubuntu1_all.deb");

        std::fs::copy(&fixture, &copied_fixture).expect("copy deb fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("adduser"))
            .expect("deb archive should be promoted to a top-level package");

        assert!(package.datasource_ids.contains(&DatasourceId::DebianDeb));
        assert_eq!(package.version.as_deref(), Some("3.112ubuntu1"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:deb/ubuntu/adduser@3.112ubuntu1?arch=all")
        );

        let deb_file = files
            .iter()
            .find(|file| file.path.ends_with("/adduser_3.112ubuntu1_all.deb"))
            .expect("copied deb fixture should be present in scan output");

        assert!(deb_file.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_debian_status_d_scan_assigns_installed_files_and_keeps_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let status_dir = temp_dir.path().join("var/lib/dpkg/status.d");
        let info_dir = temp_dir.path().join("var/lib/dpkg/info");
        let bin_dir = temp_dir.path().join("bin");
        let doc_dir = temp_dir.path().join("usr/share/doc/bash");

        std::fs::create_dir_all(&status_dir).unwrap();
        std::fs::create_dir_all(&info_dir).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&doc_dir).unwrap();

        std::fs::write(status_dir.join("bash"), "Package: bash\nStatus: install ok installed\nPriority: required\nSection: shells\nMaintainer: GNU Bash Maintainers <bash@example.com>\nArchitecture: amd64\nVersion: 5.2-1\nDepends: libc6 (>= 2.36)\nDescription: GNU Bourne Again SHell\n shell\n").unwrap();
        std::fs::write(
            info_dir.join("bash.list"),
            "/bin/bash\n/usr/share/doc/bash/copyright\n",
        )
        .unwrap();
        std::fs::write(info_dir.join("bash.md5sums"), "77506afebd3b7e19e937a678a185b62e  bin/bash\n9632d707e9eca8b3ba2b1a98c1c3fdce  usr/share/doc/bash/copyright\n").unwrap();
        std::fs::write(bin_dir.join("bash"), "#!/bin/sh\n").unwrap();
        std::fs::write(doc_dir.join("copyright"), "copyright text\n").unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("bash"))
            .unwrap();
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:deb/debian/libc6")
                && dep.scope.as_deref() == Some("depends")
                && dep.for_package_uid.as_deref() == Some(&package.package_uid)
        }));
        let bash_file = files
            .iter()
            .find(|file| file.path.ends_with("/bin/bash"))
            .unwrap();
        let copyright_file = files
            .iter()
            .find(|file| file.path.ends_with("/usr/share/doc/bash/copyright"))
            .unwrap();
        assert!(bash_file.for_packages.contains(&package.package_uid));
        assert!(copyright_file.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_debian_source_scan_assembles_control_and_copyright() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let debian_dir = temp_dir.path().join("debian");
        std::fs::create_dir_all(&debian_dir).unwrap();

        std::fs::write(
            debian_dir.join("control"),
            "Source: samplepkg\nSection: utils\nPriority: optional\nMaintainer: Example Maintainer <maintainer@example.com>\nHomepage: https://example.test/samplepkg\n\nPackage: samplepkg\nArchitecture: amd64\nDepends: libc6 (>= 2.31), adduser\nDescription: sample Debian package\n sample package\n",
        )
        .unwrap();
        std::fs::write(
            debian_dir.join("copyright"),
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\nFiles: *\nCopyright: 2024 Example Maintainer\nLicense: MIT\n",
        )
        .unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("samplepkg"))
            .expect("debian source package should be assembled");

        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::DebianControlInSource)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::DebianCopyrightInSource)
        );
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:deb/debian/libc6")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));

        let control_file = files
            .iter()
            .find(|file| file.path.ends_with("/debian/control"))
            .unwrap();
        let copyright_file = files
            .iter()
            .find(|file| file.path.ends_with("/debian/copyright"))
            .unwrap();
        assert!(control_file.for_packages.contains(&package.package_uid));
        assert!(copyright_file.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_debian_source_scan_promotes_each_binary_paragraph_to_top_level_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let debian_dir = temp_dir.path().join("debian");
        std::fs::create_dir_all(&debian_dir).unwrap();

        std::fs::write(
            debian_dir.join("control"),
            "Source: samplepkg\nSection: utils\nPriority: optional\nMaintainer: Example Maintainer <maintainer@example.com>\nHomepage: https://example.test/samplepkg\n\nPackage: samplepkg\nArchitecture: amd64\nDepends: libc6 (>= 2.31)\nDescription: sample Debian package\n sample package\n\nPackage: samplepkg-data\nArchitecture: all\nDepends: samplepkg\nDescription: sample Debian data package\n data package\n",
        )
        .unwrap();
        std::fs::write(
            debian_dir.join("copyright"),
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\nFiles: *\nCopyright: 2024 Example Maintainer\nLicense: MIT\n",
        )
        .unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());

        let samplepkg = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("samplepkg"))
            .expect("binary stanza package should be promoted");
        let samplepkg_data = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("samplepkg-data"))
            .expect("second binary stanza package should be promoted");

        assert!(
            samplepkg
                .datasource_ids
                .contains(&DatasourceId::DebianControlInSource)
        );
        assert!(
            samplepkg
                .datasource_ids
                .contains(&DatasourceId::DebianCopyrightInSource)
        );
        assert!(
            samplepkg_data
                .datasource_ids
                .contains(&DatasourceId::DebianControlInSource)
        );
        assert!(
            samplepkg_data
                .datasource_ids
                .contains(&DatasourceId::DebianCopyrightInSource)
        );

        let control_file = files
            .iter()
            .find(|file| file.path.ends_with("/debian/control"))
            .unwrap();
        let copyright_file = files
            .iter()
            .find(|file| file.path.ends_with("/debian/copyright"))
            .unwrap();

        assert!(control_file.for_packages.contains(&samplepkg.package_uid));
        assert!(
            control_file
                .for_packages
                .contains(&samplepkg_data.package_uid)
        );
        assert!(copyright_file.for_packages.contains(&samplepkg.package_uid));
        assert!(
            copyright_file
                .for_packages
                .contains(&samplepkg_data.package_uid)
        );
    }

    #[test]
    fn test_debian_dsc_scan_promotes_top_level_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let dsc_path = temp_dir.path().join("samplepkg_1.0-1.dsc");

        std::fs::write(
            &dsc_path,
            "Format: 3.0 (quilt)\nSource: samplepkg\nBinary: samplepkg\nArchitecture: all\nVersion: 1.0-1\nMaintainer: Example Maintainer <maintainer@example.com>\nDescription: sample Debian source package\nHomepage: https://example.test/samplepkg\n",
        )
        .unwrap();

        let (_files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("samplepkg"))
            .expect(".dsc package should be promoted to top-level package");

        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::DebianSourceControlDsc)
        );
        assert_eq!(package.version.as_deref(), Some("1.0-1"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:deb/debian/samplepkg@1.0-1?arch=all")
        );
    }

    #[test]
    fn test_debian_extracted_deb_scan_assigns_md5sum_file_references() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let control_dir = temp_dir
            .path()
            .join("example_1.0-1_amd64.deb-extract/control.tar.gz-extract");
        let bin_dir = temp_dir
            .path()
            .join("example_1.0-1_amd64.deb-extract/usr/bin");
        let doc_dir = temp_dir
            .path()
            .join("example_1.0-1_amd64.deb-extract/usr/share/doc/example");

        std::fs::create_dir_all(&control_dir).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&doc_dir).unwrap();

        std::fs::write(
            control_dir.join("control"),
            "Package: example\nVersion: 1.0-1\nArchitecture: amd64\nMaintainer: Example Developer <dev@example.com>\nDescription: Example package\n example\n",
        )
        .unwrap();
        std::fs::write(
            control_dir.join("md5sums"),
            "d41d8cd98f00b204e9800998ecf8427e  usr/bin/example\n9e107d9d372bb6826bd81d3542a419d6  usr/share/doc/example/copyright\n",
        )
        .unwrap();
        std::fs::write(bin_dir.join("example"), "#!/bin/sh\n").unwrap();
        std::fs::write(doc_dir.join("copyright"), "copyright text\n").unwrap();

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("example"))
            .expect("extracted deb control + md5sums should assemble a package");

        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::DebianControlExtractedDeb)
        );
        assert!(
            package
                .datasource_ids
                .contains(&DatasourceId::DebianMd5SumsInExtractedDeb)
        );

        let binary_file = files
            .iter()
            .find(|file| file.path.ends_with("/usr/bin/example"))
            .unwrap();
        let copyright_file = files
            .iter()
            .find(|file| file.path.ends_with("/usr/share/doc/example/copyright"))
            .unwrap();
        assert!(binary_file.for_packages.contains(&package.package_uid));
        assert!(copyright_file.for_packages.contains(&package.package_uid));
    }

    #[test]
    fn test_debian_standalone_copyright_scan_keeps_vcpkg_port_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let port_dir = temp_dir.path().join("ports/zlib");
        std::fs::create_dir_all(&port_dir).unwrap();

        std::fs::write(
            port_dir.join("copyright"),
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\nFiles: *\nCopyright: 2024 Example Maintainer\nLicense: Zlib\n",
        )
        .unwrap();

        let (files, _result) = scan_and_assemble(temp_dir.path());

        let copyright_file = files
            .iter()
            .find(|file| file.path.ends_with("/ports/zlib/copyright"))
            .unwrap();
        let package_data = copyright_file
            .package_data
            .iter()
            .find(|pkg_data| {
                pkg_data.datasource_id == Some(DatasourceId::DebianCopyrightStandalone)
            })
            .expect("standalone Debian copyright package data should be present");
        assert_eq!(package_data.name.as_deref(), Some("zlib"));
    }
}
