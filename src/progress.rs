use std::collections::HashMap;
use std::env;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use env_logger::Env;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use log::LevelFilter;

use crate::models::{FileInfo, FileType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressMode {
    Quiet,
    Default,
    Verbose,
}

#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub processes: i32,
    pub scan_names: String,
    pub initial_files: usize,
    pub initial_dirs: usize,
    pub initial_size: u64,
    pub excluded_count: usize,
    pub final_files: usize,
    pub final_dirs: usize,
    pub final_size: u64,
    pub error_count: usize,
    pub warning_count: usize,
    pub total_bytes_scanned: u64,
    pub packages_assembled: usize,
    pub manifests_seen: usize,
    pub top_level_timings: Vec<(String, f64)>,
    pub detail_timings: Vec<(String, f64)>,
    pub incremental_reused: usize,
}

pub struct ScanProgress {
    mode: ProgressMode,
    multi: MultiProgress,
    scan_bar: ProgressBar,
    stats: Mutex<ScanStats>,
    phase_starts: Mutex<HashMap<&'static str, Instant>>,
    phase_spinner: Mutex<Option<ProgressBar>>,
    stderr_is_tty: bool,
}

impl ScanProgress {
    pub fn new(mode: ProgressMode) -> Self {
        let stderr_is_tty = std::io::stderr().is_terminal();
        let multi = match mode {
            ProgressMode::Quiet => MultiProgress::with_draw_target(ProgressDrawTarget::hidden()),
            ProgressMode::Default if stderr_is_tty => {
                MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(15))
            }
            ProgressMode::Default | ProgressMode::Verbose => {
                MultiProgress::with_draw_target(ProgressDrawTarget::hidden())
            }
        };

        let scan_bar = if mode == ProgressMode::Default && stderr_is_tty {
            multi.add(ProgressBar::new(0))
        } else {
            ProgressBar::hidden()
        };

        scan_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({per_sec}) ({eta})",
                )
                .expect("Failed to create progress bar style")
                .progress_chars("#>-"),
        );

        Self {
            mode,
            multi,
            scan_bar,
            stats: Mutex::new(ScanStats::default()),
            phase_starts: Mutex::new(HashMap::new()),
            phase_spinner: Mutex::new(None),
            stderr_is_tty,
        }
    }

    pub fn start_setup(&self) {
        self.start_phase("setup");
    }

    pub fn finish_setup(&self) {
        self.finish_top_level_phase("setup");
    }

    pub fn set_processes(&self, processes: i32) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.processes = processes;
    }

    pub fn set_scan_names(&self, scan_names: String) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.scan_names = scan_names;
    }

    pub fn init_logging_bridge(&self) {
        if self.mode == ProgressMode::Quiet {
            return;
        }

        let logger = build_env_logger();
        let level = logger.filter();
        if LogWrapper::new(self.multi.clone(), logger)
            .try_init()
            .is_ok()
        {
            log::set_max_level(level);
        }
    }

    pub fn start_discovery(&self) {
        self.start_phase("inventory");
        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Default => {
                self.start_spinner("Collecting files...");
            }
            ProgressMode::Verbose => {
                self.message("Collecting files...");
            }
        }
    }

    pub fn finish_discovery(&self, files: usize, dirs: usize, size: u64, excluded: usize) {
        self.finish_spinner();
        self.finish_top_level_phase("inventory");
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.initial_files = files;
        stats.initial_dirs = dirs;
        stats.initial_size = size;
        stats.excluded_count = excluded;
    }

    pub fn start_license_detection_engine_creation(&self) {
        self.start_phase("license_detection_engine_creation");
        self.message("Loading SPDX data, this may take a while...");
    }

    pub fn finish_license_detection_engine_creation(&self, detail_name: impl Into<String>) {
        self.finish_detail_phase(detail_name.into(), "license_detection_engine_creation");
    }

    pub fn start_scan(&self, total_files: usize) {
        self.start_phase("scan");
        self.scan_bar.set_length(total_files as u64);
        self.scan_bar.set_position(0);

        if self.mode == ProgressMode::Default && !self.stderr_is_tty {
            self.message(&format!(
                "Scanning {total_files} {}...",
                pluralize_files(total_files)
            ));
        }
    }

    pub fn file_completed(&self, path: &Path, bytes: u64, scan_errors: &[String]) {
        self.scan_bar.inc(1);
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.total_bytes_scanned += bytes;

        let errors = scan_errors
            .iter()
            .filter(|error| !is_warning_scan_error(error))
            .cloned()
            .collect::<Vec<_>>();
        let warnings = scan_errors
            .iter()
            .filter(|error| is_warning_scan_error(error))
            .cloned()
            .collect::<Vec<_>>();

        if !errors.is_empty() {
            stats.error_count += 1;
        } else if !warnings.is_empty() {
            stats.warning_count += 1;
        }
        drop(stats);

        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Default => {
                if let Some(formatted) = format_default_scan_error_from_list(path, &errors) {
                    self.error(&formatted);
                } else if let Some(formatted) =
                    format_default_scan_warning_from_list(path, &warnings)
                {
                    self.message(&format!("Warning: {formatted}"));
                }
            }
            ProgressMode::Verbose => {
                self.message(&path.to_string_lossy());
                for err in &errors {
                    for line in err.lines() {
                        self.error(&format!("  {line}"));
                    }
                }
                for warning in &warnings {
                    for line in warning.lines() {
                        self.message(&format!("  warning: {line}"));
                    }
                }
            }
        }
    }

    pub fn record_runtime_error(&self, path: &Path, err: &str) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.error_count += 1;
        drop(stats);

        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Default => self.error(&format_default_scan_error(path, err)),
            ProgressMode::Verbose => {
                self.error(&format!("Path: {}", path.to_string_lossy()));
                for line in err.lines() {
                    self.error(&format!("  {line}"));
                }
            }
        }
    }

    pub fn record_additional_error(&self, err: &str) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.error_count += 1;
        drop(stats);

        if self.mode != ProgressMode::Quiet {
            self.error(err);
        }
    }

    pub fn finish_scan(&self) {
        self.finish_top_level_phase("scan");
        if self.mode == ProgressMode::Default && self.stderr_is_tty {
            self.scan_bar.finish_with_message("Scan complete!");
        } else {
            self.scan_bar.finish_and_clear();
            if self.mode == ProgressMode::Default {
                self.message("Scan complete.");
            }
        }
    }

    pub fn record_incremental_reused(&self, count: usize) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.incremental_reused += count;
    }

    pub fn start_assembly(&self) {
        self.start_phase("assembly");
        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Default => self.start_spinner("Assembling packages..."),
            ProgressMode::Verbose => self.message("Assembling packages..."),
        }
    }

    pub fn finish_assembly(&self, packages_assembled: usize, manifests_seen: usize) {
        self.finish_spinner();
        self.finish_top_level_phase("assembly");
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.packages_assembled = packages_assembled;
        stats.manifests_seen = manifests_seen;
    }

    pub fn start_output(&self) {
        self.start_phase("output");
        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Default => self.start_spinner("Writing output..."),
            ProgressMode::Verbose => self.message("Writing output..."),
        }
    }

    pub fn output_written(&self, text: &str) {
        self.message(text);
    }

    pub fn finish_output(&self) {
        self.finish_spinner();
        self.finish_top_level_phase("output");
    }

    pub fn start_post_scan(&self) {
        self.start_phase("post-scan");
    }

    pub fn finish_post_scan(&self) {
        self.finish_top_level_phase("post-scan");
    }

    pub fn start_finalize(&self) {
        self.start_phase("finalize");
    }

    pub fn finish_finalize(&self) {
        self.finish_top_level_phase("finalize");
    }

    pub fn record_detail_timing(&self, name: impl Into<String>, duration: f64) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        accumulate_timing(&mut stats.detail_timings, name.into(), duration);
    }

    pub fn record_final_counts(&self, files: &[FileInfo]) {
        let mut stats = self.stats.lock().expect("stats lock poisoned");
        stats.final_files = files
            .iter()
            .filter(|f| f.file_type == FileType::File)
            .count();
        stats.final_dirs = files
            .iter()
            .filter(|f| f.file_type == FileType::Directory)
            .count();
        stats.final_size = files
            .iter()
            .filter(|f| f.file_type == FileType::File)
            .map(|f| f.size)
            .sum();
    }

    pub fn display_summary(&self, scan_start: &str, scan_end: &str) {
        if self.mode == ProgressMode::Quiet {
            return;
        }

        let stats = self.stats.lock().expect("stats lock poisoned");

        if stats.error_count > 0 {
            self.error("Some files failed to scan properly:");
        } else if stats.warning_count > 0 {
            self.message("Some files reported recoverable scan warnings:");
        }
        for line in build_summary_messages(&stats, scan_start, scan_end) {
            self.message(&line);
        }
        if stats.incremental_reused > 0 {
            self.message(&format!(
                "Incremental:    {} unchanged file(s) reused",
                stats.incremental_reused
            ));
        }
    }

    fn message(&self, msg: &str) {
        if self.mode == ProgressMode::Quiet {
            return;
        }

        if self.mode == ProgressMode::Default && self.stderr_is_tty {
            let _ = self.multi.println(msg);
        } else {
            eprintln!("{msg}");
        }
    }

    fn error(&self, msg: &str) {
        if self.mode == ProgressMode::Quiet {
            return;
        }

        if supports_color(self.stderr_is_tty) {
            self.message(&format!("\u{1b}[31m{msg}\u{1b}[0m"));
        } else {
            self.message(msg);
        }
    }

    fn start_phase(&self, phase: &'static str) {
        self.phase_starts
            .lock()
            .expect("phase lock poisoned")
            .insert(phase, Instant::now());
    }

    fn finish_top_level_phase(&self, phase: &'static str) {
        let start = self
            .phase_starts
            .lock()
            .expect("phase lock poisoned")
            .remove(phase);
        if let Some(start) = start {
            let mut stats = self.stats.lock().expect("stats lock poisoned");
            accumulate_timing(
                &mut stats.top_level_timings,
                phase.to_string(),
                start.elapsed().as_secs_f64(),
            );
        }
    }

    fn finish_detail_phase(&self, name: String, phase: &'static str) {
        let start = self
            .phase_starts
            .lock()
            .expect("phase lock poisoned")
            .remove(phase);
        if let Some(start) = start {
            let mut stats = self.stats.lock().expect("stats lock poisoned");
            accumulate_timing(
                &mut stats.detail_timings,
                name,
                start.elapsed().as_secs_f64(),
            );
        }
    }

    fn start_spinner(&self, message: &str) {
        if self.mode != ProgressMode::Default || !self.stderr_is_tty {
            self.message(message);
            return;
        }

        let spinner = self.multi.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("Failed to create spinner style"),
        );
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));
        spinner.set_message(message.to_string());
        *self
            .phase_spinner
            .lock()
            .expect("phase spinner lock poisoned") = Some(spinner);
    }

    fn finish_spinner(&self) {
        if let Some(spinner) = self
            .phase_spinner
            .lock()
            .expect("phase spinner lock poisoned")
            .take()
        {
            spinner.finish_and_clear();
        }
    }
}

fn build_env_logger() -> env_logger::Logger {
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("warn"));
    apply_default_log_filters(&mut builder);
    builder.build()
}

fn apply_default_log_filters(builder: &mut env_logger::Builder) {
    apply_default_log_filters_from(builder, env::var("RUST_LOG").ok().as_deref());
}

fn apply_default_log_filters_from(builder: &mut env_logger::Builder, rust_log: Option<&str>) {
    if let Some(level) = pdf_oxide_default_log_filter_from(rust_log) {
        builder.filter_module("pdf_oxide", level);
    }
}

pub(crate) fn format_default_scan_error(path: &Path, err: &str) -> String {
    let reason = concise_scan_error_reason(err);
    format!("{reason}: {}", path.to_string_lossy())
}

pub(crate) fn format_default_scan_error_from_list(
    path: &Path,
    scan_errors: &[String],
) -> Option<String> {
    scan_errors
        .iter()
        .find(|error| is_timeout_scan_error(error))
        .or_else(|| scan_errors.first())
        .map(|error| format_default_scan_error(path, error))
}

pub(crate) fn format_default_scan_warning_from_list(
    path: &Path,
    scan_warnings: &[String],
) -> Option<String> {
    scan_warnings
        .first()
        .map(|warning| format_default_scan_error(path, warning))
}

fn concise_scan_error_reason(err: &str) -> String {
    let first_line = err
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("Scan failed");

    if let Some((prefix, _)) = first_line.split_once(" at ")
        && is_structured_error_prefix(prefix)
    {
        return prefix.to_string();
    }

    if let Some((prefix, _)) = first_line.split_once(": ")
        && is_structured_error_prefix(prefix)
    {
        return prefix.to_string();
    }

    first_line.to_string()
}

fn is_timeout_scan_error(err: &str) -> bool {
    err.contains("Timeout while ")
        || err.contains("Timeout before ")
        || err.contains("Processing interrupted due to timeout")
}

pub(crate) fn is_warning_scan_error(err: &str) -> bool {
    let first_line = err.lines().next().unwrap_or(err).trim();
    first_line.starts_with("Maven property ")
        || first_line.starts_with("Skipping Maven template coordinates")
        || first_line.starts_with("Circular include detected")
}

fn is_structured_error_prefix(prefix: &str) -> bool {
    let lowercase = prefix.to_ascii_lowercase();
    lowercase.starts_with("failed to ")
        || lowercase.ends_with(" failed")
        || lowercase.starts_with("timeout ")
        || lowercase.starts_with("processing interrupted")
}

fn pluralize_files(count: usize) -> &'static str {
    if count == 1 { "file" } else { "files" }
}

fn pdf_oxide_default_log_filter_from(rust_log: Option<&str>) -> Option<LevelFilter> {
    should_filter_pdf_oxide_default_warnings_from(rust_log).then_some(LevelFilter::Off)
}

fn should_filter_pdf_oxide_default_warnings_from(rust_log: Option<&str>) -> bool {
    rust_log.is_none_or(|value| !value.contains("pdf_oxide"))
}

fn accumulate_timing(timings: &mut Vec<(String, f64)>, name: String, duration: f64) {
    if let Some((_, existing)) = timings
        .iter_mut()
        .find(|(existing_name, _)| *existing_name == name)
    {
        *existing += duration;
    } else {
        timings.push((name, duration));
    }
}

fn supports_color(stderr_is_tty: bool) -> bool {
    if !stderr_is_tty {
        return false;
    }
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    !matches!(env::var("TERM"), Ok(term) if term == "dumb")
}

fn build_summary_messages(stats: &ScanStats, scan_start: &str, scan_end: &str) -> Vec<String> {
    let total = stats
        .top_level_timings
        .iter()
        .map(|(_, value)| *value)
        .sum::<f64>()
        .max(0.0);
    let scan_time = stats
        .top_level_timings
        .iter()
        .find_map(|(name, value)| (name == "scan").then_some(*value))
        .unwrap_or(0.0);

    let speed_files = if scan_time > 0.0 {
        stats.final_files as f64 / scan_time
    } else {
        0.0
    };
    let speed_bytes = if scan_time > 0.0 {
        stats.total_bytes_scanned as f64 / scan_time
    } else {
        0.0
    };

    let scan_names = if stats.scan_names.is_empty() {
        "scan".to_string()
    } else {
        stats.scan_names.clone()
    };

    let mut lines = vec![
        format!(
            "Summary:        {scan_names} with {} process(es)",
            stats.processes
        ),
        format!("Errors count:   {}", stats.error_count),
        format!("Warnings count: {}", stats.warning_count),
        format!(
            "Scan Speed:     {speed_files:.2} files/sec. {}/sec.",
            format_size(speed_bytes as u64)
        ),
        format!(
            "Initial counts: {} resource(s): {} file(s) and {} directorie(s) for {}",
            stats.initial_files + stats.initial_dirs,
            stats.initial_files,
            stats.initial_dirs,
            format_size(stats.initial_size)
        ),
        format!(
            "Final counts:   {} resource(s): {} file(s) and {} directorie(s) for {}",
            stats.final_files + stats.final_dirs,
            stats.final_files,
            stats.final_dirs,
            format_size(stats.final_size)
        ),
        format!("Excluded count: {}", stats.excluded_count),
        format!(
            "Packages:       {} assembled from {} manifests",
            stats.packages_assembled, stats.manifests_seen
        ),
        "Timings:".to_string(),
        format!("  scan_start: {scan_start}"),
        format!("  scan_end:   {scan_end}"),
    ];

    for (name, value) in &stats.top_level_timings {
        lines.push(format!("  {name}: {value:.2}s"));

        let detail_timings = stats
            .detail_timings
            .iter()
            .filter(|(detail_name, _)| detail_parent_phase(detail_name) == Some(name.as_str()));

        if name == "scan" {
            let scan_breakdown: Vec<_> = detail_timings.collect();
            if !scan_breakdown.is_empty() {
                lines.push("  scan breakdown (cumulative worker time):".to_string());
                lines.extend(
                    scan_breakdown
                        .into_iter()
                        .map(|(detail_name, detail_value)| {
                            format!("    {detail_name}: {detail_value:.2}s")
                        }),
                );
            }
        } else {
            lines.extend(detail_timings.map(|(detail_name, detail_value)| {
                format!("    {detail_name}: {detail_value:.2}s")
            }));
        }
    }
    lines.push(format!("  total: {total:.2}s"));

    lines
}

fn detail_parent_phase(detail_name: &str) -> Option<&'static str> {
    if detail_name.starts_with("setup:") || detail_name.starts_with("setup_scan:") {
        Some("setup")
    } else if detail_name.starts_with("scan:") {
        Some("scan")
    } else if detail_name.starts_with("post-scan:") || detail_name.starts_with("output-filter:") {
        Some("post-scan")
    } else if detail_name.starts_with("assembly:") {
        Some("assembly")
    } else if detail_name.starts_with("finalize:") {
        Some("finalize")
    } else if detail_name.starts_with("output:") {
        Some("output")
    } else {
        None
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 Bytes".to_string();
    }
    if bytes == 1 {
        return "1 Byte".to_string();
    }

    let mut size = bytes as f64;
    let units = ["Bytes", "KB", "MB", "GB", "TB"];
    let mut idx = 0;
    while size >= 1024.0 && idx < units.len() - 1 {
        size /= 1024.0;
        idx += 1;
    }

    if idx == 0 {
        format!("{} {}", bytes, units[idx])
    } else {
        format!("{size:.2} {}", units[idx])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ScanStats, apply_default_log_filters_from, build_summary_messages,
        concise_scan_error_reason, format_default_scan_error, format_default_scan_error_from_list,
        format_size, pdf_oxide_default_log_filter_from, pluralize_files,
        should_filter_pdf_oxide_default_warnings_from,
    };

    use std::path::Path;

    use log::{Level, LevelFilter, Log, MetadataBuilder};

    #[test]
    fn format_size_matches_expected_shape() {
        assert_eq!(format_size(0), "0 Bytes");
        assert_eq!(format_size(1), "1 Byte");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(2_567_000), "2.45 MB");
    }

    #[test]
    fn summary_messages_render_detail_timings_hierarchically() {
        let stats = ScanStats {
            processes: 4,
            scan_names: "licenses, packages".to_string(),
            initial_files: 10,
            initial_dirs: 2,
            initial_size: 2_048,
            excluded_count: 1,
            final_files: 8,
            final_dirs: 1,
            final_size: 1_024,
            error_count: 0,
            warning_count: 0,
            total_bytes_scanned: 800,
            packages_assembled: 3,
            manifests_seen: 5,
            incremental_reused: 0,
            top_level_timings: vec![
                ("setup".to_string(), 1.0),
                ("inventory".to_string(), 2.0),
                ("scan".to_string(), 3.0),
                ("post-scan".to_string(), 4.0),
                ("assembly".to_string(), 5.0),
                ("finalize".to_string(), 6.0),
                ("output".to_string(), 7.0),
            ],
            detail_timings: vec![
                ("setup_scan:licenses".to_string(), 0.5),
                ("scan:packages".to_string(), 1.25),
                ("output-filter:only-findings".to_string(), 1.5),
                ("finalize:output-prepare".to_string(), 2.0),
            ],
        };

        let lines = build_summary_messages(&stats, "start", "end");
        let line_index = |needle: &str| {
            lines
                .iter()
                .position(|line| line == needle)
                .unwrap_or_else(|| panic!("missing line: {needle}"))
        };

        assert!(lines.contains(&"  total: 28.00s".to_string()));
        assert!(lines.contains(&"    setup_scan:licenses: 0.50s".to_string()));
        assert!(lines.contains(&"  scan breakdown (cumulative worker time):".to_string()));
        assert!(lines.contains(&"    scan:packages: 1.25s".to_string()));
        assert!(lines.contains(&"    output-filter:only-findings: 1.50s".to_string()));
        assert!(lines.contains(&"    finalize:output-prepare: 2.00s".to_string()));
        assert!(line_index("  setup: 1.00s") < line_index("    setup_scan:licenses: 0.50s"));
        assert!(
            line_index("  scan: 3.00s") < line_index("  scan breakdown (cumulative worker time):")
        );
        assert!(
            line_index("  scan breakdown (cumulative worker time):")
                < line_index("    scan:packages: 1.25s")
        );
        assert!(
            line_index("  post-scan: 4.00s") < line_index("    output-filter:only-findings: 1.50s")
        );
        assert!(line_index("  finalize: 6.00s") < line_index("    finalize:output-prepare: 2.00s"));
    }

    #[test]
    fn summary_messages_use_scan_time_for_scan_speed() {
        let stats = ScanStats {
            final_files: 20,
            total_bytes_scanned: 2_048,
            top_level_timings: vec![("scan".to_string(), 4.0)],
            ..ScanStats::default()
        };

        let lines = build_summary_messages(&stats, "start", "end");

        assert!(lines.contains(&"Scan Speed:     5.00 files/sec. 512 Bytes/sec.".to_string()));
    }

    #[test]
    fn default_pdf_oxide_warnings_are_suppressed() {
        assert_eq!(
            pdf_oxide_default_log_filter_from(None),
            Some(LevelFilter::Off)
        );
        assert!(should_filter_pdf_oxide_default_warnings_from(None));
    }

    #[test]
    fn explicit_pdf_oxide_rust_log_override_disables_default_filter() {
        assert!(!should_filter_pdf_oxide_default_warnings_from(Some(
            "pdf_oxide::fonts::font_dict=warn"
        )));
    }

    #[test]
    fn default_pdf_oxide_filter_covers_unlisted_submodules() {
        let mut builder = env_logger::Builder::new();
        builder.filter_level(LevelFilter::Warn);
        apply_default_log_filters_from(&mut builder, None);
        let logger = builder.build();
        let warn_metadata = MetadataBuilder::new()
            .target("pdf_oxide::content::parser")
            .level(Level::Warn)
            .build();
        let error_metadata = MetadataBuilder::new()
            .target("pdf_oxide::content::parser")
            .level(Level::Error)
            .build();

        assert!(!logger.enabled(&warn_metadata));
        assert!(!logger.enabled(&error_metadata));
    }

    #[test]
    fn concise_scan_error_reason_keeps_high_level_failure_context() {
        assert_eq!(
            concise_scan_error_reason(
                "Failed to read or parse package.json at \"fixtures/package.json\": key must be a string at line 1 column 3"
            ),
            "Failed to read or parse package.json"
        );
        assert_eq!(
            concise_scan_error_reason("License detection failed: missing query token"),
            "License detection failed"
        );
        assert_eq!(
            concise_scan_error_reason("Processing interrupted due to timeout after 2.00 seconds"),
            "Processing interrupted due to timeout after 2.00 seconds"
        );
    }

    #[test]
    fn default_scan_error_format_includes_reason_and_path() {
        let formatted = format_default_scan_error(
            Path::new("fixtures/package.json"),
            "Failed to read or parse package.json at \"fixtures/package.json\": key must be a string at line 1 column 3",
        );

        assert_eq!(
            formatted,
            "Failed to read or parse package.json: fixtures/package.json"
        );
    }

    #[test]
    fn default_scan_error_format_prefers_timeout_from_error_list() {
        let formatted = format_default_scan_error_from_list(
            Path::new("fixtures/package.json"),
            &[
                "Failed to read or parse package.json at \"fixtures/package.json\": expected value"
                    .to_string(),
                "Timeout before license scan (> 120.00s)".to_string(),
            ],
        );

        assert_eq!(
            formatted.as_deref(),
            Some("Timeout before license scan (> 120.00s): fixtures/package.json")
        );
    }

    #[test]
    fn pluralize_files_uses_expected_labels() {
        assert_eq!(pluralize_files(1), "file");
        assert_eq!(pluralize_files(2), "files");
    }
}
