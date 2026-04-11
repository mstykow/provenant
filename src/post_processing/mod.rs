use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use chrono::Utc;
use glob::Pattern;
use serde_json::{Map, Value};

use self::classification::apply_file_classification;
pub(crate) use self::license_policy::apply_license_policy_from_file;
pub(crate) use self::license_references::collect_top_level_license_references;
use self::output_indexes::{OutputIndexMode, OutputIndexes};
use self::package_file_index::PackageFileIndex;
use self::package_metadata_promotion::promote_package_metadata_from_key_files;
pub(crate) use self::reference_following::apply_package_reference_following;
pub(crate) use self::reference_following::collect_top_level_license_detections;
#[cfg(test)]
use self::reference_following::{
    build_reference_follow_snapshot, resolve_referenced_resource, use_referenced_license_expression,
};
use self::summary::compute_summary_with_options;
use self::tallies::{
    compute_detailed_tallies, compute_file_tallies, compute_key_file_tallies, compute_tallies,
    compute_tallies_by_facet,
};
use crate::assembly;
pub(crate) use crate::license_detection::DEFAULT_LICENSEDB_URL_TEMPLATE;
#[cfg(test)]
use crate::models::DatasourceId;
#[cfg(test)]
use crate::models::Match;
use crate::models::{
    ExtraData, FileInfo, FileType, HEADER_NOTICE, Header, OUTPUT_FORMAT_VERSION, Output, Package,
    SystemEnvironment, TOOL_NAME, TopLevelLicenseDetection,
};
use crate::progress::{
    format_default_scan_error_from_list, format_default_scan_warning_from_list,
    is_warning_scan_error,
};
use crate::scanner;
#[cfg(test)]
use crate::utils::generated::generated_code_hints;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FileIx(pub(super) usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PackageIx(pub(super) usize);

mod classification;
#[cfg(test)]
mod classify_test;
#[cfg(test)]
mod facet_test;
mod font_policy;
#[cfg(test)]
mod generated_test;
#[cfg(all(test, feature = "golden-tests"))]
mod golden_test;
mod license_policy;
mod license_references;
mod output_indexes;
#[cfg(test)]
mod output_test;
mod package_file_index;
mod package_metadata_promotion;
mod reference_following;
mod summary;
mod summary_helpers;
mod tallies;
#[cfg(test)]
mod tallies_test;
#[cfg(test)]
mod test_utils;

pub(crate) struct CreateOutputOptions<'a> {
    pub(crate) facet_rules: &'a [FacetRule],
    pub(crate) include_classify: bool,
    pub(crate) include_summary: bool,
    pub(crate) include_license_clarity_score: bool,
    pub(crate) include_tallies: bool,
    pub(crate) include_tallies_of_key_files: bool,
    pub(crate) include_tallies_with_details: bool,
    pub(crate) include_tallies_by_facet: bool,
    pub(crate) include_generated: bool,
    pub(crate) verbose: bool,
}

pub(crate) struct CreateOutputContext<'a> {
    pub(crate) total_dirs: usize,
    pub(crate) assembly_result: assembly::AssemblyResult,
    pub(crate) license_detections: Vec<TopLevelLicenseDetection>,
    pub(crate) license_references: Vec<crate::models::LicenseReference>,
    pub(crate) license_rule_references: Vec<crate::models::LicenseRuleReference>,
    pub(crate) spdx_license_list_version: String,
    pub(crate) extra_errors: Vec<String>,
    pub(crate) extra_warnings: Vec<String>,
    pub(crate) header_options: Map<String, Value>,
    pub(crate) options: CreateOutputOptions<'a>,
}

pub(crate) fn create_output(
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
    scan_result: scanner::ProcessResult,
    context: CreateOutputContext<'_>,
) -> Output {
    let duration = (end_time - start_time).num_nanoseconds().unwrap_or(0) as f64 / 1_000_000_000.0;

    let extra_data = ExtraData {
        system_environment: current_system_environment(),
        spdx_license_list_version: context.spdx_license_list_version,
        files_count: scan_result
            .files
            .iter()
            .filter(|file| file.file_type == FileType::File)
            .count(),
        directories_count: context.total_dirs,
        excluded_count: scan_result.excluded_count,
    };

    let (mut errors, file_warnings) = summarize_header_messages(
        &scan_result.files,
        context.extra_errors,
        context.options.verbose,
    );
    let mut seen_errors = HashSet::new();
    errors.retain(|error| seen_errors.insert(error.clone()));
    let mut warnings = context.extra_warnings;
    warnings.extend(file_warnings);
    let mut seen_warnings = HashSet::new();
    warnings.retain(|warning| seen_warnings.insert(warning.clone()));

    let mut files = scan_result.files;
    let assembly::AssemblyResult {
        mut packages,
        dependencies,
    } = context.assembly_result;
    let needs_classification = context.options.include_classify
        || context.options.include_summary
        || context.options.include_license_clarity_score
        || context.options.include_tallies_of_key_files;
    let package_file_index = (needs_classification || !packages.is_empty())
        .then(|| PackageFileIndex::build(&files, &packages));

    if context.options.include_generated {
        materialize_generated_flags(&mut files);
    } else {
        clear_generated_flags(&mut files);
    }
    if needs_classification && let Some(package_file_index) = package_file_index.as_ref() {
        apply_file_classification(&mut files, package_file_index);
    }
    let output_index_mode =
        if context.options.include_summary || context.options.include_license_clarity_score {
            OutputIndexMode::Full
        } else {
            OutputIndexMode::KeyFilesOnly
        };
    let output_indexes = OutputIndexes::build(
        &files,
        package_file_index.as_ref(),
        !needs_classification,
        output_index_mode,
    );

    promote_package_metadata_from_key_files(&files, &mut packages, &output_indexes);
    assign_facets(&mut files, context.options.facet_rules);
    if context.options.include_tallies_with_details {
        compute_detailed_tallies(&mut files);
    } else if context.options.include_tallies_by_facet {
        compute_file_tallies(&mut files);
    } else {
        clear_resource_tallies(&mut files);
    }
    let summary =
        if context.options.include_summary || context.options.include_license_clarity_score {
            compute_summary_with_options(
                &files,
                &packages,
                &output_indexes,
                context.options.include_summary,
                context.options.include_license_clarity_score || context.options.include_summary,
            )
        } else {
            None
        };
    let tallies = if context.options.include_tallies || context.options.include_tallies_with_details
    {
        compute_tallies(&files)
    } else {
        None
    };
    let tallies_of_key_files = if context.options.include_tallies_of_key_files {
        compute_key_file_tallies(&files)
    } else {
        None
    };
    let tallies_by_facet = if context.options.include_tallies_by_facet {
        compute_tallies_by_facet(&files)
    } else {
        None
    };
    if !context.options.include_tallies_with_details {
        clear_resource_tallies(&mut files);
    }

    Output {
        summary,
        tallies,
        tallies_of_key_files,
        tallies_by_facet,
        headers: vec![Header {
            tool_name: TOOL_NAME.to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            options: context.header_options,
            notice: HEADER_NOTICE.to_string(),
            start_timestamp: start_time.to_rfc3339(),
            end_timestamp: end_time.to_rfc3339(),
            output_format_version: OUTPUT_FORMAT_VERSION.to_string(),
            duration,
            errors,
            warnings,
            extra_data,
        }],
        packages,
        dependencies,
        license_detections: context.license_detections,
        files,
        license_references: context.license_references,
        license_rule_references: context.license_rule_references,
    }
}

fn summarize_header_messages(
    files: &[FileInfo],
    extra_errors: Vec<String>,
    verbose: bool,
) -> (Vec<String>, Vec<String>) {
    let mut errors = extra_errors;
    let mut warnings = Vec::new();

    for file in files {
        let (file_errors, file_warnings) = partition_scan_messages(&file.scan_errors);

        if let Some(summary) = summarize_file_header_message(file, &file_errors, verbose, false) {
            errors.push(summary);
        }
        if let Some(summary) = summarize_file_header_message(file, &file_warnings, verbose, true) {
            warnings.push(summary);
        }
    }

    (errors, warnings)
}

fn partition_scan_messages(scan_messages: &[String]) -> (Vec<String>, Vec<String>) {
    scan_messages
        .iter()
        .cloned()
        .partition(|message| !is_warning_scan_error(message))
}

fn summarize_file_header_message(
    file: &FileInfo,
    messages: &[String],
    verbose: bool,
    warning: bool,
) -> Option<String> {
    let summary = if warning {
        format_default_scan_warning_from_list(Path::new(&file.path), messages)?
    } else {
        format_default_scan_error_from_list(Path::new(&file.path), messages)?
    };

    if !verbose {
        return Some(summary);
    }

    let details = messages
        .iter()
        .flat_map(|error| error.lines().map(|line| format!("  {line}")))
        .collect::<Vec<_>>();

    if details.is_empty() {
        Some(summary)
    } else {
        Some(
            std::iter::once(summary)
                .chain(details)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

fn current_system_environment() -> SystemEnvironment {
    let info = os_info::get();
    let operating_system = operating_system_name(&info);
    let platform_version = operating_system_version(&info);

    SystemEnvironment {
        operating_system: operating_system
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        cpu_architecture: env::consts::ARCH.to_string(),
        platform: build_platform_label(
            operating_system.as_deref(),
            platform_version.as_deref(),
            env::consts::ARCH,
        ),
        platform_version: platform_version.unwrap_or_else(|| "unknown".to_string()),
        rust_version: rustc_version_runtime::version().to_string(),
    }
}

fn operating_system_name(info: &os_info::Info) -> Option<String> {
    match info.os_type() {
        os_info::Type::Unknown => None,
        os_type => Some(os_type.to_string()),
    }
}

fn operating_system_version(info: &os_info::Info) -> Option<String> {
    match info.version() {
        os_info::Version::Unknown => None,
        version => Some(version.to_string()),
    }
}

fn build_platform_label(
    operating_system: Option<&str>,
    operating_system_version: Option<&str>,
    architecture: &str,
) -> String {
    format!(
        "{}-{}-{architecture}",
        operating_system.unwrap_or("unknown"),
        operating_system_version.unwrap_or("unknown")
    )
}

#[cfg(test)]
mod system_environment_tests {
    use super::{build_platform_label, operating_system_name, operating_system_version};

    #[test]
    fn build_platform_label_uses_unknown_fallbacks() {
        assert_eq!(
            build_platform_label(None, None, "x86_64"),
            "unknown-unknown-x86_64"
        );
        assert_eq!(
            build_platform_label(Some("Linux"), Some("6.8"), "x86_64"),
            "Linux-6.8-x86_64"
        );
    }

    #[test]
    fn os_info_unknown_values_map_to_none() {
        let info = os_info::Info::unknown();

        assert_eq!(operating_system_name(&info), None);
        assert_eq!(operating_system_version(&info), None);
    }
}

#[cfg(test)]
fn classify_key_files(files: &mut [FileInfo], packages: &[Package]) {
    classification::classify_key_files(files, packages);
}

fn package_root(package: &Package) -> Option<PathBuf> {
    for datafile_path in &package.datafile_paths {
        let path = Path::new(datafile_path);

        if path.file_name().and_then(|n| n.to_str()) == Some("metadata.gz-extract") {
            return path.parent().map(|p| p.to_path_buf());
        }

        if path
            .components()
            .any(|c| c.as_os_str() == "data.gz-extract")
        {
            let mut current = path;
            while let Some(parent) = current.parent() {
                if parent.file_name().and_then(|n| n.to_str()) == Some("data.gz-extract") {
                    return parent.parent().map(|p| p.to_path_buf());
                }
                current = parent;
            }
        }

        if let Some(parent) = path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    None
}

fn lowest_common_parent_path(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut paths_iter = paths.iter();
    let first = paths_iter.next()?;
    let mut common_components: Vec<_> = first.components().collect();

    for path in paths_iter {
        let current_components: Vec<_> = path.components().collect();
        let shared_len = common_components
            .iter()
            .zip(current_components.iter())
            .take_while(|(left, right)| left == right)
            .count();
        common_components.truncate(shared_len);
        if common_components.is_empty() {
            break;
        }
    }

    (!common_components.is_empty()).then(|| {
        let mut common_path = PathBuf::new();
        for component in common_components {
            common_path.push(component.as_os_str());
        }
        common_path
    })
}

const FACETS: [&str; 6] = ["core", "dev", "tests", "docs", "data", "examples"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum FacetMatchTarget {
    Path,
    NameOrPath,
}

#[derive(Clone)]
pub(crate) struct FacetRule {
    facet_index: usize,
    target: FacetMatchTarget,
    pattern: Pattern,
}

pub(crate) fn build_facet_rules(facets: &[String]) -> Result<Vec<FacetRule>> {
    let mut rules = Vec::new();

    for facet_def in facets {
        let Some((raw_facet, raw_pattern)) = facet_def.split_once('=') else {
            return Err(anyhow!(
                "Invalid --facet option: missing <pattern> in \"{}\"",
                facet_def
            ));
        };

        let facet = raw_facet.trim().to_ascii_lowercase();
        let pattern_text = raw_pattern.trim();

        if facet.is_empty() {
            return Err(anyhow!(
                "Invalid --facet option: missing <facet> in \"{}\"",
                facet_def
            ));
        }

        if pattern_text.is_empty() {
            return Err(anyhow!(
                "Invalid --facet option: missing <pattern> in \"{}\"",
                facet_def
            ));
        }

        let Some(facet_index) = FACETS.iter().position(|candidate| *candidate == facet) else {
            return Err(anyhow!(
                "Invalid --facet option: unknown <facet> in \"{}\". Valid values are: {}",
                facet_def,
                FACETS.join(", ")
            ));
        };

        let pattern = Pattern::new(pattern_text).map_err(|err| {
            anyhow!(
                "Invalid --facet option: bad glob pattern in \"{}\": {}",
                facet_def,
                err
            )
        })?;

        let target = if pattern_text.contains('/') || pattern_text.contains('\\') {
            FacetMatchTarget::Path
        } else {
            FacetMatchTarget::NameOrPath
        };

        if !rules.iter().any(|rule: &FacetRule| {
            rule.facet_index == facet_index && rule.pattern.as_str() == pattern_text
        }) {
            rules.push(FacetRule {
                facet_index,
                target,
                pattern,
            });
        }
    }

    Ok(rules)
}

fn assign_facets(files: &mut [FileInfo], facet_rules: &[FacetRule]) {
    if facet_rules.is_empty() {
        return;
    }

    for file in files.iter_mut() {
        if file.file_type != FileType::File {
            file.facets.clear();
            continue;
        }

        const FACET_SORT_ORDER: [usize; FACETS.len()] = [0, 4, 1, 3, 5, 2];
        let mut matched_facets = [false; FACETS.len()];
        for rule in facet_rules {
            let is_match = match rule.target {
                FacetMatchTarget::Path => rule.pattern.matches(&file.path),
                FacetMatchTarget::NameOrPath => {
                    rule.pattern.matches(&file.name) || rule.pattern.matches(&file.path)
                }
            };

            if is_match {
                matched_facets[rule.facet_index] = true;
            }
        }

        let facets: Vec<String> = FACET_SORT_ORDER
            .into_iter()
            .filter(|&index| matched_facets[index])
            .map(|index| FACETS[index].to_string())
            .collect();

        file.facets = if facets.is_empty() {
            vec![FACETS[0].to_string()]
        } else {
            facets
        };
    }
}

fn materialize_generated_flags(files: &mut [FileInfo]) {
    for file in files.iter_mut() {
        if file.file_type != FileType::File {
            file.is_generated = Some(false);
            continue;
        }

        if file.is_generated.is_none() {
            file.is_generated = Some(false);
        }
    }
}

#[cfg(test)]
fn mark_generated_files(files: &mut [FileInfo], scanned_root: Option<&Path>) {
    for file in files.iter_mut() {
        if file.file_type != FileType::File {
            file.is_generated = Some(false);
            continue;
        }

        if file.is_generated.is_none() {
            file.is_generated =
                Some(generated_file_hint_exists(&file.path, scanned_root).unwrap_or(false));
        }
    }
}

fn clear_generated_flags(files: &mut [FileInfo]) {
    for file in files {
        file.is_generated = None;
    }
}

fn clear_resource_tallies(files: &mut [FileInfo]) {
    for file in files {
        file.tallies = None;
    }
}

#[cfg(test)]
fn generated_file_hint_exists(path: &str, scanned_root: Option<&Path>) -> Result<bool> {
    let path = resolve_generated_scan_path(path, scanned_root)?;
    Ok(!generated_code_hints(&path)?.is_empty())
}

#[cfg(test)]
fn resolve_generated_scan_path(path: &str, scanned_root: Option<&Path>) -> Result<PathBuf> {
    let candidate = PathBuf::from(path);

    if candidate.is_absolute() {
        return candidate
            .is_file()
            .then_some(candidate)
            .ok_or_else(|| anyhow!("Generated detection path not found: {}", path));
    }

    let Some(scanned_root) = scanned_root else {
        return Err(anyhow!(
            "Generated detection fallback requires an absolute path or scanned root: {}",
            path
        ));
    };

    let anchored = scanned_root.join(&candidate);
    if anchored.is_file() {
        return Ok(anchored);
    }

    Err(anyhow!("Generated detection path not found: {}", path))
}

#[cfg(test)]
fn is_good_match(license_match: &Match) -> bool {
    match (license_match.match_coverage, license_match.rule_relevance) {
        (Some(coverage), Some(relevance)) => {
            license_match.score.is_good() && coverage >= 80.0 && relevance >= 80
        }
        _ => license_match.score.is_good(),
    }
}

#[cfg(test)]
mod tests {
    use super::is_good_match;
    use crate::models::LineNumber;
    use crate::models::MatchScore;
    use crate::models::file_info::Match;

    fn make_match(score: f64, coverage: Option<f64>, relevance: Option<u8>) -> Match {
        Match {
            license_expression: "mit".to_string(),
            license_expression_spdx: "MIT".to_string(),
            from_file: None,
            start_line: LineNumber::ONE,
            end_line: LineNumber::ONE,
            matcher: Some("1-hash".to_string()),
            score: MatchScore::from_percentage(score),
            matched_length: Some(3),
            match_coverage: coverage,
            rule_relevance: relevance,
            rule_identifier: Some("mit.LICENSE".to_string()),
            rule_url: None,
            matched_text: None,
            referenced_filenames: None,
            matched_text_diagnostics: None,
        }
    }

    #[test]
    fn test_is_good_match_does_not_upscale_low_percent_scores() {
        let license_match = make_match(1.0, Some(100.0), Some(100));

        assert!(!is_good_match(&license_match));
    }

    #[test]
    fn test_is_good_match_accepts_percent_scores() {
        let license_match = make_match(81.82, Some(100.0), Some(100));

        assert!(is_good_match(&license_match));
    }
}

fn is_score_key_file(file: &FileInfo) -> bool {
    if !file.is_key_file {
        return false;
    }

    if file.is_manifest {
        return is_score_manifest(file);
    }

    true
}

fn is_score_manifest(file: &FileInfo) -> bool {
    let path = file.path.to_ascii_lowercase();
    path == "cargo.toml"
        || path.ends_with("/cargo.toml")
        || path.ends_with("/pom.xml")
        || path.ends_with("/pom.properties")
        || path == "manifest.mf"
        || path.ends_with("/manifest.mf")
        || path == "metadata.gz-extract"
        || path.ends_with("/metadata.gz-extract")
        || path.ends_with(".gemspec")
}
