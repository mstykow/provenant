//! Common license-specific word dictionary (legalese).
//!
//! This module defines legalese tokens - common words specific to licenses
//! that are high-value for license detection. These words get lower token IDs,
//! making them more significant during matching.
//!
//! **IMPORTANT**: This dictionary is ported from the Python reference at
//! `reference/scancode-toolkit/src/licensedcode/legalese.py`.
//!
//! The Python reference contains 4506 words (including spelling variants and
//! typos that map to the same token IDs). Multiple words can map to the same
//! token ID when they are considered equivalent.
//!
//! The data is generated at build time by `build.rs` from
//! `resources/license_detection/legalese_data.txt`, serialized as an rkyv
//! `BTreeMap<String, u16>` artifact, and loaded via `include_bytes!` for
//! zero-copy access. Values are bare `u16` rather than `TokenId` because
//! `build.rs` cannot depend on the main crate's types; the caller wraps
//! them with `TokenId::new()` at the call site.

use std::collections::BTreeMap;

use rkyv::Archived;

#[repr(C, align(8))]
struct Align8([u8; 8]);

struct AlignedSlice {
    _align: Align8,
    bytes: [u8; LEGALESE_RKYV_LEN],
}

const LEGALESE_RKYV_LEN: usize = {
    const RAW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/legalese.rkyv"));
    RAW.len()
};

static LEGALESE_RKYV: AlignedSlice = {
    const RAW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/legalese.rkyv"));
    let mut bytes = [0u8; LEGALESE_RKYV_LEN];
    let mut i = 0;
    while i < LEGALESE_RKYV_LEN {
        bytes[i] = RAW[i];
        i += 1;
    }
    AlignedSlice {
        _align: Align8([0; 8]),
        bytes,
    }
};

/// Get the archived legalese dictionary for zero-copy iteration.
///
/// Returns a reference to the rkyv-archived `BTreeMap<String, u16>`,
/// which can be iterated directly without intermediate allocations.
/// Values are bare `u16` that get wrapped in `TokenId` at the call site.
pub fn archived_legalese() -> &'static Archived<BTreeMap<String, u16>> {
    rkyv::access::<Archived<BTreeMap<String, u16>>, rkyv::rancor::Error>(&LEGALESE_RKYV.bytes)
        .expect("legalese.rkyv artifact is valid")
}
