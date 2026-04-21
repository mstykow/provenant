// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::path::Path;

use ttf_parser::{Face, Permissions, name_id};

const SUPPORTED_FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

pub(crate) fn extract_font_metadata_text(path: &Path, bytes: &[u8]) -> Option<String> {
    let extension = path.extension().and_then(|ext| ext.to_str())?;
    if !SUPPORTED_FONT_EXTENSIONS
        .iter()
        .any(|supported| extension.eq_ignore_ascii_case(supported))
    {
        return None;
    }

    let face = Face::parse(bytes, 0).ok()?;
    let mut lines = Vec::new();
    let mut seen = BTreeSet::new();

    for record in face.names() {
        let Some(label) = font_name_label(record.name_id) else {
            continue;
        };
        if !record.is_unicode() {
            continue;
        }
        let Some(value) = record.to_string().map(normalize_font_value) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }

        let line = format!("{label}: {value}");
        if seen.insert(line.clone()) {
            lines.push(line);
        }
    }

    if let Some(permissions) = face.permissions() {
        let line = format!(
            "Embedding permissions: {}",
            font_permission_label(permissions)
        );
        if seen.insert(line.clone()) {
            lines.push(line);
        }
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn font_name_label(name_id_value: u16) -> Option<&'static str> {
    match name_id_value {
        name_id::COPYRIGHT_NOTICE => Some("Copyright Notice"),
        name_id::TRADEMARK => Some("Trademark"),
        name_id::MANUFACTURER => Some("Manufacturer"),
        name_id::DESCRIPTION => Some("Description"),
        name_id::VENDOR_URL => Some("Vendor URL"),
        name_id::DESIGNER_URL => Some("Designer URL"),
        name_id::LICENSE => Some("License Description"),
        name_id::LICENSE_URL => Some("License Info URL"),
        _ => None,
    }
}

fn normalize_font_value(value: String) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn font_permission_label(permission: Permissions) -> &'static str {
    match permission {
        Permissions::Installable => "Installable",
        Permissions::Restricted => "Restricted",
        Permissions::PreviewAndPrint => "Preview and Print",
        Permissions::Editable => "Editable",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::extract_font_metadata_text;

    #[test]
    fn extracts_ofl_metadata_from_lato_font_fixture() {
        let bytes =
            fs::read("testdata/font-fixtures/Lato-Bold.ttf").expect("read lato font fixture");

        let text = extract_font_metadata_text(Path::new("Lato-Bold.ttf"), &bytes)
            .expect("font metadata text");

        assert!(text.contains("License Description:"), "{text}");
        assert!(
            text.contains("Open Font License") || text.contains("OFL"),
            "{text}"
        );
    }

    #[test]
    fn extracts_apache_metadata_from_underline_test_font_fixture() {
        let bytes = fs::read("testdata/font-fixtures/UnderlineTest-Close.ttf")
            .expect("read apache font fixture");

        let text = extract_font_metadata_text(Path::new("UnderlineTest-Close.ttf"), &bytes)
            .expect("font metadata text");

        assert!(
            text.contains("License Description:") || text.contains("Copyright Notice:"),
            "{text}"
        );
        assert!(
            text.contains("Apache") || text.contains("http://www.apache.org/licenses"),
            "{text}"
        );
    }
}
