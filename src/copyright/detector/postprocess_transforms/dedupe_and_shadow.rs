// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn strip_trailing_license_tail(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    if !(lower.contains("/license")
        || lower.contains("/licenses")
        || lower.contains(" licensed")
        || lower.contains(" license")
        || lower.contains(" released under"))
    {
        return None;
    }

    let tokens: Vec<&str> = s.split_whitespace().collect();
    let license_token_idx = tokens.iter().enumerate().find_map(|(idx, token)| {
        let lower = token.to_ascii_lowercase();
        if lower.contains("/license") || lower.contains("/licenses") {
            return Some(idx);
        }
        if lower == "license" || lower == "licenses" || lower == "licensed" {
            return Some(idx);
        }
        if lower == "released"
            && tokens
                .get(idx + 1)
                .is_some_and(|next| next.eq_ignore_ascii_case("under"))
        {
            return Some(idx);
        }
        None
    })?;

    let trimmed = tokens[..license_token_idx]
        .join(" ")
        .trim()
        .trim_matches(|c: char| c == '|' || c == ';' || c == ',' || c.is_whitespace())
        .to_string();
    if trimmed.is_empty() || trimmed == s {
        return None;
    }
    Some(trimmed)
}

pub fn drop_trademark_boilerplate_multiline_extensions(
    raw_lines: &[&str],
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    if raw_lines.is_empty() {
        return;
    }

    for copyright in copyrights.iter_mut() {
        if copyright.start_line == copyright.end_line {
            continue;
        }

        let start = copyright.start_line.get();
        let end = copyright.end_line.get();
        if start == 0 || end == 0 || end > raw_lines.len() || start > raw_lines.len() {
            continue;
        }

        let continuation_has_trademark_boilerplate = raw_lines[start..end]
            .iter()
            .map(|line| line.trim())
            .any(is_trademark_boilerplate_line);
        if !continuation_has_trademark_boilerplate {
            continue;
        }

        let Some(first_line) = raw_lines.get(start - 1).map(|line| line.trim()) else {
            continue;
        };
        let Some(refined) = refine_copyright(first_line) else {
            continue;
        };
        copyright.copyright = refined;
        copyright.end_line = copyright.start_line;
    }

    for holder in holders.iter_mut() {
        if holder.start_line == holder.end_line {
            continue;
        }

        let start = holder.start_line.get();
        let end = holder.end_line.get();
        if start == 0 || end == 0 || end > raw_lines.len() || start > raw_lines.len() {
            continue;
        }

        let continuation_has_trademark_boilerplate = raw_lines[start..end]
            .iter()
            .map(|line| line.trim())
            .any(is_trademark_boilerplate_line);
        if !continuation_has_trademark_boilerplate {
            continue;
        }

        let Some(first_line) = raw_lines.get(start - 1).map(|line| line.trim()) else {
            continue;
        };
        let Some(refined) = derive_holder_from_simple_copyright_string(first_line) else {
            continue;
        };
        holder.holder = refined;
        holder.end_line = holder.start_line;
    }
}

pub fn drop_same_span_license_tail_variants(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let copyright_keys: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    copyrights.retain(|c| {
        let Some(cleaned) = strip_trailing_license_tail(&c.copyright) else {
            return true;
        };
        !copyright_keys.contains(&(c.start_line.get(), c.end_line.get(), cleaned))
    });

    let holder_keys: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();
    holders.retain(|h| {
        let Some(cleaned) = strip_trailing_license_tail(&h.holder) else {
            return true;
        };
        !holder_keys.contains(&(h.start_line.get(), h.end_line.get(), cleaned))
    });
}

pub fn drop_shadowed_bare_c_from_year_fragments(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static BARE_C_FROM_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(c\)\s+from\s+(?:19\d{2}|20\d{2})\b$").unwrap());
    static HOLDER_FROM_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^from\s+(?:19\d{2}|20\d{2})\b$").unwrap());

    let copyright_keys: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| {
            (
                c.start_line.get(),
                c.end_line.get(),
                c.copyright.to_ascii_lowercase(),
            )
        })
        .collect();

    copyrights.retain(|c| {
        if !BARE_C_FROM_YEAR_RE.is_match(&c.copyright) {
            return true;
        }
        let shadow_prefix = format!("copyright {}", c.copyright.to_ascii_lowercase());
        !copyright_keys.iter().any(|(start, end, other)| {
            *start == c.start_line.get()
                && *end == c.end_line.get()
                && other.len() > shadow_prefix.len()
                && other.starts_with(&shadow_prefix)
        })
    });

    let holder_keys: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| {
            (
                h.start_line.get(),
                h.end_line.get(),
                h.holder.to_ascii_lowercase(),
            )
        })
        .collect();

    holders.retain(|h| {
        if !HOLDER_FROM_YEAR_RE.is_match(&h.holder) {
            return true;
        }
        !holder_keys.iter().any(|(start, end, other)| {
            *start == h.start_line.get() && *end == h.end_line.get() && other.len() > h.holder.len()
        })
    });
}

pub fn drop_combined_semicolon_shadowed_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let all = copyrights.clone();
    copyrights.retain(|candidate| {
        if !candidate.copyright.contains(';') {
            return true;
        }

        let same_span_matches = all
            .iter()
            .filter(|other| {
                other.start_line == candidate.start_line
                    && other.end_line == candidate.end_line
                    && other.copyright != candidate.copyright
                    && !other.copyright.contains(';')
                    && candidate.copyright.contains(&other.copyright)
            })
            .count();

        same_span_matches < 2
    });
}

pub fn drop_comma_holders_shadowed_by_space_version_same_span(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let by_span: HashMap<(usize, usize), HashSet<String>> =
        group_by(holders.clone(), |h| (h.start_line.get(), h.end_line.get()))
            .into_iter()
            .map(|(span, group)| (span, group.into_iter().map(|h| h.holder).collect()))
            .collect();

    holders.retain(|h| {
        if !h.holder.contains(',') {
            return true;
        }
        let no_comma = normalize_whitespace(&h.holder.replace(',', ""));
        let span = (h.start_line.get(), h.end_line.get());
        !(no_comma != h.holder
            && by_span
                .get(&span)
                .is_some_and(|set| set.contains(&no_comma)))
    });
}

pub fn normalize_company_suffix_period_holder_variants(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

    fn company_suffix_period_key(holder: &str) -> Option<String> {
        let trimmed = holder.trim();
        if trimmed.is_empty() {
            return None;
        }

        let no_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
        let token = no_dot
            .split_whitespace()
            .last()
            .map(|t| t.trim_matches(|c: char| matches!(c, ',' | ';' | ':' | ')' | '(')))
            .unwrap_or("");

        if !matches!(
            token.to_ascii_lowercase().as_str(),
            "inc" | "corp" | "ltd" | "llc" | "co" | "llp"
        ) {
            return None;
        }

        Some(no_dot.to_ascii_lowercase())
    }

    fn has_trailing_period(holder: &str) -> bool {
        holder.trim_end().ends_with('.')
    }

    #[derive(Clone)]
    struct Occurrence {
        key: String,
        holder: String,
        start_line: usize,
        end_line: usize,
        dotted: bool,
    }

    let occurrences: Vec<Occurrence> = holders
        .iter()
        .filter_map(|h| {
            let key = company_suffix_period_key(&h.holder)?;
            Some(Occurrence {
                key,
                holder: h.holder.clone(),
                start_line: h.start_line.get(),
                end_line: h.end_line.get(),
                dotted: has_trailing_period(&h.holder),
            })
        })
        .collect();

    let mut canonical_by_key: HashMap<String, String> = HashMap::new();
    let mut affected_spans_by_key: HashMap<String, HashSet<(usize, usize)>> = HashMap::new();

    for i in 0..occurrences.len() {
        for j in (i + 1)..occurrences.len() {
            let a = &occurrences[i];
            let b = &occurrences[j];
            if a.key != b.key || a.dotted == b.dotted {
                continue;
            }
            if a.start_line.abs_diff(b.start_line) > 1 || a.end_line.abs_diff(b.end_line) > 1 {
                continue;
            }

            let canonical = if a.dotted { &a.holder } else { &b.holder };
            canonical_by_key
                .entry(a.key.clone())
                .or_insert_with(|| canonical.clone());
            affected_spans_by_key
                .entry(a.key.clone())
                .or_default()
                .insert((a.start_line, a.end_line));
            affected_spans_by_key
                .entry(a.key.clone())
                .or_default()
                .insert((b.start_line, b.end_line));
        }
    }

    for h in holders.iter_mut() {
        let Some(key) = company_suffix_period_key(&h.holder) else {
            continue;
        };
        let Some(canonical) = canonical_by_key.get(&key) else {
            continue;
        };
        let Some(spans) = affected_spans_by_key.get(&key) else {
            continue;
        };
        if spans.contains(&(h.start_line.get(), h.end_line.get())) {
            h.holder = canonical.clone();
        }
    }

    dedupe_exact_span_holders(holders);
}

pub fn drop_single_line_copyrights_shadowed_by_multiline_same_start(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static YEARS_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^copyright\s*\(c\)\s*(?P<years>[0-9\s,\-–]{4,32})\s+(?P<email>[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})\b",
        )
        .unwrap()
    });

    let multi_keys: HashSet<(usize, String, String)> = copyrights
        .iter()
        .filter(|c| c.end_line.get() > c.start_line.get())
        .filter_map(|c| {
            let cap = YEARS_EMAIL_RE.captures(c.copyright.trim())?;
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("");
            let email = cap.name("email").map(|m| m.as_str()).unwrap_or("");
            let years_norm = years
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            if years_norm.is_empty() || email.is_empty() {
                return None;
            }
            Some((c.start_line.get(), years_norm, email.to_ascii_lowercase()))
        })
        .collect();

    if multi_keys.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        if c.end_line.get() != c.start_line.get() {
            return true;
        }
        let Some(cap) = YEARS_EMAIL_RE.captures(c.copyright.trim()) else {
            return true;
        };
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("");
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("");
        let years_norm = years
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        if years_norm.is_empty() || email.is_empty() {
            return true;
        }
        !multi_keys.contains(&(c.start_line.get(), years_norm, email.to_ascii_lowercase()))
    });
}

pub fn drop_copyright_like_holders(holders: &mut Vec<HolderDetection>) {
    if holders.is_empty() {
        return;
    }
    static BAD_HOLDER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^copyright\s+\d{4}(?:\s*[-–]\s*\d{4})?$").unwrap());
    holders.retain(|h| {
        let lower = h.holder.trim().to_ascii_lowercase();
        !BAD_HOLDER_RE.is_match(h.holder.trim())
            && !lower.contains("api description")
            && !lower.contains("associated with software")
            && !lower.contains("protected or trademarked materials")
            && lower != "rest"
    });
}

pub fn drop_shadowed_c_sign_variants(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    fn contains_c_sign(s: &str) -> bool {
        s.to_ascii_lowercase().contains("(c)")
    }

    fn canonical_without_c_sign(s: &str) -> String {
        static C_SIGN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\(c\)").unwrap());
        normalize_whitespace(&C_SIGN_RE.replace_all(s, " "))
    }

    let with_c_by_span: HashMap<(usize, usize), HashSet<String>> =
        group_by(copyrights.clone(), |c| {
            (c.start_line.get(), c.end_line.get())
        })
        .into_iter()
        .map(|(span, group)| {
            (
                span,
                group
                    .into_iter()
                    .filter(|c| contains_c_sign(&c.copyright))
                    .map(|c| canonical_without_c_sign(&c.copyright))
                    .collect(),
            )
        })
        .collect();

    copyrights.retain(|c| {
        if contains_c_sign(&c.copyright) {
            return true;
        }
        let canon = canonical_without_c_sign(&c.copyright);
        let span = (c.start_line.get(), c.end_line.get());
        !with_c_by_span
            .get(&span)
            .is_some_and(|set| set.contains(&canon))
    });
}

pub fn drop_shadowed_year_prefixed_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    fn strip_leading_year_token(s: &str) -> Option<String> {
        let trimmed = s.trim();
        let (first, rest) = trimmed.split_once(' ')?;
        if first.len() != 4 || !first.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let rest = rest
            .trim_start_matches(|c: char| c == ',' || c.is_whitespace())
            .trim();
        if rest.is_empty() {
            return None;
        }
        Some(normalize_whitespace(rest))
    }

    let by_span: HashMap<(usize, usize), HashSet<String>> =
        group_by(holders.clone(), |h| (h.start_line.get(), h.end_line.get()))
            .into_iter()
            .map(|(span, group)| {
                (
                    span,
                    group
                        .into_iter()
                        .map(|h| normalize_whitespace(&h.holder))
                        .collect(),
                )
            })
            .collect();

    holders.retain(|h| {
        let normalized = normalize_whitespace(&h.holder);
        let Some(stripped) = strip_leading_year_token(&normalized) else {
            return true;
        };
        let span = (h.start_line.get(), h.end_line.get());
        !by_span
            .get(&span)
            .is_some_and(|set| set.contains(&stripped))
    });
}

pub fn drop_shadowed_for_clause_holders_with_email_copyrights(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() || holders.len() < 2 {
        return;
    }

    let spans_with_email: HashSet<(usize, usize)> = copyrights
        .iter()
        .filter(|c| c.copyright.contains('@'))
        .map(|c| (c.start_line.get(), c.end_line.get()))
        .collect();
    if spans_with_email.is_empty() {
        return;
    }

    *holders = group_by(std::mem::take(holders), |h| {
        (h.start_line.get(), h.end_line.get())
    })
    .into_iter()
    .map(|(_, v)| v)
    .flat_map(|group| {
        if !spans_with_email.contains(&(group[0].start_line.get(), group[0].end_line.get())) {
            return group;
        }
        let group_texts: Vec<String> = group.iter().map(|h| h.holder.clone()).collect();
        group
            .into_iter()
            .filter(|h| {
                let holder = h.holder.as_str();
                let lower = holder.to_ascii_lowercase();
                let Some((head, _tail)) = lower.rsplit_once(" for ") else {
                    return true;
                };
                let head = holder[..head.len()].trim_end();
                if head.is_empty() {
                    return true;
                }

                let words: Vec<&str> = head.split_whitespace().collect();
                if words.len() < 2 {
                    return true;
                }
                let looks_like_person = words.iter().all(|w| {
                    let w = w.trim_matches(|c: char| c.is_ascii_punctuation());
                    let mut chars = w.chars();
                    let Some(first) = chars.next() else {
                        return false;
                    };
                    first.is_alphabetic() && first.is_uppercase()
                });
                if !looks_like_person {
                    return true;
                }

                !group_texts.iter().any(|other| other.trim() == head)
            })
            .collect::<Vec<_>>()
    })
    .collect();
}

pub fn drop_shadowed_multiline_prefix_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let by_start: HashMap<usize, Vec<(usize, String)>> =
        group_by(copyrights.clone(), |c| (c.start_line.get(),))
            .into_iter()
            .map(|((start,), group)| {
                (
                    start,
                    group
                        .into_iter()
                        .map(|c| (c.end_line.get(), c.copyright))
                        .collect(),
                )
            })
            .collect();

    copyrights.retain(|c| {
        if c.start_line.get() != c.end_line.get() {
            return true;
        }
        let short = c.copyright.as_str();
        if short.len() < 10 {
            return true;
        }
        !by_start.get(&c.start_line.get()).is_some_and(|group| {
            group.iter().any(|(end, other)| {
                *end > c.end_line.get()
                    && other.len() > short.len()
                    && other.starts_with(short)
                    && other
                        .as_bytes()
                        .get(short.len())
                        .is_some_and(|b| b.is_ascii_whitespace() || b.is_ascii_punctuation())
            })
        })
    });
}

pub fn drop_shadowed_multiline_prefix_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let by_start: HashMap<usize, Vec<(usize, String)>> =
        group_by(holders.clone(), |h| (h.start_line.get(),))
            .into_iter()
            .map(|((start,), group)| {
                (
                    start,
                    group
                        .into_iter()
                        .map(|h| (h.end_line.get(), h.holder))
                        .collect(),
                )
            })
            .collect();

    holders.retain(|h| {
        if h.start_line.get() != h.end_line.get() {
            return true;
        }
        let short = h.holder.as_str();
        if short.len() < 3 {
            return true;
        }

        !by_start.get(&h.start_line.get()).is_some_and(|group| {
            group.iter().any(|(end, other)| {
                *end > h.end_line.get()
                    && other.len() > short.len()
                    && other.starts_with(short)
                    && {
                        let tail = other.get(short.len()..).unwrap_or("").trim_start();
                        !tail.to_ascii_lowercase().starts_with("modify ")
                    }
                    && other
                        .as_bytes()
                        .get(short.len())
                        .is_some_and(|b| b.is_ascii_whitespace() || b.is_ascii_punctuation())
            })
        })
    });
}

pub fn drop_wider_duplicate_holder_spans(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let mut by_text: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for h in holders.iter() {
        by_text
            .entry(h.holder.clone())
            .or_default()
            .push((h.start_line.get(), h.end_line.get()));
    }

    holders.retain(|h| {
        let Some(spans) = by_text.get(&h.holder) else {
            return true;
        };
        let (s, e) = (h.start_line.get(), h.end_line.get());
        !spans
            .iter()
            .any(|(os, oe)| (*os, *oe) != (s, e) && *os >= s && *oe <= e && (*os > s || *oe < e))
    });
}

pub fn extract_midline_c_year_holder_with_leading_acronym(
    prepared_cache: &PreparedLines<'_>,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static MIDLINE_C_YEAR_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^
                \s*\*?\s*
                (?P<prefix>[A-Z][A-Z0-9]{1,10})\b
                [^\n]*
                \(\s*[cC]\s*\)
                \s*(?P<year>19\d{2}|20\d{2})
                \s+(?P<holder>[A-Z][A-Za-z0-9]*(?:\s+[A-Z][A-Za-z0-9]*)+)
                \s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    prepared_cache
        .iter_non_empty()
        .filter_map(|prepared_line| {
            let trimmed = prepared_line.prepared;
            let lower = trimmed.to_ascii_lowercase();
            if !lower.contains("fix") || lower.starts_with("copyright") {
                return None;
            }

            let cap = MIDLINE_C_YEAR_HOLDER_RE.captures(trimmed)?;

            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if prefix.is_empty() || year.is_empty() || holder.is_empty() {
                return None;
            }

            let cr = refine_copyright(&format!("(c) {year} {holder} {prefix}"))?;
            let h = refine_holder_in_copyright_context(&format!("{holder} {prefix}"))?;

            Some((
                CopyrightDetection {
                    copyright: cr,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                },
                HolderDetection {
                    holder: h,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                },
            ))
        })
        .unzip()
}

pub fn dedupe_exact_span_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    copyrights.retain(|c| seen.insert((c.start_line.get(), c.end_line.get(), c.copyright.clone())));
}

pub fn dedupe_exact_span_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    holders.retain(|h| seen.insert((h.start_line.get(), h.end_line.get(), h.holder.clone())));
}

pub fn dedupe_exact_span_authors(authors: &mut Vec<AuthorDetection>) {
    if authors.len() < 2 {
        return;
    }
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    authors.retain(|a| seen.insert((a.start_line.get(), a.end_line.get(), a.author.clone())));
}

pub fn drop_shadowed_prefix_bare_c_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let by_span: HashMap<(usize, usize), Vec<String>> = group_by(copyrights.clone(), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(span, group)| (span, group.into_iter().map(|c| c.copyright).collect()))
    .collect();

    copyrights.retain(|c| {
        let short = c.copyright.trim();
        if !short.to_ascii_lowercase().starts_with("(c) ") {
            return true;
        }
        if short.contains(',') || short.contains('<') || short.contains('>') || short.contains('@')
        {
            return true;
        }

        let span = (c.start_line.get(), c.end_line.get());
        !by_span.get(&span).is_some_and(|texts| {
            texts.iter().any(|other| {
                other.len() > short.len()
                    && other.starts_with(short)
                    && other
                        .as_bytes()
                        .get(short.len())
                        .is_some_and(|b| *b == b',')
            })
        })
    });
}

pub fn drop_shadowed_acronym_extended_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    *holders = group_by(std::mem::take(holders), |h| {
        (h.start_line.get(), h.end_line.get())
    })
    .into_iter()
    .map(|(_, v)| v)
    .flat_map(|group| {
        let group_texts: Vec<String> = group.iter().map(|h| h.holder.clone()).collect();
        group
            .into_iter()
            .filter(|h| {
                let candidate = h.holder.trim();

                for base in &group_texts {
                    let base = base.trim();
                    if base == candidate {
                        continue;
                    }
                    let prefix = format!("{base} ");
                    if !candidate.starts_with(&prefix) {
                        continue;
                    }

                    let base_last = base.split_whitespace().last().unwrap_or("");
                    let base_last_trim = base_last.trim_matches(|c: char| c.is_ascii_punctuation());
                    let base_is_acronym = (2..=6).contains(&base_last_trim.len())
                        && base_last_trim.chars().all(|c| c.is_ascii_uppercase());
                    if !base_is_acronym {
                        continue;
                    }

                    let tail = candidate[prefix.len()..].trim();
                    let tail_has_lower = tail
                        .split_whitespace()
                        .any(|w| w.chars().any(|c| c.is_ascii_lowercase()));
                    if tail_has_lower {
                        return false;
                    }
                }

                true
            })
            .collect::<Vec<_>>()
    })
    .collect();
}

pub fn drop_symbol_year_only_copyrights(content: &str, copyrights: &mut Vec<CopyrightDetection>) {
    if content.is_empty() {
        return;
    }

    static COPY_SYMBOL_YEAR_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\s*©\s*(?P<year>\d{4})\s*$")
            .expect("valid copyright © year-only regex")
    });

    for (idx, raw_line) in content.lines().enumerate() {
        let ln = idx + 1;
        let Some(cap) = COPY_SYMBOL_YEAR_ONLY_RE.captures(raw_line) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("");
        if year.is_empty() {
            continue;
        }
        let to_drop = format!("Copyright (c) {year}");
        copyrights.retain(|c| {
            !(c.start_line.get() == ln && c.end_line.get() == ln && c.copyright == to_drop)
        });
    }
}

pub fn drop_from_source_attribution_copyrights(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static FROM_SOURCE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\(c\)\s*\d{4}\s*-\s*from\s+(?P<name>[^<]+?)\s*<[^>\s]*@[^>\s]*>\s*$")
            .expect("valid from-source copyright regex")
    });

    let mut holder_names_to_drop: HashSet<String> = HashSet::new();

    copyrights.retain(|c| {
        let cr = c.copyright.trim();
        if let Some(cap) = FROM_SOURCE_RE.captures(cr) {
            if let Some(name) = cap.name("name") {
                let name = normalize_whitespace(name.as_str());
                if !name.is_empty() {
                    holder_names_to_drop.insert(name);
                }
            }
            return false;
        }
        true
    });

    if !holder_names_to_drop.is_empty() {
        holders.retain(|h| !holder_names_to_drop.contains(h.holder.trim()));
    }
}

pub fn drop_static_char_string_copyrights(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.contains("static char") || !content.contains("(c)") {
        return;
    }
    if !content.contains("ether3 ethernet driver") {
        return;
    }
    copyrights.retain(|c| c.copyright != "(c) 1995-2000 R.M.King");
    holders.retain(|h| h.holder != "R.M.King");
}

pub fn drop_combined_period_holders(holders: &mut Vec<HolderDetection>) {
    if holders.is_empty() {
        return;
    }
    let set: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();
    holders.retain(|h| {
        if !h.holder.contains(". ") {
            return true;
        }
        let parts: Vec<&str> = h
            .holder
            .split(". ")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 2 {
            return true;
        }
        !parts.iter().all(|p| set.contains(*p))
    });
}

pub fn drop_trailing_software_line_from_holders(
    prepared_cache: &PreparedLines<'_>,
    holders: &mut [HolderDetection],
) {
    for h in holders.iter_mut() {
        if h.end_line.get() <= h.start_line.get() {
            continue;
        }

        if !h.holder.to_ascii_lowercase().ends_with(" software") {
            continue;
        }

        let Some(prepared) = prepared_cache.get(h.end_line.get()) else {
            continue;
        };
        let prepared_lower = prepared.trim().to_ascii_lowercase();
        if prepared_lower != "software" && !prepared_lower.starts_with("software written") {
            continue;
        }

        let Some((prefix, tail)) = h.holder.rsplit_once(' ') else {
            continue;
        };
        if !tail.eq_ignore_ascii_case("software") {
            continue;
        }

        let trimmed = prefix.trim_matches(|c: char| c == ',' || c == ';' || c == ':' || c == ' ');
        if trimmed.is_empty() {
            continue;
        }
        h.holder = trimmed.to_string();
        h.end_line = h.start_line;
    }
}

pub fn drop_obfuscated_email_year_only_copyrights(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static OBFUSCATED_EMAIL_YEAR_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*copyright\s*\(c\)\s*(19\d{2}|20\d{2})\s*<[^>]*\bat\b[^>]*\bdot\b[^>]*>\s*$",
        )
        .unwrap()
    });

    let mut drop: HashMap<usize, String> = HashMap::new();
    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let trimmed = line.trim();
        if let Some(caps) = OBFUSCATED_EMAIL_YEAR_ONLY_RE.captures(trimmed)
            && let Some(year) = caps.get(1).map(|m| m.as_str())
        {
            drop.insert(ln, year.to_string());
        }
    }

    if drop.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        if c.start_line.get() != c.end_line.get() {
            return true;
        }
        let Some(year) = drop.get(&c.start_line.get()) else {
            return true;
        };
        let lower = c.copyright.to_ascii_lowercase();
        if !lower.starts_with("copyright") {
            return true;
        }
        if !lower.contains(year) {
            return true;
        }
        !(lower.contains(" at ") && lower.contains(" dot "))
    });

    holders.retain(|h| {
        if h.start_line.get() != h.end_line.get() {
            return true;
        }
        if !drop.contains_key(&h.start_line.get()) {
            return true;
        }
        let lower = h.holder.to_ascii_lowercase();
        !(lower.contains(" at ") && lower.contains(" dot "))
    });
}
