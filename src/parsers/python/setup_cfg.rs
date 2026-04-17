use super::PythonParser;
use super::utils::{
    ProjectUrls, apply_project_url_mappings, default_package_data,
    extract_setup_cfg_dependency_name, has_private_classifier, parse_setup_cfg_keywords,
    parse_setup_cfg_project_urls,
};
use crate::models::{DatasourceId, Dependency, PackageData, Party};
use crate::parser_warn as warn;
use crate::parsers::PackageParser;
use crate::parsers::utils::{read_file_to_string, truncate_field};
use packageurl::PackageUrl;
use std::collections::HashMap;
use std::path::Path;

type IniSections = HashMap<String, HashMap<String, Vec<String>>>;

pub(super) fn extract_from_setup_cfg(path: &Path) -> PackageData {
    let content = match read_file_to_string(path, None) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read setup.cfg at {:?}: {}", path, e);
            return default_package_data(path);
        }
    };

    let sections = parse_setup_cfg(&content);
    let name = get_ini_value(&sections, "metadata", "name").map(truncate_field);
    let version = get_ini_value(&sections, "metadata", "version").map(truncate_field);
    let description = get_ini_value(&sections, "metadata", "description").map(truncate_field);
    let author = get_ini_value(&sections, "metadata", "author").map(truncate_field);
    let author_email = get_ini_value(&sections, "metadata", "author_email");
    let maintainer = get_ini_value(&sections, "metadata", "maintainer").map(truncate_field);
    let maintainer_email = get_ini_value(&sections, "metadata", "maintainer_email");
    let license = get_ini_value(&sections, "metadata", "license").map(truncate_field);
    let homepage_url = get_ini_value(&sections, "metadata", "url").map(truncate_field);
    let classifiers = get_ini_values(&sections, "metadata", "classifiers");
    let keywords = parse_setup_cfg_keywords(get_ini_value(&sections, "metadata", "keywords"));
    let python_requires = get_ini_value(&sections, "options", "python_requires");
    let parsed_project_urls =
        parse_setup_cfg_project_urls(&get_ini_values(&sections, "metadata", "project_urls"));
    let mut urls = ProjectUrls {
        homepage_url,
        download_url: None,
        bug_tracking_url: None,
        code_view_url: None,
        vcs_url: None,
        changelog_url: None,
    };
    let mut extra_data = HashMap::new();

    let mut parties = Vec::new();
    if author.is_some() || author_email.is_some() {
        parties.push(Party::person("author", author, author_email));
    }

    if maintainer.is_some() || maintainer_email.is_some() {
        parties.push(Party::person("maintainer", maintainer, maintainer_email));
    }

    let declared_license_expression = None;
    let declared_license_expression_spdx = None;
    let license_detections = Vec::new();
    let extracted_license_statement = license.clone();

    let dependencies = extract_setup_cfg_dependencies(&sections);

    if let Some(value) = python_requires {
        extra_data.insert(
            "python_requires".to_string(),
            serde_json::Value::String(value),
        );
    }

    apply_project_url_mappings(&parsed_project_urls, &mut urls, &mut extra_data);

    let extra_data = if extra_data.is_empty() {
        None
    } else {
        Some(extra_data)
    };

    let purl = name.as_ref().and_then(|n| {
        let mut package_url = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), n).ok()?;
        if let Some(v) = &version {
            package_url.with_version(v).ok()?;
        }
        Some(package_url.to_string())
    });

    PackageData {
        package_type: Some(PythonParser::PACKAGE_TYPE),
        name,
        version,
        primary_language: Some("Python".to_string()),
        description,
        parties,
        keywords,
        homepage_url: urls.homepage_url,
        bug_tracking_url: urls.bug_tracking_url,
        code_view_url: urls.code_view_url,
        vcs_url: urls.vcs_url,
        declared_license_expression,
        declared_license_expression_spdx,
        license_detections,
        extracted_license_statement,
        is_private: has_private_classifier(&classifiers),
        extra_data,
        dependencies,
        datasource_id: Some(DatasourceId::PypiSetupCfg),
        purl,
        ..Default::default()
    }
}

fn parse_setup_cfg(content: &str) -> IniSections {
    let mut sections: IniSections = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut current_key: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let stripped = line.trim_start();
        if stripped.starts_with('#') || stripped.starts_with(';') {
            continue;
        }

        if stripped.starts_with('[') && stripped.ends_with(']') {
            let section_name = stripped
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_ascii_lowercase();
            current_section = if section_name.is_empty() {
                None
            } else {
                Some(section_name)
            };
            current_key = None;
            continue;
        }

        if (line.starts_with(' ') || line.starts_with('\t')) && current_key.is_some() {
            if let (Some(section), Some(key)) = (current_section.as_ref(), current_key.as_ref()) {
                let value = stripped.trim();
                if !value.is_empty() {
                    sections
                        .entry(section.clone())
                        .or_default()
                        .entry(key.clone())
                        .or_default()
                        .push(value.to_string());
                }
            }
            continue;
        }

        if let Some((key, value)) = stripped.split_once('=')
            && let Some(section) = current_section.as_ref()
        {
            let key_name = key.trim().to_ascii_lowercase();
            let value_trimmed = value.trim();
            let entry = sections
                .entry(section.clone())
                .or_default()
                .entry(key_name.clone())
                .or_default();
            if !value_trimmed.is_empty() {
                entry.push(value_trimmed.to_string());
            }
            current_key = Some(key_name);
        }
    }

    sections
}

fn get_ini_value(sections: &IniSections, section: &str, key: &str) -> Option<String> {
    sections
        .get(&section.to_ascii_lowercase())
        .and_then(|values| values.get(&key.to_ascii_lowercase()))
        .and_then(|entries| entries.first())
        .map(|value| value.trim().to_string())
}

fn get_ini_values(sections: &IniSections, section: &str, key: &str) -> Vec<String> {
    sections
        .get(&section.to_ascii_lowercase())
        .and_then(|values| values.get(&key.to_ascii_lowercase()))
        .cloned()
        .unwrap_or_default()
}

fn extract_setup_cfg_dependencies(sections: &IniSections) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for (sub_section, scope) in [
        ("install_requires", "install"),
        ("tests_require", "test"),
        ("setup_requires", "setup"),
    ] {
        let reqs = get_ini_values(sections, "options", sub_section);
        dependencies.extend(parse_setup_cfg_requirements(&reqs, scope, false));
    }

    if let Some(extras) = sections.get("options.extras_require") {
        let mut extra_items: Vec<_> = extras.iter().collect();
        extra_items.sort_by_key(|(name, _)| *name);
        for (extra_name, reqs) in extra_items {
            dependencies.extend(parse_setup_cfg_requirements(reqs, extra_name, true));
        }
    }

    dependencies
}

fn parse_setup_cfg_requirements(
    reqs: &[String],
    scope: &str,
    is_optional: bool,
) -> Vec<Dependency> {
    reqs.iter()
        .filter_map(|req| build_setup_cfg_dependency(req, scope, is_optional))
        .collect()
}

fn build_setup_cfg_dependency(req: &str, scope: &str, is_optional: bool) -> Option<Dependency> {
    let trimmed = req.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let name = extract_setup_cfg_dependency_name(trimmed)?;
    let purl = PackageUrl::new(PythonParser::PACKAGE_TYPE.as_str(), &name).ok()?;

    Some(Dependency {
        purl: Some(purl.to_string()),
        extracted_requirement: Some(normalize_setup_cfg_requirement(trimmed)),
        scope: Some(scope.to_string()),
        is_runtime: Some(true),
        is_optional: Some(is_optional),
        is_pinned: Some(false),
        is_direct: Some(true),
        resolved_package: None,
        extra_data: None,
    })
}

fn normalize_setup_cfg_requirement(req: &str) -> String {
    req.chars().filter(|c| !c.is_whitespace()).collect()
}
