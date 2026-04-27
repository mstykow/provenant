// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Refinement and cleanup functions for detected copyright strings.
//!
//! After the parser produces raw detection text from parse tree nodes,
//! these functions clean up artifacts: strip junk prefixes/suffixes,
//! normalize whitespace, remove duplicate copyright words, strip
//! unbalanced parentheses, and filter out known junk patterns.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::candidates::strip_balanced_edge_parens;
mod authors_junk_patterns;
mod copyrights_junk_patterns;
mod holders_junk_patterns;

use authors_junk_patterns::AUTHORS_JUNK_PATTERNS;
use copyrights_junk_patterns::COPYRIGHTS_JUNK_PATTERNS;
use holders_junk_patterns::HOLDERS_JUNK_PATTERNS;

// ─── Constant sets ───────────────────────────────────────────────────────────

/// Generic prefixes stripped from names (holders/authors).
const PREFIXES: &[&str] = &[
    "?",
    "??",
    "????",
    "(insert",
    "then",
    "current",
    "year)",
    "maintained",
    "by",
    "developed",
    "created",
    "written",
    "recoded",
    "coded",
    "modified",
    // Note: Python has 'maintained''created' (missing comma = concatenation).
    // We include both separately.
    "maintainedcreated",
    "$year",
    "year",
    "uref",
    "owner",
    "from",
    "and",
    "of",
    "to",
    "for",
    "or",
    "<p>",
];

/// Suffixes stripped from copyright strings.
static COPYRIGHTS_SUFFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "copyright",
        ".",
        ",",
        "year",
        "parts",
        "any",
        "0",
        "1",
        "author",
        "all",
        "some",
        "and",
        "</p>",
        "is",
        "-",
        "distributed",
        "information",
        "credited",
        "by",
    ]
    .into_iter()
    .collect()
});

/// Authors prefixes = PREFIXES ∪ author-specific words.
static AUTHORS_PREFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s: HashSet<&str> = PREFIXES.iter().copied().collect();
    for w in &[
        "contributor",
        "contributor(s)",
        "authors",
        "author",
        "authors'",
        "author:",
        "author(s)",
        "authored",
        "created",
        "author.",
        "author'",
        "authors,",
        "authorship",
        "maintainer",
        "co-maintainer",
        "or",
        "spdx-filecontributor",
        "</b>",
        "mailto:",
        "name'",
        "a",
        "moduleauthor",
        "\u{a9}", // ©
    ] {
        s.insert(w);
    }
    s
});

/// Authors junk — detected author strings that are false positives.
static AUTHORS_JUNK: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "james hacker.",
        "james random hacker.",
        "contributor. c. a",
        "grant the u.s. government and others",
        "james random hacker",
        "james hacker",
        "company",
        "contributing project",
        "its author",
        "gnomovision",
        "would",
        "may",
        "attributions",
        "the",
        "app id",
        "project",
        "previous lucene",
        "group",
        "the coordinator",
        "the owner",
        "a group",
        "sonatype nexus",
        "apache tomcat",
        "visual studio",
        "apache maven",
        "visual studio and visual studio",
        "work",
        "additional",
        "builder",
        "chef-client",
        "compatible",
        "guice",
        "incorporated",
        "ds",
        "guide",
        "recommend",
        "recheck",
        "reputations",
        "review",
        "disclaims",
        "liability",
        "required",
        "desired",
        "intended",
        "someone",
        "performing",
        "volunteer",
        "volunteers",
        "donald becker",
    ]
    .into_iter()
    .collect()
});

/// Prefix that triggers ignoring the author entirely.
const AUTHORS_JUNK_PREFIX: &str = "httpProxy";

fn is_junk_author(s: &str) -> bool {
    AUTHORS_JUNK_PATTERNS.iter().any(|re| re.is_match(s))
}

/// Holders prefixes = PREFIXES ∪ holder-specific words.
static HOLDERS_PREFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s: HashSet<&str> = PREFIXES.iter().copied().collect();
    for w in &[
        "-",
        "a",
        "<a",
        "href",
        "ou",
        "portions",
        "portion",
        "notice",
        "holders",
        "holder",
        "property",
        "parts",
        "part",
        "at",
        "cppyright",
        "assemblycopyright",
        "c",
        "works",
        "present",
        "right",
        "rights",
        "reserved",
        "held",
        "is",
        "(x)",
        "later",
        "$",
        "current.year",
        "\u{a9}", // ©
        "author",
        "authors",
    ] {
        s.insert(w);
    }
    s
});

/// Holders prefixes including "all" (used when "reserved" is in the string).
static HOLDERS_PREFIXES_WITH_ALL: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HOLDERS_PREFIXES.clone();
    s.insert("all");
    s
});

/// Suffixes stripped from holder strings.
static HOLDERS_SUFFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "http",
        "and",
        "email",
        "licensing@",
        "(minizip)",
        "website",
        "(c)",
        "<http",
        "/>",
        ".",
        ",",
        "year",
        "some",
        "all",
        "right",
        "rights",
        "reserved",
        "reserved.",
        "href",
        "c",
        "a",
        "</p>",
        "or",
        "taken",
        "from",
        "is",
        "-",
        "distributed",
        "information",
        "credited",
        "$",
    ]
    .into_iter()
    .collect()
});

/// Holders junk — detected holder strings that are false positives.
static HOLDERS_JUNK: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "a href",
        "property",
        "licensing@",
        "c",
        "works",
        "http",
        "the",
        "are",
        "?",
        "cppyright",
        "parts",
        "disclaimed",
        "or",
        "<holders>",
        "author",
        // License boilerplate false positives
        "holders",
        "holder",
        "holder,",
        "and/or",
        "if",
        "grant",
        "notice",
        "do the following",
        "does",
        "has",
        "each",
        "also",
        "in",
        "simply",
        "other",
        "shall",
        "said",
        "who",
        "your",
        "their",
        "ensure",
        "allow",
        "terms",
        "conditions",
        "information",
        "contributors",
        "contributors as",
        "contributors and the university",
        "indemnification",
        "license",
        "claimed",
        "but",
        "agrees",
        "patent",
        "owner",
        "owners",
        "yyyy",
        "expressly",
        "stating",
        "enforce",
        "d",
        "ss",
        // Additional single-word junk
        "given",
        "may",
        "every",
        "no",
        "good",
        "row",
        "logo",
        "flag",
        "updated",
        "law",
        "england",
        "tm",
        "pgp",
        "distributed",
        "as",
        "null",
        "psy",
        "object",
        "indicate the origin and nature of",
        "statements",
        "protection",
        "(if any) with",
        "if any with",
        // Short gibberish from binary data
        "ga",
        "ka",
        "aa",
        "qa",
        "yx",
        "ac",
        "ae",
        "gn",
        "cb",
        "ib",
        "qb",
        "py",
        "pu",
        "ce",
        "nmd",
        "a1",
        "deg",
        "gnu",
        "with",
        "yy",
        "c/",
        "messages",
        "licenses",
        "not limited",
        "charge",
        "case 2",
        "dot",
        "public",
        // C function/macro names from ICS false positives
        "width",
        "len",
        "do",
        "date",
        "year",
        "note",
        "update",
        "info",
        "notices",
        "duplicated",
        "register",
        // C identifier/keyword false positives from ICS
        "isascii",
        "iscntrl",
        "isprint",
        "isdigit",
        "isalpha",
        "toupper",
        "yyunput",
        "ambiguous",
        "indir",
        "notive",
        "strict",
        "decoded",
        "unsigned",
        // Short numbers/tokens from code
        "0 1",
        "8",
        "9",
        "16",
        "24",
        "4",
        // More boilerplate/legal words
        "notices all the files",
        "may not be removed or altered",
        "duplicated in",
        "mjander",
        "3dfx",
        "related",
    ]
    .into_iter()
    .collect()
});

// ─── Junk detection ──────────────────────────────────────────────────────────

/// Return true if `s` matches any known junk copyright pattern.
pub fn is_junk_copyright(s: &str) -> bool {
    COPYRIGHTS_JUNK_PATTERNS.iter().any(|re| re.is_match(s))
        || is_junk_copyright_scan_phrase(s)
        || is_junk_c_sign_path_fragment(s)
}

fn has_copyright_year(s: &str) -> bool {
    static COPYRIGHT_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b(?:19\d{2}|20\d{2})(?:\s*[-–/]\s*(?:19\d{2}|20\d{2}|\d{2}))?\b").unwrap()
    });

    COPYRIGHT_YEAR_RE.is_match(s)
}

fn is_junk_copyright_scan_phrase(s: &str) -> bool {
    static COPYRIGHT_SCAN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcopyright\s+scan(?:s|ner|ning)?\b").unwrap());

    !has_copyright_year(s) && COPYRIGHT_SCAN_RE.is_match(s)
}

fn is_junk_c_sign_path_fragment(s: &str) -> bool {
    let Some(tail) = s.trim().strip_prefix("(c)") else {
        return false;
    };

    !has_copyright_year(s) && is_path_like_code_fragment(tail)
}

/// Return true if `s` matches any known junk holder pattern.
pub(crate) fn is_junk_holder(s: &str) -> bool {
    HOLDERS_JUNK_PATTERNS.iter().any(|re| re.is_match(s)) || s.eq_ignore_ascii_case("MIT")
}

pub(crate) fn is_path_like_code_fragment(s: &str) -> bool {
    static PATH_LIKE_CODE_FRAGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            [A-Za-z_$][A-Za-z0-9_$]*
            (?:
                /[A-Za-z_$][A-Za-z0-9_$]*
              | \.[A-Za-z_$][A-Za-z0-9_$]*
              | \$[A-Za-z_$][A-Za-z0-9_$]*
            )+
            $
            ",
        )
        .unwrap()
    });

    PATH_LIKE_CODE_FRAGMENT_RE.is_match(s.trim())
}

// ─── Core refinement functions ───────────────────────────────────────────────

/// Refine a detected copyright string. Returns `None` if the result is empty.
pub fn refine_copyright(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let original = normalize_whitespace(s);
    let mut c = original.clone();
    c = strip_trailing_quote_before_email(&c);
    c = normalize_b_dot_angle_emails(&c);
    c = strip_nickname_quotes(&c);
    c = strip_leading_author_label_in_copyright(&c);
    c = strip_leading_licensed_material_of(&c);
    c = strip_leading_version_number_before_c(&c);
    c = strip_contributor_parens_after_org(&c);
    c = strip_trailing_paren_email_after_c_by(&c);
    c = strip_trailing_for_clause_after_email(&c);
    c = strip_trailing_at_affiliation(&c);
    c = strip_trailing_obfuscated_email_after_dash(&c);
    c = strip_url_token_between_years_and_holder(&c);
    c = strip_obfuscated_angle_emails(&c);
    c = strip_angle_bracketed_www_domains_without_by(&c);
    c = strip_leading_simple_copyright_prefixes(&c);
    c = normalize_comma_spacing(&c);
    c = normalize_angle_bracket_comma_spacing(&c);
    c = strip_trailing_secondary_angle_email_after_comma(&c);
    c = strip_trailing_short_surname_paren_list_in_copyright(&c);
    c = strip_trailing_et_al(&c);
    c = strip_trailing_authors_clause(&c);
    c = strip_trailing_document_authors_clause(&c);
    c = strip_trailing_amp_authors(&c);
    c = strip_trailing_x509_dn_fields(&c);
    c = strip_some_punct(&c);
    c = strip_solo_quotes(&c);
    // strip trailing slashes, tildes, spaces
    c = c.trim_matches(&['/', ' ', '~'][..]).to_string();
    c = strip_all_unbalanced_parens(&c);
    c = remove_some_extra_words_and_punct(&c);
    c = strip_trailing_incomplete_as_represented_by(&c);
    c = normalize_whitespace(&c);
    c = strip_leading_js_project_version(&c);
    c = remove_dupe_copyright_words(&c);
    c = strip_trailing_portions_of(&c);
    c = strip_trailing_paren_identifier(&c);
    c = strip_trailing_company_name_placeholder(&c);
    c = strip_trailing_company_co_ltd(&c);
    c = strip_trailing_obfuscated_email_in_angle_brackets_after_copyright(&c);
    c = strip_trailing_linux_ag_location_in_copyright(&c);
    c = strip_trailing_by_person_clause_after_company(&c);
    c = strip_trailing_division_of_company_suffix(&c);
    c = strip_trailing_linux_foundation_suffix(&c);
    c = strip_trailing_paren_at_without_domain(&c);
    c = strip_trailing_inc_after_today_year_placeholder(&c);
    c = truncate_trailing_boilerplate(&c);
    c = strip_trailing_author_label(&c);
    c = strip_trailing_isc_after_inc(&c);
    c = strip_trailing_caps_after_company_suffix(&c);
    c = strip_trailing_javadoc_tags(&c);
    c = strip_trailing_batch_comment_marker(&c);
    c = strip_prefixes(&c, &HashSet::from(["by", "c"]));
    c = c.trim().to_string();
    c = c.trim_matches('+').to_string();
    c = c.trim_matches(&[',', ' '][..]).to_string();
    c = strip_balanced_edge_parens(&c).to_string();
    c = strip_suffixes(&c, &COPYRIGHTS_SUFFIXES);
    c = c.trim_end_matches(&[',', ' '][..]).to_string();
    c = strip_trailing_ampas_acronym(&c);
    c = strip_trailing_period(&c);
    c = strip_independent_jpeg_groups_software_tail(&c);
    c = strip_trailing_original_authors(&c);
    c = strip_trailing_mountain_view_ca(&c);
    c = strip_trailing_comma_after_respective_authors(&c);
    c = c.trim_end_matches(char::is_whitespace).to_string();
    c = c.trim_matches('\'').to_string();
    c = wrap_trailing_and_urls_in_parens(&c);
    c = strip_trailing_url_slash(&c);
    c = truncate_long_words(&c);
    c = strip_trailing_single_digit_token(&c);
    c = strip_trailing_period(&c);
    let result = c.trim().to_string();

    static SOFTWARE_COPYRIGHT_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)\bsoftware\s+copyright\s*\(c\)\s*(?:19\d{2}|20\d{2})\b").unwrap()
    });
    if SOFTWARE_COPYRIGHT_C_RE.is_match(original.as_str())
        && !result.to_ascii_lowercase().contains("copyright")
    {
        let restored = strip_trailing_period(&original);
        let restored = restored.trim().to_string();
        if !restored.is_empty() {
            return Some(restored);
        }
    }

    static YEAR_ONLY_WITH_OBF_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^copyright\s*\(c\)\s*(?:19\d{2}|20\d{2})\s+[a-z0-9][a-z0-9._-]{0,63}\s+at\s+[a-z0-9][a-z0-9._-]{0,63}\s+dot\s+[a-z]{2,12}$",
        )
        .unwrap()
    });
    if YEAR_ONLY_WITH_OBF_EMAIL_RE.is_match(result.as_str()) {
        return None;
    }

    let result_upper = result.to_ascii_uppercase();
    if result_upper.contains("COPYRIGHT")
        && result_upper.contains("YEAR")
        && result_upper.contains("YOUR NAME")
    {
        return None;
    }
    if is_junk_copyright_of_header(&result)
        || is_junk_copyrighted_works_header(&result)
        || is_junk_copyrighted_software_phrase(&result)
    {
        return None;
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn strip_trailing_obfuscated_email_after_dash(s: &str) -> String {
    static TRAILING_DASH_OBF_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^(?P<prefix>.+?)\s*(?:--+|-)\s*(?P<user>[a-z0-9][a-z0-9._-]{0,63})\s*(?:\[\s*at\s*\]|at)\s*(?P<host>[a-z0-9][a-z0-9._-]{0,63})\s*(?:\[\s*dot\s*\]|dot)\s*(?P<tld>[a-z]{2,12})\s*$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = TRAILING_DASH_OBF_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };

    cap.name("prefix")
        .map(|m| m.as_str().trim_end_matches(&[' ', '-', '–', '—'][..]))
        .unwrap_or(trimmed)
        .to_string()
}

fn strip_trailing_secondary_angle_email_after_comma(s: &str) -> String {
    static TRAILING_SECOND_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?<[^>\s]*@[^>\s]*>)\s*,\s*<[^>\s]*@[^>\s]*>\s*$").unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = TRAILING_SECOND_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };

    let full = cap.get(0).map(|m| m.as_str()).unwrap_or(trimmed);
    let emails: Vec<&str> = full
        .split('<')
        .skip(1)
        .filter_map(|p| p.split_once('>').map(|(e, _)| e.trim()))
        .filter(|e| e.contains('@'))
        .collect();
    if emails.len() >= 2 {
        let a = emails[0].to_ascii_lowercase();
        let b = emails[1].to_ascii_lowercase();
        if a != b {
            return s.to_string();
        }
    }

    cap.name("prefix")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn normalize_b_dot_angle_emails(s: &str) -> String {
    static B_DOT_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<\s*b\.(?P<email>[^>\s]*@[^>\s]+)\s*>").unwrap());
    B_DOT_EMAIL_RE.replace_all(s, ".${email}").into_owned()
}

fn strip_url_token_between_years_and_holder(s: &str) -> String {
    static BETWEEN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*[-,\s0-9]{4,32})\s+https?://\S+\s+(?P<tail>\p{L}.+)$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = BETWEEN_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !tail.is_empty() {
            return normalize_whitespace(&format!("{prefix} {tail}"));
        }
    }
    s.to_string()
}

fn wrap_trailing_and_urls_in_parens(s: &str) -> String {
    static TRAILING_URLS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s+(?P<urls>https?://\S+\s+and\s+https?://\S+)\s*$")
            .unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = TRAILING_URLS_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap
        .name("prefix")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim_end();
    let urls = cap.name("urls").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || urls.is_empty() {
        return s.to_string();
    }
    if urls.starts_with('(') {
        return s.to_string();
    }
    format!("{prefix} ({urls})")
}

fn strip_obfuscated_angle_emails(s: &str) -> String {
    static OBF_ANGLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s*<[^>]*(?:\[at\]|\bat\b)[^>]*>\s*").unwrap());
    let trimmed = s.trim();
    if !(trimmed.contains("<") && trimmed.contains(">")) {
        return s.to_string();
    }
    let out = OBF_ANGLE_RE.replace_all(trimmed, " ").into_owned();
    normalize_whitespace(&out)
}

fn strip_trailing_linux_foundation_suffix(s: &str) -> String {
    static LINUX_FOUNDATION_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*\d{4}(?:\s*,\s*\d{4})*)\s+Linux\s+Foundation\s*$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = LINUX_FOUNDATION_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_linux_ag_location_in_copyright(s: &str) -> String {
    static LINUX_AG_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\b.*?\s)(?P<name>\S+)\s+Linux\s+AG\s*,\s*[^,]{2,64}\s*,\s*[^,]{2,64}\s*$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = LINUX_AG_COPY_RE.captures(trimmed) {
        let prefix = cap
            .name("prefix")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim_end();
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !name.is_empty() {
            return format!("{prefix} {name}");
        }
    }
    s.to_string()
}

fn strip_trailing_quote_before_email(s: &str) -> String {
    static TRAILING_QUOTE_BEFORE_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<head>.*?\b[\p{L}])'\s+(?P<email><[^>\s]*@[^>\s]+>|[^\s<>]*@[^\s<>]+)(?P<tail>.*)$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    if !trimmed.contains('@') {
        return s.to_string();
    }
    let Some(cap) = TRAILING_QUOTE_BEFORE_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let head = cap.name("head").map(|m| m.as_str()).unwrap_or("");
    let email = cap.name("email").map(|m| m.as_str()).unwrap_or("");
    let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("");
    normalize_whitespace(&format!("{head} {email}{tail}"))
}

fn strip_nickname_quotes(s: &str) -> String {
    static NICK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<first>\b[\p{Lu}][\p{L}'-]+)\s+'(?P<nick>[A-Za-z]{2,20})'\s+(?P<last>\b[\p{Lu}][\p{L}'-]+)")
            .unwrap()
    });
    NICK_RE
        .replace_all(s, "${first} ${nick} ${last}")
        .into_owned()
}

fn strip_trailing_for_clause_after_email(s: &str) -> String {
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.contains(" for ") {
        return s.to_string();
    }
    if !lower.starts_with("copyright") {
        return s.to_string();
    }
    if !trimmed.contains('@') {
        return s.to_string();
    }
    let Some((head, _tail)) = trimmed.split_once(" for ") else {
        return s.to_string();
    };

    if let Some((_, tail)) = trimmed.split_once(" for ") {
        let tail = tail.trim();
        if tail.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            let word_count = tail.split_whitespace().count();
            let lower_tail = tail.to_ascii_lowercase();
            let looks_like_affiliation = word_count >= 3
                && (lower_tail.contains("laboratory")
                    || lower_tail.contains("computer science")
                    || lower_tail.contains("facility")
                    || lower_tail.contains("institute")
                    || lower_tail.contains("university")
                    || lower_tail.contains("department")
                    || lower_tail.contains("center"));
            if looks_like_affiliation {
                return s.to_string();
            }
        }
    }
    head.trim_end().to_string()
}

fn strip_trailing_at_affiliation(s: &str) -> String {
    let trimmed = s.trim();
    if !trimmed.to_ascii_lowercase().starts_with("copyright") {
        return s.to_string();
    }
    let Some((head, tail)) = trimmed.split_once(" @ ") else {
        return s.to_string();
    };
    let tail = tail.trim();
    if tail.is_empty() {
        return s.to_string();
    }
    if tail.contains('@') {
        return s.to_string();
    }
    if tail.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return head.trim_end().to_string();
    }
    s.to_string()
}

fn strip_trailing_paren_at_without_domain(s: &str) -> String {
    static TRAILING_PAREN_AT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(\s*(?P<inner>[^)]*\bat\b[^)]*)\)\s*$").unwrap()
    });

    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("copyright") || lower.starts_with("(c)")) {
        return s.to_string();
    }

    let Some(cap) = TRAILING_PAREN_AT_RE.captures(trimmed) else {
        return s.to_string();
    };
    let inner = cap.name("inner").map(|m| m.as_str()).unwrap_or("").trim();
    if inner.is_empty() {
        return s.to_string();
    }

    let inner_lower = inner.to_ascii_lowercase();
    if inner.contains('@') || inner.contains('.') || inner_lower.contains(" dot ") {
        return s.to_string();
    }

    cap.name("prefix")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_inc_after_today_year_placeholder(s: &str) -> String {
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.contains("today.year") || lower.contains("current_year")) {
        return s.to_string();
    }
    if !(lower.ends_with(" inc.") || lower.ends_with(" inc")) {
        return s.to_string();
    }
    let prefix = trimmed
        .trim_end_matches('.')
        .trim_end_matches(|c: char| c.is_whitespace())
        .strip_suffix("Inc")
        .or_else(|| trimmed.strip_suffix("Inc."));
    let Some(prefix) = prefix else {
        return s.to_string();
    };
    prefix.trim_end().to_string()
}

fn strip_trailing_obfuscated_email_in_angle_brackets_after_copyright(s: &str) -> String {
    static OBFUSCATED_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>copyright\b.+?)\s*<[^>]*\bat\b[^>]*\bdot\b[^>]*>\s*$").unwrap()
    });

    let trimmed = s.trim();
    if !trimmed
        .get(.."Copyright".len())
        .is_some_and(|p| p.eq_ignore_ascii_case("Copyright"))
    {
        return s.to_string();
    }

    let Some(cap) = OBFUSCATED_TAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    cap.name("prefix")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_author_label(s: &str) -> String {
    static TRAILING_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s+(?:Author|AUTHOR)\b").expect("valid trailing Author regex")
    });
    let Some(m) = TRAILING_AUTHOR_RE.find(s) else {
        return s.to_string();
    };

    let prefix = s[..m.start()].trim_end();
    if !prefix_has_holder_words(prefix) {
        return s.to_string();
    }

    prefix.to_string()
}

fn strip_leading_author_label_in_copyright(s: &str) -> String {
    static LEADING_AUTHOR_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:@?author)\s+(?P<rest>.+\(c\)\s*(?:19|20)\d{2}.*)$")
            .expect("valid leading author copyright regex")
    });
    let trimmed = s.trim();
    let Some(cap) = LEADING_AUTHOR_COPY_RE.captures(trimmed) else {
        return s.to_string();
    };
    let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("").trim();
    if rest.is_empty() {
        return s.to_string();
    }
    rest.to_string()
}

fn strip_leading_author_label_in_holder(s: &str) -> String {
    static LEADING_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:@?author)\b[:\s]+(?P<rest>.+)$").expect("valid leading author regex")
    });
    let trimmed = s.trim();
    let Some(cap) = LEADING_AUTHOR_RE.captures(trimmed) else {
        return s.to_string();
    };
    let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("").trim();
    if rest.is_empty() {
        return s.to_string();
    }
    rest.to_string()
}

fn prefix_has_holder_words(prefix: &str) -> bool {
    for raw in prefix.split_whitespace() {
        let token = raw.trim_matches(|c: char| c.is_ascii_punctuation() || matches!(c, '' | ''));
        if token.is_empty() {
            continue;
        }

        let lower = token.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "*" | "copyright" | "copr" | "(c)" | "c" | "\u{a9}"
        ) {
            continue;
        }

        // Ignore pure year-ish tokens.
        let yearish = token
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '+' | ','));
        if yearish {
            continue;
        }

        return true;
    }

    false
}

fn strip_leading_licensed_material_of(s: &str) -> String {
    static LICENSED_MATERIAL_OF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?:licensed\s+)?material\s+of\s+").unwrap());
    LICENSED_MATERIAL_OF_RE
        .replace(s, "")
        .trim_start()
        .to_string()
}

fn strip_leading_version_number_before_c(s: &str) -> String {
    static VERSION_BEFORE_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\d+\.\d+(?:\.\d+)*\.?\s+(\(c\)|\bcopyright\b)").unwrap()
    });
    if let Some(m) = VERSION_BEFORE_C_RE.find(s) {
        let cap = VERSION_BEFORE_C_RE.captures(s).unwrap();
        let keyword_start = m.start() + m.as_str().len() - cap[1].len();
        s[keyword_start..].trim_start().to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_authors_clause(s: &str) -> String {
    static AUTHORS_CLAUSE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+?)\s+Authors?\b\s+(?P<rest>.+)$").unwrap());

    let trimmed = s.trim();

    let Some(cap) = AUTHORS_CLAUSE_RE.captures(trimmed) else {
        return s.to_string();
    };

    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("");
    let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("");
    if prefix.trim().is_empty() || rest.trim().is_empty() {
        return s.to_string();
    }

    let rest_for_count = if let Some(email_idx) = rest.find('@') {
        rest[..email_idx].trim()
    } else {
        rest.trim()
    };

    let words_before_email = rest_for_count
        .split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphabetic()) && !w.contains('<') && !w.contains('>'))
        .count();
    if words_before_email > 2 {
        return s.to_string();
    }

    let prefix_trimmed = prefix.trim();
    let prefix_last_is_year = prefix_trimmed
        .split_whitespace()
        .last()
        .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()));
    if !prefix_trimmed.contains(',') && !prefix_last_is_year {
        return s.to_string();
    }

    prefix_trimmed
        .trim_end_matches(&[',', ';', ':'][..])
        .trim()
        .to_string()
}

fn strip_trailing_document_authors_clause(s: &str) -> String {
    static DOCUMENT_AUTHORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>.+?)\s+and\s+the\s+persons\s+identified\s+as\s+document\s+authors\.?$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = DOCUMENT_AUTHORS_RE.captures(trimmed) else {
        return s.to_string();
    };

    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || !prefix_has_holder_words(prefix) {
        return s.to_string();
    }

    prefix
        .trim_end_matches(&[',', ';', ':', ' '][..])
        .trim()
        .to_string()
}

fn strip_trailing_et_al(s: &str) -> String {
    static ET_AL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s*,?\s*et\s+al\.?\s*$").unwrap());

    let trimmed = s.trim();
    let Some(cap) = ET_AL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("");
    prefix.trim().trim_end_matches(',').trim().to_string()
}

fn strip_trailing_x509_dn_fields(s: &str) -> String {
    static X509_DN_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*\d{4}(?:\s*,\s*OU\s+[^,]+|\s+[^,]+))(?:\s*,\s*(?:OU|CN|O|C|L|ST)\s+.+)$",
        )
        .unwrap()
    });
    static OU_ENDORSED_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*\d{4}\s*,\s*OU\s+.+?)\s+endorsed\s*$")
            .unwrap()
    });

    let Some(cap) = X509_DN_TAIL_RE.captures(s.trim()) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    if let Some(cap2) = OU_ENDORSED_TAIL_RE.captures(prefix) {
        cap2.name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| prefix.to_string())
    } else {
        prefix.to_string()
    }
}

fn strip_independent_jpeg_groups_software_tail(s: &str) -> String {
    static JPEG_GROUP_SOFTWARE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(Independent JPEG Group's)\s+software\b\.?$").unwrap());
    JPEG_GROUP_SOFTWARE_RE.replace(s, "$1").trim().to_string()
}

fn strip_trailing_original_authors(s: &str) -> String {
    static ORIGINAL_AUTHORS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(.*\bthe original)\s+authors\b\s*$").unwrap());
    if let Some(cap) = ORIGINAL_AUTHORS_RE.captures(s) {
        cap[1].trim().to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_paren_email_after_c_by(s: &str) -> String {
    static C_BY_PAREN_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>(?:Copyright\s+)?\(c\)\s+by\s+[^()]+?)\s*\([^()]*@[^()]*\)\s*$",
        )
        .unwrap()
    });

    if let Some(caps) = C_BY_PAREN_EMAIL_RE.captures(s) {
        caps.name("prefix")
            .map(|m| normalize_whitespace(m.as_str().trim()))
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_contributor_parens_after_org(s: &str) -> String {
    static ORG_PARENS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.*)\(\s*(?P<inner>[^()]+?)\s*\)\s*$").unwrap());

    let Some(cap) = ORG_PARENS_RE.captures(s.trim()) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    let inner = cap.name("inner").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || inner.is_empty() {
        return s.to_string();
    }

    let inner_lower = inner.to_ascii_lowercase();
    let looks_like_contributor_list = inner_lower.contains(" and ") || inner.contains('<');
    if !looks_like_contributor_list {
        return s.to_string();
    }

    normalize_whitespace(&format!("{prefix} {inner}"))
}

fn strip_angle_bracketed_www_domains_without_by(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    if lower.contains(" by ") {
        return s.to_string();
    }

    static WWW_IN_COMMA_CLAUSE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i),\s*<www\.[^>]+>\s*").expect("valid www domain regex"));
    static WWW_TRAILING_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\s*<www\.[^>]+>\s*$").expect("valid trailing www domain regex")
    });

    let s = WWW_IN_COMMA_CLAUSE_RE.replace_all(s, ", ");
    let s = WWW_TRAILING_RE.replace(&s, "");
    normalize_whitespace(s.trim())
}

fn strip_angle_bracketed_www_domains(s: &str) -> String {
    static WWW_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s*<www\.[^>]+>\s*").expect("valid www domain regex"));

    let s = WWW_RE.replace_all(s, " ");
    normalize_whitespace(s.trim())
}

fn strip_trailing_mountain_view_ca(s: &str) -> String {
    static MOUNTAIN_VIEW_CA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bMountain View\s*,\s*CA\.?$").expect("valid Mountain View CA regex")
    });

    if MOUNTAIN_VIEW_CA_RE.is_match(s) {
        MOUNTAIN_VIEW_CA_RE
            .replace(s, "Mountain View")
            .trim()
            .to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_isc_after_inc(s: &str) -> String {
    static TRAILING_ISC_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?\bInc\.?)\s+ISC\s*$").unwrap());
    if let Some(cap) = TRAILING_ISC_RE.captures(s.trim()) {
        cap.name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_trailing_caps_after_company_suffix(s: &str) -> String {
    static TRAILING_CAPS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?\b(?:Corp|Inc|Ltd|LLC|Co)\.)\s+[A-Z]{2,}\s*$").unwrap()
    });
    if let Some(cap) = TRAILING_CAPS_RE.captures(s.trim()) {
        cap.name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_trailing_comma_after_respective_authors(s: &str) -> String {
    let trimmed = s.trim_end_matches(char::is_whitespace);
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("respective authors,") {
        let mut t = trimmed.to_string();
        if t.ends_with(',') {
            t.pop();
        }
        t.trim_end_matches(char::is_whitespace).to_string()
    } else {
        s.to_string()
    }
}

fn strip_leading_simple_copyright_prefixes(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    if (lower.starts_with("program copyright") || lower.starts_with("debian copyright"))
        && let Some(idx) = lower.find("copyright")
    {
        return s[idx..].trim_start().to_string();
    }

    if lower.contains("debian copyright")
        && let Some(idx) = lower.rfind("copyright")
    {
        let tail = s[idx..].trim_start();
        if tail.to_ascii_lowercase().starts_with("copyright") {
            return tail.to_string();
        }
    }

    if lower.starts_with("the ")
        && let Some(idx) = lower.rfind(". copyright")
        && idx + 2 < s.len()
    {
        let tail = s[(idx + 2)..].trim_start();
        if tail.to_ascii_lowercase().starts_with("copyright") {
            return tail.to_string();
        }
    }

    s.to_string()
}

fn is_junk_copyright_of_header(s: &str) -> bool {
    let lower = s.to_lowercase();
    let prefix = "copyright of";
    if !lower.starts_with(prefix) {
        return false;
    }

    let mut tail = s[prefix.len()..].trim();
    tail = tail.trim_matches(&[':', '-', ' ', '\t'][..]);
    if tail.is_empty() {
        return true;
    }

    let tail_lower = tail.to_lowercase();
    if tail_lower.starts_with("qt has been transferred") {
        return true;
    }
    if tail_lower.starts_with("version of nameif") {
        return true;
    }
    if tail_lower.contains("full text of") {
        return true;
    }

    if tail.contains('/') {
        return true;
    }

    !tail.chars().any(|c| c.is_ascii_uppercase())
}

fn strip_leading_js_project_version(s: &str) -> String {
    static JS_PROJECT_VERSION_PREFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[a-z0-9_.-]+\.js\s+\d+\.\d+(?:\.\d+)?\s+").unwrap());

    JS_PROJECT_VERSION_PREFIX_RE
        .replace(s, "")
        .trim()
        .to_string()
}

fn truncate_trailing_boilerplate(s: &str) -> String {
    static TRAILING_BOILERPLATE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        let patterns = [
            r"(?i)\bDistributed in the hope\b",
            r"(?i)\bMay be used\b",
            r"(?i)\bLicense-Alias\b",
            r"(?i)\bFull text of\b",
            r"(?i)\s+-\s*icon support\b",
            r"(?i)\s+-\s*maintainer\b",
            r"(?i)\s+-\s*software\b",
            r"(?i)\.\s*Software\.?$",
            r"(?i),+\s*Software\b",
            r"(?i)\bwrite\s+to\s+the\s+Free\s+Software\s+Foundation\b",
            r"(?i)\b51\s+Franklin\s+(?:Street|St)\b",
            r"(?i)\b675\s+Mass\s+Ave\b",
            r"(?i)\b901\s+San\s+Antonio\s+Road\b",
            r"(?i)\b2601\s+Elliott\s+Avenue\b",
            r"(?i)\bKoll\s+Center\s+Parkway\b",
            r"(?i)\bGNU\s+GENERAL\s+PUBLIC\s+LICENSE\b",
            r"(?i)\s+GNU\s*$",
            r"(?i)\.\s*print\s*$",
            r"(?i)\bTheir\s+notice\s+is\s+reproduced\s+below\b",
            r"(?i)\bTheir\s+notice\s+reproduced\s+below\b",
            r"(?i)\bTheir\s+notice\s+reproduced\s+below\s+in\s+its\s+entirety\b",
            r"(?i)\band/or\s+its\s+suppliers?\b",
            r"(?i)\bNOTE\s+Sort\b",
            r"(?i)\bdocumentation\s+generated\s+by\b",
            r"(?i)\(\s*The full list is in\b",
            r#"(?i)\(\s*the\s+['"]?original\s+author['"]?\s*\)\s+and\s+additional\s+contributors\b"#,
            r"(?i)\bthe\s+original\s+author\b\s+and\s+additional\s+contributors\b",
            r"\becho\s+",
            r"(?i)\bv\d+\.\d+\s*$",
            r"(?i)\bassigned\s+to\s+the\s+",
            r"(?i)\bHP\s+IS\s+AGREEING\b",
            r"(?i)\bCA\.\s*ansi2knr\b",
            r"(?i)\bDirect\s+questions\b",
            r"(?i)\bkbd\s+driver\b",
            r"(?i)\bMIDI\s+driver\b",
            r"(?i)\bLZO\s+version\b",
            r"(?i)\bpersistent\s+bitmap\b",
            r"(?i)\bLIBERATION\b",
            r"(?i)\bAHCI\s+SATA\b",
            r"(?i)\bDTMF\s+code\b",
            r"\bOPTIONS\s*$",
            r"(?i)\bindexing\s+(?:porting|code)\b",
            r"(?i)\bvortex\b",
            r"(?i)\bLinuxTV\b",
            r"(?i)-\s*OMAP\d",
            r"\bGDB\b",
            r"(?i)\band\s+software/linux\b",
            r"(?i),\s+by\s+Paul\s+Dale\b",
            r"(?i),?\s+and\s+other\s+parties\b",
            r"(?i)\b\d+\s+Parnell\s+St\b",
            r"(?i)\b\d+\s+Main\s+(?:street|st)\b",
            r"(?i)\b\d+\s+Koll\s+Center\s+Parkway\b",
            r"(?i)\bBeverly\s+Hills\b",
            r"(?i)\bBerverly\s+Hills\b",
            r"(?i)\bDublin\s+\d\b",
            r"(?i)\band\s+Bob\s+Dougherty\b",
            r"(?i)\band\s+is\s+licensed\s+under\b",
            r"(?i)\bBEGIN\s+LICENSE\s+BLOCK\b",
            r"(?i)^NOTICE,\s*DISCLAIMER,\s*and\s*LICENSE\b",
            r"(?i)\bIn\s+the\s+event\s+of\b",
            r"(?i),\s*ALL\s+RIGHTS\s+RESERVED\b",
            r"(?i)\s+All\s+rights\s+reserved\b",
            r"(?i)\s+All\s+rights\b",
            r"(?i),\s*THIS\s+SOFTWARE\s+IS\b",
            r"(?i),?\s+member\s+of\s+The\s+XFree86\s+Project\b",
            r"(?i)\s+Download\b",
            r"(?i)\bThis\s+code\s+is\s+GPL\b",
            r"(?i)\bGPLd\b",
            r"(?i)\bPlaced\s+under\s+the\s+GNU\s+GPL\b",
            r"(?i)\bSee\s+the\s+GNU\s+GPL\b",
            r"(?i)\bFor\s+other\s+copyrights\b",
            r"(?i)\bLast\s+modified\b",
            r"(?i)\(\s*the\s+original\s+version\s*\)\s*$",
            r"(?i)\bavalable\s+at\b",
            r"(?i)\bavailable\s+at\b",
            r"(?i),\s+and\s+are\s*$",
            r"(?i)\bNIN\s+logo\b",
            r"(?i),\s+with\s*$",
            r"(?i)\(\s*(?:written|brushed)\b[^)]*\)\s*$",
            r"(?i)\(\s*[^)]*implementation[^)]*\)\s*$",
            r"(?i)\bThis\s+file\s+is\s+licensed\s+under\b",
            r"(?i)\bLicensing\s+details\s+are\s+in\b",
            r"(?i)\bLinux\s+for\s+Hitachi\s+SuperH\b",
            r"(?i)\.\s*OProfile\s*$",
        ];
        patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
    });

    let mut cut: Option<usize> = None;
    for re in TRAILING_BOILERPLATE_RE.iter() {
        if let Some(m) = re.find(s) {
            cut = Some(cut.map_or(m.start(), |c| c.min(m.start())));
        }
    }

    if let Some(idx) = cut {
        s[..idx]
            .trim()
            .trim_matches(&['-', ',', ';'][..])
            .trim()
            .to_string()
    } else {
        s.trim().to_string()
    }
}

fn is_junk_copyrighted_works_header(s: &str) -> bool {
    let lower = s.to_lowercase();
    let prefix = "copyrighted works";
    if !lower.starts_with(prefix) {
        return false;
    }

    let mut tail = s[prefix.len()..].trim();
    tail = tail.trim_matches(&[':', '-', ' ', '\t'][..]);
    if tail.is_empty() {
        return true;
    }

    let tail_lower = tail.to_lowercase();
    let rest = if tail_lower == "of" {
        return true;
    } else if tail_lower.starts_with("of ") {
        tail[2..].trim()
    } else {
        return true;
    };

    if rest.is_empty() {
        return true;
    }

    !rest.chars().any(|c| c.is_ascii_uppercase())
}

fn is_junk_copyrighted_software_phrase(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("copyrighted software")
}

fn strip_trailing_company_name_placeholder(s: &str) -> String {
    static COMPANY_NAME_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)(\bCOMPANY)\s+NAME\s*$").unwrap());
    COMPANY_NAME_RE.replace(s, "$1").trim().to_string()
}

fn strip_leading_portions_comma(s: &str) -> String {
    static LEADING_PORTIONS_COMMA_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?:portions?|parts?)\s*,\s*").unwrap());
    LEADING_PORTIONS_COMMA_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_paren_identifier(s: &str) -> String {
    static TRAILING_PAREN_ID_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+\([a-z][a-z0-9]{3,}\)\s*$").unwrap());
    static TRAILING_PAREN_ID_COMMA_WORD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+\([a-z][a-z0-9]{3,}\),\s*[a-z][a-z0-9]*\.?\s*$").unwrap());
    let s = TRAILING_PAREN_ID_COMMA_WORD_RE.replace(s, "");
    TRAILING_PAREN_ID_RE.replace(&s, "").trim().to_string()
}

fn strip_trailing_portions_of(s: &str) -> String {
    static TRAILING_PORTIONS_OF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(?:some\s+)?(?:portions?|parts?)\s+of$").unwrap());
    TRAILING_PORTIONS_OF_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_short_surname_paren_list_in_holder(s: &str) -> String {
    static SHORT_SURNAME_PAREN_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<first>[\p{Lu}][\p{L}'-]+)\s+(?:[\p{Lu}][\p{Ll}])\s*\([^)]*\)\s*,\s*.+$")
            .expect("valid short-surname paren list regex")
    });

    let trimmed = s.trim();
    if let Some(cap) = SHORT_SURNAME_PAREN_LIST_RE.captures(trimmed) {
        cap.name("first")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_trailing_short_surname_paren_list_in_copyright(s: &str) -> String {
    static SHORT_SURNAME_PAREN_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s+\((?:c|C)\)\s+\d{4}(?:-\d{4})?)\s+(?P<first>[\p{Lu}][\p{L}'-]+)\s+(?:[\p{Lu}][\p{Ll}])\s*\([^)]*\)\s*,\s*.+$",
        )
        .expect("valid short-surname copyright paren list regex")
    });

    let trimmed = s.trim();
    if let Some(cap) = SHORT_SURNAME_PAREN_LIST_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let first = cap.name("first").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !first.is_empty() {
            return normalize_whitespace(&format!("{prefix} {first}"));
        }
    }
    s.to_string()
}

/// Refine a detected holder name. Returns `None` if junk or empty.
pub fn refine_holder(s: &str) -> Option<String> {
    refine_holder_impl(s, false)
}

pub fn refine_holder_in_copyright_context(s: &str) -> Option<String> {
    refine_holder_impl(s, true)
}

fn strip_parenthesized_emails(s: &str) -> String {
    static PAREN_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s*\([^()]*@[^()]*\)\s*").unwrap());
    normalize_whitespace(&PAREN_EMAIL_RE.replace_all(s, " "))
}

fn refine_holder_impl(s: &str, in_copyright_context: bool) -> Option<String> {
    if s.is_empty() {
        return None;
    }

    let had_paren_email =
        in_copyright_context && s.contains('@') && s.contains('(') && s.contains(')');

    // Choose prefix set based on whether "reserved" appears.
    let prefixes = if s.to_lowercase().contains("reserved") {
        &*HOLDERS_PREFIXES_WITH_ALL
    } else {
        &*HOLDERS_PREFIXES
    };

    let mut h = s.replace("build.year", " ");
    h = strip_trailing_quote_before_email(&h);
    h = strip_nickname_quotes(&h);
    h = strip_leading_author_label_in_holder(&h);
    h = strip_angle_bracketed_www_domains(&h);
    if in_copyright_context {
        h = strip_angle_bracketed_emails(&h);
        h = strip_trailing_email_token(&h);
        h = strip_trailing_obfuscated_email_phrase_in_holder(&h);
    }
    h = strip_parenthesized_emails(&h);
    h = strip_trailing_parenthesized_url_or_domain(&h);
    h = strip_contributor_parens_after_org(&h);
    h = normalize_comma_spacing(&h);
    h = normalize_angle_bracket_comma_spacing(&h);
    h = strip_trailing_linux_ag_location(&h);
    h = strip_trailing_but_suffix(&h);
    if had_paren_email {
        h = remove_comma_between_person_and_company_suffix(&h);
    }
    h = strip_trailing_by_person_clause_after_company(&h);
    h = strip_trailing_division_of_company_suffix(&h);
    h = strip_leading_product_operating_system_title(&h);
    h = strip_trailing_et_al(&h);
    h = strip_trailing_authors_clause(&h);
    h = strip_trailing_document_authors_clause(&h);
    h = strip_trailing_amp_authors(&h);
    h = strip_trailing_x509_dn_fields_from_holder(&h);
    h = strip_leading_js_project_version(&h);
    h = truncate_trailing_boilerplate(&h);
    h = strip_trailing_isc_after_inc(&h);
    h = strip_trailing_caps_after_company_suffix(&h);
    h = strip_trailing_javadoc_tags(&h);
    h = strip_trailing_batch_comment_marker(&h);
    h = strip_leading_portions_comma(&h);
    h = strip_trailing_paren_identifier(&h);
    h = strip_trailing_company_name_placeholder(&h);

    if in_copyright_context {
        h = strip_trailing_short_surname_paren_list_in_holder(&h);
    }

    // Strip leading date-like prefix (digits, dashes, slashes).
    if h.contains(' ')
        && let Some((prefix, suffix)) = h.split_once(' ')
        && prefix
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '/')
    {
        h = suffix.to_string();
    }

    h = remove_some_extra_words_and_punct(&h);
    h = strip_trailing_incomplete_as_represented_by(&h);
    h = h.trim_matches(&['/', ' ', '~'][..]).to_string();
    h = refine_names(&h, prefixes);
    h = strip_trailing_company_co_ltd(&h);
    h = strip_suffixes(&h, &HOLDERS_SUFFIXES);
    h = strip_trailing_ampas_acronym(&h);
    h = h.trim_matches(&['/', ' ', '~'][..]).to_string();
    h = strip_solo_quotes(&h);
    h = h.replace("( ", " ").replace(" )", " ");
    h = h.trim_matches(&['+', '-', ' '][..]).to_string();
    h = strip_trailing_period(&h);
    h = strip_independent_jpeg_groups_software_tail(&h);
    h = strip_trailing_original_authors(&h);
    h = h.trim_matches(&['+', '-', ' '][..]).to_string();
    h = remove_dupe_holder(&h);
    h = normalize_whitespace(&h);
    h = strip_trailing_url(&h);
    h = h
        .trim_matches(&['/', ' ', '~', '-', '–', '—'][..])
        .to_string();
    if in_copyright_context {
        h = strip_trailing_email_token(&h);
    }
    h = strip_trailing_at_sign(&h);
    h = strip_trailing_mountain_view_ca(&h);
    h = h.trim_matches(&[',', ' '][..]).to_string();
    h = strip_trailing_period(&h);
    h = h.trim_matches(&[',', ' '][..]).to_string();
    h = normalize_whitespace(&h);
    h = truncate_long_words(&h);
    h = strip_trailing_single_digit_token(&h);
    h = strip_trailing_period(&h);
    h = h.trim().to_string();

    let lower = h.to_lowercase();
    if h.trim_end_matches('.').eq_ignore_ascii_case("YOUR NAME") {
        return None;
    }
    let is_single_word_contributors = lower == "contributors";
    let is_contributors_as_noted_in_authors_file =
        in_copyright_context && lower.contains("contributors as noted in the authors file");
    if !h.is_empty()
        && (!HOLDERS_JUNK.contains(lower.as_str())
            || (in_copyright_context && is_single_word_contributors))
        && (is_contributors_as_noted_in_authors_file || !is_junk_holder(&h))
    {
        Some(h)
    } else {
        None
    }
}

fn strip_trailing_but_suffix(s: &str) -> String {
    static TRAILING_BUT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?),\s*but\s*$").unwrap());
    let trimmed = s.trim();
    let Some(cap) = TRAILING_BUT_RE.captures(trimmed) else {
        return s.to_string();
    };
    cap.name("prefix")
        .map(|m| m.as_str().trim_end().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_division_of_company_suffix(s: &str) -> String {
    static DIVISION_OF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?),\s*a\s+division\s+of\s+.+$").unwrap());

    let trimmed = s.trim();
    let Some(cap) = DIVISION_OF_RE.captures(trimmed) else {
        return s.to_string();
    };

    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || !prefix_has_holder_words(prefix) {
        return s.to_string();
    }

    prefix.trim_end_matches(&[',', ' '][..]).trim().to_string()
}

fn strip_trailing_linux_ag_location(s: &str) -> String {
    static LINUX_AG_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>\S+)\s+Linux\s+AG\s*,\s*[^,]{2,64}\s*,\s*[^,]{2,64}\s*$").unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = LINUX_AG_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn remove_comma_between_person_and_company_suffix(s: &str) -> String {
    static COMMA_CORP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<person>[\p{Lu}][^,]{2,64}(?:\s+[\p{Lu}][^,]{2,64})+)\s*,\s*(?P<corp>[^,]{2,64}\b(?:Corp\.?|Corporation|Inc\.?|Ltd\.?))\s*$")
            .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = COMMA_CORP_RE.captures(trimmed) {
        let person = cap.name("person").map(|m| m.as_str()).unwrap_or("").trim();
        let corp = cap.name("corp").map(|m| m.as_str()).unwrap_or("").trim();
        if !person.is_empty() && !corp.is_empty() {
            return format!("{person} {corp}");
        }
    }
    s.to_string()
}

fn strip_trailing_by_person_clause_after_company(s: &str) -> String {
    static BY_PERSON_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?\b(?:Corp\.?|Corporation|Inc\.?|Ltd\.?))\s+by\s+[\p{Lu}][\p{L}'\-\.]+(?:\s+[\p{Lu}][\p{L}'\-\.]+){1,4}\s*(?:<[^>]*>)?\s*$")
            .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = BY_PERSON_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_amp_authors(s: &str) -> String {
    static AMP_AUTHORS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s*(?:&|and)\s+authors?\s*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = AMP_AUTHORS_RE.captures(trimmed)
        && let Some(prefix) = cap.name("prefix").map(|m| m.as_str().trim())
        && !prefix.is_empty()
    {
        return prefix.to_string();
    }
    s.to_string()
}

fn strip_trailing_parenthesized_url_or_domain(s: &str) -> String {
    static TRAILING_PAREN_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(\s*(?:https?|ftp)://[^)\s]+\s*\)\s*$").unwrap()
    });
    static TRAILING_PAREN_DOMAIN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(\s*[a-z0-9._-]+\.[a-z]{2,12}\s*\)\s*$").unwrap()
    });
    static TRAILING_SINGLE_WORD_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?)\s*\(\s*(?P<inner>[A-Za-z0-9._-]{2,32})\s*\)\s*$").unwrap()
    });

    let trimmed = s.trim();
    if let Some(cap) = TRAILING_PAREN_URL_RE.captures(trimmed) {
        return cap
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string());
    }
    if let Some(cap) = TRAILING_PAREN_DOMAIN_RE.captures(trimmed) {
        return cap
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string());
    }
    if let Some(cap) = TRAILING_SINGLE_WORD_PARENS_RE.captures(trimmed)
        && let Some(inner) = cap.name("inner").map(|m| m.as_str().trim())
        && !inner.is_empty()
    {
        let inner_has_upper = inner.chars().any(|c| c.is_ascii_uppercase());
        let inner_all_lowerish = inner
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));

        if !inner_has_upper && inner_all_lowerish && inner.len() >= 4 && !inner.starts_with('-') {
            return cap
                .name("prefix")
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_else(|| s.to_string());
        }
    }

    s.to_string()
}

fn strip_angle_bracketed_emails(s: &str) -> String {
    static ANGLE_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*<[^>\s]*@[^>\s]*>\s*").unwrap());
    ANGLE_EMAIL_RE.replace_all(s, " ").trim().to_string()
}

fn strip_trailing_email_token(s: &str) -> String {
    static TRAILING_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?)\s+(?P<email>[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+)\s*$").unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = TRAILING_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    prefix.to_string()
}

fn strip_trailing_obfuscated_email_phrase_in_holder(s: &str) -> String {
    static OBFUSCATED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>.+?)\s+(?P<user>[a-z0-9][a-z0-9._-]{0,63})\s+at\s+(?P<domain>[a-z0-9][a-z0-9._-]{0,63})\s+dot\s+(?P<tld>[a-z]{2,12})(?:\s+.*)?$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = OBFUSCATED_RE.captures(trimmed) else {
        return s.to_string();
    };
    let user = cap.name("user").map(|m| m.as_str()).unwrap_or("").trim();
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    if user.is_empty() {
        return prefix.to_string();
    }
    let mut words: Vec<&str> = prefix.split_whitespace().collect();
    if words.last().is_some_and(|w| w.eq_ignore_ascii_case(user)) {
        words.pop();
    }
    words.join(" ")
}

fn strip_trailing_at_sign(s: &str) -> String {
    let trimmed = s.trim_end();
    if let Some(stripped) = trimmed.strip_suffix('@') {
        return stripped.trim_end().to_string();
    }
    s.to_string()
}

fn strip_leading_product_operating_system_title(s: &str) -> String {
    static PRODUCT_OPERATING_SYSTEM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^the\s+(?:[\p{L}0-9._-]+\s+){1,5}operating\s+system(?:[.,]|\s|$)").unwrap()
    });

    if !PRODUCT_OPERATING_SYSTEM_RE.is_match(s.trim()) {
        return s.to_string();
    }

    if let Some((_, suffix)) = s.split_once(',') {
        return suffix.trim().to_string();
    }

    s.to_string()
}

fn strip_trailing_x509_dn_fields_from_holder(s: &str) -> String {
    static X509_DN_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)(?:\s*,\s*(?:OU|CN|O|C|L|ST)\s+.+)$").unwrap()
    });
    static TRAILING_ENDORSED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s+endorsed\s*$").unwrap());

    let trimmed = s.trim();
    if !trimmed.contains(", OU ")
        && !trimmed.contains(", CN ")
        && !trimmed.contains(", O ")
        && !trimmed.contains(", C ")
        && !trimmed.contains(", L ")
        && !trimmed.contains(", ST ")
    {
        return s.to_string();
    }

    let Some(cap) = X509_DN_TAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let mut prefix = cap
        .name("prefix")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prefix.is_empty() {
        return s.to_string();
    }
    if let Some(cap2) = TRAILING_ENDORSED_RE.captures(&prefix) {
        prefix = cap2
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or(prefix);
    }
    prefix
}

fn strip_trailing_ampas_acronym(s: &str) -> String {
    static AMPAS_SUFFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+\(?A\.M\.P\.A\.S\.?\)?\s*$").unwrap());
    AMPAS_SUFFIX_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_javadoc_tags(s: &str) -> String {
    static JAVADOC_TAGS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\s+@(?:generated|version|since|param|return|see)\b.*$").unwrap()
    });
    JAVADOC_TAGS_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_batch_comment_marker(s: &str) -> String {
    static BATCH_COMMENT_TAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\.?\s+@?rem\b.*$").unwrap());
    let trimmed = s.trim();
    let Some(cap) = BATCH_COMMENT_TAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    cap.name("prefix")
        .map(|m| m.as_str().trim_end_matches(&[' ', '.'][..]).to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_paren_years(s: &str) -> String {
    static PAREN_YEARS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^(?P<prefix>.+?)\s*\(\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*\s*\)\s*$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = PAREN_YEARS_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    prefix.to_string()
}

fn strip_trailing_bare_c_copyright_clause(s: &str) -> String {
    static BARE_C_CLAUSE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(c\)\s*(?:19\d{2}|20\d{2})\b.*$").unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = BARE_C_CLAUSE_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    prefix.to_string()
}

fn strip_trailing_single_digit_token(s: &str) -> String {
    static TRAILING_SINGLE_DIGIT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+?)\s+[1-9]\s*$").unwrap());
    let trimmed = s.trim();
    let Some(cap) = TRAILING_SINGLE_DIGIT_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    if !prefix.chars().any(|c| c.is_alphabetic()) {
        return s.to_string();
    }
    prefix.to_string()
}

mod author;
mod utils;

pub(crate) use author::looks_like_name_with_parenthesized_url;
pub use author::refine_author;
pub use utils::{
    remove_dupe_copyright_words, remove_some_extra_words_and_punct, strip_all_unbalanced_parens,
    strip_prefixes, strip_solo_quotes, strip_some_punct, strip_suffixes, strip_trailing_period,
};

#[cfg(test)]
use self::utils::{strip_leading_numbers, strip_unbalanced_parens};

use self::author::{normalize_angle_bracket_comma_spacing, strip_trailing_company_co_ltd};

use self::utils::{
    normalize_comma_spacing, normalize_whitespace, refine_names, remove_dupe_holder,
    strip_trailing_incomplete_as_represented_by, strip_trailing_url, strip_trailing_url_slash,
    truncate_long_words,
};

#[cfg(test)]
mod tests;
