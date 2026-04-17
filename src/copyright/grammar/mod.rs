//! Grammar facade for copyright parse tree construction.
//!
//! Types and rule data are split into dedicated submodules to keep this module
//! small and focused while preserving existing import paths.

mod rules;
mod types;

pub(crate) use rules::GRAMMAR_RULES;
pub(crate) use types::{GrammarRule, TagMatcher};

#[cfg(test)]
mod tests;
