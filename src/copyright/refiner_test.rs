use super::*;

// ── debug tests ──────────────────────────────────────────────────

#[test]
fn test_strip_trailing_original_authors() {
    assert_eq!(
        strip_trailing_original_authors("copyright by the original authors"),
        "copyright by the original"
    );
    assert_eq!(
        strip_trailing_original_authors("the original authors"),
        "the original"
    );
    assert_eq!(
        strip_trailing_original_authors("(c) by the respective authors"),
        "(c) by the respective authors",
        "should not strip 'respective authors'"
    );
    assert_eq!(
        strip_trailing_original_authors("Copyright (c) 2007-2010 the original author or authors"),
        "Copyright (c) 2007-2010 the original author or authors",
        "should not strip 'author or authors'"
    );
    assert_eq!(
        refine_holder("the original authors"),
        Some("the original".to_string())
    );
    assert_eq!(
        refine_copyright("copyright by the original authors"),
        Some("copyright by the original".to_string())
    );
}

#[test]
fn test_refine_copyright_preserves_portions_created_by_prefix() {
    let refined = refine_copyright(
            "Portions created by the Initial Developer are Copyright (C) 1998-2000 the Initial Developer.",
        )
        .unwrap();
    assert_eq!(
        refined,
        "Portions created by the Initial Developer are Copyright (C) 1998-2000 the Initial Developer",
        "refined={refined:?}"
    );
}

#[test]
fn test_refine_copyright_strips_leading_author_label() {
    assert_eq!(
        refine_copyright("author Vlad Roubtsov, (c) 2004"),
        Some("Vlad Roubtsov, (c) 2004".to_string())
    );
}

#[test]
fn test_refine_copyright_keeps_year_only_line() {
    assert_eq!(
        refine_copyright("Copyright 2000"),
        Some("Copyright 2000".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_obfuscated_email_after_dash() {
    assert_eq!(
        refine_copyright("Copyright (c) 2005, 2006 Nick Galbreath -- nickg at modp dot com"),
        Some("Copyright (c) 2005, 2006 Nick Galbreath".to_string()),
    );
    assert_eq!(
        refine_copyright("Copyright (c) 2005, 2006 Nick Galbreath - nickg at modp dot com"),
        Some("Copyright (c) 2005, 2006 Nick Galbreath".to_string()),
    );
}

#[test]
fn test_refine_author_discards_laboriously_took_the_trouble_junk() {
    assert_eq!(
        refine_author(
            "the authors laboriously took the trouble of searching for workarounds to make these compilers happy"
        ),
        None
    );
}

#[test]
fn test_refine_author_drops_generic_role_and_prose_fragments() {
    assert_eq!(refine_author("Philip"), None);
    assert_eq!(refine_author("john"), None);
    assert_eq!(refine_author("chunchu"), Some("chunchu".to_string()));
    assert_eq!(refine_author("Guide"), None);
    assert_eq!(refine_author("maintainers with write access"), None);
    assert_eq!(refine_author("schedule and monitor workflows"), None);
    assert_eq!(refine_author("for the sample crypto project"), None);
    assert_eq!(
        refine_author("the pkg-bazaar team"),
        Some("the pkg-bazaar team".to_string())
    );
    assert_eq!(
        refine_author("the University of California, Berkeley and its contributors"),
        Some("the University of California, Berkeley and its contributors".to_string())
    );
    assert_eq!(
        refine_author(
            "transition .transition https://github.com/d3/d3-transition/blob/master/README.md"
        ),
        None
    );
    assert_eq!(
        refine_author("Daniel Vaz Gaspar (https://github.com/dpgaspar/Flask-AppBuilder)"),
        Some("Daniel Vaz Gaspar (https://github.com/dpgaspar/Flask-AppBuilder)".to_string())
    );
    assert_eq!(
        refine_author("Daniel Vaz Gaspar"),
        Some("Daniel Vaz Gaspar".to_string())
    );
}

#[test]
fn test_refine_holder_discards_symbol_table_run_junk() {
    assert_eq!(
        refine_holder("(r), & 175, & 176, & 177, & 178, & 179, & 180, & 181, & 182, & 183"),
        None
    );
}

// ── strip_some_punct ─────────────────────────────────────────────

#[test]
fn test_strip_some_punct_basic() {
    assert_eq!(strip_some_punct(",hello,"), "hello");
}

#[test]
fn test_strip_some_punct_leading_dot() {
    assert_eq!(strip_some_punct(".hello"), "hello");
}

#[test]
fn test_strip_some_punct_trailing_paren() {
    assert_eq!(strip_some_punct("hello("), "hello");
}

#[test]
fn test_strip_some_punct_empty() {
    assert_eq!(strip_some_punct(""), "");
}

// ── strip_trailing_period ────────────────────────────────────────

#[test]
fn test_strip_trailing_period_normal() {
    assert_eq!(strip_trailing_period("Hello World."), "Hello World");
}

#[test]
fn test_strip_trailing_period_inc() {
    assert_eq!(strip_trailing_period("Acme Inc."), "Acme Inc.");
}

#[test]
fn test_strip_trailing_period_ltd() {
    assert_eq!(strip_trailing_period("Foo Ltd."), "Foo Ltd.");
}

#[test]
fn test_strip_trailing_period_acronym() {
    // "e.V." — second-to-last is uppercase, multi-word
    assert_eq!(strip_trailing_period("Foo e.V."), "Foo e.V.");
}

#[test]
fn test_strip_trailing_period_short_acronym() {
    // "b.v." — third-to-last is a period
    assert_eq!(strip_trailing_period("Foo b.v."), "Foo b.v.");
}

#[test]
fn test_strip_trailing_period_no_period() {
    assert_eq!(strip_trailing_period("Hello"), "Hello");
}

#[test]
fn test_strip_trailing_period_short() {
    assert_eq!(strip_trailing_period("P."), "P.");
}

#[test]
fn test_strip_trailing_period_empty() {
    assert_eq!(strip_trailing_period(""), "");
}

// ── strip_leading_numbers ────────────────────────────────────────

#[test]
fn test_strip_leading_numbers_basic() {
    assert_eq!(strip_leading_numbers("123 456 Hello"), "Hello");
}

#[test]
fn test_strip_leading_numbers_no_numbers() {
    assert_eq!(strip_leading_numbers("Hello World"), "Hello World");
}

#[test]
fn test_strip_leading_numbers_all_numbers() {
    assert_eq!(strip_leading_numbers("123 456"), "");
}

// ── strip_prefixes / strip_suffixes ──────────────────────────────

#[test]
fn test_strip_prefixes_basic() {
    let prefixes: HashSet<&str> = ["by", "and"].into_iter().collect();
    assert_eq!(strip_prefixes("by and John Doe", &prefixes), "John Doe");
}

#[test]
fn test_strip_suffixes_basic() {
    let suffixes: HashSet<&str> = [".", ",", "and"].into_iter().collect();
    assert_eq!(strip_suffixes("John Doe and", &suffixes), "John Doe");
}

// ── strip_unbalanced_parens ──────────────────────────────────────

#[test]
fn test_strip_unbalanced_parens_balanced() {
    assert_eq!(
        strip_unbalanced_parens("This is a super(c) string", '(', ')'),
        "This is a super(c) string"
    );
}

#[test]
fn test_strip_unbalanced_parens_unbalanced_close() {
    assert_eq!(
        strip_unbalanced_parens("This )(is a super(c) string)(", '(', ')'),
        "This  (is a super(c) string) "
    );
}

#[test]
fn test_strip_unbalanced_parens_lone_open() {
    assert_eq!(strip_unbalanced_parens("This ( is", '(', ')'), "This   is");
}

#[test]
fn test_strip_unbalanced_parens_lone_close() {
    assert_eq!(strip_unbalanced_parens("This ) is", '(', ')'), "This   is");
}

#[test]
fn test_strip_unbalanced_parens_single_open() {
    assert_eq!(strip_unbalanced_parens("(", '(', ')'), " ");
}

#[test]
fn test_strip_unbalanced_parens_single_close() {
    assert_eq!(strip_unbalanced_parens(")", '(', ')'), " ");
}

// ── strip_solo_quotes ────────────────────────────────────────────

#[test]
fn test_strip_solo_quotes_url() {
    assert_eq!(
        strip_solo_quotes("https://example.com/'"),
        "https://example.com/"
    );
}

#[test]
fn test_strip_solo_quotes_paren() {
    assert_eq!(strip_solo_quotes("foo)'"), "foo)");
}

// ── remove_dupe_copyright_words ──────────────────────────────────

#[test]
fn test_remove_dupe_spdx() {
    let result = remove_dupe_copyright_words("SPDX-FileCopyrightText 2024 Acme");
    assert_eq!(result, "Copyright 2024 Acme");
}

#[test]
fn test_remove_dupe_double_copyright() {
    let result = remove_dupe_copyright_words("Copyright Copyright 2024 Acme");
    assert_eq!(result, "Copyright 2024 Acme");
}

#[test]
fn test_remove_dupe_cppyright() {
    let result = remove_dupe_copyright_words("Cppyright 2024 Acme");
    assert_eq!(result, "Copyright 2024 Acme");
}

// ── remove_some_extra_words_and_punct ─────────────────────────────

#[test]
fn test_remove_extra_words_html() {
    let result = remove_some_extra_words_and_punct("<p>Hello</a>");
    assert_eq!(result, "Hello");
}

#[test]
fn test_remove_extra_words_mailto() {
    let result = remove_some_extra_words_and_punct("mailto:foo@bar.com");
    assert_eq!(result, "foo@bar.com");
}

#[test]
fn test_remove_extra_words_as_represented_by() {
    let result = remove_some_extra_words_and_punct("Acme Corp as represented by");
    assert_eq!(result, "Acme Corp as represented by");
}

// ── is_junk_copyright ────────────────────────────────────────────

#[test]
fn test_is_junk_copyright_bare_c() {
    assert!(is_junk_copyright("(c)"));
}

#[test]
fn test_is_junk_copyright_bare_copyright_c() {
    assert!(is_junk_copyright("Copyright (c)"));
}

#[test]
fn test_is_junk_copyright_normal() {
    assert!(!is_junk_copyright("Copyright 2024 Acme Inc."));
}

#[test]
fn test_is_junk_copyright_holder_or_simply() {
    assert!(is_junk_copyright("copyright holder or simply foo"));
}

#[test]
fn test_is_junk_copyright_patents_trade_secrets() {
    assert!(is_junk_copyright("copyrights, patents, trade secrets or"));
    assert!(is_junk_copyright(
        "copyright, patent, trademark, and attribution"
    ));
    assert!(is_junk_copyright(
        "copyright, including without limitation by United States"
    ));
    assert!(is_junk_copyright("COPYRIGHTS, TRADEMARKS OR"));
    assert!(is_junk_copyright("COPYRIGHT, TRADEMARK, TRADE SECRET OR"));
    assert!(is_junk_copyright("copyright, to do the following"));
}

#[test]
fn test_is_junk_copyright_trade_secrets_fragments() {
    assert!(is_junk_copyright("copyrights, trade secrets or"));
    assert!(is_junk_copyright("COPYRIGHT, TRADE SECRET OR"));
    assert!(is_junk_copyright(
        "copyright, trade secret, trademark or other intellectual property rights of"
    ));
    assert!(is_junk_copyright("COPYRIGHT (c) TRADEMARK"));
}

#[test]
fn test_is_junk_copyright_all_caps_placeholders() {
    assert!(is_junk_copyright(
        "Copyright (c) 1999-2008 MODULEAUTHOR endif"
    ));
}

#[test]
fn test_is_junk_copyright_proprietary() {
    assert!(is_junk_copyright("copyright, proprietary"));
    assert!(is_junk_copyright("copyright proprietary"));
    assert!(is_junk_copyright("proprietary"));
}

#[test]
fn test_is_junk_copyright_rsa() {
    assert!(is_junk_copyright("Copyright RSA"));
    assert!(is_junk_copyright("copyright rsa"));
}

#[test]
fn test_is_junk_copyright_math_c_variable() {
    assert!(is_junk_copyright("(c) Convert Chebyshev"));
    assert!(is_junk_copyright("(c) Multiply a Chebyshev"));
}

#[test]
fn test_is_junk_copyright_c_cast_ternary_and_bitwise_patterns() {
    assert!(is_junk_copyright("(c) (const unsigned char*)ptr"));
    assert!(is_junk_copyright("(c) c ? foo : bar"));
    assert!(is_junk_copyright("(c) c & 0x3f"));
    assert!(is_junk_copyright("(c) flags |= 0x80"));
}

#[test]
fn test_is_junk_copyright_year_only() {
    assert!(!is_junk_copyright("Copyright (c) 2003"));
    assert!(!is_junk_copyright("Copyright (C) 1995"));
    assert!(!is_junk_copyright("Copyright 2003"));
    assert!(!is_junk_copyright("(c) 2003"));
}

#[test]
fn test_is_junk_copyright_scan_phrase() {
    assert!(is_junk_copyright(
        "Measures the end-to-end composer copyright scan"
    ));
}

#[test]
fn test_is_junk_copyright_c_sign_path_fragment() {
    assert!(is_junk_copyright("(c) Ljoptsimple/AbstractOptionSpec"));
}

// ── refine_copyright ─────────────────────────────────────────────

#[test]
fn test_refine_copyright_basic() {
    let result = refine_copyright("Copyright 2024 Acme Inc.");
    assert_eq!(result, Some("Copyright 2024 Acme Inc.".to_string()));
}

#[test]
fn test_refine_copyright_empty() {
    assert_eq!(refine_copyright(""), None);
}

#[test]
fn test_refine_copyright_strips_junk_prefix() {
    let result = refine_copyright("by Copyright 2024 Acme");
    assert_eq!(result, Some("Copyright 2024 Acme".to_string()));
}

#[test]
fn test_refine_copyright_removes_space_before_comma() {
    let result = refine_copyright("Copyright (c) Free Software Foundation, Inc. , 2006");
    assert_eq!(
        result,
        Some("Copyright (c) Free Software Foundation, Inc., 2006".to_string())
    );
}

#[test]
fn test_refine_copyright_removes_space_before_internal_commas() {
    let result = refine_copyright("Copyright (c) 1989 , 1991 Free Software Foundation , Inc.");
    assert_eq!(
        result,
        Some("Copyright (c) 1989, 1991 Free Software Foundation, Inc.".to_string())
    );
}

#[test]
fn test_normalize_angle_bracket_comma_spacing_email() {
    assert_eq!(
        normalize_angle_bracket_comma_spacing("Acme <dev@acme.test>, Foo"),
        "Acme <dev@acme.test>, Foo"
    );
}

#[test]
fn test_normalize_angle_bracket_comma_spacing_non_email_tag_unchanged() {
    assert_eq!(
        normalize_angle_bracket_comma_spacing("Acme </p>, Foo"),
        "Acme </p>, Foo"
    );
    assert_eq!(
        normalize_angle_bracket_comma_spacing("Acme <www.example.com>, Foo"),
        "Acme <www.example.com>, Foo"
    );
}

#[test]
fn test_refine_copyright_normalizes_angle_bracket_email_comma_spacing() {
    let result = refine_copyright("Copyright 2024 Acme <dev@acme.test>, Foo");
    assert_eq!(
        result,
        Some("Copyright 2024 Acme <dev@acme.test>, Foo".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_x509_dn_fields_after_holder() {
    let result = refine_copyright(
        "Copyright (c) 1997 Microsoft Corp., OU Microsoft Corporation, CN Microsoft Root",
    );
    assert_eq!(
        result,
        Some("Copyright (c) 1997 Microsoft Corp.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_x509_dn_fields_after_ou() {
    let result = refine_copyright(
        "Copyright (c) 2005, OU OISTE Foundation Endorsed, CN OISTE WISeKey Global Root",
    );
    assert_eq!(
        result,
        Some("Copyright (c) 2005, OU OISTE Foundation".to_string())
    );
}

#[test]
fn test_refine_copyright_removes_space_before_comma_after_c_sign() {
    let result = refine_copyright("Copyright (c) , 2001-2011, Omega Tech. Co., Ltd.");
    assert_eq!(
        result,
        Some("Copyright (c), 2001-2011, Omega Tech. Co., Ltd.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_portions_of_fragment() {
    let result =
        refine_copyright("Copyright (c) 1991, 1999 Free Software Foundation, Inc. Portions of");
    assert_eq!(
        result,
        Some("Copyright (c) 1991, 1999 Free Software Foundation, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_dot_software() {
    let result = refine_copyright(
        "Copyright (c) Ian F. Darwin 1986, 1987, 1989, 1990, 1991, 1992, 1994, 1995. Software",
    );
    assert_eq!(
        result,
        Some(
            "Copyright (c) Ian F. Darwin 1986, 1987, 1989, 1990, 1991, 1992, 1994, 1995"
                .to_string()
        )
    );
}

#[test]
fn test_refine_copyright_strips_trailing_some_parts_of_fragment() {
    let result = refine_copyright(
        "copyright (c) 2012 The FreeType Project (www.freetype.org). Some parts of",
    );
    assert_eq!(
        result,
        Some("copyright (c) 2012 The FreeType Project (www.freetype.org)".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_angle_bracketed_www_domain_without_by() {
    let result = refine_copyright("Copyright (C) 2012 Altera <www.altera.com>");
    assert_eq!(result, Some("Copyright (C) 2012 Altera".to_string()));
}

#[test]
fn test_refine_copyright_keeps_angle_bracketed_www_domain_with_by() {
    let result = refine_copyright("Copyright 2011 by BitRouter <www.BitRouter.com>");
    assert_eq!(
        result,
        Some("Copyright 2011 by BitRouter <www.BitRouter.com>".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_comma_delimited_www_domain_clause() {
    let result = refine_copyright(
        "(c) Copyright 2004 Texas Instruments, <www.ti.com> Richard Woodruff <r-woodruff2@ti.com>",
    );
    assert_eq!(
        result,
        Some(
            "(c) Copyright 2004 Texas Instruments, Richard Woodruff <r-woodruff2@ti.com>"
                .to_string()
        )
    );
}

#[test]
fn test_refine_copyright_strips_trailing_mountain_view_ca() {
    let result = refine_copyright("Copyright 1993 by Sun Microsystems, Inc. Mountain View, CA.");
    assert_eq!(
        result,
        Some("Copyright 1993 by Sun Microsystems, Inc. Mountain View".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_comma_with_unicode_whitespace() {
    let result = refine_copyright("(c) by the respective authors,\u{00A0}");
    assert_eq!(result, Some("(c) by the respective authors".to_string()));
}

#[test]
fn test_refine_copyright_strips_trailing_paren_email_after_c_by() {
    let result = refine_copyright("(c) by Monty (xiphmont@mit.edu)");
    assert_eq!(result, Some("(c) by Monty".to_string()));
}

#[test]
fn test_refine_copyright_strips_trailing_division_of_company_suffix() {
    let input = "Copyright (c) 2006, Industrial Light & Magic, a division of Lucasfilm Entertainment Company Ltd.";
    assert_eq!(
        refine_copyright(input),
        Some("Copyright (c) 2006, Industrial Light & Magic".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_independent_jpeg_group_software_tail() {
    let result = refine_copyright(
        "(c) 1991-1992, Thomas G. Lane, Part of the Independent JPEG Group's software.",
    );
    assert_eq!(
        result,
        Some("(c) 1991-1992, Thomas G. Lane, Part of the Independent JPEG Group's".to_string())
    );
}

#[test]
fn test_refine_copyright_keeps_plain_email_after_comma() {
    let result = refine_copyright("Parts (c) 1999 David Airlie, airlied@linux.ie");
    assert_eq!(
        result,
        Some("Parts (c) 1999 David Airlie, airlied@linux.ie".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_fsf_address_tail() {
    let result = refine_copyright(
        "Copyright (c) 1989 Free Software Foundation, Inc. 51 Franklin St, Fifth Floor, Boston, MA 02110-1301 USA",
    );
    assert_eq!(
        result,
        Some("Copyright (c) 1989 Free Software Foundation, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_fsf_675_mass_ave_tail() {
    let result = refine_copyright(
        "Copyright (c) 1989 Free Software Foundation, Inc. 675 Mass Ave, Cambridge, MA",
    );
    assert_eq!(
        result,
        Some("Copyright (c) 1989 Free Software Foundation, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_sun_address_tail() {
    let result = refine_copyright(
        "Copyright 1997, 1998 by Sun Microsystems, Inc., 901 San Antonio Road, Palo Alto, California, 94303, U.S.A.",
    );
    assert_eq!(
        result,
        Some("Copyright 1997, 1998 by Sun Microsystems, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_realnetworks_address_tail() {
    let result = refine_copyright(
        "Copyright (c) 1995-2002 RealNetworks, Inc. and/or its suppliers. 2601 Elliott Avenue, Suite 1000, Seattle, Washington 98121 U.S.A.",
    );
    assert_eq!(
        result,
        Some("Copyright (c) 1995-2002 RealNetworks, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_and_or_its_suppliers_tail() {
    let result =
        refine_copyright("Copyright (c) 1995-2002 RealNetworks, Inc. and/or its suppliers");
    assert_eq!(
        result,
        Some("Copyright (c) 1995-2002 RealNetworks, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_write_to_fsf_tail() {
    let result = refine_copyright(
        "copyrighted by the Free Software Foundation, write to the Free Software Foundation we sometimes make exceptions for",
    );
    assert_eq!(
        result,
        Some("copyrighted by the Free Software Foundation".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_their_notice_reproduced_below_tail() {
    let result = refine_copyright(
        "parts (c) RSA Data Security, Inc. Their notice reproduced below in its entirety",
    );
    assert_eq!(
        result,
        Some("parts (c) RSA Data Security, Inc.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_license_name() {
    let result = refine_copyright(
        "(c) Copyright 2009 Hewlett-Packard Development Company, L.P. GNU GENERAL PUBLIC LICENSE",
    );
    assert_eq!(
        result,
        Some("(c) Copyright 2009 Hewlett-Packard Development Company, L.P.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_doc_generated_by() {
    let result = refine_copyright(
        "(c) Copyright 2010 by the http://wtforms.simplecodes.com WTForms Team, documentation generated by http://sphinx.pocoo.org/ Sphinx",
    );
    assert_eq!(
        result,
        Some("(c) Copyright 2010 by the http://wtforms.simplecodes.com WTForms Team".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_dash_software() {
    let result =
        refine_copyright("copyright (c) 1999, IBM Corporation., http://www.ibm.com. - software");
    assert_eq!(
        result,
        Some("copyright (c) 1999, IBM Corporation., http://www.ibm.com".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_et_al() {
    let result =
        refine_copyright("Copyright (c) 1998-2001, Daniel Stenberg, <daniel@haxx.se> , et al");
    assert_eq!(
        result,
        Some("Copyright (c) 1998-2001, Daniel Stenberg, <daniel@haxx.se>".to_string())
    );
}

#[test]
fn test_refine_copyright_drops_bare_copyrighted_software_phrase() {
    assert_eq!(refine_copyright("copyrighted software"), None);
}

#[test]
fn test_is_junk_copyright_template_placeholders() {
    let refined = refine_copyright("Copyright 2014-$ date.year pkg.author").unwrap();
    assert!(is_junk_copyright(&refined));

    let refined = refine_copyright("Copyright (c) 2019 pkg.author").unwrap();
    assert!(is_junk_copyright(&refined));

    let refined = refine_copyright("Copyright (c) 2012 pkg.author pkg.homepage").unwrap();
    assert!(is_junk_copyright(&refined));

    let refined = refine_copyright("(c) 2004-2010 year .format YYYY-MM-DD, -04").unwrap();
    assert!(is_junk_copyright(&refined));

    let refined = refine_copyright("Copyright 2010- < pkg.author >").unwrap();
    assert!(is_junk_copyright(&refined));
}

#[test]
fn test_strip_some_punct_trailing_comma() {
    assert_eq!(
        strip_some_punct("copyright Free Software Foundation,"),
        "copyright Free Software Foundation"
    );
    assert_eq!(
        refine_copyright("copyright Free Software Foundation , and is licensed under the"),
        Some("copyright Free Software Foundation".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_generated_tag() {
    let result = refine_copyright("Copyright (c) 2024 Acme Corp. @generated by protobuf");
    assert_eq!(result, Some("Copyright (c) 2024 Acme Corp.".to_string()));
}

#[test]
fn test_normalize_comma_spacing_normalizes_space_before_comma() {
    assert_eq!(
        normalize_comma_spacing("Stephan Mueller , Design"),
        "Stephan Mueller, Design"
    );
    assert_eq!(
        normalize_comma_spacing("Free Software Foundation , Inc."),
        "Free Software Foundation, Inc."
    );
    assert_eq!(normalize_comma_spacing("1989 , 1991"), "1989, 1991");
}

#[test]
fn test_truncate_trailing_boilerplate_baslerstr_address() {
    assert_eq!(
        refine_holder("SVOX AG, Baslerstr. 30, 8048 Zuerich, Switzerland"),
        Some("SVOX AG, Baslerstr. 30, 8048 Zuerich, Switzerland".to_string())
    );
    assert_eq!(
        refine_copyright(
            "Copyright (c) 2008-2009 SVOX AG, Baslerstr. 30, 8048 Zuerich, Switzerland",
        ),
        Some(
            "Copyright (c) 2008-2009 SVOX AG, Baslerstr. 30, 8048 Zuerich, Switzerland".to_string(),
        )
    );
}

#[test]
fn test_truncate_trailing_boilerplate_begin_license_block() {
    assert_eq!(
        refine_holder("Google Inc BEGIN LICENSE BLOCK"),
        Some("Google Inc".to_string())
    );
    assert_eq!(
        refine_copyright("Copyright (c) 2011 Google Inc BEGIN LICENSE BLOCK"),
        Some("Copyright (c) 2011 Google Inc".to_string())
    );
}

#[test]
fn test_strip_trailing_isc_after_inc() {
    assert_eq!(
        refine_holder("Internet Systems Consortium, Inc. ISC"),
        Some("Internet Systems Consortium, Inc.".to_string())
    );
    assert_eq!(
        refine_copyright("Copyright (c) 2004,2007 by Internet Systems Consortium, Inc. ISC"),
        Some("Copyright (c) 2004,2007 by Internet Systems Consortium, Inc.".to_string())
    );
}

#[test]
fn test_refine_holder_drops_notice_disclaimer_license() {
    assert_eq!(refine_holder("NOTICE, DISCLAIMER, and LICENSE"), None);
}

#[test]
fn test_refine_holder_truncates_lzo_version_tail() {
    assert_eq!(
        refine_holder("Markus Franz Xaver Johannes Oberhumer LZO version v"),
        Some("Markus Franz Xaver Johannes Oberhumer".to_string())
    );
}

// ── refine_holder ────────────────────────────────────────────────

#[test]
fn test_refine_holder_basic() {
    let result = refine_holder("Acme Inc.");
    assert_eq!(result, Some("Acme Inc.".to_string()));
}

#[test]
fn test_refine_holder_removes_embedded_url_token() {
    let result = refine_holder("the http://wtforms.simplecodes.com WTForms Team");
    assert_eq!(result, Some("the WTForms Team".to_string()));
}

#[test]
fn test_refine_holder_strips_angle_bracketed_www_domain() {
    let result = refine_holder("Texas Instruments, <www.ti.com> Richard Woodruff");
    assert_eq!(
        result,
        Some("Texas Instruments, Richard Woodruff".to_string())
    );
}

#[test]
fn test_refine_holder_strips_trailing_mountain_view_ca() {
    let result = refine_holder("Sun Microsystems, Inc. Mountain View, CA.");
    assert_eq!(
        result,
        Some("Sun Microsystems, Inc. Mountain View".to_string())
    );
}

#[test]
fn test_refine_holder_empty() {
    assert_eq!(refine_holder(""), None);
}

#[test]
fn test_refine_holder_junk() {
    assert_eq!(refine_holder("the"), None);
}

#[test]
fn test_refine_holder_drops_bare_software_token() {
    assert_eq!(refine_holder("software"), None);
}

#[test]
fn test_refine_holder_junk_contributors_as_and_public() {
    assert_eq!(refine_holder("contributors as"), None);
    assert_eq!(refine_holder("public"), None);
}

#[test]
fn test_refine_holder_junk_patents_trade_secrets_fragments() {
    assert_eq!(refine_holder("patents, trade secrets"), None);
    assert_eq!(refine_holder("patent, or trademark"), None);
    assert_eq!(
        refine_holder("including without limitation by United States"),
        None
    );
    assert_eq!(refine_holder("TRADEMARKS"), None);
}

#[test]
fn test_refine_holder_junk_notice_and_do_the_following() {
    assert_eq!(refine_holder("notice"), None);
    assert_eq!(refine_holder("do the following"), None);
}

#[test]
fn test_refine_holder_junk_changelog_timestamp_username() {
    assert_eq!(refine_holder("11:46 vruppert"), None);
}

#[test]
fn test_refine_holder_junk_template_placeholders() {
    assert_eq!(refine_holder("date.year pkg.author"), None);
    assert_eq!(refine_holder("pkg.author"), None);
    assert_eq!(refine_holder("format YYYY-MM-DD, -04"), None);
    assert_eq!(refine_holder("< pkg.author >"), None);
}

#[test]
fn test_refine_holder_junk_symbol_conversion_table() {
    assert_eq!(refine_holder("(tm) (TM) → ™ (r) (R) → ®"), None);
    assert_eq!(refine_holder("Dot ⟶ ˙"), None);
}

#[test]
fn test_refine_holder_drops_mojibake_unicode_table_runs() {
    assert_eq!(refine_holder("ÃÃÃÃÃÃÃÃÃÃÃÃ"), None);
}

#[test]
fn test_refine_holder_junk_legal_disclaimer_fragments() {
    assert_eq!(
        refine_holder("NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES"),
        None
    );
    assert_eq!(refine_holder("TRADEMARK, TRADE SECRET"), None);
    assert_eq!(refine_holder("NOTICE, LICENSE AND DISCLAIMER."), None);
    assert_eq!(refine_holder("the Standard"), None);
    assert_eq!(refine_holder("The Product"), None);
    assert_eq!(refine_holder("proprietary"), None);
}

#[test]
fn test_refine_holder_junk_short_rsa_and_ecos_title() {
    assert_eq!(refine_holder("RSA"), None);
    assert_eq!(refine_holder("the Sample Embedded Operating System"), None);
}

#[test]
fn test_refine_holder_junk_math_c_functions() {
    assert_eq!(refine_holder("Convert Chebyshev"), None);
    assert_eq!(refine_holder("Multiply a Chebyshev"), None);
}

#[test]
fn test_refine_holder_strips_ecos_title_prefix_keeps_company() {
    assert_eq!(
        refine_holder("the Sample Embedded Operating System., Red Hat, Inc."),
        Some("Red Hat, Inc.".to_string())
    );
}

#[test]
fn test_refine_holder_junk_all_caps_placeholders() {
    assert_eq!(refine_holder("MODULEAUTHOR endif"), None);
    assert_eq!(refine_holder("THE PACKAGE'S"), None);
    assert_eq!(refine_holder("THE TOOLKIT'S"), None);
}

#[test]
fn test_refine_holder_strips_trailing_authors_section_label() {
    assert_eq!(
        refine_holder("IBM, Corp. Authors Anthony Liguori"),
        Some("IBM, Corp.".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_authors_clause() {
    let result =
        refine_copyright("Copyright IBM, Corp. 2007 Authors Anthony Liguori <aliguori@us.ibm.com>");
    assert_eq!(result, Some("Copyright IBM, Corp. 2007".to_string()));
}

#[test]
fn test_refine_copyright_strips_trailing_document_authors_clause() {
    let result = refine_copyright(
        "Copyright (c) 2011 Joyent, Inc. and the persons identified as document authors.",
    );
    assert_eq!(result, Some("Copyright (c) 2011 Joyent, Inc.".to_string()));
}

#[test]
fn test_refine_copyright_keeps_authors_clause_when_multiple_names() {
    let result = refine_copyright(
        "Copyright (c) 2006-2008 One Laptop Per Child Authors Zephaniah E. Hull Andres Salomon <dilinger@debian.org>",
    );
    assert_eq!(
            result,
            Some(
                "Copyright (c) 2006-2008 One Laptop Per Child Authors Zephaniah E. Hull Andres Salomon <dilinger@debian.org>"
                    .to_string()
            )
        );
}

#[test]
fn test_refine_copyright_keeps_authors_when_part_of_product_name() {
    let result =
        refine_copyright("Copyright (c) 2019 The Bootstrap Authors https://getbootstrap.com");
    assert_eq!(
        result,
        Some("Copyright (c) 2019 The Bootstrap Authors https://getbootstrap.com".to_string())
    );
}

#[test]
fn test_refine_holder_strips_trailing_document_authors_clause() {
    let result = refine_holder("Joyent, Inc. and the persons identified as document authors.");
    assert_eq!(result, Some("Joyent, Inc.".to_string()));
}

#[test]
fn test_refine_copyright_preserves_maintainer_suffix() {
    let result = refine_copyright("Copyright (c) 1998-2000 Michel Aubry, Maintainer");
    assert_eq!(
        result,
        Some("Copyright (c) 1998-2000 Michel Aubry, Maintainer".to_string())
    );
}

#[test]
fn test_refine_holder_preserves_maintainer_suffix() {
    assert_eq!(
        refine_holder("Michel Aubry, Maintainer"),
        Some("Michel Aubry, Maintainer".to_string())
    );
}

#[test]
fn test_refine_holder_junk_patent_and_treaties_fragments() {
    assert_eq!(refine_holder("treaties"), None);
    assert_eq!(
        refine_holder("patent or other licenses necessary and to obtain"),
        None
    );
}

#[test]
fn test_refine_holder_strips_trailing_x509_dn_fields() {
    assert_eq!(
        refine_holder("Microsoft Corp., OU Microsoft Corporation, CN Microsoft Root"),
        Some("Microsoft Corp.".to_string())
    );
    assert_eq!(
        refine_holder("OISTE Foundation Endorsed, CN OISTE WISeKey Global Root"),
        Some("OISTE Foundation".to_string())
    );
}

#[test]
fn test_refine_holder_normalizes_angle_bracket_email_comma_spacing() {
    let result = refine_holder("Acme <dev@acme.test>, Foo");
    assert_eq!(result, Some("Acme <dev@acme.test>, Foo".to_string()));
}

#[test]
fn test_refine_holder_strips_trailing_comma_software() {
    let result = refine_holder("Ian F. Darwin,,, Software");
    assert_eq!(result, Some("Ian F. Darwin".to_string()));
}

#[test]
fn test_refine_author_drops_config_and_legal_junk_fragments() {
    assert_eq!(refine_author("with the mode of 000"), None);
    assert_eq!(
        refine_author("kernel afs. skip AFS metadata and ACLs"),
        None
    );
    assert_eq!(refine_author("with a FSF"), None);
    assert_eq!(refine_author("with a DCO"), None);
    assert_eq!(refine_author("gives unlimited"), None);
    assert_eq!(
        refine_author(
            "Word Assigns past and future changes. new src/libgcrypt.pc.in, src/Makefile.am, src/secmem.c"
        ),
        None
    );
}

#[test]
fn test_refine_author_truncates_bug_reports_tail() {
    assert_eq!(
        refine_author(
            "Werner Koch <wk@gnupg.org> Bug reports https://bugs.gnupg.org Security related bug reports <security@gnupg.org> End-of-life TBD"
        ),
        Some("Werner Koch <wk@gnupg.org>".to_string())
    );
}

#[test]
fn test_refine_author_strips_trailing_comma_and() {
    assert_eq!(
        refine_author("Philip Hazel, and"),
        Some("Philip Hazel".to_string())
    );
}

#[test]
fn test_refine_author_drops_glibc_prose_fragments() {
    assert_eq!(
        refine_author(
            "Maintainers <debian-glibc@lists.debian.org> from https://sourceware.org/git/glibc.git"
        ),
        None
    );
    assert_eq!(refine_author("versions, and"), None);
    assert_eq!(refine_author("makes"), None);
    assert_eq!(refine_author("grants irrevocable"), None);
    assert_eq!(refine_author("version information"), None);
    assert_eq!(
        refine_author("not responsible for the consequences of use of"),
        None
    );
    assert_eq!(
        refine_author("at SunPro, a Sun Microsystems, Inc. business"),
        None
    );
}

#[test]
fn test_refine_holder_drops_cc0_and_libgcrypt_junk_fragments() {
    assert_eq!(
        refine_holder("Related Rights (defined below) upon the creator and subsequent"),
        None
    );
    assert_eq!(refine_holder("Related"), None);
    assert_eq!(refine_holder("related or neighboring"), None);
    assert_eq!(refine_holder("was owned solely by FSF"), None);
    assert_eq!(refine_holder("years may be listed"), None);
}

#[test]
fn test_is_junk_copyright_drops_cc0_and_libgcrypt_junk_fragments() {
    assert!(is_junk_copyright(
        "copyright and related or neighboring rights"
    ));
    assert!(is_junk_copyright("copyright was owned solely by FSF"));
    assert!(is_junk_copyright("copyright years may be listed"));
}

#[test]
fn test_refine_holder_strips_trailing_et_al() {
    let result = refine_holder("Daniel Stenberg, et al");
    assert_eq!(result, Some("Daniel Stenberg".to_string()));
}

#[test]
fn test_refine_author_normalizes_angle_bracket_email_comma_spacing() {
    let result = refine_author("dev <dev@acme.test>, Foo");
    assert_eq!(result, Some("dev <dev@acme.test>, Foo".to_string()));
}

#[test]
fn test_refine_holder_does_not_strip_normal_comma_separated_names() {
    assert_eq!(
        refine_holder("Sam Leffler, Errno Consulting, Atheros Communications, Inc."),
        Some("Sam Leffler, Errno Consulting, Atheros Communications, Inc.".to_string())
    );
}

#[test]
fn test_refine_holder_does_not_strip_lp_suffix() {
    assert_eq!(
        refine_holder("Hewlett-Packard Development Company, L.P."),
        Some("Hewlett-Packard Development Company, L.P.".to_string())
    );
}

#[test]
fn test_refine_holder_strips_prefix() {
    let result = refine_holder("by Acme Corp");
    assert_eq!(result, Some("Acme Corp".to_string()));
}

#[test]
fn test_refine_holder_strips_trailing_period() {
    let result = refine_holder("IBM Corporation.");
    assert_eq!(result, Some("IBM Corporation".to_string()));
}

#[test]
fn test_refine_holder_strips_trailing_division_of_company_suffix() {
    let input = "Industrial Light & Magic, a division of Lucas Digital Ltd. LLC";
    assert_eq!(
        refine_holder(input),
        Some("Industrial Light & Magic".to_string())
    );
}

#[test]
fn test_refine_holder_strips_trailing_period_after_trailing_comma() {
    let result = refine_holder("Sun Microsystems.,");
    assert_eq!(result, Some("Sun Microsystems".to_string()));
}

#[test]
fn test_refine_holder_strips_independent_jpeg_group_software_tail() {
    let result = refine_holder("Thomas G. Lane, Part of the Independent JPEG Group's software");
    assert_eq!(
        result,
        Some("Thomas G. Lane, Part of the Independent JPEG Group's".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_contributor_parens_after_org() {
    let result = refine_copyright(
        "Copyright (c) 1998-2001 VideoLAN (Johan Bilien <jobi@via.ecp.fr> and Gildas Bazin <gbazin@netcourrier.com> )",
    );
    assert_eq!(
            result,
            Some(
                "Copyright (c) 1998-2001 VideoLAN Johan Bilien <jobi@via.ecp.fr> and Gildas Bazin <gbazin@netcourrier.com>".to_string()
            )
        );
}

#[test]
fn test_refine_holder_strips_contributor_parens_after_org() {
    let result = refine_holder("VideoLAN (Johan Bilien and Gildas Bazin)");
    assert_eq!(
        result,
        Some("VideoLAN Johan Bilien and Gildas Bazin".to_string())
    );
}

#[test]
fn test_refine_holder_strips_see_authors_suffix() {
    let result = refine_holder("Carsten Haitzler and various contributors (see AUTHORS)");
    assert_eq!(
        result,
        Some("Carsten Haitzler and various contributors".to_string())
    );
}

#[test]
fn test_refine_holder_strips_trailing_javadoc_tags() {
    let result = refine_holder("Michal Migurski @version 1.0");
    assert_eq!(result, Some("Michal Migurski".to_string()));
}

#[test]
fn test_refine_copyright_strips_see_authors_suffix() {
    let result = refine_copyright(
        "Copyright (c) 2000 Carsten Haitzler and various contributors (see AUTHORS)",
    );
    assert_eq!(
        result,
        Some("Copyright (c) 2000 Carsten Haitzler and various contributors".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_javadoc_tags() {
    let result = refine_copyright("copyright 2005 Michal Migurski @version 1.0");
    assert_eq!(result, Some("copyright 2005 Michal Migurski".to_string()));
}

// ── refine_author ────────────────────────────────────────────────

#[test]
fn test_refine_author_basic() {
    let result = refine_author("John Doe");
    assert_eq!(result, Some("John Doe".to_string()));
}

#[test]
fn test_refine_author_empty() {
    assert_eq!(refine_author(""), None);
}

#[test]
fn test_refine_author_junk() {
    assert_eq!(refine_author("james hacker"), None);
    assert_eq!(refine_author("who hopes"), None);
}

#[test]
fn test_refine_author_strips_author_prefix() {
    let result = refine_author("author John Doe");
    assert_eq!(result, Some("John Doe".to_string()));
}

#[test]
fn test_refine_author_email_and_name() {
    let result = refine_author("@author stephane@hillion.org Stephane Hillion");
    assert_eq!(
        result,
        Some("stephane@hillion.org Stephane Hillion".to_string())
    );
}

#[test]
fn test_refine_author_strips_trailing_javadoc_tags() {
    let result = refine_author("stephane@hillion.org Stephane Hillion @version 1.0");
    assert_eq!(
        result,
        Some("stephane@hillion.org Stephane Hillion".to_string())
    );
}

#[test]
fn test_refine_author_strips_trailing_paren_years() {
    let result = refine_author("author: Theo de Raadt (1995-1999)");
    assert_eq!(result, Some("Theo de Raadt".to_string()));
}

#[test]
fn test_refine_author_strips_trailing_bare_c_clause() {
    let result = refine_author(
        "Denis Joseph Barrow (djbarrow@de.ibm.com,barrow_dj@yahoo.com) (c) 2000 IBM Corp",
    );
    assert_eq!(
        result,
        Some("Denis Joseph Barrow (djbarrow@de.ibm.com,barrow_dj@yahoo.com)".to_string())
    );
}

#[test]
fn test_refine_author_junk_prefix() {
    assert_eq!(refine_author("httpProxy something"), None);
}

// ── strip_all_unbalanced_parens ──────────────────────────────────

#[test]
fn test_strip_all_unbalanced_parens_mixed() {
    let result = strip_all_unbalanced_parens("Hello ) World < Foo >");
    // The lone ) and the balanced <> should be handled.
    assert_eq!(result, "Hello   World < Foo >");
}

// ── URL slash stripping ──────────────────────────────────────────

#[test]
fn test_refine_copyright_url_trailing_slash() {
    let result =
        refine_copyright("Copyright (c) 2007 Free Software Foundation, Inc. http://fsf.org/");
    assert_eq!(
        result,
        Some("Copyright (c) 2007 Free Software Foundation, Inc. http://fsf.org".to_string())
    );
}

#[test]
fn test_refine_copyright_keeps_w3c_registered_paren_group() {
    let result = refine_copyright("Copyright (c) YEAR W3C(r) (MIT, ERCIM, Keio, Beihang).");
    assert_eq!(
        result,
        Some("Copyright (c) YEAR W3C(r) (MIT, ERCIM, Keio, Beihang)".to_string())
    );
}

#[test]
fn test_refine_holder_sk() {
    assert_eq!(refine_holder("S K (xz64)"), Some("S K".to_string()));
    assert_eq!(refine_holder("S K"), Some("S K".to_string()));
}

#[test]
fn test_refine_holder_strips_trailing_single_digit_token() {
    assert_eq!(
        refine_holder("Waterloo Micro. 8"),
        Some("Waterloo Micro".to_string())
    );
}

#[test]
fn test_refine_copyright_strips_trailing_digit_then_period() {
    assert_eq!(
        refine_copyright("(c) 1985 Waterloo Micro. 8"),
        Some("(c) 1985 Waterloo Micro".to_string())
    );
}

#[test]
fn test_refine_author_drops_d3_transition_markdown_link_fragment() {
    assert_eq!(
        refine_author(
            "transition .transition https://github.com/d3/d3-transition/blob/master/README.md"
        ),
        None
    );
}

#[test]
fn test_refine_author_drops_path_like_fragment() {
    assert_eq!(refine_author("from/authors/alphabetic"), None);
}

#[test]
fn test_refine_author_drops_dollar_prefixed_code_tokens() {
    assert_eq!(refine_author("Agatha Christie, $sort"), None);
    assert_eq!(refine_author("$limit 10"), None);
}

#[test]
fn test_refine_author_drops_the_current_user_phrase() {
    assert_eq!(refine_author("the current user"), None);
}

#[test]
fn test_refine_author_drops_point_to_the_phrase() {
    assert_eq!(refine_author("point to the"), None);
}

#[test]
fn test_refine_copyright_drops_css_footer_noise() {
    assert!(is_junk_copyright("Copyright footer"));
    assert!(is_junk_copyright("Copyright, Legal Notice"));
    assert!(is_junk_copyright("copyright color 666666"));
}

#[test]
fn test_refine_holder_drops_css_selector_noise() {
    assert_eq!(refine_holder("footer"), None);
    assert_eq!(refine_holder("Legal Notice"), None);
    assert_eq!(refine_holder("color 666666"), None);
}
