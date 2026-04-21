// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io;
use std::path::Path;

use crate::utils::file::extract_text_for_detection;

#[allow(dead_code)] // used by golden tests and update_copyright_golden.rs
pub fn canonicalize_golden_value(s: &str) -> String {
    let s = s
        .replace(". ,", ".,")
        .replace(") ,", "),")
        .replace(" ,", ",");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[allow(dead_code)] // used by golden tests and update_copyright_golden.rs
pub fn read_input_content(input_path: &Path) -> io::Result<String> {
    let bytes = fs::read(input_path)?;
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    let skip_entirely = matches!(ext.as_deref(), Some("mp4") | Some("rgb"));
    if skip_entirely {
        return Ok(String::new());
    }

    let (text, _) = extract_text_for_detection(input_path, &bytes);
    Ok(text)
}
