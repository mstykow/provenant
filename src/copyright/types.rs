//! Core types for copyright detection.
//!
//! This module defines:
//! - Detection result types ([`CopyrightDetection`], [`HolderDetection`], [`AuthorDetection`])
//! - The POS tag enum ([`PosTag`]) with 55 variants for token classification
//! - Parse tree types ([`ParseNode`], [`TreeLabel`]) for grammar-based extraction
//! - The [`Token`] struct linking text values to POS tags and source locations

use serde::Serialize;

use crate::models::LineNumber;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopyrightDetection {
    pub copyright: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HolderDetection {
    pub holder: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthorDetection {
    pub author: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

/// Part-of-Speech tag for a token (type-safe, not stringly-typed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PosTag {
    // Copyright keywords
    Copy,        // "Copyright", "(c)", "Copr.", etc.
    SpdxContrib, // "SPDX-FileContributor"

    // Year-related
    Yr,     // A year like "2024"
    YrPlus, // Year with plus: "2024+"
    BareYr, // Short year: "99"

    // Names and entities
    Nnp,      // Proper noun: "John", "Smith"
    Nn,       // Common noun (catch-all)
    Caps,     // All-caps word: "MIT", "IBM"
    Pn,       // Dotted name: "P.", "DMTF."
    MixedCap, // Mixed case: "LeGrande"

    // Organization suffixes
    Comp, // Company suffix: "Inc.", "Ltd.", "GmbH"
    Uni,  // University: "University", "College"

    // Author keywords
    Auth,         // "Author", "@author"
    Auth2,        // "Written", "Developed", "Created"
    Auths,        // "Authors", "author's"
    AuthDot,      // "Author.", "Authors."
    Maint,        // "Maintainer", "Developer"
    Contributors, // "Contributors"
    Commit,       // "Committers"

    // Rights reserved
    Right,    // "Rights", "Rechte", "Droits"
    Reserved, // "Reserved", "Vorbehalten", "Réservés"

    // Conjunctions and prepositions
    Cc,   // "and", "&", ","
    Of,   // "of", "De", "Di"
    By,   // "by"
    In,   // "in", "en"
    Van,  // "van", "von", "de", "du"
    To,   // "to"
    Dash, // "-", "--", "/"

    // Special
    Email,      // Email address
    EmailStart, // Email opening bracket like "<foo"
    EmailEnd,   // Email closing bracket like "bar>"
    Url,        // URL with scheme
    Url2,       // URL without scheme (domain.com)
    Holder,     // "Holder", "Holders"
    Is,         // "is", "are"
    Held,       // "held"
    Notice,     // "NOTICE"
    Portions,   // "Portions", "Parts"
    Oth,        // "Others", "et al."
    Following,  // "following"
    Mit,        // "MIT" (special handling)
    Linux,      // "Linux"
    Parens,     // "(" or ")"
    At,         // "AT" (obfuscated email)
    Dot,        // "DOT" (obfuscated email)
    Ou,         // "OU" (org unit in certs)

    // Structural
    EmptyLine, // Empty line marker
    Junk,      // Junk to ignore

    // Cardinals
    Cd,    // Cardinal number
    Cds,   // Small cardinal (0-39)
    Month, // Month abbreviation
    Day,   // Day of week
}

#[derive(Debug, Clone)]
pub struct Token {
    pub value: String,
    pub tag: PosTag,
    pub start_line: LineNumber,
}

/// A node in the parse tree
#[derive(Debug, Clone)]
pub enum ParseNode {
    Leaf(Token),
    Tree {
        label: TreeLabel,
        children: Vec<ParseNode>,
    },
}

/// Labels for parse tree nodes (grammar non-terminals)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeLabel {
    YrRange,
    YrAnd,
    AllRightReserved,
    Name,
    NameEmail,
    NameYear,
    NameCopy,
    NameCaps,
    Company,
    AndCo,
    Copyright,
    Copyright2,
    Author,
    AndAuth,
    InitialDev,
    DashCaps,
}

impl ParseNode {
    /// Get the tag of this node (for leaf tokens) or None (for trees)
    pub fn tag(&self) -> Option<PosTag> {
        match self {
            ParseNode::Leaf(token) => Some(token.tag),
            ParseNode::Tree { .. } => None,
        }
    }

    /// Get the label of this node (for trees) or None (for leaf tokens)
    pub fn label(&self) -> Option<TreeLabel> {
        match self {
            ParseNode::Tree { label, .. } => Some(*label),
            ParseNode::Leaf(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copyright_detection_creation() {
        let d = CopyrightDetection {
            copyright: "Copyright 2024 Acme Inc.".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        };
        assert_eq!(d.copyright, "Copyright 2024 Acme Inc.");
    }

    #[test]
    fn test_token_creation() {
        let t = Token {
            value: "Copyright".to_string(),
            tag: PosTag::Copy,
            start_line: LineNumber::ONE,
        };
        assert_eq!(t.tag, PosTag::Copy);
    }

    #[test]
    fn test_parse_node_leaf() {
        let node = ParseNode::Leaf(Token {
            value: "2024".to_string(),
            tag: PosTag::Yr,
            start_line: LineNumber::new(5).unwrap(),
        });
        assert_eq!(node.tag(), Some(PosTag::Yr));
        assert_eq!(node.label(), None);
    }

    #[test]
    fn test_parse_node_tree() {
        let child = ParseNode::Leaf(Token {
            value: "2024".to_string(),
            tag: PosTag::Yr,
            start_line: LineNumber::new(3).unwrap(),
        });
        let tree = ParseNode::Tree {
            label: TreeLabel::YrRange,
            children: vec![child],
        };
        assert_eq!(tree.label(), Some(TreeLabel::YrRange));
        assert_eq!(tree.tag(), None);
    }
}
