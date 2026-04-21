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
    fn test_go_basic_scan_assembles_module_and_sum() {
        let (files, result) = scan_and_assemble(Path::new("testdata/assembly-golden/go-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("test-module"))
            .expect("go module should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Golang));
        assert_eq!(
            package.purl.as_deref(),
            Some("pkg:golang/example.com/test-module")
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:golang/github.com/gin-gonic/gin@v1.9.0",
            "go.sum",
        );
        assert_file_links_to_package(&files, "/go.mod", &package.package_uid, DatasourceId::GoMod);
        assert_file_links_to_package(&files, "/go.sum", &package.package_uid, DatasourceId::GoSum);
    }
}
