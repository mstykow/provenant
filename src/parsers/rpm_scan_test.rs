// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};
    use rpm::PackageBuilder;

    #[test]
    fn test_rpm_specfile_scan_assembles_package_and_dependencies() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/rpm/specfile/cpio.spec",
            temp_dir.path().join("cpio.spec"),
        )
        .expect("copy cpio.spec fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("cpio"))
            .expect("cpio package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Rpm));
        assert_eq!(package.version.as_deref(), Some("2.9"));
        assert_eq!(package.purl.as_deref(), Some("pkg:rpm/cpio@2.9"));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:rpm/texinfo")
                && dep.scope.as_deref() == Some("build")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));
        assert!(result.dependencies.iter().any(|dep| {
            dep.purl.as_deref() == Some("pkg:rpm/%2Fsbin%2Finstall-info")
                && dep.scope.as_deref() == Some("post")
                && dep.for_package_uid.as_deref() == Some(package.package_uid.as_str())
        }));

        let specfile = files
            .iter()
            .find(|file| file.path.ends_with("/cpio.spec"))
            .expect("cpio.spec should be scanned");
        assert!(specfile.for_packages.contains(&package.package_uid));
        assert!(
            specfile
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::RpmSpecfile))
        );
    }

    #[test]
    fn test_rpm_specfiles_in_same_directory_remain_separate_packages() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/rpm/specfile/cpio.spec",
            temp_dir.path().join("cpio.spec"),
        )
        .expect("copy cpio.spec fixture");
        fs::copy(
            "testdata/rpm/specfile/openssl.spec",
            temp_dir.path().join("openssl.spec"),
        )
        .expect("copy openssl.spec fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let cpio = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("cpio"))
            .expect("cpio package should be assembled");
        let openssl = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("openssl"))
            .expect("openssl package should be assembled");

        assert_ne!(cpio.package_uid, openssl.package_uid);
        assert_eq!(cpio.datafile_paths.len(), 1);
        assert!(cpio.datafile_paths[0].ends_with("/cpio.spec"));
        assert_eq!(openssl.datafile_paths.len(), 1);
        assert!(openssl.datafile_paths[0].ends_with("/openssl.spec"));

        let cpio_spec = files
            .iter()
            .find(|file| file.path.ends_with("/cpio.spec"))
            .expect("cpio.spec should be scanned");
        let openssl_spec = files
            .iter()
            .find(|file| file.path.ends_with("/openssl.spec"))
            .expect("openssl.spec should be scanned");

        assert_eq!(cpio_spec.for_packages, vec![cpio.package_uid.clone()]);
        assert_eq!(openssl_spec.for_packages, vec![openssl.package_uid.clone()]);
    }

    #[test]
    fn test_rpm_archive_scan_assembles_top_level_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let rpm_path = temp_dir.path().join("demo-1.0-1.x86_64.rpm");
        PackageBuilder::new("demo", "1.0", "MIT", "x86_64", "Demo RPM package")
            .release("1")
            .build()
            .expect("build rpm fixture")
            .write_file(&rpm_path)
            .expect("write rpm fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let rpm_file = files
            .iter()
            .find(|file| file.path.ends_with("/demo-1.0-1.x86_64.rpm"))
            .expect("rpm archive should be scanned");
        assert_eq!(rpm_file.for_packages.len(), 1);
        assert!(
            rpm_file
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::RpmArchive))
        );

        let package = result
            .packages
            .iter()
            .find(|package| Some(&package.package_uid) == rpm_file.for_packages.first())
            .expect("rpm archive should assemble a top-level package");

        assert_eq!(package.package_type, Some(PackageType::Rpm));
        assert!(package.datasource_ids.contains(&DatasourceId::RpmArchive));
        assert_eq!(package.name.as_deref(), Some("demo"));
        assert_eq!(package.version.as_deref(), Some("1.0-1"));
        assert_eq!(package.datafile_paths.len(), 1);
        assert!(package.datafile_paths[0].ends_with("/demo-1.0-1.x86_64.rpm"));
        assert_eq!(rpm_file.for_packages, vec![package.package_uid.clone()]);
    }
}
