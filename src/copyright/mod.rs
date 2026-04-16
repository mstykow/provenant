//! Copyright detection module.
//!
//! Detects copyright statements, holder names, and author information
//! from source code files using a four-stage pipeline:
//! 1. Text preparation (normalization)
//! 2. Candidate line selection
//! 3. Lexing (POS tagging) and parsing (grammar rules)
//! 4. Refinement and junk filtering

use std::time::Duration;

mod candidates;
mod credits;
mod detector;
mod detector_input_normalization;
pub mod golden_utils;
mod grammar;
mod hints;
mod lexer;
mod line_tracking;
mod parser;
mod patterns;
mod prepare;
mod refiner;
mod types;

#[cfg(all(test, feature = "golden-tests"))]
mod golden_test;

pub use credits::{detect_credits_authors, is_credits_file};
pub use types::{AuthorDetection, CopyrightDetection, HolderDetection};

pub fn detect_copyrights(
    content: &str,
    max_runtime: Option<Duration>,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    if let Some(max_runtime) = max_runtime {
        detector::detect_copyrights_from_text_with_deadline(content, Some(max_runtime))
    } else {
        detector::detect_copyrights_from_text(content)
    }
}
