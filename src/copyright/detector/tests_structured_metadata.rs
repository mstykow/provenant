// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use super::*;

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
fn test_copyright_prefix_preserved_with_html_tags() {
    let input = "    Copyright © 1998       <s>Tom Tromey</s>\n    Copyright © 1999       <s>Free Software Foundation, Inc.</s>";
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
    let input = "<li><p class=\"Legal\" style=\"margin-left: 0pt;\">Copyright © 2002-2009 \n\t Charlie Poole</p></li>";
    let (c, _h, _a) = detect_copyrights_from_text(input);
    assert!(
        c.iter()
            .any(|cr| cr.copyright == "Copyright (c) 2002-2009 Charlie Poole"),
        "Expected merged Copyright (c) statement, got: {:?}",
        c.iter().map(|cr| &cr.copyright).collect::<Vec<_>>()
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
