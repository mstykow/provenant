#[cfg(test)]
mod tests {
    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};
    use std::fs::File;
    use std::path::Path;

    #[cfg(feature = "rpm-sqlite")]
    use std::fs;

    use liblzma::read::XzDecoder;
    use tar::Archive;

    #[cfg(feature = "rpm-sqlite")]
    #[test]
    fn test_rpm_sqlite_scan_assigns_referenced_files_from_rootfs_layout() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let rpmdb_dir = temp_dir.path().join("usr/lib/sysimage/rpm");
        let licenses_dir = temp_dir.path().join("usr/share/licenses/libgcc");

        fs::create_dir_all(&rpmdb_dir).expect("create rpm db dir");
        fs::create_dir_all(&licenses_dir).expect("create licenses dir");
        fs::copy(
            Path::new("testdata/rpm/rpmdb.sqlite"),
            rpmdb_dir.join("rpmdb.sqlite"),
        )
        .expect("copy rpmdb sqlite fixture");
        for suffix in ["-wal", "-shm"] {
            let source = Path::new("testdata/rpm").join(format!("rpmdb.sqlite{suffix}"));
            if source.exists() {
                fs::copy(&source, rpmdb_dir.join(format!("rpmdb.sqlite{suffix}")))
                    .expect("copy rpmdb sidecar fixture");
            }
        }
        fs::write(licenses_dir.join("COPYING"), "license text\n").expect("write COPYING");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let rpmdb = files
            .iter()
            .find(|file| file.path.ends_with("/usr/lib/sysimage/rpm/rpmdb.sqlite"))
            .expect("rpmdb sqlite should be scanned");

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("libgcc"))
            .expect("libgcc package should be assembled");
        let copying = files
            .iter()
            .find(|file| file.path.ends_with("/usr/share/licenses/libgcc/COPYING"))
            .expect("COPYING should be scanned");
        assert_eq!(package.package_type, Some(PackageType::Rpm));
        assert!(copying.for_packages.contains(&package.package_uid));
        assert!(rpmdb.package_data.iter().any(|pkg_data| {
            pkg_data.datasource_id == Some(DatasourceId::RpmInstalledDatabaseSqlite)
        }));
    }

    #[test]
    fn test_rpm_bdb_scan_assigns_referenced_files_from_fedora_rootfs() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let archive = File::open("testdata/rpm/bdb-fedora-rootfs.tar.xz")
            .expect("open fedora rootfs archive");
        Archive::new(XzDecoder::new(archive))
            .unpack(temp_dir.path())
            .expect("extract fedora rootfs archive");

        let rootfs = temp_dir.path().join("rootfs");
        let (files, result) = scan_and_assemble(&rootfs);

        let rpmdb = files
            .iter()
            .find(|file| file.path.ends_with("/var/lib/rpm/Packages"))
            .expect("rpmdb bdb should be scanned");
        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("libgcc"))
            .expect("libgcc package should be assembled from BDB rootfs");
        let copying = files
            .iter()
            .find(|file| file.path.ends_with("/usr/share/licenses/libgcc/COPYING"))
            .expect("COPYING should be scanned from fedora rootfs");

        assert_eq!(package.package_type, Some(PackageType::Rpm));
        assert!(copying.for_packages.contains(&package.package_uid));
        assert!(rpmdb.package_data.iter().any(|pkg_data| {
            pkg_data.datasource_id == Some(DatasourceId::RpmInstalledDatabaseBdb)
                && pkg_data.name.as_deref() == Some("libgcc")
        }));
    }

    #[test]
    fn test_rpm_yumdb_scan_assembles_virtual_package_and_preserves_metadata() {
        let (files, result) = scan_and_assemble(Path::new("testdata/rpm/var/lib/yum/yumdb"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("bash"))
            .expect("bash yumdb package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Rpm));
        assert_eq!(package.version.as_deref(), Some("5.0-1.el8"));
        assert!(package.is_virtual);
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:rpm/bash@5.0-1.el8?arch=x86_64")
        );
        let extra = package
            .extra_data
            .as_ref()
            .expect("yumdb extra_data should exist");
        assert_eq!(
            extra.get("from_repo").and_then(|v| v.as_str()),
            Some("baseos")
        );
        assert_eq!(extra.get("reason").and_then(|v| v.as_str()), Some("dep"));
        assert_eq!(extra.get("releasever").and_then(|v| v.as_str()), Some("8"));

        let from_repo = files
            .iter()
            .find(|file| file.path.ends_with("/from_repo"))
            .expect("from_repo file should be scanned");
        assert!(from_repo.for_packages.contains(&package.package_uid));
        assert!(
            from_repo
                .package_data
                .iter()
                .any(|pkg_data| { pkg_data.datasource_id == Some(DatasourceId::RpmYumdb) })
        );
    }
}
