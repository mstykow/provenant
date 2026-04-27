// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::copyright::candidates::versioned_banner_holder_from_prepared;
use crate::copyright::detector::postprocess_transforms;
use crate::copyright::detector::token_utils::normalize_whitespace;
use crate::copyright::line_tracking::{LineNumberIndex, PreparedLines};
use crate::copyright::prepare::prepare_text_line;
use crate::copyright::refiner::{
    refine_copyright, refine_holder, refine_holder_in_copyright_context,
};
use crate::copyright::types::{CopyrightDetection, HolderDetection};
use crate::models::LineNumber;

fn line_number_for_offset(content: &str, offset: usize) -> LineNumber {
    LineNumber::from_0_indexed(content[..offset].bytes().filter(|b| *b == b'\n').count())
}

fn decode_markup_entities(value: &str) -> String {
    static DECIMAL_ENTITY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&#(?P<code>\d+);?").unwrap());
    static HEX_ENTITY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&#x(?P<code>[0-9a-fA-F]+);?").unwrap());

    let mut out = value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#38;", "&")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&#60;", "<")
        .replace("&#62;", ">");

    out = HEX_ENTITY_RE
        .replace_all(&out, |caps: &regex::Captures| {
            caps.name("code")
                .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string())
        })
        .into_owned();

    out = DECIMAL_ENTITY_RE
        .replace_all(&out, |caps: &regex::Captures| {
            caps.name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string())
        })
        .into_owned();

    out
}

fn normalize_markup_attribute_value(value: &str) -> String {
    let decoded = decode_markup_entities(value);
    let prepared = prepare_text_line(&decoded);
    normalize_whitespace(&prepared)
}

mod content;
mod groups;
mod prepared;

pub use content::*;
pub use groups::*;
pub use prepared::*;
