use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::parser_warn as warn;
use packageurl::PackageUrl;
use serde_json::Value as JsonValue;

use crate::models::{DatasourceId, Dependency, PackageData, PackageType, Party};
use crate::parsers::utils::{read_file_to_string, split_name_email};

use super::PackageParser;

const PACKAGE_TYPE: PackageType = PackageType::Alpm;
const PACKAGE_NAMESPACE: &str = "arch";

pub struct ArchSrcinfoParser;
pub struct ArchPkginfoParser;

impl PackageParser for ArchSrcinfoParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, ".SRCINFO" | ".AURINFO"))
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read Arch source metadata {:?}: {}", path, e);
                return vec![default_package_data(srcinfo_datasource_id(path))];
            }
        };

        parse_srcinfo_like(&content, srcinfo_datasource_id(path))
    }
}

impl PackageParser for ArchPkginfoParser {
    const PACKAGE_TYPE: PackageType = PACKAGE_TYPE;

    fn is_match(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()) == Some(".PKGINFO")
    }

    fn extract_packages(path: &Path) -> Vec<PackageData> {
        let content = match read_file_to_string(path, None) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read Arch .PKGINFO {:?}: {}", path, e);
                return vec![default_package_data(DatasourceId::ArchPkginfo)];
            }
        };

        vec![parse_pkginfo(&content)]
    }
}

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        namespace: Some(PACKAGE_NAMESPACE.to_string()),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

fn srcinfo_datasource_id(path: &Path) -> DatasourceId {
    match path.file_name().and_then(|name| name.to_str()) {
        Some(".AURINFO") => DatasourceId::ArchAurinfo,
        _ => DatasourceId::ArchSrcinfo,
    }
}

type MultiMap = HashMap<String, Vec<String>>;

fn parse_key_value_lines(content: &str) -> MultiMap {
    let mut fields: MultiMap = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() {
                fields
                    .entry(key.to_string())
                    .or_default()
                    .push(value.to_string());
            }
        }
    }

    fields
}

fn parse_srcinfo_like(content: &str, datasource_id: DatasourceId) -> Vec<PackageData> {
    let mut pkgbase: MultiMap = HashMap::new();
    let mut packages: Vec<MultiMap> = Vec::new();
    let mut current_is_pkgbase = true;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        if key == "pkgbase" {
            pkgbase
                .entry(key.to_string())
                .or_default()
                .push(value.to_string());
            current_is_pkgbase = true;
            continue;
        }

        if key == "pkgname" {
            packages.push(HashMap::from([(key.to_string(), vec![value.to_string()])]));
            current_is_pkgbase = false;
            continue;
        }

        let target = if current_is_pkgbase {
            &mut pkgbase
        } else {
            packages.last_mut().unwrap_or(&mut pkgbase)
        };

        target
            .entry(key.to_string())
            .or_default()
            .push(value.to_string());
    }

    if packages.is_empty() {
        packages.push(HashMap::new());
    }

    let results: Vec<_> = packages
        .into_iter()
        .filter_map(|package_section| {
            let merged = merge_srcinfo_sections(&pkgbase, &package_section);
            let pkg = build_package_from_arch_metadata(&merged, datasource_id, true);
            pkg.name.is_some().then_some(pkg)
        })
        .collect();

    if results.is_empty() {
        vec![default_package_data(datasource_id)]
    } else {
        results
    }
}

fn merge_srcinfo_sections(pkgbase: &MultiMap, package: &MultiMap) -> MultiMap {
    let mut merged = pkgbase.clone();

    for (key, values) in package {
        if should_append_srcinfo_values(key) {
            merged
                .entry(key.clone())
                .or_default()
                .extend(values.clone());
        } else {
            merged.insert(key.clone(), values.clone());
        }
    }

    if !merged.contains_key("pkgname")
        && let Some(pkgbase_name) = pkgbase.get("pkgbase").and_then(|vals| vals.first())
    {
        merged.insert("pkgname".to_string(), vec![pkgbase_name.clone()]);
    }

    merged
}

fn should_append_srcinfo_values(key: &str) -> bool {
    matches!(
        key,
        "arch"
            | "groups"
            | "license"
            | "noextract"
            | "options"
            | "backup"
            | "validpgpkeys"
            | "source"
            | "depends"
            | "makedepends"
            | "checkdepends"
            | "optdepends"
            | "provides"
            | "conflicts"
            | "replaces"
            | "md5sums"
            | "sha1sums"
            | "sha224sums"
            | "sha256sums"
            | "sha384sums"
            | "sha512sums"
            | "b2sums"
            | "cksums"
    ) || is_arch_variant_key(key)
}

fn is_arch_variant_key(key: &str) -> bool {
    arch_variant_base(key).is_some()
}

fn arch_variant_base(key: &str) -> Option<&'static str> {
    [
        "source",
        "depends",
        "makedepends",
        "checkdepends",
        "optdepends",
        "provides",
        "conflicts",
        "replaces",
        "md5sums",
        "sha1sums",
        "sha224sums",
        "sha256sums",
        "sha384sums",
        "sha512sums",
        "b2sums",
        "cksums",
    ]
    .into_iter()
    .find(|base| {
        key.strip_prefix(base)
            .and_then(|rest| rest.strip_prefix('_'))
            .is_some_and(|arch| !arch.is_empty())
    })
}

fn parse_pkginfo(content: &str) -> PackageData {
    let fields = parse_key_value_lines(content);
    build_package_from_arch_metadata(&fields, DatasourceId::ArchPkginfo, false)
}

fn build_package_from_arch_metadata(
    fields: &MultiMap,
    datasource_id: DatasourceId,
    is_srcinfo_like: bool,
) -> PackageData {
    let name = get_first(fields, "pkgname");
    let pkgbase = get_first(fields, "pkgbase").or_else(|| name.clone());
    let version = if is_srcinfo_like {
        build_srcinfo_version(fields)
    } else {
        get_first(fields, "pkgver")
    };
    let description = get_first(fields, "pkgdesc");
    let homepage_url = get_first(fields, "url");
    let extracted_license_statement = join_values(fields.get("license"));
    let arch_values = get_all(fields, "arch");
    let purl_arch = (arch_values.len() == 1).then(|| arch_values[0].as_str());

    let mut package = default_package_data(datasource_id);
    package.name = name.clone();
    package.version = version.clone();
    package.description = description;
    package.homepage_url = homepage_url;
    package.extracted_license_statement = extracted_license_statement;
    package.primary_language = None;
    package.purl = name
        .as_deref()
        .and_then(|name| build_alpm_purl(name, version.as_deref(), purl_arch));
    package.source_packages = pkgbase
        .as_deref()
        .and_then(|base| build_alpm_purl(base, version.as_deref(), purl_arch))
        .into_iter()
        .collect();

    if !is_srcinfo_like {
        if let Some(packager) = get_first(fields, "packager") {
            let (name, email) = split_name_email(&packager);
            package.parties.push(Party {
                r#type: Some("person".to_string()),
                role: Some("packager".to_string()),
                name,
                email,
                url: None,
                organization: None,
                organization_url: None,
                timezone: None,
            });
        }
        package.size = get_first(fields, "size").and_then(|size| size.parse::<u64>().ok());
    }

    package.dependencies = build_dependencies(fields);
    package.extra_data = build_extra_data(fields, is_srcinfo_like, purl_arch);
    package
}

fn build_srcinfo_version(fields: &MultiMap) -> Option<String> {
    let pkgver = get_first(fields, "pkgver")?;
    let pkgrel = get_first(fields, "pkgrel");
    let epoch = get_first(fields, "epoch");

    let mut version = match pkgrel {
        Some(pkgrel) => format!("{}-{}", pkgver, pkgrel),
        None => pkgver,
    };

    if let Some(epoch) = epoch
        && epoch != "0"
    {
        version = format!("{}:{}", epoch, version);
    }

    Some(version)
}

fn build_alpm_purl(name: &str, version: Option<&str>, arch: Option<&str>) -> Option<String> {
    let mut purl = PackageUrl::new(PACKAGE_TYPE.as_str(), name).ok()?;
    purl.with_namespace(PACKAGE_NAMESPACE).ok()?;

    if let Some(version) = version {
        purl.with_version(version).ok()?;
    }

    if let Some(arch) = arch {
        purl.add_qualifier("arch", arch).ok()?;
    }

    Some(purl.to_string())
}

fn build_dependencies(fields: &MultiMap) -> Vec<Dependency> {
    let mut dependencies = Vec::new();
    let mut keys: Vec<_> = fields.keys().cloned().collect();
    keys.sort();

    for key in keys {
        let Some((scope, is_runtime, is_optional)) = dependency_semantics(&key) else {
            continue;
        };

        for value in get_all(fields, &key) {
            if let Some(dep_name) = extract_arch_dependency_name(&value) {
                dependencies.push(Dependency {
                    purl: build_alpm_purl(&dep_name, None, None),
                    extracted_requirement: Some(value.clone()),
                    scope: Some(scope.to_string()),
                    is_runtime: Some(is_runtime),
                    is_optional: Some(is_optional),
                    is_pinned: Some(false),
                    is_direct: Some(true),
                    resolved_package: None,
                    extra_data: None,
                });
            }
        }
    }

    dependencies
}

fn dependency_semantics(key: &str) -> Option<(&str, bool, bool)> {
    let base = key;
    let normalized = arch_variant_base(key).unwrap_or(key);

    match normalized {
        "depends" | "depend" => Some((base, true, false)),
        "makedepends" | "makedepend" => Some((base, false, false)),
        "checkdepends" | "checkdepend" => Some((base, false, false)),
        "optdepends" | "optdepend" => Some((base, true, true)),
        _ => None,
    }
}

fn extract_arch_dependency_name(value: &str) -> Option<String> {
    let dep = value.split(':').next()?.trim();
    let end = dep.find(['<', '>', '=']).unwrap_or(dep.len());
    let name = dep[..end].trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn build_extra_data(
    fields: &MultiMap,
    is_srcinfo_like: bool,
    purl_arch: Option<&str>,
) -> Option<HashMap<String, JsonValue>> {
    let consumed: HashSet<&str> = HashSet::from([
        "pkgbase", "pkgname", "pkgver", "pkgrel", "epoch", "pkgdesc", "url", "license", "packager",
        "size",
    ]);

    let mut extra = HashMap::new();

    for (key, values) in fields {
        if consumed.contains(key.as_str()) {
            continue;
        }

        let value = if should_force_array_extra_value(key) {
            JsonValue::Array(values.iter().cloned().map(JsonValue::String).collect())
        } else if values.len() == 1 {
            if key == "builddate" {
                values[0]
                    .parse::<u64>()
                    .map(JsonValue::from)
                    .unwrap_or_else(|_| JsonValue::String(values[0].clone()))
            } else {
                JsonValue::String(values[0].clone())
            }
        } else {
            JsonValue::Array(values.iter().cloned().map(JsonValue::String).collect())
        };
        extra.insert(key.clone(), value);
    }

    if is_srcinfo_like && !fields.contains_key("pkgbase") && !fields.contains_key("pkgname") {
        return None;
    }

    if !is_srcinfo_like
        && purl_arch.is_some()
        && !extra.contains_key("arch")
        && let Some(arch) = purl_arch
    {
        extra.insert("arch".to_string(), JsonValue::String(arch.to_string()));
    }

    (!extra.is_empty()).then_some(extra)
}

fn get_first(fields: &MultiMap, key: &str) -> Option<String> {
    fields.get(key).and_then(|values| values.first()).cloned()
}

fn get_all(fields: &MultiMap, key: &str) -> Vec<String> {
    fields.get(key).cloned().unwrap_or_default()
}

fn join_values(values: Option<&Vec<String>>) -> Option<String> {
    let values = values?;
    if values.is_empty() {
        None
    } else {
        Some(values.join(" AND "))
    }
}

fn should_force_array_extra_value(key: &str) -> bool {
    matches!(
        key,
        "provides"
            | "conflict"
            | "conflicts"
            | "replace"
            | "replaces"
            | "source"
            | "arch"
            | "license"
            | "groups"
            | "options"
            | "backup"
            | "validpgpkeys"
            | "md5sums"
            | "sha1sums"
            | "sha224sums"
            | "sha256sums"
            | "sha384sums"
            | "sha512sums"
            | "b2sums"
            | "cksums"
    ) || is_arch_variant_key(key)
}

crate::register_parser!(
    "Arch Linux package metadata (.SRCINFO, .AURINFO, .PKGINFO)",
    &["**/.SRCINFO", "**/.AURINFO", "**/.PKGINFO"],
    "alpm",
    "",
    Some("https://wiki.archlinux.org/title/.SRCINFO"),
);
