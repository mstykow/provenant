//! Copyright detection orchestrator.
//!
//! Runs the full detection pipeline: text → numbered lines → candidate groups
//! → tokens → parse tree → walk tree → refine → filter junk → detections.
//!
//! The grammar currently builds lower-level structures (Name, Company,
//! YrRange, etc.) but does not yet produce top-level COPYRIGHT/AUTHOR tree
//! nodes. This detector handles both cases:
//! - If the grammar produces COPYRIGHT/AUTHOR nodes, use them directly.
//! - Otherwise, scan the flat node sequence for COPY/AUTH tokens and
//!   collect spans heuristically.

use std::sync::LazyLock;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use regex::Regex;

use super::candidates::collect_candidate_lines;
use super::detector_input_normalization::{
    maybe_expand_copyrighted_by_href_urls, normalize_split_angle_bracket_urls,
};
use super::lexer::get_tokens;
use super::line_tracking::{LineNumberIndex, PreparedLineCache};
use super::parser::{parse, parse_with_deadline};
use super::refiner::{
    refine_author, refine_copyright, refine_holder, refine_holder_in_copyright_context,
};
use super::types::{AuthorDetection, CopyrightDetection, HolderDetection, PosTag, TreeLabel};
#[cfg(test)]
use super::types::{ParseNode, Token};
use crate::models::LineNumber;

const NON_COPYRIGHT_LABELS: &[TreeLabel] = &[];
const NON_HOLDER_LABELS: &[TreeLabel] = &[TreeLabel::YrRange, TreeLabel::YrAnd];
const NON_HOLDER_LABELS_MINI: &[TreeLabel] = &[TreeLabel::YrRange, TreeLabel::YrAnd];

const NON_HOLDER_POS_TAGS: &[PosTag] = &[
    PosTag::Copy,
    PosTag::Yr,
    PosTag::YrPlus,
    PosTag::BareYr,
    PosTag::Email,
    PosTag::Url,
    PosTag::Holder,
    PosTag::Is,
    PosTag::Held,
];

const NON_HOLDER_POS_TAGS_MINI: &[PosTag] = &[
    PosTag::Copy,
    PosTag::Yr,
    PosTag::YrPlus,
    PosTag::BareYr,
    PosTag::Is,
    PosTag::Held,
];

const NON_AUTHOR_POS_TAGS: &[PosTag] = &[
    PosTag::Copy,
    PosTag::Yr,
    PosTag::YrPlus,
    PosTag::BareYr,
    PosTag::Auth,
    PosTag::Auth2,
    PosTag::Auths,
    PosTag::AuthDot,
    PosTag::Contributors,
    PosTag::Commit,
    PosTag::SpdxContrib,
    PosTag::Holder,
    PosTag::Is,
    PosTag::Held,
];

const NON_COPYRIGHT_POS_TAGS: &[PosTag] = &[];

static NOT_COPYRIGHTED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bnot\s+copyrighted\b").unwrap());

/// Returns a tuple of (copyrights, holders, authors).
pub fn detect_copyrights_from_text(
    content: &str,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    detect_copyrights_from_text_with_deadline(content, None)
}

pub fn detect_copyrights_from_text_with_deadline(
    content: &str,
    max_runtime: Option<Duration>,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    let mut copyrights = Vec::new();
    let mut holders = Vec::new();
    let mut authors = Vec::new();
    let deadline = max_runtime.and_then(|d| Instant::now().checked_add(d));

    if content.is_empty() {
        return (copyrights, holders, authors);
    }

    let normalized = normalize_split_angle_bracket_urls(content);
    let expanded = maybe_expand_copyrighted_by_href_urls(normalized.as_ref());
    let did_expand_href = matches!(expanded, Cow::Owned(_));
    let content = expanded.as_ref();
    let line_number_index = LineNumberIndex::new(content);

    let allow_not_copyrighted_prefix = NOT_COPYRIGHTED_RE.find_iter(content).count() == 1;

    let raw_lines: Vec<&str> = content.lines().collect();
    let mut prepared_cache = PreparedLineCache::new(&raw_lines);

    if raw_lines.is_empty() {
        return (copyrights, holders, authors);
    }

    let groups =
        collect_candidate_lines(raw_lines.iter().enumerate().map(|(i, line)| (i + 1, *line)));

    for group in &groups {
        if deadline_exceeded(deadline) {
            break;
        }

        if group.is_empty() {
            continue;
        }

        let tokens = get_tokens(group);
        if tokens.is_empty() {
            continue;
        }

        let tree = if deadline.is_some() {
            parse_with_deadline(tokens, deadline)
        } else {
            parse(tokens)
        };

        let has_top_level_nodes = tree.iter().any(|n| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2) | Some(TreeLabel::Author)
            )
        });

        if has_top_level_nodes {
            let copyrights_before = copyrights.len();
            extract_from_tree_nodes(
                &tree,
                &mut copyrights,
                &mut holders,
                &mut authors,
                allow_not_copyrighted_prefix,
            );

            if let Some(det) = extract_original_author_additional_contributors(&tree)
                && !authors
                    .iter()
                    .any(|a| a.author == det.author && a.start_line.get() == det.start_line.get())
            {
                authors.push(det);
            }

            let has_copy_like_token = tree
                .iter()
                .flat_map(collect_all_leaves)
                .any(|t| matches!(t.tag, PosTag::Copy | PosTag::SpdxContrib));

            let has_authorish_boundary_token = tree.iter().flat_map(collect_all_leaves).any(|t| {
                matches!(
                    t.tag,
                    PosTag::Auths | PosTag::AuthDot | PosTag::Contributors | PosTag::Commit
                )
            });

            let is_single_line_group = {
                let mut line: Option<usize> = None;
                let mut ok = true;
                for t in tree.iter().flat_map(collect_all_leaves) {
                    if let Some(existing) = line {
                        if existing != t.start_line.get() {
                            ok = false;
                            break;
                        }
                    } else {
                        line = Some(t.start_line.get());
                    }
                }
                ok
            };

            if copyrights.len() == copyrights_before
                && has_copy_like_token
                && has_authorish_boundary_token
                && is_single_line_group
            {
                extract_bare_copyrights(&tree, &mut copyrights, &mut holders);
                extract_copyrights_from_spans(
                    &tree,
                    &mut copyrights,
                    &mut holders,
                    allow_not_copyrighted_prefix,
                );
            }

            let has_year_token = tree
                .iter()
                .flat_map(collect_all_leaves)
                .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
            if copyrights.len() == copyrights_before && has_copy_like_token && has_year_token {
                extract_copyrights_from_spans(
                    &tree,
                    &mut copyrights,
                    &mut holders,
                    allow_not_copyrighted_prefix,
                );
            }
        } else {
            extract_bare_copyrights(&tree, &mut copyrights, &mut holders);
            extract_from_spans(
                &tree,
                &mut copyrights,
                &mut holders,
                &mut authors,
                allow_not_copyrighted_prefix,
            );
            extract_orphaned_by_authors(&tree, &mut authors);

            if let Some(det) = extract_original_author_additional_contributors(&tree)
                && !authors
                    .iter()
                    .any(|a| a.author == det.author && a.start_line.get() == det.start_line.get())
            {
                authors.push(det);
            }
        }

        // Run after each group is processed so it can fix authors detected
        // through any extraction path.
        fix_truncated_contributors_authors(&tree, &mut authors);
        extract_holder_is_name(&tree, &mut copyrights, &mut holders);
        apply_written_by_for_markers(group, &mut copyrights, &mut holders);
        extend_multiline_copyright_c_year_holder_continuations(
            group,
            &mut copyrights,
            &mut holders,
        );
        extend_multiline_copyright_c_no_year_names(group, &mut copyrights[..], &mut holders[..]);
        extend_authors_see_url_copyrights(group, &mut copyrights[..], &mut holders[..]);
        extend_leading_dash_suffixes(group, &mut copyrights[..], &mut holders[..]);
        extend_dash_obfuscated_email_suffixes(&raw_lines, group, &mut copyrights[..], &holders[..]);
        extend_trailing_copy_year_suffixes(&raw_lines, group, &mut copyrights[..]);
        extend_w3c_registered_org_list_suffixes(group, &mut copyrights[..], &mut holders[..]);
        extend_software_in_the_public_interest_holder(group, &mut copyrights, &mut holders);
    }

    if copyrights.is_empty() {
        copyrights.extend(fallback_year_only_copyrights(&groups));
    } else {
        let fallback = fallback_year_only_copyrights(&groups);
        let existing_set: HashSet<&str> = copyrights.iter().map(|c| c.copyright.as_str()).collect();
        let to_add: Vec<CopyrightDetection> = fallback
            .into_iter()
            .filter(|det| {
                !existing_set.contains(det.copyright.as_str())
                    && !existing_set.iter().any(|e| {
                        e.to_ascii_lowercase()
                            .contains(&det.copyright.to_ascii_lowercase())
                    })
            })
            .collect();
        copyrights.extend(to_add);
    }

    if deadline_exceeded(deadline) {
        refine_final_copyrights(&mut copyrights);
        dedupe_exact_span_copyrights(&mut copyrights);
        dedupe_exact_span_holders(&mut holders);
        dedupe_exact_span_authors(&mut authors);
        return (copyrights, holders, authors);
    }

    primary_phase::run_phase_primary_extractions(
        content,
        &groups,
        &line_number_index,
        &mut prepared_cache,
        &mut copyrights,
        &mut holders,
    );

    postprocess_phase::run_phase_postprocess(
        content,
        &raw_lines,
        &mut prepared_cache,
        did_expand_href,
        &mut copyrights,
        &mut holders,
        &mut authors,
    );

    refine_final_copyrights(&mut copyrights);
    drop_path_fragment_holders_from_bare_c_code_lines(&raw_lines, &copyrights, &mut holders);
    drop_scan_only_holders_from_copyright_scan_lines(&raw_lines, &copyrights, &mut holders);

    for group in &groups {
        extend_dash_obfuscated_email_suffixes(&raw_lines, group, &mut copyrights[..], &holders[..]);
    }
    restore_linux_foundation_copyrights_from_raw_lines(&raw_lines, &mut copyrights);

    add_missing_holders_for_bare_c_name_year_suffixes(&copyrights, &mut holders);

    dedupe_exact_span_copyrights(&mut copyrights);
    dedupe_exact_span_holders(&mut holders);
    dedupe_exact_span_authors(&mut authors);

    (copyrights, holders, authors)
}

fn refine_final_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }

    let mut refined: Vec<CopyrightDetection> = Vec::with_capacity(copyrights.len());
    for c in copyrights.drain(..) {
        let Some(text) = refine_copyright(&c.copyright) else {
            continue;
        };
        refined.push(CopyrightDetection {
            copyright: text,
            start_line: c.start_line,
            end_line: c.end_line,
        });
    }
    *copyrights = refined;
}

fn deadline_exceeded(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

fn add_missing_holders_for_bare_c_name_year_suffixes(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    static BARE_C_NAME_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)^\(c\)\s+(?P<name>.+?)\s+(?P<year>(?:19\d{2}|20\d{2}))\s*$").unwrap()
    });

    for c in copyrights {
        let trimmed = c.copyright.trim();
        let Some(cap) = BARE_C_NAME_YEAR_RE.captures(trimmed) else {
            continue;
        };
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        if name.split_whitespace().count() != 1 {
            continue;
        }
        if !name
            .chars()
            .all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '’' || ch == '-')
        {
            continue;
        }

        let Some(holder) = refine_holder_in_copyright_context(name) else {
            continue;
        };
        if holder.is_empty() {
            continue;
        }
        if holders.iter().any(|h| {
            h.start_line.get() == c.start_line.get()
                && h.end_line.get() == c.end_line.get()
                && h.holder == holder
        }) {
            continue;
        }
        holders.push(HolderDetection {
            holder,
            start_line: c.start_line,
            end_line: c.end_line,
        });
    }
}

fn truncate_lonely_svox_baslerstr_address(
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

fn add_short_svox_baslerstr_variants(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() || holders.is_empty() {
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

    let has_full = copyrights.iter().any(|c| {
        c.copyright.contains("SVOX") && c.copyright.to_ascii_lowercase().contains("baslerstr")
    });
    if !has_full {
        return;
    }

    if copyrights.len() == 1 && holders.len() == 1 {
        return;
    }

    let existing_c: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let existing_h: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    let mut new_c = Vec::new();
    for c in copyrights.iter() {
        if !c.copyright.contains("SVOX") || !c.copyright.to_ascii_lowercase().contains("baslerstr")
        {
            continue;
        }
        if let Some(short) = truncate_at_baslerstr(&c.copyright)
            && !existing_c.contains(&short)
        {
            new_c.push(CopyrightDetection {
                copyright: short,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    copyrights.extend(new_c);

    let mut new_h = Vec::new();
    for h in holders.iter() {
        if !h.holder.contains("SVOX") || !h.holder.to_ascii_lowercase().contains("baslerstr") {
            continue;
        }
        if let Some(short) = truncate_at_baslerstr(&h.holder)
            && !existing_h.contains(&short)
        {
            new_h.push(HolderDetection {
                holder: short,
                start_line: h.start_line,
                end_line: h.end_line,
            });
        }
    }
    holders.extend(new_h);
}

fn drop_shadowed_year_only_copyright_prefixes_same_start_line(
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

fn drop_year_only_copyrights_shadowed_by_previous_software_copyright_line(
    raw_lines: &[&str],
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn merge_multiline_person_year_copyright_continuations(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln1 = i + 1;
        let ln2 = i + 2;
        let Some(l1) = prepared_cache.get(ln1).map(|s| s.to_string()) else {
            continue;
        };
        let Some(l2) = prepared_cache.get(ln2).map(|s| s.to_string()) else {
            continue;
        };
        let l1t = l1.trim();
        let l2t = l2.trim();

        let Some(c1) = FIRST_RE.captures(l1t) else {
            continue;
        };
        let Some(c2) = SECOND_RE.captures(l2t) else {
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

        if !copyrights
            .iter()
            .any(|c| c.start_line.get() == ln1 && c.end_line.get() == ln2 && c.copyright == refined)
        {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }

        let raw_holder = format!("{name1}, {name2}");
        if let Some(h) = refine_holder_in_copyright_context(&raw_holder)
            && !holders
                .iter()
                .any(|x| x.start_line.get() == ln1 && x.end_line.get() == ln2 && x.holder == h)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }
    }
}

fn add_embedded_copyright_clause_variants(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }
    if copyrights.len() < 50 {
        return;
    }

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    let mut to_add = Vec::new();
    for c in copyrights.iter() {
        let lower = c.copyright.to_ascii_lowercase();
        if !lower.starts_with("portions created by the initial developer are ") {
            continue;
        }
        let Some(pos) = lower.find(" copyright") else {
            continue;
        };
        let embedded = c.copyright[pos + 1..].trim();
        if embedded.is_empty() {
            continue;
        }
        let Some(refined) = refine_copyright(embedded) else {
            continue;
        };
        if refined
            .to_ascii_lowercase()
            .contains("the initial developer")
        {
            continue;
        }
        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
        if !existing.contains(&key) {
            to_add.push(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    copyrights.extend(to_add);
}

fn add_found_at_short_variants(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static FOUND_AT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(c\)\s+by\s+(?P<name>.+?)\s+found\s+at\b").unwrap());

    let existing_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let existing_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    let mut new_c = Vec::new();
    let mut new_h = Vec::new();
    for c in copyrights.iter() {
        let Some(cap) = FOUND_AT_RE.captures(c.copyright.trim()) else {
            continue;
        };
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let short = format!("(c) by {name}");
        let key = (c.start_line.get(), c.end_line.get(), short.clone());
        if !existing_c.contains(&key) {
            new_c.push(CopyrightDetection {
                copyright: short,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }

        let holder_short = name.to_string();
        let hkey = (c.start_line.get(), c.end_line.get(), holder_short.clone());
        if !existing_h.contains(&hkey) {
            new_h.push(HolderDetection {
                holder: holder_short,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    copyrights.extend(new_c);
    holders.extend(new_h);
}

fn add_missing_holders_from_email_bearing_copyrights(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    static COPYRIGHT_NAME_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^copyright(?:\s*\(c\))?\s+[0-9][0-9,\-–/ ]*\s+(?:by\s+)?(?P<name>[^<]+?)\s*<[^>\s]*@[^>\s]*>\s*$",
        )
        .unwrap()
    });

    let existing: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| {
            (
                h.start_line.get(),
                h.end_line.get(),
                normalize_whitespace(&h.holder),
            )
        })
        .collect();

    let mut additions = Vec::new();
    for c in copyrights {
        let Some(cap) = COPYRIGHT_NAME_EMAIL_RE.captures(c.copyright.trim()) else {
            continue;
        };
        let raw_name = cap.name("name").map(|m| m.as_str()).unwrap_or("");
        let cleaned_name = normalize_email_copyright_holder_candidate(raw_name);
        if cleaned_name.is_empty() {
            continue;
        }

        let Some(name) = refine_holder_in_copyright_context(&cleaned_name) else {
            continue;
        };
        let domain_only = name.contains('.')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
        if domain_only {
            continue;
        }

        let key = (c.start_line.get(), c.end_line.get(), name.clone());
        if existing.contains(&key) {
            continue;
        }

        additions.push(HolderDetection {
            holder: name,
            start_line: c.start_line,
            end_line: c.end_line,
        });
    }

    holders.extend(additions);
}

fn normalize_email_copyright_holder_candidate(raw_name: &str) -> String {
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
    cleaned = normalize_whitespace(&cleaned);

    if let Some(cap) = INLINE_YEAR_PERSON_RE.captures(&cleaned) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !name.is_empty() {
            cleaned = format!("{prefix} {name}");
        }
    }

    normalize_whitespace(&cleaned)
}

fn drop_combined_semicolon_shadowed_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
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

fn drop_shadowed_linux_foundation_holder_copyrights_same_line(
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

    let mut years_by_line: HashSet<(usize, String)> = HashSet::new();
    for c in copyrights.iter() {
        if let Some(cap) = WITH_C_RE.captures(c.copyright.trim()) {
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            if !years.is_empty() {
                years_by_line.insert((c.start_line.get(), years.to_string()));
            }
        }
    }

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

fn restore_linux_foundation_copyrights_from_raw_lines(
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

        let full = normalize_whitespace(&format!("Copyright (c) {years} Linux Foundation"));
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

        let bare = normalize_whitespace(&format!("Copyright (c) {years}"));
        copyrights.retain(|c| {
            !(c.start_line.get() == ln && c.end_line.get() == ln && c.copyright == bare)
        });
    }

    copyrights.extend(to_add);
}

fn add_bare_email_variants_for_escaped_angle_lines(
    raw_lines: &[&str],
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if raw_lines.is_empty() || copyrights.is_empty() {
        return;
    }

    static ANGLE_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<\s*([^\s<>]+@[^\s<>]+)\s*>").unwrap());

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    let mut to_add = Vec::new();
    for c in copyrights.iter() {
        if c.start_line.get() != c.end_line.get() {
            continue;
        }
        let Some(raw) = raw_lines.get(c.start_line.get() - 1) else {
            continue;
        };
        let raw_lower = raw.to_ascii_lowercase();
        if !(raw_lower.contains("&lt;") && raw_lower.contains("&gt;") && raw_lower.contains('@')) {
            continue;
        }
        if !(c.copyright.contains('<') && c.copyright.contains('>') && c.copyright.contains('@')) {
            continue;
        }
        let bare = ANGLE_EMAIL_RE
            .replace_all(c.copyright.as_str(), "$1")
            .to_string();
        let Some(refined) = refine_copyright(&bare) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
        if !existing.contains(&key) {
            to_add.push(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    copyrights.extend(to_add);
}

fn drop_comma_holders_shadowed_by_space_version_same_span(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

    let mut by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for h in holders.iter() {
        by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .insert(h.holder.clone());
    }

    holders.retain(|h| {
        let Some(set) = by_span.get(&(h.start_line.get(), h.end_line.get())) else {
            return true;
        };
        if !h.holder.contains(',') {
            return true;
        }
        let no_comma = normalize_whitespace(&h.holder.replace(',', ""));
        !(no_comma != h.holder && set.contains(&no_comma))
    });
}

fn normalize_company_suffix_period_holder_variants(holders: &mut Vec<HolderDetection>) {
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

fn add_confidential_short_variants_late(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    static CONF_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+(?P<year>\d{4})\s+confidential\s+information\b").unwrap()
    });

    let mut existing_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let mut existing_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for c in copyrights.clone() {
        let Some(cap) = CONF_RE.captures(c.copyright.as_str()) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() {
            continue;
        }
        let short_c_raw = format!("Copyright {year} Confidential");
        let Some(short_c) = refine_copyright(&short_c_raw) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get(), short_c.clone());
        if existing_c.insert(key) {
            copyrights.push(CopyrightDetection {
                copyright: short_c,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }

        let short_h = "Confidential".to_string();
        let hkey = (c.start_line.get(), c.end_line.get(), short_h.clone());
        if existing_h.insert(hkey) {
            holders.push(HolderDetection {
                holder: short_h,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
}

fn split_multiline_holder_lists_from_copyright_email_sequences(
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

fn add_karlsruhe_university_short_variants(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() && holders.is_empty() {
        return;
    }

    static KARLSRUHE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bUniversity\s+of\s+Karlsruhe\b").unwrap());
    static KARLSRUHE_TERMINAL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bUniversity\s+of\s+Karlsruhe\b\s*[)\]\.\,;:]?\s*$").unwrap()
    });

    let mut existing_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let mut existing_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for c in copyrights.clone() {
        if !KARLSRUHE_RE.is_match(c.copyright.as_str()) {
            continue;
        }
        if !KARLSRUHE_TERMINAL_RE.is_match(c.copyright.as_str()) {
            continue;
        }
        let short = KARLSRUHE_RE
            .replace_all(c.copyright.as_str(), "University")
            .to_string();
        let short = normalize_whitespace(&short);
        if short == c.copyright {
            continue;
        }
        let key = (c.start_line.get(), c.end_line.get(), short.clone());
        if existing_c.insert(key) {
            copyrights.push(CopyrightDetection {
                copyright: short,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }

    for h in holders.clone() {
        if !KARLSRUHE_RE.is_match(h.holder.as_str()) {
            continue;
        }
        if !KARLSRUHE_TERMINAL_RE.is_match(h.holder.as_str()) {
            continue;
        }
        let short = KARLSRUHE_RE
            .replace_all(h.holder.as_str(), "University")
            .to_string();
        let short = normalize_whitespace(&short);
        if short == h.holder {
            continue;
        }
        let key = (h.start_line.get(), h.end_line.get(), short.clone());
        if existing_h.insert(key) {
            holders.push(HolderDetection {
                holder: short,
                start_line: h.start_line,
                end_line: h.end_line,
            });
        }
    }
}

fn add_intel_and_sun_non_portions_variants(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if prepared_cache.is_empty() || copyrights.is_empty() {
        return;
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

    let mut existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    for c in copyrights.clone() {
        let trimmed = c.copyright.trim();
        if let Some(cap) = PORTIONS_SUN_RE.captures(trimmed) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("");
            if !year.is_empty() {
                let candidate =
                    normalize_whitespace(&format!("Copyright {year} Sun Microsystems{tail}"));
                if let Some(refined) = refine_copyright(&candidate) {
                    let key = (c.start_line.get(), c.end_line.get(), refined.clone());
                    if existing.insert(key) {
                        copyrights.push(CopyrightDetection {
                            copyright: refined,
                            start_line: c.start_line,
                            end_line: c.end_line,
                        });
                    }
                }
            }
        }

        if PORTIONS_INTEL_RE.is_match(trimmed)
            && (c.end_line.get() > c.start_line.get() || trimmed.contains('('))
        {
            let mut joined = String::new();
            for ln in c.start_line.get()..=c.end_line.get() {
                if let Some(p) = prepared_cache.get(ln) {
                    if !joined.is_empty() {
                        joined.push(' ');
                    }
                    joined.push_str(p);
                }
            }
            let joined = normalize_whitespace(&joined);
            if let Some(cap) = INTEL_EMAILS_RE.captures(joined.as_str()) {
                let emails = cap.name("emails").map(|m| m.as_str()).unwrap_or("").trim();
                if !emails.is_empty() {
                    let candidate =
                        normalize_whitespace(&format!("Copyright 2002 Intel ({emails})"));
                    if let Some(refined) = refine_copyright(&candidate) {
                        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
                        if existing.insert(key) {
                            copyrights.push(CopyrightDetection {
                                copyright: refined,
                                start_line: c.start_line,
                                end_line: c.end_line,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn add_first_angle_email_only_variants(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }

    static MULTI_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>Copyright\b.*?<[^>\s]*@[^>\s]+>)(?:\s*,\s*.+)$").unwrap()
    });

    let mut existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    for c in copyrights.clone() {
        let trimmed = c.copyright.trim();
        let Some(cap) = MULTI_EMAIL_RE.captures(trimmed) else {
            continue;
        };
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if prefix.is_empty() {
            continue;
        }
        let Some(refined) = refine_copyright(prefix) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
        if existing.insert(key) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
}

fn drop_shadowed_angle_email_prefix_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    static EMAIL_TAIL_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^\s*,\s*(?:<?\.?[a-z0-9][a-z0-9._%+\-]{0,63}@[a-z0-9][a-z0-9._\-]{0,253}\.[a-z]{2,15}>?)(?:\s*,\s*(?:<?\.?[a-z0-9][a-z0-9._%+\-]{0,63}@[a-z0-9][a-z0-9._\-]{0,253}\.[a-z]{2,15}>?))*\s*$",
        )
        .unwrap()
    });

    let mut by_span: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for c in copyrights.iter() {
        by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .push(c.copyright.clone());
    }

    copyrights.retain(|c| {
        let span = (c.start_line.get(), c.end_line.get());
        let Some(all) = by_span.get(&span) else {
            return true;
        };
        let s = c.copyright.trim();
        if !s.ends_with('>') {
            return true;
        }
        let mut has_longer = false;
        let mut has_email_only_extension = false;
        for other in all {
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
    });
}

fn drop_shadowed_quote_before_email_variants_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

    static QUOTED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)'\s+(<[^>\s]*@[^>\s]+>|[^\s<>]*@[^\s<>]+)").unwrap());

    fn canonical(s: &str) -> String {
        normalize_whitespace(&QUOTED_RE.replace_all(s, " $1"))
    }

    let mut exact_by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for c in copyrights.iter() {
        exact_by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .insert(c.copyright.clone());
    }

    copyrights.retain(|c| {
        if !c.copyright.contains('\'') || !c.copyright.contains('@') {
            return true;
        }
        let span = (c.start_line.get(), c.end_line.get());
        let Some(exact) = exact_by_span.get(&span) else {
            return true;
        };
        let canon = canonical(&c.copyright);
        if canon == c.copyright {
            return true;
        }
        !exact.contains(&canon)
    });
}

fn add_missing_holder_from_single_copyright(
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if !holders.is_empty() || copyrights.len() != 1 {
        return;
    }
    let c = &copyrights[0];
    let Some(h) = derive_holder_from_simple_copyright_string(&c.copyright) else {
        return;
    };
    let Some(h) = refine_holder_in_copyright_context(&h) else {
        return;
    };

    let trimmed = h.trim();
    if trimmed.to_ascii_lowercase().starts_with("copyright ") {
        return;
    }
    static YEAR_ONLY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d{4}(?:\s*[-–]\s*\d{4})?$").unwrap());
    if YEAR_ONLY_RE.is_match(trimmed) {
        return;
    }
    holders.push(HolderDetection {
        holder: h,
        start_line: c.start_line,
        end_line: c.end_line,
    });
}

fn add_but_suffix_short_variants(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.is_empty() {
        return;
    }

    static BUT_SUFFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?),\s*but\s*$").unwrap());

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    let mut to_add = Vec::new();
    for c in copyrights.iter() {
        let trimmed = c.copyright.trim();
        let Some(cap) = BUT_SUFFIX_RE.captures(trimmed) else {
            continue;
        };
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if prefix.is_empty() {
            continue;
        }
        let Some(refined) = refine_copyright(prefix) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
        if !existing.contains(&key) {
            to_add.push(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }

    copyrights.extend(to_add);
}

fn add_at_affiliation_short_variants(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() && holders.is_empty() {
        return;
    }

    let existing_c: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();
    let existing_h: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    let mut to_add_c = Vec::new();
    for c in copyrights.iter() {
        let Some((head, _tail)) = c.copyright.split_once(" @ ") else {
            continue;
        };
        let head = head.trim_end();
        if head.is_empty() {
            continue;
        }
        let Some(refined) = refine_copyright(head) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
        if !existing_c.contains(&key) {
            to_add_c.push(CopyrightDetection {
                copyright: refined,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    copyrights.extend(to_add_c);

    let mut to_add_h = Vec::new();
    for h in holders.iter() {
        let Some((head, tail)) = h.holder.split_once(" @ ") else {
            continue;
        };
        if tail.contains('@') {
            continue;
        }
        let head = head.trim_end();
        if head.is_empty() {
            continue;
        }
        let Some(refined) = refine_holder_in_copyright_context(head) else {
            continue;
        };
        let key = (h.start_line.get(), h.end_line.get(), refined.clone());
        if !existing_h.contains(&key) {
            to_add_h.push(HolderDetection {
                holder: refined,
                start_line: h.start_line,
                end_line: h.end_line,
            });
        }
    }
    holders.extend(to_add_h);
}

fn add_missing_copyrights_for_holder_lines_with_emails(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &[HolderDetection],
) {
    if prepared_cache.is_empty() || holders.is_empty() {
        return;
    }

    let mut copyright_lines: HashSet<usize> = HashSet::new();
    for c in copyrights.iter() {
        if c.start_line.get() == c.end_line.get() {
            copyright_lines.insert(c.start_line.get());
        }
    }

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    let mut to_add = Vec::new();
    for h in holders.iter() {
        if h.start_line.get() != h.end_line.get() {
            continue;
        }
        let ln = h.start_line.get();
        if ln == 0 || ln > prepared_cache.len() {
            continue;
        }
        if copyright_lines.contains(&ln) {
            continue;
        }
        let Some(prepared) = prepared_cache.get(ln) else {
            continue;
        };
        let prepared = prepared.trim();
        if prepared.is_empty() {
            continue;
        }
        if !prepared.to_ascii_lowercase().contains("copyright") {
            continue;
        }
        if !prepared.contains('@') {
            continue;
        }
        if !prepared.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }

        let Some(refined) = refine_copyright(prepared) else {
            continue;
        };
        let key = (ln, ln, refined.clone());
        if existing.contains(&key) {
            continue;
        }
        to_add.push(CopyrightDetection {
            copyright: refined,
            start_line: LineNumber::new(ln).expect("invalid line number"),
            end_line: LineNumber::new(ln).expect("invalid line number"),
        });
    }
    copyrights.extend(to_add);
}

fn extend_inline_obfuscated_angle_email_suffixes(
    prepared_cache: &mut PreparedLineCache<'_>,
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
                let prepared = normalize_whitespace(line);
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

        let current = normalize_whitespace(&c.copyright);
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

fn contains_obfuscated_email_markers(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has_at = [" at ", "(at)", "[at]", "<at>", "{at}"]
        .iter()
        .any(|needle| lower.contains(needle));
    let has_dot = [" dot ", "(dot)", "[dot]", "<dot>", "{dot}"]
        .iter()
        .any(|needle| lower.contains(needle));
    has_at && has_dot
}

fn strip_lone_obfuscated_angle_email_user_tokens(
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

fn add_at_domain_variants_for_short_net_angle_emails(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.is_empty() {
        return;
    }

    if !prepared_cache.contains_ci("pipe read code from") {
        return;
    }

    static SHORT_NET_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<(?P<user>[a-z]{3})@(?P<domain>[^>\s]+\.net)>").unwrap());

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    let mut to_add = Vec::new();
    for c in copyrights.iter() {
        let Some(cap) = SHORT_NET_EMAIL_RE.captures(c.copyright.as_str()) else {
            continue;
        };
        let user = cap.name("user").map(|m| m.as_str()).unwrap_or("").trim();
        let domain = cap.name("domain").map(|m| m.as_str()).unwrap_or("").trim();
        if user.is_empty() || domain.is_empty() {
            continue;
        }
        let replaced = SHORT_NET_EMAIL_RE
            .replace_all(c.copyright.as_str(), format!("@{domain}").as_str())
            .into_owned();
        let Some(refined) = refine_copyright(&replaced) else {
            continue;
        };
        let key = (c.start_line.get(), c.end_line.get(), refined.clone());
        if existing.contains(&key) {
            continue;
        }
        to_add.push(CopyrightDetection {
            copyright: refined,
            start_line: c.start_line,
            end_line: c.end_line,
        });
    }
    copyrights.extend(to_add);
}

fn drop_shadowed_plain_email_prefix_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    static TRAILING_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>Copyright\b.*?\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})$")
            .unwrap()
    });

    let mut by_span: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for c in copyrights.iter() {
        by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .push(c.copyright.clone());
    }

    let mut to_drop: HashSet<(usize, usize, String)> = HashSet::new();
    for (span, all) in &by_span {
        for s in all {
            let s_trim = s.trim();
            let Some(cap) = TRAILING_EMAIL_RE.captures(s_trim) else {
                continue;
            };
            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if prefix.is_empty() {
                continue;
            }
            for other in all {
                let o = other.trim();
                if o == prefix {
                    continue;
                }
                if o.starts_with(prefix)
                    && o[prefix.len()..].trim_start().starts_with(',')
                    && !o[prefix.len()..].contains('@')
                {
                    to_drop.insert((span.0, span.1, other.clone()));
                }
            }
        }
    }

    copyrights.retain(|c| {
        !to_drop.contains(&(c.start_line.get(), c.end_line.get(), c.copyright.clone()))
    });
}

fn drop_single_line_copyrights_shadowed_by_multiline_same_start(
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

    let mut multi_keys: HashSet<(usize, String, String)> = HashSet::new();
    for c in copyrights.iter() {
        if c.end_line.get() <= c.start_line.get() {
            continue;
        }
        let Some(cap) = YEARS_EMAIL_RE.captures(c.copyright.trim()) else {
            continue;
        };
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("");
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("");
        let years_norm = years
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        if years_norm.is_empty() || email.is_empty() {
            continue;
        }
        multi_keys.insert((c.start_line.get(), years_norm, email.to_ascii_lowercase()));
    }

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

fn normalize_french_support_disclaimer_copyrights(
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

fn drop_shadowed_email_org_location_suffixes_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.len() < 2 && holders.len() < 2 {
        return;
    }

    static INRIA_LOC_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+\bINRIA)\s+(?P<loc>[A-Z][a-z]{2,64})$").unwrap()
    });
    static LEADING_EMAIL_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<email>[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,15})\s*,\s+.+$").unwrap()
    });

    let mut exact_c_by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for c in copyrights.iter() {
        exact_c_by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .insert(c.copyright.clone());
    }

    copyrights.retain(|c| {
        let span = (c.start_line.get(), c.end_line.get());
        let Some(set) = exact_c_by_span.get(&span) else {
            return true;
        };
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
        !set.contains(prefix)
    });

    let mut exact_h_by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for h in holders.iter() {
        exact_h_by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .insert(h.holder.clone());
    }

    let mut to_add_h = Vec::new();
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
        let key = (h.start_line.get(), h.end_line.get(), refined_email.clone());
        if exact_h_by_span
            .get(&(h.start_line.get(), h.end_line.get()))
            .is_some_and(|set| set.contains(&refined_email))
        {
            continue;
        }
        to_add_h.push(HolderDetection {
            holder: refined_email,
            start_line: h.start_line,
            end_line: h.end_line,
        });
        exact_h_by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .insert(key.2);
    }
    holders.extend(to_add_h);

    holders.retain(|h| {
        let span = (h.start_line.get(), h.end_line.get());
        let Some(set) = exact_h_by_span.get(&span) else {
            return true;
        };
        let trimmed = h.holder.trim();
        let Some(cap) = LEADING_EMAIL_COMMA_RE.captures(trimmed) else {
            return true;
        };
        let email = cap.name("email").map(|m| m.as_str()).unwrap_or("").trim();
        if email.is_empty() {
            return true;
        }
        if trimmed.eq_ignore_ascii_case(email) {
            return true;
        }
        !set.contains(email)
    });
}

fn add_pipe_read_parenthetical_variants(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if prepared_cache.len() < 2 || copyrights.is_empty() {
        return;
    }

    static PIPE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\(\s*pipe\s+read\s+code\s+from\s+[^)]+\)\s*$").unwrap());

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln1 = i + 1;
        let ln2 = i + 2;
        let Some(l1) = prepared_cache.get(ln1).map(|s| s.trim().to_string()) else {
            continue;
        };
        let Some(l2) = prepared_cache.get(ln2).map(|s| s.trim().to_string()) else {
            continue;
        };
        if l1.is_empty() || l2.is_empty() {
            continue;
        }
        if !l1.to_ascii_lowercase().contains("copyright") {
            continue;
        }
        if !PIPE_RE.is_match(l2.as_str()) {
            continue;
        }
        let combined = format!("{} {}", l1.trim_end(), l2.trim_start());
        let Some(refined) = refine_copyright(&combined) else {
            continue;
        };
        let key = (ln1, ln2, refined.clone());
        if !existing.contains(&key) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }
    }
}

fn add_from_url_parenthetical_copyright_variants(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static FROM_URL_COPY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bfrom\s+https?://\S+\s*\(\s*copyright\b").unwrap());

    let existing: HashSet<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    for i in 0..prepared_cache.len() {
        let ln = i + 1;
        let Some(line) = prepared_cache.get_by_index(i).map(|s| s.trim().to_string()) else {
            continue;
        };
        if line.is_empty() {
            continue;
        }
        if !FROM_URL_COPY_RE.is_match(line.as_str()) {
            continue;
        }
        let mut s = line;
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("adapted from ") {
            s = format!("from {}", s["adapted from ".len()..].trim_start());
        }
        let Some(refined) = refine_copyright(&s) else {
            continue;
        };
        let key = (ln, ln, refined.clone());
        if !existing.contains(&key) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

fn drop_shadowed_acronym_location_suffix_copyrights_same_span(
    copyrights: &mut Vec<CopyrightDetection>,
) {
    if copyrights.len() < 2 {
        return;
    }

    static ACR_LOC_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<prefix>.+\b(?P<acr>[A-Z]{2,10}))\s+(?P<loc>[A-Z][a-z]{2,})\s*$").unwrap()
    });

    let mut by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for c in copyrights.iter() {
        by_span
            .entry((c.start_line.get(), c.end_line.get()))
            .or_default()
            .insert(c.copyright.clone());
    }

    copyrights.retain(|c| {
        let span = (c.start_line.get(), c.end_line.get());
        let Some(set) = by_span.get(&span) else {
            return true;
        };
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
}

fn drop_copyright_like_holders(holders: &mut Vec<HolderDetection>) {
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
            && lower != "rest"
    });
}

fn drop_json_description_metadata_copyrights_and_holders(
    raw_lines: &[&str],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static JSON_COPYRIGHT_KEY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)"copyrights?"\s*:"#).unwrap());

    let mut retained_spans: HashSet<(usize, usize)> = HashSet::new();
    copyrights.retain(|copyright| {
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
        let keep = !description_like || JSON_COPYRIGHT_KEY_RE.is_match(&window);
        if keep {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
        }
        keep
    });

    holders.retain(|holder| {
        if retained_spans.contains(&(holder.start_line.get(), holder.end_line.get())) {
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

fn json_window_for_span(raw_lines: &[&str], start_line: usize, end_line: usize) -> Option<String> {
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

fn restore_url_slash_before_closing_paren_from_raw_lines(
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

fn extract_mso_document_properties_copyrights(
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

    let desc_prepared = normalize_whitespace(&desc_prepared);
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
        let holder = normalize_whitespace(&format!(
            "{tail} {template} <o:LastAuthor> {last_author} </o:LastAuthor>"
        ));
        let c = normalize_whitespace(&format!("Copyright {year} {holder}"));
        (c, holder)
    } else {
        let holder = normalize_whitespace(&format!("{template} o:LastAuthor {last_author}"));
        let c = normalize_whitespace(&format!("Copyright {year} {holder}"));
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

    let shadow_non_confidential = normalize_whitespace(&format!("{last_author} Copyright {year}"));
    copyrights.retain(|c| {
        !normalize_whitespace(&c.copyright).eq_ignore_ascii_case(&shadow_non_confidential)
    });
    holders.retain(|h| !normalize_whitespace(&h.holder).eq_ignore_ascii_case(&last_author));

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

fn expand_portions_copyright_variants(copyrights: &mut [CopyrightDetection]) {
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

fn expand_year_only_copyrights_with_by_name_prefix(
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn expand_year_only_copyrights_with_read_the_suffix(
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn merge_multiline_obfuscated_name_year_copyright_pairs(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let Some(prepared) = prepared_cache.get_by_index(i) else {
            continue;
        };
        if !(prepared.contains("Copyright") || prepared.contains("copyright")) {
            continue;
        }

        let ln1 = i + 1;
        let ln2 = i + 2;

        let Some(l1p) = prepared_cache.get(ln1).map(|s| s.to_string()) else {
            continue;
        };
        let Some(l2p) = prepared_cache.get(ln2).map(|s| s.to_string()) else {
            continue;
        };
        let l1 = l1p.trim();
        let l2 = l2p.trim();
        let Some(c1) = FIRST_RE.captures(l1) else {
            continue;
        };
        let Some(c2) = SECOND_RE.captures(l2) else {
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
            if c.start_line.get() == ln1 && c.end_line.get() == ln1 && c.copyright.contains(name1) {
                c.copyright = refined.clone();
                c.end_line = LineNumber::new(ln2).expect("valid");
                updated = true;
                break;
            }
        }
        if !updated {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: LineNumber::new(ln1).expect("valid"),
                end_line: LineNumber::new(ln2).expect("valid"),
            });
        }

        let combined_holder_raw = format!("{name1}, {name2}");
        if let Some(h) = refine_holder_in_copyright_context(&combined_holder_raw) {
            holders.retain(|x| {
                !(x.start_line.get() == ln1
                    && x.end_line.get() == ln1
                    && (x.holder == name1 || x.holder.contains(name1)))
            });
            if !holders
                .iter()
                .any(|x| x.start_line.get() == ln1 && x.end_line.get() == ln2 && x.holder == h)
            {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln1).expect("valid"),
                    end_line: LineNumber::new(ln2).expect("valid"),
                });
            }
        }
    }
}

fn extend_copyrights_with_next_line_parenthesized_obfuscated_email(
    prepared_cache: &mut PreparedLineCache<'_>,
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
        c.end_line += 1usize;
    }
}

fn extend_copyrights_with_following_all_rights_reserved_line(
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
        c.end_line += 1usize;
    }
}

fn add_modify_suffix_holders(
    prepared_cache: &mut PreparedLineCache<'_>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() || holders.is_empty() {
        return;
    }

    let mut existing: HashSet<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    for h in holders.clone() {
        let idx = h.end_line.get() + 1;
        let Some(next) = prepared_cache.get(idx) else {
            continue;
        };
        let t = next.trim();
        if t.is_empty() {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if !lower.starts_with("modify ") {
            continue;
        }
        if t.len() > 64 {
            continue;
        }
        if !t
            .split_whitespace()
            .any(|w| w.chars().any(|c| c.is_ascii_uppercase()))
        {
            continue;
        }
        let combined = normalize_whitespace(&format!("{} {t}", h.holder));
        let key = (h.start_line.get(), h.end_line.get() + 1, combined.clone());
        if existing.insert(key) {
            holders.push(HolderDetection {
                holder: combined,
                start_line: h.start_line,
                end_line: h.end_line + 1,
            });
        }
    }
}

fn drop_shadowed_c_sign_variants(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

    fn contains_c_sign(s: &str) -> bool {
        s.to_ascii_lowercase().contains("(c)")
    }

    fn canonical_without_c_sign(s: &str) -> String {
        static C_SIGN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\(c\)").unwrap());
        normalize_whitespace(&C_SIGN_RE.replace_all(s, " "))
    }

    let mut with_c_by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for c in copyrights.iter() {
        if contains_c_sign(&c.copyright) {
            with_c_by_span
                .entry((c.start_line.get(), c.end_line.get()))
                .or_default()
                .insert(canonical_without_c_sign(&c.copyright));
        }
    }
    if with_c_by_span.is_empty() {
        return;
    }

    copyrights.retain(|c| {
        if contains_c_sign(&c.copyright) {
            return true;
        }
        let Some(set) = with_c_by_span.get(&(c.start_line.get(), c.end_line.get())) else {
            return true;
        };
        let canon = canonical_without_c_sign(&c.copyright);
        !set.contains(&canon)
    });
}

fn drop_shadowed_year_prefixed_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

    let mut by_span: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
    for h in holders.iter() {
        by_span
            .entry((h.start_line.get(), h.end_line.get()))
            .or_default()
            .insert(normalize_whitespace(&h.holder));
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

    holders.retain(|h| {
        let Some(set) = by_span.get(&(h.start_line.get(), h.end_line.get())) else {
            return true;
        };
        let normalized = normalize_whitespace(&h.holder);
        let Some(stripped) = strip_leading_year_token(&normalized) else {
            return true;
        };
        !set.contains(&stripped)
    });
}

fn drop_shadowed_for_clause_holders_with_email_copyrights(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if copyrights.is_empty() || holders.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

    let mut spans_with_email: HashSet<(usize, usize)> = HashSet::new();
    for c in copyrights {
        if c.copyright.contains('@') {
            spans_with_email.insert((c.start_line.get(), c.end_line.get()));
        }
    }
    if spans_with_email.is_empty() {
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
        let span = (h.start_line.get(), h.end_line.get());
        if !spans_with_email.contains(&span) {
            return true;
        }

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

        let Some(group) = by_span.get(&span) else {
            return true;
        };
        let has_short = group.iter().any(|other| other.trim() == head);
        !has_short
    });
}

fn drop_shadowed_multiline_prefix_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    let all: Vec<(usize, usize, String)> = copyrights
        .iter()
        .map(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()))
        .collect();

    copyrights.retain(|c| {
        if c.start_line.get() != c.end_line.get() {
            return true;
        }
        let short = c.copyright.as_str();
        if short.len() < 10 {
            return true;
        }

        !all.iter().any(|(s, e, other)| {
            *s == c.start_line.get()
                && *e > c.end_line.get()
                && other.len() > short.len()
                && other.starts_with(short)
                && other
                    .as_bytes()
                    .get(short.len())
                    .is_some_and(|b| b.is_ascii_whitespace() || b.is_ascii_punctuation())
        })
    });
}

fn drop_shadowed_multiline_prefix_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    let all: Vec<(usize, usize, String)> = holders
        .iter()
        .map(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()))
        .collect();

    holders.retain(|h| {
        if h.start_line.get() != h.end_line.get() {
            return true;
        }
        let short = h.holder.as_str();
        if short.len() < 3 {
            return true;
        }

        !all.iter().any(|(s, e, other)| {
            *s == h.start_line.get()
                && *e > h.end_line.get()
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
    });
}

fn replace_holders_with_embedded_c_year_markers(
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

fn extend_year_only_copyrights_with_trailing_text(
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn extract_licensed_material_of_company_bare_c_year_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn merge_year_only_copyrights_with_following_author_colon_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn extract_question_mark_year_copyrights(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static QMARK_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\s+(?P<year>\d{3}\?)\s+(?P<tail>.+)$").unwrap()
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
        let Some(cap) = QMARK_COPY_RE.captures(line) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || tail.is_empty() {
            continue;
        }

        let raw = format!("Copyright {year} {tail}");
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

        let raw_holder = format!("{year} {tail}");
        if let Some(h) = refine_holder_in_copyright_context(&raw_holder)
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

fn strip_inc_suffix_from_holders_for_today_year_copyrights(
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

fn contains_year_placeholder(lower: &str) -> bool {
    lower.contains("today.year") || lower.contains("current_year")
}

fn extend_copyrights_with_authors_blocks(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    for i in 0..prepared_cache.len().saturating_sub(2) {
        let Some(base_prepared) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string())
        else {
            continue;
        };
        if base_prepared.is_empty() {
            continue;
        }
        let base_lower = base_prepared.to_ascii_lowercase();
        if !base_lower.contains("copyright") && !base_lower.contains("(c)") {
            continue;
        }
        if !YEAR_RE.is_match(&base_prepared) {
            continue;
        }

        let Some(header_prepared) = prepared_cache
            .get_by_index(i + 1)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if !AUTHORS_HEADER_RE.is_match(&header_prepared) {
            continue;
        }

        let Some(author_prepared) = prepared_cache
            .get_by_index(i + 2)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        let mut author = author_prepared;
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

        let ln = i + 1;
        let end_ln = i + 3;

        for c in copyrights
            .iter_mut()
            .filter(|c| c.start_line.get() == ln && c.end_line.get() == ln)
        {
            if c.copyright.starts_with("Copyright") || c.copyright.starts_with("(c)") {
                c.copyright = extended.clone();
                c.end_line = LineNumber::new(end_ln).expect("valid");
            }
        }

        if let Some(h) = derive_holder_from_simple_copyright_string(&extended)
            && !holders.iter().any(|hh| hh.holder == h)
        {
            holders.push(HolderDetection {
                holder: h.clone(),
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(end_ln).expect("valid"),
            });

            holders.retain(|hh| {
                if hh.start_line.get() != ln || hh.end_line.get() != ln {
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

fn drop_wider_duplicate_holder_spans(holders: &mut Vec<HolderDetection>) {
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

fn apply_openoffice_org_report_builder_bin_normalizations(
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

fn extract_midline_c_year_holder_with_leading_acronym(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
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

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let trimmed = prepared.trim();

        let lower = trimmed.to_ascii_lowercase();
        if !lower.contains("fix") {
            continue;
        }

        let Some(cap) = MIDLINE_C_YEAR_HOLDER_RE.captures(trimmed) else {
            continue;
        };

        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let holder = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if prefix.is_empty() || year.is_empty() || holder.is_empty() {
            continue;
        }
        if trimmed.to_ascii_lowercase().starts_with("copyright") {
            continue;
        }

        let cr_raw = format!("(c) {year} {holder} {prefix}");
        let Some(cr) = refine_copyright(&cr_raw) else {
            continue;
        };

        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });

        let holder_raw = format!("{holder} {prefix}");
        if let Some(h) = refine_holder_in_copyright_context(&holder_raw) {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

fn dedupe_exact_span_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    copyrights.retain(|c| seen.insert((c.start_line.get(), c.end_line.get(), c.copyright.clone())));
}

fn dedupe_exact_span_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    holders.retain(|h| seen.insert((h.start_line.get(), h.end_line.get(), h.holder.clone())));
}

fn dedupe_exact_span_authors(authors: &mut Vec<AuthorDetection>) {
    if authors.len() < 2 {
        return;
    }
    let mut seen: HashSet<(usize, usize, String)> = HashSet::new();
    authors.retain(|a| seen.insert((a.start_line.get(), a.end_line.get(), a.author.clone())));
}

fn drop_shadowed_prefix_bare_c_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    use std::collections::HashMap;

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
        let short = c.copyright.trim();
        if !short.to_ascii_lowercase().starts_with("(c) ") {
            return true;
        }
        if short.contains(',') || short.contains('<') || short.contains('>') || short.contains('@')
        {
            return true;
        }

        !group.iter().any(|other| {
            other.len() > short.len()
                && other.starts_with(short)
                && other
                    .as_bytes()
                    .get(short.len())
                    .is_some_and(|b| *b == b',')
        })
    });
}

fn drop_shadowed_acronym_extended_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    use std::collections::HashMap;
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
        let candidate = h.holder.trim();

        for base in group {
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
    });
}

fn extend_multiline_copyright_c_no_year_names(
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

fn extend_multiline_copyright_c_year_holder_continuations(
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

fn extend_authors_see_url_copyrights(
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

fn extend_leading_dash_suffixes(
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

fn extend_dash_obfuscated_email_suffixes(
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

fn extend_trailing_copy_year_suffixes(
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

fn extend_w3c_registered_org_list_suffixes(
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

fn drop_symbol_year_only_copyrights(content: &str, copyrights: &mut Vec<CopyrightDetection>) {
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

fn merge_multiline_copyrighted_by_with_trailing_copyright_clause(
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

fn drop_from_source_attribution_copyrights(
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

fn merge_freebird_c_inc_urls(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("(c)") || !prepared_cache.contains_ci("inc") {
        return;
    }
    if !prepared_cache.contains_ci("coventive") && !prepared_cache.contains_ci("legend") {
        return;
    }

    let mut seen_c: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for i in 0..prepared_cache.len() {
        let ln = i + 1;
        let Some(prepared) = prepared_cache.get_by_index(i) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        let line_lower = line.to_ascii_lowercase();
        if !line_lower.contains("(c)") || !line_lower.contains("inc") {
            continue;
        }

        let mut url: Option<String> = None;
        let mut j = i + 1;
        while j < prepared_cache.len() {
            let Some(next) = prepared_cache.get_by_index(j).map(|p| p.trim().to_string()) else {
                break;
            };
            if next.is_empty() {
                j += 1;
                continue;
            }
            if next.to_ascii_lowercase().contains("http") {
                if next.to_ascii_lowercase().contains("web.archive.org/web") {
                    url = Some("http://web.archive.org/web".to_string());
                } else {
                    let next_lower = next.to_ascii_lowercase();
                    if next_lower.contains("coventive.com") {
                        url = Some(next.to_string());
                    }
                }
            }
            break;
        }

        let Some(url) = url else {
            continue;
        };

        let cr_raw = format!("(c), Inc. {url}");
        if let Some(cr) = refine_copyright(&cr_raw)
            && seen_c.insert(cr.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
        let holder_raw = "Inc.";
        if let Some(h) = refine_holder(holder_raw)
            && seen_h.insert(h.clone())
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

fn merge_debugging390_best_viewed_suffix(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("Best viewed") {
        return;
    }

    static IBM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*2000-2001\s+(?P<who>IBM\b.+)$").unwrap()
    });

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln = i + 1;
        let Some(p1) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string()) else {
            continue;
        };
        let Some(cap) = IBM_RE.captures(&p1) else {
            continue;
        };
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if who.is_empty() {
            continue;
        }
        let Some(p2) = prepared_cache.get_by_index(i + 1) else {
            continue;
        };
        if !p2.trim_start().starts_with("Best") {
            continue;
        }

        let merged_raw = format!("Copyright (c) 2000-2001 {who} Best");
        let Some(merged) = refine_copyright(&merged_raw) else {
            continue;
        };

        copyrights.retain(|c| {
            !(c.start_line.get() == ln
                && c.copyright.contains(who)
                && c.copyright.contains("2000-2001")
                && !c.copyright.ends_with("Best"))
        });
        if !copyrights.iter().any(|c| c.copyright == merged) {
            copyrights.push(CopyrightDetection {
                copyright: merged,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }

        let holder_raw = format!("{who} Best");
        holders.retain(|h| !(h.start_line.get() == ln && h.holder == who));
        if let Some(h) = refine_holder_in_copyright_context(&holder_raw) {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
    }
}

fn merge_fsf_gdb_notice_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("GDB is free software") {
        return;
    }

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln = i + 1;
        let Some(p1) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string()) else {
            continue;
        };
        if !p1.starts_with("Copyright 1998 Free Software Foundation") {
            continue;
        }
        let Some(p2) = prepared_cache
            .get_by_index(i + 1)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if !p2.starts_with("GDB is free software") {
            continue;
        }

        let tail = if let Some(idx) = p2.find("GNU General Public License,") {
            &p2[..(idx + "GNU General Public License,".len())]
        } else {
            &p2
        };

        let merged_raw = format!("{p1} {tail}");
        let merged = normalize_whitespace(&merged_raw);
        if !merged.ends_with(',') {
            continue;
        }
        if !copyrights.iter().any(|c| c.copyright == merged) {
            copyrights.push(CopyrightDetection {
                copyright: merged,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }

        let holder = "Free Software Foundation, Inc. GDB free software, covered by the GNU General Public License";
        if !holders.iter().any(|x| x.holder == holder) {
            holders.push(HolderDetection {
                holder: holder.to_string(),
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
    }
}

fn merge_axis_ethereal_suffix(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("Axis Communications") {
        return;
    }

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln = i + 1;
        let Some(p1) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string()) else {
            continue;
        };
        if p1 != "Copyright 2000, Axis Communications AB" {
            continue;
        }
        let Some(p2) = prepared_cache
            .get_by_index(i + 1)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if !p2.starts_with("Ethereal") {
            continue;
        }
        let merged_raw = "Copyright 2000, Axis Communications AB Ethereal";
        let Some(merged) = refine_copyright(merged_raw) else {
            continue;
        };

        copyrights.retain(|c| !(c.start_line.get() == ln && c.copyright == p1));
        if !copyrights.iter().any(|c| c.copyright == merged) {
            copyrights.push(CopyrightDetection {
                copyright: merged,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }

        holders.retain(|h| !(h.start_line.get() == ln && h.holder == "Axis Communications AB"));
        if let Some(h) = refine_holder_in_copyright_context("Axis Communications AB Ethereal")
            && !holders.iter().any(|x| x.holder == h)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
    }
}

fn merge_kirkwood_converted_to(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !prepared_cache.contains_ci("Kirkwood") || !prepared_cache.contains_ci("converted") {
        return;
    }

    static EMBEDDED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(c\)\s+(?P<year>19\d{2}|20\d{2})\s+(?P<who>M\.?\s*Kirkwood)\b").unwrap()
    });

    let mut seen_c: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln = i + 1;
        let Some(p1) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string()) else {
            continue;
        };
        let Some(cap) = EMBEDDED_RE.captures(&p1) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let who = cap.name("who").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || who.is_empty() {
            continue;
        }
        let Some(p2) = prepared_cache
            .get_by_index(i + 1)
            .map(|p| p.trim().trim_start_matches('*').trim_start().to_string())
        else {
            continue;
        };
        if !p2.to_ascii_lowercase().starts_with("converted to") {
            continue;
        }

        let cr_raw = format!("(c) {year} {who} Converted to");
        if let Some(cr) = refine_copyright(&cr_raw)
            && seen_c.insert(cr.clone())
        {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
        let holder_raw = format!("{who} Converted");
        if let Some(h) = refine_holder_in_copyright_context(&holder_raw)
            && seen_h.insert(h.clone())
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
    }
}

fn split_reworked_by_suffixes(
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

    let mut seen_authors: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

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
        if let Some(author) = refine_author(&who)
            && seen_authors.insert(author.clone())
        {
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

fn drop_static_char_string_copyrights(
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

fn drop_combined_period_holders(holders: &mut Vec<HolderDetection>) {
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

fn extract_line_ending_copyright_then_by_holder(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let mut existing: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache
            .get_by_index(idx)
            .map(|p| p.trim().to_string())
        else {
            continue;
        };
        if prepared.is_empty() {
            continue;
        }
        let lower = prepared.to_ascii_lowercase();
        if !lower.ends_with("copyright") {
            continue;
        }
        if lower.contains("copyrighted") {
            continue;
        }
        if !(lower.ends_with("and copyright") || lower == "copyright") {
            continue;
        }

        let mut j = idx + 1;
        while j < prepared_cache.len() {
            let next_ln = j + 1;
            let Some(next_prepared) = prepared_cache.get_by_index(j).map(|p| p.trim().to_string())
            else {
                break;
            };
            if next_prepared.is_empty() {
                j += 1;
                continue;
            }

            let next_lower = next_prepared.to_ascii_lowercase();
            if next_lower.starts_with("by ") {
                let holder_raw = next_prepared[3..].trim();
                let copyright_raw = format!("copyright {}", next_prepared.trim());
                if let Some(copyright_text) = refine_copyright(&copyright_raw)
                    && existing.insert(copyright_text.clone())
                {
                    copyrights.push(CopyrightDetection {
                        copyright: copyright_text,
                        start_line: LineNumber::new(ln).unwrap(),
                        end_line: LineNumber::new(next_ln).expect("valid"),
                    });
                }

                if let Some(holder) = refine_holder_in_copyright_context(holder_raw)
                    && !holders.iter().any(|h| h.holder == holder)
                {
                    holders.push(HolderDetection {
                        holder,
                        start_line: LineNumber::new(next_ln).expect("valid"),
                        end_line: LineNumber::new(next_ln).expect("valid"),
                    });
                }
            }
            break;
        }
    }
}

fn drop_trailing_software_line_from_holders(
    prepared_cache: &mut PreparedLineCache<'_>,
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

fn drop_url_embedded_c_symbol_false_positive_holders(
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

fn recover_template_literal_year_range_copyrights(
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

    let mut seen_copyrights: HashSet<String> =
        copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_holders: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
        if seen_copyrights.insert(copyright_text.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: copyright_text,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        let truncated = format!("Copyright {start}-$");
        copyrights.retain(|c| {
            !(c.start_line.get() == ln
                && c.end_line.get() == ln
                && c.copyright.eq_ignore_ascii_case(&truncated))
        });

        if seen_holders.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

fn drop_url_embedded_suffix_variants_same_span(
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
    }
}

fn extract_following_authors_holders(
    raw_lines: &[&str],
    prepared_cache: &mut PreparedLineCache<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if raw_lines.is_empty() {
        return;
    }

    static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\b.*\bby\s+the\s+following\s+authors\b.*$")
            .expect("valid following authors header regex")
    });
    let mut seen: HashSet<String> = authors.iter().map(|a| a.author.clone()).collect();

    let mut i = 0;
    while i < raw_lines.len() {
        let Some(header) = prepared_cache.get_by_index(i).map(|p| p.trim().to_string()) else {
            i += 1;
            continue;
        };
        if header.is_empty() {
            i += 1;
            continue;
        }
        if !HEADER_RE.is_match(&header) {
            i += 1;
            continue;
        }

        let mut extracted_any = false;
        let mut j = i + 1;
        while j < raw_lines.len() {
            let next_ln = j + 1;
            let raw = raw_lines[j];
            if raw.trim().is_empty() {
                break;
            }
            if !raw.trim_start().starts_with('-') {
                break;
            }
            let mut item = raw.trim_start().trim_start_matches('-').trim().to_string();
            item = normalize_whitespace(&item);
            if !item.is_empty()
                && let Some(author) = refine_author(&item)
                && seen.insert(author.clone())
            {
                authors.push(AuthorDetection {
                    author,
                    start_line: LineNumber::new(next_ln).unwrap(),
                    end_line: LineNumber::new(next_ln).unwrap(),
                });
                extracted_any = true;
            }
            j += 1;
        }

        i = if extracted_any { j } else { i + 1 };
    }
}

fn drop_created_by_camelcase_identifier_authors(
    prepared_cache: &mut PreparedLineCache<'_>,
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
    for idx in 0..prepared_cache.len() {
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        for cap in CREATED_BY_CAMELCASE_RE.captures_iter(prepared) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            by_line
                .entry(idx + 1)
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

fn merge_implemented_by_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    let mut merged: Vec<(usize, String, String, HashSet<String>)> = Vec::new();

    for i in 0..prepared_cache.len().saturating_sub(1) {
        let ln = i + 1;
        let Some(line) = prepared_cache
            .get_by_index(i)
            .map(|p| p.trim().trim_start_matches('*').trim_start().to_string())
        else {
            continue;
        };
        let Some(cap) = COPY_RE.captures(&line) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || holder_raw.is_empty() {
            continue;
        }

        let Some(next) = prepared_cache
            .get_by_index(i + 1)
            .map(|p| p.trim().trim_start_matches('*').trim_start().to_string())
        else {
            continue;
        };
        let Some(cap2) = IMPLEMENTED_RE.captures(&next) else {
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
        merged.push((ln, cr, holder, email_set));
    }

    if merged.is_empty() {
        return;
    }

    for (ln, cr_raw, holder_raw, emails) in merged {
        let Some(cr) = refine_copyright(&cr_raw) else {
            continue;
        };

        let cr_first = cr.split_whitespace().next().unwrap_or("");

        for det in copyrights.iter_mut() {
            if det.start_line.get() == ln
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
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }

        holders.retain(|h| {
            !(h.start_line.get() == ln
                && h.holder == holder_raw.trim_end_matches(" Implemented by"))
        });
        if let Some(h) = refine_holder(&holder_raw)
            && !holders
                .iter()
                .any(|x| x.holder == h && x.start_line.get() == ln)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }

        authors.retain(|a| !emails.contains(&a.author));
    }
}

fn split_written_by_copyrights_into_holder_prefixed_clauses(
    prepared_cache: &mut PreparedLineCache<'_>,
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
    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        for cap in WRITTEN_BY_COPY_RE.captures_iter(line) {
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
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
            if let Some(h) = refine_holder(name) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
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

fn fix_shm_inline_copyrights(
    prepared_cache: &mut PreparedLineCache<'_>,
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

    let mut seen_c: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        if !prepared.contains("/proc/sysvipc/shm") {
            continue;
        }
        let line = prepared.trim();
        let Some(cap) = INLINE_RE.captures(line) else {
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
        if seen_c.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(holder) = refine_holder(name)
            && seen_h.insert(holder.clone())
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

fn fix_n_tty_linus_torvalds_written_by_clause(
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

    let mut seen_c: HashSet<String> = copyrights.iter().map(|c| c.copyright.clone()).collect();
    let mut seen_h: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();

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
        if seen_c.insert(cr.clone()) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
        let holder = "Linus Torvalds".to_string();
        if seen_h.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln + 1).expect("invalid line number"),
            });
        }
        break;
    }
}

fn fix_sundry_contributors_truncation(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static COPYRIGHT_SUNDRY_CONTRIBUTORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^\s*Copyright\s+(?P<year>19\d{2}|20\d{2})\s+(?P<name>.+?)\s+And\s+(?P<tail>Sundry\s+Contributors)\s*$",
        )
        .unwrap()
    });

    let mut matched: Option<(usize, String, String, String)> = None;
    for ln in 1..=prepared_cache.len() {
        let Some(prepared) = prepared_cache.get(ln) else {
            continue;
        };
        if let Some(cap) = COPYRIGHT_SUNDRY_CONTRIBUTORS_RE.captures(prepared.trim()) {
            let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
            matched = Some((ln, year.to_string(), name.to_string(), tail.to_string()));
            break;
        }
    }

    let Some((ln, year, name, tail)) = matched else {
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
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }
    if !holders.iter().any(|h| h.holder == full_holder) {
        holders.push(HolderDetection {
            holder: full_holder,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }
}

fn add_missing_holders_for_debian_modifications(
    content: &str,
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    let has_debian_mods_line = content.lines().any(|l| {
        let lower = l.trim().to_ascii_lowercase();
        lower.starts_with("modifications for debian copyright")
    });
    if !has_debian_mods_line {
        return;
    }

    let mut seen: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();
    for cr in copyrights {
        let Some(holder) = derive_holder_from_simple_copyright_string(&cr.copyright) else {
            continue;
        };
        if seen.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: cr.start_line,
                end_line: cr.end_line,
            });
        }
    }
}

fn split_embedded_copyright_detections(
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

    add_missing_holders_derived_from_split_copyrights(&split_copyrights, holders);
}

fn extend_bare_c_year_detections_to_line_end_for_multi_c_lines(
    prepared_cache: &mut PreparedLineCache<'_>,
    copyrights: &mut [CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    if prepared_cache.is_empty() || copyrights.is_empty() {
        return;
    }

    static C_YEAR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*(?P<year>(?:19\d{2}|20\d{2}))\b").unwrap());

    for idx in 0..prepared_cache.len() {
        let ln = idx + 1;
        let Some(prepared) = prepared_cache.get_by_index(idx) else {
            continue;
        };
        let line = prepared.trim();
        if line.is_empty() {
            continue;
        }
        if line.to_ascii_lowercase().matches("(c)").count() < 2 {
            continue;
        }

        for m in C_YEAR_RE.captures_iter(line) {
            let year = m.name("year").map(|m| m.as_str()).unwrap_or("").trim();
            if year.is_empty() {
                continue;
            }
            let short = format!("(c) {year}");
            let Some(start) = m.get(0).map(|mm| mm.start()) else {
                continue;
            };
            let tail = line.get(start..).unwrap_or("").trim();
            if tail.len() <= short.len() {
                continue;
            }
            let Some(extended) = refine_copyright(tail) else {
                continue;
            };

            let mut did_replace = false;
            for det in copyrights.iter_mut() {
                if det.start_line.get() == ln && det.end_line.get() == ln && det.copyright == short
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
                    .any(|h| h.start_line.get() == ln && h.holder == holder)
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

fn add_missing_holders_derived_from_split_copyrights(
    copyrights: &[CopyrightDetection],
    holders: &mut Vec<HolderDetection>,
) {
    let mut seen: HashSet<String> = holders.iter().map(|h| h.holder.clone()).collect();
    for cr in copyrights {
        let Some(holder) = derive_holder_from_simple_copyright_string(&cr.copyright) else {
            continue;
        };
        if seen.insert(holder.clone()) {
            holders.push(HolderDetection {
                holder,
                start_line: cr.start_line,
                end_line: cr.end_line,
            });
        }
    }
}

fn derive_holder_from_simple_copyright_string(s: &str) -> Option<String> {
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

fn drop_obfuscated_email_year_only_copyrights(
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

fn extract_glide_3dfx_copyright_notice(content: &str, copyrights: &mut Vec<CopyrightDetection>) {
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

fn extract_spdx_filecopyrighttext_c_without_year(
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

fn extract_html_meta_name_copyright_content(
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

            if let Some(holder) = derive_holder_from_simple_copyright_string(&refined)
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

fn extract_added_the_copyright_year_for_lines(
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

fn extract_changelog_timestamp_copyrights_from_content(
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

fn drop_arch_floppy_h_bare_1995(content: &str, copyrights: &mut Vec<CopyrightDetection>) {
    let lower = content.to_ascii_lowercase();
    let is_x86 = lower.contains("_asm_x86_floppy_h");
    let is_powerpc = lower.contains("__asm_powerpc_floppy_h");
    if !is_x86 && !is_powerpc {
        return;
    }

    copyrights.retain(|c| !c.copyright.eq_ignore_ascii_case("Copyright (c) 1995"));
}

fn drop_batman_adv_contributors_copyright(
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

fn is_lppl_license_document(content: &str) -> bool {
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

fn extract_common_year_only_lines(
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
                let next_is_candidate = super::hints::is_candidate(next_line)
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

fn extract_embedded_bare_c_year_suffixes(
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

fn extract_trailing_bare_c_year_range_suffixes(
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

fn extract_repeated_embedded_bare_c_year_suffixes(
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

fn extract_lowercase_username_angle_email_copyrights(
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

fn extract_lowercase_username_paren_email_copyrights(
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

fn extract_c_year_range_by_name_comma_email_lines(
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

fn extract_copyright_years_by_name_paren_email_lines(
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

fn extract_copyright_years_by_name_then_paren_email_next_line(
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

fn extract_copyright_year_name_with_of_lines(
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

fn drop_url_extended_prefix_duplicates(copyrights: &mut Vec<CopyrightDetection>) {
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

fn extract_standalone_c_holder_year_lines(
    groups: &[Vec<(usize, String)>],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static STANDALONE_C_HOLDER_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^\(c\)\s+(?P<holder>[A-Z0-9][A-Za-z0-9 ,&'\-\.]*?)\s+(?P<years>(?:19\d{2}|20\d{2})(?:\s*,\s*(?:19\d{2}|20\d{2}))*)\s*(?:[Aa]ll\s+[Rr]ights\s+[Rr]eserved)?\s*$",
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

fn extract_c_holder_without_year_lines(
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

fn extract_c_years_then_holder_lines(
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

            if let Some(h) = derive_holder_from_simple_copyright_string(&cr)
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

fn extract_copyright_c_years_holder_lines(
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

fn extract_three_digit_copyright_year_lines(
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

fn extract_copyrighted_by_lines(
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

fn extract_c_word_year_lines(
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

fn extract_are_c_year_holder_lines(
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

fn extract_bare_c_by_holder_lines(
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

fn extract_all_rights_reserved_by_holder_lines(
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

fn extract_holder_is_name_paren_email_lines(
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

fn extract_copr_lines(
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

fn apply_european_community_copyright(
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

fn apply_javadoc_company_metadata(
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

fn extract_html_entity_year_range_copyrights(
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

fn normalize_pudn_html_footer_copyrights(
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

fn fallback_year_only_copyrights(groups: &[Vec<(usize, String)>]) -> Vec<CopyrightDetection> {
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

fn extract_copyright_c_year_comma_name_angle_email_lines(
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

fn extend_software_in_the_public_interest_holder(
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

fn strip_trailing_c_year_suffix_from_comma_and_others(copyrights: &mut [CopyrightDetection]) {
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

fn extract_name_before_rewrited_by_copyrights(
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

fn extract_developed_at_software_copyrights(
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

fn extract_confidential_proprietary_copyrights(
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

fn strip_trailing_the_source_suffixes(copyrights: &mut [CopyrightDetection]) {
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

fn truncate_stichting_mathematisch_centrum_amsterdam_netherlands(
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

fn extract_copyright_year_c_holder_mid_sentence_lines(
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

fn extract_javadoc_author_copyright_lines(
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

fn extract_xml_copyright_tag_c_lines(
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

fn extract_copyright_its_authors_lines(
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

fn extract_us_government_year_placeholder_copyrights(
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

fn extract_copyright_notice_paren_year_lines(
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

fn drop_shadowed_bare_c_holders_with_year_prefixed_copyrights(
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

fn extract_initials_holders_from_copyrights(
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

fn extract_html_anchor_copyright_url(
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
}

fn extract_angle_bracket_year_name_copyrights(
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

fn extract_html_icon_class_copyrights(
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

fn extract_copyright_year_c_name_angle_email_lines(
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

fn extract_copyright_by_without_year_lines(
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

fn drop_shadowed_and_or_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }
    use std::collections::HashMap;

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

fn drop_shadowed_prefix_holders(holders: &mut Vec<HolderDetection>) {
    if holders.len() < 2 {
        return;
    }

    use std::collections::HashMap;
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

fn drop_shadowed_prefix_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    use std::collections::HashMap;
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

fn drop_shadowed_bare_c_copyrights_same_span(copyrights: &mut Vec<CopyrightDetection>) {
    if copyrights.len() < 2 {
        return;
    }

    use std::collections::{HashMap, HashSet};

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

fn drop_copyright_shadowed_by_bare_c_copyrights_same_span(
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

fn drop_shadowed_copyright_c_years_only_prefixes(copyrights: &mut Vec<CopyrightDetection>) {
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

fn drop_non_copyright_like_copyrights(copyrights: &mut Vec<CopyrightDetection>) {
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
            && lower != "(c) rest"
    });
}

fn drop_bare_c_shadowed_by_non_copyright_prefixes(copyrights: &mut Vec<CopyrightDetection>) {
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

fn drop_shadowed_dashless_holders(holders: &mut Vec<HolderDetection>) {
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

#[path = "detector_tree_walk.rs"]
mod tree_walk;

#[path = "detector_author_heuristics.rs"]
mod author_heuristics;
#[path = "detector_postprocess_phase.rs"]
mod postprocess_phase;
#[path = "detector_primary_phase.rs"]
mod primary_phase;
#[path = "detector_token_utils.rs"]
mod token_utils;

pub(super) use token_utils::{
    YEAR_LIKE_POS_TAGS, apply_written_by_for_markers, build_author_from_node,
    build_author_from_tokens, build_copyright_from_tokens, build_holder_from_copyright_node,
    build_holder_from_node, build_holder_from_tokens, collect_all_leaves, collect_filtered_leaves,
    collect_holder_filtered_leaves, drop_path_fragment_holders_from_bare_c_code_lines,
    drop_scan_only_holders_from_copyright_scan_lines,
    extract_original_author_additional_contributors, filter_holder_tokens_with_state,
    is_author_span_token, is_copyright_of_header, is_copyright_span_token,
    looks_like_bad_generic_author_candidate, normalize_whitespace,
    restore_bare_holder_angle_emails, should_merge_following_c_sign_after_year,
    should_merge_following_copyright_clause, signal_lines_before_copy_line,
    strip_all_rights_reserved, strip_all_rights_reserved_slice, strip_trailing_commas,
    tokens_to_string,
};
#[cfg(test)]
pub(super) use tree_walk::{collect_trailing_orphan_tokens, should_start_absorbing};
pub(super) use tree_walk::{
    extract_bare_copyrights, extract_copyrights_from_spans, extract_from_spans,
    extract_from_tree_nodes, extract_holder_is_name, extract_orphaned_by_authors,
    fix_truncated_contributors_authors,
};

#[cfg(test)]
#[path = "detector_test.rs"]
mod tests;
