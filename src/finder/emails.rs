use regex::Regex;
use std::sync::LazyLock;

use crate::models::LineNumber;

use super::DetectionConfig;
use super::host::is_good_email_domain;
use super::junk_data::classify_email;

#[derive(Debug, Clone, PartialEq)]
pub struct EmailDetection {
    pub email: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

static EMAILS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z0-9._%-]+@[A-Z0-9.-]+\.[A-Z]{2,63}\b").expect("valid email regex")
});

pub fn find_emails(text: &str, config: &DetectionConfig) -> Vec<EmailDetection> {
    let mut detections = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = LineNumber::from_0_indexed(line_index);
        for matched in EMAILS_REGEX.find_iter(line) {
            let email = matched.as_str().to_lowercase();
            if !is_good_email_domain(&email) {
                continue;
            }
            if !classify_email(&email) {
                continue;
            }

            detections.push(EmailDetection {
                email,
                start_line: line_number,
                end_line: line_number,
            });
        }
    }

    let mut detections = if config.unique {
        let mut seen = std::collections::HashSet::<String>::new();
        detections
            .into_iter()
            .filter(|d| seen.insert(d.email.clone()))
            .collect::<Vec<_>>()
    } else {
        detections
    };

    if config.max_emails > 0 && detections.len() > config.max_emails {
        detections.truncate(config.max_emails);
    }

    detections
}
