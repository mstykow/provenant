//! Parser for Git submodule manifest files (`.gitmodules`).
//!
//! Extracts submodule dependencies from `.gitmodules` files, treating
//! git submodules as package dependencies.
//!
//! # Supported Formats
//! - `.gitmodules` (Git submodule configuration)
//!
//! # Key Features
//! - Parses INI-style `.gitmodules` format
//! - Extracts submodule name, path, and URL
//! - Generates purl for GitHub/GitLab URLs when possible
//! - Reports submodules as dependencies
//!
//! # Implementation Notes
//! - Git submodules are treated as dependencies of the containing repository
//! - URLs are parsed to extract package name when possible
//! - Supports both https and git@ URL formats

use std::collections::HashMap;
use std::path::Path;

use crate::parser_warn as warn;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parsers::utils::read_file_to_string;

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Github;

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(DatasourceId::Gitmodules),
        ..Default::default()
    }
}

pub struct GitmodulesParser;

impl PackageParser for GitmodulesParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == ".gitmodules")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read .gitmodules {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        let submodules = parse_gitmodules(&content);
        if submodules.is_empty() {
            return vec![default_package_data()];
        }

        let dependencies: Vec<Dependency> = submodules
            .into_iter()
            .map(|sub| Dependency {
                purl: sub.purl,
                extracted_requirement: Some(format!("{} at {}", sub.path, sub.url)),
                scope: Some("runtime".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_direct: Some(true),
                resolved_package: None,
                extra_data: None,
                is_pinned: Some(false),
            })
            .collect();

        vec![PackageData {
            package_type: Some(PACKAGE_TYPE),
            datasource_id: Some(DatasourceId::Gitmodules),
            dependencies,
            ..Default::default()
        }]
    }
}

struct Submodule {
    path: String,
    url: String,
    purl: Option<String>,
}

fn parse_gitmodules(content: &str) -> Vec<Submodule> {
    let mut submodules = Vec::new();
    let mut current_section: Option<HashMap<String, String>> = None;
    let mut current_name: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(section) = current_section.take()
                && let Some(name) = current_name.take()
                && let Some(submodule) = build_submodule(name, section)
            {
                submodules.push(submodule);
            }

            let section_name = &line[1..line.len() - 1];
            if let Some(stripped) = section_name.strip_prefix("submodule ") {
                current_name = Some(stripped.trim_matches('"').to_string());
                current_section = Some(HashMap::new());
            }
        } else if let Some(ref mut section) = current_section
            && let Some((key, value)) = line.split_once('=')
        {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            section.insert(key, value);
        }
    }

    if let Some(section) = current_section
        && let Some(name) = current_name
        && let Some(submodule) = build_submodule(name, section)
    {
        submodules.push(submodule);
    }

    submodules
}

fn build_submodule(_name: String, section: HashMap<String, String>) -> Option<Submodule> {
    let path = section.get("path").cloned().unwrap_or_default();
    let url = section.get("url").cloned().unwrap_or_default();

    if path.is_empty() && url.is_empty() {
        return None;
    }

    let purl = build_purl_from_url(&url);

    Some(Submodule { path, url, purl })
}

fn build_purl_from_url(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }

    if let Some(purl) = parse_github_url(url) {
        return Some(purl);
    }

    if let Some(purl) = parse_gitlab_url(url) {
        return Some(purl);
    }

    None
}

fn parse_github_url(url: &str) -> Option<String> {
    let (namespace, name) = if url.starts_with("https://github.com/") {
        let path = url.strip_prefix("https://github.com/")?;
        parse_repo_path(path)?
    } else if url.starts_with("git@github.com:") {
        let path = url.strip_prefix("git@github.com:")?;
        parse_repo_path(path)?
    } else {
        return None;
    };

    Some(format!("pkg:github/{}/{}", namespace, name))
}

fn parse_gitlab_url(url: &str) -> Option<String> {
    let (namespace, name) = if url.starts_with("https://gitlab.com/") {
        let path = url.strip_prefix("https://gitlab.com/")?;
        parse_repo_path(path)?
    } else if url.starts_with("git@gitlab.com:") {
        let path = url.strip_prefix("git@gitlab.com:")?;
        parse_repo_path(path)?
    } else {
        return None;
    };

    Some(format!("pkg:gitlab/{}/{}", namespace, name))
}

fn parse_repo_path(path: &str) -> Option<(String, String)> {
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        return None;
    }

    let name = parts.last()?.to_string();
    let namespace = parts[..parts.len() - 1].join("/");

    if namespace.is_empty() || name.is_empty() {
        return None;
    }

    Some((namespace, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_gitmodules_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_is_match() {
        assert!(GitmodulesParser::is_match(Path::new(".gitmodules")));
        assert!(GitmodulesParser::is_match(Path::new(
            "/path/to/.gitmodules"
        )));
        assert!(!GitmodulesParser::is_match(Path::new("gitmodules")));
        assert!(!GitmodulesParser::is_match(Path::new(".gitmodules.bak")));
    }

    #[test]
    fn test_parse_single_submodule() {
        let content = r#"
[submodule "dep-lib"]
    path = lib/dep
    url = https://github.com/user/dep-lib.git
"#;
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].dependencies.len(), 1);
        let dep = &pkgs[0].dependencies[0];
        assert_eq!(dep.purl, Some("pkg:github/user/dep-lib".to_string()));
    }

    #[test]
    fn test_parse_multiple_submodules() {
        let content = r#"
[submodule "lib1"]
    path = libs/lib1
    url = https://github.com/org/lib1.git

[submodule "lib2"]
    path = libs/lib2
    url = git@github.com:org/lib2.git
"#;
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].dependencies.len(), 2);
    }

    #[test]
    fn test_parse_git_ssh_url() {
        let content = r#"
[submodule "private-repo"]
    path = private
    url = git@github.com:company/private-repo.git
"#;
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        let dep = &pkgs[0].dependencies[0];
        assert_eq!(
            dep.purl,
            Some("pkg:github/company/private-repo".to_string())
        );
    }

    #[test]
    fn test_parse_gitlab_url() {
        let content = r#"
[submodule "gitlab-dep"]
    path = gitlab-lib
    url = https://gitlab.com/group/project.git
"#;
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        let dep = &pkgs[0].dependencies[0];
        assert_eq!(dep.purl, Some("pkg:gitlab/group/project".to_string()));
    }

    #[test]
    fn test_parse_unknown_url() {
        let content = r#"
[submodule "custom"]
    path = custom
    url = https://example.com/repo.git
"#;
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        let dep = &pkgs[0].dependencies[0];
        assert!(dep.purl.is_none());
        assert!(
            dep.extracted_requirement
                .as_ref()
                .unwrap()
                .contains("https://example.com/repo.git")
        );
    }

    #[test]
    fn test_parse_empty_file() {
        let content = "";
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        assert_eq!(pkgs.len(), 1);
        assert!(pkgs[0].dependencies.is_empty());
    }

    #[test]
    fn test_parse_with_comments() {
        let content = r#"
# This is a comment
[submodule "lib"]
    ; another comment
    path = lib
    url = https://github.com/user/lib.git
"#;
        let file = create_gitmodules_file(content);
        let pkgs = GitmodulesParser::extract_packages(file.path());
        assert_eq!(pkgs[0].dependencies.len(), 1);
    }
}

crate::register_parser!(
    "Git submodules manifest",
    &["**/.gitmodules"],
    "gitmodules",
    "",
    Some("https://git-scm.com/docs/gitmodules"),
);
