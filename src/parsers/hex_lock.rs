use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, ResolvedPackage};

use super::PackageParser;

pub struct HexLockParser;

#[derive(Clone, Debug)]
enum Term {
    Map(Vec<(Term, Term)>),
    Tuple(Vec<Term>),
    List(Vec<Term>),
    KeywordList(Vec<(String, Term)>),
    String(String),
    Atom(String),
    Bool(bool),
    Integer(i64),
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    source: &'a str,
}

impl PackageParser for HexLockParser {
    const PACKAGE_TYPE: PackageType = PackageType::Hex;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some("mix.lock")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read mix.lock at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        match parse_mix_lock(&content) {
            Ok(package_data) => vec![package_data],
            Err(e) => {
                warn!("Failed to parse mix.lock at {:?}: {}", path, e);
                vec![default_package_data()]
            }
        }
    }
}

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PackageType::Hex),
        primary_language: Some("Elixir".to_string()),
        datasource_id: Some(DatasourceId::HexMixLock),
        ..Default::default()
    }
}

fn parse_mix_lock(content: &str) -> Result<PackageData, String> {
    let mut parser = Parser::new(content);
    let term = parser.parse_term()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err("Unexpected trailing content in mix.lock".to_string());
    }

    let entries = match term {
        Term::Map(entries) => entries,
        _ => return Err("mix.lock root must be a map".to_string()),
    };

    let mut dependencies = Vec::new();
    for (key, value) in entries {
        if let Some(dep) = build_dependency_from_lock_entry(&key, &value)? {
            dependencies.push(dep);
        }
    }

    let mut package = default_package_data();
    package.dependencies = dependencies;
    Ok(package)
}

fn build_dependency_from_lock_entry(
    key: &Term,
    value: &Term,
) -> Result<Option<Dependency>, String> {
    let app_name = term_to_string(key)?;

    let tuple = match value {
        Term::Tuple(items) => items,
        _ => return Ok(None),
    };

    if tuple.len() < 8 {
        return Ok(None);
    }

    let kind = term_to_atom(&tuple[0])?;
    if kind != "hex" {
        return Ok(None);
    }

    let package_name = term_to_atom(&tuple[1])?;
    let version = term_to_string(&tuple[2])?;
    let inner_checksum = term_to_string(&tuple[3])?;
    let managers = term_to_atom_list(&tuple[4])?;
    let nested_dependencies = term_to_dependency_tuples(&tuple[5])?;
    let repo = term_to_string(&tuple[6])?;
    let outer_checksum = term_to_string(&tuple[7])?;

    let purl = build_hex_purl(&package_name, Some(&version), Some(&repo));
    let resolved_package = ResolvedPackage {
        primary_language: Some("Elixir".to_string()),
        download_url: None,
        sha1: None,
        sha256: Some(inner_checksum),
        sha512: None,
        md5: None,
        is_virtual: true,
        extra_data: Some(HashMap::from([
            ("repo".to_string(), JsonValue::String(repo.clone())),
            (
                "outer_checksum".to_string(),
                JsonValue::String(outer_checksum.clone()),
            ),
            (
                "managers".to_string(),
                JsonValue::Array(managers.into_iter().map(JsonValue::String).collect()),
            ),
        ])),
        dependencies: nested_dependencies
            .into_iter()
            .map(build_nested_dependency)
            .collect::<Result<Vec<_>, _>>()?,
        repository_homepage_url: Some(build_hexdocs_homepage(&package_name, &repo)),
        repository_download_url: None,
        api_data_url: Some(build_hex_api_url(&package_name, &repo)),
        datasource_id: Some(DatasourceId::HexMixLock),
        purl: build_hex_purl(&package_name, Some(&version), Some(&repo)),
        ..ResolvedPackage::new(
            PackageType::Hex,
            if repo == "hexpm" {
                String::new()
            } else {
                repo.clone()
            },
            package_name.clone(),
            version.clone(),
        )
    };

    Ok(Some(Dependency {
        purl,
        extracted_requirement: Some(version),
        scope: Some("dependencies".to_string()),
        is_runtime: Some(true),
        is_optional: Some(false),
        is_pinned: Some(true),
        is_direct: Some(false),
        resolved_package: Some(Box::new(resolved_package)),
        extra_data: Some(HashMap::from([(
            "app".to_string(),
            JsonValue::String(app_name),
        )])),
    }))
}

fn build_nested_dependency(tuple: DependencyTuple) -> Result<Dependency, String> {
    let package_name = tuple
        .hex_name
        .clone()
        .unwrap_or_else(|| tuple.app_name.clone());
    Ok(Dependency {
        purl: build_hex_purl(&package_name, None, tuple.repo.as_deref()),
        extracted_requirement: Some(tuple.requirement),
        scope: Some("dependencies".to_string()),
        is_runtime: Some(!tuple.optional),
        is_optional: Some(tuple.optional),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

crate::register_parser!(
    "Hex mix.lock lockfile",
    &["**/mix.lock"],
    "hex",
    "Elixir",
    Some("https://hexdocs.pm/mix/Mix.Tasks.Deps.html"),
);

#[derive(Debug)]
struct DependencyTuple {
    app_name: String,
    requirement: String,
    hex_name: Option<String>,
    repo: Option<String>,
    optional: bool,
}

fn term_to_dependency_tuples(term: &Term) -> Result<Vec<DependencyTuple>, String> {
    let items = match term {
        Term::List(items) => items,
        _ => return Ok(Vec::new()),
    };

    let mut result = Vec::new();
    for item in items {
        let tuple = match item {
            Term::Tuple(items) if items.len() == 3 => items,
            _ => continue,
        };

        let app_name = term_to_atom(&tuple[0])?;
        let requirement = term_to_string(&tuple[1])?;
        let opts = term_to_keyword_map(&tuple[2])?;
        let hex_name = opts.get("hex").map(term_to_atom).transpose()?;
        let repo = opts.get("repo").map(term_to_string).transpose()?;
        let optional = opts
            .get("optional")
            .and_then(|term| match term {
                Term::Bool(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(false);

        result.push(DependencyTuple {
            app_name,
            requirement,
            hex_name,
            repo,
            optional,
        });
    }

    Ok(result)
}

fn term_to_keyword_map(term: &Term) -> Result<HashMap<String, Term>, String> {
    match term {
        Term::KeywordList(entries) => Ok(entries.iter().cloned().collect()),
        Term::List(entries) => {
            let mut map = HashMap::new();
            for entry in entries {
                if let Term::Tuple(items) = entry
                    && items.len() == 2
                {
                    map.insert(term_to_atom(&items[0])?, items[1].clone());
                }
            }
            Ok(map)
        }
        _ => Ok(HashMap::new()),
    }
}

fn build_hex_purl(name: &str, version: Option<&str>, repo: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new("hex", name).ok()?;
    if let Some(repo) = repo
        && repo != "hexpm"
    {
        purl.with_namespace(repo).ok()?;
    }
    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }
    Some(purl.to_string())
}

fn build_hexdocs_homepage(name: &str, repo: &str) -> String {
    if repo == "hexpm" {
        format!("https://hex.pm/packages/{}", name)
    } else {
        format!("https://hex.pm/packages/{}?repo={}", name, repo)
    }
}

fn build_hex_api_url(name: &str, repo: &str) -> String {
    if repo == "hexpm" {
        format!("https://hex.pm/api/packages/{}", name)
    } else {
        format!("https://hex.pm/api/repos/{}/packages/{}", repo, name)
    }
}

fn term_to_string(term: &Term) -> Result<String, String> {
    match term {
        Term::String(value) => Ok(value.clone()),
        Term::Atom(value) => Ok(value.clone()),
        Term::Integer(value) => Ok(value.to_string()),
        _ => Err("Expected string-like term".to_string()),
    }
}

fn term_to_atom(term: &Term) -> Result<String, String> {
    match term {
        Term::Atom(value) => Ok(value.clone()),
        _ => Err("Expected atom".to_string()),
    }
}

fn term_to_atom_list(term: &Term) -> Result<Vec<String>, String> {
    let items = match term {
        Term::List(items) => items,
        _ => return Ok(Vec::new()),
    };
    items.iter().map(term_to_atom).collect()
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            source,
        }
    }

    fn parse_term(&mut self) -> Result<Term, String> {
        self.skip_ws();
        match self.peek() {
            Some('%') => self.parse_map(),
            Some('{') => self.parse_tuple(),
            Some('[') => self.parse_list(),
            Some('"') => self.parse_string().map(Term::String),
            Some(':') => self.parse_atom().map(Term::Atom),
            Some(c) if c.is_ascii_digit() || c == '-' => self.parse_integer().map(Term::Integer),
            Some('t') | Some('f') => self.parse_bool().map(Term::Bool),
            Some(other) => Err(format!("Unexpected character '{}' at {}", other, self.pos)),
            None => Err("Unexpected end of mix.lock".to_string()),
        }
    }

    fn parse_map(&mut self) -> Result<Term, String> {
        self.expect('%')?;
        self.expect('{')?;
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some('}') {
                self.pos += 1;
                break;
            }
            let key = self.parse_term()?;
            self.skip_ws();
            if self.starts_with("=>") {
                self.expect_sequence("=>")?;
            } else {
                self.expect(':')?;
            }
            let value = self.parse_term()?;
            entries.push((key, value));
            self.skip_ws();
            if self.peek() == Some(',') {
                self.pos += 1;
            }
        }
        Ok(Term::Map(entries))
    }

    fn parse_tuple(&mut self) -> Result<Term, String> {
        self.expect('{')?;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some('}') {
                self.pos += 1;
                break;
            }
            items.push(self.parse_term()?);
            self.skip_ws();
            if self.peek() == Some(',') {
                self.pos += 1;
            }
        }
        Ok(Term::Tuple(items))
    }

    fn parse_list(&mut self) -> Result<Term, String> {
        self.expect('[')?;
        let mut keyword_entries = Vec::new();
        let mut items = Vec::new();
        let mut saw_keyword = false;

        loop {
            self.skip_ws();
            if self.peek() == Some(']') {
                self.pos += 1;
                break;
            }

            if let Some(keyword) = self.try_parse_keyword_key() {
                saw_keyword = true;
                let value = self.parse_term()?;
                keyword_entries.push((keyword, value));
            } else {
                items.push(self.parse_term()?);
            }

            self.skip_ws();
            if self.peek() == Some(',') {
                self.pos += 1;
            }
        }

        if saw_keyword && items.is_empty() {
            Ok(Term::KeywordList(keyword_entries))
        } else if saw_keyword {
            let mut merged = items;
            merged.extend(
                keyword_entries
                    .into_iter()
                    .map(|(k, v)| Term::Tuple(vec![Term::Atom(k), v])),
            );
            Ok(Term::List(merged))
        } else {
            Ok(Term::List(items))
        }
    }

    fn try_parse_keyword_key(&mut self) -> Option<String> {
        let saved = self.pos;
        self.skip_ws();
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '?' || c == '!' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start || self.peek() != Some(':') || self.peek_n(1) == Some(':') {
            self.pos = saved;
            return None;
        }
        let key: String = self.chars[start..self.pos].iter().collect();
        self.pos += 1;
        Some(key)
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

    fn parse_atom(&mut self) -> Result<String, String> {
        self.expect(':')?;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '?' || c == '!' || c == '@' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err("Expected atom after ':'".to_string());
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    fn parse_integer(&mut self) -> Result<i64, String> {
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
        self.source[start..self.byte_index(self.pos)]
            .parse::<i64>()
            .map_err(|e| format!("Invalid integer: {}", e))
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        if self.starts_with("true") {
            self.pos += 4;
            Ok(true)
        } else if self.starts_with("false") {
            self.pos += 5;
            Ok(false)
        } else {
            Err("Invalid boolean".to_string())
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        match self.peek() {
            Some(c) if c == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(c) => Err(format!("Expected '{}' but found '{}'", expected, c)),
            None => Err(format!("Expected '{}' but reached end of input", expected)),
        }
    }

    fn expect_sequence(&mut self, expected: &str) -> Result<(), String> {
        if self.starts_with(expected) {
            self.pos += expected.chars().count();
            Ok(())
        } else {
            Err(format!("Expected '{}' at {}", expected, self.pos))
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.chars[self.pos..]
            .iter()
            .collect::<String>()
            .starts_with(s)
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

    fn byte_index(&self, char_pos: usize) -> usize {
        self.chars.iter().take(char_pos).map(|c| c.len_utf8()).sum()
    }
}
