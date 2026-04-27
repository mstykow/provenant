// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub fn extract_added_the_copyright_year_for_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static ADDED_COPYRIGHT_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*added\s+the\s+copyright\s+year\s*\(\s*(?P<year>\d{4})\s*\)\s+for\s+(?P<holder>.+?)\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    let mut seen_h: HashSet<(String, usize)> = existing_holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for line in prepared_cache.iter_non_empty() {
        let Some(cap) = ADDED_COPYRIGHT_YEAR_RE.captures(line.prepared) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("");
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("");
        if year.is_empty() || holder_raw.trim().is_empty() {
            continue;
        }
        let holder = refine_holder(holder_raw).unwrap_or_else(|| holder_raw.trim().to_string());

        let cr = format!("Copyright year ({year}) for {holder}");
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: line.line_number,
            end_line: line.line_number,
        });

        if seen_h.insert((holder.clone(), line.line_number.get())) {
            holders.push(HolderDetection {
                holder,
                start_line: line.line_number,
                end_line: line.line_number,
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_copyright_years_by_name_then_paren_email_next_line(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPY_YEARS_BY_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bcopyright\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:\d{4}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*)\s+by\s+(?P<name>[^\(\)<>]+?)\s*$",
        )
        .unwrap()
    });
    static LEADING_PAREN_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\(\s*(?P<email>[^\)\s]+@[^\)\s]+)\s*\)").unwrap());

    let mut seen_copyrights: HashSet<String> = copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();

    for prepared_line in prepared_cache.iter_non_empty() {
        let Some(cap) = COPY_YEARS_BY_NAME_RE.captures(prepared_line.prepared) else {
            continue;
        };

        let years = cap
            .name("years")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut name = cap
            .name("name")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let matched = cap.get(0).map(|m| m.as_str()).unwrap_or("");
        if years.is_empty() || name.is_empty() {
            continue;
        }
        name = name
            .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.'))
            .to_string();
        if name.is_empty() {
            continue;
        }

        let Some(next_line) = prepared_cache.next_non_empty_line(prepared_line.line_number) else {
            continue;
        };

        let Some(email_cap) = LEADING_PAREN_EMAIL_RE.captures(next_line.prepared) else {
            continue;
        };
        let email = email_cap
            .name("email")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim();
        if email.is_empty() {
            continue;
        }

        let full_raw = format!("{} ({email})", matched.trim_end());
        if let Some(full) = refine_copyright(&full_raw)
            && seen_copyrights.insert(full.to_ascii_lowercase())
        {
            copyrights.push(CopyrightDetection {
                copyright: full,
                start_line: prepared_line.line_number,
                end_line: next_line.line_number,
            });
        }

        let year_only_raw = format!("copyright {years}");
        if let Some(year_only) = refine_copyright(&year_only_raw) {
            copyrights.retain(|c| {
                !(c.start_line == prepared_line.line_number
                    && c.end_line == prepared_line.line_number
                    && c.copyright == year_only)
            });
        }

        if let Some(holder) = refine_holder_in_copyright_context(&name) {
            holders.push(HolderDetection {
                holder,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }
}

pub fn extract_three_digit_copyright_year_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static COPYRIGHT_C_3DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\s*\(c\)\s*(?P<year>\d{3})\s+(?P<tail>.+)$").unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for prepared_line in prepared_cache.iter_non_empty() {
        let Some(cap) = COPYRIGHT_C_3DIGIT_RE.captures(prepared_line.prepared) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        if year != "200" {
            continue;
        }
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if tail.is_empty() {
            continue;
        }

        let raw = format!("Copyright (c) {year} {tail}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        if seen_cr.insert((prepared_line.line_number.get(), refined.clone())) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }

        if let Some(h) = refine_holder_in_copyright_context(tail)
            && seen_h.insert((prepared_line.line_number.get(), h.clone()))
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_copyrighted_by_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static COPYRIGHTED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcopyrighted\s+by\s+(?P<who>(?-i:[\p{Lu}][^\.\,\;\)]+))").unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for prepared_line in prepared_cache.iter_non_empty() {
        if prepared_line
            .prepared
            .to_ascii_lowercase()
            .contains("not copyrighted")
        {
            continue;
        }
        for cap in COPYRIGHTED_BY_RE.captures_iter(prepared_line.prepared) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() {
                continue;
            }
            let who_lower = who.to_ascii_lowercase();
            if who_lower.starts_with("the ")
                || who_lower.starts_with("their ")
                || who_lower.contains("following")
            {
                continue;
            }
            let raw = format!("copyrighted by {who}");
            let Some(refined) = refine_copyright(&raw) else {
                continue;
            };
            if seen_cr.insert((prepared_line.line_number.get(), refined.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(who)
                && seen_h.insert((prepared_line.line_number.get(), h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_c_word_year_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static C_WORD_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s+(?P<who>[\p{L}]{2,20})\s+(?P<year>(?:19\d{2}|20\d{2}))\b").unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for prepared_line in prepared_cache.iter_non_empty() {
        if !prepared_line.prepared.to_ascii_lowercase().contains("(c)") {
            continue;
        }
        for cap in C_WORD_YEAR_RE.captures_iter(prepared_line.prepared) {
            let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            if who.is_empty() || year.is_empty() {
                continue;
            }
            if who.eq_ignore_ascii_case("copyright") {
                continue;
            }
            if who.chars().all(|c| c.is_uppercase()) {
                continue;
            }

            let who = who.trim();
            if who.is_empty() {
                continue;
            }

            let raw = format!("(c) {who} {year}");
            let Some(refined) = refine_copyright(&raw) else {
                continue;
            };
            if seen_cr.insert((prepared_line.line_number.get(), refined.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(who)
                && seen_h.insert((prepared_line.line_number.get(), h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_are_c_year_holder_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static ARE_C_YEAR_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bare\s*\(c\)\s*(?P<year>(?:19\d{2}|20\d{2}))\s+(?P<holder>[^,\.;]+)")
            .unwrap()
    });
    static TRAILING_UNDER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+under\b.*$").unwrap());

    let mut seen_cr: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for line in prepared_cache.iter_non_empty() {
        if !line.prepared.to_ascii_lowercase().contains("(c)") {
            continue;
        }
        for cap in ARE_C_YEAR_HOLDER_RE.captures_iter(line.prepared) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let mut holder_raw = cap
                .name("holder")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            holder_raw = TRAILING_UNDER_RE
                .replace(&holder_raw, "")
                .trim()
                .to_string();
            if year.is_empty() || holder_raw.is_empty() {
                continue;
            }
            let raw = format!("(c) {year} {holder_raw}");
            let Some(refined) = refine_copyright(&raw) else {
                continue;
            };
            if seen_cr.insert((line.line_number.get(), refined.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: line.line_number,
                    end_line: line.line_number,
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(&holder_raw)
                && seen_h.insert((line.line_number.get(), h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: line.line_number,
                    end_line: line.line_number,
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_bare_c_by_holder_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static C_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*by\s+(?P<holder>[A-Z][^\n]+)$").unwrap());

    let mut seen_cr: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for line in prepared_cache.iter_non_empty() {
        let Some(cap) = C_BY_RE.captures(line.prepared) else {
            continue;
        };
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if holder_raw.is_empty() {
            continue;
        }
        let raw = format!("(c) by {holder_raw}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        if seen_cr.insert((line.line_number.get(), refined.clone())) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: line.line_number,
                end_line: line.line_number,
            });
        }
        if let Some(h) = refine_holder_in_copyright_context(holder_raw)
            && seen_h.insert((line.line_number.get(), h.clone()))
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: line.line_number,
                end_line: line.line_number,
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_all_rights_reserved_by_holder_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static RESERVED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)copyright\s*\(c\)\s*all\s+rights\s+reserved\s+by\s+(?P<holder>[^\n]+)$")
            .unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for line in prepared_cache.iter_non_empty() {
        let Some(cap) = RESERVED_BY_RE.captures(line.prepared) else {
            continue;
        };
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if holder_raw.is_empty() {
            continue;
        }

        let raw = format!("Copyright (c) by {holder_raw}");
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        if seen_cr.insert((line.line_number.get(), refined.clone())) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: line.line_number,
                end_line: line.line_number,
            });
        }

        if let Some(h) = refine_holder_in_copyright_context(holder_raw)
            && seen_h.insert((line.line_number.get(), h.clone()))
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: line.line_number,
                end_line: line.line_number,
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_holder_is_name_paren_email_lines(
    prepared_cache: &PreparedLines<'_>,
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static HOLDER_IS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bholder\s+is\s+(?P<name>[^()]{2,}?)\s*\(\s*(?P<email>[^)\s]*@[^)\s]+)\s*\)",
        )
        .unwrap()
    });

    let mut seen_c: HashSet<(usize, String)> = existing_copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = existing_holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for line in prepared_cache.iter_non_empty() {
        for cap in HOLDER_IS_RE.captures_iter(line.prepared) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() || email.is_empty() {
                continue;
            }
            let raw = format!("holder is {name} ({email})");
            let Some(cr) = refine_copyright(&raw) else {
                continue;
            };
            if seen_c.insert((line.line_number.get(), cr.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: line.line_number,
                    end_line: line.line_number,
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(name)
                && seen_h.insert((line.line_number.get(), h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: line.line_number,
                    end_line: line.line_number,
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_name_before_rewrited_by_copyrights(
    prepared_cache: &PreparedLines<'_>,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static NAME_EMAIL_YEARS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^(?P<name>[^<>]+?)\s*<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?)\s*$",
        )
        .unwrap()
    });
    static REWRITED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>rewrit(?:ed)?\s+by)\s+(?P<name>[^<>]+?)\s*<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>\s*\(c\)\s+(?P<year>(?:19\d{2}|20\d{2}))\s*$",
        )
        .unwrap()
    });

    if prepared_cache.len() < 2 {
        return (Vec::new(), Vec::new());
    }

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for (first, second) in prepared_cache.adjacent_pairs() {
        if first.prepared.is_empty() || second.prepared.is_empty() {
            continue;
        }

        let Some(cap1) = NAME_EMAIL_YEARS_RE.captures(first.prepared) else {
            continue;
        };
        let Some(cap2) = REWRITED_BY_RE.captures(second.prepared) else {
            continue;
        };

        let name1 = cap1.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let email1 = cap1.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        let years1 = cap1.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        let prefix2 = cap2.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let name2 = cap2.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let email2 = cap2.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        let year2 = cap2.name("year").map(|m| m.as_str()).unwrap_or("").trim();

        if name1.is_empty()
            || email1.is_empty()
            || years1.is_empty()
            || prefix2.is_empty()
            || name2.is_empty()
            || email2.is_empty()
            || year2.is_empty()
        {
            continue;
        }

        let combined_raw =
            format!("{name1} <{email1}> {years1} {prefix2} {name2} <{email2}> (c) {year2}");
        if let Some(refined) = refine_copyright(&combined_raw) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }

        let holder_raw = format!("{name1} {prefix2} {name2}");
        if let Some(holder) = refine_holder(&holder_raw) {
            holders.push(HolderDetection {
                holder,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_developed_at_software_copyrights(
    prepared_cache: &PreparedLines<'_>,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static DEVELOPED_AT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bat\s+(?P<holder>[^\n,]+,\s*inc\.,\s*software)\s+copyright\s*\(c\)\s+(?P<year>(?:19\d{2}|20\d{2}))\b",
        )
        .unwrap()
    });

    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for prepared_line in prepared_cache.iter() {
        let mut candidates: Vec<(LineNumber, String)> = vec![(
            prepared_line.line_number,
            prepared_line.prepared.to_string(),
        )];
        if let Some(next) = prepared_cache.line(prepared_line.line_number.next())
            && !next.prepared.is_empty()
        {
            candidates.push((
                prepared_line.line_number,
                format!(
                    "{} {}",
                    prepared_line.prepared.trim_end(),
                    next.prepared.trim_start()
                ),
            ));
        }

        for (line_number, candidate) in candidates {
            for cap in DEVELOPED_AT_RE.captures_iter(&candidate) {
                let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
                let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
                if holder.is_empty() || year.is_empty() {
                    continue;
                }
                let cr = format!("at {holder} copyright (c) {year}");
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: line_number,
                    end_line: line_number,
                });
                let h = holder.to_string();
                holders.push(HolderDetection {
                    holder: h,
                    start_line: line_number,
                    end_line: line_number,
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_confidential_proprietary_copyrights(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("confidential") {
        return;
    }

    static ABC_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^[^A-Za-z0-9]*copyright\s+(?P<year>(?:19\d{2}|20\d{2}))\s+(?P<tag>[A-Z0-9]{2,})\s*$",
        )
            .unwrap()
    });
    static CONFIDENTIAL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bconfidential\s+proprietary\b").unwrap());
    static MOTOROLA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^[^A-Za-z0-9]*copyright\s+(?P<year>(?:19\d{2}|20\d{2}))\s*\(c\)\s*,\s*(?P<holder>.+?)\s*$",
        )
        .unwrap()
    });
    static HOLDER_C_COPYRIGHT_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^[^A-Za-z0-9]*(?P<holder>.+?)\s+\(c\)\s+copyright\s+(?P<year>(?:19\d{2}|20\d{2}))\b",
        )
        .unwrap()
    });

    if prepared_cache.is_empty() {
        return;
    }

    for prepared_line in prepared_cache.iter_non_empty() {
        let line = prepared_line.prepared;

        if let Some(cap) = HOLDER_C_COPYRIGHT_YEAR_RE.captures(line) {
            let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            if !holder.is_empty() && !year.is_empty() {
                let cr = format!("{holder} (c) Copyright {year}");
                if let Some(refined) = refine_copyright(&cr) {
                    copyrights.push(CopyrightDetection {
                        copyright: refined,
                        start_line: prepared_line.line_number,
                        end_line: prepared_line.line_number,
                    });
                }
                if let Some(h) = refine_holder_in_copyright_context(holder) {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: prepared_line.line_number,
                        end_line: prepared_line.line_number,
                    });
                }

                let bare = format!("(c) Copyright {year}");
                if let Some(refined_bare) = refine_copyright(&bare) {
                    copyrights.retain(|c| c.copyright != refined_bare);
                }
            }
        }

        if let Some(cap) = ABC_LINE_RE.captures(line) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let tag = cap.name("tag").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || tag.is_empty() {
                continue;
            }
            let Some(next_clean) = prepared_cache
                .line(prepared_line.line_number.next())
                .map(|p| {
                    p.prepared
                        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
                        .to_string()
                })
            else {
                continue;
            };
            if !next_clean.is_empty() && CONFIDENTIAL_RE.is_match(&next_clean) {
                let cr_raw = format!("COPYRIGHT {year} {tag} {next_clean}");
                if let Some(cr) = refine_copyright(&cr_raw) {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: prepared_line.line_number,
                        end_line: prepared_line.line_number.next(),
                    });
                }
                let holder_raw = format!("{tag} {next_clean}");
                if let Some(h) = refine_holder_in_copyright_context(&holder_raw) {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: prepared_line.line_number,
                        end_line: prepared_line.line_number.next(),
                    });
                }
            }
        }

        if let Some(cap) = MOTOROLA_RE.captures(line) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let base_holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || base_holder.is_empty() {
                continue;
            }
            let Some(next_clean) = prepared_cache
                .line(prepared_line.line_number.next())
                .map(|p| {
                    p.prepared
                        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
                        .to_string()
                })
            else {
                continue;
            };
            if !next_clean.is_empty() && CONFIDENTIAL_RE.is_match(&next_clean) {
                let cr_raw = format!("Copyright {year} (c), {base_holder} - {next_clean}");
                if let Some(cr) = refine_copyright(&cr_raw) {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: prepared_line.line_number,
                        end_line: prepared_line.line_number.next(),
                    });
                }

                let nodash_raw = format!("Copyright {year} (c), {base_holder} {next_clean}");
                if let Some(nodash) = refine_copyright(&nodash_raw) {
                    copyrights.retain(|c| c.copyright != nodash);
                }

                let holder_raw = format!("{base_holder} - {next_clean}");
                if let Some(h) = refine_holder_in_copyright_context(&holder_raw) {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: prepared_line.line_number,
                        end_line: prepared_line.line_number.next(),
                    });
                }
            }
        }
    }
}
