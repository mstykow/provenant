use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use clap::Parser;
use rayon::prelude::*;
use regex::Regex;
use url::Url;

#[derive(Parser, Debug)]
#[command(
    name = "validate-urls",
    about = "Validate URLs in production docs and Rust docstrings"
)]
struct Args {
    #[arg(long, default_value = ".", help = "Project root to scan")]
    root: PathBuf,

    #[arg(
        long,
        default_value_t = 10,
        help = "Validation timeout per URL in seconds"
    )]
    timeout_secs: u64,

    #[arg(long, default_value_t = 20, help = "Parallel workers for URL checks")]
    workers: usize,
}

#[derive(Debug, Clone)]
struct UrlOccurrence {
    file_path: String,
    line_no: usize,
    url: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum UrlStatus {
    Fail,
    Pass,
    Skip,
}

#[derive(Debug, Clone)]
struct UrlValidationResult {
    url: String,
    status: UrlStatus,
    message: String,
}

static BARE_URL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s\"'<>)\]]+"#).expect("valid URL regex"));
static DOCSTRING_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(///|//!)").expect("valid docstring regex"));

#[derive(Debug, Clone, Copy)]
enum MarkdownUrlStyle {
    Plain,
    Angle,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let project_root = args.root;

    println!("=== Extracting URLs from Documentation ===\n");

    let mut all_urls = Vec::new();

    println!("Scanning markdown files...");
    for file in find_markdown_files(&project_root) {
        all_urls.extend(extract_urls_from_markdown(&file));
    }

    println!("Scanning Rust docstrings...");
    for file in find_rust_files(&project_root) {
        all_urls.extend(extract_urls_from_rust(&file));
    }

    let total_occurrences = all_urls.len();
    println!("\nFound {total_occurrences} URL occurrences\n");

    let mut url_locations: HashMap<String, Vec<(String, usize)>> = HashMap::new();
    for occurrence in all_urls {
        url_locations
            .entry(occurrence.url)
            .or_default()
            .push((occurrence.file_path, occurrence.line_no));
    }

    let unique_urls: Vec<String> = url_locations.keys().cloned().collect();
    println!("Unique URLs to validate: {}\n", unique_urls.len());

    println!("=== Validating URLs ===\n");

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.workers)
        .build()?;

    let progress = AtomicUsize::new(0);
    let total = unique_urls.len();

    let mut results = pool.install(|| {
        unique_urls
            .par_iter()
            .map(|url| {
                let result = validate_url(url, args.timeout_secs);
                let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                if done.is_multiple_of(10) {
                    eprintln!("Validated {done}/{total}...");
                }
                result
            })
            .collect::<Vec<_>>()
    });

    results.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.url.cmp(&b.url)));

    let mut fail_count = 0usize;
    let mut pass_count = 0usize;
    let mut skip_count = 0usize;

    println!("\n=== Validation Results ===\n");

    for result in &results {
        match result.status {
            UrlStatus::Fail => {
                fail_count += 1;
                println!("❌ FAIL: {}", result.url);
                println!("   Reason: {}", result.message);
                println!("   Found in:");

                if let Some(locations) = url_locations.get(&result.url) {
                    for (file_path, line_no) in locations.iter().take(3) {
                        println!("     - {file_path}:{line_no}");
                    }
                    if locations.len() > 3 {
                        println!("     ... and {} more", locations.len() - 3);
                    }
                }
                println!();
            }
            UrlStatus::Pass => {
                pass_count += 1;
            }
            UrlStatus::Skip => {
                skip_count += 1;
            }
        }
    }

    if pass_count > 0 {
        println!("\n✅ {pass_count} URLs validated successfully\n");
    }

    if skip_count > 0 {
        println!("\n⏭️  {skip_count} URLs skipped (templates/placeholders)\n");
    }

    println!("\n=== Summary ===");
    println!("Total URLs found: {total_occurrences}");
    println!("Unique URLs: {}", unique_urls.len());
    println!("✅ Passed: {pass_count}");
    println!("❌ Failed: {fail_count}");
    println!("⏭️  Skipped: {skip_count}");

    if fail_count > 0 {
        println!("\n⚠️  {fail_count} URL(s) failed validation!");
        std::process::exit(1);
    }

    println!("\n✅ All URLs validated successfully!");
    Ok(())
}

fn find_markdown_files(root: &Path) -> Vec<PathBuf> {
    let excludes = excluded_directory_names();
    let mut out = Vec::new();
    collect_files_recursive(
        root,
        &mut |path| {
            !is_excluded(path, &excludes)
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        },
        &mut out,
    );
    out
}

fn find_rust_files(root: &Path) -> Vec<PathBuf> {
    let src_dir = root.join("src");
    if !src_dir.exists() {
        return Vec::new();
    }

    let mut out = Vec::new();
    collect_files_recursive(
        &src_dir,
        &mut |path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("_test.rs"))
                && !path
                    .components()
                    .any(|component| component.as_os_str().to_string_lossy() == "tests")
        },
        &mut out,
    );
    out
}

fn collect_files_recursive(
    root: &Path,
    predicate: &mut dyn FnMut(&Path) -> bool,
    out: &mut Vec<PathBuf>,
) {
    if root.is_file() {
        if predicate(root) {
            out.push(root.to_path_buf());
        }
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, predicate, out);
        } else if predicate(&path) {
            out.push(path);
        }
    }
}

fn excluded_directory_names() -> HashSet<&'static str> {
    HashSet::from([
        ".git",
        "node_modules",
        "target",
        "archived",
        "testdata",
        "tests",
        ".sisyphus",
        "reference",
        "resources",
    ])
}

fn is_excluded(path: &Path, excludes: &HashSet<&str>) -> bool {
    path.components().any(|component| {
        let part = component.as_os_str().to_string_lossy();
        excludes.contains(part.as_ref())
    })
}

fn extract_markdown_link_url(line: &str, start_pos: usize) -> String {
    let bytes = line.as_bytes();
    let mut paren_depth = 0usize;
    let mut i = start_pos;

    while i < bytes.len() {
        match bytes[i] as char {
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth == 0 {
                    return line[start_pos..i].to_string();
                }
                paren_depth -= 1;
            }
            ' ' | '\t' | '\n' => break,
            _ => {}
        }
        i += 1;
    }

    line[start_pos..i].to_string()
}

fn extract_urls_from_markdown(file_path: &Path) -> Vec<UrlOccurrence> {
    let mut urls = Vec::new();
    let Ok(content) = fs::read_to_string(file_path) else {
        eprintln!("Error reading {}", file_path.display());
        return urls;
    };

    let file = file_path.display().to_string();
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let mut line_urls = Vec::new();
        let mut markdown_url_ranges = Vec::new();

        let mut i = 0usize;
        while i < line.len() {
            if let Some((pattern_start, url_start, style)) = find_next_markdown_url_start(line, i) {
                let raw_url = extract_markdown_link_url(line, url_start);
                let url = normalize_extracted_url(&raw_url);

                if !url.is_empty() && !line_urls.iter().any(|existing: &String| existing == &url) {
                    line_urls.push(url.clone());
                }

                let raw_end = url_start.saturating_add(raw_url.len());
                let end = match style {
                    MarkdownUrlStyle::Plain => raw_end,
                    MarkdownUrlStyle::Angle => raw_end.saturating_add(1),
                };
                markdown_url_ranges.push((url_start, end));

                i = end.max(pattern_start.saturating_add(1));
            } else {
                break;
            }
        }

        for captures in BARE_URL_PATTERN.find_iter(line) {
            let start = captures.start();
            let end = captures.end();
            if markdown_url_ranges
                .iter()
                .any(|(md_start, md_end)| ranges_overlap(start, end, *md_start, *md_end))
            {
                continue;
            }

            let url = captures
                .as_str()
                .trim_end_matches(&['.', ',', ';', ':', '\'', '"', '`'][..])
                .trim_start_matches('<');
            if !line_urls.iter().any(|existing| existing == url) {
                line_urls.push(url.to_string());
            }
        }

        urls.extend(line_urls.into_iter().map(|url| UrlOccurrence {
            file_path: file.clone(),
            line_no,
            url,
        }));
    }

    urls
}

fn extract_urls_from_rust(file_path: &Path) -> Vec<UrlOccurrence> {
    let mut urls = Vec::new();
    let Ok(content) = fs::read_to_string(file_path) else {
        eprintln!("Error reading {}", file_path.display());
        return urls;
    };

    let file = file_path.display().to_string();
    for (idx, line) in content.lines().enumerate() {
        if !DOCSTRING_PATTERN.is_match(line) {
            continue;
        }

        let line_no = idx + 1;
        let mut line_urls = Vec::new();
        let mut markdown_url_ranges = Vec::new();

        let mut i = 0usize;
        while i < line.len() {
            if let Some((pattern_start, url_start, style)) = find_next_markdown_url_start(line, i) {
                let raw_url = extract_markdown_link_url(line, url_start);
                let url = normalize_extracted_url(&raw_url);

                if !url.is_empty() && !line_urls.iter().any(|existing: &String| existing == &url) {
                    line_urls.push(url.clone());
                }

                let raw_end = url_start.saturating_add(raw_url.len());
                let end = match style {
                    MarkdownUrlStyle::Plain => raw_end,
                    MarkdownUrlStyle::Angle => raw_end.saturating_add(1),
                };
                markdown_url_ranges.push((url_start, end));

                i = end.max(pattern_start.saturating_add(1));
            } else {
                break;
            }
        }

        for captures in BARE_URL_PATTERN.find_iter(line) {
            let start = captures.start();
            let end = captures.end();
            if markdown_url_ranges
                .iter()
                .any(|(md_start, md_end)| ranges_overlap(start, end, *md_start, *md_end))
            {
                continue;
            }

            let url = captures
                .as_str()
                .trim_end_matches(&['.', ',', ';', ':', '\'', '"', '`'][..])
                .trim_start_matches('<');
            if !line_urls.iter().any(|existing| existing == url) {
                line_urls.push(url.to_string());
            }
        }

        urls.extend(line_urls.into_iter().map(|url| UrlOccurrence {
            file_path: file.clone(),
            line_no,
            url,
        }));
    }

    urls
}

fn validate_url(url: &str, timeout_secs: u64) -> UrlValidationResult {
    let normalized = normalize_extracted_url(url);

    if ["{", "<", "...", "example.com"]
        .iter()
        .any(|placeholder| normalized.contains(placeholder))
        || normalized == "http://"
        || normalized == "https://"
    {
        return UrlValidationResult {
            url: normalized,
            status: UrlStatus::Skip,
            message: "Template/placeholder URL".to_string(),
        };
    }

    let parsed = match Url::parse(&normalized) {
        Ok(parsed) => parsed,
        Err(err) => {
            return UrlValidationResult {
                url: normalized,
                status: UrlStatus::Fail,
                message: format!("Invalid URL: {err}"),
            };
        }
    };

    if is_placeholder_github_url(&parsed) {
        return UrlValidationResult {
            url: normalized,
            status: UrlStatus::Skip,
            message: "Template/placeholder URL".to_string(),
        };
    }

    if parsed.host_str().is_none() {
        return UrlValidationResult {
            url: normalized,
            status: UrlStatus::Skip,
            message: "Relative or fragment URL".to_string(),
        };
    }

    let allowlist_patterns = ["crates.io"];
    if allowlist_patterns
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return UrlValidationResult {
            url: normalized,
            status: UrlStatus::Skip,
            message: "Allowlisted (blocks CI user agents)".to_string(),
        };
    }

    let output = Command::new("curl")
        .arg("-sS")
        .arg("--connect-timeout")
        .arg("5")
        .arg("--max-time")
        .arg(timeout_secs.to_string())
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}")
        .arg(&normalized)
        .output();

    let Ok(output) = output else {
        return UrlValidationResult {
            url: normalized,
            status: UrlStatus::Fail,
            message: "Failed to execute curl".to_string(),
        };
    };

    let status_code = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return UrlValidationResult {
            url: normalized,
            status: UrlStatus::Fail,
            message: format!(
                "Curl error: {}",
                stderr.chars().take(100).collect::<String>()
            ),
        };
    }

    let (status, message) = if status_code.starts_with('2') {
        (UrlStatus::Pass, format!("HTTP {status_code}"))
    } else if status_code.starts_with('3') {
        (UrlStatus::Pass, format!("HTTP {status_code} (redirect)"))
    } else if status_code == "000" {
        (UrlStatus::Fail, "Connection failed".to_string())
    } else {
        (UrlStatus::Fail, format!("HTTP {status_code}"))
    };

    UrlValidationResult {
        url: normalized,
        status,
        message,
    }
}

fn normalize_extracted_url(url: &str) -> String {
    url.trim()
        .trim_start_matches('<')
        .trim_end_matches(&['.', ',', ';', ':', '\'', '"', '`', '>'][..])
        .to_string()
}

fn find_next_markdown_url_start(
    line: &str,
    from: usize,
) -> Option<(usize, usize, MarkdownUrlStyle)> {
    let plain = line[from..]
        .find("](http")
        .map(|rel| (from + rel, from + rel + 2, MarkdownUrlStyle::Plain));
    let angle = line[from..]
        .find("](<http")
        .map(|rel| (from + rel, from + rel + 3, MarkdownUrlStyle::Angle));

    match (plain, angle) {
        (Some(p), Some(a)) => {
            if p.0 <= a.0 {
                Some(p)
            } else {
                Some(a)
            }
        }
        (Some(p), None) => Some(p),
        (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

fn ranges_overlap(start_a: usize, end_a: usize, start_b: usize, end_b: usize) -> bool {
    start_a < end_b && start_b < end_a
}

fn is_placeholder_github_url(parsed: &Url) -> bool {
    if parsed.host_str() != Some("github.com") {
        return false;
    }

    let path = parsed.path().trim_start_matches('/');
    path.starts_with("example/")
        || path == "org/repo"
        || path == "user/repo"
        || path == "user/repo.git"
}

#[cfg(test)]
mod tests {
    use super::{UrlStatus, extract_markdown_link_url, extract_urls_from_markdown, validate_url};
    use std::fs;

    #[test]
    fn extract_markdown_link_url_handles_nested_parentheses() {
        let line = "See [link](https://example.com/path(test)/end) please";
        let start = line.find("https://").expect("contains URL");
        let url = extract_markdown_link_url(line, start);
        assert_eq!(url, "https://example.com/path(test)/end");
    }

    #[test]
    fn markdown_extraction_deduplicates_exact_url_only() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file = temp_dir.path().join("doc.md");
        fs::write(
            &file,
            "[a](https://example.com/path) https://example.com/path https://example.com/pathology\n",
        )
        .expect("write temp file");

        let urls = extract_urls_from_markdown(&file)
            .into_iter()
            .map(|occ| occ.url)
            .collect::<Vec<_>>();

        assert!(urls.contains(&"https://example.com/path".to_string()));
        assert!(urls.contains(&"https://example.com/pathology".to_string()));
        assert_eq!(
            urls.iter()
                .filter(|url| *url == "https://example.com/path")
                .count(),
            1
        );
    }

    #[test]
    fn markdown_extraction_handles_angle_bracket_links_with_parentheses() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file = temp_dir.path().join("doc.md");
        fs::write(
            &file,
            "- [AR Archive Format](<https://en.wikipedia.org/wiki/Ar_(Unix)>)\n",
        )
        .expect("write temp file");

        let urls = extract_urls_from_markdown(&file)
            .into_iter()
            .map(|occ| occ.url)
            .collect::<Vec<_>>();

        assert_eq!(
            urls,
            vec!["https://en.wikipedia.org/wiki/Ar_(Unix)".to_string()]
        );
    }

    #[test]
    fn validate_url_skips_scheme_only_stub() {
        let result = validate_url("https://", 10);
        assert_eq!(result.status, UrlStatus::Skip);
    }

    #[test]
    fn validate_url_skips_github_placeholder_path() {
        let result = validate_url("https://github.com/user/repo.git", 10);
        assert_eq!(result.status, UrlStatus::Skip);
    }
}
