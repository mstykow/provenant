use super::super::license_normalization::normalize_spdx_declared_license;
use super::PythonParser;
use super::utils::{
    ProjectUrls, apply_project_url_mappings, build_python_dependency, build_setup_py_purl,
    default_package_data, extract_setup_py_dependencies, extract_setup_value,
    has_private_classifier,
};
use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::utils::{read_file_to_string, truncate_field};
use regex::Regex;
use ruff_python_ast as ast;
use ruff_python_parser::parse_module;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

pub(super) const MAX_SETUP_PY_BYTES: usize = 1_048_576;
pub(super) const MAX_SETUP_PY_AST_NODES: usize = 10_000;
pub(super) const MAX_SETUP_PY_AST_DEPTH: usize = 50;

static VERSION_DUNDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*__version__\s*=\s*['\"]([^'\"]+)['\"]"#)
        .expect("__version__ regex should compile")
});
static AUTHOR_DUNDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*__author__\s*=\s*['\"]([^'\"]+)['\"]"#)
        .expect("__author__ regex should compile")
});
static LICENSE_DUNDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*__license__\s*=\s*['\"]([^'\"]+)['\"]"#)
        .expect("__license__ regex should compile")
});
static OPEN_INIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"open\(\s*['\"]([^'\"]+__init__\.py)['\"]"#)
        .expect("open __init__.py regex should compile")
});
static DUNDER_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b([A-Za-z_][A-Za-z0-9_]*)\s*\.\s*__(?:version|author|license)__\b"#)
        .expect("dunder attribute regex should compile")
});

fn regex_capture(regex: &Regex, text: &str) -> Option<String> {
    regex
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

#[derive(Debug, Clone)]
enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Dict(HashMap<String, Value>),
}

#[derive(Default)]
struct SetupKeywords {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    summary: Option<String>,
    url: Option<String>,
    home_page: Option<String>,
    author: Option<String>,
    author_email: Option<String>,
    maintainer: Option<String>,
    maintainer_email: Option<String>,
    license: Option<String>,
    classifiers: Option<Vec<String>>,
    install_requires: Option<Vec<String>>,
    tests_require: Option<Vec<String>>,
    extras_require: Option<HashMap<String, Value>>,
    project_urls: Option<HashMap<String, Value>>,
}

impl SetupKeywords {
    fn set_field(&mut self, name: &str, value: Value) {
        match name {
            "name" => self.name = value_to_string(&value),
            "version" => self.version = value_to_string(&value),
            "description" => self.description = value_to_string(&value),
            "summary" => self.summary = value_to_string(&value),
            "url" => self.url = value_to_string(&value),
            "home_page" => self.home_page = value_to_string(&value),
            "author" => self.author = value_to_string(&value),
            "author_email" => self.author_email = value_to_string(&value),
            "maintainer" => self.maintainer = value_to_string(&value),
            "maintainer_email" => self.maintainer_email = value_to_string(&value),
            "license" => self.license = value_to_string(&value),
            "classifiers" => self.classifiers = value_to_string_list(&value),
            "install_requires" => self.install_requires = value_to_string_list(&value),
            "tests_require" => self.tests_require = value_to_string_list(&value),
            "extras_require" => {
                if let Value::Dict(dict) = value {
                    self.extras_require = Some(dict);
                }
            }
            "project_urls" => {
                if let Value::Dict(dict) = value {
                    self.project_urls = Some(dict);
                }
            }
            _ => {}
        }
    }
}

struct LiteralEvaluator {
    constants: HashMap<String, Value>,
    max_depth: usize,
    max_nodes: usize,
    nodes_visited: usize,
}

impl LiteralEvaluator {
    fn new(constants: HashMap<String, Value>) -> Self {
        Self {
            constants,
            max_depth: MAX_SETUP_PY_AST_DEPTH,
            max_nodes: MAX_SETUP_PY_AST_NODES,
            nodes_visited: 0,
        }
    }

    fn insert_constant(&mut self, name: String, value: Value) {
        self.constants.insert(name, value);
    }

    fn evaluate_expr(&mut self, expr: &ast::Expr, depth: usize) -> Option<Value> {
        if depth >= self.max_depth || self.nodes_visited >= self.max_nodes {
            return None;
        }
        self.nodes_visited += 1;

        match expr {
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                Some(Value::String(value.to_str().to_string()))
            }
            ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => {
                Some(Value::Bool(*value))
            }
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
                self.evaluate_number(value)
            }
            ast::Expr::NoneLiteral(_) => None,
            ast::Expr::Name(ast::ExprName { id, .. }) => self.constants.get(id.as_str()).cloned(),
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                let mut values = Vec::new();
                for elt in elts {
                    values.push(self.evaluate_expr(elt, depth + 1)?);
                }
                Some(Value::List(values))
            }
            ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let mut values = Vec::new();
                for elt in elts {
                    values.push(self.evaluate_expr(elt, depth + 1)?);
                }
                Some(Value::Tuple(values))
            }
            ast::Expr::Dict(ast::ExprDict { items, .. }) => {
                let mut dict = HashMap::new();
                for item in items {
                    let key_expr = item.key.as_ref()?;
                    let key_value = self.evaluate_expr(key_expr, depth + 1)?;
                    let key = value_to_string(&key_value)?;
                    let value = self.evaluate_expr(&item.value, depth + 1)?;
                    dict.insert(key, value);
                }
                Some(Value::Dict(dict))
            }
            ast::Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                let args = arguments.args.as_ref();
                let keywords = arguments.keywords.as_ref();
                if keywords.is_empty()
                    && let Some(name) = dotted_name(func.as_ref(), depth + 1)
                    && matches!(name.as_str(), "OrderedDict" | "collections.OrderedDict")
                {
                    return self.evaluate_ordered_dict(args, depth + 1);
                }

                if !args.is_empty() {
                    return None;
                }

                if let ast::Expr::Name(ast::ExprName { id, .. }) = func.as_ref()
                    && id == "dict"
                {
                    let mut dict = HashMap::new();
                    for keyword in keywords {
                        let key = keyword.arg.as_ref().map(ast::Identifier::as_str)?;
                        let value = self.evaluate_expr(&keyword.value, depth + 1)?;
                        dict.insert(key.to_string(), value);
                    }
                    return Some(Value::Dict(dict));
                }

                None
            }
            _ => None,
        }
    }

    fn evaluate_number(&self, number: &ast::Number) -> Option<Value> {
        match number {
            ast::Number::Int(value) => value.to_string().parse::<f64>().ok().map(Value::Number),
            ast::Number::Float(value) => Some(Value::Number(*value)),
            ast::Number::Complex { .. } => None,
        }
    }

    fn evaluate_ordered_dict(&mut self, args: &[ast::Expr], depth: usize) -> Option<Value> {
        if args.len() != 1 {
            return None;
        }

        let items = match self.evaluate_expr(&args[0], depth)? {
            Value::List(items) | Value::Tuple(items) => items,
            _ => return None,
        };

        let mut dict = HashMap::new();
        for item in items {
            let Value::Tuple(values) = item else {
                return None;
            };
            if values.len() != 2 {
                return None;
            }
            let key = value_to_string(&values[0])?;
            dict.insert(key, values[1].clone());
        }

        Some(Value::Dict(dict))
    }
}

#[derive(Default)]
struct SetupAliases {
    setup_names: HashSet<String>,
    module_aliases: HashMap<String, String>,
}

pub(super) fn extract_setup_py_packages(path: &Path) -> Vec<PackageData> {
    vec![extract_from_setup_py(path)]
}

fn extract_from_setup_py(path: &Path) -> PackageData {
    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read setup.py at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    if content.len() > MAX_SETUP_PY_BYTES {
        warn!("setup.py too large at {:?}: {} bytes", path, content.len());
        let package_data = extract_from_setup_py_regex(&content);
        return if should_emit_setup_py_package(&package_data) {
            package_data
        } else {
            default_package_data(path)
        };
    }

    let mut package_data = match extract_from_setup_py_ast(&content) {
        Ok(Some(data)) => data,
        Ok(None) => return default_package_data(path),
        Err(e) => {
            warn!("Failed to parse setup.py AST at {:?}: {}", path, e);
            extract_from_setup_py_regex(&content)
        }
    };

    if package_data.name.is_none() {
        package_data.name = extract_setup_value(&content, "name");
    }

    if package_data.version.is_none() {
        package_data.version = extract_setup_value(&content, "version");
    }

    if package_data
        .version
        .as_deref()
        .is_some_and(|version| version.trim().is_empty())
    {
        package_data.version = None;
    }

    fill_from_sibling_dunder_metadata(path, &content, &mut package_data);
    package_data.purl = build_setup_py_purl(
        package_data.name.as_deref(),
        package_data.version.as_deref(),
    );

    if should_emit_setup_py_package(&package_data) {
        package_data
    } else {
        default_package_data(path)
    }
}

fn should_emit_setup_py_package(package_data: &PackageData) -> bool {
    package_data.name.is_some()
        || package_data.version.is_some()
        || package_data.purl.is_some()
        || !package_data.dependencies.is_empty()
        || package_data.extracted_license_statement.is_some()
        || !package_data.license_detections.is_empty()
        || !package_data.parties.is_empty()
        || package_data.description.is_some()
        || package_data.homepage_url.is_some()
        || package_data.bug_tracking_url.is_some()
        || package_data.code_view_url.is_some()
        || package_data.vcs_url.is_some()
}

fn fill_from_sibling_dunder_metadata(path: &Path, content: &str, package_data: &mut PackageData) {
    if package_data.version.is_some()
        && package_data.extracted_license_statement.is_some()
        && package_data
            .parties
            .iter()
            .any(|party| party.role.as_deref() == Some("author") && party.name.is_some())
    {
        return;
    }

    let Some(root) = path.parent() else {
        return;
    };

    let dunder_metadata = collect_sibling_dunder_metadata(root, content);

    if package_data.version.is_none() {
        package_data.version = dunder_metadata.version;
    }

    if package_data.extracted_license_statement.is_none() {
        package_data.extracted_license_statement = dunder_metadata.license;
    }

    let has_author = package_data
        .parties
        .iter()
        .any(|party| party.role.as_deref() == Some("author") && party.name.is_some());

    if !has_author && let Some(author) = dunder_metadata.author {
        package_data
            .parties
            .push(Party::person("author", Some(author), None));
    }
}

#[derive(Default)]
struct DunderMetadata {
    version: Option<String>,
    author: Option<String>,
    license: Option<String>,
}

fn collect_sibling_dunder_metadata(root: &Path, content: &str) -> DunderMetadata {
    let statements = match parse_module(content) {
        Ok(parsed) => parsed.into_suite(),
        Err(_) => return DunderMetadata::default(),
    };

    let mut metadata = DunderMetadata::default();
    let mut candidate_paths = Vec::new();

    for module in imported_dunder_modules(&statements) {
        let Some(path) = resolve_imported_module_path(root, &module) else {
            continue;
        };

        candidate_paths.push(path);
    }

    candidate_paths.extend(referenced_dunder_attribute_paths(root, content));
    candidate_paths.extend(referenced_dunder_init_paths(root, content));

    let mut seen_paths = HashSet::new();
    for path in candidate_paths {
        if !seen_paths.insert(path.clone()) {
            continue;
        }

        let Ok(module_content) = read_file_to_string(&path, None) else {
            continue;
        };

        if metadata.version.is_none() {
            metadata.version = regex_capture(&VERSION_DUNDER_RE, &module_content);
        }

        if metadata.author.is_none() {
            metadata.author = regex_capture(&AUTHOR_DUNDER_RE, &module_content);
        }

        if metadata.license.is_none() {
            metadata.license = regex_capture(&LICENSE_DUNDER_RE, &module_content);
        }

        if metadata.version.is_some() && metadata.author.is_some() && metadata.license.is_some() {
            return metadata;
        }
    }

    metadata
}

fn referenced_dunder_init_paths(root: &Path, content: &str) -> Vec<PathBuf> {
    OPEN_INIT_RE
        .captures_iter(content)
        .filter_map(|captures| captures.get(1).map(|m| m.as_str()))
        .filter_map(|relative| {
            let relative_path = PathBuf::from(relative);
            if relative_path.is_absolute()
                || relative_path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return None;
            }

            let candidate = root.join(relative_path);
            candidate.exists().then_some(candidate)
        })
        .collect()
}

fn referenced_dunder_attribute_paths(root: &Path, content: &str) -> Vec<PathBuf> {
    let mut seen_modules = HashSet::new();
    DUNDER_ATTR_RE
        .captures_iter(content)
        .filter_map(|captures| captures.get(1).map(|m| m.as_str().to_string()))
        .filter(|module| seen_modules.insert(module.clone()))
        .filter_map(|module| resolve_imported_module_path(root, &module))
        .collect()
}

fn imported_dunder_modules(statements: &[ast::Stmt]) -> Vec<String> {
    let mut modules = Vec::new();

    for statement in statements {
        let ast::Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) = statement else {
            continue;
        };
        let Some(module) = module.as_ref().map(|name| name.as_str()) else {
            continue;
        };
        let imports_dunder = names.iter().any(|alias| {
            matches!(
                alias.name.as_str(),
                "__version__" | "__author__" | "__license__"
            )
        });
        if imports_dunder {
            modules.push(module.to_string());
        }
    }

    modules
}

fn resolve_imported_module_path(root: &Path, module: &str) -> Option<PathBuf> {
    let relative = PathBuf::from_iter(module.split('.'));
    let candidates = [
        root.join(relative.with_extension("py")),
        root.join(&relative).join("__init__.py"),
        root.join("src").join(relative.with_extension("py")),
        root.join("src").join(relative).join("__init__.py"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn extract_from_setup_py_ast(content: &str) -> Result<Option<PackageData>, String> {
    let statements = parse_module(content)
        .map(|parsed| parsed.into_suite())
        .map_err(|e| e.to_string())?;
    let aliases = collect_setup_aliases(&statements);
    let mut evaluator = LiteralEvaluator::new(HashMap::new());
    build_setup_py_constants(&statements, &mut evaluator);

    let setup_call = find_setup_call(&statements, &aliases);
    let Some(call_expr) = setup_call else {
        return Ok(None);
    };

    let setup_keywords = extract_setup_keywords(call_expr, &mut evaluator);
    Ok(Some(build_setup_py_package_data(&setup_keywords)))
}

fn build_setup_py_constants(statements: &[ast::Stmt], evaluator: &mut LiteralEvaluator) {
    for stmt in statements {
        if let ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) = stmt {
            if targets.len() != 1 {
                continue;
            }

            let Some(name) = extract_assign_name(&targets[0]) else {
                continue;
            };

            if let Some(value) = evaluator.evaluate_expr(value.as_ref(), 0) {
                evaluator.insert_constant(name, value);
            }
        }
    }
}

fn extract_assign_name(target: &ast::Expr) -> Option<String> {
    match target {
        ast::Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str().to_string()),
        _ => None,
    }
}

fn collect_setup_aliases(statements: &[ast::Stmt]) -> SetupAliases {
    let mut aliases = SetupAliases::default();
    aliases.setup_names.insert("setup".to_string());

    for stmt in statements {
        match stmt {
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    let module_name = alias.name.as_str();
                    if !is_setup_module(module_name) {
                        continue;
                    }
                    let alias_name = alias
                        .asname
                        .as_ref()
                        .map(|name| name.as_str())
                        .unwrap_or(module_name);
                    aliases
                        .module_aliases
                        .insert(alias_name.to_string(), module_name.to_string());
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) => {
                let Some(module_name) = module.as_ref().map(|name| name.as_str()) else {
                    continue;
                };
                if !is_setup_module(module_name) {
                    continue;
                }
                for alias in names {
                    if alias.name.as_str() != "setup" {
                        continue;
                    }
                    let alias_name = alias
                        .asname
                        .as_ref()
                        .map(|name| name.as_str())
                        .unwrap_or("setup");
                    aliases.setup_names.insert(alias_name.to_string());
                }
            }
            _ => {}
        }
    }

    aliases
}

fn is_setup_module(module_name: &str) -> bool {
    matches!(module_name, "setuptools" | "distutils" | "distutils.core")
}

fn find_setup_call<'a>(
    statements: &'a [ast::Stmt],
    aliases: &'a SetupAliases,
) -> Option<&'a ast::Expr> {
    let mut finder = SetupCallFinder {
        aliases,
        called_function_names: collect_top_level_called_function_names(statements),
        nodes_visited: 0,
    };
    finder.find_in_statements(statements)
}

fn collect_top_level_called_function_names(statements: &[ast::Stmt]) -> HashSet<String> {
    let mut called = HashSet::new();
    collect_called_function_names_in_statements(statements, &mut called);
    called
}

fn collect_called_function_names_in_statements(
    statements: &[ast::Stmt],
    called: &mut HashSet<String>,
) {
    for stmt in statements {
        match stmt {
            ast::Stmt::Expr(ast::StmtExpr { value, .. })
            | ast::Stmt::Assign(ast::StmtAssign { value, .. }) => {
                collect_called_function_names_in_expr(value.as_ref(), called);
            }
            ast::Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                collect_called_function_names_in_statements(body, called);
                for clause in elif_else_clauses {
                    collect_called_function_names_in_statements(&clause.body, called);
                }
            }
            ast::Stmt::For(ast::StmtFor { body, orelse, .. })
            | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                collect_called_function_names_in_statements(body, called);
                collect_called_function_names_in_statements(orelse, called);
            }
            ast::Stmt::With(ast::StmtWith { body, .. }) => {
                collect_called_function_names_in_statements(body, called);
            }
            ast::Stmt::Try(ast::StmtTry {
                body,
                orelse,
                finalbody,
                handlers,
                ..
            }) => {
                collect_called_function_names_in_statements(body, called);
                collect_called_function_names_in_statements(orelse, called);
                collect_called_function_names_in_statements(finalbody, called);
                for handler in handlers {
                    let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body,
                        ..
                    }) = handler;
                    collect_called_function_names_in_statements(body, called);
                }
            }
            _ => {}
        }
    }
}

fn collect_called_function_names_in_expr(expr: &ast::Expr, called: &mut HashSet<String>) {
    if let ast::Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
            called.insert(id.as_str().to_string());
        }

        for arg in arguments.args.iter() {
            collect_called_function_names_in_expr(arg, called);
        }
        for keyword in arguments.keywords.iter() {
            collect_called_function_names_in_expr(&keyword.value, called);
        }
    }
}

struct SetupCallFinder<'a> {
    aliases: &'a SetupAliases,
    called_function_names: HashSet<String>,
    nodes_visited: usize,
}

impl<'a> SetupCallFinder<'a> {
    fn find_in_statements(&mut self, statements: &'a [ast::Stmt]) -> Option<&'a ast::Expr> {
        for stmt in statements {
            if self.nodes_visited >= MAX_SETUP_PY_AST_NODES {
                return None;
            }
            self.nodes_visited += 1;

            let found = match stmt {
                ast::Stmt::Expr(ast::StmtExpr { value, .. }) => self.visit_expr(value.as_ref()),
                ast::Stmt::Assign(ast::StmtAssign { value, .. }) => self.visit_expr(value.as_ref()),
                ast::Stmt::If(ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                }) => self.find_in_statements(body).or_else(|| {
                    for clause in elif_else_clauses {
                        if let Some(found) = self.find_in_statements(&clause.body) {
                            return Some(found);
                        }
                    }
                    None
                }),
                ast::Stmt::For(ast::StmtFor { body, orelse, .. })
                | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => self
                    .find_in_statements(body)
                    .or_else(|| self.find_in_statements(orelse)),
                ast::Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) => self
                    .called_function_names
                    .contains(name.as_str())
                    .then(|| self.find_in_statements(body))
                    .flatten(),
                ast::Stmt::With(ast::StmtWith { body, .. }) => self.find_in_statements(body),
                ast::Stmt::Try(ast::StmtTry {
                    body,
                    orelse,
                    finalbody,
                    handlers,
                    ..
                }) => self
                    .find_in_statements(body)
                    .or_else(|| self.find_in_statements(orelse))
                    .or_else(|| self.find_in_statements(finalbody))
                    .or_else(|| {
                        for handler in handlers {
                            let ast::ExceptHandler::ExceptHandler(
                                ast::ExceptHandlerExceptHandler { body, .. },
                            ) = handler;
                            if let Some(found) = self.find_in_statements(body) {
                                return Some(found);
                            }
                        }
                        None
                    }),
                _ => None,
            };

            if found.is_some() {
                return found;
            }
        }

        None
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) -> Option<&'a ast::Expr> {
        if self.nodes_visited >= MAX_SETUP_PY_AST_NODES {
            return None;
        }
        self.nodes_visited += 1;

        match expr {
            ast::Expr::Call(ast::ExprCall { func, .. })
                if is_setup_call(func.as_ref(), self.aliases) =>
            {
                Some(expr)
            }
            _ => None,
        }
    }
}

fn is_setup_call(func: &ast::Expr, aliases: &SetupAliases) -> bool {
    let Some(dotted) = dotted_name(func, 0) else {
        return false;
    };

    if aliases.setup_names.contains(&dotted) {
        return true;
    }

    let Some(module) = dotted.strip_suffix(".setup") else {
        return false;
    };

    let resolved = resolve_module_alias(module, aliases);
    is_setup_module(&resolved)
}

fn dotted_name(expr: &ast::Expr, depth: usize) -> Option<String> {
    if depth >= MAX_SETUP_PY_AST_DEPTH {
        return None;
    }

    match expr {
        ast::Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str().to_string()),
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            let base = dotted_name(value.as_ref(), depth + 1)?;
            Some(format!("{}.{}", base, attr.as_str()))
        }
        _ => None,
    }
}

fn resolve_module_alias(module: &str, aliases: &SetupAliases) -> String {
    if let Some(mapped) = aliases.module_aliases.get(module) {
        return mapped.clone();
    }

    let Some((base, rest)) = module.split_once('.') else {
        return module.to_string();
    };

    if let Some(mapped) = aliases.module_aliases.get(base) {
        return format!("{}.{}", mapped, rest);
    }

    module.to_string()
}

fn extract_setup_keywords(
    call_expr: &ast::Expr,
    evaluator: &mut LiteralEvaluator,
) -> SetupKeywords {
    let mut keywords = SetupKeywords::default();
    let ast::Expr::Call(ast::ExprCall { arguments, .. }) = call_expr else {
        return keywords;
    };

    for kw in arguments.keywords.iter() {
        if let Some(arg) = kw.arg.as_ref().map(ast::Identifier::as_str) {
            if let Some(value) = evaluator.evaluate_expr(&kw.value, 0) {
                keywords.set_field(arg, value);
            }
        } else if let Some(Value::Dict(dict)) = evaluator.evaluate_expr(&kw.value, 0) {
            for (key, value) in dict {
                keywords.set_field(&key, value);
            }
        }
    }

    keywords
}

fn build_setup_py_package_data(kw: &SetupKeywords) -> PackageData {
    let name = kw.name.clone().map(truncate_field);
    let version = kw.version.clone().map(truncate_field);
    let description = kw
        .description
        .clone()
        .or_else(|| kw.summary.clone())
        .map(truncate_field);
    let homepage_url = kw
        .url
        .clone()
        .or_else(|| kw.home_page.clone())
        .map(truncate_field);
    let author = kw.author.clone().map(truncate_field);
    let author_email = kw.author_email.clone();
    let maintainer = kw.maintainer.clone().map(truncate_field);
    let maintainer_email = kw.maintainer_email.clone();
    let license = kw.license.clone().map(truncate_field);
    let classifiers = kw.classifiers.clone().unwrap_or_default();

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party::person("author", author, author_email));
    }

    if maintainer.is_some() || maintainer_email.is_some() {
        parties.push(Party::person("maintainer", maintainer, maintainer_email));
    }

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(license.as_deref());
    let extracted_license_statement = license.clone();

    let dependencies = build_setup_py_dependencies(kw);
    let purl = build_setup_py_purl(name.as_deref(), version.as_deref());
    let mut project_urls = ProjectUrls {
        homepage_url: None,
        download_url: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        changelog_url: None,
    };
    let mut extra_data = HashMap::new();

    if let Some(parsed_project_urls) = kw.project_urls.as_ref().and_then(value_to_string_pairs) {
        apply_project_url_mappings(&parsed_project_urls, &mut project_urls, &mut extra_data);
    }

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        name,
        version,
        primary_language: Some("Python".to_string()),
        description,
        parties,
        homepage_url: homepage_url.or(project_urls.homepage_url),
        bug_tracking_url: project_urls.bug_tracking_url,
        code_view_url: project_urls.code_view_url,
        vcs_url: project_urls.vcs_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        is_private: has_private_classifier(&classifiers),
        extra_data,
        dependencies,
        datasource_id: Some(DatasourceId::PypiSetupPy),
        purl,
        ..Default::default()
    }
}

fn build_setup_py_dependencies(kw: &SetupKeywords) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    if let Some(reqs) = &kw.install_requires {
        dependencies.extend(build_setup_py_dependency_list(reqs, "install", false));
    }

    if let Some(reqs) = &kw.tests_require {
        dependencies.extend(build_setup_py_dependency_list(reqs, "test", true));
    }

    if let Some(extras) = &kw.extras_require {
        let mut extra_items: Vec<_> = extras.iter().collect();
        extra_items.sort_by_key(|(name, _)| *name);
        for (extra_name, extra_value) in extra_items {
            if let Some(reqs) = value_to_string_list(extra_value) {
                dependencies.extend(build_setup_py_dependency_list(
                    reqs.as_slice(),
                    extra_name,
                    true,
                ));
            }
        }
    }

    dependencies
}

fn build_setup_py_dependency_list(
    reqs: &[String],
    scope: &str,
    is_optional: bool,
) -> Vec<Dependency> {
    reqs.iter()
        .filter_map(|req| build_python_dependency(req, scope, is_optional, None))
        .collect()
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_string_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(value) => Some(vec![value.clone()]),
        Value::List(values) | Value::Tuple(values) => {
            let mut items = Vec::new();
            for item in values {
                items.push(value_to_string(item)?);
            }
            Some(items)
        }
        _ => None,
    }
}

fn value_to_string_pairs(dict: &HashMap<String, Value>) -> Option<Vec<(String, String)>> {
    let mut pairs: Vec<(String, String)> = dict
        .iter()
        .map(|(key, value)| Some((key.clone(), value_to_string(value)?)))
        .collect::<Option<Vec<_>>>()?;
    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    Some(pairs)
}

fn extract_from_setup_py_regex(content: &str) -> PackageData {
    let name = extract_setup_value(content, "name").map(truncate_field);
    let version = extract_setup_value(content, "version").map(truncate_field);
    let license_expression = extract_setup_value(content, "license").map(truncate_field);

    let (declared_license_expression, declared_license_expression_spdx, license_detections) =
        normalize_spdx_declared_license(license_expression.as_deref());
    let extracted_license_statement = license_expression.clone();

    let dependencies = extract_setup_py_dependencies(content);
    let homepage_url = extract_setup_value(content, "url").map(truncate_field);
    let purl = build_setup_py_purl(name.as_deref(), version.as_deref());

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        name,
        version,
        primary_language: Some("Python".to_string()),
        homepage_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        dependencies,
        datasource_id: Some(DatasourceId::PypiSetupPy),
        purl,
        ..Default::default()
    }
}

pub(super) fn package_data_to_resolved(pkg: &PackageData) -> crate::models::ResolvedPackage {
    crate::models::ResolvedPackage::from_package_data(pkg, PackageType::Pypi)
}
