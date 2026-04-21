// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;
use super::license_normalization::normalize_spdx_declared_license;
use super::utils::{MAX_ITERATION_COUNT, RecursionGuard, read_file_to_string, truncate_field};

pub struct MesonParser;

impl PackageParser for MesonParser {
    const PACKAGE_TYPE: PackageType = PackageType::Meson;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "meson.build")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read meson.build at {:?}: {}", path, error);
                return vec![default_package_data()];
            }
        };

        match parse_meson_build(&content) {
            Ok(package) => vec![package],
            Err(_) => vec![default_package_data()],
        }
    }
}

fn parse_meson_build(content: &str) -> Result<PackageData, String> {
    let sanitized = strip_comments(content)?;
    let statements = split_statements(&sanitized);

    let mut package = default_package_data();
    let mut extra_data = HashMap::new();
    let mut dependencies = Vec::new();
    let mut control_flow_depth = 0usize;

    for statement in statements.into_iter().take(MAX_ITERATION_COUNT) {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_block_closer(trimmed) {
            control_flow_depth = control_flow_depth.saturating_sub(1);
            continue;
        }

        if control_flow_depth > 0 {
            if is_block_opener(trimmed) {
                control_flow_depth += 1;
            }
            continue;
        }

        if is_block_opener(trimmed) {
            control_flow_depth += 1;
            continue;
        }

        let Ok(parsed) = parse_statement(trimmed) else {
            continue;
        };
        match parsed {
            Statement::Expr(expr) | Statement::Assignment(expr) => {
                handle_top_level_expr(&expr, &mut package, &mut extra_data, &mut dependencies)
            }
        }
    }

    package.dependencies = dependencies;
    package.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_project_purl(name, package.version.as_deref()));

    Ok(package)
}

fn handle_top_level_expr(
    expr: &Expr,
    package: &mut PackageData,
    extra_data: &mut HashMap<String, JsonValue>,
    dependencies: &mut Vec<Dependency>,
) {
    let Expr::Call(call) = expr else {
        return;
    };

    match call.name.as_str() {
        "project" if package.name.is_none() => apply_project_call(call, package, extra_data),
        "dependency" => dependencies.extend(extract_dependencies_from_call(call)),
        _ => {}
    }
}

fn apply_project_call(
    call: &CallExpr,
    package: &mut PackageData,
    extra_data: &mut HashMap<String, JsonValue>,
) {
    let Some(name) = call.positional.first().and_then(expr_as_string) else {
        return;
    };

    package.package_type = Some(PackageType::Meson);
    package.datasource_id = Some(DatasourceId::MesonBuild);
    package.name = Some(truncate_field(name.to_string()));

    let languages = call
        .positional
        .iter()
        .skip(1)
        .flat_map(extract_string_values)
        .collect::<Vec<_>>();
    if let Some(primary_language) = languages.first() {
        package.primary_language = Some(truncate_field(primary_language.clone()));
    }
    if !languages.is_empty() {
        extra_data.insert(
            "languages".to_string(),
            JsonValue::Array(languages.iter().cloned().map(JsonValue::String).collect()),
        );
    }

    if let Some(version) = call.keyword.get("version").and_then(expr_as_string) {
        package.version = Some(truncate_field(version.to_string()));
    }

    let licenses = call
        .keyword
        .get("license")
        .map(extract_string_values)
        .unwrap_or_default();
    if !licenses.is_empty() {
        package.extracted_license_statement = Some(truncate_field(licenses.join("\n")));
        if licenses.len() == 1 {
            let (declared_license_expression, declared_license_expression_spdx, license_detections) =
                normalize_spdx_declared_license(licenses.first().map(String::as_str));
            package.declared_license_expression = declared_license_expression;
            package.declared_license_expression_spdx = declared_license_expression_spdx;
            package.license_detections = license_detections;
        }
    }

    let license_files = call
        .keyword
        .get("license_files")
        .map(extract_string_values)
        .unwrap_or_default();
    if !license_files.is_empty() {
        extra_data.insert(
            "license_files".to_string(),
            JsonValue::Array(license_files.into_iter().map(JsonValue::String).collect()),
        );
    }

    if let Some(meson_version) = call.keyword.get("meson_version").and_then(expr_as_string) {
        extra_data.insert(
            "meson_version".to_string(),
            JsonValue::String(meson_version.to_string()),
        );
    }
}

fn extract_dependencies_from_call(call: &CallExpr) -> Vec<Dependency> {
    let dependency_names = call
        .positional
        .iter()
        .filter_map(expr_as_string)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if dependency_names.is_empty() {
        return Vec::new();
    }

    let extracted_requirement = call.keyword.get("version").map(|expr| {
        extract_string_values(expr)
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let required = call.keyword.get("required").and_then(expr_as_bool);
    let native = call.keyword.get("native").and_then(expr_as_bool);

    dependency_names
        .into_iter()
        .take(MAX_ITERATION_COUNT)
        .map(|name| {
            let mut extra_data = HashMap::new();

            if let Some(requirement) = extracted_requirement
                .as_ref()
                .filter(|value| !value.is_empty())
            {
                extra_data.insert(
                    "version".to_string(),
                    JsonValue::String(requirement.clone()),
                );
            }
            if let Some(required) = required {
                extra_data.insert("required".to_string(), JsonValue::Bool(required));
            }
            if let Some(method) = call.keyword.get("method").and_then(expr_as_string) {
                extra_data.insert("method".to_string(), JsonValue::String(method.to_string()));
            }
            if let Some(native) = native {
                extra_data.insert("native".to_string(), JsonValue::Bool(native));
            }

            let modules = call
                .keyword
                .get("modules")
                .map(extract_string_values)
                .unwrap_or_default();
            if !modules.is_empty() {
                extra_data.insert(
                    "modules".to_string(),
                    JsonValue::Array(modules.into_iter().map(JsonValue::String).collect()),
                );
            }

            let fallback = call
                .keyword
                .get("fallback")
                .map(extract_string_values)
                .unwrap_or_default();
            if !fallback.is_empty() {
                extra_data.insert(
                    "fallback".to_string(),
                    JsonValue::Array(fallback.into_iter().map(JsonValue::String).collect()),
                );
            }

            Dependency {
                purl: build_dependency_purl(&name),
                extracted_requirement: extracted_requirement
                    .clone()
                    .filter(|value| !value.is_empty())
                    .map(truncate_field),
                scope: Some("dependencies".to_string()),
                is_runtime: Some(native != Some(true)),
                is_optional: Some(required == Some(false)),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: (!extra_data.is_empty()).then_some(extra_data),
            }
        })
        .collect()
}

fn build_project_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Meson.as_str(), name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(truncate_field(purl.to_string()))
}

fn build_dependency_purl(name: &str) -> Option<String> {
    let mut purl = PackageUrl::new("generic", name).ok()?;
    purl.with_namespace("meson").ok()?;
    Some(truncate_field(purl.to_string()))
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Meson),
        datasource_id: Some(DatasourceId::MesonBuild),
        ..Default::default()
    }
}

fn is_block_opener(statement: &str) -> bool {
    matches!(
        statement.split_whitespace().next(),
        Some("if") | Some("foreach")
    )
}

fn is_block_closer(statement: &str) -> bool {
    matches!(statement.trim(), "endif" | "endforeach")
}

fn strip_comments(input: &str) -> Result<String, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut string_delimiter = '\0';
    let mut escaped = false;
    let mut chars_processed = 0usize;

    while index < chars.len() {
        chars_processed += 1;
        if chars_processed > MAX_ITERATION_COUNT {
            break;
        }
        let ch = chars[index];

        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == string_delimiter {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if matches!(ch, '\'' | '"') {
            in_string = true;
            string_delimiter = ch;
            output.push(ch);
            index += 1;
            continue;
        }

        if ch == '#' {
            index += 1;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }

        output.push(ch);
        index += 1;
    }

    if in_string {
        return Err("unterminated string literal".to_string());
    }

    Ok(output)
}

fn split_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut in_string = false;
    let mut string_delimiter = '\0';
    let mut escaped = false;
    let mut chars_processed = 0usize;

    for ch in input.chars() {
        chars_processed += 1;
        if chars_processed > MAX_ITERATION_COUNT {
            break;
        }
        current.push(ch);

        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == string_delimiter {
                in_string = false;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                in_string = true;
                string_delimiter = ch;
            }
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '\n' if paren_depth == 0 && bracket_depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => {}
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }

    statements
}

#[derive(Debug, Clone)]
enum Statement {
    Expr(Expr),
    Assignment(Expr),
}

#[derive(Debug, Clone)]
enum Expr {
    String(String),
    Bool(bool),
    Array(Vec<Expr>),
    Identifier,
    Call(CallExpr),
}

#[derive(Debug, Clone)]
struct CallExpr {
    name: String,
    positional: Vec<Expr>,
    keyword: HashMap<String, Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Str(String),
    Bool(bool),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Equal,
}

fn parse_statement(statement: &str) -> Result<Statement, String> {
    let tokens = tokenize(statement)?;
    if tokens.is_empty() {
        return Err("empty statement".to_string());
    }

    if let [Token::Ident(name), Token::Equal, rest @ ..] = tokens.as_slice() {
        let mut parser = Parser::new(rest);
        let expr = parser.parse_expr()?;
        parser.expect_end()?;
        let _ = name;
        return Ok(Statement::Assignment(expr));
    }

    let mut parser = Parser::new(&tokens);
    let expr = parser.parse_expr()?;
    parser.expect_end()?;
    Ok(Statement::Expr(expr))
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if tokens.len() >= MAX_ITERATION_COUNT {
            break;
        }
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        match ch {
            '(' => {
                tokens.push(Token::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                index += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                index += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                index += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                index += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            '=' => {
                tokens.push(Token::Equal);
                index += 1;
            }
            '\'' | '"' => {
                let delimiter = ch;
                index += 1;
                let start = index;
                let mut escaped = false;
                while index < chars.len() {
                    let current = chars[index];
                    if escaped {
                        escaped = false;
                    } else if current == '\\' {
                        escaped = true;
                    } else if current == delimiter {
                        break;
                    }
                    index += 1;
                }

                if index >= chars.len() {
                    return Err("unterminated string token".to_string());
                }

                let value: String = chars[start..index].iter().collect();
                tokens.push(Token::Str(value));
                index += 1;
            }
            _ if is_ident_start(ch) => {
                let start = index;
                index += 1;
                while index < chars.len() && is_ident_continue(chars[index]) {
                    index += 1;
                }
                let ident: String = chars[start..index].iter().collect();
                match ident.as_str() {
                    "true" => tokens.push(Token::Bool(true)),
                    "false" => tokens.push(Token::Bool(false)),
                    _ => tokens.push(Token::Ident(ident)),
                }
            }
            _ => {
                return Err(format!("unsupported token '{}'", ch));
            }
        }
    }

    Ok(tokens)
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    guard: RecursionGuard<()>,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            index: 0,
            guard: RecursionGuard::depth_only(),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        if self.guard.descend() {
            return Err("recursion depth exceeded".to_string());
        }
        let result = match self.peek() {
            Some(Token::Str(value)) => {
                self.index += 1;
                Ok(Expr::String(value.clone()))
            }
            Some(Token::Bool(value)) => {
                self.index += 1;
                Ok(Expr::Bool(*value))
            }
            Some(Token::LBracket) => self.parse_array(),
            Some(Token::Ident(_)) => self.parse_identifier_or_call(),
            Some(token) => Err(format!("unexpected token {:?}", token)),
            None => Err("unexpected end of input".to_string()),
        };
        self.guard.ascend();
        result
    }

    fn parse_array(&mut self) -> Result<Expr, String> {
        self.expect(Token::LBracket)?;
        let mut values = Vec::new();
        let mut element_count = 0usize;
        while !matches!(self.peek(), Some(Token::RBracket)) {
            element_count += 1;
            if element_count > MAX_ITERATION_COUNT {
                break;
            }
            let expr = self.parse_expr()?;
            values.push(expr);
            if matches!(self.peek(), Some(Token::Comma)) {
                self.index += 1;
            } else if !matches!(self.peek(), Some(Token::RBracket)) {
                return Err("expected ',' or ']' in array".to_string());
            }
        }
        self.expect(Token::RBracket)?;
        Ok(Expr::Array(values))
    }

    fn parse_identifier_or_call(&mut self) -> Result<Expr, String> {
        let Token::Ident(name) = self
            .next()
            .cloned()
            .ok_or_else(|| "expected identifier".to_string())?
        else {
            return Err("expected identifier".to_string());
        };

        if !matches!(self.peek(), Some(Token::LParen)) {
            let _ = name;
            return Ok(Expr::Identifier);
        }

        self.expect(Token::LParen)?;
        let mut positional = Vec::new();
        let mut keyword = HashMap::new();
        let mut arg_count = 0usize;

        while !matches!(self.peek(), Some(Token::RParen)) {
            arg_count += 1;
            if arg_count > MAX_ITERATION_COUNT {
                break;
            }
            if let (Some(Token::Ident(arg_name)), Some(Token::Colon)) =
                (self.peek(), self.peek_n(1))
            {
                let arg_name = arg_name.clone();
                self.index += 2;
                let value = self.parse_expr()?;
                keyword.insert(arg_name, value);
            } else {
                let expr = self.parse_expr()?;
                positional.push(expr);
            }

            if matches!(self.peek(), Some(Token::Comma)) {
                self.index += 1;
            } else if !matches!(self.peek(), Some(Token::RParen)) {
                return Err("expected ',' or ')' in call".to_string());
            }
        }

        self.expect(Token::RParen)?;
        Ok(Expr::Call(CallExpr {
            name,
            positional,
            keyword,
        }))
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        match self.next() {
            Some(token) if *token == expected => Ok(()),
            Some(token) => Err(format!("expected {:?}, found {:?}", expected, token)),
            None => Err(format!("expected {:?}, found end of input", expected)),
        }
    }

    fn expect_end(&self) -> Result<(), String> {
        if self.index == self.tokens.len() {
            Ok(())
        } else {
            Err(format!(
                "unexpected trailing tokens: {:?}",
                &self.tokens[self.index..]
            ))
        }
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.index)
    }

    fn peek_n(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.index + offset)
    }

    fn next(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.index);
        if token.is_some() {
            self.index += 1;
        }
        token
    }
}

fn expr_as_string(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn expr_as_bool(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Bool(value) => Some(*value),
        _ => None,
    }
}

fn extract_string_values(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::String(value) => vec![value.clone()],
        Expr::Array(values) => values
            .iter()
            .filter_map(expr_as_string)
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

crate::register_parser!(
    "Meson meson.build manifest",
    &["**/meson.build"],
    "meson",
    "",
    Some("https://mesonbuild.com/Syntax.html"),
);
