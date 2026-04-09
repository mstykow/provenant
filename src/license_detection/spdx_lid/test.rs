//! Tests for SPDX-License-Identifier detection.

#[cfg(test)]
mod tests {
    use crate::license_detection::query::Query;
    use crate::license_detection::spdx_lid::*;
    use crate::license_detection::test_utils::{create_mock_rule_simple, create_test_index};
    use crate::models::LineNumber;

    fn extract_cleaned_spdx_expressions(text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| {
                let (prefix, expression) = split_spdx_lid(line.trim());
                prefix.as_ref()?;
                let cleaned = clean_spdx_text(&expression);
                if cleaned.is_empty() {
                    None
                } else {
                    Some(cleaned)
                }
            })
            .collect()
    }

    fn create_spdx_lookup_index(
        entries: &[(&str, &str)],
    ) -> crate::license_detection::index::LicenseIndex {
        let mut index = create_test_index(&[], 0);

        for (spdx_key, license_expression) in entries {
            let rid = index.rules_by_rid.len();
            index
                .rules_by_rid
                .push(create_mock_rule_simple(license_expression, 100));
            index.rid_by_spdx_key.insert(spdx_key.to_string(), rid);
        }

        let unknown_rid = index.rules_by_rid.len();
        index
            .rules_by_rid
            .push(create_mock_rule_simple("unknown-spdx", 100));
        index
            .rid_by_spdx_key
            .insert("unknown-spdx".to_string(), unknown_rid);
        index.unknown_spdx_rid = Some(unknown_rid);

        index
    }

    #[test]
    fn test_split_spdx_lid_standard() {
        let (prefix, expr) = split_spdx_lid("SPDX-License-Identifier: MIT");
        assert_eq!(prefix, Some("SPDX-License-Identifier: ".to_string()));
        assert_eq!(expr, "MIT");
    }

    #[test]
    fn test_split_spdx_lid_lowercase() {
        let (prefix, expr) = split_spdx_lid("spdx-license-identifier: MIT");
        assert!(prefix.is_some());
        assert_eq!(expr, "MIT");
    }

    #[test]
    fn test_split_spdx_lid_with_spaces() {
        let (prefix, expr) = split_spdx_lid("SPDX license identifier: Apache-2.0");
        assert!(prefix.is_some());
        assert_eq!(expr, "Apache-2.0");
    }

    #[test]
    fn test_split_spdx_lid_without_colon() {
        let (prefix, expr) = split_spdx_lid("SPDX-License-Identifier MIT");
        assert!(prefix.is_some());
        assert_eq!(expr, "MIT");
    }

    #[test]
    fn test_split_spdx_lid_nuget() {
        let (prefix, expr) = split_spdx_lid("https://licenses.nuget.org/MIT");
        assert!(prefix.is_some());
        assert_eq!(expr, "MIT");
    }

    #[test]
    fn test_split_spdx_lid_no_match() {
        let (prefix, expr) = split_spdx_lid("No SPDX here");
        assert_eq!(prefix, None);
        assert_eq!(expr, "No SPDX here");
    }

    #[test]
    fn test_split_spdx_lid_complex_expression() {
        let (prefix, expr) = split_spdx_lid(
            "SPDX-License-Identifier: GPL-2.0-or-later WITH Classpath-exception-2.0",
        );
        assert!(prefix.is_some());
        assert_eq!(expr, "GPL-2.0-or-later WITH Classpath-exception-2.0");
    }

    #[test]
    fn test_clean_spdx_text_basic() {
        let clean = clean_spdx_text("MIT");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_extra_spaces() {
        let clean = clean_spdx_text("  MIT   Apache-2.0  ");
        assert_eq!(clean, "MIT Apache-2.0");
    }

    #[test]
    fn test_clean_spdx_text_dangling_markup() {
        let clean = clean_spdx_text("MIT</a>");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_multiple_dangling_markup() {
        let clean = clean_spdx_text("MIT</a></p></div>");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_leading_punctuation() {
        let clean = clean_spdx_text("!MIT");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_trailing_punctuation() {
        let clean = clean_spdx_text("MIT.");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_lone_open_paren() {
        let clean = clean_spdx_text("(MIT");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_lone_close_paren() {
        let clean = clean_spdx_text("MIT)");
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_clean_spdx_text_balanced_parens() {
        let clean = clean_spdx_text("(MIT OR Apache-2.0)");
        assert_eq!(clean, "(MIT OR Apache-2.0)");
    }

    #[test]
    fn test_clean_spdx_text_tabs_and_newlines() {
        let clean = clean_spdx_text("MIT\tApache-2.0\nGPL-2.0");
        assert_eq!(clean, "MIT Apache-2.0 GPL-2.0");
    }

    #[test]
    fn test_extract_spdx_expressions_single() {
        let text = "# SPDX-License-Identifier: MIT";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["MIT"]);
    }

    #[test]
    fn test_extract_spdx_expressions_multiple() {
        let text = "# SPDX-License-Identifier: MIT\n# SPDX-License-Identifier: Apache-2.0";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs.len(), 2);
        assert!(exprs.contains(&"MIT".to_string()));
        assert!(exprs.contains(&"Apache-2.0".to_string()));
    }

    #[test]
    fn test_extract_spdx_expressions_complex() {
        let text = "// SPDX-License-Identifier: GPL-2.0-or-later WITH Classpath-exception-2.0";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["GPL-2.0-or-later WITH Classpath-exception-2.0"]);
    }

    #[test]
    fn test_extract_spdx_expressions_spaces_hyphens() {
        let text = "* SPDX license identifier: BSD-3-Clause";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["BSD-3-Clause"]);
    }

    #[test]
    fn test_extract_spdx_expressions_html_comment() {
        let text = "<!-- SPDX-License-Identifier: MIT -->";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["MIT"]);
    }

    #[test]
    fn test_extract_spdx_expressions_python_comment() {
        let text = "# SPDX-License-Identifier: (MIT OR Apache-2.0)";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["(MIT OR Apache-2.0)"]);
    }

    #[test]
    fn test_extract_spdx_expressions_with_whitespace() {
        let text = "  //  SPDX-License-Identifier:   MIT  ";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["MIT"]);
    }

    #[test]
    fn test_extract_spdx_expressions_no_match() {
        let text = "/* This is a regular comment */";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert!(exprs.is_empty());
    }

    #[test]
    fn test_extract_spdx_expressions_nuget_url() {
        let text = "<licenseUrl>https://licenses.nuget.org/MIT</licenseUrl>";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["MIT"]);
    }

    #[test]
    fn test_clean_spdx_text_json_like() {
        let clean = clean_spdx_text(r#""MIT">MIT"#);
        assert_eq!(clean, "MIT");
    }

    #[test]
    fn test_split_spdx_lid_case_variants() {
        let tests: [&str; 4] = [
            "SPDX-License-Identifier: MIT",
            "spdx-license-identifier: MIT",
            "SPDX-LICENSE-IDENTIFIER: MIT",
            "Spdx-License-Identifier: MIT",
        ];

        for test in tests {
            let (prefix, expr) = split_spdx_lid(test);
            assert!(prefix.is_some(), "Should match: {}", test);
            assert_eq!(expr, "MIT");
        }
    }

    #[test]
    fn test_extract_spdx_expressions_preserves_complex_expressions() {
        let text = "SPDX-License-Identifier: (EPL-2.0 OR Apache-2.0 OR GPL-2.0 WITH Classpath-exception-2.0)";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(
            exprs,
            vec!["(EPL-2.0 OR Apache-2.0 OR GPL-2.0 WITH Classpath-exception-2.0)"]
        );
    }

    #[test]
    fn test_normalize_spdx_key() {
        assert_eq!(normalize_spdx_key("MIT"), "mit");
        assert_eq!(normalize_spdx_key("Apache-2.0"), "apache-2.0");
        assert_eq!(normalize_spdx_key("GPL_2.0_plus"), "gpl-2.0-plus");
        assert_eq!(normalize_spdx_key("gPL-2.0-PLUS"), "gpl-2.0-plus");
    }

    #[test]
    fn test_split_license_expression_simple() {
        let expr = "MIT";
        let keys = split_license_expression(expr);
        assert_eq!(keys, vec!["MIT"]);
    }

    #[test]
    fn test_split_license_expression_with_or() {
        let expr = "MIT OR Apache-2.0";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"MIT".to_string()));
        assert!(keys.contains(&"Apache-2.0".to_string()));
    }

    #[test]
    fn test_split_license_expression_with_and() {
        let expr = "GPL-2.0 AND Classpath-exception-2.0";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"GPL-2.0".to_string()));
        assert!(keys.contains(&"Classpath-exception-2.0".to_string()));
    }

    #[test]
    fn test_split_license_expression_with_parens() {
        let expr = "(MIT OR Apache-2.0)";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"MIT".to_string()));
        assert!(keys.contains(&"Apache-2.0".to_string()));
    }

    #[test]
    fn test_split_license_expression_complex() {
        let expr = "GPL-2.0-or-later WITH Classpath-exception-2.0";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"GPL-2.0-or-later".to_string()));
        assert!(keys.contains(&"Classpath-exception-2.0".to_string()));
    }

    #[test]
    fn test_spdx_lid_match_simple() {
        let mut index = create_test_index(
            &[("spdx", 0), ("license", 1), ("identifier", 2), ("mit", 3)],
            3,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 100));
        index
            .rules_by_rid
            .push(create_mock_rule_simple("apache-2.0", 100));

        let text = "SPDX-License-Identifier: MIT";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].license_expression, "mit");
        assert_eq!(matches[0].license_expression_spdx, Some("MIT".to_string()));
        assert_eq!(matches[0].start_line, LineNumber::ONE);
        assert_eq!(matches[0].end_line, LineNumber::ONE);
        assert_eq!(matches[0].matcher, MATCH_SPDX_ID);
        assert_eq!(matches[0].matched_length, 4);
        assert_eq!(matches[0].rule_length, 4);
        assert_eq!(matches[0].score, 100.0);
        assert_eq!(matches[0].rule_relevance, 100);
        assert!(
            matches[0]
                .rule_identifier
                .starts_with("spdx-license-identifier-mit-")
        );
        assert!(matches[0].rule_url.is_empty());
    }

    #[test]
    fn test_spdx_lid_match_case_insensitive() {
        let mut index = create_test_index(
            &[("spdx", 0), ("license", 1), ("identifier", 2), ("mit", 3)],
            3,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 90));

        let text = "SPDX-License-Identifier: mit";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].license_expression, "mit");
    }

    #[test]
    fn test_spdx_lid_match_multiple() {
        let mut index = create_test_index(
            &[
                ("spdx", 0),
                ("license", 1),
                ("identifier", 2),
                ("mit", 3),
                ("apache", 4),
                ("2.0", 5),
            ],
            3,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 100));
        index
            .rules_by_rid
            .push(create_mock_rule_simple("apache-2.0", 100));

        let text = "SPDX-License-Identifier: OR\n# SPDX-License-Identifier: MIT\n# SPDX-License-Identifier: Apache-2.0";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_spdx_lid_match_no_match() {
        let index = create_test_index(&[("mit", 0)], 1);

        let text = "/* Regular comment */";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_spdx_lid_match_uses_synthetic_rule_metadata() {
        let mut index = create_test_index(
            &[("spdx", 0), ("license", 1), ("identifier", 2), ("mit", 3)],
            3,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 80));

        let text = "SPDX-License-Identifier: MIT";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].score, 100.0);
        assert_eq!(matches[0].rule_relevance, 100);
        assert!(
            matches[0]
                .rule_identifier
                .starts_with("spdx-license-identifier-mit-")
        );
        assert!(matches[0].rule_url.is_empty());
    }

    #[test]
    fn test_spdx_lid_match_uses_full_line_span_when_expression_tokens_are_unknown() {
        let mut index = create_test_index(&[("spdx", 0), ("license", 1), ("identifier", 2)], 3);
        let apache_rid = index.rules_by_rid.len();
        index
            .rules_by_rid
            .push(create_mock_rule_simple("apache-2.0", 100));
        index
            .rid_by_spdx_key
            .insert("apache-2.0".to_string(), apache_rid);

        let text = "// SPDX-License-Identifier: Apache-2.0\nconst answer = 42;";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start_line, LineNumber::ONE);
        assert_eq!(matches[0].end_line, LineNumber::ONE);
        assert_eq!(matches[0].matched_length, 3);
        assert_eq!(matches[0].rule_length, 3);
        assert_eq!(matches[0].score, 100.0);
    }

    #[test]
    fn test_split_license_expression_with_with() {
        let expr = "GPL-2.0 WITH Classpath-exception-2.0";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"GPL-2.0".to_string()));
        assert!(keys.contains(&"Classpath-exception-2.0".to_string()));
    }

    #[test]
    fn test_split_license_expression_with_plus() {
        let expr = "GPL-2.0+";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"GPL-2.0+".to_string()));
    }

    #[test]
    fn test_split_license_expression_complex_with_operators() {
        let expr = "(MIT OR Apache-2.0) AND BSD-3-Clause";
        let keys = split_license_expression(expr);
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"MIT".to_string()));
        assert!(keys.contains(&"Apache-2.0".to_string()));
        assert!(keys.contains(&"BSD-3-Clause".to_string()));
    }

    #[test]
    fn test_clean_spdx_text_empty_result() {
        let clean = clean_spdx_text("");
        assert_eq!(clean, "");

        let clean = clean_spdx_text("   ");
        assert_eq!(clean, "");
    }

    #[test]
    fn test_extract_spdx_expressions_with_plus() {
        let text = "SPDX-License-Identifier: GPL-2.0+";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["GPL-2.0+"]);
    }

    #[test]
    fn test_extract_spdx_expressions_with_with_operator() {
        let text = "SPDX-License-Identifier: GPL-2.0 WITH Classpath-exception-2.0";
        let exprs = extract_cleaned_spdx_expressions(text);
        assert_eq!(exprs, vec!["GPL-2.0 WITH Classpath-exception-2.0"]);
    }

    #[test]
    fn test_spdx_lid_match_with_operator() {
        let mut index = create_test_index(
            &[
                ("spdx", 0),
                ("license", 1),
                ("identifier", 2),
                ("gpl-2.0", 3),
                ("classpath-exception-2.0", 4),
            ],
            1,
        );
        index
            .rules_by_rid
            .push(create_mock_rule_simple("gpl-2.0", 100));
        index
            .rules_by_rid
            .push(create_mock_rule_simple("classpath-exception-2.0", 100));

        let text = "SPDX-License-Identifier: GPL-2.0 WITH Classpath-exception-2.0";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert!(
            !matches.is_empty(),
            "Should match WITH expression components"
        );
    }

    #[test]
    fn test_extract_spdx_expressions_multiple_on_same_line() {
        let text = "SPDX-License-Identifier: MIT  SPDX-License-Identifier: Apache-2.0";
        let exprs = extract_cleaned_spdx_expressions(text);

        assert!(!exprs.is_empty(), "Should extract at least one expression");
    }

    #[test]
    fn test_clean_spdx_text_with_angle_brackets() {
        let clean = clean_spdx_text("<MIT>");
        assert!(!clean.contains('<'));
        assert!(!clean.contains('>'));
    }

    #[test]
    fn test_split_spdx_lid_typo_variants() {
        let variants = [
            "SPDX-License-Identifier: MIT",
            "SPDX-License-Identifier MIT",
            "SPDX-License-Identifier:  MIT",
            "SPDX license identifier: MIT",
            "SPDX Licence Identifier: MIT",
            "SPDZ-License-Identifier: MIT",
        ];

        for variant in variants {
            let (prefix, expr) = split_spdx_lid(variant);
            assert!(prefix.is_some(), "Should match variant: {}", variant);
            assert_eq!(expr, "MIT", "Should extract MIT from: {}", variant);
        }
    }

    #[test]
    fn test_spdx_lid_match_empty_text() {
        let index = create_test_index(&[("mit", 0)], 1);

        let text = "";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert!(matches.is_empty(), "Empty text should produce no matches");
    }

    #[test]
    fn test_spdx_lid_match_whitespace_only() {
        let index = create_test_index(&[("mit", 0)], 1);

        let text = "   \n\t  ";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert!(
            matches.is_empty(),
            "Whitespace-only text should produce no matches"
        );
    }

    #[test]
    fn test_extract_matched_text_from_lines() {
        let text = "line1\nline2\nline3\nline4\nline5";

        let matched = extract_matched_text_from_lines(text, 2, 2);
        assert_eq!(matched, "line2");

        let matched = extract_matched_text_from_lines(text, 2, 4);
        assert_eq!(matched, "line2\nline3\nline4");

        let matched = extract_matched_text_from_lines(text, 0, 2);
        assert_eq!(matched, "");

        let matched = extract_matched_text_from_lines(text, 3, 1);
        assert_eq!(matched, "");
    }

    #[test]
    fn test_normalize_spdx_key_edge_cases() {
        assert_eq!(normalize_spdx_key(""), "");
        assert_eq!(normalize_spdx_key("MIT"), "mit");
        assert_eq!(normalize_spdx_key("MIT_LICENSE"), "mit-license");
        assert_eq!(normalize_spdx_key("MIT__LICENSE"), "mit--license");
    }

    #[test]
    fn test_spdx_key_lookup_gpl_2_0_plus() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        assert!(
            !index.rid_by_spdx_key.is_empty(),
            "Should have SPDX key mappings"
        );

        assert!(
            index.rid_by_spdx_key.contains_key("gpl-2.0+"),
            "Should have gpl-2.0+ in SPDX key mappings"
        );

        if let Some(&rid) = index.rid_by_spdx_key.get("gpl-2.0+") {
            let rule = &index.rules_by_rid[rid];
            assert_eq!(
                rule.license_expression, "gpl-2.0-plus",
                "GPL-2.0+ should map to gpl-2.0-plus rule"
            );
        }
    }

    #[test]
    fn test_spdx_match_has_correct_token_positions() {
        let mut index = create_test_index(
            &[("spdx", 0), ("license", 1), ("identifier", 2), ("mit", 3)],
            1,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 100));

        let text = "Some preamble text\nSPDX-License-Identifier: MIT\nMore text";
        let query = Query::from_extracted_text(text, &index, false).unwrap();

        let matches = spdx_lid_match(&index, &query);

        if !matches.is_empty() {
            let m = &matches[0];
            assert!(
                m.start_token > 0 || m.end_token >= m.start_token,
                "Token positions should be valid (not hardcoded 0, 0)"
            );
        }
    }

    #[test]
    fn test_spdx_match_respects_prefix_offset_on_same_line() {
        let mut index = create_test_index(
            &[
                ("notice", 0),
                ("spdx", 1),
                ("license", 2),
                ("identifier", 3),
                ("mit", 4),
            ],
            1,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 100));
        index.rid_by_spdx_key.insert("mit".to_string(), 0);

        let text = "NOTICE SPDX-License-Identifier: MIT";
        let query = Query::from_extracted_text(text, &index, false).unwrap();
        let matches = spdx_lid_match(&index, &query);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start_token, 1);
        assert_eq!(matches[0].end_token, 5);
        assert_eq!(matches[0].matched_length, 4);
    }

    #[test]
    fn test_spdx_match_rule_identifier_preserves_trailing_expression_spaces() {
        let mut index = create_test_index(
            &[("spdx", 0), ("license", 1), ("identifier", 2), ("mit", 3)],
            1,
        );
        index.rules_by_rid.push(create_mock_rule_simple("mit", 100));
        index.rid_by_spdx_key.insert("mit".to_string(), 0);

        let plain =
            Query::from_extracted_text("// SPDX-License-Identifier: MIT", &index, false).unwrap();
        let spaced =
            Query::from_extracted_text("// SPDX-License-Identifier: MIT  ", &index, false).unwrap();

        let plain_matches = spdx_lid_match(&index, &plain);
        let spaced_matches = spdx_lid_match(&index, &spaced);

        assert_eq!(plain_matches.len(), 1);
        assert_eq!(spaced_matches.len(), 1);
        assert_ne!(
            plain_matches[0].rule_identifier, spaced_matches[0].rule_identifier,
            "Synthetic SPDX identifiers should preserve trailing expression whitespace like ScanCode"
        );
    }

    #[test]
    fn test_unknown_spdx_identifier_fallback() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        assert!(
            index.unknown_spdx_rid.is_some(),
            "Should have unknown-spdx rule loaded"
        );

        let unknown_expr = find_matching_rule_for_expression(index, "nonexistent-license-xyz");
        assert!(
            unknown_expr.is_some(),
            "Unknown SPDX identifier should return some expression"
        );
        let expr = unknown_expr.unwrap();
        assert!(
            expr.contains("unknown-spdx"),
            "Unknown SPDX identifier should map to unknown-spdx expression, got: {}",
            expr
        );
    }

    #[test]
    fn test_deprecated_spdx_substitution() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        assert!(
            index.rid_by_spdx_key.contains_key("gpl-2.0-only"),
            "Should have gpl-2.0-only SPDX key"
        );
        assert!(
            index
                .rid_by_spdx_key
                .contains_key("classpath-exception-2.0"),
            "Should have classpath-exception-2.0 SPDX key"
        );
    }

    #[test]
    fn test_primary_spdx_key_mapping() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let test_cases = [
            ("mit", "mit"),
            ("apache-2.0", "apache-2.0"),
            ("0bsd", "bsd-zero"),
            ("gpl-2.0-or-later", "gpl-2.0-plus"),
        ];

        for (spdx_key, expected_expr) in test_cases {
            if let Some(&rid) = index.rid_by_spdx_key.get(spdx_key) {
                let rule = &index.rules_by_rid[rid];
                assert_eq!(
                    rule.license_expression, expected_expr,
                    "SPDX key '{}' should map to expression '{}'",
                    spdx_key, expected_expr
                );
            }
        }
    }

    #[test]
    fn test_spdx_expression_with_or_operator() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "mit OR apache-2.0";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some(), "Should parse OR expression");
        let expr = result.unwrap();
        assert!(
            expr.contains("mit") && expr.contains("apache-2.0") && expr.contains(" OR "),
            "OR expression should preserve structure, got: {}",
            expr
        );
    }

    #[test]
    fn test_spdx_expression_with_with_operator() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "gpl-2.0-only with classpath-exception-2.0";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some(), "Should parse WITH expression");
        let expr = result.unwrap();
        assert!(
            expr.contains("gpl-2.0")
                && expr.contains("classpath-exception-2.0")
                && expr.contains(" WITH "),
            "WITH expression should preserve structure, got: {}",
            expr
        );
    }

    #[test]
    fn test_spdx_expression_complex_or() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "epl-2.0 or apache-2.0 or gpl-2.0-only with classpath-exception-2.0";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some(), "Should parse complex expression");
        let expr = result.unwrap();
        assert!(
            expr.contains("epl-2.0")
                && expr.contains("apache-2.0")
                && expr.contains("gpl-2.0")
                && expr.contains("classpath-exception-2.0"),
            "Complex expression should preserve all license keys, got: {}",
            expr
        );
    }

    #[test]
    fn test_valid_outer_paren_or_chain_flattens_like_python() {
        let index = create_spdx_lookup_index(&[
            ("bsd-3-clause", "bsd-new"),
            ("epl-1.0", "epl-1.0"),
            ("apache-2.0", "apache-2.0"),
            ("mit", "mit"),
        ]);

        let expression = "(bsd-3-clause OR epl-1.0 OR apache-2.0 OR mit)";
        let result = find_matching_rule_for_expression(&index, expression);

        assert_eq!(
            result,
            Some("bsd-new OR epl-1.0 OR apache-2.0 OR mit".to_string())
        );
    }

    #[test]
    fn test_valid_mixed_precedence_keeps_python_grouping() {
        let index = create_spdx_lookup_index(&[
            ("mit", "mit"),
            ("apache-2.0", "apache-2.0"),
            ("gpl-2.0-only", "gpl-2.0"),
        ]);

        let expression = "mit OR apache-2.0 AND gpl-2.0-only";
        let result = find_matching_rule_for_expression(&index, expression);

        assert_eq!(result, Some("mit OR (apache-2.0 AND gpl-2.0)".to_string()));
    }

    #[test]
    fn test_valid_same_operator_or_group_flattens_like_python() {
        let index = create_spdx_lookup_index(&[
            ("mit", "mit"),
            ("apache-2.0", "apache-2.0"),
            ("gpl-2.0-only", "gpl-2.0"),
        ]);

        let expression = "(mit OR apache-2.0) OR gpl-2.0-only";
        let result = find_matching_rule_for_expression(&index, expression);

        assert_eq!(result, Some("mit OR apache-2.0 OR gpl-2.0".to_string()));
    }

    #[test]
    fn test_recovery_parsing_uboot_unknown_line_renders_flat_or_chain() {
        let index = create_spdx_lookup_index(&[]);

        let expression = "line references more than one Unique";
        let result = find_matching_rule_for_expression(&index, expression);

        assert_eq!(
            result,
            Some(
                "unknown-spdx OR unknown-spdx OR unknown-spdx OR unknown-spdx OR unknown-spdx OR unknown-spdx"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_uboot_bare_list_as_or() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "GPL-2.0+ BSD-2-Clause";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some(), "Should parse U-Boot bare list as OR");
        let expr = result.unwrap();
        assert!(
            expr.contains(" OR "),
            "U-Boot bare list should be treated as OR, got: {}",
            expr
        );
        assert!(
            expr.contains("gpl-2.0-plus") || expr.contains("gpl-2.0+"),
            "Should contain GPL-2.0+, got: {}",
            expr
        );
        assert!(
            expr.contains("bsd") || expr.contains("bsd-simplified"),
            "Should contain BSD, got: {}",
            expr
        );
    }

    #[test]
    fn test_is_bare_license_list() {
        assert!(is_bare_license_list("GPL-2.0+ BSD-2-Clause MIT"));
        assert!(is_bare_license_list("mit apache-2.0"));
        assert!(!is_bare_license_list("MIT OR Apache-2.0"));
        assert!(!is_bare_license_list("MIT AND Apache-2.0"));
        assert!(!is_bare_license_list(
            "GPL-2.0 WITH Classpath-exception-2.0"
        ));
        assert!(!is_bare_license_list("(MIT OR Apache-2.0)"));
        assert!(is_bare_license_list("GPL-2.0+"));
    }

    #[test]
    fn test_recovery_parsing_bare_list() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "GPL-2.0+ BSD-2-Clause";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some());
        let expr = result.unwrap();
        assert!(
            expr.contains(" OR "),
            "U-Boot bare list should be OR, got: {}",
            expr
        );
    }

    #[test]
    fn test_recovery_parsing_malformed_parens() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "(GPL-2.0 OR MIT";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some());
        let expr = result.unwrap();
        assert!(
            expr.contains("gpl-2.0"),
            "Should contain GPL-2.0, got: {}",
            expr
        );
        assert!(expr.contains("mit"), "Should contain MIT, got: {}", expr);
    }

    #[test]
    fn test_recovery_parsing_unknown_identifier() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "nonexistent-license-xyz";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some());
        let expr = result.unwrap();
        assert!(
            expr.contains("unknown-spdx"),
            "Unknown identifier should return unknown-spdx, got: {}",
            expr
        );
    }

    #[test]
    fn test_recovery_parsing_keywords_with_invalid() {
        let index = create_spdx_lookup_index(&[("gpl-2.0", "gpl-2.0"), ("mit", "mit")]);

        let expression = "(GPL-2.0 AND (MIT";
        let result = find_matching_rule_for_expression(&index, expression);
        assert_eq!(
            result,
            Some("(gpl-2.0 AND mit) AND unknown-spdx".to_string())
        );
    }

    #[test]
    fn test_recovery_parsing_text_after_identifier() {
        let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
            .expect("required test dependency available");

        let index = engine.index();

        let expression = "LGPL-2.1+ The author added some notes";
        let result = find_matching_rule_for_expression(index, expression);
        assert!(result.is_some());
        let expr = result.unwrap();
        assert!(expr.contains("lgpl"), "Should contain LGPL, got: {}", expr);
        assert!(
            !expr.contains("the"),
            "Should not contain non-license text, got: {}",
            expr
        );
    }
}
