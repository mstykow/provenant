// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    extract_comment_author_supplements, extract_patch_header_author_supplements,
    is_binary_string_copyright_candidate,
};

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
