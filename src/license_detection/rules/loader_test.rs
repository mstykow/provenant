// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for rule/license file parsing.
//!
//! Tests for YAML frontmatter parsing edge cases including:
//! - PGP signatures in content
//! - Dashes in license text
//! - Various delimiter formats

use super::*;
use crate::license_detection::index::{loaded_license_to_license, loaded_rule_to_rule};
use crate::license_detection::models::{License, Rule};
use anyhow::Result;

fn parse_license_from_str(content: &str, filename: &str) -> Result<License> {
    let temp_path = std::env::temp_dir().join(filename);
    std::fs::write(&temp_path, content)?;
    let loaded = parse_license_to_loaded(&temp_path)?;
    let result = Ok(loaded_license_to_license(loaded));
    let _ = std::fs::remove_file(&temp_path);
    result
}

fn parse_rule_from_str(content: &str, filename: &str) -> Result<Rule> {
    let temp_path = std::env::temp_dir().join(filename);
    std::fs::write(&temp_path, content)?;
    let loaded = parse_rule_to_loaded(&temp_path)?;
    let result = loaded_rule_to_rule(loaded);
    let _ = std::fs::remove_file(&temp_path);
    Ok(result)
}

#[test]
fn test_parse_license_file_basic() {
    let content = r#"---
key: test-license
name: Test License
category: Permissive
---
This is the license text.
It has multiple lines."#;

    let result = parse_license_from_str(content, "test-license.LICENSE");
    assert!(result.is_ok(), "Should parse basic license: {:?}", result);
    let license = result.unwrap();
    assert_eq!(license.key, "test-license");
    assert_eq!(license.name, "Test License");
    assert!(license.text.contains("license text"));
}

#[test]
fn test_parse_rule_file_basic() {
    let content = r#"---
license_expression: mit
is_license_text: yes
---
Permission is hereby granted, free of charge."#;

    let result = parse_rule_from_str(content, "mit_1.RULE");
    assert!(result.is_ok(), "Should parse basic rule: {:?}", result);
    let rule = result.unwrap();
    assert_eq!(rule.license_expression, "mit");
    assert!(rule.text.contains("Permission"));
    assert!(rule.is_license_text());
}

#[test]
fn test_parse_license_empty_frontmatter() {
    let content = r#"---
---
This is the license text."#;

    let result = parse_license_from_str(content, "empty-fm.LICENSE");
    assert!(result.is_ok(), "Empty frontmatter should use defaults");
}

#[test]
fn test_parse_license_empty_text() {
    let content = r#"---
key: empty-text
name: Test License
---
"#;

    let result = parse_license_from_str(content, "empty-text.LICENSE");
    assert!(
        result.is_err(),
        "Empty text should fail for non-deprecated license"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("empty text content")
    );
}

#[test]
fn test_parse_license_empty_text_deprecated_allowed() {
    let content = r#"---
key: deprecated
name: Test License
is_deprecated: yes
---
"#;

    let result = parse_license_from_str(content, "deprecated.LICENSE");
    assert!(result.is_ok(), "Deprecated license can have empty text");
}

#[test]
fn test_parse_license_empty_text_unknown_allowed() {
    let content = r#"---
key: unknown
name: Test License
is_unknown: yes
---
"#;

    let result = parse_license_from_str(content, "unknown.LICENSE");
    assert!(result.is_ok(), "Unknown license can have empty text");
}

#[test]
fn test_parse_license_empty_text_generic_allowed() {
    let content = r#"---
key: generic
name: Test License
is_generic: yes
---
"#;

    let result = parse_license_from_str(content, "generic.LICENSE");
    assert!(result.is_ok(), "Generic license can have empty text");
}

#[test]
fn test_parse_rule_empty_text() {
    let content = r#"---
license_expression: mit
---
"#;

    let result = parse_rule_from_str(content, "empty-text.RULE");
    assert!(result.is_err(), "Rule with empty text should fail");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("empty text content")
    );
}

#[test]
fn test_parse_license_with_pgp_signature() {
    let content = r#"---
key: pgp-license
name: Test License
---
-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA256

This is the license text with a PGP signature.
-----END PGP SIGNATURE-----"#;

    let result = parse_license_from_str(content, "pgp-license.LICENSE");
    assert!(
        result.is_ok(),
        "Should parse license with PGP signature: {:?}",
        result
    );
    let license = result.unwrap();
    assert!(
        license.text.contains("PGP SIGNED MESSAGE"),
        "Text should contain PGP marker"
    );
    assert!(
        license.text.contains("license text"),
        "Text should contain actual license"
    );
}

#[test]
fn test_parse_license_with_dashes_in_content() {
    let content = r#"---
key: dashes-content
name: Test License
---
The Artistic License 1.0
--- 
This is a separator in the license text.
More text here.
--- end ---"#;

    let result = parse_license_from_str(content, "dashes-content.LICENSE");
    assert!(
        result.is_ok(),
        "Should parse license with dashes in content: {:?}",
        result
    );
    let license = result.unwrap();
    assert!(
        license.text.contains("separator"),
        "Text should contain content after dashes"
    );
    assert!(
        license.text.contains("--- end ---"),
        "Text should contain inline dashes"
    );
}

#[test]
fn test_parse_license_with_four_dash_delimiter() {
    let content = r#"----
key: four-dash
name: Test License
----
This is the license text."#;

    let result = parse_license_from_str(content, "four-dash.LICENSE");
    assert!(
        result.is_ok(),
        "Should parse license with 4-dash delimiter: {:?}",
        result
    );
    let license = result.unwrap();
    assert!(
        license.text.contains("license text"),
        "Text should be extracted correctly"
    );
}

#[test]
fn test_parse_rule_with_pgp_signature() {
    let content = r#"---
license_expression: test-rule
is_license_text: yes
---
-----BEGIN PGP SIGNED MESSAGE-----

Rule text here.
-----END PGP SIGNATURE-----"#;

    let result = parse_rule_from_str(content, "pgp-rule.RULE");
    assert!(
        result.is_ok(),
        "Should parse rule with PGP signature: {:?}",
        result
    );
    let rule = result.unwrap();
    assert!(
        rule.text.contains("PGP SIGNED MESSAGE"),
        "Text should contain PGP marker"
    );
}

#[test]
fn test_parse_rule_false_positive() {
    let content = r#"---
license_expression: gpl-2.0-plus
is_false_positive: yes
---
GPL"#;

    let result = parse_rule_from_str(content, "false-positive.RULE");
    assert!(result.is_ok(), "Should parse false positive rule");
    let rule = result.unwrap();
    assert!(rule.is_false_positive);
}

#[test]
fn test_parse_license_minimum_coverage() {
    let content = r#"---
key: min-coverage
name: Test License
minimum_coverage: 80
---
License text here."#;

    let result = parse_license_from_str(content, "min-coverage.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert_eq!(license.minimum_coverage, Some(80));
}

#[test]
fn test_parse_rule_minimum_coverage() {
    let content = r#"---
license_expression: test
is_license_notice: yes
minimum_coverage: 99
---
Rule text."#;

    let result = parse_rule_from_str(content, "min-coverage.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert_eq!(rule.minimum_coverage, Some(99));
    assert!(rule.has_stored_minimum_coverage);
}

#[test]
fn test_parse_rule_without_minimum_coverage_has_no_stored_provenance() {
    let content = r#"---
license_expression: test
is_license_notice: yes
---
Rule text."#;

    let result = parse_rule_from_str(content, "no-min-coverage.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert_eq!(rule.minimum_coverage, None);
    assert!(!rule.has_stored_minimum_coverage);
}

#[test]
fn test_parse_license_no_key_in_frontmatter() {
    let content = r#"---
name: Test License
---
Text."#;

    let result = parse_license_from_str(content, "no-key.LICENSE");
    assert!(result.is_ok(), "Missing key should use filename");
    let license = result.unwrap();
    assert_eq!(license.key, "no-key");
}

#[test]
fn test_parse_rule_with_all_boolean_flags() {
    let content = r#"---
license_expression: test
is_license_text: yes
is_license_notice: yes
is_license_reference: yes
is_license_tag: yes
is_license_intro: yes
is_license_clue: yes
is_false_positive: yes
is_continuous: yes
---
Text."#;

    let result = parse_rule_from_str(content, "flags.RULE");
    assert!(result.is_err());
}

#[test]
fn test_parse_license_with_multiline_yaml() {
    let content = r#"---
key: multiline
name: Test License
notes: |
    Line 1
    Line 2
    Line 3
---
Text."#;

    let result = parse_license_from_str(content, "multiline.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert!(license.notes.as_ref().unwrap().contains("Line 1"));
    assert!(license.notes.as_ref().unwrap().contains("Line 2"));
}

#[test]
fn test_parse_license_with_trailing_whitespace_on_delimiter() {
    let content = "---  \nkey: ws-delimiter\nname: Test\n---  \nText.";

    let result = parse_license_from_str(content, "ws-delimiter.LICENSE");
    assert!(
        result.is_ok(),
        "Should handle trailing whitespace on delimiter"
    );
    let license = result.unwrap();
    assert!(license.text.contains("Text"));
}

#[test]
fn test_parse_license_key_mismatch() {
    let content = r#"---
key: wrong-key
name: Test License
---
Text."#;

    let result = parse_license_from_str(content, "correct-key.LICENSE");
    assert!(result.is_err(), "Key mismatch should fail");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("key mismatch"),
        "Error should mention key mismatch: {}",
        err
    );
}

#[test]
fn test_parse_rule_missing_license_expression() {
    let content = r#"---
is_license_text: yes
---
Some text."#;

    let result = parse_rule_from_str(content, "no-expr.RULE");
    assert!(
        result.is_err(),
        "Missing license_expression should fail for non-false-positive"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("license_expression"),
        "Error should mention license_expression: {}",
        err
    );
}

#[test]
fn test_parse_rule_false_positive_without_expression() {
    let content = r#"---
is_false_positive: yes
---
Some text."#;

    let result = parse_rule_from_str(content, "fp-no-expr.RULE");
    assert!(
        result.is_ok(),
        "False positive rule can omit license_expression"
    );
    let rule = result.unwrap();
    assert!(rule.is_false_positive);
    assert_eq!(rule.license_expression, "unknown");
}

#[test]
fn test_parse_license_content_too_short() {
    let content = "abc";

    let result = parse_license_from_str(content, "short.LICENSE");
    assert!(result.is_err(), "Content too short should fail");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("too short"),
        "Error should mention too short: {}",
        err
    );
}

#[test]
fn test_parse_rule_content_too_short() {
    let content = "abc";

    let result = parse_rule_from_str(content, "short.RULE");
    assert!(result.is_err(), "Content too short should fail");
}

#[test]
fn test_parse_license_missing_delimiter() {
    let content = r#"key: no-delim
name: Test
This is text without delimiters."#;

    let result = parse_license_from_str(content, "no-delim.LICENSE");
    assert!(result.is_err(), "Missing delimiter should fail");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("delimiter"),
        "Error should mention delimiter: {}",
        err
    );
}

#[test]
fn test_parse_rule_missing_delimiter() {
    let content = r#"license_expression: mit
This is text without delimiters."#;

    let result = parse_rule_from_str(content, "no-delim.RULE");
    assert!(result.is_err(), "Missing delimiter should fail");
}

#[test]
fn test_parse_license_with_ignorable_fields() {
    let content = r#"---
key: ignorable
name: Test License
ignorable_copyrights:
    - Copyright (c) Example
ignorable_holders:
    - Example Corp
ignorable_authors:
    - John Doe
ignorable_urls:
    - http://example.com
ignorable_emails:
    - test@example.com
---
License text."#;

    let result = parse_license_from_str(content, "ignorable.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert!(license.ignorable_copyrights.is_some());
    assert!(license.ignorable_holders.is_some());
    assert!(license.ignorable_authors.is_some());
    assert!(license.ignorable_urls.is_some());
    assert!(license.ignorable_emails.is_some());
}

#[test]
fn test_parse_rule_with_ignorable_fields() {
    let content = r#"---
license_expression: test
is_license_notice: yes
ignorable_urls:
    - http://example.com
ignorable_emails:
    - test@example.com
ignorable_copyrights:
    - Copyright (c) Example
ignorable_holders:
    - Example Corp
ignorable_authors:
    - John Doe
---
Rule text."#;

    let result = parse_rule_from_str(content, "ignorable.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert!(rule.ignorable_urls.is_some());
    assert!(rule.ignorable_emails.is_some());
    assert!(rule.ignorable_copyrights.is_some());
    assert!(rule.ignorable_holders.is_some());
    assert!(rule.ignorable_authors.is_some());
}

#[test]
fn test_parse_rule_with_referenced_filenames() {
    let content = r#"---
license_expression: mit
is_license_reference: yes
referenced_filenames:
    - MIT.txt
    - LICENSE
---
MIT License"#;

    let result = parse_rule_from_str(content, "ref-files.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert!(rule.referenced_filenames.is_some());
    let refs = rule.referenced_filenames.unwrap();
    assert_eq!(refs.len(), 2);
    assert!(refs.contains(&"MIT.txt".to_string()));
}

#[test]
fn test_parse_rule_relevance_field() {
    let content = r#"---
license_expression: mit
is_license_notice: yes
relevance: 85
---
MIT License"#;

    let result = parse_rule_from_str(content, "relevance.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert_eq!(rule.relevance, 85);
}

#[test]
fn test_parse_rule_relevance_default() {
    let content = r#"---
license_expression: mit
is_license_notice: yes
---
MIT License"#;

    let result = parse_rule_from_str(content, "relevance-default.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert_eq!(rule.relevance, 100);
}

#[test]
fn test_parse_license_with_urls() {
    let content = r#"---
key: url-test
short_name: URL Test
name: Test License
owner: Example Org
homepage_url: http://example.com
osi_license_key: URL-TEST
text_urls:
    - http://text.example.com
osi_url: http://osi.example.com
faq_url: http://faq.example.com
other_urls:
    - http://other.example.com
standard_notice: Include this notice.
---
License text."#;

    let result = parse_license_from_str(content, "url-test.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert!(!license.reference_urls.is_empty());
    assert_eq!(license.short_name.as_deref(), Some("URL Test"));
    assert_eq!(license.language.as_deref(), Some("en"));
    assert_eq!(license.owner.as_deref(), Some("Example Org"));
    assert_eq!(license.homepage_url.as_deref(), Some("http://example.com"));
    assert_eq!(license.osi_license_key.as_deref(), Some("URL-TEST"));
    assert_eq!(
        license.text_urls,
        vec!["http://text.example.com".to_string()]
    );
    assert_eq!(license.osi_url.as_deref(), Some("http://osi.example.com"));
    assert_eq!(license.faq_url.as_deref(), Some("http://faq.example.com"));
    assert_eq!(
        license.other_urls,
        vec!["http://other.example.com".to_string()]
    );
    assert_eq!(
        license.standard_notice.as_deref(),
        Some("Include this notice.")
    );
}

#[test]
fn test_parse_license_replaced_by() {
    let content = r#"---
key: deprecated-replaced
name: Deprecated License
is_deprecated: yes
replaced_by:
    - mit
    - apache-2.0
---
"#;

    let result = parse_license_from_str(content, "deprecated-replaced.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert!(license.is_deprecated);
    assert_eq!(license.replaced_by.len(), 2);
}

#[test]
fn test_parse_rule_notes_field() {
    let content = r#"---
license_expression: test
is_license_notice: yes
notes: This is a test note.
---
Rule text."#;

    let result = parse_rule_from_str(content, "notes.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert!(rule.notes.is_some());
    assert!(rule.notes.unwrap().contains("test note"));
}

#[test]
fn test_parse_rule_language_field() {
    let content = r#"---
license_expression: test
is_license_notice: yes
language: en
---
Rule text."#;

    let result = parse_rule_from_str(content, "language.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert_eq!(rule.language, Some("en".to_string()));
}

#[test]
fn test_parse_license_uses_short_name_as_name_fallback() {
    let content = r#"---
key: short-name-test
short_name: Short Name
---
License text."#;

    let result = parse_license_from_str(content, "short-name-test.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert_eq!(license.name, "Short Name");
}

#[test]
fn test_parse_license_name_fallback_to_key() {
    let content = r#"---
key: key-as-name
---
License text."#;

    let result = parse_license_from_str(content, "key-as-name.LICENSE");
    assert!(result.is_ok());
    let license = result.unwrap();
    assert_eq!(license.name, "key-as-name");
}

#[test]
fn test_parse_bool_variants() {
    let content = r#"---
license_expression: test
is_license_reference: "1"
is_license_notice: "no"
is_license_text: false
is_license_tag: false
is_license_intro: "no"
is_license_clue: "0"
---
Text."#;

    let result = parse_rule_from_str(content, "bool-variants.RULE");
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert!(rule.is_license_reference());
    assert!(!rule.is_license_notice());
    assert!(!rule.is_license_text());
    assert!(!rule.is_license_tag());
    assert!(!rule.is_license_intro());
    assert!(!rule.is_license_clue());
}

#[test]
fn test_parse_rule_preserves_required_phrase_generation_metadata() {
    let content = r#"---
license_expression: mit
is_license_notice: yes
skip_for_required_phrase_generation: yes
replaced_by:
  - apache-2.0
---
MIT License"#;

    let temp_path = std::env::temp_dir().join("metadata.RULE");
    std::fs::write(&temp_path, content).unwrap();

    let result = parse_rule_to_loaded(&temp_path);
    let _ = std::fs::remove_file(&temp_path);
    assert!(result.is_ok());
    let rule = result.unwrap();
    assert!(rule.skip_for_required_phrase_generation);
    assert_eq!(rule.replaced_by, vec!["apache-2.0".to_string()]);
}

#[test]
fn test_ibmpl_rule_loaded() {
    let engine = crate::license_detection::LicenseDetectionEngine::from_embedded()
        .expect("required test dependency available");

    let index = engine.index();

    let ibmpl_1 = index
        .rules_by_rid
        .iter()
        .find(|r| r.identifier == "ibmpl-1.0_1.RULE");
    assert!(ibmpl_1.is_some(), "ibmpl-1.0_1.RULE should be loaded");

    let rule = ibmpl_1.unwrap();
    assert_eq!(rule.license_expression, "ibmpl-1.0");
    assert!(rule.is_license_reference());
    assert_eq!(rule.relevance, 100);

    let expected_text = "distributed under the IBM Public License (IPL).";
    assert_eq!(rule.text, expected_text, "Rule text should match");
}

#[test]
fn test_ibmpl_rule_tokens() {
    use crate::license_detection::tokenize::tokenize;

    let rule_text = "distributed under the IBM Public License (IPL).";
    let query_text = "Version 0.7.0 and above will be distributed under the IBM Public\nLicense (IPL). The IPL is an approved open source license";

    let rule_tokens = tokenize(rule_text);
    let query_tokens = tokenize(query_text);

    eprintln!("Rule tokens: {:?}", rule_tokens);
    eprintln!("Query tokens: {:?}", query_tokens);

    let rule_len = rule_tokens.len();
    let mut found = false;
    for i in 0..=query_tokens.len().saturating_sub(rule_len) {
        if query_tokens[i..i + rule_len] == rule_tokens[..] {
            eprintln!("MATCH FOUND at position {}", i);
            eprintln!("Query segment: {:?}", &query_tokens[i..i + rule_len]);
            found = true;
            break;
        }
    }

    assert!(found, "Rule tokens should appear in query tokens");
}

#[test]
fn test_ibmpl_detection() {
    use crate::license_detection::LicenseDetectionEngine;

    let engine =
        LicenseDetectionEngine::from_embedded().expect("required test dependency available");

    // Test with exact rule text
    let exact_text = "distributed under the IBM Public License (IPL).";
    let detections = engine
        .detect_with_kind(exact_text, false, false)
        .expect("Detection failed");

    eprintln!("Exact text match:");
    for d in &detections {
        let expr = d.license_expression.as_deref().unwrap_or("none");
        let coverage = d.matches.first().map(|m| m.match_coverage).unwrap_or(0.0);
        eprintln!("  {} (coverage: {:.1}%)", expr, coverage);
    }

    // Test with the actual test file text (split across lines)
    let test_text = "Version 0.7.0 and above will be distributed under the IBM Public\nLicense (IPL). The IPL is an approved open source license";
    let detections = engine
        .detect_with_kind(test_text, false, false)
        .expect("Detection failed");

    eprintln!("\nTest file text match:");
    for d in &detections {
        let expr = d.license_expression.as_deref().unwrap_or("none");
        let coverage = d.matches.first().map(|m| m.match_coverage).unwrap_or(0.0);
        eprintln!("  {} (coverage: {:.1}%)", expr, coverage);
    }

    let has_ibmpl_match = detections.iter().any(|d| {
        d.matches
            .iter()
            .any(|m| m.license_expression.contains("ibmpl-1.0"))
    });
    assert!(
        has_ibmpl_match,
        "Should preserve the ibmpl-1.0 match in split text"
    );
    assert!(
        detections.iter().all(|d| d.license_expression.is_none()),
        "Single low-quality fragment should remain expressionless"
    );
}
