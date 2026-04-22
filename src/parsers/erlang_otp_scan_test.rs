// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::DatasourceId;

    #[test]
    fn test_erlang_otp_scan_hoists_rebar_manifest_and_lock_dependencies() {
        let (files, result) =
            scan_and_assemble(Path::new("testdata/assembly-golden/erlang-otp-basic"));

        assert!(result.packages.is_empty());
        assert_eq!(result.dependencies.len(), 8);
        assert!(
            result
                .dependencies
                .iter()
                .all(|dependency| dependency.for_package_uid.is_none())
        );

        assert_dependency_present(
            &result.dependencies,
            "pkg:hex/cowboy@2.10.0",
            "rebar.config",
        );
        assert_dependency_present(&result.dependencies, "pkg:hex/jiffy@1.1.1", "rebar.config");
        assert_dependency_present(&result.dependencies, "pkg:hex/proper@1.4.0", "rebar.config");
        assert_dependency_present(&result.dependencies, "pkg:hex/cowboy@2.10.0", "rebar.lock");
        assert_dependency_present(&result.dependencies, "pkg:hex/cowlib@2.12.1", "rebar.lock");
        assert_dependency_present(
            &result.dependencies,
            "pkg:hex/jiffy@abc123def456",
            "rebar.lock",
        );

        let rebar_config = files
            .iter()
            .find(|file| file.path.ends_with("/rebar.config"))
            .expect("rebar.config should be scanned");
        let rebar_lock = files
            .iter()
            .find(|file| file.path.ends_with("/rebar.lock"))
            .expect("rebar.lock should be scanned");

        assert!(rebar_config.for_packages.is_empty());
        assert!(rebar_lock.for_packages.is_empty());

        assert!(
            rebar_config.package_data.iter().any(|package_data| {
                package_data.datasource_id == Some(DatasourceId::RebarConfig)
            })
        );
        assert!(
            rebar_lock.package_data.iter().any(|package_data| {
                package_data.datasource_id == Some(DatasourceId::RebarLock)
            })
        );
    }
}
