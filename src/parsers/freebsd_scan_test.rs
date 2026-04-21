// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_freebsd_scan_assembles_package_identity_and_declared_license() {
        let (files, result) = scan_and_assemble(Path::new("testdata/freebsd/basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("dmidecode"))
            .expect("dmidecode package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Freebsd));
        assert_eq!(package.version.as_deref(), Some("2.12"));
        assert_eq!(
            package.declared_license_expression.as_deref(),
            Some("gpl-2.0")
        );
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:freebsd/dmidecode@2.12?arch=freebsd:10:x86:64&origin=sysutils/dmidecode")
        );

        let manifest = files
            .iter()
            .find(|file| file.path.ends_with("/+COMPACT_MANIFEST"))
            .expect("+COMPACT_MANIFEST should be scanned");
        assert!(manifest.for_packages.contains(&package.package_uid));
        assert!(manifest.package_data.iter().any(|pkg_data| {
            pkg_data.datasource_id == Some(DatasourceId::FreebsdCompactManifest)
        }));
    }
}
