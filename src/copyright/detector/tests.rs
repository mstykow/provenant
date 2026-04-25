// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::models::LineNumber;
use std::fs;

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
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| s == "Copyright 2003 Name"),
        "copyrights: {cr:#?}"
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
    let (copyrights, holders, _authors) = detect_copyrights_from_text(&content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) OpenJS Foundation and other contributors"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        !copyrights
            .iter()
            .any(|c| c.copyright.contains("jquery.org/license")),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "OpenJS Foundation and other contributors"),
        "holders: {holders:?}"
    );
    assert!(
        !holders
            .iter()
            .any(|h| h.holder.contains("jquery.org/license")),
        "holders: {holders:?}"
    );
}

#[test]
fn test_detect_versioned_project_banner_with_mixed_case_brand_holder() {
    let content = "/*! jQuery v2.2.0 | (c) jQuery Foundation | jquery.org/license */\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "(c) jQuery Foundation"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "jQuery Foundation"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_detect_postscript_percent_copyright_prefix() {
    let content = "%%Copyright: -----------------------------------------------------------\n\
%%Copyright: Copyright 1990-2009 Adobe Systems Incorporated.\n\
%%Copyright: All rights reserved.\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter()
            .any(|s| s == "Copyright 1990-2009 Adobe Systems Incorporated"),
        "cr: {cr:#?}"
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
fn test_detect_swfobject_copyright_line() {
    let content = "/* SWFObject v2.1 <http://code.google.com/p/swfobject/>\n\
        Copyright (c) 2007-2008 Geoff Stearns, Michael Williams, and Bobby van der Sluis\n\
        This software is released under the MIT License <http://www.opensource.org/licenses/mit-license.php>\n\
*/\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    assert!(
        cr.iter().any(|s| {
            s == "Copyright (c) 2007-2008 Geoff Stearns, Michael Williams, and Bobby van der Sluis"
        }),
        "copyrights: {cr:#?}"
    );
}

#[test]
fn test_detect_holder_list_continuation_after_comma_and() {
    let content = "Copyright 1996-2002, 2006 by David Turner, Robert Wilhelm, and Werner Lemberg\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
        cr.iter().any(|s| {
            s == "Copyright 1996-2002, 2006 by David Turner, Robert Wilhelm, and Werner Lemberg"
        }),
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s == "David Turner, Robert Wilhelm, and Werner Lemberg"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_long_comma_separated_year_list_with_holder() {
    let content = "Copyright 1994, 1995, 1996, 1997, 1998, 1999, 2000, 2001, 2002, 2003 Free Software Foundation, Inc.\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    assert!(
            cr.iter().any(|s| {
                s == "Copyright 1994, 1995, 1996, 1997, 1998, 1999, 2000, 2001, 2002, 2003 Free Software Foundation, Inc."
            }),
            "copyrights: {cr:#?}"
        );
    assert!(
        hs.iter().any(|s| s == "Free Software Foundation, Inc."),
        "holders: {hs:#?}"
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
        "copyrights: {cr:#?}"
    );
    assert!(
        hs.iter()
            .any(|s| s == "MOTOROLA, BROADBAND COMMUNICATIONS SECTOR"),
        "holders: {hs:#?}"
    );
}

#[test]
fn test_detect_composite_copy_copyrighted_by_with_trailing_copyright_clause() {
    let content =
        "FaCE is copyrighted by Object Computing, Inc., St. Louis Missouri, Copyright (C) 2002,\n";
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
        "copyrights: {cr:#?}"
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
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);
    let cr: Vec<String> = copyrights.into_iter().map(|c| c.copyright).collect();
    let hs: Vec<String> = holders.into_iter().map(|h| h.holder).collect();

    let merged = "Copyright (c) 1988, 1993 Copyright (c) 1992, 1993 The Regents of the University of California";
    assert!(
        cr.iter().any(|s| s == merged),
        "copyrights: {cr:#?}\n\nholders: {hs:#?}"
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
    let (c, h, _a) = detect_copyrights_from_text(input);
    assert_eq!(c.len(), 1, "Should detect one copyright, got: {:?}", c);
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
