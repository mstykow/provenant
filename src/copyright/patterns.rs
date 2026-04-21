// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! POS tag regex patterns for copyright token classification.
//!
//! Contains ~1100 ordered regex patterns that map tokens to POS tags.
//! Patterns are tried sequentially — first match wins.
//! All patterns are compiled once at startup via LazyLock.

use std::sync::LazyLock;

use regex::Regex;

use super::types::PosTag;

/// A compiled pattern entry: regex + the POS tag it maps to.
struct PatternEntry {
    regex: Regex,
    tag: PosTag,
}

/// All compiled patterns, in order. First match wins.
pub(super) struct CompiledPatterns {
    patterns: Vec<PatternEntry>,
}

impl CompiledPatterns {
    /// Match a token value against all patterns, returning the first matching tag.
    /// Returns `PosTag::Nn` if no pattern matches (catch-all).
    pub(super) fn match_token(&self, value: &str) -> PosTag {
        for entry in &self.patterns {
            if entry.regex.is_match(value) {
                return entry.tag;
            }
        }
        PosTag::Nn
    }
}

/// Global compiled patterns, initialized once.
pub(super) static COMPILED_PATTERNS: LazyLock<CompiledPatterns> = LazyLock::new(|| {
    let raw_patterns = build_pattern_list();
    let patterns = raw_patterns
        .into_iter()
        .map(|(regex_str, tag)| PatternEntry {
            regex: Regex::new(&regex_str)
                .unwrap_or_else(|e| panic!("Failed to compile regex '{}': {}", regex_str, e)),
            tag,
        })
        .collect();
    CompiledPatterns { patterns }
});

/// Build the ordered list of (regex_string, PosTag) pairs.
/// This is split from compilation to make it easier to test pattern strings.
fn build_pattern_list() -> Vec<(String, PosTag)> {
    // Year sub-patterns (reused by multiple patterns)
    // Bug fix: Python uses 20[0-3][0-9] (2000-2039), we extend to 20[0-9][0-9] (2000-2099)
    let year = r"(19[6-9][0-9]|20[0-9][0-9]|20[0-9][xX])";

    // Bug fix: Python has [0-][0-9] which is a typo — should be [0-2][0-9]
    let year_short = r"([6-9][0-9]|[0-2][0-9])";

    // Bug fix: Python has a suspicious underscore `_` in the separator pattern
    // `(19[6-9][0-9][\\.,\\-]_)+` — we remove it
    // Also extended year range from 20[0-3][0-9] to 20[0-9][0-9]
    let year_year = &format!(
        r"(({yr}[\.,\-])+[6-9][0-9]|({yr}[\.,\-])+[0-9]|({yr}[\.,\-])+[0-2][0-9]|({yr}[\.,\-])+{yr}|({yr}[\.,\-])+{yr}x|({yr}[\.,\-])+{yr}a)",
        yr = year
    );

    // Python's _PUNCT: all ASCII punctuation chars + 'i' oddity + escaped &nbsp
    let punct = r##"([!"#$%&'()*+,\-./:;<=>?@\[\\\]^_`{|}~i]|\\&nbsp)*"##;

    let year_punct = &format!("{}{}", year, punct);
    let year_year_punct = &format!("{}{}", year_year, punct);
    let year_short_punct = &format!("{}{}", year_short, punct);
    let year_or_year_year = &format!("({}|{})", year_punct, year_year_punct);
    let year_then_short = &format!("({}({})*)", year_or_year_year, year_short_punct);
    let year_dash_present = &format!(r"{}[\-~]? ?[Pp]resent\.?,?", year);

    let mut patterns: Vec<(String, PosTag)> = Vec::with_capacity(1200);

    // Helper closure for adding patterns
    let mut add = |regex: &str, tag: PosTag| {
        patterns.push((regex.to_string(), tag));
    };

    ////////////////////////////////////////////////////////////////////////////
    // COPYRIGHT PATTERNS (Python lines 643-730)
    ////////////////////////////////////////////////////////////////////////////

    // Exceptions first — these must come before the copyright matchers

    // NOT a copyright Copyright.txt : treat as NN
    add(r"^Copyright\.txt$", PosTag::Nn);

    // when lowercase with trailing period, not a Copyright statement
    add(r"^copyright\.\)?$", PosTag::Nn);

    // NOT a copyright symbol (ie. "copyrighted."): treat as NN
    // Note: Python has duplicates at lines 657-660 — we deduplicate
    add(r"^[Cc]opyrighted[\.,\)]$", PosTag::Nn);
    add(r"^[Cc]opyrights[\.,\)]$", PosTag::Nn);
    add(r"^COPYRIGHTS[\.,\)]$", PosTag::Nn);
    add(r"^COPYRIGHTED[\.,\)]$", PosTag::Nn);

    // copyright word or symbol
    add(r"^[\(\.@_\-\#\):]*[Cc]opyrights?:?$", PosTag::Copy);
    add(r"^[\(\.@_]*COPYRIGHT[sS]?:?$", PosTag::Copy);
    add(r"^[\(\.@]*[Cc]opyrighted?:?$", PosTag::Copy);
    add(r"^[\(\.@]*COPYRIGHTED?:?$", PosTag::Copy);
    add(r"^[\(\.@]*CopyRights?:?$", PosTag::Copy);

    // rare typos in copyright
    add(r"^Copyrighy$", PosTag::Copy);
    add(r"^Copyirght$", PosTag::Copy);

    // OSGI
    add(r"^Bundle-Copyright", PosTag::Copy);

    // (c)opyright and (c)opyleft, case insensitive
    add(r"(?i)^\(c\)opy(rights?|righted|left)$", PosTag::Copy);

    // truncated opyright and opyleft, case insensitive
    add(
        r"(?i)^opy(rights?|righted|left|lefted)[\.,]?$",
        PosTag::Copy,
    );
    add(r"^//opylefted$", PosTag::Copy);
    add(r"^c'opylefted$", PosTag::Copy);
    // typo in cppyright
    add(r"^[Cc]ppyright[\.,]?$", PosTag::Copy);

    // with a trailing comma
    add(r"^Copyright,$", PosTag::Copy);

    // with a trailing quote and HTML bracket
    add(r"^[Cc]opyright'>$", PosTag::Copy);

    // as javadoc
    add(r"^@[Cc]opyrights?:?$", PosTag::Copy);

    // (C) and (c)
    add(r"^\(C\),?$", PosTag::Copy);
    add(r"^\(c\),?$", PosTag::Copy);

    // Copr.
    add(r"^COPR\.?$", PosTag::Copy);
    add(r"^copr\.?$", PosTag::Copy);
    add(r"^Copr\.?$", PosTag::Copy);

    // copyright in markup, until we strip markup: apache'>Copyright
    add(r"[A-Za-z0-9]+['\x22>]+[Cc]opyright", PosTag::Copy);

    // A copyright line in some manifest, meta or structured files such as Windows PE
    add(r"^AssemblyCopyright.?$", PosTag::Copy);
    add(r"^AppCopyright?$", PosTag::Copy);

    // seen in binaries
    add(r"^[A-Z]Copyright?$", PosTag::Copy);

    // SPDX-FileCopyrightText / SPDX-SnippetCopyrightText
    // Python uses per-char case classes; we use (?i) for simplicity
    add(
        r"^[Ss][Pp][Dd][Xx]-(?:[Ff]ile|[Ss]nippet)[Cc]opyright[Tt]ext",
        PosTag::Copy,
    );

    // SPDX-FileContributor as defined in SPDX and seen used in KDE
    add(
        r"^[Ss][Pp][Dd][Xx]-[Ff]ile[Cc]ontributor",
        PosTag::SpdxContrib,
    );

    ////////////////////////////////////////////////////////////////////////////
    // ALL RIGHTS RESERVED (Python lines 732-788)
    ////////////////////////////////////////////////////////////////////////////

    // All|Some|No Rights Reserved — should be a terminator/delimiter
    add(r"^All$", PosTag::Nn);
    add(r"^all$", PosTag::Nn);
    add(r"^ALL$", PosTag::Nn);
    add(r"^NO$", PosTag::Nn);
    add(r"^Some$", PosTag::Nn);

    add(r"^[Rr]ights?$", PosTag::Right);
    add(r"^RIGHTS?$", PosTag::Right);
    add(r"^[Rr]eserved[\.,]*$", PosTag::Reserved);
    add(r"^RESERVED[\.,]*$", PosTag::Reserved);
    // "reversed" seen in some pranky copyleft notices
    add(r"^[Rr]eversed[\.,]*$", PosTag::Reserved);
    add(r"^REVERSED[\.,]*$", PosTag::Reserved);

    // German: Alle Rechte vorbehalten
    add(r"^[Aa]lle$", PosTag::Nn);
    add(r"^[Rr]echte$", PosTag::Right);
    add(r"^[Vv]orbehalten[\.,]*$", PosTag::Reserved);

    // French: Tous droits réservés
    add(r"^[Tt]ous$", PosTag::Nn);
    // Bug fix: Python has [Dr]roits? which matches "Droits"/"rroits" but not "droits"
    add(r"^[Dd]roits?$", PosTag::Right);
    add(r"^[Rr]éservés[\.,]*$", PosTag::Reserved);
    add(r"^[Rr]eserves[\.,]*$", PosTag::Reserved);

    // Spanish: Reservados todos los derechos
    add(r"^[Rr]eservados[\.,]*$", PosTag::Reserved);
    add(r"^[Tt]odos$", PosTag::Nn);
    add(r"^[Ll]os$", PosTag::Nn);
    // Bug fix: Python has [Dr]erechos which matches "Derechos"/"rerechos" but not "derechos"
    add(r"^[Dd]erechos$", PosTag::Right);

    // Dutch: Alle rechten voorbehouden
    add(r"^[Rr]echten$", PosTag::Right);
    add(r"^[Vv]oorbehouden[\.,]*$", PosTag::Reserved);

    // IS / HELD — used to detect "copyright is held by..."
    add(r"^is$", PosTag::Is);
    add(r"^are$", PosTag::Is);
    add(r"^held$", PosTag::Held);

    // NOTICE
    add(r"^NOTICE$", PosTag::Notice);
    add(r"^NOTICES?[\.,]$", PosTag::Junk);
    add(r"^[Nn]otice$", PosTag::Notice);
    add(r"^[Nn]otices?[\.,]$", PosTag::Junk);
    add(r"^[Nn]otices?$", PosTag::Junk);

    ////////////////////////////////////////////////////////////////////////////
    // CONJUNCTIONS AND RELATED (Python lines 2067-2136)
    ////////////////////////////////////////////////////////////////////////////

    // OF
    add(r"^OF$", PosTag::Of);
    add(r"^of$", PosTag::Of);
    add(r"^Of$", PosTag::Of);

    // DE/de/di: OF (note: conflicts with VAN, but Python has them in this order)
    add(r"^De$", PosTag::Of);
    add(r"^DE$", PosTag::Of);
    add(r"^Di$", PosTag::Of);
    add(r"^di$", PosTag::Of);

    // IN
    add(r"^in$", PosTag::In);
    add(r"^en$", PosTag::In);

    // BY
    add(r"^by$", PosTag::By);
    add(r"^BY$", PosTag::By);
    add(r"^By$", PosTag::By);

    // CC: conjunction "and"
    add(r"^and$", PosTag::Cc);
    add(r"^And$", PosTag::Cc);
    add(r"^AND$", PosTag::Cc);
    add(r"^and/or$", PosTag::Cc);
    add(r"^&$", PosTag::Cc);
    add(r"^at$", PosTag::Cc);
    add(r"^et$", PosTag::Cc);
    add(r"^Et$", PosTag::Cc);
    add(r"^ET$", PosTag::Cc);
    add(r"^Und$", PosTag::Cc);
    add(r"^und$", PosTag::Cc);

    // solo comma as a conjunction
    add(r"^,$", PosTag::Cc);

    // "others" or "et al."
    add(r"^[Oo]ther?s[\.,]?$", PosTag::Oth);
    add(r"^et\. ?al[\.,]?$", PosTag::Oth);

    // DASH: in year ranges "1990-1995", "1990/1995"
    add(r"^-$", PosTag::Dash);
    add(r"^--$", PosTag::Dash);
    add(r"^/$", PosTag::Dash);

    // TO: "1990 to 1995"
    add(r"^to$", PosTag::To);

    // Portions or parts copyright
    add(r"[Pp]ortions?|[Pp]arts?$", PosTag::Portions);

    // VAN: Dutch/German/Spanish/French name particles
    add(r"^(([Vv][ao]n)|[Dd][aeu])$", PosTag::Van);
    add(r"^aan$", PosTag::Of);
    add(r"^van$", PosTag::Van);
    add(r"^Van$", PosTag::Van);
    add(r"^von$", PosTag::Van);
    add(r"^Von$", PosTag::Van);
    add(r"^Da$", PosTag::Van);
    add(r"^da$", PosTag::Van);
    // Note: De/de also appear as OF above — Python has duplicates, first match wins
    add(r"^Du$", PosTag::Van);
    add(r"^du$", PosTag::Van);

    ////////////////////////////////////////////////////////////////////////////
    // YEAR PATTERNS (Python lines 2138-2196)
    ////////////////////////////////////////////////////////////////////////////

    // Rare cases of trailing + sign on years
    // Bug fix: extended from 20[0-3][0-9] to 20[0-9][0-9]
    add(r"^20[0-9][0-9]\+$", PosTag::YrPlus);

    // Year or year ranges:
    // - plain year with various leading and trailing punct
    // - dual or multi years 1994/1995. or 1994-1995
    // - 1987,88,89,90,91,92,93,94,95,96,98,99,2000,2001,2002,2003,2004,2006
    // - dual years with second part abbreviated: 1994/95. or 2002-04 or 1991-9
    add(
        &format!(
            r"^{}{}+({}|{})*$",
            punct, year_or_year_year, year_or_year_year, year_then_short
        ),
        PosTag::Yr,
    );

    add(
        &format!(
            r"^{}{}+({}|{}|{})*$",
            punct, year_or_year_year, year_or_year_year, year_then_short, year_short_punct
        ),
        PosTag::Yr,
    );

    add(&format!(r"^({})+$", year_year), PosTag::Yr);

    add(&format!(r"^({})+$", year_dash_present), PosTag::Yr);

    // ISO dates as in 2024-12-09
    add(
        &format!(r"^{}-(0?[1-9]|1[012])-(0?[1-9]|[12][0-9]|3[01])$", year),
        PosTag::Yr,
    );

    // 88, 93, 94, 95, 96: pattern mostly used in FSF copyrights
    add(r"^[8-9][0-9],$", PosTag::Yr);

    // 80 to 99: pattern mostly used in FSF copyrights
    add(r"^[8-9][0-9]$", PosTag::BareYr);

    // slash dates as in 08/95
    add(r"^(0?[1-9]|1[012])/[6-9][0-9][\.,]?$", PosTag::Yr);

    // weird year
    add(r"today.year", PosTag::Yr);
    add(r"(?i)^current_year$", PosTag::Yr);
    add(r"^\$?LastChangedDate\$?$", PosTag::Yr);

    // Copyright templates in W3C documents
    add(r"^\$?date-of-software$", PosTag::Yr);
    add(r"^\$?date-of-document$", PosTag::Yr);

    // small-cardinal numbers, under 30
    add(r"^[0-3]?[0-9]?[\.,]?$", PosTag::Cds);

    // all other cardinal numbers
    add(r"^-?[0-9]+(.[0-9]+)?[\.,]?$", PosTag::Cd);

    ////////////////////////////////////////////////////////////////////////////
    // FOLLOWING, HOLDER, MONTH, DAY (from Python lines 1115, 1715-1718, 1779)
    // These are included here as they are part of the "special tokens" batch
    ////////////////////////////////////////////////////////////////////////////

    // "following" — used in "the following copyright holders"
    add(r"^following$", PosTag::Following);

    // "holders" is considered special
    add(r"^([Hh]olders?|HOLDERS?)\.?,?$", PosTag::Holder);

    // Month abbreviations (we don't include May, Jan, Jun — common first names)
    add(r"^(Feb|Mar|Apr|Jul|Aug|Sep|Oct|Nov|Dec),?$", PosTag::Month);

    // Day of week
    add(
        r"^([Mm]onday|[Tt]uesday|[Ww]ednesday|[Tt]hursday|[Ff]riday|[Ss]aturday|[Ss]unday),?$",
        PosTag::Day,
    );

    // MIT is problematic — special handling
    add(r"^MIT,$", PosTag::Caps);
    add(r"^MIT\.?$", PosTag::Mit);

    // Linux
    add(r"^Linux$", PosTag::Linux);

    // single parens are special
    add(r"^[\(\)]$", PosTag::Parens);

    // AT/DOT in obfuscated emails like "joe AT foo DOT com"
    add(r"^AT$", PosTag::At);
    add(r"^DOT$", PosTag::Dot);
    add(r"^dot$", PosTag::Dot);

    // OU as in Org unit, found in some certificates
    add(r"^OU$", PosTag::Ou);

    ////////////////////////////////////////////////////////////////////////////
    // JUNK EXCEPTIONS (Python lines 790-835)
    // These must come BEFORE the JUNK proper section
    ////////////////////////////////////////////////////////////////////////////

    add(r"^Special$", PosTag::Nn);
    add(r"^Member\(s\)[\.,]?$", PosTag::Nnp);
    add(r"^__authors?__$", PosTag::Auths);
    add(r"^__contributors?__$", PosTag::Auths);
    add(r"^Author\(s\)[\.,:]?$", PosTag::Auths);
    add(r"^[A-a]ffiliate\(s\)[\.,:]?$", PosTag::Comp);
    // Exceptions to short mixed caps with trailing cap
    add(r"ApS$", PosTag::Comp);
    // short two chars as B3
    add(r"^[A-Z][0-9]$", PosTag::Nn);

    // 2-letters short words, skipping some leading caps
    add(r"^[BEFHJMNPQRTUVW][a-z]$", PosTag::Nn);

    // Misc exceptions
    add(r"^dead_horse$", PosTag::Nn);
    add(r"^A11yance", PosTag::Nnp);
    add(r"^Fu$", PosTag::Nnp);
    add(r"^W3C\(r\)$", PosTag::Comp);
    add(r"^TeX$", PosTag::Nnp);

    // Three or more AsCamelCase with some exceptions
    add(
        r"^(?:OpenStreetMap|AliasDotCom|AllThingsTalk).?$",
        PosTag::Nnp,
    );

    add(r"^Re-Creating$", PosTag::Junk);
    add(r"^[Nn]o$", PosTag::Junk);
    add(r"^Earth$", PosTag::Nn);
    add(r"^Maps/Google$", PosTag::Nn);

    // verbatim star
    add(r"^\*$", PosTag::Junk);

    // misc company names exception to next rule
    add(r"^TinCanTools$", PosTag::Nnp);
    add(r"^SoftwareBitMaker$", PosTag::Nnp);
    add(r"^NetCommWireless$", PosTag::Nnp);

    // Repeated CamelCasedWords
    add(r"^([A-Z][a-z]+){3,}$", PosTag::Junk);

    ////////////////////////////////////////////////////////////////////////////
    // JUNK PROPER (Python lines 836-1303)
    ////////////////////////////////////////////////////////////////////////////

    // all lower case with dashes "enforce-trailing-newline" at least 3 times
    add(r"^((\w+-){3,}\w+)$", PosTag::Junk);

    // path with trailing year-like are NOT a year
    // Landroid/icu/impl/IDNA2003 : treat as JUNK
    add(r"^[^\\/]+[\\/][^\\/]+[\\/].*$", PosTag::Junk);

    // CamELCaseeXXX is typically JUNK such as code variable names
    add(r"^([A-Z][a-z]+){3,20}[A-Z]+[0-9]*,?$", PosTag::Junk);

    // multiple parens (at least two (x) groups) is a sign of junk
    add(r"^.*\(.*\).*\(.*\).*$", PosTag::Junk);

    // parens such as (1) or (a) is a sign of junk but NOT (c)
    add(r"^\(([abdefghi\d]|ii|iii)\)$", PosTag::Junk);

    // @link in javadoc is not a NN
    add(r"^@?link:?$", PosTag::Junk);
    add(r"@license:?$", PosTag::Junk);
    add(r"(?i)^@remark:?$", PosTag::Junk);

    // hex is JUNK 0x3fc3/0x7cff
    add(r"^0x[a-fA-F0-9]+", PosTag::Junk);

    // found in crypto certificates and LDAP
    add(r"^O=$", PosTag::Junk);
    add(r"^OU=?$", PosTag::Junk);
    add(r"^XML$", PosTag::Junk);
    add(r"^Parser$", PosTag::Junk);
    add(r"^Dual$", PosTag::Junk);
    add(r"^Crypto$", PosTag::Junk);
    add(r"^PART$", PosTag::Junk);
    add(r"^[Oo]riginally?$", PosTag::Junk);
    add(r"^[Rr]epresentations?\.?$", PosTag::Junk);
    add(r"^works,$", PosTag::Junk);
    add(r"^grant$", PosTag::Junk);
    add(r"^Refer$", PosTag::Junk);
    add(r"^Apt$", PosTag::Junk);
    add(r"^Agreement$", PosTag::Junk);
    add(r"^Usage$", PosTag::Junk);
    add(r"^Please$", PosTag::Junk);
    add(r"^\(?Based$", PosTag::Junk);
    add(r"^Upstream$", PosTag::Junk);
    add(r"^Files?$", PosTag::Junk);
    add(r"^Filename:?$", PosTag::Junk);
    add(r"^Description:?$", PosTag::Junk);
    add(r"^[Pp]rocedures?$", PosTag::Junk);
    add(r"^You$", PosTag::Junk);
    add(r"^Everyone$", PosTag::Junk);
    add(r"^[Ff]unded$", PosTag::Junk);
    add(r"^Unless$", PosTag::Junk);
    add(r"^rant$", PosTag::Junk);
    add(r"^Subject$", PosTag::Junk);
    add(r"^Acknowledgements?$", PosTag::Junk);
    add(r"^Derivative$", PosTag::Junk);
    add(r"^[Ll]icensable$", PosTag::Junk);
    add(r"^[Ss]ince$", PosTag::Junk);
    add(r"^[Ll]icen[cs]e[\.d]?$", PosTag::Junk);
    add(r"^[Ll]icen[cs]ors?$", PosTag::Junk);
    add(r"^under$", PosTag::Junk);
    add(r"^TCK$", PosTag::Junk);
    add(r"^Use$", PosTag::Junk);
    add(r"^[Rr]estrictions?$", PosTag::Junk);
    add(r"^[Ii]ntrodu`?ction$", PosTag::Junk);
    add(r"^[Ii]ncludes?$", PosTag::Junk);
    add(r"^[Vv]oluntary$", PosTag::Junk);
    add(r"^[Cc]ontributions?$", PosTag::Junk);
    add(r"^[Mm]odifications?$", PosTag::Junk);
    add(r"^Company:$", PosTag::Junk);
    add(r"^For$", PosTag::Junk);
    add(r"^File$", PosTag::Junk);
    add(r"^Last$", PosTag::Junk);
    add(r"^[Rr]eleased?$", PosTag::Junk);
    add(r"^[Cc]opyrighting$", PosTag::Junk);
    add(r"^[Aa]uthori.*$", PosTag::Junk);
    add(r"^such$", PosTag::Junk);
    add(r"^[Aa]ssignments?[\.,]?$", PosTag::Junk);
    add(r"^[Bb]uild$", PosTag::Junk);
    add(r"^[Ss]tring$", PosTag::Junk);
    add(r"^Implementation-Vendor$", PosTag::Junk);
    add(r"^dnl$", PosTag::Junk);
    add(r"^ifndef$", PosTag::Junk);

    add(r"^as$", PosTag::Nn);
    add(r"^[Vv]isit$", PosTag::Junk);

    add(r"^rem$", PosTag::Junk);
    add(r"^REM$", PosTag::Junk);
    add(r"^Supports$", PosTag::Junk);
    add(r"^Separator$", PosTag::Junk);
    add(r"^\.byte$", PosTag::Junk);
    add(r"^Idata$", PosTag::Junk);
    add(r"^[Cc]ontributed?$", PosTag::Junk);
    add(r"^[Ff]unctions?$", PosTag::Junk);
    add(r"^[Mm]ust$", PosTag::Junk);
    add(r"^ISUPPER?$", PosTag::Junk);
    add(r"^ISLOWER$", PosTag::Junk);
    add(r"^AppPublisher$", PosTag::Junk);

    add(r"^DISCLAIMS?$", PosTag::Junk);
    add(r"^SPECIFICALLY$", PosTag::Junk);

    add(r"^identifying", PosTag::Junk);
    add(r"^IDENTIFICATION$", PosTag::Junk);
    add(r"^WARRANTIE?S?$", PosTag::Junk);
    add(r"^WARRANTS?$", PosTag::Junk);
    add(r"^WARRANTYS?$", PosTag::Junk);

    add(r"^Row\(", PosTag::Junk);

    add(r"^hispagestyle$", PosTag::Junk);
    add(r"^Generic$", PosTag::Junk);
    add(r"^generate-", PosTag::Junk);
    add(r"^Change$", PosTag::Junk);
    add(r"^Add$", PosTag::Junk);
    add(r"^Average$", PosTag::Junk);
    add(r"^Taken$", PosTag::Junk);
    add(r"^design$", PosTag::Junk);
    add(r"^Driver$", PosTag::Junk);
    add(r"^[Cc]ontribution\.?", PosTag::Junk);

    add(r"DeclareUnicodeCharacter$", PosTag::Junk);
    add(r"^Language-Team$", PosTag::Junk);
    add(r"^Last-Translator$", PosTag::Junk);
    add(r"^Translated$", PosTag::Junk);
    add(r"^OMAP730$", PosTag::Junk);

    add(r"^dylid$", PosTag::Junk);
    add(r"^BeOS$", PosTag::Junk);
    add(r"^Generates?$", PosTag::Junk);
    add(r"^Thanks?$", PosTag::Junk);
    add(r"^therein$", PosTag::Junk);
    add(r"^their$", PosTag::Junk);

    // various programming constructs
    add(r"^var$", PosTag::Junk);
    add(r"^[Tt]his$", PosTag::Junk);
    add(r"^thats?$", PosTag::Junk);
    add(r"^xmlns$", PosTag::Junk);
    add(r"^[Aa]sync$", PosTag::Junk);
    add(r"^Keyspan$", PosTag::Junk);
    add(r"^grunt.template", PosTag::Junk);
    add(r"^else", PosTag::Junk);
    add(r"^constructor.$", PosTag::Junk);
    add(
        r"^(if|elsif|elif|self|catch|this|else|switch|type|typeof|case|pos|break|[Nn]one|null|var|return|def|function|method|var).?$",
        PosTag::Junk,
    );
    add(
        r"^.?(null|function|try|catch|except|throw|typeof|catch|switch).?$",
        PosTag::Junk,
    );
    add(
        r"^.*[\.:](?:value|ref|key|case|type|typeof|props|state|error|null)$",
        PosTag::Junk,
    );
    // Note: Python has r'^[a-z]{,5}\[!?]+' which is invalid regex in Rust
    // We interpret it as: short lowercase word followed by brackets
    add(r"^[a-z]{0,5}\[!?]+", PosTag::Junk);

    // func call with short var in minified code
    add(r"^\w{2,6}\([a-z, ]{1,6}\)", PosTag::Junk);

    // neither and nor conjunctions are NOT part of a copyright statement
    add(r"^neither$", PosTag::Junk);
    add(r"^nor$", PosTag::Junk);

    add(r"^data-.*$", PosTag::Junk);

    add(r"^providing$", PosTag::Junk);
    add(r"^Execute$", PosTag::Junk);
    add(r"^passes$", PosTag::Junk);
    add(r"^Should$", PosTag::Junk);
    add(r"^[Ll]icensing\@?$", PosTag::Junk);
    add(r"^Disclaimer$", PosTag::Junk);
    add(r"^Directive.?$", PosTag::Junk);
    add(r"^LAWS\,?$", PosTag::Junk);
    add(r"^me$", PosTag::Junk);
    add(r"^Derived$", PosTag::Junk);
    add(r"^Limitations?$", PosTag::Junk);
    add(r"^Nothing$", PosTag::Junk);
    add(r"^Policy$", PosTag::Junk);
    add(r"^available$", PosTag::Junk);
    add(r"^Recipient\.?$", PosTag::Junk);
    add(r"^LICEN[CS]EES?\.?$", PosTag::Junk);
    add(r"^[Ll]icen[cs]ees?,?$", PosTag::Junk);
    add(r"^Application$", PosTag::Junk);
    add(r"^Receiving$", PosTag::Junk);
    add(r"^Party$", PosTag::Junk);
    add(r"^interfaces$", PosTag::Junk);
    add(r"^owner$", PosTag::Junk);
    add(r"^Sui$", PosTag::Junk);
    add(r"^Generis$", PosTag::Junk);
    add(r"^Conditioned$", PosTag::Junk);
    // Note: Python has duplicate Disclaimer — we include once
    add(r"^Warranty$", PosTag::Junk);
    add(r"^Configure$", PosTag::Junk);
    add(r"^Excluded$", PosTag::Junk);
    add(r"^Represents$", PosTag::Junk);
    add(r"^Sufficient$", PosTag::Junk);
    add(r"^Each$", PosTag::Junk);
    add(r"^Partially$", PosTag::Junk);
    add(r"^Limitation$", PosTag::Junk);
    add(r"^Liability$", PosTag::Junk);
    add(r"^Named$", PosTag::Junk);
    add(r"^defaults?$", PosTag::Junk);
    add(r"^Use.$", PosTag::Junk);
    add(r"^EXCEPT$", PosTag::Junk);
    add(r"^OWNER\.?$", PosTag::Junk);
    add(r"^Comments\.?$", PosTag::Junk);
    add(r"^you$", PosTag::Junk);
    add(r"^means$", PosTag::Junk);
    add(r"^information$", PosTag::Junk);
    add(r"^[Aa]lternatively.?$", PosTag::Junk);
    add(r"^[Aa]lternately.?$", PosTag::Junk);
    add(r"^INFRINGEMENT.?$", PosTag::Junk);
    add(r"^Install$", PosTag::Junk);
    add(r"^Updates$", PosTag::Junk);
    add(r"^Record-keeping$", PosTag::Junk);
    add(r"^Privacy$", PosTag::Junk);
    add(r"^within$", PosTag::Junk);

    add(r"^official$", PosTag::Junk);
    add(r"^duties$", PosTag::Junk);
    add(r"^civil$", PosTag::Junk);
    add(r"^servants?$", PosTag::Junk);

    // various trailing words that are junk
    add(r"^Copyleft$", PosTag::Junk);
    add(r"^LegalCopyright$", PosTag::Junk);
    add(r"^Report$", PosTag::Junk);
    add(r"^Available$", PosTag::Junk);
    add(r"^true$", PosTag::Junk);
    add(r"^false$", PosTag::Junk);
    add(r"^node$", PosTag::Junk);
    add(r"^jshint$", PosTag::Junk);
    add(r"^node':true$", PosTag::Junk);
    add(r"^node:true$", PosTag::Junk);
    add(r"^this$", PosTag::Junk);
    add(r"^Act,?$", PosTag::Junk);
    add(r"^[Ff]unctionality$", PosTag::Junk);
    add(r"^bgcolor$", PosTag::Junk);
    add(r"^F+$", PosTag::Junk);
    add(r"^Rewrote$", PosTag::Junk);
    add(r"^Much$", PosTag::Junk);
    add(r"^remains?,?$", PosTag::Junk);
    add(r"^earlier$", PosTag::Junk);

    // there is a Mr. Law
    add(r"^Law[\.,]?$", PosTag::Nn);
    add(r"^laws?[\.,]?$", PosTag::Junk);
    add(r"^Laws[\.,]?$", PosTag::Junk);
    add(r"^LAWS?[\.,]?$", PosTag::Junk);
    add(r"^LAWS?$", PosTag::Nn);

    add(r"^taken$", PosTag::Nn);
    add(r"^Insert$", PosTag::Junk);
    add(r"^url$", PosTag::Junk);
    add(r"^file$", PosTag::Junk);
    add(r"^file\.$", PosTag::Junk);
    add(r"^[Ss]ee$", PosTag::Junk);
    add(r"^[Pp]ackage\.?$", PosTag::Junk);
    add(r"^Covered$", PosTag::Junk);
    add(r"^date$", PosTag::Junk);
    add(r"^practices$", PosTag::Junk);
    add(r"^[Aa]ny$", PosTag::Junk);
    add(r"^ANY$", PosTag::Junk);
    add(r"^fprintf.*$", PosTag::Junk);
    add(r"^CURDIR$", PosTag::Junk);
    add(r"^Environment/Libraries$", PosTag::Junk);
    add(r"^Environment/Base$", PosTag::Junk);
    add(r"^Violations\.?$", PosTag::Junk);
    add(r"^Owner$", PosTag::Junk);
    add(r"^behalf$", PosTag::Junk);
    add(r"^know-how$", PosTag::Junk);
    add(r"^[Ii]nterfaces?,?$", PosTag::Junk);
    add(r"^than$", PosTag::Junk);
    add(r"^whom$", PosTag::Junk);
    add(r"^Definitions?$", PosTag::Junk);
    add(r"^However,?$", PosTag::Junk);
    add(r"^[Cc]ollectively$", PosTag::Junk);
    // Note: "following" already added as PosTag::Following above — Python has it here too
    add(r"^[Cc]onfig$", PosTag::Junk);

    // version variables listed after Copyright variable in FFmpeg
    add(r"^ExifVersion$", PosTag::Junk);
    add(r"^FlashpixVersion$", PosTag::Junk);
    add(r"^.+ArmsAndLegs$", PosTag::Junk);

    // junk when HOLDER(S): typically used in disclaimers
    add(r"^HOLDER\(S\)$", PosTag::Junk);

    // some HTML tags
    add(r"^width$", PosTag::Junk);

    // "copyright ownership. The ASF" in Apache license headers
    add(r"^[Oo]wnership\.?$", PosTag::Junk);

    // exceptions to composed proper names, mostly debian copyright/control tag-related
    add(r"^Title:?$", PosTag::Junk);
    add(r"^Debianized-By:?$", PosTag::Junk);
    add(r"^[Dd]ebianized$", PosTag::Junk);
    add(r"^Upstream-Maintainer:?$", PosTag::Junk);
    add(r"^Content", PosTag::Junk);
    add(r"^Upstream-Author:?$", PosTag::Junk);
    add(r"^Packaged-By:?$", PosTag::Junk);

    // Windows XP
    add(r"^Windows$", PosTag::Junk);
    add(r"^XP$", PosTag::Junk);
    add(r"^SP1$", PosTag::Junk);
    add(r"^SP2$", PosTag::Junk);
    add(r"^SP3$", PosTag::Junk);
    add(r"^SP4$", PosTag::Junk);
    add(r"^assembly$", PosTag::Junk);

    // various junk bits
    add(r"^example\.com$", PosTag::Junk);
    add(r"^:Licen[cs]e$", PosTag::Junk);
    add(r"^Agent\.?$", PosTag::Junk);
    // Note: Python has duplicate "behalf" — already added above
    add(r"^[aA]nyone$", PosTag::Junk);

    // when uppercase this is likely part of some SQL statement
    add(r"^FROM$", PosTag::Junk);
    add(r"^CREATE$", PosTag::Junk);
    // Note: Python has duplicate CURDIR — already added above
    // found in sqlite
    add(r"^\+0$", PosTag::Junk);
    add(r"^ToUpper$", PosTag::Junk);
    add(r"^\+$", PosTag::Junk);

    // Java
    add(r"^.*Servlet,?$", PosTag::Junk);
    add(r"^class$", PosTag::Junk);

    // C/C++
    add(r"^template$", PosTag::Junk);
    add(r"^struct$", PosTag::Junk);
    add(r"^typedef$", PosTag::Junk);
    add(r"^type$", PosTag::Junk);
    add(r"^next$", PosTag::Junk);
    add(r"^typename$", PosTag::Junk);
    add(r"^namespace$", PosTag::Junk);
    add(r"^type_of$", PosTag::Junk);
    add(r"^begin$", PosTag::Junk);
    add(r"^end$", PosTag::Junk);

    // mixed programming words
    add(r"^Batch$", PosTag::Junk);
    add(r"^Axes", PosTag::Junk);

    // Some mixed case junk
    add(r"^LastModified$", PosTag::Junk);

    // Some font names
    add(r"^Lucida$", PosTag::Junk);

    // various trailing words that are junk
    add(r"^CVS$", PosTag::Junk);
    add(r"^EN-IE$", PosTag::Junk);
    add(r"^Info$", PosTag::Junk);
    add(r"^GA$", PosTag::Junk);
    add(r"^unzip$", PosTag::Junk);
    add(r"^EULA", PosTag::Junk);
    add(r"^Terms?[\.,]?$", PosTag::Junk);
    add(r"^Non-Assertion$", PosTag::Junk);

    // this is not Copr.
    // Note: Python has $$ (double dollar) which is a typo — we use single $
    add(r"^Coproduct,?[,\.]?$", PosTag::Junk);

    add(r"^CONTRIBUTORS?[,\.]?$", PosTag::Junk);
    add(r"^OTHERS?[,\.]?$", PosTag::Junk);
    add(r"^Contributors?\:[,\.]?$", PosTag::Junk);
    add(r"^\(?Version$", PosTag::Junk);

    // JUNK from binary
    add(r"^x1b|1H$", PosTag::Junk);

    // JUNK as camel case with a single hump such as in "processingInfo"
    add(r"^[a-z]{3,10}[A-Z][a-z]{3,10}$", PosTag::Junk);

    add(r"^\$?Guid$", PosTag::Junk);
    add(r"^implementing$", PosTag::Junk);
    add(r"^Unlike$", PosTag::Junk);
    add(r"^using$", PosTag::Junk);
    add(r"^new$", PosTag::Junk);
    add(r"^param$", PosTag::Junk);
    add(r"^which$", PosTag::Junk);

    // "Es6ToEs3ClassSideInheritance." and related names
    add(r"^[A-Z]([a-zA-Z]*[0-9]){2,}[a-zA-Z]+[\.,]?", PosTag::Junk);

    // owlocationNameEntitieship.
    add(r"^([a-z]{2,}[A-Z]){2,}[a-z]+[\.,]?", PosTag::Junk);

    add(r"^[a-z].+\(s\)[\.,]?$", PosTag::Junk);

    // parens in the middle: for(var
    add(r"^[a-zA-Z]+[\)\(]+,?[\)\(]?[a-zA-Z]+[\.,]?$", PosTag::Junk);

    // single period
    add(r"^\.$", PosTag::Junk);

    // exception to the next rule: by PaX Team
    add(r"PaX$", PosTag::Nn);

    // short mixed caps with trailing cap: ZoY
    add(r"[A-Z][a-z][A-Z]$", PosTag::Junk);

    add(r"^Tokenizers?$", PosTag::Junk);
    add(r"^Analyzers?$", PosTag::Junk);
    add(r"^PostingsFormats?$", PosTag::Junk);
    add(r"^Comment[A-Z]", PosTag::Junk);
    add(r"^fall$", PosTag::Junk);
    add(r"^[Aa]nother$", PosTag::Junk);
    add(r"^[Aa]acute", PosTag::Junk);
    add(r"^[Aa]circumflex", PosTag::Junk);
    add(r"^[Kk]eywords?", PosTag::Junk);
    add(r"^comparing$", PosTag::Junk);
    add(r"^[Ee]mail", PosTag::Junk);

    // First|Last|FamilyName
    add(r"^[A-Z][a-z]+Name", PosTag::Junk);
    add(r"^[Yy]ourself", PosTag::Junk);
    add(r"^parties$", PosTag::Junk);
    add(r"^\(?names?\)?$", PosTag::Junk);
    add(r"^[Bb]oolean$", PosTag::Nn);
    add(r"^private$", PosTag::Junk);
    add(r"^[MmNn]odules?[,\.]?$", PosTag::Junk);
    add(r"^[Rr]eturned$", PosTag::Junk);

    // misc junk
    add(r"^False.?$", PosTag::Junk);
    add(r"^True.?$", PosTag::Junk);

    add(r"^high$", PosTag::Junk);
    add(r"^low$", PosTag::Junk);
    add(r"^on$", PosTag::Junk);

    add(r"^imports?$", PosTag::Junk);
    add(r"^[Ww]arnings?$", PosTag::Junk);
    add(r"^[Ww]hether$", PosTag::Junk);
    add(r"^[Bb]oth$", PosTag::Junk);
    add(r"^[Cc]aller$", PosTag::Junk);

    // tags
    add(r"^E-?[Mm]ail:?$", PosTag::Junk);
    add(r"^URL:?$", PosTag::Junk);
    add(r"^url:?$", PosTag::Junk);

    // method names
    add(r"^[a-zA-Z]+\(\)$", PosTag::Junk);

    // :co,e):f
    add(
        r"^[\:,\)]+[a-z]+[\:,]+[a-z]+[\:,\)]+[a-z\:,\)]*$",
        PosTag::Junk,
    );

    // NN often used in conjunction with copyright
    add(r"^[Ss]tatements?.?$", PosTag::Junk);
    add(r"^issues?.?$", PosTag::Junk);
    add(r"^retain?.?$", PosTag::Junk);
    add(r"^Sun3x$", PosTag::Junk);

    ////////////////////////////////////////////////////////////////////////////
    // NOUNS AND PROPER NOUNS (Python lines 1304-1780)
    ////////////////////////////////////////////////////////////////////////////

    // Various rare bits treated as NAME directly
    add(r"^FSFE?[\.,]?$", PosTag::Nnp);
    add(r"^This_file_is_part_of_KDE$", PosTag::Nnp);

    // K.K. (a company suffix), needs special handling
    add(r"^K.K.,?$", PosTag::Comp);

    // MIT is problematic — already handled above, but Python has additional patterns here
    // MIT alone as NN (catch-all after the MIT/CAPS patterns above)
    add(r"^MIT$", PosTag::Nn);

    // ISC is always a company
    // Note: Python has a bug here — line 1323 says "ISC" but pattern is "MIT"
    // We skip the duplicate MIT pattern

    // NOT A CAPS: [YEAR] W3C® (MIT, ERCIM, Keio, Beihang)
    add(r"^YEAR", PosTag::Yr);

    // Various NN, exceptions to NNP or CAPS
    add(r"^Activation\.?$", PosTag::Nn);
    add(r"^Act[\.,]?$", PosTag::Nn);
    add(r"^Added$", PosTag::Nn);
    add(r"^added$", PosTag::Junk);
    add(r"^As$", PosTag::Nn);
    add(r"^I$", PosTag::Nn);
    add(r"^Additional$", PosTag::Nn);
    add(r"^Are$", PosTag::Nn);
    add(r"^AST$", PosTag::Nn);
    add(r"^AGPL.?$", PosTag::Nn);
    add(r"^Agreements?\.?$", PosTag::Nn);
    add(r"^AIRTM$", PosTag::Nn);
    add(r"^Angular$", PosTag::Nn);
    add(r"^Component[A-Z]", PosTag::Nn);
    add(r"^Function[A-Z]", PosTag::Nn);
    add(r"^Android$", PosTag::Nn);
    add(r"^Any$", PosTag::Nn);
    add(r"^Appropriate$", PosTag::Junk);
    add(r"^Expander$", PosTag::Nn);
    add(r"^Archiver$", PosTag::Nn);
    add(r"^APPROPRIATE", PosTag::Nn);
    add(r"^Asset$", PosTag::Nn);
    add(r"^Assignment?s$", PosTag::Nn);
    add(r"^Atomic$", PosTag::Nn);
    add(r"^Attribution\.?$", PosTag::Nn);
    add(r"^[Aa]uthored$", PosTag::Nn);
    add(r"^Baslerstr\.?$", PosTag::Nn);
    add(r"^Before$", PosTag::Nn);
    add(r"^Message$", PosTag::Nn);
    add(r"^BitLen$", PosTag::Junk);
    add(r"^BSD$", PosTag::Nn);
    add(r"^BUT$", PosTag::Nn);
    add(r"^But$", PosTag::Nn);
    add(r"^Builders?\.?$", PosTag::Nn);
    add(r"^Cacute$", PosTag::Nn);
    add(r"^CD$", PosTag::Junk);
    add(r"^Cell.$", PosTag::Nn);
    add(r"^Change\.?[lL]og$", PosTag::Nn);
    add(r"^CHANGElogger$", PosTag::Nn);
    add(r"^CHANGELOG$", PosTag::Nn);
    add(r"^CHANGES$", PosTag::Nn);
    add(r"^Cap$", PosTag::Nn);
    add(r"^Cases$", PosTag::Nn);
    add(r"^Category$", PosTag::Nn);
    add(r"^Code$", PosTag::Nn);
    add(r"^Collators?$", PosTag::Nn);
    add(r"^Commercial", PosTag::Nn);
    add(r"^Commons?$", PosTag::Nn);
    add(r"^Compilation", PosTag::Nn);
    add(r"^Contact", PosTag::Nn);
    add(r"^Contracts?$", PosTag::Nn);
    add(r"^Convention$", PosTag::Nn);
    add(r"^Copying", PosTag::Nn);
    add(r"^COPYING", PosTag::Nn);
    add(r"^Customer", PosTag::Nn);
    add(r"^Custom$", PosTag::Nn);
    add(r"^Data$", PosTag::Nn);
    add(r"^Date$", PosTag::Nn);
    add(r"^DATED$", PosTag::Nn);
    add(r"^Delay", PosTag::Nn);
    add(r"^Derivative", PosTag::Nn);
    add(r"^Direct$", PosTag::Nn);
    add(r"^DISCLAIMED", PosTag::Nn);
    add(r"^Docs?$", PosTag::Nn);
    add(r"^DOCUMENTATION", PosTag::Nn);
    add(r"^Download", PosTag::Junk);
    add(r"^DOM$", PosTag::Nn);
    add(r"^Do$", PosTag::Nn);
    add(r"^DoubleClick$", PosTag::Nn);
    // Note: Python has duplicate "Each" as NN — already added as Junk above (first match wins)
    add(r"^Education$", PosTag::Nn);
    add(r"^Extended", PosTag::Nn);
    add(r"^Every$", PosTag::Nn);
    add(r"^EXHIBIT$", PosTag::Junk);
    add(r"^Exhibit$", PosTag::Junk);
    add(r"^Digitized", PosTag::Nn);
    add(r"^OPENING", PosTag::Junk);
    add(r"^[Ds]istributed?.?$", PosTag::Nn);
    add(r"^Distributions?", PosTag::Nn);
    add(r"^Multiply$", PosTag::Nn);
    add(r"^Convert$", PosTag::Nn);
    add(r"^Compute$", PosTag::Nn);

    add(r"^\(Computer$", PosTag::Junk);
    add(r"^Programs\)", PosTag::Junk);
    add(r"^Regulations", PosTag::Junk);
    add(r"^message\.", PosTag::Junk);

    add(r"^Case$", PosTag::Nn);
    add(r"^Hessian$", PosTag::Nn);
    add(r"^Include", PosTag::Nn);
    add(r"^Downstream", PosTag::Nn);
    add(r"^Volumes?", PosTag::Nn);
    add(r"^Manuals?.?", PosTag::Nn);
    add(r"^Update.?", PosTag::Nn);
    add(r"^[Ff]ormatting.?", PosTag::Junk);
    add(r"^Lexers?.?", PosTag::Nn);
    add(r"^Symbols?.?", PosTag::Nn);
    add(r"^Tokens?.?", PosTag::Nn);
    add(r"^Initial", PosTag::Nn);
    add(r"^END$", PosTag::Nn);
    add(r"^Entity$", PosTag::Nn);
    add(r"^Example", PosTag::Nn);
    add(r"^Except", PosTag::Nn);
    add(r"^Fragments$", PosTag::Nn);
    add(r"^With$", PosTag::Nn);
    add(r"^Tick$", PosTag::Nn);
    add(r"^Dynamic$", PosTag::Nn);
    add(r"^Battery$", PosTag::Nn);
    add(r"^Charger$", PosTag::Nn);
    // Note: Python has duplicate Dynamic — already added
    add(r"^Bugfixes?$", PosTag::Nn);
    add(r"^Likes?$", PosTag::Nn);
    add(r"^STA$", PosTag::Nn);
    add(r"^Page$", PosTag::Nn);
    add(r"^Todo/Under$", PosTag::Junk);
    add(r"^Under$", PosTag::Nn);

    add(r"^Interrupt$", PosTag::Nn);
    add(r"^cleanups?$", PosTag::Junk);
    add(r"^Tape$", PosTag::Nn);

    add(r"^When$", PosTag::Nn);
    add(r"^Specifications?$", PosTag::Nn);
    add(r"^Final$", PosTag::Nn);
    add(r"^Holds$", PosTag::Nn);
    add(r"^Image", PosTag::Nn);
    add(r"^Supplier", PosTag::Nn);
    add(r"^Experimental$", PosTag::Nn);
    add(r"^F2Wku$", PosTag::Nn);
    add(r"^False$", PosTag::Nn);
    add(r"^Highlight", PosTag::Nn);
    add(r"^Line", PosTag::Nn);
    add(r"^NPM[\.,]?", PosTag::Nn);
    add(r"^Grunt[\.,]?", PosTag::Nn);
    add(r"^Numbers?", PosTag::Nn);
    add(r"^Fibonacci$", PosTag::Junk);
    add(r"^FALSE$", PosTag::Nn);
    add(r"^FAQ$", PosTag::Nn);
    add(r"^Foreign$", PosTag::Nn);
    add(r"^From$", PosTag::Nn);
    add(r"^Full$", PosTag::Nn);
    add(r"^Further", PosTag::Nn);
    add(r"^Gaim$", PosTag::Nn);
    add(r"^Generated", PosTag::Nn);
    add(r"^Glib$", PosTag::Nn);
    add(r"^GPLd?\.?$", PosTag::Nn);
    add(r"^GPL'd$", PosTag::Nn);
    add(r"^Gnome$", PosTag::Nn);
    add(r"^Port$", PosTag::Nn);
    add(r"^GnuPG$", PosTag::Nn);
    add(r"^Government.", PosTag::Nnp);
    add(r"^OProfile$", PosTag::Nnp);
    add(r"^Government$", PosTag::Comp);
    // there is a Ms. Grant
    add(r"^Grant$", PosTag::Nnp);
    add(r"^Grants?\.?,?$", PosTag::Nn);
    add(r"^Header", PosTag::Nn);
    add(r"^HylaFAX$", PosTag::Nn);
    add(r"^IA64$", PosTag::Nn);
    add(r"^IDEA$", PosTag::Nn);
    add(r"^Id$", PosTag::Nn);

    // miscapitalized last name
    add(r"^king$", PosTag::Nnp);

    add(r"^IDENTIFICATION?\.?$", PosTag::Nn);
    add(r"^IEEE$", PosTag::Nn);
    add(r"^If$", PosTag::Nn);
    add(r"^[Ii]ntltool$", PosTag::Nn);
    add(r"^Immediately$", PosTag::Nn);
    add(r"^Implementation", PosTag::Nn);
    add(r"^Improvement", PosTag::Nn);
    add(r"^INCLUDING", PosTag::Nn);
    add(r"^Indemnification", PosTag::Nn);
    add(r"^Indemnified", PosTag::Nn);
    add(r"^Unified$", PosTag::Nn);
    add(r"^Cleaned$", PosTag::Junk);
    add(r"^Information", PosTag::Nn);
    add(r"^In$", PosTag::Nn);
    add(r"^Intellij$", PosTag::Nn);
    add(r"^ISC-LICENSE$", PosTag::Nn);
    add(r"^IS$", PosTag::Nn);
    add(r"^It$", PosTag::Nn);
    add(r"^Java$", PosTag::Nn);
    add(r"^JavaScript$", PosTag::Nn);
    add(r"^JMagnetic$", PosTag::Nn);
    add(r"^Joint$", PosTag::Nn);
    add(r"^Jsunittest$", PosTag::Nn);
    add(r"^List$", PosTag::Nn);
    add(r"^Set$", PosTag::Nn);
    // Note: Python has duplicate "Last" as NN — already added as Junk above
    add(r"^Legal$", PosTag::Nn);
    add(r"^LegalTrademarks$", PosTag::Nn);
    add(r"^Library$", PosTag::Nn);
    add(r"^Liberation$", PosTag::Nn);
    add(r"^Sans$", PosTag::Nn);
    add(r"^Interview", PosTag::Nn);
    add(r"^ProducerName", PosTag::Nn);
    add(r"^Libraries$", PosTag::Nn);
    add(r"^Initials$", PosTag::Nn);
    add(r"^Licen[cs]e", PosTag::Nn);
    add(r"^License-Alias\:?$", PosTag::Nn);
    // Note: Linux already added as PosTag::Linux above — Python has it here too
    add(r"^Locker$", PosTag::Nn);
    add(r"^Log$", PosTag::Nn);
    add(r"^Logos?$", PosTag::Nn);
    add(r"^Luxi$", PosTag::Nn);
    add(r"^Lucene", PosTag::Nn);
    add(r"^Mac$", PosTag::Nn);
    add(r"^Mondrian", PosTag::Nn);
    add(r"^Manager$", PosTag::Nn);
    add(r"^Material$", PosTag::Nn);
    add(r"^Mode$", PosTag::Nn);
    add(r"^Modified$", PosTag::Nn);
    add(r"^Mouse$", PosTag::Nn);
    add(r"^Module$", PosTag::Nn);
    add(r"^Natural$", PosTag::Nn);
    add(r"^New$", PosTag::Nn);
    add(r"^NEWS$", PosTag::Nn);
    add(r"^Neither$", PosTag::Nn);
    add(r"^Norwegian$", PosTag::Nn);
    add(r"^Notes?$", PosTag::Nn);
    add(r"^NOT$", PosTag::Nn);
    add(r"^Nessus$", PosTag::Nn);
    add(r"^NULL$", PosTag::Nn);
    add(r"^Objects?$", PosTag::Nn);
    add(r"^Open$", PosTag::Nn);
    add(r"^Operating$", PosTag::Nn);
    add(r"^OriginalFilename$", PosTag::Nn);
    add(r"^Original$", PosTag::Nn);
    add(r"^OR$", PosTag::Nn);
    add(r"^OWNER", PosTag::Nn);
    add(r"^Package$", PosTag::Nn);
    add(r"^PACKAGE$", PosTag::Nn);
    add(r"^Packaging$", PosTag::Nn);
    add(r"^Patent", PosTag::Nn);
    add(r"^Pentium$", PosTag::Nn);
    add(r"^[Pp]ermission", PosTag::Junk);
    add(r"^PERMISSIONS?", PosTag::Junk);
    add(r"^PGP$", PosTag::Nn);
    add(r"^Phrase", PosTag::Nn);
    add(r"^Plugin", PosTag::Nn);
    // Note: Python has duplicate "Policy" — already added as Junk above
    add(r"^POSIX$", PosTag::Nn);
    add(r"^Possible", PosTag::Nn);
    add(r"^Powered$", PosTag::Nn);
    add(r"^defined?$", PosTag::Junk);
    add(r"^Predefined$", PosTag::Nn);
    add(r"^Promise$", PosTag::Nn);
    add(r"^Products?\.?$", PosTag::Nn);
    add(r"^PROFESSIONAL?\.?$", PosTag::Nn);
    add(r"^Programming$", PosTag::Nn);
    add(r"^PROOF", PosTag::Nn);
    add(r"^PROVIDED$", PosTag::Nn);
    add(r"^Public\.?$", PosTag::Nn);
    add(r"^Qualified$", PosTag::Nn);
    add(r"^RCSfile$", PosTag::Nn);
    add(r"^README$", PosTag::Nn);
    add(r"^Read$", PosTag::Nn);
    add(r"^RECURSIVE$", PosTag::Nn);
    add(r"^Redistribution", PosTag::Nn);
    add(r"^Refactor$", PosTag::Nn);
    add(r"^Records?$", PosTag::Nn);
    add(r"^References?$", PosTag::Nn);
    add(r"^Related$", PosTag::Nn);
    add(r"^Release$", PosTag::Nn);
    add(r"^Revisions?$", PosTag::Nn);
    add(r"^Rule$", PosTag::Nn);
    add(r"^RIGHT", PosTag::Nn);
    add(r"^[Rr]espective", PosTag::Nn);
    add(r"^SAX$", PosTag::Nn);
    add(r"^Sections?$", PosTag::Nn);
    add(r"^Send$", PosTag::Junk);
    add(r"^Separa", PosTag::Nn);
    add(r"^Service$", PosTag::Nn);
    add(r"^Several$", PosTag::Nn);
    add(r"^SIGN$", PosTag::Nn);
    add(r"^Sink\.?$", PosTag::Nn);
    add(r"^Site\.?$", PosTag::Nn);
    add(r"^Statement", PosTag::Nn);
    add(r"^software$", PosTag::Nn);
    add(r"^SOFTWARE$", PosTag::Nn);
    add(r"^So$", PosTag::Nn);
    add(r"^Sort$", PosTag::Nn);
    add(r"^Source$", PosTag::Nn);
    add(r"^Signature$", PosTag::Nn);
    add(r"^Standard$", PosTag::Nn);
    add(r"^Std$", PosTag::Nn);
    add(r"^Supplicant", PosTag::Nn);
    add(r"^Support", PosTag::Nn);
    add(r"^Tag[A-Z]", PosTag::Nn);
    add(r"^Target$", PosTag::Nn);
    add(r"^Technical$", PosTag::Nn);
    add(r"^Termination$", PosTag::Nn);
    add(r"^The$", PosTag::Nn);
    add(r"^THE", PosTag::Nn);
    add(r"^These$", PosTag::Nn);
    add(r"^[tT]here$", PosTag::Nn);
    add(r"^This$", PosTag::Nn);
    add(r"^THIS$", PosTag::Nn);
    add(r"^Those$", PosTag::Nn);
    add(r"^Timer", PosTag::Nn);
    add(r"^TODO$", PosTag::Nn);
    add(r"^Tools?.?$", PosTag::Nn);
    add(r"^Trademarks?$", PosTag::Nn);
    add(r"^True$", PosTag::Nn);
    add(r"^TRUE$", PosTag::Nn);
    add(r"^[Tt]ext$", PosTag::Nn);
    add(r"^Unicode$", PosTag::Nn);
    add(r"^Updated", PosTag::Nn);
    add(r"^Users?$", PosTag::Nn);
    add(r"^VALUE$", PosTag::Nn);
    add(r"^Various", PosTag::Nn);
    add(r"^Vendor", PosTag::Nn);
    add(r"^VIEW$", PosTag::Nn);
    add(r"^Visit", PosTag::Nn);
    add(r"^Wheel$", PosTag::Nn);
    add(r"^Win32$", PosTag::Nn);
    add(r"^Work", PosTag::Nn);
    add(r"^WPA$", PosTag::Nn);
    add(r"^Xalan$", PosTag::Nn);
    add(r"^IP", PosTag::Nn);
    add(r"^YOUR", PosTag::Nn);
    add(r"^Your", PosTag::Nn);
    add(r"^Date[A-Z]", PosTag::Nn);
    add(r"^Create$", PosTag::Nn);
    add(r"^Engine\.$", PosTag::Nn);
    add(r"^While$", PosTag::Nn);
    add(r"^Review", PosTag::Nn);
    add(r"^Help", PosTag::Nn);
    add(r"^Web", PosTag::Nn);
    add(r"^Weld$", PosTag::Nn);
    add(r"^Common[A-Z]", PosTag::Nn);
    add(r"^MultiPart", PosTag::Nn);
    add(r"^Upload", PosTag::Nn);
    add(r"^PUT$", PosTag::Nn);
    add(r"^POST$", PosTag::Nn);
    add(r"^YUI$", PosTag::Nn);
    add(r"^PicoModal$", PosTag::Nn);
    add(r"^CodeMirror$", PosTag::Nn);
    add(r"^They$", PosTag::Junk);
    add(r"^Branched$", PosTag::Nn);
    add(r"^Partial$", PosTag::Nn);
    add(r"^Fixed$", PosTag::Nn);
    add(r"^Later$", PosTag::Nn);
    add(r"^Rear$", PosTag::Nn);
    add(r"^Left$", PosTag::Nn);

    add(r"^Improved$", PosTag::Nn);
    add(r"^Designed$", PosTag::Nn);
    add(r"^Organised$", PosTag::Nn);
    add(r"^Re-organised$", PosTag::Nn);
    add(r"^Swap$", PosTag::Nn);
    add(r"^Adapted$", PosTag::Junk);
    add(r"^Thumb$", PosTag::Nn);

    // SEEN IN Copyright (c) 1997 Dan error_act (dmalek@jlc.net)
    add(r"^error_act$", PosTag::Nn);

    // alone this is not enough for an NNP
    add(r"^Free$", PosTag::Nn);

    // Hours/Date/Day/Month text references
    add(r"^am$", PosTag::Nn);
    add(r"^pm$", PosTag::Nn);
    add(r"^AM$", PosTag::Nn);
    add(r"^PM$", PosTag::Nn);

    add(r"^Name[\.,]?$", PosTag::Nn);
    add(r"^Co-Author[\.,]?$", PosTag::Nn);
    add(r"^Author's$", PosTag::Nn);
    add(r"^Co-Author's$", PosTag::Nn);
    // the Universal Copyright Convention (1971 Paris text)
    add(r"^Convention[\.,]?$", PosTag::Nn);
    add(r"^Paris[\.,]?$", PosTag::Nn);

    add(
        r"^([Jj]anuary|[Ff]ebruary|[Mm]arch|[Aa]pril|[Jj]uly|[Aa]ugust|[Ss]eptember|[Oo]ctober|[Nn]ovember|[Dd]ecember)$",
        PosTag::Nn,
    );
    // Note: Month abbreviations already added above — Python has them here too
    // Note: Day of week already added above — Python has them here too
    add(r"^(Mon|Tue|Wed|Thu|Fri|Sat|Sun|May),?$", PosTag::Nn);

    add(r"^[Dd]ebugging$", PosTag::Junk);

    // lowercase verbs ending in "ing"
    add(r"^[a-z]+ing$", PosTag::Nn);

    // other misc capitalized words
    add(r"^Flux$", PosTag::Nn);
    add(r"^Modify$", PosTag::Nn);
    add(r"^Creation[A-Z]", PosTag::Nn);
    add(r"^Creator$", PosTag::Nn);
    add(r"^Document$", PosTag::Nn);
    // Note: Python has duplicate "Data" — already added
    add(r"^Emulation$", PosTag::Nn);
    add(r"^Exposure$", PosTag::Nn);
    add(r"^Time$", PosTag::Nn);
    add(r"^CrdInfo$", PosTag::Nn);
    add(r"^Device$", PosTag::Nn);
    add(r"^Mfg$", PosTag::Nn);
    add(r"^Comment$", PosTag::Nn);
    add(r"^Frame$", PosTag::Nn);
    add(r"^Size$", PosTag::Nn);
    add(r"^Flag$", PosTag::Nn);
    add(r"^Thumbnail$", PosTag::Nn);
    add(r"^Angle$", PosTag::Nn);
    add(r"^Duration$", PosTag::Nn);
    add(r"^Override$", PosTag::Nn);
    add(r"^Handler", PosTag::Nn);
    // Note: Python has duplicate "Unlike" — already added as Junk
    add(r"^Compression$", PosTag::Nn);
    add(r"^Letter$", PosTag::Nn);
    add(r"^Moved$", PosTag::Nn);
    add(r"^More$", PosTag::Nn);
    add(r"^Phone$", PosTag::Nn);
    add(r"^[Tt]ests?$", PosTag::Junk);

    add(r"^Inputs?$", PosTag::Nn);

    // dual caps that are not NNP
    add(r"^Make[A-Z]", PosTag::Junk);
    add(r"^Create[A-Z]", PosTag::Junk);
    add(r"^Full[A-Z]", PosTag::Nn);
    add(r"^Last[A-Z]", PosTag::Nn);
    add(r"^Author[A-Z]", PosTag::Nn);
    add(r"^Schema[A-Z]", PosTag::Junk);
    // message one is a company name
    add(r"^MessageOne", PosTag::Nnp);
    add(r"^Message[A-Z]", PosTag::Junk);
    add(r"^Short[a-z]*[A-Z]+[a-z]*", PosTag::Junk);

    add(r"^[Ww]ebsites?[\.,]?", PosTag::Junk);

    // files
    add(r"^.*\.java$", PosTag::Nn);

    // knowledge
    add(r"^knowledge[,\.]?$", PosTag::Junk);

    // Note: "holders" already added as PosTag::Holder above

    ////////////////////////////////////////////////////////////////////////////
    // PROPER NOUNS (Python lines 1781-1878)
    ////////////////////////////////////////////////////////////////////////////

    // Title case word with a trailing parens is an NNP
    add(r"^[\p{Lu}][\p{Ll}]{3,}\)\.?$", PosTag::Nnp);
    // Title case word with a leading parens is an NNP
    add(r"^\([\p{Lu}][\p{Ll}]{3,}$", PosTag::Nnp);

    // names with a slash that are NNP: Research/Unidata, LCS/Telegraphics.
    add(r"^([A-Z]([a-z]|[A-Z])+/[A-Z][a-z]+[\.,]?)$", PosTag::Nnp);

    // communications
    add(r"communications", PosTag::Nnp);

    // Places
    add(
        r"^\(?(?:Cambridge|Stockholm|Davis|Sweden[\)\.]?|Massachusetts|Oregon|California|Norway|UK|Berlin|CONCORD|Manchester|MASSACHUSETTS|Finland|Espoo|Munich|Germany|Italy|Spain|Europe|Lafayette|Indiana|Belgium|France|Sweden)[\),\.]*$",
        PosTag::Nnp,
    );

    // Misc corner case combos that are NNP
    add(r"^Software,'\,$", PosTag::Nnp);
    add(r"\(Royal$", PosTag::Nnp);
    add(r"PARADIGM$", PosTag::Nnp);
    add(r"vFeed$", PosTag::Nnp);
    add(r"nexB$", PosTag::Nnp);
    add(r"UserTesting$", PosTag::Nnp);
    add(r"D\.T\.Shield\.?$", PosTag::Nnp);
    add(r"Antill'\,$", PosTag::Nnp);
    add(r"^ONeal['\,\.]?$", PosTag::Nnp);

    // Corner cases of lowercased NNPs
    add(r"^suzuki$", PosTag::Nnp);
    add(r"toshiya\.?$", PosTag::Nnp);
    add(r"leethomason$", PosTag::Nnp);
    add(r"finney$", PosTag::Nnp);
    add(r"sean$", PosTag::Nnp);
    add(r"chris$", PosTag::Nnp);
    add(r"ulrich$", PosTag::Nnp);
    add(r"wadim$", PosTag::Nnp);
    add(r"dziedzic$", PosTag::Nnp);
    add(r"okunishinishi$", PosTag::Nnp);
    add(r"yiminghe$", PosTag::Nnp);
    add(r"daniel$", PosTag::Nnp);
    add(r"wirtz$", PosTag::Nnp);
    add(r"vonautomatisch$", PosTag::Nnp);
    add(r"werkstaetten\.?$", PosTag::Nnp);
    add(r"werken$", PosTag::Nnp);
    add(r"various\.?$", PosTag::Nnp);
    add(r"SuSE$", PosTag::Comp);
    add(r"Suse$", PosTag::Comp);
    add(r"\(Winbond\),?$", PosTag::Comp);

    // copyright : (C) 2002 by karsten wiese
    add(r"karsten$", PosTag::Nnp);
    add(r"wiese$", PosTag::Nnp);

    // treat Attributable as proper noun
    add(r"^[Aa]ttributable$", PosTag::Nnp);

    // rarer caps: EPFL-LRC/ICA
    add(r"^[A-Z]{3,6}-[A-Z]{3,6}/[A-Z]{3,6}", PosTag::Nnp);

    // Copyright (c) G-Truc Creation
    add(r"^[A-Z]-[A-Z][a-z]{2,8}", PosTag::Nnp);

    // rare form of trailing punct in name: Ian Robertson).
    add(r"^Robert.*", PosTag::Nnp);

    ////////////////////////////////////////////////////////////////////////////
    // NAMED ENTITIES: COMPANIES, GROUPS, UNIVERSITIES (Python lines 1880-2016)
    ////////////////////////////////////////////////////////////////////////////

    // AT&T (the company), needs special handling
    add(r"^AT\&T[\.,]?$", PosTag::Comp);

    // company suffix name with suffix Tech.,ltd
    add(
        r"^[A-Z][a-z]+[\.,]+(LTD|LTd|LtD|Ltd|ltd|lTD|lTd|ltD).?,?$",
        PosTag::Comp,
    );

    // company suffix, including rarer Inc>
    add(r"^[Ii]nc[\.]?[,\.>]?\)?$", PosTag::Comp);
    add(r"^Incorporated[,\.]?\)?$", PosTag::Comp);

    // ,Inc. suffix without spaces is directly a company name
    add(r"^.+,Inc\.$", PosTag::Comp);

    add(r"^[Cc]ompany[,\.]?\)?$", PosTag::Comp);
    add(r"^Limited[,\.]?$", PosTag::Comp);
    add(r"^LIMITED[,\.]?$", PosTag::Comp);

    add(r"^COMPANY,LTD$", PosTag::Comp);

    // Caps company suffixes
    add(r"^INC[\.,\)]*$", PosTag::Comp);
    add(r"^INCORPORATED[\.,\)]*$", PosTag::Comp);
    add(r"^CORP[\.,\)]*$", PosTag::Comp);
    add(r"^CORPORATION[\.,\)]*$", PosTag::Comp);
    add(r"^FOUNDATION[\.,\)]*$", PosTag::Comp);
    add(r"^GROUP[\.,\)]*$", PosTag::Comp);
    add(r"^COMPANY[\.,\)]*$", PosTag::Comp);
    add(r"^\(tm\)[\.,]?$", PosTag::Comp);
    add(r"^[Ff]orum[\.,\)]*", PosTag::Comp);

    // company suffix
    add(r"^[Cc]orp[\.,\)]*$", PosTag::Comp);
    add(r"^[Cc]orporation[\.,\)]*$", PosTag::Comp);
    add(r"^[Cc][oO][\.,\)]*$", PosTag::Comp);
    add(r"^[Cc]orporations?[\.,\)]*$", PosTag::Comp);
    add(r"^[Cc]onsortium[\.,\)]*$", PosTag::Comp);

    add(r"^[Ff]oundation[\.,\)]*$", PosTag::Comp);
    add(r"^[Aa]lliance[\.,\)]*$", PosTag::Comp);
    add(r"^Working$", PosTag::Comp);
    add(r"^[Gg]roup[\.,\)]*$", PosTag::Comp);
    add(r"^[Tt]echnolog(y|ies)[\.,\)]*$", PosTag::Comp);
    add(r"^[Cc]ommunit(y|ies)[\.,\)]*$", PosTag::Comp);
    add(r"^[Mm]icrosystems[\.,\)]*$", PosTag::Comp);
    add(r"^[Pp]rojects?[\.,\)]*,?$", PosTag::Comp);
    add(r"^[Tt]eams?[\.,\)']*$", PosTag::Comp);
    add(r"^[Tt]ech[\.,\)]*$", PosTag::Comp);
    add(r"^Limited'?[\.,\)]*$", PosTag::Comp);

    // company suffix : LLC, LTD, LLP followed by one extra char
    add(r"^[Ll][Tt][Dd]\.?,?$", PosTag::Comp);
    add(r"^[Ll]\.?[Ll]\.?[CcPp]\.?,?$", PosTag::Comp);
    add(r"^L\.P\.?$", PosTag::Comp);
    add(r"^[Ss]ubsidiary$", PosTag::Comp);
    add(r"^[Ss]ubsidiaries\.?$", PosTag::Comp);
    add(r"^[Ss]ubsidiary\(\-ies\)\.?$", PosTag::Comp);

    // company suffix : SA, SAS, AG, AB, AS, CO, labs
    add(
        r"^(S\.?A\.?S?|Sas|sas|A/S|AG,?|AB|Labs?|[Cc][Oo]|Research|Center|INRIA|Societe|KG)[,\.]?$",
        PosTag::Comp,
    );
    // French SARL
    add(r"^(SARL|S\.A\.R\.L\.)[\.,\)]*$", PosTag::Comp);
    // More company suffix : a.s. in Czechia and others
    add(r"^(a\.s\.|S\.r\.l\.?)$", PosTag::Comp);
    add(r"^Vertriebsges\.m\.b\.H\.?,?$", PosTag::Comp);
    // Iceland
    add(r"^(ehf|hf|svf|ohf)\.,?$", PosTag::Comp);
    // More company abbreviations
    add(r"^(SPRL|srl)[\.,]?$", PosTag::Comp);
    // Poland
    add(r"^(sp\.|o\.o\.)$", PosTag::Comp);
    // Eingetragener Kaufmann
    add(r"^(e\.K\.|e\.Kfm\.|e\.Kfr\.)$", PosTag::Comp);

    // company suffix : AS: this is frequent beyond Norway
    add(r"^AS", PosTag::Caps);
    // that's the ASF, not some legal form
    add(r"^ASF", PosTag::Caps);
    add(r"^AS.$", PosTag::Comp);

    // (german) company suffix
    add(r"^[Gg][Mm][Bb][Hh].?$", PosTag::Comp);
    // (e.V. german) company suffix
    add(r"^[eV]\.[vV]\.?$", PosTag::Comp);
    // (italian) company suffix
    add(r"^[sS]\.[pP]\.[aA]\.?$", PosTag::Comp);
    // swedish company suffix : ASA followed by a dot
    add(r"^ASA.?$", PosTag::Comp);
    // czech company suffix: JetBrains s.r.o.
    add(r"^s\.r\.o\.?$", PosTag::Comp);
    // (Laboratory) company suffix
    add(
        r"^(Labs?|Laboratory|Laboratories|Laboratoire)\.?,?$",
        PosTag::Comp,
    );
    // (dutch and belgian) company suffix
    add(r"^[Bb]\.?[Vv]\.?|BVBA$", PosTag::Comp);
    // university
    add(
        r"^\(?[Uu]niv(?:[\.]|ersit(?:y|e|at?|ad?))[\.,\)]*$",
        PosTag::Uni,
    );
    add(r"^UNIVERSITY$", PosTag::Uni);
    add(r"^College$", PosTag::Uni);
    // Academia/ie
    add(r"^[Ac]cademi[ae]s?$", PosTag::Uni);
    add(r"^[Ac]cademy[\.,\)]*$", PosTag::Uni);

    // Partners
    add(r"^Partners.?$", PosTag::Comp);

    // institutes
    add(r"INSTITUTE", PosTag::Comp);
    add(
        r"^\(?[Ii]nstitut(s|o|os|e|es|et|a|at|as|u|i)?\)?$",
        PosTag::Comp,
    );

    // Facility
    add(r"Facility", PosTag::Comp);

    add(r"Tecnologia", PosTag::Comp);

    // (danish) company suffix
    add(r"^ApS|A/S|IVS\.?,?$", PosTag::Comp);

    // (finnish) company suffix
    add(r"^Abp\.?,?$", PosTag::Comp);

    // affiliates or "and its affiliate(s)."
    add(r"^[Aa]ffiliate(s|\(s\))?\.?$", PosTag::Nnp);

    // Various rare company names/suffix
    add(r"^FedICT$", PosTag::Comp);
    add(r"^10gen$", PosTag::Comp);

    // Division, District
    add(r"^(District|Division)\)?[,\.]?$", PosTag::Comp);

    ////////////////////////////////////////////////////////////////////////////
    // AUTHORS (Python lines 2017-2066)
    ////////////////////////////////////////////////////////////////////////////

    // "authors" or "contributors" is interesting
    add(r"^[Aa]uthors,$", PosTag::AuthDot);
    add(r"^[Aa]uthor$", PosTag::Auth);
    add(r"^[Aa]uthor\.$", PosTag::AuthDot);
    add(r"^[Aa]uthors?\.$", PosTag::AuthDot);
    add(r"^([Aa]uthors|author')$", PosTag::Auths);
    add(r"^[Aa]uthor\(s\)$", PosTag::Auths);
    add(r"^[Aa]uthor\(s\)\.?$", PosTag::AuthDot);
    // as javadoc
    add(r"^@[Aa]uthors?:?$", PosTag::Auth);

    // et al.
    add(r"^al\.$", PosTag::AuthDot);

    // in Linux LKMs
    add(r"^MODULEAUTHOR$", PosTag::Auth);

    // Contributor(s)
    add(r"^[Cc]ontributors[,\.]?$", PosTag::Contributors);
    add(r"^Contributor[,\.]?$", PosTag::Nn);
    add(r"^Contributing$", PosTag::Nn);

    add(r"^Licensor[,\.]?$", PosTag::Nn);

    // same for developed, etc...
    add(r"^[Cc]oded$", PosTag::Auth2);
    add(r"^\(?[Rr]ecoded$", PosTag::Auth2);
    add(r"^\(?[Mm]odified$", PosTag::Auth2);
    add(r"^\(?[Cc]reated$", PosTag::Auth2);
    // written is often misspelled
    add(r"^\(?[Ww]ritt?e[dn]$", PosTag::Auth2);
    // rewritten is often misspelled
    add(r"^\(?[Rr]ewritt?e[dn]$", PosTag::Auth2);
    add(r"^\(?[Mm]aintained$", PosTag::Auth2);
    add(r"^\(?[Dd]eveloped$", PosTag::Auth2);
    add(r"^\(?[Au]thored$", PosTag::Auth2);

    // committers is interesting
    add(r"[Cc]ommitters\.?,?", PosTag::Commit);

    // same for maintainers, developers, admins
    add(r"^[Aa]dmins?$", PosTag::Maint);
    add(r"^[Dd]evelopers?\.?$", PosTag::Maint);
    add(r"^[Mm]aintainers?\.?$", PosTag::Maint);
    add(r"^[Cc]o-maintainers?\.?$", PosTag::Maint);

    // Note: Conjunctions, year patterns, etc. already added above

    ////////////////////////////////////////////////////////////////////////////
    // ALL CAPS AND PROPER NOUNS (Python lines 2198-2260)
    ////////////////////////////////////////////////////////////////////////////

    // composed proper nouns, ie. Jean-Claude or ST-Microelectronics
    add(r"^[A-Z][a-zA-Z]+\s?-\s?[A-Z]?[a-zA-Z]+[\.,]?$", PosTag::Nnp);
    add(
        r"^[\p{Lu}][\p{L}]+[-–][\p{Lu}]?[\p{L}]+[\.,]?$",
        PosTag::Nnp,
    );

    // Countries abbreviations
    add(r"^U\.S\.A\.?$", PosTag::Nnp);

    // Dotted ALL CAPS initials
    add(r"^([A-Z]\.){1,3}$", PosTag::Nnp);

    add(r"^\(?@[A-Za-z0-9_]{2,}[,\)]?$", PosTag::Nnp);

    // misc corner cases such LaTeX3 Project and other
    add(r"^LaTeX3$", PosTag::Nnp);
    add(r"^Meridian'93$", PosTag::Nnp);
    add(r"^Xiph.Org$", PosTag::Nnp);
    add(r"^iClick,?$", PosTag::Nnp);
    add(r"^electronics?$", PosTag::Nnp);

    // proper nouns with digits
    add(r"^([\p{Lu}][\p{Ll}0-9]+){1,2}[\.,]?$", PosTag::Nnp);

    // saxon genitive, ie. Philippe's
    add(r"^[\p{Lu}][\p{Ll}]+'s$", PosTag::Nnp);

    // Uppercase dotted name, ie. P. or DMTF.
    add(r"^([A-Z]+\.)+$", PosTag::Pn);

    // proper noun with some separator and trailing comma
    add(r"^[\p{Lu}]+\.[\p{Lu}][\p{Ll}]+,?$", PosTag::Nnp);

    // proper noun with apostrophe ': D'Orleans, D'Arcy, T'so, Ts'o
    add(r"^[\p{Lu}][\p{Ll}]?'[\p{Lu}]?[\p{Ll}]+[,\.]?$", PosTag::Nnp);

    // proper noun with apostrophe ': d'Itri
    add(r"^[\p{Ll}]'[\p{Lu}]?[\p{Ll}]+[,\.]?$", PosTag::Nnp);

    // exceptions to all CAPS words
    add(r"^[A-Z]{3,4}[0-9]{4},?$", PosTag::Nn);

    // exceptions to CAPS used in obfuscated emails (AT/DOT already added above)

    // exceptions to CAPS
    add(r"^MMC$", PosTag::Junk);

    // all CAPS word, at least 1 char long, including optional trailing comma or dot
    add(r"^[A-Z0-9]+,?$", PosTag::Caps);

    // all CAPS word 3 chars and more, enclosed in (parens)
    add(r"^\([A-Z0-9]{2,}\)$", PosTag::Caps);

    // all CAPS word, 3 chars and more, including optional trailing single quote
    add(r"^[A-Z]{2,}'?$", PosTag::Caps);

    // proper noun: first CAP, as in JohnGlen including optional trailing period or comma
    // Unicode-aware: \p{Lu} matches any uppercase letter (including É, Ü, etc.)
    // and \p{Ll} matches any lowercase letter (including é, ü, ç, etc.)
    add(r"^([\p{Lu}][\p{Ll}0-9]+){1,2}\.?,?$", PosTag::Nnp);

    ////////////////////////////////////////////////////////////////////////////
    // URLS AND EMAILS (Python lines 2261-2298)
    ////////////////////////////////////////////////////////////////////////////

    // email start-at-end: <sebastian.classen at freenet.ag>
    add(r"^<([a-zA-Z]+[a-zA-Z\.]){2,5}$", PosTag::EmailStart);
    add(r"^[a-zA-Z\.]{2,5}>$", PosTag::EmailEnd);

    // a .sh shell script is NOT an email
    add(r"^.*\.sh\.?$", PosTag::Junk);

    // email eventually in parens or brackets with some trailing punct
    add(
        r"^(?:[A-Za-z])*[<(]?[a-zA-Z0-9]+[a-zA-Z0-9+_\-\.%]*(@|at)[a-zA-Z0-9][a-zA-Z0-9+_\-\.%]+\.[a-zA-Z]{2,3}[>)\.,]*$",
        PosTag::Email,
    );

    // mailto URLs
    add(r"^mailto:.{2,}@.{2,}\.[a-z]{2,3}", PosTag::Email);

    add(
        r"^<[a-zA-Z]+[a-zA-Z0-9\.]+@[a-zA-Z][a-zA-Z0-9]+\.[a-zA-Z]{2,5}>$",
        PosTag::Email,
    );

    // URLS such as <(http://fedorahosted.org/lohit)> or ()
    add(r"[<\(]https?:.*[>\)]", PosTag::Url);
    // URLS such as ibm.com without a scheme
    add(
        r"\s?[a-z0-9A-Z\-\.\_]+\.([Cc][Oo][Mm]|[Nn][Ee][Tt]|[Oo][Rr][Gg]|us|mil|io|edu|co\.[a-z][a-z]|eu|ch|fr|de|be|se|nl|au|biz|sy|dev)\s?[\.,]?$",
        PosTag::Url2,
    );
    // URL wrapped in () or <>
    add(
        r"[\(<]+\s?[a-z0-9A-Z\-\.\_]+\.(com|net|org|us|mil|io|edu|co\.[a-z][a-z]|eu|ch|fr|jp|de|be|se|nl|au|biz|sy|dev)\s?[\.\)>]+$",
        PosTag::Url,
    );
    add(
        r"<?a?.(href)?.\(?[a-z0-9A-Z\-\.\_]+\.(com|net|org|us|mil|io|edu|co\.[a-z][a-z]|eu|ch|fr|jp|de|be|se|nl|au|biz|sy|dev)[\.\)>]?$",
        PosTag::Url,
    );
    // derived from regex in cluecode.finder
    add(
        r"<?a?.(href)?.(?:(?:http|ftp|sftp)s?://[^\s<>\[\]]+|(?:www|ftp)\.[^\s<>\[\]]+)\.?>?",
        PosTag::Url,
    );

    add(
        r"^\(?<?https?://[a-zA-Z0-9_\-]+(\.([a-zA-Z0-9_\-])+)+.?\)?>?$",
        PosTag::Url,
    );

    // URLS with trailing/ such as http://fedorahosted.org/lohit/
    // URLS with leading( such as (http://qbnz.com/highlighter/
    add(r"\(?https?:.*/", PosTag::Url);

    ////////////////////////////////////////////////////////////////////////////
    // MISC (Python lines 2299-2361)
    ////////////////////////////////////////////////////////////////////////////

    // .\" is not a noun
    add(r#"^\.\\"?$"#, PosTag::Junk);

    // Mixed cap nouns (rare) LeGrande
    add(
        r"^[\p{Lu}][\p{Ll}]+[\p{Lu}][\p{Ll}]+[\.\,]?$",
        PosTag::MixedCap,
    );

    // Code variable names including snake case exceptions
    add(r"\(?Massachusetts_Institute_of_Technology,?$", PosTag::Nnp);
    add(
        r"National_de_Recherche_en_Informatique_et_en_Automatique,?$",
        PosTag::Nnp,
    );
    add(r"Keio_University\)?,?$", PosTag::Nnp);
    add(r"__MyCompanyName__[\.,]?$", PosTag::Nnp);

    // email in brackets <brett_AT_jdom_DOT_org>
    // (karl AT indy.rr.com)
    add(
        r"(?i:^[<\(][\w\.\-\+]+at[\w\.\-\+]+(dot)?[\w\.\-\+]+[/)>]$)",
        PosTag::Email,
    );

    // Code variable names including snake case
    add(r"^.*(_.*)+$", PosTag::Junk);

    // !$?
    add(r"^\!\$\?$", PosTag::Junk);

    // things composed only of non-word letters (e.g. junk punctuations)
    // but keeping _ ? and () and - as parts of words
    add(r"^[^\w\?\-\(\)]{3,10}$", PosTag::Junk);

    // short hex for commits
    add(r"^[abcdef0-9]{7}$", PosTag::Junk);

    // alternance of letters and puncts :co,e):f!
    add(
        r"^\W?([a-z0-9]{1,3}[\.,:;\x22\(\)!\\=%&@\#]+){3,}\W?$",
        PosTag::Junk,
    );

    // Note: "dot" already added as PosTag::Dot above

    // moment/moment is an odd name
    add(r"moment/moment$", PosTag::Nnp);

    // Note: single parens already added as PosTag::Parens above

    // some punctuation combos
    add(r"^(?:=>|->|<-|<=)$", PosTag::Junk);

    add(r"^semiconductors?[\.,]?$", PosTag::Nnp);

    ////////////////////////////////////////////////////////////////////////////
    // CATCH-ALL (Python line 2360)
    ////////////////////////////////////////////////////////////////////////////

    // nouns (default) — this is the final catch-all
    add(r".+", PosTag::Nn);

    patterns
}

#[cfg(test)]
#[path = "patterns_test.rs"]
mod tests;
