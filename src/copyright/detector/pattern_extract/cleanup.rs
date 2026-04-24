// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::copyright::detector::token_utils::{group_by, normalize_whitespace};
use crate::copyright::line_tracking::LineNumberIndex;
use crate::copyright::types::{CopyrightDetection, HolderDetection};

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

pub fn drop_url_extended_prefix_duplicates(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let has_url = |s: &str| s.contains("http://") || s.contains("https://");

    let with_url: Vec<_> = copyrights
        .iter()
        .filter(|c| has_url(&c.copyright))
        .cloned()
        .collect();

    copyrights.retain(|c| {
        if has_url(&c.copyright) {
            return true;
        }

        with_url.iter().all(|longer| {
            if c.start_line != longer.start_line || c.end_line > longer.end_line {
                return true;
            }
            if !longer.copyright.starts_with(&c.copyright) {
                return true;
            }

            let tail = longer.copyright[c.copyright.len()..].trim_start();
            !tail.starts_with('-') && !tail.starts_with("http")
        })
    });
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

pub fn drop_shadowed_and_or_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let by_span: HashMap<(usize, usize), Vec<String>> =
        group_by(holders.clone(), |h| (h.start_line.get(), h.end_line.get()))
            .into_iter()
            .map(|(span, group)| (span, group.into_iter().map(|h| h.holder).collect()))
            .collect();

    holders.retain(|h| {
        let short = h.holder.as_str();
        let shadow_prefix = format!("{short} and/or ");
        let span = (h.start_line.get(), h.end_line.get());

        !by_span.get(&span).is_some_and(|group_texts| {
            group_texts.iter().any(|other| {
                other.len() > short.len()
                    && other.starts_with(&shadow_prefix)
                    && other[shadow_prefix.len()..]
                        .to_lowercase()
                        .starts_with("its ")
            })
        })
    });
}

pub fn drop_shadowed_prefix_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let by_span: HashMap<(usize, usize), Vec<String>> =
        group_by(holders.clone(), |h| (h.start_line.get(), h.end_line.get()))
            .into_iter()
            .map(|(span, group)| (span, group.into_iter().map(|h| h.holder).collect()))
            .collect();

    holders.retain(|h| {
        let short = h.holder.trim();
        let is_short_acronym =
            (2..=3).contains(&short.len()) && short.chars().all(|c| c.is_ascii_uppercase());
        if short.len() < 4 && !is_short_acronym {
            return true;
        }

        let span = (h.start_line.get(), h.end_line.get());
        let Some(group_texts) = by_span.get(&span) else {
            return true;
        };

        let mut shadowed = false;

        if !short.contains(',') {
            let shadow_prefix = format!("{short}, ");
            shadowed = group_texts
                .iter()
                .any(|other| other.len() > short.len() && other.starts_with(&shadow_prefix));
        }

        if !shadowed {
            shadowed = group_texts.iter().any(|other| {
                other.len() > short.len()
                    && other.starts_with(short)
                    && other
                        .as_bytes()
                        .get(short.len())
                        .is_some_and(|b| *b == b',')
            });
        }

        if !shadowed {
            shadowed = group_texts.iter().any(|other| {
                if other.len() <= short.len() || !other.starts_with(short) {
                    return false;
                }
                let tail = other.get(short.len()..).unwrap_or("").trim_start();
                tail.starts_with('-') || tail.starts_with('(')
            });
        }

        if !shadowed
            && short.split_whitespace().count() == 1
            && short.chars().all(|c| c.is_ascii_lowercase())
        {
            let shadow_prefix = format!("{short} ");
            shadowed = group_texts
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

    let by_span: HashMap<(usize, usize), Vec<String>> = group_by(copyrights.clone(), |c| {
        (c.start_line.get(), c.end_line.get())
    })
    .into_iter()
    .map(|(span, group)| (span, group.into_iter().map(|c| c.copyright).collect()))
    .collect();

    copyrights.retain(|c| {
        let short = c.copyright.as_str();
        if short.len() < 10 {
            return true;
        }

        let span = (c.start_line.get(), c.end_line.get());
        let Some(group_texts) = by_span.get(&span) else {
            return true;
        };

        if group_texts.iter().any(|other| {
            if other.len() <= short.len() || !other.starts_with(short) {
                return false;
            }
            let tail = other.get(short.len()..).unwrap_or("").trim_start();
            tail.starts_with('-')
        }) {
            return false;
        }

        if group_texts.iter().any(|other| {
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
        if words.is_empty() || !words[0].eq_ignore_ascii_case("copyright") {
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
            return !group_texts
                .iter()
                .any(|other| other.len() > short.len() && other.starts_with(&shadow_prefix_comma));
        }

        if group_texts.iter().any(|other| {
            if other.len() <= short.len() || !other.starts_with(short) {
                return false;
            }
            let tail = other.get(short.len()..).unwrap_or("").trim_start();
            tail.starts_with('(')
        }) {
            return false;
        }

        if words.len() != 3 || !words[2].chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }

        !group_texts.iter().any(|other| {
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
                    shadowed.insert((*sln, *eln, short.to_string()));
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
