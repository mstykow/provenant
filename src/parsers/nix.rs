// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, RecursionGuard, read_file_to_string, truncate_field,
};

use super::PackageParser;

pub struct NixFlakeLockParser;

impl PackageParser for NixFlakeLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nix;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "flake.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read flake.lock at {:?}: {}", path, error);
                return vec![default_flake_lock_package_data()];
            }
        };

        let json: JsonValue = match serde_json::from_str(&content) {
            Ok(json) => json,
            Err(error) => {
                warn!("Failed to parse flake.lock at {:?}: {}", path, error);
                return vec![default_flake_lock_package_data()];
            }
        };

        match parse_flake_lock(path, &json) {
            Ok(package) => vec![package],
            Err(error) => {
                warn!("Failed to interpret flake.lock at {:?}: {}", path, error);
                vec![default_flake_lock_package_data()]
            }
        }
    }
}

pub struct NixFlakeParser;

impl PackageParser for NixFlakeParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nix;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "flake.nix")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read flake.nix at {:?}: {}", path, error);
                return vec![default_flake_package_data()];
            }
        };

        match parse_flake_nix(path, &content) {
            Ok(package) => vec![package],
            Err(_) => vec![default_flake_package_data()],
        }
    }
}

pub struct NixDefaultParser;

impl PackageParser for NixDefaultParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nix;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "default.nix")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read default.nix at {:?}: {}", path, error);
                return vec![default_default_nix_package_data()];
            }
        };

        match parse_default_nix(path, &content) {
            Ok(package) => vec![package],
            Err(_) => vec![default_default_nix_package_data()],
        }
    }
}

#[derive(Clone, Debug)]
enum Expr {
    AttrSet(Vec<(Vec<String>, Expr)>),
    List(Vec<Expr>),
    String(String),
    Symbol(String),
    Application(Vec<Expr>),
    Let {
        bindings: Vec<(Vec<String>, Expr)>,
        body: Box<Expr>,
    },
    Select {
        target: Box<Expr>,
        path: Vec<String>,
    },
}

type NixAttrEntries = [(Vec<String>, Expr)];
type NixAttrEntriesRef<'a> = &'a NixAttrEntries;
type NixScopeStack<'a> = Vec<NixAttrEntriesRef<'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Equals,
    Semicolon,
    Colon,
    Dot,
    Comma,
    String(String),
    Ident(String),
}

#[derive(Default)]
struct FlakeInputInfo {
    requirement: Option<String>,
    follows: Vec<String>,
    flake: Option<bool>,
}

struct Lexer {
    chars: Vec<char>,
    index: usize,
}

impl Lexer {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            index: 0,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek() {
            if tokens.len() >= MAX_ITERATION_COUNT {
                warn!("Lexer exceeded MAX_ITERATION_COUNT token limit");
                break;
            }

            if ch.is_whitespace() {
                self.index += 1;
                continue;
            }

            if ch == '#' {
                self.skip_line_comment();
                continue;
            }

            if ch == '/' && self.peek_n(1) == Some('*') {
                self.skip_block_comment()?;
                continue;
            }

            match ch {
                '$' if self.peek_n(1) == Some('{') => {
                    tokens.push(Token::Ident(self.read_interpolation_literal()?));
                }
                '.' if self.peek_n(1) == Some('/') => {
                    tokens.push(Token::Ident(self.read_path_literal()?));
                }
                '.' if self.peek_n(1) == Some('.') && self.peek_n(2) == Some('/') => {
                    tokens.push(Token::Ident(self.read_path_literal()?));
                }
                '{' => {
                    self.index += 1;
                    tokens.push(Token::LBrace);
                }
                '}' => {
                    self.index += 1;
                    tokens.push(Token::RBrace);
                }
                '[' => {
                    self.index += 1;
                    tokens.push(Token::LBracket);
                }
                ']' => {
                    self.index += 1;
                    tokens.push(Token::RBracket);
                }
                '(' => {
                    self.index += 1;
                    tokens.push(Token::LParen);
                }
                ')' => {
                    self.index += 1;
                    tokens.push(Token::RParen);
                }
                '=' => {
                    self.index += 1;
                    tokens.push(Token::Equals);
                }
                ';' => {
                    self.index += 1;
                    tokens.push(Token::Semicolon);
                }
                ':' => {
                    self.index += 1;
                    tokens.push(Token::Colon);
                }
                '.' => {
                    self.index += 1;
                    tokens.push(Token::Dot);
                }
                ',' => {
                    self.index += 1;
                    tokens.push(Token::Comma);
                }
                '"' => tokens.push(Token::String(self.read_double_quoted_string()?)),
                '\'' if self.peek_n(1) == Some('\'') => {
                    tokens.push(Token::String(self.read_indented_string()?));
                }
                _ => tokens.push(Token::Ident(self.read_ident()?)),
            }
        }

        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_n(&self, offset: usize) -> Option<char> {
        self.chars.get(self.index + offset).copied()
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            self.index += 1;
            if ch == '\n' {
                break;
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), String> {
        self.index += 2;
        while let Some(ch) = self.peek() {
            if ch == '*' && self.peek_n(1) == Some('/') {
                self.index += 2;
                return Ok(());
            }
            self.index += 1;
        }
        Err("unterminated block comment".to_string())
    }

    fn read_double_quoted_string(&mut self) -> Result<String, String> {
        self.index += 1;
        let mut result = String::new();
        let mut escaped = false;

        while let Some(ch) = self.peek() {
            self.index += 1;
            if escaped {
                result.push(match ch {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escaped = false;
                continue;
            }

            if ch == '\\' {
                escaped = true;
                continue;
            }

            if ch == '$' && self.peek() == Some('{') {
                result.push(ch);
                result.push('{');
                self.index += 1;
                let mut interpolation_depth = 1usize;

                while let Some(inner) = self.peek() {
                    self.index += 1;
                    result.push(inner);

                    match inner {
                        '{' => interpolation_depth += 1,
                        '}' => {
                            interpolation_depth = interpolation_depth.saturating_sub(1);
                            if interpolation_depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                if interpolation_depth != 0 {
                    return Err("unterminated string interpolation".to_string());
                }

                continue;
            }

            if ch == '"' {
                return Ok(result);
            }

            result.push(ch);
        }

        Err("unterminated string".to_string())
    }

    fn read_path_literal(&mut self) -> Result<String, String> {
        let start = self.index;

        while let Some(ch) = self.peek() {
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '{' | '}' | '[' | ']' | '(' | ')' | '=' | ';' | ':' | ',' | '"'
                )
                || (ch == '\'' && self.peek_n(1) == Some('\''))
                || ch == '#'
            {
                break;
            }

            if ch == '/' && self.peek_n(1) == Some('*') {
                break;
            }

            self.index += 1;
        }

        if self.index == start {
            return Err("unexpected token".to_string());
        }

        Ok(self.chars[start..self.index].iter().collect())
    }

    fn read_interpolation_literal(&mut self) -> Result<String, String> {
        let start = self.index;
        self.index += 2;
        let mut depth = 1usize;

        while let Some(ch) = self.peek() {
            self.index += 1;

            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Ok(self.chars[start..self.index].iter().collect());
                    }
                }
                _ => {}
            }
        }

        Err("unterminated interpolation literal".to_string())
    }

    fn read_indented_string(&mut self) -> Result<String, String> {
        self.index += 2;
        let mut result = String::new();

        while let Some(ch) = self.peek() {
            if ch == '\'' && self.peek_n(1) == Some('\'') {
                self.index += 2;
                return Ok(result);
            }
            result.push(ch);
            self.index += 1;
        }

        Err("unterminated indented string".to_string())
    }

    fn read_ident(&mut self) -> Result<String, String> {
        let start = self.index;

        while let Some(ch) = self.peek() {
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '{' | '}' | '[' | ']' | '(' | ')' | '=' | ';' | ':' | ',' | '.' | '"'
                )
                || (ch == '\'' && self.peek_n(1) == Some('\''))
                || ch == '#'
            {
                break;
            }

            if ch == '/' && self.peek_n(1) == Some('*') {
                break;
            }

            self.index += 1;
        }

        if self.index == start {
            return Err("unexpected token".to_string());
        }

        Ok(self.chars[start..self.index].iter().collect())
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    guard: RecursionGuard<()>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            index: 0,
            guard: RecursionGuard::depth_only(),
        }
    }

    fn parse(mut self) -> Result<Expr, String> {
        let expr = self.parse_expr()?;
        if self.peek().is_some() {
            return Err("unexpected trailing tokens".to_string());
        }
        Ok(expr)
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        if self.guard.descend() {
            return Err("recursion depth exceeded".to_string());
        }

        if self.peek() == Some(&Token::LBrace) && self.looks_like_lambda_binder_set()? {
            self.skip_lambda_binder_set()?;
            self.expect(&Token::Colon)?;
            let result = self.parse_expr();
            self.guard.ascend();
            return result;
        }

        if self.looks_like_prefixed_lambda_binder_set()? {
            self.index += 1;
            self.skip_lambda_binder_set()?;
            self.expect(&Token::Colon)?;
            let result = self.parse_expr();
            self.guard.ascend();
            return result;
        }

        let first = self.parse_term()?;
        if self.consume(&Token::Colon) {
            let result = self.parse_expr();
            self.guard.ascend();
            return result;
        }

        let mut terms = vec![first];
        while self.can_start_term() {
            terms.push(self.parse_term()?);
        }

        let expr = if terms.len() == 1 {
            terms
                .into_iter()
                .next()
                .unwrap_or_else(|| Expr::Symbol(String::new()))
        } else {
            Expr::Application(terms)
        };

        let result = self.parse_postfix(expr);
        self.guard.ascend();
        result
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, String> {
        while self.consume(&Token::Dot) {
            let mut path = vec![self.take_attr_key()?];
            while self.consume(&Token::Dot) {
                path.push(self.take_attr_key()?);
            }
            expr = Expr::Select {
                target: Box::new(expr),
                path,
            };
        }

        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::Ident(keyword)) if keyword == "let" => self.parse_let_in_expr(),
            Some(Token::Ident(keyword)) if keyword == "with" => {
                self.index += 1;
                let _ = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                self.parse_expr()
            }
            Some(Token::Ident(keyword)) if keyword == "rec" => {
                if matches!(self.peek_n(1), Some(Token::LBrace)) {
                    self.index += 1;
                    self.parse_attrset()
                } else {
                    self.parse_symbol()
                }
            }
            Some(Token::LBrace) => self.parse_attrset(),
            Some(Token::LBracket) => self.parse_list(),
            Some(Token::LParen) => {
                self.index += 1;
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Some(Token::String(_)) => self.parse_string(),
            Some(Token::Ident(_)) => self.parse_symbol(),
            _ => Err("expected expression".to_string()),
        }
    }

    fn parse_let_in_expr(&mut self) -> Result<Expr, String> {
        self.take_exact_ident("let")?;
        let mut bindings = Vec::new();

        while !matches!(self.peek(), Some(Token::Ident(keyword)) if keyword == "in") {
            if self.peek().is_none() {
                return Err("unterminated let expression".to_string());
            }

            if bindings.len() >= MAX_ITERATION_COUNT {
                warn!("parse_let_in_expr exceeded MAX_ITERATION_COUNT bindings limit");
                break;
            }

            if matches!(self.peek(), Some(Token::Ident(keyword)) if keyword == "inherit") {
                bindings.extend(self.parse_inherit_entries()?);
                continue;
            }

            let key = self.parse_attr_path()?;
            self.expect(&Token::Equals)?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            bindings.push((key, value));
        }

        self.take_exact_ident("in")?;
        let body = self.parse_expr()?;
        Ok(Expr::Let {
            bindings,
            body: Box::new(body),
        })
    }

    fn parse_attrset(&mut self) -> Result<Expr, String> {
        self.expect(&Token::LBrace)?;
        let mut entries = Vec::new();

        loop {
            if self.consume(&Token::RBrace) {
                return Ok(Expr::AttrSet(entries));
            }

            if self.peek().is_none() {
                return Err("unterminated attribute set".to_string());
            }

            if entries.len() >= MAX_ITERATION_COUNT {
                warn!("parse_attrset exceeded MAX_ITERATION_COUNT entries limit");
                break;
            }

            if matches!(self.peek(), Some(Token::Ident(keyword)) if keyword == "inherit") {
                entries.extend(self.parse_inherit_entries()?);
                continue;
            }

            let key = self.parse_attr_path()?;
            self.expect(&Token::Equals)?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            entries.push((key, value));
        }

        Ok(Expr::AttrSet(entries))
    }

    fn parse_attr_path(&mut self) -> Result<Vec<String>, String> {
        let mut path = vec![self.take_attr_key()?];
        while self.consume(&Token::Dot) {
            path.push(self.take_attr_key()?);
        }
        Ok(path)
    }

    fn parse_inherit_entries(&mut self) -> Result<Vec<(Vec<String>, Expr)>, String> {
        self.take_exact_ident("inherit")?;

        let inherit_from = if self.consume(&Token::LParen) {
            let expr = self.parse_expr()?;
            self.expect(&Token::RParen)?;
            Some(expr)
        } else {
            None
        };

        let mut entries = Vec::new();
        while !self.consume(&Token::Semicolon) {
            if self.peek().is_none() {
                return Err("unterminated inherit statement".to_string());
            }

            if entries.len() >= MAX_ITERATION_COUNT {
                warn!("parse_inherit_entries exceeded MAX_ITERATION_COUNT entries limit");
                break;
            }

            let name = self.take_attr_key()?;
            let value = match &inherit_from {
                Some(source) => Expr::Select {
                    target: Box::new(source.clone()),
                    path: vec![name.clone()],
                },
                None => Expr::Symbol(name.clone()),
            };
            entries.push((vec![name], value));
        }

        Ok(entries)
    }

    fn parse_list(&mut self) -> Result<Expr, String> {
        self.expect(&Token::LBracket)?;
        let mut items = Vec::new();
        while !self.consume(&Token::RBracket) {
            if self.peek().is_none() {
                return Err("unterminated list".to_string());
            }

            if items.len() >= MAX_ITERATION_COUNT {
                warn!("parse_list exceeded MAX_ITERATION_COUNT items limit");
                break;
            }

            items.push(self.parse_expr()?);
        }
        Ok(Expr::List(items))
    }

    fn parse_string(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::String(value)) => Ok(Expr::String(value)),
            _ => Err("expected string".to_string()),
        }
    }

    fn parse_symbol(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.take_ident()?];
        while self.consume(&Token::Dot) {
            parts.push(self.take_ident()?);
        }
        Ok(Expr::Symbol(parts.join(".")))
    }

    fn take_ident(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Token::Ident(value)) => Ok(value),
            _ => Err("expected identifier".to_string()),
        }
    }

    fn take_exact_ident(&mut self, expected: &str) -> Result<(), String> {
        match self.next() {
            Some(Token::Ident(value)) if value == expected => Ok(()),
            _ => Err(format!("expected {expected}")),
        }
    }

    fn take_attr_key(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Token::Ident(value)) | Some(Token::String(value)) => Ok(value),
            _ => Err("expected attribute key".to_string()),
        }
    }

    fn can_start_term(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::LBrace)
                | Some(Token::LBracket)
                | Some(Token::LParen)
                | Some(Token::String(_))
                | Some(Token::Ident(_))
        )
    }

    fn looks_like_lambda_binder_set(&self) -> Result<bool, String> {
        if self.peek() != Some(&Token::LBrace) {
            return Ok(false);
        }

        self.looks_like_lambda_binder_set_from(self.index)
    }

    fn looks_like_prefixed_lambda_binder_set(&self) -> Result<bool, String> {
        match (self.peek(), self.peek_n(1)) {
            (Some(Token::Ident(prefix)), Some(Token::LBrace)) if prefix.ends_with('@') => {
                self.looks_like_lambda_binder_set_from(self.index + 1)
            }
            _ => Ok(false),
        }
    }

    fn looks_like_lambda_binder_set_from(&self, start_index: usize) -> Result<bool, String> {
        if self.tokens.get(start_index) != Some(&Token::LBrace) {
            return Ok(false);
        }

        let mut depth = 0usize;
        let mut index = start_index;

        while let Some(token) = self.tokens.get(index) {
            match token {
                Token::LBrace => depth += 1,
                Token::RBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Ok(matches!(self.tokens.get(index + 1), Some(Token::Colon)));
                    }
                }
                Token::Equals | Token::Semicolon if depth == 1 => return Ok(false),
                _ => {}
            }

            index += 1;
        }

        Err("unterminated lambda binder set".to_string())
    }

    fn skip_lambda_binder_set(&mut self) -> Result<(), String> {
        self.expect(&Token::LBrace)?;
        let mut depth = 1usize;

        while depth > 0 {
            match self.next() {
                Some(Token::LBrace) => depth += 1,
                Some(Token::RBrace) => depth = depth.saturating_sub(1),
                Some(_) => {}
                None => return Err("unterminated lambda binder set".to_string()),
            }
        }

        Ok(())
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!("expected {:?}", expected))
        }
    }

    fn consume(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn peek_n(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.index + offset)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }
}

fn parse_flake_nix(path: &Path, content: &str) -> Result<PackageData, String> {
    let expr = parse_nix_expr(content)?;
    let scopes = Vec::new();
    let (root, scopes) =
        root_attrset_with_scopes(&expr, &scopes, &mut RecursionGuard::depth_only())
            .ok_or_else(|| "flake.nix root was not an attribute set".to_string())?;

    let mut package = default_flake_package_data();
    package.name = fallback_name(path).map(truncate_field);
    package.description =
        find_string_attr_with_scopes(root, &["description"], &scopes).map(truncate_field);
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, None));
    package.dependencies = build_flake_dependencies(root, &scopes);

    Ok(package)
}

fn parse_default_nix(path: &Path, content: &str) -> Result<PackageData, String> {
    match parse_nix_expr(content) {
        Ok(expr) => extract_default_nix_package(path, &expr, &Vec::new(), 0)
            .or_else(|_| extract_flake_compat_default_package_from_content(path, content)),
        Err(parse_error) => extract_flake_compat_default_package_from_content(path, content)
            .map_err(|_| parse_error),
    }
}

fn parse_flake_lock(path: &Path, json: &JsonValue) -> Result<PackageData, String> {
    let version = json
        .get("version")
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| "flake.lock missing integer version".to_string())?;
    let root = json
        .get("root")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| "flake.lock missing root".to_string())?;
    let nodes = json
        .get("nodes")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| "flake.lock missing nodes".to_string())?;
    let root_node = nodes
        .get(root)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| "flake.lock root node missing".to_string())?;
    let root_inputs = root_node
        .get("inputs")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| "flake.lock root node missing inputs".to_string())?;

    let mut package = default_flake_lock_package_data();
    package.name = fallback_name(path).map(truncate_field);
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, None));

    let mut extra_data = HashMap::new();
    extra_data.insert("lock_version".to_string(), JsonValue::from(version));
    extra_data.insert("root".to_string(), JsonValue::String(root.to_string()));
    package.extra_data = Some(extra_data);

    package.dependencies = root_inputs
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|(input_name, node_ref)| build_lock_dependency(input_name, node_ref, nodes))
        .collect();
    package
        .dependencies
        .sort_by(|left, right| left.purl.cmp(&right.purl));

    Ok(package)
}

fn build_lock_dependency(
    input_name: &str,
    node_ref: &JsonValue,
    nodes: &serde_json::Map<String, JsonValue>,
) -> Option<Dependency> {
    let node_id = node_ref.as_str()?;
    let node = nodes.get(node_id)?.as_object()?;
    let locked = node.get("locked").and_then(JsonValue::as_object)?;
    let revision = locked.get("rev").and_then(JsonValue::as_str);

    let mut extra_data = HashMap::new();
    for key in [
        "type",
        "owner",
        "repo",
        "narHash",
        "lastModified",
        "revCount",
        "url",
        "path",
        "dir",
        "host",
    ] {
        if let Some(value) = locked.get(key) {
            extra_data.insert(normalize_extra_key(key), value.clone());
        }
    }
    if let Some(value) = node.get("flake").and_then(JsonValue::as_bool) {
        extra_data.insert("flake".to_string(), JsonValue::Bool(value));
    }
    if let Some(original) = node.get("original").and_then(JsonValue::as_object) {
        if let Some(value) = original.get("type") {
            extra_data.insert("original_type".to_string(), value.clone());
        }
        if let Some(value) = original.get("id") {
            extra_data.insert("original_id".to_string(), value.clone());
        }
        if let Some(value) = original.get("ref") {
            extra_data.insert("original_ref".to_string(), value.clone());
        }
    }

    Some(Dependency {
        purl: build_nix_purl(input_name, revision),
        extracted_requirement: build_locked_requirement(locked, node.get("original"))
            .map(truncate_field),
        scope: Some("inputs".to_string()),
        is_runtime: Some(false),
        is_optional: Some(false),
        is_pinned: Some(revision.is_some()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn build_locked_requirement(
    locked: &serde_json::Map<String, JsonValue>,
    original: Option<&JsonValue>,
) -> Option<String> {
    let source_type = locked.get("type").and_then(JsonValue::as_str).or_else(|| {
        original
            .and_then(|value| value.get("type"))
            .and_then(JsonValue::as_str)
    });

    match source_type {
        Some("github") => {
            let owner = locked.get("owner").and_then(JsonValue::as_str)?;
            let repo = locked.get("repo").and_then(JsonValue::as_str)?;
            Some(format!("github:{owner}/{repo}"))
        }
        Some("indirect") => original
            .and_then(|value| value.get("id"))
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned),
        _ => locked
            .get("url")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned),
    }
}

fn normalize_extra_key(key: &str) -> String {
    match key {
        "type" => "source_type".to_string(),
        "narHash" => "nar_hash".to_string(),
        "lastModified" => "last_modified".to_string(),
        "revCount" => "rev_count".to_string(),
        other => other.to_string(),
    }
}

fn build_flake_dependencies(
    root: &[(Vec<String>, Expr)],
    scopes: &[&[(Vec<String>, Expr)]],
) -> Vec<Dependency> {
    let mut inputs: HashMap<String, FlakeInputInfo> = HashMap::new();

    for (path, expr) in root {
        if path.first().map(String::as_str) != Some("inputs") {
            continue;
        }

        if path.len() == 1 {
            if let Some(entries) = attrset_entries(expr) {
                collect_input_entries(entries, scopes, &mut inputs, None);
            }
            continue;
        }

        collect_input_path(&path[1..], expr, scopes, &mut inputs);
    }

    let mut dependencies = inputs
        .into_iter()
        .map(|(name, info)| {
            let mut extra_data = HashMap::new();
            if info.follows.len() == 1 {
                extra_data.insert(
                    "follows".to_string(),
                    JsonValue::String(info.follows[0].clone()),
                );
            } else if !info.follows.is_empty() {
                extra_data.insert(
                    "follows".to_string(),
                    JsonValue::Array(
                        info.follows
                            .iter()
                            .cloned()
                            .map(JsonValue::String)
                            .collect(),
                    ),
                );
            }
            if let Some(flake) = info.flake {
                extra_data.insert("flake".to_string(), JsonValue::Bool(flake));
            }

            Dependency {
                purl: build_nix_purl(&name, None),
                extracted_requirement: info.requirement.map(truncate_field),
                scope: Some("inputs".to_string()),
                is_runtime: Some(false),
                is_optional: Some(false),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: (!extra_data.is_empty()).then_some(extra_data),
            }
        })
        .collect::<Vec<_>>();

    dependencies.sort_by(|left, right| left.purl.cmp(&right.purl));
    dependencies
}

fn collect_input_entries(
    entries: &[(Vec<String>, Expr)],
    scopes: &[&[(Vec<String>, Expr)]],
    inputs: &mut HashMap<String, FlakeInputInfo>,
    current_input: Option<&str>,
) {
    for (path, expr) in entries {
        if let Some(input_name) = current_input {
            apply_input_field(
                inputs.entry(input_name.to_string()).or_default(),
                path,
                expr,
                scopes,
            );
            continue;
        }

        collect_input_path(path, expr, scopes, inputs);
    }
}

fn collect_input_path(
    path: &[String],
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    inputs: &mut HashMap<String, FlakeInputInfo>,
) {
    let Some(input_name) = path.first() else {
        return;
    };

    if path.len() == 1 {
        match expr {
            Expr::AttrSet(entries) => {
                collect_input_entries(entries, scopes, inputs, Some(input_name))
            }
            Expr::String(value) => {
                inputs.entry(input_name.clone()).or_default().requirement = Some(value.clone())
            }
            Expr::Symbol(value) => {
                inputs.entry(input_name.clone()).or_default().requirement =
                    expr_as_string_with_scopes(
                        &Expr::Symbol(value.clone()),
                        scopes,
                        &mut RecursionGuard::depth_only(),
                    )
            }
            _ => {}
        }
        return;
    }

    apply_input_field(
        inputs.entry(input_name.clone()).or_default(),
        &path[1..],
        expr,
        scopes,
    );
}

fn apply_input_field(
    info: &mut FlakeInputInfo,
    path: &[String],
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
) {
    if path == ["url"] {
        info.requirement =
            expr_as_string_with_scopes(expr, scopes, &mut RecursionGuard::depth_only());
        return;
    }

    if path == ["flake"] {
        info.flake = expr_as_bool_with_scopes(expr, scopes, &mut RecursionGuard::depth_only());
        return;
    }

    if path.len() == 3
        && path[0] == "inputs"
        && path[2] == "follows"
        && let Some(value) =
            expr_as_string_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
    {
        info.follows.push(value);
    }
}

fn build_list_dependencies(
    entries: &[(Vec<String>, Expr)],
    field_name: &str,
    runtime: bool,
    scopes: &[&[(Vec<String>, Expr)]],
) -> Vec<Dependency> {
    let Some(expr) = find_attr(entries, &[field_name], &mut RecursionGuard::depth_only()) else {
        return Vec::new();
    };
    let Some(items) = list_items_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
    else {
        return Vec::new();
    };

    items
        .iter()
        .take(MAX_ITERATION_COUNT)
        .flat_map(|expr| {
            expr_to_dependency_symbols_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
        })
        .filter_map(|symbol| {
            let name = symbol.rsplit('.').next()?.to_string();
            Some(Dependency {
                purl: build_nix_purl(&name, None),
                extracted_requirement: None,
                scope: Some(field_name.to_string()),
                is_runtime: Some(runtime),
                is_optional: Some(false),
                is_pinned: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
            })
        })
        .collect()
}

fn expr_to_dependency_symbols_with_scopes(
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    guard: &mut RecursionGuard<()>,
) -> Vec<String> {
    if guard.descend() {
        warn!("expr_to_dependency_symbols_with_scopes exceeded MAX_RECURSION_DEPTH");
        return Vec::new();
    }

    let result = match expr {
        Expr::Symbol(symbol) => resolve_symbol(symbol, scopes, &mut RecursionGuard::depth_only())
            .map(|resolved| expr_to_dependency_symbols_with_scopes(resolved, scopes, guard))
            .unwrap_or_else(|| vec![symbol.clone()]),
        Expr::Application(parts) => parts
            .iter()
            .filter_map(|expr| {
                expr_as_symbol_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
            })
            .collect(),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            expr_to_dependency_symbols_with_scopes(body, &scopes, guard)
        }
        Expr::Select { .. } => {
            expr_as_symbol_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    };
    guard.ascend();
    result
}

fn fallback_name(path: &Path) -> Option<String> {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

fn build_nix_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Nix.as_str(), name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(truncate_field(purl.to_string()))
}

fn parse_nix_expr(content: &str) -> Result<Expr, String> {
    let tokens = Lexer::new(content).tokenize()?;
    Parser::new(tokens).parse()
}

fn attrset_entries(expr: &Expr) -> Option<&[(Vec<String>, Expr)]> {
    match expr {
        Expr::AttrSet(entries) => Some(entries),
        _ => None,
    }
}

fn list_items_with_scopes<'a>(
    expr: &'a Expr,
    scopes: &[&'a [(Vec<String>, Expr)]],
    guard: &mut RecursionGuard<()>,
) -> Option<&'a [Expr]> {
    if guard.descend() {
        warn!("list_items_with_scopes exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = match expr {
        Expr::List(items) => Some(items.as_slice()),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            list_items_with_scopes(body, &scopes, guard)
        }
        Expr::Symbol(symbol) => resolve_symbol(symbol, scopes, &mut RecursionGuard::depth_only())
            .and_then(|resolved| list_items_with_scopes(resolved, scopes, guard)),
        Expr::Select { target, path } => {
            resolve_select(target, path, scopes, &mut RecursionGuard::depth_only())
                .and_then(|resolved| list_items_with_scopes(resolved, scopes, guard))
        }
        _ => None,
    };
    guard.ascend();
    result
}

fn expr_as_symbol(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Symbol(value) => Some(value.clone()),
        _ => None,
    }
}

fn expr_as_symbol_with_scopes(
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    guard: &mut RecursionGuard<()>,
) -> Option<String> {
    if guard.descend() {
        warn!("expr_as_symbol_with_scopes exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = match expr {
        Expr::Symbol(value) => resolve_symbol(value, scopes, &mut RecursionGuard::depth_only())
            .and_then(|resolved| expr_as_symbol_with_scopes(resolved, scopes, guard))
            .or_else(|| Some(value.clone())),
        Expr::Select { target, path } => {
            resolve_select(target, path, scopes, &mut RecursionGuard::depth_only())
                .and_then(|resolved| expr_as_symbol_with_scopes(resolved, scopes, guard))
        }
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            expr_as_symbol_with_scopes(body, &scopes, guard)
        }
        _ => expr_as_symbol(expr),
    };
    guard.ascend();
    result
}

fn expr_as_bool(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Symbol(value) if value == "true" => Some(true),
        Expr::Symbol(value) if value == "false" => Some(false),
        _ => None,
    }
}

fn expr_as_bool_with_scopes(
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    guard: &mut RecursionGuard<()>,
) -> Option<bool> {
    if guard.descend() {
        warn!("expr_as_bool_with_scopes exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = match expr {
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            expr_as_bool_with_scopes(body, &scopes, guard)
        }
        Expr::Symbol(value) => resolve_symbol(value, scopes, &mut RecursionGuard::depth_only())
            .and_then(|resolved| expr_as_bool_with_scopes(resolved, scopes, guard))
            .or_else(|| expr_as_bool(expr)),
        Expr::Select { target, path } => {
            resolve_select(target, path, scopes, &mut RecursionGuard::depth_only())
                .and_then(|resolved| expr_as_bool_with_scopes(resolved, scopes, guard))
        }
        _ => expr_as_bool(expr),
    };
    guard.ascend();
    result
}

fn expr_as_string_with_scopes(
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    guard: &mut RecursionGuard<()>,
) -> Option<String> {
    if guard.descend() {
        warn!("expr_as_string_with_scopes exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = match expr {
        Expr::String(value) => Some(interpolate_string(value, scopes)),
        Expr::Symbol(value) => resolve_symbol(value, scopes, &mut RecursionGuard::depth_only())
            .and_then(|resolved| expr_as_string_with_scopes(resolved, scopes, guard))
            .or_else(|| Some(value.clone())),
        Expr::Application(parts) => parts
            .last()
            .and_then(|expr| expr_as_string_with_scopes(expr, scopes, guard)),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            expr_as_string_with_scopes(body, &scopes, guard)
        }
        Expr::Select { target, path } => {
            resolve_select(target, path, scopes, &mut RecursionGuard::depth_only())
                .and_then(|resolved| expr_as_string_with_scopes(resolved, scopes, guard))
        }
        _ => None,
    };
    guard.ascend();
    result
}

fn expr_to_scalar_string_with_scopes(
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    guard: &mut RecursionGuard<()>,
) -> Option<String> {
    if guard.descend() {
        warn!("expr_to_scalar_string_with_scopes exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = match expr {
        Expr::Application(parts) => parts
            .last()
            .and_then(|expr| expr_to_scalar_string_with_scopes(expr, scopes, guard)),
        _ => expr_as_string_with_scopes(expr, scopes, guard),
    };
    guard.ascend();
    result
}

fn find_attr<'a>(
    entries: &'a [(Vec<String>, Expr)],
    path: &[&str],
    guard: &mut RecursionGuard<()>,
) -> Option<&'a Expr> {
    if guard.descend() {
        warn!("find_attr exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = entries.iter().find_map(|(key, value)| {
        if key.iter().map(String::as_str).eq(path.iter().copied()) {
            return Some(value);
        }

        if key.len() < path.len()
            && key
                .iter()
                .map(String::as_str)
                .eq(path[..key.len()].iter().copied())
            && let Expr::AttrSet(child_entries) = value
            && let Some(found) = find_attr(child_entries, &path[key.len()..], guard)
        {
            return Some(found);
        }

        None
    });

    guard.ascend();
    result
}

fn find_string_attr_with_scopes(
    entries: &[(Vec<String>, Expr)],
    path: &[&str],
    scopes: &[&[(Vec<String>, Expr)]],
) -> Option<String> {
    find_attr(entries, path, &mut RecursionGuard::depth_only())
        .and_then(|expr| {
            expr_to_scalar_string_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
        })
        .map(truncate_field)
}

fn find_mk_derivation_attrset(expr: &Expr) -> Option<&[(Vec<String>, Expr)]> {
    match expr {
        Expr::Application(parts) => {
            let is_derivation = parts
                .first()
                .and_then(expr_as_symbol)
                .is_some_and(|symbol| symbol.ends_with("mkDerivation"));
            if is_derivation {
                return parts.iter().rev().find_map(attrset_entries);
            }
            None
        }
        _ => None,
    }
}

fn extend_scopes<'a>(
    scopes: &[NixAttrEntriesRef<'a>],
    bindings: NixAttrEntriesRef<'a>,
) -> NixScopeStack<'a> {
    let mut extended = scopes.to_vec();
    extended.push(bindings);
    extended
}

fn root_attrset_with_scopes<'a>(
    expr: &'a Expr,
    scopes: &[NixAttrEntriesRef<'a>],
    guard: &mut RecursionGuard<()>,
) -> Option<(NixAttrEntriesRef<'a>, NixScopeStack<'a>)> {
    if guard.descend() {
        warn!("root_attrset_with_scopes exceeded MAX_RECURSION_DEPTH");
        return None;
    }

    let result = match expr {
        Expr::AttrSet(entries) => Some((entries.as_slice(), scopes.to_vec())),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            root_attrset_with_scopes(body, &scopes, guard)
        }
        _ => None,
    };
    guard.ascend();
    result
}

fn lookup_binding<'a>(scopes: &[NixAttrEntriesRef<'a>], name: &str) -> Option<&'a Expr> {
    scopes
        .iter()
        .rev()
        .find_map(|bindings| find_attr(bindings, &[name], &mut RecursionGuard::depth_only()))
}

fn resolve_symbol<'a>(
    symbol: &str,
    scopes: &[NixAttrEntriesRef<'a>],
    guard: &mut RecursionGuard<()>,
) -> Option<&'a Expr> {
    if guard.descend() {
        return None;
    }

    let mut parts = symbol.split('.');
    let head = parts.next()?;
    let mut expr = lookup_binding(scopes, head)?;
    let rest = parts.collect::<Vec<_>>();
    if rest.is_empty() {
        guard.ascend();
        return Some(expr);
    }

    for segment in rest {
        expr = resolve_select(expr, &[segment.to_string()], scopes, guard)?;
    }

    guard.ascend();
    Some(expr)
}

fn resolve_select<'a>(
    target: &'a Expr,
    path: &[String],
    scopes: &[NixAttrEntriesRef<'a>],
    guard: &mut RecursionGuard<()>,
) -> Option<&'a Expr> {
    if guard.descend() {
        return None;
    }

    let result = match target {
        Expr::AttrSet(entries) => find_attr(
            entries,
            &path.iter().map(String::as_str).collect::<Vec<_>>(),
            guard,
        ),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            resolve_select(body, path, &scopes, guard)
        }
        Expr::Symbol(symbol) => resolve_symbol(symbol, scopes, guard)
            .and_then(|resolved| resolve_select(resolved, path, scopes, guard)),
        Expr::Select {
            target: inner_target,
            path: inner_path,
        } => resolve_select(inner_target, inner_path, scopes, guard)
            .and_then(|resolved| resolve_select(resolved, path, scopes, guard)),
        _ => None,
    };
    guard.ascend();
    result
}

fn interpolate_string(value: &str, scopes: &[&[(Vec<String>, Expr)]]) -> String {
    let mut result = String::new();
    let mut index = 0usize;

    while let Some(relative_start) = value[index..].find("${") {
        let start = index + relative_start;
        result.push_str(&value[index..start]);

        let placeholder_start = start + 2;
        let Some(relative_end) = value[placeholder_start..].find('}') else {
            result.push_str(&value[start..]);
            return result;
        };
        let end = placeholder_start + relative_end;
        let placeholder = value[placeholder_start..end].trim();
        if !placeholder.is_empty()
            && placeholder
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            && let Some(resolved) =
                resolve_symbol(placeholder, scopes, &mut RecursionGuard::depth_only())
            && let Some(replacement) =
                expr_as_string_with_scopes(resolved, scopes, &mut RecursionGuard::depth_only())
        {
            result.push_str(&replacement);
        } else {
            result.push_str(&value[start..=end]);
        }

        index = end + 1;
    }

    result.push_str(&value[index..]);
    result
}

fn extract_default_nix_package(
    path: &Path,
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    depth: usize,
) -> Result<PackageData, String> {
    if depth > 2 {
        return Err("default.nix exceeded supported local import depth".to_string());
    }

    match expr {
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            extract_default_nix_package(path, body, &scopes, depth)
        }
        Expr::Application(parts) => {
            if let Some(derivation) = find_mk_derivation_attrset(expr) {
                return build_default_package_from_attrset(path, derivation, scopes);
            }

            if let Some((imported_expr, imported_path)) =
                try_follow_local_nix_application(path, parts, scopes)
            {
                return extract_default_nix_package(
                    &imported_path,
                    &imported_expr,
                    &Vec::new(),
                    depth + 1,
                );
            }

            if let Some(package) = parts
                .first()
                .and_then(|part| extract_flake_compat_package_from_expr(path, part, scopes, depth))
            {
                return Ok(package);
            }

            Err("default.nix did not contain a supported mkDerivation call".to_string())
        }
        Expr::Select {
            target,
            path: select_path,
        } => {
            if let Some(package) =
                extract_flake_compat_package_from_select(path, target, select_path, scopes, depth)
            {
                return Ok(package);
            }

            if let Some((imported_expr, imported_path)) =
                try_follow_selected_local_import(path, target, select_path, scopes)
            {
                return extract_default_nix_package(
                    &imported_path,
                    &imported_expr,
                    &Vec::new(),
                    depth + 1,
                );
            }

            if let Some(resolved) = resolve_select(
                target,
                select_path,
                scopes,
                &mut RecursionGuard::depth_only(),
            ) {
                return extract_default_nix_package(path, resolved, scopes, depth);
            }

            Err("default.nix did not contain a supported mkDerivation call".to_string())
        }
        Expr::Symbol(_) => extract_flake_compat_package_from_expr(path, expr, scopes, depth)
            .ok_or_else(|| "default.nix did not contain a supported mkDerivation call".to_string()),
        _ => Err("default.nix did not contain a supported mkDerivation call".to_string()),
    }
}

fn build_default_package_from_attrset(
    path: &Path,
    derivation: &[(Vec<String>, Expr)],
    scopes: &[&[(Vec<String>, Expr)]],
) -> Result<PackageData, String> {
    let mut package = default_default_nix_package_data();
    package.name = find_string_attr_with_scopes(derivation, &["pname"], scopes).or_else(|| {
        find_string_attr_with_scopes(derivation, &["name"], scopes)
            .map(|name| split_derivation_name(&name).0)
    });
    package.version =
        find_string_attr_with_scopes(derivation, &["version"], scopes).or_else(|| {
            find_string_attr_with_scopes(derivation, &["name"], scopes)
                .and_then(|name| split_derivation_name(&name).1)
        });
    package.description =
        find_string_attr_with_scopes(derivation, &["meta", "description"], scopes)
            .or_else(|| find_string_attr_with_scopes(derivation, &["description"], scopes));
    package.homepage_url = find_string_attr_with_scopes(derivation, &["meta", "homepage"], scopes)
        .or_else(|| find_string_attr_with_scopes(derivation, &["homepage"], scopes));
    package.extracted_license_statement = find_attr(
        derivation,
        &["meta", "license"],
        &mut RecursionGuard::depth_only(),
    )
    .and_then(|expr| {
        expr_to_scalar_string_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
    })
    .or_else(|| {
        find_attr(derivation, &["license"], &mut RecursionGuard::depth_only()).and_then(|expr| {
            expr_to_scalar_string_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
        })
    });
    package.dependencies = [
        build_list_dependencies(derivation, "nativeBuildInputs", false, scopes),
        build_list_dependencies(derivation, "buildInputs", true, scopes),
        build_list_dependencies(derivation, "propagatedBuildInputs", true, scopes),
        build_list_dependencies(derivation, "checkInputs", false, scopes),
    ]
    .concat();
    if package.name.is_none() {
        package.name = fallback_name(path).map(truncate_field);
    }
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, package.version.as_deref()));

    Ok(package)
}

fn try_follow_local_nix_application(
    path: &Path,
    parts: &[Expr],
    scopes: &[&[(Vec<String>, Expr)]],
) -> Option<(Expr, std::path::PathBuf)> {
    let head = parts.first().and_then(expr_as_symbol)?;
    let is_supported_wrapper = head == "import" || head.ends_with("callPackage");
    if !is_supported_wrapper {
        return None;
    }

    let local_path = parts.get(1).and_then(|expr| {
        expr_as_symbol_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
    })?;
    if !is_local_nix_path(&local_path) {
        return None;
    }

    let resolved_path = resolve_local_nix_path(path, &local_path)?;
    let content = read_file_to_string(&resolved_path, None).ok()?;
    let expr = parse_nix_expr(&content).ok()?;
    Some((expr, resolved_path))
}

fn try_follow_selected_local_import(
    path: &Path,
    target: &Expr,
    select_path: &[String],
    scopes: &[&[(Vec<String>, Expr)]],
) -> Option<(Expr, std::path::PathBuf)> {
    let Expr::Application(parts) = target else {
        return None;
    };

    let (imported_expr, imported_path) = try_follow_local_nix_application(path, parts, scopes)?;
    let selected = attrset_entries(&imported_expr).and_then(|entries| {
        find_attr(
            entries,
            &select_path.iter().map(String::as_str).collect::<Vec<_>>(),
            &mut RecursionGuard::depth_only(),
        )
    })?;
    Some((selected.clone(), imported_path))
}

fn extract_flake_compat_package_from_expr(
    path: &Path,
    expr: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    depth: usize,
) -> Option<PackageData> {
    if depth > 2 {
        return None;
    }

    match expr {
        Expr::Select {
            target,
            path: select_path,
        } => extract_flake_compat_package_from_select(path, target, select_path, scopes, depth),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            extract_flake_compat_package_from_expr(path, body, &scopes, depth)
        }
        Expr::Symbol(symbol) => {
            if let Some((head, rest)) = symbol.split_once('.') {
                let select_path = rest.split('.').map(ToOwned::to_owned).collect::<Vec<_>>();
                resolve_symbol(head, scopes, &mut RecursionGuard::depth_only())
                    .and_then(|resolved| {
                        extract_flake_compat_package_from_select(
                            path,
                            resolved,
                            &select_path,
                            scopes,
                            depth,
                        )
                    })
                    .or_else(|| {
                        let target = Expr::Symbol(head.to_string());
                        extract_flake_compat_package_from_select(
                            path,
                            &target,
                            &select_path,
                            scopes,
                            depth,
                        )
                    })
                    .or_else(|| {
                        resolve_symbol(symbol, scopes, &mut RecursionGuard::depth_only()).and_then(
                            |resolved| {
                                extract_flake_compat_package_from_expr(
                                    path, resolved, scopes, depth,
                                )
                            },
                        )
                    })
            } else {
                resolve_symbol(symbol, scopes, &mut RecursionGuard::depth_only()).and_then(
                    |resolved| {
                        extract_flake_compat_package_from_expr(path, resolved, scopes, depth)
                    },
                )
            }
        }
        _ => None,
    }
}

fn extract_flake_compat_package_from_select(
    path: &Path,
    target: &Expr,
    select_path: &[String],
    scopes: &[&[(Vec<String>, Expr)]],
    depth: usize,
) -> Option<PackageData> {
    if depth > 2 || select_path.first().map(String::as_str) != Some("defaultNix") {
        return None;
    }

    let source_root = resolve_flake_compat_source_root(path, target, scopes, 0)?;
    let mut package = default_default_nix_package_data();
    package.name = source_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .map(truncate_field)
        .or_else(|| fallback_name(path));
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, None));
    mark_flake_compat_wrapper(&mut package);
    Some(package)
}

fn resolve_flake_compat_source_root(
    path: &Path,
    target: &Expr,
    scopes: &[&[(Vec<String>, Expr)]],
    depth: usize,
) -> Option<std::path::PathBuf> {
    if depth > 8 {
        return None;
    }

    match target {
        Expr::Application(parts) => source_root_from_flake_compat_application(path, parts, scopes),
        Expr::Symbol(symbol) => resolve_symbol(symbol, scopes, &mut RecursionGuard::depth_only())
            .and_then(|resolved| {
                resolve_flake_compat_source_root(path, resolved, scopes, depth + 1)
            }),
        Expr::Let { bindings, body } => {
            let scopes = extend_scopes(scopes, bindings);
            resolve_flake_compat_source_root(path, body, &scopes, depth + 1)
        }
        Expr::Select {
            target: inner_target,
            path: inner_path,
        } => resolve_select(
            inner_target,
            inner_path,
            scopes,
            &mut RecursionGuard::depth_only(),
        )
        .and_then(|resolved| resolve_flake_compat_source_root(path, resolved, scopes, depth + 1)),
        _ => None,
    }
}

fn source_root_from_flake_compat_application(
    path: &Path,
    parts: &[Expr],
    scopes: &[&[(Vec<String>, Expr)]],
) -> Option<std::path::PathBuf> {
    let head = parts.first().and_then(expr_as_symbol)?;
    if head != "import" {
        return None;
    }

    let import_path = parts.get(1).and_then(|expr| {
        expr_as_symbol_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
    })?;
    if !is_local_nix_path(&import_path) {
        return None;
    }

    let args = parts.iter().find_map(attrset_entries)?;
    let src_value =
        find_attr(args, &["src"], &mut RecursionGuard::depth_only()).and_then(|expr| {
            expr_as_symbol_with_scopes(expr, scopes, &mut RecursionGuard::depth_only())
        })?;
    if !is_local_path(&src_value) {
        return None;
    }

    resolve_local_path(path, &src_value)
}

fn is_local_path(value: &str) -> bool {
    value.starts_with("./") || value.starts_with("../")
}

fn is_local_nix_path(value: &str) -> bool {
    is_local_path(value) && value.ends_with(".nix")
}

fn resolve_local_path(path: &Path, value: &str) -> Option<std::path::PathBuf> {
    let base = path.parent()?;
    let resolved = base.join(value);
    resolved.exists().then_some(resolved)
}

fn resolve_local_nix_path(path: &Path, value: &str) -> Option<std::path::PathBuf> {
    resolve_local_path(path, value).filter(|resolved| resolved.is_file())
}

fn extract_flake_compat_default_package_from_content(
    path: &Path,
    content: &str,
) -> Result<PackageData, String> {
    if !content.contains("defaultNix") || !content.contains("flake-compat.nix") {
        return Err("default.nix did not contain a supported mkDerivation call".to_string());
    }

    let src_value = extract_local_flake_compat_src_value(content).unwrap_or("./.".to_string());
    let mut package = default_default_nix_package_data();
    package.name = normalize_local_source_root(path, &src_value)
        .and_then(|source_root| {
            source_root
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| *name != ".")
                .map(ToOwned::to_owned)
        })
        .map(truncate_field)
        .or_else(|| fallback_name(path));
    if package.name.is_none() {
        return Err("default.nix did not contain a supported mkDerivation call".to_string());
    }
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, None));
    mark_flake_compat_wrapper(&mut package);
    Ok(package)
}

fn mark_flake_compat_wrapper(package: &mut PackageData) {
    let mut extra_data = package.extra_data.clone().unwrap_or_default();
    extra_data.insert(
        "nix_wrapper_kind".to_string(),
        JsonValue::String("flake_compat".to_string()),
    );
    package.extra_data = Some(extra_data);
}

fn extract_local_flake_compat_src_value(content: &str) -> Option<String> {
    let src_index = content.find("src")?;
    let after_src = &content[src_index + 3..];
    let equals_index = after_src.find('=')?;
    let remainder = after_src[equals_index + 1..].trim_start();
    let end_index = remainder.find([';', '}', '\n']).unwrap_or(remainder.len());
    let candidate = remainder[..end_index].trim();
    if is_local_path(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn normalize_local_source_root(path: &Path, value: &str) -> Option<std::path::PathBuf> {
    match value {
        "." | "./." => path.parent().map(|parent| parent.to_path_buf()),
        _ if value.ends_with("/.") => resolve_local_path(path, value.trim_end_matches("/.")),
        _ => resolve_local_path(path, value),
    }
}

fn split_derivation_name(name: &str) -> (String, Option<String>) {
    let mut parts = name.rsplitn(2, '-');
    let maybe_version = parts
        .next()
        .filter(|value| value.chars().any(|ch| ch.is_ascii_digit()));
    let maybe_name = parts.next();

    match (maybe_name, maybe_version) {
        (Some(package_name), Some(version)) => {
            (package_name.to_string(), Some(version.to_string()))
        }
        _ => (name.to_string(), None),
    }
}

fn default_flake_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Nix),
        primary_language: Some("Nix".to_string()),
        datasource_id: Some(DatasourceId::NixFlakeNix),
        ..Default::default()
    }
}

fn default_flake_lock_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Nix),
        primary_language: Some("JSON".to_string()),
        datasource_id: Some(DatasourceId::NixFlakeLock),
        ..Default::default()
    }
}

fn default_default_nix_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Nix),
        primary_language: Some("Nix".to_string()),
        datasource_id: Some(DatasourceId::NixDefaultNix),
        ..Default::default()
    }
}

crate::register_parser!(
    "Nix flake manifest",
    &["**/flake.nix"],
    "nix",
    "Nix",
    Some("https://nix.dev/manual/nix/stable/command-ref/new-cli/nix3-flake.html"),
);

crate::register_parser!(
    "Nix flake lockfile",
    &["**/flake.lock"],
    "nix",
    "JSON",
    Some("https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake.html"),
);

crate::register_parser!(
    "Nix derivation manifest",
    &["**/default.nix"],
    "nix",
    "Nix",
    Some("https://nix.dev/manual/nix/stable/language/derivations.html"),
);
