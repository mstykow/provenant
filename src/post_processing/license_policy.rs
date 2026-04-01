use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::license_detection::expression::{LicenseExpression, parse_expression};
use crate::models::{FileInfo, LicensePolicyEntry};

#[derive(Debug, Deserialize)]
struct LicensePolicyFile {
    license_policies: Vec<LicensePolicyEntry>,
}

enum PolicyFileStatus {
    Ready(Vec<LicensePolicyEntry>),
    SoftError(String),
}

pub(crate) fn apply_license_policy_from_file(
    files: &mut [FileInfo],
    policy_path: &Path,
) -> Result<Vec<String>> {
    match load_license_policy(policy_path)? {
        PolicyFileStatus::Ready(policies) => {
            apply_license_policy(files, &policies)?;
            Ok(Vec::new())
        }
        PolicyFileStatus::SoftError(error) => {
            for file in files {
                if file.file_type == crate::models::FileType::File {
                    file.license_policy = Some(vec![]);
                }
            }
            Ok(vec![error])
        }
    }
}

fn load_license_policy(policy_path: &Path) -> Result<PolicyFileStatus> {
    let policy_text = fs::read_to_string(policy_path).map_err(|err| {
        anyhow!(
            "Failed to read license policy file {:?}: {err}",
            policy_path
        )
    })?;
    let policy_file: LicensePolicyFile = yaml_serde::from_str(&policy_text).map_err(|err| {
        anyhow!(
            "Failed to parse license policy file {:?}: {err}",
            policy_path
        )
    })?;

    if policy_file.license_policies.is_empty() {
        return Ok(PolicyFileStatus::SoftError(format!(
            "License policy file {:?} is empty",
            policy_path
        )));
    }

    let mut seen = BTreeSet::new();
    for policy in &policy_file.license_policies {
        if !seen.insert(policy.license_key.clone()) {
            return Ok(PolicyFileStatus::SoftError(format!(
                "License policy file {:?} contains duplicate license key {:?}",
                policy_path, policy.license_key
            )));
        }
    }

    Ok(PolicyFileStatus::Ready(policy_file.license_policies))
}

fn apply_license_policy(files: &mut [FileInfo], policies: &[LicensePolicyEntry]) -> Result<()> {
    for file in files {
        if file.file_type != crate::models::FileType::File {
            continue;
        }
        let license_keys = file_license_keys(file)?;
        let mut matched_policies: Vec<_> = policies
            .iter()
            .filter(|policy| license_keys.contains(&policy.license_key))
            .cloned()
            .collect();
        matched_policies.sort_by(|left, right| left.license_key.cmp(&right.license_key));
        file.license_policy = Some(matched_policies);
    }

    Ok(())
}

fn file_license_keys(file: &FileInfo) -> Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for detection in &file.license_detections {
        collect_license_keys(&detection.license_expression, &mut keys)?;
    }
    Ok(keys)
}

fn collect_license_keys(expression: &str, keys: &mut BTreeSet<String>) -> Result<()> {
    if expression.trim().is_empty() {
        return Ok(());
    }

    let parsed = parse_expression(expression)
        .map_err(|err| anyhow!("Failed to parse license expression {:?}: {err}", expression))?;
    collect_expression_keys(&parsed, keys);
    Ok(())
}

fn collect_expression_keys(expression: &LicenseExpression, keys: &mut BTreeSet<String>) {
    match expression {
        LicenseExpression::License(key) | LicenseExpression::LicenseRef(key) => {
            keys.insert(key.clone());
        }
        LicenseExpression::And { left, right }
        | LicenseExpression::Or { left, right }
        | LicenseExpression::With { left, right } => {
            collect_expression_keys(left, keys);
            collect_expression_keys(right, keys);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply_license_policy_from_file;
    use crate::models::{FileInfo, FileType, LicenseDetection};

    #[test]
    fn apply_license_policy_populates_matching_file_entries() {
        let temp = tempfile::tempdir().expect("temp dir");
        let policy_path = temp.path().join("policy.yml");
        std::fs::write(
            &policy_path,
            "license_policies:\n  - license_key: mit\n    label: Approved\n    color_code: '#00ff00'\n    icon: ok\n",
        )
        .expect("policy written");

        let mut files = vec![FileInfo::new(
            "LICENSE".to_string(),
            "LICENSE".to_string(),
            String::new(),
            "LICENSE".to_string(),
            FileType::File,
            None,
            None,
            0,
            None,
            None,
            None,
            None,
            None,
            vec![],
            Some("mit".to_string()),
            vec![LicenseDetection {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                matches: vec![],
                detection_log: vec![],
                identifier: None,
            }],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )];

        apply_license_policy_from_file(&mut files, &policy_path)
            .expect("policy application succeeds");

        let entries = files[0]
            .license_policy
            .as_ref()
            .expect("license policy present");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].license_key, "mit");
        assert_eq!(entries[0].label, "Approved");
    }

    #[test]
    fn apply_license_policy_keeps_scan_running_on_duplicate_license_keys() {
        let temp = tempfile::tempdir().expect("temp dir");
        let policy_path = temp.path().join("policy.yml");
        std::fs::write(
            &policy_path,
            "license_policies:\n  - license_key: mit\n    label: Approved\n    color_code: '#00ff00'\n    icon: ok\n  - license_key: mit\n    label: Duplicate\n    color_code: '#ff0000'\n    icon: stop\n",
        )
        .expect("policy written");

        let mut files = vec![FileInfo::new(
            "LICENSE".to_string(),
            "LICENSE".to_string(),
            String::new(),
            "LICENSE".to_string(),
            FileType::File,
            None,
            None,
            0,
            None,
            None,
            None,
            None,
            None,
            vec![],
            Some("mit".to_string()),
            vec![LicenseDetection {
                license_expression: "mit".to_string(),
                license_expression_spdx: "MIT".to_string(),
                matches: vec![],
                detection_log: vec![],
                identifier: None,
            }],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )];

        let errors = apply_license_policy_from_file(&mut files, &policy_path)
            .expect("duplicate policy should not abort scan");

        assert_eq!(files[0].license_policy, Some(vec![]));
        assert!(files[0].scan_errors.is_empty());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("duplicate license key"))
        );
    }

    #[test]
    fn apply_license_policy_skips_directory_resources() {
        let temp = tempfile::tempdir().expect("temp dir");
        let policy_path = temp.path().join("policy.yml");
        std::fs::write(
            &policy_path,
            "license_policies:\n  - license_key: mit\n    label: Approved\n    color_code: '#00ff00'\n    icon: ok\n",
        )
        .expect("policy written");

        let mut files = vec![FileInfo::new(
            "src".to_string(),
            "src".to_string(),
            String::new(),
            "src".to_string(),
            FileType::Directory,
            None,
            None,
            0,
            None,
            None,
            None,
            None,
            None,
            vec![],
            None,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )];

        apply_license_policy_from_file(&mut files, &policy_path)
            .expect("policy application succeeds");

        assert!(files[0].license_policy.is_none());
    }
}
