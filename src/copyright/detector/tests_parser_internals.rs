// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_pata_ali_fixture_preserves_maintainer_suffix() {
    let path = PathBuf::from(
        "testdata/copyright-golden/copyrights/misco4/linux-copyrights/drivers/ata/pata_ali.c",
    );
    let content = fs::read_to_string(&path).expect("read fixture");

    let raw_line = " *  Copyright (C) 1998-2000 Michel Aubry, Maintainer";
    let prepared = crate::copyright::prepare::prepare_text_line(raw_line);
    assert!(prepared.contains("Maintainer"), "prepared: {prepared}");

    let maint_tokens = get_tokens(&[(1, prepared.clone())]);
    assert!(
        maint_tokens
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("Maintainer") && t.tag != PosTag::Junk),
        "maintainer tokens: {maint_tokens:#?}"
    );

    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let cs: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cs.iter()
            .any(|c| c == "Copyright (c) 1998-2000 Michel Aubry, Maintainer"),
        "copyrights: {cs:#?}\n\nholders: {hs:#?}"
    );
    assert!(
        hs.iter().any(|h| h == "Michel Aubry, Maintainer"),
        "copyrights: {cs:#?}\n\nholders: {hs:#?}"
    );
}

#[test]
fn test_index_html_tokens_tag_copyright_word_as_copy() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    assert!(!groups.is_empty(), "Expected at least one candidate group");

    let tokens = get_tokens(&groups[0]);
    assert!(
        tokens
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("copyright") && t.tag == PosTag::Copy),
        "Expected 'Copyright' token tagged as Copy. First group tokens: {:?}",
        tokens.iter().take(30).collect::<Vec<_>>()
    );

    let has_adjacent = tokens.windows(2).any(|w| {
        w[0].tag == PosTag::Copy
            && w[0].value.eq_ignore_ascii_case("copyright")
            && w[1].tag == PosTag::Copy
            && w[1].value.eq_ignore_ascii_case("(c)")
    });
    assert!(
        has_adjacent,
        "Expected adjacent Copy('Copyright') + Copy('(c)') tokens in first group"
    );
}

#[test]
fn test_mpl_prefix_line_without_trailing_holder_keeps_plain_copyright() {
    let input = "// Portions created by the Initial Developer are Copyright (C) 2007";
    let numbered_lines: Vec<(usize, String)> = input
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    assert_eq!(groups.len(), 1, "Unexpected groups: {groups:?}");

    let tokens = get_tokens(&groups[0]);
    assert!(!tokens.is_empty(), "No tokens produced");
    assert!(
        tokens.iter().any(|t| t.tag == PosTag::Copy),
        "Expected at least one Copy token, got: {tokens:?}"
    );
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.tag, PosTag::Yr | PosTag::BareYr | PosTag::YrPlus)),
        "Expected at least one year token, got: {tokens:?}"
    );

    let (c, _h, _a) = detect_copyrights_from_text(input);

    assert!(
        c.iter().any(|cr| cr.copyright == "Copyright (c) 2007"),
        "Expected plain Copyright (c) year, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_normalize_split_angle_bracket_urls_keeps_tail() {
    let input = "Copyright Krzysztof <https://github.com\nHavret>, Stack Builders <https://github.com\nstackbuilders>, end";
    let out = super::normalize_split_angle_bracket_urls(input);
    let out: &str = out.as_ref();
    assert!(
        out.contains("https://github.com Havret")
            && out.contains("https://github.com stackbuilders"),
        "normalized: {out:?}"
    );
}
