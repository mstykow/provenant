use std::collections::{BTreeSet, HashMap};

use crate::license_detection::expression::parse_expression;
use crate::license_detection::index::LicenseIndex;
use crate::license_detection::models::Rule;
use crate::license_detection::spdx_mapping::build_spdx_mapping;
use crate::models::{
    FileInfo, LicenseDetection, LicenseReference, LicenseRuleReference, Match, Package, PackageData,
};

const SCANCODE_LICENSE_URL_BASE: &str =
    "https://github.com/aboutcode-org/scancode-toolkit/tree/develop/src/licensedcode/data/licenses";
const SPDX_LICENSE_URL_BASE: &str = "https://spdx.org/licenses";

pub(crate) fn collect_top_level_license_references(
    files: &[FileInfo],
    packages: &[Package],
    license_index: &LicenseIndex,
    license_url_template: &str,
) -> (Vec<LicenseReference>, Vec<LicenseRuleReference>) {
    let licenses: Vec<_> = license_index.licenses_by_key.values().cloned().collect();
    let spdx_mapping = build_spdx_mapping(&licenses);
    let mut license_keys = BTreeSet::new();
    let mut rule_identifiers = BTreeSet::new();

    for file in files {
        collect_license_keys_from_expression(file.license_expression.as_deref(), &mut license_keys);
        collect_rule_identifiers_from_detections(&file.license_detections, &mut rule_identifiers);
        collect_rule_identifiers_from_matches(&file.license_clues, &mut rule_identifiers);

        for package_data in &file.package_data {
            collect_license_keys_from_package_data(package_data, &mut license_keys);
        }
    }

    for package in packages {
        collect_license_keys_from_expression(
            package.declared_license_expression.as_deref(),
            &mut license_keys,
        );
        collect_license_keys_from_expression(
            package.other_license_expression.as_deref(),
            &mut license_keys,
        );
        collect_license_keys_from_detections(&package.license_detections, &mut license_keys);
        collect_license_keys_from_detections(&package.other_license_detections, &mut license_keys);
        collect_rule_identifiers_from_detections(
            &package.license_detections,
            &mut rule_identifiers,
        );
        collect_rule_identifiers_from_detections(
            &package.other_license_detections,
            &mut rule_identifiers,
        );
    }

    let rules_by_identifier: HashMap<&str, &Rule> = license_index
        .rules_by_rid
        .iter()
        .map(|rule| (rule.identifier.as_str(), rule))
        .collect();

    for identifier in &rule_identifiers {
        if let Some(rule) = rules_by_identifier.get(identifier.as_str()) {
            collect_license_keys_from_expression(Some(&rule.license_expression), &mut license_keys);
        }
    }

    let license_references = license_keys
        .into_iter()
        .filter_map(|key| {
            license_index.licenses_by_key.get(&key).map(|license| {
                let spdx_license_key = spdx_mapping.scancode_to_spdx(&key).unwrap_or_default();
                let short_name = license.short_name.clone().unwrap_or_else(|| {
                    if spdx_license_key.is_empty()
                        || spdx_license_key.starts_with("LicenseRef-scancode-")
                    {
                        license.name.clone()
                    } else {
                        spdx_license_key.clone()
                    }
                });

                LicenseReference {
                    key: Some(license.key.clone()),
                    language: license.language.clone(),
                    name: license.name.clone(),
                    short_name,
                    owner: license.owner.clone(),
                    homepage_url: license.homepage_url.clone(),
                    spdx_license_key: spdx_license_key.clone(),
                    other_spdx_license_keys: license.other_spdx_license_keys.clone(),
                    osi_license_key: license.osi_license_key.clone(),
                    text_urls: license.text_urls.clone(),
                    osi_url: license.osi_url.clone(),
                    faq_url: license.faq_url.clone(),
                    other_urls: license.other_urls.clone(),
                    category: license.category.clone(),
                    is_exception: license.is_exception,
                    is_unknown: license.is_unknown,
                    is_generic: license.is_generic,
                    notes: license.notes.clone(),
                    minimum_coverage: license.minimum_coverage,
                    standard_notice: license.standard_notice.clone(),
                    ignorable_copyrights: license.ignorable_copyrights.clone().unwrap_or_default(),
                    ignorable_holders: license.ignorable_holders.clone().unwrap_or_default(),
                    ignorable_authors: license.ignorable_authors.clone().unwrap_or_default(),
                    ignorable_urls: license.ignorable_urls.clone().unwrap_or_default(),
                    ignorable_emails: license.ignorable_emails.clone().unwrap_or_default(),
                    scancode_url: Some(format!(
                        "{SCANCODE_LICENSE_URL_BASE}/{}.LICENSE",
                        license.key
                    )),
                    licensedb_url: Some(format_license_reference_url(
                        license_url_template,
                        &license.key,
                    )),
                    spdx_url: (!spdx_license_key.is_empty()
                        && !spdx_license_key.starts_with("LicenseRef-scancode-"))
                    .then(|| format!("{SPDX_LICENSE_URL_BASE}/{}", spdx_license_key)),
                    text: license.text.clone(),
                }
            })
        })
        .collect();

    let license_rule_references = rule_identifiers
        .into_iter()
        .filter_map(|identifier| {
            rules_by_identifier.get(identifier.as_str()).map(|rule| {
                let is_synthetic = is_synthetic_rule(rule);
                let metadata = license_index
                    .rule_metadata_by_identifier
                    .get(identifier.as_str());
                LicenseRuleReference {
                    identifier: rule.identifier.clone(),
                    license_expression: rule.license_expression.clone(),
                    is_license_text: rule.is_license_text(),
                    is_license_notice: rule.is_license_notice(),
                    is_license_reference: rule.is_license_reference(),
                    is_license_tag: rule.is_license_tag(),
                    is_license_clue: rule.is_license_clue(),
                    is_license_intro: rule.is_license_intro(),
                    language: rule.language.clone(),
                    rule_url: (!is_synthetic).then(|| rule.rule_url()).flatten(),
                    is_required_phrase: rule.is_required_phrase,
                    skip_for_required_phrase_generation: metadata
                        .map(|metadata| metadata.skip_for_required_phrase_generation)
                        .unwrap_or(false),
                    replaced_by: metadata
                        .map(|metadata| metadata.replaced_by.clone())
                        .unwrap_or_default(),
                    is_continuous: rule.is_continuous,
                    is_synthetic,
                    is_from_license: rule.is_from_license,
                    length: rule.tokens.len(),
                    relevance: Some(rule.relevance),
                    minimum_coverage: rule.minimum_coverage,
                    referenced_filenames: rule.referenced_filenames.clone().unwrap_or_default(),
                    notes: rule.notes.clone(),
                    ignorable_copyrights: rule.ignorable_copyrights.clone().unwrap_or_default(),
                    ignorable_holders: rule.ignorable_holders.clone().unwrap_or_default(),
                    ignorable_authors: rule.ignorable_authors.clone().unwrap_or_default(),
                    ignorable_urls: rule.ignorable_urls.clone().unwrap_or_default(),
                    ignorable_emails: rule.ignorable_emails.clone().unwrap_or_default(),
                    text: Some(rule.text.clone()),
                }
            })
        })
        .collect();

    (license_references, license_rule_references)
}

fn format_license_reference_url(template: &str, license_key: &str) -> String {
    template.replacen("{}", license_key, 1)
}

fn collect_license_keys_from_package_data(
    package_data: &PackageData,
    license_keys: &mut BTreeSet<String>,
) {
    collect_license_keys_from_expression(
        package_data.declared_license_expression.as_deref(),
        license_keys,
    );
    collect_license_keys_from_expression(
        package_data.other_license_expression.as_deref(),
        license_keys,
    );
    collect_license_keys_from_detections(&package_data.license_detections, license_keys);
    collect_license_keys_from_detections(&package_data.other_license_detections, license_keys);
}

fn collect_license_keys_from_detections(
    detections: &[LicenseDetection],
    license_keys: &mut BTreeSet<String>,
) {
    for detection in detections {
        collect_license_keys_from_expression(Some(&detection.license_expression), license_keys);
    }
}

fn collect_license_keys_from_expression(
    expression: Option<&str>,
    license_keys: &mut BTreeSet<String>,
) {
    let Some(expression) = expression else {
        return;
    };

    if let Ok(parsed) = parse_expression(expression) {
        for key in parsed.license_keys() {
            license_keys.insert(key);
        }
    }
}

fn collect_rule_identifiers_from_detections(
    detections: &[LicenseDetection],
    rule_identifiers: &mut BTreeSet<String>,
) {
    for detection in detections {
        collect_rule_identifiers_from_matches(&detection.matches, rule_identifiers);
    }
}

fn collect_rule_identifiers_from_matches(
    matches: &[Match],
    rule_identifiers: &mut BTreeSet<String>,
) {
    for license_match in matches {
        if let Some(rule_identifier) = license_match.rule_identifier.as_ref() {
            rule_identifiers.insert(rule_identifier.clone());
        }
    }
}

fn is_synthetic_rule(rule: &Rule) -> bool {
    rule.identifier.starts_with("spdx-license-identifier-")
        || rule.identifier.starts_with("spdx_license_id_")
}
