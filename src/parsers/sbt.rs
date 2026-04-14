use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::json;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;
use super::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

const MAX_RECURSION_DEPTH: usize = 50;

pub struct SbtParser;

impl PackageParser for SbtParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "build.sbt")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read {:?}: {}", path, error);
                return vec![default_package_data()];
            }
        };

        let sanitized = strip_comments(&content);
        let statements = split_top_level_statements(&sanitized);
        let aliases = resolve_string_aliases(&statements);
        let parsed = parse_statements(&statements, &aliases);

        let homepage_url = parsed.homepage.or(parsed.organization_homepage);
        let extracted_license_statement = format_license_entries(&parsed.licenses);
        let purl = build_maven_purl(
            parsed.organization.as_deref(),
            parsed.name.as_deref(),
            parsed.version.as_deref(),
        );

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            primary_language: Some("Scala".to_string()),
            namespace: parsed.organization.map(truncate_field),
            name: parsed.name.map(truncate_field),
            version: parsed.version.map(truncate_field),
            description: parsed.description.map(truncate_field),
            homepage_url: homepage_url.map(truncate_field),
            extracted_license_statement: extracted_license_statement.map(truncate_field),
            dependencies: parsed.dependencies,
            datasource_id: Some(DatasourceId::SbtBuildSbt),
            purl,
            ..Default::default()
        }]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(SbtParser::PACKAGE_TYPE),
        primary_language: Some("Scala".to_string()),
        datasource_id: Some(DatasourceId::SbtBuildSbt),
        ..Default::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Str(String),
    Symbol(&'static str),
}

#[derive(Debug, Clone)]
enum AliasExpr {
    Literal(String),
    Reference(String),
}

#[derive(Debug, Clone)]
struct ScopedValue {
    precedence: u8,
    value: String,
}

#[derive(Debug, Clone)]
struct LicenseEntry {
    name: String,
    url: String,
}

#[derive(Debug, Default)]
struct ParsedSbtData {
    organization: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    organization_homepage: Option<String>,
    licenses: Vec<LicenseEntry>,
    dependencies: Vec<Dependency>,
}

#[derive(Default)]
struct SbtParseAccumulator {
    organization: Option<ScopedValue>,
    name: Option<ScopedValue>,
    version: Option<ScopedValue>,
    description: Option<ScopedValue>,
    homepage: Option<ScopedValue>,
    organization_homepage: Option<ScopedValue>,
    licenses: Vec<LicenseEntry>,
    dependencies: Vec<Dependency>,
}

fn strip_comments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < chars.len() {
        let ch = chars[index];

        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            index += 1;
            continue;
        }

        if ch == '/' && chars.get(index + 1) == Some(&'/') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }

        if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() {
                if chars[index] == '*' && chars[index + 1] == '/' {
                    index += 2;
                    break;
                }
                if chars[index] == '\n' {
                    output.push('\n');
                }
                index += 1;
            }
            continue;
        }

        output.push(ch);
        index += 1;
    }

    output
}

fn split_top_level_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut iterations = 0usize;

    for ch in input.chars() {
        iterations += 1;
        if iterations > MAX_ITERATION_COUNT {
            break;
        }

        if in_string {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                current.push(ch);
            }
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            '\n' | ';' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }

    statements
}

fn tokenize(statement: &str) -> Vec<Token> {
    let chars: Vec<char> = statement.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut iterations = 0usize;

    while index < chars.len() {
        iterations += 1;
        if iterations > MAX_ITERATION_COUNT {
            break;
        }

        let ch = chars[index];

        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        if ch == '"' {
            index += 1;
            let start = index;
            let mut escaped = false;
            while index < chars.len() {
                let current = chars[index];
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == '"' {
                    break;
                }
                index += 1;
            }

            let value: String = chars[start..index].iter().collect();
            tokens.push(Token::Str(value));
            if index < chars.len() && chars[index] == '"' {
                index += 1;
            }
            continue;
        }

        if matches_chars(&chars, index, &['+', '+', '=']) {
            tokens.push(Token::Symbol("++="));
            index += 3;
            continue;
        }

        if matches_chars(&chars, index, &[':', '=']) {
            tokens.push(Token::Symbol(":="));
            index += 2;
            continue;
        }

        if matches_chars(&chars, index, &['+', '=']) {
            tokens.push(Token::Symbol("+="));
            index += 2;
            continue;
        }

        if matches_chars(&chars, index, &['%', '%']) {
            tokens.push(Token::Symbol("%%"));
            index += 2;
            continue;
        }

        if matches_chars(&chars, index, &['-', '>']) {
            tokens.push(Token::Symbol("->"));
            index += 2;
            continue;
        }

        match ch {
            '%' => {
                tokens.push(Token::Symbol("%"));
                index += 1;
            }
            '/' => {
                tokens.push(Token::Symbol("/"));
                index += 1;
            }
            '=' => {
                tokens.push(Token::Symbol("="));
                index += 1;
            }
            '(' => {
                tokens.push(Token::Symbol("("));
                index += 1;
            }
            ')' => {
                tokens.push(Token::Symbol(")"));
                index += 1;
            }
            '[' => {
                tokens.push(Token::Symbol("["));
                index += 1;
            }
            ']' => {
                tokens.push(Token::Symbol("]"));
                index += 1;
            }
            '{' => {
                tokens.push(Token::Symbol("{"));
                index += 1;
            }
            '}' => {
                tokens.push(Token::Symbol("}"));
                index += 1;
            }
            ',' => {
                tokens.push(Token::Symbol(","));
                index += 1;
            }
            _ if is_ident_start(ch) => {
                let start = index;
                index += 1;
                while index < chars.len() && is_ident_char(chars[index]) {
                    index += 1;
                }
                let value: String = chars[start..index].iter().collect();
                tokens.push(Token::Ident(value));
            }
            _ => {
                index += 1;
            }
        }
    }

    tokens
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
}

fn matches_chars(chars: &[char], index: usize, expected: &[char]) -> bool {
    chars.get(index..index + expected.len()) == Some(expected)
}

fn resolve_string_aliases(statements: &[String]) -> HashMap<String, String> {
    let mut raw_aliases = HashMap::new();

    for statement in statements.iter().take(MAX_ITERATION_COUNT) {
        let tokens = tokenize(statement);
        if let Some((name, expr)) = parse_alias_declaration(&tokens) {
            raw_aliases.insert(name, expr);
        }
    }

    let mut resolved = HashMap::new();
    for name in raw_aliases.keys() {
        let mut visiting = HashSet::new();
        if let Some(value) =
            resolve_alias_value(name, &raw_aliases, &mut resolved, &mut visiting, 0)
        {
            resolved.insert(name.clone(), value);
        }
    }

    resolved
}

fn parse_alias_declaration(tokens: &[Token]) -> Option<(String, AliasExpr)> {
    match tokens {
        [
            Token::Ident(keyword),
            Token::Ident(name),
            Token::Symbol("="),
            expr @ ..,
        ] if keyword == "val" => {
            if let [Token::Str(value)] = expr {
                return Some((name.clone(), AliasExpr::Literal(value.clone())));
            }
            if let [Token::Ident(reference)] = expr {
                return Some((name.clone(), AliasExpr::Reference(reference.clone())));
            }
            None
        }
        _ => None,
    }
}

fn resolve_alias_value(
    name: &str,
    raw_aliases: &HashMap<String, AliasExpr>,
    resolved: &mut HashMap<String, String>,
    visiting: &mut HashSet<String>,
    depth: usize,
) -> Option<String> {
    if depth >= MAX_RECURSION_DEPTH {
        warn!(
            "Recursion depth exceeded in alias resolution for '{}'",
            name
        );
        return None;
    }

    if let Some(value) = resolved.get(name) {
        return Some(value.clone());
    }

    if !visiting.insert(name.to_string()) {
        return None;
    }

    let value = match raw_aliases.get(name)? {
        AliasExpr::Literal(value) => Some(value.clone()),
        AliasExpr::Reference(reference) => {
            resolve_alias_value(reference, raw_aliases, resolved, visiting, depth + 1)
        }
    };

    visiting.remove(name);
    value
}

fn resolve_seq_aliases(statements: &[String]) -> HashMap<String, Vec<Vec<Token>>> {
    let mut aliases = HashMap::new();

    for statement in statements {
        let tokens = tokenize(statement);
        if let Some((name, items)) = parse_seq_alias_declaration(&tokens) {
            aliases.insert(name, items);
        }
    }

    aliases
}

fn parse_seq_alias_declaration(tokens: &[Token]) -> Option<(String, Vec<Vec<Token>>)> {
    match tokens {
        [
            Token::Ident(keyword),
            Token::Ident(name),
            Token::Symbol("="),
            expr @ ..,
        ] if keyword == "val" => parse_seq_items(expr).map(|items| (name.clone(), items)),
        _ => None,
    }
}

fn parse_seq_items(tokens: &[Token]) -> Option<Vec<Vec<Token>>> {
    let [
        Token::Ident(seq),
        Token::Symbol("("),
        inner @ ..,
        Token::Symbol(")"),
    ] = tokens
    else {
        return None;
    };
    if seq != "Seq" {
        return None;
    }

    Some(
        split_by_top_level_commas(inner)
            .into_iter()
            .map(|item| item.to_vec())
            .collect(),
    )
}

fn parse_statements(statements: &[String], aliases: &HashMap<String, String>) -> ParsedSbtData {
    let bundle_aliases = resolve_seq_aliases(statements);
    let mut state = SbtParseAccumulator::default();

    for statement in statements.iter().take(MAX_ITERATION_COUNT) {
        let tokens = tokenize(statement);
        process_statement_tokens(&tokens, aliases, &bundle_aliases, &mut state);
    }

    ParsedSbtData {
        organization: state.organization.map(|value| value.value),
        name: state.name.map(|value| value.value),
        version: state.version.map(|value| value.value),
        description: state.description.map(|value| value.value),
        homepage: state.homepage.map(|value| value.value),
        organization_homepage: state.organization_homepage.map(|value| value.value),
        licenses: state.licenses,
        dependencies: state.dependencies,
    }
}

fn process_statement_tokens(
    tokens: &[Token],
    aliases: &HashMap<String, String>,
    bundle_aliases: &HashMap<String, Vec<Vec<Token>>>,
    state: &mut SbtParseAccumulator,
) {
    if let Some(inner_items) = extract_root_settings_items(tokens, bundle_aliases) {
        for item in inner_items {
            process_statement_tokens(&item, aliases, bundle_aliases, state);
        }
        return;
    }

    if let Some((precedence, value)) = parse_string_setting(tokens, aliases, "organization") {
        set_scoped_value(&mut state.organization, precedence, value);
        return;
    }

    if let Some((precedence, value)) = parse_string_setting(tokens, aliases, "name") {
        set_scoped_value(&mut state.name, precedence, value);
        return;
    }

    if let Some((precedence, value)) = parse_string_setting(tokens, aliases, "version") {
        set_scoped_value(&mut state.version, precedence, value);
        return;
    }

    if let Some((precedence, value)) = parse_string_setting(tokens, aliases, "description") {
        set_scoped_value(&mut state.description, precedence, value);
        return;
    }

    if let Some((precedence, value)) = parse_url_setting(tokens, "homepage") {
        set_scoped_value(&mut state.homepage, precedence, value);
        return;
    }

    if let Some((precedence, value)) = parse_url_setting(tokens, "organizationHomepage") {
        set_scoped_value(&mut state.organization_homepage, precedence, value);
        return;
    }

    if let Some(license_entry) = parse_license_append(tokens) {
        state.licenses.push(license_entry);
        return;
    }

    if let Some(new_dependencies) = parse_library_dependencies(tokens, aliases, bundle_aliases) {
        state.dependencies.extend(new_dependencies);
    }
}

fn set_scoped_value(target: &mut Option<ScopedValue>, precedence: u8, value: String) {
    let should_replace = target
        .as_ref()
        .is_none_or(|current| precedence >= current.precedence);

    if should_replace {
        *target = Some(ScopedValue { precedence, value });
    }
}

fn parse_setting_prefix(tokens: &[Token]) -> (u8, &[Token]) {
    match tokens {
        [Token::Ident(scope), Token::Symbol("/"), rest @ ..] if scope == "ThisBuild" => (1, rest),
        _ => (2, tokens),
    }
}

fn parse_string_setting(
    tokens: &[Token],
    aliases: &HashMap<String, String>,
    key: &str,
) -> Option<(u8, String)> {
    let (precedence, rest) = parse_setting_prefix(tokens);
    match rest {
        [Token::Ident(name), Token::Symbol(":="), expr @ ..] if name == key => {
            parse_literal_string_expr(expr, aliases).map(|value| (precedence, value))
        }
        _ => None,
    }
}

fn parse_literal_string_expr(
    tokens: &[Token],
    aliases: &HashMap<String, String>,
) -> Option<String> {
    match tokens {
        [Token::Str(value)] => Some(truncate_field(value.clone())),
        [Token::Ident(name)] => aliases.get(name).cloned().map(truncate_field),
        _ => None,
    }
}

fn parse_url_setting(tokens: &[Token], key: &str) -> Option<(u8, String)> {
    let (precedence, rest) = parse_setting_prefix(tokens);
    match rest {
        [Token::Ident(name), Token::Symbol(":="), expr @ ..] if name == key => {
            parse_url_expr(expr).map(|value| (precedence, value))
        }
        _ => None,
    }
}

fn parse_url_expr(tokens: &[Token]) -> Option<String> {
    match tokens {
        [
            Token::Ident(some),
            Token::Symbol("("),
            Token::Ident(url_fn),
            Token::Symbol("("),
            Token::Str(url),
            Token::Symbol(")"),
            Token::Symbol(")"),
        ] if some == "Some" && url_fn == "url" => Some(truncate_field(url.clone())),
        _ => None,
    }
}

fn parse_license_append(tokens: &[Token]) -> Option<LicenseEntry> {
    match tokens {
        [
            Token::Ident(name),
            Token::Symbol("+="),
            Token::Str(license_name),
            Token::Symbol("->"),
            Token::Ident(url_fn),
            Token::Symbol("("),
            Token::Str(url),
            Token::Symbol(")"),
        ] if name == "licenses" && url_fn == "url" => Some(LicenseEntry {
            name: truncate_field(license_name.clone()),
            url: truncate_field(url.clone()),
        }),
        _ => None,
    }
}

fn parse_library_dependencies(
    tokens: &[Token],
    aliases: &HashMap<String, String>,
    bundle_aliases: &HashMap<String, Vec<Vec<Token>>>,
) -> Option<Vec<Dependency>> {
    let (inherited_scope, tokens) = parse_dependency_setting_prefix(tokens)?;

    match tokens {
        [Token::Ident(name), Token::Symbol("+="), expr @ ..] if name == "libraryDependencies" => {
            parse_dependency_expr(expr, aliases, inherited_scope.as_deref())
                .map(|dependency| vec![dependency])
        }
        [Token::Ident(name), Token::Symbol("++="), expr @ ..] if name == "libraryDependencies" => {
            parse_dependency_seq(expr, aliases, bundle_aliases, inherited_scope.as_deref())
        }
        _ => None,
    }
}

fn parse_dependency_setting_prefix(tokens: &[Token]) -> Option<(Option<String>, &[Token])> {
    match tokens {
        [Token::Ident(scope), Token::Symbol("/"), rest @ ..]
            if is_supported_config_scope(scope) =>
        {
            Some((Some(scope.to_ascii_lowercase()), rest))
        }
        _ => Some((None, tokens)),
    }
}

fn is_supported_config_scope(scope: &str) -> bool {
    matches!(
        scope,
        "Compile" | "Runtime" | "Provided" | "Test" | "compile" | "runtime" | "provided" | "test"
    )
}

fn parse_dependency_seq(
    tokens: &[Token],
    aliases: &HashMap<String, String>,
    bundle_aliases: &HashMap<String, Vec<Vec<Token>>>,
    inherited_scope: Option<&str>,
) -> Option<Vec<Dependency>> {
    let items = if let [Token::Ident(alias_name)] = tokens {
        bundle_aliases.get(alias_name)?.clone()
    } else {
        parse_seq_items(tokens)?
    };

    let mut dependencies = Vec::new();
    for item in items.iter().take(MAX_ITERATION_COUNT) {
        if let Some(dependency) = parse_dependency_expr(item, aliases, inherited_scope) {
            dependencies.push(dependency);
        }
    }

    Some(dependencies)
}

fn split_by_top_level_commas(tokens: &[Token]) -> Vec<&[Token]> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;

    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Symbol("(") => paren_depth += 1,
            Token::Symbol(")") => paren_depth = paren_depth.saturating_sub(1),
            Token::Symbol("[") => bracket_depth += 1,
            Token::Symbol("]") => bracket_depth = bracket_depth.saturating_sub(1),
            Token::Symbol("{") => brace_depth += 1,
            Token::Symbol("}") => brace_depth = brace_depth.saturating_sub(1),
            Token::Symbol(",") if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                if start < index {
                    items.push(&tokens[start..index]);
                }
                start = index + 1;
            }
            _ => {}
        }
    }

    if start < tokens.len() {
        items.push(&tokens[start..]);
    }

    items
}

fn extract_root_settings_items(
    tokens: &[Token],
    bundle_aliases: &HashMap<String, Vec<Vec<Token>>>,
) -> Option<Vec<Vec<Token>>> {
    let [
        Token::Ident(lazy),
        Token::Ident(val_kw),
        Token::Ident(root),
        Token::Symbol("="),
        rest @ ..,
    ] = tokens
    else {
        return None;
    };
    if lazy != "lazy" || val_kw != "val" || root != "root" {
        return None;
    }

    let inner = if let [
        Token::Ident(call),
        Token::Symbol("("),
        inner @ ..,
        Token::Symbol(")"),
    ] = rest
    {
        if call != "project.settings" {
            return None;
        }
        inner
    } else if let Some(settings_index) = rest
        .iter()
        .position(|token| matches!(token, Token::Ident(name) if name == "settings"))
    {
        if !is_root_project_wrapper(&rest[..settings_index]) {
            return None;
        }
        match &rest[settings_index..] {
            [
                Token::Ident(name),
                Token::Symbol("("),
                inner @ ..,
                Token::Symbol(")"),
            ] if name == "settings" => inner,
            _ => return None,
        }
    } else {
        return None;
    };

    let mut expanded = Vec::new();
    for item in split_by_top_level_commas(inner) {
        if let [Token::Ident(alias_name)] = item
            && let Some(bundle_items) = bundle_aliases.get(alias_name)
        {
            expanded.extend(bundle_items.clone());
        } else {
            expanded.push(item.to_vec());
        }
    }

    Some(expanded)
}

fn is_root_project_wrapper(tokens: &[Token]) -> bool {
    matches!(
        tokens,
        [
            Token::Symbol("("),
            Token::Ident(project),
            Token::Ident(in_kw),
            Token::Ident(file),
            Token::Symbol("("),
            Token::Str(path),
            Token::Symbol(")"),
            Token::Symbol(")")
        ] if project == "project" && in_kw == "in" && file == "file" && path == "."
    )
}

fn strip_outer_parens(tokens: &[Token]) -> &[Token] {
    let mut current = tokens;
    loop {
        if current.len() < 2 {
            return current;
        }

        if current.first() != Some(&Token::Symbol("("))
            || current.last() != Some(&Token::Symbol(")"))
            || !outer_parens_wrap_all(current)
        {
            return current;
        }

        current = &current[1..current.len() - 1];
    }
}

fn outer_parens_wrap_all(tokens: &[Token]) -> bool {
    let mut depth = 0usize;

    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Symbol("(") => depth += 1,
            Token::Symbol(")") => {
                depth = depth.saturating_sub(1);
                if depth == 0 && index + 1 != tokens.len() {
                    return false;
                }
            }
            _ => {}
        }
    }

    depth == 0
}

fn parse_dependency_expr(
    tokens: &[Token],
    aliases: &HashMap<String, String>,
    inherited_scope: Option<&str>,
) -> Option<Dependency> {
    let tokens = strip_outer_parens(tokens);
    if tokens.len() != 5 && tokens.len() != 7 {
        return None;
    }

    let operator = match tokens.get(1) {
        Some(Token::Symbol("%")) => "%",
        Some(Token::Symbol("%%")) => "%%",
        _ => return None,
    };

    if tokens.get(3) != Some(&Token::Symbol("%")) {
        return None;
    }

    let group = parse_literal_string_expr(&tokens[0..1], aliases)?;
    let artifact = parse_literal_string_expr(&tokens[2..3], aliases)?;
    let version = parse_literal_string_expr(&tokens[4..5], aliases)?;
    let explicit_scope = if tokens.len() == 7 {
        if tokens.get(5) != Some(&Token::Symbol("%")) {
            return None;
        }
        Some(parse_scope_expr(tokens.get(6)?)?)
    } else {
        None
    };
    let scope = explicit_scope.or_else(|| inherited_scope.map(ToOwned::to_owned));

    build_dependency(group, artifact, version, scope, operator)
}

fn parse_scope_expr(token: &Token) -> Option<String> {
    match token {
        Token::Str(value) => Some(value.to_ascii_lowercase()),
        Token::Ident(value) => Some(value.to_ascii_lowercase()),
        _ => None,
    }
}

fn build_dependency(
    namespace: String,
    name: String,
    version: String,
    scope: Option<String>,
    operator: &str,
) -> Option<Dependency> {
    let namespace = truncate_field(namespace);
    let name = truncate_field(name);
    let version = truncate_field(version);
    let purl = build_maven_purl(
        Some(namespace.as_str()),
        Some(name.as_str()),
        Some(version.as_str()),
    )?;
    let (is_runtime, is_optional) = classify_scope(scope.as_deref());
    let mut extra_data = HashMap::new();

    if operator == "%%" {
        extra_data.insert("sbt_cross_version".to_string(), json!(true));
        extra_data.insert("sbt_operator".to_string(), json!(operator));
    }

    Some(Dependency {
        purl: Some(purl),
        extracted_requirement: Some(version.clone()),
        scope,
        is_runtime,
        is_optional,
        is_pinned: Some(!version.is_empty()),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn classify_scope(scope: Option<&str>) -> (Option<bool>, Option<bool>) {
    match scope {
        None => (Some(true), Some(false)),
        Some("compile") | Some("runtime") => (Some(true), Some(false)),
        Some("provided") => (Some(false), Some(false)),
        Some("test") => (Some(false), Some(true)),
        Some(_) => (None, None),
    }
}

fn build_maven_purl(
    namespace: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
) -> Option<String> {
    let name = name?.trim();
    if name.is_empty() {
        return None;
    }

    let mut purl = PackageUrl::new("maven", name).ok()?;
    if let Some(namespace) = namespace.map(str::trim).filter(|value| !value.is_empty()) {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn format_license_entries(licenses: &[LicenseEntry]) -> Option<String> {
    if licenses.is_empty() {
        return None;
    }

    let mut formatted = String::new();
    for license in licenses {
        formatted.push_str("- license:\n");
        formatted.push_str("    name: ");
        formatted.push_str(&license.name);
        formatted.push('\n');
        formatted.push_str("    url: ");
        formatted.push_str(&license.url);
        formatted.push('\n');
    }

    Some(formatted)
}

crate::register_parser!(
    "Scala SBT build.sbt definition",
    &["**/build.sbt"],
    "maven",
    "Scala",
    Some("https://www.scala-sbt.org/1.x/docs/Basic-Def.html"),
);
