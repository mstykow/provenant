// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub fn split_reworked_by_suffixes(
    content: &str,
    copyrights: &mut [CopyrightDetection],
    holders: &mut [HolderDetection],
    authors: &mut Vec<AuthorDetection>,
) {
    if !content.to_ascii_lowercase().contains("re-worked by") {
        return;
    }
    static REWORKED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?P<prefix>.+?)\s+Re-worked\s+by\s+(?P<who>.+)$").unwrap()
    });

    for det in copyrights.iter_mut() {
        let current = det.copyright.clone();
        let Some(cap) = REWORKED_RE.captures(current.as_str()) else {
            continue;
        };
        let prefix = cap
            .name("prefix")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let who = cap
            .name("who")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        if prefix.is_empty() || who.is_empty() {
            continue;
        }
        if let Some(refined) = refine_copyright(&prefix) {
            det.copyright = refined;
        } else {
            det.copyright = prefix.to_string();
        }
        if let Some(author) = refine_author(&who) {
            authors.push(AuthorDetection {
                author,
                start_line: det.start_line,
                end_line: det.end_line,
            });
        }
    }

    for det in holders.iter_mut() {
        if let Some(cap) = REWORKED_RE.captures(det.holder.as_str()) {
            let prefix = cap.name("prefix").map(|m| m.as_str()).unwrap_or("").trim();
            if let Some(refined) = refine_holder_in_copyright_context(prefix) {
                det.holder = refined;
            }
        }
    }
}

pub fn extract_following_authors_holders(
    raw_lines: &[&str],
    prepared_cache: &PreparedLines<'_>,
) -> Vec<AuthorDetection> {
    if raw_lines.is_empty() {
        return Vec::new();
    }

    static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*copyright\b.*\bby\s+the\s+following\s+authors\b.*$")
            .expect("valid following authors header regex")
    });

    let mut new_authors = Vec::new();

    let mut line_number = LineNumber::ONE;
    while let Some(header_line) = prepared_cache.line(line_number) {
        if header_line.prepared.is_empty() || !HEADER_RE.is_match(header_line.prepared) {
            line_number = line_number.next();
            continue;
        }

        let mut extracted_any = false;
        let mut next_line_number = header_line.line_number.next();
        while let Some(next_line) = prepared_cache.line(next_line_number) {
            let raw = next_line.raw;
            if raw.trim().is_empty() {
                break;
            }
            if !raw.trim_start().starts_with('-') {
                break;
            }
            let mut item = raw.trim_start().trim_start_matches('-').trim().to_string();
            item = crate::copyright::detector::token_utils::normalize_whitespace(&item);
            if !item.is_empty()
                && let Some(author) = refine_author(&item)
            {
                new_authors.push(AuthorDetection {
                    author,
                    start_line: next_line.line_number,
                    end_line: next_line.line_number,
                });
                extracted_any = true;
            }
            next_line_number = next_line_number.next();
        }

        line_number = if extracted_any {
            next_line_number
        } else {
            header_line.line_number.next()
        };
    }

    new_authors
}

pub fn drop_created_by_camelcase_identifier_authors(
    prepared_cache: &PreparedLines<'_>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() || authors.is_empty() {
        return;
    }

    static CREATED_BY_CAMELCASE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcreated\s+by\s+a\s+(?P<name>[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+)\b")
            .expect("valid created-by CamelCase regex")
    });

    let mut by_line: HashMap<usize, HashSet<String>> = HashMap::new();
    for line in prepared_cache.iter_non_empty() {
        for cap in CREATED_BY_CAMELCASE_RE.captures_iter(line.prepared) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            by_line
                .entry(line.line_number.get())
                .or_default()
                .insert(name.to_ascii_lowercase());
        }
    }

    if by_line.is_empty() {
        return;
    }

    authors.retain(|author| {
        if author.start_line.get() != author.end_line.get() {
            return true;
        }
        let Some(names) = by_line.get(&author.start_line.get()) else {
            return true;
        };

        let value = author.author.trim().to_ascii_lowercase();
        for name in names {
            if value == *name
                || value == format!("{name} in")
                || value == format!("{name} for")
                || value == format!("{name} to")
                || value == format!("{name} from")
                || value == format!("{name} by")
            {
                return false;
            }
        }
        true
    });
}

pub fn merge_implemented_by_lines(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }
    if !prepared_cache.contains_ci("implemented by") {
        return;
    }

    static COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s*\(c\)\s*(?P<year>\d{4})\s*,\s*(?P<holder>.+)$").unwrap()
    });
    static IMPLEMENTED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^implemented\s+by\s+(?P<tail>.+)$").unwrap());
    static EMAIL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?P<email>[^\s<>]+@[^\s<>]+)").unwrap());

    let mut merged: Vec<(LineNumber, String, String, HashSet<String>)> = Vec::new();

    for (first, second) in prepared_cache.adjacent_pairs() {
        let line = first.prepared.trim().trim_start_matches('*').trim_start();
        let Some(cap) = COPY_RE.captures(line) else {
            continue;
        };
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() || holder_raw.is_empty() {
            continue;
        }

        let next = second.prepared.trim().trim_start_matches('*').trim_start();
        let Some(cap2) = IMPLEMENTED_RE.captures(next) else {
            continue;
        };
        let tail = cap2.name("tail").map(|m| m.as_str()).unwrap_or("");
        let mut emails: Vec<String> = EMAIL_RE
            .captures_iter(tail)
            .filter_map(|c| c.name("email").map(|m| m.as_str().to_string()))
            .collect();
        if emails.is_empty() {
            continue;
        }
        let first_email = emails.remove(0);
        let email_set: HashSet<String> = emails
            .into_iter()
            .chain(std::iter::once(first_email.clone()))
            .collect();

        let cr = format!("Copyright (c) {year}, {holder_raw} Implemented by {first_email}");
        let holder = format!("{holder_raw} Implemented by");
        merged.push((first.line_number, cr, holder, email_set));
    }

    if merged.is_empty() {
        return;
    }

    for (line_number, cr_raw, holder_raw, emails) in merged {
        let Some(cr) = refine_copyright(&cr_raw) else {
            continue;
        };

        let cr_first = cr.split_whitespace().next().unwrap_or("");

        for det in copyrights.iter_mut() {
            if det.start_line == line_number
                && det.copyright.starts_with("Copyright (c)")
                && det.copyright.contains(",")
                && det.copyright.contains(cr_first)
            {
                det.copyright = cr.clone();
            }
        }
        if !copyrights.iter().any(|c| c.copyright == cr) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: line_number,
                end_line: line_number.next(),
            });
        }

        holders.retain(|h| {
            !(h.start_line == line_number
                && h.holder == holder_raw.trim_end_matches(" Implemented by"))
        });
        if let Some(h) = refine_holder(&holder_raw)
            && !holders
                .iter()
                .any(|x| x.holder == h && x.start_line == line_number)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: line_number,
                end_line: line_number.next(),
            });
        }

        authors.retain(|a| !emails.contains(&a.author));
    }
}

pub fn split_written_by_copyrights_into_holder_prefixed_clauses(
    prepared_cache: &PreparedLines<'_>,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
    authors: &mut Vec<AuthorDetection>,
) {
    if prepared_cache.is_empty() {
        return;
    }

    static WRITTEN_BY_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)written\s+by\s+(?P<name>[^,]+),\s*copyright\s+(?P<years>\d{4}(?:\s*,\s*\d{4})*)",
        )
        .unwrap()
    });

    let mut added_any = false;
    for prepared_line in prepared_cache.iter_non_empty() {
        for cap in WRITTEN_BY_COPY_RE.captures_iter(prepared_line.prepared) {
            let name = cap.name("name").map(|m| m.as_str()).unwrap_or("").trim();
            let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
            if name.is_empty() || years.is_empty() {
                continue;
            }
            let cr_raw = format!("{name}, Copyright {years}");
            let Some(cr) = refine_copyright(&cr_raw) else {
                continue;
            };
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: prepared_line.line_number,
                end_line: prepared_line.line_number,
            });
            if let Some(h) = refine_holder(name) {
                holders.push(HolderDetection {
                    holder: h,
                    start_line: prepared_line.line_number,
                    end_line: prepared_line.line_number,
                });
            }
            added_any = true;
        }
    }

    if !added_any {
        return;
    }

    copyrights.retain(|c| {
        let lower = c.copyright.to_ascii_lowercase();
        !(lower.contains("and by julian cowley")
            || lower == "copyright 1991, 1992, 1993, and by julian cowley")
    });
    holders.retain(|h| h.holder != "Julian Cowley");
    authors.retain(|a| a.author != "Linus Torvalds" && a.author != "Theodore Ts'o");
}
