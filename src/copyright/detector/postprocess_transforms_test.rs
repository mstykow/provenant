// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::copyright::types::AuthorDetection;
use crate::models::LineNumber;

#[test]
fn test_drop_shadowed_year_only_prefix_same_start_line() {
    let mut copyrights = vec![
        CopyrightDetection {
            copyright: "(c) 2001".to_string(),
            start_line: LineNumber::new(5).unwrap(),
            end_line: LineNumber::new(5).unwrap(),
        },
        CopyrightDetection {
            copyright: "(c) 2001 Foo Bar".to_string(),
            start_line: LineNumber::new(5).unwrap(),
            end_line: LineNumber::new(5).unwrap(),
        },
    ];
    drop_shadowed_year_only_copyright_prefixes_same_start_line(&mut copyrights);
    assert!(
        !copyrights.iter().any(|c| c.copyright == "(c) 2001"),
        "should drop year-only prefix when longer exists: {copyrights:?}"
    );
}

#[test]
fn test_drop_shadowed_c_sign_variants_unit() {
    let mut c = vec![
        CopyrightDetection {
            copyright: "Copyright 2007, 2010 Linux Foundation".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
        CopyrightDetection {
            copyright: "Copyright (c) 2007, 2010 Linux Foundation".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
        CopyrightDetection {
            copyright: "Copyright 1995-2010 Jean-loup Gailly and Mark Adler".to_string(),
            start_line: LineNumber::new(10).unwrap(),
            end_line: LineNumber::new(10).unwrap(),
        },
        CopyrightDetection {
            copyright: "Copyright (c) 1995-2010 Jean-loup Gailly and Mark Adler".to_string(),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
        },
    ];
    drop_shadowed_c_sign_variants(&mut c);
    let mut got: Vec<&str> = c.iter().map(|d| d.copyright.as_str()).collect();
    got.sort();
    let mut expected = vec![
        "Copyright (c) 1995-2010 Jean-loup Gailly and Mark Adler",
        "Copyright (c) 2007, 2010 Linux Foundation",
        "Copyright 1995-2010 Jean-loup Gailly and Mark Adler",
    ];
    expected.sort();
    assert_eq!(got, expected, "After dropping variants, got: {c:?}");
}

#[test]
fn test_refine_final_authors_keeps_handle_suffixed_maintainer() {
    let mut authors = vec![AuthorDetection {
        author: "Tianon Gravi <admwiggin@gmail.com> (@tianon)".to_string(),
        start_line: LineNumber::ONE,
        end_line: LineNumber::ONE,
    }];

    refine_final_authors(&mut authors);

    assert_eq!(
        authors,
        vec![AuthorDetection {
            author: "Tianon Gravi <admwiggin@gmail.com> (@tianon)".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        }]
    );
}

#[test]
fn test_refine_final_authors_keeps_structured_metadata_collectives() {
    let mut authors = vec![
        AuthorDetection {
            author: "gRPC authors".to_string(),
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
        },
        AuthorDetection {
            author: "Meta".to_string(),
            start_line: LineNumber::new(2).unwrap(),
            end_line: LineNumber::new(2).unwrap(),
        },
        AuthorDetection {
            author: "The libunwind project".to_string(),
            start_line: LineNumber::new(3).unwrap(),
            end_line: LineNumber::new(3).unwrap(),
        },
        AuthorDetection {
            author: "S2Geometry".to_string(),
            start_line: LineNumber::new(4).unwrap(),
            end_line: LineNumber::new(4).unwrap(),
        },
    ];

    refine_final_authors(&mut authors);

    assert_eq!(
        authors
            .iter()
            .map(|author| author.author.as_str())
            .collect::<Vec<_>>(),
        vec![
            "gRPC authors",
            "Meta",
            "The libunwind project",
            "S2Geometry"
        ]
    );
}
