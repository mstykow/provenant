//! Buck BUILD and METADATA.bzl parsers
//!
//! Extracts package metadata from Buck build system files using Starlark (Python-like) syntax.
//!
//! ## Features
//! - **BuckBuildParser**: Parses BUCK files with multiple package support
//! - **BuckMetadataBzlParser**: Parses METADATA.bzl dictionary assignments with package_url support
//!
//! ## Usage
//! - `BuckBuildParser::extract_packages()` - Returns ALL packages from BUCK file
//! - `BuckMetadataBzlParser::extract_first_package()` - Returns single package from METADATA.bzl
//!
//! ## Reference
//! Python implementation: `reference/scancode-toolkit/src/packagedcode/build.py`
//! - BuckPackageHandler (lines 310-325)
//! - BuckMetadataBzlHandler (lines 328-432)

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use starlark_syntax::syntax::ast;
use starlark_syntax::syntax::module::AstModuleFields;
use starlark_syntax::syntax::{AstModule, Dialect};

use crate::models::{DatasourceId, PackageData, PackageType, Party, Sha1Digest};

use super::PackageParser;

type StarlarkCallArgs = ast::CallArgsP<ast::AstNoPayload>;

struct StarlarkCall<'a> {
    func: &'a ast::AstExpr,
    args: &'a StarlarkCallArgs,
}

/// Parser for Buck BUCK files (build rules)
pub struct BuckBuildParser;

impl PackageParser for BuckBuildParser {
    const PACKAGE_TYPE: PackageType = PackageType::Buck;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "BUCK")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match parse_buck_build(path) {
            Ok(packages) if !packages.is_empty() => packages,
            Ok(_) => vec![fallback_package_data(path)],
            Err(e) => {
                warn!("Failed to parse Buck BUCK file {:?}: {}", path, e);
                vec![fallback_package_data(path)]
            }
        }
    }
}

/// Parser for Buck METADATA.bzl files (metadata dictionaries)
pub struct BuckMetadataBzlParser;

impl PackageParser for BuckMetadataBzlParser {
    const PACKAGE_TYPE: PackageType = PackageType::Buck;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "METADATA.bzl")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        vec![match parse_metadata_bzl(path) {
            Ok(pkg) => pkg,
            Err(e) => {
                warn!("Failed to parse Buck METADATA.bzl {:?}: {}", path, e);
                PackageData {
                    package_type: Some(Self::PACKAGE_TYPE),
                    datasource_id: Some(DatasourceId::BuckMetadata),
                    ..Default::default()
                }
            }
        }]
    }
}

/// Parse a Buck BUCK file (same logic as Bazel BUILD)
fn parse_buck_build(path: &Path) -> Result<Vec<PackageData>, String> {
    let content = read_file_to_string(path, None).map_err(|e| e.to_string())?;
    let module = parse_starlark_module("<BUCK>", content)?;

    let mut packages = Vec::new();

    for statement in top_level_statements(&module)
        .iter()
        .take(MAX_ITERATION_COUNT)
    {
        if let Some(package_data) = extract_build_package_from_statement(statement) {
            packages.push(package_data);
        }
    }

    Ok(packages)
}

/// Parse a Buck METADATA.bzl file
fn parse_metadata_bzl(path: &Path) -> Result<PackageData, String> {
    let content = read_file_to_string(path, None).map_err(|e| e.to_string())?;
    let module = parse_starlark_module("<METADATA.bzl>", content)?;

    for statement in top_level_statements(&module)
        .iter()
        .take(MAX_ITERATION_COUNT)
    {
        if let Some(dict) = extract_metadata_assignment_dict(statement) {
            return Ok(extract_metadata_dict(dict));
        }
    }

    // No METADATA found
    Ok(PackageData {
        package_type: Some(BuckMetadataBzlParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::BuckMetadata),
        ..Default::default()
    })
}

fn parse_starlark_module(filename: &str, content: String) -> Result<AstModule, String> {
    let content = preprocess_starlark_content(&content);
    let dialect = Dialect {
        enable_top_level_stmt: true,
        ..Dialect::Standard
    };
    AstModule::parse(filename, content, &dialect).map_err(|error| error.to_string())
}

fn preprocess_starlark_content(content: &str) -> String {
    let mut normalized = String::with_capacity(content.len());
    let mut pending_oss_disable_indent: Option<String> = None;

    for raw_line in content.lines() {
        let trimmed_start = raw_line.trim_start();
        let indent_len = raw_line.len() - trimmed_start.len();
        let indent = &raw_line[..indent_len];

        if trimmed_start.starts_with('#') && trimmed_start.contains("@oss-disable") {
            pending_oss_disable_indent = Some(indent.to_string());
            continue;
        }

        if let Some(marker_index) = raw_line.find("# @oss-enable") {
            let code = raw_line[..marker_index].trim_end();
            if !code.is_empty() {
                if let Some(disabled_indent) = pending_oss_disable_indent.take() {
                    normalized.push_str(&disabled_indent);
                    normalized.push_str(code.trim_start());
                } else {
                    normalized.push_str(code);
                }
                normalized.push('\n');
            }
            continue;
        }

        pending_oss_disable_indent = None;
        normalized.push_str(raw_line);
        normalized.push('\n');
    }

    if !content.ends_with('\n') && normalized.ends_with('\n') {
        normalized.pop();
    }

    normalized
}

fn top_level_statements(module: &AstModule) -> &[ast::AstStmt] {
    match &module.statement().node {
        ast::StmtP::Statements(statements) => statements,
        _ => std::slice::from_ref(module.statement()),
    }
}

fn extract_metadata_assignment_dict(
    statement: &ast::AstStmt,
) -> Option<&[(ast::AstExpr, ast::AstExpr)]> {
    let ast::StmtP::Assign(assign) = &statement.node else {
        return None;
    };
    let ast::AssignTargetP::Identifier(target) = &assign.lhs.node else {
        return None;
    };
    if target.node.ident != "METADATA" {
        return None;
    }
    match &assign.rhs.node {
        ast::ExprP::Dict(items) => Some(items.as_slice()),
        _ => None,
    }
}

/// Extract metadata from a dictionary AST node
fn extract_metadata_dict(dict: &[(ast::AstExpr, ast::AstExpr)]) -> PackageData {
    let mut fields: HashMap<String, MetadataValue> = HashMap::new();

    for (key, value) in dict.iter().take(MAX_ITERATION_COUNT) {
        let Some(key_name) = expr_as_string(key) else {
            continue;
        };
        let Some(metadata_value) = metadata_value_from_expr(value) else {
            continue;
        };

        fields.insert(key_name, metadata_value);
    }

    build_package_from_metadata(fields)
}

fn get_metadata_string(fields: &HashMap<String, MetadataValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match fields.get(*key) {
        Some(MetadataValue::String(value)) => Some(value.clone()),
        _ => None,
    })
}

fn get_metadata_list(
    fields: &HashMap<String, MetadataValue>,
    keys: &[&str],
) -> Option<Vec<String>> {
    keys.iter().find_map(|key| match fields.get(*key) {
        Some(MetadataValue::List(values)) => Some(values.clone()),
        _ => None,
    })
}

/// Metadata value types
enum MetadataValue {
    String(String),
    List(Vec<String>),
}

fn split_buck_license_values(values: &[String]) -> (Vec<String>, Vec<String>) {
    let mut statements = Vec::new();
    let mut references = Vec::new();

    for value in values {
        if is_probable_local_license_reference(value) {
            references.push(value.clone());
        } else {
            statements.push(value.clone());
        }
    }

    (statements, references)
}

fn is_probable_local_license_reference(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    lower.contains('/')
        || lower.contains('\\')
        || lower.starts_with("license")
        || lower.starts_with("licence")
        || lower.starts_with("copying")
        || lower.starts_with("notice")
        || lower.starts_with("copyright")
        || lower.ends_with(".txt")
        || lower.ends_with(".md")
        || lower.ends_with(".rst")
        || lower.ends_with(".html")
}

fn insert_license_reference_extra_data(
    extra_data: &mut HashMap<String, serde_json::Value>,
    references: &[String],
) {
    match references {
        [] => {}
        [reference] => {
            extra_data.insert(
                "license_file".to_string(),
                serde_json::Value::String(reference.clone()),
            );
        }
        _ => {
            extra_data.insert(
                "license_files".to_string(),
                serde_json::Value::Array(
                    references
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
    }
}

/// Build PackageData from extracted metadata fields
fn build_package_from_metadata(fields: HashMap<String, MetadataValue>) -> PackageData {
    let mut pkg = PackageData {
        package_type: Some(BuckMetadataBzlParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::BuckMetadata),
        ..Default::default()
    };
    let mut license_references = Vec::new();

    // Extract name
    if let Some(name) = get_metadata_string(&fields, &["name"]) {
        pkg.name = Some(truncate_field(name));
    }

    // Extract version
    if let Some(version) = get_metadata_string(&fields, &["version"]) {
        pkg.version = Some(truncate_field(version));
    }

    // Extract namespace from explicit metadata when present.
    if let Some(namespace) = get_metadata_string(&fields, &["namespace"]) {
        pkg.namespace = Some(truncate_field(namespace));
    }

    // Extract package type from canonical or legacy ecosystem fields.
    // Intentionally ignore `upstream_type`: it does not describe the purl package type.
    if let Some(ecosystem) = get_metadata_string(&fields, &["ecosystem", "type", "package_type"])
        && let Ok(package_type) = ecosystem.parse::<PackageType>()
    {
        pkg.package_type = Some(package_type);
    }

    // Extract licenses (licenses or license_expression)
    if let Some(licenses) = get_metadata_list(&fields, &["licenses"]) {
        let (license_statements, references) = split_buck_license_values(&licenses);
        license_references = references;
        let extracted_license_statement = if !license_statements.is_empty() {
            Some(license_statements.join(", "))
        } else if !license_references.is_empty() {
            Some(license_references.join(", "))
        } else {
            None
        };
        pkg.extracted_license_statement = extracted_license_statement.map(truncate_field);
    } else if let Some(license_expression) = get_metadata_string(&fields, &["license_expression"]) {
        pkg.extracted_license_statement = Some(truncate_field(license_expression));
    }

    if let Some(copyright) = get_metadata_list(&fields, &["copyrights"]) {
        if !copyright.is_empty() {
            pkg.copyright = Some(truncate_field(copyright.join("\n")));
        }
    } else if let Some(copyright) = get_metadata_string(&fields, &["copyright"]) {
        pkg.copyright = Some(truncate_field(copyright));
    }

    // Extract homepage (upstream_address, upstream_url, or homepage_url)
    if let Some(homepage_url) = get_metadata_string(
        &fields,
        &["upstream_address", "upstream_url", "homepage_url"],
    ) {
        pkg.homepage_url = Some(truncate_field(homepage_url));
    }

    // Extract download_url
    if let Some(download_url) = get_metadata_string(&fields, &["download_url"]) {
        pkg.download_url = Some(truncate_field(download_url));
    }

    // Extract vcs_url
    if let Some(vcs_url) = get_metadata_string(&fields, &["vcs_url"]) {
        pkg.vcs_url = Some(truncate_field(vcs_url));
    }

    // Extract sha1 (download_archive_sha1)
    if let Some(sha1) = get_metadata_string(&fields, &["download_archive_sha1"]) {
        pkg.sha1 = Sha1Digest::from_hex(&sha1).ok();
    }

    // Extract maintainers
    if let Some(maintainers) = get_metadata_list(&fields, &["maintainers"]) {
        pkg.parties.extend(maintainers.iter().map(|name| Party {
            r#type: Some("organization".to_string()),
            name: Some(name.clone()),
            role: Some("maintainer".to_string()),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        }));
    }

    if let Some(vendor) = get_metadata_string(&fields, &["vendor", "publisher"]) {
        pkg.parties.push(Party {
            r#type: None,
            name: Some(vendor),
            role: Some("publisher".to_string()),
            email: None,
            url: None,
            organization: None,
            organization_url: None,
            timezone: None,
        });
    }

    // Extract extra_data fields
    let mut extra_data = HashMap::new();
    if let Some(vcs_commit_hash) = get_metadata_string(&fields, &["vcs_commit_hash"]) {
        extra_data.insert(
            "vcs_commit_hash".to_string(),
            serde_json::Value::String(vcs_commit_hash),
        );
    }
    if let Some(upstream_hash) =
        get_metadata_string(&fields, &["upstream_hash", "upstream_commit_hash"])
    {
        extra_data.insert(
            "upstream_hash".to_string(),
            serde_json::Value::String(upstream_hash),
        );
    }
    if let Some(upstream_branch) = get_metadata_string(&fields, &["upstream_branch"]) {
        extra_data.insert(
            "upstream_branch".to_string(),
            serde_json::Value::String(upstream_branch),
        );
    }
    insert_license_reference_extra_data(&mut extra_data, &license_references);
    if !extra_data.is_empty() {
        pkg.extra_data = Some(extra_data);
    }

    // Parse package_url if present and update package fields
    if let Some(purl_str) = get_metadata_string(&fields, &["package_url"])
        && let Ok(purl) = purl_str.parse::<PackageUrl>()
    {
        pkg.purl = Some(truncate_field(purl.to_string()));

        if let Ok(package_type) = purl.ty().parse::<PackageType>() {
            pkg.package_type = Some(package_type);
        }
        if let Some(ns) = purl.namespace() {
            pkg.namespace = Some(truncate_field(ns.to_string()));
        }
        pkg.name = Some(truncate_field(purl.name().to_string()));
        if let Some(ver) = purl.version() {
            pkg.version = Some(truncate_field(ver.to_string()));
        }
        // Qualifiers
        if !purl.qualifiers().is_empty() {
            let quals: HashMap<String, String> = purl
                .qualifiers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            pkg.qualifiers = Some(quals);
        }
        // Subpath
        if let Some(sp) = purl.subpath() {
            pkg.subpath = Some(sp.to_string());
        }
    }

    pkg
}

fn metadata_value_from_expr(expr: &ast::AstExpr) -> Option<MetadataValue> {
    if let Some(string) = expr_as_string(expr) {
        return Some(MetadataValue::String(string));
    }

    let items = match &expr.node {
        ast::ExprP::List(items) | ast::ExprP::Tuple(items) => items,
        _ => return None,
    };
    let values: Vec<_> = items
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(expr_as_string)
        .collect();
    (!values.is_empty()).then_some(MetadataValue::List(values))
}

/// Extract package data from a single AST statement (for BUCK files)
fn extract_build_package_from_statement(statement: &ast::AstStmt) -> Option<PackageData> {
    let call = extract_call(statement)?;
    let rule_name = match &call.func.node {
        ast::ExprP::Identifier(identifier) => identifier.node.ident.as_str(),
        _ => return None,
    };

    if !check_rule_name_ending(rule_name) {
        return None;
    }

    let name = extract_named_kwarg_string(&call, "name");
    let licenses = extract_named_kwarg_string_list(&call, "licenses");

    let package_name = name?;
    let (license_statements, license_references) = licenses
        .as_deref()
        .map(split_buck_license_values)
        .unwrap_or_default();
    let extracted_license_statement = if !license_statements.is_empty() {
        Some(truncate_field(license_statements.join(", ")))
    } else if !license_references.is_empty() {
        Some(truncate_field(license_references.join(", ")))
    } else {
        None
    };
    let mut extra_data = HashMap::new();
    insert_license_reference_extra_data(&mut extra_data, &license_references);

    Some(PackageData {
        package_type: Some(BuckBuildParser::PACKAGE_TYPE),
        name: Some(truncate_field(package_name)),
        extracted_license_statement,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
        datasource_id: Some(DatasourceId::BuckFile),
        ..Default::default()
    })
}

fn extract_call(statement: &ast::AstStmt) -> Option<StarlarkCall<'_>> {
    match &statement.node {
        ast::StmtP::Expression(expr) => extract_call_expr(expr),
        ast::StmtP::Assign(assign) => extract_call_expr(&assign.rhs),
        _ => None,
    }
}

fn extract_call_expr(expr: &ast::AstExpr) -> Option<StarlarkCall<'_>> {
    match &expr.node {
        ast::ExprP::Call(func, args) => Some(StarlarkCall { func, args }),
        _ => None,
    }
}

fn extract_named_kwarg<'a>(call: &'a StarlarkCall<'_>, key: &str) -> Option<&'a ast::AstExpr> {
    call.args
        .args
        .iter()
        .find_map(|argument| match &argument.node {
            ast::ArgumentP::Named(name, value) if name.node == key => Some(value),
            _ => None,
        })
}

fn extract_named_kwarg_string(call: &StarlarkCall<'_>, key: &str) -> Option<String> {
    extract_named_kwarg(call, key).and_then(expr_as_string)
}

fn extract_named_kwarg_string_list(call: &StarlarkCall<'_>, key: &str) -> Option<Vec<String>> {
    let expr = extract_named_kwarg(call, key)?;
    let items = match &expr.node {
        ast::ExprP::List(items) | ast::ExprP::Tuple(items) => items,
        _ => return None,
    };
    let values: Vec<_> = items
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(expr_as_string)
        .collect();
    (!values.is_empty()).then_some(values)
}

fn expr_as_string(expr: &ast::AstExpr) -> Option<String> {
    match &expr.node {
        ast::ExprP::Literal(ast::AstLiteral::String(value)) => Some(value.node.clone()),
        _ => None,
    }
}

/// Check if rule name ends with "binary" or "library"
fn check_rule_name_ending(rule_name: &str) -> bool {
    rule_name.ends_with("binary") || rule_name.ends_with("library")
}

/// Create fallback package data using parent directory name
fn fallback_package_data(path: &Path) -> PackageData {
    let name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    PackageData {
        package_type: Some(BuckBuildParser::PACKAGE_TYPE),
        name,
        datasource_id: Some(DatasourceId::BuckFile),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_buck_build_is_match() {
        assert!(BuckBuildParser::is_match(&PathBuf::from("BUCK")));
        assert!(BuckBuildParser::is_match(&PathBuf::from("path/to/BUCK")));
        assert!(!BuckBuildParser::is_match(&PathBuf::from("BUILD")));
        assert!(!BuckBuildParser::is_match(&PathBuf::from("buck")));
    }

    #[test]
    fn test_metadata_bzl_is_match() {
        assert!(BuckMetadataBzlParser::is_match(&PathBuf::from(
            "METADATA.bzl"
        )));
        assert!(BuckMetadataBzlParser::is_match(&PathBuf::from(
            "path/to/METADATA.bzl"
        )));
        assert!(!BuckMetadataBzlParser::is_match(&PathBuf::from(
            "metadata.bzl"
        )));
        assert!(!BuckMetadataBzlParser::is_match(&PathBuf::from("METADATA")));
    }

    #[test]
    fn test_check_rule_name_ending() {
        assert!(check_rule_name_ending("android_binary"));
        assert!(check_rule_name_ending("android_library"));
        assert!(check_rule_name_ending("java_binary"));
        assert!(!check_rule_name_ending("filegroup"));
    }

    #[test]
    fn test_preprocess_starlark_content_handles_oss_guarded_alternatives() {
        let content = r#"# @oss-disable[end= ]: load("@fbsource//tools/build_defs:rust_unittest.bzl", "rust_unittest")
prelude = native

# @oss-disable: rust_unittest(
    rust_test( # @oss-enable
        name = "test",
    )

platform_utils = None # @oss-enable
"#;

        let normalized = preprocess_starlark_content(content);

        assert!(!normalized.contains("@oss-disable"));
        assert!(!normalized.contains("@oss-enable"));
        assert!(normalized.contains("rust_test("));
        assert!(normalized.contains("platform_utils = None"));
        assert!(!normalized.contains("    rust_test("));
    }

    #[test]
    fn test_parse_buck_build_with_oss_guarded_rule() {
        let content = r#"# @oss-disable[end= ]: load("@fbsource//tools/build_defs:rust_library.bzl", "rust_library")
# @oss-disable[end= ]: load("@fbsource//tools/build_defs:rust_unittest.bzl", "rust_unittest")

oncall("build_infra")

rust_library(
    name = "library",
    srcs = ["src/lib.rs"],
)

# @oss-disable: rust_unittest(
    rust_test( # @oss-enable
    name = "test",
    srcs = ["tests/test.rs"],
)
"#;

        let temp_dir = tempfile::tempdir().unwrap();
        let buck_path = temp_dir.path().join("BUCK");
        std::fs::write(&buck_path, content).unwrap();

        let packages = parse_buck_build(&buck_path).expect("BUCK file should parse");

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package_type, Some(PackageType::Buck));
        assert_eq!(packages[0].name.as_deref(), Some("library"));
    }
}

crate::register_parser!(
    "Buck build file and METADATA.bzl",
    &["**/BUCK", "**/METADATA.bzl"],
    "buck",
    "",
    Some("https://buck.build/"),
);
