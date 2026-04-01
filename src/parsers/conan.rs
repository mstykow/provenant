//! Parser for Conan C/C++ package manager manifests.
//!
//! Extracts package metadata and dependencies from Conan manifest files.
//!
//! # Supported Formats
//! - conanfile.py (Recipe files with Python AST parsing)
//! - conanfile.txt (Simple dependency specification format)
//! - conan.lock (Lockfile with resolved dependency graph)
//!
//! # Key Features
//! - AST-based conanfile.py parsing (NO code execution)
//! - Dependency extraction from [requires] and [build_requires] sections
//! - Version constraint parsing for Conan reference format (name/version@user/channel)
//! - Package URL (purl) generation for resolved dependencies
//! - Lockfile dependency graph parsing
//!
//! # Implementation Notes
//! - conanfile.py: AST extracts class attributes and self.requires() calls
//! - conanfile.txt sections: [requires] = runtime, [build_requires] = build-time
//! - conan.lock uses JSON format with graph_lock.nodes structure
//! - Version constraints use Conan-specific operators: [>, <, ranges]
//! - Only exact versions (without operators) are extracted as pinned versions

use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use ruff_python_ast as ast;
use ruff_python_parser::parse_module;
use serde_json::Value;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;
use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, normalize_declared_license_key,
};

/// Conan conanfile.py recipe parser.
///
/// Parses Python-based Conan recipe files using AST analysis (no code execution).
/// Extracts package metadata and dependencies from ConanFile class attributes.
pub struct ConanFilePyParser;

impl PackageParser for ConanFilePyParser {
    const PACKAGE_TYPE: PackageType = PackageType::Conan;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "conanfile.py")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {}: {}", path.display(), e);
                return vec![default_package_data(DatasourceId::ConanConanFilePy)];
            }
        };

        vec![match parse_module(&contents) {
            Ok(parsed) => parse_conanfile_py(parsed.suite()),
            Err(e) => {
                warn!("Failed to parse Python AST in {}: {}", path.display(), e);
                default_package_data(DatasourceId::ConanConanFilePy)
            }
        }]
    }
}

/// Parse conanfile.py AST to extract ConanFile class attributes
fn parse_conanfile_py(statements: &[ast::Stmt]) -> PackageData {
    for stmt in statements {
        if let ast::Stmt::ClassDef(class_def) = stmt
            && has_conanfile_base(class_def)
        {
            return extract_conanfile_data(class_def);
        }
    }

    default_package_data(DatasourceId::ConanConanFilePy)
}

/// Check if class inherits from ConanFile
fn has_conanfile_base(class_def: &ast::StmtClassDef) -> bool {
    class_def.bases().iter().any(|base| {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = base {
            id.as_str() == "ConanFile"
        } else {
            false
        }
    })
}

/// Extract package data from ConanFile class definition
fn extract_conanfile_data(class_def: &ast::StmtClassDef) -> PackageData {
    let mut name = None;
    let mut version = None;
    let mut description = None;
    let mut _author = None;
    let mut homepage_url = None;
    let mut vcs_url = None;
    let mut license_list = Vec::new();
    let mut keywords = Vec::new();
    let mut requires_list = Vec::new();
    let mut tool_requires_list = Vec::new();

    for stmt in class_def.body.iter() {
        match stmt {
            ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                if let Some(target_name) = get_assignment_target(targets) {
                    match target_name.as_str() {
                        "name" => name = get_string_value(value),
                        "version" => version = get_string_value(value),
                        "description" => description = get_string_value(value),
                        "author" => _author = get_string_value(value),
                        "homepage" => homepage_url = get_string_value(value),
                        "url" => vcs_url = get_string_value(value),
                        "license" => license_list = get_list_values(value),
                        "topics" => keywords = get_list_values(value),
                        "requires" => requires_list = get_list_values(value),
                        _ => {}
                    }
                }
            }
            ast::Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
                if let Some(requires) = extract_self_requires_calls(body, "requires") {
                    requires_list.extend(requires);
                }
                if let Some(tool_requires) = extract_self_requires_calls(body, "tool_requires") {
                    tool_requires_list.extend(tool_requires);
                }
            }
            _ => {}
        }
    }

    let mut dependencies = requires_list
        .into_iter()
        .filter_map(|req| parse_conan_reference(&req))
        .collect::<Vec<_>>();
    dependencies.extend(
        tool_requires_list
            .into_iter()
            .filter_map(|req| parse_conan_reference(&req))
            .map(|dep| Dependency {
                scope: Some("build".to_string()),
                is_runtime: Some(false),
                ..dep
            }),
    );

    let extracted_license = if !license_list.is_empty() {
        Some(license_list.join(", "))
    } else {
        None
    };
    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        if license_list.len() == 1 {
            if let Some(normalized) = normalize_declared_license_key(&license_list[0]) {
                build_declared_license_data(
                    normalized,
                    DeclaredLicenseMatchMetadata::single_line(&license_list[0]),
                )
            } else {
                (None, None, Vec::new())
            }
        } else {
            (None, None, Vec::new())
        };

    PackageData {
        name,
        version,
        description,
        homepage_url,
        vcs_url,
        keywords,
        dependencies,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement: extracted_license,
        datasource_id: Some(DatasourceId::ConanConanFilePy),
        ..default_package_data(DatasourceId::ConanConanFilePy)
    }
}

/// Get assignment target name (e.g., "name" from "name = 'foo'")
fn get_assignment_target(targets: &[ast::Expr]) -> Option<String> {
    targets.first().and_then(|target| {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = target {
            Some(id.to_string())
        } else {
            None
        }
    })
}

/// Extract string value from AST expression
fn get_string_value(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            Some(value.to_str().to_string())
        }
        _ => None,
    }
}

/// Extract list of strings from tuple or list expression
fn get_list_values(expr: &ast::Expr) -> Vec<String> {
    match expr {
        ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            elts.iter().filter_map(get_string_value).collect()
        }
        ast::Expr::List(ast::ExprList { elts, .. }) => {
            elts.iter().filter_map(get_string_value).collect()
        }
        _ => {
            if let Some(s) = get_string_value(expr) {
                vec![s]
            } else {
                Vec::new()
            }
        }
    }
}

/// Extract self.requires() method calls from function body
fn extract_self_requires_calls(body: &[ast::Stmt], method_name: &str) -> Option<Vec<String>> {
    let mut requires = Vec::new();

    for stmt in body {
        collect_self_method_calls(stmt, method_name, &mut requires);
    }

    if requires.is_empty() {
        None
    } else {
        Some(requires)
    }
}

fn collect_self_method_calls(stmt: &ast::Stmt, method_name: &str, out: &mut Vec<String>) {
    match stmt {
        ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
            if let ast::Expr::Call(call) = value.as_ref()
                && is_self_method_call(call, method_name)
                && let Some(arg) = call.arguments.args.first()
                && let Some(req) = get_string_value(arg)
            {
                out.push(req);
            }
        }
        ast::Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            for nested in body {
                collect_self_method_calls(nested, method_name, out);
            }
            for clause in elif_else_clauses {
                for nested in &clause.body {
                    collect_self_method_calls(nested, method_name, out);
                }
            }
        }
        ast::Stmt::With(ast::StmtWith { body, .. })
        | ast::Stmt::While(ast::StmtWhile { body, .. })
        | ast::Stmt::For(ast::StmtFor { body, .. }) => {
            for nested in body {
                collect_self_method_calls(nested, method_name, out);
            }
        }
        ast::Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            for nested in body.iter().chain(orelse.iter()).chain(finalbody.iter()) {
                collect_self_method_calls(nested, method_name, out);
            }
            for handler in handlers {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                for nested in &handler.body {
                    collect_self_method_calls(nested, method_name, out);
                }
            }
        }
        ast::Stmt::Match(ast::StmtMatch { cases, .. }) => {
            for case in cases {
                for nested in &case.body {
                    collect_self_method_calls(nested, method_name, out);
                }
            }
        }
        _ => {}
    }
}

fn is_self_method_call(call: &ast::ExprCall, method_name: &str) -> bool {
    if let ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = call.func.as_ref()
        && let ast::Expr::Name(ast::ExprName { id, .. }) = value.as_ref()
    {
        return id.as_str() == "self" && attr.as_str() == method_name;
    }
    false
}

/// Conan conanfile.txt manifest parser.
///
/// Extracts dependencies from the simple conanfile.txt format, which uses
/// INI-style sections to specify runtime and build-time dependencies.
pub struct ConanfileTxtParser;

impl PackageParser for ConanfileTxtParser {
    const PACKAGE_TYPE: PackageType = PackageType::Conan;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "conanfile.txt")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {}: {}", path.display(), e);
                return vec![default_package_data(DatasourceId::ConanConanFileTxt)];
            }
        };

        let dependencies = parse_conanfile_txt(&contents);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            dependencies,
            primary_language: Some("C++".to_string()),
            datasource_id: Some(DatasourceId::ConanConanFileTxt),
            ..default_package_data(DatasourceId::ConanConanFileTxt)
        }]
    }
}

/// Conan lockfile (conan.lock) parser.
///
/// Extracts resolved dependencies from Conan lockfiles, which capture the
/// complete dependency graph with exact versions and revisions.
pub struct ConanLockParser;

impl PackageParser for ConanLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Conan;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "conan.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {}: {}", path.display(), e);
                return vec![default_package_data(DatasourceId::ConanLock)];
            }
        };

        let json: Value = match serde_json::from_str(&contents) {
            Ok(j) => j,
            Err(e) => {
                warn!("Failed to parse JSON in {}: {}", path.display(), e);
                return vec![default_package_data(DatasourceId::ConanLock)];
            }
        };

        let dependencies = parse_conan_lock(&json);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            dependencies,
            primary_language: Some("C++".to_string()),
            datasource_id: Some(DatasourceId::ConanLock),
            ..default_package_data(DatasourceId::ConanLock)
        }]
    }
}

fn parse_conan_reference(ref_str: &str) -> Option<Dependency> {
    let (name, version_spec) = if let Some((n, v)) = ref_str.split_once('/') {
        (n.trim(), Some(v.trim().to_string()))
    } else {
        (ref_str.trim(), None)
    };

    let version = version_spec.as_ref().and_then(|v| {
        if !v.contains('[') && !v.contains('>') && !v.contains('<') {
            Some(v.clone())
        } else {
            None
        }
    });

    let purl = if let Some(v) = version.as_deref() {
        PackageUrl::new("conan", name)
            .map(|mut p| {
                let _ = p.with_version(v);
                p.to_string()
            })
            .unwrap_or_else(|_| format!("pkg:conan/{}", name))
    } else {
        format!("pkg:conan/{}", name)
    };

    let is_pinned = version_spec
        .as_ref()
        .map(|v| !v.contains('[') && !v.contains('>') && !v.contains('<'))
        .unwrap_or(false);

    Some(Dependency {
        purl: Some(purl),
        extracted_requirement: version_spec,
        scope: Some("install".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn parse_conanfile_txt(contents: &str) -> Vec<Dependency> {
    let mut dependencies = Vec::new();
    let mut current_section = None;

    for line in contents.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = Some(trimmed.trim_matches(|c| c == '[' || c == ']').to_string());
            continue;
        }

        if let Some(ref section) = current_section {
            let (scope, is_runtime) = match section.as_str() {
                "requires" => ("install", true),
                "build_requires" => ("build", false),
                _ => continue,
            };

            if let Some(dep) = parse_conan_reference(trimmed) {
                dependencies.push(Dependency {
                    scope: Some(scope.to_string()),
                    is_runtime: Some(is_runtime),
                    ..dep
                });
            }
        }
    }

    dependencies
}

fn parse_conan_lock(json: &Value) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(graph_lock) = json.get("graph_lock")
        && let Some(nodes) = graph_lock.get("nodes").and_then(|n| n.as_object())
    {
        for (_node_id, node_data) in nodes {
            if let Some(ref_str) = node_data.get("ref").and_then(|r| r.as_str())
                && !ref_str.is_empty()
                && ref_str != "conanfile"
                && let Some(dep) = parse_conan_reference(ref_str)
            {
                dependencies.push(dep);
            }
        }
    }

    dependencies
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(ConanFilePyParser::PACKAGE_TYPE),
        primary_language: Some("C++".to_string()),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

crate::register_parser!(
    "Conan C/C++ package manifest",
    &["**/conanfile.py", "**/conanfile.txt", "**/conan.lock"],
    "conan",
    "C++",
    Some("https://docs.conan.io/"),
);
