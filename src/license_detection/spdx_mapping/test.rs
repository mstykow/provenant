// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for SPDX license key mapping.

#[cfg(test)]
mod tests {
    use crate::license_detection::models::License;
    use crate::license_detection::spdx_mapping::*;

    fn simple_license(
        key: &str,
        name: &str,
        spdx_license_key: Option<&str>,
        category: Option<&str>,
        text: &str,
    ) -> License {
        License {
            key: key.to_string(),
            short_name: Some(name.to_string()),
            name: name.to_string(),
            language: Some("en".to_string()),
            spdx_license_key: spdx_license_key.map(str::to_string),
            other_spdx_license_keys: vec![],
            category: category.map(str::to_string),
            owner: None,
            homepage_url: None,
            text: text.to_string(),
            reference_urls: vec![],
            osi_license_key: spdx_license_key.map(str::to_string),
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        }
    }

    fn create_test_licenses() -> Vec<License> {
        vec![
            simple_license(
                "mit",
                "MIT License",
                Some("MIT"),
                Some("Permissive"),
                "MIT License text...",
            ),
            simple_license(
                "gpl-2.0-plus",
                "GNU GPL v2.0 or later",
                Some("GPL-2.0-or-later"),
                Some("Copyleft"),
                "GPL text...",
            ),
            simple_license(
                "apache-2.0",
                "Apache License 2.0",
                Some("Apache-2.0"),
                Some("Permissive"),
                "Apache License text...",
            ),
            simple_license(
                "custom-1",
                "Custom License 1",
                None,
                Some("Unstated License"),
                "Custom license text...",
            ),
        ]
    }

    #[test]
    fn test_build_spdx_mapping() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(mapping.scancode_to_spdx("mit"), Some("MIT".to_string()));
        assert_eq!(
            mapping.scancode_to_spdx("gpl-2.0-plus"),
            Some("GPL-2.0-or-later".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("apache-2.0"),
            Some("Apache-2.0".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("custom-1"),
            Some("LicenseRef-scancode-custom-1".to_string())
        );
    }

    #[test]
    fn test_scancode_to_spdx_unknown_key() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(mapping.scancode_to_spdx("unknown-key"), None);
    }

    #[test]
    fn test_expression_scancode_to_spdx_simple() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("mit");
        assert_eq!(result.unwrap(), "MIT");
    }

    #[test]
    fn test_expression_scancode_to_spdx_and() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("mit AND gpl-2.0-plus");
        assert_eq!(result.unwrap(), "GPL-2.0-or-later AND MIT");
    }

    #[test]
    fn test_expression_scancode_to_spdx_or() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("mit OR apache-2.0");
        assert_eq!(result.unwrap(), "Apache-2.0 OR MIT");
    }

    #[test]
    fn test_expression_scancode_to_spdx_with() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("gpl-2.0-plus WITH custom-1");
        assert!(result.is_ok());
        let spdx_expr = result.unwrap();
        assert!(spdx_expr.contains("GPL-2.0-or-later"));
        assert!(spdx_expr.contains("LicenseRef-scancode-custom-1"));
    }

    #[test]
    fn test_expression_scancode_to_spdx_complex() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("(mit OR apache-2.0) AND gpl-2.0-plus");
        assert!(result.is_ok());
        let spdx_expr = result.unwrap();
        assert!(spdx_expr.contains("MIT"));
        assert!(spdx_expr.contains("Apache-2.0"));
        assert!(spdx_expr.contains("GPL-2.0-or-later"));
    }

    #[test]
    fn test_expression_scancode_to_spdx_custom_license() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("custom-1");
        assert_eq!(result.unwrap(), "LicenseRef-scancode-custom-1");
    }

    #[test]
    fn test_expression_scancode_to_spdx_parentheses() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("(mit)");
        assert_eq!(result.unwrap(), "MIT");
    }

    #[test]
    fn test_expression_scancode_to_spdx_whitespace() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result1 = mapping.expression_scancode_to_spdx("mit AND apache-2.0");
        let result2 = mapping.expression_scancode_to_spdx("mit   AND   apache-2.0");
        assert_eq!(result1.unwrap(), result2.unwrap());
    }

    #[test]
    fn test_expression_scancode_to_spdx_cli_validate() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let scancode_expr = "mit OR gpl-2.0-plus";
        let spdx_expr = mapping.expression_scancode_to_spdx(scancode_expr);

        assert!(spdx_expr.is_ok());
        let exp_str = spdx_expr.unwrap();
        assert_eq!(exp_str, "GPL-2.0-or-later OR MIT");
    }

    #[test]
    fn test_expression_scancode_to_spdx_case_insensitive_input() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("MIT AND Apache-2.0");
        assert_eq!(result.unwrap(), "Apache-2.0 AND MIT");
    }

    #[test]
    fn test_expression_scancode_to_spdx_parse_error() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("mit @ apache-2.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_expression_scancode_to_spdx_empty_parens() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("()");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_from_licenses_empty() {
        let licenses: Vec<License> = vec![];
        let mapping = build_spdx_mapping(&licenses);

        assert!(mapping.scancode_to_spdx("mit").is_none());
    }

    #[test]
    fn test_mapping_with_all_spdx_keys() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(mapping.scancode_to_spdx("mit"), Some("MIT".to_string()));
        assert_eq!(
            mapping.scancode_to_spdx("gpl-2.0-plus"),
            Some("GPL-2.0-or-later".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("apache-2.0"),
            Some("Apache-2.0".to_string())
        );
    }

    #[test]
    fn test_expression_with_license_ref() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("custom-1");
        assert_eq!(result.unwrap(), "LicenseRef-scancode-custom-1");
    }

    #[test]
    fn test_multiple_scancode_keys_same_spdx() {
        let licenses = vec![
            simple_license(
                "mit",
                "MIT License",
                Some("MIT"),
                Some("Permissive"),
                "MIT text",
            ),
            simple_license(
                "mit-x11",
                "MIT X11 License",
                Some("MIT"),
                Some("Permissive"),
                "MIT X11 text",
            ),
        ];
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(mapping.scancode_to_spdx("mit"), Some("MIT".to_string()));
        assert_eq!(mapping.scancode_to_spdx("mit-x11"), Some("MIT".to_string()));
    }

    #[test]
    fn test_deprecated_license_mapping() {
        let mut deprecated = simple_license(
            "gpl-2.0-old",
            "GNU GPL 2.0 (deprecated)",
            Some("GPL-2.0"),
            Some("Copyleft"),
            "Old GPL",
        );
        deprecated.is_deprecated = true;
        deprecated.replaced_by = vec!["gpl-2.0".to_string()];
        let licenses = vec![
            simple_license(
                "gpl-2.0",
                "GNU General Public License 2.0",
                Some("GPL-2.0"),
                Some("Copyleft"),
                "GPL 2.0 text",
            ),
            deprecated,
        ];
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(
            mapping.scancode_to_spdx("gpl-2.0"),
            Some("GPL-2.0".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("gpl-2.0-old"),
            Some("GPL-2.0".to_string())
        );
    }

    #[test]
    fn test_or_later_variants() {
        let licenses = vec![
            simple_license(
                "gpl-2.0-plus",
                "GPL 2.0 or later",
                Some("GPL-2.0-or-later"),
                Some("Copyleft"),
                "GPL text",
            ),
            simple_license(
                "gpl-3.0-plus",
                "GPL 3.0 or later",
                Some("GPL-3.0-or-later"),
                Some("Copyleft"),
                "GPL 3.0 text",
            ),
            simple_license(
                "lgpl-2.1-plus",
                "LGPL 2.1 or later",
                Some("LGPL-2.1-or-later"),
                Some("Copyleft"),
                "LGPL text",
            ),
        ];
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(
            mapping.scancode_to_spdx("gpl-2.0-plus"),
            Some("GPL-2.0-or-later".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("gpl-3.0-plus"),
            Some("GPL-3.0-or-later".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("lgpl-2.1-plus"),
            Some("LGPL-2.1-or-later".to_string())
        );

        let result = mapping.expression_scancode_to_spdx("gpl-2.0-plus OR lgpl-2.1-plus");
        assert_eq!(result.unwrap(), "GPL-2.0-or-later OR LGPL-2.1-or-later");
    }

    #[test]
    fn test_with_exception_expressions() {
        let licenses = vec![
            simple_license(
                "gpl-2.0",
                "GPL 2.0",
                Some("GPL-2.0-only"),
                Some("Copyleft"),
                "GPL text",
            ),
            simple_license(
                "classpath-exception-2.0",
                "Classpath Exception 2.0",
                Some("Classpath-exception-2.0"),
                Some("Copyleft"),
                "Exception text",
            ),
            simple_license(
                "gcc-exception-3.1",
                "GCC Exception 3.1",
                Some("GCC-exception-3.1"),
                Some("Copyleft"),
                "GCC exception text",
            ),
        ];
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("gpl-2.0 WITH classpath-exception-2.0");
        assert_eq!(result.unwrap(), "GPL-2.0-only WITH Classpath-exception-2.0");

        let result2 = mapping.expression_scancode_to_spdx("gpl-2.0 WITH gcc-exception-3.1");
        assert_eq!(result2.unwrap(), "GPL-2.0-only WITH GCC-exception-3.1");
    }

    #[test]
    fn test_deeply_nested_expression() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping
            .expression_scancode_to_spdx("((mit OR apache-2.0) AND gpl-2.0-plus) OR custom-1");
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(expr.contains("MIT"));
        assert!(expr.contains("Apache-2.0"));
        assert!(expr.contains("GPL-2.0-or-later"));
        assert!(expr.contains("LicenseRef-scancode-custom-1"));
    }

    #[test]
    fn test_license_key_with_special_chars() {
        let licenses = vec![
            simple_license(
                "bsd-2-clause",
                "BSD 2-Clause",
                Some("BSD-2-Clause"),
                Some("Permissive"),
                "BSD text",
            ),
            simple_license(
                "boost-1.0",
                "Boost 1.0",
                Some("BSL-1.0"),
                Some("Permissive"),
                "Boost text",
            ),
            simple_license(
                "unicode_dfs_2015",
                "Unicode DFS 2015",
                Some("Unicode-DFS-2015"),
                Some("Permissive"),
                "Unicode text",
            ),
        ];
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(
            mapping.scancode_to_spdx("bsd-2-clause"),
            Some("BSD-2-Clause".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("boost-1.0"),
            Some("BSL-1.0".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("unicode_dfs_2015"),
            Some("Unicode-DFS-2015".to_string())
        );
    }

    #[test]
    fn test_multiple_with_in_expression() {
        let licenses = vec![
            simple_license(
                "gpl-2.0",
                "GPL 2.0",
                Some("GPL-2.0-only"),
                Some("Copyleft"),
                "GPL text",
            ),
            simple_license(
                "exception-a",
                "Exception A",
                Some("Exception-A"),
                Some("Exception"),
                "Exception A text",
            ),
            simple_license(
                "exception-b",
                "Exception B",
                Some("Exception-B"),
                Some("Exception"),
                "Exception B text",
            ),
        ];
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx(
            "(gpl-2.0 WITH exception-a) OR (gpl-2.0 WITH exception-b)",
        );
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(expr.contains("GPL-2.0-only WITH Exception-A"));
        assert!(expr.contains("GPL-2.0-only WITH Exception-B"));
    }

    #[test]
    fn test_licenseref_preserved_on_unknown_key() {
        let licenses: Vec<License> = vec![];
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("unknown-license");
        assert_eq!(result.unwrap(), "LicenseRef-scancode-unknown-license");
    }

    #[test]
    fn test_mixed_known_and_unknown_licenses() {
        let licenses = vec![simple_license(
            "mit",
            "MIT License",
            Some("MIT"),
            Some("Permissive"),
            "MIT text",
        )];
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("mit AND unknown-license");
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(expr.contains("MIT"));
        assert!(expr.contains("LicenseRef-scancode-unknown-license"));
    }

    #[test]
    fn test_expression_to_spdx_preserves_operator_case() {
        let licenses = create_test_licenses();
        let mapping = build_spdx_mapping(&licenses);

        let result = mapping.expression_scancode_to_spdx("mit and gpl-2.0-plus or apache-2.0");
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(expr.contains(" AND "));
        assert!(expr.contains(" OR "));
    }

    #[test]
    fn test_common_license_mappings() {
        let licenses = vec![
            simple_license(
                "mit",
                "MIT License",
                Some("MIT"),
                Some("Permissive"),
                "MIT text",
            ),
            simple_license(
                "apache-2.0",
                "Apache 2.0",
                Some("Apache-2.0"),
                Some("Permissive"),
                "Apache text",
            ),
            simple_license(
                "bsd-3-clause",
                "BSD 3-Clause",
                Some("BSD-3-Clause"),
                Some("Permissive"),
                "BSD text",
            ),
            simple_license(
                "isc",
                "ISC License",
                Some("ISC"),
                Some("Permissive"),
                "ISC text",
            ),
            simple_license(
                "mpl-2.0",
                "MPL 2.0",
                Some("MPL-2.0"),
                Some("Copyleft"),
                "MPL text",
            ),
        ];
        let mapping = build_spdx_mapping(&licenses);

        assert_eq!(mapping.scancode_to_spdx("mit"), Some("MIT".to_string()));
        assert_eq!(
            mapping.scancode_to_spdx("apache-2.0"),
            Some("Apache-2.0".to_string())
        );
        assert_eq!(
            mapping.scancode_to_spdx("bsd-3-clause"),
            Some("BSD-3-Clause".to_string())
        );
        assert_eq!(mapping.scancode_to_spdx("isc"), Some("ISC".to_string()));
        assert_eq!(
            mapping.scancode_to_spdx("mpl-2.0"),
            Some("MPL-2.0".to_string())
        );
    }
}
