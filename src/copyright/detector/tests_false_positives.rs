// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn test_detect_arch_floppy_h_bare_1995_dropped_for_x86() {
    let content =
        "* Copyright (C) 1995\n */\n#ifndef _ASM_X86_FLOPPY_H\n#define _ASM_X86_FLOPPY_H\n";
    let (copyrights, _holders, _authors) = detect_copyrights_from_text(content);
    assert!(copyrights.is_empty());
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
fn test_detect_no_copyright() {
    let (c, h, a) = detect_copyrights_from_text("This is just some random code.");
    assert!(c.is_empty());
    assert!(h.is_empty());
    assert!(a.is_empty());
}

#[test]
fn test_detect_junk_filtered() {
    let (c, _h, _a) = detect_copyrights_from_text("Copyright (c)");
    assert!(
        c.is_empty(),
        "Bare 'Copyright (c)' should be filtered as junk"
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
fn test_doc_doc_no_overabsorb() {
    let input = "are copyrighted by Douglas C. Schmidt and his research group at Washington University, University of California, Irvine, and Vanderbilt University, Copyright (c) 1993-2008, all rights reserved.";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright == "copyrighted by Douglas C. Schmidt and his research group at Washington University, University of California, Irvine, and Vanderbilt University, Copyright (c) 1993-2008"),
        "Should merge trailing Copyright (c) clause, got: {:?}",
        c
    );
}
