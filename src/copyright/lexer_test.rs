// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::models::LineNumber;

#[test]
fn test_simple_copyright_line() {
    let lines = vec![(1, "Copyright 2024 Acme Inc.".to_string())];
    let tokens = get_tokens(&lines);
    assert!(!tokens.is_empty(), "Should produce tokens");

    assert_eq!(tokens[0].value, "Copyright");
    assert_eq!(tokens[0].tag, PosTag::Copy);
    assert_eq!(tokens[0].start_line, LineNumber::ONE);

    assert_eq!(tokens[1].value, "2024");
    assert_eq!(tokens[1].tag, PosTag::Yr);

    assert!(tokens.len() >= 3, "tokens: {tokens:?}");
}

#[test]
fn test_empty_input() {
    let lines: Vec<(usize, String)> = vec![];
    let tokens = get_tokens(&lines);
    assert!(tokens.is_empty());
}

#[test]
fn test_splits_on_tabs_and_equals() {
    let lines = vec![(1, "foo\tbar=baz".to_string())];
    let tokens = get_tokens(&lines);
    let values: Vec<&str> = tokens.iter().map(|t| t.value.as_str()).collect();
    assert_eq!(values, vec!["foo", "bar", "baz"]);
}

#[test]
fn test_strips_trailing_colon() {
    let lines = vec![(1, "Author: John".to_string())];
    let tokens = get_tokens(&lines);
    // "Author:" should have colon stripped → "Author"
    assert_eq!(tokens[0].value, "Author");
}

#[test]
fn test_discards_lone_colon_and_dot() {
    let lines = vec![(1, "foo : . bar".to_string())];
    let tokens = get_tokens(&lines);
    let values: Vec<&str> = tokens.iter().map(|t| t.value.as_str()).collect();
    assert_eq!(values, vec!["foo", "bar"]);
}

#[test]
fn test_strips_trailing_quote_comma() {
    let lines = vec![(1, "name',".to_string())];
    let tokens = get_tokens(&lines);
    assert_eq!(tokens[0].value, "name");
}

#[test]
fn test_strips_dangling_quote_after_colon_trim() {
    let lines = vec![(
        1,
        "'attributionText': 'Data provided by Marvel.'".to_string(),
    )];
    let tokens = get_tokens(&lines);
    let values: Vec<&str> = tokens.iter().map(|t| t.value.as_str()).collect();
    assert!(values.contains(&"attributionText"), "tokens: {tokens:?}");
    assert!(!values.contains(&"attributionText'"), "tokens: {tokens:?}");
}

#[test]
fn test_splits_trailing_commas_into_cc_token() {
    let lines = vec![(1, "Stearns, Michael".to_string())];
    let tokens = get_tokens(&lines);
    let values: Vec<&str> = tokens.iter().map(|t| t.value.as_str()).collect();
    let tags: Vec<PosTag> = tokens.iter().map(|t| t.tag).collect();
    assert_eq!(values, vec!["Stearns", ",", "Michael"]);
    assert_eq!(tags[1], PosTag::Cc);
}

#[test]
fn test_empty_line_continuation() {
    let lines = vec![
        (1, "Copyright 2024".to_string()),
        (2, "".to_string()),
        (3, "Acme Inc.".to_string()),
    ];
    let tokens = get_tokens(&lines);
    // Empty line after "Copyright 2024" should be skipped (continuation).
    let has_empty = tokens.iter().any(|t| t.tag == PosTag::EmptyLine);
    assert!(!has_empty, "Empty line should be skipped as continuation");
}

#[test]
fn test_empty_line_break() {
    let lines = vec![
        (1, "Acme Inc.".to_string()),
        (2, "".to_string()),
        (3, "Other stuff".to_string()),
    ];
    let tokens = get_tokens(&lines);
    let has_empty = tokens.iter().any(|t| t.tag == PosTag::EmptyLine);
    assert!(
        has_empty,
        "Empty line after non-continuation should emit EMPTY_LINE"
    );
}

#[test]
fn test_pos_tags_assigned() {
    let lines = vec![(1, "Copyright (c) 2020-2024 Acme and Bar".to_string())];
    let tokens = get_tokens(&lines);
    assert!(tokens.len() >= 4, "tokens: {tokens:?}");

    assert_eq!(tokens[0].tag, PosTag::Copy);
    assert_eq!(tokens[1].tag, PosTag::Copy); // (c)
}

#[test]
fn test_line_numbers_preserved() {
    let lines = vec![
        (10, "Copyright 2024".to_string()),
        (11, "Acme Inc.".to_string()),
    ];
    let tokens = get_tokens(&lines);
    assert_eq!(tokens[0].start_line, LineNumber::new(10).unwrap());
    // "Acme" should be on line 11
    let acme = tokens.iter().find(|t| t.value == "Acme").unwrap();
    assert_eq!(acme.start_line, LineNumber::new(11).unwrap());
}

#[test]
fn test_semicolons_split() {
    let lines = vec![(1, "foo;bar;baz".to_string())];
    let tokens = get_tokens(&lines);
    assert_eq!(tokens.len(), 3);
}

#[test]
fn test_token_count_matches_words() {
    let lines = vec![(1, "Copyright 2024 Acme Inc.".to_string())];
    let tokens = get_tokens(&lines);
    assert!(
        tokens.len() >= 3,
        "Expected at least 3 tokens, got {}",
        tokens.len()
    );
}

#[test]
fn test_retag_camelcase_junk_before_company_suffix_in_copyright_context() {
    let lines = vec![(1, "Copyright (C) OpenSharedMap Inc.".to_string())];
    let tokens = get_tokens(&lines);
    let osm = tokens
        .iter()
        .find(|t| t.value == "OpenSharedMap")
        .expect("OpenSharedMap token should exist");
    assert_eq!(
        osm.tag,
        PosTag::Nnp,
        "Expected OpenSharedMap to be retagged as Nnp in copyright context"
    );
}

#[test]
fn test_does_not_retag_camelcase_junk_outside_copyright_context() {
    let lines = vec![(1, "Use OpenSharedMap Inc. APIs".to_string())];
    let tokens = get_tokens(&lines);
    let osm = tokens
        .iter()
        .find(|t| t.value == "OpenSharedMap")
        .expect("OpenSharedMap token should exist");
    assert_eq!(
        osm.tag,
        PosTag::Junk,
        "OpenSharedMap should remain Junk outside copyright context"
    );
}

#[test]
fn test_does_not_retag_all_caps_token_before_company_suffix() {
    let lines = vec![(
        1,
        "Copyright Owner Inc. AS IS AND ANY EXPRESS OR IMPLIED WARRANTIES".to_string(),
    )];
    let tokens = get_tokens(&lines);
    let as_token = tokens
        .iter()
        .find(|t| t.value == "AS")
        .expect("AS token should exist");
    assert_ne!(
        as_token.tag,
        PosTag::Nnp,
        "All-caps AS token should not be retagged as a name"
    );
}

#[test]
fn test_quoted_lower_camel_key_remains_junk_after_trimming() {
    let lines = vec![(1, "'attributionText': value".to_string())];
    let tokens = get_tokens(&lines);
    let token = tokens
        .iter()
        .find(|t| t.value == "attributionText")
        .expect("attributionText token should exist");
    assert_eq!(token.tag, PosTag::Junk, "tokens: {tokens:?}");
}

#[test]
fn test_quoted_plain_key_is_junk_for_structured_literals() {
    let lines = vec![(1, "'data': {}".to_string())];
    let tokens = get_tokens(&lines);
    let token = tokens
        .iter()
        .find(|t| t.value == "data")
        .expect("data token should exist");
    assert_eq!(token.tag, PosTag::Junk, "tokens: {tokens:?}");
}
