// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use packageurl::PackageUrl;

use crate::models::{Dependency, Party};
use crate::parsers::utils::truncate_field;

use super::PACKAGE_TYPE;

pub(super) fn make_party(
    r#type: Option<&str>,
    role: &str,
    name: Option<String>,
    email: Option<String>,
) -> Party {
    Party {
        r#type: r#type.map(|t| t.to_string()),
        role: Some(role.to_string()),
        name,
        email,
        url: None,
        organization: None,
        organization_url: None,
        timezone: None,
    }
}
use super::{DEP_FIELDS, DEP_RE, NAMESPACE_PRIORITY, Namespace};

pub(super) fn detect_namespace(version: Option<&str>, maintainer: Option<&str>) -> Option<String> {
    if let Some(ver) = version {
        let ver_lower = ver.to_lowercase();
        for ns in NAMESPACE_PRIORITY {
            if ns
                .version_clues()
                .iter()
                .any(|clue| ver_lower.contains(clue))
            {
                return Some(ns.to_string());
            }
        }
    }

    if let Some(maint) = maintainer {
        let maint_lower = maint.to_lowercase();
        for ns in NAMESPACE_PRIORITY {
            if ns
                .maintainer_clues()
                .iter()
                .any(|clue| maint_lower.contains(clue))
            {
                return Some(ns.to_string());
            }
        }
    }

    Some(Namespace::Debian.to_string())
}

pub(super) fn build_debian_purl(
    name: &str,
    version: Option<&str>,
    namespace: Option<&str>,
    architecture: Option<&str>,
) -> Option<String> {
    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;

    if let Some(ns) = namespace {
        purl.with_namespace(ns).ok()?;
    }

    if let Some(ver) = version {
        purl.with_version(ver).ok()?;
    }

    if let Some(arch) = architecture {
        purl.add_qualifier("arch", arch).ok()?;
    }

    Some(purl.to_string())
}

pub(super) fn parse_all_dependencies(
    headers: &HashMap<String, Vec<String>>,
    namespace: Option<&str>,
) -> Vec<Dependency> {
    let mut dependencies = Vec::new();

    for spec in DEP_FIELDS {
        if let Some(dep_str) = crate::parsers::rfc822::get_header_first(headers, spec.field()) {
            dependencies.extend(parse_dependency_field(
                &dep_str,
                spec.scope(),
                spec.is_runtime(),
                spec.is_optional(),
                namespace,
            ));
        }
    }

    dependencies
}

/// Parses a Debian dependency field value.
///
/// Debian dependencies are comma-separated, with optional version constraints
/// in parentheses and alternative packages separated by `|`.
///
/// Format: `pkg1 (>= 1.0), pkg2 | pkg3 (<< 2.0), pkg4`
///
/// Alternatives (|) are treated as separate optional dependencies.
pub(super) fn parse_dependency_field(
    dep_str: &str,
    scope: &str,
    is_runtime: bool,
    is_optional: bool,
    namespace: Option<&str>,
) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for group in dep_str
        .split(',')
        .take(crate::parsers::utils::MAX_ITERATION_COUNT)
    {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }

        let alternatives: Vec<&str> = group.split('|').collect();
        let has_alternatives = alternatives.len() > 1;

        for alt in alternatives {
            let alt = alt.trim();
            if alt.is_empty() {
                continue;
            }

            if let Some(caps) = DEP_RE.captures(alt) {
                let pkg_name = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let operator = caps.get(2).map(|m| m.as_str().trim());
                let version = caps.get(3).map(|m| m.as_str().trim());

                if pkg_name.is_empty() {
                    continue;
                }

                if pkg_name.starts_with('$') {
                    continue;
                }

                let extracted_requirement = match (operator, version) {
                    (Some(op), Some(ver)) => Some(truncate_field(format!("{} {}", op, ver))),
                    _ => None,
                };

                let is_pinned = operator.map(|op| op == "=");

                let purl = build_debian_purl(pkg_name, None, namespace, None);

                deps.push(Dependency {
                    purl,
                    extracted_requirement,
                    scope: Some(scope.to_string()),
                    is_runtime: Some(is_runtime),
                    is_optional: Some(is_optional || has_alternatives),
                    is_pinned,
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                });
            }
        }
    }

    deps
}

/// Parses the Source field which may contain a version in parentheses.
///
/// Format: `source-name` or `source-name (version)`
pub(super) fn parse_source_field(source: Option<&str>, namespace: Option<&str>) -> Vec<String> {
    let Some(source_str) = source else {
        return Vec::new();
    };

    let trimmed = source_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Extract name and optional version from "name (version)" format
    let (name, version) = if let Some(paren_start) = trimmed.find(" (") {
        let name = trimmed[..paren_start].trim();
        let version = trimmed[paren_start + 2..].trim_end_matches(')').trim();
        (
            name,
            if version.is_empty() {
                None
            } else {
                Some(version)
            },
        )
    } else {
        (trimmed, None)
    };

    if let Some(purl) = build_debian_purl(name, version, namespace, None) {
        vec![purl]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_namespace_from_ubuntu_version() {
        assert_eq!(
            detect_namespace(Some("1.0-1ubuntu1"), None),
            Some("ubuntu".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_from_debian_version() {
        assert_eq!(
            detect_namespace(Some("1.0-1+deb11u1"), None),
            Some("debian".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_from_ubuntu_maintainer() {
        assert_eq!(
            detect_namespace(
                None,
                Some("Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>")
            ),
            Some("ubuntu".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_from_debian_maintainer() {
        assert_eq!(
            detect_namespace(None, Some("John Doe <john@debian.org>")),
            Some("debian".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_default() {
        assert_eq!(
            detect_namespace(None, Some("Unknown <unknown@example.com>")),
            Some("debian".to_string())
        );
    }

    #[test]
    fn test_detect_namespace_version_takes_priority() {
        assert_eq!(
            detect_namespace(Some("1.0ubuntu1"), Some("maintainer@debian.org")),
            Some("ubuntu".to_string())
        );
    }

    #[test]
    fn test_build_purl_basic() {
        let purl = build_debian_purl("curl", Some("7.68.0-1"), Some("debian"), Some("amd64"));
        assert_eq!(
            purl,
            Some("pkg:deb/debian/curl@7.68.0-1?arch=amd64".to_string())
        );
    }

    #[test]
    fn test_build_purl_no_version() {
        let purl = build_debian_purl("curl", None, Some("debian"), Some("any"));
        assert_eq!(purl, Some("pkg:deb/debian/curl?arch=any".to_string()));
    }

    #[test]
    fn test_build_purl_no_arch() {
        let purl = build_debian_purl("curl", Some("7.68.0"), Some("ubuntu"), None);
        assert_eq!(purl, Some("pkg:deb/ubuntu/curl@7.68.0".to_string()));
    }

    #[test]
    fn test_build_purl_no_namespace() {
        let purl = build_debian_purl("curl", Some("7.68.0"), None, None);
        assert_eq!(purl, Some("pkg:deb/curl@7.68.0".to_string()));
    }

    #[test]
    fn test_parse_simple_dependency() {
        let deps = parse_dependency_field("libc6", "depends", true, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
        assert_eq!(deps[0].extracted_requirement, None);
        assert_eq!(deps[0].scope, Some("depends".to_string()));
    }

    #[test]
    fn test_parse_dependency_with_version() {
        let deps =
            parse_dependency_field("libc6 (>= 2.17)", "depends", true, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
        assert_eq!(deps[0].extracted_requirement, Some(">= 2.17".to_string()));
    }

    #[test]
    fn test_parse_dependency_exact_version() {
        let deps = parse_dependency_field(
            "libc6 (= 2.31-13+deb11u5)",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].is_pinned, Some(true));
    }

    #[test]
    fn test_parse_dependency_strict_less() {
        let deps =
            parse_dependency_field("libgcc-s1 (<< 12)", "breaks", false, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].extracted_requirement, Some("<< 12".to_string()));
        assert_eq!(deps[0].scope, Some("breaks".to_string()));
    }

    #[test]
    fn test_parse_multiple_dependencies() {
        let deps = parse_dependency_field(
            "libc6 (>= 2.17), libssl1.1 (>= 1.1.0), zlib1g (>= 1:1.2.0)",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 3);
    }

    #[test]
    fn test_parse_dependency_alternatives() {
        let deps = parse_dependency_field(
            "libssl1.1 | libssl3",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].is_optional, Some(true));
        assert_eq!(deps[1].is_optional, Some(true));
    }

    #[test]
    fn test_parse_dependency_skips_substitutions() {
        let deps = parse_dependency_field(
            "${shlibs:Depends}, ${misc:Depends}, libc6",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
    }

    #[test]
    fn test_parse_dependency_with_arch_qualifier() {
        let deps = parse_dependency_field(
            "libc6 (>= 2.17) [amd64]",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].purl, Some("pkg:deb/debian/libc6".to_string()));
    }

    #[test]
    fn test_parse_empty_dependency() {
        let deps = parse_dependency_field("", "depends", true, false, Some("debian"));
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_source_field_name_only() {
        let sources = parse_source_field(Some("util-linux"), Some("debian"));
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], "pkg:deb/debian/util-linux");
    }

    #[test]
    fn test_parse_source_field_with_version() {
        let sources = parse_source_field(Some("util-linux (2.36.1-8+deb11u1)"), Some("debian"));
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], "pkg:deb/debian/util-linux@2.36.1-8%2Bdeb11u1");
    }

    #[test]
    fn test_parse_source_field_empty() {
        let sources = parse_source_field(None, Some("debian"));
        assert!(sources.is_empty());
    }

    #[test]
    fn test_dependency_with_epoch_version() {
        let deps = parse_dependency_field(
            "zlib1g (>= 1:1.2.11)",
            "depends",
            true,
            false,
            Some("debian"),
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].extracted_requirement,
            Some(">= 1:1.2.11".to_string())
        );
    }

    #[test]
    fn test_dependency_with_plus_in_name() {
        let deps =
            parse_dependency_field("libstdc++6 (>= 10)", "depends", true, false, Some("debian"));
        assert_eq!(deps.len(), 1);
        assert!(deps[0].purl.as_ref().unwrap().contains("libstdc%2B%2B6"));
    }
}
