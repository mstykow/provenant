use std::collections::BTreeMap;
use std::path::Path;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType};
use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, read_file_to_string, truncate_field};

use super::PackageParser;
use super::go::{create_golang_purl, split_module_path};

const PACKAGE_TYPE: PackageType = PackageType::Golang;

fn default_package_data() -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("Go".to_string()),
        datasource_id: Some(DatasourceId::GoModGraph),
        ..Default::default()
    }
}

pub struct GoModGraphParser;

impl PackageParser for GoModGraphParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "go.mod.graph" | "go.modgraph"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Go module graph at {:?}: {}", path, e);
                return vec![default_package_data()];
            }
        };

        vec![parse_go_mod_graph(&content)]
    }
}

#[derive(Debug, Clone)]
struct GraphModule<'a> {
    module_path: &'a str,
    version: Option<&'a str>,
}

pub(crate) fn parse_go_mod_graph(content: &str) -> PackageData {
    let mut root_module: Option<String> = None;
    let mut dependency_map: BTreeMap<String, Dependency> = BTreeMap::new();

    for line in content.lines().take(MAX_ITERATION_COUNT) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let Some(source) = parts.next() else {
            continue;
        };
        let Some(target) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            continue;
        }

        let source = parse_graph_module(source);
        let target = parse_graph_module(target);

        if source.version.is_none() && root_module.is_none() {
            root_module = Some(truncate_field(source.module_path.to_string()));
        }

        let Some(purl) = create_golang_purl(target.module_path, target.version) else {
            continue;
        };

        dependency_map
            .entry(purl.clone())
            .and_modify(|existing: &mut Dependency| {
                if source.version.is_none() {
                    existing.is_direct = Some(true);
                }
            })
            .or_insert_with(|| Dependency {
                purl: Some(truncate_field(purl)),
                extracted_requirement: target.version.map(|v| truncate_field(v.to_string())),
                scope: Some("dependency".to_string()),
                is_runtime: Some(true),
                is_optional: Some(false),
                is_pinned: Some(target.version.is_some()),
                is_direct: Some(source.version.is_none()),
                resolved_package: None,
                extra_data: None,
            });
    }

    let (namespace, name): (Option<String>, String) = root_module
        .as_deref()
        .map(split_module_path)
        .unwrap_or((None, String::new()));

    let homepage_url = root_module
        .as_ref()
        .map(|module| truncate_field(format!("https://pkg.go.dev/{module}")));

    let vcs_url = root_module
        .as_ref()
        .map(|module| truncate_field(format!("https://{module}.git")));

    let purl = root_module
        .as_deref()
        .and_then(|module| create_golang_purl(module, None))
        .map(truncate_field);

    PackageData {
        package_type: Some(PACKAGE_TYPE),
        primary_language: Some("Go".to_string()),
        datasource_id: Some(DatasourceId::GoModGraph),
        namespace: namespace.map(truncate_field),
        name: (!name.is_empty()).then_some(truncate_field(name)),
        homepage_url: homepage_url.clone(),
        repository_homepage_url: homepage_url,
        vcs_url,
        purl,
        dependencies: dependency_map.into_values().collect(),
        ..Default::default()
    }
}

fn parse_graph_module(token: &str) -> GraphModule<'_> {
    if let Some((module_path, version)) = token.rsplit_once('@') {
        GraphModule {
            module_path,
            version: Some(version),
        }
    } else {
        GraphModule {
            module_path: token,
            version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatasourceId;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_match() {
        assert!(GoModGraphParser::is_match(Path::new("go.mod.graph")));
        assert!(GoModGraphParser::is_match(Path::new("go.modgraph")));
        assert!(!GoModGraphParser::is_match(Path::new("go.mod")));
    }

    #[test]
    fn test_parse_go_mod_graph_direct_and_transitive() {
        let content = "example.com/myapp github.com/gin-gonic/gin@v1.9.0\nexample.com/myapp github.com/stretchr/testify@v1.8.4\ngithub.com/gin-gonic/gin@v1.9.0 golang.org/x/net@v0.10.0\n";

        let package_data = parse_go_mod_graph(content);

        assert_eq!(package_data.datasource_id, Some(DatasourceId::GoModGraph));
        assert_eq!(package_data.namespace.as_deref(), Some("example.com"));
        assert_eq!(package_data.name.as_deref(), Some("myapp"));
        assert_eq!(
            package_data.purl.as_deref(),
            Some("pkg:golang/example.com/myapp")
        );
        assert_eq!(package_data.dependencies.len(), 3);

        let direct = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:golang/github.com/gin-gonic/gin@v1.9.0"))
            .unwrap();
        assert_eq!(direct.is_direct, Some(true));

        let transitive = package_data
            .dependencies
            .iter()
            .find(|dep| dep.purl.as_deref() == Some("pkg:golang/golang.org/x/net@v0.10.0"))
            .unwrap();
        assert_eq!(transitive.is_direct, Some(false));
    }

    #[test]
    fn test_extract_packages_graceful_error_handling() {
        let path = Path::new("/nonexistent/path/go.mod.graph");
        let result = GoModGraphParser::extract_first_package(path);

        assert_eq!(result.package_type, Some(PackageType::Golang));
        assert_eq!(result.datasource_id, Some(DatasourceId::GoModGraph));
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_extract_packages_reads_file() {
        let file = NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            "example.com/myapp github.com/gin-gonic/gin@v1.9.0\n",
        )
        .unwrap();

        let package_data = GoModGraphParser::extract_first_package(file.path());

        assert_eq!(package_data.name.as_deref(), Some("myapp"));
        assert_eq!(package_data.dependencies.len(), 1);
    }
}

crate::register_parser!(
    "Go module graph file",
    &["*go.mod.graph", "*go.modgraph"],
    "golang",
    "Go",
    Some("https://go.dev/ref/mod#go-mod-graph"),
);
