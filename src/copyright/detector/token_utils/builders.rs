// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::copyright::refiner::{
    is_junk_copyright, is_junk_holder, refine_author, refine_copyright, refine_holder,
    refine_holder_in_copyright_context,
};
use crate::copyright::types::{
    AuthorDetection, CopyrightDetection, HolderDetection, ParseNode, PosTag, Token, TreeLabel,
};
use crate::models::LineNumber;

use super::{
    collect_filtered_leaves, collect_holder_filtered_leaves, normalized_tokens_to_string,
    strip_all_rights_reserved,
};

pub fn extract_original_author_additional_contributors(
    tree: &[ParseNode],
) -> Option<AuthorDetection> {
    let all_leaves: Vec<&Token> = tree.iter().flat_map(collect_all_leaves).collect();
    if all_leaves.is_empty() {
        return None;
    }

    let mut has_original = false;
    let mut has_author = false;
    for t in &all_leaves {
        let v = t
            .value
            .trim_matches(|c: char| c.is_ascii_punctuation())
            .to_ascii_lowercase();
        if v == "original" {
            has_original = true;
        } else if v == "author" {
            has_author = true;
        }
    }
    if !has_original || !has_author {
        return None;
    }

    for (i, t) in all_leaves.iter().enumerate() {
        let v = t
            .value
            .trim_matches(|c: char| c.is_ascii_punctuation())
            .to_ascii_lowercase();
        if v != "additional" {
            continue;
        }
        let line = t.start_line;
        for u in all_leaves.iter().skip(i + 1).take(6) {
            if u.start_line != line {
                break;
            }
            let uv = u
                .value
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_ascii_lowercase();
            let is_contributors = u.tag == PosTag::Contributors || uv.starts_with("contributor");
            if is_contributors {
                let tokens: Vec<&Token> = vec![*t, *u];
                return build_author_from_tokens(&tokens);
            }
        }
    }

    None
}

pub fn collect_all_leaves(node: &ParseNode) -> Vec<&Token> {
    let mut result = Vec::new();
    collect_all_leaves_inner(node, &mut result);
    result
}

fn collect_all_leaves_inner<'a>(node: &'a ParseNode, result: &mut Vec<&'a Token>) {
    match node {
        ParseNode::Leaf(token) => result.push(token),
        ParseNode::Tree { children, .. } => {
            for child in children {
                collect_all_leaves_inner(child, result);
            }
        }
    }
}

pub fn build_holder_from_node(
    node: &ParseNode,
    ignored_labels: &[TreeLabel],
    ignored_pos_tags: &[PosTag],
) -> Option<HolderDetection> {
    let leaves = collect_holder_filtered_leaves(node, ignored_labels, ignored_pos_tags);
    let filtered = strip_all_rights_reserved(leaves);
    let allow_single_word_contributors = collect_all_leaves(node)
        .iter()
        .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
    build_holder_from_tokens(&filtered, allow_single_word_contributors)
}

pub fn build_holder_from_copyright_node(
    node: &ParseNode,
    ignored_labels: &[TreeLabel],
    ignored_pos_tags: &[PosTag],
) -> Option<HolderDetection> {
    let all_leaves = collect_all_leaves(node);
    let held_by_clause = all_leaves.len() >= 4
        && all_leaves[0].tag == PosTag::Copy
        && all_leaves[1].tag == PosTag::Is
        && all_leaves[2].tag == PosTag::Held
        && all_leaves[3].tag == PosTag::By;
    if held_by_clause {
        return None;
    }

    let copy_line = all_leaves
        .iter()
        .filter(|t| t.tag == PosTag::Copy && t.value.eq_ignore_ascii_case("copyright"))
        .map(|t| t.start_line)
        .min();

    let keep_prefix_lines = copy_line
        .map(|cl| signal_lines_before_copy_line(node, cl))
        .unwrap_or_default();

    let leaves = collect_holder_filtered_leaves(node, ignored_labels, ignored_pos_tags);
    let mut filtered = strip_all_rights_reserved(leaves);
    if let Some(copy_line) = copy_line {
        filtered.retain(|t| {
            t.start_line >= copy_line || keep_prefix_lines.contains(&t.start_line.get())
        });
    }

    let allow_single_word_contributors = all_leaves
        .iter()
        .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));

    build_holder_from_tokens(&filtered, allow_single_word_contributors)
}

pub fn signal_lines_before_copy_line(node: &ParseNode, copy_line: LineNumber) -> HashSet<usize> {
    let mut by_line: HashMap<usize, Vec<&Token>> = HashMap::new();
    for t in collect_all_leaves(node) {
        if t.start_line < copy_line {
            by_line.entry(t.start_line.get()).or_default().push(t);
        }
    }

    let mut keep = HashSet::new();
    for (line, tokens) in by_line {
        let has_strong_signal = tokens.iter().any(|t| {
            matches!(
                t.tag,
                PosTag::Yr
                    | PosTag::YrPlus
                    | PosTag::BareYr
                    | PosTag::Copy
                    | PosTag::Auth
                    | PosTag::Auth2
                    | PosTag::Auths
                    | PosTag::AuthDot
                    | PosTag::Maint
                    | PosTag::Contributors
                    | PosTag::Commit
                    | PosTag::SpdxContrib
            ) || t.value.eq_ignore_ascii_case("author")
                || t.value.eq_ignore_ascii_case("authors")
        });
        if has_strong_signal {
            keep.insert(line);
            continue;
        }

        let clean: Vec<&Token> = tokens
            .iter()
            .copied()
            .filter(|t| !matches!(t.tag, PosTag::Junk | PosTag::EmptyLine | PosTag::Parens))
            .collect();
        if clean.is_empty() {
            continue;
        }
        if clean.len() > 3 {
            continue;
        }

        let is_fragment = clean.iter().all(|t| {
            let v = t.value.trim_matches(|c: char| !c.is_alphanumeric());
            if v.is_empty() {
                return false;
            }
            let lower = v.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "the" | "and" | "or" | "of" | "by" | "in" | "to"
            ) {
                return false;
            }
            v.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        });

        if is_fragment {
            keep.insert(line);
        }
    }

    keep
}

pub fn build_author_from_node(node: &ParseNode) -> Option<AuthorDetection> {
    let leaves = collect_filtered_leaves(
        node,
        &[TreeLabel::YrRange, TreeLabel::YrAnd],
        super::super::NON_AUTHOR_POS_TAGS,
    );
    build_author_from_tokens(&leaves)
}

pub fn build_copyright_from_tokens(tokens: &[&Token]) -> Option<CopyrightDetection> {
    if tokens.is_empty() {
        return None;
    }
    let node_string = normalized_tokens_to_string(tokens);
    let refined = refine_copyright(&node_string)?;
    if is_junk_copyright(&refined) {
        return None;
    }
    Some(CopyrightDetection {
        copyright: refined,
        start_line: tokens
            .first()
            .map(|t| t.start_line)
            .unwrap_or(LineNumber::ONE),
        end_line: tokens
            .last()
            .map(|t| t.start_line)
            .unwrap_or(LineNumber::ONE),
    })
}

pub fn build_holder_from_tokens(
    tokens: &[&Token],
    allow_single_word_contributors: bool,
) -> Option<HolderDetection> {
    if tokens.is_empty() {
        return None;
    }
    let node_string = normalized_tokens_to_string(tokens);
    let refined = if allow_single_word_contributors {
        refine_holder_in_copyright_context(&node_string)?
    } else {
        refine_holder(&node_string)?
    };
    if is_junk_copyright(&refined) || is_junk_holder(&refined) {
        return None;
    }
    Some(HolderDetection {
        holder: refined,
        start_line: tokens
            .first()
            .map(|t| t.start_line)
            .unwrap_or(LineNumber::ONE),
        end_line: tokens
            .last()
            .map(|t| t.start_line)
            .unwrap_or(LineNumber::ONE),
    })
}

pub fn build_author_from_tokens(tokens: &[&Token]) -> Option<AuthorDetection> {
    if tokens.is_empty() {
        return None;
    }
    let node_string = normalized_tokens_to_string(tokens);
    let refined = refine_author(&node_string)?;
    if is_junk_copyright(&refined) {
        return None;
    }
    Some(AuthorDetection {
        author: refined,
        start_line: tokens
            .first()
            .map(|t| t.start_line)
            .unwrap_or(LineNumber::ONE),
        end_line: tokens
            .last()
            .map(|t| t.start_line)
            .unwrap_or(LineNumber::ONE),
    })
}
