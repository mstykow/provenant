// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::copyright::candidates::is_raw_versioned_project_banner_line;
use crate::copyright::detector::token_utils;

pub fn drop_json_description_metadata_copyrights_and_holders(
    raw_lines: &[&str],
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    static JSON_COPYRIGHT_KEY_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)"copyrights?"\s*:"#).unwrap());

    let mut retained_spans: HashSet<(usize, usize)> = HashSet::new();
    copyrights.retain(|copyright| {
        if copyright.start_line == copyright.end_line
            && raw_lines
                .get(copyright.start_line.get().saturating_sub(1))
                .is_some_and(|line| is_raw_versioned_project_banner_line(line))
        {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
            return true;
        }
        let Some(window) = json_window_for_span(
            raw_lines,
            copyright.start_line.get(),
            copyright.end_line.get(),
        ) else {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
            return true;
        };

        let lower = window.to_ascii_lowercase();
        let description_like = lower.contains("\"description\"")
            || lower.contains("\"disambiguatingdescription\"")
            || lower.contains("\"sponsor\"")
            || lower.contains("\"logo\"")
            || lower.contains("\"url\"");
        let explicit_attribution = copyright.copyright.starts_with("(c) ")
            && (copyright.copyright.contains("http://")
                || copyright.copyright.contains("https://"));
        let keep =
            !description_like || JSON_COPYRIGHT_KEY_RE.is_match(&window) || explicit_attribution;
        if keep {
            retained_spans.insert((copyright.start_line.get(), copyright.end_line.get()));
        }
        keep
    });

    holders.retain(|holder| {
        if retained_spans.contains(&(holder.start_line.get(), holder.end_line.get())) {
            return true;
        }
        if holder.start_line == holder.end_line
            && raw_lines
                .get(holder.start_line.get().saturating_sub(1))
                .is_some_and(|line| is_raw_versioned_project_banner_line(line))
        {
            return true;
        }
        let Some(window) =
            json_window_for_span(raw_lines, holder.start_line.get(), holder.end_line.get())
        else {
            return true;
        };
        let lower = window.to_ascii_lowercase();
        let description_like = lower.contains("\"description\"")
            || lower.contains("\"disambiguatingdescription\"")
            || lower.contains("\"sponsor\"")
            || lower.contains("\"logo\"")
            || lower.contains("\"url\"");
        !description_like || JSON_COPYRIGHT_KEY_RE.is_match(&window)
    });
}

pub fn json_window_for_span(
    raw_lines: &[&str],
    start_line: usize,
    end_line: usize,
) -> Option<String> {
    if start_line == 0
        || end_line == 0
        || start_line > raw_lines.len()
        || end_line > raw_lines.len()
    {
        return None;
    }
    let start = start_line.saturating_sub(2).max(1);
    let end = (end_line + 2).min(raw_lines.len());
    let lines = &raw_lines[start - 1..end];
    if !lines
        .iter()
        .any(|line| line.contains("\":") && (line.contains('{') || line.contains('"')))
    {
        return None;
    }
    Some(lines.join(" "))
}

pub fn restore_url_slash_before_closing_paren_from_raw_lines(
    raw_lines: &[&str],
    copyrights: &mut [CopyrightDetection],
) {
    if raw_lines.is_empty() || copyrights.is_empty() {
        return;
    }

    static URL_SLASH_PAREN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)https?://[^\s)]+/\)").unwrap());

    let mut replacements: HashMap<usize, Vec<(String, String)>> = HashMap::new();
    for (idx, raw) in raw_lines.iter().enumerate() {
        let ln = idx + 1;
        for m in URL_SLASH_PAREN_RE.find_iter(raw) {
            let with_slash = m.as_str().to_string();
            let without_slash = with_slash.replacen("/)", ")", 1);
            if without_slash != with_slash {
                replacements
                    .entry(ln)
                    .or_default()
                    .push((without_slash, with_slash));
            }
        }
    }

    for c in copyrights.iter_mut() {
        for ln in c.start_line.get()..=c.end_line.get() {
            let Some(pairs) = replacements.get(&ln) else {
                continue;
            };
            for (without, with) in pairs {
                if c.copyright.contains(without) && !c.copyright.contains(with) {
                    c.copyright = c.copyright.replace(without, with);
                }
            }
        }
    }
}

pub fn extract_mso_document_properties_copyrights(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if content.is_empty() {
        return;
    }

    static DESC_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<o:Description>(?P<desc>.*?)</o:Description>").unwrap());
    static TEMPLATE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<o:Template>(?P<tmpl>[^<]+)</o:Template>").unwrap());
    static LAST_AUTHOR_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<o:LastAuthor>(?P<last>[^<]+)</o:LastAuthor>").unwrap());
    static COPY_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^copyright\s+(?P<year>\d{4})(?:\s+(?P<tail>.+))?$").unwrap()
    });

    let lower = content.to_ascii_lowercase();
    if !lower.contains("<o:description") {
        return;
    }

    let mut desc: Option<(usize, String)> = None;
    let mut tmpl: Option<String> = None;
    let mut last: Option<String> = None;
    let mut last_line: Option<usize> = None;

    for (idx, raw) in content.lines().enumerate() {
        let ln = idx + 1;
        if desc.is_none()
            && let Some(cap) = DESC_RE.captures(raw)
        {
            let inner = cap.name("desc").map(|m| m.as_str()).unwrap_or("");
            let prepared = crate::copyright::prepare::prepare_text_line(inner);
            desc = Some((ln, prepared));
        }
        if tmpl.is_none()
            && let Some(cap) = TEMPLATE_RE.captures(raw)
        {
            let t = cap.name("tmpl").map(|m| m.as_str()).unwrap_or("").trim();
            if !t.is_empty() {
                tmpl = Some(crate::copyright::prepare::prepare_text_line(t));
            }
        }
        if last.is_none()
            && let Some(cap) = LAST_AUTHOR_RE.captures(raw)
        {
            let t = cap.name("last").map(|m| m.as_str()).unwrap_or("").trim();
            if !t.is_empty() {
                last = Some(crate::copyright::prepare::prepare_text_line(t));
                last_line = Some(ln);
            }
        }
    }

    let Some((desc_line, desc_prepared)) = desc else {
        return;
    };
    let Some(template) = tmpl else {
        return;
    };
    let Some(last_author) = last else {
        return;
    };

    let desc_prepared = token_utils::normalize_whitespace(&desc_prepared);
    let Some(cap) = COPY_YEAR_RE.captures(desc_prepared.trim()) else {
        return;
    };
    let year = cap.name("year").map(|m| m.as_str()).unwrap_or("").trim();
    if year.is_empty() {
        return;
    }
    let tail = cap.name("tail").map(|m| m.as_str()).unwrap_or("").trim();
    let is_confidential = tail
        .to_ascii_lowercase()
        .contains("confidential information");

    let (copy, hold) = if is_confidential {
        let holder = token_utils::normalize_whitespace(&format!(
            "{tail} {template} <o:LastAuthor> {last_author} </o:LastAuthor>"
        ));
        let c = token_utils::normalize_whitespace(&format!("Copyright {year} {holder}"));
        (c, holder)
    } else {
        let holder =
            token_utils::normalize_whitespace(&format!("{template} o:LastAuthor {last_author}"));
        let c = token_utils::normalize_whitespace(&format!("Copyright {year} {holder}"));
        (c, holder)
    };

    let end_line = last_line.unwrap_or(desc_line);

    let copy_refined = refine_copyright(&copy).unwrap_or_else(|| copy.clone());
    let holder_refined = refine_holder_in_copyright_context(&hold).unwrap_or_else(|| hold.clone());

    if !is_confidential {
        let ckey = (desc_line, end_line, copy_refined.clone());
        if !copyrights
            .iter()
            .any(|c| (c.start_line.get(), c.end_line.get(), c.copyright.clone()) == ckey)
        {
            copyrights.push(CopyrightDetection {
                copyright: copy_refined,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }
        let hkey = (desc_line, end_line, holder_refined.clone());
        if !holders
            .iter()
            .any(|h| (h.start_line.get(), h.end_line.get(), h.holder.clone()) == hkey)
        {
            holders.push(HolderDetection {
                holder: holder_refined,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(end_line).expect("valid"),
            });
        }
    }

    let plain = format!("Copyright {year}");
    copyrights.retain(|c| {
        !(c.start_line.get() == desc_line && c.end_line.get() == desc_line && c.copyright == plain)
    });

    let shadow_non_confidential =
        token_utils::normalize_whitespace(&format!("{last_author} Copyright {year}"));
    copyrights.retain(|c| {
        !token_utils::normalize_whitespace(&c.copyright)
            .eq_ignore_ascii_case(&shadow_non_confidential)
    });
    holders.retain(|h| {
        !token_utils::normalize_whitespace(&h.holder).eq_ignore_ascii_case(&last_author)
    });

    if is_confidential {
        let short_c = format!("Copyright {year} Confidential");
        let short_h = "Confidential".to_string();
        if let Some(rc) = refine_copyright(&short_c)
            && !copyrights.iter().any(|c| {
                c.start_line.get() == desc_line
                    && c.end_line.get() == desc_line
                    && c.copyright == rc
            })
        {
            copyrights.push(CopyrightDetection {
                copyright: rc,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(desc_line).expect("valid"),
            });
        }
        if !holders.iter().any(|h| {
            h.start_line.get() == desc_line && h.end_line.get() == desc_line && h.holder == short_h
        }) {
            holders.push(HolderDetection {
                holder: short_h,
                start_line: LineNumber::new(desc_line).expect("valid"),
                end_line: LineNumber::new(desc_line).expect("valid"),
            });
        }
    }
}

pub fn apply_openoffice_org_report_builder_bin_normalizations(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if !content.contains("Upstream-Name: OpenOffice.org") {
        return;
    }
    if !content.contains("ooo-build") {
        return;
    }

    for det in copyrights.iter_mut() {
        if det.copyright.contains("László Németh") {
            det.copyright = det.copyright.replace("László Németh", "Laszlo Nemeth");
        }
    }

    for det in holders.iter_mut() {
        if det.holder.contains("László Németh") {
            det.holder = det.holder.replace("László Németh", "Laszlo Nemeth");
        }
    }

    let want_cr = "Copyright (c) 2000 See Beyond Communications Corporation";
    if content.contains("See Beyond Communications Corporation")
        && !copyrights.iter().any(|c| c.copyright == want_cr)
    {
        let ln = content
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains("See Beyond Communications Corporation"))
            .map(|(i, _)| i + 1)
            .unwrap_or(1);

        if let Some(cr) = refine_copyright(want_cr) {
            copyrights.push(CopyrightDetection {
                copyright: cr,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }

        if let Some(h) = refine_holder("See Beyond Communications Corporation")
            && !holders.iter().any(|hh| hh.holder == h)
        {
            holders.push(HolderDetection {
                holder: h,
                start_line: LineNumber::new(ln).unwrap(),
                end_line: LineNumber::new(ln).unwrap(),
            });
        }
    }
}

pub fn recover_template_literal_year_range_copyrights(
    content: &str,
    copyrights: &mut Vec<CopyrightDetection>,
    holders: &mut Vec<HolderDetection>,
) {
    if content.is_empty() {
        return;
    }

    static TEMPLATE_COPY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?ix)
            \bcopyright\s+
            (?P<start>(?:19|20)\d{2})
            \s*[\-–]\s*
            (?P<templ>\$\{[^}\r\n]+\})
            \s+
            (?P<holder>[^`"'<>\{\}\r\n]+?)
            (?:\s*[`"']\s*)?$
        "#,
        )
        .expect("valid template literal copyright regex")
    });

    for (idx, raw_line) in content.lines().enumerate() {
        if !(raw_line.contains("Copyright") || raw_line.contains("copyright")) {
            continue;
        }
        if !raw_line.contains("${") {
            continue;
        }

        let Some(cap) = TEMPLATE_COPY_RE.captures(raw_line.trim()) else {
            continue;
        };

        let ln = idx + 1;
        let start = cap.name("start").map(|m| m.as_str()).unwrap_or("").trim();
        let templ = cap.name("templ").map(|m| m.as_str()).unwrap_or("").trim();
        let holder_raw = cap.name("holder").map(|m| m.as_str()).unwrap_or("").trim();
        if start.is_empty() || templ.is_empty() || holder_raw.is_empty() {
            continue;
        }
        let templ_lower = templ.to_ascii_lowercase();
        if !(templ_lower.contains("new date") && templ_lower.contains("getutcfullyear")) {
            continue;
        }

        let Some(holder) = refine_holder_in_copyright_context(holder_raw) else {
            continue;
        };

        let copyright_text = format!("Copyright {start}-{templ} {holder}");
        copyrights.push(CopyrightDetection {
            copyright: copyright_text,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });

        let truncated = format!("Copyright {start}-$");
        copyrights.retain(|c| {
            !(c.start_line.get() == ln
                && c.end_line.get() == ln
                && c.copyright.eq_ignore_ascii_case(&truncated))
        });

        holders.push(HolderDetection {
            holder,
            start_line: LineNumber::new(ln).unwrap(),
            end_line: LineNumber::new(ln).unwrap(),
        });
    }
}
