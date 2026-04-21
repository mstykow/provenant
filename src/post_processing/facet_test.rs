// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::test_utils::{dir, file};
use super::*;
use crate::models::{FileInfo, Tallies, TallyEntry};

#[test]
fn build_facet_rules_and_assign_facets_match_reference_semantics() {
    let rules = build_facet_rules(&[
        "dev=*.c".to_string(),
        "tests=*/tests/*".to_string(),
        "data=*.json".to_string(),
        "docs=*/docs/*".to_string(),
    ])
    .expect("facet rules should compile");

    let mut files = vec![
        dir("cli"),
        file("cli/README.first"),
        file("cli/composer.json"),
        file("cli/tests/some.c"),
        file("cli/docs/prefix-license-suffix"),
    ];

    assign_facets(&mut files, &rules);

    assert_eq!(files[0].facets, Vec::<String>::new());
    assert_eq!(files[1].facets, vec!["core".to_string()]);
    assert_eq!(files[2].facets, vec!["data".to_string()]);
    assert_eq!(
        files[3].facets,
        vec!["dev".to_string(), "tests".to_string()]
    );
    assert_eq!(files[4].facets, vec!["docs".to_string()]);
}

#[test]
fn assign_facets_uses_path_only_for_slash_patterns_and_keeps_broad_slashless_matching() {
    let rules = build_facet_rules(&[
        "data=*.json".to_string(),
        "dev=*ada*".to_string(),
        "docs=*/docs/*".to_string(),
    ])
    .expect("facet rules should compile");

    let mut files = vec![
        file("project/nested/config.json"),
        file("project/ada/config.txt"),
        file("project/docs/config.txt"),
        file("project/config.txt"),
    ];

    assign_facets(&mut files, &rules);

    assert_eq!(files[0].facets, vec!["data".to_string()]);
    assert_eq!(files[1].facets, vec!["dev".to_string()]);
    assert_eq!(files[2].facets, vec!["docs".to_string()]);
    assert_eq!(files[3].facets, vec!["core".to_string()]);
}

#[test]
fn assign_facets_emits_each_facet_once_even_when_multiple_rules_match() {
    let rules = build_facet_rules(&[
        "dev=*.rs".to_string(),
        "dev=*src*".to_string(),
        "dev=*.rs".to_string(),
    ])
    .expect("facet rules should compile");

    let mut files = vec![file("project/src/lib.rs")];

    assign_facets(&mut files, &rules);

    assert_eq!(files[0].facets, vec!["dev".to_string()]);
}

#[test]
fn compute_tallies_by_facet_uses_fixed_order_and_drops_null_buckets() {
    let files = vec![
        FileInfo {
            facets: vec!["core".to_string()],
            tallies: Some(Tallies {
                detected_license_expression: vec![
                    TallyEntry {
                        value: None,
                        count: 1,
                    },
                    TallyEntry {
                        value: Some("mit".to_string()),
                        count: 1,
                    },
                ],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                programming_language: vec![TallyEntry {
                    value: Some("Rust".to_string()),
                    count: 1,
                }],
            }),
            ..file("project/src/lib.rs")
        },
        FileInfo {
            facets: vec!["dev".to_string(), "tests".to_string()],
            tallies: Some(Tallies {
                detected_license_expression: vec![TallyEntry {
                    value: Some("apache-2.0".to_string()),
                    count: 1,
                }],
                copyrights: vec![],
                holders: vec![],
                authors: vec![],
                programming_language: vec![TallyEntry {
                    value: Some("C".to_string()),
                    count: 1,
                }],
            }),
            ..file("project/tests/test.c")
        },
    ];

    let tallies_by_facet = compute_tallies_by_facet(&files).expect("tallies by facet exist");

    assert_eq!(
        tallies_by_facet
            .iter()
            .map(|entry| entry.facet.as_str())
            .collect::<Vec<_>>(),
        vec!["core", "dev", "tests", "docs", "data", "examples"]
    );
    assert_eq!(
        tallies_by_facet[0].tallies.detected_license_expression[0]
            .value
            .as_deref(),
        Some("mit")
    );
    assert_eq!(
        tallies_by_facet[1].tallies.programming_language[0]
            .value
            .as_deref(),
        Some("C")
    );
    assert!(tallies_by_facet[3].tallies.is_empty());
}

#[test]
fn compute_tallies_by_facet_emits_empty_buckets_for_directory_only_input() {
    let files = vec![dir("project"), dir("project/src")];

    let tallies_by_facet = compute_tallies_by_facet(&files).expect("facet buckets should exist");

    assert_eq!(
        tallies_by_facet
            .iter()
            .map(|entry| entry.facet.as_str())
            .collect::<Vec<_>>(),
        vec!["core", "dev", "tests", "docs", "data", "examples"]
    );
    assert!(
        tallies_by_facet
            .iter()
            .all(|entry| entry.tallies.is_empty())
    );
}
