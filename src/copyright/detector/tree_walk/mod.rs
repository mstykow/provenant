// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
#[path = "../tree_walk_test.rs"]
mod tests;

mod author;
mod copyright;

pub use author::*;
pub use copyright::*;
