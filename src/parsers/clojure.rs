use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;
use crate::parsers::utils::{
    MAX_ITERATION_COUNT, RecursionGuard, read_file_to_string, truncate_field,
};
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};

use super::PackageParser;

pub struct ClojureDepsEdnParser;

impl PackageParser for ClojureDepsEdnParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "deps.edn")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read deps.edn at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::ClojureDepsEdn))];
            }
        };

        match parse_forms(&content)
            .and_then(|forms| {
                forms
                    .into_iter()
                    .next()
                    .ok_or_else(|| "deps.edn contained no readable forms".to_string())
            })
            .and_then(|form| parse_deps_edn_form(&form))
        {
            Ok(package) => vec![package],
            Err(error) => {
                warn!("Failed to parse deps.edn at {:?}: {}", path, error);
                vec![default_package_data(Some(DatasourceId::ClojureDepsEdn))]
            }
        }
    }
}

pub struct ClojureProjectCljParser;

impl PackageParser for ClojureProjectCljParser {
    const PACKAGE_TYPE: PackageType = PackageType::Maven;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "project.clj")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read project.clj at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::ClojureProjectClj))];
            }
        };

        if looks_like_template_project_clj(&content) {
            return vec![default_package_data(Some(DatasourceId::ClojureProjectClj))];
        }

        if !content.contains("(defproject") {
            return vec![default_package_data(Some(DatasourceId::ClojureProjectClj))];
        }

        let forms = match parse_forms(&content) {
            Ok(forms) => forms,
            Err(error) => {
                warn!("Failed to parse project.clj at {:?}: {}", path, error);
                return vec![default_package_data(Some(DatasourceId::ClojureProjectClj))];
            }
        };

        let Some(form) = forms.into_iter().find(|form| {
            matches!(
                form,
                Form::List(items) if matches!(items.first(), Some(Form::Symbol(symbol)) if symbol == "defproject")
            )
        }) else {
            return vec![default_package_data(Some(DatasourceId::ClojureProjectClj))];
        };

        match parse_project_clj_form(&form) {
            Ok(package) => vec![package],
            Err(error) => {
                warn!("Failed to parse project.clj at {:?}: {}", path, error);
                vec![default_package_data(Some(DatasourceId::ClojureProjectClj))]
            }
        }
    }
}

#[derive(Clone, Debug)]
enum Form {
    Nil,
    Bool(bool),
    String(String),
    Keyword(String),
    Symbol(String),
    Vector(Vec<Form>),
    List(Vec<Form>),
    Map(Vec<(Form, Form)>),
    Prefixed(Box<Form>),
}

struct Reader {
    chars: Vec<char>,
    index: usize,
    guard: RecursionGuard<()>,
}

impl Reader {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            index: 0,
            guard: RecursionGuard::depth_only(),
        }
    }

    fn parse_all(mut self) -> Result<Vec<Form>, String> {
        let mut forms = Vec::new();
        let mut count = 0usize;
        while self.skip_ws_and_comments() {
            count += 1;
            if count > MAX_ITERATION_COUNT {
                warn!("Reached MAX_ITERATION_COUNT in parse_all, stopping early");
                break;
            }
            forms.push(self.parse_form()?);
        }
        Ok(forms)
    }

    fn skip_ws_and_comments(&mut self) -> bool {
        loop {
            while self
                .peek()
                .is_some_and(|ch| ch.is_whitespace() || ch == ',')
            {
                self.index += 1;
            }
            if self.peek() == Some(';') {
                while let Some(ch) = self.peek() {
                    self.index += 1;
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }
            return self.peek().is_some();
        }
    }

    fn parse_form(&mut self) -> Result<Form, String> {
        if self.guard.descend() {
            return Err("recursion depth exceeded".to_string());
        }
        self.skip_ws_and_comments();
        let result = match self.peek() {
            Some('"') => self.parse_string().map(Form::String),
            Some(':') => self.parse_keyword().map(Form::Keyword),
            Some('[') => self.parse_collection('[', ']').map(Form::Vector),
            Some('(') => self.parse_collection('(', ')').map(Form::List),
            Some('{') => self.parse_map(),
            Some('^') => {
                self.index += 1;
                let _ = self.parse_form()?;
                let result = self.parse_form();
                self.guard.ascend();
                return result;
            }
            Some('~') | Some('\'') | Some('`') | Some('@') => {
                self.index += 1;
                let form = self.parse_form()?;
                self.guard.ascend();
                return Ok(Form::Prefixed(Box::new(form)));
            }
            Some('#') => {
                let result = self.parse_dispatch_form();
                self.guard.ascend();
                return result;
            }
            Some(_) => self.parse_atom(),
            None => Err("unexpected end of input".to_string()),
        };
        self.guard.ascend();
        result
    }

    fn parse_dispatch_form(&mut self) -> Result<Form, String> {
        self.expect('#')?;
        match self.peek() {
            Some('_') => {
                self.index += 1;
                let _ = self.parse_form()?;
                self.parse_form()
            }
            Some('=') => Err("unsupported reader eval dispatch".to_string()),
            Some('"') => {
                // Tolerate regex literals in ignored fields without implementing reader semantics.
                self.parse_string().map(Form::String)
            }
            Some('{') => {
                // Tolerate set literals in ignored fields by treating them as plain collections.
                self.parse_collection('{', '}').map(Form::Vector)
            }
            Some('(') => {
                // Tolerate function literals in ignored fields without implementing reader semantics.
                self.parse_collection('(', ')').map(Form::List)
            }
            Some('?') => {
                // Tolerate reader conditionals by skipping the dispatch token and
                // returning the selected readable form without evaluating features.
                self.index += 1;
                if self.peek() == Some('@') {
                    self.index += 1;
                }
                let _ = self.parse_form()?;
                self.parse_form()
            }
            Some(ch) if !is_delimiter(ch) => {
                // Tolerate tagged literals in ignored fields by ignoring the tag and
                // parsing the following readable form as plain data.
                let _ = self.parse_atom()?;
                self.parse_form()
            }
            Some(ch) => Err(format!("unsupported reader dispatch '#{ch}'")),
            None => Err("unexpected end of input after '#'".to_string()),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
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
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                return Ok(result);
            } else {
                result.push(ch);
            }
        }
        Err("unterminated string".to_string())
    }

    fn parse_keyword(&mut self) -> Result<String, String> {
        self.expect(':')?;
        let start = self.index;
        while let Some(ch) = self.peek() {
            if is_delimiter(ch) {
                break;
            }
            self.index += 1;
        }
        if self.index == start {
            return Err("empty keyword".to_string());
        }
        Ok(self.chars[start..self.index].iter().collect())
    }

    fn parse_collection(&mut self, open: char, close: char) -> Result<Vec<Form>, String> {
        self.expect(open)?;
        let mut forms = Vec::new();
        let mut count = 0usize;
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some(close) {
                self.index += 1;
                return Ok(forms);
            }
            if self.peek().is_none() {
                return Err(format!("unterminated collection starting with {open}"));
            }
            count += 1;
            if count > MAX_ITERATION_COUNT {
                warn!("Reached MAX_ITERATION_COUNT in parse_collection, stopping early");
                break;
            }
            forms.push(self.parse_form()?);
        }
        Ok(forms)
    }

    fn parse_map(&mut self) -> Result<Form, String> {
        self.expect('{')?;
        let mut entries = Vec::new();
        let mut count = 0usize;
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some('}') {
                self.index += 1;
                return Ok(Form::Map(entries));
            }
            if self.peek().is_none() {
                return Err("unterminated map".to_string());
            }
            count += 1;
            if count > MAX_ITERATION_COUNT {
                warn!("Reached MAX_ITERATION_COUNT in parse_map, stopping early");
                break;
            }
            let key = self.parse_form()?;
            self.skip_ws_and_comments();
            if self.peek() == Some('}') {
                return Err("map missing value".to_string());
            }
            let value = self.parse_form()?;
            entries.push((key, value));
        }
        Ok(Form::Map(entries))
    }

    fn parse_atom(&mut self) -> Result<Form, String> {
        let start = self.index;
        while let Some(ch) = self.peek() {
            if is_delimiter(ch) {
                break;
            }
            self.index += 1;
        }
        let token: String = self.chars[start..self.index].iter().collect();
        if token.is_empty() {
            return Err("empty token".to_string());
        }
        Ok(match token.as_str() {
            "nil" => Form::Nil,
            "true" => Form::Bool(true),
            "false" => Form::Bool(false),
            _ => Form::Symbol(token),
        })
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        match self.peek() {
            Some(ch) if ch == expected => {
                self.index += 1;
                Ok(())
            }
            Some(ch) => Err(format!("expected '{expected}', found '{ch}'")),
            None => Err(format!("expected '{expected}', found end of input")),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }
}

fn is_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || ch == ','
        || matches!(
            ch,
            '[' | ']' | '{' | '}' | '(' | ')' | '"' | ';' | '\'' | '`' | '~' | '@'
        )
}

fn parse_forms(input: &str) -> Result<Vec<Form>, String> {
    Reader::new(input).parse_all()
}

fn parse_deps_edn_form(form: &Form) -> Result<PackageData, String> {
    let Form::Map(entries) = form else {
        return Err("deps.edn root is not a map".to_string());
    };

    let mut package = default_package_data(Some(DatasourceId::ClojureDepsEdn));
    let mut dependencies = Vec::new();
    let mut extra_data = HashMap::new();

    if let Some(Form::Map(dep_map)) = map_get_keyword(entries, "deps") {
        dependencies.extend(extract_deps_map(dep_map, None, true));
    }

    if let Some(Form::Map(alias_map)) = map_get_keyword(entries, "aliases") {
        for (alias_key, alias_value) in alias_map {
            let Some(alias_name) = keyword_or_symbol_name(alias_key) else {
                continue;
            };
            let Form::Map(alias_entries) = alias_value else {
                continue;
            };
            for dep_key in [
                "extra-deps",
                "override-deps",
                "default-deps",
                "deps",
                "replace-deps",
            ] {
                if let Some(Form::Map(dep_map)) = map_get_keyword(alias_entries, dep_key) {
                    dependencies.extend(extract_deps_map(dep_map, Some(&alias_name), false));
                }
            }
        }
        if let Some(json) = form_to_json(
            &Form::Map(alias_map.clone()),
            &mut RecursionGuard::depth_only(),
        ) {
            extra_data.insert("aliases".to_string(), json);
        }
    }

    if let Some(value) = map_get_keyword(entries, "paths")
        .and_then(|f| form_to_json(f, &mut RecursionGuard::depth_only()))
    {
        extra_data.insert("paths".to_string(), value);
    }
    if let Some(value) = map_get_keyword(entries, "mvn/repos")
        .and_then(|f| form_to_json(f, &mut RecursionGuard::depth_only()))
    {
        extra_data.insert("mvn_repos".to_string(), value);
    }

    package.dependencies = dependencies;
    package.extra_data = (!extra_data.is_empty()).then_some(extra_data);
    Ok(package)
}

fn parse_project_clj_form(form: &Form) -> Result<PackageData, String> {
    let Form::List(items) = form else {
        return Err("project.clj root is not a list".to_string());
    };
    if !matches!(items.first(), Some(Form::Symbol(symbol)) if symbol == "defproject") {
        return Err("project.clj root is not defproject".to_string());
    }

    let Some((namespace, name)) = items.get(1).and_then(parse_lib_form) else {
        return Err("defproject missing project identifier".to_string());
    };
    let Some(version) = items.get(2).and_then(form_as_string) else {
        return Err("defproject missing project version".to_string());
    };

    let mut package = default_package_data(Some(DatasourceId::ClojureProjectClj));
    package.namespace = namespace.clone().map(truncate_field);
    package.name = Some(truncate_field(name.clone()));
    package.version = Some(truncate_field(version.to_string()));
    package.purl = build_maven_purl(namespace.as_deref(), &name, Some(version)).map(truncate_field);

    let mut index = 3usize;
    while index + 1 < items.len() {
        let Some(key) = form_as_keyword(&items[index]) else {
            index += 1;
            continue;
        };
        let value = &items[index + 1];

        match key {
            "description" => {
                package.description = form_as_string(value).map(|s| truncate_field(s.to_owned()))
            }
            "url" => {
                package.homepage_url = form_as_string(value).map(|s| truncate_field(s.to_owned()))
            }
            "license" => {
                package.extracted_license_statement =
                    format_license(value, &mut RecursionGuard::depth_only()).map(truncate_field);
            }
            "scm" => {
                if let Form::Map(entries) = value {
                    package.vcs_url = map_get_keyword(entries, "url")
                        .and_then(form_as_string)
                        .map(|s| truncate_field(s.to_owned()));
                }
            }
            "dependencies" => {
                if let Form::Vector(deps) = value {
                    package
                        .dependencies
                        .extend(extract_project_dependencies(deps, None));
                }
            }
            "profiles" => {
                if let Form::Map(entries) = value {
                    for (profile_key, profile_value) in entries {
                        let Some(profile_name) = keyword_or_symbol_name(profile_key) else {
                            continue;
                        };
                        let Form::Map(profile_entries) = profile_value else {
                            continue;
                        };
                        if let Some(Form::Vector(deps)) =
                            map_get_keyword(profile_entries, "dependencies")
                        {
                            package
                                .dependencies
                                .extend(extract_project_dependencies(deps, Some(&profile_name)));
                        }
                    }
                }
            }
            _ => {}
        }
        index += 2;
    }

    Ok(package)
}

fn extract_deps_map(
    entries: &[(Form, Form)],
    scope: Option<&str>,
    runtime: bool,
) -> Vec<Dependency> {
    entries
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|(lib, coord)| build_deps_edn_dependency(lib, coord, scope, runtime))
        .collect()
}

fn build_deps_edn_dependency(
    lib: &Form,
    coord: &Form,
    scope: Option<&str>,
    runtime: bool,
) -> Option<Dependency> {
    let (namespace, name) = parse_lib_form(lib)?;
    let mut extra_data = HashMap::new();
    let mut requirement = None;
    let mut pinned = false;

    if let Form::Map(entries) = coord {
        if let Some(version) = map_get_keyword(entries, "mvn/version").and_then(form_as_string) {
            requirement = Some(version.to_string());
            pinned = is_exact_version(version);
        }
        for (key, data_key) in [
            ("git/url", "git_url"),
            ("git/tag", "git_tag"),
            ("git/sha", "git_sha"),
            ("deps/root", "deps_root"),
            ("deps/manifest", "deps_manifest"),
            ("local/root", "local_root"),
            ("exclusions", "exclusions"),
        ] {
            if let Some(value) = map_get_keyword(entries, key)
                .and_then(|f| form_to_json(f, &mut RecursionGuard::depth_only()))
            {
                extra_data.insert(data_key.to_string(), value);
            }
        }
    }

    Some(Dependency {
        purl: build_maven_purl(
            namespace.as_deref(),
            &name,
            requirement.as_deref().map(strip_exact_prefix),
        )
        .map(truncate_field),
        extracted_requirement: requirement.map(truncate_field),
        scope: scope.map(ToOwned::to_owned),
        is_runtime: Some(runtime),
        is_optional: Some(scope.is_some()),
        is_pinned: Some(pinned),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: (!extra_data.is_empty()).then_some(extra_data),
    })
}

fn extract_project_dependencies(entries: &[Form], scope: Option<&str>) -> Vec<Dependency> {
    entries
        .iter()
        .take(MAX_ITERATION_COUNT)
        .filter_map(|entry| {
            let Form::Vector(parts) = entry else {
                return None;
            };
            let (namespace, name) = parse_lib_form(parts.first()?)?;
            let version = form_as_string(parts.get(1)?)?;

            let mut extra_data = HashMap::new();
            let mut index = 2usize;
            while index + 1 < parts.len() {
                if let Some(key) = form_as_keyword(&parts[index])
                    && let Some(value) =
                        form_to_json(&parts[index + 1], &mut RecursionGuard::depth_only())
                {
                    extra_data.insert(key.replace('-', "_"), value);
                }
                index += 2;
            }

            let (is_runtime, is_optional) = match scope {
                Some("dev") | Some("test") => (false, true),
                Some("provided") => (false, false),
                Some(_) => (false, true),
                None => (true, false),
            };

            Some(Dependency {
                purl: build_maven_purl(
                    namespace.as_deref(),
                    &name,
                    Some(strip_exact_prefix(version)),
                )
                .map(truncate_field),
                extracted_requirement: Some(truncate_field(version.to_string())),
                scope: scope.map(ToOwned::to_owned),
                is_runtime: Some(is_runtime),
                is_optional: Some(is_optional),
                is_pinned: Some(is_exact_version(version)),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: (!extra_data.is_empty()).then_some(extra_data),
            })
        })
        .collect()
}

fn parse_lib_form(form: &Form) -> Option<(Option<String>, String)> {
    let raw = match form {
        Form::Symbol(value) | Form::String(value) => value,
        _ => return None,
    };

    if let Some((namespace, name)) = raw.split_once('/') {
        Some((Some(namespace.to_string()), name.to_string()))
    } else {
        Some((Some(raw.to_string()), raw.to_string()))
    }
}

fn map_get_keyword<'a>(entries: &'a [(Form, Form)], key: &str) -> Option<&'a Form> {
    entries.iter().find_map(|(entry_key, entry_value)| {
        if form_as_keyword(entry_key) == Some(key) {
            Some(entry_value)
        } else {
            None
        }
    })
}

fn form_as_keyword(form: &Form) -> Option<&str> {
    match form {
        Form::Keyword(value) => Some(value.as_str()),
        _ => None,
    }
}

fn form_as_string(form: &Form) -> Option<&str> {
    match form {
        Form::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn keyword_or_symbol_name(form: &Form) -> Option<String> {
    match form {
        Form::Keyword(value) | Form::Symbol(value) => Some(value.clone()),
        _ => None,
    }
}

fn map_key_name(form: &Form) -> Option<String> {
    match form {
        Form::Keyword(value) | Form::Symbol(value) | Form::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn form_to_json(form: &Form, guard: &mut RecursionGuard<()>) -> Option<JsonValue> {
    if guard.descend() {
        warn!("form_to_json exceeded MAX_RECURSION_DEPTH");
        return None;
    }
    let result = Some(match form {
        Form::Nil => JsonValue::Null,
        Form::Bool(value) => JsonValue::Bool(*value),
        Form::String(value) => JsonValue::String(value.clone()),
        Form::Keyword(value) => JsonValue::String(format!(":{value}")),
        Form::Symbol(value) => JsonValue::String(value.clone()),
        Form::Vector(values) | Form::List(values) => JsonValue::Array(
            values
                .iter()
                .filter_map(|f| form_to_json(f, guard))
                .collect(),
        ),
        Form::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (key, value) in entries {
                let Some(key_name) = map_key_name(key) else {
                    continue;
                };
                if let Some(json) = form_to_json(value, guard) {
                    map.insert(key_name, json);
                }
            }
            JsonValue::Object(map)
        }
        Form::Prefixed(value) => form_to_json(value, guard)?,
    });
    guard.ascend();
    result
}

fn format_license(form: &Form, guard: &mut RecursionGuard<()>) -> Option<String> {
    if guard.descend() {
        warn!("format_license exceeded MAX_RECURSION_DEPTH");
        return None;
    }
    let result = match form {
        Form::Map(entries) => format_license_map(entries),
        Form::Vector(values) | Form::List(values) => {
            let licenses: Vec<String> = values
                .iter()
                .filter_map(|f| format_license(f, guard))
                .collect();
            if licenses.is_empty() {
                None
            } else {
                Some(licenses.join("\n"))
            }
        }
        _ => None,
    };
    guard.ascend();
    result
}

fn format_license_map(entries: &[(Form, Form)]) -> Option<String> {
    let name = map_get_keyword(entries, "name").and_then(form_as_string)?;
    let mut rendered = format!("- license:\n    name: {name}\n");
    if let Some(url) = map_get_keyword(entries, "url").and_then(form_as_string) {
        rendered.push_str(&format!("    url: {url}\n"));
    }
    Some(rendered)
}

fn build_maven_purl(namespace: Option<&str>, name: &str, version: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PackageType::Maven.as_str(), name).ok()?;
    if let Some(namespace) = namespace {
        purl.with_namespace(namespace).ok()?;
    }
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn is_exact_version(version: &str) -> bool {
    let normalized = strip_exact_prefix(version).trim();
    !normalized.is_empty()
        && !normalized.contains('*')
        && !normalized.contains('^')
        && !normalized.contains('~')
        && !normalized.contains('>')
        && !normalized.contains('<')
        && !normalized.contains('|')
        && !normalized.contains(',')
        && !normalized.contains(' ')
}

fn strip_exact_prefix(version: &str) -> &str {
    version.trim_start_matches('=')
}

fn looks_like_template_project_clj(content: &str) -> bool {
    let Some(defproject_index) = content.find("(defproject") else {
        return false;
    };

    let manifest_window = &content[defproject_index..content.len().min(defproject_index + 256)];
    manifest_window.contains("{{") && manifest_window.contains("}}")
}

fn default_package_data(datasource_id: Option<DatasourceId>) -> PackageData {
    PackageData {
        package_type: Some(PackageType::Maven),
        primary_language: Some("Clojure".to_string()),
        datasource_id,
        ..Default::default()
    }
}

crate::register_parser!(
    "Clojure deps.edn and project.clj manifests",
    &["**/deps.edn", "**/project.clj"],
    "maven",
    "Clojure",
    Some("https://clojure.org/reference/deps_edn"),
);
