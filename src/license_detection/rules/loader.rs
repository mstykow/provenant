//! Parse .LICENSE and .RULE files.
//!
//! This module provides two-stage loading:
//! 1. Loader-stage: Parse files into `LoadedRule` and `LoadedLicense`
//! 2. Build-stage: Convert to runtime `Rule` and `License` (deprecated filtering, etc.)
//!
//! The loader-stage functions (`parse_rule_to_loaded`, `parse_license_to_loaded`,
//! `load_loaded_rules_from_directory`, `load_loaded_licenses_from_directory`) return
//! all entries including deprecated ones. Deprecated filtering is a build-stage concern.

use crate::license_detection::index::{loaded_license_to_license, loaded_rule_to_rule};
use crate::license_detection::models::{License, LoadedLicense, LoadedRule, Rule};
use anyhow::{Context, Result, anyhow};
use log::warn;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

static FM_BOUNDARY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^-{3,}\s*$").expect("Invalid frontmatter regex"));

fn deserialize_yes_no_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize, Serialize)]
    #[serde(untagged)]
    enum YesNoOrBool {
        String(String),
        Bool(bool),
    }

    match YesNoOrBool::deserialize(deserializer)? {
        YesNoOrBool::Bool(b) => Ok(Some(b)),
        YesNoOrBool::String(s) => {
            let lower = s.to_lowercase();
            if lower == "yes" || lower == "true" || lower == "1" {
                Ok(Some(true))
            } else if lower == "no" || lower == "false" || lower == "0" {
                Ok(Some(false))
            } else {
                Ok(None)
            }
        }
    }
}

trait ParseNumber {
    fn as_u8(&self) -> Option<u8>;
}

impl ParseNumber for yaml_serde::Number {
    fn as_u8(&self) -> Option<u8> {
        self.as_i64()
            .and_then(|n| {
                if n >= 0 && n <= u8::MAX as i64 {
                    Some(n as u8)
                } else {
                    None
                }
            })
            .or_else(|| {
                self.as_f64().and_then(|f| {
                    if f >= 0.0 && f <= u8::MAX as f64 {
                        Some(f as u8)
                    } else {
                        None
                    }
                })
            })
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LicenseFrontmatter {
    #[serde(default)]
    key: Option<String>,

    #[serde(default)]
    short_name: Option<String>,

    #[serde(default)]
    name: Option<String>,

    #[serde(default)]
    category: Option<String>,

    #[serde(default)]
    owner: Option<String>,

    #[serde(default)]
    homepage_url: Option<String>,

    #[serde(default)]
    notes: Option<String>,

    #[serde(default)]
    spdx_license_key: Option<String>,

    #[serde(default)]
    other_spdx_license_keys: Option<Vec<String>>,

    #[serde(default)]
    osi_license_key: Option<String>,

    #[serde(default)]
    text_urls: Option<Vec<String>>,

    #[serde(default)]
    osi_url: Option<String>,

    #[serde(default)]
    faq_url: Option<String>,

    #[serde(default)]
    other_urls: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_deprecated: Option<bool>,

    #[serde(default)]
    replaced_by: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_exception: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_unknown: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_generic: Option<bool>,

    #[serde(default)]
    minimum_coverage: Option<yaml_serde::Number>,

    #[serde(default)]
    standard_notice: Option<String>,

    #[serde(default)]
    ignorable_copyrights: Option<Vec<String>>,

    #[serde(default)]
    ignorable_holders: Option<Vec<String>>,

    #[serde(default)]
    ignorable_authors: Option<Vec<String>>,

    #[serde(default)]
    ignorable_urls: Option<Vec<String>>,

    #[serde(default)]
    ignorable_emails: Option<Vec<String>>,
}

/// Parsed rule file content, split into frontmatter and text.
struct ParsedRuleFile {
    yaml_content: String,
    text_content: String,
    has_stored_minimum_coverage: bool,
}

/// Parsed license file content, split into frontmatter and text.
struct ParsedLicenseFile {
    yaml_content: String,
    text_content: String,
}

/// Parse file content into frontmatter and text sections.
///
/// Returns `ParsedRuleFile` with yaml_content, text_content, and metadata.
/// The `path` parameter is used for error messages only.
fn parse_file_content(content: &str, path: &Path) -> Result<ParsedRuleFile> {
    if content.len() < 6 {
        return Err(anyhow!("File content too short: {}", path.display()));
    }

    let parts: Vec<&str> = FM_BOUNDARY.splitn(content, 3).collect();

    if parts.len() < 3 {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(anyhow!(
                "File is empty or has no content: {}",
                path.display()
            ));
        }
        return Err(anyhow!("File missing delimiter '---': {}", path.display()));
    }

    let yaml_content = parts
        .get(1)
        .ok_or_else(|| anyhow!("Missing YAML frontmatter in {}", path.display()))?
        .to_string();
    let text_content = parts
        .get(2)
        .ok_or_else(|| {
            anyhow!(
                "Missing text content after frontmatter in {}",
                path.display()
            )
        })?
        .trim_start_matches('\n')
        .trim()
        .to_string();

    let frontmatter_value: yaml_serde::Value =
        yaml_serde::from_str(&yaml_content).map_err(|e| {
            anyhow!(
                "Failed to parse frontmatter YAML in {}: {}\nContent was:\n{}",
                path.display(),
                e,
                yaml_content
            )
        })?;

    let has_stored_minimum_coverage = frontmatter_value.as_mapping().is_some_and(|mapping| {
        mapping.contains_key(yaml_serde::Value::String("minimum_coverage".to_string()))
    });

    Ok(ParsedRuleFile {
        yaml_content,
        text_content,
        has_stored_minimum_coverage,
    })
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RuleFrontmatter {
    #[serde(default)]
    license_expression: Option<String>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_license_text: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_license_notice: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_license_reference: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_license_tag: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_license_intro: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_license_clue: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_false_positive: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_required_phrase: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    skip_for_required_phrase_generation: Option<bool>,

    #[serde(default)]
    relevance: Option<yaml_serde::Number>,

    #[serde(default)]
    minimum_coverage: Option<yaml_serde::Number>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_continuous: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_yes_no_bool")]
    is_deprecated: Option<bool>,

    #[serde(default)]
    referenced_filenames: Option<Vec<String>>,

    #[serde(default)]
    replaced_by: Option<Vec<String>>,

    #[serde(default)]
    ignorable_urls: Option<Vec<String>>,

    #[serde(default)]
    ignorable_emails: Option<Vec<String>>,

    #[serde(default)]
    notes: Option<String>,

    #[serde(default)]
    ignorable_copyrights: Option<Vec<String>>,

    #[serde(default)]
    ignorable_holders: Option<Vec<String>>,

    #[serde(default)]
    ignorable_authors: Option<Vec<String>>,

    #[serde(default)]
    language: Option<String>,
}

/// Parse a .RULE file into a `LoadedRule` (loader-stage).
///
/// This function parses the file and returns a `LoadedRule` with normalized data.
/// Deprecated entries are included - filtering is a build-stage concern.
///
/// # Arguments
/// * `path` - Path to the .RULE file
///
/// # Returns
/// * `Ok(LoadedRule)` - Successfully parsed rule
/// * `Err(...)` - Parse error with context
pub fn parse_rule_to_loaded(path: &Path) -> Result<LoadedRule> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read rule file: {}", path.display()))?;

    let identifier = LoadedRule::derive_identifier(
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown.RULE"),
    );

    let parsed = parse_file_content(&content, path)?;

    if parsed.text_content.is_empty() {
        return Err(anyhow!(
            "Rule file has empty text content: {}",
            path.display()
        ));
    }

    let fm: RuleFrontmatter = yaml_serde::from_str(&parsed.yaml_content).map_err(|e| {
        anyhow!(
            "Failed to parse rule frontmatter YAML in {}: {}\nContent was:\n{}",
            path.display(),
            e,
            parsed.yaml_content
        )
    })?;

    let is_false_positive = fm.is_false_positive.unwrap_or(false);

    let rule_kind = LoadedRule::derive_rule_kind(
        fm.is_license_text.unwrap_or(false),
        fm.is_license_notice.unwrap_or(false),
        fm.is_license_reference.unwrap_or(false),
        fm.is_license_tag.unwrap_or(false),
        fm.is_license_intro.unwrap_or(false),
        fm.is_license_clue.unwrap_or(false),
    )
    .map_err(|e| {
        anyhow!(
            "Rule file has invalid rule-kind flags: {}: {}",
            path.display(),
            e
        )
    })?;

    LoadedRule::validate_rule_kind_flags(rule_kind, is_false_positive)
        .map_err(|e| anyhow!("Rule file has invalid flags: {}: {}", path.display(), e))?;

    let license_expression = LoadedRule::normalize_license_expression(
        fm.license_expression.as_deref(),
        is_false_positive,
    )
    .map_err(|e| {
        anyhow!(
            "Rule file has invalid license_expression: {}: {}",
            path.display(),
            e
        )
    })?;

    let relevance = fm.relevance.and_then(|n| n.as_u8());

    let minimum_coverage = fm.minimum_coverage.and_then(|n| n.as_u8());

    Ok(LoadedRule {
        identifier,
        license_expression,
        text: parsed.text_content,
        rule_kind,
        is_false_positive,
        is_required_phrase: fm.is_required_phrase.unwrap_or(false),
        skip_for_required_phrase_generation: fm
            .skip_for_required_phrase_generation
            .unwrap_or(false),
        relevance,
        minimum_coverage,
        has_stored_minimum_coverage: parsed.has_stored_minimum_coverage,
        is_continuous: fm.is_continuous.unwrap_or(false),
        referenced_filenames: LoadedRule::normalize_optional_list(
            fm.referenced_filenames.as_deref(),
        ),
        ignorable_urls: LoadedRule::normalize_optional_list(fm.ignorable_urls.as_deref()),
        ignorable_emails: LoadedRule::normalize_optional_list(fm.ignorable_emails.as_deref()),
        ignorable_copyrights: LoadedRule::normalize_optional_list(
            fm.ignorable_copyrights.as_deref(),
        ),
        ignorable_holders: LoadedRule::normalize_optional_list(fm.ignorable_holders.as_deref()),
        ignorable_authors: LoadedRule::normalize_optional_list(fm.ignorable_authors.as_deref()),
        language: LoadedRule::normalize_optional_string(fm.language.as_deref()),
        notes: LoadedRule::normalize_optional_string(fm.notes.as_deref()),
        is_deprecated: fm.is_deprecated.unwrap_or(false),
        replaced_by: fm.replaced_by.unwrap_or_default(),
    })
}

/// Parse a .LICENSE file into a `LoadedLicense` (loader-stage).
///
/// This function parses the file and returns a `LoadedLicense` with normalized data.
/// Deprecated entries are included - filtering is a build-stage concern.
///
/// # Arguments
/// * `path` - Path to the .LICENSE file
///
/// # Returns
/// * `Ok(LoadedLicense)` - Successfully parsed license
/// * `Err(...)` - Parse error with context
pub fn parse_license_to_loaded(path: &Path) -> Result<LoadedLicense> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read license file: {}", path.display()))?;

    let key = LoadedLicense::derive_key(path)?;

    let parsed = parse_license_file_content(&content, path)?;

    let fm: LicenseFrontmatter = yaml_serde::from_str(&parsed.yaml_content).map_err(|e| {
        anyhow!(
            "Failed to parse license frontmatter YAML in {}: {}\nContent was:\n{}",
            path.display(),
            e,
            parsed.yaml_content
        )
    })?;

    LoadedLicense::validate_key_match(&key, fm.key.as_deref())
        .map_err(|e| anyhow!("License file has key mismatch: {}: {}", path.display(), e))?;

    let is_deprecated = fm.is_deprecated.unwrap_or(false);
    let is_unknown = fm.is_unknown.unwrap_or(false);
    let is_generic = fm.is_generic.unwrap_or(false);

    LoadedLicense::validate_text_content(
        &parsed.text_content,
        is_deprecated,
        is_unknown,
        is_generic,
    )
    .map_err(|e| {
        anyhow!(
            "License file has invalid content: {}: {}",
            path.display(),
            e
        )
    })?;

    let name = LoadedLicense::derive_name(fm.name.as_deref(), fm.short_name.as_deref(), &key);

    let reference_urls = LoadedLicense::merge_reference_urls(
        fm.text_urls.as_deref(),
        fm.other_urls.as_deref(),
        fm.osi_url.as_deref(),
        fm.faq_url.as_deref(),
        fm.homepage_url.as_deref(),
    );

    let minimum_coverage = fm.minimum_coverage.and_then(|n| n.as_u8());

    Ok(LoadedLicense {
        key,
        short_name: LoadedLicense::normalize_optional_string(fm.short_name.as_deref()),
        name,
        language: Some("en".to_string()),
        spdx_license_key: LoadedLicense::normalize_optional_string(fm.spdx_license_key.as_deref()),
        other_spdx_license_keys: fm.other_spdx_license_keys.unwrap_or_default(),
        category: LoadedLicense::normalize_optional_string(fm.category.as_deref()),
        owner: LoadedLicense::normalize_optional_string(fm.owner.as_deref()),
        homepage_url: LoadedLicense::normalize_optional_string(fm.homepage_url.as_deref()),
        text: parsed.text_content,
        reference_urls,
        osi_license_key: LoadedLicense::normalize_optional_string(fm.osi_license_key.as_deref()),
        text_urls: LoadedLicense::normalize_optional_list(fm.text_urls.as_deref())
            .unwrap_or_default(),
        osi_url: LoadedLicense::normalize_optional_string(fm.osi_url.as_deref()),
        faq_url: LoadedLicense::normalize_optional_string(fm.faq_url.as_deref()),
        other_urls: LoadedLicense::normalize_optional_list(fm.other_urls.as_deref())
            .unwrap_or_default(),
        notes: LoadedLicense::normalize_optional_string(fm.notes.as_deref()),
        is_deprecated,
        is_exception: fm.is_exception.unwrap_or(false),
        is_unknown,
        is_generic,
        replaced_by: fm.replaced_by.unwrap_or_default(),
        minimum_coverage,
        standard_notice: LoadedLicense::normalize_optional_string(fm.standard_notice.as_deref()),
        ignorable_copyrights: LoadedLicense::normalize_optional_list(
            fm.ignorable_copyrights.as_deref(),
        ),
        ignorable_holders: LoadedLicense::normalize_optional_list(fm.ignorable_holders.as_deref()),
        ignorable_authors: LoadedLicense::normalize_optional_list(fm.ignorable_authors.as_deref()),
        ignorable_urls: LoadedLicense::normalize_optional_list(fm.ignorable_urls.as_deref()),
        ignorable_emails: LoadedLicense::normalize_optional_list(fm.ignorable_emails.as_deref()),
    })
}

/// Parse license file content into frontmatter and text sections.
///
/// The `path` parameter is used for error messages only.
fn parse_license_file_content(content: &str, path: &Path) -> Result<ParsedLicenseFile> {
    if content.len() < 6 {
        return Err(anyhow!(
            "License file content too short: {}",
            path.display()
        ));
    }

    let parts: Vec<&str> = FM_BOUNDARY.splitn(content, 3).collect();

    if parts.len() < 3 {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(anyhow!(
                "License file is empty or has no content: {}",
                path.display()
            ));
        }
        return Err(anyhow!(
            "License file missing delimiter '---': {}",
            path.display()
        ));
    }

    let yaml_content = parts
        .get(1)
        .ok_or_else(|| anyhow!("Missing YAML frontmatter in {}", path.display()))?
        .to_string();
    let text_content = parts
        .get(2)
        .ok_or_else(|| {
            anyhow!(
                "Missing text content after frontmatter in {}",
                path.display()
            )
        })?
        .trim_start_matches('\n')
        .trim()
        .to_string();

    Ok(ParsedLicenseFile {
        yaml_content,
        text_content,
    })
}

/// Load all .RULE files from a directory into `LoadedRule` values (loader-stage).
///
/// This function loads ALL rules, including deprecated ones.
/// Deprecated filtering is a build-stage concern.
///
/// # Arguments
/// * `dir` - Directory containing .RULE files
///
/// # Returns
/// * `Ok(Vec<LoadedRule>)` - All loaded rules (including deprecated)
/// * `Err(...)` - Directory read error
pub fn load_loaded_rules_from_directory(dir: &Path) -> Result<Vec<LoadedRule>> {
    let mut rules = Vec::new();

    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read rules directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry
            .with_context(|| format!("Failed to read directory entry in: {}", dir.display()))?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("RULE") {
            match parse_rule_to_loaded(&path) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    warn!("Failed to parse rule file {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(rules)
}

/// Load all .LICENSE files from a directory into `LoadedLicense` values (loader-stage).
///
/// This function loads ALL licenses, including deprecated ones.
/// Deprecated filtering is a build-stage concern.
///
/// # Arguments
/// * `dir` - Directory containing .LICENSE files
///
/// # Returns
/// * `Ok(Vec<LoadedLicense>)` - All loaded licenses (including deprecated)
/// * `Err(...)` - Directory read error
pub fn load_loaded_licenses_from_directory(dir: &Path) -> Result<Vec<LoadedLicense>> {
    let mut licenses = Vec::new();

    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read licenses directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry
            .with_context(|| format!("Failed to read directory entry in: {}", dir.display()))?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("LICENSE") {
            match parse_license_to_loaded(&path) {
                Ok(license) => licenses.push(license),
                Err(e) => {
                    warn!("Failed to parse license file {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(licenses)
}

/// Validate loaded rules for common issues.
///
/// Checks for:
/// 1. Duplicate rule texts (warns if found)
/// 2. Empty license expressions for non-false-positive rules (warns if found)
///
/// Corresponds to Python:
/// - `models.py:validate()` for license expression validation
/// - `index.py:_add_rules()` for duplicate detection via hash
///
/// Kept for backward compatibility with `load_rules_from_directory`.
#[allow(dead_code)]
fn validate_rules(rules: &[Rule]) {
    let mut seen_texts: HashSet<&str> = HashSet::new();
    let mut duplicate_count = 0;

    for rule in rules {
        if !seen_texts.insert(&rule.text) {
            warn!(
                "Duplicate rule text found for license_expression: {}",
                rule.license_expression
            );
            duplicate_count += 1;
        }

        if !rule.is_false_positive && rule.license_expression.trim().is_empty() {
            warn!("Rule has empty license_expression but is not marked as false_positive");
        }
    }

    if duplicate_count > 0 {
        warn!(
            "Found {} duplicate rule text(s) during rule validation",
            duplicate_count
        );
    }
}

/// Load all .RULE files from a directory into `Rule` values (backward-compatible).
///
/// This function loads rules and applies deprecated filtering during loading.
/// For the two-stage pipeline, prefer `load_loaded_rules_from_directory` and
/// `build_index_from_loaded`.
///
/// Kept for backward compatibility and testing despite not being used in production code.
/// The new pipeline uses the two-stage loading process instead.
#[allow(dead_code)]
pub fn load_rules_from_directory(dir: &Path, with_deprecated: bool) -> Result<Vec<Rule>> {
    let loaded = load_loaded_rules_from_directory(dir)?;
    let rules: Vec<Rule> = loaded
        .into_iter()
        .filter(|r| with_deprecated || !r.is_deprecated)
        .map(loaded_rule_to_rule)
        .collect();
    validate_rules(&rules);
    Ok(rules)
}

/// Load all .LICENSE files from a directory into `License` values (backward-compatible).
///
/// This function loads licenses and applies deprecated filtering during loading.
/// For the two-stage pipeline, prefer `load_loaded_licenses_from_directory` and
/// `build_index_from_loaded`.
///
/// Kept for backward compatibility and testing despite not being used in production code.
/// The new pipeline uses the two-stage loading process instead.
#[allow(dead_code)]
pub fn load_licenses_from_directory(dir: &Path, with_deprecated: bool) -> Result<Vec<License>> {
    let loaded = load_loaded_licenses_from_directory(dir)?;
    let licenses: Vec<License> = loaded
        .into_iter()
        .filter(|l| with_deprecated || !l.is_deprecated)
        .map(loaded_license_to_license)
        .collect();
    Ok(licenses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    pub fn parse_rule_file(path: &Path) -> Result<Rule> {
        let loaded = parse_rule_to_loaded(path)?;
        Ok(loaded_rule_to_rule(loaded))
    }

    #[test]
    fn test_parse_number_as_u8() {
        let num_int: yaml_serde::Number = yaml_serde::from_str("100").unwrap();
        assert_eq!(num_int.as_u8(), Some(100));

        let num_out_of_range: yaml_serde::Number = yaml_serde::from_str("500").unwrap();
        assert_eq!(num_out_of_range.as_u8(), None);

        let num_float: yaml_serde::Number = yaml_serde::from_str("90.5").unwrap();
        assert_eq!(num_float.as_u8(), Some(90));
    }

    #[test]
    fn test_parse_simple_license_file() {
        let dir = tempdir().unwrap();
        let license_path = dir.path().join("mit.LICENSE");
        fs::write(
            &license_path,
            r#"---
key: mit
short_name: MIT License
name: MIT License
category: Permissive
spdx_license_key: MIT
---
MIT License text here"#,
        )
        .unwrap();

        let license = parse_license_to_loaded(&license_path)
            .map(loaded_license_to_license)
            .unwrap();
        assert_eq!(license.key, "mit");
        assert_eq!(license.name, "MIT License");
        assert!(license.text.contains("MIT License text"));
    }

    #[test]
    fn test_parse_simple_rule_file() {
        let dir = tempdir().unwrap();
        let rule_path = dir.path().join("mit_1.RULE");
        fs::write(
            &rule_path,
            r#"---
license_expression: mit
is_license_reference: yes
relevance: 90
referenced_filenames:
    - MIT.txt
---
MIT.txt"#,
        )
        .unwrap();

        let rule = parse_rule_file(&rule_path).unwrap();
        assert_eq!(rule.license_expression, "mit");
        assert_eq!(rule.text, "MIT.txt");
        assert!(rule.is_license_reference());
        assert_eq!(rule.relevance, 90);
    }

    #[test]
    fn test_deserialize_yes_no_bool() {
        let dir = tempdir().unwrap();
        let rule_path = dir.path().join("test.RULE");

        fs::write(
            &rule_path,
            r#"---
license_expression: mit
is_license_notice: yes
is_license_tag: no
---
MIT License"#,
        )
        .unwrap();

        let rule = parse_rule_file(&rule_path).unwrap();
        assert!(rule.is_license_notice());
        assert!(!rule.is_license_tag());
    }

    #[test]
    fn test_load_licenses_from_directory() {
        let dir = tempdir().unwrap();

        fs::write(
            dir.path().join("test.LICENSE"),
            r#"---
key: test
name: Test License
spdx_license_key: TEST
category: Permissive
---
Test license text here"#,
        )
        .unwrap();

        let licenses = load_licenses_from_directory(dir.path(), false).unwrap();
        assert_eq!(licenses.len(), 1);

        let license = &licenses[0];
        assert_eq!(license.key, "test");
        assert_eq!(license.name, "Test License");
        assert_eq!(license.spdx_license_key, Some("TEST".to_string()));
        assert!(!license.text.is_empty());
    }

    #[test]
    fn test_load_rules_from_directory() {
        let dir = tempdir().unwrap();

        fs::write(
            dir.path().join("test_1.RULE"),
            r#"---
license_expression: test
is_license_reference: yes
relevance: 85
referenced_filenames:
    - TEST.txt
---
TEST.txt"#,
        )
        .unwrap();

        let rules = load_rules_from_directory(dir.path(), false).unwrap();
        assert_eq!(rules.len(), 1);

        let rule = &rules[0];
        assert_eq!(rule.license_expression, "test");
        assert!(rule.is_license_reference());
        assert_eq!(rule.relevance, 85);
    }

    #[test]
    fn test_validate_rules_detects_duplicates() {
        let rules = vec![
            Rule {
                identifier: "mit.LICENSE".to_string(),
                license_expression: "mit".to_string(),
                text: "MIT License".to_string(),
                tokens: vec![],
                rule_kind: crate::license_detection::models::RuleKind::Text,
                is_false_positive: false,
                is_required_phrase: false,
                is_from_license: false,
                relevance: 100,
                minimum_coverage: None,
                has_stored_minimum_coverage: false,
                is_continuous: false,
                required_phrase_spans: vec![],
                stopwords_by_pos: HashMap::new(),
                referenced_filenames: None,
                ignorable_urls: None,
                ignorable_emails: None,
                ignorable_copyrights: None,
                ignorable_holders: None,
                ignorable_authors: None,
                language: None,
                notes: None,
                length_unique: 0,
                high_length_unique: 0,
                high_length: 0,
                min_matched_length: 0,
                min_high_matched_length: 0,
                min_matched_length_unique: 0,
                min_high_matched_length_unique: 0,
                is_small: false,
                is_tiny: false,
                starts_with_license: false,
                ends_with_license: false,
                is_deprecated: false,
                spdx_license_key: None,
                other_spdx_license_keys: vec![],
            },
            Rule {
                identifier: "apache-2.0.LICENSE".to_string(),
                license_expression: "apache-2.0".to_string(),
                text: "MIT License".to_string(),
                tokens: vec![],
                rule_kind: crate::license_detection::models::RuleKind::Text,
                is_false_positive: false,
                is_required_phrase: false,
                is_from_license: false,
                relevance: 100,
                minimum_coverage: None,
                has_stored_minimum_coverage: false,
                is_continuous: false,
                required_phrase_spans: vec![],
                stopwords_by_pos: HashMap::new(),
                referenced_filenames: None,
                ignorable_urls: None,
                ignorable_emails: None,
                ignorable_copyrights: None,
                ignorable_holders: None,
                ignorable_authors: None,
                language: None,
                notes: None,
                length_unique: 0,
                high_length_unique: 0,
                high_length: 0,
                min_matched_length: 0,
                min_high_matched_length: 0,
                min_matched_length_unique: 0,
                min_high_matched_length_unique: 0,
                is_small: false,
                is_tiny: false,
                starts_with_license: false,
                ends_with_license: false,
                is_deprecated: false,
                spdx_license_key: None,
                other_spdx_license_keys: vec![],
            },
        ];

        validate_rules(&rules);
    }

    #[test]
    fn test_validate_rules_accepts_false_positive_without_expression() {
        let rules = vec![Rule {
            identifier: "fp.RULE".to_string(),
            license_expression: "".to_string(),
            text: "Some text".to_string(),
            tokens: vec![],
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_false_positive: true,
            is_required_phrase: false,
            is_from_license: false,
            relevance: 100,
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: false,
            required_phrase_spans: vec![],
            stopwords_by_pos: HashMap::new(),
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: Some("False positive for common pattern".to_string()),
            length_unique: 0,
            high_length_unique: 0,
            high_length: 0,
            min_matched_length: 0,
            min_high_matched_length: 0,
            min_matched_length_unique: 0,
            min_high_matched_length_unique: 0,
            is_small: false,
            is_tiny: false,
            starts_with_license: false,
            ends_with_license: false,
            is_deprecated: false,
            spdx_license_key: None,
            other_spdx_license_keys: vec![],
        }];

        validate_rules(&rules);
    }

    #[test]
    fn test_validate_rules_no_duplicates() {
        let rules = vec![
            Rule {
                identifier: "mit.LICENSE".to_string(),
                license_expression: "mit".to_string(),
                text: "MIT License".to_string(),
                tokens: vec![],
                rule_kind: crate::license_detection::models::RuleKind::Text,
                is_false_positive: false,
                is_required_phrase: false,
                is_from_license: false,
                relevance: 100,
                minimum_coverage: None,
                has_stored_minimum_coverage: false,
                is_continuous: false,
                required_phrase_spans: vec![],
                stopwords_by_pos: HashMap::new(),
                referenced_filenames: None,
                ignorable_urls: None,
                ignorable_emails: None,
                ignorable_copyrights: None,
                ignorable_holders: None,
                ignorable_authors: None,
                language: None,
                notes: None,
                length_unique: 0,
                high_length_unique: 0,
                high_length: 0,
                min_matched_length: 0,
                min_high_matched_length: 0,
                min_matched_length_unique: 0,
                min_high_matched_length_unique: 0,
                is_small: false,
                is_tiny: false,
                starts_with_license: false,
                ends_with_license: false,
                is_deprecated: false,
                spdx_license_key: None,
                other_spdx_license_keys: vec![],
            },
            Rule {
                identifier: "apache-2.0.LICENSE".to_string(),
                license_expression: "apache-2.0".to_string(),
                text: "Apache License".to_string(),
                tokens: vec![],
                rule_kind: crate::license_detection::models::RuleKind::Text,
                is_false_positive: false,
                is_required_phrase: false,
                is_from_license: false,
                relevance: 100,
                minimum_coverage: None,
                has_stored_minimum_coverage: false,
                is_continuous: false,
                required_phrase_spans: vec![],
                stopwords_by_pos: HashMap::new(),
                referenced_filenames: None,
                ignorable_urls: None,
                ignorable_emails: None,
                ignorable_copyrights: None,
                ignorable_holders: None,
                ignorable_authors: None,
                language: None,
                notes: None,
                length_unique: 0,
                high_length_unique: 0,
                high_length: 0,
                min_matched_length: 0,
                min_high_matched_length: 0,
                min_matched_length_unique: 0,
                min_high_matched_length_unique: 0,
                is_small: false,
                is_tiny: false,
                starts_with_license: false,
                ends_with_license: false,
                is_deprecated: false,
                spdx_license_key: None,
                other_spdx_license_keys: vec![],
            },
        ];

        validate_rules(&rules);
    }

    #[test]
    fn test_load_licenses_filters_deprecated_by_default() {
        let dir = tempdir().unwrap();

        fs::write(
            dir.path().join("active.LICENSE"),
            r#"---
key: active
name: Active License
---
Active license text"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("deprecated.LICENSE"),
            r#"---
key: deprecated
name: Deprecated License
is_deprecated: yes
---
Deprecated license text"#,
        )
        .unwrap();

        let licenses_without = load_licenses_from_directory(dir.path(), false).unwrap();
        assert_eq!(licenses_without.len(), 1);
        assert_eq!(licenses_without[0].key, "active");

        let licenses_with = load_licenses_from_directory(dir.path(), true).unwrap();
        assert_eq!(licenses_with.len(), 2);
    }

    #[test]
    fn test_load_rules_filters_deprecated_by_default() {
        let dir = tempdir().unwrap();

        fs::write(
            dir.path().join("active.RULE"),
            r#"---
license_expression: active
is_license_notice: yes
---
Active rule text"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("deprecated.RULE"),
            r#"---
license_expression: deprecated
is_license_notice: yes
is_deprecated: yes
---
Deprecated rule text"#,
        )
        .unwrap();

        let rules_without = load_rules_from_directory(dir.path(), false).unwrap();
        assert_eq!(rules_without.len(), 1);
        assert_eq!(rules_without[0].license_expression, "active");

        let rules_with = load_rules_from_directory(dir.path(), true).unwrap();
        assert_eq!(rules_with.len(), 2);
    }

    #[test]
    fn test_parse_rule_to_loaded() {
        let dir = tempdir().unwrap();
        let rule_path = dir.path().join("mit_1.RULE");
        fs::write(
            &rule_path,
            r#"---
license_expression: mit
is_license_reference: yes
relevance: 90
referenced_filenames:
    - MIT.txt
---
MIT.txt"#,
        )
        .unwrap();

        let loaded = parse_rule_to_loaded(&rule_path).unwrap();
        assert_eq!(loaded.identifier, "mit_1.RULE");
        assert_eq!(loaded.license_expression, "mit");
        assert_eq!(loaded.text, "MIT.txt");
        assert_eq!(
            loaded.rule_kind,
            crate::license_detection::models::RuleKind::Reference
        );
        assert_eq!(loaded.relevance, Some(90));
        assert_eq!(
            loaded.referenced_filenames,
            Some(vec!["MIT.txt".to_string()])
        );
        assert!(!loaded.is_deprecated);
    }

    #[test]
    fn test_parse_license_to_loaded() {
        let dir = tempdir().unwrap();
        let license_path = dir.path().join("mit.LICENSE");
        fs::write(
            &license_path,
            r#"---
key: mit
short_name: MIT License
name: MIT License
category: Permissive
spdx_license_key: MIT
---
MIT License text here"#,
        )
        .unwrap();

        let loaded = parse_license_to_loaded(&license_path).unwrap();
        assert_eq!(loaded.key, "mit");
        assert_eq!(loaded.name, "MIT License");
        assert!(loaded.text.contains("MIT License text"));
        assert_eq!(loaded.spdx_license_key, Some("MIT".to_string()));
    }

    #[test]
    fn test_load_loaded_rules_from_directory_includes_deprecated() {
        let dir = tempdir().unwrap();

        fs::write(
            dir.path().join("active.RULE"),
            r#"---
license_expression: active
is_license_notice: yes
---
Active rule text"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("deprecated.RULE"),
            r#"---
license_expression: deprecated
is_license_notice: yes
is_deprecated: yes
---
Deprecated rule text"#,
        )
        .unwrap();

        let loaded_rules = load_loaded_rules_from_directory(dir.path()).unwrap();
        assert_eq!(loaded_rules.len(), 2);

        let active = loaded_rules
            .iter()
            .find(|r| r.license_expression == "active")
            .unwrap();
        assert!(!active.is_deprecated);

        let deprecated = loaded_rules
            .iter()
            .find(|r| r.license_expression == "deprecated")
            .unwrap();
        assert!(deprecated.is_deprecated);
    }

    #[test]
    fn test_load_loaded_licenses_from_directory_includes_deprecated() {
        let dir = tempdir().unwrap();

        fs::write(
            dir.path().join("active.LICENSE"),
            r#"---
key: active
name: Active License
---
Active license text"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("deprecated.LICENSE"),
            r#"---
key: deprecated
name: Deprecated License
is_deprecated: yes
---
Deprecated license text"#,
        )
        .unwrap();

        let loaded_licenses = load_loaded_licenses_from_directory(dir.path()).unwrap();
        assert_eq!(loaded_licenses.len(), 2);

        let active = loaded_licenses.iter().find(|l| l.key == "active").unwrap();
        assert!(!active.is_deprecated);

        let deprecated = loaded_licenses
            .iter()
            .find(|l| l.key == "deprecated")
            .unwrap();
        assert!(deprecated.is_deprecated);
    }
}
