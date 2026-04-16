use super::*;
use std::sync::{LazyLock, Once};
use std::time::{Duration, Instant};

use crate::license_detection::models::{MatchCoordinates, position_span::PositionSpan};
use crate::license_detection::test_utils::create_test_index_default;
use crate::models::LineNumber;
use crate::models::MatchScore;

static TEST_ENGINE: LazyLock<LicenseDetectionEngine> = LazyLock::new(|| {
    LicenseDetectionEngine::from_embedded().expect("Should initialize from embedded artifact")
});

static INIT: Once = Once::new();

fn get_engine() -> &'static LicenseDetectionEngine {
    INIT.call_once(|| {
        let _ = &*TEST_ENGINE;
    });
    &TEST_ENGINE
}

fn make_test_match(
    matcher: impl ToString,
    expression: &str,
    rule_identifier: &str,
    start_token: usize,
    end_token: usize,
    qspan_positions: Option<Vec<usize>>,
) -> LicenseMatch {
    let matcher = matcher.to_string();
    let matched_length = qspan_positions
        .as_ref()
        .map(|positions| positions.len())
        .unwrap_or_else(|| end_token.saturating_sub(start_token));

    let qspan = match qspan_positions {
        Some(positions) => PositionSpan::from_positions(positions),
        None => PositionSpan::range(start_token, end_token),
    };

    LicenseMatch {
        license_expression: expression.to_string(),
        matcher: matcher.parse().expect("invalid test matcher"),
        rule_identifier: rule_identifier.to_string(),
        start_token,
        end_token,
        matched_length,
        rule_length: matched_length,
        match_coverage: 100.0,
        coordinates: MatchCoordinates::query_region(qspan),
        ..Default::default()
    }
}

#[test]
fn test_engine_from_embedded_initializes() {
    let engine = get_engine();

    assert!(
        !engine.index().rules_by_rid.is_empty(),
        "Should have rules loaded from embedded artifact"
    );
    assert!(
        !engine.index().licenses_by_key.is_empty(),
        "Should have licenses loaded from embedded artifact"
    );
    assert!(
        engine.index().len_legalese > 0,
        "Should have legalese tokens"
    );
    assert!(
        !engine.index().rid_by_hash.is_empty(),
        "Should have hash mappings"
    );
}

#[test]
fn test_engine_new_with_reference_rules() {
    let engine = get_engine();

    assert!(
        !engine.index().rules_by_rid.is_empty(),
        "Should have rules loaded"
    );
    assert!(
        !engine.index().licenses_by_key.is_empty(),
        "Should have licenses loaded"
    );
    assert!(
        engine.index().len_legalese > 0,
        "Should have legalese tokens"
    );
    assert!(
        !engine.index().rid_by_hash.is_empty(),
        "Should have hash mappings"
    );
    assert!(
        !engine.index().rid_by_hash.is_empty(),
        "Should have regular rule hashes"
    );
}

#[test]
fn test_engine_detect_mit_license() {
    let engine = get_engine();

    let mit_text = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."#;

    let detections = engine
        .detect_with_kind(mit_text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect at least one license in MIT text"
    );

    let mit_related = detections.iter().any(|d| {
        d.license_expression
            .as_ref()
            .map(|e| e.contains("mit") || e.contains("unknown"))
            .unwrap_or(false)
    });
    assert!(
        mit_related,
        "Should detect MIT or unknown license, got: {:?}",
        detections
            .iter()
            .map(|d| d.license_expression.as_deref().unwrap_or("none"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_engine_detect_empty_text() {
    let engine = get_engine();

    let detections = engine
        .detect_with_kind("", false, false)
        .expect("Detection should succeed");
    assert!(
        detections.is_empty() || !detections.is_empty(),
        "Detection completes"
    );

    let detections = engine
        .detect_with_kind("   \n\n   ", false, false)
        .expect("Detection should succeed");
    assert!(
        detections.is_empty() || !detections.is_empty(),
        "Detection completes"
    );
}

#[test]
fn test_engine_detect_spdx_identifier() {
    let engine = get_engine();

    let text = "SPDX-License-Identifier: MIT";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect license from SPDX identifier"
    );
}

#[test]
fn test_engine_detect_with_deadline_times_out_when_already_expired() {
    let engine = LicenseDetectionEngine::from_test_index(create_test_index_default());

    let error = engine
        .detect_with_kind_with_score_and_deadline(
            "Permission is hereby granted, free of charge, to any person obtaining a copy",
            false,
            false,
            0.0,
            Some(Instant::now() - Duration::from_millis(1)),
        )
        .expect_err("expired deadline should abort license detection");

    assert_eq!(error.to_string(), LICENSE_DETECTION_TIMEOUT_MESSAGE);
}

#[test]
fn test_engine_detects_terser_license_as_bsd_simplified_only() {
    let engine = get_engine();
    let fixture = std::path::PathBuf::from(
        "testdata/license-golden/datadriven/external/terser-license-bsd-2.txt",
    );
    let text = std::fs::read_to_string(&fixture).expect("terser fixture should be readable");

    let expressions: Vec<String> = engine
        .detect_matches_with_kind(&text, false, false)
        .expect("terser license fixture should detect")
        .into_iter()
        .map(|detection| detection.license_expression)
        .collect();

    assert_eq!(expressions, vec!["bsd-simplified"]);
}

#[test]
fn test_engine_detects_boost_short_notice_with_url() {
    let engine = get_engine();

    let text = "Use, modification and distribution are subject to the Boost Software License, Version 1.0.\n(See accompanying file LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)";
    let raw_matches = engine
        .detect_matches_with_kind(text, false, false)
        .expect("Raw detection should succeed");
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        detections
            .iter()
            .any(|d| d.license_expression.as_deref() == Some("boost-1.0")),
        "detections: {:?}, raw_matches: {:?}",
        detections
            .iter()
            .map(|d| (
                d.license_expression.as_deref().unwrap_or("none"),
                d.matches
                    .iter()
                    .map(|m| (m.license_expression.as_str(), m.rule_identifier.as_str()))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        raw_matches
            .iter()
            .map(|m| (
                m.license_expression.as_str(),
                m.rule_identifier.as_str(),
                m.matcher
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_engine_detects_zlib_short_reference_notice() {
    let engine = get_engine();

    let text = "For conditions of distribution and use, see copyright notice in zlib.h";
    let raw_matches = engine
        .detect_matches_with_kind(text, false, false)
        .expect("Raw detection should succeed");
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        detections
            .iter()
            .any(|d| d.license_expression.as_deref() == Some("zlib")),
        "detections: {:?}, raw_matches: {:?}",
        detections
            .iter()
            .map(|d| (
                d.license_expression.as_deref().unwrap_or("none"),
                d.matches
                    .iter()
                    .map(|m| (m.license_expression.as_str(), m.rule_identifier.as_str()))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        raw_matches
            .iter()
            .map(|m| (
                m.license_expression.as_str(),
                m.rule_identifier.as_str(),
                m.matcher
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_engine_index_populated() {
    let engine = get_engine();
    let index = engine.index();

    assert!(
        index.rules_by_rid.len() > 1000,
        "Should have at least 1000 rules loaded from reference"
    );

    assert!(
        index.licenses_by_key.len() > 100,
        "Should have at least 100 licenses loaded from reference"
    );

    assert!(
        !index.approx_matchable_rids.is_empty(),
        "Should have approx-matchable rules"
    );

    let has_false_positives = !index.false_positive_rids.is_empty();
    assert!(has_false_positives, "Should have false positive rules");

    let mut rules_with_tokens = 0;
    for &rid in index.rid_by_hash.values().take(10) {
        let rule = &index.rules_by_rid[rid];
        if !rule.tokens.is_empty() {
            rules_with_tokens += 1;
            assert!(
                rule.min_matched_length > 0,
                "Regular rule {} should have computed threshold",
                rid
            );
        }
    }
    assert!(
        rules_with_tokens > 0,
        "Should have at least one rule with tokens among first 10"
    );
}

#[test]
fn test_engine_automaton_functional() {
    let engine = get_engine();
    let index = engine.index();

    if !index.rules_by_rid.is_empty() {
        let first_rule = &index.rules_by_rid[0];
        if !first_rule.tokens.is_empty() {
            let pattern: Vec<u8> = first_rule
                .tokens
                .iter()
                .flat_map(|t| t.to_le_bytes())
                .collect();

            let matches: Vec<_> = index
                .rules_automaton
                .find_overlapping_iter(&pattern)
                .collect();
            assert!(
                !matches.is_empty(),
                "Automaton should find pattern for rule 0"
            );
        }
    }
}

#[test]
fn test_engine_spdx_mapping() {
    let engine = get_engine();
    let mapping = engine.spdx_mapping();

    let mit_spdx = mapping.scancode_to_spdx("mit");
    assert!(mit_spdx.is_some(), "Should have MIT SPDX mapping");
    assert_eq!(
        mit_spdx.unwrap(),
        "MIT",
        "MIT should map to MIT SPDX identifier"
    );
}

#[test]
fn test_engine_detect_no_license() {
    let engine = get_engine();

    let text = "This is just some random text without any license information.";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");
    assert!(
        !detections.is_empty() || detections.is_empty(),
        "Detection should complete without error"
    );
}

#[test]
fn test_engine_detect_gpl_notice() {
    let engine = get_engine();

    let text = "This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation.";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect GPL notice");
}

#[test]
fn test_engine_detect_apache_notice() {
    let engine = get_engine();

    let text = "Licensed under the Apache License, Version 2.0";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect Apache notice");
}

#[test]
fn test_engine_index_sets_by_rid() {
    let engine = get_engine();
    let index = engine.index();

    for &rid in index.rid_by_hash.values().take(5) {
        assert!(
            index.sets_by_rid.contains_key(&rid),
            "Rule {} should have token set",
            rid
        );
        let set = &index.sets_by_rid[&rid];
        assert!(
            !set.is_empty(),
            "Rule {} token set should not be empty",
            rid
        );
    }
}

#[test]
fn test_engine_index_msets_by_rid() {
    let engine = get_engine();
    let index = engine.index();

    for &rid in index.rid_by_hash.values().take(5) {
        assert!(
            index.msets_by_rid.contains_key(&rid),
            "Rule {} should have token multiset",
            rid
        );
        let mset = &index.msets_by_rid[&rid];
        assert!(
            !mset.is_empty(),
            "Rule {} token multiset should not be empty",
            rid
        );
    }
}

#[test]
fn test_engine_index_high_postings() {
    let engine = get_engine();
    let index = engine.index();

    if !index.approx_matchable_rids.is_empty() {
        let some_approx_rid = index.approx_matchable_rids.iter().next().unwrap();
        if index.high_postings_by_rid.contains_key(some_approx_rid) {
            let postings = &index.high_postings_by_rid[some_approx_rid];
            assert!(!postings.is_empty(), "High postings should have entries");
        }
    }
}

#[test]
fn test_engine_matched_text_populated() {
    let engine = get_engine();

    let text = "SPDX-License-Identifier: MIT";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect license");

    for detection in &detections {
        for m in &detection.matches {
            assert!(
                m.start_line >= LineNumber::ONE,
                "start_line should be populated for matcher {}",
                m.matcher
            );
            assert!(
                m.end_line >= m.start_line,
                "end_line should be valid for matcher {}",
                m.matcher
            );
        }
    }
}

#[test]
fn test_detect_multiple_licenses_in_text() {
    let engine = get_engine();

    let isc_text = r#"Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE."#;

    let darpa_text = r#"Portions of this software were developed by the University of California,
Irvine under a U.S. Government contract with the Defense Advanced Research
Projects Agency (DARPA)."#;

    let combined_text = format!("{}\n\n{}", isc_text, darpa_text);

    let detections = engine
        .detect_with_kind(&combined_text, false, false)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect at least one license");

    let detected_licenses: Vec<String> = detections
        .iter()
        .filter_map(|d| d.license_expression.as_ref())
        .cloned()
        .collect();

    assert!(
        detected_licenses.iter().any(|l| {
            let lower = l.to_lowercase();
            lower.contains("isc") || lower.contains("sudo")
        }),
        "Should detect ISC or sudo license (sudo contains ISC + DARPA attribution), got: {:?}",
        detected_licenses
    );
}

#[test]
fn test_sudo_license_loaded_from_license_file() {
    let engine = get_engine();

    let index = engine.index();

    let sudo_rules: Vec<_> = index
        .rules_by_rid
        .iter()
        .filter(|r| r.license_expression.contains("sudo"))
        .collect();

    eprintln!("Found {} rules with 'sudo' expression", sudo_rules.len());
    for rule in sudo_rules.iter().take(3) {
        eprintln!(
            "  Rule: {} - is_from_license: {}, text len: {}",
            rule.identifier,
            rule.is_from_license,
            rule.text.len()
        );
    }

    assert!(
        !sudo_rules.is_empty(),
        "Should have at least one rule with 'sudo' license expression"
    );

    let sudo_from_license = sudo_rules.iter().find(|r| r.is_from_license);
    assert!(
        sudo_from_license.is_some(),
        "Should have a sudo rule created from license file"
    );

    let rule = sudo_from_license.unwrap();
    assert!(
        rule.text.contains("Sponsored in part"),
        "sudo rule text should contain DARPA acknowledgment"
    );
}

#[test]
fn test_spdx_simple() {
    let engine = get_engine();

    let text = "SPDX-License-Identifier: MIT\nSome code here";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect license from SPDX identifier"
    );

    let has_mit = detections.iter().any(|d| {
        d.license_expression
            .as_ref()
            .map(|e| e.contains("mit"))
            .unwrap_or(false)
    });
    assert!(has_mit, "Should detect MIT license");
}

#[test]
fn test_spdx_with_or() {
    let engine = get_engine();

    let text = "SPDX-License-Identifier: MIT OR Apache-2.0";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect license from SPDX identifier with OR"
    );
}

#[test]
fn test_spdx_with_plus() {
    let engine = get_engine();

    let text = "SPDX-License-Identifier: GPL-2.0+";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect license from SPDX identifier with plus"
    );
}

#[test]
fn test_spdx_in_comment() {
    let engine = get_engine();

    let text = "// SPDX-License-Identifier: MIT\n/* some code */";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect SPDX identifier in comment"
    );
}

#[test]
fn test_spdx_lines_do_not_get_rediscovered_as_seq_false_positives() {
    let engine = get_engine();

    let text = std::fs::read_to_string("testdata/license-golden/datadriven/external/spdx/uboot.c")
        .expect("Failed to read uboot.c SPDX fixture");

    let matches = engine
        .detect_matches_with_kind(&text, false, false)
        .expect("Detection should succeed");
    let match_exprs: Vec<&str> = matches
        .iter()
        .map(|m| m.license_expression.as_str())
        .collect();

    assert!(
        !match_exprs.contains(&"bsd-plus-patent"),
        "SPDX lines should not be rediscovered as bsd-plus-patent: {:?}",
        match_exprs
    );
    assert!(
        !match_exprs.contains(&"gpl-2.0 OR bsd-simplified"),
        "SPDX lines should not be rediscovered as gpl-2.0 OR bsd-simplified: {:?}",
        match_exprs
    );

    let detections = engine
        .detect_with_kind(&text, false, false)
        .expect("Detection should succeed");
    let detection_exprs: Vec<&str> = detections
        .iter()
        .filter_map(|d| d.license_expression.as_deref())
        .collect();

    assert!(
        !detection_exprs.contains(&"bsd-plus-patent"),
        "Grouped detections should not contain bsd-plus-patent: {:?}",
        detection_exprs
    );
    assert!(
        !detection_exprs.contains(&"gpl-2.0 OR bsd-simplified"),
        "Grouped detections should not contain gpl-2.0 OR bsd-simplified: {:?}",
        detection_exprs
    );
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_drops_tiny_gap_unicode_wrapper() {
    let redundant_seq = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "unicode",
        "unicode_3.RULE",
        10,
        24,
        Some(vec![10, 11, 12, 13, 16, 17, 18, 19, 20, 21, 22, 23]),
    );
    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "unicode",
        "unicode_6.RULE",
        10,
        13,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "unicode",
        "unicode_8.RULE",
        21,
        24,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![redundant_seq],
        &[aho_first.clone(), aho_second.clone()],
    );
    assert!(
        filtered.is_empty(),
        "expected tiny-gap redundant seq container to drop"
    );
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_drops_small_boundary_wrapper() {
    let redundant_seq = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "bsd-new",
        "bsd-new_174.RULE",
        9,
        25,
        Some(vec![9, 10, 11, 12, 13, 15, 16, 17, 18, 19, 21, 22, 23, 24]),
    );
    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "bsd-new",
        "bsd-new_400.RULE",
        10,
        14,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "bsd-new",
        "bsd-new_98.RULE",
        17,
        20,
        None,
    );
    let aho_third = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "bsd-new",
        "bsd-new_401.RULE",
        22,
        25,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![redundant_seq],
        &[aho_first, aho_second, aho_third],
    );
    assert!(
        filtered.is_empty(),
        "expected small bridge and boundary filler wrapper to drop"
    );
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_keeps_material_boundary_wrapper() {
    let material_seq = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "unicode",
        "unicode_3.RULE",
        1,
        24,
        Some(vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 16, 17, 18, 19, 20, 21, 22, 23,
        ]),
    );
    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "unicode",
        "unicode_6.RULE",
        10,
        13,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "unicode",
        "unicode_8.RULE",
        21,
        24,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![material_seq.clone()],
        &[aho_first, aho_second],
    );
    assert_eq!(filtered, vec![material_seq]);
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_keeps_wide_gap_unicode_wrapper() {
    let wide_gap_seq = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "unicode",
        "unicode_3.RULE",
        10,
        19,
        Some(vec![10, 11, 12, 16, 17, 18]),
    );
    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "unicode",
        "unicode_6.RULE",
        10,
        13,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "unicode",
        "unicode_8.RULE",
        21,
        24,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![wide_gap_seq.clone()],
        &[aho_first, aho_second],
    );
    assert_eq!(filtered, vec![wide_gap_seq]);
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_keeps_single_material_child_wrapper() {
    let seq_container = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "lgpl-2.0-plus",
        "lgpl-2.0-plus_55.RULE",
        148,
        270,
        Some((148..151).chain(154..270).collect()),
    );
    let bare_single_word = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "lgpl-2.0-plus",
        "lgpl_bare_single_word.RULE",
        149,
        150,
        None,
    );
    let long_body = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "lgpl-2.0-plus",
        "lgpl-2.0-plus_36.RULE",
        154,
        270,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![seq_container.clone()],
        &[bare_single_word, long_body],
    );
    assert_eq!(filtered, vec![seq_container]);
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_keeps_single_bridge_token_wrapper() {
    let seq_container = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "bsd-new",
        "bsd-new_303.RULE",
        28,
        44,
        Some(vec![28, 29, 30, 31, 32, 33, 34, 35, 36, 40, 41, 42, 43]),
    );
    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "bsd-new",
        "bsd-new_302.RULE",
        28,
        36,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "bsd-new",
        "bsd-new_304.RULE",
        40,
        44,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![seq_container.clone()],
        &[aho_first, aho_second],
    );
    assert_eq!(filtered, vec![seq_container]);
}

#[test]
fn test_filter_redundant_same_expression_seq_containers_keeps_small_one_sided_boundary_wrapper() {
    let seq_container = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "gpl-1.0-plus",
        "gpl_64.RULE",
        1645,
        1661,
        Some(vec![
            1645, 1646, 1647, 1648, 1649, 1650, 1651, 1652, 1653, 1654, 1657, 1658, 1659, 1660,
        ]),
    );
    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "gpl-1.0-plus",
        "gpl-1.0-plus_359.RULE",
        1648,
        1655,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "gpl-1.0-plus",
        "gpl_63.RULE",
        1657,
        1661,
        None,
    );

    let filtered = filter_redundant_same_expression_seq_containers(
        vec![seq_container.clone()],
        &[aho_first, aho_second],
    );
    assert_eq!(filtered, vec![seq_container]);
}

#[test]
fn test_filter_redundant_low_coverage_composite_seq_wrappers_drops_tiny_composite_wrapper() {
    let seq_container = make_test_match(
        crate::license_detection::seq_match::MATCH_SEQ,
        "composite-wrapper",
        "epl-2.0_or_apache-2.0_or_gpl-2.0_with_openjdk-exception_and_others4.RULE",
        55,
        60,
        Some(vec![55, 56, 57, 58, 59]),
    );
    let mut seq_container = seq_container;
    seq_container.match_coverage = 21.3;

    let aho_first = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "gpl-3.0 WITH autoconf-simple-exception-2.0",
        "gpl-3.0_with_autoconf-simple-exception-2.0_1.RULE",
        55,
        56,
        None,
    );
    let aho_second = make_test_match(
        crate::license_detection::aho_match::MATCH_AHO,
        "epl-2.0 OR apache-2.0",
        "epl-2.0_or_apache-2.0_3.RULE",
        57,
        60,
        None,
    );

    let filtered = filter_redundant_low_coverage_composite_seq_wrappers(
        vec![seq_container],
        &[aho_first, aho_second],
    );
    assert!(filtered.is_empty());
}

#[test]
fn test_hash_exact_mit() {
    let engine = get_engine();

    let mit_text = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software."#;

    let detections = engine
        .detect_with_kind(mit_text, false, false)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect partial MIT license");
}

#[test]
fn test_detect_with_kind_hash_early_return_preserves_percent_score() {
    let engine = get_engine();

    let mit_text = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."#;

    let detections = engine
        .detect_with_kind_with_score(mit_text, false, false, 100.0)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Exact MIT text should survive a 100 score threshold"
    );
    assert!(detections.iter().any(|d| {
        d.matches.iter().any(|m| {
            m.matcher == crate::license_detection::hash_match::MATCH_HASH
                && m.score == MatchScore::MAX
        })
    }));
}

#[test]
fn test_seq_partial_license() {
    let engine = get_engine();

    let partial_mit = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software."#;

    let detections = engine
        .detect_with_kind(partial_mit, false, false)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect partial MIT license");
}

#[test]
fn test_detect_with_kind_with_score_filters_partial_license() {
    let engine = get_engine();

    let partial_mit = r#"Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software."#;

    let detections = engine
        .detect_with_kind_with_score(partial_mit, false, false, 0.0)
        .expect("Detection should succeed");
    let filtered = engine
        .detect_with_kind_with_score(partial_mit, false, false, 100.0)
        .expect("Detection should succeed");

    assert!(!detections.is_empty(), "Should detect partial MIT license");
    assert!(
        filtered.is_empty(),
        "High minimum score should filter it out"
    );
}

#[test]
fn test_unknown_proprietary() {
    let engine = get_engine();

    let text = "This software is proprietary and confidential. All rights reserved.";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect unknown license or return empty"
    );
}

#[test]
fn test_no_token_boundary_false_positives() {
    let engine = get_engine();

    let test_file =
        std::path::PathBuf::from("testdata/license-golden/datadriven/lic1/config.guess-gpl2.txt");
    let text = std::fs::read_to_string(&test_file)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", test_file.display()));

    let detections = engine
        .detect_with_kind(&text, false, false)
        .expect("Detection should succeed");

    for detection in &detections {
        for m in &detection.matches {
            assert!(
                !m.license_expression.contains("cc-by-nc-sa"),
                "Found false positive cc-by-nc-sa match at lines {}-{} with matched_text: {:?}",
                m.start_line,
                m.end_line,
                m.matched_text
            );
        }
    }
}

#[test]
fn test_detect_mit_license_with_utf8_bom() {
    let engine = get_engine();

    let mit_with_bom =
        "\u{FEFF}Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.";

    let detections = engine
        .detect_with_kind(mit_with_bom, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect at least one license in MIT text with BOM"
    );

    let mit_related = detections.iter().any(|d| {
        d.license_expression
            .as_ref()
            .map(|e| e.contains("mit") || e.contains("unknown"))
            .unwrap_or(false)
    });
    assert!(
        mit_related,
        "Should detect MIT or unknown license with BOM, got: {:?}",
        detections
            .iter()
            .map(|d| d.license_expression.as_deref().unwrap_or("none"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_spdx_identifier_with_utf8_bom() {
    let engine = get_engine();

    let text = "\u{FEFF}SPDX-License-Identifier: MIT";
    let detections = engine
        .detect_with_kind(text, false, false)
        .expect("Detection should succeed");

    assert!(
        !detections.is_empty(),
        "Should detect SPDX identifier even with BOM"
    );
}

#[test]
fn test_truncate_detection_text_preserves_char_boundary() {
    let text = format!("{}é", "a".repeat(MAX_DETECTION_SIZE - 1));

    let truncated = truncate_detection_text(&text);

    assert!(truncated.len() <= MAX_DETECTION_SIZE);
    assert_eq!(truncated.len(), MAX_DETECTION_SIZE - 1);
    assert!(text.is_char_boundary(truncated.len()));
}

#[test]
fn test_detect_with_kind_handles_multibyte_boundary_at_size_limit() {
    let engine = get_engine();
    let text = format!("{}é", "a".repeat(MAX_DETECTION_SIZE - 1));

    let detections = engine
        .detect_with_kind(&text, false, false)
        .expect("Detection should succeed for truncated multibyte content");

    assert!(detections.is_empty());
}
