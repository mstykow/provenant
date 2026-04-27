// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    extract_comment_author_supplements, extract_copyright_information,
    extract_patch_header_author_supplements, is_binary_string_copyright_candidate,
};
use crate::copyright;
use crate::models::{FileInfoBuilder, FileType};
use std::path::Path;
use std::time::Duration;

#[test]
fn test_binary_string_copyright_candidate_rejects_gibberish_holder_text() {
    let gibberish = "(c) S8@9 K @9 D @9 I,@9N(@ F@@9L,@ HD@9) M0@9s J'@y DH@9Ih@y";
    assert!(!is_binary_string_copyright_candidate(gibberish));
}

#[test]
fn test_binary_string_copyright_candidate_keeps_real_notice() {
    let notice = "Copyright nexB and others (c) 2012";
    assert!(is_binary_string_copyright_candidate(notice));
}

#[test]
fn test_binary_string_copyright_candidate_rejects_changelog_phrase() {
    assert!(!is_binary_string_copyright_candidate(
        "Copyright - split out libs"
    ));
}

#[test]
fn test_extract_patch_header_author_supplements_collects_common_patch_headers() {
    let text = "From: Robert Scheck <robert@fedoraproject.org>\n\
Signed-off-by: Khem Raj <raj.khem@gmail.com>\n\
Patch by Example Person <example@example.com>\n";

    let authors = extract_patch_header_author_supplements(text);
    let values: Vec<_> = authors.into_iter().map(|author| author.author).collect();

    assert_eq!(
        values,
        vec![
            "Robert Scheck <robert@fedoraproject.org>",
            "Khem Raj <raj.khem@gmail.com>",
            "Example Person <example@example.com>",
        ]
    );
}

#[test]
fn test_extract_comment_author_supplements_collects_written_by_and_email_name_forms() {
    let text = "# udhcpc script edited by Tim Riker <Tim@Rikers.org>\n\
#   clst@ambu.com (Claus Stovgaard)\n\
#                by Ian Murdock <imurdock@gnu.ai.mit.edu>.\n";

    let authors = extract_comment_author_supplements(text);
    let values: Vec<_> = authors.into_iter().map(|author| author.author).collect();

    assert_eq!(
        values,
        vec![
            "Tim Riker <Tim@Rikers.org>",
            "Claus Stovgaard <clst@ambu.com>",
            "Ian Murdock <imurdock@gnu.ai.mit.edu>",
        ]
    );
}

#[test]
fn test_extract_comment_author_supplements_collects_comment_by_and_docker_maintainer_lines() {
    let text = "# a2enmod by Stefan Fritsch <sf@debian.org>\n\
LABEL maintainer=\"Progress Chef <docker@chef.io>\"\n";

    let authors = extract_comment_author_supplements(text);
    let values: Vec<_> = authors.into_iter().map(|author| author.author).collect();

    assert_eq!(
        values,
        vec![
            "Stefan Fritsch <sf@debian.org>",
            "Progress Chef <docker@chef.io>",
        ]
    );
}

#[test]
fn test_extract_comment_author_supplements_handles_c_style_translator_headers() {
    let text = "/* Translated by Jorge Barreiro <yortx.barry@gmail.com>. */\n\
/* Written by Mathias Bynens <https://mathiasbynens.be/> */\n\
/* Written by Cloudream (cloudream@gmail.com). */\n\
/* Written by S A Sureshkumar (saskumar@live.com). */\n";

    let authors = extract_comment_author_supplements(text);
    let values: Vec<_> = authors.into_iter().map(|author| author.author).collect();

    assert_eq!(
        values,
        vec![
            "Jorge Barreiro <yortx.barry@gmail.com>",
            "Mathias Bynens https://mathiasbynens.be",
            "Cloudream (cloudream@gmail.com)",
            "S A Sureshkumar (saskumar@live.com)",
        ]
    );
}

#[test]
fn test_extract_comment_author_supplements_ignores_html_tags() {
    let text = "the order defined by the DTD (see Section 13.3).</p>";

    let authors = extract_comment_author_supplements(text);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_extract_comment_author_supplements_ignores_plain_markdown_prose() {
    let text =
        "Support this project by [becoming a sponsor](https://opencollective.com/pnpm#sponsor).";

    let authors = extract_comment_author_supplements(text);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_extract_copyright_information_ignores_pnpm_markdown_link_prose() {
    let text = concat!(
        "</table>\n\n",
        "<!-- sponsors end -->\n\n",
        "Support this project by [becoming a sponsor](https://opencollective.com/pnpm#sponsor).\n\n",
        "## Background\n",
    );

    let mut builder = FileInfoBuilder::default();
    extract_copyright_information(&mut builder, Path::new("README.md"), text, 120.0, false);

    let file = builder
        .name("README.md".to_string())
        .base_name("README".to_string())
        .extension(".md".to_string())
        .path("README.md".to_string())
        .file_type(FileType::File)
        .size(text.len() as u64)
        .build()
        .expect("builder should produce file info");

    assert!(file.authors.is_empty(), "authors: {:?}", file.authors);
}

#[test]
fn test_detector_timeout_and_non_timeout_paths_match_for_pnpm_markdown_link_prose() {
    let text = concat!(
        "</table>\n\n",
        "<!-- sponsors end -->\n\n",
        "Support this project by [becoming a sponsor](https://opencollective.com/pnpm#sponsor).\n\n",
        "## Background\n",
    );

    let (_c1, _h1, authors_no_deadline) = copyright::detect_copyrights(text, None);
    let (_c2, _h2, authors_with_deadline) =
        copyright::detect_copyrights(text, Some(Duration::from_secs(120)));

    assert_eq!(authors_no_deadline, authors_with_deadline);
    assert!(
        authors_with_deadline.is_empty(),
        "authors_with_deadline: {authors_with_deadline:?}"
    );
}

#[test]
fn test_extract_copyright_information_ignores_pnpm_changelog_markdown_link_on_large_input() {
    let repeated = "- Do not hang indefinitely, when there is a glob that starts with `!/` in `pnpm-workspace.yaml`. This fixes a regression introduced by [#9169](https://github.com/pnpm/pnpm/pull/9169).\n";
    let text = repeated.repeat(4000);

    let mut builder = FileInfoBuilder::default();
    extract_copyright_information(
        &mut builder,
        Path::new("pnpm/CHANGELOG.md"),
        &text,
        0.000001,
        false,
    );

    let file = builder
        .name("CHANGELOG.md".to_string())
        .base_name("CHANGELOG".to_string())
        .extension(".md".to_string())
        .path("pnpm/CHANGELOG.md".to_string())
        .file_type(FileType::File)
        .size(text.len() as u64)
        .build()
        .expect("builder should produce file info");

    assert!(file.authors.is_empty(), "authors: {:?}", file.authors);
}
