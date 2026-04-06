use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;

pub struct NixFlakeLockParser;

impl PackageParser for NixFlakeLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Nix;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "flake.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
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
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read flake.nix at {:?}: {}", path, error);
                return vec![default_flake_package_data()];
            }
        };

        match parse_flake_nix(path, &content) {
            Ok(package) => vec![package],
            Err(error) => {
                warn!("Failed to parse flake.nix at {:?}: {}", path, error);
                vec![default_flake_package_data()]
            }
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
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read default.nix at {:?}: {}", path, error);
                return vec![default_default_nix_package_data()];
            }
        };

        match parse_default_nix(path, &content) {
            Ok(package) => vec![package],
            Err(error) => {
                warn!("Failed to parse default.nix at {:?}: {}", path, error);
                vec![default_default_nix_package_data()]
            }
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
}

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

            if ch == '"' {
                return Ok(result);
            }

            result.push(ch);
        }

        Err("unterminated string".to_string())
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
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse(mut self) -> Result<Expr, String> {
        let expr = self.parse_expr()?;
        if self.peek().is_some() {
            return Err("unexpected trailing tokens".to_string());
        }
        Ok(expr)
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Token::LBrace) && self.looks_like_lambda_binder_set()? {
            self.skip_lambda_binder_set()?;
            self.expect(&Token::Colon)?;
            return self.parse_expr();
        }

        let first = self.parse_term()?;
        if self.consume(&Token::Colon) {
            return self.parse_expr();
        }

        let mut terms = vec![first];
        while self.can_start_term() {
            terms.push(self.parse_term()?);
        }

        if terms.len() == 1 {
            Ok(terms.pop().expect("single term"))
        } else {
            Ok(Expr::Application(terms))
        }
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

        while !matches!(self.peek(), Some(Token::Ident(keyword)) if keyword == "in") {
            if self.peek().is_none() {
                return Err("unterminated let expression".to_string());
            }

            if matches!(self.peek(), Some(Token::Ident(keyword)) if keyword == "inherit") {
                self.skip_until_semicolon()?;
                continue;
            }

            let _key = self.parse_attr_path()?;
            self.expect(&Token::Equals)?;
            let _value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
        }

        self.take_exact_ident("in")?;
        self.parse_expr()
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

            if matches!(self.peek(), Some(Token::Ident(keyword)) if keyword == "inherit") {
                self.skip_until_semicolon()?;
                continue;
            }

            let key = self.parse_attr_path()?;
            self.expect(&Token::Equals)?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            entries.push((key, value));
        }
    }

    fn parse_attr_path(&mut self) -> Result<Vec<String>, String> {
        let mut path = vec![self.take_attr_key()?];
        while self.consume(&Token::Dot) {
            path.push(self.take_attr_key()?);
        }
        Ok(path)
    }

    fn parse_list(&mut self) -> Result<Expr, String> {
        self.expect(&Token::LBracket)?;
        let mut items = Vec::new();
        while !self.consume(&Token::RBracket) {
            if self.peek().is_none() {
                return Err("unterminated list".to_string());
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

    fn skip_until_semicolon(&mut self) -> Result<(), String> {
        while !self.consume(&Token::Semicolon) {
            if self.peek().is_none() {
                return Err("unterminated statement".to_string());
            }
            self.index += 1;
        }
        Ok(())
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

        let mut depth = 0usize;
        let mut index = self.index;

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
    let root = attrset_entries(&expr)
        .ok_or_else(|| "flake.nix root was not an attribute set".to_string())?;

    let mut package = default_flake_package_data();
    package.name = fallback_name(path);
    package.description = find_string_attr(root, &["description"]);
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, None));
    package.dependencies = build_flake_dependencies(root);

    Ok(package)
}

fn parse_default_nix(path: &Path, content: &str) -> Result<PackageData, String> {
    let expr = parse_nix_expr(content)?;
    let derivation = find_mk_derivation_attrset(&expr)
        .ok_or_else(|| "default.nix did not contain a supported mkDerivation call".to_string())?;

    let mut package = default_default_nix_package_data();
    package.name = find_string_attr(derivation, &["pname"]).or_else(|| {
        find_string_attr(derivation, &["name"]).map(|name| split_derivation_name(&name).0)
    });
    package.version = find_string_attr(derivation, &["version"]).or_else(|| {
        find_string_attr(derivation, &["name"]).and_then(|name| split_derivation_name(&name).1)
    });
    package.description = find_string_attr(derivation, &["meta", "description"])
        .or_else(|| find_string_attr(derivation, &["description"]));
    package.homepage_url = find_string_attr(derivation, &["meta", "homepage"])
        .or_else(|| find_string_attr(derivation, &["homepage"]));
    package.extracted_license_statement = find_attr(derivation, &["meta", "license"])
        .and_then(expr_to_scalar_string)
        .or_else(|| find_attr(derivation, &["license"]).and_then(expr_to_scalar_string));
    package.dependencies = [
        build_list_dependencies(derivation, "nativeBuildInputs", false),
        build_list_dependencies(derivation, "buildInputs", true),
        build_list_dependencies(derivation, "propagatedBuildInputs", true),
        build_list_dependencies(derivation, "checkInputs", false),
    ]
    .concat();
    if package.name.is_none() {
        package.name = fallback_name(path);
    }
    package.purl = package
        .name
        .as_deref()
        .and_then(|name| build_nix_purl(name, package.version.as_deref()));

    Ok(package)
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
    package.name = fallback_name(path);
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
        extracted_requirement: build_locked_requirement(locked, node.get("original")),
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

fn build_flake_dependencies(root: &[(Vec<String>, Expr)]) -> Vec<Dependency> {
    let mut inputs: HashMap<String, FlakeInputInfo> = HashMap::new();

    for (path, expr) in root {
        if path.first().map(String::as_str) != Some("inputs") {
            continue;
        }

        if path.len() == 1 {
            if let Some(entries) = attrset_entries(expr) {
                collect_input_entries(entries, &mut inputs, None);
            }
            continue;
        }

        collect_input_path(&path[1..], expr, &mut inputs);
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
                extracted_requirement: info.requirement,
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
    inputs: &mut HashMap<String, FlakeInputInfo>,
    current_input: Option<&str>,
) {
    for (path, expr) in entries {
        if let Some(input_name) = current_input {
            apply_input_field(
                inputs.entry(input_name.to_string()).or_default(),
                path,
                expr,
            );
            continue;
        }

        collect_input_path(path, expr, inputs);
    }
}

fn collect_input_path(path: &[String], expr: &Expr, inputs: &mut HashMap<String, FlakeInputInfo>) {
    let Some(input_name) = path.first() else {
        return;
    };

    if path.len() == 1 {
        match expr {
            Expr::AttrSet(entries) => collect_input_entries(entries, inputs, Some(input_name)),
            Expr::String(value) => {
                inputs.entry(input_name.clone()).or_default().requirement = Some(value.clone())
            }
            _ => {}
        }
        return;
    }

    apply_input_field(
        inputs.entry(input_name.clone()).or_default(),
        &path[1..],
        expr,
    );
}

fn apply_input_field(info: &mut FlakeInputInfo, path: &[String], expr: &Expr) {
    if path == ["url"] {
        info.requirement = expr_as_string(expr);
        return;
    }

    if path == ["flake"] {
        info.flake = expr_as_bool(expr);
        return;
    }

    if path.len() == 3
        && path[0] == "inputs"
        && path[2] == "follows"
        && let Some(value) = expr_as_string(expr)
    {
        info.follows.push(value);
    }
}

fn build_list_dependencies(
    entries: &[(Vec<String>, Expr)],
    field_name: &str,
    runtime: bool,
) -> Vec<Dependency> {
    let Some(expr) = find_attr(entries, &[field_name]) else {
        return Vec::new();
    };
    let Some(items) = list_items(expr) else {
        return Vec::new();
    };

    items
        .iter()
        .flat_map(expr_to_dependency_symbols)
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

fn expr_to_dependency_symbols(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::Symbol(symbol) => vec![symbol.clone()],
        Expr::Application(parts) => parts.iter().filter_map(expr_as_symbol).collect(),
        _ => Vec::new(),
    }
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
    Some(purl.to_string())
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

fn list_items(expr: &Expr) -> Option<&[Expr]> {
    match expr {
        Expr::List(items) => Some(items),
        _ => None,
    }
}

fn expr_as_string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::String(value) => Some(value.clone()),
        Expr::Symbol(value) => Some(value.clone()),
        _ => None,
    }
}

fn expr_as_symbol(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Symbol(value) => Some(value.clone()),
        _ => None,
    }
}

fn expr_as_bool(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Symbol(value) if value == "true" => Some(true),
        Expr::Symbol(value) if value == "false" => Some(false),
        _ => None,
    }
}

fn expr_to_scalar_string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::String(value) | Expr::Symbol(value) => Some(value.clone()),
        Expr::Application(parts) => parts.last().and_then(expr_to_scalar_string),
        _ => None,
    }
}

fn find_attr<'a>(entries: &'a [(Vec<String>, Expr)], path: &[&str]) -> Option<&'a Expr> {
    for (key, value) in entries {
        if key.iter().map(String::as_str).eq(path.iter().copied()) {
            return Some(value);
        }

        if key.len() < path.len()
            && key
                .iter()
                .map(String::as_str)
                .eq(path[..key.len()].iter().copied())
            && let Expr::AttrSet(child_entries) = value
            && let Some(found) = find_attr(child_entries, &path[key.len()..])
        {
            return Some(found);
        }
    }

    None
}

fn find_string_attr(entries: &[(Vec<String>, Expr)], path: &[&str]) -> Option<String> {
    find_attr(entries, path).and_then(expr_to_scalar_string)
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
