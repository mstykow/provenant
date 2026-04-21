// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

pub(super) fn maybe_expand_copyrighted_by_href_urls<'a>(content: &'a str) -> Cow<'a, str> {
    let lower = content.to_ascii_lowercase();
    if !lower.contains("copyrighted by") || !lower.contains("href=") {
        return Cow::Borrowed(content);
    }
    if lower.contains("<html") || lower.contains("<head") {
        return Cow::Borrowed(content);
    }
    if content.lines().count() > 40 {
        return Cow::Borrowed(content);
    }

    static HREF_HTTP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)\bhref\s*=\s*['\"](?P<url>http://[^'\">\s]+)['\"]\s*/?>?"#).unwrap()
    });

    Cow::Owned(HREF_HTTP_RE.replace_all(content, " ${url} ").into_owned())
}

pub(super) fn normalize_split_angle_bracket_urls<'a>(content: &'a str) -> Cow<'a, str> {
    static SPLIT_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<\s*(?P<url>https?://[^\s>]+)\s*\r?\n\s*(?P<tail>[^\s>]+)\s*>").unwrap()
    });

    if !content.contains('<') || !content.contains('>') {
        return Cow::Borrowed(content);
    }
    if !SPLIT_URL_RE.is_match(content) {
        return Cow::Borrowed(content);
    }

    Cow::Owned(
        SPLIT_URL_RE
            .replace_all(content, "${url} ${tail}")
            .into_owned(),
    )
}
