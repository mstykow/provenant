// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub fn merge_multiline_person_year_copyright_continuations(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.len() < 2 {
        return;
    }

    static FIRST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s+(?P<name>.+?)\s*(?:<[^>]+>)?\s*,\s*(?P<year>\d{4})\s*$",
        )
        .unwrap()
    });
    static SECOND_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<name>[\p{Lu}][^<,]{2,64}(?:\s+[\p{Lu}][^<,]{2,64})*)\s*(?:<[^>]+>)?\s*,\s*(?P<years>\d{4}\s*-\s*\d{4}|\d{4})\s*$",
        )
        .unwrap()
    });

    fn strip_obfuscated_email_suffix(name: &str) -> String {
        static OBF_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(
                r"(?i)^(?P<prefix>.+?)\s+[a-z0-9._-]{1,64}\s+at\s+[a-z0-9._-]{1,64}(?:\s+dot\s+[a-z]{2,12}|\.[a-z]{2,12})\s*$",
            )
            .unwrap()
        });
        let trimmed = name.trim();
        if let Some(cap) = OBF_RE.captures(trimmed) {
            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if !prefix.is_empty() {
                return prefix.to_string();
            }
        }
        trimmed.to_string()
    }

    for (first, second) in prepared_cache.adjacent_pairs() {
        let Some(c1) = FIRST_RE.captures(first.prepared) else {
            continue;
        };
        let Some(c2) = SECOND_RE.captures(second.prepared) else {
            continue;
        };
        let name1 =
            strip_obfuscated_email_suffix(c1.name("name").map(|m| m.as_str()).unwrap_or("").trim());
        let year1 = c1.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let name2 =
            strip_obfuscated_email_suffix(c2.name("name").map(|m| m.as_str()).unwrap_or("").trim());
        let years2 = c2.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if name1.is_empty() || year1.is_empty() || name2.is_empty() || years2.is_empty() {
            continue;
        }

        let raw = format!("Copyright (c) {name1}, {year1} {name2}, {years2}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };

        if !copyrights.iter().any(|c| {
            c.start_line == first.line_number
                && c.end_line == second.line_number
                && c.copyright == refined
        }) {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }

        let raw_holder = format!("{name1}, {name2}");
        if let Some(h) = refine_holder_in_copyright_context(&raw_holder)
            && !holders.iter().any(|x| {
                x.start_line == first.line_number
                    && x.end_line == second.line_number
                    && x.holder == h
            })
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
    }
}

pub fn split_multiline_holder_lists_from_copyright_email_sequences(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() || holders.is_empty() {
        return;
    }

    static NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<name>[^<>\n]+?)\s*<[^>\s]+@[^>\s]+>").expect("valid name-email regex")
    });
    static LEADING_COPY_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*(?:copyright\s*)?\(c\)\s*(?:\d{2,4}(?:\s*-\s*\d{2,4})?(?:\s*,\s*\d{2,4})*)\s+",
        )
        .expect("valid leading copyright-year regex")
    });

    let mut to_add: Vec<HolderDetection> = Vec::new();
    let mut to_remove: HashSet<(usize, usize, String)> = HashSet::new();

    for c in copyrights {
        if c.end_line.get() <= c.start_line.get() {
            continue;
        }

        let c_trimmed = c.copyright.trim();
        let c_lower = c_trimmed.to_ascii_lowercase();
        if !(c_lower.starts_with("(c)") || c_lower.starts_with("copyright (c)")) {
            continue;
        }
        if c_trimmed.contains(',') {
            continue;
        }

        if !c.copyright.contains('@') || !c.copyright.contains('<') || !c.copyright.contains('>') {
            continue;
        }

        let mut split_names: Vec<String> = NAME_EMAIL_RE
            .captures_iter(&c.copyright)
            .filter_map(|cap| {
                cap.name("name")
                    .map(|m| normalize_whitespace(m.as_str().trim()))
            })
            .map(|name| LEADING_COPY_YEAR_RE.replace(&name, "").trim().to_string())
            .filter_map(|name| refine_holder(&name))
            .collect();

        if split_names.len() < 2 {
            continue;
        }

        split_names.dedup();
        if split_names.len() != 2 {
            continue;
        }
        let joined = normalize_whitespace(&split_names.join(" "));

        let mut has_joined_holder = false;
        for h in holders.iter() {
            if h.start_line.get() == c.start_line.get()
                && h.end_line.get() == c.end_line.get()
                && normalize_whitespace(&h.holder) == joined
            {
                has_joined_holder = true;
                to_remove.insert((h.start_line.get(), h.end_line.get(), h.holder.clone()));
            }
        }

        if !has_joined_holder {
            continue;
        }

        for name in split_names {
            let key = (c.start_line.get(), c.end_line.get(), name.clone());
            if !holders
                .iter()
                .any(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()) == key)
            {
                to_add.push(HolderDetection {
                    holder: name,
                    start_line: c.start_line,
                    end_line: c.end_line,
                });
            }
        }
    }

    if !to_remove.is_empty() {
        holders.retain(|h| {
            !to_remove.contains(&(h.start_line.get(), h.end_line.get(), h.holder.clone()))
        });
    }
    if !to_add.is_empty() {
        holders.extend(to_add);
        dedupe_exact_span_holders(holders);
    }
}

pub fn add_missing_holders_from_preceding_name_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.len() < 2 || copyrights.is_empty() {
        return;
    }

    static YEAR_ONLY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\((?:c|C)\)\s*(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?)$",
        )
        .unwrap()
    });
    static TRAILING_PAREN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<name>.+?)\s*\((?P<tail>[^()]+)\)$").unwrap());

    fn strip_author_label(line: &str) -> Option<String> {
        let trimmed = line.trim();
        for prefix in ["author:", "authors:"] {
            if trimmed
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
            {
                let rest = trimmed[prefix.len()..].trim();
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
        None
    }

    fn is_attribution_parenthetical_tail(tail: &str) -> bool {
        let lower = tail.trim().to_ascii_lowercase();
        if lower.contains('@') || lower.contains("http://") || lower.contains("https://") {
            return true;
        }

        [
            "llc",
            "inc",
            "ltd",
            "gmbh",
            "corp",
            "corporation",
            "foundation",
            "project",
            "team",
            "company",
        ]
        .iter()
        .any(|needle| {
            lower
                .split_whitespace()
                .any(|word| word.trim_matches('.') == *needle)
        })
    }

    let mut seen_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for copyright in copyrights.iter_mut() {
        if copyright.start_line.get() != copyright.end_line.get() || copyright.start_line.get() <= 1
        {
            continue;
        }

        let Some(copy_cap) = YEAR_ONLY_COPY_RE.captures(copyright.copyright.trim()) else {
            continue;
        };
        let years = copy_cap
            .name("years")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim();
        if years.is_empty() {
            continue;
        }

        let Some(previous_line) = prepared_cache
            .get(copyright.start_line.get() - 1)
            .map(|line| line.trim().trim_start_matches('*').trim_start().to_string())
        else {
            continue;
        };
        if previous_line.is_empty() {
            continue;
        }

        let previous_lower = previous_line.to_ascii_lowercase();
        if previous_lower.starts_with("copyright")
            || previous_lower.starts_with("written by")
            || previous_lower.starts_with("developed by")
            || previous_lower.starts_with("created by")
        {
            continue;
        }

        let labeled_candidate = strip_author_label(&previous_line);
        let mut had_attribution_tail = false;
        let candidate_raw = if let Some(labeled) = labeled_candidate.clone() {
            labeled
        } else if let Some(paren_cap) = TRAILING_PAREN_RE.captures(&previous_line) {
            let name = paren_cap
                .name("name")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim();
            let tail = paren_cap
                .name("tail")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim();
            if !name.is_empty() && is_attribution_parenthetical_tail(tail) {
                had_attribution_tail = true;
                name.to_string()
            } else {
                previous_line.clone()
            }
        } else {
            previous_line.clone()
        };

        if candidate_raw.split_whitespace().count() < 2 || candidate_raw.len() > 80 {
            continue;
        }

        if labeled_candidate.is_none() {
            if candidate_raw.contains(',')
                || candidate_raw.to_ascii_lowercase().contains(" and ")
                || candidate_raw.contains(':')
            {
                continue;
            }

            let has_contact_marker = previous_line.contains('@')
                || previous_line.contains("http://")
                || previous_line.contains("https://");
            if !has_contact_marker && !had_attribution_tail {
                continue;
            }
        }

        let starts_name_like = candidate_raw
            .chars()
            .find(|ch| ch.is_alphabetic())
            .is_some_and(|ch| ch.is_uppercase());
        if !starts_name_like {
            continue;
        }

        let Some(holder) = refine_holder_in_copyright_context(&candidate_raw) else {
            continue;
        };
        let updated = format!("{holder}, Copyright (c) {years}");
        copyright.copyright = updated;

        let holder_key = (
            copyright.start_line.get() - 1,
            copyright.start_line.get(),
            holder.clone(),
        );
        if seen_h.insert(holder_key) {
            holders.push(HolderDetection {
                holder,
                start_line: copyright
                    .start_line
                    .prev()
                    .expect("valid preceding line number"),
                end_line: copyright.end_line,
            });
        }
    }
}

pub fn merge_multiline_obfuscated_name_year_copyright_pairs(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static FIRST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s+(?P<name>.+?)\s*<[^>]+>\s*,\s*(?P<year>\d{4})\s*$")
            .unwrap()
    });
    static SECOND_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<name>.+?)\s*<[^>]+>\s*,\s*(?P<years>\d{4}\s*-\s*\d{4}|\d{4})\s*$")
            .unwrap()
    });

    for (first, second) in prepared_cache.adjacent_pairs() {
        if !(first.prepared.contains("Copyright") || first.prepared.contains("copyright")) {
            continue;
        }

        let Some(c1) = FIRST_RE.captures(first.prepared) else {
            continue;
        };
        let Some(c2) = SECOND_RE.captures(second.prepared) else {
            continue;
        };

        let name1 = c1.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let year1 = c1.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let name2 = c2.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let years2 = c2.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if name1.is_empty() || year1.is_empty() || name2.is_empty() || years2.is_empty() {
            continue;
        }

        let raw = format!("Copyright (c) {name1}, {year1} {name2}, {years2}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        let mut updated = false;
        for c in copyrights.iter_mut() {
            if c.start_line == first.line_number
                && c.end_line == first.line_number
                && c.copyright.contains(name1)
            {
                c.copyright = refined.clone();
                c.end_line = second.line_number;
                updated = true;
                break;
            }
        }
        if !updated {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }

        let combined_holder_raw = format!("{name1}, {name2}");
        if let Some(h) = refine_holder_in_copyright_context(&combined_holder_raw) {
            holders.retain(|x| {
                !(x.start_line == first.line_number
                    && x.end_line == first.line_number
                    && (x.holder == name1 || x.holder.contains(name1)))
            });
            if !holders.iter().any(|x| {
                x.start_line == first.line_number
                    && x.end_line == second.line_number
                    && x.holder == h
            }) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: first.line_number,
                    end_line: second.line_number,
                });
            }
        }
    }
}

pub fn extend_copyrights_with_next_line_parenthesized_obfuscated_email(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
) {
    if copyrights.is_empty() {
        return;
    }

    static PAREN_OBF_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^(?:\(\s*)?[a-z0-9][a-z0-9._-]{0,63}\s+(?:at|\[\s*at\s*\])\s+[a-z0-9][a-z0-9._-]{0,63}(?:(?:\s+(?:dot|\[\s*dot\s*\])\s+[a-z]{2,12})+|(?:\.[a-z0-9][a-z0-9-]{0,62})+)(?:\s*\))?$",
        )
        .unwrap()
    });
    static BASE_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?\s+.+$")
            .unwrap()
    });

    for c in copyrights.iter_mut() {
        if c.start_line.get() != c.end_line.get() {
            continue;
        }

        let current = c.copyright.trim();
        if !BASE_COPY_RE.is_match(current) {
            continue;
        }
        if current.contains("@") || current.contains(" AT ") || current.contains(" at ") {
            continue;
        }

        let prepared_next = match prepared_cache.get(c.end_line.get() + 1) {
            Some(p) => p,
            None => continue,
        };
        let next_trim = prepared_next.trim();
        if !PAREN_OBF_EMAIL_RE.is_match(next_trim) {
            continue;
        }

        let merged = format!("{current} {next_trim}");
        let Some(refined) = refine_copyright(&merged) else {
            continue;
        };

        c.copyright = refined;
        c.end_line = c.end_line.next();
    }
}

pub fn extend_copyrights_with_following_all_rights_reserved_line(
    raw_lines: &[&str],
    copyrights: &mut [CopyrightDetection],
) {
    if copyrights.is_empty() || raw_lines.is_empty() {
        return;
    }

    static ALL_RIGHTS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^all\s+rights\s+reserved\.?$").unwrap());
    static BASE_COPY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*(?:copyright\b|\(c\))").unwrap());

    for c in copyrights.iter_mut() {
        if c.start_line.get() != c.end_line.get() {
            continue;
        }

        let trimmed = c.copyright.trim();
        if !BASE_COPY_RE.is_match(trimmed) {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("all rights reserved") {
            continue;
        }

        let Some(next_raw) = raw_lines.get(c.end_line.get()) else {
            continue;
        };
        let next_trim = next_raw.trim().trim_start_matches('*').trim_start();
        if !ALL_RIGHTS_RE.is_match(next_trim) {
            continue;
        }

        let merged = format!("{trimmed} {next_trim}");
        let refined = refine_copyright(&merged).unwrap_or_else(|| normalize_whitespace(&merged));
        let merged_normalized = normalize_whitespace(&merged);
        c.copyright = if refined.to_ascii_lowercase().contains("all rights reserved") {
            refined
        } else {
            merged_normalized
        };
        c.end_line = c.end_line.next();
    }
}

pub fn add_modify_suffix_holders(
    prepared_cache: &PreparedLines<'_>,
    holders: &[HolderDetection],
) -> Vec<HolderDetection> {
    if prepared_cache.is_empty() || holders.is_empty() {
        return Vec::new();
    }

    holders
        .iter()
        .filter_map(|h| {
            let idx = h.end_line.get() + 1;
            let t = prepared_cache.get(idx)?.trim();
            if t.is_empty() {
                return None;
            }
            let lower = t.to_ascii_lowercase();
            if !lower.starts_with("modify ") {
                return None;
            }
            if t.len() > 64 {
                return None;
            }
            if !t
                .split_whitespace()
                .any(|w| w.chars().any(|c| c.is_ascii_uppercase()))
            {
                return None;
            }
            let combined = normalize_whitespace(&format!("{} {t}", h.holder));
            Some(HolderDetection {
                holder: combined,
                start_line: h.start_line,
                end_line: h.end_line.next(),
            })
        })
        .collect()
}

pub fn extend_copyrights_with_authors_blocks(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.len() < 3 {
        return;
    }

    static AUTHORS_HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*authors?\s*:\s*$").unwrap());
    static YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(?:19\d{2}|20\d{2})\b").unwrap());
    static STRIP_ANGLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*<[^>]*>\s*").unwrap());

    for (base, header, author_line) in prepared_cache.adjacent_triples() {
        let base_prepared = base.prepared;
        if base_prepared.is_empty() {
            continue;
        }
        let base_lower = base_prepared.to_ascii_lowercase();
        if !base_lower.contains("copyright") && !base_lower.contains("(c)") {
            continue;
        }
        if !YEAR_RE.is_match(base_prepared) {
            continue;
        }

        if !AUTHORS_HEADER_RE.is_match(header.prepared) {
            continue;
        }

        let mut author = author_line.prepared.to_string();
        if author.is_empty() {
            continue;
        }

        if author.contains('<') && author.contains('>') {
            author = STRIP_ANGLE_RE.replace_all(&author, " ").into_owned();
        }
        if let Some(idx) = author.to_ascii_lowercase().find(" at ") {
            let head = author[..idx].trim();
            if head.split_whitespace().count() >= 2 {
                let mut parts: Vec<&str> = head.split_whitespace().collect();
                if parts.len() >= 3
                    && let Some(last) = parts.last()
                    && last
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && parts[..parts.len() - 1].iter().all(|w| {
                        let w = w.trim_matches(|c: char| c.is_ascii_punctuation());
                        let mut chars = w.chars();
                        chars
                            .next()
                            .is_some_and(|c| c.is_alphabetic() && c.is_uppercase())
                    })
                {
                    parts.pop();
                }
                author = parts.join(" ");
            }
        }
        author = author
            .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
            .to_string();
        if author.split_whitespace().count() < 2 {
            continue;
        }

        let extended_raw = format!("{base_prepared} Authors {author}");
        let Some(extended) = refine_copyright(&extended_raw) else {
            continue;
        };

        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line == base.line_number && c.end_line == base.line_number)
        {
            if c.copyright.starts_with("Copyright") || c.copyright.starts_with("(c)") {
                c.copyright = extended.clone();
                c.end_line = author_line.line_number;
            }
        }

        if let Some(h) = derive_holder_from_simple_copyright_string(&extended)
            && !holders.iter().any(|hh| hh.holder == h)
        {
            holders.push(HolderDetection {
                holder: h.clone(),
                start_line: base.line_number,
                end_line: author_line.line_number,
            });

            holders.retain(|hh| {
                if hh.start_line != base.line_number || hh.end_line != base.line_number {
                    return true;
                }
                if hh.holder == h {
                    return true;
                }
                !(h.starts_with(hh.holder.as_str()) && h.len() > hh.holder.len())
            });
        }
    }
}

pub fn extend_multiline_copyright_c_no_year_names(
    group: &[(usize, String)],
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    static LEADING_COPY_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*(?P<tail>.+)$").expect("valid copyright (c) regex")
    });
    static YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(19\d{2}|20\d{2})").expect("valid year regex"));
    static ACRONYM_PARENS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\([A-Z0-9]{2,}\)").expect("valid acronym parens regex"));

    for i in 0..group.len() {
        let (start_ln, raw_line) = &group[i];
        let line = raw_line.trim().trim_start_matches('*').trim();
        let Some(cap) = LEADING_COPY_C_RE.captures(line) else {
            continue;
        };

        if YEAR_RE.is_match(line) {
            continue;
        }

        let mut tail = cap
            .name("tail")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if tail.is_empty() {
            continue;
        }

        let ends_with_wrapping_connector = {
            let last = tail
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_matches(|c: char| c.is_ascii_punctuation());
            matches!(
                last.to_ascii_lowercase().as_str(),
                "of" | "for" | "and" | "the" | "to" | "in"
            )
        };
        if !ends_with_wrapping_connector {
            continue;
        }

        let mut end_ln = *start_ln;
        let mut did_extend = false;
        for (ln, cont_raw) in group.iter().skip(i + 1) {
            if *ln != end_ln + 1 {
                break;
            }

            let cont = cont_raw.trim().trim_start_matches('*').trim();
            if cont.is_empty() {
                break;
            }

            if cont.contains('@') {
                break;
            }

            let lower = cont.to_ascii_lowercase();
            if lower.contains("copyright")
                || lower.starts_with("all rights")
                || lower.starts_with("reserved")
            {
                break;
            }

            let starts_upper = cont.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            if !starts_upper {
                break;
            }
            let upper_words = cont
                .split_whitespace()
                .filter(|w| w.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
                .count();
            let looks_like_name = upper_words >= 2 || ACRONYM_PARENS_RE.is_match(cont);
            if !looks_like_name {
                break;
            }

            tail.push(' ');
            tail.push_str(cont);
            end_ln = *ln;
            did_extend = true;
        }

        if !did_extend {
            continue;
        }

        let raw = format!("Copyright (c) {tail}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        let Some(refined_holder) = refine_holder_in_copyright_context(&tail) else {
            continue;
        };

        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line.get() == *start_ln && !YEAR_RE.is_match(c.copyright.as_str()))
        {
            if refined.len() > c.copyright.len() && refined.starts_with(&c.copyright) {
                c.copyright = refined.clone();
                c.end_line = LineNumber::new(end_ln).expect("valid");
            }
        }

        for h in holders
            .iter_mut()
            .filter(|h| h.start_line.get() == *start_ln && !YEAR_RE.is_match(h.holder.as_str()))
        {
            if refined_holder.len() > h.holder.len() && refined_holder.starts_with(&h.holder) {
                h.holder = refined_holder.clone();
                h.end_line = LineNumber::new(end_ln).expect("valid");
            }
        }
    }
}

pub fn extend_multiline_copyright_c_year_holder_continuations(
    group: &[(usize, String)],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static LEADING_COPY_C_YEARS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s*(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*?)\s+(?P<tail>.+)$",
        )
        .expect("valid multiline copyright (c) years regex")
    });
    static ACRONYM_PARENS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\([A-Z0-9]{2,}\)").expect("valid acronym parens regex"));

    fn looks_like_holder_continuation(line: &str) -> bool {
        if is_trademark_boilerplate_line(line) {
            return false;
        }
        let starts_upper = line.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        if !starts_upper {
            return false;
        }

        let upper_words = line
            .split_whitespace()
            .filter(|word| word.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            .count();
        upper_words >= 2
            || line.contains('@')
            || line.to_ascii_lowercase().contains(" at ")
            || ACRONYM_PARENS_RE.is_match(line)
    }

    for i in 0..group.len() {
        let (start_ln, raw_line) = &group[i];
        let line = raw_line.trim().trim_start_matches('*').trim();
        let Some(cap) = LEADING_COPY_C_YEARS_RE.captures(line) else {
            continue;
        };

        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        let mut tail = cap
            .name("tail")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if years.is_empty()
            || tail.is_empty()
            || !tail.trim_end().ends_with(',')
            || tail.to_ascii_lowercase().contains("copyright")
        {
            continue;
        }

        let mut end_ln = *start_ln;
        let mut did_extend = false;
        let mut invalid_continuation = false;
        for (ln, cont_raw) in group.iter().skip(i + 1) {
            if *ln != end_ln + 1 {
                break;
            }

            let cont = cont_raw.trim().trim_start_matches('*').trim();
            if cont.is_empty() {
                break;
            }

            let lower = cont.to_ascii_lowercase();
            if lower.contains("copyright")
                || lower.starts_with("all rights")
                || lower.starts_with("reserved")
                || is_trademark_boilerplate_line(cont)
            {
                break;
            }

            if !looks_like_holder_continuation(cont) {
                break;
            }

            if did_extend || cont.trim_end().ends_with(',') {
                invalid_continuation = true;
                break;
            }

            tail.push(' ');
            tail.push_str(cont);
            end_ln = *ln;
            did_extend = true;
        }

        if !did_extend || invalid_continuation {
            continue;
        }

        let raw = format!("Copyright (c) {years} {tail}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };

        let mut updated_copyright = false;
        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line.get() == *start_ln)
        {
            if refined.len() > c.copyright.len() && refined.starts_with(&c.copyright) {
                c.copyright = refined.clone();
                c.end_line = LineNumber::new(end_ln).expect("valid");
                updated_copyright = true;
            }
        }
        if !updated_copyright
            && !copyrights.iter().any(|c| {
                c.start_line.get() == *start_ln
                    && c.end_line.get() == end_ln
                    && c.copyright == refined
            })
        {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: LineNumber::new(*start_ln).expect("invalid line number"),
                end_line: LineNumber::new(end_ln).expect("valid"),
            });
        }

        let Some(refined_holder) = derive_holder_from_simple_copyright_string(&refined) else {
            continue;
        };

        let mut updated_holder = false;
        for h in holders
            .iter_mut()
            .filter(|h| h.start_line.get() == *start_ln)
        {
            if refined_holder.len() > h.holder.len() && refined_holder.starts_with(&h.holder) {
                h.holder = refined_holder.clone();
                h.end_line = LineNumber::new(end_ln).expect("valid");
                updated_holder = true;
            }
        }
        if !updated_holder
            && !holders.iter().any(|h| {
                h.start_line.get() == *start_ln
                    && h.end_line.get() == end_ln
                    && h.holder == refined_holder
            })
        {
            holders.push(HolderDetection {
                holder: refined_holder,
                start_line: LineNumber::new(*start_ln).expect("invalid line number"),
                end_line: LineNumber::new(end_ln).expect("valid"),
            });
        }
    }
}

pub fn extend_authors_see_url_copyrights(
    group: &[(usize, String)],
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    static PREFIX_SEE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\(\s*see\s*$").expect("valid (see prefix regex")
    });
    static URL_LINE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^https?://\S+").expect("valid url line regex"));

    for w in group.windows(2) {
        let (ln1, line1) = &w[0];
        let (ln2, line2) = &w[1];
        if *ln2 != ln1 + 1 {
            continue;
        }

        let l1 = line1.trim().trim_start_matches('*').trim();
        let Some(cap) = PREFIX_SEE_RE.captures(l1) else {
            continue;
        };
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if prefix.is_empty() {
            continue;
        }
        if !prefix.to_ascii_lowercase().contains("authors") {
            continue;
        }

        let l2 = line2.trim().trim_start_matches('*').trim();
        if !URL_LINE_RE.is_match(l2) {
            continue;
        }

        let url = l2
            .trim_end_matches(|c: char| c.is_ascii_whitespace() || matches!(c, '.' | ')'))
            .trim();
        if url.is_empty() {
            continue;
        }

        let raw = format!("{prefix} (see {url})");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        let refined_holder = derive_holder_from_simple_copyright_string(&format!("{prefix} see"));

        for c in copyrights.iter_mut().filter(|c| c.start_line.get() == *ln1) {
            if refined.len() > c.copyright.len() && refined.starts_with(&c.copyright) {
                c.copyright = refined.clone();
                c.end_line = LineNumber::new(*ln2).expect("valid");
            }
        }

        if let Some(refined_holder) = refined_holder {
            for h in holders.iter_mut().filter(|h| h.start_line.get() == *ln1) {
                if refined_holder.len() > h.holder.len() && refined_holder.starts_with(&h.holder) {
                    h.holder = refined_holder.clone();
                    h.end_line = LineNumber::new(*ln2).expect("valid");
                }
            }
        }
    }
}

pub fn extend_leading_dash_suffixes(
    group: &[(usize, String)],
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    static DASH_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<dash>[-–]{1,2})\s+(?P<tail>\S(?:.*\S)?)$").expect("valid dash line regex")
    });

    for w in group.windows(2) {
        let (ln1, line1) = &w[0];
        let (ln2, line2) = &w[1];
        if *ln2 != ln1 + 1 {
            continue;
        }

        let has_copyright_on_line1 = copyrights.iter().any(|c| c.start_line.get() == *ln1);
        if !has_copyright_on_line1 {
            continue;
        }

        let _l1 = line1.trim();
        let l2 = line2.trim().trim_start_matches('*').trim();
        let Some(cap) = DASH_LINE_RE.captures(l2) else {
            continue;
        };
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if tail.is_empty() {
            continue;
        }

        if tail.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        if tail.to_ascii_lowercase().contains("copyright") {
            continue;
        }

        let tail_words: Vec<&str> = tail.split_whitespace().collect();
        if tail_words.len() > 3 {
            continue;
        }
        let looks_titley = tail_words.iter().all(|w| {
            w.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                || w.chars().all(|c| c.is_ascii_uppercase())
        });
        if !looks_titley {
            continue;
        }

        let suffix = format!("- {tail}");

        for c in copyrights.iter_mut().filter(|c| c.start_line.get() == *ln1) {
            if c.copyright.contains(&suffix) {
                continue;
            }
            let extended = format!("{} {}", c.copyright.trim_end(), suffix);
            let refined = refine_copyright(&extended)
                .filter(|r| r.contains(tail))
                .unwrap_or(extended);
            c.copyright = refined;
            c.end_line = LineNumber::new(*ln2).expect("valid");
        }

        let derived: Option<String> = copyrights
            .iter()
            .find(|c| c.start_line.get() == *ln1)
            .and_then(|c| derive_holder_from_simple_copyright_string(&c.copyright));

        for h in holders.iter_mut().filter(|h| h.start_line.get() == *ln1) {
            if let Some(ref d) = derived
                && d.len() >= h.holder.len()
                && d.contains(tail)
            {
                h.holder = d.clone();
                h.end_line = LineNumber::new(*ln2).expect("valid");
                continue;
            }

            if h.holder.contains(&suffix) {
                continue;
            }

            let extended = format!("{} {}", h.holder.trim_end(), suffix);
            if let Some(refined) = refine_holder(&extended)
                && refined.contains(tail)
            {
                h.holder = refined;
                h.end_line = LineNumber::new(*ln2).expect("valid");
            } else {
                h.holder = extended;
                h.end_line = LineNumber::new(*ln2).expect("valid");
            }
        }
    }
}

pub fn extend_dash_obfuscated_email_suffixes(
    raw_lines: &[&str],
    group: &[(usize, String)],
    copyrights: &mut [CopyrightDetection],
    holders: &[HolderDetection],
) {
    static DASH_OBFUSCATED_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)(?:--|-)\s*
            (?P<user>[a-z0-9._%+\-]{1,64})\s*
            (?:\[\s*at\s*\]|at)\s*
            (?P<host>[a-z0-9._\-]{1,64})\s*
            (?:\[\s*dot\s*\]|dot)\s*
            (?P<tld>[a-z]{2,10})",
        )
        .expect("valid dash obfuscated email regex")
    });

    for (ln, _) in group {
        if !copyrights.iter().any(|c| c.start_line.get() == *ln) {
            continue;
        }

        let has_named_holder = holders.iter().any(|h| {
            h.start_line.get() == *ln
                && !h.holder.to_ascii_lowercase().contains(" at ")
                && !h.holder.to_ascii_lowercase().contains(" dot ")
        });
        if !has_named_holder {
            continue;
        }

        let Some(raw) = raw_lines.get(ln.saturating_sub(1)) else {
            continue;
        };
        let Some(cap) = DASH_OBFUSCATED_EMAIL_RE.captures(raw) else {
            continue;
        };
        let user = cap.name("user").map(|m| m.as_str()).unwrap_or("");
        let host = cap.name("host").map(|m| m.as_str()).unwrap_or("");
        let tld = cap.name("tld").map(|m| m.as_str()).unwrap_or("");
        if user.is_empty() || host.is_empty() || tld.is_empty() {
            continue;
        }

        let obfuscated = format!("{user} at {host} dot {tld}");

        for c in copyrights.iter_mut().filter(|c| c.start_line.get() == *ln) {
            if c.copyright.contains(&obfuscated) {
                continue;
            }
            c.copyright = format!("{} - {obfuscated}", c.copyright.trim_end());
        }
    }
}

pub fn extend_trailing_copy_year_suffixes(
    raw_lines: &[&str],
    group: &[(usize, String)],
    copyrights: &mut [CopyrightDetection],
) {
    static COPY_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(\s*c\s*\)\s*(?P<year>\d{4})\b").unwrap());

    for (ln, _) in group {
        let Some(raw) = raw_lines.get(ln.saturating_sub(1)) else {
            continue;
        };
        let Some(cap) = COPY_YEAR_RE.captures(raw) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("");
        if year.is_empty() {
            continue;
        }

        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line.get() == *ln && c.end_line.get() == *ln)
        {
            let lower = c.copyright.to_ascii_lowercase();
            if !lower.starts_with("copyright") {
                continue;
            }
            if lower.contains("(c)") {
                continue;
            }
            if c.copyright.contains(year) {
                continue;
            }
            c.copyright = format!("{} (c) {}", c.copyright.trim_end(), year);
        }
    }
}

pub fn extend_w3c_registered_org_list_suffixes(
    group: &[(usize, String)],
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    static W3C_ORGS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\bW3C\s*\(r\)\s*\((?P<orgs>[^)]+)\)").unwrap());

    for (ln, prepared) in group {
        let line = prepared.trim();
        let Some(cap) = W3C_ORGS_RE.captures(line) else {
            continue;
        };
        let orgs = cap.name("orgs").map(|m| m.as_str()).unwrap_or("").trim();
        if orgs.is_empty() {
            continue;
        }

        let full = format!("W3C(r) ({orgs})");

        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line.get() == *ln || c.start_line.get() + 1 == *ln)
        {
            if c.copyright.contains(&full) {
                continue;
            }
            if c.copyright.contains("W3C(r)") {
                c.copyright = c.copyright.replace("W3C(r)", &full);
                c.end_line =
                    LineNumber::new(c.end_line.get().max(*ln)).expect("invalid line number");
            }
        }

        for h in holders
            .iter_mut()
            .filter(|h| h.start_line.get() == *ln || h.start_line.get() + 1 == *ln)
        {
            if h.holder.contains(&full) {
                continue;
            }
            if h.holder == "W3C(r)" {
                h.holder = full.clone();
                h.end_line =
                    LineNumber::new(h.end_line.get().max(*ln)).expect("invalid line number");
            }
        }
    }
}

pub fn merge_multiline_copyrighted_by_with_trailing_copyright_clause(
    did_expand_href: bool,
    content: &str,
    copyrights: &mut [CopyrightDetection],
) {
    if !did_expand_href {
        return;
    }
    if copyrights.is_empty() {
        return;
    }

    let lower = content.to_ascii_lowercase();
    if !lower.contains("copyrighted by") {
        return;
    }

    static TRAILING_COPY_C_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bcopyright\s*\(c\)\s*(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?)",
        )
        .unwrap()
    });

    let line_number_index = LineNumberIndex::new(content);
    let mut unique_clauses: HashSet<String> = HashSet::new();
    let mut clause_max_line: HashMap<String, usize> = HashMap::new();
    for cap in TRAILING_COPY_C_YEAR_RE.captures_iter(content) {
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() {
            continue;
        }
        let candidate = format!("Copyright (c) {years}");
        if let Some(refined) = refine_copyright(&candidate) {
            let ln = cap
                .get(0)
                .map(|m| line_number_index.line_number_at_offset(m.start()).get())
                .unwrap_or(1);
            unique_clauses.insert(refined.clone());
            clause_max_line
                .entry(refined)
                .and_modify(|e| *e = (*e).max(ln))
                .or_insert(ln);
        }
    }
    if unique_clauses.len() != 1 {
        return;
    }
    let suffix = unique_clauses.into_iter().next().unwrap_or_default();
    if suffix.is_empty() {
        return;
    }
    let suffix_line = clause_max_line.get(&suffix).copied().unwrap_or(1);

    for det in copyrights.iter_mut() {
        let det_lower = det.copyright.to_ascii_lowercase();
        if !det_lower.starts_with("copyrighted by") {
            continue;
        }
        if det_lower.contains("copyright (c)") {
            continue;
        }
        if det.copyright.contains(&suffix) {
            continue;
        }

        let base = det.copyright.trim().trim_end_matches(',').trim();
        let merged = format!("{base}, {suffix}");
        if let Some(refined) = refine_copyright(&merged) {
            det.copyright = refined;
        } else {
            det.copyright = merged;
        }
        det.end_line =
            LineNumber::new(det.end_line.get().max(suffix_line)).expect("invalid line number");
    }
}

pub fn extract_line_ending_copyright_then_by_holder(
    prepared_cache: &PreparedLines<'_>,
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    let mut new_copyrights = Vec::new();
    let mut new_holders = Vec::new();

    for prepared_line in prepared_cache.iter_non_empty() {
        let lower = prepared_line.prepared.to_ascii_lowercase();
        if !lower.ends_with("copyright") {
            continue;
        }
        if lower.contains("copyrighted") {
            continue;
        }
        if !(lower.ends_with("and copyright") || lower == "copyright") {
            continue;
        }

        if let Some(next_line) = prepared_cache.next_non_empty_line(prepared_line.line_number) {
            let next_prepared = next_line.prepared;
            let next_lower = next_prepared.to_ascii_lowercase();
            if next_lower.starts_with("by ") {
                let holder_raw = next_prepared[3..].trim();
                let copyright_raw = format!("copyright {}", next_prepared.trim());
                if let Some(copyright_text) = refine_copyright(&copyright_raw) {
                    new_copyrights.push(CopyrightDetection {
                        copyright: copyright_text,
                        start_line: prepared_line.line_number,
                        end_line: next_line.line_number,
                    });
                }

                if let Some(holder) = refine_holder_in_copyright_context(holder_raw)
                    && !existing_holders.iter().any(|h| h.holder == holder)
                {
                    new_holders.push(HolderDetection {
                        holder,
                        start_line: next_line.line_number,
                        end_line: next_line.line_number,
                    });
                }
            }
        }
    }

    (new_copyrights, new_holders)
}

pub fn split_embedded_copyright_detections(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COMMA_C_YEAR_SPLIT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i),\s*\(c\)\s*(?:19\d{2}|20\d{2})\b").unwrap());

    let mut out: Vec<CopyrightDetection> = Vec::new();
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    let mut split_copyrights: Vec<CopyrightDetection> = Vec::new();

    for det in copyrights.drain(..) {
        let s = det.copyright;
        let lower = s.to_ascii_lowercase();
        if let Some(idx) = lower.find(". copyright") {
            let mut start = 0usize;
            let mut splits: Vec<usize> = Vec::new();

            let mut search_from = idx;
            while let Some(rel) = lower[search_from..].find(". copyright") {
                splits.push(search_from + rel);
                search_from = search_from + rel + ". copyright".len();
            }

            for cut in splits {
                let part = s[start..cut].trim();
                if let Some(refined) = refine_copyright(part)
                    && seen.insert((det.start_line.get(), det.end_line.get(), refined.clone()))
                {
                    let d = CopyrightDetection {
                        copyright: refined,
                        start_line: det.start_line,
                        end_line: det.end_line,
                    };
                    split_copyrights.push(d.clone());
                    out.push(d);
                }
                start = cut + 2;
            }

            let tail = s[start..].trim();
            if let Some(refined) = refine_copyright(tail)
                && seen.insert((det.start_line.get(), det.end_line.get(), refined.clone()))
            {
                let d = CopyrightDetection {
                    copyright: refined,
                    start_line: det.start_line,
                    end_line: det.end_line,
                };
                split_copyrights.push(d.clone());
                out.push(d);
            }
            continue;
        }

        if COMMA_C_YEAR_SPLIT_RE.is_match(&s) && lower.matches("(c)").count() >= 2 {
            let mut start = 0usize;
            let mut cuts: Vec<usize> = Vec::new();
            for m in COMMA_C_YEAR_SPLIT_RE.find_iter(&s) {
                cuts.push(m.start());
            }

            for cut in cuts {
                let part = s[start..cut].trim();
                if let Some(refined) = refine_copyright(part)
                    && seen.insert((det.start_line.get(), det.end_line.get(), refined.clone()))
                {
                    let d = CopyrightDetection {
                        copyright: refined,
                        start_line: det.start_line,
                        end_line: det.end_line,
                    };
                    split_copyrights.push(d.clone());
                    out.push(d);
                }
                start = cut + 1;
            }

            let tail = s[start..].trim().trim_start_matches(',').trim();
            if let Some(refined) = refine_copyright(tail)
                && seen.insert((det.start_line.get(), det.end_line.get(), refined.clone()))
            {
                let d = CopyrightDetection {
                    copyright: refined,
                    start_line: det.start_line,
                    end_line: det.end_line,
                };
                split_copyrights.push(d.clone());
                out.push(d);
            }
            continue;
        }

        if seen.insert((det.start_line.get(), det.end_line.get(), s.clone())) {
            out.push(CopyrightDetection {
                copyright: s,
                start_line: det.start_line,
                end_line: det.end_line,
            });
        }
    }

    *copyrights = out;

    holders.extend(add_missing_holders_derived_from_split_copyrights(
        &split_copyrights,
    ));
}
