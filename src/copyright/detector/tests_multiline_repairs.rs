// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::models::LineNumber;

#[test]
fn test_multiline_two_copyrights_adjacent_lines() {
    let input = "\tCopyright 1988, 1989 by Carnegie Mellon University\n\tCopyright 1989\tTGV, Incorporated\n";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("Carnegie Mellon")),
        "Should detect CMU copyright"
    );
    assert!(
        c.iter().any(|cr| cr.copyright.contains("TGV")),
        "Should detect TGV copyright, got: {:?}",
        c
    );
    assert!(
        h.iter().any(|hr| hr.holder.contains("TGV")),
        "Should detect TGV holder, got: {:?}",
        h
    );
}

#[test]
fn test_multiline_copyright_after_created_line() {
    let input = "// Created: Sun Feb  9 10:06:01 2003 by faith@dict.org\n// Copyright 2003, 2004 Rickard E. Faith (faith@dict.org)\n";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("Rickard")),
        "Should detect Faith copyright, got: {:?}",
        c
    );
    assert!(
        h.iter().any(|hr| hr.holder.contains("Faith")),
        "Should detect Faith holder, got: {:?}",
        h
    );
}

#[test]
fn test_multiline_copyrighted_by_href_links_merges_trailing_copyright_clause() {
    let input = "copyrighted by <A\nHREF=\"http://www.dre.vanderbilt.edu/~schmidt/\">Douglas C. Schmidt</A>\nand his <a\nHREF=\"http://www.cs.wustl.edu/~schmidt/ACE-members.html\">research\ngroup</a> at <A HREF=\"http://www.wustl.edu/\">Washington\nUniversity</A>, <A HREF=\"http://www.uci.edu\">University of California,\nIrvine</A>, and <A HREF=\"http://www.vanderbilt.edu\">Vanderbilt\nUniversity</A>, Copyright (c) 1993-2009, all rights reserved.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    let expected = "copyrighted by http://www.dre.vanderbilt.edu/~schmidt/ Douglas C. Schmidt and his http://www.cs.wustl.edu/~schmidt/ACE-members.html research group at http://www.wustl.edu/ Washington University, http://www.uci.edu University of California, Irvine, and http://www.vanderbilt.edu Vanderbilt University, Copyright (c) 1993-2009";
    assert!(
        c.iter().any(|cr| cr.copyright == expected),
        "Expected merged copyrighted-by href copyright, got: {:?}",
        c
    );
    let merged = c.iter().find(|cr| cr.copyright == expected).unwrap();
    assert!(
        merged.end_line > merged.start_line,
        "Expected merged span to extend across lines, got: {:?}",
        merged
    );
}

#[test]
fn test_html_anchor_copyright_url_multiline_span_preserved() {
    let input = "<a href=\"https://example.com/path\">\ncopyright\n</a>";
    let (c, h, _a) = detect_copyrights_from_text(input);

    let cd = c
        .iter()
        .find(|cr| cr.copyright == "copyright https://example.com/path")
        .unwrap();
    assert_eq!(
        (cd.start_line, cd.end_line),
        (LineNumber::new(1).unwrap(), LineNumber::new(3).unwrap()),
        "copyrights: {c:?}"
    );

    let hd = h
        .iter()
        .find(|hr| hr.holder == "https://example.com/path")
        .unwrap();
    assert_eq!(
        (hd.start_line, hd.end_line),
        (LineNumber::new(1).unwrap(), LineNumber::new(3).unwrap()),
        "holders: {h:?}"
    );
}

#[test]
fn test_split_multiline_holder_list_with_emails() {
    let input = "(c) 1999                Terrehon Bowden <terrehon@pacbell.net>\n                        Bodo Bauer <bb@ricochet.net>\n";

    let (_copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        holders.iter().any(|h| h.holder == "Terrehon Bowden"),
        "holders: {holders:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Bodo Bauer"),
        "holders: {holders:?}"
    );
    assert!(
        !holders
            .iter()
            .any(|h| h.holder == "Terrehon Bowden Bodo Bauer"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_boost_style_multiline_holder_continuation_after_year_first_line() {
    let input = "// Copyright (c) 2019 Peter Dimov (pdimov at gmail dot com),\n\
//                    Vinnie Falco (vinnie.falco@gmail.com)\n\
// Copyright (c) 2020 Krystian Stasiowski (sdkrystian@gmail.com)\n";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights.iter().any(|c| {
            c.start_line == LineNumber::ONE
                && c.end_line == LineNumber::new(2).unwrap()
                && c.copyright.contains("Peter Dimov")
                && c.copyright.contains("Vinnie Falco")
        }),
        "copyrights: {copyrights:?}"
    );

    assert!(
        holders.iter().any(|h| {
            h.start_line == LineNumber::ONE
                && h.end_line == LineNumber::new(2).unwrap()
                && h.holder.contains("Peter Dimov")
                && h.holder.contains("Vinnie Falco")
        }),
        "holders: {holders:?}"
    );
}

#[test]
fn test_year_first_multiline_holder_repair_does_not_absorb_multiline_holder_lists() {
    let input = "Copyright (c) 1995, 1996, 1997 Francis.Dupont@inria.fr, INRIA Rocquencourt,\n\
Alain.Durand@imag.fr, IMAG,\n\
Jean-Luc.Richier@imag.fr, IMAG-LSR.\n";

    let (copyrights, _holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights.iter().any(|c| {
            c.start_line == LineNumber::ONE
                && c.end_line == LineNumber::ONE
                && c.copyright == "Copyright (c) 1995, 1996, 1997 Francis.Dupont@inria.fr, INRIA"
        }),
        "copyrights: {copyrights:?}"
    );

    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright.contains("Rocquencourt") || c.copyright.contains("Alain.Durand")),
        "copyrights: {copyrights:?}"
    );
}

#[test]
fn test_extend_copyright_with_following_all_rights_reserved_line() {
    let input = "Copyright 2010-2015 Mike Bostock\nAll rights reserved.";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright 2010-2015 Mike Bostock"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        copyrights
            .iter()
            .any(|c| c.start_line == LineNumber::ONE && c.end_line == LineNumber::new(2).unwrap()),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Mike Bostock"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_drop_url_embedded_suffix_copyright_and_holder_variants() {
    let input =
        "/* Copyright (c) 2020 Example Corp. See url(\"https://dummy-url-for-test.com\") */";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) 2020 Example Corp."),
        "copyrights: {copyrights:?}"
    );
    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright.contains("See url") || c.copyright.contains("https://")),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Example Corp."),
        "holders: {holders:?}"
    );
    assert!(
        !holders
            .iter()
            .any(|h| h.holder.contains("See url") || h.holder.contains("http")),
        "holders: {holders:?}"
    );
}

#[test]
fn test_add_missing_holder_from_preceding_name_line_for_year_only_copyright() {
    let input = "Author:  David Beazley (http://www.dabeaz.com)\nCopyright (C) 2007\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "David Beazley, Copyright (c) 2007"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "David Beazley"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_descriptive_line_does_not_expand_year_only_copyright_holder() {
    let input = "Tru64 audio module for SDL (Simple DirectMedia Layer)\nCopyright (C) 2003\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) 2003"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright == "Tru64 audio module for SDL, Copyright (c) 2003"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .all(|h| h.holder != "Tru64 audio module for SDL"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_drop_trademarked_materials_prose_false_positive_copyrights_and_holders() {
    let input = "SPDX-FileCopyrightText: <years> Univention GmbH\n\nBinary versions of this program provided by Univention to you as well\nas other copyrighted, protected or trademarked materials like Logos,\ngraphics, fonts, specific documentations and configurations,\ncryptographic keys etc. are subject to a license agreement between you\nand Univention and not subject to the AGPL-3.0-only.\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .all(|c| !c.copyright.contains("protected or trademarked materials")),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .all(|h| !h.holder.contains("protected or trademarked materials")),
        "holders: {holders:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Univention GmbH"),
        "holders: {holders:?}"
    );
}
