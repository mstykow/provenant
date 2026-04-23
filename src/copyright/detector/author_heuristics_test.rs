// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::copyright::line_tracking::PreparedLineCache;
use crate::copyright::types::AuthorDetection;

#[test]
fn test_author_colon_multiline_keeps_emails() {
    let input = "/*\n * Authors: Jorge Cwik, <jorge@laser.satlink.net>\n *\t\tArnt Gulbrandsen, <agulbra@nvg.unit.no>\n */\n";

    let raw_lines: Vec<&str> = input.lines().collect();
    let prepared_cache = PreparedLineCache::new(&raw_lines).materialize();
    let mut extracted: Vec<AuthorDetection> = Vec::new();
    extract_author_colon_blocks(&prepared_cache, &mut extracted);
    assert!(
        extracted.iter().any(|ad| ad.author
            == "Jorge Cwik, <jorge@laser.satlink.net> Arnt Gulbrandsen, <agulbra@nvg.unit.no>"),
        "Expected direct author-colon extraction to keep emails, got: {:?}",
        extracted.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );

    let (_c, _h, a) = super::super::detect_copyrights_from_text(input);

    assert!(
        a.iter().any(|ad| ad.author
            == "Jorge Cwik, <jorge@laser.satlink.net> Arnt Gulbrandsen, <agulbra@nvg.unit.no>"),
        "Expected merged multiline author block, got: {:?}",
        a.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_author_colon_empty_tail_collects_following_rst_roster_lines() {
    let input =
        "Authors:\n\t Richard Walker,\n\t Jamie Honan,\n\t Michael Hunold\n\nGeneral information\n";

    let raw_lines: Vec<&str> = input.lines().collect();
    let prepared_cache = PreparedLineCache::new(&raw_lines).materialize();
    let mut extracted: Vec<AuthorDetection> = Vec::new();
    extract_author_colon_blocks(&prepared_cache, &mut extracted);
    assert!(
        extracted
            .iter()
            .any(|ad| { ad.author == "Richard Walker, Jamie Honan, Michael Hunold" }),
        "Expected empty-tail Authors: block to merge following roster lines, got: {:?}",
        extracted.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );

    let (_c, _h, a) = super::super::detect_copyrights_from_text(input);
    assert!(
        a.iter()
            .any(|ad| ad.author == "Richard Walker, Jamie Honan, Michael Hunold"),
        "Expected pipeline to keep merged roster author block, got: {:?}",
        a.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_authors_from_dense_name_email_list() {
    let input = "John Doe <john@example.com>\nJane Smith <jane@example.com>\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "John Doe <john@example.com>"),
        "authors: {authors:?}"
    );
    assert!(
        authors
            .iter()
            .any(|a| a.author == "Jane Smith <jane@example.com>"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_collective_author_with_contributors_before_email() {
    let input = "authors = [\"Tokio Contributors <team@tokio.rs>\"]\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Tokio Contributors <team@tokio.rs>"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_toml_singular_author_array_with_handle() {
    let input = "author = [\"Tom Breloff (@tbreloff)\"]\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Tom Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_created_by_author_with_handle() {
    let input = "Created by Tom Breloff (@tbreloff)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Tom Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_primary_author_with_handle() {
    let input = "Primary author: Josef Heinen (@jheinen)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Josef Heinen"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_original_author_with_handle() {
    let input = "Original author: Thomas Breloff (@tbreloff)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Thomas Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_primary_package_author_with_handle() {
    let input = "Primary PlotlyJS.jl author: Spencer Lyon (@spencerlyon2)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Spencer Lyon"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_author_colon_inline_roster_with_handles() {
    let input = "authors: Benoit Pasquier (@briochemc) - David Gustavsson (@gustaphe) - Jan Weidner (@jw3126)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Benoit Pasquier"),
        "authors: {authors:?}"
    );
    assert!(
        authors.iter().any(|a| a.author == "David Gustavsson"),
        "authors: {authors:?}"
    );
    assert!(
        authors.iter().any(|a| a.author == "Jan Weidner"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_markdown_heading_original_author_with_handle() {
    let input = "### Original author: Thomas Breloff (@tbreloff)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Thomas Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_original_author_before_maintained_by_clause() {
    let input =
        "### Original author: Thomas Breloff (@tbreloff), maintained by the JuliaPlots members\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Thomas Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_originally_implemented_by_author_with_parenthesized_email() {
    let input = "LALR(1) support was originally implemented by Elias Ioup (ezioup@alumni.uchicago.edu),\nusing the algorithm found in Aho, Sethi, and Ullman.\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Elias Ioup (ezioup@alumni.uchicago.edu)"),
        "authors: {authors:?}"
    );
}
