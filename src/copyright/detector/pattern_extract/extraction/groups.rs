// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub fn extract_common_year_only_lines(groups: &[Vec<(usize, String)>]) -> Vec<CopyrightDetection> {
    const MIN_YEAR_ONLY_LINES: usize = 3;
    const MAX_YEAR: u32 = 2020;

    static COPYRIGHT_YEAR_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*(?:\(c\)\s*)?(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });
    static BARE_C_YEAR_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\(c\)\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut matches: Vec<(usize, String)> = Vec::new();
    for group in groups {
        for (idx, (ln, line)) in group.iter().enumerate() {
            let line = line.trim();
            if !(COPYRIGHT_YEAR_ONLY_RE.is_match(line) || BARE_C_YEAR_ONLY_RE.is_match(line)) {
                continue;
            }

            if let Some((_next_ln, next_line)) = group
                .iter()
                .skip(idx + 1)
                .find(|(_n, l)| !l.trim().is_empty())
            {
                let next_line = next_line.trim();
                let next_is_candidate = crate::copyright::hints::is_candidate(next_line)
                    || next_line.contains("http")
                    || next_line.contains("s>");
                if !next_is_candidate {
                    continue;
                }
            }

            let first_year = line
                .split(|c: char| !c.is_ascii_digit())
                .find(|p| p.len() == 4)
                .and_then(|p| p.parse::<u32>().ok());
            if let Some(y) = first_year
                && y <= MAX_YEAR
                && let Some(refined) = refine_copyright(line)
            {
                matches.push((*ln, refined));
            }
        }
    }

    if matches.len() < MIN_YEAR_ONLY_LINES {
        return Vec::new();
    }

    for (ln, refined) in matches {
        copyrights.push(CopyrightDetection {
            copyright: refined,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }

    copyrights
}

pub fn extract_embedded_bare_c_year_suffixes(
    groups: &[Vec<(usize, String)>],
    existing_copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    const MAX_YEAR: u32 = 2099;

    static EMBEDDED_BARE_C_YEAR_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\(c\)\s*((?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?)\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut seen: HashSet<String> = existing_copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Some(m) = EMBEDDED_BARE_C_YEAR_SUFFIX_RE.find(trimmed) else {
                continue;
            };

            let years = trimmed[m.start()..m.end()]
                .trim()
                .trim_start_matches(|c: char| c != '(')
                .trim();
            let years = years
                .split(|c: char| !c.is_ascii_digit())
                .find(|p| p.len() == 4)
                .and_then(|p| p.parse::<u32>().ok());
            if let Some(y) = years {
                if y > MAX_YEAR {
                    continue;
                }
            } else {
                continue;
            }

            let prefix = trimmed[..m.start()].trim();
            if prefix.is_empty() {
                continue;
            }
            if !prefix.as_bytes().iter().any(|b| b.is_ascii_digit()) {
                continue;
            }
            if prefix.split_whitespace().count() > 2 {
                continue;
            }
            let prefix_lower = prefix.to_ascii_lowercase();
            if prefix_lower.contains("copyright")
                || prefix_lower.contains("http")
                || prefix.contains('@')
                || prefix.contains('<')
                || prefix.contains('>')
            {
                continue;
            }
            if prefix.len() > 32 {
                continue;
            }

            let cap = EMBEDDED_BARE_C_YEAR_SUFFIX_RE
                .captures(trimmed)
                .and_then(|c| c.get(1).map(|m| m.as_str()))
                .unwrap_or("")
                .trim();
            if cap.is_empty() {
                continue;
            }
            let cr = format!("(c) {cap}");
            let cr_lower = cr.to_ascii_lowercase();
            if seen.insert(cr_lower) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    copyrights
}

pub fn extract_trailing_bare_c_year_range_suffixes(
    groups: &[Vec<(usize, String)>],
) -> Vec<CopyrightDetection> {
    static TRAILING_BARE_C_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s*(?:19\d{2}|20\d{2})\s*[-–]\s*(?:19\d{2}|20\d{2})\s*\.?\s*$")
            .unwrap()
    });

    let mut copyrights = Vec::new();

    for group in groups {
        for (idx, (ln, line)) in group.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            if !lower.contains("license") || lower.contains("copyright") {
                continue;
            }
            let Some(m) = TRAILING_BARE_C_RANGE_RE.find(trimmed) else {
                continue;
            };

            if let Some((_next_ln, next_line)) = group
                .iter()
                .skip(idx + 1)
                .find(|(_n, l)| !l.trim().is_empty())
            {
                let next_line = next_line.trim();
                if next_line.contains("http://") || next_line.contains("https://") {
                    continue;
                }
            }

            let suffix = trimmed[m.start()..m.end()].trim();
            if let Some(cr) = refine_copyright(suffix) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    copyrights
}

pub fn extract_repeated_embedded_bare_c_year_suffixes(
    groups: &[Vec<(usize, String)>],
) -> Vec<CopyrightDetection> {
    const MIN_REPEATS: usize = 2;
    const MAX_YEAR: u32 = 2020;

    static EMBEDDED_BARE_C_YEAR_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\(c\)\s*((?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?)\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut license_counts: HashMap<String, (usize, usize)> = HashMap::new();
    let mut copyright_line_sets: HashMap<String, (HashSet<String>, usize)> = HashMap::new();
    for group in groups {
        for (ln, line) in group {
            let line = line.trim();
            let Some(cap) = EMBEDDED_BARE_C_YEAR_SUFFIX_RE.captures(line) else {
                continue;
            };

            let years = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            let first_year = years
                .split(|c: char| !c.is_ascii_digit())
                .find(|p| p.len() == 4)
                .and_then(|p| p.parse::<u32>().ok());
            if let Some(y) = first_year
                && y <= MAX_YEAR
            {
                let lower = line.to_lowercase();
                let is_copyright_line = lower.starts_with("copyright");
                let is_license_line = lower.contains("license");
                if !is_copyright_line && !is_license_line {
                    continue;
                }

                let bare = format!("(c) {years}");
                if is_license_line {
                    let entry = license_counts.entry(bare.clone()).or_insert((0, *ln));
                    entry.0 += 1;
                }
                if is_copyright_line {
                    let entry = copyright_line_sets
                        .entry(bare)
                        .or_insert((HashSet::new(), *ln));
                    entry.0.insert(line.to_string());
                }
            }
        }
    }

    for (bare, (count, first_ln)) in license_counts {
        if count < MIN_REPEATS {
            continue;
        }
        if let Some(refined) = refine_copyright(&bare) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(first_ln).expect("valid"),
                end_line: LineNumber::new(first_ln).expect("valid"),
            });
        }
    }

    for (bare, (lines, first_ln)) in copyright_line_sets {
        if lines.len() < MIN_REPEATS {
            continue;
        }
        if let Some(refined) = refine_copyright(&bare) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(first_ln).expect("valid"),
                end_line: LineNumber::new(first_ln).expect("valid"),
            });
        }
    }

    copyrights
}

pub fn extract_lowercase_username_angle_email_copyrights(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static USER_EMAIL_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^[Cc]opyright\s*(?:\([Cc]\)\s*)?(19\d{2}|20\d{2})\s+([a-z0-9][a-z0-9_\-]{2,63})\s*<\s*([^>\s]+@[^>\s]+)\s*>\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let line = line.trim();
            let Some(cap) = USER_EMAIL_COPYRIGHT_RE.captures(line) else {
                continue;
            };

            let year = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            let user = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim();
            let email = cap.get(3).map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || user.is_empty() || email.is_empty() {
                continue;
            }

            let cr_raw = format!("Copyright (c) {year} {user} <{email}>");
            if let Some(cr) = refine_copyright(&cr_raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            holders.push(HolderDetection {
                holder: user.to_string(),
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_lowercase_username_paren_email_copyrights(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static USER_EMAIL_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\b[Cc]opyright\s*(?:\([Cc]\)\s*)?(19\d{2}|20\d{2})\s+([a-z0-9][a-z0-9_\-]{2,63})\s*\(\s*([^\)\s]+@[^\)\s]+)\s*\)",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            for cap in USER_EMAIL_PARENS_RE.captures_iter(line) {
                let year = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
                let user = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim();
                let email = cap.get(3).map(|m| m.as_str()).unwrap_or("").trim();
                if year.is_empty() || user.is_empty() || email.is_empty() {
                    continue;
                }

                let cr_raw = format!("copyright {year} {user} ({email})");
                if let Some(cr) = refine_copyright(&cr_raw) {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: LineNumber::new(*ln).expect("invalid line number"),
                        end_line: LineNumber::new(*ln).expect("invalid line number"),
                    });
                }

                holders.push(HolderDetection {
                    holder: user.to_string(),
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_c_year_range_by_name_comma_email_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static C_BY_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\(c\)\s+(?P<years>\d{4}(?:\s*[-–]\s*(?:\d{4}|\d{2}))?)\s+by\s+(?P<name>[^,]+),\s*(?P<email>[^\s,]+@[^\s,]+)\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            let Some(cap) = C_BY_NAME_EMAIL_RE.captures(trimmed) else {
                continue;
            };

            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
            if years.is_empty() || name.is_empty() || email.is_empty() {
                continue;
            }

            let cr_raw = format!("(c) {years} by {name}, {email}");
            if let Some(cr) = refine_copyright(&cr_raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder(name) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_copyright_years_by_name_paren_email_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPY_YEARS_BY_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bcopyright\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:\d{4}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*)\s+by\s+(?P<name>[^\(\)<>]+?)\s*\(\s*(?P<email>[^\)\s]+@[^\)\s]+)\s*\)",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> = copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();

    for group in groups {
        for (ln, line) in group {
            for cap in COPY_YEARS_BY_NAME_EMAIL_RE.captures_iter(line) {
                let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
                let mut name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
                let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
                if years.is_empty() || name.is_empty() || email.is_empty() {
                    continue;
                }

                name = name.trim_end_matches(|c: char| {
                    c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.')
                });
                if name.is_empty() {
                    continue;
                }

                let matched = cap.get(0).map(|m| m.as_str()).unwrap_or("");
                let Some(full) = refine_copyright(matched) else {
                    continue;
                };

                if !seen_copyrights.insert(full.to_ascii_lowercase()) {
                    continue;
                }

                copyrights.push(CopyrightDetection {
                    copyright: full.clone(),
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });

                let year_only_raw = format!("copyright {years}");
                if let Some(year_only) = refine_copyright(&year_only_raw) {
                    copyrights.retain(|c| {
                        !(c.start_line.get() == *ln
                            && c.end_line.get() == *ln
                            && c.copyright == year_only
                            && c.copyright != full)
                    });
                }

                if let Some(holder) = refine_holder_in_copyright_context(name) {
                    holders.push(HolderDetection {
                        holder,
                        start_line: LineNumber::new(*ln).expect("invalid line number"),
                        end_line: LineNumber::new(*ln).expect("invalid line number"),
                    });
                }
            }
        }
    }
}

pub fn extract_copyright_year_name_with_of_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static COPY_YEAR_OF_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^Copyright\s*\(c\)\s+(?P<year>19\d{2}|20\d{2})\s+(?P<holder>[A-Z][A-Za-z0-9.'\-]*(?:\s+of\s+[A-Z][A-Za-z0-9.'\-]*)+)\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            let Some(cap) = COPY_YEAR_OF_NAME_RE.captures(trimmed) else {
                continue;
            };

            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || holder_raw.is_empty() {
                continue;
            }

            let cr_raw = format!("Copyright (c) {year} {holder_raw}");
            if let Some(cr) = refine_copyright(&cr_raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder(holder_raw) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_standalone_c_holder_year_lines(
    groups: &[Vec<(usize, String)>],
    existing_copyrights: &[CopyrightDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static STANDALONE_C_HOLDER_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^\(c\)\s+(?P<holder>[A-Z0-9][A-Za-z0-9 ,&'\-\.]*?)\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*)\s*\.?\s*(?:[Aa]ll\s+[Rr]ights\s+[Rr]eserved)?\s*$",
        )
        .unwrap()
    });
    static STANDALONE_C_HOLDER_YEAR_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^\(c\)\s+(?P<holder>[A-Z0-9][A-Za-z0-9 ,&'\-\.]*)\s*,\s*(?P<years>(?:19\d{2}|20\d{2})(?:\s*,\s*(?:19\d{2}|20\d{2})){1,})\s*$",
        )
        .unwrap()
    });
    static EMAIL_ONLY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[^\s@]+@[^\s@]+$").unwrap());

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for idx in 0..group.len() {
            let (ln, line) = &group[idx];
            let trimmed = line.trim();
            let (holder_raw, yearish, has_separator_comma) =
                if let Some(cap) = STANDALONE_C_HOLDER_YEAR_RE.captures(trimmed) {
                    (
                        cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim(),
                        cap.name("years")
                            .map(|m| m.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        false,
                    )
                } else if let Some(cap) = STANDALONE_C_HOLDER_YEAR_LIST_RE.captures(trimmed) {
                    (
                        cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim(),
                        cap.name("years")
                            .map(|m| m.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        true,
                    )
                } else {
                    continue;
                };
            if holder_raw.is_empty() || yearish.is_empty() {
                continue;
            }

            let already_covered = existing_copyrights.iter().any(|c| {
                c.start_line.get() <= *ln
                    && c.end_line.get() >= *ln
                    && c.copyright.contains(&yearish)
            });
            if already_covered {
                continue;
            }

            let mut email_suffix: Option<String> = None;
            if let Some((_, next_line)) = group
                .iter()
                .skip(idx + 1)
                .find(|(_, l)| !l.trim().is_empty())
            {
                let next_trim = next_line.trim();
                if EMAIL_ONLY_RE.is_match(next_trim) {
                    email_suffix = Some(next_trim.to_string());
                }
            }

            let use_copyright_prefix = trimmed.to_ascii_lowercase().contains("all rights reserved");
            let cr_raw = if let Some(email) = &email_suffix {
                if use_copyright_prefix {
                    format!("Copyright (c) {holder_raw} {yearish} - {email}")
                } else {
                    format!("(c) {holder_raw} {yearish} - {email}")
                }
            } else if has_separator_comma {
                if use_copyright_prefix {
                    format!("Copyright (c) {holder_raw}, {yearish}")
                } else {
                    format!("(c) {holder_raw}, {yearish}")
                }
            } else if use_copyright_prefix {
                format!("Copyright (c) {holder_raw} {yearish}")
            } else {
                format!("(c) {holder_raw} {yearish}")
            };
            if let Some(cr) = refine_copyright(&cr_raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: if email_suffix.is_some() {
                        group
                            .iter()
                            .skip(idx + 1)
                            .find(|(_, l)| !l.trim().is_empty())
                            .map(|(n, _)| LineNumber::new(*n).expect("invalid line number"))
                            .unwrap_or(LineNumber::new(*ln).expect("invalid line number"))
                    } else {
                        LineNumber::new(*ln).expect("invalid line number")
                    },
                });
            }

            if let Some(h) = refine_holder(holder_raw) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_c_holder_without_year_lines(
    content: &str,
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static STRING_NAME_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<\s*string\b[^>]*\bname\s*=").unwrap());
    static C_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*\d{4}").unwrap());
    static YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(?:19\d{2}|20\d{2})\b").unwrap());
    static PREFIX_C_HOLDER_DOT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*(?P<prefix>[a-z0-9][a-z0-9_-]{0,48})\s+\(c\)\s+(?P<holder>[^.]{3,}?)\.\s*$",
        )
        .unwrap()
    });

    if !STRING_NAME_RE.is_match(content) || !C_YEAR_RE.is_match(content) {
        return (Vec::new(), Vec::new());
    }

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if YEAR_RE.is_match(trimmed) {
                continue;
            }
            let Some(cap) = PREFIX_C_HOLDER_DOT_RE.captures(trimmed) else {
                continue;
            };
            let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if holder_raw.split_whitespace().count() < 2 {
                continue;
            }
            if !holder_raw
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
            {
                continue;
            }

            let cr_raw = format!("(c) {holder_raw}");
            if let Some(cr) = refine_copyright(&cr_raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(holder) = refine_holder_in_copyright_context(holder_raw) {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_versioned_project_c_holder_banner_lines(
    groups: &[Vec<(usize, String)>],
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
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

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(holder_raw) = versioned_banner_holder_from_prepared(trimmed) else {
                continue;
            };

            let first_token = holder_raw.split_whitespace().next().unwrap_or("");
            let starts_upper = holder_raw
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase());
            let starts_mixed_case_brand = holder_raw
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
                && first_token.chars().skip(1).any(|c| c.is_ascii_uppercase());
            if !(starts_upper || starts_mixed_case_brand) {
                continue;
            }

            let raw = format!("(c) {holder_raw}");
            let Some(cr) = refine_copyright(&raw) else {
                continue;
            };
            if seen_c.insert((*ln, cr.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(&holder_raw)
                && seen_h.insert((*ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_c_years_then_holder_lines(
    groups: &[Vec<(usize, String)>],
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
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

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_ascii_lowercase();
            if !lower.starts_with("(c)") {
                continue;
            }
            if !trimmed.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }

            let tail = trimmed["(c)".len()..].trim_start();
            if tail.is_empty() {
                continue;
            }
            let mut start = 0usize;
            for (i, ch) in tail.char_indices() {
                if ch.is_ascii_digit() || matches!(ch, ' ' | ',' | '-' | '–' | '/' | '+') {
                    start = i + ch.len_utf8();
                    continue;
                }
                break;
            }
            if start == 0 || start >= tail.len() {
                continue;
            }

            let years = tail[..start].trim();
            let holder_raw = tail[start..].trim();
            if years.is_empty() || holder_raw.is_empty() {
                continue;
            }

            let raw = format!("(c) {years} {holder_raw}");
            let Some(cr) = refine_copyright(&raw) else {
                continue;
            };
            if seen_cr.insert((*ln, cr.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: cr.clone(),
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = postprocess_transforms::derive_holder_from_simple_copyright_string(&cr)
                && seen_h.insert((*ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_copyright_c_years_holder_lines(
    groups: &[Vec<(usize, String)>],
    existing_copyrights: &[CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static COPY_C_YEARS_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s*(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*?)\s+(?P<holder>.+?)\s*$",
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

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(cap) = COPY_C_YEARS_HOLDER_RE.captures(trimmed) else {
                continue;
            };
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if years.is_empty() || holder_raw.is_empty() {
                continue;
            }
            if holder_raw.to_ascii_lowercase().contains("all rights") {
                continue;
            }
            let raw = format!("Copyright (c) {years} {holder_raw}");
            let Some(cr) = refine_copyright(&raw) else {
                continue;
            };
            if seen_c.insert((*ln, cr.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: cr.clone(),
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(holder_raw)
                && seen_h.insert((*ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_copr_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static ANY_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(?:19\d{2}|20\d{2})\b").unwrap());
    static COPR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bcopr\.").unwrap());
    static LEADING_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?\s+").unwrap()
    });
    static TRAILING_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s+(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?\s*$").unwrap()
    });

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !COPR_RE.is_match(trimmed) {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            let is_copyright_or_copr =
                lower.starts_with("copyright") && lower.contains(" or copr.");
            let no_year = !ANY_YEAR_RE.is_match(trimmed);

            let (cr_raw, holder_candidate) = if is_copyright_or_copr && no_year {
                let holder_tail = lower
                    .rfind("copr.")
                    .and_then(|idx| trimmed.get(idx + "copr.".len()..))
                    .unwrap_or("")
                    .trim_matches(&[' ', ':'][..])
                    .to_string();

                let is_acronym_only = !holder_tail.contains(' ')
                    && holder_tail.len() >= 2
                    && holder_tail.chars().all(|c| c.is_ascii_uppercase());

                let cr_raw = if is_acronym_only {
                    normalize_whitespace(trimmed)
                } else if holder_tail.is_empty() {
                    "Copr.".to_string()
                } else {
                    format!("Copr. {holder_tail}")
                };

                (cr_raw, holder_tail)
            } else {
                let copr_idx = match lower.find("copr.") {
                    Some(i) => i,
                    None => continue,
                };

                let starts_with_c_marker = trimmed.starts_with("(c)");

                let rest = trimmed
                    .get(copr_idx + "copr.".len()..)
                    .unwrap_or("")
                    .trim_start();

                if rest.matches(" - ").count() < 2 {
                    continue;
                }

                let cr_raw = if rest.is_empty() {
                    if starts_with_c_marker {
                        "(c) Copr.".to_string()
                    } else {
                        "Copr.".to_string()
                    }
                } else if starts_with_c_marker {
                    format!("(c) Copr. {rest}")
                } else {
                    format!("Copr. {rest}")
                };

                (normalize_whitespace(&cr_raw), rest.to_string())
            };

            let Some(cr) = refine_copyright(&cr_raw) else {
                continue;
            };

            copyrights.retain(|c| {
                if !c.copyright.to_ascii_lowercase().starts_with("copr.") {
                    return true;
                }
                if c.copyright == cr {
                    return true;
                }
                !(cr.starts_with(&c.copyright) && cr.len() > c.copyright.len())
            });

            copyrights.push(CopyrightDetection {
                copyright: cr.clone(),
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });

            let mut holder_raw = holder_candidate;
            holder_raw = LEADING_YEAR_RE.replace(&holder_raw, "").to_string();
            holder_raw = TRAILING_YEAR_RE.replace(&holder_raw, "").to_string();
            holder_raw = holder_raw.trim().to_string();

            if holder_raw.is_empty() {
                continue;
            }

            let Some(h) = refine_holder_in_copyright_context(&holder_raw) else {
                continue;
            };

            if h.contains(" - ") {
                holders.retain(|existing| {
                    if existing.holder == h {
                        return true;
                    }
                    !(h.starts_with(&existing.holder) && h.len() > existing.holder.len())
                });
            }

            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });
        }
    }
}

pub fn fallback_year_only_copyrights(groups: &[Vec<(usize, String)>]) -> Vec<CopyrightDetection> {
    static COPYRIGHT_YEAR_OR_RANGE_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*(?:\(c\)\s*)?(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });
    static BARE_C_YEAR_OR_RANGE_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\(c\)\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });
    static MPL_PORTIONS_CREATED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^portions\s+created\s+by\s+the\s+initial\s+developer\s+are\s+copyright\s*\(c\)\s*(?P<year>\d{4})\s*$",
        )
        .unwrap()
    });
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let line = line.trim();

            if let Some(cap) = MPL_PORTIONS_CREATED_RE.captures(line) {
                let year = cap.name("year").map(|m| m.as_str()).unwrap_or("");
                if !year.is_empty() {
                    let s = format!("Copyright (c) {year}");
                    if let Some(refined) = refine_copyright(&s)
                        && seen.insert(refined.clone())
                    {
                        out.push(CopyrightDetection {
                            copyright: refined,
                            start_line: LineNumber::new(*ln).expect("invalid line number"),
                            end_line: LineNumber::new(*ln).expect("invalid line number"),
                        });
                    }
                }
                continue;
            }

            if COPYRIGHT_YEAR_OR_RANGE_ONLY_RE.is_match(line)
                && let Some(refined) = refine_copyright(line)
                && seen.insert(refined.clone())
            {
                out.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if BARE_C_YEAR_OR_RANGE_ONLY_RE.is_match(line)
                && let Some(refined) = refine_copyright(line)
                && seen.insert(refined.clone())
            {
                out.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    out
}

pub fn extract_copyright_c_year_comma_name_angle_email_lines(
    groups: &[Vec<(usize, String)>],
    existing_copyrights: &[CopyrightDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    let has_copyright_label_lines = groups.iter().flatten().any(|(_, l)| {
        l.trim_start()
            .to_ascii_lowercase()
            .starts_with("copyright:")
    });
    if !has_copyright_label_lines {
        return (Vec::new(), Vec::new());
    }

    static COPY_C_YEAR_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:\d{4}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*)\s*,\s*(?P<name>[^<>]+?)\s*<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();
    let mut seen_copyrights: HashSet<String> = existing_copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            let Some(cap) = COPY_C_YEAR_NAME_EMAIL_RE.captures(trimmed) else {
                continue;
            };
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
            if years.is_empty() || name.is_empty() || email.is_empty() {
                continue;
            }

            let raw = format!("Copyright (c) {years}, {name} <{email}>");
            let Some(cr) = refine_copyright(&raw) else {
                continue;
            };

            if !seen_copyrights.insert(cr.to_ascii_lowercase()) {
                continue;
            }

            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });

            if let Some(holder) = refine_holder_in_copyright_context(name) {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extend_software_in_the_public_interest_holder(
    group: &[(usize, String)],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static SPI_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bsoftware\s+in\s+the\s+public\s+interest,\s*inc\.?\b").unwrap()
    });
    static YEARS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:\d{4}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*)",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> = copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();

    for (ln, prepared) in group {
        let line = prepared.trim();
        if !SPI_RE.is_match(line) {
            continue;
        }
        let Some(year_cap) = YEARS_RE.captures(line) else {
            continue;
        };
        let years = year_cap
            .name("years")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim();
        if years.is_empty() {
            continue;
        }

        let holder_raw = "Software in the Public Interest, Inc.";
        let cr_raw = format!("Copyright (c) {years} {holder_raw}");
        let Some(cr) = refine_copyright(&cr_raw) else {
            continue;
        };
        if seen_copyrights.insert(cr.to_ascii_lowercase()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });
        }

        let truncated = format!("Copyright (c) {years} Software");
        copyrights.retain(|c| c.copyright != truncated);
        holders.retain(|h| h.holder != "Software");

        if !holders.iter().any(|h| h.holder == holder_raw) {
            holders.push(HolderDetection {
                holder: holder_raw.to_string(),
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });
        }
    }
}

pub fn extract_copyright_year_c_holder_mid_sentence_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static COPY_YEAR_C_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*copyright\s+(?P<year>19\d{2}|20\d{2})\s+\(c\)\s+(?P<holder>.+?)\s+is\s+licensed\b",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !trimmed.to_ascii_lowercase().contains("licensed") {
                continue;
            }
            let Some(cap) = COPY_YEAR_C_HOLDER_RE.captures(trimmed) else {
                continue;
            };
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || holder.is_empty() {
                continue;
            }

            let holder = holder.trim_end_matches(|c: char| {
                c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':')
            });
            if holder.is_empty() {
                continue;
            }

            let raw = format!("Copyright {year} (c) {holder}");
            if let Some(cr) = refine_copyright(&raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(holder) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_javadoc_author_copyright_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static JAVADOC_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*@author\s+(?P<name>.+?)\s*,?\s*\(\s*c\s*\)\s*(?P<year>(?:19|20)\d{2})\b",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(cap) = JAVADOC_AUTHOR_RE.captures(trimmed) else {
                continue;
            };
            let name_raw = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            if name_raw.is_empty() || year.is_empty() {
                continue;
            }

            let name = name_raw
                .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
                .to_string();
            if name.is_empty() {
                continue;
            }

            let cr_raw = format!("{name}, (c) {year}");
            if let Some(cr) = refine_copyright(&cr_raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(&name) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_copyright_its_authors_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static ITS_AUTHORS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcopyright\s+its\s+authors\b(?P<tail>.*)$").unwrap());

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !ITS_AUTHORS_RE.is_match(trimmed) {
                continue;
            }

            if let Some(cap) = ITS_AUTHORS_RE.captures(trimmed) {
                let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("");
                if tail.trim_start().to_ascii_lowercase().starts_with("and") {
                    continue;
                }
            }

            let cr = "copyright its authors".to_string();
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });

            let holder = "its authors".to_string();
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(*ln).expect("invalid line number"),
                end_line: LineNumber::new(*ln).expect("invalid line number"),
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_us_government_year_placeholder_copyrights(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\b.*\bYEAR\b.*\bUnited\s+States\s+Government\b").unwrap()
    });
    static HAS_DIGIT_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(?:19\d{2}|20\d{2})\b").unwrap());

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if HAS_DIGIT_YEAR_RE.is_match(trimmed) {
                continue;
            }
            if !LINE_RE.is_match(trimmed) {
                continue;
            }

            let raw = "Copyright YEAR United States Government";
            if let Some(cr) = refine_copyright(raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            let holder_raw = "United States Government";
            if let Some(holder) = refine_holder_in_copyright_context(holder_raw) {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_copyright_notice_paren_year_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bcopyright\s+notice\s*\(\s*(?P<year>\d{4})\s*\)\s+(?P<holder>[^\n]+?)\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(cap) = RE.captures(trimmed) else {
                continue;
            };
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || holder_raw.is_empty() {
                continue;
            }

            let raw = format!("Copyright Notice ({year}) {holder_raw}");
            if let Some(cr) = refine_copyright(&raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(holder) = refine_holder_in_copyright_context(holder_raw) {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_initials_holders_from_copyrights(
    copyrights: &[CopyrightDetection],
) -> Vec<HolderDetection> {
    static INITIALS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s+\(c\)\s+(?:19\d{2}|20\d{2})\s*,?\s+(?P<holder>[A-Z](?:\s+[A-Z]){1,2})$",
        )
        .unwrap()
    });

    let mut holders = Vec::new();

    for det in copyrights {
        let Some(cap) = INITIALS_RE.captures(&det.copyright) else {
            continue;
        };
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if holder_raw.is_empty() {
            continue;
        }
        if let Some(holder) = refine_holder_in_copyright_context(holder_raw) {
            holders.push(HolderDetection {
                holder,
                start_line: det.start_line,
                end_line: det.end_line,
            });
        }
    }

    holders
}

pub fn extract_angle_bracket_year_name_copyrights(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut [CopyrightDetection],
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static COPY_C_ANGLE_YEAR_ANGLE_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s*<\s*(?P<years>[^>]+?)\s*>\s*<\s*(?P<name>[A-Z][A-Za-z'\.-]+(?:\s+[A-Z][A-Za-z'\.-]+){1,2})\s*>\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> = copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();
    let mut seen_holders: HashSet<String> = existing_holders
        .iter()
        .map(|h| h.holder.to_ascii_lowercase())
        .collect();

    let mut new_copyrights = Vec::new();
    let mut new_holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            let Some(cap) = COPY_C_ANGLE_YEAR_ANGLE_NAME_RE.captures(trimmed) else {
                continue;
            };
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            if years.is_empty() || name.is_empty() {
                continue;
            }

            let full = format!("Copyright (c) <{years}> <{name}>");
            let full_lower = full.to_ascii_lowercase();
            if !seen_copyrights.contains(&full_lower) {
                let short = format!("Copyright (c) <{years}>");
                if let Some(existing) = copyrights.iter_mut().find(|c| {
                    c.start_line.get() == *ln && c.end_line.get() == *ln && c.copyright == short
                }) {
                    existing.copyright = full.clone();
                } else {
                    new_copyrights.push(CopyrightDetection {
                        copyright: full.clone(),
                        start_line: LineNumber::new(*ln).expect("invalid line number"),
                        end_line: LineNumber::new(*ln).expect("invalid line number"),
                    });
                }
                seen_copyrights.insert(full_lower);
            }

            let holder =
                refine_holder_in_copyright_context(name).unwrap_or_else(|| name.to_string());
            let holder_lower = holder.to_ascii_lowercase();
            if seen_holders.insert(holder_lower) {
                new_holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (new_copyrights, new_holders)
}

pub fn extract_copyright_year_c_name_angle_email_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static COPY_YEAR_C_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s+(?P<year>19\d{2}|20\d{2})\s+\(c\)\s+(?P<name>.+?)\s*<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>\s*$",
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        for (ln, line) in group {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(cap) = COPY_YEAR_C_NAME_EMAIL_RE.captures(trimmed) else {
                continue;
            };
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || name.is_empty() || email.is_empty() {
                continue;
            }

            let raw = format!("Copyright {year} (c) {name} <{email}>");
            if let Some(cr) = refine_copyright(&raw) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(name) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_copyright_by_without_year_lines(
    groups: &[Vec<(usize, String)>],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static COPYRIGHT_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcopyright\s+by\s+(?P<who>.+?)(?:\s+all\s+rights\s+reserved\b|\.|$)")
            .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for group in groups {
        if group.is_empty() {
            continue;
        }
        let combined = normalize_whitespace(
            &group
                .iter()
                .map(|(_, l)| l.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
        let Some(cap) = COPYRIGHT_BY_RE.captures(&combined) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }

        let who_lower = who.to_ascii_lowercase();
        if !(who_lower.contains("regents") && who_lower.contains("university")) {
            continue;
        }

        let who = who.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'));
        if who.is_empty() {
            continue;
        }
        let cr = format!("copyright by {who}");
        let start_line = group.first().map(|(n, _)| *n).unwrap_or(1);
        let end_line = group.last().map(|(n, _)| *n).unwrap_or(start_line);
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: LineNumber::new(start_line).expect("valid"),
            end_line: LineNumber::new(end_line).expect("valid"),
        });

        if let Some(holder) = refine_holder_in_copyright_context(who) {
            let start_line = group.first().map(|(n, _)| *n).unwrap_or(1);
            let end_line = group.last().map(|(n, _)| *n).unwrap_or(start_line);
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(start_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }
    }

    (copyrights, holders)
}
