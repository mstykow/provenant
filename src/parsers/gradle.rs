// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Parser for Gradle build files (Groovy and Kotlin DSL).
//!
//! Extracts dependencies from Gradle build scripts using a custom token-based
//! lexer and recursive descent parser supporting both Groovy and Kotlin syntax.
//!
//! # Supported Formats
//! - build.gradle (Groovy DSL)
//! - build.gradle.kts (Kotlin DSL)
//!
//! # Key Features
//! - Token-based lexer for Gradle syntax parsing (not full language parser)
//! - Support for multiple dependency declaration styles
//! - Dependency scope tracking (implementation, testImplementation, etc.)
//! - Project dependency references and platform dependencies
//! - Version interpolation and constraint parsing
//! - Package URL (purl) generation for Maven packages
//!
//! # Implementation Notes
//! - Custom 870-line lexer instead of external parser (smaller binary, easier maintenance)
//! - Supports Groovy and Kotlin syntax variations
//! - Graceful error handling with `warn!()` logs
//! - Direct dependency tracking (all in build file are direct)

use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

const MAX_RECURSION_DEPTH: usize = 50;
use packageurl::PackageUrl;
use serde_json::json;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::PackageParser;

use super::license_normalization::{
    DeclaredLicenseMatchMetadata, build_declared_license_data, empty_declared_license_data,
    normalize_spdx_expression,
};

/// Parses Gradle build files (build.gradle, build.gradle.kts).
///
/// Extracts dependencies from Gradle build scripts using a custom
/// token-based lexer and recursive descent parser. Supports both
/// Groovy and Kotlin DSL syntax.
///
/// # Supported Patterns
/// - String notation: `implementation 'group:name:version'`
/// - Named parameters: `implementation group: 'x', name: 'y', version: 'z'`
/// - Map format: `implementation([group: 'x', name: 'y'])`
/// - Nested functions: `implementation(enforcedPlatform("..."))`
/// - Project references: `implementation(project(":module"))`
/// - String interpolation: `implementation("group:name:${version}")`
///
/// # Implementation
/// Uses a custom token-based lexer (870 lines) instead of tree-sitter for:
/// - Lighter binary size (no external parser dependencies)
/// - Easier maintenance for DSL-specific quirks
/// - Better error messages for malformed input
///
/// # Example
/// ```no_run
/// use provenant::parsers::{GradleParser, PackageParser};
/// use std::path::Path;
///
/// let path = Path::new("testdata/gradle-golden/groovy1/build.gradle");
/// let package_data = GradleParser::extract_first_package(path);
/// assert!(!package_data.dependencies.is_empty());
/// ```
pub struct GradleParser;

impl PackageParser for GradleParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            let name_str = name.to_string_lossy();
            name_str == "build.gradle" || name_str == "build.gradle.kts"
        })
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let tokens = lex(&content);
        let mut dependencies = extract_dependencies(&tokens);
        resolve_gradle_version_catalog_aliases(path, &mut dependencies);
        let (
            extracted_license_statement,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
        ) = extract_gradle_license_metadata(&tokens);

        vec![PackageData {
            package_type: Some(Self::PACKAGE_TYPE),
            namespace: None,
            name: None,
            version: None,
            qualifiers: None,
            subpath: None,
            primary_language: None,
            description: None,
            release_date: None,
            parties: Vec::new(),
            keywords: Vec::new(),
            homepage_url: None,
            download_url: None,
            size: None,
            sha1: None,
            md5: None,
            sha256: None,
            sha512: None,
            bug_tracking_url: None,
            code_view_url: None,
            vcs_url: None,
            copyright: None,
            holder: None,
            declared_license_expression,
            declared_license_expression_spdx,
            license_detections,
            other_license_expression: None,
            other_license_expression_spdx: None,
            other_license_detections: Vec::new(),
            extracted_license_statement,
            notice_text: None,
            source_packages: Vec::new(),
            file_references: Vec::new(),
            extra_data: None,
            dependencies,
            repository_homepage_url: None,
            repository_download_url: None,
            api_data_url: None,
            datasource_id: Some(DatasourceId::BuildGradle),
            purl: None,
            is_private: false,
            is_virtual: false,
        }]
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(GradleParser::PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::BuildGradle),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    MalformedStr(String),
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Colon,
    Comma,
    Equals,
}

fn lex(input: &str) -> Vec<Tok> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < len {
        if tokens.len() >= MAX_ITERATION_COUNT {
            warn!(
                "Lexer exceeded MAX_ITERATION_COUNT ({}) tokens, stopping",
                MAX_ITERATION_COUNT
            );
            break;
        }
        let c = chars[i];

        if c == '/' && i + 1 < len && chars[i + 1] == '/' {
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        if c == '/' && i + 1 < len && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c == '\'' {
            i += 1;
            let start = i;
            while i < len && chars[i] != '\'' && chars[i] != '\n' {
                i += 1;
            }
            let val: String = chars[start..i].iter().collect();
            let val = truncate_field(val);
            if i < len && chars[i] == '\'' {
                tokens.push(Tok::Str(val));
                i += 1;
            } else {
                tokens.push(Tok::MalformedStr(val));
            }
            continue;
        }

        if c == '"' {
            i += 1;
            let start = i;
            while i < len && chars[i] != '"' && chars[i] != '\n' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            let val: String = chars[start..i].iter().collect();
            let val = truncate_field(val);
            if i < len && chars[i] == '"' {
                tokens.push(Tok::Str(val));
                i += 1;
            } else {
                tokens.push(Tok::MalformedStr(val));
            }
            continue;
        }

        match c {
            '(' => {
                tokens.push(Tok::OpenParen);
                i += 1;
            }
            ')' => {
                tokens.push(Tok::CloseParen);
                i += 1;
            }
            '[' => {
                tokens.push(Tok::OpenBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Tok::CloseBracket);
                i += 1;
            }
            '{' => {
                tokens.push(Tok::OpenBrace);
                i += 1;
            }
            '}' => {
                tokens.push(Tok::CloseBrace);
                i += 1;
            }
            ':' => {
                tokens.push(Tok::Colon);
                i += 1;
            }
            ',' => {
                tokens.push(Tok::Comma);
                i += 1;
            }
            '=' => {
                tokens.push(Tok::Equals);
                i += 1;
            }
            _ if is_ident_start(c) => {
                let start = i;
                while i < len && is_ident_char(chars[i]) {
                    i += 1;
                }
                let val: String = chars[start..i].iter().collect();
                tokens.push(Tok::Ident(truncate_field(val)));
            }
            _ => {
                i += 1;
            }
        }
    }

    tokens
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '$'
}

// ---------------------------------------------------------------------------
// Dependency block extraction
// ---------------------------------------------------------------------------

fn find_dependency_blocks(tokens: &[Tok]) -> Vec<Vec<Tok>> {
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Tok::Ident(ref name) = tokens[i]
            && name == "dependencies"
            && i + 1 < tokens.len()
            && tokens[i + 1] == Tok::OpenBrace
        {
            i += 2;
            let mut depth = 1;
            let start = i;
            while i < tokens.len() && depth > 0 {
                match &tokens[i] {
                    Tok::OpenBrace => {
                        depth += 1;
                        if depth > MAX_RECURSION_DEPTH {
                            warn!(
                                "Gradle parser: nesting depth exceeded {} in find_dependency_blocks",
                                MAX_RECURSION_DEPTH
                            );
                            break;
                        }
                    }
                    Tok::CloseBrace => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    i += 1;
                }
            }
            blocks.push(tokens[start..i].to_vec());
            if i < tokens.len() {
                i += 1;
            }
            continue;
        }
        i += 1;
    }

    blocks
}

// ---------------------------------------------------------------------------
// Dependency extraction from blocks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RawDep {
    namespace: String,
    name: String,
    version: String,
    scope: String,
    catalog_alias: Option<String>,
    project_path: Option<String>,
}

fn extract_dependencies(tokens: &[Tok]) -> Vec<Dependency> {
    let blocks = find_dependency_blocks(tokens);
    let mut dependencies = Vec::new();

    for block in blocks {
        for rd in parse_block(&block).into_iter().take(MAX_ITERATION_COUNT) {
            if rd.name.is_empty() {
                continue;
            }
            if let Some(dep) = create_dependency(&rd) {
                dependencies.push(dep);
            }
        }
    }

    dependencies
}

fn parse_block(tokens: &[Tok]) -> Vec<RawDep> {
    let mut deps = Vec::new();
    let mut i = 0;
    let mut iterations = 0;

    while i < tokens.len() {
        iterations += 1;
        if iterations > MAX_ITERATION_COUNT {
            warn!(
                "parse_block exceeded MAX_ITERATION_COUNT ({}) iterations, stopping",
                MAX_ITERATION_COUNT
            );
            break;
        }
        // Skip nested blocks (closures like `{ transitive = true }`)
        if tokens[i] == Tok::OpenBrace {
            let mut depth = 1;
            i += 1;
            while i < tokens.len() && depth > 0 {
                match &tokens[i] {
                    Tok::OpenBrace => {
                        depth += 1;
                        if depth > MAX_RECURSION_DEPTH {
                            warn!(
                                "Gradle parser: nesting depth exceeded {} in parse_block",
                                MAX_RECURSION_DEPTH
                            );
                            break;
                        }
                    }
                    Tok::CloseBrace => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            continue;
        }

        if let Tok::Str(_) = &tokens[i]
            && i + 1 < tokens.len()
            && tokens[i + 1] == Tok::OpenParen
            && let Some(end) = find_matching_paren(tokens, i + 1)
        {
            let inner = &tokens[i + 2..end];
            if let Some(Tok::Ident(inner_fn)) = inner.first()
                && inner_fn == "project"
                && inner.len() > 1
                && inner[1] == Tok::OpenParen
                && let Some(project_end) = find_matching_paren(inner, 1)
            {
                let project_tokens = &inner[2..project_end];
                if let Some(rd) = parse_project_ref(project_tokens) {
                    deps.push(rd);
                }
                i = end + 1;
                continue;
            }
        }

        let scope_name = match &tokens[i] {
            Tok::Ident(name) => name.clone(),
            _ => {
                i += 1;
                continue;
            }
        };

        if is_skip_keyword(&scope_name) {
            i += 1;
            continue;
        }

        let next = i + 1;

        // PATTERN: scope ( ... )  — parenthesized dependency
        if next < tokens.len() && tokens[next] == Tok::OpenParen {
            let paren_end = find_matching_paren(tokens, next);
            if let Some(end) = paren_end {
                let inner = &tokens[next + 1..end];
                parse_paren_content(&scope_name, inner, &mut deps);
                i = end + 1;
                continue;
            }
        }

        // PATTERN: scope group: ..., name: ..., version: ... (named params without parens)
        if next < tokens.len()
            && let Tok::Ident(ref label) = tokens[next]
            && label == "group"
            && next + 1 < tokens.len()
            && tokens[next + 1] == Tok::Colon
            && let Some((rd, consumed)) = parse_named_params(&scope_name, &tokens[next..])
        {
            deps.push(rd);
            i = next + consumed;
            continue;
        }

        // PATTERN: scope 'string:notation' (string notation)
        if next < tokens.len()
            && matches!(
                tokens.get(next),
                Some(Tok::Str(_)) | Some(Tok::MalformedStr(_))
            )
        {
            let (val, is_malformed) = match &tokens[next] {
                Tok::Str(val) => (val.as_str(), false),
                Tok::MalformedStr(val) => (val.as_str(), true),
                _ => unreachable!(),
            };

            if !val.contains(':') {
                i = next + 1;
                continue;
            }

            if val.chars().next().is_some_and(|c| c.is_whitespace()) {
                break;
            }

            // `scope 'str', { closure }` → skip (unparenthesized call with trailing closure)
            if next + 1 < tokens.len()
                && tokens[next + 1] == Tok::Comma
                && next + 2 < tokens.len()
                && tokens[next + 2] == Tok::OpenBrace
            {
                i = next + 1;
                continue;
            }
            let is_multi = i + 2 < tokens.len()
                && tokens[next + 1] == Tok::Comma
                && matches!(tokens.get(next + 2), Some(Tok::Str(_)));
            let effective_scope = if is_multi { "" } else { &scope_name };
            let rd = parse_colon_string(val, effective_scope);
            deps.push(rd);
            if is_malformed {
                break;
            }
            i = next + 1;
            while i < tokens.len() && tokens[i] == Tok::Comma {
                i += 1;
                if i < tokens.len()
                    && let Tok::Str(ref v2) = tokens[i]
                    && v2.contains(':')
                {
                    deps.push(parse_colon_string(v2, ""));
                    i += 1;
                    continue;
                }
                break;
            }
            continue;
        }

        // PATTERN: scope ident.attr (variable reference / dotted identifier)
        // Note: Skip references starting with "dependencies." as Python's pygmars
        // relabels the "dependencies" token, breaking the DEPENDENCY-5 grammar rule.
        if next < tokens.len()
            && let Tok::Ident(ref val) = tokens[next]
            && val.contains('.')
            && !val.starts_with("dependencies.")
            && let Some(last_seg) = val.rsplit('.').next()
            && !last_seg.is_empty()
        {
            deps.push(RawDep {
                namespace: String::new(),
                name: truncate_field(last_seg.to_string()),
                version: String::new(),
                scope: truncate_field(scope_name.clone()),
                catalog_alias: val
                    .strip_prefix("libs.")
                    .map(|alias| truncate_field(alias.to_string())),
                project_path: None,
            });
            i = next + 1;
            continue;
        }

        // PATTERN: scope project(':module') — project reference without parens
        if next < tokens.len()
            && let Tok::Ident(ref name) = tokens[next]
            && name == "project"
            && next + 1 < tokens.len()
            && tokens[next + 1] == Tok::OpenParen
            && let Some(end) = find_matching_paren(tokens, next + 1)
        {
            let inner = &tokens[next + 2..end];
            if let Some(rd) = parse_project_ref(inner) {
                deps.push(rd);
            }
            i = end + 1;
            continue;
        }

        i += 1;
    }

    deps
}

fn is_skip_keyword(name: &str) -> bool {
    matches!(
        name,
        "plugins"
            | "apply"
            | "ext"
            | "configurations"
            | "repositories"
            | "subprojects"
            | "allprojects"
            | "buildscript"
            | "pluginManager"
            | "publishing"
            | "sourceSets"
            | "tasks"
            | "task"
    )
}

fn parse_paren_content(scope: &str, tokens: &[Tok], deps: &mut Vec<RawDep>) {
    if tokens.is_empty() {
        return;
    }

    // Check for bracket-enclosed maps: [group: ..., name: ..., version: ...]
    if tokens[0] == Tok::OpenBracket {
        parse_bracket_maps(tokens, deps);
        return;
    }

    // Check for named parameters: group: 'x' or group = "x"
    if let Some(Tok::Ident(label)) = tokens.first()
        && label == "group"
        && tokens.len() > 1
        && tokens[1] == Tok::Colon
    {
        if let Some((rd, _)) = parse_named_params("", tokens) {
            deps.push(rd);
        }
        return;
    }

    // Check for nested function call or project reference
    if let Some(Tok::Ident(inner_fn)) = tokens.first()
        && tokens.len() > 1
        && tokens[1] == Tok::OpenParen
    {
        if inner_fn == "project" {
            if let Some(end) = find_matching_paren(tokens, 1) {
                let inner = &tokens[2..end];
                if let Some(rd) = parse_project_ref(inner) {
                    deps.push(rd);
                }
            }
            return;
        }

        if let Some(end) = find_matching_paren(tokens, 1) {
            let inner = &tokens[2..end];
            if let Some(Tok::Str(val)) = inner.first()
                && val.contains(':')
            {
                deps.push(parse_colon_string(val, inner_fn));
                return;
            }
        }
    }

    // Simple string: ("g:n:v")
    if let Some(Tok::Str(val)) = tokens.first()
        && val.contains(':')
    {
        deps.push(parse_colon_string(val, scope));
    }
}

fn parse_bracket_maps(tokens: &[Tok], deps: &mut Vec<RawDep>) {
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == Tok::OpenBracket
            && let Some(end) = find_matching_bracket(tokens, i)
        {
            let map_tokens = &tokens[i + 1..end];
            if let Some(rd) = parse_map_entries(map_tokens)
                && !contains_equivalent_map_dep(deps, &rd)
            {
                deps.push(rd);
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
}

fn contains_equivalent_map_dep(existing: &[RawDep], candidate: &RawDep) -> bool {
    existing.iter().any(|dep| {
        dep.name == candidate.name
            && dep.version == candidate.version
            && dep.scope == candidate.scope
            && (dep.namespace == candidate.namespace
                || dep.namespace.is_empty()
                || candidate.namespace.is_empty())
    })
}

fn parse_map_entries(tokens: &[Tok]) -> Option<RawDep> {
    let mut name = String::new();
    let mut version = String::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Tok::Ident(ref label) = tokens[i]
            && i + 2 < tokens.len()
            && tokens[i + 1] == Tok::Colon
            && let Tok::Str(ref val) = tokens[i + 2]
        {
            match label.as_str() {
                "name" => name = truncate_field(val.clone()),
                "version" => version = truncate_field(val.clone()),
                _ => {}
            }
            i += 3;
            if i < tokens.len() && tokens[i] == Tok::Comma {
                i += 1;
            }
            continue;
        }
        i += 1;
    }

    if name.is_empty() {
        return None;
    }

    Some(RawDep {
        namespace: String::new(),
        name,
        version,
        scope: String::new(),
        catalog_alias: None,
        project_path: None,
    })
}

fn parse_named_params(scope: &str, tokens: &[Tok]) -> Option<(RawDep, usize)> {
    let mut group = String::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Tok::Ident(ref label) = tokens[i]
            && i + 2 < tokens.len()
            && tokens[i + 1] == Tok::Colon
            && let Tok::Str(ref val) = tokens[i + 2]
        {
            match label.as_str() {
                "group" => group = truncate_field(val.clone()),
                "name" => name = truncate_field(val.clone()),
                "version" => version = truncate_field(val.clone()),
                _ => {}
            }
            i += 3;
            if i < tokens.len() && tokens[i] == Tok::Comma {
                i += 1;
            }
            continue;
        }
        break;
    }

    if name.is_empty() {
        return None;
    }

    Some((
        RawDep {
            namespace: group,
            name,
            version,
            scope: scope.to_string(),
            catalog_alias: None,
            project_path: None,
        },
        i,
    ))
}

fn parse_project_ref(tokens: &[Tok]) -> Option<RawDep> {
    if let Some(Tok::Str(val)) = tokens.first() {
        let module_name = val.trim_start_matches(':');
        let mut segments = module_name
            .split(':')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let name = segments.pop().unwrap_or(module_name);
        if name.is_empty() {
            return None;
        }
        return Some(RawDep {
            namespace: if segments.is_empty() {
                String::new()
            } else {
                truncate_field(segments.join("/"))
            },
            name: truncate_field(name.to_string()),
            version: String::new(),
            scope: "project".to_string(),
            catalog_alias: None,
            project_path: Some(truncate_field(module_name.to_string())),
        });
    }
    None
}

fn parse_colon_string(val: &str, scope: &str) -> RawDep {
    let parts: Vec<&str> = val.split(':').collect();
    let (namespace, name, version) = match parts.len() {
        n if n >= 4 => (
            truncate_field(parts[0].to_string()),
            truncate_field(parts[1].to_string()),
            truncate_field(parts[2].to_string()),
        ),
        3 => (
            truncate_field(parts[0].to_string()),
            truncate_field(parts[1].to_string()),
            truncate_field(parts[2].to_string()),
        ),
        2 => (
            truncate_field(parts[0].to_string()),
            truncate_field(parts[1].to_string()),
            String::new(),
        ),
        _ => (
            String::new(),
            truncate_field(val.to_string()),
            String::new(),
        ),
    };

    RawDep {
        namespace,
        name,
        version,
        scope: truncate_field(scope.to_string()),
        catalog_alias: None,
        project_path: None,
    }
}

fn find_matching_paren(tokens: &[Tok], start: usize) -> Option<usize> {
    if tokens.get(start) != Some(&Tok::OpenParen) {
        return None;
    }
    let mut depth = 1;
    let mut i = start + 1;
    while i < tokens.len() && depth > 0 {
        match &tokens[i] {
            Tok::OpenParen => {
                depth += 1;
                if depth > MAX_RECURSION_DEPTH {
                    warn!(
                        "Gradle parser: nesting depth exceeded {} in find_matching_paren",
                        MAX_RECURSION_DEPTH
                    );
                    break;
                }
            }
            Tok::CloseParen => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_matching_bracket(tokens: &[Tok], start: usize) -> Option<usize> {
    if tokens.get(start) != Some(&Tok::OpenBracket) {
        return None;
    }
    let mut depth = 1;
    let mut i = start + 1;
    while i < tokens.len() && depth > 0 {
        match &tokens[i] {
            Tok::OpenBracket => {
                depth += 1;
                if depth > MAX_RECURSION_DEPTH {
                    warn!(
                        "Gradle parser: nesting depth exceeded {} in find_matching_bracket",
                        MAX_RECURSION_DEPTH
                    );
                    break;
                }
            }
            Tok::CloseBracket => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Dependency construction
// ---------------------------------------------------------------------------

fn create_dependency(raw: &RawDep) -> Option<Dependency> {
    let namespace = raw.namespace.as_str();
    let name = raw.name.as_str();
    let version = raw.version.as_str();
    let scope = raw.scope.as_str();
    if name.is_empty() {
        return None;
    }

    let mut purl = PackageUrl::new("maven", name).ok()?;

    if !namespace.is_empty() {
        purl.with_namespace(namespace).ok()?;
    }

    if !version.is_empty() {
        purl.with_version(version).ok()?;
    }

    let (is_runtime, is_optional) = classify_scope(scope);
    let is_pinned = !version.is_empty();

    let purl_string = truncate_field(purl.to_string().replace("$", "%24").replace('\'', "%27"));
    let mut extra_data = std::collections::HashMap::new();
    if let Some(alias) = &raw.catalog_alias {
        extra_data.insert(
            "catalog_alias".to_string(),
            json!(truncate_field(alias.clone())),
        );
    }
    if let Some(project_path) = &raw.project_path {
        extra_data.insert(
            "project_path".to_string(),
            json!(truncate_field(project_path.clone())),
        );
    }

    Some(Dependency {
        purl: Some(purl_string),
        extracted_requirement: Some(truncate_field(version.to_string())),
        scope: Some(truncate_field(scope.to_string())),
        is_runtime: Some(is_runtime),
        is_optional: Some(is_optional),
        is_pinned: Some(is_pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn classify_scope(scope: &str) -> (bool, bool) {
    let scope_lower = scope.to_lowercase();

    if scope_lower.contains("test") {
        return (false, true);
    }

    if matches!(
        scope_lower.as_str(),
        "compileonly" | "compileonlyapi" | "annotationprocessor" | "kapt" | "ksp"
    ) {
        return (false, false);
    }

    (true, false)
}

#[derive(Debug, Clone)]
struct GradleCatalogEntry {
    namespace: String,
    name: String,
    version: Option<String>,
}

fn resolve_gradle_version_catalog_aliases(path: &Path, dependencies: &mut [Dependency]) {
    let Some(catalog_path) = find_gradle_version_catalog(path) else {
        return;
    };
    let Some(entries) = parse_gradle_version_catalog(&catalog_path) else {
        return;
    };

    for dep in dependencies.iter_mut() {
        let alias = dep
            .extra_data
            .as_ref()
            .and_then(|data| data.get("catalog_alias"))
            .and_then(|value| value.as_str());
        let Some(alias) = alias else {
            continue;
        };
        let Some(entry) = entries.get(alias) else {
            continue;
        };

        let mut purl = PackageUrl::new("maven", &entry.name).ok();
        if let Some(ref mut purl) = purl {
            if !entry.namespace.is_empty() {
                let _ = purl.with_namespace(&entry.namespace);
            }
            if let Some(version) = &entry.version {
                let _ = purl.with_version(version);
            }
        }

        dep.purl = purl.map(|p| truncate_field(p.to_string()));
        dep.extracted_requirement = entry.version.as_ref().map(|v| truncate_field(v.clone()));
        dep.is_pinned = Some(entry.version.is_some());
    }
}

fn find_gradle_version_catalog(path: &Path) -> Option<std::path::PathBuf> {
    for ancestor in path.ancestors() {
        let nested = ancestor.join("gradle").join("libs.versions.toml");
        if nested.is_file() {
            return Some(nested);
        }

        let sibling = ancestor.join("libs.versions.toml");
        if sibling.is_file() {
            return Some(sibling);
        }
    }

    None
}

fn parse_gradle_version_catalog(
    path: &Path,
) -> Option<std::collections::HashMap<String, GradleCatalogEntry>> {
    let content = read_file_to_string(path, None).ok()?;
    let mut section = "";
    let mut versions = std::collections::HashMap::new();
    let mut libraries = std::collections::HashMap::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.trim_matches(&['[', ']'][..]);
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim().to_string();

        match section {
            "versions" => {
                versions.insert(key, truncate_field(strip_quotes(&value).to_string()));
            }
            "libraries" => {
                libraries.insert(key, value);
            }
            _ => {}
        }
    }

    let mut result = std::collections::HashMap::new();
    for (alias, raw_value) in libraries.into_iter().take(MAX_ITERATION_COUNT) {
        let Some(entry) = parse_gradle_catalog_entry(&raw_value, &versions) else {
            continue;
        };
        result.insert(truncate_field(alias.replace('-', ".")), entry);
    }

    Some(result)
}

fn parse_gradle_catalog_entry(
    raw_value: &str,
    versions: &std::collections::HashMap<String, String>,
) -> Option<GradleCatalogEntry> {
    if raw_value.starts_with('"') && raw_value.ends_with('"') {
        let notation = strip_quotes(raw_value);
        let mut parts = notation.split(':');
        let namespace = truncate_field(parts.next()?.to_string());
        let name = truncate_field(parts.next()?.to_string());
        let version = parts.next().map(|v| truncate_field(v.to_string()));
        return Some(GradleCatalogEntry {
            namespace,
            name,
            version,
        });
    }

    if !(raw_value.starts_with('{') && raw_value.ends_with('}')) {
        return None;
    }

    let inner = &raw_value[1..raw_value.len() - 1];
    let mut fields = std::collections::HashMap::new();
    for pair in inner.split(',').take(MAX_ITERATION_COUNT) {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        fields.insert(
            truncate_field(key.trim().to_string()),
            truncate_field(strip_quotes(value.trim()).to_string()),
        );
    }

    let (namespace, name) = if let Some(module) = fields.get("module") {
        let (group, artifact) = module.split_once(':')?;
        (
            truncate_field(group.to_string()),
            truncate_field(artifact.to_string()),
        )
    } else {
        (
            truncate_field(fields.get("group")?.to_string()),
            truncate_field(fields.get("name")?.to_string()),
        )
    };

    let version = if let Some(version) = fields.get("version") {
        Some(truncate_field(version.to_string()))
    } else if let Some(version_ref) = fields.get("version.ref") {
        versions.get(version_ref).cloned().map(truncate_field)
    } else {
        None
    };

    Some(GradleCatalogEntry {
        namespace,
        name,
        version,
    })
}

fn strip_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

fn extract_gradle_license_metadata(
    tokens: &[Tok],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<crate::models::LicenseDetection>,
) {
    let mut i = 0;
    while i < tokens.len() {
        if let Tok::Ident(name) = &tokens[i]
            && name == "licenses"
            && i + 1 < tokens.len()
            && tokens[i + 1] == Tok::OpenBrace
            && let Some(block_end) = find_matching_brace(tokens, i + 1)
        {
            let inner = &tokens[i + 2..block_end];
            if let Some((license_name, license_url)) = parse_license_block(inner) {
                let extracted =
                    format_gradle_license_statement(&license_name, license_url.as_deref());
                let declared_candidate =
                    derive_gradle_license_expression(&license_name, license_url.as_deref());
                if let Some(declared_candidate) = declared_candidate
                    && let Some(normalized) = normalize_spdx_expression(&declared_candidate)
                {
                    let matched_text = extracted.as_deref().unwrap_or(&declared_candidate);
                    let (declared, declared_spdx, detections) = build_declared_license_data(
                        normalized,
                        DeclaredLicenseMatchMetadata::single_line(matched_text),
                    );
                    return (
                        extracted.map(truncate_field),
                        declared.map(truncate_field),
                        declared_spdx.map(truncate_field),
                        detections,
                    );
                }

                return (
                    extracted.map(truncate_field),
                    None,
                    None,
                    empty_declared_license_data().2,
                );
            }
            i = block_end + 1;
            continue;
        }
        i += 1;
    }

    (None, None, None, Vec::new())
}

fn parse_license_block(tokens: &[Tok]) -> Option<(String, Option<String>)> {
    let mut i = 0;
    while i < tokens.len() {
        if let Tok::Ident(name) = &tokens[i]
            && name == "license"
            && i + 1 < tokens.len()
            && tokens[i + 1] == Tok::OpenBrace
            && let Some(block_end) = find_matching_brace(tokens, i + 1)
        {
            let mut license_name = None;
            let mut license_url = None;
            let block = &tokens[i + 2..block_end];
            let mut j = 0;
            while j < block.len() {
                if let Tok::Ident(label) = &block[j] {
                    let normalized = label.strip_suffix(".set").unwrap_or(label);
                    if (normalized == "name" || normalized == "url")
                        && let Some(value) = next_string_literal(block, j + 1)
                    {
                        if normalized == "name" {
                            license_name = Some(value);
                        } else {
                            license_url = Some(value);
                        }
                    }
                }
                j += 1;
            }

            return license_name.map(|name| (name, license_url));
        }
        i += 1;
    }
    None
}

fn next_string_literal(tokens: &[Tok], start: usize) -> Option<String> {
    for token in tokens.iter().skip(start) {
        match token {
            Tok::Str(value) => return Some(truncate_field(value.clone())),
            Tok::MalformedStr(value) => return Some(truncate_field(value.clone())),
            Tok::Ident(_) | Tok::Colon | Tok::Equals | Tok::OpenParen | Tok::CloseParen => continue,
            _ => break,
        }
    }
    None
}

fn find_matching_brace(tokens: &[Tok], start: usize) -> Option<usize> {
    if tokens.get(start) != Some(&Tok::OpenBrace) {
        return None;
    }
    let mut depth = 1;
    let mut i = start + 1;
    while i < tokens.len() && depth > 0 {
        match &tokens[i] {
            Tok::OpenBrace => {
                depth += 1;
                if depth > MAX_RECURSION_DEPTH {
                    warn!(
                        "Gradle parser: nesting depth exceeded {} in find_matching_brace",
                        MAX_RECURSION_DEPTH
                    );
                    break;
                }
            }
            Tok::CloseBrace => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn format_gradle_license_statement(name: &str, url: Option<&str>) -> Option<String> {
    let mut output = format!("- license:\n    name: {name}\n");
    if let Some(url) = url {
        output.push_str(&format!("    url: {url}\n"));
    }
    Some(truncate_field(output))
}

fn derive_gradle_license_expression(name: &str, url: Option<&str>) -> Option<String> {
    let trimmed = name.trim();
    let candidates = [trimmed, url.unwrap_or("")];

    for candidate in candidates {
        let lower = candidate.to_ascii_lowercase();
        if trimmed == "Apache-2.0"
            || lower.contains("apache-2.0")
            || lower.contains("apache license, version 2.0")
            || lower.contains("apache.org/licenses/license-2.0")
        {
            return Some(truncate_field("Apache-2.0".to_string()));
        }
        if trimmed == "MIT" || lower.contains("opensource.org/licenses/mit") {
            return Some(truncate_field("MIT".to_string()));
        }
        if trimmed == "BSD-2-Clause" || trimmed == "BSD-3-Clause" {
            return Some(truncate_field(trimmed.to_string()));
        }
    }

    None
}

crate::register_parser!(
    "Gradle build script",
    &["**/build.gradle", "**/build.gradle.kts"],
    "maven",
    "Java",
    Some("https://gradle.org/"),
);

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_match() {
        assert!(GradleParser::is_match(Path::new("build.gradle")));
        assert!(GradleParser::is_match(Path::new("build.gradle.kts")));
        assert!(GradleParser::is_match(Path::new("project/build.gradle")));
        assert!(!GradleParser::is_match(Path::new("build.xml")));
        assert!(!GradleParser::is_match(Path::new("settings.gradle")));
    }

    #[test]
    fn test_extract_simple_dependencies() {
        let content = r#"
dependencies {
    compile 'org.apache.commons:commons-text:1.1'
    testCompile 'junit:junit:4.12'
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);

        let dep1 = &deps[0];
        assert_eq!(
            dep1.purl,
            Some("pkg:maven/org.apache.commons/commons-text@1.1".to_string())
        );
        assert_eq!(dep1.scope, Some("compile".to_string()));
        assert_eq!(dep1.is_runtime, Some(true));
        assert_eq!(dep1.is_pinned, Some(true));

        let dep2 = &deps[1];
        assert_eq!(dep2.purl, Some("pkg:maven/junit/junit@4.12".to_string()));
        assert_eq!(dep2.scope, Some("testCompile".to_string()));
        assert_eq!(dep2.is_runtime, Some(false));
        assert_eq!(dep2.is_optional, Some(true));
    }

    #[test]
    fn test_extract_parens_notation() {
        let content = r#"
dependencies {
    implementation("com.example:library:1.0.0")
    testImplementation("junit:junit:4.13")
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/com.example/library@1.0.0".to_string())
        );
    }

    #[test]
    fn test_extract_named_parameters() {
        let content = r#"
dependencies {
    api group: 'com.google.guava', name: 'guava', version: '30.1-jre'
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/com.google.guava/guava@30.1-jre".to_string())
        );
        assert_eq!(deps[0].scope, Some("api".to_string()));
    }

    #[test]
    fn test_multiple_dependency_blocks_all_parsed() {
        let content = r#"
dependencies {
    implementation 'org.scala-lang:scala-library:2.11.12'
}

dependencies {
    implementation 'commons-collections:commons-collections:3.2.2'
    testImplementation 'junit:junit:4.13'
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 3);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/org.scala-lang/scala-library@2.11.12".to_string())
        );
        assert_eq!(
            deps[1].purl,
            Some("pkg:maven/commons-collections/commons-collections@3.2.2".to_string())
        );
        assert_eq!(deps[2].purl, Some("pkg:maven/junit/junit@4.13".to_string()));
        assert_eq!(deps[2].scope, Some("testImplementation".to_string()));
    }

    #[test]
    fn test_nested_dependency_blocks_all_parsed() {
        let content = r#"
buildscript {
    dependencies {
        classpath("org.eclipse.jgit:org.eclipse.jgit:$jgitVersion")
    }
}

subprojects {
    dependencies {
        implementation("org.jetbrains.kotlin:kotlin-stdlib-jdk8:$kotlinPluginVersion")
    }
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);

        assert_eq!(deps.len(), 2);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/org.eclipse.jgit/org.eclipse.jgit@%24jgitVersion".to_string())
        );
        assert_eq!(deps[0].scope, Some("classpath".to_string()));
        assert_eq!(
            deps[1].purl,
            Some(
                "pkg:maven/org.jetbrains.kotlin/kotlin-stdlib-jdk8@%24kotlinPluginVersion"
                    .to_string()
            )
        );
        assert_eq!(deps[1].scope, Some("implementation".to_string()));
    }

    #[test]
    fn test_no_version() {
        let content = r#"
dependencies {
    compile 'org.example:library'
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].is_pinned, Some(false));
        assert_eq!(deps[0].extracted_requirement, Some("".to_string()));
    }

    #[test]
    fn test_nested_function_calls() {
        let content = r#"
dependencies {
    implementation(enforcedPlatform("com.fasterxml.jackson:jackson-bom:2.12.2"))
    testImplementation(platform("org.junit:junit-bom:5.7.2"))
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/com.fasterxml.jackson/jackson-bom@2.12.2".to_string())
        );
        assert_eq!(deps[0].scope, Some("enforcedPlatform".to_string()));
        assert_eq!(deps[1].scope, Some("platform".to_string()));
    }

    #[test]
    fn test_map_format() {
        let content = r#"
dependencies {
    runtimeOnly(
        [group: 'org.jacoco', name: 'org.jacoco.ant', version: '0.7.4.201502262128'],
        [group: 'org.jacoco', name: 'org.jacoco.agent', version: '0.7.4.201502262128']
    )
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].scope, Some("".to_string()));
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/org.jacoco.ant@0.7.4.201502262128".to_string())
        );
    }

    #[test]
    fn test_bracket_map_dedupes_exact_string_overlap() {
        let content = r#"
dependencies {
    runtimeOnly 'org.springframework:spring-core:2.5',
            'org.springframework:spring-aop:2.5'
    runtimeOnly(
        [group: 'org.springframework', name: 'spring-core', version: '2.5'],
        [group: 'org.springframework', name: 'spring-aop', version: '2.5']
    )
}
"#;

        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/org.springframework/spring-core@2.5".to_string())
        );
        assert_eq!(
            deps[1].purl,
            Some("pkg:maven/org.springframework/spring-aop@2.5".to_string())
        );
    }

    #[test]
    fn test_malformed_string_stops_cascading_false_positives() {
        let content = r#"
dependencies {
    implementation "com.fasterxml.jackson:jackson-bom:2.12.2'
    implementation" com.fasterxml.jackson.core:jackson-core"
    testImplementation 'org.junit:junit-bom:5.7.2'"
    testImplementation "org.junit.platform:junit-platform-commons"
}
"#;

        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/com.fasterxml.jackson/jackson-bom@2.12.2%27".to_string())
        );
    }

    #[test]
    fn test_project_references() {
        let content = r#"
dependencies {
    implementation(project(":documentation"))
    implementation(project(":basics"))
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].scope, Some("project".to_string()));
        assert_eq!(deps[0].purl, Some("pkg:maven/documentation".to_string()));
        assert_eq!(deps[1].purl, Some("pkg:maven/basics".to_string()));
    }

    #[test]
    fn test_nested_project_references_preserve_parent_path() {
        let content = r#"
dependencies {
    implementation(project(":libs:download"))
    implementation(project(":libs:index"))
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].purl, Some("pkg:maven/libs/download".to_string()));
        assert_eq!(deps[0].scope, Some("project".to_string()));
        assert_eq!(deps[1].purl, Some("pkg:maven/libs/index".to_string()));
    }

    #[test]
    fn test_compile_only_is_not_runtime() {
        let content = r#"
dependencies {
    compileOnly 'org.antlr:antlr:2.7.7'
    compileOnlyApi 'com.example:annotations:1.0.0'
    testCompileOnly 'junit:junit:4.13'
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);

        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].scope, Some("compileOnly".to_string()));
        assert_eq!(deps[0].is_runtime, Some(false));
        assert_eq!(deps[0].is_optional, Some(false));

        assert_eq!(deps[1].scope, Some("compileOnlyApi".to_string()));
        assert_eq!(deps[1].is_runtime, Some(false));
        assert_eq!(deps[1].is_optional, Some(false));

        assert_eq!(deps[2].scope, Some("testCompileOnly".to_string()));
        assert_eq!(deps[2].is_runtime, Some(false));
        assert_eq!(deps[2].is_optional, Some(true));
    }

    #[test]
    fn test_version_catalog_alias_resolution_from_libs_versions_toml() {
        let temp_dir = tempdir().unwrap();
        let gradle_dir = temp_dir.path().join("gradle");
        std::fs::create_dir_all(&gradle_dir).unwrap();

        std::fs::write(
            gradle_dir.join("libs.versions.toml"),
            r#"
[versions]
androidxAppcompat = "1.7.0"

[libraries]
androidx-appcompat = { module = "androidx.appcompat:appcompat", version.ref = "androidxAppcompat" }
guardianproject-panic = { group = "info.guardianproject", name = "panic", version = "1.0.0" }
"#,
        )
        .unwrap();

        let build_gradle = temp_dir.path().join("build.gradle");
        std::fs::write(
            &build_gradle,
            r#"
dependencies {
    implementation libs.androidx.appcompat
    fullImplementation libs.guardianproject.panic
}
"#,
        )
        .unwrap();

        let package_data = GradleParser::extract_first_package(&build_gradle);

        assert_eq!(package_data.dependencies.len(), 2);
        assert_eq!(
            package_data.dependencies[0].purl,
            Some("pkg:maven/androidx.appcompat/appcompat@1.7.0".to_string())
        );
        assert_eq!(
            package_data.dependencies[0].scope,
            Some("implementation".to_string())
        );
        assert_eq!(
            package_data.dependencies[1].purl,
            Some("pkg:maven/info.guardianproject/panic@1.0.0".to_string())
        );
        assert_eq!(
            package_data.dependencies[1].scope,
            Some("fullImplementation".to_string())
        );
    }

    #[test]
    fn test_extract_gradle_license_metadata_from_pom_block() {
        let content = r#"
plugins {
    id 'java-library'
    id 'maven'
}

dependencies {
    api 'org.apache.commons:commons-text:1.1'
}

configure(install.repositories.mavenInstaller) {
    pom.project {
        licenses {
            license {
                name 'The Apache License, Version 2.0'
                url 'http://www.apache.org/licenses/LICENSE-2.0.txt'
            }
        }
    }
}
"#;

        let temp_dir = tempdir().unwrap();
        let build_gradle = temp_dir.path().join("build.gradle");
        std::fs::write(&build_gradle, content).unwrap();

        let package_data = GradleParser::extract_first_package(&build_gradle);

        assert_eq!(
            package_data.extracted_license_statement,
            Some(
                "- license:\n    name: The Apache License, Version 2.0\n    url: http://www.apache.org/licenses/LICENSE-2.0.txt\n"
                    .to_string()
            )
        );
        assert_eq!(
            package_data.declared_license_expression_spdx,
            Some("Apache-2.0".to_string())
        );
    }

    #[test]
    fn test_parse_gradle_version_catalog_helper() {
        let temp_dir = tempdir().unwrap();
        let catalog_path = temp_dir.path().join("libs.versions.toml");
        std::fs::write(
            &catalog_path,
            r#"
[versions]
androidxAppcompat = "1.7.0"

[libraries]
androidx-appcompat = { module = "androidx.appcompat:appcompat", version.ref = "androidxAppcompat" }
"#,
        )
        .unwrap();

        let entries = parse_gradle_version_catalog(&catalog_path).unwrap();
        let entry = entries.get("androidx.appcompat").unwrap();

        assert_eq!(entry.namespace, "androidx.appcompat");
        assert_eq!(entry.name, "appcompat");
        assert_eq!(entry.version.as_deref(), Some("1.7.0"));
    }

    #[test]
    fn test_string_interpolation() {
        let content = r#"
dependencies {
    compile "com.amazonaws:aws-java-sdk-core:${awsVer}"
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].extracted_requirement, Some("${awsVer}".to_string()));
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/com.amazonaws/aws-java-sdk-core@%24%7BawsVer%7D".to_string())
        );
    }

    #[test]
    fn test_multi_value_string_notation() {
        let content = r#"
dependencies {
    runtimeOnly 'org.springframework:spring-core:2.5',
            'org.springframework:spring-aop:2.5'
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].scope, Some("".to_string()));
        assert_eq!(deps[1].scope, Some("".to_string()));
    }

    #[test]
    fn test_kotlin_quoted_scope_not_extracted() {
        let content = r#"
dependencies {
    "js"("jquery:jquery:3.2.1@js")
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_kotlin_quoted_scope_project_reference_extracted() {
        let content = r#"
subprojects {
    dependencies {
        "testImplementation"(project(":utils:test-utils"))
    }
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].scope, Some("project".to_string()));
        assert_eq!(deps[0].purl, Some("pkg:maven/utils/test-utils".to_string()));
    }

    #[test]
    fn test_closure_after_dependency() {
        let content = r#"
dependencies {
    runtimeOnly('org.hibernate:hibernate:3.0.5') {
        transitive = true
    }
}
"#;
        let tokens = lex(content);
        let deps = extract_dependencies(&tokens);
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].purl,
            Some("pkg:maven/org.hibernate/hibernate@3.0.5".to_string())
        );
        assert_eq!(deps[0].scope, Some("runtimeOnly".to_string()));
    }
}
