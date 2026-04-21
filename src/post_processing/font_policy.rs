// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

const FONT_ASSET_EXTENSIONS: &[&str] = &["ttf", "otf", "woff", "woff2", "eot"];

pub(super) fn is_font_asset_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            FONT_ASSET_EXTENSIONS
                .iter()
                .any(|supported| ext.eq_ignore_ascii_case(supported))
        })
}

pub(super) fn is_font_license_file_name(file_name: &str, base_name: &str) -> bool {
    matches!(
        file_name.to_ascii_lowercase().as_str(),
        "ofl.txt" | "ofl-1.1.txt" | "ufl.txt"
    ) || matches!(
        base_name.to_ascii_lowercase().as_str(),
        "ofl" | "ofl-1.1" | "ufl"
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_font_asset_path, is_font_license_file_name};

    #[test]
    fn matches_font_asset_extensions_used_for_sidecar_inheritance() {
        assert!(is_font_asset_path(Path::new("demo.ttf")));
        assert!(is_font_asset_path(Path::new("demo.woff2")));
        assert!(is_font_asset_path(Path::new("demo.EOT")));
        assert!(!is_font_asset_path(Path::new("demo.txt")));
    }

    #[test]
    fn matches_font_license_sidecar_names() {
        assert!(is_font_license_file_name("OFL.txt", "OFL"));
        assert!(is_font_license_file_name("license", "ufl"));
        assert!(!is_font_license_file_name("LICENSE.txt", "LICENSE"));
    }
}
