// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_academy_copyright() {
    let input = "Academy Copyright 2008 by the VideoLAN team";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter().any(|cr| cr.copyright.contains("VideoLAN team")),
        "Should include holder after Academy Copyright, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
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
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright.contains("MIT, ERCIM, Keio University")),
        "expected W3C copyright with MIT/ERCIM/Keio, got: {c:#?}"
    );
}

#[test]
fn test_detect_copyright_with_dots_single_line() {
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
fn test_tomcat_footer_trademark_line_not_absorbed_into_copyright() {
    let input = "Copyright (c) 1999-2026, The Apache Software Foundation\nApache Tomcat, Tomcat, Apache, the Apache Tomcat logo and the Apache logo\nare either registered trademarks or trademarks of the Apache Software Foundation.";
    let (copyrights, holders, _authors) = detect_copyrights_from_text(input);

    assert!(
        copyrights
            .iter()
            .any(|c| c.copyright == "Copyright (c) 1999-2026, The Apache Software Foundation"),
        "copyrights: {copyrights:?}"
    );
    assert!(
        !copyrights.iter().any(|c| c
            .copyright
            .contains("Apache Tomcat, Tomcat, Apache, the Apache Tomcat")),
        "copyrights: {copyrights:?}"
    );
    assert!(
        holders
            .iter()
            .any(|h| h.holder == "The Apache Software Foundation"),
        "holders: {holders:?}"
    );
    assert!(
        !holders.iter().any(|h| h
            .holder
            .contains("Apache Tomcat, Tomcat, Apache, the Apache Tomcat")),
        "holders: {holders:?}"
    );
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
