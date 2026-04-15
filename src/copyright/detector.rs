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

#[path = "detector_postprocess_transforms.rs"]
mod postprocess_transforms;

#[path = "detector_pattern_extract.rs"]
mod pattern_extract;

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

pub(super) use pattern_extract::{
    apply_european_community_copyright, apply_javadoc_company_metadata,
    drop_arch_floppy_h_bare_1995, drop_bare_c_shadowed_by_non_copyright_prefixes,
    drop_batman_adv_contributors_copyright, drop_copyright_shadowed_by_bare_c_copyrights_same_span,
    drop_non_copyright_like_copyrights, drop_shadowed_and_or_holders,
    drop_shadowed_bare_c_copyrights_same_span,
    drop_shadowed_bare_c_holders_with_year_prefixed_copyrights,
    drop_shadowed_copyright_c_years_only_prefixes, drop_shadowed_dashless_holders,
    drop_shadowed_prefix_copyrights, drop_shadowed_prefix_holders,
    drop_url_extended_prefix_duplicates, extend_software_in_the_public_interest_holder,
    extract_added_the_copyright_year_for_lines, extract_all_rights_reserved_by_holder_lines,
    extract_angle_bracket_year_name_copyrights, extract_are_c_year_holder_lines,
    extract_bare_c_by_holder_lines, extract_c_holder_without_year_lines, extract_c_word_year_lines,
    extract_c_year_range_by_name_comma_email_lines, extract_c_years_then_holder_lines,
    extract_changelog_timestamp_copyrights_from_content, extract_common_year_only_lines,
    extract_confidential_proprietary_copyrights, extract_copr_lines,
    extract_copyright_by_without_year_lines, extract_copyright_c_year_comma_name_angle_email_lines,
    extract_copyright_c_years_holder_lines, extract_copyright_its_authors_lines,
    extract_copyright_notice_paren_year_lines, extract_copyright_year_c_holder_mid_sentence_lines,
    extract_copyright_year_c_name_angle_email_lines, extract_copyright_year_name_with_of_lines,
    extract_copyright_years_by_name_paren_email_lines,
    extract_copyright_years_by_name_then_paren_email_next_line, extract_copyrighted_by_lines,
    extract_developed_at_software_copyrights, extract_embedded_bare_c_year_suffixes,
    extract_glide_3dfx_copyright_notice, extract_holder_is_name_paren_email_lines,
    extract_html_anchor_copyright_url, extract_html_entity_year_range_copyrights,
    extract_html_icon_class_copyrights, extract_html_meta_name_copyright_content,
    extract_initials_holders_from_copyrights, extract_javadoc_author_copyright_lines,
    extract_lowercase_username_angle_email_copyrights,
    extract_lowercase_username_paren_email_copyrights, extract_name_before_rewrited_by_copyrights,
    extract_repeated_embedded_bare_c_year_suffixes, extract_spdx_filecopyrighttext_c_without_year,
    extract_standalone_c_holder_year_lines, extract_three_digit_copyright_year_lines,
    extract_trailing_bare_c_year_range_suffixes, extract_us_government_year_placeholder_copyrights,
    extract_xml_copyright_tag_c_lines, fallback_year_only_copyrights, is_lppl_license_document,
    normalize_pudn_html_footer_copyrights, strip_trailing_c_year_suffix_from_comma_and_others,
    strip_trailing_the_source_suffixes,
    truncate_stichting_mathematisch_centrum_amsterdam_netherlands,
};
pub(super) use postprocess_transforms::{
    add_at_affiliation_short_variants, add_at_domain_variants_for_short_net_angle_emails,
    add_bare_email_variants_for_escaped_angle_lines, add_but_suffix_short_variants,
    add_confidential_short_variants_late, add_embedded_copyright_clause_variants,
    add_first_angle_email_only_variants, add_found_at_short_variants,
    add_from_url_parenthetical_copyright_variants, add_intel_and_sun_non_portions_variants,
    add_karlsruhe_university_short_variants, add_missing_copyrights_for_holder_lines_with_emails,
    add_missing_holder_from_single_copyright, add_missing_holders_for_bare_c_name_year_suffixes,
    add_missing_holders_for_debian_modifications,
    add_missing_holders_from_email_bearing_copyrights, add_modify_suffix_holders,
    add_pipe_read_parenthetical_variants, add_short_svox_baslerstr_variants,
    apply_openoffice_org_report_builder_bin_normalizations, deadline_exceeded,
    dedupe_exact_span_authors, dedupe_exact_span_copyrights, dedupe_exact_span_holders,
    derive_holder_from_simple_copyright_string, drop_combined_period_holders,
    drop_combined_semicolon_shadowed_copyrights,
    drop_comma_holders_shadowed_by_space_version_same_span, drop_copyright_like_holders,
    drop_created_by_camelcase_identifier_authors, drop_from_source_attribution_copyrights,
    drop_json_description_metadata_copyrights_and_holders,
    drop_obfuscated_email_year_only_copyrights, drop_shadowed_acronym_extended_holders,
    drop_shadowed_acronym_location_suffix_copyrights_same_span,
    drop_shadowed_angle_email_prefix_copyrights_same_span, drop_shadowed_c_sign_variants,
    drop_shadowed_email_org_location_suffixes_same_span,
    drop_shadowed_for_clause_holders_with_email_copyrights,
    drop_shadowed_linux_foundation_holder_copyrights_same_line,
    drop_shadowed_multiline_prefix_copyrights, drop_shadowed_multiline_prefix_holders,
    drop_shadowed_plain_email_prefix_copyrights_same_span,
    drop_shadowed_prefix_bare_c_copyrights_same_span,
    drop_shadowed_quote_before_email_variants_same_span,
    drop_shadowed_year_only_copyright_prefixes_same_start_line,
    drop_shadowed_year_prefixed_holders,
    drop_single_line_copyrights_shadowed_by_multiline_same_start,
    drop_static_char_string_copyrights, drop_symbol_year_only_copyrights,
    drop_trailing_software_line_from_holders, drop_url_embedded_c_symbol_false_positive_holders,
    drop_url_embedded_suffix_variants_same_span, drop_wider_duplicate_holder_spans,
    drop_year_only_copyrights_shadowed_by_previous_software_copyright_line,
    expand_portions_copyright_variants, expand_year_only_copyrights_with_by_name_prefix,
    expand_year_only_copyrights_with_read_the_suffix, extend_authors_see_url_copyrights,
    extend_bare_c_year_detections_to_line_end_for_multi_c_lines,
    extend_copyrights_with_authors_blocks,
    extend_copyrights_with_following_all_rights_reserved_line,
    extend_copyrights_with_next_line_parenthesized_obfuscated_email,
    extend_dash_obfuscated_email_suffixes, extend_inline_obfuscated_angle_email_suffixes,
    extend_leading_dash_suffixes, extend_multiline_copyright_c_no_year_names,
    extend_multiline_copyright_c_year_holder_continuations, extend_trailing_copy_year_suffixes,
    extend_w3c_registered_org_list_suffixes, extend_year_only_copyrights_with_trailing_text,
    extract_following_authors_holders, extract_licensed_material_of_company_bare_c_year_lines,
    extract_line_ending_copyright_then_by_holder,
    extract_midline_c_year_holder_with_leading_acronym, extract_mso_document_properties_copyrights,
    extract_question_mark_year_copyrights, fix_n_tty_linus_torvalds_written_by_clause,
    fix_shm_inline_copyrights, fix_sundry_contributors_truncation, merge_axis_ethereal_suffix,
    merge_debugging390_best_viewed_suffix, merge_freebird_c_inc_urls, merge_fsf_gdb_notice_lines,
    merge_implemented_by_lines, merge_kirkwood_converted_to,
    merge_multiline_copyrighted_by_with_trailing_copyright_clause,
    merge_multiline_obfuscated_name_year_copyright_pairs,
    merge_multiline_person_year_copyright_continuations,
    merge_year_only_copyrights_with_following_author_colon_lines,
    normalize_company_suffix_period_holder_variants,
    normalize_french_support_disclaimer_copyrights, recover_template_literal_year_range_copyrights,
    refine_final_copyrights, replace_holders_with_embedded_c_year_markers,
    restore_linux_foundation_copyrights_from_raw_lines,
    restore_url_slash_before_closing_paren_from_raw_lines, split_embedded_copyright_detections,
    split_multiline_holder_lists_from_copyright_email_sequences, split_reworked_by_suffixes,
    split_written_by_copyrights_into_holder_prefixed_clauses,
    strip_inc_suffix_from_holders_for_today_year_copyrights,
    strip_lone_obfuscated_angle_email_user_tokens, truncate_lonely_svox_baslerstr_address,
};
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
