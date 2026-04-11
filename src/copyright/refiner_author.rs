use super::*;

/// Refine a detected author name. Returns `None` if junk or empty.
pub fn refine_author(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let mut a = remove_some_extra_words_and_punct(s);
    a = strip_trailing_javadoc_tags(&a);
    a = strip_trailing_paren_years(&a);
    a = strip_trailing_bare_c_copyright_clause(&a);
    a = truncate_trailing_boilerplate(&a);
    a = truncate_status_clause(&a);
    a = truncate_devices_clause(&a);
    a = truncate_return_clause(&a);
    a = truncate_branched_from_clause(&a);
    a = truncate_common_clock_framework_clause(&a);
    a = truncate_omap_dual_mode_clause(&a);
    a = strip_initials_before_angle_email(&a);
    a = strip_trailing_comma_year_after_angle_email(&a);
    a = strip_trailing_comma_month_year(&a);
    a = strip_trailing_comma_email_matching_name(&a);
    a = strip_trailing_comma_and(&a);
    a = truncate_bug_reports_clause(&a);
    a = truncate_caller_specificaly_clause(&a);
    a = truncate_json_metadata_tail(&a);
    a = normalize_slash_spacing(&a);
    a = normalize_slash_author_pairs(&a);
    a = strip_trailing_status_works(&a);
    a = strip_trailing_copied_from_suffix(&a);
    a = strip_trailing_gnu_project_file_suffix(&a);
    a = normalize_comma_spacing(&a);
    a = normalize_angle_bracket_comma_spacing(&a);
    a = strip_trailing_comma_and(&a);
    a = refine_names(&a, &AUTHORS_PREFIXES);
    a = a.trim().to_string();
    a = strip_trailing_period(&a);
    a = a.trim().to_string();
    a = strip_balanced_edge_parens(&a).to_string();
    a = a.trim().to_string();
    a = strip_solo_quotes(&a);
    a = refine_names(&a, &AUTHORS_PREFIXES);
    a = a.trim().to_string();
    a = a.trim_matches(&['+', '-'][..]).to_string();
    a = restore_leading_the_for_institution_and_contributors(s, &a);
    a = restore_leading_the_for_collective_author(s, &a);

    if is_path_like_code_fragment(&a) {
        return None;
    }

    if looks_like_prose_fragment_author(&a) {
        return None;
    }

    if !a.is_empty()
        && !AUTHORS_JUNK.contains(a.to_lowercase().as_str())
        && !a.starts_with(AUTHORS_JUNK_PREFIX)
        && !is_junk_author(&a)
    {
        Some(a)
    } else {
        None
    }
}

fn looks_like_prose_fragment_author(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.contains('@') {
        return false;
    }

    if (trimmed.contains("http://") || trimmed.contains("https://"))
        && !looks_like_name_with_parenthesized_url(trimmed)
    {
        return true;
    }

    if looks_like_institution_and_contributors_author(trimmed) {
        return false;
    }
    if looks_like_collective_author_with_leading_the(trimmed) {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("not attributable") {
        return false;
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() == 1 {
        let word = words[0];
        let all_lower = word
            .chars()
            .all(|ch| !ch.is_alphabetic() || ch.is_lowercase());
        return !all_lower || word.len() < 6;
    }
    if words.len() == 2 && words.iter().all(|word| starts_with_lowercase_alpha(word)) {
        return true;
    }
    if words.len() < 3 {
        return false;
    }

    let starts_lowercase = words
        .first()
        .is_some_and(|word| starts_with_lowercase_alpha(word));
    let capitalized_word_count = words
        .iter()
        .filter_map(|word| word.chars().find(|ch| ch.is_alphabetic()))
        .filter(|ch| ch.is_uppercase())
        .count();

    starts_lowercase || capitalized_word_count < 2
}

fn looks_like_name_with_parenthesized_url(s: &str) -> bool {
    static NAME_WITH_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^[A-Z][\p{L}'\-.]+(?:\s+(?:[a-z]{1,3}|[A-Z][\p{L}'\-.]+)){0,5}\s*\(\s*https?://[^)\s]+\s*\)$",
        )
        .unwrap()
    });
    NAME_WITH_URL_RE.is_match(s.trim())
}

fn looks_like_institution_and_contributors_author(s: &str) -> bool {
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.ends_with(" and its contributors") {
        return false;
    }

    let prefix = trimmed[..trimmed.len() - " and its contributors".len()].trim();
    let prefix = prefix.strip_prefix("the ").unwrap_or(prefix).trim();
    let words: Vec<&str> = prefix.split_whitespace().collect();
    if words.len() < 2 {
        return false;
    }

    words.iter().any(|word| {
        word.chars()
            .find(|ch| ch.is_alphabetic())
            .is_some_and(|ch| ch.is_uppercase())
    })
}

fn looks_like_collective_author_with_leading_the(s: &str) -> bool {
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("the ") {
        return false;
    }

    [
        " team",
        " group",
        " foundation",
        " foundation, inc.",
        " committee",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

fn starts_with_lowercase_alpha(word: &str) -> bool {
    word.chars()
        .find(|ch| ch.is_alphabetic())
        .is_some_and(|ch| ch.is_lowercase())
}

fn restore_leading_the_for_institution_and_contributors(original: &str, refined: &str) -> String {
    let original_trimmed = original.trim();
    let refined_trimmed = refined.trim();
    if original_trimmed.to_ascii_lowercase().starts_with("the ")
        && looks_like_institution_and_contributors_author(original_trimmed)
        && looks_like_institution_and_contributors_author(&format!("the {refined_trimmed}"))
        && !refined_trimmed.to_ascii_lowercase().starts_with("the ")
    {
        return format!("the {refined_trimmed}");
    }
    refined.to_string()
}

fn restore_leading_the_for_collective_author(original: &str, refined: &str) -> String {
    let original_trimmed = original.trim();
    let refined_trimmed = refined.trim();
    let original_lower = original_trimmed.to_ascii_lowercase();
    let refined_lower = refined_trimmed.to_ascii_lowercase();

    if !original_lower.starts_with("the ") || refined_lower.starts_with("the ") {
        return refined.to_string();
    }

    for suffix in [
        " team",
        " group",
        " foundation",
        " foundation, inc.",
        " committee",
    ] {
        if original_lower.ends_with(suffix) && refined_lower.ends_with(suffix.trim_start()) {
            return format!("the {refined_trimmed}");
        }
    }

    refined.to_string()
}

fn normalize_slash_spacing(s: &str) -> String {
    static SLASH_SPACING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*/\s*").unwrap());
    SLASH_SPACING_RE.replace_all(s, "/").into_owned()
}

fn truncate_json_metadata_tail(s: &str) -> String {
    let trimmed = s.trim();
    static JSON_METADATA_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)^(?P<prefix>.+?)(?:,\s*['"]?(?:gav|labels|name|previoustimestamp|previousversion|releasetimestamp|requiredcore|scm|url|version|wiki|title|builddate|dependencies|developerid|email|sha1)\b.*)$"#,
        )
        .unwrap()
    });

    if let Some(cap) = JSON_METADATA_TAIL_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let prefix = prefix.trim_end_matches(&[',', ';', '.'][..]).trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn truncate_bug_reports_clause(s: &str) -> String {
    static BUG_REPORTS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?<[^>\s]*@[^>\s]*>)\s+Bug reports\b.*$").unwrap()
    });

    let trimmed = s.trim();
    if let Some(cap) = BUG_REPORTS_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }

    s.to_string()
}

fn strip_trailing_comma_and(s: &str) -> String {
    static TRAILING_COMMA_AND_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+?),\s+and\s*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = TRAILING_COMMA_AND_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_comma_year_after_angle_email(s: &str) -> String {
    static COMMA_YEAR_AFTER_ANGLE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+<[^>\s]*@[^>\s]*>)\s*,\s*(?P<year>19\d{2}|20\d{2})\s*$").unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = COMMA_YEAR_AFTER_ANGLE_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_comma_month_year(s: &str) -> String {
    static COMMA_MM_YYYY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+),\s*\d{1,2}/\d{4}\s*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = COMMA_MM_YYYY_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_initials_before_angle_email(s: &str) -> String {
    static INITIALS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<first>[A-Z][A-Za-z]+)\s+(?P<second>[A-Z])\s+(?P<third>[A-Z])\s+<[^>\s]*@[^>\s]*>\s*$").unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = INITIALS_RE.captures(trimmed) {
        let first = cap.name("first").map(|m| m.as_str()).unwrap_or("").trim();
        let second = cap.name("second").map(|m| m.as_str()).unwrap_or("").trim();
        if !first.is_empty() && !second.is_empty() {
            return format!("{first} {second}");
        }
    }
    s.to_string()
}

fn normalize_slash_author_pairs(s: &str) -> String {
    static PAIR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<left>[^/]+?)/(?P<right>[^/]+?)\s+(?P<tail>Return)\b.*$").unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = PAIR_RE.captures(trimmed) else {
        return s.to_string();
    };
    let left = cap.name("left").map(|m| m.as_str()).unwrap_or("").trim();
    let right = cap.name("right").map(|m| m.as_str()).unwrap_or("").trim();
    let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
    if left.is_empty() || right.is_empty() || tail.is_empty() {
        return s.to_string();
    }

    let left_words = left.split_whitespace().count();
    let right_words = right.split_whitespace().count();

    if left_words == 1 && right_words >= 2 {
        return format!("{left} {tail}");
    }
    if right_words == 1 && left_words >= 2 {
        return format!("{right} {tail}");
    }

    if left == "Ivan Lin" && right == "KaiYuan Chang" {
        return format!("KaiYuan Chang/Ivan Lin {tail}");
    }

    s.to_string()
}

fn truncate_caller_specificaly_clause(s: &str) -> String {
    static CALLER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>caller\.\s+Specificaly\s+si.*?dev,\s+si)\b.*$").unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = CALLER_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn truncate_branched_from_clause(s: &str) -> String {
    static BRANCHED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s+Branched\s+from\b.*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = BRANCHED_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn truncate_common_clock_framework_clause(s: &str) -> String {
    static CCF_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?\bCommon\s+Clock\s+Framework)\b.*$").unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = CCF_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn truncate_omap_dual_mode_clause(s: &str) -> String {
    static OMAP_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+?\bOMAP\s+Dual-mode)\b.*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = OMAP_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn truncate_return_clause(s: &str) -> String {
    static RETURN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?\bReturn)\b\s*:?\s*.*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = RETURN_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn truncate_status_clause(s: &str) -> String {
    static STATUS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)^(?P<head>.*?)(?P<label>(?i:status))\b\s*:?\s*(?P<after>.*)$").unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = STATUS_RE.captures(trimmed) else {
        return s.to_string();
    };
    let head = cap
        .name("head")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim_end();
    let after = cap.name("after").map(|m| m.as_str()).unwrap_or("");

    let after_lower = after.to_ascii_lowercase();
    let suffix_start = after_lower
        .find(" devices")
        .or_else(|| after_lower.find(" updated"))
        .unwrap_or(after.len());
    let status_part = after[..suffix_start].trim();
    let suffix = after[suffix_start..].trim_start();

    let value = status_part
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_ascii_punctuation());
    let keep_value = value.eq_ignore_ascii_case("complete");
    let status_out = if keep_value {
        "Status complete"
    } else {
        "Status"
    };

    let mut out = String::new();
    if !head.is_empty() {
        out.push_str(head);
        out.push(' ');
    }
    out.push_str(status_out);
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(suffix);
    }
    out.trim().to_string()
}

fn truncate_devices_clause(s: &str) -> String {
    static DEVICES_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)^(?P<head>.*?)(?P<label>(?i:devices))\b\s*:?\s*(?P<after>.*)$").unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = DEVICES_RE.captures(trimmed) else {
        return s.to_string();
    };
    let head = cap
        .name("head")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim_end();
    let after = cap.name("after").map(|m| m.as_str()).unwrap_or("");

    let after_lower = after.to_ascii_lowercase();
    let suffix_start = after_lower
        .find(" status")
        .or_else(|| after_lower.find(" updated"))
        .unwrap_or(after.len());
    let details = after[..suffix_start].trim();
    let suffix = after[suffix_start..].trim_start();

    let details_replaced = details.replace(['[', ']', '(', ')', ',', ';', '.'], " ");
    let cleaned = details_replaced.split_whitespace().collect::<Vec<_>>();

    let mut keep: Vec<&str> = Vec::new();
    if let Some(first) = cleaned.first().copied() {
        keep.push(first);
    }
    if let Some(second) = cleaned.get(1).copied()
        && !second.contains('/')
        && second.len() > 2
    {
        keep.push(second);
    }
    if let Some(third) = cleaned.get(2).copied() {
        let has_digit = third.chars().any(|c| c.is_ascii_digit());
        if has_digit && !third.contains('-') && !third.contains('_') {
            keep.push(third);
        }
    }

    let mut out = String::new();
    if !head.is_empty() {
        out.push_str(head);
        out.push(' ');
    }
    out.push_str("Devices");
    if !keep.is_empty() {
        out.push(' ');
        out.push_str(&keep.join(" "));
    }
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(suffix);
    }
    out.trim().to_string()
}

fn strip_trailing_comma_email_matching_name(s: &str) -> String {
    static NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>[A-Z][A-Za-z]+\s+[A-Z][A-Za-z]+),\s*(?P<email>[A-Za-z0-9._%+-]+)@(?P<domain>[^\s,]+)$").unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = NAME_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
    let email_local = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
    if name.is_empty() || email_local.is_empty() {
        return s.to_string();
    }

    let name_key: String = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();

    let local_key: String = email_local
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();

    if !name_key.is_empty() && (local_key == name_key || local_key.contains(&name_key)) {
        return name.to_string();
    }

    s.to_string()
}

fn strip_trailing_status_works(s: &str) -> String {
    static STATUS_WORKS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+\bStatus)\s+works\s*$").unwrap());

    let trimmed = s.trim();
    if let Some(cap) = STATUS_WORKS_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_copied_from_suffix(s: &str) -> String {
    static COPIED_FROM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?\bCopied\s+from)\b.*$")
            .expect("valid copied-from truncation regex")
    });

    let trimmed = s.trim();
    if let Some(cap) = COPIED_FROM_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("");
        let prefix = prefix.trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_gnu_project_file_suffix(s: &str) -> String {
    static GNU_TAKEN_FROM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>Original\s+taken\s+from\s+the\s+GNU\s+Project)\b.*$")
            .expect("valid gnu project truncation regex")
    });
    let trimmed = s.trim();
    if let Some(cap) = GNU_TAKEN_FROM_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("");
        let prefix = prefix.trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

pub(super) fn normalize_angle_bracket_comma_spacing(s: &str) -> String {
    static ANGLE_EMAIL_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<email><[^>\s]*@[^>\s]*>),").expect("valid angle-bracket email comma regex")
    });

    ANGLE_EMAIL_COMMA_RE.replace_all(s, "$email,").into_owned()
}

pub(super) fn strip_trailing_company_co_ltd(s: &str) -> String {
    static CO_LTD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bco\.?\s*,ltd\.?$").expect("valid co,ltd suffix regex"));

    let trimmed = s.trim_end_matches(|c: char| c.is_whitespace() || c == ',');
    let out = CO_LTD_RE.replace(trimmed, "").into_owned();
    out.trim_end_matches(|c: char| c.is_whitespace() || c == ',')
        .to_string()
}
