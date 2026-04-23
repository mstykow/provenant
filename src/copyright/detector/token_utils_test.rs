// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::copyright::types::{ParseNode, PosTag, Token, TreeLabel};
use crate::models::LineNumber;

#[test]
fn test_strip_all_rights_reserved_basic() {
    let tokens = [
        Token {
            value: "Copyright".to_string(),
            tag: PosTag::Copy,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "2024".to_string(),
            tag: PosTag::Yr,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "Acme".to_string(),
            tag: PosTag::Nnp,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "All".to_string(),
            tag: PosTag::Nn,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "Rights".to_string(),
            tag: PosTag::Right,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "Reserved".to_string(),
            tag: PosTag::Reserved,
            start_line: LineNumber::ONE,
        },
    ];
    let refs: Vec<&Token> = tokens.iter().collect();
    let result = strip_all_rights_reserved(refs);
    assert_eq!(result.len(), 3, "Should strip All Rights Reserved");
    assert_eq!(result[0].value, "Copyright");
    assert_eq!(result[1].value, "2024");
    assert_eq!(result[2].value, "Acme");
}

#[test]
fn test_collect_filtered_leaves_filters_pos_tags() {
    let node = ParseNode::Tree {
        label: TreeLabel::Copyright,
        children: vec![
            ParseNode::Leaf(Token {
                value: "Copyright".to_string(),
                tag: PosTag::Copy,
                start_line: LineNumber::ONE,
            }),
            ParseNode::Leaf(Token {
                value: "2024".to_string(),
                tag: PosTag::Yr,
                start_line: LineNumber::ONE,
            }),
            ParseNode::Leaf(Token {
                value: "Acme".to_string(),
                tag: PosTag::Nnp,
                start_line: LineNumber::ONE,
            }),
        ],
    };
    let leaves = collect_filtered_leaves(&node, &[], &[PosTag::Copy, PosTag::Yr]);
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].value, "Acme");
}

#[test]
fn test_collect_filtered_leaves_filters_tree_labels() {
    let node = ParseNode::Tree {
        label: TreeLabel::Copyright,
        children: vec![
            ParseNode::Leaf(Token {
                value: "Copyright".to_string(),
                tag: PosTag::Copy,
                start_line: LineNumber::ONE,
            }),
            ParseNode::Tree {
                label: TreeLabel::YrRange,
                children: vec![ParseNode::Leaf(Token {
                    value: "2024".to_string(),
                    tag: PosTag::Yr,
                    start_line: LineNumber::ONE,
                })],
            },
            ParseNode::Leaf(Token {
                value: "Acme".to_string(),
                tag: PosTag::Nnp,
                start_line: LineNumber::ONE,
            }),
        ],
    };
    let leaves = collect_filtered_leaves(&node, &[TreeLabel::YrRange], &[]);
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[0].value, "Copyright");
    assert_eq!(leaves[1].value, "Acme");
}
