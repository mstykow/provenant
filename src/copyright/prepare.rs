//! Text preparation and normalization for copyright detection.
//!
//! Normalizes raw text lines before copyright detection:
//! - Copyright symbol normalization (©, (C), &#169;, etc. → (c))
//! - HTML entity decoding (&amp;, &lt;, &gt;, etc.)
//! - Comment marker removal (/*, */, #, etc.)
//! - Markup stripping (Debian <s></s>, HTML tags)
//! - Quote normalization (backticks, double quotes → single quotes)
//! - Escape handling (\t, \n → spaces)
//! - Punctuation cleanup
//! - Emdash normalization (– → -)
//! - Placeholder removal (<year>, <name>, etc.)

use std::sync::LazyLock;

use regex::Regex;

fn normalize_replacement_chars(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, ch) in chars.iter().enumerate() {
        if *ch == '\u{FFFD}' {
            let prev_is_letter = i
                .checked_sub(1)
                .and_then(|j| chars.get(j))
                .is_some_and(|c| c.is_alphabetic());
            let next_is_letter = chars.get(i + 1).is_some_and(|c| c.is_alphabetic());
            if prev_is_letter && next_is_letter {
                out.push(*ch);
            } else {
                out.push(' ');
            }
        } else {
            out.push(*ch);
        }
    }
    out
}

/// Regex to remove C-style printf format codes like ` %s ` or ` #d `.
static PRINTF_FORMAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" [#%][a-zA-Z] ").unwrap());

/// Regex to remove punctuation characters: `*#"%[]{}` and backtick.
static PUNCTUATION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[*#"%\[\]{}`]+"#).unwrap());

/// Regex to fold consecutive quotes (2+ single quotes → one).
static CONSECUTIVE_QUOTES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'{2,}").unwrap());

/// Regex to remove less common comment markers: `rem`, `@rem`, `dnl` at line start.
static WEIRD_COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(@?rem|dnl)\s+").unwrap());

static LEADING_DOUBLE_DASH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*--+\s*").unwrap());

/// Regex to remove man page comment markers: `."`.
static MAN_COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\.\""#).unwrap());

/// Regex to match angle-bracketed content (excluding email addresses with `@`).
static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>@]+>").unwrap());

/// Regex to strip known HTML tags even without a closing `>`.
/// Covers the most common HTML tags that appear in source files.
/// Python's `split_on_tags` uses `< */? *[a-z]+[a-z0-9@\-\._\+]* */? *>?` which
/// makes `>` optional, allowing malformed tags like `<b `, `<div `, `</a ` to be stripped.
static HTML_TAG_MALFORMED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)<\s*/?\s*(?:a|abbr|address|area|article|aside|audio|b|base|bdi|bdo|blockquote|body|br|button|canvas|caption|cite|code|col|colgroup|data|datalist|dd|del|details|dfn|dialog|div|dl|dt|em|embed|fieldset|figcaption|figure|font|footer|form|h[1-6]|head|header|hgroup|hr|html|i|iframe|img|input|ins|kbd|label|legend|li|link|main|map|mark|menu|meta|meter|nav|noscript|object|ol|optgroup|option|output|p|param|picture|pre|progress|q|rp|rt|ruby|s|samp|script|section|select|slot|small|source|span|strong|style|sub|summary|sup|table|tbody|td|template|textarea|tfoot|th|thead|time|title|tr|track|u|ul|var|video|wbr)\b\s*/?\s*>?",
    )
    .unwrap()
});

static ONE_LETTER_ANGLE_EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<\s*([A-Za-z])@([^>\s]+)\s*>").unwrap());

static ANGLE_EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<\s*(?P<email>[^>\s]*@[^>\s]+)\s*>").unwrap());

static MSO_O_TEMPLATE_TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bo:template\b").unwrap());

static MSO_TEMPLATE_ELEMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<o:template>.*?</o:template>").unwrap());

static MSO_LASTAUTHOR_ELEMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<o:lastauthor>.*?</o:lastauthor>").unwrap());

/// Regex to strip HTML attribute tokens that leak into copyright text.
/// Python's `SKIP_ATTRIBUTES` skips tokens starting with `href=`, `class=`, etc.
static HTML_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:href|class|width|style|xmlns|xml|lang|type|rel|src|alt|id|name|action|method|target|value|placeholder)=[^\s>]*",
    )
    .unwrap()
});

static MAILTO_ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s+href=['\"]mailto:([^'\"]+)['\"]\s*>\s*([^<]+?)\s*</a>"#).unwrap()
});

static EMAIL_ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s+href=['\"]([^'\"]+@[^'\"]+\.[^'\"]+)['\"][^>]*>\s*([^<]+?)\s*</a>"#)
        .unwrap()
});

static HTTP_ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s+href=['\"](https?://[^'\"]+)['\"][^>]*>\s*([^<]+?)\s*</a>"#).unwrap()
});

static TAG_VALUE_ATTR_DQ_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?is)\bvalue\s*=\s*\"([^\"]+)\""#).unwrap());

static TAG_VALUE_ATTR_SQ_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)\bvalue\s*=\s*'([^']+)'").unwrap());

static MAILTO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)mailto:([^\s\"'>]+)"#).unwrap());

static ANGLE_BRACKET_MARKDOWN_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<\[(?P<text>[^\]]+)\]\((?P<url>[^)]+)\)>").unwrap());

static ANGLE_BRACKET_SINGLE_YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<\s*(?P<year>\d{4})\s*>").unwrap());

/// Regex to strip CSS measurement artifacts like "0pt" that leak through HTML demarkup.
static CSS_MEASUREMENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d+pt\b").unwrap());

static JOIN_REGISTERED_MARK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?P<head>[A-Za-z0-9])\s+\(r\)").unwrap());

static REGISTERED_SIGN_AFTER_ASCII_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?P<head>[A-Za-z0-9])(?:®|\u{00AE})").unwrap());

static LEADING_JAVADOC_AT_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*@(?:remark|file|brief|details|since|version)\b[:\s]*").unwrap()
});

static ESCAPED_ANGLE_EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&lt;\s*(?P<email>[^\s&<>]+@[^\s&<>]+)\s*&gt;").unwrap());

/// Mirroring Python's `keep_tag()` logic: strip angle-bracketed content only
/// when it looks like an HTML tag. Preserve years, names, URLs, and
/// copyright/author/legal markers.
fn replace_tags_preserving_copyright(text: &str, re: &Regex) -> String {
    re.replace_all(text, |caps: &regex::Captures| {
        let m = caps.get(0).unwrap().as_str();
        if should_keep_angle_bracket_content(m) {
            m.trim_start_matches('<')
                .trim_end_matches('>')
                .trim_start_matches('/')
                .trim()
                .to_string()
        } else {
            " ".to_string()
        }
    })
    .into_owned()
}

fn should_keep_angle_bracket_content(m: &str) -> bool {
    let inner = m
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_start_matches('/')
        .trim();
    if inner.is_empty() {
        return false;
    }

    let inner_lower = inner.to_ascii_lowercase();
    if inner_lower.starts_with("o:") {
        return false;
    }
    let inner_simple = inner_lower
        .trim()
        .trim_end_matches('/')
        .trim_end()
        .to_string();

    if inner_simple == "copyright" || inner_simple == "author" || inner_simple == "legal" {
        return true;
    }
    // Preserve years/numeric content: <2013>, <2010-2012>
    if inner.as_bytes()[0].is_ascii_digit() {
        return true;
    }

    if inner.contains('=') {
        return false;
    }

    let lower = m.to_ascii_lowercase();
    if inner_simple.contains("copyright")
        && !inner_simple.contains(' ')
        && !inner_simple.contains(':')
        && !inner_simple.starts_with("copyright")
    {
        return false;
    }
    if lower.contains("copyright") || lower.contains("author") || lower.contains("legal") {
        return true;
    }

    // Preserve URLs/domains in angle brackets: <http://...>, <https://...>, <www...>
    let looks_like_url_or_domain = inner_lower.contains("http://")
        || inner_lower.contains("https://")
        || inner_lower.contains("ftp://")
        || inner_lower.starts_with("www.")
        || inner_lower.starts_with("ftp.");

    let looks_like_email = inner.contains('@') && inner.contains('.');

    let looks_like_obfuscated_email = (inner_lower.contains(" at ")
        || inner_lower.contains(" [at] "))
        && (inner_lower.contains(" dot ")
            || inner_lower.contains(" [dot] ")
            || inner_lower
                .rsplit_once(" at ")
                .map(|(_, tail)| tail.contains('.'))
                .unwrap_or(false)
            || inner_lower
                .rsplit_once(" [at] ")
                .map(|(_, tail)| tail.contains('.'))
                .unwrap_or(false));

    if looks_like_url_or_domain || looks_like_email || looks_like_obfuscated_email {
        return true;
    }

    if inner.contains(' ') {
        let inner_lower = inner.to_ascii_lowercase();
        let ends_with_corp_suffix = inner_lower
            .split_whitespace()
            .last()
            .is_some_and(|w| matches!(w.trim_end_matches('.'), "inc" | "ltd" | "llc" | "corp"));

        if !ends_with_corp_suffix {
            let mut words = inner.split_whitespace();
            let word_count = words.clone().count();
            if (2..=3).contains(&word_count) && inner.len() <= 64 {
                let looks_like_name = words.all(|w| {
                    let mut chars = w.chars();
                    let first = chars.next();
                    let first_ok = first.is_some_and(|c| c.is_alphabetic() && c.is_uppercase());
                    first_ok && chars.all(|c| c.is_alphabetic() || matches!(c, '\'' | '-' | '.'))
                });
                if looks_like_name {
                    return true;
                }
            }
        }
    }

    false
}

/// Prepare a text `line` for copyright detection.
///
/// Applies a sequence of normalizations to clean up raw text before
/// copyright/author detection. This mirrors the Python `prepare_text_line()`
/// function from ScanCode Toolkit.
pub fn prepare_text_line(line: &str) -> String {
    let mut s = line.to_string();

    s.retain(|ch| ch != '\0');

    s = normalize_replacement_chars(&s);

    // ── Man page junk removal ──
    if s.contains("\\\\ co") || s.contains("\\ co") || s.contains("(co ") {
        s = s
            .replace("\\\\ co", " ")
            .replace("\\ co", " ")
            .replace("(co ", " ");
    }

    // Remove printf format codes like ` %s ` or ` #d `
    s = PRINTF_FORMAT_RE.replace_all(&s, " ").into_owned();

    // Remove less common comment markers (rem, @rem, dnl)
    s = WEIRD_COMMENT_RE.replace_all(&s, " ").into_owned();

    s = LEADING_DOUBLE_DASH_RE.replace_all(&s, " ").into_owned();

    // Remove man page comment markers: `."` → space
    s = MAN_COMMENT_RE.replace_all(&s, " ").into_owned();

    // Remove C/C++ block comment markers only (not # and % yet — those
    // would destroy HTML entities like &#169; and printf-like patterns
    // that have already been handled above).
    s = s.replace("/*", " ").replace("*/", " ");

    // ── Copyright symbol normalization ──
    // Must happen BEFORE aggressive # and % removal so that HTML numeric
    // entities (&#169;, &#xa9;, etc.) and backslash escapes (\\XA9) are
    // recognized and converted first.
    s = s
        // RST |copy| directive
        .replace("|copy|", " (c) ")
        // Uncommon pipe chars in ASCII art
        .replace('|', " ")
        // Normalize spacing around "Copyright
        .replace("\"Copyright", "\" Copyright")
        // All copyright sign variants → (c)
        .replace("( C)", " (c) ")
        .replace("(C)", " (c) ")
        .replace("(c)", " (c) ")
        .replace("[C]", " (c) ")
        .replace("[c]", " (c) ")
        .replace("( © )", " (c) ")
        .replace("(©)", " (c) ")
        .replace("(© )", " (c) ")
        .replace("( ©)", " (c) ")
        .replace(['©', '\u{00A9}'], " (c) ")
        // HTML entities
        .replace("&copy;", " (c) ")
        .replace("&copy", " (c) ")
        .replace("&#169;", " (c) ")
        .replace("&#xa9;", " (c) ")
        .replace("&#xA9;", " (c) ")
        .replace("&#Xa9;", " (c) ")
        .replace("&#XA9;", " (c) ")
        // Unicode escape forms
        .replace("u00A9", " (c) ")
        .replace("u00a9", " (c) ")
        // Backslash hex escapes
        .replace("\\XA9", " (c) ")
        .replace("\\A9", " (c) ")
        .replace("\\a9", " (c) ")
        .replace("<A9>", " (c) ")
        .replace("XA9;", " (c) ")
        .replace("Xa9;", " (c) ")
        .replace("xA9;", " (c) ")
        .replace("xa9;", " (c) ")
        // \xc2 is UTF-8 prefix for © — remove it
        .replace('\u{00C2}', "")
        .replace("\\xc2", "");
    s = REGISTERED_SIGN_AFTER_ASCII_RE
        .replace_all(&s, "${head} (r) ")
        .into_owned();
    s = s
        .replace("&reg;", " (r) ")
        .replace("&reg", " (r) ")
        .replace("&#174;", " (r) ");

    // ── HTML entity decoding ──
    // Must also happen BEFORE # and % removal for the same reason.

    if s.to_ascii_lowercase().contains("&lt;") && s.contains('@') {
        s = ESCAPED_ANGLE_EMAIL_RE
            .replace_all(&s, "<${email}>")
            .into_owned();
    }

    s = s
        // Emdash
        .replace('\u{2013}', "-")
        // CR/LF entities
        .replace("&#13;&#10;", " ")
        .replace("&#13;", " ")
        .replace("&#10;", " ")
        // Space entities
        .replace("&nbsp;", " ")
        .replace("&nbsp", " ")
        .replace("&ensp;", " ")
        .replace("&emsp;", " ")
        .replace("&thinsp;", " ")
        // Named entities
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&auml;", "ä")
        .replace("&auml", "ä")
        .replace("&Auml;", "Ä")
        .replace("&Auml", "Ä")
        .replace("&ouml;", "ö")
        .replace("&ouml", "ö")
        .replace("&Ouml;", "Ö")
        .replace("&Ouml", "Ö")
        .replace("&uuml;", "ü")
        .replace("&uuml", "ü")
        .replace("&Uuml;", "Ü")
        .replace("&Uuml", "Ü")
        .replace("&szlig;", "ß")
        .replace("&szlig", "ß")
        .replace("&#196;", "Ä")
        .replace("&#214;", "Ö")
        .replace("&#220;", "Ü")
        .replace("&#228;", "ä")
        .replace("&#246;", "ö")
        .replace("&#252;", "ü")
        .replace("&#223;", "ß")
        .replace("&#xC4;", "Ä")
        .replace("&#xD6;", "Ö")
        .replace("&#xDC;", "Ü")
        .replace("&#xE4;", "ä")
        .replace("&#xF6;", "ö")
        .replace("&#xFC;", "ü")
        .replace("&#xDF;", "ß")
        .replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&gt;", ">")
        .replace("&gt", ">")
        .replace("&#62;", ">")
        .replace("&lt;", "<")
        .replace("&lt", "<")
        .replace("&#60;", "<");

    // Now remove remaining code comment markers (*, #, %) and strip edges.
    // HTML entities have already been decoded so # and % are safe to remove.
    s = s.replace(['*', '#', '%'], " ");
    s = s.trim_matches(|c: char| " \\/*#%;".contains(c)).to_string();

    if s.contains('@') {
        s = LEADING_JAVADOC_AT_TAG_RE.replace(&s, " ").into_owned();
    }

    // ── Quote normalization ──
    s = s
        .replace(['`', '"'], "'")
        // Python unicode prefix
        .replace(" u'", " '")
        // Section sign
        .replace('§', " ")
        // Keep http URLs
        .replace("<http", " http")
        // Placeholders
        .replace("<insert ", " ")
        .replace("year>", " ")
        .replace("<year>", " ")
        .replace("<name>", " ");

    // ── Fold consecutive quotes ──
    s = CONSECUTIVE_QUOTES_RE.replace_all(&s, "'").into_owned();

    // ── Escape handling ──
    if s.contains('\\') || s.contains("('") || s.contains("')") || s.contains("],") {
        s = s
            .replace("\\t", " ")
            .replace("\\n", " ")
            .replace("\\r", " ")
            .replace("\\0", " ")
            .replace('\\', " ")
            .replace("('", " ")
            .replace("')", " ")
            .replace("],", " ");
    }

    // ── Debian markup removal ──
    if s.contains("<s") || s.contains("</s>") {
        s = s.replace("</s>", "").replace("<s>", "").replace("<s/>", "");
    }

    s = ANGLE_BRACKET_SINGLE_YEAR_RE
        .replace_all(&s, "${year}")
        .into_owned();

    s = ANGLE_BRACKET_MARKDOWN_LINK_RE
        .replace_all(&s, "${text} (${url})")
        .into_owned();

    s = MAILTO_ANCHOR_RE.replace_all(&s, "$1 $2").into_owned();

    s = EMAIL_ANCHOR_RE.replace_all(&s, "$1 $2").into_owned();

    s = HTTP_ANCHOR_RE.replace_all(&s, "$1 $2").into_owned();

    if s.contains("<o:p>") || s.contains("</o:p>") {
        s = s.replace("<o:p>", " ").replace("</o:p>", " ");
    }

    if s.to_ascii_lowercase().contains("<o:template>") {
        s = MSO_TEMPLATE_ELEMENT_RE.replace_all(&s, " ").into_owned();
    }
    if s.to_ascii_lowercase().contains("<o:lastauthor>") {
        s = MSO_LASTAUTHOR_ELEMENT_RE.replace_all(&s, " ").into_owned();
    }

    if s.to_ascii_lowercase().contains("o:template") {
        s = MSO_O_TEMPLATE_TOKEN_RE.replace_all(&s, " ").into_owned();
    }

    if s.to_ascii_lowercase().contains("value=") {
        let mut extracted: Vec<String> = Vec::new();
        for cap in TAG_VALUE_ATTR_DQ_RE.captures_iter(&s) {
            let v = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let lower = v.to_ascii_lowercase();
            if lower.contains("copyright") || lower.contains("(c)") || lower.contains("today.year")
            {
                extracted.push(v.to_string());
            }
        }
        for cap in TAG_VALUE_ATTR_SQ_RE.captures_iter(&s) {
            let v = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let lower = v.to_ascii_lowercase();
            if lower.contains("copyright") || lower.contains("(c)") || lower.contains("today.year")
            {
                extracted.push(v.to_string());
            }
        }
        if !extracted.is_empty() {
            s.push(' ');
            s.push_str(&extracted.join(" "));
        }
    }

    // ── HTML tag stripping (copyright/author/legal-aware) ──
    const PROTECT_LT: char = '\u{E000}';
    const PROTECT_GT: char = '\u{E001}';
    s = ONE_LETTER_ANGLE_EMAIL_RE
        .replace_all(&s, |caps: &regex::Captures| {
            format!("{PROTECT_LT}{}@{}{PROTECT_GT}", &caps[1], &caps[2])
        })
        .into_owned();

    s = ANGLE_EMAIL_RE
        .replace_all(&s, |caps: &regex::Captures| {
            let email = caps.name("email").map(|m| m.as_str()).unwrap_or("").trim();
            if email.is_empty() {
                " ".to_string()
            } else {
                format!("{PROTECT_LT}{email}{PROTECT_GT}")
            }
        })
        .into_owned();

    s = replace_tags_preserving_copyright(&s, &HTML_TAG_RE);

    // ── Malformed HTML tag stripping (no closing `>` required) ──
    s = replace_tags_preserving_copyright(&s, &HTML_TAG_MALFORMED_RE);

    s = s.replace(PROTECT_LT, "<").replace(PROTECT_GT, ">");

    // ── HTML attribute token removal ──
    s = HTML_ATTR_RE.replace_all(&s, " ").into_owned();
    s = MAILTO_RE.replace_all(&s, "$1").into_owned();

    // ── CSS measurement artifact removal ──
    // Strip CSS measurement units like "0pt" that leak through HTML demarkup
    // (e.g., from `margin:0pt` or `font-size:0pt` in inline styles).
    s = CSS_MEASUREMENT_RE.replace_all(&s, " ").into_owned();

    // ── Punctuation cleanup ──
    s = PUNCTUATION_RE.replace_all(&s, " ").into_owned();

    // ── Space normalization around commas ──
    s = s.replace(" , ", ", ");

    // ── Angle bracket spacing ──
    s = s.replace('>', "> ").replace('<', " <");
    s = JOIN_REGISTERED_MARK_RE
        .replace_all(&s, "${head}(r)")
        .into_owned();

    // ── Strip leading/trailing stars and spaces ──
    s = s.trim_matches(|c: char| c == ' ' || c == '*').to_string();

    // ── Normalize whitespace ──
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_strips_unicode_replacement_char() {
        let prepared = prepare_text_line("Copyright \u{FFFD}1996-1999 Foo");
        assert!(
            !prepared.contains('\u{FFFD}'),
            "Prepared still contains replacement char: {prepared:?}"
        );
        assert!(
            prepared.contains("Copyright")
                && prepared.contains("1996-1999")
                && prepared.contains("Foo"),
            "Unexpected prepared text: {prepared:?}"
        );
    }

    #[test]
    fn test_prepare_preserves_unicode_replacement_char_in_words() {
        let prepared = prepare_text_line("Dag-Erling Co\u{FFFD}dan Sm\u{FFFD}rgrav");
        assert!(
            prepared.contains('\u{FFFD}'),
            "Expected replacement chars preserved: {prepared:?}"
        );
    }

    #[test]
    fn test_prepare_drops_nul_bytes() {
        let prepared = prepare_text_line("C\0o\0p\0y\0r\0i\0g\0h\0t \u{00A9} 2001 Acme");
        assert!(
            prepared.starts_with("Copyright"),
            "Expected NUL-stripped Copyright prefix, got: {prepared:?}"
        );
        assert!(
            prepared.contains("(c)"),
            "Expected (c) symbol, got: {prepared:?}"
        );
    }

    #[test]
    fn test_copyright_symbol_c_upper() {
        let result = prepare_text_line("(C) 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
        assert!(result.contains("2024"));
    }

    #[test]
    fn test_copyright_symbol_c_lower() {
        let result = prepare_text_line("(c) 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_c_spaced() {
        let result = prepare_text_line("( C) 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_unicode() {
        let result = prepare_text_line("© 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_html_entity_named() {
        let result = prepare_text_line("&copy; 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_html_entity_numeric() {
        let result = prepare_text_line("&#169; 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_html_entity_hex() {
        let result = prepare_text_line("&#xA9; 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_copy_without_semicolon() {
        let result = prepare_text_line("&copy 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_rst_copy() {
        let result = prepare_text_line("|copy| 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_u00a9() {
        let result = prepare_text_line("u00A9 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_backslash_xa9() {
        let result = prepare_text_line("\\XA9 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_copyright_symbol_angle_a9() {
        let result = prepare_text_line("<A9> 2024 Acme");
        assert!(result.contains("(c)"), "got: {result}");
    }

    #[test]
    fn test_html_entity_amp() {
        assert_eq!(prepare_text_line("foo &amp; bar"), "foo & bar");
    }

    #[test]
    fn test_prepare_unwraps_mailto_anchor() {
        let prepared = prepare_text_line(
            r#"* @author <a href="mailto:stephane@hillion.org">Stephane Hillion</a>"#,
        );
        assert!(
            prepared.contains("stephane@hillion.org Stephane Hillion"),
            "got: {prepared:?}"
        );
        assert!(!prepared.contains("mailto:"), "got: {prepared:?}");
        assert!(!prepared.contains("<a"), "got: {prepared:?}");
    }

    #[test]
    fn test_prepare_unwraps_http_anchor() {
        let prepared = prepare_text_line(
            r#"&copy; 2009 Google - <a href="http://example.com/privacy.html">Privacy Policy</a>"#,
        );
        assert!(prepared.contains("(c) 2009 Google"), "got: {prepared:?}");
        assert!(
            prepared.contains("http://example.com/privacy.html"),
            "got: {prepared:?}"
        );
        assert!(prepared.contains("Privacy Policy"), "got: {prepared:?}");
        assert!(!prepared.contains("href="), "got: {prepared:?}");
        assert!(!prepared.contains("<a"), "got: {prepared:?}");
    }

    #[test]
    fn test_prepare_email_anchor_expands_to_email_and_text() {
        let prepared = prepare_text_line(r#"@author <a href="dev@example.com">Dev</a>"#);
        assert!(
            prepared.contains("dev@example.com Dev"),
            "got: {prepared:?}"
        );
        assert!(!prepared.contains("href="), "got: {prepared:?}");
        assert!(!prepared.contains("<a"), "got: {prepared:?}");
    }

    #[test]
    fn test_prepare_strips_author_attribute_without_promoting_text() {
        let prepared =
            prepare_text_line(r#"<note author="Vinnie Falco">C++11 is required.</note>"#);
        assert!(
            !prepared.contains("Written by Vinnie Falco"),
            "got: {prepared:?}"
        );
        assert!(!prepared.contains("author="), "got: {prepared:?}");
    }

    #[test]
    fn test_prepare_does_not_promote_plain_text_author_assignment() {
        let prepared = prepare_text_line(r#"config says author="Vinnie Falco" and nothing else"#);
        assert!(
            !prepared.contains("Written by Vinnie Falco"),
            "got: {prepared:?}"
        );
    }

    #[test]
    fn test_prepare_preserves_angle_bracket_name() {
        let result = prepare_text_line("Copyright (c) <2010-2012> <Ciaran Jessup>");
        assert!(
            result.contains("Ciaran Jessup"),
            "Expected name preserved, got: {result:?}"
        );
    }

    #[test]
    fn test_prepare_registered_sign_only_after_ascii() {
        let result = prepare_text_line("W3C® (MIT)");
        assert!(result.contains("W3C(r)"), "got: {result:?}");

        let mojibake = prepare_text_line("Ö®¼ä");
        assert!(!mojibake.contains("(r)"), "got: {mojibake:?}");
    }

    #[test]
    fn test_prepare_preserves_escaped_angle_bracket_email() {
        let result =
            prepare_text_line("Copyright (C) 2006-2008 Jason Evans &lt;jasone@FreeBSD.org&gt;.");
        assert!(
            result.contains("<jasone@FreeBSD.org>"),
            "expected escaped email preserved with <>, got: {result:?}"
        );
    }

    #[test]
    fn test_html_entity_lt_gt() {
        // &lt; and &gt; are decoded to < and >, then < b > is stripped as
        // an HTML tag by the tag-stripping regex. This matches Python behavior.
        let result = prepare_text_line("a &lt;b&gt; c");
        assert!(result.contains("a"), "got: {result}");
        assert!(result.contains("c"), "got: {result}");
    }

    #[test]
    fn test_html_entity_quot() {
        // Quotes get normalized to single quotes, then punctuation removes them
        let result = prepare_text_line("say &quot;hello&quot;");
        assert!(result.contains("say"), "got: {result}");
        assert!(result.contains("hello"), "got: {result}");
    }

    #[test]
    fn test_html_entity_spaces() {
        let result = prepare_text_line("a&ensp;b&emsp;c&thinsp;d");
        assert_eq!(result, "a b c d");
    }

    #[test]
    fn test_html_entity_nbsp() {
        let result = prepare_text_line("a&nbsp;b&nbsp;c");
        assert_eq!(result, "a b c");
    }

    #[test]
    fn test_emdash_normalization() {
        assert_eq!(prepare_text_line("2020\u{2013}2024"), "2020-2024");
    }

    #[test]
    fn test_whitespace_normalization() {
        assert_eq!(prepare_text_line("  foo   bar   baz  "), "foo bar baz");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(prepare_text_line(""), "");
    }

    #[test]
    fn test_debian_markup_removal() {
        let result = prepare_text_line("Copyright <s>Foo</s>");
        assert!(!result.contains("<s>"), "got: {result}");
        assert!(!result.contains("</s>"), "got: {result}");
        assert!(result.contains("Foo"), "got: {result}");
    }

    #[test]
    fn test_debian_markup_self_closing() {
        let result = prepare_text_line("text <s/> more");
        assert!(!result.contains("<s/>"), "got: {result}");
    }

    #[test]
    fn test_comment_marker_c_style() {
        let result = prepare_text_line("/* Copyright 2024 Acme */");
        assert!(result.contains("Copyright"), "got: {result}");
        assert!(result.contains("2024"), "got: {result}");
    }

    #[test]
    fn test_comment_marker_star_prefix() {
        let result = prepare_text_line(" * Copyright 2024 Acme");
        assert!(result.contains("Copyright"), "got: {result}");
        assert!(result.contains("2024"), "got: {result}");
    }

    #[test]
    fn test_comment_marker_hash() {
        let result = prepare_text_line("# Copyright 2024 Acme");
        assert!(result.contains("Copyright"), "got: {result}");
        assert!(result.contains("2024"), "got: {result}");
    }

    #[test]
    fn test_comment_marker_rem() {
        let result = prepare_text_line("rem Copyright 2024 Acme");
        assert!(result.contains("Copyright"), "got: {result}");
    }

    #[test]
    fn test_comment_marker_dnl() {
        let result = prepare_text_line("dnl Copyright 2024 Acme");
        assert!(result.contains("Copyright"), "got: {result}");
    }

    #[test]
    fn test_placeholder_removal_year() {
        let result = prepare_text_line("Copyright <year> Author");
        assert!(!result.contains("<year>"), "got: {result}");
        assert!(result.contains("Author"), "got: {result}");
    }

    #[test]
    fn test_placeholder_removal_name() {
        let result = prepare_text_line("Copyright 2024 <name>");
        assert!(!result.contains("<name>"), "got: {result}");
    }

    #[test]
    fn test_placeholder_http_preserved() {
        let result = prepare_text_line("<http://example.com>");
        assert!(result.contains("http"), "got: {result}");
    }

    #[test]
    fn test_escape_handling_tab() {
        let result = prepare_text_line("foo\\tbar");
        assert!(result.contains("foo"), "got: {result}");
        assert!(result.contains("bar"), "got: {result}");
        assert!(!result.contains("\\t"), "got: {result}");
    }

    #[test]
    fn test_escape_handling_newline() {
        let result = prepare_text_line("foo\\nbar");
        assert!(!result.contains("\\n"), "got: {result}");
    }

    #[test]
    fn test_backslash_removal() {
        let result = prepare_text_line("foo\\bar");
        assert!(!result.contains('\\'), "got: {result}");
    }

    #[test]
    fn test_quote_normalization_backtick() {
        // Backticks become single quotes, then punctuation may remove them
        let result = prepare_text_line("say `hello`");
        assert!(result.contains("say"), "got: {result}");
        assert!(result.contains("hello"), "got: {result}");
    }

    #[test]
    fn test_consecutive_quotes_folded() {
        let result = prepare_text_line("it''s a test");
        // Two single quotes should become one
        assert!(result.contains("it"), "got: {result}");
    }

    #[test]
    fn test_pipe_removal() {
        let result = prepare_text_line("foo | bar");
        assert!(!result.contains('|'), "got: {result}");
    }

    #[test]
    fn test_section_sign_removal() {
        let result = prepare_text_line("Section§3");
        assert!(!result.contains('§'), "got: {result}");
    }

    #[test]
    fn test_html_tag_stripping() {
        let result = prepare_text_line("Copyright <b>2024</b> Acme");
        assert!(!result.contains("<b>"), "got: {result}");
        assert!(!result.contains("</b>"), "got: {result}");
        assert!(result.contains("2024"), "got: {result}");
    }

    #[test]
    fn test_comma_spacing() {
        assert_eq!(prepare_text_line("a , b , c"), "a, b, c");
    }

    #[test]
    fn test_printf_format_codes_removed() {
        let result = prepare_text_line("foo %s bar");
        // %s surrounded by spaces should be removed
        assert_eq!(result, "foo bar");
    }

    #[test]
    fn test_man_page_comment() {
        let result = prepare_text_line(".\" Copyright 2024");
        assert!(result.contains("Copyright"), "got: {result}");
    }

    #[test]
    fn test_combined_normalization() {
        let result = prepare_text_line(" * (C) 2024 Acme &amp; Co.");
        assert!(result.contains("(c)"), "got: {result}");
        assert!(result.contains("2024"), "got: {result}");
        assert!(result.contains("Acme"), "got: {result}");
        assert!(result.contains("& Co."), "got: {result}");
    }

    #[test]
    fn test_complex_line() {
        let result =
            prepare_text_line("/* Copyright &#169; 2020\u{2013}2024 Foo &amp; Bar <name> */");
        assert!(result.contains("(c)"), "got: {result}");
        assert!(result.contains("2020-2024"), "got: {result}");
        assert!(result.contains("Foo"), "got: {result}");
        assert!(result.contains("& Bar"), "got: {result}");
        assert!(!result.contains("<name>"), "got: {result}");
    }

    #[test]
    fn test_man_page_co_junk() {
        let result = prepare_text_line("\\\\ co 2024 Author");
        assert!(result.contains("2024"), "got: {result}");
        assert!(result.contains("Author"), "got: {result}");
    }

    #[test]
    fn test_cr_lf_entities() {
        let result = prepare_text_line("line1&#13;&#10;line2");
        assert_eq!(result, "line1 line2");
    }

    #[test]
    fn test_insert_placeholder() {
        let result = prepare_text_line("<insert your name>");
        assert!(!result.contains("<insert"), "got: {result}");
    }

    #[test]
    fn test_bracket_removal() {
        let result = prepare_text_line("foo [bar] {baz}");
        assert!(!result.contains('['), "got: {result}");
        assert!(!result.contains(']'), "got: {result}");
        assert!(!result.contains('{'), "got: {result}");
        assert!(!result.contains('}'), "got: {result}");
    }

    #[test]
    fn test_only_whitespace() {
        assert_eq!(prepare_text_line("   \t  \n  "), "");
    }

    #[test]
    fn test_passthrough_normal_text() {
        assert_eq!(
            prepare_text_line("Copyright 2024 John Doe"),
            "Copyright 2024 John Doe"
        );
    }

    #[test]
    fn test_unicode_names_preserved() {
        let result = prepare_text_line("Copyright 2024 François Müller");
        assert_eq!(result, "Copyright 2024 François Müller");
    }

    #[test]
    fn test_unicode_spanish_names_preserved() {
        let result = prepare_text_line("Copyright 2024 José García");
        assert_eq!(result, "Copyright 2024 José García");
    }

    #[test]
    fn test_unicode_nordic_names_preserved() {
        let result = prepare_text_line("Copyright 2024 Björn Ångström");
        assert_eq!(result, "Copyright 2024 Björn Ångström");
    }

    #[test]
    fn test_unicode_polish_names_preserved() {
        let result = prepare_text_line("Copyright 2024 Łukasz Żółw");
        assert_eq!(result, "Copyright 2024 Łukasz Żółw");
    }

    // ── Gap 1: Malformed/unclosed HTML tag stripping ──

    #[test]
    fn test_strip_malformed_tag_b_no_closing() {
        let result = prepare_text_line("Copyright <b 2024 Acme");
        assert!(
            !result.contains("<b"),
            "Malformed tag should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
        assert!(
            result.contains("Acme"),
            "Name should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_malformed_tag_div_no_closing() {
        let result = prepare_text_line("Copyright <div 2024 Acme");
        assert!(
            !result.contains("<div"),
            "Malformed tag should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_malformed_closing_tag() {
        let result = prepare_text_line("Copyright </a 2024 Acme");
        assert!(
            !result.contains("</a"),
            "Malformed closing tag should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_malformed_tag_span() {
        let result = prepare_text_line("Copyright <span 2024 Acme");
        assert!(
            !result.contains("<span"),
            "Malformed span should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_malformed_tag_p() {
        let result = prepare_text_line("<p Copyright 2024 Acme");
        assert!(
            !result.contains("<p"),
            "Malformed p tag should be stripped: {result}"
        );
        assert!(
            result.contains("Copyright"),
            "Copyright should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_self_closing_br() {
        let result = prepare_text_line("Copyright 2024<br/>Acme");
        assert!(
            !result.contains("<br"),
            "br tag should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
        assert!(
            result.contains("Acme"),
            "Name should be preserved: {result}"
        );
    }

    // ── Gap 2: HTML attribute token removal ──

    #[test]
    fn test_strip_href_attribute() {
        let result = prepare_text_line("Copyright href=http://example.com 2024 Acme");
        assert!(
            !result.contains("href="),
            "href attribute should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_class_attribute() {
        let result = prepare_text_line("Copyright class=main 2024 Acme");
        assert!(
            !result.contains("class="),
            "class attribute should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_width_attribute() {
        let result = prepare_text_line("Copyright width=100 2024 Acme");
        assert!(
            !result.contains("width="),
            "width attribute should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_xmlns_attribute() {
        let result = prepare_text_line("Copyright xmlns=http://www.w3.org 2024 Acme");
        assert!(
            !result.contains("xmlns="),
            "xmlns attribute should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_mailto() {
        let result = prepare_text_line("Copyright 2024 mailto:john@example.com Acme");
        assert!(
            !result.contains("mailto:"),
            "mailto should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
        assert!(
            result.contains("Acme"),
            "Name should be preserved: {result}"
        );
    }

    #[test]
    fn test_preserve_angle_bracket_email_with_i_prefix() {
        let result = prepare_text_line("Copyright (c) 2024 bgme <i@bgme.me>.");
        assert!(
            result.contains("<i@bgme.me>"),
            "Expected angle-bracket email preserved, got: {result:?}"
        );
        assert!(
            !result.contains(" bgme @bgme.me>"),
            "Did not expect stripped '<i' prefix, got: {result:?}"
        );
    }

    #[test]
    fn test_strip_lang_attribute() {
        let result = prepare_text_line("Copyright lang=en 2024 Acme");
        assert!(
            !result.contains("lang="),
            "lang attribute should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_style_attribute() {
        let result = prepare_text_line("Copyright style=color:red 2024 Acme");
        assert!(
            !result.contains("style="),
            "style attribute should be stripped: {result}"
        );
        assert!(
            result.contains("2024"),
            "Year should be preserved: {result}"
        );
    }

    // ── Gap 3: Preserve copyright/author/legal in angle brackets ──

    #[test]
    fn test_preserve_copyright_in_angle_brackets() {
        let result = prepare_text_line("<copyright notice> 2024 Acme");
        assert!(
            result.contains("copyright"),
            "copyright token should be preserved: {result}"
        );
    }

    #[test]
    fn test_preserve_author_in_angle_brackets() {
        let result = prepare_text_line("<author> John Doe");
        assert!(
            result.contains("author"),
            "author token should be preserved: {result}"
        );
    }

    #[test]
    fn test_preserve_legal_in_angle_brackets() {
        let result = prepare_text_line("<legal> 2024 Acme Corp");
        assert!(
            result.contains("legal"),
            "legal token should be preserved: {result}"
        );
    }

    #[test]
    fn test_strip_regular_tag_but_preserve_copyright_tag() {
        let result = prepare_text_line("<div>Copyright</div> <copyright> 2024");
        assert!(
            result.contains("copyright"),
            "copyright tag should be preserved: {result}"
        );
        assert!(
            !result.contains("<div>"),
            "div tag should be stripped: {result}"
        );
    }

    #[test]
    fn test_strip_o_lastauthor_markup_tag() {
        let result = prepare_text_line("Copyright 2024 Foo <o:LastAuthor>bar</o:LastAuthor>");
        assert!(!result.to_ascii_lowercase().contains("o:lastauthor"));
        assert!(result.contains("Copyright 2024 Foo"));
    }

    #[test]
    fn test_strip_o_lastauthor_element_content() {
        let result = prepare_text_line("<o:LastAuthor>Jennifer Hruska</o:LastAuthor>");
        assert!(!result.to_ascii_lowercase().contains("jennifer"));
        assert!(!result.to_ascii_lowercase().contains("hruska"));
    }

    #[test]
    fn test_strip_o_template_token() {
        let result = prepare_text_line("Copyright 2024 Foo <o:template>");
        assert!(!result.to_ascii_lowercase().contains("o:template"));
    }

    #[test]
    fn test_strip_o_template_element_content() {
        let result = prepare_text_line("<o:Template>techdoc.dot</o:Template>");
        assert!(!result.to_ascii_lowercase().contains("techdoc.dot"));
    }
}

#[test]
fn test_copyright_symbol_square_c() {
    let result = prepare_text_line("[C] The Regents");
    assert!(result.contains("(c)"), "got: {result}");
}
