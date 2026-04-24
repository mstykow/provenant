// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::copyright::detector::token_utils;

pub fn add_missing_holders_from_email_bearing_copyrights(
    copyrights: &[CopyrightDetection],
    _holders: &[HolderDetection],
) -> Vec<HolderDetection> {
    static COPYRIGHT_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright(?:\s*\(c\))?\s+[0-9][0-9,\-–/ ]*\s+(?:by\s+)?(?P<name>[^<]+?)\s*<[^>\s]*@[^>\s]*>\s*$",
        )
        .unwrap()
    });

    copyrights
        .iter()
        .filter_map(|c| {
            let cap = COPYRIGHT_NAME_EMAIL_RE.captures(c.copyright.trim())?;
            let raw_name = cap.name("name").map(|m| m.as_str()).unwrap_or("");
            let cleaned_name = normalize_email_copyright_holder_candidate(raw_name);
            if cleaned_name.is_empty() {
                return None;
            }

            let name = refine_holder_in_copyright_context(&cleaned_name)?;
            let domain_only = name.contains('.')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
            if domain_only {
                return None;
            }

            Some(HolderDetection {
                holder: name,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect()
}

pub fn normalize_email_copyright_holder_candidate(raw_name: &str) -> String {
    static LEADING_COPY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(c\)\s+").unwrap());
    static INLINE_YEAR_PERSON_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^(?P<prefix>.+?)\s+(?:19\d{2}|20\d{2})\s+(?P<name>[A-Z][\p{L}'\-.]+(?:\s+[A-Z][\p{L}'\-.]+){1,4})$",
        )
        .unwrap()
    });

    let mut cleaned = raw_name.trim_start_matches("by ").trim().to_string();
    cleaned = LEADING_COPY_RE.replace(&cleaned, "").trim().to_string();
    cleaned = token_utils::normalize_whitespace(&cleaned);

    if let Some(cap) = INLINE_YEAR_PERSON_RE.captures(&cleaned) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !name.is_empty() {
            cleaned = format!("{prefix} {name}");
        }
    }

    token_utils::normalize_whitespace(&cleaned)
}

pub fn add_bare_email_variants_for_escaped_angle_lines(
    raw_lines: &[&str],
    copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if raw_lines.is_empty() || copyrights.is_empty() {
        return Vec::new();
    }

    static ANGLE_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<\s*([^\s<>]+@[^\s<>]+)\s*>").unwrap());

    copyrights
        .iter()
        .filter_map(|c| {
            if c.start_line.get() != c.end_line.get() {
                return None;
            }
            let raw = raw_lines.get(c.start_line.get() - 1)?;
            let raw_lower = raw.to_ascii_lowercase();
            if !(raw_lower.contains("&lt;")
                && raw_lower.contains("&gt;")
                && raw_lower.contains('@'))
            {
                return None;
            }
            if !(c.copyright.contains('<')
                && c.copyright.contains('>')
                && c.copyright.contains('@'))
            {
                return None;
            }
            let bare = ANGLE_EMAIL_RE
                .replace_all(c.copyright.as_str(), "$1")
                .to_string();
            let refined = refine_copyright(&bare)?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect()
}

pub fn add_first_angle_email_only_variants(
    copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if copyrights.is_empty() {
        return Vec::new();
    }

    static MULTI_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>Copyright\b.*?<[^>\s]*@[^>\s]+>)(?:\s*,\s*.+)$").unwrap()
    });

    copyrights
        .iter()
        .filter_map(|c| {
            let trimmed = c.copyright.trim();
            let cap = MULTI_EMAIL_RE.captures(trimmed)?;
            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if prefix.is_empty() {
                return None;
            }
            let refined = refine_copyright(prefix)?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect()
}

pub fn drop_shadowed_angle_email_prefix_copyrights_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static EMAIL_TAIL_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^\s*,\s*(?:<?\.?[a-z0-9][a-z0-9._%+\-]{0,63}@[a-z0-9][a-z0-9._\-]{0,253}\.[a-z]{2,15}>?)(?:\s*,\s*(?:<?\.?[a-z0-9][a-z0-9._%+\-]{0,63}@[a-z0-9][a-z0-9._\-]{0,253}\.[a-z]{2,15}>?))*\s*$",
        )
        .unwrap()
    });

    *copyrights = group_by(std::mem::take(copyrights), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(_, v)| v)
    .flat_map(|group| {
        let texts: Vec<String> = group.iter().map(|c| c.copyright.clone()).collect();
        group
            .into_iter()
            .filter(|c| {
                let s = c.copyright.trim();
                if !s.ends_with('>') {
                    return true;
                }
                let mut has_longer = false;
                let mut has_email_only_extension = false;
                for other in &texts {
                    let o = other.trim();
                    if o == s {
                        continue;
                    }
                    if let Some(tail) = o.strip_prefix(s) {
                        has_longer = true;
                        let tail = tail.trim_end();
                        if EMAIL_TAIL_ONLY_RE.is_match(tail) {
                            has_email_only_extension = true;
                            break;
                        }
                    }
                }
                if !has_longer {
                    return true;
                }
                has_email_only_extension
            })
            .collect::<Vec<_>>()
    })
    .collect();
}

pub fn drop_shadowed_quote_before_email_variants_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static QUOTED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)'\s+(<[^>\s]*@[^>\s]+>|[^\s<>]*@[^\s<>]+)").unwrap());

    fn canonical(s: &str) -> String {
        token_utils::normalize_whitespace(&QUOTED_RE.replace_all(s, " $1"))
    }

    let by_span: HashMap<(usize, usize), HashSet<String>> = group_by(copyrights.clone(), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(span, group)| (span, group.into_iter().map(|c| c.copyright).collect()))
    .collect();

    copyrights.retain(|c| {
        if !c.copyright.contains('\'') || !c.copyright.contains('@') {
            return true;
        }
        let canon = canonical(&c.copyright);
        if canon == c.copyright {
            return true;
        }
        let span = (c.start_line.get(), c.end_line.get());
        !by_span.get(&span).is_some_and(|set| set.contains(&canon))
    });
}

pub fn add_at_affiliation_short_variants(
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if copyrights.is_empty() && holders.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let new_c = copyrights
        .iter()
        .filter_map(|c| {
            let (head, _tail) = c.copyright.split_once(" @ ")?;
            let refined = refine_copyright(head.trim_end())?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect();

    let new_h = holders
        .iter()
        .filter_map(|h| {
            let (head, tail) = h.holder.split_once(" @ ")?;
            if tail.contains('@') {
                return None;
            }
            let refined = refine_holder_in_copyright_context(head.trim_end())?;
            Some(HolderDetection {
                holder: refined,
                start_line: h.start_line,
                end_line: h.end_line,
            })
        })
        .collect();
    (new_c, new_h)
}

pub fn add_missing_copyrights_for_holder_lines_with_emails(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
) -> Vec<CopyrightDetection> {
    if prepared_cache.is_empty() || holders.is_empty() {
        return Vec::new();
    }

    let copyright_lines: HashSet<usize> = copyrights
        .iter()
        .filter(|c| c.start_line == c.end_line)
        .map(|c| c.start_line.get())
        .collect();

    holders
        .iter()
        .filter_map(|h| {
            if h.start_line != h.end_line {
                return None;
            }
            let line_number = h.start_line;
            if copyright_lines.contains(&line_number.get()) {
                return None;
            }
            let prepared = prepared_cache.get(line_number.get())?.trim();
            if prepared.is_empty()
                || !prepared.to_ascii_lowercase().contains("copyright")
                || !prepared.contains('@')
                || !prepared.chars().any(|c| c.is_ascii_digit())
            {
                return None;
            }

            let refined = refine_copyright(prepared)?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: line_number,
                end_line: line_number,
            })
        })
        .collect()
}

pub fn extend_inline_obfuscated_angle_email_suffixes(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
) {
    if copyrights.is_empty() {
        return;
    }

    let mut refined_line_cache: HashMap<usize, Option<String>> = HashMap::new();

    static OBF_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^(?:[,;:()\[\]{}]+\s*)?(?P<user>[a-z0-9][a-z0-9._-]{0,63})\s+at\s+(?P<host>[a-z0-9][a-z0-9._-]{0,63})\s+dot\s+(?P<tld>[a-z]{2,12})\s*$",
        )
        .unwrap()
    });

    for c in copyrights.iter_mut() {
        if c.start_line.get() != c.end_line.get() {
            continue;
        }
        if c.copyright.to_ascii_lowercase().contains(" at ")
            && c.copyright.to_ascii_lowercase().contains(" dot ")
        {
            continue;
        }

        let ln = c.start_line.get();
        let Some(refined_line) = refined_line_cache
            .entry(ln)
            .or_insert_with(|| {
                let line = prepared_cache.get(ln)?;
                let prepared = token_utils::normalize_whitespace(line);
                if !contains_obfuscated_email_markers(&prepared) {
                    return None;
                }
                refine_copyright(&prepared)
            })
            .as_deref()
        else {
            continue;
        };

        let refined_lower = refined_line.to_ascii_lowercase();
        if !refined_lower.contains(" at ") || !refined_lower.contains(" dot ") {
            continue;
        }

        let current = token_utils::normalize_whitespace(&c.copyright);
        let Some(tail) = refined_line.strip_prefix(current.as_str()) else {
            continue;
        };
        let tail = tail.trim();
        if tail.is_empty() {
            continue;
        }
        if OBF_TAIL_RE.captures(tail).is_none() {
            continue;
        }
        c.copyright = refined_line.to_string();
    }
}

pub fn contains_obfuscated_email_markers(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has_at = [" at ", "(at)", "[at]", "<at>", "{at}"]
        .iter()
        .any(|needle| lower.contains(needle));
    let has_dot = [" dot ", "(dot)", "[dot]", "<dot>", "{dot}"]
        .iter()
        .any(|needle| lower.contains(needle));
    has_at && has_dot
}

pub fn strip_lone_obfuscated_angle_email_user_tokens(
    raw_lines: &[&str],
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    if raw_lines.is_empty() {
        return;
    }
    if copyrights.is_empty() && holders.is_empty() {
        return;
    }

    static ANGLE_OBF_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)<\s*(?P<user>[a-z0-9][a-z0-9._-]{0,63})\s*(?:\[\s*at\s*\]|at)\s*(?P<host>[a-z0-9][a-z0-9._-]{0,63})\s*(?:\[\s*dot\s*\]|dot)\s*(?P<tld>[a-z]{2,12})\s*>",
        )
        .unwrap()
    });

    fn strip_trailing_word(s: &str, word: &str) -> Option<String> {
        if word.is_empty() {
            return None;
        }
        let trimmed = s.trim_end();
        let mut words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() < 2 {
            return None;
        }
        if !words.last().is_some_and(|w| w.eq_ignore_ascii_case(word)) {
            return None;
        }
        words.pop();
        let out = words.join(" ");
        if out.is_empty() { None } else { Some(out) }
    }

    for (idx, raw_line) in raw_lines.iter().enumerate() {
        let ln = idx + 1;
        let Some(cap) = ANGLE_OBF_RE.captures(raw_line) else {
            continue;
        };
        let user = cap.name("user").map(|m| m.as_str()).unwrap_or("").trim();
        if user.is_empty() {
            continue;
        }

        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line.get() == ln && c.end_line.get() == ln)
        {
            let lower = c.copyright.to_ascii_lowercase();
            if lower.contains(" at ") || lower.contains(" dot ") {
                continue;
            }
            let Some(stripped) = strip_trailing_word(c.copyright.as_str(), user) else {
                continue;
            };
            if let Some(refined) = refine_copyright(&stripped) {
                c.copyright = refined;
            } else {
                c.copyright = stripped;
            }
        }

        for h in holders
            .iter_mut()
            .filter(|h| h.start_line.get() == ln && h.end_line.get() == ln)
        {
            let lower = h.holder.to_ascii_lowercase();
            if lower.contains(" at ") || lower.contains(" dot ") {
                continue;
            }
            let Some(stripped) = strip_trailing_word(h.holder.as_str(), user) else {
                continue;
            };
            if let Some(refined) = refine_holder(&stripped) {
                h.holder = refined;
            } else {
                h.holder = stripped;
            }
        }
    }
}

pub fn add_at_domain_variants_for_short_net_angle_emails(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if copyrights.is_empty() {
        return Vec::new();
    }

    if !prepared_cache.contains_ci("pipe read code from") {
        return Vec::new();
    }

    static SHORT_NET_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<(?P<user>[a-z]{3})@(?P<domain>[^>\s]+\.net)>").unwrap());

    copyrights
        .iter()
        .filter_map(|c| {
            let cap = SHORT_NET_EMAIL_RE.captures(c.copyright.as_str())?;
            let user = cap.name("user").map(|m| m.as_str()).unwrap_or("").trim();
            let domain = cap.name("domain").map(|m| m.as_str()).unwrap_or("").trim();
            if user.is_empty() || domain.is_empty() {
                return None;
            }
            let replaced = SHORT_NET_EMAIL_RE
                .replace_all(c.copyright.as_str(), format!("@{domain}").as_str())
                .into_owned();
            let refined = refine_copyright(&replaced)?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect()
}

pub fn drop_shadowed_plain_email_prefix_copyrights_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static TRAILING_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>Copyright\b.*?\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})$")
            .unwrap()
    });

    *copyrights = group_by(std::mem::take(copyrights), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(_, v)| v)
    .flat_map(|group| {
        let all: Vec<String> = group.iter().map(|c| c.copyright.clone()).collect();
        let mut to_drop: HashSet<String> = HashSet::new();
        for s in &all {
            let s_trim = s.trim();
            let Some(cap) = TRAILING_EMAIL_RE.captures(s_trim) else {
                continue;
            };
            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if prefix.is_empty() {
                continue;
            }
            for other in &all {
                let o = other.trim();
                if o == prefix {
                    continue;
                }
                if o.starts_with(prefix)
                    && o[prefix.len()..].trim_start().starts_with(',')
                    && !o[prefix.len()..].contains('@')
                {
                    to_drop.insert(other.clone());
                }
            }
        }
        group
            .into_iter()
            .filter(|c| !to_drop.contains(&c.copyright))
            .collect::<Vec<_>>()
    })
    .collect();
}

pub fn normalize_french_support_disclaimer_copyrights(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?P<email>[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})").unwrap()
    });

    let existing_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let existing_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    let mut to_add_c = Vec::new();
    let mut to_add_h = Vec::new();
    for c in copyrights.iter() {
        let lower = c.copyright.to_ascii_lowercase();
        if !lower.contains("support ou responsabil") && !lower.contains("ce logiciel est derive") {
            continue;
        }
        let Some(m) = EMAIL_RE.find(c.copyright.as_str()) else {
            continue;
        };
        let email = m.as_str();
        let short_raw = c.copyright[..m.end()].trim_end();
        let Some(short) = refine_copyright(short_raw) else {
            continue;
        };
        let ckey = (c.start_line.get(), c.end_line.get(), short.clone());
        if !existing_c.contains(&ckey) {
            to_add_c.push(CopyrightDetection {
                copyright: short,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
        let Some(refined_email) = refine_holder_in_copyright_context(email) else {
            continue;
        };
        let hkey = (c.start_line.get(), c.end_line.get(), refined_email.clone());
        if !existing_h.contains(&hkey) {
            to_add_h.push(HolderDetection {
                holder: refined_email,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    copyrights.extend(to_add_c);
    holders.extend(to_add_h);

    copyrights.retain(|c| {
        let lower = c.copyright.to_ascii_lowercase();
        !lower.contains("support ou responsabil") && !lower.contains("ce logiciel est derive")
    });
    holders.retain(|h| {
        let lower = h.holder.to_ascii_lowercase();
        !lower.contains("support ou responsabil") && !lower.contains("ce logiciel est derive")
    });
}

pub fn drop_shadowed_inria_location_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    static INRIA_LOC_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+\bINRIA)\s+(?P<loc>[A-Z][a-z]{2,64})$").unwrap()
    });

    let by_span: HashMap<(usize, usize), HashSet<String>> = group_by(copyrights.clone(), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(span, group)| (span, group.into_iter().map(|c| c.copyright).collect()))
    .collect();

    copyrights.retain(|c| {
        let Some(cap) = INRIA_LOC_RE.captures(c.copyright.trim()) else {
            return true;
        };
        let prefix = cap
            .name("prefix")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim_end();
        if prefix.is_empty() {
            return true;
        }
        let span = (c.start_line.get(), c.end_line.get());
        !by_span.get(&span).is_some_and(|set| set.contains(prefix))
    });
}

pub fn add_email_holders_from_leading_email_comma_holders(
    holders: &[HolderDetection],
) -> Vec<HolderDetection> {
    if holders.len() < 2 {
        return Vec::new();
    }

    static LEADING_EMAIL_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<email>[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})\s*,\s+.+$").unwrap()
    });

    let mut exact_h_by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for h in holders.iter() {
        exact_h_by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .insert(h.holder.clone());
    }

    let mut to_add = Vec::new();
    for h in holders.iter() {
        let Some(cap) = LEADING_EMAIL_COMMA_RE.captures(h.holder.trim()) else {
            continue;
        };
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        if email.is_empty() {
            continue;
        }
        let Some(refined_email) = refine_holder_in_copyright_context(email) else {
            continue;
        };
        if exact_h_by_span
            .get(&(h.start_line.get(), h.end_line.get()))
            .is_some_and(|set| set.contains(&refined_email))
        {
            continue;
        }
        exact_h_by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .insert(refined_email.clone());
        to_add.push(HolderDetection {
            holder: refined_email,
            start_line: h.start_line,
            end_line: h.end_line,
        });
    }
    to_add
}

pub fn drop_shadowed_email_comma_holders_same_span(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    static LEADING_EMAIL_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<email>[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})\s*,\s+.+$").unwrap()
    });

    let by_span: HashMap<(usize, usize), HashSet<String>> =
        group_by(holders.clone(), |h| (h.start_line.get(), h.end_line.get()))
            .into_iter()
            .map(|(span, group)| (span, group.into_iter().map(|h| h.holder).collect()))
            .collect();

    holders.retain(|h| {
        let trimmed = h.holder.trim();
        let Some(cap) = LEADING_EMAIL_COMMA_RE.captures(trimmed) else {
            return true;
        };
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        if email.is_empty() || trimmed.eq_ignore_ascii_case(email) {
            return true;
        }
        let span = (h.start_line.get(), h.end_line.get());
        !by_span.get(&span).is_some_and(|set| set.contains(email))
    });
}

pub fn drop_shadowed_acronym_location_suffix_copyrights_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static ACR_LOC_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<prefix>.+\b(?P<acr>[A-Z]{2,10}))\s+(?P<loc>[A-Z][a-z]{2,})\s*$").unwrap()
    });

    *copyrights = group_by(std::mem::take(copyrights), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(_, v)| v)
    .flat_map(|group| {
        let set: HashSet<String> = group.iter().map(|c| c.copyright.clone()).collect();
        group
            .into_iter()
            .filter(|c| {
                let Some(cap) = ACR_LOC_RE.captures(c.copyright.trim()) else {
                    return true;
                };
                let prefix = cap
                    .name("prefix")
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .trim_end();
                if prefix.is_empty() {
                    return true;
                }
                if !prefix.contains('@') {
                    return true;
                }
                !set.contains(prefix)
            })
            .collect::<Vec<_>>()
    })
    .collect();
}

pub fn drop_url_embedded_c_symbol_false_positive_holders(
    content: &str,
    holders: &mut Vec<HolderDetection>,
) {
    static URL_EMBEDDED_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)https?://\S*\(c\)\S*").expect("valid URL embedded (c) regex")
    });

    let lines: Vec<&str> = content.lines().collect();
    holders.retain(|holder| {
        let Some(raw_line) = lines.get(holder.start_line.saturating_sub(1)) else {
            return true;
        };
        if !URL_EMBEDDED_C_RE.is_match(raw_line) {
            return true;
        }

        let value = holder.holder.trim();
        let is_single_token = !value.chars().any(char::is_whitespace);
        let is_lower_pathish = value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');

        !(is_single_token && is_lower_pathish)
    });
}

pub fn drop_url_embedded_suffix_variants_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !copyrights.is_empty() {
        let mut drop: HashSet<(usize, usize, String)> = HashSet::new();

        for longer in copyrights.iter() {
            let longer_lower = longer.copyright.to_ascii_lowercase();
            if !(longer_lower.contains("http://") || longer_lower.contains("https://")) {
                continue;
            }

            for shorter in copyrights.iter() {
                if longer.start_line != shorter.start_line || longer.end_line != shorter.end_line {
                    continue;
                }
                if longer.copyright == shorter.copyright {
                    continue;
                }

                let short = shorter.copyright.trim();
                if !longer.copyright.starts_with(short) {
                    continue;
                }

                let tail = longer.copyright[short.len()..]
                    .trim_start()
                    .to_ascii_lowercase();
                if tail.starts_with("see url")
                    || tail.starts_with("url ")
                    || tail.starts_with("http")
                {
                    drop.insert((
                        longer.start_line.get(),
                        longer.end_line.get(),
                        longer.copyright.clone(),
                    ));
                    break;
                }
            }
        }

        if !drop.is_empty() {
            copyrights.retain(|c| {
                !drop.contains(&(c.start_line.get(), c.end_line.get(), c.copyright.clone()))
            });
        }

        let mut drop_shorter: HashSet<(usize, usize, String)> = HashSet::new();
        for shorter in copyrights.iter() {
            let shorter_lower = shorter.copyright.to_ascii_lowercase();
            if !(shorter_lower.contains("http://") || shorter_lower.contains("https://")) {
                continue;
            }

            for longer in copyrights.iter() {
                if longer.start_line != shorter.start_line || longer.end_line != shorter.end_line {
                    continue;
                }
                if longer.copyright == shorter.copyright
                    || !longer.copyright.starts_with(&shorter.copyright)
                {
                    continue;
                }

                let tail = longer.copyright[shorter.copyright.len()..].trim();
                if tail.chars().any(|c| c.is_ascii_alphabetic())
                    && !tail.to_ascii_lowercase().starts_with("http")
                {
                    drop_shorter.insert((
                        shorter.start_line.get(),
                        shorter.end_line.get(),
                        shorter.copyright.clone(),
                    ));
                    break;
                }
            }
        }

        if !drop_shorter.is_empty() {
            copyrights.retain(|c| {
                !drop_shorter.contains(&(c.start_line.get(), c.end_line.get(), c.copyright.clone()))
            });
        }
    }

    if !holders.is_empty() {
        let mut drop: HashSet<(usize, usize, String)> = HashSet::new();

        for longer in holders.iter() {
            let longer_lower = longer.holder.to_ascii_lowercase();
            if !(longer_lower.contains(" see url")
                || longer_lower.contains(" http://")
                || longer_lower.contains(" https://"))
            {
                continue;
            }

            for shorter in holders.iter() {
                if longer.start_line != shorter.start_line || longer.end_line != shorter.end_line {
                    continue;
                }
                if longer.holder == shorter.holder {
                    continue;
                }

                let short = shorter.holder.trim();
                if !longer.holder.starts_with(short) {
                    continue;
                }

                let tail = longer.holder[short.len()..]
                    .trim_start()
                    .to_ascii_lowercase();
                if tail.starts_with("see url")
                    || tail.starts_with("url ")
                    || tail.starts_with("http")
                {
                    drop.insert((
                        longer.start_line.get(),
                        longer.end_line.get(),
                        longer.holder.clone(),
                    ));
                    break;
                }
            }
        }

        if !drop.is_empty() {
            holders.retain(|h| {
                !drop.contains(&(h.start_line.get(), h.end_line.get(), h.holder.clone()))
            });
        }

        let mut drop_url_only: HashSet<(usize, usize, String)> = HashSet::new();
        for shorter in holders.iter() {
            let shorter_lower = shorter.holder.to_ascii_lowercase();
            if !(shorter_lower.starts_with("http://") || shorter_lower.starts_with("https://")) {
                continue;
            }

            if holders.iter().any(|other| {
                other.start_line == shorter.start_line
                    && other.end_line == shorter.end_line
                    && other.holder != shorter.holder
                    && !(other.holder.to_ascii_lowercase().starts_with("http://")
                        || other.holder.to_ascii_lowercase().starts_with("https://"))
            }) {
                drop_url_only.insert((
                    shorter.start_line.get(),
                    shorter.end_line.get(),
                    shorter.holder.clone(),
                ));
            }
        }

        if !drop_url_only.is_empty() {
            holders.retain(|h| {
                !drop_url_only.contains(&(h.start_line.get(), h.end_line.get(), h.holder.clone()))
            });
        }
    }
}
