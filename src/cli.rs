use clap::{ArgGroup, Parser};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::fs;
use std::path::Path;
use yaml_serde::Value as YamlValue;

use crate::license_detection::DEFAULT_LICENSEDB_URL_TEMPLATE;
use crate::output::OutputFormat;

const PDF_OXIDE_LOG_HELP: &str = "Troubleshooting PDF parser logs:\n  Provenant suppresses noisy pdf_oxide logs by default.\n  To inspect raw pdf_oxide logs for debugging, rerun with RUST_LOG=pdf_oxide=warn (or =error).";

fn parse_license_policy_arg(value: &str) -> Result<String, String> {
    let policy_path = Path::new(value);
    let metadata = fs::metadata(policy_path).map_err(|err| {
        format!(
            "Failed to read license policy file {:?}: {err}",
            policy_path
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "License policy path {:?} is not a regular file",
            policy_path
        ));
    }

    let policy_text = fs::read_to_string(policy_path).map_err(|err| {
        format!(
            "Failed to read license policy file {:?}: {err}",
            policy_path
        )
    })?;
    if policy_text.trim().is_empty() {
        return Err(format!("License policy file {:?} is empty", policy_path));
    }

    let policy_value: YamlValue = yaml_serde::from_str(&policy_text).map_err(|err| {
        format!(
            "Failed to parse license policy file {:?}: {err}",
            policy_path
        )
    })?;
    let has_license_policies = policy_value
        .as_mapping()
        .and_then(|mapping| mapping.get(YamlValue::String("license_policies".to_string())))
        .is_some();
    if !has_license_policies {
        return Err(format!(
            "License policy file {:?} is missing a 'license_policies' attribute",
            policy_path
        ));
    }

    Ok(value.to_string())
}

#[derive(Parser, Debug)]
#[command(
    author = "The Provenant contributors",
    version = crate::version::BUILD_VERSION,
    long_version = crate::version::build_long_version(),
    after_help = PDF_OXIDE_LOG_HELP,
    about,
    long_about = None,
    group(
        ArgGroup::new("output")
            .required(true)
            .args([
                "output_json",
                "output_json_pp",
                "output_json_lines",
                "output_yaml",
                "output_debian",
                "output_html",
                "output_spdx_tv",
                "output_spdx_rdf",
                "output_cyclonedx",
                "output_cyclonedx_xml",
                "custom_output",
                "show_attribution"
            ])
    )
)]
pub struct Cli {
    /// File or directory paths to scan
    #[arg(required = false)]
    pub dir_path: Vec<String>,

    /// Write scan output as compact JSON to FILE
    #[arg(long = "json", value_name = "FILE", allow_hyphen_values = true)]
    pub output_json: Option<String>,

    /// Write scan output as pretty-printed JSON to FILE
    #[arg(long = "json-pp", value_name = "FILE", allow_hyphen_values = true)]
    pub output_json_pp: Option<String>,

    /// Write scan output as JSON Lines to FILE
    #[arg(long = "json-lines", value_name = "FILE", allow_hyphen_values = true)]
    pub output_json_lines: Option<String>,

    /// Write scan output as YAML to FILE
    #[arg(long = "yaml", value_name = "FILE", allow_hyphen_values = true)]
    pub output_yaml: Option<String>,

    /// Write scan output in machine-readable Debian copyright format to FILE (requires --license, --copyright, and --license-text)
    #[arg(
        long = "debian",
        value_name = "FILE",
        allow_hyphen_values = true,
        requires_all = ["copyright", "license", "license_text"]
    )]
    pub output_debian: Option<String>,

    /// Write scan output as HTML report to FILE
    #[arg(long = "html", value_name = "FILE", allow_hyphen_values = true)]
    pub output_html: Option<String>,

    /// Write scan output as SPDX tag/value to FILE
    #[arg(long = "spdx-tv", value_name = "FILE", allow_hyphen_values = true)]
    pub output_spdx_tv: Option<String>,

    /// Write scan output as SPDX RDF/XML to FILE
    #[arg(long = "spdx-rdf", value_name = "FILE", allow_hyphen_values = true)]
    pub output_spdx_rdf: Option<String>,

    /// Write scan output as CycloneDX JSON to FILE
    #[arg(long = "cyclonedx", value_name = "FILE", allow_hyphen_values = true)]
    pub output_cyclonedx: Option<String>,

    /// Write scan output as CycloneDX XML to FILE
    #[arg(
        long = "cyclonedx-xml",
        value_name = "FILE",
        allow_hyphen_values = true
    )]
    pub output_cyclonedx_xml: Option<String>,

    /// Write scan output to FILE formatted with the custom template
    #[arg(
        long = "custom-output",
        value_name = "FILE",
        requires = "custom_template",
        allow_hyphen_values = true
    )]
    pub custom_output: Option<String>,

    /// Use this template FILE with --custom-output
    #[arg(
        long = "custom-template",
        value_name = "FILE",
        requires = "custom_output"
    )]
    pub custom_template: Option<String>,

    /// Maximum recursion depth (0 means no depth limit)
    #[arg(short, long, default_value = "0")]
    pub max_depth: usize,

    #[arg(short = 'n', long, default_value_t = default_processes(), allow_hyphen_values = true)]
    pub processes: i32,

    #[arg(long, default_value_t = 120.0)]
    pub timeout: f64,

    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    #[arg(short, long, conflicts_with = "quiet")]
    pub verbose: bool,

    #[arg(long, conflicts_with = "full_root")]
    pub strip_root: bool,

    #[arg(long, conflicts_with = "strip_root")]
    pub full_root: bool,

    /// Exclude patterns (ScanCode-compatible alias: --ignore)
    #[arg(long = "exclude", visible_alias = "ignore", value_delimiter = ',')]
    pub exclude: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    pub include: Vec<String>,

    #[arg(long = "cache-dir", value_name = "PATH")]
    pub cache_dir: Option<String>,

    #[arg(long = "cache-clear")]
    pub cache_clear: bool,

    #[arg(long = "incremental")]
    pub incremental: bool,

    /// Maximum number of file and directory scan details kept in memory.
    /// Use 0 for unlimited memory or -1 for disk-only spill during the scan.
    #[arg(
        long = "max-in-memory",
        value_name = "INT",
        default_value_t = 10000,
        value_parser = parse_max_in_memory,
        allow_hyphen_values = true
    )]
    pub max_in_memory: i64,

    /// Collect file information such as checksums, type hints, and source/script flags.
    #[arg(short = 'i', long)]
    pub info: bool,

    /// Load one or more existing ScanCode-style JSON scans instead of rescanning inputs.
    #[arg(long)]
    pub from_json: bool,

    /// Scan input for application package and dependency manifests, lockfiles and related data
    #[arg(short = 'p', long)]
    pub package: bool,

    /// Scan input for installed system package databases (RPM, dpkg, apk, etc.)
    #[arg(long = "system-package")]
    pub system_package: bool,

    /// Scan supported compiled Go and Rust binaries for embedded package metadata.
    #[arg(long = "package-in-compiled")]
    pub package_in_compiled: bool,

    /// Scan for system and application package data and skip license/copyright detection and top-level package creation.
    #[arg(
        long = "package-only",
        conflicts_with_all = ["license", "summary", "package", "system_package"]
    )]
    pub package_only: bool,

    /// Disable package assembly (merging related manifest/lockfiles into packages)
    #[arg(long)]
    pub no_assemble: bool,

    /// Path to license rules directory containing .LICENSE and .RULE files.
    /// If not specified, uses the built-in embedded license index.
    #[arg(long, value_name = "PATH", requires = "license")]
    pub license_rules_path: Option<String>,

    /// Include matched text in license detection output
    #[arg(long = "license-text", requires = "license")]
    pub license_text: bool,

    #[arg(long = "license-text-diagnostics", requires = "license_text")]
    pub license_text_diagnostics: bool,

    #[arg(long = "license-diagnostics", requires = "license")]
    pub license_diagnostics: bool,

    #[arg(long = "unknown-licenses", requires = "license")]
    pub unknown_licenses: bool,

    #[arg(
        long = "license-score",
        default_value_t = 0,
        requires = "license",
        value_parser = clap::value_parser!(u8).range(0..=100)
    )]
    pub license_score: u8,

    #[arg(
        long = "license-url-template",
        default_value = DEFAULT_LICENSEDB_URL_TEMPLATE,
        requires = "license"
    )]
    pub license_url_template: String,

    #[arg(long)]
    pub filter_clues: bool,

    #[arg(
        long = "ignore-author",
        value_name = "PATTERN",
        help = "Ignore a file and all its findings if an author matches the regex PATTERN"
    )]
    pub ignore_author: Vec<String>,

    #[arg(
        long = "ignore-copyright-holder",
        value_name = "PATTERN",
        help = "Ignore a file and all its findings if a copyright holder matches the regex PATTERN"
    )]
    pub ignore_copyright_holder: Vec<String>,

    #[arg(long)]
    pub only_findings: bool,

    #[arg(long, requires = "info")]
    pub mark_source: bool,

    #[arg(long)]
    pub classify: bool,

    #[arg(long, requires = "classify")]
    pub summary: bool,

    #[arg(long = "license-clarity-score", requires = "classify")]
    pub license_clarity_score: bool,

    #[arg(long = "license-references", requires = "license")]
    pub license_references: bool,

    /// Evaluate file license detections against a YAML license policy file.
    #[arg(
        long = "license-policy",
        value_name = "FILE",
        value_parser = parse_license_policy_arg
    )]
    pub license_policy: Option<String>,

    #[arg(long)]
    pub tallies: bool,

    #[arg(long = "tallies-key-files", requires_all = ["tallies", "classify"])]
    pub tallies_key_files: bool,

    #[arg(long = "tallies-with-details")]
    pub tallies_with_details: bool,

    #[arg(long = "facet", value_name = "<facet>=<pattern>")]
    pub facet: Vec<String>,

    #[arg(long = "tallies-by-facet", requires_all = ["facet", "tallies"])]
    pub tallies_by_facet: bool,

    #[arg(long)]
    pub generated: bool,

    /// Scan input for licenses
    #[arg(short = 'l', long)]
    pub license: bool,

    #[arg(short = 'c', long)]
    pub copyright: bool,

    /// Scan input for email addresses
    #[arg(short = 'e', long)]
    pub email: bool,

    /// Report only up to INT emails found in a file. Use 0 for no limit.
    #[arg(long, default_value_t = 50, requires = "email")]
    pub max_email: usize,

    /// Scan input for URLs
    #[arg(short = 'u', long)]
    pub url: bool,

    /// Report only up to INT URLs found in a file. Use 0 for no limit.
    #[arg(long, default_value_t = 50, requires = "url")]
    pub max_url: usize,

    /// Show attribution notices for embedded license detection data
    #[arg(long)]
    pub show_attribution: bool,
}

fn default_processes() -> i32 {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    if cpus > 1 { (cpus - 1) as i32 } else { 1 }
}

fn parse_max_in_memory(value: &str) -> Result<i64, String> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| format!("invalid integer value: {value}"))?;
    if parsed < -1 {
        return Err("--max-in-memory must be -1, 0, or a positive integer".to_string());
    }
    Ok(parsed)
}

#[derive(Debug, Clone)]
pub struct OutputTarget {
    pub format: OutputFormat,
    pub file: String,
    pub custom_template: Option<String>,
}

impl Cli {
    pub fn output_targets(&self) -> Vec<OutputTarget> {
        let mut targets = Vec::new();

        if let Some(file) = &self.output_json {
            targets.push(OutputTarget {
                format: OutputFormat::Json,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_json_pp {
            targets.push(OutputTarget {
                format: OutputFormat::JsonPretty,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_json_lines {
            targets.push(OutputTarget {
                format: OutputFormat::JsonLines,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_yaml {
            targets.push(OutputTarget {
                format: OutputFormat::Yaml,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_debian {
            targets.push(OutputTarget {
                format: OutputFormat::Debian,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_html {
            targets.push(OutputTarget {
                format: OutputFormat::Html,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_spdx_tv {
            targets.push(OutputTarget {
                format: OutputFormat::SpdxTv,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_spdx_rdf {
            targets.push(OutputTarget {
                format: OutputFormat::SpdxRdf,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_cyclonedx {
            targets.push(OutputTarget {
                format: OutputFormat::CycloneDxJson,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.output_cyclonedx_xml {
            targets.push(OutputTarget {
                format: OutputFormat::CycloneDxXml,
                file: file.clone(),
                custom_template: None,
            });
        }

        if let Some(file) = &self.custom_output {
            targets.push(OutputTarget {
                format: OutputFormat::CustomTemplate,
                file: file.clone(),
                custom_template: self.custom_template.clone(),
            });
        }

        targets
    }

    pub fn output_header_options(&self) -> JsonMap<String, JsonValue> {
        let mut options = JsonMap::new();
        if !self.dir_path.is_empty() {
            options.insert(
                "input".to_string(),
                JsonValue::Array(
                    self.dir_path
                        .iter()
                        .cloned()
                        .map(JsonValue::String)
                        .collect(),
                ),
            );
        }

        let mut flags = Vec::new();

        push_string_option(&mut flags, "--cache-dir", self.cache_dir.as_ref());
        push_bool_option(&mut flags, "--cache-clear", self.cache_clear);
        push_bool_option(&mut flags, "--classify", self.classify);
        push_string_option(&mut flags, "--custom-output", self.custom_output.as_ref());
        push_string_option(
            &mut flags,
            "--custom-template",
            self.custom_template.as_ref(),
        );
        push_bool_option(&mut flags, "--copyright", self.copyright);
        push_string_option(&mut flags, "--cyclonedx", self.output_cyclonedx.as_ref());
        push_string_option(
            &mut flags,
            "--cyclonedx-xml",
            self.output_cyclonedx_xml.as_ref(),
        );
        push_string_option(&mut flags, "--debian", self.output_debian.as_ref());
        push_bool_option(&mut flags, "--email", self.email);
        push_array_option(&mut flags, "--facet", &self.facet);
        push_bool_option(&mut flags, "--filter-clues", self.filter_clues);
        push_bool_option(&mut flags, "--from-json", self.from_json);
        push_bool_option(&mut flags, "--full-root", self.full_root);
        push_bool_option(&mut flags, "--generated", self.generated);
        push_string_option(&mut flags, "--html", self.output_html.as_ref());
        push_array_option(&mut flags, "--ignore", &self.exclude);
        push_array_option(&mut flags, "--ignore-author", &self.ignore_author);
        push_array_option(
            &mut flags,
            "--ignore-copyright-holder",
            &self.ignore_copyright_holder,
        );
        push_bool_option(&mut flags, "--incremental", self.incremental);
        push_array_option(&mut flags, "--include", &self.include);
        push_bool_option(&mut flags, "--info", self.info);
        push_string_option(&mut flags, "--json", self.output_json.as_ref());
        push_string_option(&mut flags, "--json-lines", self.output_json_lines.as_ref());
        push_string_option(&mut flags, "--json-pp", self.output_json_pp.as_ref());
        push_bool_option(&mut flags, "--license", self.license);
        push_bool_option(
            &mut flags,
            "--license-clarity-score",
            self.license_clarity_score,
        );
        push_bool_option(
            &mut flags,
            "--license-diagnostics",
            self.license_diagnostics,
        );
        push_string_option(&mut flags, "--license-policy", self.license_policy.as_ref());
        push_bool_option(&mut flags, "--license-references", self.license_references);
        push_non_default_u8_option(&mut flags, "--license-score", self.license_score, 0);
        push_bool_option(&mut flags, "--license-text", self.license_text);
        push_bool_option(
            &mut flags,
            "--license-text-diagnostics",
            self.license_text_diagnostics,
        );
        push_non_default_string_option(
            &mut flags,
            "--license-url-template",
            &self.license_url_template,
            DEFAULT_LICENSEDB_URL_TEMPLATE,
        );
        push_non_default_usize_option(&mut flags, "--max-depth", self.max_depth, 0);
        push_non_default_i64_option(&mut flags, "--max-in-memory", self.max_in_memory, 10000);
        if self.email {
            push_non_default_usize_option(&mut flags, "--max-email", self.max_email, 50);
        }
        if self.url {
            push_non_default_usize_option(&mut flags, "--max-url", self.max_url, 50);
        }
        push_bool_option(&mut flags, "--mark-source", self.mark_source);
        push_bool_option(&mut flags, "--no-assemble", self.no_assemble);
        push_bool_option(&mut flags, "--only-findings", self.only_findings);
        push_bool_option(&mut flags, "--package", self.package);
        push_bool_option(
            &mut flags,
            "--package-in-compiled",
            self.package_in_compiled,
        );
        push_bool_option(&mut flags, "--package-only", self.package_only);
        push_non_default_i32_option(
            &mut flags,
            "--processes",
            self.processes,
            default_processes(),
        );
        push_bool_option(&mut flags, "--quiet", self.quiet);
        push_string_option(&mut flags, "--spdx-rdf", self.output_spdx_rdf.as_ref());
        push_string_option(&mut flags, "--spdx-tv", self.output_spdx_tv.as_ref());
        push_bool_option(&mut flags, "--strip-root", self.strip_root);
        push_bool_option(&mut flags, "--summary", self.summary);
        push_bool_option(&mut flags, "--system-package", self.system_package);
        push_bool_option(&mut flags, "--tallies", self.tallies);
        push_bool_option(&mut flags, "--tallies-by-facet", self.tallies_by_facet);
        push_bool_option(&mut flags, "--tallies-key-files", self.tallies_key_files);
        push_bool_option(
            &mut flags,
            "--tallies-with-details",
            self.tallies_with_details,
        );
        push_non_default_f64_option(&mut flags, "--timeout", self.timeout, 120.0);
        push_bool_option(&mut flags, "--unknown-licenses", self.unknown_licenses);
        push_bool_option(&mut flags, "--url", self.url);
        push_bool_option(&mut flags, "--verbose", self.verbose);
        push_string_option(&mut flags, "--yaml", self.output_yaml.as_ref());

        flags.sort_by(|left, right| left.0.cmp(&right.0));
        for (key, value) in flags {
            options.insert(key, value);
        }

        options
    }
}

fn push_bool_option(options: &mut Vec<(String, JsonValue)>, key: &str, enabled: bool) {
    if enabled {
        options.push((key.to_string(), JsonValue::Bool(true)));
    }
}

fn push_string_option(options: &mut Vec<(String, JsonValue)>, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        options.push((key.to_string(), JsonValue::String(value.clone())));
    }
}

fn push_non_default_string_option(
    options: &mut Vec<(String, JsonValue)>,
    key: &str,
    value: &str,
    default: &str,
) {
    if value != default {
        options.push((key.to_string(), JsonValue::String(value.to_string())));
    }
}

fn push_array_option(options: &mut Vec<(String, JsonValue)>, key: &str, values: &[String]) {
    if !values.is_empty() {
        options.push((
            key.to_string(),
            JsonValue::Array(values.iter().cloned().map(JsonValue::String).collect()),
        ));
    }
}

fn push_non_default_usize_option(
    options: &mut Vec<(String, JsonValue)>,
    key: &str,
    value: usize,
    default: usize,
) {
    if value != default {
        options.push((key.to_string(), JsonValue::Number(value.into())));
    }
}

fn push_non_default_u8_option(
    options: &mut Vec<(String, JsonValue)>,
    key: &str,
    value: u8,
    default: u8,
) {
    if value != default {
        options.push((key.to_string(), JsonValue::Number(value.into())));
    }
}

fn push_non_default_i32_option(
    options: &mut Vec<(String, JsonValue)>,
    key: &str,
    value: i32,
    default: i32,
) {
    if value != default {
        options.push((key.to_string(), JsonValue::Number(value.into())));
    }
}

fn push_non_default_i64_option(
    options: &mut Vec<(String, JsonValue)>,
    key: &str,
    value: i64,
    default: i64,
) {
    if value != default {
        options.push((key.to_string(), JsonValue::Number(value.into())));
    }
}

fn push_non_default_f64_option(
    options: &mut Vec<(String, JsonValue)>,
    key: &str,
    value: f64,
    default: f64,
) {
    if (value - default).abs() > f64::EPSILON
        && let Some(number) = JsonNumber::from_f64(value)
    {
        options.push((key.to_string(), JsonValue::Number(number)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_requires_at_least_one_output_option() {
        let parsed = Cli::try_parse_from(["provenant", "samples"]);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_parses_json_pretty_output_option() {
        let parsed = Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "samples"])
            .expect("cli parse should succeed");

        assert_eq!(parsed.output_json_pp.as_deref(), Some("scan.json"));
        assert_eq!(parsed.output_targets().len(), 1);
        assert_eq!(parsed.output_targets()[0].format, OutputFormat::JsonPretty);
    }

    #[test]
    fn test_output_header_options_use_scancode_style_keys() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--package",
            "--strip-root",
            "--ignore",
            "*.git*",
            "--ignore",
            "target/*",
            "samples",
        ])
        .expect("cli parse should succeed");

        let options = parsed.output_header_options();

        assert_eq!(
            options.get("input"),
            Some(&JsonValue::Array(vec![JsonValue::String(
                "samples".to_string()
            )]))
        );
        assert_eq!(
            options.get("--json-pp"),
            Some(&JsonValue::String("scan.json".to_string()))
        );
        assert_eq!(options.get("--license"), Some(&JsonValue::Bool(true)));
        assert_eq!(options.get("--package"), Some(&JsonValue::Bool(true)));
        assert_eq!(options.get("--strip-root"), Some(&JsonValue::Bool(true)));
        assert_eq!(
            options.get("--ignore"),
            Some(&JsonValue::Array(vec![
                JsonValue::String("*.git*".to_string()),
                JsonValue::String("target/*".to_string()),
            ]))
        );
    }

    #[test]
    fn test_output_header_options_skip_defaults_and_include_non_defaults() {
        let default_options =
            Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "samples"])
                .expect("default cli parse should succeed")
                .output_header_options();
        assert!(!default_options.contains_key("--timeout"));
        assert!(!default_options.contains_key("--processes"));

        let custom_options = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--timeout",
            "30",
            "--processes",
            "4",
            "samples",
        ])
        .expect("custom cli parse should succeed")
        .output_header_options();

        assert_eq!(
            custom_options.get("--timeout"),
            Some(&JsonValue::Number(
                JsonNumber::from_f64(30.0).expect("valid number")
            ))
        );
        assert_eq!(
            custom_options.get("--processes"),
            Some(&JsonValue::Number(4.into()))
        );
    }

    #[test]
    fn test_allows_stdout_dash_as_output_target() {
        let parsed = Cli::try_parse_from(["provenant", "--json-pp", "-", "samples"])
            .expect("cli parse should allow stdout dash output target");

        assert_eq!(parsed.output_json_pp.as_deref(), Some("-"));
    }

    #[test]
    fn test_debian_requires_license_copyright_and_license_text() {
        let missing_license_text = Cli::try_parse_from([
            "provenant",
            "--debian",
            "scan.copyright",
            "--license",
            "--copyright",
            "samples",
        ]);
        assert!(missing_license_text.is_err());

        let parsed = Cli::try_parse_from([
            "provenant",
            "--debian",
            "scan.copyright",
            "--license",
            "--copyright",
            "--license-text",
            "samples",
        ])
        .expect("cli parse should accept debian output");

        assert_eq!(parsed.output_targets().len(), 1);
        assert_eq!(parsed.output_targets()[0].format, OutputFormat::Debian);
        assert_eq!(parsed.output_debian.as_deref(), Some("scan.copyright"));
    }

    #[test]
    fn test_debian_help_mentions_required_companion_flags() {
        let command = Cli::command();
        let debian_arg = command
            .get_arguments()
            .find(|arg| arg.get_long() == Some("debian"))
            .expect("debian arg should exist");

        let help = debian_arg
            .get_help()
            .expect("debian arg should have help text")
            .to_string();

        assert!(help.contains("requires --license, --copyright, and --license-text"));
    }

    #[test]
    fn test_help_mentions_pdf_oxide_rust_log_escape_hatch() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("RUST_LOG=pdf_oxide=warn"));
        assert!(help.contains("suppresses noisy pdf_oxide logs by default"));
    }

    #[test]
    fn test_parses_license_policy_flag() {
        let temp = tempfile::tempdir().expect("temp dir");
        let policy_path = temp.path().join("policy.yml");
        std::fs::write(&policy_path, "license_policies: []\n").expect("policy written");

        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license-policy",
            policy_path.to_str().expect("utf8 path"),
            "samples",
        ])
        .expect("cli parse should accept license-policy");

        assert_eq!(
            parsed.license_policy.as_deref(),
            Some(policy_path.to_str().expect("utf8 path"))
        );
    }

    #[test]
    fn test_rejects_invalid_license_policy_flag_value() {
        let temp = tempfile::tempdir().expect("temp dir");
        let policy_path = temp.path().join("policy.yml");
        std::fs::write(&policy_path, "not_license_policies: []\n").expect("policy written");

        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license-policy",
            policy_path.to_str().expect("utf8 path"),
            "samples",
        ]);

        assert!(parsed.is_err());
    }

    #[test]
    fn test_custom_template_and_output_must_be_paired() {
        let missing_template =
            Cli::try_parse_from(["provenant", "--custom-output", "result.txt", "samples"]);
        assert!(missing_template.is_err());

        let missing_output =
            Cli::try_parse_from(["provenant", "--custom-template", "tpl.tera", "samples"]);
        assert!(missing_output.is_err());
    }

    #[test]
    fn test_parses_processes_and_timeout_options() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "-n",
            "4",
            "--timeout",
            "30",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert_eq!(parsed.processes, 4);
        assert_eq!(parsed.timeout, 30.0);
    }

    #[test]
    fn test_strip_root_conflicts_with_full_root() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--strip-root",
            "--full-root",
            "samples",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_parses_include_and_only_findings_and_filter_clues() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--include",
            "src/**,Cargo.toml",
            "--only-findings",
            "--filter-clues",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert_eq!(parsed.include, vec!["src/**", "Cargo.toml"]);
        assert!(parsed.only_findings);
        assert!(parsed.filter_clues);
    }

    #[test]
    fn test_parses_ignore_author_and_holder_filters() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--ignore-author",
            "Jane.*",
            "--ignore-author",
            ".*Bot$",
            "--ignore-copyright-holder",
            "Example Corp",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert_eq!(parsed.ignore_author, vec!["Jane.*", ".*Bot$"]);
        assert_eq!(parsed.ignore_copyright_holder, vec!["Example Corp"]);
    }

    #[test]
    fn test_parses_ignore_alias_for_exclude_patterns() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--ignore",
            "*.git*,target/*",
            "samples",
        ])
        .expect("cli parse should accept --ignore alias");

        assert_eq!(parsed.exclude, vec!["*.git*", "target/*"]);
    }

    #[test]
    fn test_quiet_conflicts_with_verbose() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--quiet",
            "--verbose",
            "samples",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_parses_from_json_and_mark_source() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--from-json",
            "--info",
            "--mark-source",
            "sample-scan.json",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.from_json);
        assert!(parsed.info);
        assert_eq!(parsed.dir_path, vec!["sample-scan.json"]);
        assert!(parsed.mark_source);
    }

    #[test]
    fn test_mark_source_requires_info() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--mark-source",
            "samples",
        ]);

        assert!(parsed.is_err());
    }

    #[test]
    fn test_parses_classify_facet_and_tallies_by_facet() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--classify",
            "--tallies",
            "--facet",
            "dev=*.c",
            "--facet",
            "tests=*/tests/*",
            "--tallies-by-facet",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.classify);
        assert!(parsed.tallies);
        assert_eq!(parsed.facet, vec!["dev=*.c", "tests=*/tests/*"]);
        assert!(parsed.tallies_by_facet);
    }

    #[test]
    fn test_tallies_by_facet_requires_facet_definitions() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--tallies-by-facet",
            "samples",
        ]);

        assert!(parsed.is_err());
    }

    #[test]
    fn test_summary_requires_classify() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--summary",
            "samples",
        ]);

        assert!(parsed.is_err());
    }

    #[test]
    fn test_tallies_key_files_requires_tallies_and_classify() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--tallies-key-files",
            "samples",
        ]);

        assert!(parsed.is_err());
    }

    #[test]
    fn test_parses_summary_tallies_and_generated_flags() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--classify",
            "--summary",
            "--license-clarity-score",
            "--tallies",
            "--tallies-key-files",
            "--tallies-with-details",
            "--generated",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.classify);
        assert!(parsed.summary);
        assert!(parsed.license_clarity_score);
        assert!(parsed.tallies);
        assert!(parsed.tallies_key_files);
        assert!(parsed.tallies_with_details);
        assert!(parsed.generated);
    }

    #[test]
    fn test_parses_copyright_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--copyright",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.copyright);
    }

    #[test]
    fn test_package_flag_defaults_to_disabled() {
        let parsed = Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "samples"])
            .expect("cli parse should succeed");

        assert!(!parsed.package);
    }

    #[test]
    fn test_parses_system_package_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--system-package",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.system_package);
    }

    #[test]
    fn test_parses_package_in_compiled_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--package-in-compiled",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.package_in_compiled);
    }

    #[test]
    fn test_parses_package_only_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--package-only",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.package_only);
    }

    #[test]
    fn test_package_only_conflicts_with_upstream_incompatible_flags() {
        let with_license = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--package-only",
            "--license",
            "samples",
        ]);
        assert!(with_license.is_err());

        let with_package = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--package-only",
            "--package",
            "samples",
        ]);
        assert!(with_package.is_err());
    }

    #[test]
    fn test_parses_package_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--package",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.package);
    }

    #[test]
    fn test_package_short_flag() {
        let parsed = Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "-p", "samples"])
            .expect("cli parse should succeed");

        assert!(parsed.package);
    }

    #[test]
    fn test_parses_license_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.license);
    }

    #[test]
    fn test_license_short_flag() {
        let parsed = Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "-l", "samples"])
            .expect("cli parse should succeed");

        assert!(parsed.license);
    }

    #[test]
    fn test_license_text_requires_license() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license-text",
            "samples",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_include_text_is_rejected() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--include-text",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_license_text_diagnostics_requires_license_text() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--license-text-diagnostics",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_parses_license_text_and_diagnostics_flags() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--license-text",
            "--license-text-diagnostics",
            "--license-diagnostics",
            "--unknown-licenses",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.license_text);
        assert!(parsed.license_text_diagnostics);
        assert!(parsed.license_diagnostics);
        assert!(parsed.unknown_licenses);
        assert_eq!(parsed.license_score, 0);
        assert_eq!(parsed.license_url_template, DEFAULT_LICENSEDB_URL_TEMPLATE);
    }

    #[test]
    fn test_license_score_requires_license() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license-score",
            "70",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_license_url_template_requires_license() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license-url-template",
            "https://example.com/licenses/{}/",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_parses_license_score_and_url_template_flags() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--license-score",
            "70",
            "--license-url-template",
            "https://example.com/licenses/{}/",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert_eq!(parsed.license_score, 70);
        assert_eq!(
            parsed.license_url_template,
            "https://example.com/licenses/{}/"
        );
    }

    #[test]
    fn test_rejects_license_score_above_range() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--license-score",
            "101",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_license_references_requires_license() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license-references",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_parses_license_references_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--license-references",
            "samples",
        ])
        .expect("cli parse should succeed");

        assert!(parsed.license_references);
    }

    #[test]
    fn test_include_text_alias_is_not_supported() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--license",
            "--include-text",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_parses_short_scan_flags() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "-c",
            "-e",
            "-u",
            "samples",
        ])
        .expect("cli parse should support short scan flags");

        assert!(parsed.copyright);
        assert!(parsed.email);
        assert!(parsed.url);
    }

    #[test]
    fn test_parses_processes_compat_values_zero_and_minus_one() {
        let zero =
            Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "-n", "0", "samples"])
                .expect("cli parse should accept processes=0");
        assert_eq!(zero.processes, 0);

        let parsed =
            Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "-n", "-1", "samples"])
                .expect("cli parse should accept processes=-1");
        assert_eq!(parsed.processes, -1);
    }

    #[test]
    fn test_parses_cache_flags() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--cache-dir",
            "/tmp/sc-cache",
            "--cache-clear",
            "--max-in-memory",
            "5000",
            "samples",
        ])
        .expect("cli parse should accept cache flags");

        assert_eq!(parsed.cache_dir.as_deref(), Some("/tmp/sc-cache"));
        assert!(parsed.cache_clear);
        assert!(!parsed.incremental);
        assert_eq!(parsed.max_in_memory, 5000);
    }

    #[test]
    fn test_parses_incremental_flag() {
        let parsed = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--incremental",
            "samples",
        ])
        .expect("cli parse should accept incremental flag");

        assert!(parsed.incremental);
    }

    #[test]
    fn test_max_in_memory_defaults_and_special_values() {
        let default_parsed =
            Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "samples"])
                .expect("default max-in-memory should parse");
        assert_eq!(default_parsed.max_in_memory, 10000);

        let disk_only = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--max-in-memory",
            "-1",
            "samples",
        ])
        .expect("-1 should parse");
        assert_eq!(disk_only.max_in_memory, -1);

        let unlimited = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--max-in-memory",
            "0",
            "samples",
        ])
        .expect("0 should parse");
        assert_eq!(unlimited.max_in_memory, 0);
    }

    #[test]
    fn test_max_in_memory_rejects_values_below_negative_one() {
        let result = Cli::try_parse_from([
            "provenant",
            "--json-pp",
            "scan.json",
            "--max-in-memory",
            "-2",
            "samples",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn test_max_depth_default_matches_reference_behavior() {
        let parsed = Cli::try_parse_from(["provenant", "--json-pp", "scan.json", "samples"])
            .expect("cli parse should succeed");

        assert_eq!(parsed.max_depth, 0);
    }
}
