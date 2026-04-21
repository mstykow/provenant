// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::DatasourceId;

    #[test]
    fn test_arch_srcinfo_scan_remains_unassembled_and_hoists_dependencies() {
        let (files, result) = scan_and_assemble(Path::new("testdata/arch/srcinfo/split"));

        assert!(result.packages.is_empty());
        assert_dependency_present(&result.dependencies, "pkg:alpm/arch/glibc", ".SRCINFO");
        assert_dependency_present(&result.dependencies, "pkg:alpm/arch/gcc-libs", ".SRCINFO");
        assert!(
            result
                .dependencies
                .iter()
                .all(|dep| dep.for_package_uid.is_none())
        );
        let srcinfo = files
            .iter()
            .find(|file| file.path.ends_with("/.SRCINFO"))
            .expect(".SRCINFO should be scanned");
        assert!(srcinfo.for_packages.is_empty());
        assert!(
            srcinfo
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::ArchSrcinfo))
        );
    }
}
