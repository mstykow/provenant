// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

/// Refine a name string (shared logic for holders and authors).
pub(super) fn refine_names(s: &str, prefixes: &HashSet<&str>) -> String {
    let mut r = strip_some_punct(s);
    r = strip_leading_numbers(&r);
    r = strip_all_unbalanced_parens(&r);
    r = strip_some_punct(&r);
    r = r.trim().to_string();
    r = strip_balanced_edge_parens(&r).to_string();
    r = r.trim().to_string();
    r = strip_prefixes(&r, prefixes);
    r = strip_some_punct(&r);
    r = r.trim().to_string();
    r
}

// ─── Helper / utility functions ──────────────────────────────────────────────

/// Normalize whitespace: collapse runs of whitespace to single spaces.
pub(super) fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn normalize_comma_spacing(s: &str) -> String {
    static MULTI_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",{2,}").unwrap());

    let out = s
        .replace(". ,", ".,")
        .replace(") ,", "),")
        .replace(" ,", ",");
    MULTI_COMMA_RE.replace_all(&out, ",").into_owned()
}

/// Remove duplicate/variant copyright words and normalize them.
pub fn remove_dupe_copyright_words(c: &str) -> String {
    let mut c = c.to_string();
    c = c.replace("SPDX-FileCopyrightText", "Copyright");
    c = c.replace("SPDX-SnippetCopyrightText", "Copyright");
    c = c.replace("Bundle-Copyright", "Copyright");
    c = c.replace("AssemblyCopyright", "Copyright");
    c = c.replace("AppCopyright", "Copyright");
    c = c.replace("Cppyright", "Copyright");
    c = c.replace("cppyright", "Copyright");

    // Various prefix to the word copyright seen in binaries.
    static BINARY_COPYRIGHT_PREFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b[a-z]'?Copyright").unwrap());
    c = BINARY_COPYRIGHT_PREFIX_RE
        .replace_all(&c, "Copyright")
        .into_owned();

    for prefix in &["B", "E", "F", "J", "M", "m", "r", "V"] {
        let from = format!("{prefix}Copyright");
        c = c.replace(&from, "Copyright");
    }
    c = c.replace("JCOPYRIGHT", "Copyright");

    // Duplicate copyright words from markup artifacts.
    c = c.replace("COPYRIGHT Copyright", "Copyright");
    c = c.replace("Copyright Copyright", "Copyright");
    c = c.replace("Copyright copyright", "Copyright");
    c = c.replace("copyright copyright", "Copyright");
    c = c.replace("copyright Copyright", "Copyright");
    c = c.replace("copyright'Copyright", "Copyright");
    c = c.replace("copyright\"Copyright", "Copyright");
    c = c.replace("copyright' Copyright", "Copyright");
    c = c.replace("copyright\" Copyright", "Copyright");
    c = c.replace("Copyright @copyright", "Copyright");
    c = c.replace("copyright @copyright", "Copyright");

    // Broken copyright words.
    c = c.replace("(c) opyrighted", "Copyright (c)");
    c = c.replace("(c) opyrights", "Copyright (c)");
    c = c.replace("(c) opyright", "Copyright (c)");
    c = c.replace("(c) opyleft", "Copyleft (c)");
    c = c.replace("(c) opylefted", "Copyleft (c)");
    c = c.replace("copyright'", "Copyright");
    c = c.replace("and later", " ");
    c = c.replace("build.year", " ");
    c
}

/// Remove miscellaneous junk words and punctuation.
pub fn remove_some_extra_words_and_punct(c: &str) -> String {
    static VCS_KEYWORD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\$(?:id|revision|author|date)(?::[^$]*)?\$?").unwrap());

    let mut c = c.to_string();
    c = c.replace("<p>", " ");
    c = c.replace("<a href", " ");
    c = c.replace("date-of-software", " ");
    c = c.replace("date-of-document", " ");
    c = c.replace(" $ ", " ");
    c = c.replace(" ? ", " ");
    c = c.replace("</a>", " ");
    c = c.replace("( )", " ");
    c = c.replace("()", " ");
    c = c.replace("__", " ");
    c = c.replace("--", "-");
    c = c.replace(".com'", ".com");
    c = c.replace(".org'", ".org");
    c = c.replace(".net'", ".net");
    c = c.replace("mailto:", "");
    c = c.replace("@see", "");

    c = VCS_KEYWORD_RE.replace_all(&c, " ").to_string();

    let lower = c.to_lowercase();
    if let Some(start) = lower.find("(see authors")
        && let Some(end) = lower[start..].find(')')
    {
        c.replace_range(start..start + end + 1, " ");
    }
    c.trim().to_string()
}

pub(super) fn strip_trailing_incomplete_as_represented_by(s: &str) -> String {
    static TRAILING_AS_REPRESENTED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+as\s+represented\s+by\s*(?:\.|,)?\s*$").unwrap());

    let trimmed = s.trim();
    if !TRAILING_AS_REPRESENTED_RE.is_match(trimmed) {
        return s.to_string();
    }

    TRAILING_AS_REPRESENTED_RE
        .replace(trimmed, "")
        .trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string()
}

/// Strip leading words that match any of the given prefixes (case-insensitive).
pub fn strip_prefixes(s: &str, prefixes: &HashSet<&str>) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut start = 0usize;
    while start < words.len() && prefixes.contains(words[start].to_lowercase().as_str()) {
        start += 1;
    }
    words[start..].join(" ")
}

/// Strip trailing words that match any of the given suffixes (case-insensitive).
pub fn strip_suffixes(s: &str, suffixes: &HashSet<&str>) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut end = words.len();
    while end > 0 && suffixes.contains(words[end - 1].to_lowercase().as_str()) {
        end -= 1;
    }
    words[..end].join(" ")
}

/// Strip trailing period, preserving it for acronyms and company suffixes.
pub fn strip_trailing_period(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() || !s.ends_with('.') {
        return s.to_string();
    }
    // Keep periods for very short strings (acronyms like "P.").
    if s.len() < 3 {
        return s.to_string();
    }

    let is_single_word = s.split_whitespace().count() == 1;
    let bytes = s.as_bytes();

    // U.S.A., e.V., M.I.T. — second-to-last char is uppercase and multi-word.
    if bytes[bytes.len() - 2].is_ascii_uppercase() && !is_single_word {
        return s.to_string();
    }

    // S.A., e.v., b.v. — third-to-last char is a period.
    if bytes.len() >= 3 && bytes[bytes.len() - 3] == b'.' {
        return s.to_string();
    }

    // Company suffixes.
    let lower = s.to_lowercase();
    if lower.ends_with("inc.")
        || lower.ends_with("corp.")
        || lower.ends_with("ltd.")
        || lower.ends_with("llc.")
        || lower.ends_with("co.")
        || lower.ends_with("llp.")
    {
        return s.to_string();
    }

    s.trim_end_matches('.').to_string()
}

/// Strip leading words that are purely digits.
pub fn strip_leading_numbers(s: &str) -> String {
    let mut words: Vec<&str> = s.split_whitespace().collect();
    while let Some(first) = words.first() {
        if first.contains('$') {
            break;
        }
        if first.contains('?') {
            break;
        }
        let trimmed = first.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
        if trimmed.is_empty() {
            words.remove(0);
            continue;
        }
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            words.remove(0);
            continue;
        }
        break;
    }
    words.join(" ")
}

/// Strip some leading and trailing punctuation.
pub fn strip_some_punct(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    let s = s.trim_matches(&[',', '\'', '"', '}', '{', '-', '_', ':', ';', '&', '@', '!'][..]);
    let s = s.trim_start_matches(&['.', '>', ')', ']', '\\', '/'][..]);
    let is_urlish = (s.contains("http://") || s.contains("https://") || s.contains("ftp://"))
        && s.ends_with('/');
    let end_trim: &[char] = if is_urlish {
        &['<', '(', '[', '\\']
    } else {
        &['<', '(', '[', '\\', '/']
    };
    let s = s.trim_end_matches(end_trim);
    s.to_string()
}

/// Replace unbalanced parentheses with spaces for a given pair of delimiters.
pub fn strip_unbalanced_parens(s: &str, open: char, close: char) -> String {
    if !s.contains(open) && !s.contains(close) {
        return s.to_string();
    }

    let mut stack: Vec<usize> = Vec::new();
    let mut unbalanced: Vec<usize> = Vec::new();

    for (i, ch) in s.chars().enumerate() {
        if ch == open {
            stack.push(i);
        } else if ch == close && stack.pop().is_none() {
            unbalanced.push(i);
        }
    }
    // Remaining opens are unbalanced.
    unbalanced.extend(stack);

    if unbalanced.is_empty() {
        return s.to_string();
    }

    let positions: HashSet<usize> = unbalanced.into_iter().collect();
    s.chars()
        .enumerate()
        .map(|(i, c)| if positions.contains(&i) { ' ' } else { c })
        .collect()
}

/// Strip all unbalanced parentheses for (), <>, [], {}.
pub fn strip_all_unbalanced_parens(s: &str) -> String {
    let mut c = strip_unbalanced_parens(s, '(', ')');
    c = strip_unbalanced_parens(&c, '<', '>');
    c = strip_unbalanced_parens(&c, '[', ']');
    c = strip_unbalanced_parens(&c, '{', '}');
    c
}

/// Strip solo quotes in certain contexts.
pub fn strip_solo_quotes(s: &str) -> String {
    s.replace("/'", "/")
        .replace(")'", ")")
        .replace(":'", ":")
        .replace("':", ":")
        .replace("',", ",")
}

/// Strip trailing URL from a string (e.g., "Acme Corp, https://example.com" → "Acme Corp").
pub(super) fn strip_trailing_url(s: &str) -> String {
    static URL_TOKEN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bhttps?://\S+").unwrap());

    if !(s.contains("http://") || s.contains("https://")) {
        return s.to_string();
    }

    let stripped = URL_TOKEN_RE.replace_all(s, " ").into_owned();
    let stripped = normalize_whitespace(&stripped);
    let stripped = stripped.trim_matches(&[',', ' ', ';'][..]).to_string();

    if stripped.is_empty() {
        s.to_string()
    } else {
        stripped
    }
}

/// Strip trailing slash from URLs at the end of a string.
/// `"FSF http://fsf.org/"` → `"FSF http://fsf.org"`
pub(super) fn strip_trailing_url_slash(s: &str) -> String {
    if s.ends_with('/') && (s.contains("http://") || s.contains("https://")) {
        s.trim_end_matches('/').to_string()
    } else {
        s.to_string()
    }
}

/// Remove duplicated holder strings.
pub(super) fn remove_dupe_holder(h: &str) -> String {
    let mut s = h.replace(
        "the Initial Developer the Initial Developer",
        "the Initial Developer",
    );

    let mut words: Vec<&str> = s.split_whitespace().collect();
    let is_all_caps_word = |w: &str| {
        let mut has_alpha = false;
        for c in w.chars() {
            if c.is_ascii_alphabetic() {
                has_alpha = true;
                if !c.is_ascii_uppercase() {
                    return false;
                }
            }
        }
        has_alpha
    };

    while words.len() >= 2 {
        let last = words[words.len() - 1];
        let prev = words[words.len() - 2];
        if last == prev && is_all_caps_word(last) {
            words.pop();
        } else {
            break;
        }
    }

    if words.len() >= 3 {
        let last = words[words.len() - 1];
        let prev = words[words.len() - 2];
        let is_single_upper = |w: &str| w.len() == 1 && w.chars().all(|c| c.is_ascii_uppercase());
        if is_single_upper(prev) && is_single_upper(last) {
            words.pop();
        }
    }

    s = words.join(" ");
    s
}

/// Drop trailing words longer than 80 characters (garbled/binary data).
pub(super) fn truncate_long_words(s: &str) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut result: Vec<&str> = Vec::new();
    for w in &words {
        if w.len() > 80 {
            break;
        }
        result.push(w);
    }
    result.join(" ")
}
