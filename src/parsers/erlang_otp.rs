// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{
    DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage, Sha256Digest,
};
use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, MAX_RECURSION_DEPTH, read_file_to_string, truncate_field,
};

use super::PackageParser;

// ── Parser structs ──

pub struct ErlangAppSrcParser;
pub struct RebarConfigParser;
pub struct RebarLockParser;

// ── Erlang term AST ──

#[derive(Clone, Debug)]
enum ErlTerm {
    Atom(String),
    String(String),
    Binary(String),
    Integer(i64),
    Float(f64),
    Tuple(Vec<ErlTerm>),
    List(Vec<ErlTerm>),
    Map(Vec<(ErlTerm, ErlTerm)>),
}

// ── Erlang term parser ──

struct ErlParser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

impl ErlParser {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            depth: 0,
        }
    }

    fn parse_term(&mut self) -> Result<ErlTerm, String> {
        if self.depth >= MAX_RECURSION_DEPTH {
            return Err("recursion depth exceeded".to_string());
        }
        self.depth += 1;
        let result = self.parse_term_inner();
        self.depth -= 1;
        result
    }

    fn parse_term_inner(&mut self) -> Result<ErlTerm, String> {
        self.skip_whitespace_and_comments();
        match self.peek() {
            Some('{') => self.parse_tuple(),
            Some('[') => self.parse_list(),
            Some('#') if self.peek_n(1) == Some('{') => self.parse_map(),
            Some('"') => self.parse_string().map(ErlTerm::String),
            Some('<') if self.peek_n(1) == Some('<') => self.parse_binary().map(ErlTerm::Binary),
            Some('\'') => self.parse_quoted_atom().map(ErlTerm::Atom),
            Some(c) if c.is_ascii_digit() || c == '-' => self.parse_number(),
            Some(c) if c.is_ascii_lowercase() || c == '_' => self.parse_atom_or_bool(),
            Some(c) => Err(format!(
                "Unexpected character '{}' at position {}",
                c, self.pos
            )),
            None => Err("Unexpected end of input".to_string()),
        }
    }

    fn parse_tuple(&mut self) -> Result<ErlTerm, String> {
        self.expect('{')?;
        let items = self.parse_comma_separated('}')?;
        Ok(ErlTerm::Tuple(items))
    }

    fn parse_list(&mut self) -> Result<ErlTerm, String> {
        self.expect('[')?;
        let items = self.parse_comma_separated(']')?;
        Ok(ErlTerm::List(items))
    }

    fn parse_map(&mut self) -> Result<ErlTerm, String> {
        self.expect('#')?;
        self.expect('{')?;

        let mut entries = Vec::new();
        let mut count = 0usize;

        loop {
            self.skip_whitespace_and_comments();
            if self.peek() == Some('}') {
                self.pos += 1;
                break;
            }

            if count >= MAX_ITERATION_COUNT {
                return Err("too many map entries".to_string());
            }

            let key = self.parse_term()?;
            self.skip_whitespace_and_comments();

            match (self.peek(), self.peek_n(1)) {
                (Some('='), Some('>')) | (Some(':'), Some('=')) => {
                    self.pos += 2;
                }
                _ => {
                    return Err(format!(
                        "Expected map association operator at position {}",
                        self.pos
                    ));
                }
            }

            let value = self.parse_term()?;
            entries.push((key, value));
            count += 1;

            self.skip_whitespace_and_comments();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    return Err(format!(
                        "Expected ',' or '}}' in map but found '{}' at position {}",
                        c, self.pos
                    ));
                }
                None => return Err("Unterminated map literal".to_string()),
            }
        }

        Ok(ErlTerm::Map(entries))
    }

    fn parse_comma_separated(&mut self, closing: char) -> Result<Vec<ErlTerm>, String> {
        let mut items = Vec::new();
        let mut count = 0usize;
        loop {
            self.skip_whitespace_and_comments();
            if self.peek() == Some(closing) {
                self.pos += 1;
                break;
            }
            if count >= MAX_ITERATION_COUNT {
                return Err("too many items".to_string());
            }
            items.push(self.parse_term()?);
            count += 1;
            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.pos += 1;
            } else if self.peek() == Some('|') {
                // list tail syntax: [H | T] — skip rest
                self.pos += 1;
                self.parse_term()?;
                self.skip_whitespace_and_comments();
                if self.peek() == Some(closing) {
                    self.pos += 1;
                }
                break;
            }
        }
        Ok(items)
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| "Unterminated string escape".to_string())?;
                    self.pos += 1;
                    out.push(match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                }
                other => out.push(other),
            }
        }
        Err("Unterminated string literal".to_string())
    }

    fn parse_binary(&mut self) -> Result<String, String> {
        self.expect('<')?;
        self.expect('<')?;
        self.skip_whitespace_and_comments();
        let value = if self.peek() == Some('"') {
            self.parse_string()?
        } else {
            String::new()
        };
        self.skip_whitespace_and_comments();
        self.expect('>')?;
        self.expect('>')?;
        Ok(value)
    }

    fn parse_quoted_atom(&mut self) -> Result<String, String> {
        self.expect('\'')?;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '\'' => return Ok(out),
                '\\' => {
                    if let Some(escaped) = self.peek() {
                        self.pos += 1;
                        out.push(escaped);
                    }
                }
                other => out.push(other),
            }
        }
        Err("Unterminated quoted atom".to_string())
    }

    fn parse_atom_or_bool(&mut self) -> Result<ErlTerm, String> {
        let atom = self.parse_bare_atom()?;
        match atom.as_str() {
            "true" => Ok(ErlTerm::Atom("true".to_string())),
            "false" => Ok(ErlTerm::Atom("false".to_string())),
            _ => Ok(ErlTerm::Atom(atom)),
        }
    }

    fn parse_bare_atom(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '@' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err("Expected atom".to_string());
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    fn parse_number(&mut self) -> Result<ErlTerm, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.peek() == Some('.') && self.peek_n(1).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let s: String = self.chars[start..self.pos].iter().collect();
            return s
                .parse::<f64>()
                .map(ErlTerm::Float)
                .map_err(|e| format!("Invalid float: {}", e));
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<i64>()
            .map(ErlTerm::Integer)
            .map_err(|e| format!("Invalid integer: {}", e))
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.pos += 1;
                }
                Some('%') => {
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        self.skip_whitespace_and_comments();
        match self.peek() {
            Some(c) if c == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(c) => Err(format!(
                "Expected '{}' but found '{}' at position {}",
                expected, c, self.pos
            )),
            None => Err(format!("Expected '{}' but reached end of input", expected)),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_n(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.chars.len()
    }
}

fn parse_dotted_terms(content: &str) -> Result<Vec<ErlTerm>, String> {
    let mut parser = ErlParser::new(content);
    let mut terms = Vec::new();
    let mut count = 0usize;
    loop {
        parser.skip_whitespace_and_comments();
        if parser.is_eof() {
            break;
        }
        if count >= MAX_ITERATION_COUNT {
            break;
        }
        let term = parser.parse_term()?;
        terms.push(term);
        count += 1;
        parser.skip_whitespace_and_comments();
        if parser.peek() == Some('.') {
            parser.pos += 1;
        }
    }
    Ok(terms)
}

// ── Helpers ──

fn term_to_str(term: &ErlTerm) -> Option<String> {
    match term {
        ErlTerm::String(s) | ErlTerm::Binary(s) | ErlTerm::Atom(s) => Some(s.clone()),
        ErlTerm::Integer(n) => Some(n.to_string()),
        ErlTerm::Float(f) => Some(f.to_string()),
        _ => None,
    }
}

fn term_to_proplist(term: &ErlTerm) -> Option<Vec<(String, ErlTerm)>> {
    let items = match term {
        ErlTerm::List(items) => items,
        _ => return None,
    };
    let mut result = Vec::new();
    for item in items {
        if let ErlTerm::Tuple(fields) = item
            && fields.len() == 2
            && let Some(key) = term_to_str(&fields[0])
        {
            result.push((key, fields[1].clone()));
        }
    }
    Some(result)
}

fn term_to_key_value_pairs(term: &ErlTerm) -> Option<Vec<(String, ErlTerm)>> {
    match term {
        ErlTerm::Map(entries) => Some(
            entries
                .iter()
                .filter_map(|(key, value)| term_to_str(key).map(|key| (key, value.clone())))
                .collect(),
        ),
        _ => term_to_proplist(term),
    }
}

fn term_to_atom_list(term: &ErlTerm) -> Vec<String> {
    match term {
        ErlTerm::List(items) => items.iter().filter_map(term_to_str).collect(),
        _ => Vec::new(),
    }
}

fn build_hex_purl(name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("hex", name).ok()?;
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

// ── ErlangAppSrcParser ──

impl PackageParser for ErlangAppSrcParser {
    const PACKAGE_TYPE: PackageType = PackageType::Hex;

    fn is_match(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext == "src")
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem.ends_with(".app"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {:?}: {}", path, e);
                return vec![default_app_src_package()];
            }
        };

        match parse_app_src(&content) {
            Ok(pkg) => vec![pkg],
            Err(e) => {
                warn!("Failed to parse {:?}: {}", path, e);
                vec![default_app_src_package()]
            }
        }
    }
}

fn default_app_src_package() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Hex),
        primary_language: Some("Erlang".to_string()),
        datasource_id: Some(DatasourceId::ErlangOtpAppSrc),
        ..Default::default()
    }
}

fn parse_app_src(content: &str) -> Result<PackageData, String> {
    let terms = parse_dotted_terms(content)?;

    let app_tuple = terms
        .into_iter()
        .find_map(|term| {
            if let ErlTerm::Tuple(fields) = &term
                && fields.len() == 3
                && term_to_str(&fields[0]).as_deref() == Some("application")
            {
                Some(term)
            } else {
                None
            }
        })
        .ok_or_else(|| "No {application, _, _} tuple found".to_string())?;

    let fields = match app_tuple {
        ErlTerm::Tuple(fields) => fields,
        _ => unreachable!(),
    };

    let app_name = term_to_str(&fields[1]);
    let props = term_to_proplist(&fields[2]).unwrap_or_default();

    let mut package = default_app_src_package();
    package.name = app_name.map(truncate_field);

    let mut extra_data = HashMap::new();

    for (key, value) in &props {
        match key.as_str() {
            "vsn" => {
                if let Some(v) = term_to_str(value)
                    && !v.contains('%')
                {
                    package.version = Some(truncate_field(v));
                }
            }
            "description" => {
                package.description = term_to_str(value).map(truncate_field);
            }
            "licenses" => {
                let licenses = term_to_atom_list(value);
                if !licenses.is_empty() {
                    package.extracted_license_statement = Some(truncate_field(licenses.join(", ")));
                }
            }
            "links" => {
                if let Some(link_props) = term_to_key_value_pairs(value) {
                    for (link_name, link_val) in &link_props {
                        if let Some(url) = term_to_str(link_val) {
                            let lower = link_name.to_lowercase();
                            if lower.contains("github")
                                || lower.contains("source")
                                || lower.contains("repo")
                            {
                                package.vcs_url = Some(truncate_field(url.clone()));
                            }
                            if package.homepage_url.is_none() {
                                package.homepage_url = Some(truncate_field(url));
                            }
                        }
                    }
                }
            }
            "applications" => {
                let apps = term_to_atom_list(value);
                for app in apps {
                    if is_otp_stdlib(&app) {
                        continue;
                    }
                    package.dependencies.push(Dependency {
                        purl: build_hex_purl(&app, None).map(truncate_field),
                        extracted_requirement: None,
                        scope: Some("dependencies".to_string()),
                        is_runtime: Some(true),
                        is_optional: None,
                        is_pinned: None,
                        is_direct: None,
                        resolved_package: None,
                        extra_data: None,
                    });
                }
            }
            "runtime_dependencies" => {
                let deps = term_to_atom_list(value);
                for dep_str in deps {
                    if let Some((name, version)) = dep_str.split_once('-') {
                        if is_otp_stdlib(name) {
                            continue;
                        }
                        let version_str = if version.starts_with('@') {
                            None
                        } else {
                            Some(version)
                        };
                        package.dependencies.push(Dependency {
                            purl: build_hex_purl(name, version_str).map(truncate_field),
                            extracted_requirement: version_str
                                .map(|v| truncate_field(v.to_string())),
                            scope: Some("dependencies".to_string()),
                            is_runtime: Some(true),
                            is_optional: None,
                            is_pinned: None,
                            is_direct: None,
                            resolved_package: None,
                            extra_data: None,
                        });
                    }
                }
            }
            "maintainers" => {
                let maintainers = term_to_atom_list(value);
                if !maintainers.is_empty() {
                    extra_data.insert(
                        "maintainers".to_string(),
                        JsonValue::Array(
                            maintainers
                                .into_iter()
                                .map(|m| JsonValue::String(truncate_field(m)))
                                .collect(),
                        ),
                    );
                }
            }
            "keywords" => {
                let keywords = term_to_atom_list(value);
                if !keywords.is_empty() {
                    package.keywords = keywords.into_iter().map(truncate_field).collect();
                }
            }
            _ => {}
        }
    }

    if let Some(ref name) = package.name {
        package.purl = build_hex_purl(name, package.version.as_deref()).map(truncate_field);
        package.repository_homepage_url =
            Some(truncate_field(format!("https://hex.pm/packages/{}", name)));
        package.api_data_url = Some(truncate_field(format!(
            "https://hex.pm/api/packages/{}",
            name
        )));
    }

    if !extra_data.is_empty() {
        package.extra_data = Some(extra_data);
    }

    Ok(package)
}

fn is_otp_stdlib(name: &str) -> bool {
    matches!(
        name,
        "kernel"
            | "stdlib"
            | "sasl"
            | "erts"
            | "compiler"
            | "crypto"
            | "inets"
            | "ssl"
            | "public_key"
            | "asn1"
            | "syntax_tools"
            | "tools"
            | "os_mon"
            | "runtime_tools"
            | "mnesia"
            | "observer"
            | "wx"
            | "debugger"
            | "reltool"
            | "xmerl"
            | "edoc"
            | "eunit"
            | "common_test"
            | "dialyzer"
            | "et"
            | "megaco"
            | "parsetools"
            | "snmp"
            | "ssh"
            | "tftp"
            | "ftp"
            | "erl_interface"
            | "jinterface"
            | "odbc"
            | "eldap"
            | "diameter"
    )
}

// ── RebarConfigParser ──

impl PackageParser for RebarConfigParser {
    const PACKAGE_TYPE: PackageType = PackageType::Hex;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|n| n.to_str()) == Some("rebar.config")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {:?}: {}", path, e);
                return vec![default_rebar_config_package()];
            }
        };

        match parse_rebar_config(&content) {
            Ok(pkg) => vec![pkg],
            Err(e) => {
                warn!("Failed to parse {:?}: {}", path, e);
                vec![default_rebar_config_package()]
            }
        }
    }
}

fn default_rebar_config_package() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Hex),
        primary_language: Some("Erlang".to_string()),
        datasource_id: Some(DatasourceId::RebarConfig),
        ..Default::default()
    }
}

fn parse_rebar_config(content: &str) -> Result<PackageData, String> {
    let terms = parse_dotted_terms(content)?;

    let mut package = default_rebar_config_package();

    for term in &terms {
        if let ErlTerm::Tuple(fields) = term
            && fields.len() == 2
        {
            let key = term_to_str(&fields[0]);
            match key.as_deref() {
                Some("deps") => {
                    if let ErlTerm::List(deps) = &fields[1] {
                        for dep in deps.iter().take(MAX_ITERATION_COUNT) {
                            if let Some(d) = parse_rebar_dep(dep) {
                                package.dependencies.push(d);
                            }
                        }
                    }
                }
                Some("profiles") => {
                    parse_profile_deps(&fields[1], &mut package.dependencies);
                }
                _ => {}
            }
        }
    }

    Ok(package)
}

fn parse_rebar_dep(term: &ErlTerm) -> Option<Dependency> {
    let fields = match term {
        ErlTerm::Tuple(fields) => fields,
        _ => return None,
    };

    if fields.is_empty() {
        return None;
    }

    if let Some(key) = term_to_str(&fields[0])
        && key.starts_with("if_")
    {
        return None;
    }

    let app_name = term_to_str(&fields[0])?;

    match fields.len() {
        // {Name, Version} or {Name, {git, URL, Ref}}
        2 => {
            if let Some(version) = term_to_str(&fields[1]) {
                // {Name, Version}
                Some(Dependency {
                    purl: build_hex_purl(&app_name, Some(&version)).map(truncate_field),
                    extracted_requirement: Some(truncate_field(version)),
                    scope: Some("dependencies".to_string()),
                    is_runtime: None,
                    is_optional: None,
                    is_pinned: None,
                    is_direct: None,
                    resolved_package: None,
                    extra_data: None,
                })
            } else {
                let package_name = extract_rebar_package_name(&fields[1], &app_name);
                let vcs_url = extract_git_url(&fields[1]);
                let version = extract_git_version(&fields[1]);
                Some(Dependency {
                    purl: build_hex_purl(&package_name, version.as_deref()).map(truncate_field),
                    extracted_requirement: version.map(truncate_field),
                    scope: Some("dependencies".to_string()),
                    is_runtime: None,
                    is_optional: None,
                    is_pinned: None,
                    is_direct: None,
                    resolved_package: None,
                    extra_data: build_rebar_dependency_extra_data(
                        vcs_url,
                        app_name.as_str(),
                        package_name.as_str(),
                    ),
                })
            }
        }
        // {Name, Version, Source}
        3 => {
            if let Some(version) = term_to_str(&fields[1]) {
                let package_name = extract_rebar_package_name(&fields[2], &app_name);
                let vcs_url = extract_git_url(&fields[2]);
                Some(Dependency {
                    purl: build_hex_purl(&package_name, Some(&version)).map(truncate_field),
                    extracted_requirement: Some(truncate_field(version)),
                    scope: Some("dependencies".to_string()),
                    is_runtime: None,
                    is_optional: None,
                    is_pinned: None,
                    is_direct: None,
                    resolved_package: None,
                    extra_data: build_rebar_dependency_extra_data(
                        vcs_url,
                        app_name.as_str(),
                        package_name.as_str(),
                    ),
                })
            } else {
                let package_name = extract_rebar_package_name(&fields[1], &app_name);
                let vcs_url = extract_git_url(&fields[1]);
                let version = extract_git_version(&fields[1]);
                Some(Dependency {
                    purl: build_hex_purl(&package_name, version.as_deref()).map(truncate_field),
                    extracted_requirement: version.map(truncate_field),
                    scope: Some("dependencies".to_string()),
                    is_runtime: None,
                    is_optional: None,
                    is_pinned: None,
                    is_direct: None,
                    resolved_package: None,
                    extra_data: build_rebar_dependency_extra_data(
                        vcs_url,
                        app_name.as_str(),
                        package_name.as_str(),
                    ),
                })
            }
        }
        _ => None,
    }
}

fn extract_rebar_package_name(term: &ErlTerm, fallback_name: &str) -> String {
    if let ErlTerm::Tuple(fields) = term
        && fields.len() >= 2
        && term_to_str(&fields[0]).as_deref() == Some("pkg")
        && let Some(package_name) = term_to_str(&fields[1])
    {
        package_name
    } else {
        fallback_name.to_string()
    }
}

fn build_rebar_dependency_extra_data(
    vcs_url: Option<String>,
    app_name: &str,
    package_name: &str,
) -> Option<HashMap<String, JsonValue>> {
    let mut extra_data = HashMap::new();

    if let Some(url) = vcs_url {
        extra_data.insert(
            "vcs_url".to_string(),
            JsonValue::String(truncate_field(url)),
        );
    }

    if app_name != package_name {
        extra_data.insert(
            "app_name".to_string(),
            JsonValue::String(truncate_field(app_name.to_string())),
        );
    }

    if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    }
}

fn extract_git_url(term: &ErlTerm) -> Option<String> {
    if let ErlTerm::Tuple(fields) = term
        && fields.len() >= 2
        && matches!(
            term_to_str(&fields[0]).as_deref(),
            Some("git") | Some("git_subdir")
        )
    {
        term_to_str(&fields[1])
    } else {
        None
    }
}

fn extract_git_version(term: &ErlTerm) -> Option<String> {
    if let ErlTerm::Tuple(fields) = term
        && fields.len() >= 3
        && matches!(
            term_to_str(&fields[0]).as_deref(),
            Some("git") | Some("git_subdir")
        )
    {
        if let ErlTerm::Tuple(ref_fields) = &fields[2]
            && ref_fields.len() == 2
        {
            let ref_type = term_to_str(&ref_fields[0])?;
            let ref_val = term_to_str(&ref_fields[1])?;
            match ref_type.as_str() {
                "tag" => Some(ref_val),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    }
}

fn parse_profile_deps(term: &ErlTerm, dependencies: &mut Vec<Dependency>) {
    let profiles = match term {
        ErlTerm::List(items) => items,
        _ => return,
    };

    for profile in profiles.iter().take(MAX_ITERATION_COUNT) {
        if let ErlTerm::Tuple(fields) = profile
            && fields.len() == 2
        {
            let profile_name = term_to_str(&fields[0]).unwrap_or_default();
            if let ErlTerm::List(profile_opts) = &fields[1] {
                for opt in profile_opts {
                    if let ErlTerm::Tuple(opt_fields) = opt
                        && opt_fields.len() == 2
                        && term_to_str(&opt_fields[0]).as_deref() == Some("deps")
                        && let ErlTerm::List(deps) = &opt_fields[1]
                    {
                        for dep in deps.iter().take(MAX_ITERATION_COUNT) {
                            if let Some(mut d) = parse_rebar_dep(dep) {
                                d.scope = Some(truncate_field(profile_name.clone()));
                                dependencies.push(d);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── RebarLockParser ──

impl PackageParser for RebarLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Hex;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|n| n.to_str()) == Some("rebar.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read {:?}: {}", path, e);
                return vec![default_rebar_lock_package()];
            }
        };

        match parse_rebar_lock(&content) {
            Ok(pkg) => vec![pkg],
            Err(e) => {
                warn!("Failed to parse {:?}: {}", path, e);
                vec![default_rebar_lock_package()]
            }
        }
    }
}

fn default_rebar_lock_package() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Hex),
        primary_language: Some("Erlang".to_string()),
        datasource_id: Some(DatasourceId::RebarLock),
        ..Default::default()
    }
}

fn parse_rebar_lock(content: &str) -> Result<PackageData, String> {
    let terms = parse_dotted_terms(content)?;

    // rebar.lock format: first term is either:
    // - {Version, [deps]}  (v2 format, e.g. {"1.2.0", [...]})
    // - [deps]             (v1 format, flat list)
    // Second term (if present): [{pkg_hash, [...]}, {pkg_hash_ext, [...]}]

    let (dep_list, hash_map) = match terms.as_slice() {
        // v2 format: {"1.2.0", [deps]}
        [ErlTerm::Tuple(fields), rest @ ..] if fields.len() == 2 => {
            let deps = match &fields[1] {
                ErlTerm::List(items) => items.clone(),
                _ => return Err("Expected dependency list in lock tuple".to_string()),
            };
            let hashes = rest.first().map(extract_pkg_hashes).unwrap_or_default();
            (deps, hashes)
        }
        // v1 format: [deps]
        [ErlTerm::List(items), rest @ ..] => {
            let hashes = rest.first().map(extract_pkg_hashes).unwrap_or_default();
            (items.clone(), hashes)
        }
        _ => return Err("Unrecognized rebar.lock format".to_string()),
    };

    let mut package = default_rebar_lock_package();

    for dep_term in dep_list.iter().take(MAX_ITERATION_COUNT) {
        if let Some(dep) = parse_lock_dep(dep_term, &hash_map) {
            package.dependencies.push(dep);
        }
    }

    Ok(package)
}

fn parse_lock_dep(term: &ErlTerm, hashes: &HashMap<String, String>) -> Option<Dependency> {
    let fields = match term {
        ErlTerm::Tuple(fields) if fields.len() >= 3 => fields,
        _ => return None,
    };

    let app_name = term_to_str(&fields[0])?;
    // fields[2] is the level (integer)

    let (package_name, version, vcs_url) = match &fields[1] {
        // {pkg, <<"name">>, <<"version">>}
        ErlTerm::Tuple(pkg_fields)
            if pkg_fields.len() >= 3 && term_to_str(&pkg_fields[0]).as_deref() == Some("pkg") =>
        {
            let package_name = term_to_str(&pkg_fields[1]).unwrap_or_else(|| app_name.clone());
            let ver = term_to_str(&pkg_fields[2]);
            (package_name, ver, None)
        }
        // {git, "url", {ref, "hash"}}
        ErlTerm::Tuple(git_fields)
            if git_fields.len() >= 2
                && matches!(
                    term_to_str(&git_fields[0]).as_deref(),
                    Some("git") | Some("git_subdir")
                ) =>
        {
            let url = term_to_str(&git_fields[1]);
            let ver = if git_fields.len() >= 3 {
                extract_git_version_from_lock_ref(&git_fields[2])
            } else {
                None
            };
            (app_name.clone(), ver, url)
        }
        _ => (app_name.clone(), None, None),
    };

    let sha256 = hashes
        .get(&app_name)
        .or_else(|| hashes.get(&package_name))
        .and_then(|h| Sha256Digest::from_hex(h).ok());

    let resolved_package = ResolvedPackage {
        primary_language: Some("Erlang".to_string()),
        sha256,
        is_virtual: true,
        datasource_id: Some(DatasourceId::RebarLock),
        purl: build_hex_purl(&package_name, version.as_deref()).map(truncate_field),
        repository_homepage_url: Some(truncate_field(format!(
            "https://hex.pm/packages/{}",
            package_name
        ))),
        api_data_url: Some(truncate_field(format!(
            "https://hex.pm/api/packages/{}",
            package_name
        ))),
        ..ResolvedPackage::new(
            PackageType::Hex,
            String::new(),
            package_name.clone(),
            version.clone().unwrap_or_default(),
        )
    };

    Some(Dependency {
        purl: build_hex_purl(&package_name, version.as_deref()).map(truncate_field),
        extracted_requirement: version.map(truncate_field),
        scope: Some("dependencies".to_string()),
        is_runtime: None,
        is_optional: None,
        is_pinned: Some(true),
        is_direct: None,
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: build_rebar_dependency_extra_data(
            vcs_url,
            app_name.as_str(),
            package_name.as_str(),
        ),
    })
}

fn extract_git_version_from_lock_ref(term: &ErlTerm) -> Option<String> {
    if let ErlTerm::Tuple(fields) = term
        && fields.len() == 2
        && term_to_str(&fields[0]).as_deref() == Some("ref")
    {
        term_to_str(&fields[1])
    } else {
        None
    }
}

fn extract_pkg_hashes(term: &ErlTerm) -> HashMap<String, String> {
    let items = match term {
        ErlTerm::List(items) => items,
        _ => return HashMap::new(),
    };

    let mut hashes = HashMap::new();
    for item in items {
        if let ErlTerm::Tuple(fields) = item
            && fields.len() == 2
            && term_to_str(&fields[0]).as_deref() == Some("pkg_hash")
            && let ErlTerm::List(hash_list) = &fields[1]
        {
            for entry in hash_list.iter().take(MAX_ITERATION_COUNT) {
                if let ErlTerm::Tuple(pair) = entry
                    && pair.len() == 2
                    && let (Some(name), Some(hash)) = (term_to_str(&pair[0]), term_to_str(&pair[1]))
                {
                    hashes.insert(name, hash);
                }
            }
        }
    }
    hashes
}

// ── Parser metadata registration ──

crate::register_parser!(
    "Erlang OTP application resource file",
    &["**/*.app.src"],
    "hex",
    "Erlang",
    Some("https://www.erlang.org/doc/apps/kernel/application"),
);

crate::register_parser!(
    "Rebar3 configuration",
    &["**/rebar.config"],
    "hex",
    "Erlang",
    Some("https://rebar3.org/docs/configuration/configuration/"),
);

crate::register_parser!(
    "Rebar3 lockfile",
    &["**/rebar.lock"],
    "hex",
    "Erlang",
    Some("https://rebar3.org/docs/configuration/configuration/"),
);
