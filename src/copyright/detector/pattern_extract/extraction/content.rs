// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub fn extract_glide_3dfx_copyright_notice(content: &str) -> Vec<CopyrightDetection> {
    static GLIDE_3DFX_COPYRIGHT_NOTICE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bcopyright\s+notice\s*\(3dfx\s+interactive,\s+inc\.\s+1999\)").unwrap()
    });

    let mut copyrights = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        if let Some(m) = GLIDE_3DFX_COPYRIGHT_NOTICE_RE.find(line) {
            let raw = m.as_str();
            if let Some(refined) = refine_copyright(raw) {
                copyrights.push(CopyrightDetection {
                    copyright: refined,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }

    copyrights
}

pub fn extract_spdx_filecopyrighttext_c_without_year(
    content: &str,
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static SPDX_COPYRIGHT_C_NO_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bSPDX-FileCopyrightText:\s*Copyright\s*\(c\)\s+(.+?)\s*$").unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    let mut seen_h: HashSet<(String, usize)> = existing_holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let trimmed = line.trim();
        let Some(caps) = SPDX_COPYRIGHT_C_NO_YEAR_RE.captures(trimmed) else {
            continue;
        };
        let tail = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if tail.is_empty() {
            continue;
        }

        let raw = format!("Copyright (c) {tail}");
        if let Some(refined) = refine_copyright(&raw) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(holder) = refine_holder(tail)
            && seen_h.insert((holder.clone(), ln))
        {
            holders.push(HolderDetection {
                holder,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_html_meta_name_copyright_content(
    content: &str,
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static META_COPYRIGHT_CONTENT_DQ_NAME_CONTENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)<meta\s+[^>]*\bname\s*=\s*"copyright"[^>]*\bcontent\s*=\s*"([^"]+)""#)
            .unwrap()
    });
    static META_COPYRIGHT_CONTENT_DQ_CONTENT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)<meta\s+[^>]*\bcontent\s*=\s*"([^"]+)"[^>]*\bname\s*=\s*"copyright""#)
            .unwrap()
    });
    static META_COPYRIGHT_CONTENT_SQ_NAME_CONTENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)<meta\s+[^>]*\bname\s*=\s*'copyright'[^>]*\bcontent\s*=\s*'([^']+)'")
            .unwrap()
    });
    static META_COPYRIGHT_CONTENT_SQ_CONTENT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)<meta\s+[^>]*\bcontent\s*=\s*'([^']+)'[^>]*\bname\s*=\s*'copyright'")
            .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    let mut seen_h: HashSet<(String, usize)> = existing_holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let raw = if let Some(caps) = META_COPYRIGHT_CONTENT_DQ_NAME_CONTENT_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else if let Some(caps) = META_COPYRIGHT_CONTENT_DQ_CONTENT_NAME_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else if let Some(caps) = META_COPYRIGHT_CONTENT_SQ_NAME_CONTENT_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else if let Some(caps) = META_COPYRIGHT_CONTENT_SQ_CONTENT_NAME_RE.captures(line) {
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        } else {
            continue;
        };

        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }

        if let Some(refined) = refine_copyright(raw) {
            copyrights.push(CopyrightDetection {
                copyright: refined.clone(),
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });

            if let Some(holder) =
                postprocess_transforms::derive_holder_from_simple_copyright_string(&refined)
                && seen_h.insert((holder.clone(), ln))
            {
                holders.push(HolderDetection {
                    holder,
                    start_line: LineNumber::new(ln).unwrap(),
                    end_line: LineNumber::new(ln).unwrap(),
                });
            }
        }
    }

    (copyrights, holders)
}

pub fn extract_markup_copyright_attributes(
    content: &str,
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static LEGAL_ATTR_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<[^>]*\bcopyright\s*=\s*(?:\"[^\"]*\"|'[^']*')[^>]*>"#).unwrap()
    });
    static LEGAL_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)\b(?P<name>copyright|company|holder|owner)\s*=\s*(?:\"(?P<dq>[^\"]*)\"|'(?P<sq>[^']*)')"#,
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    let mut seen_h: HashSet<(String, usize)> = existing_holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    for tag_match in LEGAL_ATTR_TAG_RE.find_iter(content) {
        let line = line_number_for_offset(content, tag_match.start());
        let mut copyright_attr = None;
        let mut holder_candidates = Vec::new();

        for attr in LEGAL_ATTR_RE.captures_iter(tag_match.as_str()) {
            let Some(name) = attr.name("name").map(|m| m.as_str().to_ascii_lowercase()) else {
                continue;
            };
            let value = attr
                .name("dq")
                .or_else(|| attr.name("sq"))
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim();
            if value.is_empty() {
                continue;
            }
            let normalized = normalize_markup_attribute_value(value);
            if normalized.is_empty() {
                continue;
            }

            match name.as_str() {
                "copyright" if copyright_attr.is_none() => {
                    copyright_attr = Some(normalized);
                }
                "copyright" => {}
                "company" | "holder" | "owner" => holder_candidates.push(normalized),
                _ => {}
            }
        }

        let Some(copyright_attr) = copyright_attr else {
            continue;
        };

        let copyright_raw = if copyright_attr.contains("copyright")
            || copyright_attr.contains("(c)")
            || copyright_attr.contains('©')
        {
            copyright_attr.clone()
        } else {
            format!("copyright {copyright_attr}")
        };

        let Some(refined) = refine_copyright(&copyright_raw) else {
            continue;
        };

        copyrights.push(CopyrightDetection {
            copyright: refined.clone(),
            start_line: line,
            end_line: line,
        });

        let mut emitted_holder = false;
        for holder_raw in holder_candidates {
            let Some(holder) = refine_holder_in_copyright_context(&holder_raw)
                .or_else(|| refine_holder(&holder_raw))
            else {
                continue;
            };
            if seen_h.insert((holder.clone(), line.get())) {
                holders.push(HolderDetection {
                    holder,
                    start_line: line,
                    end_line: line,
                });
            }
            emitted_holder = true;
        }

        if !emitted_holder
            && let Some(holder) =
                postprocess_transforms::derive_holder_from_simple_copyright_string(&refined)
            && seen_h.insert((holder.clone(), line.get()))
        {
            holders.push(HolderDetection {
                holder,
                start_line: line,
                end_line: line,
            });
        }
    }

    (copyrights, holders)
}

pub fn extract_changelog_timestamp_copyrights_from_content(
    content: &str,
    existing_holders: &[HolderDetection],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    static CHANGELOG_TS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})\s+(\d{2}:\d{2})\s+(.+?)\s*$").unwrap());

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    let mut seen_h: HashSet<(String, usize)> = existing_holders
        .iter()
        .map(|h| (h.holder.clone(), h.start_line.get()))
        .collect();

    let mut matches: Vec<(usize, String, String)> = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let ln = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(caps) = CHANGELOG_TS_RE.captures(trimmed) else {
            continue;
        };
        let date = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let time = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let tail = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        if date.is_empty() || time.is_empty() || tail.is_empty() {
            continue;
        }
        matches.push((ln, format!("{date} {time}"), tail.to_string()));
    }

    if matches.len() < 2 {
        return (Vec::new(), Vec::new());
    }

    let (ln, dt, tail) = &matches[0];
    let raw = format!("copyright {dt} {tail}");
    if let Some(refined) = refine_copyright(&raw) {
        copyrights.push(CopyrightDetection {
            copyright: refined,
            start_line: LineNumber::new(*ln).expect("invalid line number"),
            end_line: LineNumber::new(*ln).expect("invalid line number"),
        });
    }

    if let Some(holder) = refine_holder(tail)
        && seen_h.insert((holder.clone(), *ln))
    {
        holders.push(HolderDetection {
            holder,
            start_line: LineNumber::new(*ln).expect("invalid line number"),
            end_line: LineNumber::new(*ln).expect("invalid line number"),
        });
    }

    (copyrights, holders)
}

pub fn apply_european_community_copyright(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static EUROPEAN_COMMUNITY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:©|\(c\))\s*the\s+european\s+community\s+(\d{4})").unwrap()
    });

    let Some(cap) = EUROPEAN_COMMUNITY_RE.captures(content) else {
        return;
    };
    let Some(m) = cap.get(0) else {
        return;
    };
    let year = cap.get(1).map(|m| m.as_str());
    let Some(year) = year else {
        return;
    };

    let holder = "the European Community";
    let desired_copyright = format!("(c) {holder} {year}");
    let ln = line_number_index.line_number_at_offset(m.start());

    if !copyrights.iter().any(|c| c.copyright == desired_copyright) {
        copyrights.push(CopyrightDetection {
            copyright: desired_copyright,
            start_line: ln,
            end_line: ln,
        });
    }

    if !holders.iter().any(|h| h.holder == holder) {
        holders.push(HolderDetection {
            holder: holder.to_string(),
            start_line: ln,
            end_line: ln,
        });
    }
}

pub fn apply_javadoc_company_metadata(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static JAVADOC_P_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<p>\s*Copyright:\s*Copyright\s*\(c\)\s*(\d{4})\s*</p>").unwrap()
    });
    static JAVADOC_P_COMPANY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<p>\s*Company:\s*([^<\r\n]+)").unwrap());

    let Some(copy_cap) = JAVADOC_P_COPYRIGHT_RE.captures(content) else {
        return;
    };
    let year = copy_cap.get(1).map(|m| m.as_str());

    let company_val = JAVADOC_P_COMPANY_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim());

    let (Some(year), Some(company_val)) = (year, company_val) else {
        return;
    };

    let ln = copy_cap
        .get(0)
        .map(|m| line_number_index.line_number_at_offset(m.start()).get())
        .unwrap_or(1);

    let append_company_value = company_val.split_whitespace().count() >= 2;
    let company_holder = if append_company_value {
        format!("Company {company_val}")
    } else {
        "Company".to_string()
    };

    let base_holder = "Company";
    let base_copyright = format!("Copyright (c) {year} {base_holder}");
    let desired_copyright = format!("Copyright (c) {year} {company_holder}");

    copyrights.retain(|c| c.copyright != desired_copyright && c.copyright != base_copyright);
    holders.retain(|h| {
        h.holder != company_holder && (!append_company_value || h.holder != base_holder)
    });

    if !copyrights.iter().any(|c| c.copyright == desired_copyright) {
        copyrights.push(CopyrightDetection {
            copyright: desired_copyright,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }

    if !holders.iter().any(|h| h.holder == company_holder) {
        holders.push(HolderDetection {
            holder: company_holder,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }
}

pub fn extract_html_entity_year_range_copyrights(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
) {
    static COPY_ENTITY_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Copyright\s*&copy;?\s*(\d{4}\s*[-–]\s*\d{4})\b").unwrap()
    });
    static HEX_A9_ENTITY_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Copyright\s*&#xA9;?\s*(\d{4}\s*[-–]\s*\d{4})\b").unwrap()
    });
    static DEC_169_ENTITY_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)Copyright\s*&#169;?\s*(\d{4}\s*[-–]\s*\d{4})\b").unwrap()
    });
    static ARE_COPYRIGHT_C_RANGE_DOT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bare\s+copyright\s*\(c\)\s*(\d{4}\s*[-–]\s*\d{4})\s*\.").unwrap()
    });

    let is_terminator = |s: &str| {
        let tail = s.trim_start();
        if tail.is_empty() {
            return true;
        }
        matches!(
            tail.chars().next(),
            Some('<' | '"' | '\'' | ')' | ']' | '}' | '.' | ';' | ':')
        )
    };

    for cap in COPY_ENTITY_RANGE_RE.captures_iter(content) {
        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());
        if !is_terminator(&content[m.end()..]) {
            continue;
        }
        let range = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if range.is_empty() {
            continue;
        }
        let raw = format!("Copyright (c) {range}");
        if let Some(refined) = refine_copyright(&raw) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: ln,
                end_line: ln,
            });
        }
    }

    for cap in HEX_A9_ENTITY_RANGE_RE
        .captures_iter(content)
        .chain(DEC_169_ENTITY_RANGE_RE.captures_iter(content))
    {
        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());
        if !is_terminator(&content[m.end()..]) {
            continue;
        }
        let range = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if range.is_empty() {
            continue;
        }
        let raw = format!("(c) {range}");
        if let Some(refined) = refine_copyright(&raw) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: ln,
                end_line: ln,
            });

            let full = format!("Copyright (c) {range}");
            copyrights.retain(|c| !(c.start_line == ln && c.end_line == ln && c.copyright == full));
        }
    }

    for cap in ARE_COPYRIGHT_C_RANGE_DOT_RE.captures_iter(content) {
        let Some(m) = cap.get(0) else {
            continue;
        };
        let ln = line_number_index.line_number_at_offset(m.start());
        let range = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if range.is_empty() {
            continue;
        }
        let raw = format!("Copyright (c) {range}");
        if let Some(refined) = refine_copyright(&raw) {
            copyrights.push(CopyrightDetection {
                copyright: refined,
                start_line: ln,
                end_line: ln,
            });
        }
    }
}

pub fn extract_xml_copyright_tag_c_lines(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.to_ascii_lowercase().contains("<copyright") {
        return;
    }

    static BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<\s*copyright\b[^>]*>(?P<body>.*?)</\s*copyright\s*>").unwrap()
    });
    static C_SEG_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\(c\)\s*(?P<body>.+)").unwrap());
    static ALL_RIGHTS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i),?\s*all\s+rights\s+reserved\.?\s*$").unwrap());

    for cap in BLOCK_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let inner = cap.name("body").map(|m| m.as_str()).unwrap_or("");
        if inner.is_empty() {
            continue;
        }

        let mut bodies: Vec<String> = Vec::new();
        for raw_line in inner.lines() {
            let prepared = crate::copyright::prepare::prepare_text_line(raw_line);
            let line = prepared.trim();
            if line.is_empty() {
                continue;
            }
            let Some(c_cap) = C_SEG_RE.captures(line) else {
                continue;
            };
            let mut body = c_cap
                .name("body")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if body.is_empty() {
                continue;
            }
            body = ALL_RIGHTS_RE.replace(&body, "").into_owned();
            body = body
                .trim()
                .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
                .to_string();
            if body.is_empty() {
                continue;
            }
            bodies.push(body);
        }

        if bodies.len() < 2 {
            continue;
        }

        let combined = bodies
            .iter()
            .map(|b| format!("(c) {b}"))
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(cr) = refine_copyright(&combined) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: ln,
                end_line: ln,
            });
        }

        let combined_holder = bodies.join(" ");
        if let Some(h) = refine_holder_in_copyright_context(&combined_holder) {
            holders.push(HolderDetection {
                holder: h,
                start_line: ln,
                end_line: ln,
            });
        }

        let mut to_remove: HashSet<String> = HashSet::new();
        for b in &bodies {
            to_remove.insert(b.clone());
            to_remove.insert(b.trim_end_matches('.').to_string());
        }
        holders.retain(|h| !to_remove.contains(&h.holder));
    }
}

pub fn extract_html_anchor_copyright_url(
    content: &str,
    line_number_index: &LineNumberIndex,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    if !content.to_ascii_lowercase().contains("href=") {
        return (Vec::new(), Vec::new());
    }

    static A_HREF_COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)<\s*a\b[^>]*\bhref\s*=\s*['\"](?P<url>https?://[^'\">]+)['\"][^>]*>\s*copyright\s*</\s*a\s*>"#,
        )
        .unwrap()
    });
    static COPY_SYMBOL_A_HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)(?:&copy;|&#169;|&#xa9;|&#xA9;|\(c\)|©)\s*<\s*a\b[^>]*\bhref\s*=\s*(?:\\?['\"])(?P<url>https?://[^\\'\">]+)(?:\\?['\"])[^>]*>\s*(?P<text>[^<]+?)\s*</\s*a\s*>"#,
        )
        .unwrap()
    });

    let mut copyrights = Vec::new();
    let mut holders = Vec::new();

    for cap in A_HREF_COPYRIGHT_RE.captures_iter(content) {
        let start_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let end_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.end()))
            .unwrap_or(start_line);
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or("").trim();
        if url.is_empty() {
            continue;
        }
        let url = url.split('#').next().unwrap_or(url).trim();
        if url.is_empty() {
            continue;
        }

        let cr = format!("copyright {url}");
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line,
            end_line,
        });

        let holder = url.to_string();
        holders.push(HolderDetection {
            holder,
            start_line,
            end_line,
        });
    }

    for cap in COPY_SYMBOL_A_HREF_RE.captures_iter(content) {
        let start_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let end_line = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.end()))
            .unwrap_or(start_line);
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or("").trim();
        let holder = cap.name("text").map(|m| m.as_str()).unwrap_or("").trim();
        if url.is_empty() || holder.is_empty() {
            continue;
        }
        let url = url.split('#').next().unwrap_or(url).trim();
        if url.is_empty() {
            continue;
        }

        let cr = format!("(c) {url} {holder}");
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line,
            end_line,
        });

        let holder = holder.to_string();
        holders.push(HolderDetection {
            holder,
            start_line,
            end_line,
        });
    }

    (copyrights, holders)
}

pub fn extract_html_icon_class_copyrights(
    content: &str,
    line_number_index: &LineNumberIndex,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    let lower = content.to_ascii_lowercase();
    if !lower.contains("fa-copyright") && !lower.contains("glyphicon-copyright-mark") {
        return;
    }

    static FA_AUTHORS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?is)\bcopyright\b(?P<middle>.*?)\bfa-copyright\b(?P<tail>.*?)\b(?P<year>19\d{2}|20\d{2})\b\s+by\s+the\s+authors\b",
        )
        .unwrap()
    });
    static GLYPHICON_DALEGROUP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)\bcopyright\b(?P<middle>.*?)\bglyphicon-copyright-mark\b(?P<tail>.*?)<\s*a\b[^>]*\bhref\s*=\s*['\"](?P<url>https?://[^'\">]+)['\"]"#,
        )
        .unwrap()
    });
    static GLYPHICON_RUBIX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?is)\bcopyright\b(?P<middle>.*?)\bglyphicon-copyright-mark\b(?P<tail>.*?)\b(?P<years>\d{4}\s*[-–]\s*\d{4})\b\s+(?P<name>Rubix)\b",
        )
        .unwrap()
    });

    for cap in FA_AUTHORS_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
        if year.is_empty() {
            continue;
        }
        let cr = format!("Copyright fa-copyright {year} by the authors");
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: ln,
            end_line: ln,
        });
        let holder = "fa-copyright by the authors".to_string();
        holders.push(HolderDetection {
            holder,
            start_line: ln,
            end_line: ln,
        });

        let simple = format!("Copyright {year} by the authors");
        copyrights.retain(|c| c.copyright != simple);
        holders.retain(|h| h.holder != "the authors");
    }

    for cap in GLYPHICON_DALEGROUP_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let url = cap.name("url").map(|m| m.as_str()).unwrap_or("").trim();
        if url.is_empty() {
            continue;
        }
        let url = url.split('#').next().unwrap_or(url).trim_end_matches('/');
        if url.is_empty() {
            continue;
        }

        let cr = format!("Copyright glyphicon-copyright-mark {url}");
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: ln,
            end_line: ln,
        });
        let holder = "glyphicon-copyright-mark".to_string();
        holders.push(HolderDetection {
            holder,
            start_line: ln,
            end_line: ln,
        });

        copyrights.retain(|c| c.copyright != "Copyright Dalegroup");
        holders.retain(|h| h.holder != "Dalegroup");
    }

    for cap in GLYPHICON_RUBIX_RE.captures_iter(content) {
        let ln = cap
            .get(0)
            .map(|m| line_number_index.line_number_at_offset(m.start()))
            .unwrap_or(LineNumber::ONE);
        let years = cap.name("years").map(|m| m.as_str()).unwrap_or("").trim();
        if years.is_empty() {
            continue;
        }
        let cr = format!("Copyright glyphicon-copyright-mark {years} Rubix");
        copyrights.push(CopyrightDetection {
            copyright: cr,
            start_line: ln,
            end_line: ln,
        });

        let holder = "glyphicon-copyright-mark Rubix".to_string();
        holders.push(HolderDetection {
            holder,
            start_line: ln,
            end_line: ln,
        });

        let simple = format!("Copyright {years} Rubix");
        copyrights.retain(|c| c.copyright != simple);
        holders.retain(|h| h.holder != "Rubix");
    }
}
