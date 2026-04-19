use crate::copyright::line_tracking::PreparedLineCache;
use crate::copyright::types::{AuthorDetection, CopyrightDetection, HolderDetection};

pub(super) fn run_phase_postprocess(
    content: &str,
    raw_lines: &[&str],
    prepared_cache: &mut PreparedLineCache<'_>,
    did_expand_href: bool,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    authors: &mut Vec<AuthorDetection>,
) {
    super::postprocess_transforms::extract_question_mark_year_copyrights(
        prepared_cache,
        copyrights,
        holders,
    );

    if super::pattern_extract::is_lppl_license_document(content) {
        holders.retain(|h| h.holder != "M. Y.");
    }

    super::pattern_extract::drop_arch_floppy_h_bare_1995(content, copyrights);
    super::pattern_extract::drop_batman_adv_contributors_copyright(content, copyrights, holders);

    super::postprocess_transforms::split_embedded_copyright_detections(copyrights, holders);
    super::postprocess_transforms::add_missing_holders_from_email_bearing_copyrights(
        copyrights, holders,
    );
    super::postprocess_transforms::extend_bare_c_year_detections_to_line_end_for_multi_c_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    super::postprocess_transforms::replace_holders_with_embedded_c_year_markers(
        copyrights, holders,
    );
    super::postprocess_transforms::add_missing_holders_for_debian_modifications(
        content, copyrights, holders,
    );
    super::postprocess_transforms::fix_sundry_contributors_truncation(
        prepared_cache,
        copyrights,
        holders,
    );
    super::token_utils::restore_bare_holder_angle_emails(copyrights, holders);
    super::postprocess_transforms::drop_trailing_software_line_from_holders(
        prepared_cache,
        holders,
    );
    super::postprocess_transforms::drop_url_embedded_c_symbol_false_positive_holders(
        content, holders,
    );
    super::postprocess_transforms::recover_template_literal_year_range_copyrights(
        content, copyrights, holders,
    );

    super::author_heuristics::extract_markup_authors(content, authors);
    super::author_heuristics::extract_rst_field_authors(prepared_cache, authors);
    super::author_heuristics::extract_toml_author_assignment_authors(raw_lines, authors);
    super::author_heuristics::merge_metadata_author_and_email_lines(prepared_cache, authors);
    super::author_heuristics::extract_debian_maintainer_authors(prepared_cache, authors);
    super::author_heuristics::extract_maintainers_label_authors(prepared_cache, authors);
    super::author_heuristics::extract_maintained_by_authors(prepared_cache, authors);
    super::author_heuristics::extract_package_comment_named_authors(prepared_cache, authors);
    super::author_heuristics::extract_created_by_project_author(prepared_cache, authors);
    super::author_heuristics::extract_created_by_authors(prepared_cache, authors);
    super::author_heuristics::extract_written_by_comma_and_copyright_authors(
        prepared_cache,
        authors,
    );
    super::author_heuristics::extract_multiline_written_by_author_blocks(prepared_cache, authors);
    super::author_heuristics::extract_dash_bullet_attribution_authors(prepared_cache, authors);
    super::author_heuristics::extract_json_excerpt_developed_by_authors(content, authors);
    super::author_heuristics::extract_modified_portion_developed_by_authors(content, authors);
    super::author_heuristics::extract_was_developed_by_author_blocks(prepared_cache, authors);
    super::author_heuristics::extract_developed_by_sentence_authors(prepared_cache, authors);
    super::author_heuristics::extract_developed_by_phrase_authors(prepared_cache, authors);
    super::author_heuristics::extract_developed_by_contributors_authors(prepared_cache, authors);
    super::author_heuristics::extract_with_additional_hacking_by_authors(prepared_cache, authors);
    super::author_heuristics::extract_parenthesized_inline_by_authors(raw_lines, authors);
    super::author_heuristics::extract_developed_and_created_by_authors(prepared_cache, authors);
    super::author_heuristics::extract_author_colon_blocks(prepared_cache, authors);
    super::author_heuristics::extract_module_author_macros(content, copyrights, holders, authors);
    super::author_heuristics::extract_code_written_by_author_blocks(prepared_cache, authors);
    super::author_heuristics::extract_converted_to_by_authors(prepared_cache, authors);
    super::author_heuristics::extract_various_bugfixes_and_enhancements_by_authors(
        prepared_cache,
        authors,
    );
    super::author_heuristics::extract_dense_name_email_author_lists(prepared_cache, authors);
    super::author_heuristics::drop_author_colon_lines_absorbed_into_year_only_copyrights(
        prepared_cache,
        copyrights,
        authors,
    );
    super::author_heuristics::drop_authors_embedded_in_copyrights(copyrights, authors);
    super::author_heuristics::drop_authors_from_copyright_by_lines(prepared_cache, authors);
    super::author_heuristics::drop_merged_dash_bullet_attribution_authors(authors);
    super::postprocess_transforms::drop_created_by_camelcase_identifier_authors(
        prepared_cache,
        authors,
    );
    super::author_heuristics::drop_shadowed_compound_email_authors(authors);
    super::author_heuristics::drop_shadowed_prefix_authors(authors);

    super::postprocess_transforms::merge_implemented_by_lines(
        prepared_cache,
        copyrights,
        holders,
        authors,
    );
    super::postprocess_transforms::split_written_by_copyrights_into_holder_prefixed_clauses(
        prepared_cache,
        copyrights,
        holders,
        authors,
    );
    super::author_heuristics::drop_written_by_authors_preceded_by_copyright(
        prepared_cache,
        authors,
    );
    super::author_heuristics::drop_ref_markup_authors(authors);
    super::author_heuristics::extract_json_author_object_authors(raw_lines, authors);
    super::author_heuristics::normalize_json_blob_authors(raw_lines, authors);

    super::postprocess_transforms::extract_following_authors_holders(
        raw_lines,
        prepared_cache,
        authors,
    );
    super::author_heuristics::drop_json_code_example_authors(raw_lines, authors);

    super::postprocess_transforms::merge_multiline_copyrighted_by_with_trailing_copyright_clause(
        did_expand_href,
        content,
        copyrights,
    );
    super::postprocess_transforms::extend_copyrights_with_next_line_parenthesized_obfuscated_email(
        prepared_cache,
        copyrights,
    );
    super::postprocess_transforms::extend_copyrights_with_following_all_rights_reserved_line(
        raw_lines, copyrights,
    );

    super::postprocess_transforms::drop_symbol_year_only_copyrights(content, copyrights);

    super::postprocess_transforms::drop_from_source_attribution_copyrights(copyrights, holders);

    super::postprocess_transforms::fix_shm_inline_copyrights(prepared_cache, copyrights, holders);
    super::postprocess_transforms::fix_n_tty_linus_torvalds_written_by_clause(
        content, copyrights, holders,
    );

    super::postprocess_transforms::merge_freebird_c_inc_urls(prepared_cache, copyrights, holders);
    super::postprocess_transforms::merge_debugging390_best_viewed_suffix(
        prepared_cache,
        copyrights,
        holders,
    );
    super::postprocess_transforms::merge_fsf_gdb_notice_lines(prepared_cache, copyrights, holders);
    super::postprocess_transforms::merge_axis_ethereal_suffix(prepared_cache, copyrights, holders);
    super::postprocess_transforms::merge_kirkwood_converted_to(prepared_cache, copyrights, holders);
    super::postprocess_transforms::split_reworked_by_suffixes(
        content, copyrights, holders, authors,
    );
    super::postprocess_transforms::drop_static_char_string_copyrights(content, copyrights, holders);
    super::postprocess_transforms::drop_combined_period_holders(holders);
    super::pattern_extract::drop_shadowed_prefix_holders(holders);
    super::pattern_extract::strip_trailing_c_year_suffix_from_comma_and_others(copyrights);
    super::pattern_extract::drop_bare_c_shadowed_by_non_copyright_prefixes(copyrights);
    super::pattern_extract::extract_name_before_rewrited_by_copyrights(
        prepared_cache,
        copyrights,
        holders,
    );
    super::pattern_extract::extract_developed_at_software_copyrights(
        prepared_cache,
        copyrights,
        holders,
    );
    super::pattern_extract::extract_confidential_proprietary_copyrights(
        prepared_cache,
        copyrights,
        holders,
    );
    super::pattern_extract::drop_shadowed_bare_c_holders_with_year_prefixed_copyrights(
        copyrights, holders,
    );
    super::pattern_extract::drop_shadowed_dashless_holders(holders);
    super::pattern_extract::extract_initials_holders_from_copyrights(copyrights, holders);
    super::pattern_extract::strip_trailing_the_source_suffixes(copyrights);
    super::pattern_extract::truncate_stichting_mathematisch_centrum_amsterdam_netherlands(
        copyrights, holders,
    );

    super::postprocess_transforms::strip_inc_suffix_from_holders_for_today_year_copyrights(
        copyrights, holders,
    );

    super::postprocess_transforms::apply_openoffice_org_report_builder_bin_normalizations(
        content, copyrights, holders,
    );

    super::pattern_extract::drop_shadowed_bare_c_copyrights_same_span(copyrights);

    super::pattern_extract::drop_copyright_shadowed_by_bare_c_copyrights_same_span(copyrights);
    super::pattern_extract::drop_shadowed_copyright_c_years_only_prefixes(copyrights);

    super::pattern_extract::drop_non_copyright_like_copyrights(copyrights);

    super::postprocess_transforms::drop_wider_duplicate_holder_spans(holders);

    super::postprocess_transforms::drop_shadowed_multiline_prefix_copyrights(copyrights);
    super::postprocess_transforms::drop_shadowed_multiline_prefix_holders(holders);

    super::pattern_extract::drop_shadowed_prefix_copyrights(copyrights);
    super::postprocess_transforms::drop_combined_semicolon_shadowed_copyrights(copyrights);

    super::postprocess_transforms::drop_shadowed_for_clause_holders_with_email_copyrights(
        copyrights, holders,
    );

    super::postprocess_transforms::drop_shadowed_c_sign_variants(copyrights);
    super::postprocess_transforms::drop_shadowed_year_prefixed_holders(holders);

    super::postprocess_transforms::truncate_lonely_svox_baslerstr_address(copyrights, holders);
    super::postprocess_transforms::add_short_svox_baslerstr_variants(copyrights, holders);

    super::postprocess_transforms::drop_shadowed_year_only_copyright_prefixes_same_start_line(
        copyrights,
    );
    super::postprocess_transforms::drop_year_only_copyrights_shadowed_by_previous_software_copyright_line(
        raw_lines,
        prepared_cache,
        copyrights,
    );

    super::postprocess_transforms::add_embedded_copyright_clause_variants(copyrights);
    super::postprocess_transforms::add_found_at_short_variants(copyrights, holders);
    super::postprocess_transforms::drop_shadowed_linux_foundation_holder_copyrights_same_line(
        copyrights,
    );
    super::postprocess_transforms::add_bare_email_variants_for_escaped_angle_lines(
        raw_lines, copyrights,
    );
    super::postprocess_transforms::drop_comma_holders_shadowed_by_space_version_same_span(holders);
    super::postprocess_transforms::normalize_company_suffix_period_holder_variants(holders);
    super::postprocess_transforms::add_confidential_short_variants_late(copyrights, holders);
    super::postprocess_transforms::add_karlsruhe_university_short_variants(copyrights, holders);
    super::postprocess_transforms::add_intel_and_sun_non_portions_variants(
        prepared_cache,
        copyrights,
    );
    super::postprocess_transforms::add_pipe_read_parenthetical_variants(prepared_cache, copyrights);
    super::postprocess_transforms::add_from_url_parenthetical_copyright_variants(
        prepared_cache,
        copyrights,
    );
    super::postprocess_transforms::add_at_affiliation_short_variants(copyrights, holders);
    super::postprocess_transforms::add_but_suffix_short_variants(copyrights);
    super::postprocess_transforms::add_missing_copyrights_for_holder_lines_with_emails(
        prepared_cache,
        copyrights,
        holders,
    );
    super::postprocess_transforms::extend_inline_obfuscated_angle_email_suffixes(
        prepared_cache,
        copyrights,
    );
    super::postprocess_transforms::strip_lone_obfuscated_angle_email_user_tokens(
        raw_lines, copyrights, holders,
    );
    super::postprocess_transforms::add_at_domain_variants_for_short_net_angle_emails(
        prepared_cache,
        copyrights,
    );
    super::postprocess_transforms::normalize_french_support_disclaimer_copyrights(
        copyrights, holders,
    );
    super::postprocess_transforms::drop_shadowed_email_org_location_suffixes_same_span(
        copyrights, holders,
    );
    super::postprocess_transforms::drop_shadowed_plain_email_prefix_copyrights_same_span(
        copyrights,
    );
    super::postprocess_transforms::drop_single_line_copyrights_shadowed_by_multiline_same_start(
        copyrights,
    );
    super::postprocess_transforms::restore_url_slash_before_closing_paren_from_raw_lines(
        raw_lines, copyrights,
    );
    super::postprocess_transforms::add_missing_holders_from_preceding_name_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    super::postprocess_transforms::add_first_angle_email_only_variants(copyrights);
    super::postprocess_transforms::drop_shadowed_angle_email_prefix_copyrights_same_span(
        copyrights,
    );
    super::postprocess_transforms::drop_shadowed_quote_before_email_variants_same_span(copyrights);
    super::postprocess_transforms::drop_url_embedded_suffix_variants_same_span(copyrights, holders);
    super::postprocess_transforms::add_missing_holder_from_single_copyright(copyrights, holders);

    super::postprocess_transforms::drop_shadowed_acronym_location_suffix_copyrights_same_span(
        copyrights,
    );
    super::postprocess_transforms::split_multiline_holder_lists_from_copyright_email_sequences(
        copyrights, holders,
    );
    super::postprocess_transforms::drop_json_description_metadata_copyrights_and_holders(
        raw_lines, copyrights, holders,
    );
    super::postprocess_transforms::drop_copyright_like_holders(holders);
}
