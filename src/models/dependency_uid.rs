// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct DependencyUid(String);

impl DependencyUid {
    /// Creates a new `DependencyUid` by appending a UUID to the given purl.
    pub fn new(purl: &str) -> Self {
        let uuid = Uuid::new_v4();
        if purl.contains('?') {
            DependencyUid(format!("{}&uuid={}", purl, uuid))
        } else {
            DependencyUid(format!("{}?uuid={}", purl, uuid))
        }
    }

    /// Wraps an existing UID string without validation or UUID generation.
    ///
    /// Use this for deserialization boundaries and round-trip conversions
    /// where the UID string is already well-formed.
    pub fn from_raw(s: String) -> Self {
        DependencyUid(s)
    }

    /// Returns the empty-string sentinel representing "no purl".
    pub fn empty() -> Self {
        DependencyUid(String::new())
    }

    /// Returns a new `DependencyUid` with the purl base replaced, preserving the UUID suffix.
    pub fn replace_base(&self, new_purl: &str) -> Self {
        if let Some((_, suffix)) = self.0.split_once("?uuid=") {
            return DependencyUid(format!("{}?uuid={}", new_purl, suffix));
        }
        if let Some((_, suffix)) = self.0.split_once("&uuid=") {
            let separator = if new_purl.contains('?') { '&' } else { '?' };
            return DependencyUid(format!("{}{separator}uuid={suffix}", new_purl));
        }
        DependencyUid(self.0.clone())
    }
}

impl fmt::Display for DependencyUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for DependencyUid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for DependencyUid {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Deref for DependencyUid {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
