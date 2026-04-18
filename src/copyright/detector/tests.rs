use super::postprocess_transforms::{
    drop_shadowed_c_sign_variants, drop_shadowed_year_only_copyright_prefixes_same_start_line,
};
use super::token_utils::{
    collect_filtered_leaves, collect_holder_filtered_leaves, normalize_whitespace,
    signal_lines_before_copy_line, strip_all_rights_reserved, tokens_to_string,
};
use super::*;
use crate::models::LineNumber;
use std::fs;
use std::path::PathBuf;

// ── End-to-end pipeline tests ────────────────────────────────────

#[test]
fn test_copyright_prefix_preserved_with_unicode_symbol() {
    let input = "Copyright \u{00A9} 1998 Tom Tromey";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.starts_with("Copyright")),
        "Should preserve 'Copyright' prefix with \u{00A9} symbol, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_drop_shadowed_year_only_prefix_same_start_line() {
    let mut copyrights = vec![
        CopyrightDetection {
            copyright: "(c) 2001".to_string(),
            start_line: LineNumber::new(5).unwrap(),
            end_line: LineNumber::new(5).unwrap(),
        },
        CopyrightDetection {
            copyright: "(c) 2001 Foo Bar".to_string(),
            start_line: LineNumber::new(5).unwrap(),
            end_line: LineNumber::new(5).unwrap(),
        },
    ];
    drop_shadowed_year_only_copyright_prefixes_same_start_line(&mut copyrights);
    assert!(
        !copyrights.iter().any(|c| c.copyright == "(c) 2001"),
        "should drop year-only prefix when longer exists: {copyrights:?}"
    );
}

#[test]
fn test_multiline_c_style_holder_name_not_truncated() {
    let input = "*\n\
* Copyright (c) The International Cooperation for the Integration of \n\
* Processes in  Prepress, Press and Postpress (CIP4).  All rights \n\
* reserved.\n";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
            copyrights.iter().any(|c| c.copyright
                == "Copyright (c) The International Cooperation for the Integration of Processes in Prepress, Press and Postpress (CIP4)"),
            "copyrights: {:?}",
            copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
        );
    assert!(
            holders.iter().any(|h| h.holder
                == "The International Cooperation for the Integration of Processes in Prepress, Press and Postpress (CIP4)"),
            "holders: {:?}",
            holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
        );
}

#[test]
fn test_multiline_leading_dash_suffix_is_extended() {
    let input = "Copyright 1998-2010 AOL Inc.\n - Apache\n";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright 1998-2010 AOL Inc. - Apache"),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "AOL Inc. - Apache"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_standalone_c_holder_year_range_with_trailing_period_is_extracted() {
    let input = "(c) The University of Glasgow 2004-2009.";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) The University of Glasgow 2004-2009"),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "The University of Glasgow"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_obfuscated_angle_email_is_kept_in_copyright() {
    let input = "(C)opyright MMIV-MMV Anselm R. Garbe <garbeam at gmail dot com>";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
        copyrights.iter().any(|c| {
            c.copyright == "Copyright (c) MMIV-MMV Anselm R. Garbe garbeam at gmail dot com"
        }),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "MMIV-MMV Anselm R. Garbe"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_dash_obfuscated_email_is_kept_in_copyright() {
    let input = "Copyright (c) 2005, 2006  Nick Galbreath -- nickg [at] modp [dot] com";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
        copyrights.iter().any(|c| {
            c.copyright == "Copyright (c) 2005, 2006 Nick Galbreath - nickg at modp dot com"
        }),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "Nick Galbreath"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_trailing_copy_year_suffix_is_kept() {
    let input = "Copyright base-x contributors (c) 2016";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright base-x contributors (c) 2016"),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "base-x contributors"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_author_prefix_dedup_keeps_short_email_list() {
    let input = "Author(s): gthomas, sorin@netappi.com\nContributors: gthomas, sorin@netappi.com, andrew.lunn@ascom.ch\n";
    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let vals: Vec<&str> = authors.iter().map(|a| a.author.as_str()).collect();
    assert!(
        vals.contains(&"gthomas, sorin@netappi.com"),
        "authors: {vals:?}"
    );
    assert!(
        vals.contains(&"gthomas, sorin@netappi.com, andrew.lunn@ascom.ch"),
        "authors: {vals:?}"
    );
}

#[test]
fn test_added_copyright_year_for_line_is_extracted() {
    let input = "Added the Copyright year (2020) for A11yance";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright year (2020) for A11yance"),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "A11yance"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_was_developed_by_multiline_author_is_extracted() {
    let input = "1. GOST R 34.11-2012 was developed by the Center for Information\nProtection and Special Communications of the Federal Security\nService of the Russian Federation with participation of the Open\n";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);
    assert!(
            authors.iter().any(|a| {
                a.author
                    == "the Center for Information Protection and Special Communications of the Federal Security Service of the Russian Federation"
            }),
            "authors: {:?}",
            authors.iter().map(|a| &a.author).collect::<Vec<_>>()
        );
}

#[test]
fn test_written_by_author_email_for_project_is_extracted() {
    let input = "Written by Andy Polyakov <appro@openssl.org> for the OpenSSL\nproject.";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);
    assert!(
        authors
            .iter()
            .any(|a| a.author == "Andy Polyakov <appro@openssl.org>"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_w3c_registered_holder_is_extracted() {
    let input = "This software includes material\n\
copied from [title]. Copyright ©\n\
[YEAR] W3C® (MIT, ERCIM, Keio, Beihang).";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);
    assert!(
        copyrights
            .iter()
            .any(|c| { c.copyright == "Copyright (c) YEAR W3C(r) (MIT, ERCIM, Keio, Beihang)" }),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "W3C(r) (MIT, ERCIM, Keio, Beihang)"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_boost_html_holder_drops_symbol_table_run_junk() {
    let input = concat!(
        "<p>Copyright &copy; John Maddock, Joel de Guzman, Eric Niebler and Matias Capeletto</p>\n",
        "<p>(r), & 175, & 176, & 177, & 178, & 179, & 180, & 181, & 182, & 183</p>",
    );

    let (_copyrights, holders, _authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = holders.iter().map(|h| h.holder.as_str()).collect();

    assert_eq!(
        values,
        vec!["John Maddock, Joel de Guzman, Eric Niebler and Matias Capeletto"],
        "holders: {values:?}"
    );
    assert!(
        !values.iter().any(|holder| holder.starts_with("(r), & 175")),
        "holders: {values:?}"
    );
}

#[test]
fn test_prose_snippet_does_not_report_laboriously_took_the_trouble_as_author() {
    let input = concat!(
        "<para>the authors laboriously took the trouble of searching for workarounds ",
        "to make these compilers happy</para>",
    );

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {:?}", authors);
}

#[test]
fn test_developed_by_sentence_author_is_extracted() {
    let input = "developed by the U.S. Government. BAE Systems is enhancing and supporting the SMP";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);
    assert!(
        authors
            .iter()
            .any(|a| a.author == "the U.S. Government. BAE Systems"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_developed_by_phrase_author_is_extracted() {
    let input = "to acknowledge that it was\n\
      developed by the National Center for Supercomputing Applications at the University of Illinois at Urbana-Champaign and to credit the\n\
      contributors.";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);
    assert!(
            authors.iter().any(|a| {
                a.author
                    == "the National Center for Supercomputing Applications at the University of Illinois at Urbana-Champaign"
            }),
            "authors: {:?}",
            authors.iter().map(|a| &a.author).collect::<Vec<_>>()
        );
}

#[test]
fn test_modified_portion_developed_by_author_with_url_is_extracted() {
    let input = concat!(
        "# This product contains a modified portion of 'Flask App Builder' developed by Daniel Vaz Gaspar.\n",
        "# (https://github.com/dpgaspar/Flask-AppBuilder).\n",
    );

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);
    assert!(
        authors.iter().any(
            |a| a.author == "Daniel Vaz Gaspar. (https://github.com/dpgaspar/Flask-AppBuilder)"
        ),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_current_year_placeholder_copyright_holder_detected() {
    let input = "Copyright 2016- CURRENT_YEAR The Apache Software Foundation";

    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights.iter().any(|c| {
            c.copyright == "Copyright 2016-CURRENT_YEAR The Apache Software Foundation"
                || c.copyright == "Copyright 2016- CURRENT_YEAR The Apache Software Foundation"
        }),
        "copyrights: {:?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "The Apache Software Foundation"),
        "holders: {:?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_author_colon_block_stops_at_status_and_devices_metadata() {
    let input = "Author: ds\nStatus: works in immediate mode\nDevices: [standard] parallel port\n";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors.is_empty(),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_author_colon_block_keeps_named_author_without_devices_tail() {
    let input =
        "Author: Pablo Mejia <pablo.mejia@cctechnol.com>\nDevices: [Access I/O] PC-104 AIO12-8\n";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Pablo Mejia <pablo.mejia@cctechnol.com>"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_written_by_comma_and_copyright_keeps_parenthesized_email_author() {
    let input =
        "written by Philip Hazel, and copyright\nby the University of Cambridge, England.\n";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Philip Hazel"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_with_additional_hacking_by_keeps_parenthesized_email_author() {
    let input = "With additional hacking by Jeffrey Kuskin (jsk@mojave.stanford.edu)\n";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Jeffrey Kuskin (jsk@mojave.stanford.edu)"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_adapted_written_by_keeps_parenthesized_email_author() {
    let input = "Adapted from baycom.c driver written by Thomas Sailer (sailer@ife.ee.ethz.ch)\n";

    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Thomas Sailer (sailer@ife.ee.ethz.ch)"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_copyright_prefix_preserved_multiline_debian() {
    let input = "Copyright:\n\n    Copyright \u{00A9} 1999-2009  Red Hat, Inc.\n    Copyright \u{00A9} 1998       Tom Tromey\n    Copyright \u{00A9} 1999       Free Software Foundation, Inc.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    let missing: Vec<_> = c
        .iter()
        .filter(|cr| !cr.copyright.starts_with("Copyright"))
        .map(|cr| &cr.copyright)
        .collect();
    assert!(
        missing.is_empty(),
        "All copyrights should start with 'Copyright', but these don't: {:?}",
        missing
    );
}

#[test]
fn test_copyright_prefix_preserved_with_html_tags() {
    let input = "    Copyright \u{00A9} 1998       <s>Tom Tromey</s>\n    Copyright \u{00A9} 1999       <s>Free Software Foundation, Inc.</s>";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    let missing: Vec<_> = c
        .iter()
        .filter(|cr| !cr.copyright.starts_with("Copyright"))
        .map(|cr| &cr.copyright)
        .collect();
    assert!(
        missing.is_empty(),
        "All copyrights should start with 'Copyright', but these don't: {:?}",
        missing
    );
}

#[test]
fn test_copyright_prefix_preserved_debian_copyright_header() {
    let input = "Copyright:\n\n\tCopyright (C) 1998-2005 <s>Oliver Rauch</s>";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.starts_with("Copyright")),
        "Should preserve 'Copyright' prefix after 'Copyright:' header, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_copyright_prefix_preserved_multi_copyright_block() {
    let input = "Copyright:\n    Copyright \u{00A9} 1999-2009  <s>Red Hat, Inc.</s>\n    Copyright \u{00A9} 1998       <s>Tom Tromey</s>\n    Copyright \u{00A9} 1999       <s>Free Software Foundation, Inc.</s>\n    Copyright \u{00A9} 2003       <s>Sun Microsystems, Inc.</s>";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    let missing: Vec<_> = c
        .iter()
        .filter(|cr| !cr.copyright.starts_with("Copyright"))
        .map(|cr| &cr.copyright)
        .collect();
    assert!(
        missing.is_empty(),
        "All copyrights should start with 'Copyright', but these don't: {:?}",
        missing
    );
}

#[test]
fn test_detect_html_multiline_copyright_keeps_copyright_word() {
    let input = "<li><p class=\"Legal\" style=\"margin-left: 0pt;\">Copyright \u{00A9} 2002-2009 \n\t Charlie Poole</p></li>";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "Expected merged Copyright (c) statement, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_lua_org_puc_rio_not_truncated() {
    let content = "Copyright © 1994-2011 Lua.org, PUC-Rio\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s.contains("Lua.org") && s.contains("PUC-Rio")),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s.contains("Lua.org") && s.contains("PUC-Rio")),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_copyright_or_copr_without_year() {
    let content = "Copyright or Copr. CNRS\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| s == "Copyright or Copr. CNRS"),
        "copyrights: {cr:#?}"
    );
    assert!(hs.iter().any(|s| s == "CNRS"), "holders: {hs:#?}");
}

#[test]
fn test_detect_copr_with_multiple_dash_segments_not_truncated() {
    let content = "Copyright  or Copr. 2006 INRIA - CIRAD - INRA\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| s == "Copr. 2006 INRIA - CIRAD - INRA"),
        "copyrights: {cr:#?}"
    );
    assert!(
        !cr.iter().any(|s| s == "Copr. 2006 INRIA - CIRAD"),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter().any(|s| s == "INRIA - CIRAD - INRA"),
        "holders: {hs:#?}"
    );
    assert!(!hs.iter().any(|s| s == "INRIA - CIRAD"), "holders: {hs:#?}");
}

#[test]
fn test_detect_lppl_single_copyright_line() {
    let content = "Copyright 2003 Name\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens.clone())
    };

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| s == "Copyright 2003 Name"),
        "groups: {groups:#?}\n\ntokens: {tokens:#?}\n\ntree: {tree:#?}\n\ncopyrights: {cr:#?}"
    );
    assert!(hs.iter().any(|s| s == "Name"), "holders: {hs:#?}");
}

#[test]
fn test_detect_person_name_with_middle_initial() {
    let content = "Copyright (c) 2004, Richard S. Hall\n";
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(
        hs.iter().any(|s| s == "Richard S. Hall"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_hall_copyright_fixture_contains_richard_s_hall_holder() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/hall-copyright.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(
        hs.iter().any(|s| s == "Richard S. Hall"),
        "copyrights: {cr:#?}\n\nholders: {hs:#?}"
    );
}

#[test]
fn test_math_c_fixture_restores_angle_email_holders_for_modified_by_lines() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/math.c");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    let debug: Vec<String> = holders
        .iter()
        .map(|h| format!("{} [{}-{}]", h.holder, h.start_line, h.end_line))
        .collect();
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "Paul Mundt <lethal@linux-sh.org>"),
        "holders: {debug:#?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "Vladimir Oleynik <dzo@simtreas.ru>"),
        "holders: {debug:#?}"
    );
}

#[test]
fn test_gailly_c_fixture_keeps_original_busybox_gzip_copyright() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/gailly-c.c");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) 1992-1993 Jean-loup Gailly"),
        "copyrights: {:#?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "Jean-loup Gailly"),
        "holders: {:#?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_busybox_env_modified_by_line_does_not_absorb_correct_usage_bullet() {
    let content = "* Modified by Vladimir Oleynik <dzo@simtreas.ru> (C) 2003\n* - correct \"-\" option usage\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Vladimir Oleynik <dzo@simtreas.ru> (c) 2003"),
        "copyrights: {:#?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        !copyrights.iter().any(|c| c.copyright.contains("- correct")),
        "copyrights: {:#?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "Vladimir Oleynik"),
        "holders: {:#?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_andre_darcy_fixture_extracts_modifications_copyright_by_line() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/andre_darcy-c.c");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    assert!(
        copyrights.iter().any(|c| {
            c.copyright == "copyright 1997, 1998, 1999 by D'Arcy J.M. Cain (darcy@druid.net)"
        }),
        "copyrights: {:#?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "D'Arcy J.M. Cain"),
        "holders: {:#?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright == "copyright 1997, 1998, 1999"),
        "copyrights: {:#?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_licco_fixture_merges_author_and_author_email_metadata() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/licco.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(&content);

    assert!(
        authors
            .iter()
            .any(|a| { a.author == "Hartmut Goebel Author-email h.goebel@crazy-compilers.com" }),
        "authors: {:#?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
    assert!(
        !authors.iter().any(|a| a.author == "Hartmut Goebel"),
        "authors: {:#?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
    assert!(
        !authors
            .iter()
            .any(|a| a.author == "Author-email h.goebel@crazy-compilers.com"),
        "authors: {:#?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_libcompress_raw_zlib_perl_fixture_does_not_merge_debian_copyright_lines() {
    let path = PathBuf::from(
        "testdata/copyright-golden/copyrights/libcompress_raw_zlib_perl-libcompress_raw_zlib_perl.copyright",
    );
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    let cr: Vec<&str> = copyrights.iter().map(|c| c.copyright.as_str()).collect();
    let hs: Vec<&str> = holders.iter().map(|h| h.holder.as_str()).collect();

    assert!(
        cr.contains(&"Copyright 1995-2005, Jean-loup Gailly <jloup@gzip.org>"),
        "copyrights: {cr:#?}"
    );
    assert!(
        cr.contains(&"Copyright 1995-2005, Mark Adler <madler@alumni.caltech.edu>"),
        "copyrights: {cr:#?}"
    );
    assert!(
            !cr.contains(
                &"Jean-loup Gailly <jloup@gzip.org> Copyright 1995-2005, Mark Adler <madler@alumni.caltech.edu>"
            ),
            "copyrights: {cr:#?}"
        );
    assert!(hs.contains(&"Jean-loup Gailly"), "holders: {hs:#?}");
    assert!(hs.contains(&"Mark Adler"), "holders: {hs:#?}");
}

#[test]
fn test_libopenraw_fixture_does_not_merge_multiple_debian_copyrights() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/libopenraw1-libopenraw.label");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    let cr: Vec<&str> = copyrights.iter().map(|c| c.copyright.as_str()).collect();
    let hs: Vec<&str> = holders.iter().map(|h| h.holder.as_str()).collect();

    assert!(
        cr.contains(&"(c) 1994, Kongji Huang and Brian C. Smith, Cornell University"),
        "copyrights: {cr:#?}"
    );
    assert!(
        cr.contains(&"(c) 2001, Lutz M\u{00fc}ller <lutz@users.sourceforge.net>"),
        "copyrights: {cr:#?}"
    );
    assert!(
        cr.contains(&"Copyright (c) 2006, Hubert Figuiere <hub@figuiere.net>"),
        "copyrights: {cr:#?}"
    );
    assert!(
            !cr.contains(
                &"Hubert Figuiere <hub@figuiere.net> (c) 1994, Kongji Huang and Brian C. Smith, Cornell University"
            ),
            "copyrights: {cr:#?}"
        );
    assert!(
            !cr.contains(
                &"Hubert Figuiere <hub@figuiere.net> (c) 2001, Lutz M\u{00fc}ller <lutz@users.sourceforge.net>"
            ),
            "copyrights: {cr:#?}"
        );
    assert!(
        hs.contains(&"Kongji Huang and Brian C. Smith, Cornell University"),
        "holders: {hs:#?}"
    );
    assert!(hs.contains(&"Lutz M\u{00fc}ller"), "holders: {hs:#?}");
    assert!(hs.contains(&"Hubert Figuiere"), "holders: {hs:#?}");
}

#[test]
fn test_pre_name_fixture_does_not_restore_angle_email_holders() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/misco4/linux3/pre-name.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    let hs: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
    assert!(hs.iter().any(|h| h == "Paul Mundt"), "holders: {hs:#?}");
    assert!(
        hs.iter().any(|h| h == "Vladimir Oleynik"),
        "holders: {hs:#?}"
    );
    assert!(
        !hs.iter().any(|h| h.contains("<lethal@linux-sh.org>")),
        "holders: {hs:#?}"
    );
    assert!(
        !hs.iter().any(|h| h.contains("<dzo@simtreas.ru>")),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_with_trailing_software_fixture_does_not_append_software_to_holder() {
    let path =
        PathBuf::from("testdata/copyright-golden/copyrights/copytest/with_trailing_software.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    let hs: Vec<String> = holders.iter().map(|h| h.holder.clone()).collect();
    assert!(hs.iter().any(|h| h == "Ian F. Darwin"), "holders: {hs:#?}");
    assert!(
        !hs.iter().any(|h| h == "Ian F. Darwin Software"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_copyright_span_does_not_absorb_following_author_line() {
    let input = "Copyright (c) Ian F. Darwin 1986\nSoftware written by Ian F. Darwin and others;";
    let (_c, holders, _authors) = detect_copyrights_from_text(input);
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(hs.iter().any(|h| h == "Ian F. Darwin"), "holders: {hs:#?}");
    assert!(
        !hs.iter().any(|h| h == "Ian F. Darwin Software"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_copyright_span_does_not_absorb_following_lint_directive_line() {
    let input = concat!(
        "// (c) Example Corp. and affiliates. Confidential and proprietary.\n",
        "// @lint-ignore-every FBOBJCIMPORTORDER1 METHOD_BRACKETSMETHOD_BRACKETS\n",
    );

    let (copyrights, _holders, _authors) = detect_copyrights_from_text(input);
    let values: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();

    assert!(
        values
            .iter()
            .any(|c| c == "(c) Example Corp. and affiliates. Confidential and proprietary"),
        "copyrights: {values:#?}"
    );
    assert!(
        !values.iter().any(|c| c.contains("@lint-ignore-every")),
        "copyrights: {values:#?}"
    );
}

#[test]
fn test_history_written_by_fixture_keeps_following_author() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/misco4/more-linux/multi.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(&content);

    let values: Vec<String> = authors.into_iter().map(|a| a.author).collect();
    assert!(
        values.iter().any(|a| a == "Ben Dooks <ben@simtec.co.uk>"),
        "authors: {values:#?}"
    );
}

#[test]
fn test_multilines_fixture_detects_split_copyright_by_holder() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/misco4/linux4/multilines.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, holders, authors) = detect_copyrights_from_text(&content);

    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(
        cr.iter()
            .any(|c| c == "copyright by the University of Cambridge, England"),
        "copyrights: {cr:#?}\n\nholders: {hs:#?}"
    );
    assert!(
        hs.iter()
            .any(|h| h == "the University of Cambridge, England"),
        "holders: {hs:#?}"
    );

    let as_: Vec<String> = authors.into_iter().map(|a| a.author).collect();
    assert!(as_.iter().any(|a| a == "Philip Hazel"), "authors: {as_:#?}");
}

#[test]
fn test_extract_from_tree_nodes_builds_hall_holder_tokens() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/hall-copyright.txt");
    let content = fs::read_to_string(&path).expect("read fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let group = groups
        .iter()
        .find(|g| {
            g.iter()
                .any(|(_ln, l)| l.contains("Richard") && l.contains("Hall"))
        })
        .expect("group containing Richard Hall");

    let tokens = get_tokens(group);
    let tree = parse(tokens);

    let mut debug_lines: Vec<String> = Vec::new();
    for (i, node) in tree.iter().enumerate() {
        let leaves = collect_all_leaves(node);
        let line = leaves.first().map(|t| t.start_line.get()).unwrap_or(0);
        let has_2004 = leaves
            .iter()
            .any(|t| t.tag == PosTag::Yr && t.value.starts_with("2004"));
        let preview = leaves
            .iter()
            .take(8)
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if has_2004 {
            debug_lines.push(format!(
                "idx={i} label={:?} line={line} preview={preview:?}",
                node.label()
            ));
        }
    }

    let mut hall_idx: Option<usize> = None;
    for (i, node) in tree.iter().enumerate() {
        let leaves = collect_all_leaves(node);
        let has_2004 = leaves
            .iter()
            .any(|t| t.tag == PosTag::Yr && t.value.starts_with("2004"));
        if !has_2004 {
            continue;
        }
        let has_richard = leaves.iter().any(|t| t.value == "Richard");
        let has_hall = leaves.iter().any(|t| t.value == "Hall");
        if has_richard && has_hall {
            hall_idx = Some(i);
            break;
        }
    }
    let hall_idx = hall_idx
        .unwrap_or_else(|| panic!("hall node not found. nodes-with-2004: {debug_lines:#?}"));
    let hall_node = &tree[hall_idx];

    let (trailing_tokens, _skip) = collect_trailing_orphan_tokens(hall_node, &tree, hall_idx + 1);
    let copy_line = collect_all_leaves(hall_node)
        .iter()
        .filter(|t| t.tag == PosTag::Copy && t.value.eq_ignore_ascii_case("copyright"))
        .map(|t| t.start_line.get())
        .min();
    let keep_prefix_lines = copy_line
        .map(|cl| signal_lines_before_copy_line(hall_node, LineNumber::new(cl).unwrap()))
        .unwrap_or_default();

    let node_holder_leaves =
        collect_holder_filtered_leaves(hall_node, NON_HOLDER_LABELS, NON_HOLDER_POS_TAGS);
    let mut holder_tokens: Vec<&Token> = Vec::new();
    let mut node_holder_leaves = strip_all_rights_reserved(node_holder_leaves);
    if let Some(copy_line) = copy_line {
        node_holder_leaves.retain(|t| {
            t.start_line.get() >= copy_line || keep_prefix_lines.contains(&t.start_line.get())
        });
    }
    holder_tokens.extend(node_holder_leaves);
    holder_tokens.extend(&trailing_tokens);

    let holder_string = normalize_whitespace(&tokens_to_string(&holder_tokens));
    let refined = refine_holder_in_copyright_context(&holder_string);

    assert_eq!(
        refined.as_deref(),
        Some("Richard S. Hall"),
        "idx={hall_idx} holder_string={holder_string:?} trailing={:?} node={hall_node:#?}",
        trailing_tokens
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_hisax_debug_fixture_holder_phrase() {
    let path = PathBuf::from(
        "testdata/copyright-golden/copyrights/misco4/linux-copyrights/drivers/isdn/hisax/hisax_debug.h",
    );
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(
        hs.iter().any(|s| s == "Frode Isaksen by Frode Isaksen"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_holder_super_fixture_drops_trailing_comma_before_company_line() {
    let path = PathBuf::from("testdata/copyright-golden/holders/holder_super_c-c.c");
    let content = fs::read_to_string(&path).expect("read fixture");

    let (_copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(
        hs.iter().any(|s| s == "Benjamin Herrenschmuidt IBM Corp."),
        "holders: {hs:#?}"
    );
    assert!(
        !hs.iter().any(|s| s == "Benjamin Herrenschmuidt, IBM Corp."),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_somefile_cpp_fixture_extracts_licensed_material_copyright() {
    let path =
        PathBuf::from("testdata/copyright-golden/holders/holder_somefile_cpp-somefile_cpp.cpp");
    let content = fs::read_to_string(&path).expect("read fixture");

    let (copyrights, _holders, _authors) = detect_copyrights_from_text(&content);
    let cs: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cs.iter().any(|s| s == "Foobar Company, (c) 2005"),
        "copyrights: {cs:#?}"
    );
}

#[test]
fn test_device_tree_fixture_extracts_authors_block() {
    let path = PathBuf::from("testdata/copyright-golden/authors/device_tree.c");
    let content = fs::read_to_string(&path).expect("read fixture");

    let (c, _h, authors) = detect_copyrights_from_text(&content);
    let authors: Vec<String> = authors.into_iter().map(|a| a.author).collect();
    assert!(
            authors
                .iter()
                .any(|a| a
                    == "Jerone Young <jyoung5@us.ibm.com> Hollis Blanchard <hollisb@us.ibm.com>"),
            "authors: {authors:#?}\n\ncopyrights: {c:#?}"
        );
}

#[test]
fn test_multiline_written_and_maintained_by_block_extracts_individual_authors() {
    let input = concat!(
        "GNU tar, heavily based on John Gilmore's public domain version of tar,\n",
        "was originally written by Graham Todd.\n",
        "It is now maintained by Sergey Poznyakoff.\n",
        "This package is maintained for Debian by Janos Lenart <ocsi@debian.org>.\n",
    );

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let authors: Vec<String> = authors.into_iter().map(|a| a.author).collect();

    assert!(
        authors.iter().any(|a| a == "Graham Todd"),
        "authors: {authors:#?}"
    );
    assert!(
        authors.iter().any(|a| a == "Sergey Poznyakoff"),
        "authors: {authors:#?}"
    );
    assert!(
        authors
            .iter()
            .any(|a| a == "Janos Lenart <ocsi@debian.org>"),
        "authors: {authors:#?}"
    );
    assert!(
        !authors
            .iter()
            .any(|a| a.contains("GNU tar, heavily based on")),
        "authors: {authors:#?}"
    );
}

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
fn test_detect_misc_linux_fixture_tieto_holder() {
    let path =
        PathBuf::from("testdata/copyright-golden/copyrights/misco4/more-linux/misc-linux.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(hs.iter().any(|s| s == "Tieto Poland"), "holders: {hs:#?}");
}

#[test]
fn test_detect_notice_txt_fixture_bare_c_year_range_suffix() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/notice_txt-NOTICE.txt");
    let content = fs::read_to_string(&path).expect("read fixture");
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(&content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cr.iter().any(|s| s == "(c) 2001-2004"),
        "copyrights: {cr:#?}"
    );
}

#[test]
fn test_detect_arch_floppy_h_bare_1995_dropped_for_x86() {
    let content =
        "* Copyright (C) 1995\n */\n#ifndef _ASM_X86_FLOPPY_H\n#define _ASM_X86_FLOPPY_H\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    assert!(copyrights.is_empty());
}

#[test]
fn test_detect_arch_floppy_h_bare_1995_kept_for_alpha() {
    let content =
        "* Copyright (C) 1995\n */\n#ifndef __ASM_ALPHA_FLOPPY_H\n#define __ASM_ALPHA_FLOPPY_H\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright.eq_ignore_ascii_case("Copyright (c) 1995"))
    );
}

#[test]
fn test_detect_changelog_timestamp_copyright_and_holder() {
    let content = "2008-01-26 11:46  vruppert\n\n2002-09-08 21:14  vruppert\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();
    assert!(
        cr.iter()
            .any(|s| s == "copyright 2008-01-26 11:46 vruppert")
    );
    assert!(hs.iter().any(|s| s == "vruppert"));
}

#[test]
fn test_detect_changelog_single_timestamp_is_ignored() {
    let content = "updated year in copyright\n\n2008-01-26 11:46  vruppert\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    assert!(copyrights.is_empty());
    assert!(holders.is_empty());
}

#[test]
fn test_drop_obfuscated_email_year_only_copyright() {
    let content = "Copyright (C) 2008 <srinivasa.deevi at conexant dot com>\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    assert!(copyrights.is_empty());
}

#[test]
fn test_extract_parenthesized_copyright_notice() {
    let content = "an appropriate copyright notice (3dfx Interactive, Inc. 1999), a notice\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cr.iter()
            .any(|s| s == "copyright notice (3dfx Interactive, Inc. 1999)")
    );
}

#[test]
fn test_glide_3dfx_copyright_notice_does_not_trigger_for_notice_s_plural() {
    let content = "copyright notice(s)\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    assert!(!copyrights.iter().any(|c| {
        c.copyright
            .to_ascii_lowercase()
            .contains("copyright notice")
    }));
}

#[test]
fn test_detect_spdx_filecopyrighttext_c_without_year() {
    let content = "# SPDX-FileCopyrightText: Copyright (c) SOIM\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) SOIM")
    );
    assert!(holders.iter().any(|h| h.holder == "SOIM"));
}

#[test]
fn test_detect_versioned_project_banner_with_bare_license_path() {
    let line1 =
        "/*! jQuery v3.7.1 | (c) OpenJS Foundation and other contributors | jquery.org/license */";
    let line2 =
        r#"!function(){var meta={"description":"demo","url":"https://example.com"};return meta;}"#
            .repeat(40);
    let content = format!("{line1}\n{line2}");
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) OpenJS Foundation and other contributors"),
        "copyrights: {copyrights:?}\ngroups: {groups:?}"
    );
    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright.contains("jquery.org/license")),
        "copyrights: {copyrights:?}\ngroups: {groups:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "OpenJS Foundation and other contributors"),
        "holders: {holders:?}\ngroups: {groups:?}"
    );
    assert!(
        !holders
            .iter()
            .any(|h| h.holder.contains("jquery.org/license")),
        "holders: {holders:?}\ngroups: {groups:?}"
    );
}

#[test]
fn test_detect_versioned_project_banner_with_mixed_case_brand_holder() {
    let content = "/*! jQuery v2.2.0 | (c) jQuery Foundation | jquery.org/license */\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) jQuery Foundation"),
        "copyrights: {copyrights:?}\ngroups: {groups:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "jQuery Foundation"),
        "holders: {holders:?}\ngroups: {groups:?}"
    );
}

#[test]
fn test_extract_html_meta_name_copyright_content() {
    let content = concat!(
        r#"<meta name="copyright" content="copyright 2005-2006 Cedrik LIME"/>"#,
        "\n",
        r#"<meta content="copyright 2005-2006 Cedrik LIME" name="copyright"/>"#,
        "\n",
        r#"<meta NAME = 'copyright' CONTENT = 'copyright 2005-2006 Cedrik LIME'/>"#,
        "\n",
        r#"<meta content='copyright 2005-2006 Cedrik LIME' name='copyright'/>"#,
    );
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "copyright 2005-2006 Cedrik LIME")
    );
    assert!(holders.iter().any(|h| h.holder == "Cedrik LIME"));
}

#[test]
fn test_extract_pudn_footer_canonicalizes_to_domain_only() {
    let content = "&#169; 2004-2009 <a href=\"http://www.pudn.com/\"><font color=\"red\">pudn.com</font></a> ÏæICP±¸07000446";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) 2004-2009 pudn.com"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "pudn.com"),
        "holders: {holders:?}"
    );
    assert!(!holders.iter().any(|h| h.holder.contains("upload_log.asp")));
}

#[test]
fn test_extract_pudn_upload_log_link_does_not_create_copyright() {
    let content = r#"&nbsp;&nbsp;�� �� ��: <a href="http://s.pudn.com/upload_log.asp?e=234428" target="_blank">ɭ��</a>"#;
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright.contains("upload_log.asp")),
        "copyrights: {copyrights:?}"
    );
}

#[test]
fn test_identical_pudn_html_fixtures_produce_identical_canonical_output() {
    let url_path =
        PathBuf::from("testdata/copyright-golden/copyrights/url_in_html-detail_9_html.html");
    let incorrect_path =
        PathBuf::from("testdata/copyright-golden/copyrights/html_incorrect-detail_9_html.html");

    let url_bytes = fs::read(&url_path).expect("url_in_html fixture must be readable");
    let incorrect_bytes =
        fs::read(&incorrect_path).expect("html_incorrect fixture must be readable");

    assert_eq!(
        url_bytes, incorrect_bytes,
        "fixtures must be byte-identical"
    );

    let url_content = crate::copyright::golden_utils::read_input_content(&url_path)
        .expect("url_in_html fixture content must load");
    let incorrect_content = crate::copyright::golden_utils::read_input_content(&incorrect_path)
        .expect("html_incorrect fixture content must load");

    let (c1, h1, a1) = detect_copyrights_from_text(&url_content);
    let (c2, h2, a2) = detect_copyrights_from_text(&incorrect_content);

    let mut c1v: Vec<String> = c1.into_iter().map(|d| d.copyright).collect();
    let mut h1v: Vec<String> = h1.into_iter().map(|d| d.holder).collect();
    let mut a1v: Vec<String> = a1.into_iter().map(|d| d.author).collect();
    let mut c2v: Vec<String> = c2.into_iter().map(|d| d.copyright).collect();
    let mut h2v: Vec<String> = h2.into_iter().map(|d| d.holder).collect();
    let mut a2v: Vec<String> = a2.into_iter().map(|d| d.author).collect();

    c1v.sort();
    h1v.sort();
    a1v.sort();
    c2v.sort();
    h2v.sort();
    a2v.sort();
    c1v.dedup();
    h1v.dedup();
    a1v.dedup();
    c2v.dedup();
    h2v.dedup();
    a2v.dedup();

    assert_eq!(c1v, c2v, "copyright outputs differ for identical content");
    assert_eq!(h1v, h2v, "holder outputs differ for identical content");
    assert_eq!(a1v, a2v, "author outputs differ for identical content");

    assert_eq!(c1v, vec!["(c) 2004-2009 pudn.com".to_string()]);
    assert_eq!(h1v, vec!["pudn.com".to_string()]);
    assert!(a1v.is_empty());
}

#[test]
fn test_detect_postscript_percent_copyright_prefix() {
    let content = "%%Copyright: -----------------------------------------------------------\n\
%%Copyright: Copyright 1990-2009 Adobe Systems Incorporated.\n\
%%Copyright: All rights reserved.\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    assert!(!groups.is_empty(), "groups unexpectedly empty");

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s == "Copyright 1990-2009 Adobe Systems Incorporated"),
        "groups: {groups:#?}\ncr: {cr:#?}"
    );
    assert!(
        hs.iter().any(|s| s == "Adobe Systems Incorporated"),
        "{hs:#?}"
    );
}

#[test]
fn test_drop_batman_adv_contributors_copyright() {
    let content = "/* Copyright (C) 2007-2018  B.A.T.M.A.N. contributors: */\n\
#ifndef _NET_BATMAN_ADV_TYPES_H_\n\
#define _NET_BATMAN_ADV_TYPES_H_\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    assert!(!copyrights.iter().any(|c| {
        c.copyright
            .to_ascii_lowercase()
            .contains("b.a.t.m.a.n. contributors")
    }));
    assert!(
        !holders
            .iter()
            .any(|h| h.holder == "B.A.T.M.A.N. contributors")
    );
}

#[test]
fn test_detect_ed_ed_fixture_does_not_merge_adjacent_copyright_lines() {
    let content = "Program Copyright (C) 1993, 1994 Andrew Moore, Talke Studio.\n\
Copyright (C) 2006, 2007 Antonio Diaz Diaz.\n\
Modifications for Debian Copyright (C) 1997-2007 James Troup.\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s == "Copyright (c) 1993, 1994 Andrew Moore, Talke Studio"),
        "{cr:#?}"
    );
    assert!(
        cr.iter()
            .any(|s| s == "Copyright (c) 2006, 2007 Antonio Diaz Diaz"),
        "{cr:#?}"
    );
    assert!(
        cr.iter()
            .any(|s| s == "Copyright (c) 1997-2007 James Troup"),
        "{cr:#?}"
    );

    assert!(
        hs.iter().any(|s| s == "Andrew Moore, Talke Studio"),
        "{hs:#?}"
    );
    assert!(hs.iter().any(|s| s == "Antonio Diaz Diaz"), "{hs:#?}");
    assert!(hs.iter().any(|s| s == "James Troup"), "{hs:#?}");
}

#[test]
fn test_detect_icedax_fixture_includes_libedc_by_line_with_email() {
    let path = PathBuf::from("testdata/copyright-golden/copyrights/icedax-icedax.label");
    let content = fs::read_to_string(&path).expect("icedax fixture must be readable");
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(&content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cr.iter()
            .any(|s| { s == "(c) 1998-2002 by Heiko Eissfeldt, heiko@colossus.escape.de" }),
        "copyrights: {cr:#?}"
    );
}

#[test]
fn test_detect_c_year_range_by_name_comma_email_single_line() {
    let content = "(c) 1998-2002 by Heiko Eissfeldt, heiko@colossus.escape.de\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cr.iter()
            .any(|s| { s == "(c) 1998-2002 by Heiko Eissfeldt, heiko@colossus.escape.de" }),
        "copyrights: {cr:#?}"
    );
}

#[test]
fn test_detect_gnome_session_fixture_includes_queen_of_england() {
    let path =
        PathBuf::from("testdata/copyright-golden/copyrights/gnome_session-gnome_session.copyright");
    let content = fs::read_to_string(&path).expect("gnome session fixture must be readable");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s == "Copyright (c) 2001 Queen of England"),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter().any(|s| s == "Queen of England"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_copyright_year_name_with_of_single_line() {
    let content = "Copyright (c) 2001 Queen of England\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) 2001 Queen of England"),
        "copyrights: {:#?}",
        copyrights.iter().map(|c| &c.copyright).collect::<Vec<_>>()
    );
    assert!(
        holders.iter().any(|h| h.holder == "Queen of England"),
        "holders: {:#?}",
        holders.iter().map(|h| &h.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_libsox_alsa_fixture_keeps_sundry_contributors() {
    let path = PathBuf::from(
        "testdata/copyright-golden/copyrights/libsox_fmt_alsa-libsox_fmt_alsa.copyright",
    );
    let content = fs::read_to_string(&path).expect("libsox alsa fixture must be readable");
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s == "Copyright 1991 Lance Norskog And Sundry Contributors"),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s == "Lance Norskog And Sundry Contributors"),
        "holders: {hs:#?}"
    );

    assert!(
        !cr.iter()
            .any(|s| s == "Copyright 1991 Lance Norskog And Sundry"),
        "copyrights: {cr:#?}"
    );
}

#[test]
fn test_detect_swfobject_copyright_line() {
    let content = "/* SWFObject v2.1 <http://code.google.com/p/swfobject/>\n\
        Copyright (c) 2007-2008 Geoff Stearns, Michael Williams, and Bobby van der Sluis\n\
        This software is released under the MIT License <http://www.opensource.org/licenses/mit-license.php>\n\
*/\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let token_dbg: Vec<Vec<(String, PosTag)>> = groups
        .iter()
        .map(|g| {
            crate::copyright::lexer::get_tokens(g)
                .into_iter()
                .map(|t| (t.value, t.tag))
                .collect::<Vec<_>>()
        })
        .collect();

    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens)
    };
    let has_top_level_nodes = tree.iter().any(|n| {
        matches!(
            n.label(),
            Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2) | Some(TreeLabel::Author)
        )
    });

    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cr.iter().any(|s| {
            s == "Copyright (c) 2007-2008 Geoff Stearns, Michael Williams, and Bobby van der Sluis"
        }),
        "groups: {groups:#?}\ntokens: {token_dbg:#?}\nparsed_has_top_level_nodes: {has_top_level_nodes}\ncopyrights: {cr:#?}"
    );
}

#[test]
fn test_detect_holder_list_continuation_after_comma_and() {
    let content = "Copyright 1996-2002, 2006 by David Turner, Robert Wilhelm, and Werner Lemberg\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let token_dbg: Vec<(String, PosTag)> =
        tokens.iter().map(|t| (t.value.clone(), t.tag)).collect();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens)
    };
    let labels_dbg: Vec<Option<TreeLabel>> = tree.iter().map(|n| n.label()).collect();

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| {
            s == "Copyright 1996-2002, 2006 by David Turner, Robert Wilhelm, and Werner Lemberg"
        }),
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\ncopyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s == "David Turner, Robert Wilhelm, and Werner Lemberg"),
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\nholders: {hs:#?}"
    );
}

#[test]
fn test_detect_long_comma_separated_year_list_with_holder() {
    let content = "Copyright 1994, 1995, 1996, 1997, 1998, 1999, 2000, 2001, 2002, 2003 Free Software Foundation, Inc.\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let token_dbg: Vec<(String, PosTag)> =
        tokens.iter().map(|t| (t.value.clone(), t.tag)).collect();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens)
    };
    let labels_dbg: Vec<Option<TreeLabel>> = tree.iter().map(|n| n.label()).collect();

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
            cr.iter().any(|s| {
                s == "Copyright 1994, 1995, 1996, 1997, 1998, 1999, 2000, 2001, 2002, 2003 Free Software Foundation, Inc."
            }),
            "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\ncopyrights: {cr:#?}"
        );
    assert!(
        hs.iter().any(|s| s == "Free Software Foundation, Inc."),
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\nholders: {hs:#?}"
    );
}

#[test]
fn test_detect_all_caps_holder_not_truncated_tech_sys() {
    let content = "(C) Copyright 1985-1999 ADVANCED TECHNOLOGY SYSTEMS\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s.contains("1985-1999") && s.contains("ADVANCED TECHNOLOGY SYSTEMS")),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter().any(|s| s == "ADVANCED TECHNOLOGY SYSTEMS"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_all_caps_holder_not_truncated_moto_broad() {
    let content = "/****************************************************************************\n\
 *       COPYRIGHT (C) 2005 MOTOROLA, BROADBAND COMMUNICATIONS SECTOR\n\
 *\n\
 *       ALL RIGHTS RESERVED.\n\
 *\n\
 *       NO PART OF THIS CODE MAY BE COPIED OR MODIFIED WITHOUT\n\
 *       THE WRITTEN CONSENT OF MOTOROLA, BROADBAND COMMUNICATIONS SECTOR\n\
 ****************************************************************************/\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let token_dbg: Vec<(String, PosTag)> =
        tokens.iter().map(|t| (t.value.clone(), t.tag)).collect();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens)
    };
    let labels_dbg: Vec<Option<TreeLabel>> = tree.iter().map(|n| n.label()).collect();

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| {
            s.contains("COPYRIGHT")
                && s.contains("2005")
                && s.contains("MOTOROLA")
                && s.contains("BROADBAND COMMUNICATIONS SECTOR")
        }),
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\ncopyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s == "MOTOROLA, BROADBAND COMMUNICATIONS SECTOR"),
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\nholders: {hs:#?}"
    );
}

#[test]
fn test_detect_composite_copy_copyrighted_by_with_trailing_copyright_clause() {
    let content =
        "FaCE is copyrighted by Object Computing, Inc., St. Louis Missouri, Copyright (C) 2002,\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens.clone())
    };

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| {
            s.contains("copyrighted by Object Computing")
                && s.contains("St. Louis Missouri")
                && s.to_ascii_lowercase().contains("copyright")
                && s.contains("2002")
        }),
        "groups: {groups:#?}\n\ntokens: {tokens:#?}\n\ntree: {tree:#?}\n\ncopyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s.contains("Object Computing") && s.contains("St. Louis Missouri")),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_regents_multi_line_merges_year_only_prefix() {
    let content = "Copyright (c) 1988, 1993\nCopyright (c) 1992, 1993\nThe Regents of the University of California. All rights reserved.\n";
    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens.clone())
    };

    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    let merged = "Copyright (c) 1988, 1993 Copyright (c) 1992, 1993 The Regents of the University of California";
    assert!(
        cr.iter().any(|s| s == merged),
        "groups: {groups:#?}\n\ntokens: {tokens:#?}\n\ntree: {tree:#?}\n\ncopyrights: {cr:#?}\n\nholders: {hs:#?}"
    );
    assert!(
        !cr.iter().any(|s| s == "Copyright (c) 1988, 1993"),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s == "The Regents of the University of California"),
        "holders: {hs:#?}"
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
fn test_index_html_first_group_span_extraction_keeps_copyright_word() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    let tokens = get_tokens(&groups[0]);
    let tree = parse(tokens);

    let mut c = Vec::new();
    let mut h = Vec::new();
    let mut a = Vec::new();
    extract_from_spans(&tree, &mut c, &mut h, &mut a, false);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "Span extraction did not produce expected Copyright (c) line. Got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_index_html_first_group_tree_node_extraction_matches_span_extraction() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");

    let numbered_lines: Vec<(usize, String)> = content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    let tokens = get_tokens(&groups[0]);
    let tree = parse(tokens);

    let mut c = Vec::new();
    let mut h = Vec::new();
    let mut a = Vec::new();
    extract_from_tree_nodes(&tree, &mut c, &mut h, &mut a, false);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "Tree-node extraction did not produce expected Copyright (c) line. Got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_index_html_end_to_end_has_copyright_word() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");
    let (c, _h, _a) = detect_copyrights_from_text(&content);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "End-to-end detection missing expected Copyright (c) line. Got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );

    assert!(
        !c.iter()
            .any(|cr| cr.copyright == "(c) 2002-2009 Charlie Poole"),
        "Expected bare (c) variant to be dropped. Got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_index_html_does_not_emit_shadowed_digia_plc_holder() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("testdata/copyright-golden/copyrights/index.html");
    let content = fs::read_to_string(&path).expect("read index.html fixture");
    let (_c, h, _a) = detect_copyrights_from_text(&content);

    assert!(
        h.iter().any(|hd| {
            hd.holder == "Digia Plc and/or its subsidiary(-ies) and other contributors"
        }),
        "Expected full Digia holder, got: {:?}",
        h.iter().map(|hd| &hd.holder).collect::<Vec<_>>()
    );

    assert!(
        !h.iter().any(|hd| hd.holder == "Digia Plc"),
        "Expected shadowed short holder to be dropped, got: {:?}",
        h.iter().map(|hd| &hd.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_mpl_portions_created_prefix_preserved() {
    let input = "Portions created by the Initial Developer are Copyright (C) 2002\n  the Initial Developer.";
    let (c, h, _a) = detect_copyrights_from_text(input);

    assert!(
            c.iter().any(|cr| {
                cr.copyright
                    == "Portions created by the Initial Developer are Copyright (c) 2002 the Initial Developer"
            }),
            "Expected MPL portions-created prefix preserved, got: {:?}",
            c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
        );

    assert!(
        h.iter().any(|hd| hd.holder == "the Initial Developer"),
        "Expected holder 'the Initial Developer', got: {:?}",
        h.iter().map(|hd| &hd.holder).collect::<Vec<_>>()
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
fn test_bare_c_year_only_detected() {
    let input = "(c) 2008";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "(c) 2008"),
        "Expected bare (c) year, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_drop_symbol_year_only_copyright() {
    let input = "Copyright © 2021\nCopyright (c) 2017\n";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        !c.iter().any(|cr| cr.copyright == "Copyright (c) 2021"),
        "Expected © year-only to be dropped, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        c.iter().any(|cr| cr.copyright == "Copyright (c) 2017"),
        "Expected non-© year-only to be kept, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_c_sign_path_fragment_is_not_detected_as_copyright() {
    let input = "(C)Ljoptsimple/AbstractOptionSpec";
    let (c, h, a) = detect_copyrights_from_text(input);
    assert!(c.is_empty(), "copyrights: {c:#?}");
    assert!(h.is_empty(), "holders: {h:#?}");
    assert!(a.is_empty(), "authors: {a:#?}");
}

#[test]
fn test_copyright_scan_phrase_is_not_detected_as_copyright() {
    let input = "Measures the end-to-end composer copyright scan";
    let (c, h, a) = detect_copyrights_from_text(input);
    assert!(c.is_empty(), "copyrights: {c:#?}");
    assert!(h.is_empty(), "holders: {h:#?}");
    assert!(a.is_empty(), "authors: {a:#?}");
}

#[test]
fn test_generated_annotation_line_is_not_absorbed_into_copyright() {
    let input = "/* Copyright (C) 2024 Acme Corp.\n * @generated by protobuf */";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2024 Acme Corp."),
        "copyrights: {c:#?}"
    );
    assert!(
        !c.iter().any(|cr| cr.copyright.contains("@generated")),
        "copyrights: {c:#?}"
    );
    assert!(
        h.iter().any(|holder| holder.holder == "Acme Corp."),
        "holders: {h:#?}"
    );
}

#[test]
fn test_dart_structured_literal_keys_are_not_absorbed_into_marvel_copyright() {
    let input = "'copyright': '© 2020 MARVEL',\n'attributionText': 'Data provided by Marvel. © 2020 MARVEL',\n'etag': 'eba58984956be48bdfd28818fa4fad1ff5f5cf81',\n'data': {}";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|entry| entry.copyright == "(c) 2020 MARVEL"),
        "copyrights: {copyrights:#?}"
    );
    assert!(
        copyrights
            .iter()
            .any(|entry| entry.copyright == "Marvel. (c) 2020 MARVEL"),
        "copyrights: {copyrights:#?}"
    );
    assert!(
        !copyrights.iter().any(|entry| {
            entry.copyright.contains("attributionText") || entry.copyright.contains("etag")
        }),
        "copyrights: {copyrights:#?}"
    );
    assert!(
        holders.iter().any(|entry| entry.holder == "MARVEL"),
        "holders: {holders:#?}"
    );
    assert!(
        holders.iter().any(|entry| entry.holder == "Marvel. MARVEL"),
        "holders: {holders:#?}"
    );
    assert!(
        !holders
            .iter()
            .any(|entry| entry.holder.contains("attributionText") || entry.holder.contains("etag")),
        "holders: {holders:#?}"
    );
}

#[test]
fn test_author_colon_multiline_keeps_emails() {
    let input = "/*\n * Authors: Jorge Cwik, <jorge@laser.satlink.net>\n *\t\tArnt Gulbrandsen, <agulbra@nvg.unit.no>\n */\n";

    let raw_lines: Vec<&str> = input.lines().collect();
    let mut prepared_cache = crate::copyright::line_tracking::PreparedLineCache::new(&raw_lines);
    let mut extracted: Vec<AuthorDetection> = Vec::new();
    super::author_heuristics::extract_author_colon_blocks(&mut prepared_cache, &mut extracted);
    assert!(
        extracted.iter().any(|ad| ad.author
            == "Jorge Cwik, <jorge@laser.satlink.net> Arnt Gulbrandsen, <agulbra@nvg.unit.no>"),
        "Expected direct author-colon extraction to keep emails, got: {:?}",
        extracted.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );

    let (_c, _h, a) = detect_copyrights_from_text(input);

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
    let mut prepared_cache = crate::copyright::line_tracking::PreparedLineCache::new(&raw_lines);
    let mut extracted: Vec<AuthorDetection> = Vec::new();
    super::author_heuristics::extract_author_colon_blocks(&mut prepared_cache, &mut extracted);
    assert!(
        extracted
            .iter()
            .any(|ad| { ad.author == "Richard Walker, Jamie Honan, Michael Hunold" }),
        "Expected empty-tail Authors: block to merge following roster lines, got: {:?}",
        extracted.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );

    let (_c, _h, a) = detect_copyrights_from_text(input);
    assert!(
        a.iter()
            .any(|ad| ad.author == "Richard Walker, Jamie Honan, Michael Hunold"),
        "Expected pipeline to keep merged roster author block, got: {:?}",
        a.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_dense_name_email_author_lists_still_extract_with_copyright_present() {
    let input = "Copyright (C) 2004 BULL SA.\n\nPaul Jackson <pj@sgi.com>\nChristoph Lameter <cl@gentwo.org>\nHidetoshi Seto <seto.hidetoshi@jp.fujitsu.com>\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = authors
        .iter()
        .map(|author| author.author.as_str())
        .collect();
    assert!(
        values.contains(&"Paul Jackson <pj@sgi.com>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Christoph Lameter <cl@gentwo.org>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Hidetoshi Seto <seto.hidetoshi@jp.fujitsu.com>"),
        "authors: {values:?}"
    );
}

#[test]
fn test_developer_section_prose_is_not_extracted_as_author() {
    let input = "stable/\n\tThis directory documents the interfaces that the developer has\n\tdefined to be stable.\n\nUsers:\t\tAll users of this interface who wish to be notified when\n\t\tit changes.  This is very important for interfaces in\n\t\tthe \"testing\" stage, so that kernel developers can work\n\t\twith userspace developers to ensure that things do not\n\t\tbreak in ways that are unacceptable.\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_modified_by_lines_extract_real_authors_without_cpusets_prose_false_positives() {
    let input = "Written by Simon.Derr@bull.net\n- Modified by Paul Jackson <pj@sgi.com>\n- Modified by Christoph Lameter <cl@gentwo.org>\n- Modified by Paul Menage <menage@google.com>\n- Modified by Hidetoshi Seto <seto.hidetoshi@jp.fujitsu.com>\n\nCPUs and Memory Nodes, and attached tasks, are modified by writing\nto the appropriate file in that cpusets directory.\n\nExcept perhaps as modified by the task's NUMA mempolicy or cpuset\nconfiguration, so long as sufficient free memory pages are available.\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = authors
        .iter()
        .map(|author| author.author.as_str())
        .collect();
    assert!(
        values.contains(&"Simon.Derr@bull.net"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Paul Jackson <pj@sgi.com>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Christoph Lameter <cl@gentwo.org>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Paul Menage <menage@google.com>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Hidetoshi Seto <seto.hidetoshi@jp.fujitsu.com>"),
        "authors: {values:?}"
    );
    assert!(
        !values
            .iter()
            .any(|value| value.contains("writing to the appropriate")),
        "authors: {values:?}"
    );
    assert!(
        !values.iter().any(|value| value.contains("NUMA mempolicy")),
        "authors: {values:?}"
    );
}

#[test]
fn test_rst_field_author_and_maintainer_extracts_single_author() {
    let input = ":License:\t\tGPLv2\n:Author & Maintainer:\tMiguel Ojeda <ojeda@kernel.org>\n:Date:\t\t\t2006-10-27\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = authors
        .iter()
        .map(|author| author.author.as_str())
        .collect();
    assert!(
        values.contains(&"Miguel Ojeda <ojeda@kernel.org>"),
        "authors: {values:?}"
    );
}

#[test]
fn test_maintainers_label_extracts_author_and_trims_gitrepo_suffix() {
    let input = "Maintainers Tianon Gravi <admwiggin@gmail.com> (@tianon) GitRepo https://github.com/tianon/docker-bash.git\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = authors
        .iter()
        .map(|author| author.author.as_str())
        .collect();
    assert!(
        values.contains(&"Maintainers Tianon Gravi <admwiggin@gmail.com> (@tianon)"),
        "authors: {values:?}"
    );
    assert!(
        !values.iter().any(|value| value.contains("GitRepo")),
        "authors: {values:?}"
    );
}

#[test]
fn test_maintainers_label_without_email_does_not_extract_author() {
    let input = "Maintainers the Docker Community\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_dash_bullet_changelog_lines_extract_individual_authors() {
    let input = "- Written by Mydraal <vulpyne@vulpyne.net>\n- Updated by Adam Sulmicki <adam@cfar.umd.edu>\n- Updated by Jeremy M. Dolan <jmd@turbogeek.org> 2001/01/28 10:15:59\n- Added to by Crutcher Dunnavant <crutcher+kernel@datastacks.com>\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = authors
        .iter()
        .map(|author| author.author.as_str())
        .collect();
    assert!(
        values.contains(&"Mydraal <vulpyne@vulpyne.net>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Adam Sulmicki <adam@cfar.umd.edu>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Jeremy M. Dolan <jmd@turbogeek.org>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Crutcher Dunnavant <crutcher+kernel@datastacks.com>"),
        "authors: {values:?}"
    );
    assert!(
        !values
            .iter()
            .any(|value| value.contains("Updated by Adam Sulmicki")),
        "authors: {values:?}"
    );
}

#[test]
fn test_author_colon_dash_bullet_hwmon_roster_extracts_individual_authors() {
    let input = "Authors:\n\t- Mark M. Hoffman <mhoffman@lightlink.com>\n\t- Ported to 2.6 by Eric J. Bowersox <ericb@aspsys.com>\n\t- Adapted to 2.6.20 by Carsten Emde <ce@osadl.org>\n\t- Modified for mainline integration by Hans J. Koch <hjk@hansjkoch.de>\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    let values: Vec<&str> = authors
        .iter()
        .map(|author| author.author.as_str())
        .collect();
    assert!(
        values.contains(&"Mark M. Hoffman <mhoffman@lightlink.com>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Eric J. Bowersox <ericb@aspsys.com>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Carsten Emde <ce@osadl.org>"),
        "authors: {values:?}"
    );
    assert!(
        values.contains(&"Hans J. Koch <hjk@hansjkoch.de>"),
        "authors: {values:?}"
    );
}

#[test]
fn test_passive_written_phrase_does_not_create_abi_author_false_positive() {
    let input = "Description:\tWhen read, this file returns general data like firmware version.\n\t\tWhen written, the device can be reset.\n\t\tBefore reading this file, control has to be written to select\n\t\twhich profile to read.\n";

    let (_c, _h, authors) = detect_copyrights_from_text(input);
    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_copyright_year_range_only_detected() {
    let input = "Copyright (c) 1995-1999.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "Copyright (c) 1995-1999"),
        "Expected Copyright (c) year range, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_copyright_year_range_only_without_c_detected() {
    let input = "Copyright 2013-2015,";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "Copyright 2013-2015"),
        "Expected Copyright year range, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_parts_copyright_prefix_preserved() {
    let input = "Parts Copyright (C) 1992 Uri Blumenthal, IBM";
    let (c, _h, _a) = detect_copyrights_from_text(input);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Parts Copyright (c) 1992 Uri Blumenthal, IBM"),
        "Expected Parts prefix preserved, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_copyright_prefix_preserved_after_name() {
    let input = "Adobe(R) Flash(R) Player. Copyright (C) 1996 - 2008. Adobe Systems Incorporated. All Rights Reserved.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("Copyright")),
        "Should preserve 'Copyright' prefix when preceded by a name, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_copyright_with_email() {
    let (c, h, _a) = detect_copyrights_from_text(
        "Copyright (c) 2009 Masayuki Hatta (mhatta) <mhatta@debian.org>",
    );
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(
        c[0].copyright,
        "Copyright (c) 2009 Masayuki Hatta (mhatta) <mhatta@debian.org>"
    );
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Masayuki Hatta");
}

#[test]
fn test_detect_copyright_with_short_holder_and_trailing_punct_email() {
    let input = "Copyright (c) 2024 bgme <i@bgme.me>.";
    let numbered_lines: Vec<(usize, String)> = input
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = collect_candidate_lines(numbered_lines);
    assert!(
        !groups.is_empty(),
        "Expected candidate group, got: {groups:?}"
    );

    let (c, h, _a) = detect_copyrights_from_text(input);
    assert_eq!(
        c.len(),
        1,
        "Should detect one copyright, got: {:?}; groups: {:?}",
        c,
        groups
    );
    assert_eq!(
        c[0].copyright, "Copyright (c) 2024 bgme <i@bgme.me>",
        "Copyright text: {:?}",
        c[0].copyright
    );
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "bgme");
}

#[test]
fn test_detect_copyright_compact_c_parens_with_lowercase_holder_and_email() {
    let input = "Copyright(c) 2014 dead_horse <dead_horse@qq.com>";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2014 dead_horse <dead_horse@qq.com>"),
        "Expected copyright detected, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hd| hd.holder == "dead_horse"),
        "Expected holder detected, got: {:?}",
        h.iter().map(|hd| &hd.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_lowercase_username_email_in_parens_fragment() {
    let input = "Adapted from bzip2.js, copyright 2011 antimatter15 (antimatter15@gmail.com).";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "copyright 2011 antimatter15 (antimatter15@gmail.com)"),
        "Expected extracted copyright fragment, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hd| hd.holder == "antimatter15"),
        "Expected extracted holder, got: {:?}",
        h.iter().map(|hd| &hd.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_copy_entity_year_range_only() {
    let input = "expectedHtml = \"<p>Copyright &copy; 2003-2014</p>\",";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "Copyright (c) 2003-2014"),
        "Expected Copyright (c) year range extracted, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_hex_a9_entity_year_range_only_as_bare_c() {
    let input = "expectedXml = \"<p>Copyright &#xA9; 2003-2014</p>\",";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "(c) 2003-2014"),
        "Expected (c) year range extracted, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_are_copyright_c_year_range_clause() {
    let input = "Portions created by Ricoh Silicon Valley, Inc. are Copyright (C) 1995-1999. All Rights Reserved.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "Copyright (c) 1995-1999"),
        "Expected year-range clause extracted, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_empty_input() {
    let (c, h, a) = detect_copyrights_from_text("");
    assert!(c.is_empty());
    assert!(h.is_empty());
    assert!(a.is_empty());
}

#[test]
fn test_detect_no_copyright() {
    let (c, h, a) = detect_copyrights_from_text("This is just some random code.");
    assert!(c.is_empty());
    assert!(h.is_empty());
    assert!(a.is_empty());
}

#[test]
fn test_detect_simple_copyright() {
    let (c, h, _a) = detect_copyrights_from_text("Copyright 2024 Acme Inc.");
    assert!(!c.is_empty(), "Should detect copyright");
    assert!(
        c[0].copyright.contains("Copyright"),
        "Copyright text: {}",
        c[0].copyright
    );
    assert!(
        c[0].copyright.contains("2024"),
        "Should contain year: {}",
        c[0].copyright
    );
    assert_eq!(c[0].start_line, LineNumber::ONE);
    assert!(!h.is_empty(), "Should detect holder");
}

#[test]
fn test_detect_spdx_filecopyrighttext_contributors_to_project() {
    let input = "SPDX-FileCopyrightText: © 2020 Contributors to the project Clay <https://github.com/liferay/clay/graphs/contributors>";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
            c.iter().any(|cr| cr.copyright == "Copyright (c) 2020 Contributors to the project Clay https://github.com/liferay/clay/graphs/contributors"),
            "Missing SPDX-FileCopyrightText copyright, got: {:?}",
            c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
        );
    assert!(
        h.iter()
            .any(|ho| ho.holder == "Contributors to the project Clay"),
        "Missing SPDX-FileCopyrightText holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_contributors_as_noted_in_authors_file() {
    let input = "Copyright (c) 2020 Contributors as noted in the AUTHORS file";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == input),
        "Missing copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter()
            .any(|ho| ho.holder == "Contributors as noted in the AUTHORS file"),
        "Missing holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_contributors_et_al() {
    let input = "Copyright (c) 2017 Contributors et.al.";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2017 Contributors et.al"),
        "Missing copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|ho| ho.holder == "Contributors et.al"),
        "Missing holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_joyent_document_authors_keeps_company_prefix() {
    let input = "Copyright (c) 2011 Joyent, Inc. and the persons identified as document authors.";
    let (c, h, a) = detect_copyrights_from_text(input);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2011 Joyent, Inc."),
        "Missing Joyent copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|ho| ho.holder == "Joyent, Inc."),
        "Missing Joyent holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_detect_not_copyrighted_statement() {
    let input = "Not copyrighted 1992 by Mark Adler";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == input),
        "Missing copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|ho| ho.holder == "Not by Mark Adler"),
        "Missing holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_fixture_adler_inflate_not_copyrighted() {
    let content =
        std::fs::read_to_string("testdata/copyright-golden/copyrights/adler_inflate_c-inflate_c.c")
            .unwrap();
    let (c, h, _a) = detect_copyrights_from_text(&content);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Not copyrighted 1992 by Mark Adler"),
        "Missing expected copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|ho| ho.holder == "Not by Mark Adler"),
        "Missing expected holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_fixture_linux_inflate_not_copyrighted_normalized() {
    let content = std::fs::read_to_string(
        "testdata/copyright-golden/copyrights/misco4/linux-copyrights/lib/inflate.c",
    )
    .unwrap();
    let (c, h, _a) = detect_copyrights_from_text(&content);
    let cr_texts: Vec<&str> = c.iter().map(|cr| cr.copyright.as_str()).collect();
    assert!(
        cr_texts.contains(&"copyrighted 1990 Mark Adler"),
        "Missing 1990 expected copyright, got: {:?}",
        cr_texts
    );
    assert!(
        cr_texts.contains(&"copyrighted 1992 by Mark Adler"),
        "Missing 1992 expected copyright, got: {:?}",
        cr_texts
    );
    assert!(
        h.iter().any(|ho| ho.holder == "Mark Adler"),
        "Missing expected holder, got: {:?}",
        h.iter().map(|ho| &ho.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_detect_copyright_c_symbol() {
    let (c, h, _a) = detect_copyrights_from_text("Copyright (c) 2020-2024 Foo Bar");
    assert!(!c.is_empty(), "Should detect copyright with (c)");
    assert_eq!(c[0].copyright, "Copyright (c) 2020-2024 Foo Bar");
    assert!(!h.is_empty(), "Should detect holder");
}

#[test]
fn test_detect_copyright_c_symbol_with_all_rights_reserved() {
    let (c, _, _) = detect_copyrights_from_text(
        "Copyright (c) 1999-2002 Zend Technologies Ltd. All rights reserved.",
    );
    assert_eq!(
        c[0].copyright,
        "Copyright (c) 1999-2002 Zend Technologies Ltd."
    );
}

#[test]
fn test_detect_copyright_unicode_symbol() {
    let (c, _, _) = detect_copyrights_from_text(
        "/* Copyright \u{00A9} 2000 ACME, Inc., All Rights Reserved */",
    );
    assert!(!c.is_empty(), "Should detect copyright with \u{00A9}");
    assert!(
        c[0].copyright.starts_with("Copyright"),
        "Should start with Copyright, got: {}",
        c[0].copyright
    );
}

#[test]
fn test_detect_copyright_c_no_all_rights() {
    let (c, _, _) = detect_copyrights_from_text("Copyright (c) 2009 Google");
    assert!(!c.is_empty());
    assert_eq!(c[0].copyright, "Copyright (c) 2009 Google");
}

#[test]
fn test_detect_copyright_c_multiline() {
    let input = "Copyright (c) 2001 by the TTF2PT1 project\nCopyright (c) 2001 by Sergey Babkin";
    let (c, _, _) = detect_copyrights_from_text(input);
    assert_eq!(c.len(), 2, "Should detect two copyrights, got: {:?}", c);
    assert_eq!(c[0].copyright, "Copyright (c) 2001 by the TTF2PT1 project");
    assert_eq!(c[1].copyright, "Copyright (c) 2001 by Sergey Babkin");
}

#[test]
fn test_detect_multiline_copyright() {
    let text = "Copyright 2024\n  Acme Corporation\n  All rights reserved.";
    let (c, _h, _a) = detect_copyrights_from_text(text);
    assert!(!c.is_empty(), "Should detect multiline copyright");
}

#[test]
fn test_detect_author() {
    let (c, h, a) = detect_copyrights_from_text("Written by John Doe");
    // "Written" is tagged Auth2, triggering author span extraction.
    assert!(c.is_empty(), "Should not detect copyright");
    assert!(h.is_empty(), "Should not detect holder");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "John Doe");
    assert_eq!(a[0].start_line, LineNumber::ONE);
    assert_eq!(a[0].end_line, LineNumber::ONE);
}

#[test]
fn test_detect_author_from_xml_author_attribute() {
    let text = r#"<note author="Vinnie Falco">C++11 is the minimum requirement.</note>"#;
    let (c, h, a) = detect_copyrights_from_text(text);

    assert!(c.is_empty(), "Should not detect copyright");
    assert!(h.is_empty(), "Should not detect holder");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Vinnie Falco");
    assert_eq!(a[0].start_line, LineNumber::ONE);
    assert_eq!(a[0].end_line, LineNumber::ONE);
}

#[test]
fn test_detect_author_from_xml_author_attribute_without_note_body_noise() {
    let text = r#"<note author="Chris Kohlhoff">
This compiler does not support enable_if, which is needed by the library.
</note>"#;
    let (_c, _h, a) = detect_copyrights_from_text(text);

    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Chris Kohlhoff");
}

#[test]
fn test_detect_author_from_xml_author_attribute_decodes_entities() {
    let text = r#"<note author="Joaqu&#237;n M L&#243;pez Mu&#241;oz">Compiler bug.</note>"#;
    let (_c, _h, a) = detect_copyrights_from_text(text);

    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Joaquín M López Muñoz");
}

#[test]
fn test_detect_author_from_repeated_xml_author_attributes_keeps_multiple_occurrences() {
    let text = r#"<mark-expected-failures>
<note author="Aleksey Gurtovoy" refid="4"/>
<note author="Aleksey Gurtovoy" refid="19"/>
</mark-expected-failures>"#;
    let (_c, _h, a) = detect_copyrights_from_text(text);

    let matching: Vec<_> = a
        .iter()
        .filter(|ad| ad.author == "Aleksey Gurtovoy")
        .collect();
    assert_eq!(matching.len(), 2, "authors: {a:#?}");
    assert_eq!(matching[0].start_line, LineNumber::new(2).expect("valid"));
    assert_eq!(matching[1].start_line, LineNumber::new(3).expect("valid"));
}

#[test]
fn test_detect_author_from_xml_author_attribute_splits_obvious_multi_name_lists() {
    let text = r#"<note author="Robert Ramey,Roland Schwarz" date="16 Feb 07" refid="19"/>"#;
    let (_c, _h, a) = detect_copyrights_from_text(text);

    let names: Vec<&str> = a.iter().map(|ad| ad.author.as_str()).collect();
    assert!(names.contains(&"Robert Ramey"), "authors: {names:?}");
    assert!(names.contains(&"Roland Schwarz"), "authors: {names:?}");
    assert_eq!(names.len(), 2, "authors: {names:?}");
}

#[test]
fn test_detect_docbook_html_authorgroup_authors() {
    let text = r#"<div class="authorgroup">
<div class="author"><h3 class="author"><span class="firstname">John</span> <span class="surname">Maddock</span></h3></div>
<div class="author"><h3 class="author"><span class="firstname">Joel</span> <span class="surname">de Guzman</span></h3></div>
<div class="author"><h3 class="author"><span class="firstname">Eric</span> <span class="surname">Niebler</span></h3></div>
<div class="author"><h3 class="author"><span class="firstname">Matias</span> <span class="surname">Capeletto</span></h3></div>
</div>"#;
    let (_c, _h, a) = detect_copyrights_from_text(text);
    let names: Vec<&str> = a.iter().map(|d| d.author.as_str()).collect();

    assert!(names.contains(&"John Maddock"), "authors: {names:?}");
    assert!(names.contains(&"Joel de Guzman"), "authors: {names:?}");
    assert!(names.contains(&"Eric Niebler"), "authors: {names:?}");
    assert!(names.contains(&"Matias Capeletto"), "authors: {names:?}");
}

#[test]
fn test_detect_created_by_current_user_comment_is_not_author() {
    let text = "Get the IDs of pipelines created by the current user on the same branch.";
    let (_c, _h, a) = detect_copyrights_from_text(text);
    assert!(a.is_empty(), "authors: {a:?}");
}

#[test]
fn test_detect_junk_filtered() {
    let (c, _h, _a) = detect_copyrights_from_text("Copyright (c)");
    // "Copyright (c)" alone is junk.
    assert!(
        c.is_empty(),
        "Bare 'Copyright (c)' should be filtered as junk"
    );
}

#[test]
fn test_detect_multiple_copyrights() {
    let text = "Copyright 2020 Foo Inc.\n\n\n\nCopyright 2024 Bar Corp.";
    let (c, h, _a) = detect_copyrights_from_text(text);
    assert!(
        c.len() >= 2,
        "Should detect two copyrights, got {}: {:?}",
        c.len(),
        c
    );
    assert!(
        h.len() >= 2,
        "Should detect two holders, got {}: {:?}",
        h.len(),
        h
    );
}

#[test]
fn test_detect_spdx_copyright() {
    let (c, _h, _a) = detect_copyrights_from_text("SPDX-FileCopyrightText: 2024 Example Corp");
    assert!(!c.is_empty(), "Should detect SPDX copyright");
    // The refiner normalizes SPDX-FileCopyrightText to Copyright.
    assert!(
        c[0].copyright.contains("Copyright"),
        "Should normalize to Copyright: {}",
        c[0].copyright
    );
}

#[test]
fn test_detect_line_numbers() {
    let text = "Some header\nCopyright 2024 Acme Inc.\nSome footer";
    let (c, _h, _a) = detect_copyrights_from_text(text);
    assert!(!c.is_empty(), "Should detect copyright");
    assert_eq!(
        c[0].start_line,
        LineNumber::new(2).unwrap(),
        "Copyright should be on line 2"
    );
}

#[test]
fn test_detect_copyright_year_range() {
    let (c, h, _a) = detect_copyrights_from_text("Copyright 2020-2024 Foo Corp.");
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(c[0].copyright, "Copyright 2020-2024 Foo Corp.");
    assert_eq!(c[0].start_line, LineNumber::ONE);
    assert_eq!(c[0].end_line, LineNumber::ONE);
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Foo Corp.");
    assert_eq!(h[0].start_line, LineNumber::ONE);
}

#[test]
fn test_fixture_sample_py_motorola_holder_has_dash_variant_only() {
    let content =
        fs::read_to_string("testdata/copyright-golden/copyrights/sample_py-py.py").unwrap();

    let (_c, h, _a) = detect_copyrights_from_text(&content);
    let hs: Vec<&str> = h.iter().map(|d| d.holder.as_str()).collect();

    assert!(
        hs.contains(&"Motorola, Inc. - Motorola Confidential Proprietary"),
        "holders: {hs:?}"
    );
    assert!(
        !hs.contains(&"Motorola, Inc. Motorola Confidential Proprietary"),
        "holders: {hs:?}"
    );
}

#[test]
fn test_mso_document_properties_non_confidential_uses_template_lastauthor_variant() {
    let content = "<o:Description>Copyright 2009</o:Description>\n<o:Template>techdoc.dot</o:Template>\n<o:LastAuthor>Jennifer Hruska</o:LastAuthor>";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright 2009 techdoc.dot o:LastAuthor Jennifer Hruska"),
        "copyrights: {:?}",
        copyrights
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "techdoc.dot o:LastAuthor Jennifer Hruska"),
        "holders: {:?}",
        holders
    );
    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright == "Jennifer Hruska Copyright 2009")
    );
    assert!(!holders.iter().any(|h| h.holder == "Jennifer Hruska"));
}

#[test]
fn test_mso_document_properties_confidential_does_not_emit_template_lastauthor_variant() {
    let content = "<o:Description>Copyright 2009 Confidential Information</o:Description>\n<o:Template>techdoc.dot</o:Template>\n<o:LastAuthor>Jennifer Hruska</o:LastAuthor>";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright 2009 Confidential"),
        "copyrights: {:?}",
        copyrights
    );
    assert!(
        holders.iter().any(|h| h.holder == "Confidential"),
        "holders: {:?}",
        holders
    );
    assert!(
        !copyrights.iter().any(|c| c
            .copyright
            .contains("techdoc.dot o:LastAuthor Jennifer Hruska")),
        "copyrights: {:?}",
        copyrights
    );
    assert!(
        !holders.iter().any(|h| h
            .holder
            .contains("techdoc.dot o:LastAuthor Jennifer Hruska")),
        "holders: {:?}",
        holders
    );
}

#[test]
fn test_detect_copyright_holder_suffix_authors() {
    let (c, h, a) = detect_copyrights_from_text("Copyright 2015 The Error Prone Authors.");
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright 2015 The Error Prone Authors"),
        "Should keep 'Authors' as part of holder in copyright: {:?}",
        c
    );
    assert!(
        h.iter().any(|hd| hd.holder == "The Error Prone Authors"),
        "Should keep 'Authors' as part of holder: {:?}",
        h
    );
    assert!(
        a.is_empty(),
        "Should not treat trailing 'Authors' token as an author: {:?}",
        a
    );
}

#[test]
fn test_detect_filters_code_like_c_marker_lines() {
    let text = "(c) (const unsigned char*)ptr\n(c) c ? foo : bar\n(c) c & 0x3f\n(c) flags |= 0x80";
    let (copyrights, holders, authors) = detect_copyrights_from_text(text);
    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_complex_html_preserves_parenthesized_obfuscated_email_continuation() {
    let content =
        fs::read_to_string("testdata/copyright-golden/copyrights/misco4/linux9/complex-html.txt")
            .unwrap();

    let (copyrights, _holders, _authors) = detect_copyrights_from_text(&content);
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) 2001 Karl Garrison (karl AT indy.rr.com)"),
        "copyrights: {:?}",
        copyrights
    );
}

#[test]
fn test_detect_copyright_holder_suffix_university() {
    let (c, h, a) = detect_copyrights_from_text("Copyright (c) 2001, Rice University");
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2001, Rice University"),
        "Should keep trailing University token in copyright: {:?}",
        c
    );
    assert!(
        h.iter().any(|hd| hd.holder == "Rice University"),
        "Should keep trailing University token in holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_detect_copyright_holder_suffix_as_represented() {
    let text = "Copyright: (c) 2000 United States Government as represented by the\nSecretary of the Navy. All rights reserved.";
    let (c, h, _a) = detect_copyrights_from_text(text);
    assert!(
            c.iter().any(|cr| {
                cr.copyright
                    == "Copyright (c) 2000 United States Government as represented by the Secretary of the Navy"
            }),
            "Should keep 'as represented by' continuation in copyright: {:?}",
            c
        );
    assert!(
        h.iter().any(|hd| {
            hd.holder == "United States Government as represented by the Secretary of the Navy"
        }),
        "Should keep 'as represented by' continuation in holder: {:?}",
        h
    );
}

#[test]
fn test_detect_copyright_does_not_absorb_unexpected_as_represented() {
    let text = "Copyright 1993 United States Government as represented by the\nDirector, National Security Agency.";
    let (c, h, _a) = detect_copyrights_from_text(text);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright 1993 United States Government"),
        "Should keep only government without continuation: {:?}",
        c
    );
    assert!(
        h.iter().any(|hd| hd.holder == "United States Government"),
        "Should keep only government holder without continuation: {:?}",
        h
    );
}

#[test]
fn test_detect_copyright_holder_suffix_committers() {
    let (c, h, a) =
        detect_copyrights_from_text("Copyright (c) 2006, 2007, 2008 XStream committers");
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2006, 2007, 2008 XStream committers"),
        "Should keep 'committers' as part of holder in copyright: {:?}",
        c
    );
    assert!(
        h.iter().any(|hd| hd.holder == "XStream committers"),
        "Should keep 'committers' as part of holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_detect_copyright_holder_suffix_contributors_only() {
    let (c, h, a) = detect_copyrights_from_text("Copyright (c) 2015, Contributors");
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2015, Contributors"),
        "Should keep Contributors in copyright: {:?}",
        c
    );
    assert!(
        h.iter().any(|hd| hd.holder == "Contributors"),
        "Should detect Contributors as holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_detect_copyright_holder_suffix_authors_and_contributors() {
    let text = "Copyright 2018-2019 @paritytech/substrate-light-ui authors & contributors";
    let prepared = super::super::prepare::prepare_text_line(text);
    let tokens = get_tokens(&[(1, prepared)]);
    let tree = parse(tokens);
    let (copyright_idx, copyright_node) = tree
        .iter()
        .enumerate()
        .find(|(_i, n)| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        })
        .expect("Should parse a COPYRIGHT node");
    let start = copyright_idx + 1;
    assert!(
        should_start_absorbing(copyright_node, &tree, start),
        "Should start absorbing trailing suffix nodes; tree={:?}",
        tree
    );
    let (trailing, _skip) = collect_trailing_orphan_tokens(copyright_node, &tree, start);
    assert!(
        trailing
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("authors")),
        "Trailing tokens should include 'authors', got: {:?}",
        trailing
    );
    assert!(
        trailing
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("contributors")),
        "Trailing tokens should include 'contributors', got: {:?}",
        trailing
    );

    let (c, h, a) = detect_copyrights_from_text(text);
    assert!(
        c.iter().any(|cr| cr.copyright == text),
        "Should keep authors/contributors suffix in copyright: {:?}",
        c
    );
    assert!(
        h.iter()
            .any(|hd| hd.holder == "paritytech/substrate-light-ui authors & contributors"),
        "Should keep authors/contributors suffix in holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_detect_copyright_holder_suffix_the_respective_contributors() {
    let text = "Copyright (c) 2014, 2015, the respective contributors";
    let prepared = super::super::prepare::prepare_text_line(text);
    let tokens = get_tokens(&[(1, prepared)]);
    let tree = parse(tokens);
    let (copyright_idx, copyright_node) = tree
        .iter()
        .enumerate()
        .find(|(_i, n)| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        })
        .expect("Should parse a COPYRIGHT node");
    let start = copyright_idx + 1;
    assert!(
        should_start_absorbing(copyright_node, &tree, start),
        "Should start absorbing respective-contributors suffix; tree={:?}",
        tree
    );
    let (trailing, _skip) = collect_trailing_orphan_tokens(copyright_node, &tree, start);
    assert!(
        trailing
            .iter()
            .any(|t| t.value.eq_ignore_ascii_case("contributors")),
        "Trailing tokens should include 'contributors', got: {:?}",
        trailing
    );

    let (c, h, a) = detect_copyrights_from_text(text);

    assert!(
        c.iter().any(|cr| cr.copyright == text),
        "Should keep the full respective-contributors suffix in copyright: {:?}",
        c
    );
    assert!(
        h.iter()
            .any(|hd| hd.holder == "the respective contributors"),
        "Should detect 'the respective contributors' as holder: {:?}",
        h
    );
    assert!(a.is_empty(), "Unexpected authors detected: {:?}", a);
}

#[test]
fn test_collect_trailing_orphan_tokens_keeps_confidential_and_proprietary_phrase() {
    let text = "(c) Example Corp. and affiliates. Confidential and proprietary.";
    let prepared = super::super::prepare::prepare_text_line(text);
    let tokens = get_tokens(&[(1, prepared)]);
    let tree = parse(tokens);
    let (copyright_idx, copyright_node) = tree
        .iter()
        .enumerate()
        .find(|(_i, n)| {
            matches!(
                n.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        })
        .expect("Should parse a COPYRIGHT node");
    let start = copyright_idx + 1;

    assert!(
        should_start_absorbing(copyright_node, &tree, start),
        "Should absorb trailing confidentiality phrase; tree={:?}",
        tree
    );

    let (trailing, _skip) = collect_trailing_orphan_tokens(copyright_node, &tree, start);
    let trailing_values: Vec<&str> = trailing.iter().map(|t| t.value.as_str()).collect();

    assert!(
        trailing_values.contains(&"and"),
        "Trailing tokens should include trailing conjunction, got: {:?}",
        trailing_values
    );
    assert!(
        trailing_values.contains(&"proprietary."),
        "Trailing tokens should include 'proprietary.', got: {:?}",
        trailing_values
    );
}

#[test]
fn test_detect_copyright_unicode_holder() {
    let (c, h, _a) = detect_copyrights_from_text("Copyright 2024 François Müller");
    assert!(!c.is_empty(), "Should detect copyright, got: {:?}", c);
    assert!(
        c[0].copyright.contains("François Müller"),
        "Copyright should preserve Unicode names: {}",
        c[0].copyright
    );
    assert!(!h.is_empty(), "Should detect Unicode holder: {:?}", h);
    assert!(
        h[0].holder.contains("Müller") || h[0].holder.contains("François"),
        "Holder should preserve original Unicode name: {}",
        h[0].holder
    );
}

#[test]
fn test_detect_all_rights_reserved_by_unicode_holder() {
    let text = "Copyright (C) All rights Reserved by 株式会社　朝日住宅社";
    let (c, h, _a) = detect_copyrights_from_text(text);

    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) by 株式会社 朝日住宅社"),
        "Should detect reserved-by copyright with Unicode holder: {:?}",
        c
    );
    assert!(
        h.iter().any(|hd| hd.holder == "株式会社 朝日住宅社"),
        "Should detect Unicode holder from reserved-by line: {:?}",
        h
    );
}

#[test]
fn test_detect_copyright_and_author_same_text() {
    // Adjacent lines are grouped into one candidate, so the author
    // span gets absorbed into the copyright group. Separating them
    // with blank lines produces independent candidate groups.
    let text = "Copyright 2024 Acme Inc.\n\n\n\nWritten by Jane Smith";
    let (c, h, a) = detect_copyrights_from_text(text);
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(c[0].copyright, "Copyright 2024 Acme Inc.");
    assert_eq!(c[0].start_line, LineNumber::ONE);
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Acme Inc.");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Jane Smith");
    assert_eq!(a[0].start_line, LineNumber::new(5).unwrap());
}

#[test]
fn test_detect_author_written_by() {
    let (_c, _h, a) = detect_copyrights_from_text("Written by Jane Smith");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Jane Smith");
    assert_eq!(a[0].start_line, LineNumber::ONE);
    assert_eq!(a[0].end_line, LineNumber::ONE);
}

#[test]
fn test_detect_author_maintained_by() {
    let (_c, _h, a) = detect_copyrights_from_text("Maintained by Bob Jones");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Bob Jones");
    assert_eq!(a[0].start_line, LineNumber::ONE);
    assert_eq!(a[0].end_line, LineNumber::ONE);
}

#[test]
fn test_detect_author_authors_keyword() {
    let (_c, _h, a) = detect_copyrights_from_text("Authors John Smith");
    assert_eq!(
        a.len(),
        1,
        "Should detect author from 'Authors', got: {:?}",
        a
    );
    assert!(
        a[0].author.contains("John Smith"),
        "Author: {}",
        a[0].author
    );
}

#[test]
fn test_detect_author_contributors_keyword() {
    let (_c, _h, a) = detect_copyrights_from_text("Contributors Jane Doe");
    assert_eq!(
        a.len(),
        1,
        "Should detect author from 'Contributors', got: {:?}",
        a
    );
    assert!(a[0].author.contains("Jane Doe"), "Author: {}", a[0].author);
}

#[test]
fn test_detect_author_spdx_contributor() {
    let (_c, _h, a) = detect_copyrights_from_text("SPDX-FileContributor: Alice Johnson");
    assert_eq!(
        a.len(),
        1,
        "Should detect author from SPDX-FileContributor, got: {:?}",
        a
    );
    assert!(
        a[0].author.contains("Alice Johnson"),
        "Author: {}",
        a[0].author
    );
}

#[test]
fn test_detect_copyright_with_company() {
    let (c, h, _a) = detect_copyrights_from_text("Copyright (c) 2024 Google LLC");
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(c[0].copyright, "Copyright (c) 2024 Google LLC");
    assert_eq!(c[0].start_line, LineNumber::ONE);
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Google LLC");
    assert_eq!(h[0].start_line, LineNumber::ONE);
}

#[test]
fn test_detect_copyright_all_rights_reserved() {
    let (c, h, _a) = detect_copyrights_from_text("Copyright 2024 Apple Inc. All rights reserved.");
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(
        c[0].copyright, "Copyright 2024 Apple Inc.",
        "All rights reserved should be stripped from copyright text"
    );
    assert_eq!(c[0].start_line, LineNumber::ONE);
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Apple Inc.");
    assert_eq!(h[0].start_line, LineNumber::ONE);
}

// ── strip_all_rights_reserved ────────────────────────────────────

#[test]
fn test_strip_all_rights_reserved_basic() {
    let tokens = [
        Token {
            value: "Copyright".to_string(),
            tag: PosTag::Copy,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "2024".to_string(),
            tag: PosTag::Yr,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "Acme".to_string(),
            tag: PosTag::Nnp,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "All".to_string(),
            tag: PosTag::Nn,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "Rights".to_string(),
            tag: PosTag::Right,
            start_line: LineNumber::ONE,
        },
        Token {
            value: "Reserved".to_string(),
            tag: PosTag::Reserved,
            start_line: LineNumber::ONE,
        },
    ];
    let refs: Vec<&Token> = tokens.iter().collect();
    let result = strip_all_rights_reserved(refs);
    assert_eq!(result.len(), 3, "Should strip All Rights Reserved");
    assert_eq!(result[0].value, "Copyright");
    assert_eq!(result[1].value, "2024");
    assert_eq!(result[2].value, "Acme");
}

// ── collect_filtered_leaves ──────────────────────────────────────

#[test]
fn test_collect_filtered_leaves_filters_pos_tags() {
    let node = ParseNode::Tree {
        label: TreeLabel::Copyright,
        children: vec![
            ParseNode::Leaf(Token {
                value: "Copyright".to_string(),
                tag: PosTag::Copy,
                start_line: LineNumber::ONE,
            }),
            ParseNode::Leaf(Token {
                value: "2024".to_string(),
                tag: PosTag::Yr,
                start_line: LineNumber::ONE,
            }),
            ParseNode::Leaf(Token {
                value: "Acme".to_string(),
                tag: PosTag::Nnp,
                start_line: LineNumber::ONE,
            }),
        ],
    };
    // Filter out Copy and Yr tags.
    let leaves = collect_filtered_leaves(&node, &[], &[PosTag::Copy, PosTag::Yr]);
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].value, "Acme");
}

#[test]
fn test_collect_filtered_leaves_filters_tree_labels() {
    let node = ParseNode::Tree {
        label: TreeLabel::Copyright,
        children: vec![
            ParseNode::Leaf(Token {
                value: "Copyright".to_string(),
                tag: PosTag::Copy,
                start_line: LineNumber::ONE,
            }),
            ParseNode::Tree {
                label: TreeLabel::YrRange,
                children: vec![ParseNode::Leaf(Token {
                    value: "2024".to_string(),
                    tag: PosTag::Yr,
                    start_line: LineNumber::ONE,
                })],
            },
            ParseNode::Leaf(Token {
                value: "Acme".to_string(),
                tag: PosTag::Nnp,
                start_line: LineNumber::ONE,
            }),
        ],
    };
    // Filter out YrRange tree label.
    let leaves = collect_filtered_leaves(&node, &[TreeLabel::YrRange], &[]);
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[0].value, "Copyright");
    assert_eq!(leaves[1].value, "Acme");
}

#[test]
fn test_detect_copyright_url_trailing_slash() {
    let input = "Copyright (c) 2007 Free Software Foundation, Inc. http://fsf.org/";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(
        c[0].copyright, "Copyright (c) 2007 Free Software Foundation, Inc. http://fsf.org",
        "Should strip trailing URL slash"
    );
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Free Software Foundation, Inc.");
}

#[test]
fn test_detect_copyright_url_angle_brackets_trailing_slash() {
    let input = "Copyright \u{00A9} 2007 Free Software Foundation, Inc. <http://fsf.org/>";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(
        c[0].copyright, "Copyright (c) 2007 Free Software Foundation, Inc. http://fsf.org",
        "Should strip angle brackets and trailing URL slash"
    );
}

#[test]
fn test_detect_copyright_url_slash_full_file() {
    let content =
        std::fs::read_to_string("testdata/copyright-golden/copyrights/afferogplv3-AfferoGPLv")
            .unwrap();
    let (c, _h, _a) = detect_copyrights_from_text(&content);
    assert!(!c.is_empty(), "Should detect copyright");
    assert!(
        c.iter()
            .any(|cr| cr.copyright
                == "Copyright (c) 2007 Free Software Foundation, Inc. http://fsf.org"),
        "Should strip trailing URL slash"
    );
}

#[test]
fn test_refine_relay_tom_zanussi_line() {
    let raw = " * Copyright (C) 2002, 2003 - Tom Zanussi (zanussi@us.ibm.com), IBM Corp";
    let prepared = crate::copyright::prepare::prepare_text_line(raw);
    let refined = refine_copyright(&prepared);
    assert_eq!(
        refined,
        Some("Copyright (c) 2002, 2003 - Tom Zanussi (zanussi@us.ibm.com), IBM Corp".to_string())
    );
}

#[test]
fn test_add_missing_copyrights_for_relay_holder_line() {
    let content = std::fs::read_to_string(
        "testdata/copyright-golden/ics/kernel-headers-original-linux/relay.h",
    )
    .unwrap();
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);
    assert!(
        holders.iter().any(|h| h.holder.contains("Tom Zanussi")),
        "expected Tom holder"
    );
    assert!(
        copyrights.iter().any(|c| {
            c.copyright == "Copyright (c) 2002, 2003 - Tom Zanussi (zanussi@us.ibm.com), IBM Corp"
        }),
        "expected Tom copyright added, got: {:?}",
        copyrights
    );
}

#[test]
fn test_contributed_by_with_latin1_diacritics() {
    let content = std::fs::read("testdata/copyright-golden/authors/strverscmp.c").unwrap();
    let text = crate::utils::file::decode_bytes_to_string(&content);
    let (_c, _h, a) = detect_copyrights_from_text(&text);
    assert!(
        a.iter()
            .any(|a| a.author.contains("Jean-Fran\u{00e7}ois Bignolles")),
        "Should detect author with preserved diacritics, got: {:?}",
        a
    );
}

#[test]
fn test_contributed_by_with_utf8_diacritics() {
    let content = std::fs::read("testdata/copyright-golden/authors/strverscmp2.c").unwrap();
    let text = crate::utils::file::decode_bytes_to_string(&content);
    let (_c, _h, a) = detect_copyrights_from_text(&text);
    assert!(
        a.iter()
            .any(|a| a.author.contains("Jean-Fran\u{00e7}ois Bignolles")),
        "Should detect author with preserved diacritics, got: {:?}",
        a
    );
}

#[test]
fn test_date_by_author() {
    let content = "\
Copyright (c) 1998 Softweyr LLC.  All rights reserved.
strtok_r, from Berkeley strtok
Oct 13, 1998 by Wes Peters <wes@softweyr.com>";
    let (_c, _h, a) = detect_copyrights_from_text(content);
    assert!(
        a.iter().any(|a| a.author.contains("Wes Peters")),
        "Should detect Wes Peters as author, got: {:?}",
        a
    );
}

#[test]
fn test_oprofile_authors_copyright() {
    let content = " * @remark Copyright 2002 OProfile authors
 * @remark Read the file COPYING
 *
 * @Modifications Daniel Hansel
 * Modified by Aravind Menon for Xen
 * These modifications are:
 * Copyright (C) 2005 Hewlett-Packard Co.";
    let (c, h, _a) = detect_copyrights_from_text(content);

    let prepared_line =
        crate::copyright::prepare::prepare_text_line(" * @remark Copyright 2002 OProfile authors");
    let tokens = crate::copyright::lexer::get_tokens(&[(1, prepared_line.clone())]);
    let parsed = crate::copyright::parser::parse(tokens.clone());
    let refined = crate::copyright::refiner::refine_copyright(&prepared_line);
    let token_debug: Vec<String> = tokens
        .iter()
        .map(|t| format!("{}:{:?}", t.value, t.tag))
        .collect();
    let parsed_debug: Vec<String> = parsed
        .iter()
        .map(|n| {
            let leaves: Vec<String> = super::token_utils::collect_all_leaves(n)
                .iter()
                .map(|t| format!("{}:{:?}", t.value, t.tag))
                .collect();
            format!("label={:?} tag={:?} leaves={leaves:?}", n.label(), n.tag())
        })
        .collect();
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright 2002 OProfile authors"),
        "Should detect 'Copyright 2002 OProfile authors'. prepared={prepared_line:?} refined={refined:?} tokens={token_debug:?} parsed={parsed_debug:?} got: {c:?}",
    );
    assert!(
        h.iter().any(|h| h.holder == "OProfile authors"),
        "Should detect 'OProfile authors' holder. prepared={prepared_line:?} tokens={token_debug:?} got: {h:?}",
    );
}

#[test]
fn test_drop_shadowed_c_sign_variants_unit() {
    let mut c = vec![
        CopyrightDetection {
            copyright: "Copyright 2007, 2010 Linux Foundation".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
        CopyrightDetection {
            copyright: "Copyright (c) 2007, 2010 Linux Foundation".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
        CopyrightDetection {
            copyright: "Copyright 1995-2010 Jean-loup Gailly and Mark Adler".to_string(),
            start_line: LineNumber::new(10).unwrap(),
            end_line: LineNumber::new(10).unwrap(),
        },
        CopyrightDetection {
            copyright: "Copyright (c) 1995-2010 Jean-loup Gailly and Mark Adler".to_string(),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
        },
    ];
    drop_shadowed_c_sign_variants(&mut c);
    let mut got: Vec<&str> = c.iter().map(|d| d.copyright.as_str()).collect();
    got.sort();
    let mut expected = vec![
        "Copyright (c) 1995-2010 Jean-loup Gailly and Mark Adler",
        "Copyright (c) 2007, 2010 Linux Foundation",
        "Copyright 1995-2010 Jean-loup Gailly and Mark Adler",
    ];
    expected.sort();
    assert_eq!(got, expected, "After dropping variants, got: {c:?}");
}

#[test]
fn test_linux_foundation_line_prefers_holder_variant_over_bare_years() {
    let content = "* Copyright (c) 2007, 2010 Linux Foundation";
    let (c, h, _a) = detect_copyrights_from_text(content);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2007, 2010 Linux Foundation"),
        "copyrights: {:?}",
        c
    );
    assert!(
        !c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2007, 2010"),
        "copyrights: {:?}",
        c
    );
    assert!(
        h.iter().any(|holder| holder.holder == "Linux Foundation"),
        "holders: {:?}",
        h
    );
}

#[test]
fn test_holder_extracted_from_year_range_with_the_prefix() {
    let content = "// Copyright 2016-2022 The Linux Foundation\n// Copyright 2016-2017 The New York Times Company";
    let (_c, h, _a) = detect_copyrights_from_text(content);
    let holders: Vec<_> = h.iter().map(|holder| holder.holder.as_str()).collect();
    assert!(
        holders.contains(&"The Linux Foundation"),
        "holders: {:?}",
        h
    );
    assert!(
        holders.contains(&"The New York Times Company"),
        "holders: {:?}",
        h
    );
}

#[test]
fn test_originally_by_author() {
    let content = "\
#   Copyright 1996-2006 Free Software Foundation, Inc.
#   Taken from GNU libtool, 2001
#   Originally by Gordon Matzigkeit <gord@gnu.ai.mit.edu>, 1996";
    let (_c, _h, a) = detect_copyrights_from_text(content);
    assert!(
        a.iter().any(|a| a.author.contains("Gordon Matzigkeit")),
        "Should detect Gordon Matzigkeit as author, got: {:?}",
        a
    );
}

#[test]
fn test_by_name_email_author_full_file() {
    let content = std::fs::read_to_string(
        "testdata/copyright-golden/authors/author_var_route_c-var_route_c.c",
    )
    .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    assert!(
        a.iter()
            .any(|a| a.author.contains("Jennifer Bray of Origin")),
        "Should detect Jennifer Bray, got: {:?}",
        a
    );
    assert!(
        a.iter().any(|a| a.author.contains("Erik Schoenfelder")),
        "Should detect Erik Schoenfelder, got: {:?}",
        a
    );
    assert!(
        a.iter().any(|a| a.author.contains("Simon Leinen")),
        "Should detect Simon Leinen, got: {:?}",
        a
    );
}

#[test]
fn test_author_uc_contributors() {
    let content =
        std::fs::read_to_string("testdata/copyright-golden/authors/author_uc-LICENSE").unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    assert!(
        a.iter()
            .any(|a| a.author == "UC Berkeley and its contributors"),
        "Should detect 'UC Berkeley and its contributors', got: {:?}",
        a
    );
    assert!(
        a.iter().any(|a| a
            .author
            .contains("University of California, Berkeley and its contributors")),
        "Should detect 'University of California, Berkeley and its contributors', got: {:?}",
        a
    );
}

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
fn test_co_maintainer_fixture_extracts_authors() {
    let content =
        std::fs::read_to_string("testdata/copyright-golden/copyrights/misco4/co-maintainer.txt")
            .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    let authors: Vec<&str> = a.iter().map(|a| a.author.as_str()).collect();
    for expected in [
        "Norbert Tretkowski <nobse@debian.org>",
        "Jeff Bailey <jbailey@raspberryginger.com>",
        "Rob Weir <rweir@ertius.org>",
        "Andres Salomon <dilinger@debian.org>",
        "Lars Wirzenius <liw@iki.fi>",
        "Adeodato Simó <dato@net.com.org.es>",
        "Wouter van Heyst <larstiq@larstiq.dyndns.org>",
        "Jelmer Vernooij <jelmer@samba.org>",
        "the pkg-bazaar team",
    ] {
        assert!(authors.contains(&expected), "authors: {authors:#?}");
    }
}

#[test]
fn test_debianized_by_fixture_extracts_author() {
    let content =
        std::fs::read_to_string("testdata/copyright-golden/copyrights/misco4/debianized-by.txt")
            .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    let authors: Vec<&str> = a.iter().map(|a| a.author.as_str()).collect();
    assert!(
        authors.contains(&"Christian Marillat <marillat@debian.org>"),
        "authors: {authors:#?}"
    );
}

#[test]
fn test_final_agreement_fixture_extracts_created_by_author() {
    let content =
        std::fs::read_to_string("testdata/copyright-golden/copyrights/misco4/final-agreement.txt")
            .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    let authors: Vec<&str> = a.iter().map(|a| a.author.as_str()).collect();
    assert!(authors.contains(&"the Project"), "authors: {authors:#?}");
}

#[test]
fn test_sata_mv_fixture_merges_written_by_author_block() {
    let content = std::fs::read_to_string(
        "testdata/copyright-golden/copyrights/misco4/linux-copyrights/drivers/ata/sata_mv.c",
    )
    .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    let authors: Vec<&str> = a.iter().map(|a| a.author.as_str()).collect();
    assert!(
        authors.contains(
            &"Brett Russ. Extensive overhaul and enhancement by Mark Lord <mlord@pobox.com>"
        ),
        "authors: {authors:#?}"
    );
}

#[test]
fn test_hid_appleir_fixture_merges_written_by_author_block() {
    let content = std::fs::read_to_string(
        "testdata/copyright-golden/copyrights/misco4/linux-copyrights/drivers/hid/hid-appleir.c",
    )
    .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    let authors: Vec<&str> = a.iter().map(|a| a.author.as_str()).collect();
    assert!(
            authors.contains(&"James McKenzie Ported to recent 2.6 kernel versions by Greg Kroah-Hartman <gregkh@suse.de> Updated to support newer remotes by Bastien Nocera <hadess@hadess.net> Ported to HID subsystem by Benjamin Tissoires <benjamin.tissoires@gmail.com>"),
            "authors: {authors:#?}"
        );
}

#[test]
fn test_dvb_frontend_fixture_merges_written_by_author_block() {
    let content = std::fs::read_to_string(
        "testdata/copyright-golden/copyrights/misco4/linux-copyrights/include/media/dvb_frontend.h",
    )
    .unwrap();
    let (_c, _h, a) = detect_copyrights_from_text(&content);
    let authors: Vec<&str> = a.iter().map(|a| a.author.as_str()).collect();
    assert!(
            authors.contains(&"Ralph Metzler Overhauled by Holger Waechtler Kernel I2C stuff by Michael Hunold <hunold@convergence.de>"),
            "authors: {authors:#?}"
        );
}

#[test]
fn test_auth_nl_copyright_not_author() {
    // When "Copyright (C) YEAR" is followed by "Author: Name <email>" on the next line,
    // the Author name should be absorbed into the copyright, not treated as a standalone author.
    let input = "* Copyright (C) 2016-2018\n* Author: Matt Ranostay <matt.ranostay@konsulko.com>";
    let (c, h, a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("Matt Ranostay")),
        "Should detect copyright with Matt Ranostay, got: {:?}",
        c
    );
    assert!(
        h.iter().any(|hr| hr.holder.contains("Matt Ranostay")),
        "Should detect Matt Ranostay as holder, got: {:?}",
        h
    );
    // The expected output has NO author entries
    assert!(
        a.is_empty(),
        "Should NOT detect authors (Author: is part of copyright), got: {:?}",
        a
    );
}

#[test]
fn test_notice_file_multiple_copyrights() {
    let text = "   Copyright (C) 1997, 2002, 2005 Free Software Foundation, Inc.\n\
                    * Copyright (C) 2005 Jens Axboe <axboe@suse.de>\n\
                    * Copyright (C) 2006 Alan D. Brunelle <Alan.Brunelle@hp.com>\n\
                    * Copyright (C) 2006 Jens Axboe <axboe@kernel.dk>\n\
                    * Copyright (C) 2006. Bob Jenkins (bob_jenkins@burtleburtle.net)\n\
                    * Copyright (C) 2009 Jozsef Kadlecsik (kadlec@blackhole.kfki.hu)\n\
                    * Copyright IBM Corp. 2008\n\
                    # Copyright (c) 2005 SUSE LINUX Products GmbH, Nuernberg, Germany.\n\
                    # Copyright (c) 2005 Silicon Graphics, Inc.";
    let (c, _h, _a) = detect_copyrights_from_text(text);
    let cr_texts: Vec<&str> = c.iter().map(|cr| cr.copyright.as_str()).collect();
    assert!(
        c.len() >= 9,
        "Should detect at least 9 copyrights, got {}: {:?}",
        c.len(),
        cr_texts
    );
}

#[test]
fn test_doc_doc_no_overabsorb() {
    let input = "are copyrighted by Douglas C. Schmidt and his research group at Washington University, University of California, Irvine, and Vanderbilt University, Copyright (c) 1993-2008, all rights reserved.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
            c.iter().any(|cr| cr.copyright == "copyrighted by Douglas C. Schmidt and his research group at Washington University, University of California, Irvine, and Vanderbilt University, Copyright (c) 1993-2008"),
            "Should merge trailing Copyright (c) clause, got: {:?}",
            c
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
fn test_json_escaped_html_anchor_copyright_url_detected() {
    let input = r#"&copy; <a href=\"http://www.openstreetmap.org/copyright\">OpenStreetMap</a>"#;
    let (c, h, _a) = detect_copyrights_from_text(input);

    assert!(
        c.iter().any(|cr| {
            cr.copyright == "(c) http://www.openstreetmap.org/copyright OpenStreetMap"
        }),
        "copyrights: {c:?}"
    );
    assert!(
        h.iter().any(|hr| hr.holder == "OpenStreetMap"),
        "holders: {h:?}"
    );
    assert!(
        !c.iter()
            .any(|cr| cr.copyright == "(c) http://www.openstreetmap.org/copyright"),
        "copyrights: {c:?}"
    );
    assert!(
        !h.iter()
            .any(|hr| hr.holder == "http://www.openstreetmap.org/copyright"),
        "holders: {h:?}"
    );
}

#[test]
fn test_json_description_keeps_explicit_anchor_attribution() {
    let input = r#"{"description":"&copy; <a href=\"http://www.openstreetmap.org/copyright\">OpenStreetMap</a>"}"#;
    let (c, h, _a) = detect_copyrights_from_text(input);

    assert!(
        c.iter().any(|cr| {
            cr.copyright == "(c) http://www.openstreetmap.org/copyright OpenStreetMap"
        }),
        "copyrights: {c:?}"
    );
    assert!(
        h.iter().any(|hr| hr.holder == "OpenStreetMap"),
        "holders: {h:?}"
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

#[test]
fn test_academy_copyright() {
    let input = "Copyright (c) 2006 Academy of Motion Picture Arts and Sciences";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright
                == "Copyright (c) 2006 Academy of Motion Picture Arts and Sciences"),
        "Should detect Academy copyright, got: {:?}",
        c
    );
    assert!(
        h.iter()
            .any(|hr| hr.holder == "Academy of Motion Picture Arts and Sciences"),
        "Should detect Academy holder, got: {:?}",
        h
    );
}

#[test]
fn test_define_copyright() {
    let input = "#define COPYRIGHT       \"Copyright (c) 1999-2008 LSI Corporation\"\n#define MODULEAUTHOR    \"LSI Corporation\"";
    let (c, h, a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 1999-2008 LSI Corporation"),
        "Should detect 'Copyright (c) 1999-2008 LSI Corporation', got: {:?}",
        c
    );
    assert!(
        h.iter().any(|h| h.holder == "LSI Corporation"),
        "Should detect holder, got: {:?}",
        h
    );
    assert!(
        a.iter().any(|a| a.author == "LSI Corporation"),
        "Should detect author from MODULEAUTHOR, got: {:?}",
        a
    );
}

#[test]
fn test_parts_copyright_prefix() {
    let input = " * Parts (C) 1999 David Airlie, airlied@linux.ie";
    let numbered_lines: Vec<(usize, String)> = input
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let token_dbg: Vec<(String, PosTag)> =
        tokens.iter().map(|t| (t.value.clone(), t.tag)).collect();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens)
    };
    let labels_dbg: Vec<Option<TreeLabel>> = tree.iter().map(|n| n.label()).collect();

    let (c, h, _a) = detect_copyrights_from_text(input);
    assert_eq!(
        c.len(),
        1,
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\nexpected one copyright, got: {c:#?}"
    );
    assert_eq!(
        c[0].copyright, "Parts (c) 1999 David Airlie, airlied@linux.ie",
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\ncopyrights: {c:#?}"
    );
    assert_eq!(
        h.len(),
        1,
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\nexpected one holder, got: {h:#?}"
    );
    assert_eq!(h[0].holder, "David Airlie");
}

#[test]
fn test_trailing_year_included_in_copyright() {
    let cases = &[
        (
            "Copyright (c) IBM Corporation 2008",
            "Copyright (c) IBM Corporation 2008",
            "IBM Corporation",
        ),
        (
            "Copyright (c) Zeus Technology Limited 1996",
            "Copyright (c) Zeus Technology Limited 1996",
            "Zeus Technology Limited",
        ),
        (
            "Copyright IBM, Corp. 2007",
            "Copyright IBM, Corp. 2007",
            "IBM, Corp.",
        ),
        (
            "Copyright IBM Corp. 2004, 2010",
            "Copyright IBM Corp. 2004, 2010",
            "IBM Corp.",
        ),
    ];
    for (input, expected_cr, expected_h) in cases {
        let (c, h, _a) = detect_copyrights_from_text(input);
        assert!(
            c.iter().any(|cr| cr.copyright == *expected_cr),
            "For '{}': expected CR '{}', got {:?}",
            input,
            expected_cr,
            c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
        );
        assert!(
            h.iter().any(|hh| hh.holder == *expected_h),
            "For '{}': expected holder '{}', got {:?}",
            input,
            expected_h,
            h.iter().map(|hh| &hh.holder).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_holder_after_year_range_absorbed() {
    let input = "COPYRIGHT (c) 2006 - 2009 DIONYSOS";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("DIONYSOS")),
        "Should include 'DIONYSOS' in copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hh| hh.holder.contains("DIONYSOS")),
        "Should include 'DIONYSOS' in holder, got: {:?}",
        h.iter().map(|hh| &hh.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_multi_word_holder_after_year_range() {
    let input = "Copyright (C) 1999-2000 VA Linux Systems";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("VA Linux Systems")),
        "Should include full company name, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hh| hh.holder.contains("VA Linux Systems")),
        "Should include full company name in holder, got: {:?}",
        h.iter().map(|hh| &hh.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_by_keyword_holder_captured() {
    let input = "Copyright (c) 1991, 2000, 2001 by Lucent Technologies.";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright.contains("Lucent Technologies")),
        "Should include holder after 'by', got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hh| hh.holder.contains("Lucent Technologies")),
        "Should include holder after 'by', got: {:?}",
        h.iter().map(|hh| &hh.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_holder_company_with_digits_absorbed() {
    let input = "Copyright (c) 1995-1996 Guy Eric Schalnat, Group 42, Inc.";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("Group 42, Inc.")),
        "Should include full company name with digits, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hh| hh.holder.contains("Group 42, Inc.")),
        "Should include full company name with digits in holder, got: {:?}",
        h.iter().map(|hh| &hh.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_copyright_dash_email_tail_absorbed() {
    let input = "Copyright (c) 1999, Bob Withers - bwit@pobox.com";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("bwit@pobox.com")),
        "Should include dash-email tail in copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
    );
    assert!(
        h.iter().any(|hh| hh.holder == "Bob Withers"),
        "Expected holder 'Bob Withers', got: {:?}",
        h.iter().map(|hh| &hh.holder).collect::<Vec<_>>()
    );
}

#[test]
fn test_w3c_paren_group_debug() {
    let input = "(c) 1998-2008 (W3C) MIT, ERCIM, Keio University";
    let numbered_lines: Vec<(usize, String)> = input
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect();
    let groups = crate::copyright::candidates::collect_candidate_lines(numbered_lines);
    let tokens: Vec<Token> = groups.first().map(|g| get_tokens(g)).unwrap_or_default();
    let token_dbg: Vec<(String, PosTag)> =
        tokens.iter().map(|t| (t.value.clone(), t.tag)).collect();
    let tree = if tokens.is_empty() {
        Vec::new()
    } else {
        parse(tokens)
    };
    let labels_dbg: Vec<(String, Option<TreeLabel>)> =
        tree.iter().map(|n| (format!("{n:?}"), n.label())).collect();

    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright.contains("MIT, ERCIM, Keio University")),
        "tokens: {token_dbg:#?}\nlabels: {labels_dbg:#?}\nexpected W3C copyright with MIT/ERCIM/Keio, got: {c:#?}"
    );
}

#[test]
fn test_detect_copyright_with_dots_single_line() {
    // "Copyright . 2008 Foo Name, Inc." - dot after Copyright should be stripped,
    // and "Foo Name, Inc." should be detected as the full holder.
    let input = "Copyright . 2008 Foo Name, Inc.";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
    assert_eq!(
        c[0].copyright, "Copyright 2008 Foo Name, Inc.",
        "Should detect full copyright with company name"
    );
    assert_eq!(h.len(), 1, "Should detect one holder, got: {:?}", h);
    assert_eq!(h[0].holder, "Foo Name, Inc.");
}

#[test]
fn test_detect_copyright_with_dots_multiline() {
    let input = "Copyright . 2008 company name, inc.";
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert!(
        !c.is_empty(),
        "Should detect at least one copyright, got: {:?}",
        c
    );
    assert!(
        c.iter().any(|cr| cr.copyright.contains("2008")),
        "Should detect copyright with year 2008, got: {:?}",
        c
    );
    assert!(
        c.iter()
            .any(|cr| cr.copyright.to_lowercase().contains("company name")),
        "Should detect full company name, got: {:?}",
        c
    );
    assert!(
        h.iter()
            .any(|hr| hr.holder.to_lowercase().contains("company name")),
        "Should detect holder with company name, got: {:?}",
        h
    );
}

#[test]
fn test_opensharedmap_inc_holder_detected() {
    let input = "Copyright (C) OpenSharedMap Inc.";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright.contains("OpenSharedMap Inc")),
        "Expected OpenSharedMap copyright detection, got: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder.contains("OpenSharedMap Inc")),
        "Expected OpenSharedMap holder detection, got: {holders:?}"
    );
}

#[test]
fn test_disclaimer_tail_with_inc_as_does_not_create_holder() {
    let input = "Copyright Owner Inc. AS IS AND ANY EXPRESS OR IMPLIED WARRANTIES";
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        holders.is_empty(),
        "Unexpected disclaimer-derived holders: {holders:?}"
    );
}

#[test]
fn test_camelcase_provider_not_author_false_positive() {
    let input = "A meter implementation is created by a MeterProvider in this system.\nA trace implementation is created by a TracerProvider in this system.";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    let author_values: Vec<&str> = authors.iter().map(|a| a.author.as_str()).collect();
    assert!(
        author_values
            .iter()
            .all(|a| *a != "MeterProvider in" && *a != "TracerProvider in"),
        "Unexpected provider false-positive authors: {author_values:?}"
    );
}

#[test]
fn test_platformdirs_lowercase_holder_detected() {
    let input = "Copyright (c) 2010-202x The platformdirs developers";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights.iter().any(|c| c
            .copyright
            .contains("2010-202x The platformdirs developers")),
        "Expected platformdirs copyright, got: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "The platformdirs developers"),
        "Expected platformdirs holder, got: {holders:?}"
    );
}
#[test]
fn test_square_c_sign_detected() {
    let input = "[C] The Regents of the University of Michigan and Merit Network, Inc. 1992, 1993, 1994, 1995 All Rights Reserved";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights.iter().any(|c| c
            .copyright
            .contains("Regents of the University of Michigan")),
        "Expected Regents copyright detection, got: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder.contains("Regents of the University of Michigan")),
        "Expected Regents holder detection, got: {holders:?}"
    );
}

#[test]
fn test_template_literal_copyright_holder_detected() {
    let input = "copyright: `Copyright 2010–${new Date().getUTCFullYear()} Mike Bostock`";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright 2010-${new Date .getUTCFullYear } Mike Bostock"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Mike Bostock"),
        "holders: {holders:?}"
    );
    assert!(
        !copyrights.iter().any(|c| c.copyright == "Copyright 2010-$"),
        "copyrights: {copyrights:?}"
    );
}

#[test]
fn test_markdown_transition_line_not_author() {
    let input = "The meaning of [*transition*.delay](https://github.com/d3/d3-transition/blob/master/README.md#transition_delay) has changed for chained transitions created by [*transition*.transition](https://github.com/d3/d3-transition/blob/master/README.md#transition_transition).";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        !authors.iter().any(|a| a.author.contains(
            "transition .transition https://github.com/d3/d3-transition/blob/master/README.md"
        )),
        "authors: {authors:?}"
    );
}

#[test]
fn test_json_author_field_does_not_capture_following_metadata_blob() {
    let input = "author: Box UK,\nurl: http://updates.jenkins-ci.org/download/plugins/jslint/0.7.6/jslint.hpi,\nversion: 0.7.6,\nwiki: https://wiki.jenkins-ci.org/display/JENKINS/JSLint+plugin";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    let author_values: Vec<&str> = authors.iter().map(|a| a.author.as_str()).collect();
    assert_eq!(author_values, vec!["Box UK"], "authors: {authors:?}");
}

#[test]
fn test_sentence_fragment_not_author() {
    let input = "with key equal to unescaped token";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_call_ref_fragment_not_author() {
    let input = "call @ref";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_boost_value_stack_call_ref_sentence_not_author() {
    let input = "Then to build a @ref value, first call @ref reset and optionally specify the boost::container::pmr::memory_resource.";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_json_excerpt_developed_by_company_author_detected() {
    let input = r#"mes/0.2/jsgames.hpi","version":"0.2","wiki":"https://wiki.jenkins-ci.org/display/JENKINS/JSGames+Plugin"},"jslint":{"buildDate":"Jan 03, 2013","dependencies":[],"developers":[{"developerId":"gavd","email":"gavin.davies@boxuk.com","name":"Gavin Davies"}],"excerpt":"Lint JavaScript files, outputting to checkstyle format. Supports all JSLint options. Developed by Box UK.","gav":"org.jenkins-ci.plugins:jslint:0.7.6","labels":["misc"],"name":"jslint","previousTimestamp":"2013-01-03T20:22:38.00Z","previousVersion":"0.7.5","releaseTimestamp":"2013-01-03T20:29:06.00Z","requiredCore":"1.474","scm":"github.com""#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Box UK"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_plain_json_author_string_preserved() {
    let input = r#""author": "Google's Web DevRel Team","#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert_eq!(
        authors
            .iter()
            .map(|a| a.author.as_str())
            .collect::<Vec<_>>(),
        vec!["Google's Web DevRel Team"],
        "authors: {authors:?}"
    );
}

#[test]
fn test_plain_json_author_string_with_parenthesized_url_preserved() {
    let input = r#""author": "Qix (http://github.com/qix-)","#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert_eq!(
        authors
            .iter()
            .map(|a| a.author.as_str())
            .collect::<Vec<_>>(),
        vec!["Qix (http://github.com/qix-)"],
        "authors: {authors:?}"
    );
}

#[test]
fn test_plain_json_author_string_with_parenthesized_url_and_following_key_preserved() {
    let input = concat!(
        "   \"author\": \"Qix (http://github.com/qix-)\",\n",
        "   \"keywords\": [\n",
    );
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert_eq!(
        authors
            .iter()
            .map(|a| a.author.as_str())
            .collect::<Vec<_>>(),
        vec!["Qix (http://github.com/qix-)"],
        "authors: {authors:?}"
    );
}

#[test]
fn test_json_author_object_name_preferred_over_url_tail() {
    let input =
        "\"author\": { \"name\": \"Chen Fengyuan\", \"url\": \"https://chenfengyuan.com/\" }";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert_eq!(
        authors
            .iter()
            .map(|a| a.author.as_str())
            .collect::<Vec<_>>(),
        vec!["Chen Fengyuan"],
        "authors: {authors:?}"
    );
}

#[test]
fn test_multiline_json_author_object_name_detected() {
    let input = concat!(
        "  \"author\": {\n",
        "    \"name\": \"Chen Fengyuan\",\n",
        "    \"url\": \"https://chenfengyuan.com/\"\n",
        "  }\n",
    );
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Chen Fengyuan"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_metadata_json_author_fallback_keeps_collective_and_single_word_names() {
    let input = r#"{
  "components": [
    {
      "supplier": { "name": "Google LLC" },
      "author": "gRPC authors",
      "name": "gRPC (C++)"
    },
    {
      "supplier": { "name": "Meta Open Source" },
      "author": "Meta",
      "name": "folly"
    },
    {
      "supplier": { "name": "The libunwind project" },
      "author": "The libunwind project",
      "name": "libunwind"
    },
    {
      "supplier": { "name": "Google LLC" },
      "author": "S2Geometry",
      "name": "S2 Geometry Library"
    }
  ]
}"#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    let values: Vec<&str> = authors.iter().map(|a| a.author.as_str()).collect();
    assert!(values.contains(&"gRPC authors"), "authors: {authors:?}");
    assert!(values.contains(&"Meta"), "authors: {authors:?}");
    assert!(
        values.contains(&"The libunwind project"),
        "authors: {authors:?}"
    );
    assert!(values.contains(&"S2Geometry"), "authors: {authors:?}");
}

#[test]
fn test_json_code_example_author_fields_do_not_create_authors() {
    let input = r#"{
  "expectedStages": [
    {
      "$match": {
        "author": "Agatha Christie"
      }
    },
    {
      "$setMetadata": {
        "score": {
          "$divide": [1, 2]
        }
      }
    }
  ]
}"#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_code_pipeline_author_match_not_detected_as_author() {
    let input = r#"{
  $scoreFusion: {
    input: {
      pipelines: {
        pipeOne: [
          { $match : { author : "Agatha Christie" } },
          { $sort: {author: 1} }
        ]
      }
    }
  }
}"#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_json_sponsor_description_does_not_create_authors() {
    let input = r#"{
  "description": "A useful plugin",
  "sponsor": {
    "@type": "Organization",
    "name": "Example Org",
    "description": "Developer as a service Plugin in the cloud when a workflow runs."
  }
}"#;
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_fast_path_proposal_phrase_not_author() {
    let input = "Clinger's fast path, inspired by Jakub Jelínek's proposal";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_readme_security_review_prose_not_author() {
    let input = "application developers can trust, the C++ Alliance has commissioned Bishop Fox to perform a security audit of the Boost.JSON library. The report linked here";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_update_center_metadata_blob_not_multiple_authors() {
    let input = "author: Box UK, url: http://updates.jenkins-ci.org/download/plugins/jslint/0.7.6/jslint.hpi, version: 0.7.6, wiki: https://wiki.jenkins-ci.org/display/JENKINS/JSLint+plugin, title: JSLint plugin, buildDate: Jan 03, 2013, developerId: gavd";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    let values: Vec<&str> = authors.iter().map(|a| a.author.as_str()).collect();
    assert_eq!(values, vec!["Box UK"], "authors: {authors:?}");
}

#[test]
fn test_gsoc_javascript_language_phrase_not_author() {
    let input = "My proposal is based on getting full support for JavaScript within the RoboComp framework. For this, the current state of generation of written components in the JavaScript language must be improved.";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        !authors
            .iter()
            .any(|a| a.author == "components in the JavaScript language"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_apostrophized_person_name_metadata_not_author() {
    let input = "type' Person name' AadityaNair";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_language_fragment_not_author() {
    let input = "in PHP";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_single_word_stopword_not_author() {
    let input = "In";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_short_comma_metadata_phrase_not_author() {
    let input = "GENIVI, several standard";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_short_url_suffix_phrase_not_author() {
    let input = "J.L.Blanco (https://github.com/jlblancoc)";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_lowercase_starting_multiword_fragment_not_author() {
    let input = "around the world. It";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_dangling_two_word_phrase_not_author() {
    let input = "Sandcastle and";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_gsoc_spdx_sentence_not_copyright_or_holder() {
    let input = "Software Package Data Exchange (SPDX) is a set of standards for communicating the components, licenses, and copyrights associated with software.";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
}

#[test]
fn test_json_description_and_sponsor_not_copyright_or_holder() {
    let input = r#""description": "Software Package Data Exchange (SPDX) is a set of standards for communicating the components, licenses, and copyrights associated with software.", "sponsor": { "@type": "Organization", "name": "FOSSology", "disambiguatingDescription": "Open Source License Compliance by Open Source Software", "url": "http://example.com/logo" }"#;
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
}

#[test]
fn test_mit_intro_not_copyright_or_holder() {
    let input = "These files are covered by the following copyright and MIT License, reproduced from the original project:";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright == "copyright and MIT"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        !holders.iter().any(|h| h.holder == "MIT"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_rest_api_description_not_holder_or_copyright() {
    let input = "We provide developers, researchers, and students the ability to access any model using a simple REST API call. The REST API description.";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
}

#[test]
fn test_bare_rest_marker_not_copyright() {
    let input = "(c) REST";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
}

#[test]
fn test_semicolon_joined_copyright_list_does_not_keep_combined_variant() {
    let input =
        "(C) 2019--2020 Vinnie Falco; (C) 2020 Krystian Stasiowski; (C) 2022 Dmitry Arkhipov";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        !copyrights.iter().any(|c| {
            c
            .copyright
            == "(c) 2019-2020 Vinnie Falco; (c) 2020 Krystian Stasiowski; (c) 2022 Dmitry Arkhipov"
        }),
        "copyrights: {copyrights:?}"
    );
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) 2019-2020 Vinnie Falco"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) 2020 Krystian Stasiowski"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) 2022 Dmitry Arkhipov"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Vinnie Falco")
            && holders.iter().any(|h| h.holder == "Krystian Stasiowski")
            && holders.iter().any(|h| h.holder == "Dmitry Arkhipov"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_normalize_company_suffix_period_holder_variants() {
    let input = "Copyright (c) 2020 Foo, Inc\nCopyright (c) 2021 Foo, Inc.";
    let (_copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert_eq!(holders.len(), 2, "holders: {holders:?}");
    assert!(
        holders.iter().all(|h| h.holder == "Foo, Inc."),
        "holders: {holders:?}"
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
fn test_extract_authors_from_dense_name_email_list() {
    let input = "John Doe <john@example.com>\nJane Smith <jane@example.com>\n";
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

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
    let (_copyrights, _holders, authors) = detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Tokio Contributors <team@tokio.rs>"),
        "authors: {authors:?}"
    );
}
