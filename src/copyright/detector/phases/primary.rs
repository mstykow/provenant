// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::copyright::line_tracking::{LineNumberIndex, PreparedLines};
use crate::copyright::types::{CopyrightDetection, HolderDetection};

use super::super::seen_text::SeenTextSets;

#[allow(clippy::too_many_arguments)]
fn run_pre_pattern_repairs(
    content: &str,
    line_number_index: &LineNumberIndex,
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    seen: &mut SeenTextSets,
) {
    let (mut new_c, mut new_h) =
        super::super::postprocess_transforms::extract_midline_c_year_holder_with_leading_acronym(
            prepared_cache,
        );
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::extend_copyrights_with_authors_blocks(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::extend_year_only_copyrights_with_trailing_text(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::merge_year_only_copyrights_with_following_author_colon_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::extract_licensed_material_of_company_bare_c_year_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    super::super::pattern_extract::drop_shadowed_and_or_holders(holders);
    super::super::pattern_extract::drop_shadowed_prefix_holders(holders);
    super::super::postprocess_transforms::drop_shadowed_acronym_extended_holders(holders);
    super::super::pattern_extract::drop_shadowed_prefix_copyrights(copyrights);
    super::super::postprocess_transforms::drop_shadowed_c_sign_variants(copyrights);
    super::super::postprocess_transforms::drop_shadowed_year_prefixed_holders(holders);
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::merge_multiline_person_year_copyright_continuations(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::extract_mso_document_properties_copyrights(
        content, copyrights, holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    super::super::postprocess_transforms::expand_portions_copyright_variants(copyrights);
    seen.rebuild_copyrights_from(copyrights);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::expand_year_only_copyrights_with_by_name_prefix(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::expand_year_only_copyrights_with_read_the_suffix(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::postprocess_transforms::merge_multiline_obfuscated_name_year_copyright_pairs(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let new_h = super::super::postprocess_transforms::add_modify_suffix_holders(
        prepared_cache,
        &holders[..],
    );
    holders.extend(new_h);

    super::super::postprocess_transforms::dedupe_exact_span_holders(holders);

    super::super::postprocess_transforms::drop_shadowed_prefix_bare_c_copyrights_same_span(
        copyrights,
    );
    seen.rebuild_copyrights_from(copyrights);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::apply_javadoc_company_metadata(
        content,
        line_number_index,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::apply_european_community_copyright(
        content,
        line_number_index,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let c_before = copyrights.len();
    super::super::pattern_extract::extract_html_entity_year_range_copyrights(
        content,
        line_number_index,
        copyrights,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
}

#[allow(clippy::too_many_arguments)]
fn run_group_based_extractions(
    content: &str,
    groups: &[Vec<(usize, String)>],
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    seen: &mut SeenTextSets,
) {
    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::extract_copr_lines(groups, copyrights, holders);
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_standalone_c_holder_year_lines(groups, copyrights);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_c_years_then_holder_lines(
        groups, copyrights, holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_copyright_c_years_holder_lines(
        groups, copyrights, holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) =
        super::super::pattern_extract::extract_versioned_project_c_holder_banner_lines(
            groups, copyrights, holders,
        );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_c_holder_without_year_lines(content, groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_three_digit_copyright_year_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_copyrighted_by_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_c_word_year_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_are_c_year_holder_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_bare_c_by_holder_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_all_rights_reserved_by_holder_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let mut new_c =
        super::super::pattern_extract::extract_trailing_bare_c_year_range_suffixes(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    copyrights.extend(new_c);

    let mut new_c = super::super::pattern_extract::extract_common_year_only_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    copyrights.extend(new_c);

    let mut new_c =
        super::super::pattern_extract::extract_embedded_bare_c_year_suffixes(groups, copyrights);
    seen.dedup_new_copyrights(&mut new_c, 0);
    copyrights.extend(new_c);

    let mut new_c =
        super::super::pattern_extract::extract_repeated_embedded_bare_c_year_suffixes(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    copyrights.extend(new_c);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_lowercase_username_angle_email_copyrights(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_lowercase_username_paren_email_copyrights(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_c_year_comma_name_angle_email_lines(
            groups, copyrights,
        );
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_c_year_range_by_name_comma_email_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::extract_copyright_years_by_name_then_paren_email_next_line(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::extract_copyright_years_by_name_paren_email_lines(
        groups, copyrights, holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);
    seen.rebuild_copyrights_from(copyrights);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_year_name_with_of_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);
}

#[allow(clippy::too_many_arguments)]
fn run_content_and_markup_extractions(
    content: &str,
    groups: &[Vec<(usize, String)>],
    line_number_index: &LineNumberIndex,
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    seen: &mut SeenTextSets,
) {
    let c_before = copyrights.len();
    let h_before = holders.len();
    let (extracted_c, extracted_h) =
        super::super::postprocess_transforms::extract_line_ending_copyright_then_by_holder(
            prepared_cache,
            holders,
        );
    copyrights.extend(extracted_c);
    holders.extend(extracted_h);
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let (mut new_c, new_h) =
        super::super::pattern_extract::extract_changelog_timestamp_copyrights_from_content(
            content, holders,
        );
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    super::super::pattern_extract::drop_url_extended_prefix_duplicates(copyrights);

    super::super::postprocess_transforms::drop_obfuscated_email_year_only_copyrights(
        content, copyrights, holders,
    );
    seen.rebuild_copyrights_from(copyrights);
    seen.rebuild_holders_from(holders);

    let mut new_c = super::super::pattern_extract::extract_glide_3dfx_copyright_notice(content);
    seen.dedup_new_copyrights(&mut new_c, 0);
    copyrights.extend(new_c);

    let (mut new_c, new_h) =
        super::super::pattern_extract::extract_spdx_filecopyrighttext_c_without_year(
            content, holders,
        );
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, new_h) =
        super::super::pattern_extract::extract_html_meta_name_copyright_content(content, holders);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, new_h) =
        super::super::pattern_extract::extract_markup_copyright_attributes(content, holders);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) = super::super::pattern_extract::extract_html_anchor_copyright_url(
        content,
        line_number_index,
    );
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::normalize_pudn_html_footer_copyrights(
        content,
        line_number_index,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let (mut new_c, new_h) =
        super::super::pattern_extract::extract_angle_bracket_year_name_copyrights(
            groups, copyrights, holders,
        );
    seen.rebuild_copyrights_from(copyrights);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::extract_html_icon_class_copyrights(
        content,
        line_number_index,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let (mut new_c, new_h) =
        super::super::pattern_extract::extract_added_the_copyright_year_for_lines(
            prepared_cache,
            holders,
        );
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_by_without_year_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_notice_paren_year_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_year_c_holder_mid_sentence_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_javadoc_author_copyright_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let c_before = copyrights.len();
    let h_before = holders.len();
    super::super::pattern_extract::extract_xml_copyright_tag_c_lines(
        content,
        line_number_index,
        copyrights,
        holders,
    );
    seen.dedup_new_copyrights(copyrights, c_before);
    seen.dedup_new_holders(holders, h_before);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_its_authors_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_copyright_year_c_name_angle_email_lines(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (mut new_c, mut new_h) =
        super::super::pattern_extract::extract_us_government_year_placeholder_copyrights(groups);
    seen.dedup_new_copyrights(&mut new_c, 0);
    seen.dedup_new_holders(&mut new_h, 0);
    copyrights.extend(new_c);
    holders.extend(new_h);

    let (new_c, new_h) = super::super::pattern_extract::extract_holder_is_name_paren_email_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    seen.register_copyrights(&new_c);
    seen.register_holders(&new_h);
    copyrights.extend(new_c);
    holders.extend(new_h);
}

#[allow(clippy::too_many_arguments)]
pub(in super::super) fn run_phase_primary_extractions(
    content: &str,
    groups: &[Vec<(usize, String)>],
    line_number_index: &LineNumberIndex,
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    seen: &mut SeenTextSets,
) {
    run_pre_pattern_repairs(
        content,
        line_number_index,
        prepared_cache,
        copyrights,
        holders,
        seen,
    );
    run_group_based_extractions(content, groups, prepared_cache, copyrights, holders, seen);
    run_content_and_markup_extractions(
        content,
        groups,
        line_number_index,
        prepared_cache,
        copyrights,
        holders,
        seen,
    );
}
