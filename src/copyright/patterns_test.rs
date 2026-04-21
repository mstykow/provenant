// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_patterns_compile() {
    // Force lazy initialization — panics if any regex is invalid
    let _ = &*COMPILED_PATTERNS;
}

#[test]
fn test_copyright_markers() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Copyright"), PosTag::Copy);
    assert_eq!(p.match_token("copyright"), PosTag::Copy);
    assert_eq!(p.match_token("(c)"), PosTag::Copy);
    assert_eq!(p.match_token("(C)"), PosTag::Copy);
    assert_eq!(p.match_token("COPR."), PosTag::Copy);
    assert_eq!(p.match_token("Copr"), PosTag::Copy);
    assert_eq!(p.match_token("COPYRIGHT"), PosTag::Copy);
    assert_eq!(p.match_token("Copyrights"), PosTag::Copy);
    assert_eq!(p.match_token("copyrighted"), PosTag::Copy);
    assert_eq!(p.match_token("COPYRIGHTED"), PosTag::Copy);
    assert_eq!(p.match_token("CopyRights"), PosTag::Copy);
}

#[test]
fn test_copyright_typos() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Copyrighy"), PosTag::Copy);
    assert_eq!(p.match_token("Copyirght"), PosTag::Copy);
    assert_eq!(p.match_token("Cppyright"), PosTag::Copy);
}

#[test]
fn test_copyright_exceptions() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Copyright.txt"), PosTag::Nn);
    assert_eq!(p.match_token("copyrighted."), PosTag::Nn);
    assert_eq!(p.match_token("copyrights."), PosTag::Nn);
    assert_eq!(p.match_token("COPYRIGHTS."), PosTag::Nn);
    assert_eq!(p.match_token("COPYRIGHTED."), PosTag::Nn);
    assert_eq!(p.match_token("copyright.)"), PosTag::Nn);
}

#[test]
fn test_copyright_special_forms() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Bundle-Copyright"), PosTag::Copy);
    assert_eq!(p.match_token("(c)opyright"), PosTag::Copy);
    assert_eq!(p.match_token("(c)opyleft"), PosTag::Copy);
    assert_eq!(p.match_token("opyright"), PosTag::Copy);
    assert_eq!(p.match_token("opyleft"), PosTag::Copy);
    assert_eq!(p.match_token("Copyright,"), PosTag::Copy);
    assert_eq!(p.match_token("copyright'>"), PosTag::Copy);
    assert_eq!(p.match_token("@Copyright"), PosTag::Copy);
    assert_eq!(p.match_token("(C),"), PosTag::Copy);
    assert_eq!(p.match_token("copr"), PosTag::Copy);
    assert_eq!(p.match_token("AssemblyCopyright"), PosTag::Copy);
}

#[test]
fn test_spdx() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("SPDX-FileCopyrightText"), PosTag::Copy);
    assert_eq!(p.match_token("SPDX-SnippetCopyrightText"), PosTag::Copy);
    assert_eq!(p.match_token("SPDX-FileContributor"), PosTag::SpdxContrib);
}

#[test]
fn test_rights_reserved() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Rights"), PosTag::Right);
    assert_eq!(p.match_token("rights"), PosTag::Right);
    assert_eq!(p.match_token("RIGHTS"), PosTag::Right);
    assert_eq!(p.match_token("Reserved"), PosTag::Reserved);
    assert_eq!(p.match_token("Reserved."), PosTag::Reserved);
    assert_eq!(p.match_token("RESERVED"), PosTag::Reserved);
    assert_eq!(p.match_token("Reversed"), PosTag::Reserved);
}

#[test]
fn test_rights_reserved_multilingual() {
    let p = &*COMPILED_PATTERNS;
    // German
    assert_eq!(p.match_token("Rechte"), PosTag::Right);
    assert_eq!(p.match_token("Vorbehalten"), PosTag::Reserved);
    // French
    assert_eq!(p.match_token("droits"), PosTag::Right);
    assert_eq!(p.match_token("réservés"), PosTag::Reserved);
    // Spanish
    assert_eq!(p.match_token("derechos"), PosTag::Right);
    assert_eq!(p.match_token("Reservados"), PosTag::Reserved);
    // Dutch
    assert_eq!(p.match_token("rechten"), PosTag::Right);
    assert_eq!(p.match_token("Voorbehouden"), PosTag::Reserved);
}

#[test]
fn test_is_held() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("is"), PosTag::Is);
    assert_eq!(p.match_token("are"), PosTag::Is);
    assert_eq!(p.match_token("held"), PosTag::Held);
}

#[test]
fn test_notice() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("NOTICE"), PosTag::Notice);
    assert_eq!(p.match_token("notice"), PosTag::Notice);
    assert_eq!(p.match_token("NOTICE."), PosTag::Junk);
    assert_eq!(p.match_token("notices"), PosTag::Junk);
}

#[test]
fn test_conjunctions() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("and"), PosTag::Cc);
    assert_eq!(p.match_token("And"), PosTag::Cc);
    assert_eq!(p.match_token("AND"), PosTag::Cc);
    assert_eq!(p.match_token("&"), PosTag::Cc);
    assert_eq!(p.match_token("and/or"), PosTag::Cc);
    assert_eq!(p.match_token(","), PosTag::Cc);
    assert_eq!(p.match_token("et"), PosTag::Cc);
    assert_eq!(p.match_token("und"), PosTag::Cc);
}

#[test]
fn test_prepositions() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("of"), PosTag::Of);
    assert_eq!(p.match_token("OF"), PosTag::Of);
    assert_eq!(p.match_token("by"), PosTag::By);
    assert_eq!(p.match_token("BY"), PosTag::By);
    assert_eq!(p.match_token("in"), PosTag::In);
    assert_eq!(p.match_token("en"), PosTag::In);
    assert_eq!(p.match_token("to"), PosTag::To);
}

#[test]
fn test_van_particles() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("van"), PosTag::Van);
    assert_eq!(p.match_token("Van"), PosTag::Van);
    assert_eq!(p.match_token("von"), PosTag::Van);
    assert_eq!(p.match_token("Von"), PosTag::Van);
    assert_eq!(p.match_token("du"), PosTag::Van);
}

#[test]
fn test_dash() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("-"), PosTag::Dash);
    assert_eq!(p.match_token("--"), PosTag::Dash);
    assert_eq!(p.match_token("/"), PosTag::Dash);
}

#[test]
fn test_others() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("others"), PosTag::Oth);
    assert_eq!(p.match_token("Others"), PosTag::Oth);
    assert_eq!(p.match_token("et. al."), PosTag::Oth);
}

#[test]
fn test_portions() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Portions"), PosTag::Portions);
    assert_eq!(p.match_token("portions"), PosTag::Portions);
    assert_eq!(p.match_token("Parts"), PosTag::Portions);
    assert_eq!(p.match_token("part"), PosTag::Portions);
}

#[test]
fn test_year_patterns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("2024"), PosTag::Yr);
    assert_eq!(p.match_token("1999"), PosTag::Yr);
    assert_eq!(p.match_token("1960"), PosTag::Yr);
    assert_eq!(p.match_token("202x"), PosTag::Yr);
    // Bug fix: beyond Python's 2039 limit
    assert_eq!(p.match_token("2040"), PosTag::Yr);
    assert_eq!(p.match_token("2099"), PosTag::Yr);
}

#[test]
fn test_year_ranges() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("2020-2024"), PosTag::Yr);
    assert_eq!(p.match_token("2010-202x"), PosTag::Yr);
    assert_eq!(p.match_token("1999,2000"), PosTag::Yr);
    assert_eq!(p.match_token("2020-present"), PosTag::Yr);
}

#[test]
fn test_year_iso_date() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("2024-12-09"), PosTag::Yr);
    assert_eq!(p.match_token("2024-1-9"), PosTag::Yr);
}

#[test]
fn test_year_plus() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("2024+"), PosTag::YrPlus);
}

#[test]
fn test_bare_year() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("99"), PosTag::BareYr);
    assert_eq!(p.match_token("80"), PosTag::BareYr);
}

#[test]
fn test_year_fsf_comma() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("88,"), PosTag::Yr);
    assert_eq!(p.match_token("93,"), PosTag::Yr);
}

#[test]
fn test_year_slash_date() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("08/95"), PosTag::Yr);
    assert_eq!(p.match_token("12/99"), PosTag::Yr);
}

#[test]
fn test_year_special() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("$date-of-software"), PosTag::Yr);
    assert_eq!(p.match_token("$date-of-document"), PosTag::Yr);
    assert_eq!(p.match_token("LastChangedDate"), PosTag::Yr);
    // Dollar sign as year-end placeholder (e.g. "2011-$")
    eprintln!("2011-$ = {:?}", p.match_token("2011-$"));
    eprintln!("2010-$ = {:?}", p.match_token("2010-$"));
}

#[test]
fn test_cardinals() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("5"), PosTag::Cds);
    assert_eq!(p.match_token("29"), PosTag::Cds);
    assert_eq!(p.match_token("100"), PosTag::Cd);
    assert_eq!(p.match_token("3.14"), PosTag::Cd);
}

#[test]
fn test_special_tokens() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("following"), PosTag::Following);
    assert_eq!(p.match_token("Holders"), PosTag::Holder);
    assert_eq!(p.match_token("holders"), PosTag::Holder);
    assert_eq!(p.match_token("HOLDER"), PosTag::Holder);
    assert_eq!(p.match_token("MIT"), PosTag::Mit);
    assert_eq!(p.match_token("MIT,"), PosTag::Caps);
    assert_eq!(p.match_token("Linux"), PosTag::Linux);
    assert_eq!(p.match_token("("), PosTag::Parens);
    assert_eq!(p.match_token(")"), PosTag::Parens);
    assert_eq!(p.match_token("AT"), PosTag::At);
    assert_eq!(p.match_token("DOT"), PosTag::Dot);
    assert_eq!(p.match_token("dot"), PosTag::Dot);
    assert_eq!(p.match_token("OU"), PosTag::Ou);
}

#[test]
fn test_month_day() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Feb"), PosTag::Month);
    assert_eq!(p.match_token("Mar"), PosTag::Month);
    assert_eq!(p.match_token("Dec"), PosTag::Month);
    assert_eq!(p.match_token("Monday"), PosTag::Day);
    assert_eq!(p.match_token("friday"), PosTag::Day);
}

#[test]
fn test_catch_all() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("randomword"), PosTag::Nn);
    assert_eq!(p.match_token("xyzzy"), PosTag::Nn);
}

// ===== Tests for batch 2 patterns =====

#[test]
fn test_author_markers() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Author"), PosTag::Auth);
    assert_eq!(p.match_token("author"), PosTag::Auth);
    assert_eq!(p.match_token("@author"), PosTag::Auth);
    assert_eq!(p.match_token("@Authors"), PosTag::Auth);
    assert_eq!(p.match_token("MODULEAUTHOR"), PosTag::Auth);
    assert_eq!(p.match_token("Authors"), PosTag::Auths);
    assert_eq!(p.match_token("Author(s)"), PosTag::Auths);
    assert_eq!(p.match_token("__authors__"), PosTag::Auths);
    assert_eq!(p.match_token("__contributor__"), PosTag::Auths);
}

#[test]
fn test_author_dot() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Author."), PosTag::AuthDot);
    assert_eq!(p.match_token("Authors."), PosTag::AuthDot);
    assert_eq!(p.match_token("authors,"), PosTag::AuthDot);
    assert_eq!(p.match_token("al."), PosTag::AuthDot);
}

#[test]
fn test_auth2_patterns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Written"), PosTag::Auth2);
    assert_eq!(p.match_token("written"), PosTag::Auth2);
    assert_eq!(p.match_token("Created"), PosTag::Auth2);
    assert_eq!(p.match_token("Developed"), PosTag::Auth2);
    assert_eq!(p.match_token("Maintained"), PosTag::Auth2);
    assert_eq!(p.match_token("Coded"), PosTag::Auth2);
    assert_eq!(p.match_token("Rewritten"), PosTag::Auth2);
}

#[test]
fn test_maintainer_patterns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Maintainer"), PosTag::Maint);
    assert_eq!(p.match_token("maintainers"), PosTag::Maint);
    assert_eq!(p.match_token("Developer"), PosTag::Maint);
    assert_eq!(p.match_token("developers"), PosTag::Maint);
    assert_eq!(p.match_token("Admin"), PosTag::Maint);
    assert_eq!(p.match_token("admins"), PosTag::Maint);
    assert_eq!(p.match_token("Co-maintainer"), PosTag::Maint);
}

#[test]
fn test_contributors_commit() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Contributors"), PosTag::Contributors);
    assert_eq!(p.match_token("contributors"), PosTag::Contributors);
    assert_eq!(p.match_token("Committers"), PosTag::Commit);
    assert_eq!(p.match_token("committers"), PosTag::Commit);
}

#[test]
fn test_company_suffixes() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Inc."), PosTag::Comp);
    assert_eq!(p.match_token("Inc"), PosTag::Comp);
    assert_eq!(p.match_token("Incorporated"), PosTag::Comp);
    assert_eq!(p.match_token("Corp."), PosTag::Comp);
    assert_eq!(p.match_token("Corporation"), PosTag::Comp);
    assert_eq!(p.match_token("Ltd."), PosTag::Comp);
    assert_eq!(p.match_token("Ltd"), PosTag::Comp);
    assert_eq!(p.match_token("Limited"), PosTag::Comp);
    assert_eq!(p.match_token("GmbH"), PosTag::Comp);
    assert_eq!(p.match_token("LLC"), PosTag::Comp);
    assert_eq!(p.match_token("LLP"), PosTag::Comp);
}

#[test]
fn test_company_suffixes_caps() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("INC"), PosTag::Comp);
    assert_eq!(p.match_token("INC."), PosTag::Comp);
    assert_eq!(p.match_token("CORP"), PosTag::Comp);
    assert_eq!(p.match_token("CORPORATION"), PosTag::Comp);
    assert_eq!(p.match_token("FOUNDATION"), PosTag::Comp);
    assert_eq!(p.match_token("GROUP"), PosTag::Comp);
    assert_eq!(p.match_token("COMPANY"), PosTag::Comp);
    assert_eq!(p.match_token("INCORPORATED"), PosTag::Comp);
}

#[test]
fn test_company_suffixes_international() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("GmbH"), PosTag::Comp);
    assert_eq!(p.match_token("e.V."), PosTag::Comp);
    assert_eq!(p.match_token("SARL"), PosTag::Comp);
    assert_eq!(p.match_token("S.A.R.L."), PosTag::Comp);
    assert_eq!(p.match_token("SA"), PosTag::Comp);
    assert_eq!(p.match_token("S.p.A."), PosTag::Comp);
    assert_eq!(p.match_token("s.r.o."), PosTag::Comp);
    assert_eq!(p.match_token("AB"), PosTag::Comp);
}

#[test]
fn test_company_org_types() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("Foundation"), PosTag::Comp);
    assert_eq!(p.match_token("Alliance"), PosTag::Comp);
    assert_eq!(p.match_token("Consortium"), PosTag::Comp);
    assert_eq!(p.match_token("Group"), PosTag::Comp);
    assert_eq!(p.match_token("Technology"), PosTag::Comp);
    assert_eq!(p.match_token("Technologies"), PosTag::Comp);
    assert_eq!(p.match_token("Community"), PosTag::Comp);
    assert_eq!(p.match_token("Project"), PosTag::Comp);
    assert_eq!(p.match_token("Team"), PosTag::Comp);
}

#[test]
fn test_university() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("University"), PosTag::Uni);
    assert_eq!(p.match_token("UNIVERSITY"), PosTag::Uni);
    assert_eq!(p.match_token("Univ."), PosTag::Uni);
    assert_eq!(p.match_token("College"), PosTag::Uni);
    assert_eq!(p.match_token("Academy"), PosTag::Uni);
}

#[test]
fn test_nnp_proper_nouns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("John"), PosTag::Nnp);
    assert_eq!(p.match_token("Smith"), PosTag::Nnp);
    assert_eq!(p.match_token("Cambridge"), PosTag::Nnp);
    assert_eq!(p.match_token("Germany"), PosTag::Nnp);
    assert_eq!(p.match_token("Jean-Claude"), PosTag::Nnp);
    assert_eq!(p.match_token("J."), PosTag::Nnp);
    assert_eq!(p.match_token("U.S.A."), PosTag::Nnp);
    assert_eq!(p.match_token("D'Arcy"), PosTag::Nnp);
}

#[test]
fn test_nnp_unicode_names() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("François"), PosTag::Nnp);
    assert_eq!(p.match_token("Müller"), PosTag::Nnp);
    assert_eq!(p.match_token("José"), PosTag::Nnp);
    assert_eq!(p.match_token("García"), PosTag::Nnp);
    assert_eq!(p.match_token("Björn"), PosTag::Nnp);
    assert_eq!(p.match_token("Ångström"), PosTag::Nnp);
    assert_eq!(p.match_token("Łukasz"), PosTag::Nnp);
    assert_eq!(p.match_token("Żółw"), PosTag::Nnp);
}

#[test]
fn test_nnp_lowercased_names() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("suzuki"), PosTag::Nnp);
    assert_eq!(p.match_token("sean"), PosTag::Nnp);
    assert_eq!(p.match_token("chris"), PosTag::Nnp);
    assert_eq!(p.match_token("daniel"), PosTag::Nnp);
    assert_eq!(p.match_token("karsten"), PosTag::Nnp);
    assert_eq!(p.match_token("wiese"), PosTag::Nnp);
}

#[test]
fn test_caps_patterns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("IBM"), PosTag::Caps);
    assert_eq!(p.match_token("HP"), PosTag::Caps);
    assert_eq!(p.match_token("ACME"), PosTag::Caps);
    assert_eq!(p.match_token("GNU"), PosTag::Caps);
    assert_eq!(p.match_token("(GNU)"), PosTag::Caps);
    assert_eq!(p.match_token("AT&T"), PosTag::Comp);
}

#[test]
fn test_junk_patterns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("license"), PosTag::Junk);
    assert_eq!(p.match_token("License"), PosTag::Junk);
    assert_eq!(p.match_token("under"), PosTag::Junk);
    assert_eq!(p.match_token("Permission"), PosTag::Junk);
    assert_eq!(p.match_token("Disclaimer"), PosTag::Junk);
    assert_eq!(p.match_token("Windows"), PosTag::Junk);
    assert_eq!(p.match_token("template"), PosTag::Junk);
    assert_eq!(p.match_token("struct"), PosTag::Junk);
}

#[test]
fn test_nn_exceptions() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("The"), PosTag::Nn);
    assert_eq!(p.match_token("Free"), PosTag::Nn);
    // "Software" is title-case and matches NNP before any NN pattern
    assert_eq!(p.match_token("software"), PosTag::Nn);
    assert_eq!(p.match_token("SOFTWARE"), PosTag::Nn);
    assert_eq!(p.match_token("Public"), PosTag::Nn);
    assert_eq!(p.match_token("Open"), PosTag::Nn);
    assert_eq!(p.match_token("Java"), PosTag::Nn);
    assert_eq!(p.match_token("BSD"), PosTag::Nn);
}

#[test]
fn test_email_patterns() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("user@example.com"), PosTag::Email);
    assert_eq!(p.match_token("<user@example.com>"), PosTag::Email);
    assert_eq!(p.match_token("mailto:user@example.com"), PosTag::Email);
}

#[test]
fn test_url_patterns() {
    let p = &*COMPILED_PATTERNS;
    // example.com is explicitly JUNK; use real domains for Url2 tests
    assert_eq!(p.match_token("example.com"), PosTag::Junk);
    assert_eq!(p.match_token("ibm.com"), PosTag::Url2);
    assert_eq!(p.match_token("apache.org"), PosTag::Url2);
    assert_eq!(p.match_token("(http://example.com)"), PosTag::Url);
    assert_eq!(p.match_token("<http://example.com>"), PosTag::Url);
    assert_eq!(p.match_token("https://example.org/"), PosTag::Url);
}

#[test]
fn test_puc_rio_tagged_as_name() {
    let p = &*COMPILED_PATTERNS;
    assert_ne!(p.match_token("PUC-Rio"), PosTag::Junk);
}

#[test]
fn test_mixed_cap() {
    let p = &*COMPILED_PATTERNS;
    // MixedCap pattern: ^[A-Z][a-z]+[A-Z][a-z]+[\.\,]?$
    // NNP pattern ^([A-Z][a-z0-9]+){1,2}\.?,?$ also matches these
    // In practice, NNP fires first for bare mixed-cap words
    assert_eq!(p.match_token("LeGrande"), PosTag::Nnp);
    assert_eq!(p.match_token("DeVries"), PosTag::Nnp);
}

#[test]
fn test_pn_dotted_names() {
    let p = &*COMPILED_PATTERNS;
    assert_eq!(p.match_token("P."), PosTag::Nnp);
    assert_eq!(p.match_token("DMTF."), PosTag::Pn);
    assert_eq!(p.match_token("A.B."), PosTag::Nnp);
}
