// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub fn add_missing_holders_for_bare_c_name_year_suffixes(
    copyrights: &[CopyrightDetection],
) -> Vec<HolderDetection> {
    static BARE_C_NAME_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)^\(c\)\s+(?P<name>.+?)\s+(?P<year>(?:19\d{2}|20\d{2}))\s*$").unwrap()
    });

    copyrights
        .iter()
        .filter_map(|c| {
            let trimmed = c.copyright.trim();
            let cap = BARE_C_NAME_YEAR_RE.captures(trimmed)?;
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() || name.split_whitespace().count() != 1 {
                return None;
            }
            if !name
                .chars()
                .all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '’' || ch == '-')
            {
                return None;
            }

            let holder = refine_holder_in_copyright_context(name)?;
            if holder.is_empty() {
                return None;
            }

            Some(HolderDetection {
                holder,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect()
}

pub fn drop_shadowed_year_only_copyright_prefixes_same_start_line(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static YEAR_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>(?:copyright\s*)?\(c\)|copyright)\s*(?P<years>\d{4}(?:\s*[-,]\s*\d{2,4})*(?:\s*,\s*\d{4}(?:\s*[-,]\s*\d{2,4})*)*)$")
            .unwrap()
    });

    let mut by_start: HashMap<usize, Vec<String>> = HashMap::new();
    for c in copyrights.iter() {
        by_start
            .entry(c.start_line.get())
            .or_default()
            .push(normalize_whitespace(&c.copyright));
    }

    copyrights.retain(|c| {
        let short = normalize_whitespace(&c.copyright);
        if !YEAR_ONLY_RE.is_match(short.as_str()) {
            return true;
        }
        let Some(all) = by_start.get(&c.start_line.get()) else {
            return true;
        };
        !all.iter()
            .any(|other| other != &short && other.starts_with(&short) && other.len() > short.len())
    });
}

pub fn drop_year_only_copyrights_shadowed_by_previous_software_copyright_line(
    raw_lines: &[&str],
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if raw_lines.is_empty() || copyrights.is_empty() {
        return;
    }

    static YEAR_ONLY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^copyright\s*\(c\)\s*(?P<year>\d{4})$").unwrap());
    static PREV_SOFTWARE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)software\s+copyright\s*\(c\)\s*(?P<year>\d{4})").unwrap()
    });

    copyrights.retain(|c| {
        if c.start_line <= LineNumber::ONE {
            return true;
        }
        let Some(cap) = YEAR_ONLY_RE.captures(c.copyright.trim()) else {
            return true;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("");
        if year.is_empty() {
            return true;
        }

        let this_raw = raw_lines.get(c.start_line.get() - 1).copied().unwrap_or("");
        if this_raw.to_ascii_lowercase().contains("software copyright") {
            return false;
        }

        let prev_prepared = prepared_cache.get(c.start_line.get() - 1).unwrap_or("");
        if let Some(prev) = PREV_SOFTWARE_RE.captures(prev_prepared) {
            let y2 = prev.name("year").map(|m| m.as_str()).unwrap_or("");
            return y2 != year;
        }
        true
    });
}

pub fn add_embedded_copyright_clause_variants(
    copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if copyrights.is_empty() {
        return Vec::new();
    }
    if copyrights.len() < 50 {
        return Vec::new();
    }

    copyrights
        .iter()
        .filter_map(|c| {
            let lower = c.copyright.to_ascii_lowercase();
            if !lower.starts_with("portions created by the initial developer are ") {
                return None;
            }
            let pos = lower.find(" copyright")?;
            let embedded = c.copyright[pos + 1..].trim();
            if embedded.is_empty() {
                return None;
            }
            let refined = refine_copyright(embedded)?;
            if refined
                .to_ascii_lowercase()
                .contains("the initial developer")
            {
                return None;
            }
            Some(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect()
}

pub fn replace_holders_with_embedded_c_year_markers(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if holders.is_empty() {
        return;
    }

    static EMBEDDED_C_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*(?:19|20)\d{2}\b").unwrap());

    let mut to_add: Vec<HolderDetection> = Vec::new();
    let mut seen: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    holders.retain(|h| {
        if !EMBEDDED_C_YEAR_RE.is_match(h.holder.as_str()) {
            return true;
        }

        for c in copyrights.iter().filter(|c| {
            c.start_line.get() == h.start_line.get() && c.end_line.get() == h.end_line.get()
        }) {
            if let Some(derived) = derive_holder_from_simple_copyright_string(&c.copyright) {
                let key = (h.start_line.get(), h.end_line.get(), derived.clone());
                if seen.insert(key) {
                    to_add.push(HolderDetection {
                        holder: derived,
                        start_line: h.start_line,
                        end_line: h.end_line,
                    });
                }
            }
        }

        false
    });

    holders.extend(to_add);
}

pub fn expand_portions_copyright_variants(copyrights: &mut [CopyrightDetection]) {
    if copyrights.is_empty() {
        return;
    }

    for c in copyrights.iter_mut() {
        let lower = c.copyright.to_ascii_lowercase();
        if lower.starts_with("portions copyright") {
            let trimmed = c.copyright.trim();
            if trimmed.ends_with(')')
                && let Some(open) = trimmed.rfind('(')
            {
                let inner = trimmed[open + 1..trimmed.len() - 1].trim();
                let parts: Vec<&str> = inner
                    .split(',')
                    .map(|p| p.trim())
                    .filter(|p| !p.is_empty())
                    .collect();
                if parts.len() >= 3 && parts.iter().all(|p| p.contains('@')) {
                    let mut kept = parts;
                    kept.pop();
                    let prefix = trimmed[..open].trim_end();
                    let new_tail = kept.join(", ");
                    let rebuilt = normalize_whitespace(&format!("{prefix} {new_tail}"));
                    c.copyright = rebuilt;
                }
            }
        }
    }
}

pub fn expand_year_only_copyrights_with_by_name_prefix(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static BY_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bby\s+(?P<name>(?:[A-Z]\.|[\p{Lu}][\p{L}'\-\.]+)(?:\s+(?:[A-Z]\.|[\p{Lu}][\p{L}'\-\.]+))+),\s*Copyright\s*\(c\)\s*(?P<year>\d{4})\b",
        )
        .unwrap()
    });
    static YEAR_ONLY_COPY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^copyright\s*\(c\)\s*\d{4}$").unwrap());

    let mut seen_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for c in copyrights.iter_mut() {
        if !YEAR_ONLY_COPY_RE.is_match(c.copyright.trim()) {
            continue;
        }
        let Some(line) = prepared_cache.get(c.start_line.get()) else {
            continue;
        };
        let Some(cap) = BY_NAME_RE.captures(line) else {
            continue;
        };
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        if name.is_empty() || year.is_empty() {
            continue;
        }
        let expected_year = c
            .copyright
            .trim()
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if expected_year != year {
            continue;
        }
        c.copyright = format!("{name}, Copyright (c) {year}");

        if let Some(h) = refine_holder_in_copyright_context(name) {
            let key = (c.start_line.get(), c.end_line.get(), h.clone());
            if seen_h.insert(key) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: c.start_line,
                    end_line: c.end_line,
                });
            }
        }
    }
}

pub fn expand_year_only_copyrights_with_read_the_suffix(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static YEAR_ONLY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^copyright\s+\d{4}$").unwrap());

    let mut seen_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    let current = copyrights.clone();
    let mut new_copyrights = Vec::new();
    let mut new_holders = Vec::new();

    for c in current.iter() {
        if c.start_line.get() != c.end_line.get() {
            continue;
        }
        if !YEAR_ONLY_RE.is_match(c.copyright.trim()) {
            continue;
        }
        let Some(next_line) = prepared_cache.get(c.end_line.get() + 1) else {
            continue;
        };
        let next_trim = next_line.trim();
        if !next_trim.to_ascii_lowercase().starts_with("read the") {
            continue;
        }
        let tail = "Read the";
        let raw = format!("{} {tail}", c.copyright.trim());
        let Some(refined) = refine_copyright(&raw) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get() + 1, refined.clone());
        if seen_c.insert(key) {
            new_copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line + 1,
            });
        }
        if let Some(h) = refine_holder_in_copyright_context(tail) {
            let hkey = (c.end_line.get() + 1, c.end_line.get() + 1, h.clone());
            if seen_h.insert(hkey) {
                new_holders.push(HolderDetection {
                    holder: h,
                    start_line: c.end_line + 1,
                    end_line: c.end_line + 1,
                });
            }
        }
    }

    copyrights.extend(new_copyrights);
    holders.extend(new_holders);
}

pub fn extend_year_only_copyrights_with_trailing_text(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static YEAR_ONLY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*(?:\(c\)\s*)?(?P<years>[0-9\s,\-–/]+)\s*$").unwrap()
    });
    static TRAILING_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*(?:\(c\)\s*)?(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?)*)\s+(?P<tail>.+)$",
        )
        .unwrap()
    });

    let mut seen_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for c in copyrights.iter_mut() {
        if !YEAR_ONLY_COPY_RE.is_match(c.copyright.as_str()) {
            continue;
        }

        let Some(prepared) = prepared_cache.get(c.start_line.get()) else {
            continue;
        };
        let line = prepared.trim();
        let Some(cap) = TRAILING_TAIL_RE.captures(line) else {
            continue;
        };
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() || tail.is_empty() {
            continue;
        }

        let raw_full = format!("Copyright {years} {tail}");
        let Some(refined) = refine_copyright(&raw_full) else {
            continue;
        };
        if refined == c.copyright {
            continue;
        }

        c.copyright = refined.clone();

        if let Some(h) = refine_holder_in_copyright_context(tail) {
            let key = (c.start_line.get(), c.end_line.get(), h.clone());
            if seen_h.insert(key) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: c.start_line,
                    end_line: c.end_line,
                });
            }
        }
    }
}

pub fn extract_licensed_material_of_company_bare_c_year_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static LICENSED_MATERIAL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^(?:licensed\s+)?material\s+of\s+(?P<holder>.+?)\s*,\s*(?:all\s+rights\s+reserved\s*,\s*)?\(c\)\s*(?P<year>(?:19\d{2}|20\d{2}))\b",
        )
        .unwrap()
    });

    let mut seen_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for ln in 1..=prepared_cache.raw_line_count() {
        let Some(line) = prepared_cache.get(ln) else {
            continue;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(cap) = LICENSED_MATERIAL_RE.captures(line) else {
            continue;
        };
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        if holder_raw.is_empty() || year.is_empty() {
            continue;
        }

        let raw = format!("{holder_raw}, (c) {year}");
        let Some(cr) = refine_copyright(&raw) else {
            continue;
        };

        let ckey = (ln, ln, cr.clone());
        if seen_c.insert(ckey) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(h) = refine_holder_in_copyright_context(holder_raw) {
            let hkey = (ln, ln, h.clone());
            if seen_h.insert(hkey) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }

        copyrights.retain(|c| {
            !(c.start_line.get() == ln
                && c.end_line.get() == ln
                && c.copyright == format!("(c) {year}"))
        });
    }
}

pub fn merge_year_only_copyrights_with_following_author_colon_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }
    if prepared_cache.raw_line_count() < 2 {
        return;
    }

    static AUTHOR_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^author\s*:\s*(?P<name>[^<]+?)\s*(?:<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>)?\s*$",
        )
        .unwrap()
    });
    static YEAR_ONLY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)^copyright\s*\(c\)\s*(?P<years>[0-9\s,\-–/]+)$").unwrap()
    });

    let mut seen_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for i in 1..prepared_cache.raw_line_count() {
        let ln1 = i;
        let ln2 = i + 1;

        let Some(prev) = copyrights
            .iter()
            .find(|c| c.start_line.get() == ln1 && c.end_line.get() == ln1)
        else {
            continue;
        };
        let Some(cap) = YEAR_ONLY_COPY_RE.captures(prev.copyright.as_str()) else {
            continue;
        };
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() {
            continue;
        }
        if years.chars().any(|c| c.is_alphabetic()) {
            continue;
        }
        if !years.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }

        let Some(next_prepared) = prepared_cache.get(ln2) else {
            continue;
        };
        let next_line = next_prepared.trim();
        let Some(acap) = AUTHOR_LINE_RE.captures(next_line) else {
            continue;
        };
        let name = acap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let email = acap.name("email").map(|m| m.as_str()).unwrap_or("").trim();

        let raw = if email.is_empty() {
            format!("Copyright (c) {years} {name}")
        } else {
            format!("Copyright (c) {years} {name} <{email}>")
        };
        let Some(cr) = refine_copyright(&raw) else {
            continue;
        };

        let ckey = (ln1, ln2, cr.clone());
        if seen_c.insert(ckey) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }
        if let Some(h) = refine_holder_in_copyright_context(name) {
            let hkey = (ln1, ln2, h.clone());
            if seen_h.insert(hkey) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln1).expect("valid"),
                    end_line: LineNumber::new(ln2).expect("valid"),
                });
            }
        }

        copyrights.retain(|c| {
            !(c.start_line.get() == ln1
                && c.end_line.get() == ln1
                && YEAR_ONLY_COPY_RE.is_match(c.copyright.as_str()))
        });
    }
}

pub fn extract_question_mark_year_copyrights(
    prepared_cache: &PreparedLines<'_>,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if prepared_cache.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static QMARK_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\s+(?P<year>\d{3}\?)\s+(?P<tail>.+)$").unwrap()
    });

    prepared_cache
        .iter_non_empty()
        .filter_map(|line| {
            let cap = QMARK_COPY_RE.captures(line.prepared)?;
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || tail.is_empty() {
                return None;
            }

            let raw = format!("Copyright {year} {tail}");
            let cr = refine_copyright(&raw)?;
            let h = refine_holder_in_copyright_context(&format!("{year} {tail}"))?;

            Some((
                CopyrightDetection {
                    copyright: cr,
                    start_line: line.line_number,
                    end_line: line.line_number,
                },
                HolderDetection {
                    holder: h,
                    start_line: line.line_number,
                    end_line: line.line_number,
                },
            ))
        })
        .unzip()
}

pub fn strip_inc_suffix_from_holders_for_today_year_copyrights(
    copyrights: &[CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    if copyrights.is_empty() || holders.is_empty() {
        return;
    }

    let has_today_year = copyrights
        .iter()
        .any(|c| contains_year_placeholder(&c.copyright.to_ascii_lowercase()));
    if !has_today_year {
        return;
    }

    let copyright_texts: Vec<String> = copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();

    for h in holders.iter_mut() {
        let trimmed = h.holder.trim();
        let lower = trimmed.to_ascii_lowercase();
        if !(lower.ends_with(" inc.") || lower.ends_with(" inc")) {
            continue;
        }
        let base = trimmed
            .trim_end_matches('.')
            .trim_end()
            .strip_suffix("Inc")
            .or_else(|| trimmed.strip_suffix("Inc."))
            .map(|s| s.trim_end())
            .unwrap_or(trimmed);
        if base == trimmed || base.is_empty() {
            continue;
        }
        let base_lower = base.to_ascii_lowercase();
        if copyright_texts
            .iter()
            .any(|c| contains_year_placeholder(c) && c.contains(&base_lower))
        {
            h.holder = base.to_string();
        }
    }
}

pub fn contains_year_placeholder(lower: &str) -> bool {
    lower.contains("today.year") || lower.contains("current_year")
}

pub fn extend_bare_c_year_detections_to_line_end_for_multi_c_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() || copyrights.is_empty() {
        return;
    }

    static C_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*(?P<year>(?:19\d{2}|20\d{2}))\b").unwrap());

    for prepared_line in prepared_cache.iter_non_empty() {
        if prepared_line
            .prepared
            .to_ascii_lowercase()
            .matches("(c)")
            .count()
            < 2
        {
            continue;
        }

        for m in C_YEAR_RE.captures_iter(prepared_line.prepared) {
            let year = m.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() {
                continue;
            }
            let short = format!("(c) {year}");
            let Some(start) = m.get(0).map(|mm| mm.start()) else {
                continue;
            };
            let tail = prepared_line.prepared.get(start..).unwrap_or("").trim();
            if tail.len() <= short.len() {
                continue;
            }
            let Some(extended) = refine_copyright(tail) else {
                continue;
            };

            let mut did_replace = false;
            for det in copyrights.iter_mut() {
                if det.start_line == prepared_line.line_number
                    && det.end_line == prepared_line.line_number
                    && det.copyright == short
                {
                    det.copyright = extended.clone();
                    did_replace = true;
                }
            }
            if !did_replace {
                continue;
            }

            if let Some(holder) = derive_holder_from_simple_copyright_string(&extended)
                && !holders
                    .iter()
                    .any(|h| h.start_line == prepared_line.line_number && h.holder == holder)
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }
        }
    }
}

pub fn add_missing_holders_derived_from_split_copyrights(
    copyrights: &[CopyrightDetection],
) -> Vec<HolderDetection> {
    copyrights
        .iter()
        .filter_map(|cr| {
            let holder = derive_holder_from_simple_copyright_string(&cr.copyright)?;
            Some(HolderDetection {
                holder,
                start_line: cr.start_line,
                end_line: cr.end_line,
            })
        })
        .collect()
}

pub fn derive_holder_from_simple_copyright_string(s: &str) -> Option<String> {
    static BARE_HOLDER_C_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<holder>[^\n]+?)\s*,?\s*\(c\)\s*(?:19|20)\d{2}\b")
            .expect("valid bare holder (c) year regex")
    });

    static EMBEDDED_C_MARKER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<head>.+?)\s*,\s*\(c\)\s*(?:19|20)\d{2}\b").unwrap());

    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("copyright") {
        let t = trimmed.trim_start();
        if t.to_ascii_lowercase().starts_with("(c)") {
            let mut tail = t.get(3..).unwrap_or("").trim_start();
            let mut start = 0usize;
            for (i, ch) in tail.char_indices() {
                if ch.is_ascii_digit() || matches!(ch, ' ' | ',' | '-' | '–' | '/' | '+') {
                    start = i + ch.len_utf8();
                    continue;
                }
                break;
            }
            if start > 0 && start < tail.len() {
                tail = tail[start..].trim();
            }
            if tail.is_empty() {
                return None;
            }
            return refine_holder_in_copyright_context(tail);
        }

        let cap = BARE_HOLDER_C_YEAR_RE.captures(trimmed)?;
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if holder_raw.is_empty() {
            return None;
        }
        return refine_holder_in_copyright_context(holder_raw);
    }
    let next = trimmed.chars().nth("copyright".len());
    if !matches!(next, Some(' ') | Some('\t') | Some('(') | None) {
        return None;
    }

    let mut tail = trimmed["copyright".len()..].trim_start();
    if let Some(rest) = tail.strip_prefix("(c)") {
        tail = rest.trim_start();
    } else if let Some(rest) = tail.strip_prefix("(C)") {
        tail = rest.trim_start();
    }

    tail = tail.trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ':' | '.'));

    let mut start = 0usize;
    for (i, ch) in tail.char_indices() {
        if ch.is_ascii_digit() || matches!(ch, ' ' | ',' | '-' | '–' | '/') {
            start = i + ch.len_utf8();
            continue;
        }
        break;
    }
    let mut holder_raw = tail[start..].trim();
    if let Some(rest) = holder_raw.strip_prefix("by ") {
        holder_raw = rest.trim();
    }

    if let Some(cap) = EMBEDDED_C_MARKER_RE.captures(holder_raw)
        && let Some(head) = cap.name("head").map(|m| m.as_str().trim())
        && !head.is_empty()
    {
        holder_raw = head;
    }
    if holder_raw.is_empty() {
        return None;
    }

    refine_holder_in_copyright_context(holder_raw)
}
