//! Refinement and cleanup functions for detected copyright strings.
//!
//! After the parser produces raw detection text from parse tree nodes,
//! these functions clean up artifacts: strip junk prefixes/suffixes,
//! normalize whitespace, remove duplicate copyright words, strip
//! unbalanced parentheses, and filter out known junk patterns.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::candidates::strip_balanced_edge_parens;

// ─── Constant sets ───────────────────────────────────────────────────────────

/// Generic prefixes stripped from names (holders/authors).
const PREFIXES: &[&str] = &[
    "?",
    "??",
    "????",
    "(insert",
    "then",
    "current",
    "year)",
    "maintained",
    "by",
    "developed",
    "created",
    "written",
    "recoded",
    "coded",
    "modified",
    // Note: Python has 'maintained''created' (missing comma = concatenation).
    // We include both separately.
    "maintainedcreated",
    "$year",
    "year",
    "uref",
    "owner",
    "from",
    "and",
    "of",
    "to",
    "for",
    "or",
    "<p>",
];

/// Suffixes stripped from copyright strings.
static COPYRIGHTS_SUFFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "copyright",
        ".",
        ",",
        "year",
        "parts",
        "any",
        "0",
        "1",
        "author",
        "all",
        "some",
        "and",
        "</p>",
        "is",
        "-",
        "distributed",
        "information",
        "credited",
        "by",
    ]
    .into_iter()
    .collect()
});

/// Authors prefixes = PREFIXES ∪ author-specific words.
static AUTHORS_PREFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s: HashSet<&str> = PREFIXES.iter().copied().collect();
    for w in &[
        "contributor",
        "contributor(s)",
        "authors",
        "author",
        "authors'",
        "author:",
        "author(s)",
        "authored",
        "created",
        "author.",
        "author'",
        "authors,",
        "authorship",
        "maintainer",
        "co-maintainer",
        "or",
        "spdx-filecontributor",
        "</b>",
        "mailto:",
        "name'",
        "a",
        "moduleauthor",
        "\u{a9}", // ©
    ] {
        s.insert(w);
    }
    s
});

/// Authors junk — detected author strings that are false positives.
static AUTHORS_JUNK: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "james hacker.",
        "james random hacker.",
        "contributor. c. a",
        "grant the u.s. government and others",
        "james random hacker",
        "james hacker",
        "company",
        "contributing project",
        "its author",
        "gnomovision",
        "would",
        "may",
        "attributions",
        "the",
        "app id",
        "project",
        "previous lucene",
        "group",
        "the coordinator",
        "the owner",
        "a group",
        "sonatype nexus",
        "apache tomcat",
        "visual studio",
        "apache maven",
        "visual studio and visual studio",
        "work",
        "additional",
        "builder",
        "guice",
        "incorporated",
        "ds",
        "guide",
        "recommend",
        "recheck",
        "volunteer",
        "donald becker",
    ]
    .into_iter()
    .collect()
});

/// Prefix that triggers ignoring the author entirely.
const AUTHORS_JUNK_PREFIX: &str = "httpProxy";

static AUTHORS_JUNK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let patterns = [
        r"(?i)\bpromote products derived from\b",
        r"(?i)\bendorse or promote\b",
        r"(?i)^the builder\b",
        r"(?i)^the line highlight\b",
        r"(?i)^the initial developer\b",
        r"(?i)^trademark\b",
        r"(?i)^time to time\b",
        r"(?i)^the group of people\b",
        r"(?i)^by,? or\b",
        r"(?i)^group conversion\b",
        r"(?i)^ctnewmethod\b",
        r"(?i)\bplugins?\. fixes\b",
        r"(?i)\bnormalized to upper\b",
        r"(?i)\benhancing and supporting\b",
        r"(?i)\band to credit the\b",
        r"(?i)^other promise\b",
        r"(?i)^record factory\b",
        r"(?i)^the object\b",
        r"(?i)^the owner,?\b",
        r"(?i)^the job$",
        r"(?i)^the ietf\b",
        r"(?i)^manually\b",
        r"(?i)^register\b",
        r"(?i)^communication sent\b",
        r"(?i)^developers tom\b",
        r"(?i)^ext4\.\b",
        r"(?i)\bmore documentation\b",
        r"(?i)\breturn enum\b",
        r"(?i)\breturn u\d",
        r"(?i)\bmore details of status\b",
        r"(?i)\bu64$",
        r"(?i)^\d+\.\d+\s+\d+-\w+-\d+\s+fix\b",
        r"(?i)\bbut not limited to communication\b",
        r"(?i)\bfor have helping\b",
        r"(?i)\bunit of \d+mb\b",
        r"(?i)\bfor the\s+[\p{L}0-9._+-]+(?:\s+[\p{L}0-9._+-]+){0,2}\s+project\b",
        r"(?i)\bTenable licenses\b",
        r"(?i)\bwith participation of the open\b",
        r"(?i)\bfurthermore\b",
        r"(?i)\bits cell\. we\b",
        r"(?i)\b@version \$id\b",
        r"(?i)\bsymbols viewer\b",
        r"(?i)\bfinal specification itself\b",
        r"(?i)\bfor each of the audio\b",
        r"(?i)\bfrom start to end\b",
        r"(?i)\boperator to\b",
        r"(?i)^programming with objects\b",
        r"(?i)^grateful to\b",
        r"(?i)^would also like to thank\b",
        r"(?i)^would like to thank\b",
        r"(?i)^date modified$",
        r"(?i)^date header id name\b",
        r"(?i)^technical committee$",
        r"(?i)^users of the program$",
        r"(?i)^should not be interpreted\b",
        r"(?i)^works devices national\b",
        r"(?i)\band its \d+\.\s*neither\b",
        r"(?i)\band its effective immediately\b",
        r"(?i)\band its neither the\b",
        r"(?i)^effective immediately\b",
        r"(?i)^(?:\d+\.)?\s*neither\b",
        r"(?i)\blastmod\b.*\bstream.*\bregisterfield\b",
        r"(?i)^who\s+hopes\b",
        r"(?i)^so preceded by\b",
        r"(?i)^bounce, so we\b",
        r"(?i)^the current user$",
        r"(?i)^select\s+\.\w+$",
        r"(?i)^with inkscape\b",
        r"(?i)^in my book\b",
        r"(?i)^chebyshev approximation to the$",
        r"(?i)^developers,\s+testers,\s+and\s+maintainers\b",
        r"(?i)^maintainers\.\s+when making changes to\b",
        r"(?i)^point to the$",
        r"(?i)^did not confirm that$",
        r"(?i)^could not find a direct reference to$",
        r"(?i)^developers aware of the problem and plan to fix it$",
        r"(?i)^with the help of\s+.+$",
        r"(?i)^with the mode of \d{3,4}$",
        r"(?i)^kernel afs\.\s+skip afs metadata and acls$",
        r"(?i)^with a (?:fsf|dco)$",
        r"(?i)^gives unlimited$",
        r"(?i)^word assigns past and future changes\b",
        r"(?i)^maintainers\s*<[^>]+>\s+from\s+https?://\S+$",
        r"(?i)^versions,\s+and$",
        r"(?i)^versions$",
        r"(?i)^makes$",
        r"(?i)^grants irrevocable$",
        r"(?i)^version information$",
        r"(?i)^not responsible for the consequences of use of$",
        r"(?i)^at sunpro, a sun microsystems, inc\. business$",
        r"(?i)^debian by .+\bupstream-contact\b.*$",
        r"(?i)^was originally written by .+\bit is now maintained by\b.*$",
        r"(?i)^writing to the appropriate\b.*$",
        r"(?i)^the task'?s numa mempolicy\b.*$",
        r"(?i)^the authors laboriously took the trouble\b.*$",
        r"(?i)^laboriously took the trouble\b.*$",
    ];
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
});

fn is_junk_author(s: &str) -> bool {
    AUTHORS_JUNK_PATTERNS.iter().any(|re| re.is_match(s))
}

/// Holders prefixes = PREFIXES ∪ holder-specific words.
static HOLDERS_PREFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s: HashSet<&str> = PREFIXES.iter().copied().collect();
    for w in &[
        "-",
        "a",
        "<a",
        "href",
        "ou",
        "portions",
        "portion",
        "notice",
        "holders",
        "holder",
        "property",
        "parts",
        "part",
        "at",
        "cppyright",
        "assemblycopyright",
        "c",
        "works",
        "present",
        "right",
        "rights",
        "reserved",
        "held",
        "is",
        "(x)",
        "later",
        "$",
        "current.year",
        "\u{a9}", // ©
        "author",
        "authors",
    ] {
        s.insert(w);
    }
    s
});

/// Holders prefixes including "all" (used when "reserved" is in the string).
static HOLDERS_PREFIXES_WITH_ALL: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HOLDERS_PREFIXES.clone();
    s.insert("all");
    s
});

/// Suffixes stripped from holder strings.
static HOLDERS_SUFFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "http",
        "and",
        "email",
        "licensing@",
        "(minizip)",
        "website",
        "(c)",
        "<http",
        "/>",
        ".",
        ",",
        "year",
        "some",
        "all",
        "right",
        "rights",
        "reserved",
        "reserved.",
        "href",
        "c",
        "a",
        "</p>",
        "or",
        "taken",
        "from",
        "is",
        "-",
        "distributed",
        "information",
        "credited",
        "$",
    ]
    .into_iter()
    .collect()
});

/// Holders junk — detected holder strings that are false positives.
static HOLDERS_JUNK: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "a href",
        "property",
        "licensing@",
        "c",
        "works",
        "http",
        "the",
        "are",
        "?",
        "cppyright",
        "parts",
        "disclaimed",
        "or",
        "<holders>",
        "author",
        // License boilerplate false positives
        "holders",
        "holder",
        "holder,",
        "and/or",
        "if",
        "grant",
        "notice",
        "do the following",
        "does",
        "has",
        "each",
        "also",
        "in",
        "simply",
        "other",
        "shall",
        "said",
        "who",
        "your",
        "their",
        "ensure",
        "allow",
        "terms",
        "conditions",
        "information",
        "contributors",
        "contributors as",
        "contributors and the university",
        "indemnification",
        "license",
        "claimed",
        "but",
        "agrees",
        "patent",
        "owner",
        "owners",
        "yyyy",
        "expressly",
        "stating",
        "enforce",
        "d",
        "ss",
        // Additional single-word junk
        "given",
        "may",
        "every",
        "no",
        "good",
        "row",
        "logo",
        "flag",
        "updated",
        "law",
        "england",
        "tm",
        "pgp",
        "distributed",
        "as",
        "null",
        "psy",
        "object",
        "indicate the origin and nature of",
        "statements",
        "protection",
        "(if any) with",
        "if any with",
        // Short gibberish from binary data
        "ga",
        "ka",
        "aa",
        "qa",
        "yx",
        "ac",
        "ae",
        "gn",
        "cb",
        "ib",
        "qb",
        "py",
        "pu",
        "ce",
        "nmd",
        "a1",
        "deg",
        "gnu",
        "with",
        "yy",
        "c/",
        "messages",
        "licenses",
        "not limited",
        "charge",
        "case 2",
        "dot",
        "public",
        // C function/macro names from ICS false positives
        "width",
        "len",
        "do",
        "date",
        "year",
        "note",
        "update",
        "info",
        "notices",
        "duplicated",
        "register",
        // C identifier/keyword false positives from ICS
        "isascii",
        "iscntrl",
        "isprint",
        "isdigit",
        "isalpha",
        "toupper",
        "yyunput",
        "ambiguous",
        "indir",
        "notive",
        "strict",
        "decoded",
        "unsigned",
        // Short numbers/tokens from code
        "0 1",
        "8",
        "9",
        "16",
        "24",
        "4",
        // More boilerplate/legal words
        "notices all the files",
        "may not be removed or altered",
        "duplicated in",
        "mjander",
        "3dfx",
        "related",
    ]
    .into_iter()
    .collect()
});

/// Junk copyright regex patterns (compiled once).
static COPYRIGHTS_JUNK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let patterns = [
        r"(?i)^copyright \(c\)$",
        r"(?i)^\(c\) by$",
        r"(?i)\(c\) [a-zA-Z][a-z] \(c\)",
        r"(?i)^copyright holder or simply",
        r"(?i)^copyright notice\.",
        r"(?i)^copyright of uc berkeley's berkeley software distribution",
        r"(?i)^and/or the universal copyright convention",
        r"(?i)^attn copyright",
        r"(?i)^\(c\)$",
        r"(?i)^c$",
        r"(?i)^\(c\) any recipient$",
        r"(?i)^\(c\) as$",
        r"(?i)^\(c\),? \(c\)$",
        r"(?i)^\(c\) cockroach enterprise",
        r"(?i)^\(c\) each recipient$",
        r"(?i)^\(c\) forums$",
        r"(?i)^\(c\) if you",
        r"(?i)^\(c\) individual use",
        r"(?i)^code copyright grant",
        r"(?i)^copyright and license, contributing",
        r"(?i)^copyright as is group",
        r"(?i)^copyright \(c\) , and others",
        r"(?i)^copyright \(c\) <holders>",
        r"(?i)^copyright copyright and",
        r"(?i)^copyright\s+and\s+conditions\b",
        r"(?i)^copyright \(c\) year$",
        r"(?i)^copyright \(c\) year your",
        r"(?i)^copyright\s+rsa$",
        r"(?i)^copyright, designs and patents",
        r"(?i)copyright \d+ m\. y\.( name)?",
        r"(?i)^copyright\s*,\s*proprietary\b",
        r"(?i)^copyright\s+proprietary\b",
        r"(?i)^proprietary$",
        r"(?i)^copyrighte?d? (by)?$",
        r"(?i)^copyrighted by its$",
        r"(?i)^copyrighted by their authors",
        r"(?i)^copyrighted material, only this license",
        r"(?i)^copyright for a new language",
        r"(?i)^copyright from license",
        r"(?i)^copyright help center",
        r"(?i)^copyright holder and contributors?\.?$",
        r"(?i)^copyright-holder and its contributors$",
        r"(?i)^copyright holder has",
        r"(?i)\bMODULEAUTHOR\b",
        r"(?i)\bTHE\s+\S+'S\s+COPYRIGHT\b",
        r"(?i)\bTHE\s+PACKAGE'S\b",
        r"(?i)\bTHE\s+\S+'S$",
        r"(?i)^copyright holder means",
        r"(?i)^copyright holder who",
        r"(?i)^copyright holder nor",
        r"(?i)^copyright holder,? or",
        r"(?i)^copyright holders and contribut",
        r"(?i)^copyright holder's",
        r"(?i)^copyright holder\(s\) or the author\(s\)",
        r"(?i)^copyrights?\s*,\s*patents?\s*,\s*trade secrets?\s+or\b",
        r"(?i)^copyrights?\s*,\s*patents?\s*,\s*trade secrets?\s*$",
        r"(?i)^copyright\s*,\s*patents?\s*,\s*trade secrets?\s+or\b",
        r"(?i)^copyright\s*,\s*patents?\s*,\s*trade secrets?\s*$",
        r"(?i)^copyright\s*,\s*patents?\s+or\b",
        r"(?i)^copyright\s*,\s*patents?\s*$",
        r"(?i)^copyright\s*,\s*patents?\s*,\s*trade secrets?\b",
        r"(?i)^copyright\s*,\s*including\b",
        r"(?i)^copyright\s*,\s*patent\s*,\s*or trademark\b",
        r"(?i)^copyright\s*,\s*patent\s*,\s*trademark\s*,\s*or\b",
        r"(?i)^copyright\s*,\s*patent\s*,\s*trademark\s*,\s*and attribution\b",
        r"(?i)^copyrights?\s*,\s*trade secrets?\s+or\b",
        r"(?i)^copyrights?\s*,\s*trade secrets?\s*$",
        r"(?i)^copyright\s*,\s*trade secrets?\s+or\b",
        r"(?i)^copyright\s*,\s*trade secrets?\s*$",
        r"(?i)^copyright\s*,\s*trade secret\s*,\s*trademark\s+or\b",
        r"(?i)^copyright\s*,\s*trade secret\s*,\s*trademark\s+or\s+other intellectual property rights\b",
        r"(?i)^copyright\s*\(c\)\s*trademark\b",
        r"(?i)^copyrights?\s*,\s*trademarks?\s+or\b",
        r"(?i)^copyrights?\s*,\s*trademarks?\s*$",
        r"(?i)^copyright\s*,\s*trademark\b",
        r"(?i)^copyright\s*,\s*trademark\s*,\s*trade secrets?\s+or\b",
        r"(?i)^copyright\s*,\s*trademark\s*,\s*trade secrets?\s*$",
        r"(?i)^copyright\s*,\s*to do the following\b",
        r"(?i)^copyright including",
        r"(?i)^copyright in section",
        r"(?i)^copyright john wiley & sons, inc\. year",
        r"(?i)^copyright l?gpl group",
        r"(?i)^copyright, license, and",
        r"(?i)^copyright merged arm",
        r"(?i)^copyright neither",
        r"(?i)^copyright notices, authorship",
        r"(?i)^copyright not limited",
        r"(?i)^copyright owner or",
        r"(?i)^copyright redistributions",
        r"(?i)^copyright the project$",
        r"(?i)^copyright\.? united states$",
        r"(?i)^\(c\) software activation",
        r"(?i)^\(c\) source code",
        r"(?i)^full copyright statement",
        r"(?i)^universal copyright convention",
        r"(?i)^u\.s\. copyright act",
        r"(?i)^\(c\) Object c$",
        r"(?i)^copyright headers?",
        r"(?i)^\(c\) \(c\) B$",
        r"(?i)^\(c\) group$",
        r"(?i)^\(c\) \(c\) A$",
        r"(?i)^\(c\) the\b",
        r"(?i)^\(c\) if\b",
        r"(?i)^\(c\) for\b",
        r"(?i)^\(c\)\s+(?:convert|multiply)\b",
        r"(?i)^\(c\) contributor,\s*path$",
        r"(?i)^the\s+Embedded\s+Configurable\s+Operating\s+System\b",
        r"(?i)\bpkg\.(author|homepage)\b",
        r"(?i)\bdate\.year\b",
        r"(?i)\bYYYY-MM-DD\b",
        r"(?i)<\s*pkg\.[a-zA-Z0-9_.-]+\s*>",
        r"(?i)\bCopyright\b.*\s\$\s*$",
        r"(?i)^\(c\) to\b",
        r"(?i)^\(c\) one\b",
        r"(?i)^\(c\) all\b",
        r"(?i)^\(c\) allow\b",
        r"(?i)^\(c\) ensure\b",
        r"(?i)^\(c\) permit\b",
        r"(?i)^\(c\) delete\b",
        r"(?i)^\(c\) return\b",
        r"(?i)^\(c\) flag\b",
        r"(?i)^\(c\) charge\b",
        r"(?i)^\(c\) automatically\b",
        r"(?i)^\(c\) completely\b",
        r"(?i)^\(c\) terminate\b",
        r"(?i)^\(c\) suspend\b",
        r"(?i)^\(c\) material\b",
        r"(?i)^\(c\) indemnification\b",
        r"(?i)^\(c\) england\b",
        r"(?i)^\(c\) a$",
        r"(?i)^\(c\) b$",
        r"(?i)^\(c\) c$",
        r"(?i)^\(c\) s$",
        r"(?i)^\(c\) u$",
        r"(?i)^\(c\) this\.",
        r"(?i)^\(c\) nat\d",
        r"(?i)^\(c\) ss+y?$",
        r"(?i)^\(c\) objc",
        r"(?i)^\(c\) \.year",
        r"(?i)^\(c\) case\b",
        r"(?i)^\(c\) offer\b",
        r"(?i)^\(c\) compute\b",
        r"(?i)^\(c\) there\b",
        r"(?i)^\(c\) c printf\b",
        r"(?i)^\(c\) -\d",
        r"(?i)^\(c\) ac$",
        r"(?i)^\(c\) eu$",
        r"(?i)^\(c\) continue\b",
        r"(?i)^\(c\) component\b",
        r"(?i)^\(c\) ext\.",
        r"(?i)^\(c\) assert\.",
        r"(?i)^\(c\) ,\(d\)",
        r"(?i)^copyright notice\b",
        r"(?i)^copyright holders? be\b",
        r"(?i)^copyright holders? and/?or\b",
        r"(?i)^copyright holders?$",
        r"(?i)^copyright holders? shall\b",
        r"(?i)^copyright\s+footer$",
        r"(?i)^copyright,\s*legal\s+notice(?:\s+div\s+div\.legalnotice\s+p)?$",
        r"(?i)^copyright\s+color\s+666666$",
        r"(?i)^copyright holder saying\b",
        r"(?i)^copyright holders of\b",
        r"(?i)^copyright holder,$",
        r"(?i)^copyright holder notifies\b",
        r"(?i)^copyright holder is reinstated\b",
        r"(?i)^copyright holder fails\b",
        r"(?i)^copyright holders, but\b",
        r"(?i)^copyright holders, or\b",
        r"(?i)^copyright holders, authors\b",
        r"(?i)^copyright holder\. ",
        r"(?i)^copyright holder, author\b",
        r"(?i)^copyright holders? disclaim\b",
        r"(?i)^copyright and has\b",
        r"(?i)^copyright and trademark\b",
        r"(?i)^copyright and other proprietary\b",
        r"(?i)^copyright in the\b",
        r"(?i)^copyright in and\b",
        r"(?i)^copyright the software\b",
        r"(?i)^copyright info for\b",
        r"(?i)^copyright grant\b",
        r"(?i)^copyright terms\b",
        r"(?i)^copyright does\b",
        r"(?i)^copyright unless\b",
        r"(?i)^copyright also\b",
        r"(?i)^copyright are\b",
        r"(?i)^copyright line\b",
        r"(?i)^copyright resulting\b",
        r"(?i)^copyright treaty\b",
        r"(?i)^copyright rights\b",
        r"(?i)^copyright appears?\b",
        r"(?i)^copyright years? updated\b",
        r"(?i)^copyright license\b",
        r"(?i)^copyright copyright\b",
        r"(?i)^copyrights covering\b",
        r"(?i)^copyrights for the\b",
        r"(?i)^copyright for the\b",
        r"(?i)^copyright symbol\b",
        r"(?i)^copyright claim\b",
        r"(?i)^copyright interest\b",
        r"(?i)^copyright shall\b",
        r"(?i)^copyright statement\b",
        r"(?i)^copyright disclaimer\b",
        r"(?i)^copyright permission\b",
        r"(?i)^copyright protection\b",
        r"(?i)^copyright owner\b",
        r"(?i)^copyright yyyy\b",
        r"(?i)^copyright exceptions\b",
        r"(?i)^copyright or patent\b",
        r"(?i)^copyright is claimed\b",
        r"(?i)^copyright messages\b",
        r"(?i)^copyright information\b",
        r"(?i)^copyright at the\b",
        r"(?i)^copyright claimed\b",
        r"(?i)^copyright law\b",
        r"(?i)^copyright page\b",
        r"(?i)^copyright holders? or\b",
        r"(?i)^copyrighted material outside\b",
        r"(?i)^copyright holder as a result\b",
        r"(?i)^copyright holder explicitly\b",
        r"(?i)^copyright holder collectively\b",
        r"(?i)^copyright holder stating\b",
        r"(?i)^copyright holder to enforce\b",
        r"(?i)^copyright holder expressly\b",
        r"(?i)^copyright holder maintains\b",
        r"(?i)^copyright holder may\b",
        r"(?i)^copyright holder is whoever\b",
        r"(?i)^copyright holder, and\b",
        r"(?i)^copyright holder, but\b",
        r"(?i)^copyright holder and seek\b",
        r"(?i)^copyright holder of\b",
        r"(?i)^copyright or\b",
        r"(?i)^copyright is held by\b",
        r"(?i)^copyright as specified\b",
        r"(?i)^copyrights and patent\b",
        r"(?i)^copyright holder provides\b",
        r"(?i)^copyright holder agrees\b",
        r"(?i)^copyright holder and current maintainer\b",
        r"(?i)^copyright holder,?\s*referring\b",
        r"(?i)\bmaintainer referring to the person\b",
        r"(?i)\bexplicitly and prominently states\b",
        r"(?i)\bm\. y\. name\b",
        r"(?i)^copyrights are property of\b",
        r"(?i)^copyright holder,? we do not list\b",
        r"(?i)^copyright and no-warranty notice\b",
        r"(?i)^copyright pages? of volumes?\b",
        r"(?i)^copyright as is\b",
        r"(?i)^copyright its (contributors|licensors|respective)\b",
        r"(?i)^copyright owned\b",
        r"(?i)^copyright attr\b",
        r"(?i)^copyright content\b",
        r"(?i)^copyright a href\b",
        r"(?i)^copyright designation\b",
        r"(?i)^copyright infringement\b",
        r"(?i)^copyright General Public\b",
        r"(?i)^copyright owners\b",
        r"(?i)^copyright and as\b",
        r"(?i)^copyright applies\b",
        r"(?i)^copyrights of all\b",
        r"(?i)^copyright As I\b",
        r"(?i)^copyright by The Regents\b",
        r"(?i)^copyright by other\b",
        r"(?i)^copyrighted by C\.\b",
        r"(?i)^copyright note\b",
        r"(?i)^copyright clause\b",
        r"(?i)^copyright message\b",
        r"(?i)^copyright below\b",
        r"(?i)^copyright is below\b",
        r"(?i)^copyright date\b",
        r"(?i)^copyright year$",
        r"(?i)^copyright notive\b",
        r"(?i)^copyright inside\b",
        r"(?i)^copyright match\b",
        r"(?i)^copyright notices\b",
        r"(?i)^copyright GNU\b",
        r"(?i)^COPYRIGHT AS PER\b",
        r"(?i)^Copyright and Related Rights\b",
        r"(?i)^copyright by Section\b",
        r"(?i)^Copyright The GNOME\b",
        r"(?i)^Copyright The$",
        r"(?i)^Copyright notices\b",
        r"(?i)^copyright to$",
        r"(?i)^copyrights in$",
        r"(?i)^copyright to the\b",
        r"(?i)^copyrighted \(with\b",
        r"(?i)^\(Copyright notice\)",
        r"(?i)^COPYRIGHT HOLDER ALLOWS\b",
        r"(?i)^copyright holders?,? disclaims?\b",
        r"(?i)\bwe do not list the\b",
        r"(?i)\bno-warranty notice unaltered\b",
        r"(?i)\bprovides the program as\b",
        r"(?i)\breferring to the person\b",
        r"(?i)\bderivatives of$",
        r"(?i)^copyright in$",
        r"(?i)^copyright and other$",
        r"(?i)\bline and a pointer to where\b",
        r"(?i)\binterest in the program\b",
        r"(?i)\binterest in the library\b",
        r"(?i)\bhas no obligation to provide maintenance\b",
        r"(?i)^be liable to\b",
        r"(?i)\bthe respective terms and conditions\b",
        r"(?i)\bthe terms and conditions of the copyright\b",
        r"(?i)\bwho places the library\b",
        r"(?i)\bthe library among them\b",
        r"(?i)\bdisclaimer for the library\b",
        r"(?i)\bprofile authors\s+@remark",
        r"(?i)\banybody can make use of my programs\b",
        r"(?i)\bof computers and typesetting\b",
        r"(?i)^copyright the library,?$",
        r"(?i)^\(c\) endif$",
        r"(?i)^endif$",
        r"(?i)^\(c\) \?$",
        r"(?i)^\(c\) [a-z]$",
        r"(?i)^[a-z]$",
        r"(?i)^\(c\) [a-z] [a-z]$",
        r"(?i)^[a-z] [a-z]$",
        r"(?i)^\(c\) ISLOWER$",
        r"(?i)^ISLOWER$",
        r"(?i)^\(c\) - [a-z]$",
        r"(?i)^0$",
        r"(?i)^\(c\) 0$",
        // Keep year-only copyright lines: ScanCode reference fixtures expect these.
        r"(?i)^year\(\d{4}\)\.format\b",
        r"(?i)^SSY$",
        r"(?i)^Object$",
        // "Copyright Holder as/to/the" boilerplate
        r"(?i)^copyright holder as specified\b",
        r"(?i)^copyright holder to\b",
        r"(?i)^copyright holder,? the\b",
        r"(?i)^copyrights as noted\b",
        r"(?i)^COPYRIGHT DOCUMENTATION\b",
        r"(?i)^COPYRIGHT STATEMENTS\b",
        r"(?i)^copyright and other intellectual\b",
        r"(?i)^copyright treaties\b",
        // (c) followed by code-like constructs
        r"(?i)^\(c\) [\!\?&\|\.;:,\+\-\*/<>=]",
        r"(?i)^\(c\) [\w]+\.\w+\(",
        r"(?i)^\(c\) [\w]+\[",
        r"(?i)^\(c\) &&",
        r"(?i)^\(c\) \|\|",
        r"(?i)^\(c\) [\w]+\?",
        r"(?i)^\(c\)\s+[A-Za-z_][A-Za-z0-9_]*\s*\?\s*[A-Za-z0-9_()><=+\-*/&|]+\s*:\s*[A-Za-z0-9_()><=+\-*/&|]+\s*$",
        r"(?i)^\(c\) [\w]+\.[\w]+\.",
        r"(?i)^\(c\) [\w]+\([\w,]+\)",
        // (c) followed by short gibberish (1-3 mixed-case chars) from binary data
        r"^\(c\) [A-Z][a-z]{1,2}$",
        // (c) followed by "Unknown" (binary/PDF artifacts)
        r"(?i)^\(c\) Unknown\b",
        // (c) followed by binary/garbled data patterns
        r"^\(c\) [^\x20-\x7E]",
        r"^\(c\) [\x00-\x1F]",
        r"^\(c\) [A-Z][a-z]+ d [A-Z][a-z]+$",
        r"^\(c\) [A-Z]{2,}[0-9]",
        r"^\(c\) [a-z]{1,3}$",
        // (c) followed by C code patterns
        r"(?i)^\(c\) c -",
        r"(?i)^\(c\) c TOUPPER",
        r"(?i)^\(c\) isascii",
        r"(?i)^\(c\) isupper",
        r"(?i)^\(c\) isdigit",
        r"(?i)^\(c\) isalnum",
        r"(?i)^\(c\) isalpha",
        r"(?i)^\(c\) isspace",
        r"(?i)^\(c\) iscntrl",
        r"(?i)^\(c\) isprint",
        r"(?i)^\(c\) ifdef",
        r"(?i)^\(c\) undef\b",
        r"(?i)^\(c\) endif\b",
        r"(?i)^\(c\) sgn\b",
        r"(?i)^\(c\) dst",
        r"(?i)^\(c\) ptr\b",
        r"(?i)^\(c\) slen\b",
        r"(?i)^\(c\) len\b",
        r"(?i)^\(c\) do$",
        r"(?i)^\(c\) uint",
        r"(?i)^\(c\) gunichar\b",
        r"(?i)^\(c\) TRUE FALSE",
        r"(?i)^\(c\) yyunput\b",
        r"(?i)^\(c\) yylval\b",
        r"(?i)^\(c\) ungetc\b",
        r"(?i)^\(c\) 0x[0-9a-fA-F]",
        r"(?i)^\(c\) \(\(unsigned",
        r"(?i)^\(c\) \(int\)",
        r"(?i)^\(c\) \(uint",
        r"(?i)^\(c\) \(s\)",
        r"(?i)^\(c\) \d+ \(\(", // "(c) 16 ((d) 24)"
        r"(?i)^\(c\) \d+ &",    // "(c) 6 (trail&0x3f)"
        r"(?i)^\(c\)\s+[A-Za-z_][A-Za-z0-9_]*\s*(?:&|\||\^|>>|<<)\s*(?:0x[0-9A-Fa-f]+|\d+)\b",
        r"(?i)^\(c\)\s+[A-Za-z_][A-Za-z0-9_]*\s*(?:\|=|&=|\^=|>>=|<<=)\s*(?:0x[0-9A-Fa-f]+|\d+)\b",
        r"(?i)^\(c\) strict\b",
        r"(?i)^\(c\) width\b",
        r"(?i)^\(c\) arg\b",
        r"(?i)^\(c\) cindex\b",
        r"(?i)^\(c\) foot-",
        r"(?i)^\(c\) put\b",
        r"(?i)^\(c\) DEBUGP\b",
        r"(?i)^\(c\) Chain\b",
        r"(?i)^\(c\) Only\b",
        r"(?i)^\(c\) Walked\b",
        r"(?i)^\(c\) Construct\b",
        r"(?i)^\(c\) p can\b",
        r"(?i)^\(c\) c\.warn\b",
        r"(?i)^\(c\) b\.status\b",
        r"(?i)^\(c\) table\.set\b",
        r"(?i)^\(c\) in$",
        r"(?i)^\(c\) macro\b",
        r"(?i)^\(c\) decoded\b",
        r"(?i)^\(c\) IP_VS",
        r"(?i)^\(c\) Like\b",
        r"(?i)^\(c\) Page\b",
        r"(?i)^\(c\) WITH\b",
        r"(?i)^\(c\) \(1\b",
        r"(?i)^\(c\) \(2\)",
        r"(?i)^\(c\) \(MON\b",
        r"(?i)^\(c\) M this\b",
        r"(?i)^\(c\) \(0,",
        // (c) followed by PDF/PostScript artifacts
        r"(?i)^\(c\) Tj\b",
        r"(?i)^\(c\) ET\b",
        r"(?i)^\(c\) Registered$",
        // (c) followed by garbled/encoded text
        r"^\(c\) uL",
        r"^\(c\) [¡¢£¤¥¦§¨©ª«¬®¯°±²³´µ¶·¸¹º»¼½¾¿ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿ]",
        r"^\(c\) .*ÿÿÿ",
        r"^\(c\) .*°°°",
        // (c) followed by license/legal boilerplate
        r"(?i)^\(c\) Inclusion\b",
        r"(?i)^\(c\) Whenever\b",
        r"(?i)^\(c\) Customer",
        r"(?i)^\(c\) Splunk\b",
        r"(?i)^\(c\) No$",
        r"(?i)^\(c\) CockroachDB\b",
        r"(?i)^\(c\) Custom Nessus\b",
        r"(?i)^\(c\) Products\.",
        r"(?i)^\(c\) \u{201c}", // left double quotation mark
        // (c) followed by number-only patterns (not years)
        r"^\(c\) \d{1,2}$",
        r"^\(c\) \d+ \d+ y\b",
        // (c) followed by PostScript/font data
        r"(?i)^\(c\) SS'",
        r"(?i)^\(c\) PSPSY",
        r"(?i)^\(c\) PSY$",
        r"(?i)^\(c\) a! ",
        r"(?i)^\(c\) aae\b",
        r"(?i)^\(c\) \(r\)",
        r"(?i)^\(c\) D'O\b",
        r"(?i)^\(c\) AT r'b",
        r"(?i)^\(c\) C,BLACK",
        r"(?i)^\(c\) hUja\b",
        r"(?i)^\(c\) NULL$",
        r"(?i)^\(c\) cc\.fr",
        r"(?i)^\(c\) Oo2\b",
        r"(?i)^\(c\) UOSSOO",
        r"(?i)^\(c\) q ltd",
        r"(?i)^\(c\) zbar",
        r"(?i)^\(c\) distributed$",
        r"(?i)^\(c\) \(tm\)",
        r"(?i)^\(c\) ,\s*,",
        r"(?i)^\(c\) notice\b",
        r"(?i)^\(c\) create\b",
        r"(?i)^\(c\) do not\b",
        r"(?i)^\(c\) give\b",
        r"(?i)^copyright logo\b",
        r"(?i)^copyright targetpath\b",
        r"(?i)^copyright \(xmlns\b",
        r"(?i)^copyright its authors\b",
        r"(?i)^\(copyright\s*\)\b",
        r"(?i)^copyright the product\b",
        r"(?i)^copyright year\b.*\bfor\b",
        r"(?i)^copyrights? and licenses\b",
        r"(?i)^copyright applied to\b",
        r"(?i)^copyrighted material,\b",
        r"(?i)^copyright is\b",
        r"(?i)^\(c\) of the\b",
        r"(?i)^\(c\) other$",
        r"(?i)^\(c\) dates of\b",
        r"(?i)^\(c\) improved syntax\b",
        r"(?i)^\(c\),?\s*,",
        r"(?i)^\(c\),?\s*group\b",
        r"(?i)^\(c\),?\s*count\b",
        r"(?i)^\(c\),?\s*b\s",
        r"(?i)^\(c\),?\s*c$",
        r"(?i)^copyright act\b",
        r"(?i)^copyright for$",
        r"(?i)^copyright holder for the\b",
        r"(?i)^copyright man page\b",
        r"(?i)^copyright s status\b",
        r"(?i)^copyright and things like\b",
        r"(?i)^copyrights cover\b",
        r"(?i)^copyrights in the original\b",
        r"(?i)^copyrights in the portions\b",
        r"(?i)^copyrighted$",
        r"(?i)^copyright tue\b",
        r"(?i)^copyright sign\b",
        r"(?i)^c.opylefted\b",
        r"(?i)^i\.\s*\(c\)\b",
        r"(?i)^u1e\s*\(c\)\b",
        r"(?i)^xz\b.*\(c\)\b",
        r"^\(c\) [A-Z]{3,}[a-z]{1,3}$",
        r"^\(c\) [A-Z][a-z][A-Z][a-z]",
        r"^\(c\) [A-Z]{2}[a-z][A-Z]",
        r"^\(c\) [A-Z][A-Z][a-z][a-z][a-z]?[A-Z]",
        r"(?i)^copyright info have been\b",
        r"(?i)^\(copyright\s*\)\s*gnu general\b",
        r"(?i)^\(copyright\b.*\bvoltagefactor\b",
        r"(?i)^\(copyright unasserted\)\b",
        r"(?i)^copyright the lavantech\b",
        r"(?i)^copyright year united states\b",
        r"(?i)^copyright 1991-\d+ imatix\b.*\bwith exception\b",
        r"(?i)^\(c\) io\\0",
        r"(?i)^\(c\) ecfieldelement\b",
        r"(?i)^\(c\) distributed\b",
        r"(?i)^\(c\) yyyy\b",
        r"(?i)^\(c\) rebel\b",
        r"(?i)^\(c\) metastuff\b",
        r"(?i)^\(c\) mihai\b",
        r"(?i)^\(c\) linux foundation\b.*\bunified\b",
        r"(?i)^\(c\) helge deller\b.*\bcopyright\b",
        r"(?i)^\(c\) hewlett-packard company$",
        r"(?i)^copyright \(c\) david j\. bradshaw$",
        r"(?i)^copyright \(c\) tim ruffles$",
        r"(?i)^copyright \(c\) gias kay lee$",
        r"(?i)^copyright \(c\) xerox corporation$",
        r"(?i)^copyright -+\s*copyright\b",
        r"(?i)^copyright \u{fffd}",
        r"(?i)^\u{fffd}\d+-\d+\b",
        r"(?i)^copyright \u{a9}\d",
        r"(?i)^copyrighted material,? only\b",
        r"(?i)^copyrights of the\b",
        r"(?i)^\(c\) p b i n do$",
        r"(?i)^\(c\) 2004-2009 pudn\.com\b",
        r"(?i)^.{1,5}\s*\(c\)\s*.{1,5}$",
        r"(?i)^swfobject\b.*\bcopyright\b",
        r"(?i)^the the oscar\b",
        r"(?i)^\(c\) 2004-2010$",
        r"(?i)^\(c\) 1997 m\. kirkwood converted\b",
        r"(?i)^\(c\) 1998 red hat tcp\b",
        r"(?i)^\(c\) 1999 david airlie\b.*\bbugfixes\b",
        r"(?i)^\(c\) 1998-2002 by heiko eissfeldt\b",
        r"(?i)^\(c\) 2001 dave jones\b",
        r"(?i)^\(c\) 2003-2004 paul clements\b",
        r"(?i)^\(c\) 2014-\$$",
        r"(?i)^copyright 2014-\$$",
        r"(?i)^copyright 2010 ben dooks fluff\b",
        r"(?i)^\(c\)\s*(indir|then|unacceptable)\b",
        r"(?i)^\(c\) c arg\b",
        r"(?i)^\(c\) @ ?(symrec|ungetc|yylval)\b",
        r"(?i)^\(c\) \(the parens\b",
        r"(?i)^\(c\) s-\d",
        r"(?i)^\(c\) register\b",
        r"(?i)^\(c\) Mouse Wheel\b",
        r"(?i)^copyright info$",
        r"(?i)^copyright for a\b",
        r"(?i)^COPYRIGHT HOLDERS AS\b",
        r"(?i)@remark Read",
        r"(?i)\bContact <\w+@\w+",
        r"(?i)^\d{1,2}$",
        r"(?i)^\(c\) yyunput\b",
        r"(?i)^\(c\) yylval\b",
        r"(?i)^IsLower\s*\(c\)\s*IsDigit\b",
        r"(?i)^copyright \d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}\b",
        r"(?i)^Copyright \(c\) \d{4} Contributors$",
        r"(?i)^ds Status works\b",
        r"(?i)^Copyright \(c\) The team$",
        r"(?i)^holder\.\s*AS\b",
        r"(?i)^as\(c,\s*field\b",
        r"(?i)^skb\.\s*The buffer\b",
        r"(?i)^partial mlock\b",
        r"(?i)^\(c\) \(c\) \(c\) SSY",
        r"(?i)^\(c\) \(c\) 2AICAA",
        r"(?i)^\(c\) \d{4} \$$",
        r"(?i)^\(c\) \d{4} - Rear\b",
        r"(?i)^copyrights?,? to$",
        r"(?i)^Copyright \(c\) \d{4} kavol$",
        // (c) followed by C variable/type patterns
        r"(?i)^\(c\) (unsigned|int|char|void|long|short|float|double|static|struct)\b",
        r"(?i)^\(c\) (classify|ctable|cvPoint|fWidth|macroptr|MAGIC)\b",
        r"(?i)^\(c\) (letters|ok letters)\b",
        r"(?i)^\(c\) (res|ret|run|save|sizeof|temp)\b",
        r"(?i)^\(c\) (flags|buffer|buflen)\b",
        r"(?i)^\(c\) (pr |prec |printf )\b",
        r"(?i)^\(c\) (Accumulate|Bit8u|Returns the|SkReplicate)\b",
        r"(?i)^\(c\) (asm|xlp|we copy)\b",
        r"(?i)^\(c\) (d \d|num \d|mat \d)\b",
        r"(?i)^\(c\) do (prec|while)\b",
        r"(?i)^\(c\) etc\b",
        r"(?i)^\(c\) Finn Thain\b.*\bCopying\b",
        r"(?i)^\(c\) Kasım\b",
        r"(?i)^\(c\) z \?$",
        r"(?i)^\(c\) Z \?$",
        r"(?i)^\(c\) \d+ \+0x",
        r"(?i)^\(c\) \d+ static\b",
        r"(?i)^\(c\) 0-9",
        r"(?i)^\(c\) 122$",
        r"(?i)^\(c\) \d+ endif\b",
        r"(?i)^\(c\) \(c&",
        r"(?i)^\(c\) \(cp\)",
        r"(?i)^\(c\) \( cp\)",
        r"(?i)^\(c\)\s*\(\s*(?:const\s+)?(?:signed\s+|unsigned\s+)?(?:char|short|int|long|float|double|void|size_t|ssize_t|uintptr_t|intptr_t|u?int(?:8|16|32|64)_t)\s*(?:\*+\s*)?\)\s*[A-Za-z_(]",
        r"(?i)^\(c\) \(l\)",
        r"(?i)^\(c\) \(out\.\b",
        r"(?i)^\(c\) \(run\)",
        r"(?i)^\(c\) \(scale\b",
        r"(?i)^\(c\) \^ \(",
        r"(?i)^\(c\) \(DBus",
        r"(?i)^\(c\) c c c\b",
        r"(?i)^\(c\) c toascii\b",
        r"(?i)^\(c\) c tolower\b",
        r"(?i)^\(c\) c \(qbuf\b",
        r"(?i)^\(c\) c / endif\b",
        r"(?i)^\(c\) c \^ 0x",
        r"(?i)^\(c\) c 03o\b",
        r"(?i)^\(c\) c 0x\d",
        r"(?i)^\(c\) this-\b",
        r"(?i)^\(c\) putchar\b",
        // (c) followed by year + trailing junk
        r"(?i)^\(c\) \d{4}(-\d{4})? Jean-loup Gailly\b.*\b(END|VALUE)\b",
        r"(?i)^\(c\) \d{4}(-\d{4})? Julian Seward\b.*\btitle\b",
        r"(?i)^\(c\) \d{4} Paul Rusty Russell\b.*\bPlaced\b",
        r"(?i)^\(c\) \d{4} Dan Potter\b.*\bmodify\b",
        r"(?i)^\(c\) \d{4} Red Hat\.\s*GPLd\b",
        r"(?i)^\(c\) \d{4}-\d{4}$",
        r"(?i)^\(c\) \d{4} Andreas Gruenbacher\b.*\bgruenbacher@\b",
        r"(?i)^\(c\) \d{4},?\s*\d{4},?\s*\d{4} Thomas Vander Stichele\b",
        r"(?i)^\(c\) \d{4} Adam Nielsen\b.*\bniel?sen@\b",
        r"(?i)^\(c\) \d+ \(trail",
        r"(?i)^\(c\) 4\+\(r\)",
        // copyright followed by non-copyright text
        r"(?i)^copyright :G2P\b",
        r"(?i)^copyright \d+ trademark\b",
        r"(?i)^copyright 60$",
        r"(?i)^copyright ACM and IEEE\b",
        r"(?i)^copyright and mit$",
        r"(?i)^copyright and placed into\b",
        r"(?i)^copyright and to distribute\b",
        r"(?i)^copyright as follows\b",
        r"(?i)^copyright definedummyword\b",
        r"(?i)^copyright FILE\b",
        r"(?i)^copyright info to be\b",
        r"(?i)^copyright mea-\b",
        r"(?i)^copyright meta-\b",
        r"(?i)^copyright others$",
        r"(?i)^copyright problem,?\b",
        r"(?i)^copyright SGI\b",
        r"(?i)^copyright to help\b",
        r"(?i)^copyright year to\b",
        r"(?i)^copyrighted - provided\b",
        r"(?i)^copyrighted by the following\b",
        r"(?i)^copyrighted software$",
        r"(?i)^copyrighted work\b",
        r"(?i)^copyrights apply\b",
        r"(?i)^copyrights to use\b",
        // Non-copyright holder-like strings that are false positives
        r"(?i)^count count\b",
        r"(?i)^const char\b",
        r"(?i)^int\s",
        r"(?i)^int$",
        r"(?i)^lack of warranty\b",
        r"(?i)^macro for checking\b",
        r"(?i)^mat \d\b",
        r"(?i)^MD5Update\b",
        r"(?i)^message$",
        r"(?i)^Nuance Communications,? but\b",
        r"(?i)^NULL,? \d",
        r"(?i)^placed into PD\b",
        r"(?i)^preserved in its entirety\b",
        r"(?i)^Protocol Engineering Lab\b",
        r"(?i)^ptr$",
        r"(?i)^Regents of the University\b.*\bBerkeley Software\b",
        r"(?i)^res$",
        r"(?i)^ret$",
        r"(?i)^run$",
        r"(?i)^sgn$",
        r"(?i)^sizeof$",
        r"(?i)^SIGN\(b\)",
        r"(?i)^strict forbid\b",
        r"(?i)^terms and conditions$",
        r"(?i)^toascii$",
        r"(?i)^tolower$",
        r"(?i)^trademark acute\b",
        r"(?i)^TRADEMARK \d+NOTICES\b",
        r"(?i)^true$",
        r"(?i)^unacceptable$",
        r"(?i)^unsigned\s+(char|int|long|short|b|g|r|sb|sg)\b",
        r"(?i)^we copy data\b",
        r"(?i)^work$",
        r"(?i)^wide$",
        r"(?i)^joint with$",
        r"^others$",
        r"(?i)^symbol,? for example\b",
        r"(?i)^the shared library will be\b",
        r"(?i)^SkReplicateNibble\b",
        r"(?i)^Returns the (multiplicative|product)\b",
        r"(?i)^Walked too far\b",
        r"(?i)^xlp xep\b",
        r"(?i)^yyunput\b",
        r"(?i)^yylval\b",
        r"(?i)^\?1:0$",
        r"(?i)^\(\(DBus",
        r"(?i)^\(unsigned char\)",
        r"(?i)^16 \(\(d\)\b",
        r"(?i)^l \(unsigned\b",
        r"(?i)^\(\(unsigned\b",
        // ICS false positive copyrights
        r"(?i)^\(c\) \(unsigned int\)",
        r"(?i)^\(c\) A &&",
        r"(?i)^\(c\) a &&",
        r"(?i)^COPYRIGHT undef\b",
        r"(?i)^\(c\) \(\(DBusCondVar",
        r"(?i)^\(c\) s-$",
        r"(?i)^\(c\) A1$",
        r"(?i)^\(c\) this-\s*set\w+\b",
        r"(?i)^\(c\) \(unsigned\)$",
        r"(?i)^COPYRIGHT CREDITS\b",
        r"(?i)^COPYRIGHT HOLDERS,?\s*AND/OR\b",
        r"(?i)^COPYRIGHT exploring\b",
        r"(?i)^Copyright,?\s*lack of warranty\b",
        r"(?i)^COPYRIGHT const char\b",
        r"(?i)^copyright const char\b",
        r"(?i)^copyright mea-\s*setOffset\b",
        r"(?i)^copyright meta-\s*registerClass\b",
        r"(?i)^\(c\) \(unsigned char\)\(",
        r"(?i)^\(c\) \d+L$",
        r"(?i)^\(c\) cvPoint3D32f$",
        r"(?i)^\(c\) temp3$",
        r"(?i)^http://\S+\s+Copyright\b",
        r"(?i)^Foundation Copyright\b",
        r"(?i)^http://sizzlejs\b",
        r"(?i)^\(c\) \(unsigned char\)$",
        r"(?i)@remark Read",
        r"(?i)\bWritten by\b",
        r"(?i)\bcontributors Thomas Broyer\b",
        r"(?i), and are$",
        // Garbled/binary data patterns (junk-copyright-* tests)
        r"^\(c\) Io\\0",
        r"^\(c\) AaeaMOOAA\d",
        r"^\(c\) EEIaeIaAAOAE",
        r"^\(c\) AaACEEeUB",
        r"^\(c\) AIuaey",
        r"^\(c\) ATo\b",
        r"^\(c\) U Q\d",
        r"^\(c\) Vo\b.*\bAoa\b",
        r"^\(c\) Y Rd$",
        r"^\(c\) YY ThQ",
        r"^\(c\) ZIgd\d",
        r"^\(c\) OCOthDTh",
        r"^\(c\) IoUOi",
        r"^\(c\) OthO$",
        r"^\(c\) ErXA\d",
        r"(?i)^\(c\) Dean$",
        r"(?i)^Copyright \(c\) The team$",
        r"^\(c\) 1 \?\d",
        r"^\(c\) 34 b$",
        r"^\(c\) A - 10 a - 10$",
        r"(?i)^\(c\) AS z$",
        // French legal text fragments
        r"(?i)^\(c\) dig[ÃA]",
        r"(?i)^\(c\) que le pr[ÃA]",
        r"(?i)^\(c\) s en anglais",
        r"(?i)^\(c\) sent contrat\b",
        // Garbled text with (c) in middle
        r"(?i)^Xz\b.*\(c\)\s*Ijr",
        // Binary data from image files
        r"^\(c\) [^\x20-\x7e]{2}",
        r"(?i)^COPYRIGHT AS$",
        r"^\(c\) E QuGU",
        r"^\(c\) YY$",
        // (c) followed by non-ASCII byte (binary garbage from image/font files)
        r"^\(c\) [a-zA-Z]{1,3}[\x00-\x1f\x80-\xff]",
        r"[\x00-\x08]",
        r"(?i)^copyright\s+\d{4}\s+www\.\S+",
        r"-{5,}",
        r"(?i)\bjornada\s+\d+$",
        r"(?i)^BSD\s+\d+\s+clause\b",
        // Dollar-sign placeholder year (e.g. "Copyright (c) 2011 $ new", "Copyright 2014 $")
        r"(?i)^copyright\s*(?:\(c\)\s*)?(?:19|20)\d{2}\s*\$",
        // Product names with (c) in the middle (not a real copyright)
        r"(?i)^Creative\s+Card\s+\d",
        // "(c)" after "marked" — product description, not a copyright
        r"(?i)\bmarked\s+\(c\)",
        // Specific junk from copyrights-to-fix.txt (Python reference has this exact pattern)
        r"(?i)^Copyright\s+\(c\)\s+2021\s+Dot\b",
        r"(?i)^copyright\s+\d{4}\s+[a-z][a-z0-9_-]+\s+[a-z][a-z0-9_-]+@\S+",
        r"(?i)^\(c\)\s*(?:19|20)\d{2}\s+@author\b.*$",
        r"(?i)^copyrights?$",
        r"(?i)^copyrights?,\s*licenses?,\s+and/or\s*$",
        r"(?i)^copyrighted,?\s+but\s+its\s+distribution\b.*$",
        r"(?i)^COPYRIGHT\s+(?:NOTICE|HOLDER|STATEMENT|OWNER|INFORMATION|HEADER|BLOCK|TEXT|YEAR|DATE|NAME|SYMBOL|SIGN|MARK|TAG|LABEL|LINE|SECTION|CLAUSE|TERMS|POLICY|LAWS?|RULES?|RIGHTS?|LAWS?)\s*$",
        r"^COPYRIGHT\s+[A-Z0-9]{2,}\s*$",
        r"(?i)\bcopyright\b.*@var\b",
        r"(?i)\$timestamp\b",
        r"(?i)^copyrights?,\s*patents?\b",
        r"(?i)^copyright\s*,\s*(?:version|etc)\b",
        r"(?i)^not\s+copyrighted\s*[-–]\s*provided\s+to\s+the\s+public\s+domain\b",
        r"(?i)^copyright\s+and\s+related\s+rights\b",
        r"(?i)^copyright\s+and\s+related\s+or\s+neighboring\s+rights$",
        r"(?i)^copyright\s+was\s+owned\s+solely\s+by\s+fsf$",
        r"(?i)^copyright\s+years\s+may\s+be\s+listed$",
        r"(?i)^\(c\)\s+final\s+[a-z_][a-zA-Z0-9_]*$",
    ];
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
});

/// Regex patterns for junk holder detections (license boilerplate fragments).
static HOLDERS_JUNK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let patterns = [
        r"(?i)^licenses?,\s+and/or\b",
        r"(?i)^holders?,\s*authors\b",
        r"(?i)^notice,\s+and\b",
        r"(?i)^notice,\s+in\b",
        r"(?i)^but\s+its\s+distribution\b",
        r"(?i)\bprovided\s+to\s+the\s+public\s+domain\b",
        r"(?i)^patent\s+or\s+trademark\b",
        r"(?i)^notice,\s+but\s+the\s+BSD,\s+MIT\s+and\s+UoI/NCSA\s+licenses\s+do\s+not\b",
        r"(?i)\bVALUE\s+OriginalFilename\b",
        r"(?i)\bOriginalFilename\b",
        r"(?i)\bEND\s*$",
        r"(?i)^etc\.?\s+in\b",
        r"(?i)^version\s*,\s*etc\b",
        r"(?i)^\(d\),\s*\d+(?:\.\d+)*\.?$",
        r"(?i)\bliable for\b",
        r"(?i)\bappear in all copies\b",
        r"(?i)\bdisclaimer of warranty\b",
        r"(?i)\bdisclaimer for the program\b",
        r"(?i)\bit may be distributed\b",
        r"(?i)\bwho places the program\b",
        r"(?i)\bkeep intact all the\b",
        r"(?i)\bshall not be used in advertising\b",
        r"(?i)\bpromote the sale\b",
        r"(?i)\bpromote products derived\b",
        r"(?i)\bother dealings in\b",
        r"(?i)\bhas been advised of the possibility\b",
        r"(?i)\bfailure of essential purpose\b",
        r"(?i)\bthe licenses? granted in\b",
        r"(?i)\bcovering the original code\b",
        r"(?i)\bwithout notice from apple\b",
        r"(?i)\bcompletely and accurately document\b",
        r"(?i)\bother proprietary\b",
        r"(?i)\bpatent rights?\b",
        r"(?i)^patents?\s*,\s*trade secrets?\b",
        r"(?i)^patent\s*,\s*or trademark\b",
        r"(?i)^patent\s*,\s*trademark\b",
        r"(?i)^trade secrets?\b",
        r"(?i)^including\s+without\s+limitation\b",
        r"(?i)^trademarks?$",
        r"(?i)\bincluding.{0,10}but not limited\b",
        r"(?i)\bincluding your\b",
        r"(?i)\bincluding the\b",
        r"(?i)\bcopyrighted material\b",
        r"(?i)\bselected patent\b",
        r"(?i)\bin the work\b",
        r"(?i)\bin the document\b",
        r"(?i)\bthe original work\b",
        r"(?i)\bpermit and encourage\b",
        r"(?i)\bpermitted copying\b",
        r"(?i)\bto do the following\b",
        r"(?i)\bas a result of\b",
        r"(?i)\breinstated permanently\b",
        r"(?i)\breinstated\b",
        r"(?i)\bexplicitly and finally terminates\b",
        r"(?i)\bfails to notify\b",
        r"(?i)\bnotifies\b",
        r"(?i)\bthe above\b",
        r"(?i)\bthe software,?$",
        r"(?i)^software$",
        r"(?i)\bsuspend your rights\b",
        r"(?i)\bderivative works\b",
        r"(?i)\bpublicly display\b",
        r"(?i)\bpublicly perform\b",
        r"(?i)\bof competent jurisdiction\b",
        r"(?i)\bexceptions and limitations\b",
        r"(?i)\bfair use\b",
        r"(?i)\bfair dealing\b",
        r"(?i)\btreaty adopted\b",
        r"(?i)\breflecting the\b",
        r"(?i)^\d{1,2}:\d{2}\s+[a-z0-9][a-z0-9._-]*$",
        r"(?i)\bappears? in\b",
        r"(?i)\bsaying\b.*\bdistributed\b",
        r"(?i)\bif the item a binary\b",
        r"(?i)\bone digital image or graphic\b",
        r"(?i)\bperceptible, measurable\b",
        r"(?i)\bthe entire\b",
        r"(?i)\bsemblance of artistic control\b",
        r"(?i)\bcommercially reasonable efforts\b",
        r"(?i)\bto endorse or promote\b",
        r"(?i)\bimmediately at the beginning\b",
        r"(?i)\bunmodified\b",
        r"(?i)\beasier identification\b",
        r"(?i)\b(l?gpl|lgpl) group\b",
        r"(?i)^symbol in\b",
        r"(?i)^trademark$",
        r"(?i)^printf\b",
        r"(?i)^the top level of\b",
        r"(?i)^the following\b",
        r"(?i)^the resulting\b",
        r"(?i)^whoever named in\b",
        r"(?i)^as specified below\b",
        r"(?i)^\((?:r|c|tm)\)\s*,\s*(?:&\s*\d{2,4}\s*,\s*){5,}.*$",
        r"(?i)^not used to limit\b",
        r"(?i)^the coordinator$",
        r"(?i)^provided\b",
        r"(?i)^provides the work\b",
        r"(?i)^final\s+[a-z_][a-zA-Z0-9_]*$",
        r"(?i)\bthis\.[a-zA-Z]",
        r"(?i):function\b",
        r"^(?:[a-z_][a-zA-Z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*\([^)]*\)|[a-z_][a-zA-Z0-9_]*\.[A-Za-z_]*[A-Z][A-Za-z0-9_]*)$",
        r"(?i)\bm\. y\. name\b",
        r"(?i)^version of nameif\b",
        r"(?i)\bunless explicitly identified\b",
        r"(?i)^version 3 of the$",
        // Holder false positives from license boilerplate
        r"(?i)\b(if any) with\b",
        r"(?i)^(d),\b",
        r"(?i)\bas a market\b",
        r"(?i)\bprocedures\b",
        r"(?i)\bcollectively\b",
        r"(?i)\bgiving your\b",
        r"(?i)\bspecified addresses\b",
        r"(?i)^the base\b",
        r"(?i)^the library\b",
        r"(?i)\bthe library,\b",
        r"(?i)\bthe library among\b",
        r"(?i)\breferences to\b",
        r"(?i)\bstating\b.*\bdistributed\b",
        r"(?i)^terminate\b",
        r"(?i)\beffective immediately\b",
        r"(?i)^keep intact\b",
        r"(?i)^material outside\b",
        r"(?i)\bsaying\b",
        // Trailing legal text patterns
        r"(?i)\bdistributed under\b",
        r"(?i)\blicensed under\b",
        r"(?i)\bthe terms\b.*\blicense\b",
        r"(?i)\bthe standard version\b",
        // Code-like patterns in holders
        r"(?i)\bif\s*\(",
        r"(?i)\bfunction\s*\(",
        r"(?i)\breturn\b.*\bfunction\b",
        r"(?i)\bvar\s+\w",
        r"(?i)\bthis\.\w+\(",
        // Trailing text patterns in holders
        r"(?i)\bCredited\b",
        r"(?i)\bConverted to\b",
        r"(?i)\breworked by\b",
        r"(?i)\bVarious bits\b",
        r"(?i)\bCopying and distribution\b",
        r"(?i)\bGPLd\b",
        r"(?i)\bLicense-Alias\b",
        r"(?i)\bWritten by\b",
        r"(?i)\bModified by the\b",
        r"(?i)\btitle Legal\b",
        r"(?i)\bContact <",
        r"(?i)\b- Placed\b",
        r"(?i)\bUnder the terms\b",
        r"(?i)\binfo have been\b",
        r"(?i)\bicon support\b",
        r"(?i)\bfull list\b",
        r"(?i)^proprietary$",
        r"(?i)^not limited to\b",
        r"(?i)\bprocurement of substitute goods or services\b",
        r"(?i)^related rights\b",
        r"(?i)^related or neighboring\b",
        r"(?i)^was owned solely by fsf$",
        r"(?i)^years may be listed$",
        r"(?i)^notice\s*,\s*license\s+and\s+disclaimer\.?$",
        r"(?i)^trademarks?\s*,\s*trade\s+secrets?\b",
        r"(?i)^the\s+standard$",
        r"(?i)^the\s+product$",
        r"(?i)^rsa$",
        r"(?i)^xerox corporation$",
        r"(?i)^david j\. bradshaw$",
        r"(?i)^gias kay lee$",
        r"(?i)^tim ruffles$",
        r"(?i)^products\.\s+tenable licenses$",
        r"(?i)^the Embedded Configurable Operating System\.?$",
        r"(?i)^(?:convert|multiply)\s+(?:a\s+)?(?:chebyshev|hermite|laguerre|legendre)\b",
        r"(?i)^treaties$",
        r"(?i)\bpatent\s+or\s+other\s+licenses\s+necessary\b",
        r"(?i)\bMODULEAUTHOR\b",
        r"(?i)^THE\s+[\p{L}0-9._-]+'S\b",
        r"(?i)^the\s+(?:[\p{L}0-9._-]+\s+){1,5}operating\s+system\.?$",
        r"(?i)\bpkg\.(author|homepage)\b",
        r"(?i)\bdate\.year\b",
        r"(?i)\bYYYY-MM-DD\b",
        r"(?i)<\s*pkg\.[a-zA-Z0-9_.-]+\s*>",
        r"(?i)^http://www\.quirksmode\b",
        r"[→⟶]",
        r"(?i)\bSTATEMENTS AND\b",
        r"(?i)\bAS IS$",
        r"(?i)\bAS IS CONDITION\b",
        r"(?i)\bNOTICES OR THIS\b",
        r"(?i)\bDOCUMENTATION ISC\b",
        r"(?i)\bpixmaps svg\b",
        r"(?i)\bFull text of\b",
        r"(?i)\btransferred to Nokia\b",
        r"(?i)\bAS PER APPLICABLE\b",
        r"(?i)\bSection 105\b",
        r"(?i)\bGNU AGPL\b",
        r"(?i)\bagreement with the\b",
        r"(?i)\bgives Customer\b",
        r"(?i)\bshall mean\b",
        r"(?i)\bEnterprise Edition\b",
        r"(?i)\bContributing Authors\b",
        r"(?i)\bAll Downstream\b",
        r"(?i)\bSource Code to\b",
        r"(?i)\bPROTECTION AND IS\b",
        r"(?i)\bnot removed\b",
        r"(?i)\bversion 3\.1 of\b",
        r"(?i)\bGPL version\b",
        r"(?i)\bCopyright/g\b",
        r"(?i)\bdata/c\.m4\b",
        r"(?i)\binside so it\b",
        r"(?i)\bmatch standard format\b",
        r"(?i)\bin each output\b",
        r"(?i)\bstr::npos\b",
        r"(?i)\btimes in xrange\b",
        r"(?i)\bin zlib\.h\b",
        r"(?i)\ball paragraphs\b",
        r"(?i)\buse, copy, modify\b",
        r"(?i)\bdistribute it with\b",
        r"(?i)\bother intellectual property\b",
        r"(?i)\btreaties\. Title\b",
        r"(?i)\bexempting the\b",
        r"(?i)\bwith exception of\b",
        r"(?i)\bas noted in the\b",
        r"(?i)\bThe Product is\b",
        r"(?i)\bThe arguments as\b",
        r"(?i)\bpertaining to distribution\b",
        r"(?i)\bVERBATIM\b",
        r"(?i)\bintact$",
        r"(?i)\binformation\.\b",
        r"(?i)\bdoing$",
        r"(?i)^holders,? but\b",
        r"(?i)^its author\b",
        r"(?i)^in its\b",
        r"(?i)^in the\b",
        r"(?i)^offer\b",
        r"(?i)^copy the\b",
        r"(?i)^owned by\b",
        r"(?i)^the team$",
        r"(?i)^the project$",
        r"(?i)^the republic of\b",
        r"(?i)^the google\b",
        r"(?i)^the jetty\b",
        r"(?i)^the acknowledgment\b",
        r"(?i)^the combination of\b",
        r"(?i)^the lavantech\b",
        r"(?i)^all source code\b",
        r"(?i)^all translated\b",
        r"(?i)^all the rich\b",
        r"(?i)^author,? or contributor\b",
        r"(?i)^authors,? and contributors\b",
        r"(?i)^its authors\b",
        r"(?i)^its cell\b",
        r"(?i)^automatically without\b",
        r"(?i)^more information\b",
        r"(?i)^infringement can\b",
        r"(?i)^header of\b",
        r"(?i)^const (group|projects)\b",
        r"(?i)^there clear\b",
        r"(?i)^things like\b",
        r"(?i)^custom nessus\b",
        r"(?i)^whenever reasonably\b",
        r"(?i)^gnu general\b",
        r"(?i)^general public\b",
        r"(?i)^man page\b",
        r"(?i)^merged arm\b",
        r"(?i)^tcl/tk policy\b",
        r"(?i)^in license\b",
        r"(?i)^law,? \b",
        r"(?i)^license,? to the\b",
        r"(?i)^s status\b",
        r"(?i)^as i developed\b",
        r"(?i)^improved syntax\b",
        r"(?i)^inclusion in\b",
        r"(?i)^disclaim all\b",
        r"(?i)^directly copied\b",
        r"(?i)^as found in\b",
        r"(?i)^years updated\b",
        r"(?i)\bcontrol over the development\b",
        r"(?i)\bartistic control\b",
        r"(?i)\bcompilation not used to limit\b",
        r"(?i)\blegal rights of the compilation\b",
        r"(?i)\bindividual works permit\b",
        r"(?i)\bDocument included in\b",
        r"(?i)\blocated in .* and .* located in\b",
        r"(?i)\binternational treaty\b",
        r"(?i)\bapplicable$",
        r"(?i)\bcontrat et tous\b",
        r"(?i)\ben anglais\b",
        r"(?i)\bdocuments connexes\b",
        r"(?i)^seek a different\b",
        r"(?i)^sign so\b",
        r"(?i)^like sta\b",
        r"(?i)^page i/o\b",
        r"(?i)^\(mon tue\b",
        r"(?i)^gt\. zero\b",
        r"(?i)^with recursive\b",
        r"(?i)^ecfieldelement\b",
        r"(?i)^setresultsname\b",
        r"(?i)^semanticdirection\b",
        r"(?i)^content ssense\b",
        r"(?i)^attr value\b",
        r"(?i)^match\(ident\)\b",
        r"(?i)^assert\.equal\b",
        r"(?i)^h\.matches\b",
        r"(?i)^bd\(b\.\b",
        r"(?i)^b\(an\)\d",
        r"(?i)^b\(ase\b",
        r"(?i)^b\(onstant\b",
        r"(?i)^g\(al\)\b",
        r"(?i)^y fj\b",
        r"(?i)^y fp\b",
        r"(?i)^u r\(\d",
        r"(?i)^u q\d",
        r"(?i)^y rd\b",
        r"(?i)^y aey\b",
        r"(?i)^as z$",
        r"(?i)^i\. uao\b",
        r"(?i)^e qugu\b",
        r"(?i)^bj d\b",
        r"(?i)^cj d\b",
        r"(?i)^dj d\b",
        r"(?i)^jj d\b",
        r"(?i)^objc,? bp\b",
        r"(?i)^10 a - 10$",
        r"(?i)^b a, b$",
        r"(?i)^unknown [a-z]{1,3}$",
        r"(?i)^unknown [a-z]\d\b",
        r"^[a-z]{1,2} [a-z]{1,2}$",
        r"^[A-Z][a-z] [A-Z]$",
        r"^[a-z][A-Z] [A-Z]{1,2}$",
        r"(?i)^ato\b.*\bae\b",
        r"(?i)^xz\b.*\bijr\b",
        r"(?i)^zigd\d\b",
        r"(?i)^yy thq\b",
        r"(?i)^ss'ss",
        r"(?i)^pspsy\b",
        r"(?i)^oo2\b",
        r"(?i)^c/ps\b",
        r"(?i)^cn:class\b",
        r"(?i)^c2001\b",
        r"(?i)^ocoo\b",
        r"(?i)^a!\b",
        r"(?i)^aae\b",
        r"(?i)^a\(r\)\b",
        r"(?i)^deg,?\b.*deg\b",
        r"(?i)^cii1/4\b",
        r"(?i)^vo u\d",
        r"(?i)^ul\b",
        r"(?i)^xl\b",
        r"(?i)^wl\b",
        r"(?i)^crarr\b",
        r"(?i)^x\$\?\b",
        r"(?i)^e\$\?\b",
        r"(?i)^length\?null\b",
        r"(?i)^c\.warn\b",
        r"(?i)^b\.status\b",
        r"(?i)^as\(c,\b",
        r"(?i)^cc\.fr$",
        r"(?i)^q ltd$",
        r"(?i)^zbar\b",
        r"(?i)^ssssy$",
        r"(?i)^ssss$",
        r"(?i)^as5$",
        r"(?i)^r'b$",
        r"(?i)^\?12$",
        r"(?i)^tj et\b",
        r"(?i)^adobe.*\bairtm\b",
        r"(?i)^adobe.*\bair\u{2122}\b",
        r"[\x00-\x1f]",
        r"°°°",
        r"ÿÿÿ",
        r"\u{9a}f",
        r"\u{96}b",
        r"\u{9d}v",
        r"^[A-Z][a-z]$",
        r"^[A-Z][b-z]$",
        r"^[a-z][A-Z]$",
        r"^holder\.\b",
        r"^holder,\b",
        r"^holders,\b",
        r"^holder as\b",
        r"(?i)^applied to\b",
        r"(?i)^designation\b",
        r"(?i)^registered$",
        r"(?i)^component$",
        r"(?i)^count$",
        r"(?i)^group$",
        r"(?i)^isupper$",
        r"(?i)^folded$",
        r"(?i)^dean$",
        r"(?i)^targetpath$",
        r"(?i)^libre-software$",
        r"(?i)^\(2\)\.\s*if\b",
        r"(?i)^\(as found in\b",
        r"(?i)^\(directly copied\b",
        r"(?i)^\(if any\)\b",
        r"(?i)^m\(h",
        r"(?i)^b\(onsisting\b",
        r"(?i)^inria-enpc\b",
        r"(?i)^uossoo\b",
        r"(?i)^ocothd\b",
        r"(?i)^otho\b",
        r"(?i)^iouoi\b",
        r"(?i)^aiuaey\b",
        r"(?i)^aoth\b",
        r"(?i)^ato\b",
        r"(?i)^aaeamooa\b",
        r"(?i)^eeiaeiaaoa\b",
        r"(?i)^exauauuao\b",
        r"(?i)^erxa\d",
        r"(?i)^ijax\b",
        r"(?i)^u1e\b",
        r"(?i)^degu\b",
        r"(?i)^xmlns\b",
        r"(?i)^\u{201c}adobe\b",
        r"(?i)\bthe resulting\b",
        r"(?i)\ball source code included in\b",
        r"(?i)\bsource code distributed need not\b",
        r"(?i)\bdo not make\b",
        r"(?i)\bgive all recipients\b",
        r"(?i)\brecipients of the\b",
        r"(?i)\bmay be used\b",
        r"(?i)\bthe accompanying\b",
        r"(?i)\bas represented by no\b",
        r"(?i)^compute hessian\b",
        r"(?i)^nat\d+\.is",
        r"(?i)^a\.compatibility\b",
        r"(?i)^retained at the\b",
        r"(?i)^timer code\b",
        r"(?i)^an sr-iov\b",
        r"(?i)^applies to the regex\b",
        r"(?i)^apple's sf pro\b",
        r"(?i)^xmlns:\?",
        r"(?i)^swfobject\b",
        r"(?i)^program\b.*\btalke studio\b",
        r"(?i)^footer$",
        r"(?i)^legal\s+notice(?:\s+div\s+div\.legalnotice\s+p)?$",
        r"(?i)^color\s+666666$",
        r"(?i)^debian\b.*\bjames troup\b",
        r"(?i)^\$id\$\b",
        r"(?i)^the uc berkeley\b",
        r"(?i)^ococoa\b",
        r"(?i)^grant\. i\b",
        r"(?i)^the gnome libraries\b",
        r"(?i)^as is group\b",
        r"(?i)^match\(ident\)\s*ast\b",
        r"(?i)^holder,? author,? or contributor\b",
        r"(?i)^holders,? authors,? and contributors\b",
        r"(?i)\bportions of\b",
        r"(?i)\bsome parts of\b",
        r"(?i)\bthe source$",
        r"(?i)\bthe source code\b",
        r"(?i)\b\. the source\b",
        r"(?i)^p b i n do$",
        r"(?i)^tue \w+ \d+ \w+ \w+ -",
        r"(?i)^info for$",
        r"(?i)^material,? only\b",
        r"(?i)^(d),? \d",
        r"(?i)^c\. schmidt$",
        r"(?i)^gdb free software\b",
        r"(?i)^va$",
        r"(?i)^wing$",
        r"(?i)^hillion$",
        r"(?i)^(TOUPPER|isascii|iscntrl|isprint|yyunput|ambiguous|TRUE FALSE)$",
        r"(?i)^(width|len|do|date|year|note|update|notive|all the)$",
        r"(?i)^undef\s+\w+$",
        r"(?i)^i\.e\.,\b",
        r"(?i)^endif\b",
        r"(?i)^definedummyword\b",
        r"(?i)^register int\b",
        r"(?i)^l \(unsigned\b",
        r"(?i)^\(\(unsigned\b",
        r"(?i)^notices all\b",
        r"(?i)^may not be removed\b",
        r"(?i)^duplicated in\b",
        r"(?i)^copyright for a\b",
        r"(?i)^copyright info\b",
        r"(?i)^COPYRIGHT HOLDERS AS\b",
        r"(?i)^Mouse Wheel Support\b",
        r"(?i)^Joseph Gil avalable\b",
        r"(?i)^Original code for Bayer\b",
        r"(?i)@remark Read the",
        r"(?i)\bEND END$",
        r"(?i)^inc\.,\s*Id Software\b",
        r"(?i)^Id Software.*Id Software\b",
        r"(?i)\bavalab?le at\b",
        r"(?i)^\(k \d+ k \d+\b",
        r"(?i)^\(unsigned char\)\b",
        r"(?i)^\(int\) TOUPPER\b",
        r"(?i)^(isascii|isdigit|isalpha|isalnum|isupper|islower|isspace|isgraph|ispunct|isxdigit)\b",
        r"(?i)^ungetc\b",
        r"(?i)^yylval\b",
        r"(?i)^symrec\b",
        r"(?i)^arg\s*\+\+",
        r"(?i)^gunichar\b",
        r"(?i)^TRUE FALSE$",
        r"(?i)^undef\b",
        r"(?i)^0 1$",
        r"(?i)^\d{1,2}$",
        r"(?i)^ok-for-header$",
        r"(?i)^date\b.*\bDon't assume\b",
        r"(?i)^notive in the\b",
        r"(?i)\bdon't assume ascii\b",
        r"(?i)^all the$",
        r"(?i)\bftp://\b",
        r"(?i)^CC Computer Consultants\b.*\bContact\b",
        r"(?i)^16 \(\(d\)\b",
        r"(?i)^\(c\) s-$",
        r"(?i)^z \?$",
        r"(?i)^Z \?$",
        r"(?i)^this-\s*setStencil\b",
        r"(?i)^temp\d+$",
        r"(?i)^table\.set\b",
        r"(?i)^strict!?\s*-?\d",
        r"(?i)^slen$",
        r"(?i)^save to iv\b",
        r"(?i)^r,?\s*div\b",
        r"(?i)^r sround\b",
        r"(?i)^r \(s\)$",
        r"(?i)^put chain\b",
        r"(?i)^problem,?\s*work-around\b",
        r"(?i)^prec prec\b",
        r"(?i)^pr this\b",
        r"(?i)^Paul Rusty Russell\b.*\bPlaced\b",
        r"(?i)^Paul Mackerras\b.*\bpipe read\b",
        r"(?i)^packet$",
        r"(?i)^p can be called\b",
        // ICS false positives: code fragments, boilerplate, gibberish
        r"(?i)^the parens part of\b",
        r"(?i)^i\.e\.\s*,?\s*call the\b",
        r"(?i)^8 \(\(b\)\b",
        r"(?i)^\d+ \(trail\b",
        r"(?i)^\d+ illegal\b",
        r"(?i)^strict!\s",
        r"(?i)^\(s\) \(i\)$",
        r"(?i)^0x[0-9a-fA-F]+",
        r"(?i)^\d+ \+0x",
        r"(?i)^\d+ &0x",
        r"(?i)^it a lead surrogate\b",
        r"(?i)^uint\d+$",
        r"(?i)^Construct a set of\b",
        r"(?i)^clause removed\b",
        r"(?i)^0x\d",
        r"(?i)^below\.?\s*(Please)?\b",
        r"(?i)^below$",
        r"(?i)^above$",
        r"(?i)^\(qbuf\b",
        r"(?i)^applies to code\b",
        r"(?i)^0x7f\b",
        r"(?i)^\(with the right granted\b",
        r"(?i)^fi$",
        r"(?i)^as follows$",
        r"(?i)^dst$",
        r"(?i)^dst-\s",
        r"(?i)^< 0 e->",
        r"(?i)^\(\d+ \(pattern\b",
        r"(?i)^ACM and IEEE\b",
        r"(?i)^make it clear$",
        r"(?i)^ifdef$",
        r"(?i)^exploring the\b.*\bcultural\b",
        r"(?i)^do prec\b",
        r"(?i)^EOF &&\b",
        r"(?i)^4\+\(r\)$",
        r"(?i)^c&0x",
        r"(?i)^cp\)$",
        r"(?i)^ctype$",
        r"(?i)^macroptr\b",
        r"(?i)^\(shf\)\b",
        r"(?i)^MAGIC$",
        r"(?i)^out\.ro$",
        r"(?i)^attribution$",
        r"(?i)^\d+ ,\s*l\b.*\b(unsigned|endif)\b",
        r"(?i)^asm bswapl\b",
        r"(?i)^d'\(l\)$",
        r"(?i)^l endif$",
        r"(?i)^03o$",
        r"(?i)^apply$",
        r"(?i)^\(cp\)$",
        r"(?i)^buflen\b.*\bbuf$",
        r"(?i)^buffer\s+[a-z]$",
        r"(?i)^etc\b.*\bstrings\b",
        r"(?i)^1,\s*cls\.\b",
        r"(?i)^this-\s*set\w+\b",
        r"(?i)^\(scale\s+\d\)\s+\d",
        r"(?i)^fWidth$",
        r"(?i)^dst \d$",
        r"(?i)^i \(s\)\b.*\b(while|endif)\b",
        r"(?i)^i ,?\s*div\b",
        r"(?i)^i sround\b",
        r"(?i)^i,?\s*s while\b",
        r"(?i)^r,?\s*div\b",
        r"(?i)^r,?\s*s$",
        r"(?i)^\(a\) \(b\)$",
        r"(?i)^FILE\.*\s+\w+\.\w+\s+AUTHOR\b",
        r"(?i)^Accumulate$",
        r"(?i)^ctable\b",
        r"(?i)^CREDITS PORTING\b",
        r"(?i)^24 endif$",
        r"(?i)^\^ 0x\d",
        r"(?i)^putchar\b.*\bputchar\b",
        r"(?i)^do while$",
        r"(?i)^\^ \(b\)$",
        r"(?i)^help$",
        r"(?i)^in gzlog\.h\b",
        r"(?i)^decoded by\b",
        r"(?i)^IBM Corporation\.$",
        r"(?i)^Lotus Development Corporation\.$",
        r"(?i)^disclaimer$",
        r"(?i)^disclaims all\b",
        r"(?i)^Foundation IBM\b",
        r"(?i)^http://sizzlejs\b",
        r"(?i)^Kasım$",
        r"(?i)^Akim Demaille$",
        r"(?i)^Joel E\. Denny$",
        r"(?i)^num \d$",
        r"(?i)^letters\b.*\bc - A\b",
        r"(?i)^classify$",
        r"(?i)^\(r\) l l$",
        r"(?i)^ok letters\b.*\bcond\b",
        r"(?i)^flags ptbl-\b",
        r"(?i)^holders,?\s*disclaims\b",
        r"(?i)^\d+L$",
        r"(?i)^\d+L,\s*l\b",
        r"(?i)^Chain has\b",
        r"(?i)^DEBUGP\b",
        r"(?i)^Only user$",
        r"(?i)^cindex chains$",
        r"(?i)^foot-\s*target\b",
        r"(?i)^OProfile authors\b.*@remark",
        r"(?i)^and are$",
        r"(?i)^mea-\s*setOffset\b",
        r"(?i)^meta-\s*registerClass\b",
        r"(?i)^EOF &&\s",
        r"(?i)^Bit8u$",
        r"(?i)^cvPoint3D32f$",
        r"(?i)^G2P ADJ\b",
        r"(?i)^info to be inserted\b",
        r"(?i)^0 isupper$",
        r"(?i)^0-9,- \d",
        r"(?i)^97 static$",
        r"(?i)^8 \(\(b\)\s*\d",
        r"(?i)^\d+ \(\(d\)\s*\d",
        r"(?i)^\d+ \(\(a\)\s*\d",
        r"(?i)\b@version \$Id\b",
        r"(?i)^ds Status works\b",
        r"(?i)^holder\.\s*AS\b",
        r"(?i)^oCOOA\b",
        r"(?i)^as\(c,\s*field\b",
        r"(?i)^a!\s*\+-",
        r"(?i)^\(xmlns:\?\s*\^",
        r"(?i)^Tue\s+\w+\s+\d+\s+\w+\s+\w+\s+<",
        r"(?i)\b\d{2}-[A-Z]{3}-\d{2}\s+Bugfixes\b",
        r"(?i)\bpartial mlock\b",
        r"(?i)\bskb\.\s*The buffer\b",
        r"(?i)^IBM Corp\.\s*Auxtrace\b",
        r"(?i)^digÃ",
        r"(?i)^que le prÃ",
        // Garbled binary data holders (from junk-copyright tests)
        r"^AaACEEeUB",
        r"^AaeaMOOAA\d",
        r"^EEIaeIaAAOAE",
        r"^OCOthDTh",
        r"^YY ThQ",
        r"^YY$",
        r"(?i)^NIST\.\d+\.\d+\.",
        r"(?i)AEEEUAU",
        r"(?i)\$\?I\$\?i\$\?I",
        r"^x!C/!O$",
        r"(?i)^33,BD\(b\b",
        r"(?i)^2AICAA",
        r"(?i)^Rear\b",
        r"(?i)^Rear Left$",
        r"(?i)^Rear Right$",
        r"(?i)^kavol$",
        r"(?i)^assigned to the United States Government\b",
        r"(?i)^so preceded by\b",
        r"(?i)^bounce, so we\b",
        r"(?i)^conditions,?\s+but\s+instead\b",
        r"(?i)^www\.\S+",
        r"(?i)^jornada\s+\d+$",
        r"(?i)^mjander\b",
        r"(?i)^notice,\s*a\s+notice\b",
        r"(?i),\s*a\s+notice\b",
        r"(?i)^\(scale\s+\d+\)$",
        r"(?i)^scale\s+\d+$",
        r"(?i)^i,\s*div\s+while$",
        r"(?i)^isinstanceof$",
        r"(?i)^contributor,\s*path$",
        r"(?x)\b[A-Za-z_][A-Za-z0-9_]*\([^)]*\).*[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*\(",
        r"^(?:[\u{0080}-\u{00FF}]+\s*){6,}$",
    ];
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
});

// ─── Junk detection ──────────────────────────────────────────────────────────

/// Return true if `s` matches any known junk copyright pattern.
pub fn is_junk_copyright(s: &str) -> bool {
    COPYRIGHTS_JUNK_PATTERNS.iter().any(|re| re.is_match(s))
        || is_junk_copyright_scan_phrase(s)
        || is_junk_c_sign_path_fragment(s)
}

fn has_copyright_year(s: &str) -> bool {
    static COPYRIGHT_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b(?:19\d{2}|20\d{2})(?:\s*[-–/]\s*(?:19\d{2}|20\d{2}|\d{2}))?\b").unwrap()
    });

    COPYRIGHT_YEAR_RE.is_match(s)
}

fn is_junk_copyright_scan_phrase(s: &str) -> bool {
    static COPYRIGHT_SCAN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bcopyright\s+scan(?:s|ner|ning)?\b").unwrap());

    !has_copyright_year(s) && COPYRIGHT_SCAN_RE.is_match(s)
}

fn is_junk_c_sign_path_fragment(s: &str) -> bool {
    let Some(tail) = s.trim().strip_prefix("(c)") else {
        return false;
    };

    !has_copyright_year(s) && is_path_like_code_fragment(tail)
}

/// Return true if `s` matches any known junk holder pattern.
pub(crate) fn is_junk_holder(s: &str) -> bool {
    HOLDERS_JUNK_PATTERNS.iter().any(|re| re.is_match(s)) || s.eq_ignore_ascii_case("MIT")
}

pub(crate) fn is_path_like_code_fragment(s: &str) -> bool {
    static PATH_LIKE_CODE_FRAGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)
            ^
            [A-Za-z_$][A-Za-z0-9_$]*
            (?:
                /[A-Za-z_$][A-Za-z0-9_$]*
              | \.[A-Za-z_$][A-Za-z0-9_$]*
              | \$[A-Za-z_$][A-Za-z0-9_$]*
            )+
            $
            ",
        )
        .unwrap()
    });

    PATH_LIKE_CODE_FRAGMENT_RE.is_match(s.trim())
}

// ─── Core refinement functions ───────────────────────────────────────────────

/// Refine a detected copyright string. Returns `None` if the result is empty.
pub fn refine_copyright(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let original = normalize_whitespace(s);
    let mut c = original.clone();
    c = strip_trailing_quote_before_email(&c);
    c = normalize_b_dot_angle_emails(&c);
    c = strip_nickname_quotes(&c);
    c = strip_leading_author_label_in_copyright(&c);
    c = strip_leading_licensed_material_of(&c);
    c = strip_leading_version_number_before_c(&c);
    c = strip_contributor_parens_after_org(&c);
    c = strip_trailing_paren_email_after_c_by(&c);
    c = strip_trailing_for_clause_after_email(&c);
    c = strip_trailing_at_affiliation(&c);
    c = strip_trailing_obfuscated_email_after_dash(&c);
    c = strip_url_token_between_years_and_holder(&c);
    c = strip_obfuscated_angle_emails(&c);
    c = strip_angle_bracketed_www_domains_without_by(&c);
    c = strip_leading_simple_copyright_prefixes(&c);
    c = normalize_comma_spacing(&c);
    c = normalize_angle_bracket_comma_spacing(&c);
    c = strip_trailing_secondary_angle_email_after_comma(&c);
    c = strip_trailing_short_surname_paren_list_in_copyright(&c);
    c = strip_trailing_et_al(&c);
    c = strip_trailing_authors_clause(&c);
    c = strip_trailing_document_authors_clause(&c);
    c = strip_trailing_amp_authors(&c);
    c = strip_trailing_x509_dn_fields(&c);
    c = strip_some_punct(&c);
    c = strip_solo_quotes(&c);
    // strip trailing slashes, tildes, spaces
    c = c.trim_matches(&['/', ' ', '~'][..]).to_string();
    c = strip_all_unbalanced_parens(&c);
    c = remove_some_extra_words_and_punct(&c);
    c = strip_trailing_incomplete_as_represented_by(&c);
    c = normalize_whitespace(&c);
    c = strip_leading_js_project_version(&c);
    c = remove_dupe_copyright_words(&c);
    c = strip_trailing_portions_of(&c);
    c = strip_trailing_paren_identifier(&c);
    c = strip_trailing_company_name_placeholder(&c);
    c = strip_trailing_company_co_ltd(&c);
    c = strip_trailing_obfuscated_email_in_angle_brackets_after_copyright(&c);
    c = strip_trailing_linux_ag_location_in_copyright(&c);
    c = strip_trailing_by_person_clause_after_company(&c);
    c = strip_trailing_division_of_company_suffix(&c);
    c = strip_trailing_linux_foundation_suffix(&c);
    c = strip_trailing_paren_at_without_domain(&c);
    c = strip_trailing_inc_after_today_year_placeholder(&c);
    c = truncate_trailing_boilerplate(&c);
    c = strip_trailing_author_label(&c);
    c = strip_trailing_isc_after_inc(&c);
    c = strip_trailing_caps_after_company_suffix(&c);
    c = strip_trailing_javadoc_tags(&c);
    c = strip_trailing_batch_comment_marker(&c);
    c = strip_prefixes(&c, &HashSet::from(["by", "c"]));
    c = c.trim().to_string();
    c = c.trim_matches('+').to_string();
    c = c.trim_matches(&[',', ' '][..]).to_string();
    c = strip_balanced_edge_parens(&c).to_string();
    c = strip_suffixes(&c, &COPYRIGHTS_SUFFIXES);
    c = c.trim_end_matches(&[',', ' '][..]).to_string();
    c = strip_trailing_ampas_acronym(&c);
    c = strip_trailing_period(&c);
    c = strip_independent_jpeg_groups_software_tail(&c);
    c = strip_trailing_original_authors(&c);
    c = strip_trailing_mountain_view_ca(&c);
    c = strip_trailing_comma_after_respective_authors(&c);
    c = c.trim_end_matches(char::is_whitespace).to_string();
    c = c.trim_matches('\'').to_string();
    c = wrap_trailing_and_urls_in_parens(&c);
    c = strip_trailing_url_slash(&c);
    c = truncate_long_words(&c);
    c = strip_trailing_single_digit_token(&c);
    c = strip_trailing_period(&c);
    let result = c.trim().to_string();

    static SOFTWARE_COPYRIGHT_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)\bsoftware\s+copyright\s*\(c\)\s*(?:19\d{2}|20\d{2})\b").unwrap()
    });
    if SOFTWARE_COPYRIGHT_C_RE.is_match(original.as_str())
        && !result.to_ascii_lowercase().contains("copyright")
    {
        let restored = strip_trailing_period(&original);
        let restored = restored.trim().to_string();
        if !restored.is_empty() {
            return Some(restored);
        }
    }

    static YEAR_ONLY_WITH_OBF_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^copyright\s*\(c\)\s*(?:19\d{2}|20\d{2})\s+[a-z0-9][a-z0-9._-]{0,63}\s+at\s+[a-z0-9][a-z0-9._-]{0,63}\s+dot\s+[a-z]{2,12}$",
        )
        .unwrap()
    });
    if YEAR_ONLY_WITH_OBF_EMAIL_RE.is_match(result.as_str()) {
        return None;
    }

    let result_upper = result.to_ascii_uppercase();
    if result_upper.contains("COPYRIGHT")
        && result_upper.contains("YEAR")
        && result_upper.contains("YOUR NAME")
    {
        return None;
    }
    if is_junk_copyright_of_header(&result)
        || is_junk_copyrighted_works_header(&result)
        || is_junk_copyrighted_software_phrase(&result)
    {
        return None;
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn strip_trailing_obfuscated_email_after_dash(s: &str) -> String {
    static TRAILING_DASH_OBF_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^(?P<prefix>.+?)\s*(?:--+|-)\s*(?P<user>[a-z0-9][a-z0-9._-]{0,63})\s*(?:\[\s*at\s*\]|at)\s*(?P<host>[a-z0-9][a-z0-9._-]{0,63})\s*(?:\[\s*dot\s*\]|dot)\s*(?P<tld>[a-z]{2,12})\s*$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = TRAILING_DASH_OBF_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };

    cap.name("prefix")
        .map(|m| m.as_str().trim_end_matches(&[' ', '-', '–', '—'][..]))
        .unwrap_or(trimmed)
        .to_string()
}

fn strip_trailing_secondary_angle_email_after_comma(s: &str) -> String {
    static TRAILING_SECOND_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?<[^>\s]*@[^>\s]*>)\s*,\s*<[^>\s]*@[^>\s]*>\s*$").unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = TRAILING_SECOND_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };

    let full = cap.get(0).map(|m| m.as_str()).unwrap_or(trimmed);
    let emails: Vec<&str> = full
        .split('<')
        .skip(1)
        .filter_map(|p| p.split_once('>').map(|(e, _)| e.trim()))
        .filter(|e| e.contains('@'))
        .collect();
    if emails.len() >= 2 {
        let a = emails[0].to_ascii_lowercase();
        let b = emails[1].to_ascii_lowercase();
        if a != b {
            return s.to_string();
        }
    }

    cap.name("prefix")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn normalize_b_dot_angle_emails(s: &str) -> String {
    static B_DOT_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<\s*b\.(?P<email>[^>\s]*@[^>\s]+)\s*>").unwrap());
    B_DOT_EMAIL_RE.replace_all(s, ".${email}").into_owned()
}

fn strip_url_token_between_years_and_holder(s: &str) -> String {
    static BETWEEN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*[-,\s0-9]{4,32})\s+https?://\S+\s+(?P<tail>\p{L}.+)$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = BETWEEN_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !tail.is_empty() {
            return normalize_whitespace(&format!("{prefix} {tail}"));
        }
    }
    s.to_string()
}

fn wrap_trailing_and_urls_in_parens(s: &str) -> String {
    static TRAILING_URLS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s+(?P<urls>https?://\S+\s+and\s+https?://\S+)\s*$")
            .unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = TRAILING_URLS_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap
        .name("prefix")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim_end();
    let urls = cap.name("urls").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || urls.is_empty() {
        return s.to_string();
    }
    if urls.starts_with('(') {
        return s.to_string();
    }
    format!("{prefix} ({urls})")
}

fn strip_obfuscated_angle_emails(s: &str) -> String {
    static OBF_ANGLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s*<[^>]*(?:\[at\]|\bat\b)[^>]*>\s*").unwrap());
    let trimmed = s.trim();
    if !(trimmed.contains("<") && trimmed.contains(">")) {
        return s.to_string();
    }
    let out = OBF_ANGLE_RE.replace_all(trimmed, " ").into_owned();
    normalize_whitespace(&out)
}

fn strip_trailing_linux_foundation_suffix(s: &str) -> String {
    static LINUX_FOUNDATION_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*\d{4}(?:\s*,\s*\d{4})*)\s+Linux\s+Foundation\s*$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = LINUX_FOUNDATION_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_linux_ag_location_in_copyright(s: &str) -> String {
    static LINUX_AG_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\b.*?\s)(?P<name>\S+)\s+Linux\s+AG\s*,\s*[^,]{2,64}\s*,\s*[^,]{2,64}\s*$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = LINUX_AG_COPY_RE.captures(trimmed) {
        let prefix = cap
            .name("prefix")
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim_end();
        let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !name.is_empty() {
            return format!("{prefix} {name}");
        }
    }
    s.to_string()
}

fn strip_trailing_quote_before_email(s: &str) -> String {
    static TRAILING_QUOTE_BEFORE_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<head>.*?\b[\p{L}])'\s+(?P<email><[^>\s]*@[^>\s]+>|[^\s<>]*@[^\s<>]+)(?P<tail>.*)$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    if !trimmed.contains('@') {
        return s.to_string();
    }
    let Some(cap) = TRAILING_QUOTE_BEFORE_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let head = cap.name("head").map(|m| m.as_str()).unwrap_or("");
    let email = cap.name("email").map(|m| m.as_str()).unwrap_or("");
    let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("");
    normalize_whitespace(&format!("{head} {email}{tail}"))
}

fn strip_nickname_quotes(s: &str) -> String {
    static NICK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<first>\b[\p{Lu}][\p{L}'-]+)\s+'(?P<nick>[A-Za-z]{2,20})'\s+(?P<last>\b[\p{Lu}][\p{L}'-]+)")
            .unwrap()
    });
    NICK_RE
        .replace_all(s, "${first} ${nick} ${last}")
        .into_owned()
}

fn strip_trailing_for_clause_after_email(s: &str) -> String {
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.contains(" for ") {
        return s.to_string();
    }
    if !lower.starts_with("copyright") {
        return s.to_string();
    }
    if !trimmed.contains('@') {
        return s.to_string();
    }
    let Some((head, _tail)) = trimmed.split_once(" for ") else {
        return s.to_string();
    };

    if let Some((_, tail)) = trimmed.split_once(" for ") {
        let tail = tail.trim();
        if tail.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            let word_count = tail.split_whitespace().count();
            let lower_tail = tail.to_ascii_lowercase();
            let looks_like_affiliation = word_count >= 3
                && (lower_tail.contains("laboratory")
                    || lower_tail.contains("computer science")
                    || lower_tail.contains("facility")
                    || lower_tail.contains("institute")
                    || lower_tail.contains("university")
                    || lower_tail.contains("department")
                    || lower_tail.contains("center"));
            if looks_like_affiliation {
                return s.to_string();
            }
        }
    }
    head.trim_end().to_string()
}

fn strip_trailing_at_affiliation(s: &str) -> String {
    let trimmed = s.trim();
    if !trimmed.to_ascii_lowercase().starts_with("copyright") {
        return s.to_string();
    }
    let Some((head, tail)) = trimmed.split_once(" @ ") else {
        return s.to_string();
    };
    let tail = tail.trim();
    if tail.is_empty() {
        return s.to_string();
    }
    if tail.contains('@') {
        return s.to_string();
    }
    if tail.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return head.trim_end().to_string();
    }
    s.to_string()
}

fn strip_trailing_paren_at_without_domain(s: &str) -> String {
    static TRAILING_PAREN_AT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(\s*(?P<inner>[^)]*\bat\b[^)]*)\)\s*$").unwrap()
    });

    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("copyright") || lower.starts_with("(c)")) {
        return s.to_string();
    }

    let Some(cap) = TRAILING_PAREN_AT_RE.captures(trimmed) else {
        return s.to_string();
    };
    let inner = cap.name("inner").map(|m| m.as_str()).unwrap_or("").trim();
    if inner.is_empty() {
        return s.to_string();
    }

    let inner_lower = inner.to_ascii_lowercase();
    if inner.contains('@') || inner.contains('.') || inner_lower.contains(" dot ") {
        return s.to_string();
    }

    cap.name("prefix")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_inc_after_today_year_placeholder(s: &str) -> String {
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.contains("today.year") || lower.contains("current_year")) {
        return s.to_string();
    }
    if !(lower.ends_with(" inc.") || lower.ends_with(" inc")) {
        return s.to_string();
    }
    let prefix = trimmed
        .trim_end_matches('.')
        .trim_end_matches(|c: char| c.is_whitespace())
        .strip_suffix("Inc")
        .or_else(|| trimmed.strip_suffix("Inc."));
    let Some(prefix) = prefix else {
        return s.to_string();
    };
    prefix.trim_end().to_string()
}

fn strip_trailing_obfuscated_email_in_angle_brackets_after_copyright(s: &str) -> String {
    static OBFUSCATED_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>copyright\b.+?)\s*<[^>]*\bat\b[^>]*\bdot\b[^>]*>\s*$").unwrap()
    });

    let trimmed = s.trim();
    if !trimmed
        .get(.."Copyright".len())
        .is_some_and(|p| p.eq_ignore_ascii_case("Copyright"))
    {
        return s.to_string();
    }

    let Some(cap) = OBFUSCATED_TAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    cap.name("prefix")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_author_label(s: &str) -> String {
    static TRAILING_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\s+(?:Author|AUTHOR)\b").expect("valid trailing Author regex")
    });
    let Some(m) = TRAILING_AUTHOR_RE.find(s) else {
        return s.to_string();
    };

    let prefix = s[..m.start()].trim_end();
    if !prefix_has_holder_words(prefix) {
        return s.to_string();
    }

    prefix.to_string()
}

fn strip_leading_author_label_in_copyright(s: &str) -> String {
    static LEADING_AUTHOR_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:@?author)\s+(?P<rest>.+\(c\)\s*(?:19|20)\d{2}.*)$")
            .expect("valid leading author copyright regex")
    });
    let trimmed = s.trim();
    let Some(cap) = LEADING_AUTHOR_COPY_RE.captures(trimmed) else {
        return s.to_string();
    };
    let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("").trim();
    if rest.is_empty() {
        return s.to_string();
    }
    rest.to_string()
}

fn strip_leading_author_label_in_holder(s: &str) -> String {
    static LEADING_AUTHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:@?author)\b[:\s]+(?P<rest>.+)$").expect("valid leading author regex")
    });
    let trimmed = s.trim();
    let Some(cap) = LEADING_AUTHOR_RE.captures(trimmed) else {
        return s.to_string();
    };
    let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("").trim();
    if rest.is_empty() {
        return s.to_string();
    }
    rest.to_string()
}

fn prefix_has_holder_words(prefix: &str) -> bool {
    for raw in prefix.split_whitespace() {
        let token = raw.trim_matches(|c: char| c.is_ascii_punctuation() || matches!(c, '' | ''));
        if token.is_empty() {
            continue;
        }

        let lower = token.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "*" | "copyright" | "copr" | "(c)" | "c" | "\u{a9}"
        ) {
            continue;
        }

        // Ignore pure year-ish tokens.
        let yearish = token
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '+' | ','));
        if yearish {
            continue;
        }

        return true;
    }

    false
}

fn strip_leading_licensed_material_of(s: &str) -> String {
    static LICENSED_MATERIAL_OF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?:licensed\s+)?material\s+of\s+").unwrap());
    LICENSED_MATERIAL_OF_RE
        .replace(s, "")
        .trim_start()
        .to_string()
}

fn strip_leading_version_number_before_c(s: &str) -> String {
    static VERSION_BEFORE_C_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\d+\.\d+(?:\.\d+)*\.?\s+(\(c\)|\bcopyright\b)").unwrap()
    });
    if let Some(m) = VERSION_BEFORE_C_RE.find(s) {
        let cap = VERSION_BEFORE_C_RE.captures(s).unwrap();
        let keyword_start = m.start() + m.as_str().len() - cap[1].len();
        s[keyword_start..].trim_start().to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_authors_clause(s: &str) -> String {
    static AUTHORS_CLAUSE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+?)\s+Authors?\b\s+(?P<rest>.+)$").unwrap());

    let trimmed = s.trim();

    let Some(cap) = AUTHORS_CLAUSE_RE.captures(trimmed) else {
        return s.to_string();
    };

    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("");
    let rest = cap.name("rest").map(|m| m.as_str()).unwrap_or("");
    if prefix.trim().is_empty() || rest.trim().is_empty() {
        return s.to_string();
    }

    let rest_for_count = if let Some(email_idx) = rest.find('@') {
        rest[..email_idx].trim()
    } else {
        rest.trim()
    };

    let words_before_email = rest_for_count
        .split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphabetic()) && !w.contains('<') && !w.contains('>'))
        .count();
    if words_before_email > 2 {
        return s.to_string();
    }

    let prefix_trimmed = prefix.trim();
    let prefix_last_is_year = prefix_trimmed
        .split_whitespace()
        .last()
        .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()));
    if !prefix_trimmed.contains(',') && !prefix_last_is_year {
        return s.to_string();
    }

    prefix_trimmed
        .trim_end_matches(&[',', ';', ':'][..])
        .trim()
        .to_string()
}

fn strip_trailing_document_authors_clause(s: &str) -> String {
    static DOCUMENT_AUTHORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>.+?)\s+and\s+the\s+persons\s+identified\s+as\s+document\s+authors\.?$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = DOCUMENT_AUTHORS_RE.captures(trimmed) else {
        return s.to_string();
    };

    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || !prefix_has_holder_words(prefix) {
        return s.to_string();
    }

    prefix
        .trim_end_matches(&[',', ';', ':', ' '][..])
        .trim()
        .to_string()
}

fn strip_trailing_et_al(s: &str) -> String {
    static ET_AL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s*,?\s*et\s+al\.?\s*$").unwrap());

    let trimmed = s.trim();
    let Some(cap) = ET_AL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("");
    prefix.trim().trim_end_matches(',').trim().to_string()
}

fn strip_trailing_x509_dn_fields(s: &str) -> String {
    static X509_DN_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*\d{4}(?:\s*,\s*OU\s+[^,]+|\s+[^,]+))(?:\s*,\s*(?:OU|CN|O|C|L|ST)\s+.+)$",
        )
        .unwrap()
    });
    static OU_ENDORSED_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>Copyright\s*\(c\)\s*\d{4}\s*,\s*OU\s+.+?)\s+endorsed\s*$")
            .unwrap()
    });

    let Some(cap) = X509_DN_TAIL_RE.captures(s.trim()) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    if let Some(cap2) = OU_ENDORSED_TAIL_RE.captures(prefix) {
        cap2.name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| prefix.to_string())
    } else {
        prefix.to_string()
    }
}

fn strip_independent_jpeg_groups_software_tail(s: &str) -> String {
    static JPEG_GROUP_SOFTWARE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(Independent JPEG Group's)\s+software\b\.?$").unwrap());
    JPEG_GROUP_SOFTWARE_RE.replace(s, "$1").trim().to_string()
}

fn strip_trailing_original_authors(s: &str) -> String {
    static ORIGINAL_AUTHORS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(.*\bthe original)\s+authors\b\s*$").unwrap());
    if let Some(cap) = ORIGINAL_AUTHORS_RE.captures(s) {
        cap[1].trim().to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_paren_email_after_c_by(s: &str) -> String {
    static C_BY_PAREN_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>(?:Copyright\s+)?\(c\)\s+by\s+[^()]+?)\s*\([^()]*@[^()]*\)\s*$",
        )
        .unwrap()
    });

    if let Some(caps) = C_BY_PAREN_EMAIL_RE.captures(s) {
        caps.name("prefix")
            .map(|m| normalize_whitespace(m.as_str().trim()))
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_contributor_parens_after_org(s: &str) -> String {
    static ORG_PARENS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.*)\(\s*(?P<inner>[^()]+?)\s*\)\s*$").unwrap());

    let Some(cap) = ORG_PARENS_RE.captures(s.trim()) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    let inner = cap.name("inner").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || inner.is_empty() {
        return s.to_string();
    }

    let inner_lower = inner.to_ascii_lowercase();
    let looks_like_contributor_list = inner_lower.contains(" and ") || inner.contains('<');
    if !looks_like_contributor_list {
        return s.to_string();
    }

    normalize_whitespace(&format!("{prefix} {inner}"))
}

fn strip_angle_bracketed_www_domains_without_by(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    if lower.contains(" by ") {
        return s.to_string();
    }

    static WWW_IN_COMMA_CLAUSE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i),\s*<www\.[^>]+>\s*").expect("valid www domain regex"));
    static WWW_TRAILING_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\s*<www\.[^>]+>\s*$").expect("valid trailing www domain regex")
    });

    let s = WWW_IN_COMMA_CLAUSE_RE.replace_all(s, ", ");
    let s = WWW_TRAILING_RE.replace(&s, "");
    normalize_whitespace(s.trim())
}

fn strip_angle_bracketed_www_domains(s: &str) -> String {
    static WWW_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s*<www\.[^>]+>\s*").expect("valid www domain regex"));

    let s = WWW_RE.replace_all(s, " ");
    normalize_whitespace(s.trim())
}

fn strip_trailing_mountain_view_ca(s: &str) -> String {
    static MOUNTAIN_VIEW_CA_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bMountain View\s*,\s*CA\.?$").expect("valid Mountain View CA regex")
    });

    if MOUNTAIN_VIEW_CA_RE.is_match(s) {
        MOUNTAIN_VIEW_CA_RE
            .replace(s, "Mountain View")
            .trim()
            .to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_isc_after_inc(s: &str) -> String {
    static TRAILING_ISC_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?\bInc\.?)\s+ISC\s*$").unwrap());
    if let Some(cap) = TRAILING_ISC_RE.captures(s.trim()) {
        cap.name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_trailing_caps_after_company_suffix(s: &str) -> String {
    static TRAILING_CAPS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?\b(?:Corp|Inc|Ltd|LLC|Co)\.)\s+[A-Z]{2,}\s*$").unwrap()
    });
    if let Some(cap) = TRAILING_CAPS_RE.captures(s.trim()) {
        cap.name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_trailing_comma_after_respective_authors(s: &str) -> String {
    let trimmed = s.trim_end_matches(char::is_whitespace);
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("respective authors,") {
        let mut t = trimmed.to_string();
        if t.ends_with(',') {
            t.pop();
        }
        t.trim_end_matches(char::is_whitespace).to_string()
    } else {
        s.to_string()
    }
}

fn strip_leading_simple_copyright_prefixes(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    if (lower.starts_with("program copyright") || lower.starts_with("debian copyright"))
        && let Some(idx) = lower.find("copyright")
    {
        return s[idx..].trim_start().to_string();
    }

    if lower.contains("debian copyright")
        && let Some(idx) = lower.rfind("copyright")
    {
        let tail = s[idx..].trim_start();
        if tail.to_ascii_lowercase().starts_with("copyright") {
            return tail.to_string();
        }
    }

    if lower.starts_with("the ")
        && let Some(idx) = lower.rfind(". copyright")
        && idx + 2 < s.len()
    {
        let tail = s[(idx + 2)..].trim_start();
        if tail.to_ascii_lowercase().starts_with("copyright") {
            return tail.to_string();
        }
    }

    s.to_string()
}

fn is_junk_copyright_of_header(s: &str) -> bool {
    let lower = s.to_lowercase();
    let prefix = "copyright of";
    if !lower.starts_with(prefix) {
        return false;
    }

    let mut tail = s[prefix.len()..].trim();
    tail = tail.trim_matches(&[':', '-', ' ', '\t'][..]);
    if tail.is_empty() {
        return true;
    }

    let tail_lower = tail.to_lowercase();
    if tail_lower.starts_with("qt has been transferred") {
        return true;
    }
    if tail_lower.starts_with("version of nameif") {
        return true;
    }
    if tail_lower.contains("full text of") {
        return true;
    }

    if tail.contains('/') {
        return true;
    }

    !tail.chars().any(|c| c.is_ascii_uppercase())
}

fn strip_leading_js_project_version(s: &str) -> String {
    static JS_PROJECT_VERSION_PREFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[a-z0-9_.-]+\.js\s+\d+\.\d+(?:\.\d+)?\s+").unwrap());

    JS_PROJECT_VERSION_PREFIX_RE
        .replace(s, "")
        .trim()
        .to_string()
}

fn truncate_trailing_boilerplate(s: &str) -> String {
    static TRAILING_BOILERPLATE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        let patterns = [
            r"(?i)\bDistributed in the hope\b",
            r"(?i)\bMay be used\b",
            r"(?i)\bLicense-Alias\b",
            r"(?i)\bFull text of\b",
            r"(?i)\s+-\s*icon support\b",
            r"(?i)\s+-\s*maintainer\b",
            r"(?i)\s+-\s*software\b",
            r"(?i)\.\s*Software\.?$",
            r"(?i),+\s*Software\b",
            r"(?i)\bwrite\s+to\s+the\s+Free\s+Software\s+Foundation\b",
            r"(?i)\b51\s+Franklin\s+(?:Street|St)\b",
            r"(?i)\b675\s+Mass\s+Ave\b",
            r"(?i)\b901\s+San\s+Antonio\s+Road\b",
            r"(?i)\b2601\s+Elliott\s+Avenue\b",
            r"(?i)\bKoll\s+Center\s+Parkway\b",
            r"(?i)\bGNU\s+GENERAL\s+PUBLIC\s+LICENSE\b",
            r"(?i)\s+GNU\s*$",
            r"(?i)\.\s*print\s*$",
            r"(?i)\bTheir\s+notice\s+is\s+reproduced\s+below\b",
            r"(?i)\bTheir\s+notice\s+reproduced\s+below\b",
            r"(?i)\bTheir\s+notice\s+reproduced\s+below\s+in\s+its\s+entirety\b",
            r"(?i)\band/or\s+its\s+suppliers?\b",
            r"(?i)\bNOTE\s+Sort\b",
            r"(?i)\bdocumentation\s+generated\s+by\b",
            r"(?i)\(\s*The full list is in\b",
            r#"(?i)\(\s*the\s+['"]?original\s+author['"]?\s*\)\s+and\s+additional\s+contributors\b"#,
            r"(?i)\bthe\s+original\s+author\b\s+and\s+additional\s+contributors\b",
            r"\becho\s+",
            r"(?i)\bv\d+\.\d+\s*$",
            r"(?i)\bassigned\s+to\s+the\s+",
            r"(?i)\bHP\s+IS\s+AGREEING\b",
            r"(?i)\bCA\.\s*ansi2knr\b",
            r"(?i)\bDirect\s+questions\b",
            r"(?i)\bkbd\s+driver\b",
            r"(?i)\bMIDI\s+driver\b",
            r"(?i)\bLZO\s+version\b",
            r"(?i)\bpersistent\s+bitmap\b",
            r"(?i)\bLIBERATION\b",
            r"(?i)\bAHCI\s+SATA\b",
            r"(?i)\bDTMF\s+code\b",
            r"\bOPTIONS\s*$",
            r"(?i)\bindexing\s+(?:porting|code)\b",
            r"(?i)\bvortex\b",
            r"(?i)\bLinuxTV\b",
            r"(?i)-\s*OMAP\d",
            r"\bGDB\b",
            r"(?i)\band\s+software/linux\b",
            r"(?i),\s+by\s+Paul\s+Dale\b",
            r"(?i),?\s+and\s+other\s+parties\b",
            r"(?i)\b\d+\s+Parnell\s+St\b",
            r"(?i)\b\d+\s+Main\s+(?:street|st)\b",
            r"(?i)\b\d+\s+Koll\s+Center\s+Parkway\b",
            r"(?i)\bBeverly\s+Hills\b",
            r"(?i)\bBerverly\s+Hills\b",
            r"(?i)\bDublin\s+\d\b",
            r"(?i)\band\s+Bob\s+Dougherty\b",
            r"(?i)\band\s+is\s+licensed\s+under\b",
            r"(?i)\bBEGIN\s+LICENSE\s+BLOCK\b",
            r"(?i)^NOTICE,\s*DISCLAIMER,\s*and\s*LICENSE\b",
            r"(?i)\bIn\s+the\s+event\s+of\b",
            r"(?i),\s*ALL\s+RIGHTS\s+RESERVED\b",
            r"(?i)\s+All\s+rights\s+reserved\b",
            r"(?i)\s+All\s+rights\b",
            r"(?i),\s*THIS\s+SOFTWARE\s+IS\b",
            r"(?i),?\s+member\s+of\s+The\s+XFree86\s+Project\b",
            r"(?i)\s+Download\b",
            r"(?i)\bThis\s+code\s+is\s+GPL\b",
            r"(?i)\bGPLd\b",
            r"(?i)\bPlaced\s+under\s+the\s+GNU\s+GPL\b",
            r"(?i)\bSee\s+the\s+GNU\s+GPL\b",
            r"(?i)\bFor\s+other\s+copyrights\b",
            r"(?i)\bLast\s+modified\b",
            r"(?i)\(\s*the\s+original\s+version\s*\)\s*$",
            r"(?i)\bavalable\s+at\b",
            r"(?i)\bavailable\s+at\b",
            r"(?i),\s+and\s+are\s*$",
            r"(?i)\bNIN\s+logo\b",
            r"(?i),\s+with\s*$",
            r"(?i)\(\s*(?:written|brushed)\b[^)]*\)\s*$",
            r"(?i)\(\s*[^)]*implementation[^)]*\)\s*$",
            r"(?i)\bThis\s+file\s+is\s+licensed\s+under\b",
            r"(?i)\bLicensing\s+details\s+are\s+in\b",
            r"(?i)\bLinux\s+for\s+Hitachi\s+SuperH\b",
            r"(?i)\.\s*OProfile\s*$",
        ];
        patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
    });

    let mut cut: Option<usize> = None;
    for re in TRAILING_BOILERPLATE_RE.iter() {
        if let Some(m) = re.find(s) {
            cut = Some(cut.map_or(m.start(), |c| c.min(m.start())));
        }
    }

    if let Some(idx) = cut {
        s[..idx]
            .trim()
            .trim_matches(&['-', ',', ';'][..])
            .trim()
            .to_string()
    } else {
        s.trim().to_string()
    }
}

fn is_junk_copyrighted_works_header(s: &str) -> bool {
    let lower = s.to_lowercase();
    let prefix = "copyrighted works";
    if !lower.starts_with(prefix) {
        return false;
    }

    let mut tail = s[prefix.len()..].trim();
    tail = tail.trim_matches(&[':', '-', ' ', '\t'][..]);
    if tail.is_empty() {
        return true;
    }

    let tail_lower = tail.to_lowercase();
    let rest = if tail_lower == "of" {
        return true;
    } else if tail_lower.starts_with("of ") {
        tail[2..].trim()
    } else {
        return true;
    };

    if rest.is_empty() {
        return true;
    }

    !rest.chars().any(|c| c.is_ascii_uppercase())
}

fn is_junk_copyrighted_software_phrase(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("copyrighted software")
}

fn strip_trailing_company_name_placeholder(s: &str) -> String {
    static COMPANY_NAME_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)(\bCOMPANY)\s+NAME\s*$").unwrap());
    COMPANY_NAME_RE.replace(s, "$1").trim().to_string()
}

fn strip_leading_portions_comma(s: &str) -> String {
    static LEADING_PORTIONS_COMMA_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?:portions?|parts?)\s*,\s*").unwrap());
    LEADING_PORTIONS_COMMA_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_paren_identifier(s: &str) -> String {
    static TRAILING_PAREN_ID_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+\([a-z][a-z0-9]{3,}\)\s*$").unwrap());
    static TRAILING_PAREN_ID_COMMA_WORD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s+\([a-z][a-z0-9]{3,}\),\s*[a-z][a-z0-9]*\.?\s*$").unwrap());
    let s = TRAILING_PAREN_ID_COMMA_WORD_RE.replace(s, "");
    TRAILING_PAREN_ID_RE.replace(&s, "").trim().to_string()
}

fn strip_trailing_portions_of(s: &str) -> String {
    static TRAILING_PORTIONS_OF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(?:some\s+)?(?:portions?|parts?)\s+of$").unwrap());
    TRAILING_PORTIONS_OF_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_short_surname_paren_list_in_holder(s: &str) -> String {
    static SHORT_SURNAME_PAREN_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<first>[\p{Lu}][\p{L}'-]+)\s+(?:[\p{Lu}][\p{Ll}])\s*\([^)]*\)\s*,\s*.+$")
            .expect("valid short-surname paren list regex")
    });

    let trimmed = s.trim();
    if let Some(cap) = SHORT_SURNAME_PAREN_LIST_RE.captures(trimmed) {
        cap.name("first")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

fn strip_trailing_short_surname_paren_list_in_copyright(s: &str) -> String {
    static SHORT_SURNAME_PAREN_LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>Copyright\s+\((?:c|C)\)\s+\d{4}(?:-\d{4})?)\s+(?P<first>[\p{Lu}][\p{L}'-]+)\s+(?:[\p{Lu}][\p{Ll}])\s*\([^)]*\)\s*,\s*.+$",
        )
        .expect("valid short-surname copyright paren list regex")
    });

    let trimmed = s.trim();
    if let Some(cap) = SHORT_SURNAME_PAREN_LIST_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        let first = cap.name("first").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() && !first.is_empty() {
            return normalize_whitespace(&format!("{prefix} {first}"));
        }
    }
    s.to_string()
}

/// Refine a detected holder name. Returns `None` if junk or empty.
pub fn refine_holder(s: &str) -> Option<String> {
    refine_holder_impl(s, false)
}

pub fn refine_holder_in_copyright_context(s: &str) -> Option<String> {
    refine_holder_impl(s, true)
}

fn strip_parenthesized_emails(s: &str) -> String {
    static PAREN_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s*\([^()]*@[^()]*\)\s*").unwrap());
    normalize_whitespace(&PAREN_EMAIL_RE.replace_all(s, " "))
}

fn refine_holder_impl(s: &str, in_copyright_context: bool) -> Option<String> {
    if s.is_empty() {
        return None;
    }

    let had_paren_email =
        in_copyright_context && s.contains('@') && s.contains('(') && s.contains(')');

    // Choose prefix set based on whether "reserved" appears.
    let prefixes = if s.to_lowercase().contains("reserved") {
        &*HOLDERS_PREFIXES_WITH_ALL
    } else {
        &*HOLDERS_PREFIXES
    };

    let mut h = s.replace("build.year", " ");
    h = strip_trailing_quote_before_email(&h);
    h = strip_nickname_quotes(&h);
    h = strip_leading_author_label_in_holder(&h);
    h = strip_angle_bracketed_www_domains(&h);
    if in_copyright_context {
        h = strip_angle_bracketed_emails(&h);
        h = strip_trailing_email_token(&h);
        h = strip_trailing_obfuscated_email_phrase_in_holder(&h);
    }
    h = strip_parenthesized_emails(&h);
    h = strip_trailing_parenthesized_url_or_domain(&h);
    h = strip_contributor_parens_after_org(&h);
    h = normalize_comma_spacing(&h);
    h = normalize_angle_bracket_comma_spacing(&h);
    h = strip_trailing_linux_ag_location(&h);
    h = strip_trailing_but_suffix(&h);
    if had_paren_email {
        h = remove_comma_between_person_and_company_suffix(&h);
    }
    h = strip_trailing_by_person_clause_after_company(&h);
    h = strip_trailing_division_of_company_suffix(&h);
    h = strip_leading_product_operating_system_title(&h);
    h = strip_trailing_et_al(&h);
    h = strip_trailing_authors_clause(&h);
    h = strip_trailing_document_authors_clause(&h);
    h = strip_trailing_amp_authors(&h);
    h = strip_trailing_x509_dn_fields_from_holder(&h);
    h = strip_leading_js_project_version(&h);
    h = truncate_trailing_boilerplate(&h);
    h = strip_trailing_isc_after_inc(&h);
    h = strip_trailing_caps_after_company_suffix(&h);
    h = strip_trailing_javadoc_tags(&h);
    h = strip_trailing_batch_comment_marker(&h);
    h = strip_leading_portions_comma(&h);
    h = strip_trailing_paren_identifier(&h);
    h = strip_trailing_company_name_placeholder(&h);

    if in_copyright_context {
        h = strip_trailing_short_surname_paren_list_in_holder(&h);
    }

    // Strip leading date-like prefix (digits, dashes, slashes).
    if h.contains(' ')
        && let Some((prefix, suffix)) = h.split_once(' ')
        && prefix
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '/')
    {
        h = suffix.to_string();
    }

    h = remove_some_extra_words_and_punct(&h);
    h = strip_trailing_incomplete_as_represented_by(&h);
    h = h.trim_matches(&['/', ' ', '~'][..]).to_string();
    h = refine_names(&h, prefixes);
    h = strip_trailing_company_co_ltd(&h);
    h = strip_suffixes(&h, &HOLDERS_SUFFIXES);
    h = strip_trailing_ampas_acronym(&h);
    h = h.trim_matches(&['/', ' ', '~'][..]).to_string();
    h = strip_solo_quotes(&h);
    h = h.replace("( ", " ").replace(" )", " ");
    h = h.trim_matches(&['+', '-', ' '][..]).to_string();
    h = strip_trailing_period(&h);
    h = strip_independent_jpeg_groups_software_tail(&h);
    h = strip_trailing_original_authors(&h);
    h = h.trim_matches(&['+', '-', ' '][..]).to_string();
    h = remove_dupe_holder(&h);
    h = normalize_whitespace(&h);
    h = strip_trailing_url(&h);
    if in_copyright_context {
        h = strip_trailing_email_token(&h);
    }
    h = strip_trailing_at_sign(&h);
    h = strip_trailing_mountain_view_ca(&h);
    h = h.trim_matches(&[',', ' '][..]).to_string();
    h = strip_trailing_period(&h);
    h = h.trim_matches(&[',', ' '][..]).to_string();
    h = normalize_whitespace(&h);
    h = truncate_long_words(&h);
    h = strip_trailing_single_digit_token(&h);
    h = strip_trailing_period(&h);
    h = h.trim().to_string();

    let lower = h.to_lowercase();
    if h.trim_end_matches('.').eq_ignore_ascii_case("YOUR NAME") {
        return None;
    }
    let is_single_word_contributors = lower == "contributors";
    let is_contributors_as_noted_in_authors_file =
        in_copyright_context && lower.contains("contributors as noted in the authors file");
    if !h.is_empty()
        && (!HOLDERS_JUNK.contains(lower.as_str())
            || (in_copyright_context && is_single_word_contributors))
        && (is_contributors_as_noted_in_authors_file || !is_junk_holder(&h))
    {
        Some(h)
    } else {
        None
    }
}

fn strip_trailing_but_suffix(s: &str) -> String {
    static TRAILING_BUT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?),\s*but\s*$").unwrap());
    let trimmed = s.trim();
    let Some(cap) = TRAILING_BUT_RE.captures(trimmed) else {
        return s.to_string();
    };
    cap.name("prefix")
        .map(|m| m.as_str().trim_end().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_division_of_company_suffix(s: &str) -> String {
    static DIVISION_OF_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?),\s*a\s+division\s+of\s+.+$").unwrap());

    let trimmed = s.trim();
    let Some(cap) = DIVISION_OF_RE.captures(trimmed) else {
        return s.to_string();
    };

    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() || !prefix_has_holder_words(prefix) {
        return s.to_string();
    }

    prefix.trim_end_matches(&[',', ' '][..]).trim().to_string()
}

fn strip_trailing_linux_ag_location(s: &str) -> String {
    static LINUX_AG_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>\S+)\s+Linux\s+AG\s*,\s*[^,]{2,64}\s*,\s*[^,]{2,64}\s*$").unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = LINUX_AG_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn remove_comma_between_person_and_company_suffix(s: &str) -> String {
    static COMMA_CORP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<person>[\p{Lu}][^,]{2,64}(?:\s+[\p{Lu}][^,]{2,64})+)\s*,\s*(?P<corp>[^,]{2,64}\b(?:Corp\.?|Corporation|Inc\.?|Ltd\.?))\s*$")
            .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = COMMA_CORP_RE.captures(trimmed) {
        let person = cap.name("person").map(|m| m.as_str()).unwrap_or("").trim();
        let corp = cap.name("corp").map(|m| m.as_str()).unwrap_or("").trim();
        if !person.is_empty() && !corp.is_empty() {
            return format!("{person} {corp}");
        }
    }
    s.to_string()
}

fn strip_trailing_by_person_clause_after_company(s: &str) -> String {
    static BY_PERSON_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?\b(?:Corp\.?|Corporation|Inc\.?|Ltd\.?))\s+by\s+[\p{Lu}][\p{L}'\-\.]+(?:\s+[\p{Lu}][\p{L}'\-\.]+){1,4}\s*(?:<[^>]*>)?\s*$")
            .unwrap()
    });
    let trimmed = s.trim();
    if let Some(cap) = BY_PERSON_RE.captures(trimmed) {
        let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    s.to_string()
}

fn strip_trailing_amp_authors(s: &str) -> String {
    static AMP_AUTHORS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s*(?:&|and)\s+authors?\s*$").unwrap());
    let trimmed = s.trim();
    if let Some(cap) = AMP_AUTHORS_RE.captures(trimmed)
        && let Some(prefix) = cap.name("prefix").map(|m| m.as_str().trim())
        && !prefix.is_empty()
    {
        return prefix.to_string();
    }
    s.to_string()
}

fn strip_trailing_parenthesized_url_or_domain(s: &str) -> String {
    static TRAILING_PAREN_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(\s*(?:https?|ftp)://[^)\s]+\s*\)\s*$").unwrap()
    });
    static TRAILING_PAREN_DOMAIN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(\s*[a-z0-9._-]+\.[a-z]{2,12}\s*\)\s*$").unwrap()
    });
    static TRAILING_SINGLE_WORD_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?)\s*\(\s*(?P<inner>[A-Za-z0-9._-]{2,32})\s*\)\s*$").unwrap()
    });

    let trimmed = s.trim();
    if let Some(cap) = TRAILING_PAREN_URL_RE.captures(trimmed) {
        return cap
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string());
    }
    if let Some(cap) = TRAILING_PAREN_DOMAIN_RE.captures(trimmed) {
        return cap
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| s.to_string());
    }
    if let Some(cap) = TRAILING_SINGLE_WORD_PARENS_RE.captures(trimmed)
        && let Some(inner) = cap.name("inner").map(|m| m.as_str().trim())
        && !inner.is_empty()
    {
        let inner_has_upper = inner.chars().any(|c| c.is_ascii_uppercase());
        let inner_all_lowerish = inner
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));

        if !inner_has_upper && inner_all_lowerish && inner.len() >= 4 && !inner.starts_with('-') {
            return cap
                .name("prefix")
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_else(|| s.to_string());
        }
    }

    s.to_string()
}

fn strip_angle_bracketed_emails(s: &str) -> String {
    static ANGLE_EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*<[^>\s]*@[^>\s]*>\s*").unwrap());
    ANGLE_EMAIL_RE.replace_all(s, " ").trim().to_string()
}

fn strip_trailing_email_token(s: &str) -> String {
    static TRAILING_EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<prefix>.+?)\s+(?P<email>[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+)\s*$").unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = TRAILING_EMAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    prefix.to_string()
}

fn strip_trailing_obfuscated_email_phrase_in_holder(s: &str) -> String {
    static OBFUSCATED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?P<prefix>.+?)\s+(?P<user>[a-z0-9][a-z0-9._-]{0,63})\s+at\s+(?P<domain>[a-z0-9][a-z0-9._-]{0,63})\s+dot\s+(?P<tld>[a-z]{2,12})(?:\s+.*)?$",
        )
        .unwrap()
    });

    let trimmed = s.trim();
    let Some(cap) = OBFUSCATED_RE.captures(trimmed) else {
        return s.to_string();
    };
    let user = cap.name("user").map(|m| m.as_str()).unwrap_or("").trim();
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    if user.is_empty() {
        return prefix.to_string();
    }
    let mut words: Vec<&str> = prefix.split_whitespace().collect();
    if words.last().is_some_and(|w| w.eq_ignore_ascii_case(user)) {
        words.pop();
    }
    words.join(" ")
}

fn strip_trailing_at_sign(s: &str) -> String {
    let trimmed = s.trim_end();
    if let Some(stripped) = trimmed.strip_suffix('@') {
        return stripped.trim_end().to_string();
    }
    s.to_string()
}

fn strip_leading_product_operating_system_title(s: &str) -> String {
    static PRODUCT_OPERATING_SYSTEM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^the\s+(?:[\p{L}0-9._-]+\s+){1,5}operating\s+system(?:[.,]|\s|$)").unwrap()
    });

    if !PRODUCT_OPERATING_SYSTEM_RE.is_match(s.trim()) {
        return s.to_string();
    }

    if let Some((_, suffix)) = s.split_once(',') {
        return suffix.trim().to_string();
    }

    s.to_string()
}

fn strip_trailing_x509_dn_fields_from_holder(s: &str) -> String {
    static X509_DN_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)(?:\s*,\s*(?:OU|CN|O|C|L|ST)\s+.+)$").unwrap()
    });
    static TRAILING_ENDORSED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\s+endorsed\s*$").unwrap());

    let trimmed = s.trim();
    if !trimmed.contains(", OU ")
        && !trimmed.contains(", CN ")
        && !trimmed.contains(", O ")
        && !trimmed.contains(", C ")
        && !trimmed.contains(", L ")
        && !trimmed.contains(", ST ")
    {
        return s.to_string();
    }

    let Some(cap) = X509_DN_TAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    let mut prefix = cap
        .name("prefix")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prefix.is_empty() {
        return s.to_string();
    }
    if let Some(cap2) = TRAILING_ENDORSED_RE.captures(&prefix) {
        prefix = cap2
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or(prefix);
    }
    prefix
}

fn strip_trailing_ampas_acronym(s: &str) -> String {
    static AMPAS_SUFFIX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\s+\(?A\.M\.P\.A\.S\.?\)?\s*$").unwrap());
    AMPAS_SUFFIX_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_javadoc_tags(s: &str) -> String {
    static JAVADOC_TAGS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\s+@(?:generated|version|since|param|return|see)\b.*$").unwrap()
    });
    JAVADOC_TAGS_RE.replace(s, "").trim().to_string()
}

fn strip_trailing_batch_comment_marker(s: &str) -> String {
    static BATCH_COMMENT_TAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?P<prefix>.+?)\.?\s+@?rem\b.*$").unwrap());
    let trimmed = s.trim();
    let Some(cap) = BATCH_COMMENT_TAIL_RE.captures(trimmed) else {
        return s.to_string();
    };
    cap.name("prefix")
        .map(|m| m.as_str().trim_end_matches(&[' ', '.'][..]).to_string())
        .unwrap_or_else(|| s.to_string())
}

fn strip_trailing_paren_years(s: &str) -> String {
    static PAREN_YEARS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^(?P<prefix>.+?)\s*\(\s*(?:19\d{2}|20\d{2})(?:\s*[-–]\s*(?:19\d{2}|20\d{2}|\d{2}))?(?:\s*,\s*(?:19\d{2}|20\d{2}))*\s*\)\s*$",
        )
        .unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = PAREN_YEARS_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    prefix.to_string()
}

fn strip_trailing_bare_c_copyright_clause(s: &str) -> String {
    static BARE_C_CLAUSE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s*\(c\)\s*(?:19\d{2}|20\d{2})\b.*$").unwrap()
    });
    let trimmed = s.trim();
    let Some(cap) = BARE_C_CLAUSE_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    prefix.to_string()
}

fn strip_trailing_single_digit_token(s: &str) -> String {
    static TRAILING_SINGLE_DIGIT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<prefix>.+?)\s+[1-9]\s*$").unwrap());
    let trimmed = s.trim();
    let Some(cap) = TRAILING_SINGLE_DIGIT_RE.captures(trimmed) else {
        return s.to_string();
    };
    let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
    if prefix.is_empty() {
        return s.to_string();
    }
    if prefix.split_whitespace().count() < 2 {
        return s.to_string();
    }
    if !prefix.chars().any(|c| c.is_alphabetic()) {
        return s.to_string();
    }
    prefix.to_string()
}

#[path = "refiner_author.rs"]
mod author;
#[path = "refiner_utils.rs"]
mod utils;

pub(crate) use author::looks_like_name_with_parenthesized_url;
pub use author::refine_author;
pub use utils::{
    remove_dupe_copyright_words, remove_some_extra_words_and_punct, strip_all_unbalanced_parens,
    strip_prefixes, strip_solo_quotes, strip_some_punct, strip_suffixes, strip_trailing_period,
};

#[cfg(test)]
use self::utils::{strip_leading_numbers, strip_unbalanced_parens};

use self::author::{normalize_angle_bracket_comma_spacing, strip_trailing_company_co_ltd};

use self::utils::{
    normalize_comma_spacing, normalize_whitespace, refine_names, remove_dupe_holder,
    strip_trailing_incomplete_as_represented_by, strip_trailing_url, strip_trailing_url_slash,
    truncate_long_words,
};

#[cfg(test)]
#[path = "refiner_test.rs"]
mod tests;
