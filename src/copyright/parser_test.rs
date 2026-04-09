use super::*;
use crate::copyright::types::{PosTag, TreeLabel};
use crate::models::LineNumber;

fn make_token(value: &str, tag: PosTag, line: usize) -> Token {
    Token {
        value: value.to_string(),
        tag,
        start_line: LineNumber::new(line).unwrap(),
    }
}

#[test]
fn test_parse_empty() {
    let result = parse(vec![]);
    assert!(result.is_empty());
}

#[test]
fn test_parse_single_token() {
    let tokens = vec![make_token("hello", PosTag::Nn, 1)];
    let result = parse(tokens);
    assert_eq!(result.len(), 1);
    assert!(result[0].tag().is_some());
}

#[test]
fn test_parse_year_range() {
    let tokens = vec![
        make_token("2020", PosTag::Yr, 1),
        make_token("-", PosTag::Dash, 1),
        make_token("2024", PosTag::Yr, 1),
    ];
    let result = parse(tokens);
    // Should be reduced to a single YR-RANGE node.
    assert_eq!(result.len(), 1, "result: {result:?}");
    assert_eq!(result[0].label(), Some(TreeLabel::YrRange));
}

#[test]
fn test_parse_year_comma_year() {
    let tokens = vec![
        make_token("2020", PosTag::Yr, 1),
        make_token(",", PosTag::Cc, 1),
        make_token("2024", PosTag::Yr, 1),
    ];
    let result = parse(tokens);
    assert_eq!(result.len(), 1, "result: {result:?}");
    assert_eq!(result[0].label(), Some(TreeLabel::YrRange));
}

#[test]
fn test_parse_preserves_unmatched() {
    let tokens = vec![
        make_token("hello", PosTag::Nn, 1),
        make_token("world", PosTag::Nn, 1),
    ];
    let result = parse(tokens);
    // Two NN tokens — no grammar rule matches NN NN, so both preserved.
    assert_eq!(result.len(), 2);
}

#[test]
fn test_parse_with_expired_deadline_returns_without_reducing() {
    let tokens = vec![
        make_token("2020", PosTag::Yr, 1),
        make_token("-", PosTag::Dash, 1),
        make_token("2024", PosTag::Yr, 1),
    ];

    let result = parse_with_deadline(tokens, Some(Instant::now()));
    assert_eq!(result.len(), 3);
    assert!(result.iter().all(|n| n.tag().is_some()));
}

#[test]
fn test_matcher_tag() {
    let node = ParseNode::Leaf(make_token("2024", PosTag::Yr, 1));
    assert!(matcher_matches(&TagMatcher::Tag(PosTag::Yr), &node));
    assert!(!matcher_matches(&TagMatcher::Tag(PosTag::Nn), &node));
}

#[test]
fn test_matcher_label() {
    let node = ParseNode::Tree {
        label: TreeLabel::YrRange,
        children: vec![],
    };
    assert!(matcher_matches(
        &TagMatcher::Label(TreeLabel::YrRange),
        &node
    ));
    assert!(!matcher_matches(&TagMatcher::Label(TreeLabel::Name), &node));
}

#[test]
fn test_matcher_any_tag() {
    let node = ParseNode::Leaf(make_token("2024", PosTag::Yr, 1));
    assert!(matcher_matches(
        &TagMatcher::AnyTag(&[PosTag::Yr, PosTag::BareYr]),
        &node
    ));
    assert!(!matcher_matches(
        &TagMatcher::AnyTag(&[PosTag::Nn, PosTag::Cc]),
        &node
    ));
}
