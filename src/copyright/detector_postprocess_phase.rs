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
    super::extract_question_mark_year_copyrights(prepared_cache, copyrights, holders);

    if super::is_lppl_license_document(content) {
        holders.retain(|h| h.holder != "M. Y.");
    }

    super::drop_arch_floppy_h_bare_1995(content, copyrights);
    super::drop_batman_adv_contributors_copyright(content, copyrights, holders);

    super::split_embedded_copyright_detections(copyrights, holders);
    super::extend_bare_c_year_detections_to_line_end_for_multi_c_lines(
        prepared_cache,
        copyrights,
        holders,
    );
    super::replace_holders_with_embedded_c_year_markers(copyrights, holders);
    super::add_missing_holders_for_debian_modifications(content, copyrights, holders);
    super::fix_sundry_contributors_truncation(prepared_cache, copyrights, holders);
    super::restore_bare_holder_angle_emails(copyrights, holders);
    super::drop_trailing_software_line_from_holders(prepared_cache, holders);
    super::drop_url_embedded_c_symbol_false_positive_holders(content, holders);
    super::recover_template_literal_year_range_copyrights(content, copyrights, holders);

    super::author_heuristics::extract_markup_authors(content, authors);
    super::author_heuristics::extract_rst_field_authors(prepared_cache, authors);
    super::author_heuristics::merge_metadata_author_and_email_lines(prepared_cache, authors);
    super::author_heuristics::extract_debian_maintainer_authors(prepared_cache, authors);
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
    super::author_heuristics::extract_with_additional_hacking_by_authors(prepared_cache, authors);
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
    super::author_heuristics::drop_authors_embedded_in_copyrights(copyrights, authors);
    super::author_heuristics::drop_merged_dash_bullet_attribution_authors(authors);
    super::drop_created_by_camelcase_identifier_authors(prepared_cache, authors);
    super::author_heuristics::drop_shadowed_prefix_authors(authors);
    super::author_heuristics::drop_comedi_ds_status_devices_authors(content, copyrights, authors);

    super::merge_implemented_by_lines(prepared_cache, copyrights, holders, authors);
    super::split_written_by_copyrights_into_holder_prefixed_clauses(
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
    super::author_heuristics::normalize_json_blob_authors(raw_lines, authors);

    super::extract_following_authors_holders(raw_lines, prepared_cache, holders);

    super::merge_multiline_copyrighted_by_with_trailing_copyright_clause(
        did_expand_href,
        content,
        copyrights,
    );
    super::extend_copyrights_with_next_line_parenthesized_obfuscated_email(
        prepared_cache,
        copyrights,
    );
    super::extend_copyrights_with_following_all_rights_reserved_line(raw_lines, copyrights);

    super::drop_symbol_year_only_copyrights(content, copyrights);

    super::drop_from_source_attribution_copyrights(copyrights, holders);

    super::fix_shm_inline_copyrights(prepared_cache, copyrights, holders);
    super::fix_n_tty_linus_torvalds_written_by_clause(content, copyrights, holders);

    super::merge_freebird_c_inc_urls(prepared_cache, copyrights, holders);
    super::merge_debugging390_best_viewed_suffix(prepared_cache, copyrights, holders);
    super::merge_fsf_gdb_notice_lines(prepared_cache, copyrights, holders);
    super::merge_axis_ethereal_suffix(prepared_cache, copyrights, holders);
    super::merge_kirkwood_converted_to(prepared_cache, copyrights, holders);
    super::split_reworked_by_suffixes(content, copyrights, holders, authors);
    super::drop_static_char_string_copyrights(content, copyrights, holders);
    super::drop_combined_period_holders(holders);
    super::drop_shadowed_prefix_holders(holders);
    super::strip_trailing_c_year_suffix_from_comma_and_others(copyrights);
    super::drop_bare_c_shadowed_by_non_copyright_prefixes(copyrights);
    super::extract_name_before_rewrited_by_copyrights(prepared_cache, copyrights, holders);
    super::extract_developed_at_software_copyrights(prepared_cache, copyrights, holders);
    super::extract_confidential_proprietary_copyrights(prepared_cache, copyrights, holders);
    super::drop_shadowed_bare_c_holders_with_year_prefixed_copyrights(copyrights, holders);
    super::drop_shadowed_dashless_holders(holders);
    super::extract_initials_holders_from_copyrights(copyrights, holders);
    super::strip_trailing_the_source_suffixes(copyrights);
    super::truncate_stichting_mathematisch_centrum_amsterdam_netherlands(copyrights, holders);

    super::strip_inc_suffix_from_holders_for_today_year_copyrights(copyrights, holders);

    super::apply_openoffice_org_report_builder_bin_normalizations(content, copyrights, holders);

    super::drop_shadowed_bare_c_copyrights_same_span(copyrights);

    super::drop_copyright_shadowed_by_bare_c_copyrights_same_span(copyrights);
    super::drop_shadowed_copyright_c_years_only_prefixes(copyrights);

    super::drop_non_copyright_like_copyrights(copyrights);

    super::drop_wider_duplicate_holder_spans(holders);

    super::drop_shadowed_multiline_prefix_copyrights(copyrights);
    super::drop_shadowed_multiline_prefix_holders(holders);

    super::drop_shadowed_prefix_copyrights(copyrights);
    super::drop_combined_semicolon_shadowed_copyrights(copyrights);

    super::drop_shadowed_for_clause_holders_with_email_copyrights(copyrights, holders);

    super::drop_shadowed_c_sign_variants(copyrights);
    super::drop_shadowed_year_prefixed_holders(holders);

    super::truncate_lonely_svox_baslerstr_address(copyrights, holders);
    super::add_short_svox_baslerstr_variants(copyrights, holders);

    super::drop_shadowed_year_only_copyright_prefixes_same_start_line(copyrights);
    super::drop_year_only_copyrights_shadowed_by_previous_software_copyright_line(
        raw_lines,
        prepared_cache,
        copyrights,
    );

    super::add_embedded_copyright_clause_variants(copyrights);
    super::add_found_at_short_variants(copyrights, holders);
    super::drop_shadowed_linux_foundation_holder_copyrights_same_line(copyrights);
    super::add_bare_email_variants_for_escaped_angle_lines(raw_lines, copyrights);
    super::drop_comma_holders_shadowed_by_space_version_same_span(holders);
    super::normalize_company_suffix_period_holder_variants(holders);
    super::add_confidential_short_variants_late(copyrights, holders);
    super::add_karlsruhe_university_short_variants(copyrights, holders);
    super::add_intel_and_sun_non_portions_variants(prepared_cache, copyrights);
    super::add_pipe_read_parenthetical_variants(prepared_cache, copyrights);
    super::add_from_url_parenthetical_copyright_variants(prepared_cache, copyrights);
    super::add_at_affiliation_short_variants(copyrights, holders);
    super::add_but_suffix_short_variants(copyrights);
    super::add_missing_copyrights_for_holder_lines_with_emails(prepared_cache, copyrights, holders);
    super::extend_inline_obfuscated_angle_email_suffixes(prepared_cache, copyrights);
    super::strip_lone_obfuscated_angle_email_user_tokens(raw_lines, copyrights, holders);
    super::add_at_domain_variants_for_short_net_angle_emails(prepared_cache, copyrights);
    super::normalize_french_support_disclaimer_copyrights(copyrights, holders);
    super::drop_shadowed_email_org_location_suffixes_same_span(copyrights, holders);
    super::drop_shadowed_plain_email_prefix_copyrights_same_span(copyrights);
    super::drop_single_line_copyrights_shadowed_by_multiline_same_start(copyrights);
    super::restore_url_slash_before_closing_paren_from_raw_lines(raw_lines, copyrights);
    super::add_first_angle_email_only_variants(copyrights);
    super::drop_shadowed_angle_email_prefix_copyrights_same_span(copyrights);
    super::drop_shadowed_quote_before_email_variants_same_span(copyrights);
    super::drop_url_embedded_suffix_variants_same_span(copyrights, holders);
    super::add_missing_holder_from_single_copyright(copyrights, holders);

    super::drop_shadowed_acronym_location_suffix_copyrights_same_span(copyrights);
    super::split_multiline_holder_lists_from_copyright_email_sequences(copyrights, holders);
    super::drop_json_description_metadata_copyrights_and_holders(raw_lines, copyrights, holders);
    super::drop_copyright_like_holders(holders);
}
