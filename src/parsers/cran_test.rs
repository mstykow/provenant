// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::cran::CranParser;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_is_match() {
        let valid_path = PathBuf::from("/some/path/DESCRIPTION");
        let invalid_path = PathBuf::from("/some/path/description");
        let another_invalid = PathBuf::from("/some/path/DESCRIPTION.txt");

        assert!(CranParser::is_match(&valid_path));
        assert!(!CranParser::is_match(&invalid_path));
        assert!(!CranParser::is_match(&another_invalid));
    }

    #[test]
    fn test_extract_from_testdata_geometry() {
        let desc_path = PathBuf::from("testdata/cran/geometry/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        // Basic fields
        assert_eq!(package_data.package_type, Some(PackageType::Cran));
        assert_eq!(package_data.name, Some("geometry".to_string()));
        assert_eq!(package_data.version, Some("0.4.2".to_string()));
        assert_eq!(package_data.primary_language, Some("R".to_string()));

        // License
        assert_eq!(
            package_data.extracted_license_statement,
            Some("GPL (>= 3)".to_string())
        );

        // PURL
        assert_eq!(
            package_data.purl,
            Some("pkg:cran/geometry@0.4.2".to_string())
        );

        // Repository URL
        assert_eq!(
            package_data.repository_homepage_url,
            Some("https://cran.r-project.org/package=geometry".to_string())
        );

        // Homepage URL
        assert_eq!(
            package_data.homepage_url,
            Some("https://davidcsterratt.github.io/geometry".to_string())
        );

        // Description (Title + Description combined)
        assert!(package_data.description.is_some());
        let desc = package_data.description.unwrap();
        assert!(desc.contains("Mesh Generation and Surface Tessellation"));
        assert!(desc.contains("Qhull"));

        // Dependencies
        // Depends: R (>= 3.0.0) -- should be filtered out
        // Imports: magic, Rcpp, lpSolve, linprog (4 packages)
        // Suggests: spelling, testthat, rgl, R.matlab, tripack (5 packages)
        // LinkingTo: Rcpp, RcppProgress (2 packages)
        // Total: 11 dependencies (R version filtered out)

        assert_eq!(package_data.dependencies.len(), 11);

        // Check Imports scope
        let imports: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("imports".to_string()))
            .collect();
        assert_eq!(imports.len(), 4);

        // Check Suggests scope
        let suggests: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("suggests".to_string()))
            .collect();
        assert_eq!(suggests.len(), 5);

        // Check LinkingTo scope
        let linking_to: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope == Some("linkingto".to_string()))
            .collect();
        assert_eq!(linking_to.len(), 2);

        // Check that Imports are runtime dependencies
        for dep in &imports {
            assert_eq!(dep.is_runtime, Some(true));
            assert_eq!(dep.is_optional, Some(false));
        }

        // Check that Suggests are optional
        for dep in &suggests {
            assert_eq!(dep.is_optional, Some(true));
        }

        // Parties (Maintainer and Authors from Author field)
        // Maintainer: David C. Sterratt <david.c.sterratt@ed.ac.uk>
        // Author field has 7 authors (line-wrapped, comma-separated)
        assert!(!package_data.parties.is_empty());

        let maintainers: Vec<_> = package_data
            .parties
            .iter()
            .filter(|p| p.role == Some("maintainer".to_string()))
            .collect();
        assert_eq!(maintainers.len(), 1);
        assert_eq!(maintainers[0].name, Some("David C. Sterratt".to_string()));
        assert_eq!(
            maintainers[0].email,
            Some("david.c.sterratt@ed.ac.uk".to_string())
        );

        // Authors
        let authors: Vec<_> = package_data
            .parties
            .iter()
            .filter(|p| p.role == Some("author".to_string()))
            .collect();
        // Note: The Author field is not properly comma-separated in this test file
        // It's actually Authors@R which we don't parse. But we have an Author field
        // in the DESCRIPTION that lists multiple authors.
        assert!(!authors.is_empty());
    }

    #[test]
    fn test_extract_from_testdata_codetools() {
        let desc_path = PathBuf::from("testdata/cran/codetools/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        // Basic fields
        assert_eq!(package_data.name, Some("codetools".to_string()));
        assert_eq!(package_data.version, Some("0.2-16".to_string()));
        assert_eq!(
            package_data.extracted_license_statement,
            Some("GPL".to_string())
        );

        // Description
        assert!(package_data.description.is_some());
        let desc = package_data.description.unwrap();
        assert!(desc.contains("Code Analysis Tools for R"));
        assert!(desc.contains("Code analysis tools for R"));

        // Dependencies
        // Depends: R (>= 2.1) -- should be filtered out
        // No other dependencies
        assert_eq!(package_data.dependencies.len(), 0);

        // Parties
        let maintainers: Vec<_> = package_data
            .parties
            .iter()
            .filter(|p| p.role == Some("maintainer".to_string()))
            .collect();
        assert_eq!(maintainers.len(), 1);
        assert_eq!(maintainers[0].name, Some("Luke Tierney".to_string()));
        assert_eq!(
            maintainers[0].email,
            Some("luke-tierney@uiowa.edu".to_string())
        );

        let authors: Vec<_> = package_data
            .parties
            .iter()
            .filter(|p| p.role == Some("author".to_string()))
            .collect();
        assert_eq!(authors.len(), 1);
        assert_eq!(authors[0].name, Some("Luke Tierney".to_string()));
    }

    #[test]
    fn test_dependencies_with_versions() {
        // Test version constraint parsing
        let desc_path = PathBuf::from("testdata/cran/geometry/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        // All dependencies in this file don't have version constraints
        // except the R version in Depends which we filter out
        for dep in &package_data.dependencies {
            assert_eq!(dep.is_pinned, Some(false));
            assert!(dep.extracted_requirement.is_none());
        }
    }

    #[test]
    fn test_filter_r_version() {
        // Verify that R version requirements are filtered out
        let desc_path = PathBuf::from("testdata/cran/geometry/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        // Check that no dependency has "R" as its package name (but allow Rcpp, RcppProgress)
        for dep in &package_data.dependencies {
            if let Some(ref purl) = dep.purl {
                // Should not be exactly "pkg:cran/R" or "pkg:cran/R@version"
                assert!(!purl.starts_with("pkg:cran/R@"));
                assert!(purl != "pkg:cran/R");
            }
        }
    }

    #[test]
    fn test_multiple_dependency_types() {
        let desc_path = PathBuf::from("testdata/cran/geometry/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        // Verify we have dependencies from multiple scopes
        let scopes: std::collections::HashSet<_> = package_data
            .dependencies
            .iter()
            .filter_map(|d| d.scope.as_ref())
            .collect();

        assert!(scopes.contains(&"imports".to_string()));
        assert!(scopes.contains(&"suggests".to_string()));
        assert!(scopes.contains(&"linkingto".to_string()));
    }

    #[test]
    fn test_author_maintainer_parsing() {
        let desc_path = PathBuf::from("testdata/cran/codetools/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        // Check maintainer extraction
        let maintainers: Vec<_> = package_data
            .parties
            .iter()
            .filter(|p| p.role == Some("maintainer".to_string()))
            .collect();

        assert_eq!(maintainers.len(), 1);
        assert!(maintainers[0].name.is_some());
        assert!(maintainers[0].email.is_some());

        // Check author extraction
        let authors: Vec<_> = package_data
            .parties
            .iter()
            .filter(|p| p.role == Some("author".to_string()))
            .collect();

        assert_eq!(authors.len(), 1);
        assert!(authors[0].name.is_some());
    }

    #[test]
    fn test_empty_description() {
        // Test with minimal DESCRIPTION file
        let nonexistent_path = PathBuf::from("testdata/cran/nonexistent/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&nonexistent_path);

        // Should return default data with proper type and datasource
        assert_eq!(package_data.package_type, Some(PackageType::Cran));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::CranDescription)
        );
        assert_eq!(package_data.primary_language, Some("R".to_string()));
        assert!(package_data.name.is_none());
        assert!(package_data.version.is_none());
    }

    #[test]
    fn test_purl_generation() {
        let desc_path = PathBuf::from("testdata/cran/codetools/DESCRIPTION");
        let package_data = CranParser::extract_first_package(&desc_path);

        assert_eq!(
            package_data.purl,
            Some("pkg:cran/codetools@0.2-16".to_string())
        );

        // Check dependency PURLs
        for dep in &package_data.dependencies {
            if let Some(ref purl) = dep.purl {
                assert!(purl.starts_with("pkg:cran/"));
            }
        }
    }

    #[test]
    fn test_dependency_versions_with_hyphenated_constraints() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cran-hyphenated-constraint-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");

        let desc_path = dir.join("DESCRIPTION");
        fs::write(
            &desc_path,
            "Package: demo\nVersion: 0.1.0\nSuggests:\n    sf (>= 0.7-3), svglite (>= 2.1.2), testthat (== 3.3.0)\n",
        )
        .expect("write temp DESCRIPTION");

        let package_data = CranParser::extract_first_package(&desc_path);

        let sf = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cran/sf"))
            .expect("sf dependency missing");
        assert_eq!(sf.extracted_requirement.as_deref(), Some(">= 0.7-3"));
        assert_eq!(sf.is_pinned, Some(false));

        let svglite = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cran/svglite"))
            .expect("svglite dependency missing");
        assert_eq!(svglite.extracted_requirement.as_deref(), Some(">= 2.1.2"));
        assert_eq!(svglite.is_pinned, Some(false));

        let testthat = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:cran/testthat@3.3.0"))
            .expect("testthat dependency missing");
        assert_eq!(testthat.extracted_requirement.as_deref(), Some("== 3.3.0"));
        assert_eq!(testthat.is_pinned, Some(true));

        fs::remove_dir_all(&dir).expect("remove temp dir");
    }
}
