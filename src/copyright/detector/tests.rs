// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

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
fn test_swift_convention_c_signatures_do_not_produce_copyrights_or_holders() {
    let input = concat!(
        "let invokeSuperSetter: @convention(c) (NSObject, AnyClass, Selector, AnyObject?) -> Void = { object, superclass, selector, delegate in\n",
        "typealias Setter = @convention(c) (NSObject, Selector, AnyObject?) -> Void\n",
    );

    let (copyrights, holders, authors) = detect_copyrights_from_text(input);

    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
    assert!(authors.is_empty(), "authors: {authors:?}");
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
fn test_play_header_does_not_emit_bare_c_from_year_shadow() {
    let content = "Copyright (C) from 2022 The Play Framework Contributors <https://github.com/playframework>, 2011-2021 Lightbend Inc. <https://www.lightbend.com>\n";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright.contains("The Play Framework Contributors")),
        "copyrights: {copyrights:?}"
    );
    assert!(
        !copyrights.iter().any(|c| c.copyright == "(c) from 2022"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder.contains("The Play Framework Contributors")),
        "holders: {holders:?}"
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
fn test_extract_xml_copyright_and_company_attributes() {
    let content = r#"<assembly company="Microsoft Corporation" copyright="Microsoft Corporation" supportInformation="https://support.microsoft.com/help/5049993">"#;
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "copyright Microsoft Corporation"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders.iter().any(|h| h.holder == "Microsoft Corporation"),
        "holders: {holders:?}"
    );
}

#[test]
fn test_company_attribute_without_copyright_attribute_does_not_emit_copyright() {
    let content = r#"<assembly company="Microsoft Corporation">"#;
    let (copyrights, holders, _authors) = detect_copyrights_from_text(content);

    assert!(copyrights.is_empty(), "copyrights: {copyrights:?}");
    assert!(holders.is_empty(), "holders: {holders:?}");
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
