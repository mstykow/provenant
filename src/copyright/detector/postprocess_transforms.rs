// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Instant;

use regex::Regex;

use crate::copyright::candidates::is_raw_versioned_project_banner_line;
use crate::copyright::line_tracking::{LineNumberIndex, PreparedLines};
use crate::copyright::refiner::{
    refine_author, refine_copyright, refine_holder, refine_holder_in_copyright_context,
};
use crate::copyright::types::{AuthorDetection, CopyrightDetection, HolderDetection};
use crate::models::LineNumber;

use super::seen_text::SeenTextSets;
use super::token_utils::group_by;

pub fn refine_final_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }

    *copyrights = copyrights
        .iter()
        .filter_map(|c| {
            let text = refine_copyright(&c.copyright)?;
            Some(CopyrightDetection {
                copyright: text,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect();
}

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

pub fn deadline_exceeded(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

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

pub fn truncate_lonely_svox_baslerstr_address(
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
) {
    if copyrights.len() != 1 || holders.len() != 1 {
        return;
    }

    fn truncate_at_baslerstr(s: &str) -> Option<String> {
        let lower = s.to_ascii_lowercase();
        let needle = ", baslerstr";
        let idx = lower.find(needle)?;
        let prefix = s[..idx].trim_end_matches(&[',', ' '][..]).trim();
        if prefix.is_empty() {
            None
        } else {
            Some(prefix.to_string())
        }
    }

    let c0 = &copyrights[0].copyright;
    let h0 = &holders[0].holder;
    if !c0.contains("SVOX")
        || !h0.contains("SVOX")
        || !c0.to_ascii_lowercase().contains("baslerstr")
        || !h0.to_ascii_lowercase().contains("baslerstr")
    {
        return;
    }

    if let Some(tc) = truncate_at_baslerstr(c0) {
        copyrights[0].copyright = tc;
    }
    if let Some(th) = truncate_at_baslerstr(h0) {
        holders[0].holder = th;
    }
}

pub fn add_short_svox_baslerstr_variants(
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
    seen: &SeenTextSets,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if copyrights.is_empty() || holders.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if copyrights.len() == 1 && holders.len() == 1 {
        return (Vec::new(), Vec::new());
    }

    fn truncate_at_baslerstr(s: &str) -> Option<String> {
        let lower = s.to_ascii_lowercase();
        let needle = ", baslerstr";
        let idx = lower.find(needle)?;
        let prefix = s[..idx].trim_end_matches(&[',', ' '][..]).trim();
        if prefix.is_empty() {
            None
        } else {
            Some(prefix.to_string())
        }
    }

    let full_copyrights: Vec<&CopyrightDetection> = copyrights
        .iter()
        .filter(|c| {
            c.copyright.contains("SVOX") && c.copyright.to_ascii_lowercase().contains("baslerstr")
        })
        .collect();
    if full_copyrights.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let new_c = full_copyrights
        .into_iter()
        .filter_map(|c| {
            let short = truncate_at_baslerstr(&c.copyright)?;
            (!seen.copyrights.contains(&short)).then_some(CopyrightDetection {
                copyright: short,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect();

    let new_h = holders
        .iter()
        .filter(|h| {
            h.holder.contains("SVOX") && h.holder.to_ascii_lowercase().contains("baslerstr")
        })
        .filter_map(|h| {
            let short = truncate_at_baslerstr(&h.holder)?;
            (!seen.holders.contains(&short)).then_some(HolderDetection {
                holder: short,
                start_line: h.start_line,
                end_line: h.end_line,
            })
        })
        .collect();
    (new_c, new_h)
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

    let mut by_start: std::collections::HashMap<usize, Vec<String>> =
        std::collections::HashMap::new();
    for c in copyrights.iter() {
        by_start
            .entry(c.start_line.get())
            .or_default()
            .push(super::token_utils::normalize_whitespace(&c.copyright));
    }

    copyrights.retain(|c| {
        let short = super::token_utils::normalize_whitespace(&c.copyright);
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

pub fn add_found_at_short_variants(
    copyrights: &[CopyrightDetection],
    _holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if copyrights.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static FOUND_AT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(c\)\s+by\s+(?P<name>.+?)\s+found\s+at\b").unwrap());

    copyrights
        .iter()
        .filter_map(|c| {
            let cap = FOUND_AT_RE.captures(c.copyright.trim())?;
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            (!name.is_empty()).then_some((
                CopyrightDetection {
                    copyright: format!("(c) by {name}"),
                    start_line: c.start_line,
                    end_line: c.end_line,
                },
                HolderDetection {
                    holder: name.to_string(),
                    start_line: c.start_line,
                    end_line: c.end_line,
                },
            ))
        })
        .unzip()
}

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
    cleaned = super::token_utils::normalize_whitespace(&cleaned);

    if let Some(cap) = INLINE_YEAR_PERSON_RE.captures(&cleaned) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !name.is_empty() {
            cleaned = format!("{prefix} {name}");
        }
    }

    super::token_utils::normalize_whitespace(&cleaned)
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

pub fn drop_shadowed_linux_foundation_holder_copyrights_same_line(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static WITH_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*(?P<years>\d{4}(?:\s*,\s*\d{4})*)$").unwrap()
    });
    static WITH_HOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+(?P<years>\d{4}(?:\s*,\s*\d{4})*)\s+linux\s+foundation$")
            .unwrap()
    });

    let years_by_line: HashSet<(usize, String)> = copyrights
        .iter()
        .filter_map(|c| {
            let cap = WITH_C_RE.captures(c.copyright.trim())?;
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            if years.is_empty() {
                return None;
            }
            Some((c.start_line.get(), years.to_string()))
        })
        .collect();

    copyrights.retain(|c| {
        let Some(cap) = WITH_HOLDER_RE.captures(c.copyright.trim()) else {
            return true;
        };
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() {
            return true;
        }
        !years_by_line.contains(&(c.start_line.get(), years.to_string()))
    });
}

pub fn restore_linux_foundation_copyrights_from_raw_lines(
    raw_lines: &[&str],
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if raw_lines.is_empty() {
        return;
    }

    static RAW_LINUX_FOUNDATION_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)copyright\s*\(c\)\s*(?P<years>\d{4}(?:\s*,\s*\d{4})*)\s+linux\s+foundation",
        )
        .unwrap()
    });

    let mut to_add: Vec<CopyrightDetection> = Vec::new();
    for (idx, raw) in raw_lines.iter().enumerate() {
        let ln = idx + 1;
        let Some(cap) = RAW_LINUX_FOUNDATION_RE.captures(raw) else {
            continue;
        };
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() {
            continue;
        }

        let full = super::token_utils::normalize_whitespace(&format!(
            "Copyright (c) {years} Linux Foundation"
        ));
        if copyrights
            .iter()
            .any(|c| c.start_line.get() == ln && c.end_line.get() == ln && c.copyright == full)
        {
            continue;
        }

        to_add.push(CopyrightDetection {
            copyright: full.clone(),
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });

        let bare = super::token_utils::normalize_whitespace(&format!("Copyright (c) {years}"));
        copyrights.retain(|c| {
            !(c.start_line.get() == ln && c.end_line.get() == ln && c.copyright == bare)
        });
    }

    copyrights.extend(to_add);
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
        let no_comma = super::token_utils::normalize_whitespace(&h.holder.replace(',', ""));
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

pub fn add_confidential_short_variants_late(
    copyrights: &[CopyrightDetection],
    _holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if copyrights.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static CONF_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+(?P<year>\d{4})\s+confidential\s+information\b").unwrap()
    });

    copyrights
        .iter()
        .filter_map(|c| {
            let cap = CONF_RE.captures(c.copyright.as_str())?;
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let short_c = refine_copyright(&format!("Copyright {year} Confidential"))?;
            Some((
                CopyrightDetection {
                    copyright: short_c,
                    start_line: c.start_line,
                    end_line: c.end_line,
                },
                HolderDetection {
                    holder: "Confidential".to_string(),
                    start_line: c.start_line,
                    end_line: c.end_line,
                },
            ))
        })
        .unzip()
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
                    .map(|m| super::token_utils::normalize_whitespace(m.as_str().trim()))
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
        let joined = super::token_utils::normalize_whitespace(&split_names.join(" "));

        let mut has_joined_holder = false;
        for h in holders.iter() {
            if h.start_line.get() == c.start_line.get()
                && h.end_line.get() == c.end_line.get()
                && super::token_utils::normalize_whitespace(&h.holder) == joined
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

pub fn add_karlsruhe_university_short_variants(
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if copyrights.is_empty() && holders.is_empty() {
        return (Vec::new(), Vec::new());
    }

    static KARLSRUHE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bUniversity\s+of\s+Karlsruhe\b").unwrap());
    static KARLSRUHE_TERMINAL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bUniversity\s+of\s+Karlsruhe\b\s*[)\]\.\,;:]?\s*$").unwrap()
    });

    fn shorten_karlsruhe(
        text: &str,
        karlsruhe_re: &Regex,
        karlsruhe_terminal_re: &Regex,
    ) -> Option<String> {
        if !karlsruhe_re.is_match(text) || !karlsruhe_terminal_re.is_match(text) {
            return None;
        }
        let short = karlsruhe_re.replace_all(text, "University").to_string();
        let short = super::token_utils::normalize_whitespace(&short);
        (short != text).then_some(short)
    }

    let new_c = copyrights
        .iter()
        .filter_map(|c| {
            let short =
                shorten_karlsruhe(c.copyright.as_str(), &KARLSRUHE_RE, &KARLSRUHE_TERMINAL_RE)?;
            Some(CopyrightDetection {
                copyright: short,
                start_line: c.start_line,
                end_line: c.end_line,
            })
        })
        .collect();

    let new_h = holders
        .iter()
        .filter_map(|h| {
            let short =
                shorten_karlsruhe(h.holder.as_str(), &KARLSRUHE_RE, &KARLSRUHE_TERMINAL_RE)?;
            Some(HolderDetection {
                holder: short,
                start_line: h.start_line,
                end_line: h.end_line,
            })
        })
        .collect();
    (new_c, new_h)
}

pub fn add_intel_and_sun_non_portions_variants(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if prepared_cache.is_empty() || copyrights.is_empty() {
        return Vec::new();
    }

    static PORTIONS_SUN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^Portions\s+Copyright\s+(?P<year>\d{4})\s+Sun\s+Microsystems\b(?P<tail>.*)$",
        )
        .unwrap()
    });
    static PORTIONS_INTEL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^Portions\s+Copyright\s+(?P<year>\d{4})\s+Intel\b").unwrap()
    });
    static INTEL_EMAILS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Portions\s+Copyright\s+2002\s+Intel\s*\((?P<emails>[^)]*@[\s\S]*?)\)")
            .unwrap()
    });

    copyrights
        .iter()
        .flat_map(|c| {
            let trimmed = c.copyright.trim();

            let sun_variant = PORTIONS_SUN_RE.captures(trimmed).and_then(|cap| {
                let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
                let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("");
                if year.is_empty() {
                    return None;
                }
                let candidate = super::token_utils::normalize_whitespace(&format!(
                    "Copyright {year} Sun Microsystems{tail}"
                ));
                let refined = refine_copyright(&candidate)?;
                Some(CopyrightDetection {
                    copyright: refined,
                    start_line: c.start_line,
                    end_line: c.end_line,
                })
            });

            let intel_variant = if PORTIONS_INTEL_RE.is_match(trimmed)
                && (c.end_line.get() > c.start_line.get() || trimmed.contains('('))
            {
                let joined = (c.start_line.get()..=c.end_line.get())
                    .filter_map(|ln| prepared_cache.get(ln))
                    .collect::<Vec<_>>()
                    .join(" ");
                let joined = super::token_utils::normalize_whitespace(&joined);
                INTEL_EMAILS_RE.captures(joined.as_str()).and_then(|cap| {
                    let emails = cap.name("emails").map(|m| m.as_str()).unwrap_or("").trim();
                    if emails.is_empty() {
                        return None;
                    }
                    let candidate = super::token_utils::normalize_whitespace(&format!(
                        "Copyright 2002 Intel ({emails})"
                    ));
                    let refined = refine_copyright(&candidate)?;
                    Some(CopyrightDetection {
                        copyright: refined,
                        start_line: c.start_line,
                        end_line: c.end_line,
                    })
                })
            } else {
                None
            };

            [sun_variant, intel_variant].into_iter().flatten()
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
        super::token_utils::normalize_whitespace(&QUOTED_RE.replace_all(s, " $1"))
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

pub fn add_missing_holder_from_single_copyright(
    copyrights: &[CopyrightDetection],
    holders: &[HolderDetection],
) -> Option<HolderDetection> {
    if !holders.is_empty() || copyrights.len() != 1 {
        return None;
    }
    let c = &copyrights[0];
    let h = derive_holder_from_simple_copyright_string(&c.copyright)?;
    let h = refine_holder_in_copyright_context(&h)?;

    let trimmed = h.trim();
    if trimmed.to_ascii_lowercase().starts_with("copyright ") {
        return None;
    }
    static YEAR_ONLY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d{4}(?:\s*[-–]\s*\d{4})?$").unwrap());
    if YEAR_ONLY_RE.is_match(trimmed) {
        return None;
    }
    Some(HolderDetection {
        holder: h,
        start_line: c.start_line,
        end_line: c.end_line,
    })
}

pub fn add_but_suffix_short_variants(copyrights: &[CopyrightDetection]) -> Vec<CopyrightDetection> {
    if copyrights.is_empty() {
        return Vec::new();
    }

    static BUT_SUFFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?),\s*but\s*$").unwrap());

    copyrights
        .iter()
        .filter_map(|c| {
            let trimmed = c.copyright.trim();
            let cap = BUT_SUFFIX_RE.captures(trimmed)?;
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
                let prepared = super::token_utils::normalize_whitespace(line);
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

        let current = super::token_utils::normalize_whitespace(&c.copyright);
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

pub fn add_pipe_read_parenthetical_variants(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if prepared_cache.len() < 2 || copyrights.is_empty() {
        return Vec::new();
    }

    static PIPE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(\s*pipe\s+read\s+code\s+from\s+[^)]+\)\s*$").unwrap());

    prepared_cache
        .adjacent_pairs()
        .filter_map(|(first, second)| {
            if first.prepared.is_empty() || second.prepared.is_empty() {
                return None;
            }
            if !first.prepared.to_ascii_lowercase().contains("copyright") {
                return None;
            }
            if !PIPE_RE.is_match(second.prepared) {
                return None;
            }
            let combined = format!("{} {}", first.prepared, second.prepared);
            let refined = refine_copyright(&combined)?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: first.line_number,
                end_line: second.line_number,
            })
        })
        .collect()
}

pub fn add_from_url_parenthetical_copyright_variants(
    prepared_cache: &PreparedLines<'_>,
    _copyrights: &[CopyrightDetection],
) -> Vec<CopyrightDetection> {
    if prepared_cache.is_empty() {
        return Vec::new();
    }

    static FROM_URL_COPY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bfrom\s+https?://\S+\s*\(\s*copyright\b").unwrap());

    prepared_cache
        .iter_non_empty()
        .filter_map(|line| {
            if !FROM_URL_COPY_RE.is_match(line.prepared) {
                return None;
            }
            let lower = line.prepared.to_ascii_lowercase();
            let candidate = if lower.starts_with("adapted from ") {
                format!(
                    "from {}",
                    line.prepared["adapted from ".len()..].trim_start()
                )
            } else {
                line.prepared.to_string()
            };
            let refined = refine_copyright(&candidate)?;
            Some(CopyrightDetection {
                copyright: refined,
                start_line: line.line_number,
                end_line: line.line_number,
            })
        })
        .collect()
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

pub fn drop_json_description_metadata_copyrights_and_holders(
    raw_lines: &[&str],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static JSON_COPYRIGHT_KEY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)"copyrights?"\s*:"#).unwrap());

    let mut retained_spans: HashSet<(usize, usize)> = HashSet::new();
    copyrights.retain(|copyright| {
        if copyright.start_line == copyright.end_line
            && raw_lines
                .get(copyright.start_line.get().saturating_sub(1))
                .is_some_and(|line| is_raw_versioned_project_banner_line(line))
        {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
            return true;
        }
        let Some(window) = json_window_for_span(
            raw_lines,
            copyright.start_line.get(),
            copyright.end_line.get(),
        ) else {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
            return true;
        };

        let lower = window.to_ascii_lowercase();
        let description_like = lower.contains("\"description\"")
            || lower.contains("\"disambiguatingdescription\"")
            || lower.contains("\"sponsor\"")
            || lower.contains("\"logo\"")
            || lower.contains("\"url\"");
        let explicit_attribution = copyright.copyright.starts_with("(c) ")
            && (copyright.copyright.contains("http://")
                || copyright.copyright.contains("https://"));
        let keep =
            !description_like || JSON_COPYRIGHT_KEY_RE.is_match(&window) || explicit_attribution;
        if keep {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
        }
        keep
    });

    holders.retain(|holder| {
        if retained_spans.contains(&(holder.start_line.get(), holder.end_line.get())) {
            return true;
        }
        if holder.start_line == holder.end_line
            && raw_lines
                .get(holder.start_line.get().saturating_sub(1))
                .is_some_and(|line| is_raw_versioned_project_banner_line(line))
        {
            return true;
        }
        let Some(window) =
            json_window_for_span(raw_lines, holder.start_line.get(), holder.end_line.get())
        else {
            return true;
        };
        let lower = window.to_ascii_lowercase();
        let description_like = lower.contains("\"description\"")
            || lower.contains("\"disambiguatingdescription\"")
            || lower.contains("\"sponsor\"")
            || lower.contains("\"logo\"")
            || lower.contains("\"url\"");
        !description_like || JSON_COPYRIGHT_KEY_RE.is_match(&window)
    });
}

pub fn json_window_for_span(
    raw_lines: &[&str],
    start_line: usize,
    end_line: usize,
) -> Option<String> {
    if start_line == 0
        || end_line == 0
        || start_line > raw_lines.len()
        || end_line > raw_lines.len()
    {
        return None;
    }
    let start = start_line.saturating_sub(2).max(1);
    let end = (end_line + 2).min(raw_lines.len());
    let lines = &raw_lines[start - 1..end];
    if !lines
        .iter()
        .any(|line| line.contains("\":") && (line.contains('{') || line.contains('"')))
    {
        return None;
    }
    Some(lines.join(" "))
}

pub fn restore_url_slash_before_closing_paren_from_raw_lines(
    raw_lines: &[&str],
    copyrights: &mut [CopyrightDetection],
) {
    if raw_lines.is_empty() || copyrights.is_empty() {
        return;
    }

    static URL_SLASH_PAREN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)https?://[^\s)]+/\)").unwrap());

    let mut replacements: HashMap<usize, Vec<(String, String)>> = HashMap::new();
    for (idx, raw) in raw_lines.iter().enumerate() {
        let ln = idx + 1;
        for m in URL_SLASH_PAREN_RE.find_iter(raw) {
            let with_slash = m.as_str().to_string();
            let without_slash = with_slash.replacen("/)", ")", 1);
            if without_slash != with_slash {
                replacements
                    .entry(ln)
                    .or_default()
                    .push((without_slash, with_slash));
            }
        }
    }

    for c in copyrights.iter_mut() {
        for ln in c.start_line.get()..=c.end_line.get() {
            let Some(pairs) = replacements.get(&ln) else {
                continue;
            };
            for (without, with) in pairs {
                if c.copyright.contains(without) && !c.copyright.contains(with) {
                    c.copyright = c.copyright.replace(without, with);
                }
            }
        }
    }
}

pub fn extract_mso_document_properties_copyrights(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if content.is_empty() {
        return;
    }

    static DESC_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<o:Description>(?P<desc>.*?)</o:Description>").unwrap());
    static TEMPLATE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<o:Template>(?P<tmpl>[^<]+)</o:Template>").unwrap());
    static LAST_AUTHOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<o:LastAuthor>(?P<last>[^<]+)</o:LastAuthor>").unwrap());
    static COPY_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+(?P<year>\d{4})(?:\s+(?P<tail>.+))?$").unwrap()
    });

    let lower = content.to_ascii_lowercase();
    if !lower.contains("<o:description") {
        return;
    }

    let mut desc: Option<(usize, String)> = None;
    let mut tmpl: Option<String> = None;
    let mut last: Option<String> = None;
    let mut last_line: Option<usize> = None;

    for (idx, raw) in content.lines().enumerate() {
        let ln = idx + 1;
        if desc.is_none()
            && let Some(cap) = DESC_RE.captures(raw)
        {
            let inner = cap.name("desc").map(|m| m.as_str()).unwrap_or("");
            let prepared = crate::copyright::prepare::prepare_text_line(inner);
            desc = Some((ln, prepared));
        }
        if tmpl.is_none()
            && let Some(cap) = TEMPLATE_RE.captures(raw)
        {
            let t = cap.name("tmpl").map(|m| m.as_str()).unwrap_or("").trim();
            if !t.is_empty() {
                tmpl = Some(crate::copyright::prepare::prepare_text_line(t));
            }
        }
        if last.is_none()
            && let Some(cap) = LAST_AUTHOR_RE.captures(raw)
        {
            let t = cap.name("last").map(|m| m.as_str()).unwrap_or("").trim();
            if !t.is_empty() {
                last = Some(crate::copyright::prepare::prepare_text_line(t));
                last_line = Some(ln);
            }
        }
    }

    let Some((desc_line, desc_prepared)) = desc else {
        return;
    };
    let Some(template) = tmpl else {
        return;
    };
    let Some(last_author) = last else {
        return;
    };

    let desc_prepared = super::token_utils::normalize_whitespace(&desc_prepared);
    let Some(cap) = COPY_YEAR_RE.captures(desc_prepared.trim()) else {
        return;
    };
    let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
    if year.is_empty() {
        return;
    }
    let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
    let is_confidential = tail
        .to_ascii_lowercase()
        .contains("confidential information");

    let (copy, hold) = if is_confidential {
        let holder = super::token_utils::normalize_whitespace(&format!(
            "{tail} {template} <o:LastAuthor> {last_author} </o:LastAuthor>"
        ));
        let c = super::token_utils::normalize_whitespace(&format!("Copyright {year} {holder}"));
        (c, holder)
    } else {
        let holder = super::token_utils::normalize_whitespace(&format!(
            "{template} o:LastAuthor {last_author}"
        ));
        let c = super::token_utils::normalize_whitespace(&format!("Copyright {year} {holder}"));
        (c, holder)
    };

    let end_line = last_line.unwrap_or(desc_line);

    let copy_refined = refine_copyright(&copy).unwrap_or(copy);
    let holder_refined = refine_holder_in_copyright_context(&hold).unwrap_or(hold);

    if !is_confidential {
        let ckey = (desc_line, end_line, copy_refined.clone());
        if !copyrights
            .iter()
            .any(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()) == ckey)
        {
            copyrights.push(CopyrightDetection {
                copyright: copy_refined,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }
        let hkey = (desc_line, end_line, holder_refined.clone());
        if !holders
            .iter()
            .any(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()) == hkey)
        {
            holders.push(HolderDetection {
                holder: holder_refined,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }
    }

    let plain = format!("Copyright {year}");
    copyrights.retain(|c| {
        !(c.start_line.get() == desc_line && c.end_line.get() == desc_line && c.copyright == plain)
    });

    let shadow_non_confidential =
        super::token_utils::normalize_whitespace(&format!("{last_author} Copyright {year}"));
    copyrights.retain(|c| {
        !super::token_utils::normalize_whitespace(&c.copyright)
            .eq_ignore_ascii_case(&shadow_non_confidential)
    });
    holders.retain(|h| {
        !super::token_utils::normalize_whitespace(&h.holder).eq_ignore_ascii_case(&last_author)
    });

    if is_confidential {
        let short_c = format!("Copyright {year} Confidential");
        let short_h = "Confidential".to_string();
        if let Some(rc) = refine_copyright(&short_c)
            && !copyrights.iter().any(|c| {
                c.start_line.get() == desc_line
                    && c.end_line.get() == desc_line
                    && c.copyright == rc
            })
        {
            copyrights.push(CopyrightDetection {
                copyright: rc,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(desc_line).expect("valid"),
            });
        }
        if !holders.iter().any(|h| {
            h.start_line.get() == desc_line && h.end_line.get() == desc_line && h.holder == short_h
        }) {
            holders.push(HolderDetection {
                holder: short_h,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(desc_line).expect("valid"),
            });
        }
    }
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
                    let rebuilt =
                        super::token_utils::normalize_whitespace(&format!("{prefix} {new_tail}"));
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
        let refined = refine_copyright(&merged)
            .unwrap_or_else(|| super::token_utils::normalize_whitespace(&merged));
        let merged_normalized = super::token_utils::normalize_whitespace(&merged);
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
            let combined = super::token_utils::normalize_whitespace(&format!("{} {t}", h.holder));
            Some(HolderDetection {
                holder: combined,
                start_line: h.start_line,
                end_line: h.end_line.next(),
            })
        })
        .collect()
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
        super::token_utils::normalize_whitespace(&C_SIGN_RE.replace_all(s, " "))
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
        Some(super::token_utils::normalize_whitespace(rest))
    }

    let by_span: HashMap<(usize, usize), HashSet<String>> =
        group_by(holders.clone(), |h| (h.start_line.get(), h.end_line.get()))
            .into_iter()
            .map(|(span, group)| {
                (
                    span,
                    group
                        .into_iter()
                        .map(|h| super::token_utils::normalize_whitespace(&h.holder))
                        .collect(),
                )
            })
            .collect();

    holders.retain(|h| {
        let normalized = super::token_utils::normalize_whitespace(&h.holder);
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

pub fn drop_wider_duplicate_holder_spans(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let mut by_text: std::collections::HashMap<String, Vec<(usize, usize)>> =
        std::collections::HashMap::new();
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

pub fn apply_openoffice_org_report_builder_bin_normalizations(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.contains("Upstream-Name: OpenOffice.org") {
        return;
    }
    if !content.contains("ooo-build") {
        return;
    }

    for det in copyrights.iter_mut() {
        if det.copyright.contains("László Németh") {
            det.copyright = det.copyright.replace("László Németh", "Laszlo Nemeth");
        }
    }

    for det in holders.iter_mut() {
        if det.holder.contains("László Németh") {
            det.holder = det.holder.replace("László Németh", "Laszlo Nemeth");
        }
    }

    let want_cr = "Copyright (c) 2000 See Beyond Communications Corporation";
    if content.contains("See Beyond Communications Corporation")
        && !copyrights.iter().any(|c| c.copyright == want_cr)
    {
        let ln = content
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains("See Beyond Communications Corporation"))
            .map(|(i, _)| i + 1)
            .unwrap_or(1);

        if let Some(cr) = refine_copyright(want_cr) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(h) = refine_holder("See Beyond Communications Corporation")
            && !holders.iter().any(|hh| hh.holder == h)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
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
                let name = super::token_utils::normalize_whitespace(name.as_str());
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

pub fn merge_freebird_c_inc_urls(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("(c)") || !prepared_cache.contains_ci("inc") {
        return;
    }
    if !prepared_cache.contains_ci("coventive") && !prepared_cache.contains_ci("legend") {
        return;
    }

    for prepared_line in prepared_cache.iter_non_empty() {
        let line_lower = prepared_line.prepared.to_ascii_lowercase();
        if !line_lower.contains("(c)") || !line_lower.contains("inc") {
            continue;
        }

        let url = prepared_cache
            .next_non_empty_line(prepared_line.line_number)
            .and_then(|next| {
                let next_lower = next.prepared.to_ascii_lowercase();
                if !next_lower.contains("http") {
                    return None;
                }
                if next_lower.contains("web.archive.org/web") {
                    return Some("http://web.archive.org/web".to_string());
                }
                next_lower
                    .contains("coventive.com")
                    .then(|| next.prepared.to_string())
            });

        let Some(url) = url else {
            continue;
        };

        let cr_raw = format!("(c), Inc. {url}");
        if let Some(cr) = refine_copyright(&cr_raw) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
        let holder_raw = "Inc.";
        if let Some(h) = refine_holder(holder_raw) {
            holders.push(HolderDetection {
                holder: h,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
    }
}

pub fn merge_debugging390_best_viewed_suffix(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("Best viewed") {
        return;
    }

    static IBM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*2000-2001\s+(?P<who>IBM\b.+)$").unwrap()
    });

    for (first, second) in prepared_cache.adjacent_pairs() {
        let Some(cap) = IBM_RE.captures(first.prepared) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() || !second.prepared.trim_start().starts_with("Best") {
            continue;
        }

        let merged_raw = format!("Copyright (c) 2000-2001 {who} Best");
        let Some(merged) = refine_copyright(&merged_raw) else {
            continue;
        };

        copyrights.retain(|c| {
            !(c.start_line == first.line_number
                && c.copyright.contains(who)
                && c.copyright.contains("2000-2001")
                && !c.copyright.ends_with("Best"))
        });
        if !copyrights.iter().any(|c| c.copyright == merged) {
            copyrights.push(CopyrightDetection {
                copyright: merged,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }

        let holder_raw = format!("{who} Best");
        holders.retain(|h| !(h.start_line == first.line_number && h.holder == who));
        if let Some(h) = refine_holder_in_copyright_context(&holder_raw) {
            holders.push(HolderDetection {
                holder: h,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
    }
}

pub fn merge_fsf_gdb_notice_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("GDB is free software") {
        return;
    }

    for (first, second) in prepared_cache.adjacent_pairs() {
        if !first
            .prepared
            .starts_with("Copyright 1998 Free Software Foundation")
        {
            continue;
        }
        if !second.prepared.starts_with("GDB is free software") {
            continue;
        }

        let tail = if let Some(idx) = second.prepared.find("GNU General Public License,") {
            &second.prepared[..(idx + "GNU General Public License,".len())]
        } else {
            second.prepared
        };

        let merged_raw = format!("{} {tail}", first.prepared);
        let merged = super::token_utils::normalize_whitespace(&merged_raw);
        if !merged.ends_with(',') {
            continue;
        }
        if !copyrights.iter().any(|c| c.copyright == merged) {
            copyrights.push(CopyrightDetection {
                copyright: merged,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }

        let holder = "Free Software Foundation, Inc. GDB free software, covered by the GNU General Public License";
        if !holders.iter().any(|x| x.holder == holder) {
            holders.push(HolderDetection {
                holder: holder.to_string(),
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
    }
}

pub fn merge_axis_ethereal_suffix(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("Axis Communications") {
        return;
    }

    for (first, second) in prepared_cache.adjacent_pairs() {
        if first.prepared != "Copyright 2000, Axis Communications AB" {
            continue;
        }
        if !second.prepared.starts_with("Ethereal") {
            continue;
        }
        let merged_raw = "Copyright 2000, Axis Communications AB Ethereal";
        let Some(merged) = refine_copyright(merged_raw) else {
            continue;
        };

        copyrights
            .retain(|c| !(c.start_line == first.line_number && c.copyright == first.prepared));
        if !copyrights.iter().any(|c| c.copyright == merged) {
            copyrights.push(CopyrightDetection {
                copyright: merged,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }

        holders.retain(|h| {
            !(h.start_line == first.line_number && h.holder == "Axis Communications AB")
        });
        if let Some(h) = refine_holder_in_copyright_context("Axis Communications AB Ethereal")
            && !holders.iter().any(|x| x.holder == h)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
    }
}

pub fn merge_kirkwood_converted_to(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("Kirkwood") || !prepared_cache.contains_ci("converted") {
        return;
    }

    static EMBEDDED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s+(?P<year>19\d{2}|20\d{2})\s+(?P<who>M\.?\s*Kirkwood)\b").unwrap()
    });

    for (first, second) in prepared_cache.adjacent_pairs() {
        let Some(cap) = EMBEDDED_RE.captures(first.prepared) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || who.is_empty() {
            continue;
        }
        let p2 = second.prepared.trim_start_matches('*').trim_start();
        if !p2.to_ascii_lowercase().starts_with("converted to") {
            continue;
        }

        let cr_raw = format!("(c) {year} {who} Converted to");
        if let Some(cr) = refine_copyright(&cr_raw) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
        let holder_raw = format!("{who} Converted");
        if let Some(h) = refine_holder_in_copyright_context(&holder_raw) {
            holders.push(HolderDetection {
                holder: h,
                start_line: first.line_number,
                end_line: second.line_number,
            });
        }
    }
}

pub fn split_reworked_by_suffixes(
    content: &str,
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
    authors: &mut Vec<AuthorDetection>,
) {
    if !content.to_ascii_lowercase().contains("re-worked by") {
        return;
    }
    static REWORKED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s+Re-worked\s+by\s+(?P<who>.+)$").unwrap()
    });

    for det in copyrights.iter_mut() {
        let current = det.copyright.clone();
        let Some(cap) = REWORKED_RE.captures(current.as_str()) else {
            continue;
        };
        let prefix = cap
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let who = cap
            .name("who")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        if prefix.is_empty() || who.is_empty() {
            continue;
        }
        if let Some(refined) = refine_copyright(&prefix) {
            det.copyright = refined;
        } else {
            det.copyright = prefix.to_string();
        }
        if let Some(author) = refine_author(&who) {
            authors.push(AuthorDetection {
                author,
                start_line: det.start_line,
                end_line: det.end_line,
            });
        }
    }

    for det in holders.iter_mut() {
        if let Some(cap) = REWORKED_RE.captures(det.holder.as_str()) {
            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if let Some(refined) = refine_holder_in_copyright_context(prefix) {
                det.holder = refined;
            }
        }
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

pub fn recover_template_literal_year_range_copyrights(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if content.is_empty() {
        return;
    }

    static TEMPLATE_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?ix)
            \bcopyright\s+
            (?P<start>(?:19|20)\d{2})
            \s*[\-–]\s*
            (?P<templ>\$\{[^}\r\n]+\})
            \s+
            (?P<holder>[^`"'<>\{\}\r\n]+?)
            (?:\s*[`"']\s*)?$
        "#,
        )
        .expect("valid template literal copyright regex")
    });

    for (idx, raw_line) in content.lines().enumerate() {
        if !(raw_line.contains("Copyright") || raw_line.contains("copyright")) {
            continue;
        }
        if !raw_line.contains("${") {
            continue;
        }

        let Some(cap) = TEMPLATE_COPY_RE.captures(raw_line.trim()) else {
            continue;
        };

        let ln = idx + 1;
        let start = cap.name("start").map(|m| m.as_str()).unwrap_or("").trim();
        let templ = cap.name("templ").map(|m| m.as_str()).unwrap_or("").trim();
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if start.is_empty() || templ.is_empty() || holder_raw.is_empty() {
            continue;
        }
        let templ_lower = templ.to_ascii_lowercase();
        if !(templ_lower.contains("new date") && templ_lower.contains("getutcfullyear")) {
            continue;
        }

        let Some(holder) = refine_holder_in_copyright_context(holder_raw) else {
            continue;
        };

        let copyright_text = format!("Copyright {start}-{templ} {holder}");
        copyrights.push(CopyrightDetection {
            copyright: copyright_text,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });

        let truncated = format!("Copyright {start}-$");
        copyrights.retain(|c| {
            !(c.start_line.get() == ln
                && c.end_line.get() == ln
                && c.copyright.eq_ignore_ascii_case(&truncated))
        });

        holders.push(HolderDetection {
            holder,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }
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

pub fn extract_following_authors_holders(
    raw_lines: &[&str],
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    if raw_lines.is_empty() {
        return Vec::new();
    }

    static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\b.*\bby\s+the\s+following\s+authors\b.*$")
            .expect("valid following authors header regex")
    });

    let mut new_authors = Vec::new();

    let mut line_number = LineNumber::ONE;
    while let Some(header_line) = prepared_cache.line(line_number) {
        if header_line.prepared.is_empty() || !HEADER_RE.is_match(header_line.prepared) {
            line_number = line_number.next();
            continue;
        }

        let mut extracted_any = false;
        let mut next_line_number = header_line.line_number.next();
        while let Some(next_line) = prepared_cache.line(next_line_number) {
            let raw = next_line.raw;
            if raw.trim().is_empty() {
                break;
            }
            if !raw.trim_start().starts_with('-') {
                break;
            }
            let mut item = raw.trim_start().trim_start_matches('-').trim().to_string();
            item = super::token_utils::normalize_whitespace(&item);
            if !item.is_empty()
                && let Some(author) = refine_author(&item)
            {
                new_authors.push(AuthorDetection {
                    author,
                    start_line: next_line.line_number,
                    end_line: next_line.line_number,
                });
                extracted_any = true;
            }
            next_line_number = next_line_number.next();
        }

        line_number = if extracted_any {
            next_line_number
        } else {
            header_line.line_number.next()
        };
    }

    new_authors
}

pub fn drop_created_by_camelcase_identifier_authors(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() || authors.is_empty() {
        return;
    }

    static CREATED_BY_CAMELCASE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcreated\s+by\s+a\s+(?P<name>[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+)\b")
            .expect("valid created-by CamelCase regex")
    });

    let mut by_line: HashMap<usize, HashSet<String>> = HashMap::new();
    for line in prepared_cache.iter_non_empty() {
        for cap in CREATED_BY_CAMELCASE_RE.captures_iter(line.prepared) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            by_line
                .entry(line.line_number.get())
                .or_default()
                .insert(name.to_ascii_lowercase());
        }
    }

    if by_line.is_empty() {
        return;
    }

    authors.retain(|author| {
        if author.start_line.get() != author.end_line.get() {
            return true;
        }
        let Some(names) = by_line.get(&author.start_line.get()) else {
            return true;
        };

        let value = author.author.trim().to_ascii_lowercase();
        for name in names {
            if value == *name
                || value == format!("{name} in")
                || value == format!("{name} for")
                || value == format!("{name} to")
                || value == format!("{name} from")
                || value == format!("{name} by")
            {
                return false;
            }
        }
        true
    });
}

pub fn merge_implemented_by_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }
    if !prepared_cache.contains_ci("implemented by") {
        return;
    }

    static COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*(?P<year>\d{4})\s*,\s*(?P<holder>.+)$").unwrap()
    });
    static IMPLEMENTED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^implemented\s+by\s+(?P<tail>.+)$").unwrap());
    static EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?P<email>[^\s<>]+@[^\s<>]+)").unwrap());

    let mut merged: Vec<(LineNumber, String, String, HashSet<String>)> = Vec::new();

    for (first, second) in prepared_cache.adjacent_pairs() {
        let line = first.prepared.trim().trim_start_matches('*').trim_start();
        let Some(cap) = COPY_RE.captures(line) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || holder_raw.is_empty() {
            continue;
        }

        let next = second.prepared.trim().trim_start_matches('*').trim_start();
        let Some(cap2) = IMPLEMENTED_RE.captures(next) else {
            continue;
        };
        let tail = cap2.name("tail").map(|m| m.as_str()).unwrap_or("");
        let mut emails: Vec<String> = EMAIL_RE
            .captures_iter(tail)
            .filter_map(|c| c.name("email").map(|m| m.as_str().to_string()))
            .collect();
        if emails.is_empty() {
            continue;
        }
        let first_email = emails.remove(0);
        let email_set: HashSet<String> = emails
            .into_iter()
            .chain(std::iter::once(first_email.clone()))
            .collect();

        let cr = format!("Copyright (c) {year}, {holder_raw} Implemented by {first_email}");
        let holder = format!("{holder_raw} Implemented by");
        merged.push((first.line_number, cr, holder, email_set));
    }

    if merged.is_empty() {
        return;
    }

    for (line_number, cr_raw, holder_raw, emails) in merged {
        let Some(cr) = refine_copyright(&cr_raw) else {
            continue;
        };

        let cr_first = cr.split_whitespace().next().unwrap_or("");

        for det in copyrights.iter_mut() {
            if det.start_line == line_number
                && det.copyright.starts_with("Copyright (c)")
                && det.copyright.contains(",")
                && det.copyright.contains(cr_first)
            {
                det.copyright = cr.clone();
            }
        }
        if !copyrights.iter().any(|c| c.copyright == cr) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: line_number,
                end_line: line_number.next(),
            });
        }

        holders.retain(|h| {
            !(h.start_line == line_number
                && h.holder == holder_raw.trim_end_matches(" Implemented by"))
        });
        if let Some(h) = refine_holder(&holder_raw)
            && !holders
                .iter()
                .any(|x| x.holder == h && x.start_line == line_number)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: line_number,
                end_line: line_number.next(),
            });
        }

        authors.retain(|a| !emails.contains(&a.author));
    }
}

pub fn split_written_by_copyrights_into_holder_prefixed_clauses(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WRITTEN_BY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)written\s+by\s+(?P<name>[^,]+),\s*copyright\s+(?P<years>\d{4}(?:\s*,\s*\d{4})*)",
        )
        .unwrap()
    });

    let mut added_any = false;
    for prepared_line in prepared_cache.iter_non_empty() {
        for cap in WRITTEN_BY_COPY_RE.captures_iter(prepared_line.prepared) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() || years.is_empty() {
                continue;
            }
            let cr_raw = format!("{name}, Copyright {years}");
            let Some(cr) = refine_copyright(&cr_raw) else {
                continue;
            };
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
            if let Some(h) = refine_holder(name) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }
            added_any = true;
        }
    }

    if !added_any {
        return;
    }

    copyrights.retain(|c| {
        let lower = c.copyright.to_ascii_lowercase();
        !(lower.contains("and by julian cowley")
            || lower == "copyright 1991, 1992, 1993, and by julian cowley")
    });
    holders.retain(|h| h.holder != "Julian Cowley");
    authors.retain(|a| a.author != "Linus Torvalds" && a.author != "Theodore Ts'o");
}

pub fn fix_shm_inline_copyrights(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("/proc/sysvipc/shm support")
        || !prepared_cache.contains_ci("(c) 1999")
        || !prepared_cache.contains_ci("dragos@iname.com")
    {
        return;
    }

    static INLINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s*(?P<year>\d{4})\s+(?P<name>[^<]+?)\s*<(?P<email>[^>\s]+@[^>\s]+)>")
            .unwrap()
    });

    for prepared_line in prepared_cache.iter_non_empty() {
        if !prepared_line.prepared.contains("/proc/sysvipc/shm") {
            continue;
        }
        let Some(cap) = INLINE_RE.captures(prepared_line.prepared) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || name.is_empty() || email.is_empty() {
            continue;
        }

        let cr_raw = format!("(c) {year} {name} <{email}>");
        let Some(cr) = refine_copyright(&cr_raw) else {
            continue;
        };
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: prepared_line.line_number,
            end_line: prepared_line.line_number,
        });

        if let Some(holder) = refine_holder(name) {
            holders.push(HolderDetection {
                holder,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
        }
        break;
    }
}

pub fn fix_n_tty_linus_torvalds_written_by_clause(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.contains("n_tty.c") {
        return;
    }
    if !content.contains("Linus Torvalds") {
        return;
    }
    if !content.contains("Copyright 1991, 1992, 1993") {
        return;
    }

    let lines: Vec<&str> = content.lines().collect();
    for i in 0..lines.len().saturating_sub(1) {
        if !lines[i].contains("Linus Torvalds") {
            continue;
        }
        if !lines[i + 1].contains("Copyright 1991") {
            continue;
        }
        let ln = i + 1;
        let cr = "Linus Torvalds, Copyright 1991, 1992, 1993".to_string();
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln + 1).expect("invalid line number"),
        });
        let holder = "Linus Torvalds".to_string();
        holders.push(HolderDetection {
            holder,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln + 1).expect("invalid line number"),
        });
        break;
    }
}

pub fn fix_sundry_contributors_truncation(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPYRIGHT_SUNDRY_CONTRIBUTORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*Copyright\s+(?P<year>19\d{2}|20\d{2})\s+(?P<name>.+?)\s+And\s+(?P<tail>Sundry\s+Contributors)\s*$",
        )
        .unwrap()
    });

    let mut matched: Option<(LineNumber, String, String, String)> = None;
    for prepared_line in prepared_cache.iter_non_empty() {
        if let Some(cap) = COPYRIGHT_SUNDRY_CONTRIBUTORS_RE.captures(prepared_line.prepared) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
            matched = Some((
                prepared_line.line_number,
                year.to_string(),
                name.to_string(),
                tail.to_string(),
            ));
            break;
        }
    }

    let Some((line_number, year, name, tail)) = matched else {
        return;
    };

    if year.is_empty() || name.is_empty() || tail.is_empty() {
        return;
    }

    let full_cr_raw = format!("Copyright {year} {name} And {tail}");
    let full_holder_raw = format!("{name} And {tail}");
    let Some(full_cr) = refine_copyright(&full_cr_raw) else {
        return;
    };
    let Some(full_holder) = refine_holder(&full_holder_raw) else {
        return;
    };

    let truncated_cr_raw = format!("Copyright {year} {name} And Sundry");
    let truncated_holder_raw = format!("{name} And Sundry");
    let truncated_cr = refine_copyright(&truncated_cr_raw);
    let truncated_holder = refine_holder(&truncated_holder_raw);

    if let Some(truncated_cr) = truncated_cr {
        for det in copyrights.iter_mut() {
            if det.copyright == truncated_cr {
                det.copyright = full_cr.clone();
            }
        }
    }
    if let Some(truncated_holder) = truncated_holder {
        for det in holders.iter_mut() {
            if det.holder == truncated_holder {
                det.holder = full_holder.clone();
            }
        }
    }

    if !copyrights.iter().any(|c| c.copyright == full_cr) {
        copyrights.push(CopyrightDetection {
            copyright: full_cr,
            start_line: line_number,
            end_line: line_number,
        });
    }
    if !holders.iter().any(|h| h.holder == full_holder) {
        holders.push(HolderDetection {
            holder: full_holder,
            start_line: line_number,
            end_line: line_number,
        });
    }
}

pub fn add_missing_holders_for_debian_modifications(
    content: &str,
    copyrights: &[CopyrightDetection],
) -> Vec<HolderDetection> {
    let has_debian_mods_line = content.lines().any(|l| {
        let lower = l.trim().to_ascii_lowercase();
        lower.starts_with("modifications for debian copyright")
    });
    if !has_debian_mods_line {
        return Vec::new();
    }

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
