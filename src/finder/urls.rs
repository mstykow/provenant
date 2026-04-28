// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use regex::Regex;
use std::sync::LazyLock;

use url::Url;

use crate::models::LineNumber;

use super::DetectionConfig;
use super::host::is_good_url_host_domain;
use super::junk_data::classify_url;

#[derive(Debug, Clone, PartialEq)]
pub struct UrlDetection {
    pub url: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
}

static URLS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?ix)
        (
            (?:https?|ftps?|sftp|rsync|ssh|svn|git|hg|https?\+git|https?\+svn|https?\+hg)://[^\s<>\[\]"]+
            |
            (?:www|ftp)\.[^\s<>\[\]"]+
            |
            git\@[^\s<>\[\]"]+:[^\s<>\[\]"]+\.git
        )
        "#,
    )
    .expect("valid url regex")
});

static INVALID_URLS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:https?|ftps?|sftp|rsync|ssh|svn|git|hg|https?\+git|https?\+svn|https?\+hg)://(?:[$%*/_])+$")
        .expect("valid invalid-url regex")
});

const EMPTY_URLS: &[&str] = &["https", "http", "ftp", "www"];

fn is_filterable(url: &str) -> bool {
    !url.starts_with("git@")
}

fn verbatim_crlf_url_cleaner(url: &str) -> String {
    url.to_string()
}

fn end_of_url_cleaner(url: &str) -> String {
    let mut cleaned = if url.ends_with('/') {
        url.to_string()
    } else {
        url.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    };

    for marker in ['\\', '<', '>', '(', ')', '[', ']', '"', '\'', '`', '*'] {
        if let Some((before, _)) = cleaned.split_once(marker) {
            cleaned = before.to_string();
        }
    }

    if let Some((before, after)) = cleaned.split_once('}')
        && !has_unclosed_dollar_template(before)
        && ({
            let trimmed_after =
                after.trim_matches(|c: char| [',', '.', ':', ';', '!', '?'].contains(&c));
            trimmed_after.is_empty()
                || trimmed_after.chars().all(|ch| ch == '}')
                || trimmed_after.starts_with('{')
        })
    {
        cleaned = before.to_string();
    }

    cleaned = trim_trailing_template_openers(&cleaned);

    cleaned
        .trim_end_matches(|c: char| [',', '.', ':', ';', '!', '?'].contains(&c))
        .to_string()
}

fn has_unclosed_dollar_template(url: &str) -> bool {
    url.rfind("${")
        .is_some_and(|idx| !url[idx + 2..].contains('}'))
}

fn trim_trailing_template_openers(url: &str) -> String {
    let mut cleaned = url.to_string();

    for opener in ["${{", "${"] {
        if cleaned.ends_with(opener) {
            cleaned.truncate(cleaned.len() - opener.len());
            cleaned = cleaned.trim_end_matches('/').to_string();
            break;
        }
    }

    cleaned
}

fn add_fake_scheme(url: &str) -> String {
    if is_filterable(url) && !url.contains("://") && !url.contains('@') {
        format!("http://{url}")
    } else {
        url.to_string()
    }
}

fn remove_user_password(url: &str) -> Option<String> {
    if !is_filterable(url) {
        return Some(url.to_string());
    }

    if let Ok(mut parsed) = Url::parse(url) {
        parsed.set_username("").ok()?;
        parsed.set_password(None).ok()?;
        parsed.host_str()?;
        return Some(parsed.to_string());
    }

    strip_manual_userinfo(url)
}

fn strip_manual_userinfo(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let authority_and_rest = &url[scheme_end + 3..];
    let authority_end = authority_and_rest
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_rest.len());
    let authority = &authority_and_rest[..authority_end];
    let at_index = authority.rfind('@')?;

    let mut rebuilt = String::with_capacity(url.len());
    rebuilt.push_str(&url[..scheme_end + 3]);
    rebuilt.push_str(&authority[at_index + 1..]);
    rebuilt.push_str(&authority_and_rest[authority_end..]);
    Some(rebuilt)
}

fn canonical_url(url: &str) -> Option<String> {
    if !is_filterable(url) {
        return Some(url.to_string());
    }
    Some(Url::parse(url).ok()?.to_string())
}

pub fn find_urls(text: &str, config: &DetectionConfig) -> Vec<UrlDetection> {
    let mut detections = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = LineNumber::from_0_indexed(line_index);
        let normalized_line = line.replace("\\r\\n", "\\n").replace("\\r", "\\n");

        for segment in normalized_line.split("\\n") {
            for matched in URLS_REGEX.find_iter(segment) {
                let mut candidate = matched.as_str().to_string();

                candidate = verbatim_crlf_url_cleaner(&candidate);
                candidate = end_of_url_cleaner(&candidate);

                let candidate_lower = candidate.to_ascii_lowercase();
                if candidate.is_empty() || EMPTY_URLS.contains(&candidate_lower.as_str()) {
                    continue;
                }

                candidate = add_fake_scheme(&candidate);

                let Some(candidate) = remove_user_password(&candidate) else {
                    continue;
                };
                if INVALID_URLS_PATTERN.is_match(&candidate) {
                    continue;
                }

                let Some(candidate) = canonical_url(&candidate) else {
                    continue;
                };

                if is_filterable(&candidate) && !is_good_url_host_domain(&candidate) {
                    continue;
                }
                if !classify_url(&candidate.to_ascii_lowercase()) {
                    continue;
                }

                detections.push(UrlDetection {
                    url: candidate,
                    start_line: line_number,
                    end_line: line_number,
                });
            }
        }
    }

    let mut detections = if config.unique {
        let mut seen = std::collections::HashSet::<String>::new();
        detections
            .into_iter()
            .filter(|d| seen.insert(d.url.clone()))
            .collect::<Vec<_>>()
    } else {
        detections
    };

    if config.max_urls > 0 && detections.len() > config.max_urls {
        detections.truncate(config.max_urls);
    }

    detections
}
