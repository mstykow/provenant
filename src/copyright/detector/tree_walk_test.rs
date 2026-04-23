// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use super::super::token_utils::{
    collect_all_leaves, collect_holder_filtered_leaves, normalized_tokens_to_string,
    signal_lines_before_copy_line, strip_all_rights_reserved,
};
use super::*;
use crate::copyright::candidates::collect_candidate_lines;
use crate::copyright::lexer::get_tokens;
use crate::copyright::parser::parse;
use crate::copyright::refiner::refine_holder_in_copyright_context;
use crate::copyright::types::{PosTag, Token, TreeLabel};
use crate::models::LineNumber;

#[test]
fn test_extract_from_tree_nodes_builds_hall_holder_tokens() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/hall-copyright.txt");
    let content = fs::read_to_string(&path).expect("read fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    let group = groups
        .iter()
        .find(|g| {
            g.iter()
                .any(|(_ln, l)| l.contains("Richard") && l.contains("Hall"))
        })
        .expect("group containing Richard Hall");

    let tokens = get_tokens(group);
    let tree = parse(tokens);

    let mut debug_lines: Vec<String> = Vec::new();
    for (i, node) in tree.iter().enumerate() {
        let leaves = collect_all_leaves(node);
        let line = leaves.first().map(|t| t.start_line.get()).unwrap_or(0);
        let has_2004 = leaves
            .iter()
            .any(|t| t.tag == PosTag::Yr && t.value.starts_with("2004"));
        let preview = leaves
            .iter()
            .take(8)
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if has_2004 {
            debug_lines.push(format!(
                "idx={i} label={:?} line={line} preview={preview:?}",
                node.label()
            ));
        }
    }

    let mut hall_idx: Option<usize> = None;
    for (i, node) in tree.iter().enumerate() {
        let leaves = collect_all_leaves(node);
        let has_2004 = leaves
            .iter()
            .any(|t| t.tag == PosTag::Yr && t.value.starts_with("2004"));
        if !has_2004 {
            continue;
        }
        let has_richard = leaves.iter().any(|t| t.value == "Richard");
        let has_hall = leaves.iter().any(|t| t.value == "Hall");
        if has_richard && has_hall {
            hall_idx = Some(i);
            break;
        }
    }
    let hall_idx = hall_idx
        .unwrap_or_else(|| panic!("hall node not found. nodes-with-2004: {debug_lines:#?}"));
    let hall_node = &tree[hall_idx];

    let (trailing_tokens, _skip) = collect_trailing_orphan_tokens(hall_node, &tree, hall_idx + 1);
    let copy_line = collect_all_leaves(hall_node)
        .iter()
        .filter(|t| t.tag == PosTag::Copy && t.value.eq_ignore_ascii_case("copyright"))
        .map(|t| t.start_line.get())
        .min();
    let keep_prefix_lines = copy_line
        .map(|cl| signal_lines_before_copy_line(hall_node, LineNumber::new(cl).unwrap()))
        .unwrap_or_default();

    let node_holder_leaves = collect_holder_filtered_leaves(
        hall_node,
        super::super::NON_HOLDER_LABELS,
        super::super::NON_HOLDER_POS_TAGS,
    );
    let mut holder_tokens: Vec<&Token> = Vec::new();
    let mut node_holder_leaves = strip_all_rights_reserved(node_holder_leaves);
    if let Some(copy_line) = copy_line {
        node_holder_leaves.retain(|t| {
            t.start_line.get() >= copy_line || keep_prefix_lines.contains(&t.start_line.get())
        });
    }
    holder_tokens.extend(node_holder_leaves);
    holder_tokens.extend(&trailing_tokens);

    let holder_string = normalized_tokens_to_string(&holder_tokens);
    let refined = refine_holder_in_copyright_context(&holder_string);

    assert_eq!(
        refined.as_deref(),
        Some("Richard S. Hall"),
        "idx={hall_idx} holder_string={holder_string:?} trailing={:?} node={hall_node:#?}",
        trailing_tokens
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_collect_trailing_orphan_tokens_keeps_confidential_and_proprietary_phrase() {
    let text = "(c) Example Corp. and affiliates. Confidential and proprietary.";
    let prepared = super::super::super::prepare::prepare_text_line(text);
    let tokens = get_tokens(&[(1, prepared)]);
    let tree = parse(tokens);
    let (copyright_idx, copyright_node) = tree
        .iter()
        .enumerate()
        .find(|(_i, n)| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        })
        .expect("Should parse a COPYRIGHT node");
    let start = copyright_idx + 1;

    assert!(
        should_start_absorbing(copyright_node, &tree, start),
        "Should absorb trailing confidentiality phrase; tree={:?}",
        tree
    );

    let (trailing, _skip) = collect_trailing_orphan_tokens(copyright_node, &tree, start);
    let trailing_values: Vec<&str> = trailing.iter().map(|t| t.value.as_str()).collect();

    assert!(
        trailing_values.contains(&"and"),
        "Trailing tokens should include trailing conjunction, got: {:?}",
        trailing_values
    );
    assert!(
        trailing_values.contains(&"proprietary."),
        "Trailing tokens should include 'proprietary.', got: {:?}",
        trailing_values
    );
}

#[test]
fn test_index_html_first_group_span_extraction_keeps_copyright_word() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    let tokens = get_tokens(&groups[0]);
    let tree = parse(tokens);

    let (c, _h, _a) = extract_from_spans(&tree, false);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "Span extraction did not produce expected Copyright (c) line. Got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_index_html_first_group_tree_node_extraction_matches_span_extraction() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    let tokens = get_tokens(&groups[0]);
    let tree = parse(tokens);

    let (c, _h, _a) = extract_from_tree_nodes(&tree, false);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "Tree-node extraction did not produce expected Copyright (c) line. Got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_copyright_holder_suffix_authors_and_contributors() {
    let text = "Copyright 2018-2019 @paritytech/substrate-light-ui authors & contributors";
    let prepared = super::super::super::prepare::prepare_text_line(text);
    let tokens = get_tokens(&[(1, prepared)]);
    let tree = parse(tokens);
    let (copyright_idx, copyright_node) = tree
        .iter()
        .enumerate()
        .find(|(_i, n)| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        })
        .expect("Should parse a COPYRIGHT node");
    let start = copyright_idx + 1;
    assert!(
        should_start_absorbing(copyright_node, &tree, start),
        "Should start absorbing trailing suffix nodes; tree={:?}",
        tree
    );
    let (trailing, _skip) = collect_trailing_orphan_tokens(copyright_node, &tree, start);
    assert!(
        trailing
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("authors")),
        "Trailing tokens should include 'authors', got: {:?}",
        trailing
    );
    assert!(
        trailing
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("contributors")),
        "Trailing tokens should include 'contributors', got: {:?}",
        trailing
    );

    let (c, h, a) = super::super::detect_copyrights_from_text(text);
    assert!(
        c.iter().any(|cr| cr.copyright == text),
        "Should keep authors/contributors suffix in copyright: {:?}",
        c
    );
    assert!(
        h.iter()
            .any(|hd| hd.holder == "paritytech/substrate-light-ui authors & contributors"),
        "Should keep authors/contributors suffix in holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_detect_copyright_holder_suffix_the_respective_contributors() {
    let text = "Copyright (c) 2014, 2015, the respective contributors";
    let prepared = super::super::super::prepare::prepare_text_line(text);
    let tokens = get_tokens(&[(1, prepared)]);
    let tree = parse(tokens);
    let (copyright_idx, copyright_node) = tree
        .iter()
        .enumerate()
        .find(|(_i, n)| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        })
        .expect("Should parse a COPYRIGHT node");
    let start = copyright_idx + 1;
    assert!(
        should_start_absorbing(copyright_node, &tree, start),
        "Should start absorbing respective-contributors suffix; tree={:?}",
        tree
    );
    let (trailing, _skip) = collect_trailing_orphan_tokens(copyright_node, &tree, start);
    assert!(
        trailing
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("contributors")),
        "Trailing tokens should include 'contributors', got: {:?}",
        trailing
    );

    let (c, h, a) = super::super::detect_copyrights_from_text(text);

    assert!(
        c.iter().any(|cr| cr.copyright == text),
        "Should keep the full respective-contributors suffix in copyright: {:?}",
        c
    );
    assert!(
        h.iter()
            .any(|hd| hd.holder == "the respective contributors"),
        "Should detect 'the respective contributors' as holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_oprofile_authors_copyright() {
    let content = " * @remark Copyright 2002 OProfile authors
 * @remark Read the file COPYING
 *
 * @Modifications Daniel Hansel
 * Modified by Aravind Menon for Xen
 * These modifications are:
 * Copyright (C) 2005 Hewlett-Packard Co.";
    let (c, h, _a) = super::super::detect_copyrights_from_text(content);

    let prepared_line = super::super::super::prepare::prepare_text_line(
        " * @remark Copyright 2002 OProfile authors",
    );
    let tokens = get_tokens(&[(1, prepared_line.clone())]);
    let parsed = parse(tokens.clone());
    let refined = crate::copyright::refiner::refine_copyright(&prepared_line);
    let token_debug: Vec<String> = tokens
        .iter()
        .map(|t| format!("{}:{:?}", t.value, t.tag))
        .collect();
    let parsed_debug: Vec<String> = parsed
        .iter()
        .map(|n| {
            let leaves: Vec<String> = collect_all_leaves(n)
                .iter()
                .map(|t| format!("{}:{:?}", t.value, t.tag))
                .collect();
            format!("label={:?} tag={:?} leaves={leaves:?}", n.label(), n.tag())
        })
        .collect();
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright 2002 OProfile authors"),
        "Should detect 'Copyright 2002 OProfile authors'. prepared={prepared_line:?} refined={refined:?} tokens={token_debug:?} parsed={parsed_debug:?} got: {c:?}",
    );
    assert!(
        h.iter().any(|h| h.holder == "OProfile authors"),
        "Should detect 'OProfile authors' holder. prepared={prepared_line:?} tokens={token_debug:?} got: {h:?}",
    );
}
