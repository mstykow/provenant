use crate::finder::{self, DetectionConfig};
use crate::parsers::utils::split_name_email;

pub(super) fn is_binary_string_email_candidate(email: &str) -> bool {
    let Some((local, domain)) = email.rsplit_once('@') else {
        return false;
    };

    if !has_strong_binary_local_part(local) {
        return false;
    }

    has_strong_binary_host_shape(domain)
}

pub(super) fn is_binary_string_url_candidate(url: &str) -> bool {
    let parsed = url::Url::parse(url).ok();
    let Some(parsed) = parsed else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };

    has_strong_binary_host_shape(host) && has_meaningful_binary_url_context(&parsed)
}

pub(super) fn normalize_binary_string_url(url: &str) -> Option<String> {
    let mut parsed = url::Url::parse(url).ok()?;

    if let Some(host) = parsed.host_str() {
        let normalized_host = normalize_binary_url_host(host);
        if normalized_host != host {
            parsed.set_host(Some(&normalized_host)).ok()?;
        }
    }

    let normalized_path = normalize_binary_url_path(parsed.path());
    if normalized_path != parsed.path() {
        parsed.set_path(&normalized_path);
    }

    let normalized = parsed.to_string();
    is_binary_string_url_candidate(&normalized).then_some(normalized)
}

pub(super) fn is_binary_string_author_candidate(author: &str) -> bool {
    let trimmed = author.trim();
    if trimmed.is_empty()
        || !has_sufficient_alphabetic_content(trimmed)
        || has_excessive_at_noise(trimmed)
    {
        return false;
    }

    if trimmed.contains('@') {
        let emails = finder::find_emails(
            trimmed,
            &DetectionConfig {
                max_emails: 4,
                max_urls: 0,
                unique: true,
            },
        );
        if emails.len() > 1 {
            return false;
        }

        if let Some(extracted) = extract_named_author_from_binary_line(trimmed) {
            return !extracted.is_empty();
        }

        let Some(email) = emails.first().map(|d| d.email.as_str()) else {
            return false;
        };
        if !is_binary_string_email_candidate(email) {
            return false;
        }

        let (name, _) = split_name_email(trimmed);
        return name.as_deref().is_some_and(has_binary_name_like_shape);
    }

    has_binary_name_like_shape(trimmed)
}

pub(super) fn extract_named_author_from_binary_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let emails = finder::find_emails(
        line,
        &DetectionConfig {
            max_emails: 4,
            max_urls: 0,
            unique: false,
        },
    );
    let email = emails.first()?.email.as_str();
    if !is_binary_string_email_candidate(email) {
        return None;
    }

    let lower_line = line.to_ascii_lowercase();
    let email_start = lower_line.find(email)?;
    let raw_prefix = &line[..email_start];
    let has_author_marker = contains_binary_author_marker(raw_prefix);
    let prefix = take_suffix_after_last_author_marker(raw_prefix)?;
    let prefix = prefix
        .trim_start_matches(['*', '-', ':', ';', ',', '.', ' '])
        .trim_end_matches(['<', '(', '[', ' ', ':', '-'])
        .trim();

    let (name, _) = split_name_email(prefix);
    let name = name.or_else(|| {
        let trimmed = prefix.trim_matches(|c: char| c == '<' || c == '(' || c == '[' || c == ' ');
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });

    let Some(name) = name.map(|name| name.trim().to_string()) else {
        if has_author_marker {
            return Some(email.to_string());
        }
        return None;
    };

    if name.is_empty() && has_author_marker {
        return Some(email.to_string());
    }

    if !has_binary_name_like_shape(&name) {
        return None;
    }

    if line.contains(&format!("<{email}>")) {
        Some(format!("{name} <{email}>"))
    } else if line.contains(&format!("({email})")) {
        Some(format!("{name} ({email})"))
    } else {
        Some(format!("{name} {email}"))
    }
}

pub(super) fn has_binary_name_like_shape(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains(" - ") || trimmed.chars().any(|c| c.is_ascii_digit())
    {
        return false;
    }

    let tokens: Vec<&str> = trimmed
        .split(|c: char| !c.is_ascii_alphabetic() && c != '.' && c != '\'')
        .filter(|segment| segment.chars().any(|c| c.is_ascii_alphabetic()))
        .collect();
    if tokens.is_empty() {
        return false;
    }

    let uppercase_like = tokens
        .iter()
        .filter(|token| {
            let token = token.trim_matches('.');
            token
                .chars()
                .find(|c| c.is_ascii_alphabetic())
                .is_some_and(|c| c.is_ascii_uppercase())
        })
        .count();

    uppercase_like >= 2 && uppercase_like * 2 >= tokens.len()
        || tokens
            .iter()
            .any(|token| is_company_like_suffix(token.trim_matches(|c: char| !c.is_alphanumeric())))
}

pub(super) fn has_sufficient_alphabetic_content(text: &str) -> bool {
    let alnum_count = text.chars().filter(|c| c.is_ascii_alphanumeric()).count();
    if alnum_count == 0 {
        return false;
    }

    let alpha_count = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    alpha_count * 2 >= alnum_count
}

pub(super) fn has_excessive_at_noise(text: &str) -> bool {
    text.chars().filter(|c| *c == '@').count() >= 3
}

pub(super) fn is_company_like_suffix(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "inc"
            | "corp"
            | "corporation"
            | "co"
            | "company"
            | "ltd"
            | "llc"
            | "gmbh"
            | "foundation"
            | "project"
            | "systems"
            | "software"
            | "technologies"
            | "technology"
    )
}

fn take_suffix_after_last_ascii_marker<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let lower = text.to_ascii_lowercase();
    let idx = lower.rfind(marker)?;
    Some(text[idx + marker.len()..].trim())
}

fn take_suffix_after_last_author_marker(text: &str) -> Option<&str> {
    const MARKERS: &[&str] = &[
        " patch author: ",
        " patch author ",
        " written by ",
        " contributed by ",
        " original work done by ",
        " work done by ",
        " thanks to ",
        " review by ",
        " by ",
        " from ",
    ];

    MARKERS
        .iter()
        .filter_map(|marker| take_suffix_after_last_ascii_marker(text, marker))
        .next()
}

fn contains_binary_author_marker(text: &str) -> bool {
    take_suffix_after_last_author_marker(text).is_some()
}

fn has_meaningful_binary_url_context(parsed: &url::Url) -> bool {
    if parsed.path() != "/"
        && parsed
            .path()
            .split('/')
            .any(|segment| segment.chars().any(|c| c.is_ascii_alphabetic()) && segment.len() >= 2)
    {
        return true;
    }

    if parsed.query().is_some() || parsed.fragment().is_some() {
        return true;
    }

    let Some(host) = parsed.host_str() else {
        return false;
    };

    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 2 {
        return labels[..labels.len() - 1].iter().any(|label| {
            label.len() >= 3 && label.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
        });
    }

    if matches!(labels.first(), Some(&"www")) {
        return true;
    }

    if labels.len() == 2 {
        let domain = labels[0];
        let tld = labels[1];
        if domain.len() >= 8 && matches!(tld, "org" | "edu" | "gov" | "mil" | "io" | "dev") {
            return true;
        }
    }

    labels
        .iter()
        .take(labels.len().saturating_sub(1))
        .any(|label| {
            label.contains('-') && label.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 4
        })
}

fn has_strong_binary_local_part(local: &str) -> bool {
    local
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|segment| segment.len() >= 3)
}

fn has_strong_binary_host_shape(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }

    let relevant = if matches!(labels.first(), Some(&"www" | &"ftp")) {
        &labels[1..]
    } else {
        &labels[..]
    };

    if relevant.len() < 2 {
        return false;
    }

    relevant[..relevant.len() - 1].iter().any(|label| {
        label.len() >= 3 && label.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
    })
}

fn normalize_binary_url_host(host: &str) -> String {
    let mut labels = host.split('.').map(ToOwned::to_owned).collect::<Vec<_>>();
    if let Some(last_label) = labels.last_mut() {
        *last_label = trim_binary_tld_tail(last_label);
    }
    labels.join(".")
}

fn trim_binary_tld_tail(label: &str) -> String {
    const KNOWN_TLDS: &[&str] = &["com", "org", "net", "edu", "gov", "mil", "io", "dev"];
    for tld in KNOWN_TLDS {
        let Some(suffix) = label.get(tld.len()..) else {
            continue;
        };
        if label.len() > tld.len()
            && label[..tld.len()].eq_ignore_ascii_case(tld)
            && suffix.starts_with(|ch: char| ch.is_ascii_digit())
            && suffix.len() <= 3
            && suffix
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '!' | '$'))
        {
            return (*tld).to_string();
        }
    }
    label.to_string()
}

fn normalize_binary_url_path(path: &str) -> String {
    let mut chars = path.chars().rev();
    let Some(last) = chars.next() else {
        return path.to_string();
    };
    let Some(prev) = chars.next() else {
        return path.to_string();
    };
    if matches!(last, '_' | '!' | '$') && prev.is_ascii_digit() {
        path[..path.len() - last.len_utf8()].to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
#[path = "binary_text_test.rs"]
mod tests;
