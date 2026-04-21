// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Bottom-up grammar parser for copyright detection.
//!
//! Applies grammar rules to a sequence of POS-tagged tokens, building
//! a parse tree by replacing matched token/node spans with tree nodes.
//! Uses a single-pass approach matching Python's pygmars `loop=1` behavior.

use std::time::Instant;

use super::grammar::{GRAMMAR_RULES, GrammarRule, TagMatcher};
use super::types::{ParseNode, Token};
use crate::models::LineNumber;

fn first_line(node: &ParseNode) -> Option<LineNumber> {
    match node {
        ParseNode::Leaf(t) => Some(t.start_line),
        ParseNode::Tree { children, .. } => children.iter().filter_map(first_line).min(),
    }
}

fn last_line(node: &ParseNode) -> Option<LineNumber> {
    match node {
        ParseNode::Leaf(t) => Some(t.start_line),
        ParseNode::Tree { children, .. } => children.iter().filter_map(last_line).max(),
    }
}

/// Parse a sequence of POS-tagged tokens into a parse tree.
///
/// Applies grammar rules bottom-up: scans the node sequence for patterns
/// that match a rule, replaces the matched span with a tree node, and
/// continues until no more rules fire (fixpoint).
///
/// Returns the final sequence of `ParseNode` (mix of leaf tokens and
/// tree nodes).
pub fn parse(tokens: Vec<Token>) -> Vec<ParseNode> {
    parse_with_deadline(tokens, None)
}

pub fn parse_with_deadline(tokens: Vec<Token>, deadline: Option<Instant>) -> Vec<ParseNode> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut nodes: Vec<ParseNode> = tokens.into_iter().map(ParseNode::Leaf).collect();

    // Iterate until fixpoint (no rules fire in a full pass).
    // Safety bound to prevent infinite loops.
    let max_iterations = 50;
    for _ in 0..max_iterations {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }

        let mut changed = false;

        for rule in GRAMMAR_RULES.iter() {
            if let Some(new_nodes) = try_apply_rule(rule, &nodes) {
                nodes = new_nodes;
                changed = true;
                break;
            }
        }

        if !changed {
            break;
        }
    }

    nodes
}

/// Try to apply a single grammar rule to the node sequence.
/// Returns `Some(new_nodes)` if the rule matched somewhere, `None` otherwise.
fn try_apply_rule(rule: &GrammarRule, nodes: &[ParseNode]) -> Option<Vec<ParseNode>> {
    let pattern_len = rule.pattern.len();
    if pattern_len == 0 || nodes.len() < pattern_len {
        return None;
    }

    // Scan for the first position where the pattern matches.
    for start in 0..=(nodes.len() - pattern_len) {
        if matches_at(rule, nodes, start) {
            // Build the replacement tree node.
            let matched: Vec<ParseNode> = nodes[start..start + pattern_len].to_vec();
            let tree_node = ParseNode::Tree {
                label: rule.label,
                children: matched,
            };

            // Construct new node sequence: before + tree_node + after.
            let mut new_nodes = Vec::with_capacity(nodes.len() - pattern_len + 1);
            new_nodes.extend_from_slice(&nodes[..start]);
            new_nodes.push(tree_node);
            new_nodes.extend_from_slice(&nodes[start + pattern_len..]);

            return Some(new_nodes);
        }
    }

    None
}

/// Check if a rule's pattern matches the node sequence at position `start`.
fn matches_at(rule: &GrammarRule, nodes: &[ParseNode], start: usize) -> bool {
    if rule.label == crate::copyright::types::TreeLabel::NameCopy
        && rule.pattern.len() == 2
        && matches!(
            rule.pattern[0],
            TagMatcher::Tag(crate::copyright::types::PosTag::Nnp)
        )
        && matches!(
            rule.pattern[1],
            TagMatcher::Tag(crate::copyright::types::PosTag::Copy)
        )
        && last_line(&nodes[start]) != first_line(&nodes[start + 1])
    {
        return false;
    }

    if rule.label == crate::copyright::types::TreeLabel::Copyright2
        && rule.pattern.len() == 2
        && matches!(
            rule.pattern[1],
            TagMatcher::Label(crate::copyright::types::TreeLabel::Copyright2)
        )
        && let TagMatcher::AnyTagOrLabel(tags, labels) = &rule.pattern[0]
        && tags.len() == 1
        && tags[0] == crate::copyright::types::PosTag::Nnp
        && labels.len() == 2
        && labels.contains(&crate::copyright::types::TreeLabel::Name)
        && labels.contains(&crate::copyright::types::TreeLabel::Company)
        && last_line(&nodes[start]) != first_line(&nodes[start + 1])
    {
        return false;
    }

    if rule.label == crate::copyright::types::TreeLabel::Copyright
        && rule.pattern.len() == 2
        && matches!(
            rule.pattern[0],
            TagMatcher::Label(crate::copyright::types::TreeLabel::Copyright2)
        )
        && matches!(
            rule.pattern[1],
            TagMatcher::Label(crate::copyright::types::TreeLabel::Copyright)
        )
        && last_line(&nodes[start]) != first_line(&nodes[start + 1])
    {
        return false;
    }

    for (i, matcher) in rule.pattern.iter().enumerate() {
        if !matcher_matches(matcher, &nodes[start + i]) {
            return false;
        }
    }
    true
}

/// Check if a single `TagMatcher` matches a single `ParseNode`.
fn matcher_matches(matcher: &TagMatcher, node: &ParseNode) -> bool {
    match matcher {
        TagMatcher::Tag(expected_tag) => node.tag() == Some(*expected_tag),

        TagMatcher::Label(expected_label) => node.label() == Some(*expected_label),

        TagMatcher::AnyTag(tags) => {
            if let Some(node_tag) = node.tag() {
                tags.contains(&node_tag)
            } else {
                false
            }
        }

        TagMatcher::AnyLabel(labels) => {
            if let Some(node_label) = node.label() {
                labels.contains(&node_label)
            } else {
                false
            }
        }

        TagMatcher::AnyTagOrLabel(tags, labels) => {
            if let Some(node_tag) = node.tag()
                && tags.contains(&node_tag)
            {
                return true;
            }
            if let Some(node_label) = node.label()
                && labels.contains(&node_label)
            {
                return true;
            }
            false
        }
    }
}

#[cfg(test)]
#[path = "parser_test.rs"]
mod tests;
