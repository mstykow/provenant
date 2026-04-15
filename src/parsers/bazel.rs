//! Bazel BUILD file parser
//!
//! Extracts package metadata from Bazel BUILD files using Starlark (Python-like) syntax.
//!
//! ## Features
//! - Parses Starlark syntax using starlark_syntax
//! - Extracts build rules ending with "binary" or "library" (e.g., cc_binary, cc_library)
//! - Extracts name and licenses fields from rule arguments
//! - Falls back to parent directory name if no rules found
//! - **Supports multiple packages**: `extract_packages()` returns all rules (100% parity)
//!
//! ## Usage
//! - `extract_first_package()` - Returns first package (convenience method)
//! - `extract_packages()` - Returns ALL packages (recommended for BUILD files)
//!
//! ## Reference
//! Python implementation: `reference/scancode-toolkit/src/packagedcode/build.py` (BazelBuildHandler)

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::utils::{MAX_ITERATION_COUNT, RecursionGuard, truncate_field};
use packageurl::PackageUrl;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::path::Path;

use crate::parser_warn as warn;
use starlark_syntax::syntax::ast;
use starlark_syntax::syntax::module::AstModuleFields;
use starlark_syntax::syntax::{AstModule, Dialect};

use super::PackageParser;

type StarlarkCallArgs = ast::CallArgsP<ast::AstNoPayload>;
const SCANCODE_SIMPLE_TOP_LEVEL_KEY: &str = "scancode_simple_top_level";

struct StarlarkCall<'a> {
    func: &'a ast::AstExpr,
    args: &'a StarlarkCallArgs,
}

pub struct BazelBuildParser;

impl PackageParser for BazelBuildParser {
    const PACKAGE_TYPE: PackageType = PackageType::Bazel;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "BUILD")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match parse_bazel_build(path) {
            Ok(packages) if !packages.is_empty() => packages,
            Ok(_) => vec![fallback_package_data(path)],
            Err(e) => {
                warn!("Failed to parse Bazel BUILD file {:?}: {}", path, e);
                vec![fallback_package_data(path)]
            }
        }
    }
}

/// Parse a Bazel BUILD file and extract all package data
fn parse_bazel_build(path: &Path) -> Result<Vec<PackageData>, String> {
    let content =
        crate::parsers::utils::read_file_to_string(path, None).map_err(|e| e.to_string())?;
    let module = parse_starlark_module("<BUILD>", content)?;
    let scancode_simple_top_level = is_scancode_simple_top_level_module(&module);

    let mut packages = Vec::new();

    for statement in top_level_statements(&module)
        .iter()
        .take(MAX_ITERATION_COUNT)
    {
        if let Some(mut package_data) = extract_package_from_statement(statement) {
            set_scancode_simple_top_level(&mut package_data, scancode_simple_top_level);
            packages.push(package_data);
        }
    }

    Ok(packages)
}

/// Extract package data from a single AST statement
fn extract_package_from_statement(statement: &ast::AstStmt) -> Option<PackageData> {
    let call = extract_call(statement)?;
    let rule_name = extract_call_name(&call)?;

    if !check_rule_name_ending(rule_name) {
        return None;
    }

    let name = extract_string_kwarg(&call, "name")?;
    let licenses = extract_string_list_kwarg(&call, "licenses");
    let purl = build_bazel_purl(&name, None).map(truncate_field);

    Some(PackageData {
        package_type: Some(BazelBuildParser::PACKAGE_TYPE),
        name: Some(truncate_field(name)),
        extracted_license_statement: licenses.map(|licenses| truncate_field(licenses.join(", "))),
        datasource_id: Some(DatasourceId::BazelBuild),
        purl,
        ..Default::default()
    })
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
        .map(|s| truncate_field(s.to_string()));

    PackageData {
        package_type: Some(BazelBuildParser::PACKAGE_TYPE),
        purl: name
            .as_deref()
            .and_then(|name| build_bazel_purl(name, None))
            .map(truncate_field),
        name,
        datasource_id: Some(DatasourceId::BazelBuild),
        ..Default::default()
    }
}

fn set_scancode_simple_top_level(package_data: &mut PackageData, enabled: bool) {
    let extra_data = package_data.extra_data.get_or_insert_with(Default::default);
    extra_data.insert(
        SCANCODE_SIMPLE_TOP_LEVEL_KEY.to_string(),
        JsonValue::Bool(enabled),
    );
}

fn is_scancode_simple_top_level_module(module: &AstModule) -> bool {
    top_level_statements(module)
        .iter()
        .all(is_scancode_simple_top_level_statement)
}

fn is_scancode_simple_top_level_statement(statement: &ast::AstStmt) -> bool {
    match &statement.node {
        ast::StmtP::Expression(expr) => {
            matches!(&expr.node, ast::ExprP::Call(func, _) if matches!(&func.node, ast::ExprP::Identifier(_)))
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match() {
        assert!(BazelBuildParser::is_match(&PathBuf::from("BUILD")));
        assert!(BazelBuildParser::is_match(&PathBuf::from("path/to/BUILD")));
        assert!(!BazelBuildParser::is_match(&PathBuf::from("BUILD.bazel")));
        assert!(!BazelBuildParser::is_match(&PathBuf::from("build")));
        assert!(!BazelBuildParser::is_match(&PathBuf::from("BUCK")));
    }

    #[test]
    fn test_check_rule_name_ending() {
        assert!(check_rule_name_ending("cc_binary"));
        assert!(check_rule_name_ending("cc_library"));
        assert!(check_rule_name_ending("java_binary"));
        assert!(check_rule_name_ending("py_library"));
        assert!(!check_rule_name_ending("filegroup"));
        assert!(!check_rule_name_ending("load"));
        assert!(!check_rule_name_ending("cc_test"));
    }

    #[test]
    fn test_fallback_package_data() {
        let path = PathBuf::from("/path/to/myproject/BUILD");
        let pkg = fallback_package_data(&path);
        assert_eq!(pkg.package_type, Some(PackageType::Bazel));
        assert_eq!(pkg.name, Some("myproject".to_string()));
        assert_eq!(pkg.purl.as_deref(), Some("pkg:bazel/myproject"));
    }

    #[test]
    fn test_scancode_simple_top_level_allows_direct_calls() {
        let module = parse_starlark_module(
            "<BUILD>",
            "cc_library(name = \"demo\")\npy_binary(name = \"tool\")\n".to_string(),
        )
        .expect("parse BUILD");

        assert!(is_scancode_simple_top_level_module(&module));
    }

    #[test]
    fn test_scancode_simple_top_level_rejects_attribute_calls() {
        let module = parse_starlark_module(
            "<BUILD>",
            "selects.config_setting_group(name = \"demo\")\ncc_library(name = \"demo\")\n"
                .to_string(),
        )
        .expect("parse BUILD");

        assert!(!is_scancode_simple_top_level_module(&module));
    }

    #[test]
    fn test_scancode_simple_top_level_rejects_non_call_expressions() {
        let module =
            parse_starlark_module("<BUILD>", "[(cc_binary(name = \"demo\"),)]\n".to_string())
                .expect("parse BUILD");

        assert!(!is_scancode_simple_top_level_module(&module));
    }
}

crate::register_parser!(
    "Bazel BUILD file",
    &["**/BUILD"],
    "bazel",
    "",
    Some("https://bazel.build/"),
);

pub struct BazelModuleParser;

impl PackageParser for BazelModuleParser {
    const PACKAGE_TYPE: PackageType = PackageType::Bazel;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "MODULE.bazel")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        match parse_bazel_module(path) {
            Ok(package) => vec![package],
            Err(e) => {
                warn!("Failed to parse Bazel MODULE.bazel {:?}: {}", path, e);
                vec![default_bazel_module_package_data()]
            }
        }
    }
}

fn parse_bazel_module(path: &Path) -> Result<PackageData, String> {
    let content =
        crate::parsers::utils::read_file_to_string(path, None).map_err(|e| e.to_string())?;
    let module = parse_starlark_module("<MODULE.bazel>", content)?;

    let mut package = default_bazel_module_package_data();
    let mut extra_data = JsonMap::new();
    let mut dependencies = Vec::new();
    let mut overrides = Vec::new();

    for statement in top_level_statements(&module)
        .iter()
        .take(MAX_ITERATION_COUNT)
    {
        let Some(call) = extract_call(statement) else {
            continue;
        };

        let Some(function_name) = extract_call_name(&call) else {
            continue;
        };

        match function_name {
            "module" => {
                package.name = extract_string_kwarg(&call, "name").map(truncate_field);
                package.version = extract_string_kwarg(&call, "version").map(truncate_field);
                package.purl = package
                    .name
                    .as_deref()
                    .and_then(|name| build_bazel_purl(name, package.version.as_deref()))
                    .map(truncate_field);

                if let Some(repo_name) =
                    extract_string_kwarg(&call, "repo_name").map(truncate_field)
                {
                    extra_data.insert("repo_name".to_string(), JsonValue::String(repo_name));
                }
                if let Some(compatibility_level) = extract_int_kwarg(&call, "compatibility_level") {
                    extra_data.insert(
                        "compatibility_level".to_string(),
                        JsonValue::Number(compatibility_level.into()),
                    );
                }
                if let Some(bazel_compatibility) = extract_kwarg_json(&call, "bazel_compatibility")
                {
                    extra_data.insert("bazel_compatibility".to_string(), bazel_compatibility);
                }
            }
            "bazel_dep" => {
                if let Some(dep) = extract_bazel_dependency(&call) {
                    dependencies.push(dep);
                }
            }
            "archive_override"
            | "git_override"
            | "local_path_override"
            | "single_version_override"
            | "multiple_version_override" => {
                overrides.push(extract_override(function_name, &call));
            }
            _ => {}
        }
    }

    if package.name.is_none() {
        return Ok(default_bazel_module_package_data());
    }

    if !overrides.is_empty() {
        extra_data.insert("overrides".to_string(), JsonValue::Array(overrides));
    }

    package.dependencies = dependencies;
    package.extra_data = (!extra_data.is_empty()).then(|| extra_data.into_iter().collect());
    Ok(package)
}

fn parse_starlark_module(filename: &str, content: String) -> Result<AstModule, String> {
    let dialect = Dialect {
        enable_top_level_stmt: true,
        ..Dialect::Standard
    };
    AstModule::parse(filename, content, &dialect).map_err(|error| error.to_string())
}

fn top_level_statements(module: &AstModule) -> &[ast::AstStmt] {
    match &module.statement().node {
        ast::StmtP::Statements(statements) => statements,
        _ => std::slice::from_ref(module.statement()),
    }
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

fn extract_call_name<'a>(call: &'a StarlarkCall<'_>) -> Option<&'a str> {
    match &call.func.node {
        ast::ExprP::Identifier(identifier) => Some(identifier.node.ident.as_str()),
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

fn extract_string_kwarg(call: &StarlarkCall<'_>, key: &str) -> Option<String> {
    extract_named_kwarg(call, key).and_then(expr_as_string)
}

fn extract_string_list_kwarg(call: &StarlarkCall<'_>, key: &str) -> Option<Vec<String>> {
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

fn extract_bool_kwarg(call: &StarlarkCall<'_>, key: &str) -> Option<bool> {
    extract_named_kwarg(call, key).and_then(expr_as_bool)
}

fn extract_int_kwarg(call: &StarlarkCall<'_>, key: &str) -> Option<i64> {
    extract_named_kwarg(call, key).and_then(expr_as_i64)
}

fn extract_kwarg_json(call: &StarlarkCall<'_>, key: &str) -> Option<JsonValue> {
    extract_named_kwarg(call, key)
        .and_then(|expr| expr_to_json(expr, &mut RecursionGuard::depth_only()))
}

fn extract_bazel_dependency(call: &StarlarkCall<'_>) -> Option<Dependency> {
    let name = extract_string_kwarg(call, "name").map(truncate_field)?;
    let version = extract_string_kwarg(call, "version").map(truncate_field);
    let is_dev = extract_bool_kwarg(call, "dev_dependency").unwrap_or(false);
    let mut extra_data = JsonMap::new();

    for field in ["repo_name", "max_compatibility_level", "registry"]
        .iter()
        .take(MAX_ITERATION_COUNT)
    {
        if let Some(value) = extract_kwarg_json(call, field) {
            extra_data.insert(field.to_string(), value);
        }
    }

    Some(Dependency {
        purl: build_bazel_purl(&name, version.as_deref()).map(truncate_field),
        extracted_requirement: version.clone(),
        scope: Some(if is_dev { "dev" } else { "dependencies" }.to_string()),
        is_runtime: Some(!is_dev),
        is_optional: Some(is_dev),
        is_pinned: Some(version.is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then(|| extra_data.into_iter().collect()),
    })
}

fn extract_override(kind: &str, call: &StarlarkCall<'_>) -> JsonValue {
    let mut override_map = JsonMap::new();
    override_map.insert("kind".to_string(), JsonValue::String(kind.to_string()));
    for argument in call.args.args.iter().take(MAX_ITERATION_COUNT) {
        if let ast::ArgumentP::Named(name, value) = &argument.node
            && let Some(value) = expr_to_json(value, &mut RecursionGuard::depth_only())
        {
            override_map.insert(name.node.clone(), value);
        }
    }
    JsonValue::Object(override_map)
}

fn expr_as_string(expr: &ast::AstExpr) -> Option<String> {
    match &expr.node {
        ast::ExprP::Literal(ast::AstLiteral::String(value)) => Some(value.node.clone()),
        _ => None,
    }
}

fn expr_as_bool(expr: &ast::AstExpr) -> Option<bool> {
    match &expr.node {
        ast::ExprP::Identifier(identifier) => match identifier.node.ident.as_str() {
            "True" => Some(true),
            "False" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn expr_as_i64(expr: &ast::AstExpr) -> Option<i64> {
    match &expr.node {
        ast::ExprP::Literal(ast::AstLiteral::Int(value)) => value.node.to_string().parse().ok(),
        _ => None,
    }
}

fn expr_to_json(expr: &ast::AstExpr, guard: &mut RecursionGuard<()>) -> Option<JsonValue> {
    if guard.descend() {
        return None;
    }
    let result = match &expr.node {
        ast::ExprP::Literal(ast::AstLiteral::String(value)) => {
            Some(JsonValue::String(value.node.clone()))
        }
        ast::ExprP::Literal(ast::AstLiteral::Int(value)) => value
            .node
            .to_string()
            .parse::<i64>()
            .ok()
            .map(|value| JsonValue::Number(value.into()))
            .or_else(|| Some(JsonValue::String(value.node.to_string()))),
        ast::ExprP::Literal(ast::AstLiteral::Float(value)) => {
            serde_json::Number::from_f64(value.node).map(JsonValue::Number)
        }
        ast::ExprP::Identifier(identifier) => match identifier.node.ident.as_str() {
            "True" => Some(JsonValue::Bool(true)),
            "False" => Some(JsonValue::Bool(false)),
            "None" => Some(JsonValue::Null),
            _ => None,
        },
        ast::ExprP::List(elts) | ast::ExprP::Tuple(elts) => Some(JsonValue::Array(
            elts.iter()
                .take(MAX_ITERATION_COUNT)
                .filter_map(|e| expr_to_json(e, guard))
                .collect(),
        )),
        ast::ExprP::Dict(items) => {
            let mut map = JsonMap::new();
            for (key, value) in items.iter().take(MAX_ITERATION_COUNT) {
                let Some(key) = expr_as_string(key) else {
                    continue;
                };
                if let Some(value) = expr_to_json(value, guard) {
                    map.insert(key, value);
                }
            }
            Some(JsonValue::Object(map))
        }
        _ => None,
    };
    guard.ascend();
    result
}

fn build_bazel_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("bazel", name).ok()?;
    if let Some(version) = version.filter(|value| !value.trim().is_empty()) {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn default_bazel_module_package_data() -> PackageData {
    PackageData {
        package_type: Some(BazelModuleParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::BazelModule),
        ..Default::default()
    }
}

crate::register_parser!(
    "Bazel MODULE.bazel file",
    &["**/MODULE.bazel"],
    "bazel",
    "",
    Some("https://bazel.build/external/module"),
);
