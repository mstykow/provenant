// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::copyright::line_tracking::PreparedLineCache;
use crate::copyright::types::AuthorDetection;
use crate::models::LineNumber;

#[test]
fn test_author_colon_multiline_keeps_emails() {
    let input = "/*\n * Authors: Jorge Cwik, <jorge@laser.satlink.net>\n *\t\tArnt Gulbrandsen, <agulbra@nvg.unit.no>\n */\n";

    let raw_lines: Vec<&str> = input.lines().collect();
    let prepared_cache = PreparedLineCache::new(&raw_lines).materialize();
    let mut extracted: Vec<AuthorDetection> = Vec::new();
    extract_author_colon_blocks(&prepared_cache, &mut extracted);
    assert!(
        extracted.iter().any(|ad| ad.author
            == "Jorge Cwik, <jorge@laser.satlink.net> Arnt Gulbrandsen, <agulbra@nvg.unit.no>"),
        "Expected direct author-colon extraction to keep emails, got: {:?}",
        extracted.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );

    let (_c, _h, a) = super::super::detect_copyrights_from_text(input);

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
    let prepared_cache = PreparedLineCache::new(&raw_lines).materialize();
    let mut extracted: Vec<AuthorDetection> = Vec::new();
    extract_author_colon_blocks(&prepared_cache, &mut extracted);
    assert!(
        extracted
            .iter()
            .any(|ad| { ad.author == "Richard Walker, Jamie Honan, Michael Hunold" }),
        "Expected empty-tail Authors: block to merge following roster lines, got: {:?}",
        extracted.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );

    let (_c, _h, a) = super::super::detect_copyrights_from_text(input);
    assert!(
        a.iter()
            .any(|ad| ad.author == "Richard Walker, Jamie Honan, Michael Hunold"),
        "Expected pipeline to keep merged roster author block, got: {:?}",
        a.iter().map(|ad| &ad.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_authors_from_dense_name_email_list() {
    let input = "John Doe <john@example.com>\nJane Smith <jane@example.com>\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

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
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Tokio Contributors <team@tokio.rs>"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_toml_singular_author_array_with_handle() {
    let input = "author = [\"Tom Breloff (@tbreloff)\"]\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Tom Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_created_by_author_with_handle() {
    let input = "Created by Tom Breloff (@tbreloff)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Tom Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_primary_author_with_handle() {
    let input = "Primary author: Josef Heinen (@jheinen)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Josef Heinen"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_original_author_with_handle() {
    let input = "Original author: Thomas Breloff (@tbreloff)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Thomas Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_primary_package_author_with_handle() {
    let input = "Primary PlotlyJS.jl author: Spencer Lyon (@spencerlyon2)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Spencer Lyon"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_author_colon_inline_roster_with_handles() {
    let input = "authors: Benoit Pasquier (@briochemc) - David Gustavsson (@gustaphe) - Jan Weidner (@jw3126)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Benoit Pasquier"),
        "authors: {authors:?}"
    );
    assert!(
        authors.iter().any(|a| a.author == "David Gustavsson"),
        "authors: {authors:?}"
    );
    assert!(
        authors.iter().any(|a| a.author == "Jan Weidner"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_markdown_heading_original_author_with_handle() {
    let input = "### Original author: Thomas Breloff (@tbreloff)\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Thomas Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_original_author_before_maintained_by_clause() {
    let input =
        "### Original author: Thomas Breloff (@tbreloff), maintained by the JuliaPlots members\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Thomas Breloff"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_extract_originally_implemented_by_author_with_parenthesized_email() {
    let input = "LALR(1) support was originally implemented by Elias Ioup (ezioup@alumni.uchicago.edu),\nusing the algorithm found in Aho, Sethi, and Ullman.\n";
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Elias Ioup (ezioup@alumni.uchicago.edu)"),
        "authors: {authors:?}"
    );
}

#[test]
fn test_was_developed_by_multiline_author_is_extracted() {
    let input = "1. GOST R 34.11-2012 was developed by the Center for Information\nProtection and Special Communications of the Federal Security\nService of the Russian Federation with participation of the Open\n";

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
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
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
    assert!(
        authors
            .iter()
            .any(|a| a.author == "Andy Polyakov <appro@openssl.org>"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_written_by_author_with_contact_after_copyright_is_kept() {
    let input = concat!(
        "Copyright 2021-2025 The OpenSSL Project Authors. All Rights Reserved.\n",
        "\n",
        "Written by Ben Avison <bavison@riscosopen.org> for the OpenSSL\n",
        "project. Rights for redistribution and usage in source and binary\n",
        "forms are granted according to the OpenSSL license.\n",
    );
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
    assert!(
        authors
            .iter()
            .any(|a| a.author == "Ben Avison <bavison@riscosopen.org>"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_originally_written_by_for_project_block_without_contact_is_extracted() {
    let input = concat!(
        "Originally written by Christophe Renou and Peter Sylvester,\n",
        "for the EdelKey project.\n",
    );
    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
    assert!(
        authors
            .iter()
            .any(|a| a.author == "Christophe Renou and Peter Sylvester"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_prose_snippet_does_not_report_laboriously_took_the_trouble_as_author() {
    let input = concat!(
        "<para>the authors laboriously took the trouble of searching for workarounds ",
        "to make these compilers happy</para>",
    );

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(authors.is_empty(), "authors: {:?}", authors);
}

#[test]
fn test_developed_by_sentence_author_is_extracted() {
    let input = "developed by the U.S. Government. BAE Systems is enhancing and supporting the SMP";

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
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
    let input = "to acknowledge that it was\n      developed by the National Center for Supercomputing Applications at the University of Illinois at Urbana-Champaign and to credit the\n      contributors.";

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
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

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);
    assert!(
        authors.iter().any(
            |a| a.author == "Daniel Vaz Gaspar. (https://github.com/dpgaspar/Flask-AppBuilder)"
        ),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_author_colon_block_stops_at_status_and_devices_metadata() {
    let input = "Author: ds\nStatus: works in immediate mode\nDevices: [standard] parallel port\n";

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

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

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

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

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Philip Hazel"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
    );
}

#[test]
fn test_with_additional_hacking_by_keeps_parenthesized_email_author() {
    let input = "With additional hacking by Jeffrey Kuskin (jsk@mojave.stanford.edu)\n";

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

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

    let (_copyrights, _holders, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors
            .iter()
            .any(|a| a.author == "Thomas Sailer (sailer@ife.ee.ethz.ch)"),
        "authors: {:?}",
        authors.iter().map(|a| &a.author).collect::<Vec<_>>()
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

    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);
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
fn test_rst_field_author_and_maintainer_extracts_single_author() {
    let input = ":License:\t\tGPLv2\n:Author & Maintainer:\tMiguel Ojeda <ojeda@kernel.org>\n:Date:\t\t\t2006-10-27\n";

    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);
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
fn test_dash_bullet_changelog_lines_extract_individual_authors() {
    let input = "- Written by Mydraal <vulpyne@vulpyne.net>\n- Updated by Adam Sulmicki <adam@cfar.umd.edu>\n- Updated by Jeremy M. Dolan <jmd@turbogeek.org> 2001/01/28 10:15:59\n- Added to by Crutcher Dunnavant <crutcher+kernel@datastacks.com>\n";

    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);
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

    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);
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

    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);
    assert!(authors.is_empty(), "authors: {authors:?}");
}

#[test]
fn test_detect_author() {
    let (c, h, a) = super::super::detect_copyrights_from_text("Written by John Doe");
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
    let (c, h, a) = super::super::detect_copyrights_from_text(text);

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
    let (_c, _h, a) = super::super::detect_copyrights_from_text(text);

    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Chris Kohlhoff");
}

#[test]
fn test_detect_author_from_xml_author_attribute_decodes_entities() {
    let text = r#"<note author="Joaqu&#237;n M L&#243;pez Mu&#241;oz">Compiler bug.</note>"#;
    let (_c, _h, a) = super::super::detect_copyrights_from_text(text);

    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Joaquín M López Muñoz");
}

#[test]
fn test_detect_author_from_repeated_xml_author_attributes_keeps_multiple_occurrences() {
    let text = r#"<mark-expected-failures>
<note author="Aleksey Gurtovoy" refid="4"/>
<note author="Aleksey Gurtovoy" refid="19"/>
</mark-expected-failures>"#;
    let (_c, _h, a) = super::super::detect_copyrights_from_text(text);

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
    let (_c, _h, a) = super::super::detect_copyrights_from_text(text);

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
    let (_c, _h, a) = super::super::detect_copyrights_from_text(text);
    let names: Vec<&str> = a.iter().map(|d| d.author.as_str()).collect();

    assert!(names.contains(&"John Maddock"), "authors: {names:?}");
    assert!(names.contains(&"Joel de Guzman"), "authors: {names:?}");
    assert!(names.contains(&"Eric Niebler"), "authors: {names:?}");
    assert!(names.contains(&"Matias Capeletto"), "authors: {names:?}");
}

#[test]
fn test_detect_created_by_current_user_comment_is_not_author() {
    let text = "Get the IDs of pipelines created by the current user on the same branch.";
    let (_c, _h, a) = super::super::detect_copyrights_from_text(text);
    assert!(a.is_empty(), "authors: {a:?}");
}

#[test]
fn test_detect_author_written_by() {
    let (_c, _h, a) = super::super::detect_copyrights_from_text("Written by Jane Smith");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Jane Smith");
    assert_eq!(a[0].start_line, LineNumber::ONE);
    assert_eq!(a[0].end_line, LineNumber::ONE);
}

#[test]
fn test_detect_author_maintained_by() {
    let (_c, _h, a) = super::super::detect_copyrights_from_text("Maintained by Bob Jones");
    assert_eq!(a.len(), 1, "Should detect one author, got: {:?}", a);
    assert_eq!(a[0].author, "Bob Jones");
    assert_eq!(a[0].start_line, LineNumber::ONE);
    assert_eq!(a[0].end_line, LineNumber::ONE);
}

#[test]
fn test_detect_author_authors_keyword() {
    let (_c, _h, a) = super::super::detect_copyrights_from_text("Authors John Smith");
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
    let (_c, _h, a) = super::super::detect_copyrights_from_text("Contributors Jane Doe");
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
    let (_c, _h, a) =
        super::super::detect_copyrights_from_text("SPDX-FileContributor: Alice Johnson");
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
fn test_name_contributed_line_is_detected_as_author() {
    let input = "\\author{\nRandall Prium contributed most of the implementation of\n\\code{cut_width()}.\n}";
    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.iter().any(|a| a.author == "Randall Prium"),
        "expected Randall Prium author, got: {:?}",
        authors
    );
}

#[test]
fn test_name_contributed_line_ignores_portions_holder_phrase() {
    let input = "Copyright (c) 2006, Industrial Light & Magic, a division of Lucasfilm\nEntertainment Company Ltd. Portions contributed and copyright held by\nothers as indicated. All rights reserved.";
    let (_c, _h, authors) = super::super::detect_copyrights_from_text(input);

    assert!(
        authors.is_empty(),
        "expected no authors, got: {:?}",
        authors
    );
}

#[test]
fn test_date_by_author() {
    let content = "\
Copyright (c) 1998 Softweyr LLC.  All rights reserved.
strtok_r, from Berkeley strtok
Oct 13, 1998 by Wes Peters <wes@softweyr.com>";
    let (_c, _h, a) = super::super::detect_copyrights_from_text(content);
    assert!(
        a.iter().any(|a| a.author.contains("Wes Peters")),
        "Should detect Wes Peters as author, got: {:?}",
        a
    );
}

#[test]
fn test_originally_by_author() {
    let content = "\
#   Copyright 1996-2006 Free Software Foundation, Inc.
#   Taken from GNU libtool, 2001
#   Originally by Gordon Matzigkeit <gord@gnu.ai.mit.edu>, 1996";
    let (_c, _h, a) = super::super::detect_copyrights_from_text(content);
    assert!(
        a.iter().any(|a| a.author.contains("Gordon Matzigkeit")),
        "Should detect Gordon Matzigkeit as author, got: {:?}",
        a
    );
}
