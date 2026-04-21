// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::copyright::candidates::versioned_banner_holder_from_prepared;
use crate::copyright::line_tracking::{LineNumberIndex, PreparedLineCache};
use crate::copyright::refiner::{
    refine_copyright, refine_holder, refine_holder_in_copyright_context,
};
use crate::copyright::types::{CopyrightDetection, HolderDetection};
use crate::models::LineNumber;

use super::token_utils::normalize_whitespace;

pub fn extract_glide_3dfx_copyright_notice(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    static GLIDE_3DFX_COPYRIGHT_NOTICE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcopyright\s+notice\s*\(3dfx\s+interactive,\s+inc\.\s+1999\)").unwrap()
    });

    let mut seen: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        if let Some(m) = GLIDE_3DFX_COPYRIGHT_NOTICE_RE.find(line) {
            let raw = m.as_str();
            if let Some(refined) = refine_copyright(raw)
                && seen.insert(refined.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }
}

pub fn extract_spdx_filecopyrighttext_c_without_year(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static SPDX_COPYRIGHT_C_NO_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bSPDX-FileCopyrightText:\s*Copyright\s*\(c\)\s+(.+?)\s*$").unwrap()
    });

    let mut seen_cr: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<(String, usize)> = holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let trimmed = line.trim();
        let Some(caps) = SPDX_COPYRIGHT_C_NO_YEAR_RE.captures(trimmed) else {
            continue;
        };
        let tail = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if tail.is_empty() {
            continue;
        }

        let raw = format!("Copyright (c) {tail}");
        if let Some(refined) = refine_copyright(&raw)
            && seen_cr.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(holder) = refine_holder(tail)
            && seen_h.insert((holder.clone(), ln))
        {
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn extract_html_meta_name_copyright_content(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static META_COPYRIGHT_CONTENT_DQ_NAME_CONTENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)<meta\s+[^>]*\bname\s*=\s*"copyright"[^>]*\bcontent\s*=\s*"([^"]+)""#)
            .unwrap()
    });
    static META_COPYRIGHT_CONTENT_DQ_CONTENT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)<meta\s+[^>]*\bcontent\s*=\s*"([^"]+)"[^>]*\bname\s*=\s*"copyright""#)
            .unwrap()
    });
    static META_COPYRIGHT_CONTENT_SQ_NAME_CONTENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)<meta\s+[^>]*\bname\s*=\s*'copyright'[^>]*\bcontent\s*=\s*'([^']+)'")
            .unwrap()
    });
    static META_COPYRIGHT_CONTENT_SQ_CONTENT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)<meta\s+[^>]*\bcontent\s*=\s*'([^']+)'[^>]*\bname\s*=\s*'copyright'")
            .unwrap()
    });

    let mut seen_cr: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<(String, usize)> = holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let raw = if let Some(caps) = META_COPYRIGHT_CONTENT_DQ_NAME_CONTENT_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else if let Some(caps) = META_COPYRIGHT_CONTENT_DQ_CONTENT_NAME_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else if let Some(caps) = META_COPYRIGHT_CONTENT_SQ_NAME_CONTENT_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else if let Some(caps) = META_COPYRIGHT_CONTENT_SQ_CONTENT_NAME_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else {
            continue;
        };

        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }

        if let Some(refined) = refine_copyright(raw)
            && seen_cr.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });

            if let Some(holder) =
                super::postprocess_transforms::derive_holder_from_simple_copyright_string(&refined)
                && seen_h.insert((holder.clone(), ln))
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }
}

pub fn extract_added_the_copyright_year_for_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static ADDED_COPYRIGHT_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*added\s+the\s+copyright\s+year\s*\(\s*(?P<year>\d{4})\s*\)\s+for\s+(?P<holder>.+?)\s*$",
        )
        .unwrap()
    });

    let mut seen_cr: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<(String, usize)> = holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let Some(cap) = ADDED_COPYRIGHT_YEAR_RE.captures(prepared) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("");
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("");
        if year.is_empty() || holder_raw.trim().is_empty() {
            continue;
        }
        let holder = refine_holder(holder_raw).unwrap_or_else(|| holder_raw.trim().to_string());

        let cr = format!("Copyright year ({year}) for {holder}");
        if seen_cr.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if seen_h.insert((holder.clone(), ln)) {
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn extract_changelog_timestamp_copyrights_from_content(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static CHANGELOG_TS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})\s+(\d{2}:\d{2})\s+(.+?)\s*$").unwrap());

    let mut seen_cr: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<(String, usize)> = holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    let mut matches: Vec<(usize, String, String)> = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(caps) = CHANGELOG_TS_RE.captures(trimmed) else {
            continue;
        };
        let date = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let time = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let tail = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        if date.is_empty() || time.is_empty() || tail.is_empty() {
            continue;
        }
        matches.push((ln, format!("{date} {time}"), tail.to_string()));
    }

    if matches.len() < 2 {
        return;
    }

    let (ln, dt, tail) = &matches[0];
    let raw = format!("copyright {dt} {tail}");
    if let Some(refined) = refine_copyright(&raw)
        && seen_cr.insert(refined.clone())
    {
        copyrights.push(CopyrightDetection {
            copyright: refined,
            start_line: LineNumber::new(*ln).expect("invalid line number"),
            end_line: LineNumber::new(*ln).expect("invalid line number"),
        });
    }

    if let Some(holder) = refine_holder(tail)
        && seen_h.insert((holder.clone(), *ln))
    {
        holders.push(HolderDetection {
            holder,
            start_line: LineNumber::new(*ln).expect("invalid line number"),
            end_line: LineNumber::new(*ln).expect("invalid line number"),
        });
    }
}

pub fn drop_arch_floppy_h_bare_1995(content: &str, copyrights: &mut Vec<CopyrightDetection>) {
    let lower = content.to_ascii_lowercase();
    let is_x86 = lower.contains("_asm_x86_floppy_h");
    let is_powerpc = lower.contains("__asm_powerpc_floppy_h");
    if !is_x86 && !is_powerpc {
        return;
    }

    copyrights.retain(|c| !c.copyright.eq_ignore_ascii_case("Copyright (c) 1995"));
}

pub fn drop_batman_adv_contributors_copyright(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let lower = content.to_ascii_lowercase();
    if !lower.contains("_net_batman_adv_types_h_") {
        return;
    }

    copyrights.retain(|c| {
        !c.copyright
            .to_ascii_lowercase()
            .contains("b.a.t.m.a.n. contributors")
    });
    holders.retain(|h| h.holder != "B.A.T.M.A.N. contributors");
}

pub fn is_lppl_license_document(content: &str) -> bool {
    let first = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let first_is_lppl_title = first.eq_ignore_ascii_case("LaTeX Project Public License")
        || first.eq_ignore_ascii_case("The LaTeX Project Public License");
    if !first_is_lppl_title {
        return false;
    }
    content.to_ascii_lowercase().contains("lppl version")
}

pub fn extract_common_year_only_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
) {
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
        return;
    }

    let mut seen: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    for (ln, refined) in matches {
        if seen.insert(refined.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn extract_embedded_bare_c_year_suffixes(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
) {
    const MAX_YEAR: u32 = 2099;

    static EMBEDDED_BARE_C_YEAR_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\(c\)\s*((?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?)\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut seen: HashSet<String> = copyrights
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
}

pub fn extract_trailing_bare_c_year_range_suffixes(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
) {
    static TRAILING_BARE_C_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s*(?:19\d{2}|20\d{2})\s*[-–]\s*(?:19\d{2}|20\d{2})\s*\.?\s*$")
            .unwrap()
    });

    let mut seen: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();

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
            if let Some(cr) = refine_copyright(suffix)
                && seen.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_repeated_embedded_bare_c_year_suffixes(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
) {
    const MIN_REPEATS: usize = 2;
    const MAX_YEAR: u32 = 2020;

    static EMBEDDED_BARE_C_YEAR_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\(c\)\s*((?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?)\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut license_counts: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::new();
    let mut copyright_line_sets: std::collections::HashMap<String, (HashSet<String>, usize)> =
        std::collections::HashMap::new();
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

    let mut seen: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();

    for (bare, (count, first_ln)) in license_counts {
        if count < MIN_REPEATS {
            continue;
        }
        if let Some(refined) = refine_copyright(&bare)
            && seen.insert(refined.clone())
        {
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
        if let Some(refined) = refine_copyright(&bare)
            && seen.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(first_ln).expect("valid"),
                end_line: LineNumber::new(first_ln).expect("valid"),
            });
        }
    }
}

pub fn extract_lowercase_username_angle_email_copyrights(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static USER_EMAIL_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^[Cc]opyright\s*(?:\([Cc]\)\s*)?(19\d{2}|20\d{2})\s+([a-z0-9][a-z0-9_\-]{2,63})\s*<\s*([^>\s]+@[^>\s]+)\s*>\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&cr_raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if seen_holders.insert(user.to_string()) {
                holders.push(HolderDetection {
                    holder: user.to_string(),
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_lowercase_username_paren_email_copyrights(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static USER_EMAIL_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\b[Cc]opyright\s*(?:\([Cc]\)\s*)?(19\d{2}|20\d{2})\s+([a-z0-9][a-z0-9_\-]{2,63})\s*\(\s*([^\)\s]+@[^\)\s]+)\s*\)",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
                if let Some(cr) = refine_copyright(&cr_raw)
                    && seen_copyrights.insert(cr.clone())
                {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: LineNumber::new(*ln).expect("invalid line number"),
                        end_line: LineNumber::new(*ln).expect("invalid line number"),
                    });
                }

                if seen_holders.insert(user.to_string()) {
                    holders.push(HolderDetection {
                        holder: user.to_string(),
                        start_line: LineNumber::new(*ln).expect("invalid line number"),
                        end_line: LineNumber::new(*ln).expect("invalid line number"),
                    });
                }
            }
        }
    }
}

pub fn extract_c_year_range_by_name_comma_email_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static C_BY_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\(c\)\s+(?P<years>\d{4}(?:\s*[-–]\s*(?:\d{4}|\d{2}))?)\s+by\s+(?P<name>[^,]+),\s*(?P<email>[^\s,]+@[^\s,]+)\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&cr_raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder(name)
                && seen_holders.insert(h.clone())
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
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
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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

                if seen_copyrights.insert(full.to_ascii_lowercase()) {
                    copyrights.push(CopyrightDetection {
                        copyright: full.clone(),
                        start_line: LineNumber::new(*ln).expect("invalid line number"),
                        end_line: LineNumber::new(*ln).expect("invalid line number"),
                    });
                }

                let year_only_raw = format!("copyright {years}");
                if let Some(year_only) = refine_copyright(&year_only_raw) {
                    copyrights.retain(|c| {
                        !(c.start_line.get() == *ln
                            && c.end_line.get() == *ln
                            && c.copyright == year_only
                            && c.copyright != full)
                    });
                }

                if let Some(holder) = refine_holder_in_copyright_context(name)
                    && seen_holders.insert(holder.clone())
                {
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

pub fn extract_copyright_years_by_name_then_paren_email_next_line(
    prepared_cache: &mut PreparedLineCache<'_>,
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
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache
            .get_by_index(idx)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        let Some(cap) = COPY_YEARS_BY_NAME_RE.captures(&prepared) else {
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
        let matched = cap.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
        if years.is_empty() || name.is_empty() {
            continue;
        }
        name = name
            .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.'))
            .to_string();
        if name.is_empty() {
            continue;
        }

        let mut j = idx + 1;
        while j < prepared_cache.len() {
            let next_ln = j + 1;
            let Some(next_trimmed) = prepared_cache.get_by_index(j).map(|p| p.trim().to_string())
            else {
                break;
            };
            if next_trimmed.is_empty() {
                j += 1;
                continue;
            }

            let Some(email_cap) = LEADING_PAREN_EMAIL_RE.captures(&next_trimmed) else {
                break;
            };
            let email = email_cap
                .name("email")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim();
            if email.is_empty() {
                break;
            }

            let full_raw = format!("{} ({email})", matched.trim_end());
            if let Some(full) = refine_copyright(&full_raw)
                && seen_copyrights.insert(full.to_ascii_lowercase())
            {
                copyrights.push(CopyrightDetection {
                    copyright: full,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(next_ln).expect("valid"),
                });
            }

            let year_only_raw = format!("copyright {years}");
            if let Some(year_only) = refine_copyright(&year_only_raw) {
                copyrights.retain(|c| {
                    !(c.start_line.get() == ln
                        && c.end_line.get() == ln
                        && c.copyright == year_only)
                });
            }

            if let Some(holder) = refine_holder_in_copyright_context(&name)
                && seen_holders.insert(holder.clone())
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }

            break;
        }
    }
}

pub fn extract_copyright_year_name_with_of_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPY_YEAR_OF_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^Copyright\s*\(c\)\s+(?P<year>19\d{2}|20\d{2})\s+(?P<holder>[A-Z][A-Za-z0-9.'\-]*(?:\s+of\s+[A-Z][A-Za-z0-9.'\-]*)+)\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&cr_raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder(holder_raw)
                && seen_holders.insert(h.clone())
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn drop_url_extended_prefix_duplicates(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let has_url = |s: &str| s.contains("http://") || s.contains("https://");

    let mut drop = vec![false; copyrights.len()];
    for i in 0..copyrights.len() {
        let shorter = &copyrights[i];
        if has_url(&shorter.copyright) {
            continue;
        }

        for (j, longer) in copyrights.iter().enumerate() {
            if i == j {
                continue;
            }
            if !has_url(&longer.copyright) {
                continue;
            }
            if shorter.start_line != longer.start_line || shorter.end_line > longer.end_line {
                continue;
            }
            if !longer.copyright.starts_with(&shorter.copyright) {
                continue;
            }

            let tail = longer.copyright[shorter.copyright.len()..].trim_start();
            if tail.starts_with('-') || tail.starts_with("http") {
                drop[i] = true;
                break;
            }
        }
    }

    if drop.iter().all(|d| !*d) {
        return;
    }

    let mut kept = Vec::with_capacity(copyrights.len());
    for (i, c) in copyrights.iter().cloned().enumerate() {
        if !drop[i] {
            kept.push(c);
        }
    }
    *copyrights = kept;
}

pub fn extract_standalone_c_holder_year_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
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

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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

            let already_covered = copyrights.iter().any(|c| {
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
            if let Some(cr) = refine_copyright(&cr_raw)
                && seen_copyrights.insert(cr.clone())
            {
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

            if let Some(h) = refine_holder(holder_raw)
                && seen_holders.insert(h.clone())
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_c_holder_without_year_lines(
    content: &str,
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
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
        return;
    }

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&cr_raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(holder) = refine_holder_in_copyright_context(holder_raw)
                && seen_holders.insert(holder.clone())
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_versioned_project_c_holder_banner_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let mut seen_c: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

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
}

pub fn extract_c_years_then_holder_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

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

            if let Some(h) =
                super::postprocess_transforms::derive_holder_from_simple_copyright_string(&cr)
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
}

pub fn extract_copyright_c_years_holder_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPY_C_YEARS_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s*(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*?)\s+(?P<holder>.+?)\s*$",
        )
        .unwrap()
    });

    let mut seen_c: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

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
}

pub fn extract_three_digit_copyright_year_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static COPYRIGHT_C_3DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\s*\(c\)\s*(?P<year>\d{3})\s+(?P<tail>.+)$").unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        let Some(cap) = COPYRIGHT_C_3DIGIT_RE.captures(line) else {
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
        if seen_cr.insert((ln, refined.clone())) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(h) = refine_holder_in_copyright_context(tail)
            && seen_h.insert((ln, h.clone()))
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn extract_copyrighted_by_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static COPYRIGHTED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcopyrighted\s+by\s+(?P<who>(?-i:[\p{Lu}][^\.\,\;\)]+))").unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        if line.to_ascii_lowercase().contains("not copyrighted") {
            continue;
        }
        for cap in COPYRIGHTED_BY_RE.captures_iter(line) {
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
            if seen_cr.insert((ln, refined.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(who)
                && seen_h.insert((ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }
}

pub fn extract_c_word_year_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static C_WORD_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s+(?P<who>[\p{L}]{2,20})\s+(?P<year>(?:19\d{2}|20\d{2}))\b").unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        if !line.to_ascii_lowercase().contains("(c)") {
            continue;
        }
        for cap in C_WORD_YEAR_RE.captures_iter(line) {
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
            if seen_cr.insert((ln, refined.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(who)
                && seen_h.insert((ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }
}

pub fn extract_are_c_year_holder_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static ARE_C_YEAR_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bare\s*\(c\)\s*(?P<year>(?:19\d{2}|20\d{2}))\s+(?P<holder>[^,\.;]+)")
            .unwrap()
    });
    static TRAILING_UNDER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+under\b.*$").unwrap());

    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for ln in 1..=prepared_cache.len() {
        let Some(prepared) = prepared_cache.get(ln) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        if !line.to_ascii_lowercase().contains("(c)") {
            continue;
        }
        for cap in ARE_C_YEAR_HOLDER_RE.captures_iter(line) {
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
            if seen_cr.insert((ln, refined.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(&holder_raw)
                && seen_h.insert((ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }
}

pub fn extract_bare_c_by_holder_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static C_BY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*by\s+(?P<holder>[A-Z][^\n]+)$").unwrap());

    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for ln in 1..=prepared_cache.len() {
        let Some(prepared) = prepared_cache.get(ln) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        let Some(cap) = C_BY_RE.captures(line) else {
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
        if seen_cr.insert((ln, refined.clone())) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
        if let Some(h) = refine_holder_in_copyright_context(holder_raw)
            && seen_h.insert((ln, h.clone()))
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn extract_all_rights_reserved_by_holder_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static RESERVED_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)copyright\s*\(c\)\s*all\s+rights\s+reserved\s+by\s+(?P<holder>[^\n]+)$")
            .unwrap()
    });

    let mut seen_cr: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for ln in 1..=prepared_cache.len() {
        let Some(prepared) = prepared_cache.get(ln) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        let Some(cap) = RESERVED_BY_RE.captures(line) else {
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
        if seen_cr.insert((ln, refined.clone())) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(h) = refine_holder_in_copyright_context(holder_raw)
            && seen_h.insert((ln, h.clone()))
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn extract_holder_is_name_paren_email_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static HOLDER_IS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bholder\s+is\s+(?P<name>[^()]{2,}?)\s*\(\s*(?P<email>[^)\s]*@[^)\s]+)\s*\)",
        )
        .unwrap()
    });

    let mut seen_c: HashSet<(usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_h: HashSet<(usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.holder.clone()))
        .collect();

    for ln in 1..=prepared_cache.len() {
        let Some(prepared) = prepared_cache.get(ln) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        for cap in HOLDER_IS_RE.captures_iter(line) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() || email.is_empty() {
                continue;
            }
            let raw = format!("holder is {name} ({email})");
            let Some(cr) = refine_copyright(&raw) else {
                continue;
            };
            if seen_c.insert((ln, cr.clone())) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(name)
                && seen_h.insert((ln, h.clone()))
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }
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

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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

            if seen_copyrights.insert(cr.clone()) {
                copyrights.push(CopyrightDetection {
                    copyright: cr.clone(),
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

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

            if seen_holders.insert(h.clone()) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn apply_european_community_copyright(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static EUROPEAN_COMMUNITY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:©|\(c\))\s*the\s+european\s+community\s+(\d{4})").unwrap()
    });

    let Some(cap) = EUROPEAN_COMMUNITY_RE.captures(content) else {
        return;
    };
    let Some(m) = cap.get(0) else {
        return;
    };
    let year = cap.get(1).map(|m| m.as_str());
    let Some(year) = year else {
        return;
    };

    let holder = "the European Community";
    let desired_copyright = format!("(c) {holder} {year}");
    let ln = line_number_index.line_number_at_offset(m.start());

    if !copyrights.iter().any(|c| c.copyright == desired_copyright) {
        copyrights.push(CopyrightDetection {
            copyright: desired_copyright,
            start_line: ln,
            end_line: ln,
        });
    }

    if !holders.iter().any(|h| h.holder == holder) {
        holders.push(HolderDetection {
            holder: holder.to_string(),
            start_line: ln,
            end_line: ln,
        });
    }
}

pub fn apply_javadoc_company_metadata(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static JAVADOC_P_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<p>\s*Copyright:\s*Copyright\s*\(c\)\s*(\d{4})\s*</p>").unwrap()
    });
    static JAVADOC_P_COMPANY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<p>\s*Company:\s*([^<\r\n]+)").unwrap());

    let Some(copy_cap) = JAVADOC_P_COPYRIGHT_RE.captures(content) else {
        return;
    };
    let year = copy_cap.get(1).map(|m| m.as_str());

    let company_val = JAVADOC_P_COMPANY_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim());

    let (Some(year), Some(company_val)) = (year, company_val) else {
        return;
    };

    let ln = copy_cap
        .get(0)
        .map(|m| line_number_index.line_number_at_offset(m.start()).get())
        .unwrap_or(1);

    let append_company_value = company_val.split_whitespace().count() >= 2;
    let company_holder = if append_company_value {
        format!("Company {company_val}")
    } else {
        "Company".to_string()
    };

    let base_holder = "Company";
    let base_copyright = format!("Copyright (c) {year} {base_holder}");
    let desired_copyright = format!("Copyright (c) {year} {company_holder}");

    copyrights.retain(|c| c.copyright != desired_copyright && c.copyright != base_copyright);
    holders.retain(|h| {
        h.holder != company_holder && (!append_company_value || h.holder != base_holder)
    });

    if !copyrights.iter().any(|c| c.copyright == desired_copyright) {
        copyrights.push(CopyrightDetection {
            copyright: desired_copyright,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }

    if !holders.iter().any(|h| h.holder == company_holder) {
        holders.push(HolderDetection {
            holder: company_holder,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }
}

pub fn extract_html_entity_year_range_copyrights(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    static COPY_ENTITY_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Copyright\s*&copy;?\s*(\d{4}\s*[-–]\s*\d{4})\b").unwrap()
    });
    static HEX_A9_ENTITY_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Copyright\s*&#xA9;?\s*(\d{4}\s*[-–]\s*\d{4})\b").unwrap()
    });
    static DEC_169_ENTITY_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Copyright\s*&#169;?\s*(\d{4}\s*[-–]\s*\d{4})\b").unwrap()
    });
    static ARE_COPYRIGHT_C_RANGE_DOT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bare\s+copyright\s*\(c\)\s*(\d{4}\s*[-–]\s*\d{4})\s*\.").unwrap()
    });

    let mut seen: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();

    let is_terminator = |s: &str| {
        let tail = s.trim_start();
        if tail.is_empty() {
            return true;
        }
        matches!(
            tail.chars().next(),
            Some('<' | '"' | '\'' | ')' | ']' | '}' | '.' | ';' | ':')
        )
    };

    for cap in COPY_ENTITY_RANGE_RE.captures_iter(content) {
        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());
        if !is_terminator(&content[m.end()..]) {
            continue;
        }
        let range = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if range.is_empty() {
            continue;
        }
        let raw = format!("Copyright (c) {range}");
        if let Some(refined) = refine_copyright(&raw)
            && seen.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: ln,
                end_line: ln,
            });
        }
    }

    for cap in HEX_A9_ENTITY_RANGE_RE
        .captures_iter(content)
        .chain(DEC_169_ENTITY_RANGE_RE.captures_iter(content))
    {
        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());
        if !is_terminator(&content[m.end()..]) {
            continue;
        }
        let range = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if range.is_empty() {
            continue;
        }
        let raw = format!("(c) {range}");
        if let Some(refined) = refine_copyright(&raw)
            && seen.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: ln,
                end_line: ln,
            });

            let full = format!("Copyright (c) {range}");
            copyrights.retain(|c| !(c.start_line == ln && c.end_line == ln && c.copyright == full));
        }
    }

    for cap in ARE_COPYRIGHT_C_RANGE_DOT_RE.captures_iter(content) {
        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());
        let range = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if range.is_empty() {
            continue;
        }
        let raw = format!("Copyright (c) {range}");
        if let Some(refined) = refine_copyright(&raw)
            && seen.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: ln,
                end_line: ln,
            });
        }
    }
}

pub fn normalize_pudn_html_footer_copyrights(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.to_ascii_lowercase().contains("pudn.com") {
        return;
    }

    static PUDN_FOOTER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)&#169;\s*(?P<range>\d{4}\s*[-–]\s*\d{4})\s*<a\b[^>]*\bhref\s*=\s*['\"](?P<url>https?://[^'\">]+)['\"][^>]*>\s*<font\b[^>]*\bcolor\s*=\s*['\"](?P<color>[^'\">]+)['\"][^>]*>\s*(?P<name>[^<]+?)\s*</font>\s*</a>"#,
        )
        .unwrap()
    });
    let mut seen_copyrights: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let mut seen_holders: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    let mut saw_pudn_footer = false;

    for cap in PUDN_FOOTER_RE.captures_iter(content) {
        saw_pudn_footer = true;

        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());

        let years_raw = cap.name("range").map(|m| m.as_str()).unwrap_or("").trim();
        let mut years = years_raw.replace('–', "-");
        years = years.split_whitespace().collect::<Vec<_>>().join(" ");
        years = years
            .replace(" - ", "-")
            .replace(" -", "-")
            .replace("- ", "-");

        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();

        if years.is_empty() || name.is_empty() {
            continue;
        }

        if !name.to_ascii_lowercase().contains("pudn.com") {
            continue;
        }

        let expected_copyright = normalize_whitespace(&format!("(c) {years} pudn.com"));
        let expected_holder = "pudn.com".to_string();

        let ckey = (ln.get(), ln.get(), expected_copyright.clone());
        if seen_copyrights.insert(ckey) {
            copyrights.push(CopyrightDetection {
                copyright: expected_copyright.clone(),
                start_line: ln,
                end_line: ln,
            });
        }

        let hkey = (ln.get(), ln.get(), expected_holder.clone());
        if seen_holders.insert(hkey) {
            holders.push(HolderDetection {
                holder: expected_holder,
                start_line: ln,
                end_line: ln,
            });
        }

        let canonical_lc = expected_copyright.to_ascii_lowercase();
        let bare_prefix = format!("(c) {}", years.to_ascii_lowercase());
        copyrights.retain(|c| {
            if c.start_line != ln || c.end_line != ln {
                return true;
            }
            let lc = c.copyright.to_ascii_lowercase();
            if lc == canonical_lc {
                return true;
            }
            if lc.contains("upload_log.asp") {
                return false;
            }
            if lc.contains("pudn.com") {
                return false;
            }
            !(lc.starts_with(&bare_prefix) && lc.contains("icp"))
        });

        holders.retain(|h| {
            if h.start_line != ln || h.end_line != ln {
                return true;
            }
            let lower = h.holder.to_ascii_lowercase();
            if lower == "pudn.com" {
                return true;
            }
            if lower.contains("upload_log.asp") || lower.contains("pudn.com") {
                return false;
            }
            let char_count = h.holder.chars().count().max(1);
            let non_ascii_count = h.holder.chars().filter(|ch| !ch.is_ascii()).count();
            let non_ascii_ratio = non_ascii_count as f32 / char_count as f32;
            non_ascii_ratio <= 0.75
        });
    }

    if saw_pudn_footer {
        let has_mojibake_markers = |s: &str| {
            s.len() > 40
                && (s.contains("¿Ø")
                    || s.contains("¼þ")
                    || s.contains("£¨")
                    || s.contains("ÏÔ")
                    || s.contains("×é")
                    || s.contains("¶àÐÐ"))
        };

        holders.retain(|h| {
            let lower = h.holder.to_ascii_lowercase();
            if lower == "pudn.com" {
                return true;
            }
            if lower == "pudn.com"
                || lower.contains("pudn.com ï")
                || (lower.contains("pudn.com") && lower.contains("icp"))
                || lower.contains("upload_log.asp")
            {
                return false;
            }
            let char_count = h.holder.chars().count().max(1);
            let non_ascii_count = h.holder.chars().filter(|ch| !ch.is_ascii()).count();
            let non_ascii_ratio = non_ascii_count as f32 / char_count as f32;
            let has_ascii_alnum = h.holder.chars().any(|ch| ch.is_ascii_alphanumeric());
            if non_ascii_ratio > 0.75
                && !lower.contains("http://")
                && !lower.contains("https://")
                && !lower.contains('@')
                && (!has_ascii_alnum || h.holder.chars().count() <= 8)
            {
                return false;
            }
            !has_mojibake_markers(&h.holder)
        });
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
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let has_copyright_label_lines = groups.iter().flatten().any(|(_, l)| {
        l.trim_start()
            .to_ascii_lowercase()
            .starts_with("copyright:")
    });
    if !has_copyright_label_lines {
        return;
    }

    static COPY_C_YEAR_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:\d{4}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*)\s*,\s*(?P<name>[^<>]+?)\s*<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>\s*[\.,;:]*\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> = copyrights
        .iter()
        .map(|c| c.copyright.to_ascii_lowercase())
        .collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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

            if seen_copyrights.insert(cr.to_ascii_lowercase()) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(holder) = refine_holder_in_copyright_context(name)
                && seen_holders.insert(holder.clone())
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
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

pub fn strip_trailing_c_year_suffix_from_comma_and_others(copyrights: &mut [CopyrightDetection]) {
    static COMMA_OTHERS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^Copyright (?P<who>.+, .+ and others)\s+\(c\)\s+(?P<year>\d{4})$").unwrap()
    });

    for det in copyrights.iter_mut() {
        let Some(cap) = COMMA_OTHERS_RE.captures(&det.copyright) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        det.copyright = format!("Copyright {who}");
    }
}

pub fn extract_name_before_rewrited_by_copyrights(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
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
        return;
    }

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for idx in 0..prepared_cache.len().saturating_sub(1) {
        let ln1 = idx + 1;
        let ln2 = idx + 2;

        let Some(l1) = prepared_cache
            .get_by_index(idx)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        let Some(l2) = prepared_cache
            .get_by_index(idx + 1)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if l1.is_empty() || l2.is_empty() {
            continue;
        }

        let Some(cap1) = NAME_EMAIL_YEARS_RE.captures(&l1) else {
            continue;
        };
        let Some(cap2) = REWRITED_BY_RE.captures(&l2) else {
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
        if let Some(refined) = refine_copyright(&combined_raw)
            && seen_copyrights.insert(refined.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }

        let holder_raw = format!("{name1} {prefix2} {name2}");
        if let Some(holder) = refine_holder(&holder_raw)
            && seen_holders.insert(holder.clone())
        {
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }
    }
}

pub fn extract_developed_at_software_copyrights(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static DEVELOPED_AT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bat\s+(?P<holder>[^\n,]+,\s*inc\.,\s*software)\s+copyright\s*\(c\)\s+(?P<year>(?:19\d{2}|20\d{2}))\b",
        )
        .unwrap()
    });

    if prepared_cache.is_empty() {
        return;
    }

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx).map(|p| p.to_string()) else {
            continue;
        };
        let mut candidates: Vec<(usize, String)> = vec![(ln, prepared.clone())];
        if let Some(next) = prepared_cache.get_by_index(idx + 1)
            && !next.trim().is_empty()
        {
            candidates.push((ln, format!("{} {}", prepared.trim_end(), next.trim_start())));
        }

        for (_ln, candidate) in candidates {
            for cap in DEVELOPED_AT_RE.captures_iter(&candidate) {
                let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
                let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
                if holder.is_empty() || year.is_empty() {
                    continue;
                }
                let cr = format!("at {holder} copyright (c) {year}");
                if seen_copyrights.insert(cr.clone()) {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln).unwrap(),
                    });
                }
                let h = holder.to_string();
                if seen_holders.insert(h.clone()) {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln).unwrap(),
                    });
                }
            }
        }
    }
}

pub fn extract_confidential_proprietary_copyrights(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(line) = prepared_cache
            .get_by_index(idx)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if line.is_empty() {
            continue;
        }

        if let Some(cap) = HOLDER_C_COPYRIGHT_YEAR_RE.captures(&line) {
            let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            if !holder.is_empty() && !year.is_empty() {
                let cr = format!("{holder} (c) Copyright {year}");
                if let Some(refined) = refine_copyright(&cr)
                    && seen_copyrights.insert(refined.clone())
                {
                    copyrights.push(CopyrightDetection {
                        copyright: refined,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln).unwrap(),
                    });
                }
                if let Some(h) = refine_holder_in_copyright_context(holder)
                    && seen_holders.insert(h.clone())
                {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln).unwrap(),
                    });
                }

                let bare = format!("(c) Copyright {year}");
                if let Some(refined_bare) = refine_copyright(&bare) {
                    copyrights.retain(|c| c.copyright != refined_bare);
                }
            }
        }

        if let Some(cap) = ABC_LINE_RE.captures(&line) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let tag = cap.name("tag").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || tag.is_empty() {
                continue;
            }
            let Some(next_clean) = prepared_cache.get_by_index(idx + 1).map(|p| {
                p.trim()
                    .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
                    .to_string()
            }) else {
                continue;
            };
            if !next_clean.is_empty() && CONFIDENTIAL_RE.is_match(&next_clean) {
                let cr_raw = format!("COPYRIGHT {year} {tag} {next_clean}");
                if let Some(cr) = refine_copyright(&cr_raw)
                    && seen_copyrights.insert(cr.clone())
                {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln + 1).expect("invalid line number"),
                    });
                }
                let holder_raw = format!("{tag} {next_clean}");
                if let Some(h) = refine_holder_in_copyright_context(&holder_raw)
                    && seen_holders.insert(h.clone())
                {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln + 1).expect("invalid line number"),
                    });
                }
            }
        }

        if let Some(cap) = MOTOROLA_RE.captures(&line) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let base_holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() || base_holder.is_empty() {
                continue;
            }
            let Some(next_clean) = prepared_cache.get_by_index(idx + 1).map(|p| {
                p.trim()
                    .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
                    .to_string()
            }) else {
                continue;
            };
            if !next_clean.is_empty() && CONFIDENTIAL_RE.is_match(&next_clean) {
                let cr_raw = format!("Copyright {year} (c), {base_holder} - {next_clean}");
                if let Some(cr) = refine_copyright(&cr_raw)
                    && seen_copyrights.insert(cr.clone())
                {
                    copyrights.push(CopyrightDetection {
                        copyright: cr,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln + 1).expect("invalid line number"),
                    });
                }

                let nodash_raw = format!("Copyright {year} (c), {base_holder} {next_clean}");
                if let Some(nodash) = refine_copyright(&nodash_raw) {
                    copyrights.retain(|c| c.copyright != nodash);
                }

                let holder_raw = format!("{base_holder} - {next_clean}");
                if let Some(h) = refine_holder_in_copyright_context(&holder_raw)
                    && seen_holders.insert(h.clone())
                {
                    holders.push(HolderDetection {
                        holder: h,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(ln + 1).expect("invalid line number"),
                    });
                }
            }
        }
    }
}

pub fn strip_trailing_the_source_suffixes(copyrights: &mut [CopyrightDetection]) {
    static THE_SOURCE_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)(?:[\.,;:]?\s+)the\s+source\s*$").unwrap()
    });

    for det in copyrights.iter_mut() {
        let lower = det.copyright.to_ascii_lowercase();
        if !lower.contains("copyright") {
            continue;
        }
        if !lower.trim_end().ends_with("the source") {
            continue;
        }
        let Some(cap) = THE_SOURCE_SUFFIX_RE.captures(&det.copyright) else {
            continue;
        };
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if prefix.is_empty() {
            continue;
        }
        det.copyright = prefix.to_string();
    }
}

pub fn truncate_stichting_mathematisch_centrum_amsterdam_netherlands(
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    const FULL: &str = "Stichting Mathematisch Centrum, Amsterdam, The Netherlands";
    const SHORT: &str = "Stichting Mathematisch Centrum, Amsterdam";

    for det in copyrights.iter_mut() {
        if det.copyright.contains(FULL) {
            det.copyright = det.copyright.replace(FULL, SHORT);
        }
    }
    for det in holders.iter_mut() {
        if det.holder == FULL {
            det.holder = SHORT.to_string();
        }
    }
}

pub fn extract_copyright_year_c_holder_mid_sentence_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPY_YEAR_C_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*copyright\s+(?P<year>19\d{2}|20\d{2})\s+\(c\)\s+(?P<holder>.+?)\s+is\s+licensed\b",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(holder)
                && seen_holders.insert(h.clone())
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_javadoc_author_copyright_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static JAVADOC_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*@author\s+(?P<name>.+?)\s*,?\s*\(\s*c\s*\)\s*(?P<year>(?:19|20)\d{2})\b",
        )
        .unwrap()
    });

    let mut seen_c: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&cr_raw)
                && seen_c.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(&name)
                && seen_h.insert(h.clone())
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_xml_copyright_tag_c_lines(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.to_ascii_lowercase().contains("<copyright") {
        return;
    }

    static BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<\s*copyright\b[^>]*>(?P<body>.*?)</\s*copyright\s*>").unwrap()
    });
    static C_SEG_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*(?P<body>.+)").unwrap());
    static ALL_RIGHTS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i),?\s*all\s+rights\s+reserved\.?\s*$").unwrap());

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for cap in BLOCK_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let inner = cap.name("body").map(|m| m.as_str()).unwrap_or("");
        if inner.is_empty() {
            continue;
        }

        let mut bodies: Vec<String> = Vec::new();
        for raw_line in inner.lines() {
            let prepared = crate::copyright::prepare::prepare_text_line(raw_line);
            let line = prepared.trim();
            if line.is_empty() {
                continue;
            }
            let Some(c_cap) = C_SEG_RE.captures(line) else {
                continue;
            };
            let mut body = c_cap
                .name("body")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if body.is_empty() {
                continue;
            }
            body = ALL_RIGHTS_RE.replace(&body, "").into_owned();
            body = body
                .trim()
                .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
                .to_string();
            if body.is_empty() {
                continue;
            }
            bodies.push(body);
        }

        if bodies.len() < 2 {
            continue;
        }

        let combined = bodies
            .iter()
            .map(|b| format!("(c) {b}"))
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(cr) = refine_copyright(&combined)
            && seen_copyrights.insert(cr.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: ln,
                end_line: ln,
            });
        }

        let combined_holder = bodies.join(" ");
        if let Some(h) = refine_holder_in_copyright_context(&combined_holder)
            && seen_holders.insert(h.clone())
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: ln,
                end_line: ln,
            });
        }

        let mut to_remove: HashSet<String> = HashSet::new();
        for b in &bodies {
            to_remove.insert(b.clone());
            to_remove.insert(b.trim_end_matches('.').to_string());
        }
        holders.retain(|h| !to_remove.contains(&h.holder));
    }
}

pub fn extract_copyright_its_authors_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static ITS_AUTHORS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcopyright\s+its\s+authors\b(?P<tail>.*)$").unwrap());

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if seen_copyrights.insert(cr.clone()) {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            let holder = "its authors".to_string();
            if seen_holders.insert(holder.clone()) {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_us_government_year_placeholder_copyrights(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\b.*\bYEAR\b.*\bUnited\s+States\s+Government\b").unwrap()
    });
    static HAS_DIGIT_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(?:19\d{2}|20\d{2})\b").unwrap());

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            let holder_raw = "United States Government";
            if let Some(holder) = refine_holder_in_copyright_context(holder_raw)
                && seen_holders.insert(holder.clone())
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_copyright_notice_paren_year_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\bcopyright\s+notice\s*\(\s*(?P<year>\d{4})\s*\)\s+(?P<holder>[^\n]+?)\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(holder) = refine_holder_in_copyright_context(holder_raw)
                && seen_holders.insert(holder.clone())
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn drop_shadowed_bare_c_holders_with_year_prefixed_copyrights(
    copyrights: &mut Vec<CopyrightDetection>,
    _holders: &mut Vec<HolderDetection>,
) {
    static COPY_YEAR_C_HOLDER_FULL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+(?:19\d{2}|20\d{2})\s+\(c\)\s+(?P<holder>.+)$").unwrap()
    });

    let mut covered: HashSet<String> = HashSet::new();
    for c in copyrights.iter() {
        if let Some(cap) = COPY_YEAR_C_HOLDER_FULL_RE.captures(&c.copyright) {
            let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
            if !holder.is_empty() {
                covered.insert(holder.to_ascii_lowercase());
            }
        }
    }
    if covered.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        if let Some(tail) = c.copyright.strip_prefix("(c)") {
            let holder = tail.trim();
            !covered.contains(&holder.to_ascii_lowercase())
        } else {
            true
        }
    });
}

pub fn extract_initials_holders_from_copyrights(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    static INITIALS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s+\(c\)\s+(?:19\d{2}|20\d{2})\s*,?\s+(?P<holder>[A-Z](?:\s+[A-Z]){1,2})$",
        )
        .unwrap()
    });

    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for det in copyrights {
        let Some(cap) = INITIALS_RE.captures(&det.copyright) else {
            continue;
        };
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if holder_raw.is_empty() {
            continue;
        }
        if let Some(holder) = refine_holder_in_copyright_context(holder_raw)
            && seen_holders.insert(holder.clone())
        {
            holders.push(HolderDetection {
                holder,
                start_line: det.start_line,
                end_line: det.end_line,
            });
        }
    }
}

pub fn extract_html_anchor_copyright_url(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.to_ascii_lowercase().contains("href=") {
        return;
    }

    static A_HREF_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)<\s*a\b[^>]*\bhref\s*=\s*['\"](?P<url>https?://[^'\">]+)['\"][^>]*>\s*copyright\s*</\s*a\s*>"#,
        )
        .unwrap()
    });
    static COPY_SYMBOL_A_HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)(?:&copy;|&#169;|&#xa9;|&#xA9;|\(c\)|©)\s*<\s*a\b[^>]*\bhref\s*=\s*(?:\\?['\"])(?P<url>https?://[^\\'\">]+)(?:\\?['\"])[^>]*>\s*(?P<text>[^<]+?)\s*</\s*a\s*>"#,
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for cap in A_HREF_COPYRIGHT_RE.captures_iter(content) {
        let start_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let end_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.end()))
            .unwrap_or(start_line);
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or("").trim();
        if url.is_empty() {
            continue;
        }
        let url = url.split('#').next().unwrap_or(url).trim();
        if url.is_empty() {
            continue;
        }

        let cr = format!("copyright {url}");
        if seen_copyrights.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line,
                end_line,
            });
        }

        let holder = url.to_string();
        if seen_holders.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line,
                end_line,
            });
        }
    }

    for cap in COPY_SYMBOL_A_HREF_RE.captures_iter(content) {
        let start_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let end_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.end()))
            .unwrap_or(start_line);
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or("").trim();
        let holder = cap.name("text").map(|m| m.as_str()).unwrap_or("").trim();
        if url.is_empty() || holder.is_empty() {
            continue;
        }
        let url = url.split('#').next().unwrap_or(url).trim();
        if url.is_empty() {
            continue;
        }

        let cr = format!("(c) {url} {holder}");
        if seen_copyrights.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line,
                end_line,
            });
        }

        let holder = holder.to_string();
        if seen_holders.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line,
                end_line,
            });
        }
    }
}

pub fn extract_angle_bracket_year_name_copyrights(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
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
    let mut seen_holders: HashSet<String> = holders
        .iter()
        .map(|h| h.holder.to_ascii_lowercase())
        .collect();

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
                    copyrights.push(CopyrightDetection {
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
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_html_icon_class_copyrights(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let lower = content.to_ascii_lowercase();
    if !lower.contains("fa-copyright") && !lower.contains("glyphicon-copyright-mark") {
        return;
    }

    static FA_AUTHORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?is)\bcopyright\b(?P<middle>.*?)\bfa-copyright\b(?P<tail>.*?)\b(?P<year>19\d{2}|20\d{2})\b\s+by\s+the\s+authors\b",
        )
        .unwrap()
    });
    static GLYPHICON_DALEGROUP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)\bcopyright\b(?P<middle>.*?)\bglyphicon-copyright-mark\b(?P<tail>.*?)<\s*a\b[^>]*\bhref\s*=\s*['\"](?P<url>https?://[^'\">]+)['\"]"#,
        )
        .unwrap()
    });
    static GLYPHICON_RUBIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?is)\bcopyright\b(?P<middle>.*?)\bglyphicon-copyright-mark\b(?P<tail>.*?)\b(?P<years>\d{4}\s*[-–]\s*\d{4})\b\s+(?P<name>Rubix)\b",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for cap in FA_AUTHORS_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() {
            continue;
        }
        let cr = format!("Copyright fa-copyright {year} by the authors");
        if seen_copyrights.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: ln,
                end_line: ln,
            });
        }
        let holder = "fa-copyright by the authors".to_string();
        if seen_holders.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: ln,
                end_line: ln,
            });
        }

        let simple = format!("Copyright {year} by the authors");
        copyrights.retain(|c| c.copyright != simple);
        holders.retain(|h| h.holder != "the authors");
    }

    for cap in GLYPHICON_DALEGROUP_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or("").trim();
        if url.is_empty() {
            continue;
        }
        let url = url.split('#').next().unwrap_or(url).trim_end_matches('/');
        if url.is_empty() {
            continue;
        }

        let cr = format!("Copyright glyphicon-copyright-mark {url}");
        if seen_copyrights.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: ln,
                end_line: ln,
            });
        }
        let holder = "glyphicon-copyright-mark".to_string();
        if seen_holders.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: ln,
                end_line: ln,
            });
        }

        copyrights.retain(|c| c.copyright != "Copyright Dalegroup");
        holders.retain(|h| h.holder != "Dalegroup");
    }

    for cap in GLYPHICON_RUBIX_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() {
            continue;
        }
        let cr = format!("Copyright glyphicon-copyright-mark {years} Rubix");
        if seen_copyrights.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: ln,
                end_line: ln,
            });
        }

        let holder = "glyphicon-copyright-mark Rubix".to_string();
        if seen_holders.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: ln,
                end_line: ln,
            });
        }

        let simple = format!("Copyright {years} Rubix");
        copyrights.retain(|c| c.copyright != simple);
        holders.retain(|h| h.holder != "Rubix");
    }
}

pub fn extract_copyright_year_c_name_angle_email_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPY_YEAR_C_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s+(?P<year>19\d{2}|20\d{2})\s+\(c\)\s+(?P<name>.+?)\s*<\s*(?P<email>[^>\s]+@[^>\s]+)\s*>\s*$",
        )
        .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
            if let Some(cr) = refine_copyright(&raw)
                && seen_copyrights.insert(cr.clone())
            {
                copyrights.push(CopyrightDetection {
                    copyright: cr,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }

            if let Some(h) = refine_holder_in_copyright_context(name)
                && seen_holders.insert(h.clone())
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(*ln).expect("invalid line number"),
                    end_line: LineNumber::new(*ln).expect("invalid line number"),
                });
            }
        }
    }
}

pub fn extract_copyright_by_without_year_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPYRIGHT_BY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcopyright\s+by\s+(?P<who>.+?)(?:\s+all\s+rights\s+reserved\b|\.|$)")
            .unwrap()
    });

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
        if seen_copyrights.insert(cr.clone()) {
            let start_line = group.first().map(|(n, _)| *n).unwrap_or(1);
            let end_line = group.last().map(|(n, _)| *n).unwrap_or(start_line);
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(start_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }

        if let Some(holder) = refine_holder_in_copyright_context(who)
            && seen_holders.insert(holder.clone())
        {
            let start_line = group.first().map(|(n, _)| *n).unwrap_or(1);
            let end_line = group.last().map(|(n, _)| *n).unwrap_or(start_line);
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(start_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }
    }
}

pub fn drop_shadowed_and_or_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let mut by_span: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for h in holders.iter() {
        by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .push(h.holder.clone());
    }

    holders.retain(|h| {
        let Some(group) = by_span.get(&(h.start_line.get(), h.end_line.get())) else {
            return true;
        };

        let short = h.holder.as_str();
        let shadow_prefix = format!("{short} and/or ");

        let is_shadowed = group.iter().any(|other| {
            other.len() > short.len()
                && other.starts_with(&shadow_prefix)
                && other[shadow_prefix.len()..]
                    .to_lowercase()
                    .starts_with("its ")
        });

        !is_shadowed
    });
}

pub fn drop_shadowed_prefix_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let mut by_span: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for h in holders.iter() {
        by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .push(h.holder.clone());
    }

    holders.retain(|h| {
        let Some(group) = by_span.get(&(h.start_line.get(), h.end_line.get())) else {
            return true;
        };

        let short = h.holder.trim();
        let is_short_acronym =
            (2..=3).contains(&short.len()) && short.chars().all(|c| c.is_ascii_uppercase());
        if short.len() < 4 && !is_short_acronym {
            return true;
        }

        let mut shadowed = false;

        if !short.contains(',') {
            let shadow_prefix = format!("{short}, ");
            shadowed = group
                .iter()
                .any(|other| other.len() > short.len() && other.starts_with(&shadow_prefix));
        }

        if !shadowed {
            shadowed = group.iter().any(|other| {
                other.len() > short.len()
                    && other.starts_with(short)
                    && other
                        .as_bytes()
                        .get(short.len())
                        .is_some_and(|b| *b == b',')
            });
        }

        if !shadowed {
            shadowed = group.iter().any(|other| {
                if other.len() <= short.len() {
                    return false;
                }
                if !other.starts_with(short) {
                    return false;
                }
                let tail = other.get(short.len()..).unwrap_or("");
                let tail = tail.trim_start();
                tail.starts_with('-') || tail.starts_with('(')
            });
        }

        if !shadowed
            && short.split_whitespace().count() == 1
            && short.chars().all(|c| c.is_ascii_lowercase())
        {
            let shadow_prefix = format!("{short} ");
            shadowed = group
                .iter()
                .any(|other| other.len() > short.len() && other.starts_with(&shadow_prefix));
        }

        !shadowed
    });
}

pub fn drop_shadowed_prefix_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let mut by_span: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for c in copyrights.iter() {
        by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .push(c.copyright.clone());
    }

    copyrights.retain(|c| {
        let Some(group) = by_span.get(&(c.start_line.get(), c.end_line.get())) else {
            return true;
        };
        let short = c.copyright.as_str();
        if short.len() < 10 {
            return true;
        }

        if group.iter().any(|other| {
            if other.len() <= short.len() {
                return false;
            }
            if !other.starts_with(short) {
                return false;
            }
            let tail = other.get(short.len()..).unwrap_or("");
            let tail = tail.trim_start();
            tail.starts_with('-')
        }) {
            return false;
        }

        if group.iter().any(|other| {
            other.len() > short.len()
                && other.starts_with(short)
                && other
                    .as_bytes()
                    .get(short.len())
                    .is_some_and(|b| *b == b',')
        }) {
            return false;
        }
        let words: Vec<&str> = short.split_whitespace().collect();
        if words.is_empty() {
            return true;
        }
        if !words[0].eq_ignore_ascii_case("copyright") {
            return true;
        }

        if words.len() == 2 {
            let tail = words[1];
            let is_acronym =
                (2..=10).contains(&tail.len()) && tail.chars().all(|c| c.is_ascii_uppercase());
            if !is_acronym {
                return true;
            }
            let shadow_prefix_comma = format!("{short},");
            return !group
                .iter()
                .any(|other| other.len() > short.len() && other.starts_with(&shadow_prefix_comma));
        }

        if group.iter().any(|other| {
            if other.len() <= short.len() {
                return false;
            }
            if !other.starts_with(short) {
                return false;
            }
            let tail = other.get(short.len()..).unwrap_or("");
            let tail = tail.trim_start();
            tail.starts_with('(')
        }) {
            return false;
        }

        if words.len() != 3 {
            return true;
        }
        if !words[2].chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }

        !group.iter().any(|other| {
            other.len() > short.len()
                && other.starts_with(short)
                && other
                    .as_bytes()
                    .get(short.len())
                    .is_some_and(|b| *b == b' ')
        })
    });
}

pub fn drop_shadowed_bare_c_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let mut bare_by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for c in copyrights.iter() {
        let trimmed = c.copyright.trim();
        if !trimmed
            .get(.."Copyright".len())
            .is_some_and(|p| p.eq_ignore_ascii_case("Copyright"))
        {
            continue;
        }
        let tail = trimmed.get("Copyright".len()..).unwrap_or("").trim_start();
        if !tail.to_ascii_lowercase().starts_with("(c)") {
            continue;
        }
        bare_by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .insert(normalize_whitespace(tail));
    }

    if bare_by_span.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        let trimmed = normalize_whitespace(c.copyright.trim());
        if !trimmed.to_ascii_lowercase().starts_with("(c)") {
            return true;
        }
        bare_by_span
            .get(&(c.start_line.get(), c.end_line.get()))
            .is_none_or(|set| !set.contains(&trimmed))
    });
}

pub fn drop_copyright_shadowed_by_bare_c_copyrights_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    let mut tails: HashSet<(usize, usize, String)> = HashSet::new();
    for c in copyrights.iter() {
        let trimmed = c.copyright.trim_start();
        if !trimmed.to_ascii_lowercase().starts_with("(c)") {
            continue;
        }
        let tail = trimmed
            .trim_start_matches(|ch: char| ch != ')')
            .trim_start_matches(')')
            .trim_start();
        if !tail
            .get(.."Copyright".len())
            .is_some_and(|p| p.eq_ignore_ascii_case("Copyright"))
        {
            continue;
        }
        tails.insert((
            c.start_line.get(),
            c.end_line.get(),
            normalize_whitespace(tail),
        ));
    }

    if tails.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        let trimmed = normalize_whitespace(c.copyright.trim());
        if !trimmed
            .get(.."Copyright".len())
            .is_some_and(|p| p.eq_ignore_ascii_case("Copyright"))
        {
            return true;
        }
        !tails.contains(&(c.start_line.get(), c.end_line.get(), trimmed))
    });
}

pub fn drop_shadowed_copyright_c_years_only_prefixes(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    static COPY_C_YEARS_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright\s*\(c\)\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*\s*$",
        )
        .unwrap()
    });

    let normalized: Vec<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| {
            (
                c.start_line.get(),
                c.end_line.get(),
                normalize_whitespace(c.copyright.trim()),
            )
        })
        .collect();

    let mut shadowed: HashSet<(usize, usize, String)> = HashSet::new();
    for (sln, eln, short) in &normalized {
        if !COPY_C_YEARS_ONLY_RE.is_match(short) {
            continue;
        }
        for (sln2, eln2, long) in &normalized {
            if sln2 != sln || eln2 != eln {
                continue;
            }
            if long.len() <= short.len() {
                continue;
            }
            if long.starts_with(short) {
                let tail = long[short.len()..].trim_start();
                if !tail.is_empty() {
                    shadowed.insert((*sln, *eln, short.clone()));
                    break;
                }
            }
        }
    }

    if shadowed.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        let key = (
            c.start_line.get(),
            c.end_line.get(),
            normalize_whitespace(c.copyright.trim()),
        );
        !shadowed.contains(&key)
    });
}

pub fn drop_non_copyright_like_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        let s = c.copyright.trim();
        if s.is_empty() {
            return false;
        }
        let lower = s.to_ascii_lowercase();
        (lower.contains("copyright")
            || lower.starts_with("(c)")
            || lower.contains("(c)")
            || lower.contains("copr")
            || lower.contains("holder is"))
            && !lower.contains("associated with software")
            && !lower.contains("api description")
            && !lower.contains("protected or trademarked materials")
            && lower != "(c) rest"
    });
}

pub fn drop_bare_c_shadowed_by_non_copyright_prefixes(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let normalized: Vec<(usize, usize, String, String)> = copyrights
        .iter()
        .map(|c| {
            let norm = normalize_whitespace(c.copyright.trim());
            let lower = norm.to_ascii_lowercase();
            (c.start_line.get(), c.end_line.get(), norm, lower)
        })
        .collect();

    copyrights.retain(|c| {
        let bare = normalize_whitespace(c.copyright.trim());
        let bare_lower = bare.to_ascii_lowercase();
        if !bare_lower.starts_with("(c)") {
            return true;
        }

        for (sln, eln, other, other_lower) in &normalized {
            if *sln != c.start_line.get() || *eln != c.end_line.get() {
                continue;
            }
            if other_lower.starts_with("copyright") || other_lower.starts_with("(c)") {
                continue;
            }
            if other_lower.contains(&bare_lower) && other.len() > bare.len() {
                return false;
            }
        }

        true
    });
}

pub fn drop_shadowed_dashless_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let set: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();
    let mut shadowed: HashSet<String> = HashSet::new();
    for h in &set {
        if h.contains('-') {
            shadowed.insert(normalize_whitespace(&h.replace('-', " ")));
        }
    }

    if shadowed.is_empty() {
        return;
    }

    holders.retain(|h| {
        let norm = normalize_whitespace(&h.holder);
        !shadowed.contains(&norm) || h.holder.contains('-')
    });
}
