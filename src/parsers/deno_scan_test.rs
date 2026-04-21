// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_deno_basic_scan_assembles_manifest_and_lockfile() {
        let (files, result) = scan_and_assemble(Path::new("testdata/assembly-golden/deno-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("deno-sample"))
            .expect("deno package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Deno));
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:generic/%40provenant/deno-sample@1.0.0")
        );
        assert_dependency_present(&result.dependencies, "pkg:npm/chalk", "deno.json");
        assert_dependency_present(&result.dependencies, "pkg:npm/chalk@5.6.2", "deno.lock");
        assert_file_links_to_package(
            &files,
            "/deno.json",
            &package.package_uid,
            DatasourceId::DenoJson,
        );
        assert_file_links_to_package(
            &files,
            "/deno.lock",
            &package.package_uid,
            DatasourceId::DenoLock,
        );
    }
}
