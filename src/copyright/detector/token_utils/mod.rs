// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::copyright::types::{PosTag, Token};

#[cfg(test)]
#[path = "../token_utils_test.rs"]
mod tests;

mod builders;
mod filters;

pub use builders::*;
pub use filters::*;

pub fn is_copyright_span_token(token: &Token) -> bool {
    !matches!(token.tag, PosTag::EmptyLine | PosTag::Junk)
}

pub fn is_author_span_token(token: &Token) -> bool {
    !matches!(
        token.tag,
        PosTag::EmptyLine | PosTag::Junk | PosTag::Copy | PosTag::SpdxContrib
    )
}

pub fn normalized_tokens_to_string(tokens: &[&Token]) -> String {
    let mut out = String::new();
    let mut first = true;

    for token in tokens {
        for piece in token.value.split_whitespace() {
            if !first {
                out.push(' ');
            }
            out.push_str(piece);
            first = false;
        }
    }

    out
}

pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn group_by<T, K>(items: Vec<T>, key_fn: impl Fn(&T) -> K) -> Vec<(K, Vec<T>)>
where
    K: std::hash::Hash + Eq + Clone,
{
    let mut order: Vec<K> = Vec::new();
    let mut seen: HashSet<K> = HashSet::new();
    let mut map: HashMap<K, Vec<T>> = HashMap::new();
    for item in items {
        let key = key_fn(&item);
        if seen.insert(key.clone()) {
            order.push(key.clone());
        }
        map.entry(key).or_default().push(item);
    }
    order
        .into_iter()
        .filter_map(|k| map.remove_entry(&k))
        .collect()
}
