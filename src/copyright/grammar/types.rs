//! Grammar rules for copyright parse tree construction.
//!
//! Rules are applied bottom-up to a sequence of POS-tagged tokens.
//! Each rule matches a pattern of tags/labels and replaces the matched
//! span with a new tree node.
//!
//! Ported from the Python `GRAMMAR` string in
//! `reference/scancode-toolkit/src/cluecode/copyrights.py` lines 2367–3530.
//! Includes all rule categories: YEAR, ALL RIGHTS RESERVED, EMAIL, CC, NAME,
//! COMPANY, ANDCO, DASHCAPS, NAME-EMAIL, NAME-YEAR, URL, INITIALDEV,
//! COPYRIGHT, COPYRIGHT2, NAME-COPY, NAME-CAPS, AUTHOR, and ANDAUTH.
//!
//! Quantifier expansion strategy:
//! - `<X>+` (one or more) → rules for 1, 2, and sometimes 3 instances.
//!   The parser applies rules iteratively so longer sequences build up.
//! - `<X>?` (optional) → two rules: one with X and one without.
//! - `<X>*` (zero or more) → rules without X, and with 1 instance.
//! - `<X>{3}` (exactly 3) → one rule with exactly 3 instances.

use crate::copyright::types::{PosTag, TreeLabel};

/// A matcher for a single position in a grammar rule pattern.
#[derive(Debug, Clone)]
pub(crate) enum TagMatcher {
    /// Match a specific POS tag on a leaf token.
    Tag(PosTag),
    /// Match a specific tree label on a tree node.
    Label(TreeLabel),
    /// Match any of several POS tags.
    AnyTag(&'static [PosTag]),
    /// Match any of several tree labels.
    AnyLabel(&'static [TreeLabel]),
    /// Match any of several tags OR labels.
    AnyTagOrLabel(&'static [PosTag], &'static [TreeLabel]),
}

/// A grammar rule: matches a pattern and produces a tree node with the given label.
#[derive(Debug, Clone)]
pub(crate) struct GrammarRule {
    /// The label for the tree node produced by this rule.
    pub(crate) label: TreeLabel,
    /// The pattern to match (sequence of matchers).
    pub(crate) pattern: &'static [TagMatcher],
}
