use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::cache::write_bytes_atomically;
use crate::license_detection::embedded::index::load_loader_snapshot_from_bytes;
use crate::license_detection::embedded::schema::EmbeddedArtifactMetadata;
use crate::license_detection::license_cache::compute_rules_fingerprint;
use crate::license_detection::models::{LoadedLicense, LoadedRule, RuleKind};
use crate::license_detection::rules::{parse_license_to_loaded, parse_rule_to_loaded};
use crate::models::Sha256Digest;
use crate::version::BUILD_VERSION;

pub const LICENSE_DATASET_RULES_DIR: &str = "rules";
pub const LICENSE_DATASET_LICENSES_DIR: &str = "licenses";
pub const LICENSE_DATASET_MANIFEST_FILE: &str = "manifest.json";
pub const LICENSE_DATASET_README_FILE: &str = "README.md";
pub const CUSTOM_LICENSE_DATASET_SOURCE: &str = "custom-license-dataset";
const LICENSE_DATASET_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseDatasetManifest {
    pub schema_version: u32,
    pub spdx_license_list_version: String,
    pub dataset_fingerprint: String,
    pub exported_from_source: String,
    pub exported_by_version: String,
}

#[derive(Debug, Clone)]
pub struct LoadedLicenseDataset {
    pub manifest: LicenseDatasetManifest,
    pub rules: Vec<LoadedRule>,
    pub licenses: Vec<LoadedLicense>,
}

pub fn export_embedded_license_dataset(target_root: &Path) -> Result<LicenseDatasetManifest> {
    let artifact_bytes = include_bytes!("../../resources/license_detection/license_index.zst");
    let snapshot = load_loader_snapshot_from_bytes(artifact_bytes)
        .map_err(|error| anyhow!("Failed to load embedded license dataset: {}", error))?;

    export_license_dataset_to_root(
        target_root,
        &snapshot.rules,
        &snapshot.licenses,
        &snapshot.metadata,
    )
}

pub fn export_license_dataset_to_root(
    target_root: &Path,
    rules: &[LoadedRule],
    licenses: &[LoadedLicense],
    metadata: &EmbeddedArtifactMetadata,
) -> Result<LicenseDatasetManifest> {
    ensure_export_target_is_empty(target_root)?;

    let manifest = LicenseDatasetManifest {
        schema_version: LICENSE_DATASET_SCHEMA_VERSION,
        spdx_license_list_version: metadata.spdx_license_list_version.clone(),
        dataset_fingerprint: compute_dataset_fingerprint_string(rules, licenses)?,
        exported_from_source: metadata.license_index_provenance.source.clone(),
        exported_by_version: BUILD_VERSION.to_string(),
    };

    write_dataset_manifest(target_root, &manifest)?;
    write_dataset_readme(target_root, &manifest)?;
    write_rule_files(target_root, rules)?;
    write_license_files(target_root, licenses)?;

    Ok(manifest)
}

pub fn load_license_dataset_from_root(root: &Path) -> Result<LoadedLicenseDataset> {
    let rules_dir = root.join(LICENSE_DATASET_RULES_DIR);
    let licenses_dir = root.join(LICENSE_DATASET_LICENSES_DIR);

    if !root.is_dir() {
        return Err(anyhow!(
            "License dataset root does not exist or is not a directory: {}",
            root.display()
        ));
    }
    if !rules_dir.is_dir() {
        return Err(anyhow!(
            "License dataset is missing required rules/ directory: {}",
            rules_dir.display()
        ));
    }
    if !licenses_dir.is_dir() {
        return Err(anyhow!(
            "License dataset is missing required licenses/ directory: {}",
            licenses_dir.display()
        ));
    }

    let manifest_path = root.join(LICENSE_DATASET_MANIFEST_FILE);
    let manifest_text = std::fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "License dataset is missing required manifest.json at {}",
            manifest_path.display()
        )
    })?;
    let manifest: LicenseDatasetManifest =
        serde_json::from_str(&manifest_text).with_context(|| {
            format!(
                "Failed to parse license dataset manifest at {}",
                manifest_path.display()
            )
        })?;

    if manifest.schema_version != LICENSE_DATASET_SCHEMA_VERSION {
        return Err(anyhow!(
            "Unsupported license dataset schema version {} in {} (expected {})",
            manifest.schema_version,
            manifest_path.display(),
            LICENSE_DATASET_SCHEMA_VERSION
        ));
    }

    let rules = load_strict_loaded_rules_from_directory(&rules_dir)?;
    let licenses = load_strict_loaded_licenses_from_directory(&licenses_dir)?;

    Ok(LoadedLicenseDataset {
        manifest,
        rules,
        licenses,
    })
}

pub fn compute_dataset_fingerprint_string(
    rules: &[LoadedRule],
    licenses: &[LoadedLicense],
) -> Result<String> {
    Ok(Sha256Digest::from_bytes(compute_rules_fingerprint(rules, licenses)?).to_string())
}

fn ensure_export_target_is_empty(target_root: &Path) -> Result<()> {
    if target_root.exists() {
        let mut entries = std::fs::read_dir(target_root)
            .with_context(|| format!("Failed to read export target {}", target_root.display()))?;
        if entries.next().is_some() {
            return Err(anyhow!(
                "Refusing to export into non-empty directory {}",
                target_root.display()
            ));
        }
    } else {
        std::fs::create_dir_all(target_root)
            .with_context(|| format!("Failed to create export target {}", target_root.display()))?;
    }

    Ok(())
}

fn write_dataset_manifest(root: &Path, manifest: &LicenseDatasetManifest) -> Result<()> {
    let payload = serde_json::to_vec_pretty(manifest).context("Serialize dataset manifest")?;
    write_bytes_atomically(&root.join(LICENSE_DATASET_MANIFEST_FILE), &payload)
        .context("Write dataset manifest")?;
    Ok(())
}

fn write_dataset_readme(root: &Path, manifest: &LicenseDatasetManifest) -> Result<()> {
    let text = format!(
        "# Exported Provenant license dataset\n\nThis directory contains the effective `.RULE` and `.LICENSE` files used by Provenant.\n\n- Reuse it with `provenant --license-dataset-path <DIR> --license ...`\n- Edit files under `rules/` and `licenses/` to customize scan behavior\n- `manifest.json` records the exported dataset fingerprint and SPDX license list version\n- The fingerprint in `manifest.json` is informational; if you edit files, Provenant computes the active dataset fingerprint from current file contents\n\nExport metadata:\n\n- schema_version: {}\n- spdx_license_list_version: {}\n- dataset_fingerprint: {}\n- exported_from_source: {}\n- exported_by_version: {}\n",
        manifest.schema_version,
        manifest.spdx_license_list_version,
        manifest.dataset_fingerprint,
        manifest.exported_from_source,
        manifest.exported_by_version,
    );
    write_bytes_atomically(&root.join(LICENSE_DATASET_README_FILE), text.as_bytes())
        .context("Write dataset README")?;
    Ok(())
}

fn write_rule_files(root: &Path, rules: &[LoadedRule]) -> Result<()> {
    let mut sorted = rules.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|rule| &rule.identifier);

    for rule in sorted {
        validate_dataset_filename_component(&rule.identifier, "rule identifier")?;
        let rendered = render_rule(rule)?;
        let output_path = root.join(LICENSE_DATASET_RULES_DIR).join(&rule.identifier);
        write_bytes_atomically(&output_path, rendered.as_bytes())
            .with_context(|| format!("Write rule dataset file {}", output_path.display()))?;
    }

    Ok(())
}

fn write_license_files(root: &Path, licenses: &[LoadedLicense]) -> Result<()> {
    let mut sorted = licenses.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|license| &license.key);

    for license in sorted {
        validate_dataset_filename_component(&license.key, "license key")?;
        let rendered = render_license(license)?;
        let output_path = root
            .join(LICENSE_DATASET_LICENSES_DIR)
            .join(format!("{}.LICENSE", license.key));
        write_bytes_atomically(&output_path, rendered.as_bytes())
            .with_context(|| format!("Write license dataset file {}", output_path.display()))?;
    }

    Ok(())
}

fn load_strict_loaded_rules_from_directory(dir: &Path) -> Result<Vec<LoadedRule>> {
    let mut rules = Vec::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read rules directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry
            .with_context(|| format!("Failed to read directory entry in: {}", dir.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("RULE") {
            rules.push(parse_rule_to_loaded(&path).with_context(|| {
                format!("Failed to parse dataset rule file {}", path.display())
            })?);
        }
    }

    Ok(rules)
}

fn load_strict_loaded_licenses_from_directory(dir: &Path) -> Result<Vec<LoadedLicense>> {
    let mut licenses = Vec::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read licenses directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry
            .with_context(|| format!("Failed to read directory entry in: {}", dir.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("LICENSE") {
            licenses.push(parse_license_to_loaded(&path).with_context(|| {
                format!("Failed to parse dataset license file {}", path.display())
            })?);
        }
    }

    Ok(licenses)
}

fn validate_dataset_filename_component(value: &str, kind: &str) -> Result<()> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || Path::new(value).components().count() != 1
    {
        return Err(anyhow!(
            "Invalid {} for exported license dataset: {}",
            kind,
            value
        ));
    }

    Ok(())
}

fn render_rule(rule: &LoadedRule) -> Result<String> {
    let mut rendered = String::from("---\n");
    push_yaml_string(
        &mut rendered,
        "license_expression",
        Some(&rule.license_expression),
    )?;
    push_rule_kind(&mut rendered, rule.rule_kind);
    push_yaml_bool(&mut rendered, "is_false_positive", rule.is_false_positive);
    push_yaml_bool(&mut rendered, "is_required_phrase", rule.is_required_phrase);
    push_yaml_bool(
        &mut rendered,
        "skip_for_required_phrase_generation",
        rule.skip_for_required_phrase_generation,
    );
    push_yaml_u8(&mut rendered, "relevance", rule.relevance);
    if rule.has_stored_minimum_coverage {
        push_yaml_u8(&mut rendered, "minimum_coverage", rule.minimum_coverage);
    }
    push_yaml_bool(&mut rendered, "is_continuous", rule.is_continuous);
    push_yaml_bool(&mut rendered, "is_deprecated", rule.is_deprecated);
    push_yaml_list(
        &mut rendered,
        "referenced_filenames",
        rule.referenced_filenames.as_deref(),
    )?;
    push_yaml_list(&mut rendered, "replaced_by", Some(&rule.replaced_by))?;
    push_yaml_list(
        &mut rendered,
        "ignorable_urls",
        rule.ignorable_urls.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_emails",
        rule.ignorable_emails.as_deref(),
    )?;
    push_yaml_string(&mut rendered, "notes", rule.notes.as_deref())?;
    push_yaml_list(
        &mut rendered,
        "ignorable_copyrights",
        rule.ignorable_copyrights.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_holders",
        rule.ignorable_holders.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_authors",
        rule.ignorable_authors.as_deref(),
    )?;
    push_yaml_string(&mut rendered, "language", rule.language.as_deref())?;
    rendered.push_str("---\n\n");
    rendered.push_str(&rule.text);
    rendered.push('\n');
    Ok(rendered)
}

fn render_license(license: &LoadedLicense) -> Result<String> {
    let mut rendered = String::from("---\n");
    push_yaml_string(&mut rendered, "key", Some(&license.key))?;
    push_yaml_string(&mut rendered, "short_name", license.short_name.as_deref())?;
    push_yaml_string(&mut rendered, "name", Some(&license.name))?;
    push_yaml_string(
        &mut rendered,
        "spdx_license_key",
        license.spdx_license_key.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "other_spdx_license_keys",
        Some(&license.other_spdx_license_keys),
    )?;
    push_yaml_string(&mut rendered, "category", license.category.as_deref())?;
    push_yaml_string(&mut rendered, "owner", license.owner.as_deref())?;
    push_yaml_string(
        &mut rendered,
        "homepage_url",
        license.homepage_url.as_deref(),
    )?;
    push_yaml_string(
        &mut rendered,
        "osi_license_key",
        license.osi_license_key.as_deref(),
    )?;
    push_yaml_list(&mut rendered, "text_urls", Some(&license.text_urls))?;
    push_yaml_string(&mut rendered, "osi_url", license.osi_url.as_deref())?;
    push_yaml_string(&mut rendered, "faq_url", license.faq_url.as_deref())?;
    push_yaml_list(&mut rendered, "other_urls", Some(&license.other_urls))?;
    push_yaml_string(&mut rendered, "notes", license.notes.as_deref())?;
    push_yaml_bool(&mut rendered, "is_deprecated", license.is_deprecated);
    push_yaml_bool(&mut rendered, "is_exception", license.is_exception);
    push_yaml_bool(&mut rendered, "is_unknown", license.is_unknown);
    push_yaml_bool(&mut rendered, "is_generic", license.is_generic);
    push_yaml_list(&mut rendered, "replaced_by", Some(&license.replaced_by))?;
    push_yaml_u8(&mut rendered, "minimum_coverage", license.minimum_coverage);
    push_yaml_string(
        &mut rendered,
        "standard_notice",
        license.standard_notice.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_copyrights",
        license.ignorable_copyrights.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_holders",
        license.ignorable_holders.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_authors",
        license.ignorable_authors.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_urls",
        license.ignorable_urls.as_deref(),
    )?;
    push_yaml_list(
        &mut rendered,
        "ignorable_emails",
        license.ignorable_emails.as_deref(),
    )?;
    rendered.push_str("---\n\n");
    rendered.push_str(&license.text);
    rendered.push('\n');
    Ok(rendered)
}

fn push_rule_kind(rendered: &mut String, rule_kind: RuleKind) {
    let key = match rule_kind {
        RuleKind::None => return,
        RuleKind::Text => "is_license_text",
        RuleKind::Notice => "is_license_notice",
        RuleKind::Reference => "is_license_reference",
        RuleKind::Tag => "is_license_tag",
        RuleKind::Intro => "is_license_intro",
        RuleKind::Clue => "is_license_clue",
    };
    let _ = writeln!(rendered, "{key}: true");
}

fn push_yaml_bool(rendered: &mut String, key: &str, value: bool) {
    if value {
        let _ = writeln!(rendered, "{key}: true");
    }
}

fn push_yaml_u8(rendered: &mut String, key: &str, value: Option<u8>) {
    if let Some(value) = value {
        let _ = writeln!(rendered, "{key}: {value}");
    }
}

fn push_yaml_string(rendered: &mut String, key: &str, value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let quoted = serde_json::to_string(value).context("serialize yaml string")?;
    let _ = writeln!(rendered, "{key}: {quoted}");
    Ok(())
}

fn push_yaml_list(rendered: &mut String, key: &str, values: Option<&[String]>) -> Result<()> {
    let Some(values) = values else {
        return Ok(());
    };
    if values.is_empty() {
        return Ok(());
    }

    let _ = writeln!(rendered, "{key}:");
    for value in values {
        let quoted = serde_json::to_string(value).context("serialize yaml list entry")?;
        let _ = writeln!(rendered, "  - {quoted}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::RuleKind;
    use crate::license_detection::rules::{parse_license_str_to_loaded, parse_rule_str_to_loaded};
    use tempfile::TempDir;

    fn create_loaded_rule() -> LoadedRule {
        LoadedRule {
            identifier: "example.RULE".to_string(),
            license_expression: "mit OR apache-2.0".to_string(),
            text: "Example rule text".to_string(),
            rule_kind: RuleKind::Notice,
            is_false_positive: false,
            is_required_phrase: true,
            skip_for_required_phrase_generation: true,
            relevance: Some(100),
            minimum_coverage: Some(75),
            has_stored_minimum_coverage: true,
            is_continuous: true,
            referenced_filenames: Some(vec!["LICENSE".to_string()]),
            ignorable_urls: Some(vec!["https://example.com".to_string()]),
            ignorable_emails: Some(vec!["legal@example.com".to_string()]),
            ignorable_copyrights: Some(vec!["Copyright Example".to_string()]),
            ignorable_holders: Some(vec!["Example Org".to_string()]),
            ignorable_authors: Some(vec!["Jane Doe".to_string()]),
            language: Some("en".to_string()),
            notes: Some("Example note".to_string()),
            is_deprecated: true,
            replaced_by: vec!["replacement.RULE".to_string()],
        }
    }

    fn create_loaded_license() -> LoadedLicense {
        LoadedLicense {
            key: "example-license".to_string(),
            short_name: Some("Example".to_string()),
            name: "Example License".to_string(),
            language: Some("en".to_string()),
            spdx_license_key: Some("MIT".to_string()),
            other_spdx_license_keys: vec!["Apache-2.0".to_string()],
            category: Some("Permissive".to_string()),
            owner: Some("Example Org".to_string()),
            homepage_url: Some("https://example.com".to_string()),
            text: "Example license text".to_string(),
            reference_urls: vec![
                "https://example.com/text".to_string(),
                "https://example.com/other".to_string(),
                "https://opensource.org/licenses/MIT".to_string(),
                "https://example.com/faq".to_string(),
                "https://example.com".to_string(),
            ],
            osi_license_key: Some("MIT".to_string()),
            text_urls: vec!["https://example.com/text".to_string()],
            osi_url: Some("https://opensource.org/licenses/MIT".to_string()),
            faq_url: Some("https://example.com/faq".to_string()),
            other_urls: vec!["https://example.com/other".to_string()],
            notes: Some("Example note".to_string()),
            is_deprecated: true,
            is_exception: true,
            is_unknown: true,
            is_generic: true,
            replaced_by: vec!["replacement".to_string()],
            minimum_coverage: Some(55),
            standard_notice: Some("Standard notice".to_string()),
            ignorable_copyrights: Some(vec!["Copyright Example".to_string()]),
            ignorable_holders: Some(vec!["Example Org".to_string()]),
            ignorable_authors: Some(vec!["Jane Doe".to_string()]),
            ignorable_urls: Some(vec!["https://example.com".to_string()]),
            ignorable_emails: Some(vec!["legal@example.com".to_string()]),
        }
    }

    #[test]
    fn render_rule_roundtrips_through_loader() {
        let rule = create_loaded_rule();
        let rendered = render_rule(&rule).expect("render rule");
        let reparsed = parse_rule_str_to_loaded(&rule.identifier, &rendered).expect("reparse rule");
        assert_eq!(reparsed, rule);
    }

    #[test]
    fn render_license_roundtrips_through_loader() {
        let license = create_loaded_license();
        let rendered = render_license(&license).expect("render license");
        let reparsed =
            parse_license_str_to_loaded("example-license.LICENSE", &rendered).expect("reparse");
        assert_eq!(reparsed, license);
    }

    #[test]
    fn load_license_dataset_requires_manifest_and_expected_dirs() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::create_dir_all(temp.path().join("rules")).expect("rules dir");
        std::fs::create_dir_all(temp.path().join("licenses")).expect("licenses dir");

        let error = load_license_dataset_from_root(temp.path()).expect_err("missing manifest");
        assert!(error.to_string().contains("manifest.json"));
    }

    #[test]
    fn load_license_dataset_fails_on_invalid_rule_file() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("rules")).expect("rules dir");
        std::fs::create_dir_all(root.join("licenses")).expect("licenses dir");
        std::fs::write(
            root.join("manifest.json"),
            serde_json::json!({
                "schema_version": 1,
                "spdx_license_list_version": "3.27",
                "dataset_fingerprint": "abc",
                "exported_from_source": "embedded-artifact",
                "exported_by_version": "test",
            })
            .to_string(),
        )
        .expect("manifest");
        std::fs::write(root.join("rules").join("broken.RULE"), "not-frontmatter")
            .expect("broken rule");
        std::fs::write(
            root.join("licenses").join("mit.LICENSE"),
            "---\nkey: \"mit\"\nname: \"MIT License\"\n---\n\nMIT text\n",
        )
        .expect("license");

        let error = load_license_dataset_from_root(root).expect_err("invalid rule should fail");
        assert!(
            error
                .to_string()
                .contains("Failed to parse dataset rule file")
        );
    }

    #[test]
    fn export_license_dataset_rejects_path_like_rule_identifier() {
        let manifest = EmbeddedArtifactMetadata {
            spdx_license_list_version: "3.27".to_string(),
            license_index_provenance: crate::models::LicenseIndexProvenance {
                source: "embedded-artifact".to_string(),
                dataset_fingerprint: "abc123".to_string(),
                ignored_rules: vec![],
                ignored_licenses: vec![],
                ignored_rules_due_to_licenses: vec![],
                added_rules: vec![],
                replaced_rules: vec![],
                added_licenses: vec![],
                replaced_licenses: vec![],
            },
        };
        let temp = TempDir::new().expect("temp dir");

        let error = export_license_dataset_to_root(
            temp.path(),
            &[LoadedRule {
                identifier: "nested/path.RULE".to_string(),
                ..create_loaded_rule()
            }],
            &[create_loaded_license()],
            &manifest,
        )
        .expect_err("path-like identifiers should be rejected");

        assert!(
            error
                .to_string()
                .contains("Invalid rule identifier for exported license dataset")
        );
    }
}
