use std::collections::HashSet;
use std::io::{self, Write};

use crate::models::{FileInfo, Output};

const COPYRIGHT_FORMAT_URL: &str =
    "https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/";
const DEBIAN_DOCUMENT_NOTICE: &[&str] = &[
    "Generated with Provenant and provided on an \"AS IS\" BASIS, WITHOUT WARRANTIES",
    "OR CONDITIONS OF ANY KIND, either express or implied. No content created from",
    "Provenant should be considered or used as legal advice. Consult an attorney",
    "for legal advice.",
    "Provenant is a free software code scanning tool.",
    "Visit https://github.com/mstykow/provenant/ for support and download.",
];

pub(crate) fn write_debian_copyright(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    writer.write_all(format!("Format: {COPYRIGHT_FORMAT_URL}\n").as_bytes())?;
    write_multiline_field(writer, "Comment", DEBIAN_DOCUMENT_NOTICE)?;
    writer.write_all(b"\n")?;

    let mut files: Vec<_> = output
        .files
        .iter()
        .filter(|file| !matches!(file.file_type, crate::models::FileType::Directory))
        .collect();
    files.sort_by(|left, right| left.path.cmp(&right.path));

    for file in files {
        write_file_paragraph(writer, file)?;
    }

    Ok(())
}

fn write_file_paragraph(writer: &mut dyn Write, file: &FileInfo) -> io::Result<()> {
    writer.write_all(format!("Files: {}\n", file.path).as_bytes())?;

    let copyright_lines: Vec<_> = file
        .holders
        .iter()
        .map(|holder| holder.holder.as_str())
        .collect();
    if !copyright_lines.is_empty() {
        write_multiline_field(writer, "Copyright", &copyright_lines)?;
    }

    if let Some(license_expression) = file.license_expression.as_deref() {
        writer.write_all(format!("License: {license_expression}\n").as_bytes())?;

        let license_texts = unique_license_texts(&file.license_detections);
        for text in license_texts {
            for line in text.lines() {
                if line.is_empty() {
                    writer.write_all(b" .\n")?;
                } else {
                    writer.write_all(format!(" {line}\n").as_bytes())?;
                }
            }
        }
    }

    writer.write_all(b"\n")
}

fn write_multiline_field(writer: &mut dyn Write, key: &str, lines: &[&str]) -> io::Result<()> {
    if let Some((first, rest)) = lines.split_first() {
        writer.write_all(format!("{key}: {first}\n").as_bytes())?;
        let padding = " ".repeat(key.len() + 2);
        for line in rest {
            writer.write_all(format!("{padding}{line}\n").as_bytes())?;
        }
    }

    Ok(())
}

fn unique_license_texts(detections: &[crate::models::LicenseDetection]) -> Vec<&str> {
    let mut seen = HashSet::new();
    let mut texts = Vec::new();

    for detection in detections {
        for match_item in &detection.matches {
            let Some(text) = match_item.matched_text.as_deref() else {
                continue;
            };

            let key = (
                match_item.start_line,
                match_item.end_line,
                match_item.rule_identifier.as_deref().unwrap_or_default(),
            );

            if seen.insert(key) {
                texts.push(text);
            }
        }
    }

    texts
}

#[cfg(test)]
mod tests {
    use super::unique_license_texts;
    use crate::models::{LicenseDetection, Match};

    #[test]
    fn unique_license_texts_deduplicates_by_region_and_rule() {
        let detections = vec![LicenseDetection {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            matches: vec![
                Match {
                    license_expression: "mit".to_string(),
                    license_expression_spdx: "MIT".to_string(),
                    from_file: Some("src/lib.rs".to_string()),
                    start_line: 1,
                    end_line: 3,
                    matcher: Some("1-hash".to_string()),
                    score: 100.0,
                    matched_length: Some(3),
                    match_coverage: Some(100.0),
                    rule_relevance: Some(100),
                    rule_identifier: Some("mit_1.RULE".to_string()),
                    rule_url: None,
                    matched_text: Some("MIT text".to_string()),
                    referenced_filenames: None,
                    matched_text_diagnostics: None,
                },
                Match {
                    license_expression: "mit".to_string(),
                    license_expression_spdx: "MIT".to_string(),
                    from_file: Some("src/lib.rs".to_string()),
                    start_line: 1,
                    end_line: 3,
                    matcher: Some("2-aho".to_string()),
                    score: 100.0,
                    matched_length: Some(3),
                    match_coverage: Some(100.0),
                    rule_relevance: Some(100),
                    rule_identifier: Some("mit_1.RULE".to_string()),
                    rule_url: None,
                    matched_text: Some("MIT text duplicate".to_string()),
                    referenced_filenames: None,
                    matched_text_diagnostics: None,
                },
            ],
            detection_log: vec![],
            identifier: None,
        }];

        assert_eq!(unique_license_texts(&detections), vec!["MIT text"]);
    }
}
