use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::license_detection::expression::parse_expression;
use crate::license_detection::models::{LoadedLicense, LoadedRule, RuleKind};
use crate::license_detection::rules::{parse_license_str_to_loaded, parse_rule_str_to_loaded};
use crate::models::LicenseIndexProvenance;
use crate::utils::hash::calculate_sha256;

pub const DEFAULT_INDEX_BUILD_POLICY_PATH: &str =
    "resources/license_detection/index_build_policy.toml";
pub const DEFAULT_INDEX_BUILD_OVERLAY_ROOT: &str = "resources/license_detection/overlay";
pub const EMBEDDED_LICENSE_INDEX_SOURCE: &str = "embedded-artifact";
pub const CUSTOM_RULES_LICENSE_INDEX_SOURCE: &str = "custom-rules";

const DEFAULT_INDEX_BUILD_POLICY_TEXT: &str =
    include_str!("../../resources/license_detection/index_build_policy.toml");

pub(crate) struct BundledOverlayFile {
    pub identifier: &'static str,
    pub contents: &'static str,
}

mod bundled_overlay_manifest {
    use super::BundledOverlayFile;

    include!(concat!(env!("OUT_DIR"), "/bundled_license_overlays.rs"));
}

use bundled_overlay_manifest::{BUNDLED_LICENSE_OVERLAY_FILES, BUNDLED_RULE_OVERLAY_FILES};

static DEFAULT_INDEX_BUILD_POLICY: LazyLock<IndexBuildPolicy> = LazyLock::new(|| {
    toml::from_str(DEFAULT_INDEX_BUILD_POLICY_TEXT).unwrap_or_else(|error| {
        panic!(
            "Failed to parse bundled license index build policy at {}: {}",
            DEFAULT_INDEX_BUILD_POLICY_PATH, error
        )
    })
});

static DEFAULT_INDEX_BUILD_POLICY_FINGERPRINT: LazyLock<String> = LazyLock::new(|| {
    let mut fingerprint_material = Vec::new();
    fingerprint_material.extend_from_slice(DEFAULT_INDEX_BUILD_POLICY_TEXT.as_bytes());

    for overlay in BUNDLED_RULE_OVERLAY_FILES {
        fingerprint_material.extend_from_slice(overlay.identifier.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(overlay.contents.as_bytes());
        fingerprint_material.push(0xFF);
    }

    for overlay in BUNDLED_LICENSE_OVERLAY_FILES {
        fingerprint_material.extend_from_slice(overlay.identifier.as_bytes());
        fingerprint_material.push(0);
        fingerprint_material.extend_from_slice(overlay.contents.as_bytes());
        fingerprint_material.push(0xFF);
    }

    calculate_sha256(&fingerprint_material).to_string()
});

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct IndexBuildPolicy {
    #[serde(default)]
    pub ignored_rules: Vec<String>,
    #[serde(default)]
    pub ignored_licenses: Vec<String>,
}

impl IndexBuildPolicy {
    pub fn is_empty(&self) -> bool {
        self.ignored_rules.is_empty() && self.ignored_licenses.is_empty()
    }

    fn ignored_rule_set(&self) -> HashSet<String> {
        self.ignored_rules
            .iter()
            .map(|identifier| identifier.trim())
            .filter(|identifier| !identifier.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    fn ignored_license_set(&self) -> HashSet<String> {
        self.ignored_licenses
            .iter()
            .map(|key| normalize_license_key(key))
            .filter(|key| !key.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedIndexBuildPolicy {
    pub policy_path: String,
    pub curation_fingerprint: String,
    pub ignored_rules: Vec<String>,
    pub ignored_licenses: Vec<String>,
    pub ignored_rules_due_to_licenses: Vec<String>,
    pub added_rules: Vec<String>,
    pub replaced_rules: Vec<String>,
    pub added_licenses: Vec<String>,
    pub replaced_licenses: Vec<String>,
}

impl AppliedIndexBuildPolicy {
    pub fn is_empty(&self) -> bool {
        self.ignored_rules.is_empty()
            && self.ignored_licenses.is_empty()
            && self.ignored_rules_due_to_licenses.is_empty()
            && self.added_rules.is_empty()
            && self.replaced_rules.is_empty()
            && self.added_licenses.is_empty()
            && self.replaced_licenses.is_empty()
    }

    fn sort_and_dedup(&mut self) {
        for values in [
            &mut self.ignored_rules,
            &mut self.ignored_licenses,
            &mut self.ignored_rules_due_to_licenses,
            &mut self.added_rules,
            &mut self.replaced_rules,
            &mut self.added_licenses,
            &mut self.replaced_licenses,
        ] {
            values.sort();
            values.dedup();
        }
    }

    fn with_default_provenance(mut self) -> Self {
        self.policy_path = DEFAULT_INDEX_BUILD_POLICY_PATH.to_string();
        self.curation_fingerprint = default_index_build_policy_fingerprint().to_string();
        self
    }

    pub fn to_license_index_provenance(&self, source: &str) -> LicenseIndexProvenance {
        LicenseIndexProvenance {
            source: source.to_string(),
            policy_path: self.policy_path.clone(),
            curation_fingerprint: self.curation_fingerprint.clone(),
            ignored_rules: self.ignored_rules.clone(),
            ignored_licenses: self.ignored_licenses.clone(),
            ignored_rules_due_to_licenses: self.ignored_rules_due_to_licenses.clone(),
            added_rules: self.added_rules.clone(),
            replaced_rules: self.replaced_rules.clone(),
            added_licenses: self.added_licenses.clone(),
            replaced_licenses: self.replaced_licenses.clone(),
        }
    }
}

pub fn default_index_build_policy() -> &'static IndexBuildPolicy {
    &DEFAULT_INDEX_BUILD_POLICY
}

pub fn default_index_build_policy_fingerprint() -> &'static str {
    DEFAULT_INDEX_BUILD_POLICY_FINGERPRINT.as_str()
}

pub fn apply_default_index_build_policy(
    loaded_rules: Vec<LoadedRule>,
    loaded_licenses: Vec<LoadedLicense>,
) -> Result<(Vec<LoadedRule>, Vec<LoadedLicense>, AppliedIndexBuildPolicy)> {
    let overlay_rules = load_default_overlay_rules()?;
    let overlay_licenses = load_default_overlay_licenses()?;
    let (loaded_rules, loaded_licenses, report) = apply_index_build_policy(
        loaded_rules,
        loaded_licenses,
        default_index_build_policy(),
        &overlay_rules,
        &overlay_licenses,
    )?;
    Ok((
        loaded_rules,
        loaded_licenses,
        report.with_default_provenance(),
    ))
}

pub fn apply_index_build_policy(
    loaded_rules: Vec<LoadedRule>,
    loaded_licenses: Vec<LoadedLicense>,
    policy: &IndexBuildPolicy,
    overlay_rules: &[LoadedRule],
    overlay_licenses: &[LoadedLicense],
) -> Result<(Vec<LoadedRule>, Vec<LoadedLicense>, AppliedIndexBuildPolicy)> {
    if policy.is_empty() && overlay_rules.is_empty() && overlay_licenses.is_empty() {
        return Ok((
            loaded_rules,
            loaded_licenses,
            AppliedIndexBuildPolicy::default(),
        ));
    }

    let ignored_rule_identifiers = policy.ignored_rule_set();
    let ignored_license_keys = policy.ignored_license_set();
    let mut report = AppliedIndexBuildPolicy::default();

    let mut filtered_licenses: Vec<_> = loaded_licenses
        .into_iter()
        .filter_map(|license| {
            if ignored_license_keys.contains(&normalize_license_key(&license.key)) {
                report.ignored_licenses.push(license.key.clone());
                None
            } else {
                Some(license)
            }
        })
        .collect();

    let mut filtered_rules: Vec<_> = loaded_rules
        .into_iter()
        .filter_map(|rule| {
            if ignored_rule_identifiers.contains(rule.identifier.as_str()) {
                report.ignored_rules.push(rule.identifier.clone());
                return None;
            }

            if rule_references_ignored_license(&rule, &ignored_license_keys) {
                report
                    .ignored_rules_due_to_licenses
                    .push(rule.identifier.clone());
                return None;
            }

            Some(rule)
        })
        .collect();

    ensure_all_ignored_entries_exist(&ignored_rule_identifiers, &ignored_license_keys, &report)?;

    apply_license_overlays(
        &mut filtered_licenses,
        overlay_licenses,
        &ignored_license_keys,
        &mut report,
    )?;
    apply_rule_overlays(
        &mut filtered_rules,
        overlay_rules,
        &ignored_rule_identifiers,
        &ignored_license_keys,
        &filtered_licenses,
        &mut report,
    )?;

    report.sort_and_dedup();

    Ok((filtered_rules, filtered_licenses, report))
}

fn load_default_overlay_rules() -> Result<Vec<LoadedRule>> {
    BUNDLED_RULE_OVERLAY_FILES
        .iter()
        .map(|overlay| {
            parse_rule_str_to_loaded(overlay.identifier, overlay.contents).map_err(|error| {
                anyhow!(
                    "Failed to parse bundled overlay rule {} from {}: {}",
                    overlay.identifier,
                    DEFAULT_INDEX_BUILD_OVERLAY_ROOT,
                    error
                )
            })
        })
        .collect()
}

fn load_default_overlay_licenses() -> Result<Vec<LoadedLicense>> {
    BUNDLED_LICENSE_OVERLAY_FILES
        .iter()
        .map(|overlay| {
            parse_license_str_to_loaded(overlay.identifier, overlay.contents).map_err(|error| {
                anyhow!(
                    "Failed to parse bundled overlay license {} from {}: {}",
                    overlay.identifier,
                    DEFAULT_INDEX_BUILD_OVERLAY_ROOT,
                    error
                )
            })
        })
        .collect()
}

fn ensure_all_ignored_entries_exist(
    ignored_rule_identifiers: &HashSet<String>,
    ignored_license_keys: &HashSet<String>,
    report: &AppliedIndexBuildPolicy,
) -> Result<()> {
    let applied_ignored_rules = report.ignored_rules.iter().cloned().collect::<HashSet<_>>();
    let missing_rules = ignored_rule_identifiers
        .difference(&applied_ignored_rules)
        .cloned()
        .collect::<Vec<_>>();

    let applied_ignored_licenses = report
        .ignored_licenses
        .iter()
        .map(|key| normalize_license_key(key))
        .collect::<HashSet<_>>();
    let missing_licenses = ignored_license_keys
        .difference(&applied_ignored_licenses)
        .cloned()
        .collect::<Vec<_>>();

    if missing_rules.is_empty() && missing_licenses.is_empty() {
        Ok(())
    } else {
        let mut problems = Vec::new();
        if !missing_rules.is_empty() {
            problems.push(format!(
                "ignored rule identifiers not found upstream: {}",
                missing_rules.join(", ")
            ));
        }
        if !missing_licenses.is_empty() {
            problems.push(format!(
                "ignored license keys not found upstream: {}",
                missing_licenses.join(", ")
            ));
        }
        Err(anyhow!(
            "stale index-build policy entries detected; remove or update them: {}",
            problems.join("; ")
        ))
    }
}

fn apply_license_overlays(
    licenses: &mut Vec<LoadedLicense>,
    overlays: &[LoadedLicense],
    ignored_license_keys: &HashSet<String>,
    report: &mut AppliedIndexBuildPolicy,
) -> Result<()> {
    let mut indices = build_license_index_map(licenses)?;
    let mut seen_overlay_keys = HashSet::new();

    for overlay in overlays {
        let key = normalize_license_key(&overlay.key);

        if !seen_overlay_keys.insert(key.clone()) {
            return Err(anyhow!(
                "bundled overlay contains duplicate license key '{}'",
                overlay.key
            ));
        }

        if ignored_license_keys.contains(&key) {
            return Err(anyhow!(
                "overlay license '{}' conflicts with ignored_licenses",
                overlay.key
            ));
        }

        if let Some(index) = indices.get(&key).copied() {
            if licenses[index] == *overlay {
                return Err(anyhow!(
                    "overlay license '{}' is now identical to upstream; remove the local overlay file",
                    overlay.key
                ));
            }
            report.replaced_licenses.push(overlay.key.clone());
            licenses[index] = overlay.clone();
        } else {
            report.added_licenses.push(overlay.key.clone());
            licenses.push(overlay.clone());
            indices.insert(key, licenses.len() - 1);
        }
    }

    Ok(())
}

fn apply_rule_overlays(
    rules: &mut Vec<LoadedRule>,
    overlays: &[LoadedRule],
    ignored_rule_identifiers: &HashSet<String>,
    ignored_license_keys: &HashSet<String>,
    licenses: &[LoadedLicense],
    report: &mut AppliedIndexBuildPolicy,
) -> Result<()> {
    let mut indices = build_rule_index_map(rules)?;
    let mut seen_overlay_identifiers = HashSet::new();
    let available_license_keys = licenses
        .iter()
        .map(|license| normalize_license_key(&license.key))
        .collect::<HashSet<_>>();

    for overlay in overlays {
        let identifier = overlay.identifier.clone();

        if !seen_overlay_identifiers.insert(identifier.clone()) {
            return Err(anyhow!(
                "bundled overlay contains duplicate rule identifier '{}'",
                identifier
            ));
        }

        if ignored_rule_identifiers.contains(identifier.as_str()) {
            return Err(anyhow!(
                "overlay rule '{}' conflicts with ignored_rules",
                identifier
            ));
        }

        if rule_references_ignored_license(overlay, ignored_license_keys) {
            return Err(anyhow!(
                "overlay rule '{}' references an ignored license key",
                identifier
            ));
        }

        ensure_rule_references_known_licenses(overlay, &available_license_keys)?;

        if let Some(index) = indices.get(identifier.as_str()).copied() {
            if rules[index] == *overlay {
                return Err(anyhow!(
                    "overlay rule '{}' is now identical to upstream; remove the local overlay file",
                    identifier
                ));
            }
            report.replaced_rules.push(identifier.clone());
            rules[index] = overlay.clone();
        } else {
            report.added_rules.push(identifier.clone());
            rules.push(overlay.clone());
            indices.insert(identifier, rules.len() - 1);
        }
    }

    Ok(())
}

fn build_rule_index_map(rules: &[LoadedRule]) -> Result<HashMap<String, usize>> {
    let mut indices = HashMap::new();
    for (index, rule) in rules.iter().enumerate() {
        if indices.insert(rule.identifier.clone(), index).is_some() {
            return Err(anyhow!(
                "cannot apply overlay because duplicate rule identifier '{}' is already present",
                rule.identifier
            ));
        }
    }
    Ok(indices)
}

fn build_license_index_map(licenses: &[LoadedLicense]) -> Result<HashMap<String, usize>> {
    let mut indices = HashMap::new();
    for (index, license) in licenses.iter().enumerate() {
        let normalized_key = normalize_license_key(&license.key);
        if indices.insert(normalized_key, index).is_some() {
            return Err(anyhow!(
                "cannot apply overlay because duplicate license key '{}' is already present",
                license.key
            ));
        }
    }
    Ok(indices)
}

fn ensure_rule_references_known_licenses(
    rule: &LoadedRule,
    available_license_keys: &HashSet<String>,
) -> Result<()> {
    if rule.rule_kind == RuleKind::None && rule.is_false_positive {
        return Ok(());
    }

    let expression = parse_expression(&rule.license_expression).map_err(|error| {
        anyhow!(
            "overlay rule '{}' has an invalid license expression '{}': {}",
            rule.identifier,
            rule.license_expression,
            error
        )
    })?;

    let missing_keys = expression
        .license_keys()
        .into_iter()
        .map(|key| normalize_license_key(&key))
        .filter(|key| !available_license_keys.contains(key))
        .collect::<Vec<_>>();

    if missing_keys.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "overlay rule '{}' references unknown license keys: {}",
            rule.identifier,
            missing_keys.join(", ")
        ))
    }
}

fn normalize_license_key(key: &str) -> String {
    key.trim().to_lowercase()
}

fn rule_references_ignored_license(
    rule: &LoadedRule,
    ignored_license_keys: &HashSet<String>,
) -> bool {
    if ignored_license_keys.is_empty() {
        return false;
    }

    let normalized_expression = normalize_license_key(&rule.license_expression);
    if ignored_license_keys.contains(&normalized_expression) {
        return true;
    }

    if rule.rule_kind == RuleKind::None && rule.is_false_positive {
        return false;
    }

    parse_expression(&rule.license_expression)
        .map(|expression| {
            expression
                .license_keys()
                .into_iter()
                .map(|key| normalize_license_key(&key))
                .any(|key| ignored_license_keys.contains(&key))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_loaded_rule(identifier: &str, expression: &str) -> LoadedRule {
        LoadedRule {
            identifier: identifier.to_string(),
            license_expression: expression.to_string(),
            text: format!("{identifier} text"),
            rule_kind: RuleKind::Text,
            is_false_positive: false,
            is_required_phrase: false,
            skip_for_required_phrase_generation: false,
            relevance: Some(100),
            minimum_coverage: None,
            has_stored_minimum_coverage: false,
            is_continuous: false,
            referenced_filenames: None,
            ignorable_urls: None,
            ignorable_emails: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            language: None,
            notes: None,
            is_deprecated: false,
            replaced_by: vec![],
        }
    }

    fn create_loaded_license(key: &str) -> LoadedLicense {
        LoadedLicense {
            key: key.to_string(),
            short_name: Some(key.to_uppercase()),
            name: format!("{key} license"),
            language: Some("en".to_string()),
            spdx_license_key: Some(key.to_uppercase()),
            other_spdx_license_keys: vec![],
            category: Some("Permissive".to_string()),
            owner: None,
            homepage_url: None,
            text: format!("{key} text"),
            reference_urls: vec![],
            osi_license_key: None,
            text_urls: vec![],
            osi_url: None,
            faq_url: None,
            other_urls: vec![],
            notes: None,
            is_deprecated: false,
            is_exception: false,
            is_unknown: false,
            is_generic: false,
            replaced_by: vec![],
            minimum_coverage: None,
            standard_notice: None,
            ignorable_copyrights: None,
            ignorable_holders: None,
            ignorable_authors: None,
            ignorable_urls: None,
            ignorable_emails: None,
        }
    }

    #[test]
    fn test_apply_index_build_policy_filters_direct_and_dependent_entries() {
        let policy = IndexBuildPolicy {
            ignored_rules: vec!["direct.RULE".to_string()],
            ignored_licenses: vec!["apache-2.0".to_string()],
        };

        let rules = vec![
            create_loaded_rule("keep.RULE", "mit"),
            create_loaded_rule("direct.RULE", "mit"),
            create_loaded_rule("dependent.RULE", "mit OR apache-2.0"),
        ];
        let licenses = vec![
            create_loaded_license("mit"),
            create_loaded_license("apache-2.0"),
        ];

        let (filtered_rules, filtered_licenses, report) =
            apply_index_build_policy(rules, licenses, &policy, &[], &[])
                .expect("policy application");

        assert_eq!(
            filtered_rules
                .iter()
                .map(|rule| rule.identifier.as_str())
                .collect::<Vec<_>>(),
            vec!["keep.RULE"]
        );
        assert_eq!(
            filtered_licenses
                .iter()
                .map(|license| license.key.as_str())
                .collect::<Vec<_>>(),
            vec!["mit"]
        );
        assert_eq!(report.ignored_rules, vec!["direct.RULE".to_string()]);
        assert_eq!(report.ignored_licenses, vec!["apache-2.0".to_string()]);
        assert_eq!(
            report.ignored_rules_due_to_licenses,
            vec!["dependent.RULE".to_string()]
        );
    }

    #[test]
    fn test_apply_index_build_policy_fails_for_stale_ignored_entries() {
        let policy = IndexBuildPolicy {
            ignored_rules: vec!["missing.RULE".to_string()],
            ignored_licenses: vec![],
        };

        let error = apply_index_build_policy(
            vec![create_loaded_rule("keep.RULE", "mit")],
            vec![create_loaded_license("mit")],
            &policy,
            &[],
            &[],
        )
        .expect_err("missing ignored rule should fail");

        assert!(
            error
                .to_string()
                .contains("ignored rule identifiers not found upstream: missing.RULE")
        );
    }

    #[test]
    fn test_apply_index_build_policy_infers_add_from_new_overlay_entries() {
        let policy = IndexBuildPolicy::default();
        let overlay_rules = vec![create_loaded_rule("custom-rule.RULE", "mit")];
        let overlay_licenses = vec![create_loaded_license("custom-license")];
        let rules = vec![create_loaded_rule("keep.RULE", "mit")];
        let licenses = vec![create_loaded_license("mit")];

        let (filtered_rules, filtered_licenses, report) =
            apply_index_build_policy(rules, licenses, &policy, &overlay_rules, &overlay_licenses)
                .expect("policy application");

        assert!(
            filtered_rules
                .iter()
                .any(|rule| rule.identifier == "custom-rule.RULE")
        );
        assert!(
            filtered_licenses
                .iter()
                .any(|license| license.key == "custom-license")
        );
        assert_eq!(report.added_rules, vec!["custom-rule.RULE".to_string()]);
        assert_eq!(report.added_licenses, vec!["custom-license".to_string()]);
    }

    #[test]
    fn test_apply_index_build_policy_infers_replace_from_colliding_overlay_entries() {
        let policy = IndexBuildPolicy::default();
        let overlay_rules = vec![LoadedRule {
            text: "updated rule text".to_string(),
            ..create_loaded_rule("replace.RULE", "mit")
        }];
        let overlay_licenses = vec![LoadedLicense {
            name: "MIT Updated".to_string(),
            text: "updated license text".to_string(),
            ..create_loaded_license("mit")
        }];
        let rules = vec![create_loaded_rule("replace.RULE", "mit")];
        let licenses = vec![create_loaded_license("mit")];

        let (filtered_rules, filtered_licenses, report) =
            apply_index_build_policy(rules, licenses, &policy, &overlay_rules, &overlay_licenses)
                .expect("policy application");

        assert_eq!(filtered_rules[0].text, "updated rule text");
        assert_eq!(filtered_licenses[0].name, "MIT Updated");
        assert_eq!(report.replaced_rules, vec!["replace.RULE".to_string()]);
        assert_eq!(report.replaced_licenses, vec!["mit".to_string()]);
    }

    #[test]
    fn test_apply_index_build_policy_rejects_redundant_rule_overlay() {
        let policy = IndexBuildPolicy::default();
        let base_rule = create_loaded_rule("replace.RULE", "mit");
        let error = apply_index_build_policy(
            vec![base_rule.clone()],
            vec![create_loaded_license("mit")],
            &policy,
            &[base_rule],
            &[],
        )
        .expect_err("redundant overlay should fail");

        assert!(
            error
                .to_string()
                .contains("overlay rule 'replace.RULE' is now identical to upstream")
        );
    }

    #[test]
    fn test_apply_index_build_policy_rejects_redundant_license_overlay() {
        let policy = IndexBuildPolicy::default();
        let base_license = create_loaded_license("mit");
        let error = apply_index_build_policy(
            vec![create_loaded_rule("keep.RULE", "mit")],
            vec![base_license.clone()],
            &policy,
            &[],
            &[base_license],
        )
        .expect_err("redundant overlay should fail");

        assert!(
            error
                .to_string()
                .contains("overlay license 'mit' is now identical to upstream")
        );
    }
}
