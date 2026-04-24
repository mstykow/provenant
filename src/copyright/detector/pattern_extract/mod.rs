// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

mod cleanup;
mod extraction;

pub use cleanup::*;
pub use extraction::*;

pub fn is_lppl_license_document(content: &str) -> bool {
    let first = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let first_is_lppl_title = first.eq_ignore_ascii_case("LaTeX Project Public License")
        || first.eq_ignore_ascii_case("The LaTeX Project Public License");
    if !first_is_lppl_title {
        return false;
    }
    content.to_ascii_lowercase().contains("lppl version")
}
