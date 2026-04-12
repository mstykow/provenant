//! Tests for query module.

#[cfg(test)]
mod tests {
    use crate::license_detection::index::LicenseIndex;
    use crate::license_detection::index::dictionary::{TokenId, tid};
    use crate::license_detection::models::PositionSpan;
    use crate::license_detection::query::{Query, QueryRun};
    use crate::license_detection::test_utils::create_test_index;

    fn create_query_test_index() -> LicenseIndex {
        create_test_index(&[("license", 0), ("copyright", 1), ("permission", 2)], 3)
    }

    fn build_query<'a>(text: &str, index: &'a LicenseIndex) -> anyhow::Result<Query<'a>> {
        Query::from_extracted_text(text, index, false)
    }

    fn build_query_detecting_binary<'a>(
        text: &str,
        index: &'a LicenseIndex,
    ) -> anyhow::Result<Query<'a>> {
        Query::with_source_options(text, index, Query::TEXT_LINE_THRESHOLD, None)
    }

    fn build_query_with_threshold<'a>(
        text: &str,
        index: &'a LicenseIndex,
        line_threshold: usize,
    ) -> anyhow::Result<Query<'a>> {
        Query::with_source_options(text, index, line_threshold, None)
    }

    fn query_unknown_count_after(query: &Query<'_>, pos: Option<usize>) -> usize {
        query.unknowns_by_pos.get(&pos).copied().unwrap_or(0)
    }

    fn query_stopword_count_after(query: &Query<'_>, pos: Option<usize>) -> usize {
        query.stopwords_by_pos.get(&pos).copied().unwrap_or(0)
    }

    fn run_start_line(run: &QueryRun<'_>) -> Option<usize> {
        run.source_line_by_pos().get(run.start).copied()
    }

    fn run_end_line(run: &QueryRun<'_>) -> Option<usize> {
        run.end
            .and_then(|end| run.source_line_by_pos().get(end).copied())
    }

    #[test]
    fn test_query_new_with_empty_text() {
        let index = create_query_test_index();
        let query = build_query("", &index).unwrap();

        assert!(query.is_empty());
        assert_eq!(query.tokens.len(), 0);
        assert!(!query.is_binary);
    }

    #[test]
    fn test_query_new_with_known_tokens() {
        let index = create_query_test_index();
        let text = "License copyright permission";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 3);
        assert_eq!(query.tokens.first().copied(), Some(tid(0)));
        assert_eq!(query.tokens.get(1).copied(), Some(tid(1)));
        assert_eq!(query.tokens.get(2).copied(), Some(tid(2)));
        assert_eq!(query.line_for_pos(0), Some(1));
        assert_eq!(query.line_for_pos(1), Some(1));
        assert_eq!(query.line_for_pos(2), Some(1));
    }

    #[test]
    fn test_query_new_with_unknown_tokens() {
        let index = create_query_test_index();
        let text = "License foobar copyright";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 2);
        assert_eq!(query.tokens.first().copied(), Some(tid(0)));
        assert_eq!(query.tokens.get(1).copied(), Some(tid(1)));

        assert_eq!(query_unknown_count_after(&query, Some(0)), 1);
        assert_eq!(query_unknown_count_after(&query, Some(1)), 0);
    }

    #[test]
    fn test_query_new_with_stopwords() {
        let index = create_query_test_index();
        let text = "license div copyright p";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 2);

        assert_eq!(query_stopword_count_after(&query, Some(0)), 1);
        assert_eq!(query_stopword_count_after(&query, Some(1)), 1);
    }

    #[test]
    fn test_query_new_with_short_tokens() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("x");
        let _ = index.dictionary.get_or_assign("y");
        let _ = index.dictionary.get_or_assign("z");

        let text = "x y z license";
        let query = build_query(text, &index).unwrap();

        assert!(!query.is_empty());
        assert!(query.tokens.len() <= 4);

        for pos in 0..query.tokens.len().min(3) {
            assert!(
                query.shorts_and_digits_pos.contains(pos),
                "Position {} should be short",
                pos
            );
        }
    }

    #[test]
    fn test_query_new_with_digit_tokens() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("123");
        let _ = index.dictionary.get_or_assign("456");

        let text = "123 456 license";
        let query = build_query(text, &index).unwrap();

        assert!(query.shorts_and_digits_pos.contains(0));
        assert!(query.shorts_and_digits_pos.contains(1));
        assert!(!query.shorts_and_digits_pos.contains(2));
    }

    #[test]
    fn test_query_new_multiline() {
        let index = create_query_test_index();
        let text = "Line 1 license\nLine 2 copyright\nLine 3 permission";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 3);
        assert_eq!(query.line_for_pos(0), Some(1));
        assert_eq!(query.line_for_pos(1), Some(2));
        assert_eq!(query.line_for_pos(2), Some(3));
    }

    #[test]
    fn test_query_detect_binary_text() {
        let index = create_query_test_index();

        let query = build_query_detecting_binary("license copyright", &index).unwrap();
        assert!(!query.is_binary);
    }

    #[test]
    fn test_query_detect_binary_null_bytes() {
        let index = create_query_test_index();
        let text = "license\0copyright";

        let query = build_query_detecting_binary(text, &index).unwrap();
        assert!(query.is_binary);
    }

    #[test]
    fn test_query_new_with_empty_lines() {
        let index = create_query_test_index();
        let text = "license\n\ncopyright";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 2);
        assert_eq!(query.line_for_pos(0), Some(1));
        assert_eq!(query.line_for_pos(1), Some(3));
    }

    #[test]
    fn test_query_new_with_leading_unknowns() {
        let index = create_query_test_index();
        let text = "unknown1 unknown2 license";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 1);
        assert_eq!(query_unknown_count_after(&query, None), 2);
    }

    #[test]
    fn test_query_new_with_leading_stopwords() {
        let index = create_query_test_index();
        let text = "div p license";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 1);
        assert_eq!(query_stopword_count_after(&query, None), 2);
    }

    #[test]
    fn test_query_run_new() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        assert_eq!(run.start, 0);
        assert_eq!(run.end, Some(2));
    }

    #[test]
    fn test_query_whole_query_run() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = query.whole_query_run();

        assert_eq!(run.start, 0);
        assert_eq!(run.end, Some(2));
        assert_eq!(run_start_line(&run), Some(1));
        assert_eq!(run_end_line(&run), Some(1));
    }

    #[test]
    fn test_query_run_tokens() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(1));

        assert_eq!(run.tokens(), vec![tid(0), tid(1)]);
    }

    #[test]
    fn test_query_run_tokens_with_pos() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(1));

        let tokens_with_pos: Vec<(usize, TokenId)> = run.tokens_with_pos().collect();
        assert_eq!(tokens_with_pos, vec![(0, tid(0)), (1, tid(1))]);
    }

    #[test]
    fn test_query_run_empty() {
        let index = create_query_test_index();
        let query = build_query("", &index).unwrap();
        let run = QueryRun::new(&query, 0, None);

        assert_eq!(run.tokens().len(), 0);
        assert_eq!(run.start, 0);
        assert_eq!(run.end, None);
    }

    #[test]
    fn test_query_high_matchables() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        let high = run.high_matchables();
        assert_eq!(high.len(), 3);
        assert!(query.high_matchables.contains(0));
        assert!(query.high_matchables.contains(1));
        assert!(query.high_matchables.contains(2));
    }

    #[test]
    fn test_query_low_matchables_in_range() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("word");
        let query = build_query("license word copyright", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        let low = run.low_matchables();
        assert!(low.contains(1));
    }

    #[test]
    fn test_query_run_matchables() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        let matchables = run.matchables(true);
        assert_eq!(matchables.len(), 3);

        let high_matchables = run.matchables(false);
        assert_eq!(high_matchables.len(), 3);
    }

    #[test]
    fn test_query_run_is_matchable() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        assert!(run.is_matchable(false, &[]));
        assert!(run.is_matchable(true, &[]));
    }

    #[test]
    fn test_query_run_is_not_matchable_digits_only() {
        let mut index = create_query_test_index();
        let _tid_123 = index.dictionary.get_or_assign("123");
        let _tid_456 = index.dictionary.get_or_assign("456");

        let query = build_query("123 456", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(1));

        assert!(!run.is_matchable(false, &[]));
    }

    #[test]
    fn test_query_run_is_matchable_with_exclude() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        let exclude_span = PositionSpan::new(0, 2);
        assert!(run.is_matchable(false, &[exclude_span]));
    }

    #[test]
    fn test_query_matchable_tokens() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        let tokens = run.matchable_tokens();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Some(tid(0)));
        assert_eq!(tokens[1], Some(tid(1)));
        assert_eq!(tokens[2], Some(tid(2)));
    }

    #[test]
    fn test_query_run_subtract() {
        let index = create_query_test_index();
        let mut query = build_query("license copyright permission", &index).unwrap();

        let span = PositionSpan::new(0, 2);
        query.subtract(&span);

        assert!(!query.high_matchables.contains(0));
        assert!(!query.high_matchables.contains(1));
        assert!(query.high_matchables.contains(2));
    }

    #[test]
    fn test_query_new_lowercase_tokens() {
        let index = create_query_test_index();
        let text = "LICENSE COPYRIGHT Permission";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 3);
        assert_eq!(query.tokens.first().copied(), Some(tid(0)));
        assert_eq!(query.tokens.get(1).copied(), Some(tid(1)));
        assert_eq!(query.tokens.get(2).copied(), Some(tid(2)));
    }

    #[test]
    fn test_query_matched_text_single_line() {
        let index = create_query_test_index();
        let text = "license copyright permission";
        let query = build_query(text, &index).unwrap();

        let matched = query.matched_text(1, 1);
        assert_eq!(matched, "license copyright permission");
    }

    #[test]
    fn test_query_matched_text_multiple_lines() {
        let index = create_query_test_index();
        let text = "line1\nline2\nline3\nline4";
        let query = build_query(text, &index).unwrap();

        let matched = query.matched_text(2, 3);
        assert_eq!(matched, "line2\nline3");
    }

    #[test]
    fn test_query_matched_text_full_range() {
        let index = create_query_test_index();
        let text = "line1\nline2\nline3";
        let query = build_query(text, &index).unwrap();

        let matched = query.matched_text(1, 3);
        assert_eq!(matched, "line1\nline2\nline3");
    }

    #[test]
    fn test_query_matched_text_invalid_range() {
        let index = create_query_test_index();
        let text = "line1\nline2";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.matched_text(0, 1), "");
        assert_eq!(query.matched_text(2, 1), "");
        assert_eq!(query.matched_text(0, 0), "");
    }

    #[test]
    fn test_query_run_get_index() {
        let index = create_query_test_index();
        let query = build_query("license copyright", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(1));

        let idx = run.get_index();
        assert_eq!(idx.len_legalese, 3);
    }

    #[test]
    fn test_query_run_line_for_pos() {
        let index = create_query_test_index();
        let text = "license\ncopyright\npermission";
        let query = build_query(text, &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        assert_eq!(run.line_for_pos(0), Some(1));
        assert_eq!(run.line_for_pos(1), Some(2));
        assert_eq!(run.line_for_pos(2), Some(3));
        assert_eq!(run.line_for_pos(10), None);
    }

    #[test]
    fn test_query_run_is_matchable_all_excluded() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(2));

        let exclude_span = PositionSpan::new(0, 3);
        assert!(!run.is_matchable(false, &[exclude_span]));
    }

    #[test]
    fn test_query_new_with_unicode() {
        let index = create_query_test_index();
        let text = "licença copyright 许可";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.tokens.len(), 1);
        assert_eq!(query.tokens.first().copied(), Some(tid(1)));
    }

    #[test]
    fn test_query_high_matchables_out_of_range() {
        let index = create_query_test_index();
        let query = build_query("license copyright", &index).unwrap();
        let run = QueryRun::new(&query, 100, Some(200));

        let high = run.high_matchables();
        assert!(high.is_empty());
    }

    #[test]
    fn test_query_low_matchables_out_of_range() {
        let index = create_query_test_index();
        let query = build_query("license copyright", &index).unwrap();
        let run = QueryRun::new(&query, 100, Some(200));

        let low = run.low_matchables();
        assert!(low.is_empty());
    }

    #[test]
    fn test_query_high_matchables_unbounded_end() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 1, None);

        let high_set = run.high_matchables();
        assert!(high_set.contains(1));
        assert!(high_set.contains(2));
        assert!(!high_set.contains(0));
    }

    #[test]
    fn test_query_run_end_line_none() {
        let index = create_query_test_index();
        let query = build_query("", &index).unwrap();
        let run = QueryRun::new(&query, 0, None);

        assert_eq!(run_end_line(&run), None);
    }

    #[test]
    fn test_query_with_only_stopwords() {
        let index = create_query_test_index();
        let text = "div p a br";
        let query = build_query(text, &index).unwrap();

        assert!(query.is_empty());
        assert_eq!(query_stopword_count_after(&query, None), 4);
    }

    #[test]
    fn test_query_with_only_unknowns() {
        let index = create_query_test_index();
        let text = "unknown1 unknown2 unknown3";
        let query = build_query(text, &index).unwrap();

        assert!(query.is_empty());
        assert_eq!(query_unknown_count_after(&query, None), 3);
    }

    #[test]
    fn test_query_run_matchable_tokens_empty_high_matchables() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("word");
        index.len_legalese = 0;

        let query = build_query("word word", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(1));

        let tokens = run.matchable_tokens();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_query_run_is_digits_only_mixed() {
        let mut index = create_query_test_index();
        let _tid_123 = index.dictionary.get_or_assign("123");
        let _ = index.dictionary.get_or_assign("license");

        let query = build_query("123 license", &index).unwrap();
        let run = QueryRun::new(&query, 0, Some(1));

        assert!(!run.is_digits_only());
    }

    #[test]
    fn test_query_run_high_low_matchables_slice() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = QueryRun::new(&query, 1, Some(2));

        let high = run.high_matchables();
        assert_eq!(high.len(), 2);
        assert!(high.contains(1));
        assert!(high.contains(2));
        assert!(!high.contains(0));

        let low = run.low_matchables();
        assert!(low.is_empty());
    }

    #[test]
    fn test_query_run_splitting_single_run() {
        let index = create_query_test_index();
        let text = "license copyright permission";
        let query = build_query_with_threshold(text, &index, 4).unwrap();

        let runs = query.query_runs();
        assert_eq!(runs.len(), 1);
        assert_eq!(query.query_run_ranges, vec![(0, Some(2))]);
        assert_eq!(runs[0].start, 0);
        assert_eq!(runs[0].end, Some(2));
    }

    #[test]
    fn test_query_run_splitting_with_empty_lines() {
        let index = create_query_test_index();
        let text = "license\n\n\n\n\ncopyright";
        let query = build_query_with_threshold(text, &index, 4).unwrap();

        let runs = query.query_runs();
        assert_eq!(query.query_run_ranges, vec![(0, Some(0)), (1, Some(1))]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].start, 0);
        assert_eq!(runs[0].end, Some(0));
        assert_eq!(runs[1].start, 1);
        assert_eq!(runs[1].end, Some(1));
    }

    #[test]
    fn test_query_run_splitting_below_threshold() {
        let index = create_query_test_index();
        let text = "license\n\n\ncopyright";
        let query = build_query_with_threshold(text, &index, 4).unwrap();

        let runs = query.query_runs();
        assert_eq!(runs.len(), 1);
        assert_eq!(query.query_run_ranges, vec![(0, Some(1))]);
    }

    #[test]
    fn test_query_run_splitting_empty_query() {
        let index = create_query_test_index();
        let query = build_query_with_threshold("", &index, 4).unwrap();

        assert!(query.query_run_ranges.is_empty());

        let runs = query.query_runs();
        assert!(runs.is_empty());
    }

    #[test]
    fn test_query_tracks_spdx_lines_with_positions() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("spdx");
        let _ = index.dictionary.get_or_assign("license");
        let _ = index.dictionary.get_or_assign("identifier");
        let _ = index.dictionary.get_or_assign("mit");
        let _ = index.dictionary.get_or_assign("apache");

        let text = "SPDX-License-Identifier: MIT\nSPDX-License-Identifier: Apache-2.0";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.spdx_lines.len(), 2, "Should track 2 SPDX lines");

        for (spdx_text, start, end) in &query.spdx_lines {
            assert!(*start <= *end, "Token positions should be valid");
            assert!(
                spdx_text.to_lowercase().contains("spdx"),
                "SPDX text should contain SPDX keyword"
            );
        }
    }

    #[test]
    fn test_query_spdx_lines_not_at_position_zero() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("spdx");
        let _ = index.dictionary.get_or_assign("license");
        let _ = index.dictionary.get_or_assign("identifier");
        let _ = index.dictionary.get_or_assign("mit");

        let text = "license copyright\nSPDX-License-Identifier: MIT";
        let query = build_query(text, &index).unwrap();

        assert_eq!(query.spdx_lines.len(), 1, "Should track 1 SPDX line");

        let (_, start, _) = &query.spdx_lines[0];
        assert!(
            *start >= 2,
            "SPDX start position should be >= 2 (after 'license copyright')"
        );
    }

    #[test]
    fn test_query_run_splitting_multiple_segments() {
        let index = create_query_test_index();
        let text = "license\n\n\n\n\ncopyright\n\n\n\n\npermission";
        let query = build_query_with_threshold(text, &index, 4).unwrap();

        let runs = query.query_runs();
        assert_eq!(
            query.query_run_ranges,
            vec![(0, Some(0)), (1, Some(1)), (2, Some(2))]
        );
        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn test_query_run_splitting_breaks_good_long_line_into_python_pseudolines() {
        let mut index = create_query_test_index();
        let low_tokens: Vec<String> = (0..25).map(|i| format!("word{}", i)).collect();
        for token in &low_tokens {
            let _ = index.dictionary.get_or_assign(token);
        }

        let text = format!("{} license", low_tokens.join(" "));
        let query = build_query_with_threshold(&text, &index, 1).unwrap();

        assert!(Query::detect_long_lines(&text));
        assert_eq!(query.query_run_ranges, vec![(0, Some(24)), (25, Some(25))]);
    }

    #[test]
    fn test_query_run_splitting_unknown_only_lines_count_toward_threshold() {
        let index = create_query_test_index();
        let text = "license\nfoobar bazqux\nbleep bloop\ncopyright";
        let query = build_query_with_threshold(text, &index, 2).unwrap();

        assert_eq!(query.query_run_ranges, vec![(0, Some(0)), (1, Some(1))]);
    }

    #[test]
    fn test_query_run_splitting_low_only_lines_count_toward_threshold() {
        let mut index = create_query_test_index();
        let _ = index.dictionary.get_or_assign("word1");
        let _ = index.dictionary.get_or_assign("word2");

        let text = "license\nword1 word2\nword1\ncopyright";
        let query = build_query_with_threshold(text, &index, 2).unwrap();

        assert_eq!(query.query_run_ranges, vec![(0, Some(3)), (4, Some(4))]);
    }

    #[test]
    fn test_query_run_splitting_digits_only_lines_do_not_emit_final_digits_only_run() {
        let mut index = create_query_test_index();
        let _tid_123 = index.dictionary.get_or_assign("123");
        let _tid_456 = index.dictionary.get_or_assign("456");

        let text = "license\n123\n456";
        let query = build_query_with_threshold(text, &index, 1).unwrap();

        assert_eq!(query.query_run_ranges, vec![(0, Some(1))]);
    }

    #[test]
    fn test_query_run_splitting_breaks_at_exact_threshold() {
        let index = create_query_test_index();
        let text = "license\n\n\ncopyright";
        let query = build_query_with_threshold(text, &index, 2).unwrap();

        assert_eq!(query.query_run_ranges, vec![(0, Some(0)), (1, Some(1))]);
    }

    #[test]
    fn test_query_run_splitting_exact_long_line_boundary_does_not_split() {
        let mut index = create_query_test_index();
        let low_tokens: Vec<String> = (0..24).map(|i| format!("word{}", i)).collect();
        for token in &low_tokens {
            let _ = index.dictionary.get_or_assign(token);
        }

        let text = format!("{} license", low_tokens.join(" "));
        let query = build_query_with_threshold(&text, &index, 1).unwrap();

        assert!(!Query::detect_long_lines(&text));
        assert_eq!(query.query_run_ranges, vec![(0, Some(24))]);
    }

    #[test]
    fn test_query_run_splitting_uses_python_pseudoline_boundaries() {
        let mut index = create_query_test_index();
        let low_tokens: Vec<String> = (0..30).map(|i| format!("word{}", i)).collect();
        for token in &low_tokens {
            let _ = index.dictionary.get_or_assign(token);
        }

        let text = format!("license\nword0 word1\n{} license", low_tokens.join(" "));
        let query = build_query_with_threshold(&text, &index, 2).unwrap();

        assert!(Query::detect_long_lines(&text));
        assert_eq!(query.query_run_ranges, vec![(0, Some(27)), (28, Some(33))]);
    }

    #[test]
    fn test_query_from_extracted_text_uses_binary_line_threshold_and_flag() {
        let index = create_query_test_index();
        let text = format!("license\n{}copyright", "\n".repeat(20));

        let text_query = build_query(&text, &index).unwrap();
        let binary_query = Query::from_extracted_text(&text, &index, true).unwrap();

        assert_eq!(
            text_query.query_run_ranges,
            vec![(0, Some(0)), (1, Some(1))]
        );
        assert_eq!(binary_query.query_run_ranges, vec![(0, Some(1))]);
        assert!(binary_query.is_binary);
    }

    #[test]
    fn test_query_subtract_removes_positions() {
        let index = create_query_test_index();
        let mut query = build_query("license copyright permission", &index).unwrap();

        assert!(query.high_matchables.contains(0));
        assert!(query.high_matchables.contains(1));

        let span = PositionSpan::new(0, 2);
        query.subtract(&span);

        assert!(!query.high_matchables.contains(0));
        assert!(!query.high_matchables.contains(1));
        assert!(query.high_matchables.contains(2));
    }

    #[test]
    fn test_query_run_is_matchable_with_exclusions() {
        let index = create_query_test_index();
        let query = build_query("license copyright permission", &index).unwrap();
        let run = query.whole_query_run();

        assert!(run.is_matchable(false, &[]));

        let exclude = vec![PositionSpan::new(0, 2)];
        assert!(run.is_matchable(false, &exclude));

        let exclude_all = vec![PositionSpan::new(0, 3)];
        assert!(!run.is_matchable(false, &exclude_all));
    }

    #[test]
    fn test_subtraction_after_near_duplicate_match() {
        let index = create_query_test_index();
        let mut query = build_query("license copyright license copyright", &index).unwrap();

        assert!(query.high_matchables.contains(0));
        assert!(query.high_matchables.contains(1));

        let near_dupe_span = PositionSpan::new(0, 2);
        query.subtract(&near_dupe_span);

        assert!(!query.high_matchables.contains(0));
        assert!(!query.high_matchables.contains(1));
        assert!(query.high_matchables.contains(2));
        assert!(query.high_matchables.contains(3));
    }

    #[test]
    fn test_whole_query_run_snapshot_preserves_pre_subtraction_matchables() {
        let index = create_query_test_index();
        let mut query = build_query("license copyright permission", &index).unwrap();

        let whole_run = query.whole_query_run();
        let before_subtraction = whole_run.high_matchables();
        assert_eq!(before_subtraction.len(), 3);
        assert!(before_subtraction.contains(0));
        assert!(before_subtraction.contains(1));
        assert!(before_subtraction.contains(2));

        query.subtract(&PositionSpan::new(0, 2));

        let snapshot_after_subtraction = whole_run.high_matchables();
        assert_eq!(snapshot_after_subtraction, before_subtraction);

        let live_run = query.query_runs().into_iter().next().unwrap();
        let live_matchables = live_run.high_matchables();
        assert_eq!(live_matchables.len(), 1);
        assert!(live_matchables.contains(2));
        assert!(!live_matchables.contains(0));
        assert!(!live_matchables.contains(1));
    }
}
