use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;
use regex::Regex;

const BENCHMARKS_PATH: &str = "docs/BENCHMARKS.md";
const SVG_PATH: &str = "docs/benchmarks/scan-duration-vs-files.svg";
const WIDTH: f64 = 1080.0;
const HEIGHT: f64 = 720.0;
const PLOT_LEFT: f64 = 96.0;
const PLOT_RIGHT: f64 = 32.0;
const PLOT_TOP: f64 = 88.0;
const PLOT_BOTTOM: f64 = 88.0;
const PROVENANT_COLOR: &str = "#2563eb";
const SCANCODE_COLOR: &str = "#d97706";

#[derive(Parser, Debug)]
struct Args {
    /// Check whether the generated SVG is up to date.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct BenchmarkPoint {
    label: String,
    files: u64,
    provenant_seconds: f64,
    scancode_seconds: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let benchmarks_path = Path::new(BENCHMARKS_PATH);
    let output_path = Path::new(SVG_PATH);
    let points = read_points(benchmarks_path)?;
    let svg = generate_svg(&points)?;

    if args.check {
        let existing = fs::read_to_string(output_path)
            .with_context(|| format!("failed to read {}", output_path.display()))?;
        if existing == svg {
            println!("✓ {} is up to date", output_path.display());
            return Ok(());
        }
        eprintln!("✗ {} is out of date", output_path.display());
        eprintln!("Run: cargo run --manifest-path xtask/Cargo.toml --bin generate-benchmark-chart");
        std::process::exit(1);
    }

    fs::write(output_path, svg)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    println!("✓ Generated {}", output_path.display());
    Ok(())
}

fn read_points(path: &Path) -> Result<Vec<BenchmarkPoint>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_points(&content)
        .with_context(|| format!("failed to parse benchmark rows from {}", path.display()))
}

fn parse_points(markdown: &str) -> Result<Vec<BenchmarkPoint>> {
    let row_pattern = Regex::new(
        r"^\|\s*\[(?P<label>[^\]]+)\]\([^)]*\)<br>(?P<files>[0-9][0-9,]*) files?\s*\|\s*[^|]+\|\s*Provenant:\s*(?P<prov>[0-9]+(?:\.[0-9]+)?)s<br>ScanCode:\s*(?P<scan>[0-9]+(?:\.[0-9]+)?)s<br>\*\*[^*]+\*\*\s*\|",
    )
    .expect("benchmark row regex should compile");

    let mut points = Vec::new();
    for line in markdown.lines() {
        let Some(captures) = row_pattern.captures(line) else {
            continue;
        };
        let label = captures
            .name("label")
            .expect("label capture should exist")
            .as_str()
            .to_string();
        let files = captures
            .name("files")
            .expect("files capture should exist")
            .as_str()
            .replace(',', "")
            .parse::<u64>()
            .with_context(|| format!("invalid file count in row: {line}"))?;
        let provenant_seconds = captures
            .name("prov")
            .expect("provenant capture should exist")
            .as_str()
            .parse::<f64>()
            .with_context(|| format!("invalid Provenant duration in row: {line}"))?;
        let scancode_seconds = captures
            .name("scan")
            .expect("ScanCode capture should exist")
            .as_str()
            .parse::<f64>()
            .with_context(|| format!("invalid ScanCode duration in row: {line}"))?;
        points.push(BenchmarkPoint {
            label,
            files,
            provenant_seconds,
            scancode_seconds,
        });
    }

    if points.is_empty() {
        bail!("no benchmark rows matched the expected markdown table format");
    }

    points.sort_by(|a, b| a.files.cmp(&b.files).then_with(|| a.label.cmp(&b.label)));
    for point in &points {
        if point.label.trim().is_empty() {
            bail!("benchmark plot data contains an empty label");
        }
        if point.files == 0 {
            bail!(
                "{} has zero files, which is invalid for a log-scale plot",
                point.label
            );
        }
        if point.provenant_seconds <= 0.0 || point.scancode_seconds <= 0.0 {
            bail!(
                "{} has a non-positive duration, which is invalid for a log-scale plot",
                point.label
            );
        }
    }

    Ok(points)
}

fn generate_svg(points: &[BenchmarkPoint]) -> Result<String> {
    let plot_width = WIDTH - PLOT_LEFT - PLOT_RIGHT;
    let plot_height = HEIGHT - PLOT_TOP - PLOT_BOTTOM;

    let min_files = points.iter().map(|point| point.files).min().unwrap() as f64;
    let max_files = points.iter().map(|point| point.files).max().unwrap() as f64;
    let min_seconds = points
        .iter()
        .flat_map(|point| [point.provenant_seconds, point.scancode_seconds])
        .fold(f64::INFINITY, f64::min);
    let max_seconds = points
        .iter()
        .flat_map(|point| [point.provenant_seconds, point.scancode_seconds])
        .fold(f64::NEG_INFINITY, f64::max);

    let x_domain = axis_domain(min_files, max_files)?;
    let y_domain = axis_domain(min_seconds, max_seconds)?;
    let x_ticks = log_ticks(x_domain.0, x_domain.1);
    let y_ticks = log_ticks(y_domain.0, y_domain.1);

    let mut svg = String::new();
    svg.push_str(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1080" height="720" viewBox="0 0 1080 720" role="img" aria-labelledby="title desc">
  <title id="title">Scan duration vs. file count for Provenant and ScanCode</title>
  <desc id="desc">Log-log scatter plot with file count on the x-axis and wall-clock duration in seconds on the y-axis. Provenant and ScanCode runs share the same axes.</desc>
  <style>
    text { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; fill: #111827; }
    .title { font-size: 26px; font-weight: 700; }
    .subtitle { font-size: 14px; fill: #4b5563; }
    .axis-label { font-size: 14px; font-weight: 600; }
    .tick { font-size: 12px; fill: #374151; }
    .grid { stroke: #e5e7eb; stroke-width: 1; }
    .axis { stroke: #6b7280; stroke-width: 1.25; }
    .legend-label { font-size: 13px; }
  </style>
"#,
    );
    svg.push_str(&format!(
        "  <rect x=\"0\" y=\"0\" width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"white\" />\n"
    ));
    svg.push_str(
        r#"  <text class="title" x="96" y="42">Scan duration vs. file count</text>
"#,
    );
    svg.push_str(r#"  <text class="subtitle" x="96" y="64">Recorded compare-outputs runs plotted on log-log axes to keep tiny artifacts and large repositories readable together.</text>
"#);

    for tick in &y_ticks {
        let y = plot_y(*tick, y_domain, plot_height);
        svg.push_str(&format!(
            "  <line class=\"grid\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />\n",
            PLOT_LEFT,
            y,
            PLOT_LEFT + plot_width,
            y
        ));
    }
    for tick in &x_ticks {
        let x = plot_x(*tick, x_domain, plot_width);
        svg.push_str(&format!(
            "  <line class=\"grid\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />\n",
            x,
            PLOT_TOP,
            x,
            PLOT_TOP + plot_height
        ));
    }

    svg.push_str(&format!(
        "  <line class=\"axis\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />\n",
        PLOT_LEFT,
        PLOT_TOP + plot_height,
        PLOT_LEFT + plot_width,
        PLOT_TOP + plot_height
    ));
    svg.push_str(&format!(
        "  <line class=\"axis\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />\n",
        PLOT_LEFT,
        PLOT_TOP,
        PLOT_LEFT,
        PLOT_TOP + plot_height
    ));

    for tick in &x_ticks {
        let x = plot_x(*tick, x_domain, plot_width);
        svg.push_str(&format!(
            "  <line class=\"axis\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />\n",
            x,
            PLOT_TOP + plot_height,
            x,
            PLOT_TOP + plot_height + 6.0
        ));
        svg.push_str(&format!(
            "  <text class=\"tick\" x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\">{}</text>\n",
            x,
            PLOT_TOP + plot_height + 24.0,
            format_tick(*tick)
        ));
    }
    for tick in &y_ticks {
        let y = plot_y(*tick, y_domain, plot_height);
        svg.push_str(&format!(
            "  <line class=\"axis\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />\n",
            PLOT_LEFT - 6.0,
            y,
            PLOT_LEFT,
            y
        ));
        svg.push_str(&format!(
            "  <text class=\"tick\" x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"end\" dominant-baseline=\"middle\">{}</text>\n",
            PLOT_LEFT - 10.0,
            y,
            format_tick(*tick)
        ));
    }

    svg.push_str(&format!(
        "  <text class=\"axis-label\" x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\">Files scanned (log scale)</text>\n",
        PLOT_LEFT + plot_width / 2.0,
        HEIGHT - 28.0
    ));
    svg.push_str(&format!(
        "  <text class=\"axis-label\" x=\"28\" y=\"{:.2}\" transform=\"rotate(-90 28 {:.2})\" text-anchor=\"middle\">Wall-clock duration (seconds, log scale)</text>\n",
        PLOT_TOP + plot_height / 2.0,
        PLOT_TOP + plot_height / 2.0
    ));

    svg.push_str(
        r##"  <g aria-label="Legend">
    <circle cx="794" cy="42" r="5" fill="#2563eb" />
    <text class="legend-label" x="808" y="46">Provenant</text>
    <rect x="900" y="37" width="10" height="10" fill="#d97706" rx="1.5" />
    <text class="legend-label" x="916" y="46">ScanCode</text>
  </g>
"##,
    );

    svg.push_str(
        r#"  <g aria-label="ScanCode points">
"#,
    );
    for point in points {
        let x = plot_x(point.files as f64, x_domain, plot_width);
        let y = plot_y(point.scancode_seconds, y_domain, plot_height);
        svg.push_str(&format!(
            "    <rect x=\"{:.2}\" y=\"{:.2}\" width=\"8\" height=\"8\" fill=\"{}\" rx=\"1.5\"><title>{}</title></rect>\n",
            x - 4.0,
            y - 4.0,
            SCANCODE_COLOR,
            xml_escape(&format!(
                "{}\nFiles: {}\nScanCode: {:.2}s",
                point.label,
                point.files,
                point.scancode_seconds
            ))
        ));
    }
    svg.push_str("  </g>\n");

    svg.push_str(
        r#"  <g aria-label="Provenant points">
"#,
    );
    for point in points {
        let x = plot_x(point.files as f64, x_domain, plot_width);
        let y = plot_y(point.provenant_seconds, y_domain, plot_height);
        svg.push_str(&format!(
            "    <circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"4.5\" fill=\"{}\"><title>{}</title></circle>\n",
            x,
            y,
            PROVENANT_COLOR,
            xml_escape(&format!(
                "{}\nFiles: {}\nProvenant: {:.2}s",
                point.label,
                point.files,
                point.provenant_seconds
            ))
        ));
    }
    svg.push_str("  </g>\n");
    svg.push_str("</svg>\n");
    Ok(svg)
}

fn axis_domain(min_value: f64, max_value: f64) -> Result<(f64, f64)> {
    if !min_value.is_finite() || !max_value.is_finite() {
        bail!("plot domain contains a non-finite value");
    }
    if min_value <= 0.0 || max_value <= 0.0 {
        bail!("plot domain contains a non-positive value");
    }
    let min_power = min_value.log10().floor();
    let max_power = max_value.log10().ceil();
    Ok((10f64.powf(min_power), 10f64.powf(max_power)))
}

fn log_ticks(min_value: f64, max_value: f64) -> Vec<f64> {
    let mut ticks = Vec::new();
    let mut value = min_value;
    while value <= max_value * 1.000_000_1 {
        ticks.push(value);
        value *= 10.0;
    }
    ticks
}

fn plot_x(value: f64, domain: (f64, f64), plot_width: f64) -> f64 {
    let min_log = domain.0.log10();
    let max_log = domain.1.log10();
    let value_log = value.log10();
    let fraction = (value_log - min_log) / (max_log - min_log);
    PLOT_LEFT + fraction * plot_width
}

fn plot_y(value: f64, domain: (f64, f64), plot_height: f64) -> f64 {
    let min_log = domain.0.log10();
    let max_log = domain.1.log10();
    let value_log = value.log10();
    let fraction = (value_log - min_log) / (max_log - min_log);
    PLOT_TOP + plot_height - fraction * plot_height
}

fn format_tick(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{}M", trim_float(value / 1_000_000.0))
    } else if value >= 1_000.0 {
        format!("{}k", trim_float(value / 1_000.0))
    } else {
        trim_float(value)
    }
}

fn trim_float(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < 0.000_001 {
        format!("{:.0}", rounded)
    } else {
        format!("{:.1}", rounded)
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_points_extracts_benchmark_rows() {
        let markdown = r#"
| Target snapshot | Run context | Timing snapshot | Advantages over ScanCode |
| --- | --- | --- | --- |
| [demo/repo @ abc1234](https://github.com/demo/repo/tree/abc1234)<br>1,234 files | 2026-04-19 · demo-123 · macOS | Provenant: 12.34s<br>ScanCode: 56.78s<br>**4.60× faster (-78.3%)** | Demo advantage |
| [demo/slow @ def5678](https://github.com/demo/slow/tree/def5678)<br>300 files | 2026-04-19 · slow-456 · macOS | Provenant: 18.81s<br>ScanCode: 17.13s<br>**1.10× slower (+9.8%)** | Slow example |
"#;

        let points = parse_points(markdown).expect("benchmark rows should parse");
        assert_eq!(
            points,
            vec![
                BenchmarkPoint {
                    label: "demo/slow @ def5678".to_string(),
                    files: 300,
                    provenant_seconds: 18.81,
                    scancode_seconds: 17.13,
                },
                BenchmarkPoint {
                    label: "demo/repo @ abc1234".to_string(),
                    files: 1234,
                    provenant_seconds: 12.34,
                    scancode_seconds: 56.78,
                },
            ]
        );
    }

    #[test]
    fn generated_svg_contains_both_series_and_labels() {
        let points = vec![
            BenchmarkPoint {
                label: "small fixture".to_string(),
                files: 10,
                provenant_seconds: 2.5,
                scancode_seconds: 12.0,
            },
            BenchmarkPoint {
                label: "large repo".to_string(),
                files: 10_000,
                provenant_seconds: 45.0,
                scancode_seconds: 320.0,
            },
        ];

        let svg = generate_svg(&points).expect("svg generation should succeed");
        assert!(svg.contains("Scan duration vs. file count"));
        assert!(svg.contains("Provenant"));
        assert!(svg.contains("ScanCode"));
        assert!(svg.contains("small fixture"));
        assert!(svg.contains("large repo"));
    }

    #[test]
    fn axis_domain_expands_to_powers_of_ten() {
        let domain = axis_domain(53.0, 12_345.0).expect("domain should be valid");
        assert_eq!(domain, (10.0, 100_000.0));
    }
}
