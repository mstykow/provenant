// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::PackageType;

    #[test]
    fn test_about_scan_promotes_packages_and_assigns_referenced_files() {
        let (_, result) = scan_and_assemble(Path::new("testdata/about"));

        assert_eq!(result.packages.len(), 2);
        let apipkg = result
            .packages
            .iter()
            .find(|pkg| pkg.name.as_deref() == Some("apipkg"))
            .expect("apipkg package exists");
        let appdirs = result
            .packages
            .iter()
            .find(|pkg| pkg.name.as_deref() == Some("appdirs"))
            .expect("appdirs package exists");

        assert_eq!(apipkg.package_type, Some(PackageType::Pypi));
        assert_eq!(appdirs.package_type, Some(PackageType::Pypi));
        assert_eq!(apipkg.purl.as_deref(), Some("pkg:pypi/apipkg@1.4"));
        assert_eq!(appdirs.purl.as_deref(), Some("pkg:pypi/appdirs@1.4.3"));
    }

    #[test]
    fn test_about_scan_tracks_missing_file_references() {
        let (_, result) = scan_and_assemble(Path::new("testdata/about"));

        let apipkg = result
            .packages
            .iter()
            .find(|pkg| pkg.name.as_deref() == Some("apipkg"))
            .unwrap();
        let appdirs = result
            .packages
            .iter()
            .find(|pkg| pkg.name.as_deref() == Some("appdirs"))
            .unwrap();

        let apipkg_missing = apipkg
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("missing_file_references"))
            .and_then(|value| value.as_array())
            .expect("apipkg missing refs should exist");
        let apipkg_missing_paths: Vec<_> = apipkg_missing
            .iter()
            .filter_map(|value| value.get("path").and_then(|path| path.as_str()))
            .collect();
        assert_eq!(apipkg_missing_paths, vec!["apipkg.LICENSE"]);

        let missing = appdirs
            .extra_data
            .as_ref()
            .and_then(|extra| extra.get("missing_file_references"))
            .and_then(|value| value.as_array())
            .expect("appdirs missing refs should exist");
        let missing_paths: Vec<_> = missing
            .iter()
            .filter_map(|value| value.get("path").and_then(|path| path.as_str()))
            .collect();
        assert!(missing_paths.contains(&"appdirs-1.4.3-py2.py3-none-any.whl"));
        assert!(missing_paths.contains(&"appdirs.LICENSE"));
    }
}
