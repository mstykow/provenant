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

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;

use super::candidates::collect_candidate_lines;
use super::detector_input_normalization::{
    maybe_expand_copyrighted_by_href_urls, normalize_split_angle_bracket_urls,
};
use super::lexer::get_tokens;
use super::line_tracking::{LineNumberIndex, PreparedLineCache};
use super::parser::{parse, parse_with_deadline};
#[cfg(test)]
use super::refiner::{refine_copyright, refine_holder_in_copyright_context};
use super::types::{AuthorDetection, CopyrightDetection, HolderDetection, PosTag, TreeLabel};
#[cfg(test)]
use super::types::{ParseNode, Token};

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

mod author_heuristics;
mod pattern_extract;
mod postprocess_phase;
mod postprocess_transforms;
mod primary_phase;
mod token_utils;
mod tree_walk;

use pattern_extract::{
    extend_software_in_the_public_interest_holder, fallback_year_only_copyrights,
};
use postprocess_transforms::{
    add_missing_holders_for_bare_c_name_year_suffixes, deadline_exceeded,
    dedupe_exact_span_authors, dedupe_exact_span_copyrights, dedupe_exact_span_holders,
    extend_authors_see_url_copyrights, extend_dash_obfuscated_email_suffixes,
    extend_leading_dash_suffixes, extend_multiline_copyright_c_no_year_names,
    extend_multiline_copyright_c_year_holder_continuations, extend_trailing_copy_year_suffixes,
    extend_w3c_registered_org_list_suffixes, refine_final_copyrights,
    restore_linux_foundation_copyrights_from_raw_lines,
};
use token_utils::{
    apply_written_by_for_markers, collect_all_leaves,
    drop_path_fragment_holders_from_bare_c_code_lines,
    drop_scan_only_holders_from_copyright_scan_lines,
    extract_original_author_additional_contributors,
};
#[cfg(test)]
use tree_walk::{collect_trailing_orphan_tokens, should_start_absorbing};
use tree_walk::{
    extract_bare_copyrights, extract_copyrights_from_spans, extract_from_spans,
    extract_from_tree_nodes, extract_holder_is_name, extract_orphaned_by_authors,
    fix_truncated_contributors_authors,
};

#[cfg(test)]
mod tests;
