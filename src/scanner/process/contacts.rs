use super::binary_text::{is_binary_string_email_candidate, normalize_binary_string_url};
use crate::finder::{self, DetectionConfig};
use crate::models::{FileInfoBuilder, OutputEmail, OutputURL};
use crate::scanner::TextDetectionOptions;
use std::collections::HashSet;

pub(super) fn extract_email_url_information(
    file_info_builder: &mut FileInfoBuilder,
    text_content: &str,
    text_options: &TextDetectionOptions,
    from_binary_strings: bool,
) {
    if !text_options.detect_emails && !text_options.detect_urls {
        return;
    }

    if text_options.detect_emails {
        let config = DetectionConfig {
            max_emails: text_options.max_emails,
            max_urls: text_options.max_urls,
            unique: from_binary_strings,
        };
        let emails = finder::find_emails(text_content, &config)
            .into_iter()
            .filter(|d| !from_binary_strings || is_binary_string_email_candidate(&d.email))
            .map(|d| OutputEmail {
                email: d.email,
                start_line: d.start_line,
                end_line: d.end_line,
            })
            .collect::<Vec<_>>();
        file_info_builder.emails(emails);
    }

    if text_options.detect_urls {
        let config = DetectionConfig {
            max_emails: text_options.max_emails,
            max_urls: if from_binary_strings {
                0
            } else {
                text_options.max_urls
            },
            unique: !from_binary_strings,
        };
        let mut urls = finder::find_urls(text_content, &config)
            .into_iter()
            .filter_map(|d| {
                let url = if from_binary_strings {
                    normalize_binary_string_url(&d.url)?
                } else {
                    d.url
                };
                Some(OutputURL {
                    url,
                    start_line: d.start_line,
                    end_line: d.end_line,
                })
            })
            .collect::<Vec<_>>();
        if from_binary_strings {
            let mut seen = HashSet::new();
            urls.retain(|url| seen.insert(url.url.clone()));
            if text_options.max_urls > 0 && urls.len() > text_options.max_urls {
                urls.truncate(text_options.max_urls);
            }
        }
        file_info_builder.urls(urls);
    }
}

#[cfg(test)]
#[path = "tests/contacts.rs"]
mod tests;
