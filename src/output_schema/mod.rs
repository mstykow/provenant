//! ScanCode-compatible output schema types.
//!
//! This module will house the serde-facing types that define the stable JSON
//! output contract. Internal domain types from [`crate::models`] are converted
//! into these schema types before serialization, keeping the wire format
//! separate from the internal representation.
