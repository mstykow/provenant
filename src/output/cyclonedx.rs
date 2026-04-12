use std::collections::HashSet;
use std::io::{self, Write};

use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::output_schema::{Output, OutputPackage as Package};
use crate::utils::time::{convert_header_timestamp_to_iso_utc, fallback_iso_utc_timestamp};

use super::shared::{io_other, xml_escape};

pub(crate) fn write_cyclonedx_json(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    let bom = build_cyclonedx_json(output);
    serde_json::to_writer_pretty(&mut *writer, &bom).map_err(io_other)?;
    writer.write_all(b"\n")
}

pub(crate) fn write_cyclonedx_xml(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    let serial = format!("urn:uuid:{}", Uuid::new_v4());
    let timestamp = output
        .headers
        .first()
        .and_then(|h| convert_header_timestamp_to_iso_utc(&h.end_timestamp))
        .unwrap_or_else(|| fallback_iso_utc_timestamp().to_string());

    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<bom xmlns=\"http://cyclonedx.org/schema/bom/1.3\" serialNumber=\"{}\" version=\"1\">\n",
        xml_escape(&serial)
    ));
    xml.push_str("  <metadata>\n");
    xml.push_str("    <timestamp>");
    xml.push_str(&xml_escape(&timestamp));
    xml.push_str("</timestamp>\n");
    xml.push_str("    <tools>\n");
    xml.push_str("      <tool><vendor>Provenant</vendor><name>Provenant</name><version>");
    xml.push_str(crate::version::BUILD_VERSION);
    xml.push_str("</version></tool>\n");
    xml.push_str("    </tools>\n");
    xml.push_str("  </metadata>\n");

    xml.push_str("  <components>\n");
    for (idx, pkg) in output.packages.iter().enumerate() {
        let name = pkg.name.as_deref().unwrap_or("unknown");
        let version = pkg.version.as_deref().unwrap_or("unknown");
        let bom_ref = cyclonedx_component_ref(pkg, idx);
        xml.push_str(&format!(
            "    <component type=\"library\" bom-ref=\"{}\">\n",
            xml_escape(&bom_ref)
        ));
        xml.push_str("      <name>");
        xml.push_str(&xml_escape(name));
        xml.push_str("</name>\n");
        xml.push_str("      <version>");
        xml.push_str(&xml_escape(version));
        xml.push_str("</version>\n");
        if let Some(description) = &pkg.description {
            xml.push_str("      <description>");
            xml.push_str(&xml_escape(description));
            xml.push_str("</description>\n");
        }
        if let Some(author) = package_author(pkg) {
            xml.push_str("      <author>");
            xml.push_str(&xml_escape(&author));
            xml.push_str("</author>\n");
        }
        xml.push_str("      <scope>required</scope>\n");
        if let Some(purl) = &pkg.purl {
            xml.push_str("      <purl>");
            xml.push_str(&xml_escape(purl));
            xml.push_str("</purl>\n");
        }
        let hashes = component_hashes(pkg);
        if !hashes.is_empty() {
            xml.push_str("      <hashes>\n");
            for (alg, content) in hashes {
                xml.push_str("        <hash alg=\"");
                xml.push_str(alg);
                xml.push_str("\">");
                xml.push_str(&xml_escape(&content));
                xml.push_str("</hash>\n");
            }
            xml.push_str("      </hashes>\n");
        }
        if let Some(license_expression) = cyclonedx_license_expression(pkg) {
            xml.push_str("      <licenses><expression>");
            xml.push_str(&xml_escape(&license_expression));
            xml.push_str("</expression></licenses>\n");
        }
        let external_refs = component_external_references(pkg);
        if !external_refs.is_empty() {
            xml.push_str("      <externalReferences>\n");
            for (ref_type, url) in external_refs {
                xml.push_str("        <reference type=\"");
                xml.push_str(ref_type);
                xml.push_str("\"><url>");
                xml.push_str(&xml_escape(&url));
                xml.push_str("</url></reference>\n");
            }
            xml.push_str("      </externalReferences>\n");
        }
        xml.push_str("    </component>\n");
    }
    xml.push_str("  </components>\n");

    if !output.dependencies.is_empty() {
        xml.push_str("  <dependencies>\n");
        for (idx, dep) in output.dependencies.iter().enumerate() {
            let dep_ref = dep
                .purl
                .clone()
                .unwrap_or_else(|| format!("dependency-{}", idx + 1));
            xml.push_str(&format!(
                "    <dependency ref=\"{}\">\n",
                xml_escape(&dep_ref)
            ));
            if let Some(resolved) = &dep.resolved_package
                && let Some(resolved_purl) = &resolved.purl
            {
                xml.push_str(&format!(
                    "      <dependency ref=\"{}\"/>\n",
                    xml_escape(resolved_purl)
                ));
            }
            xml.push_str("    </dependency>\n");
        }
        xml.push_str("  </dependencies>\n");
    }
    xml.push_str("</bom>\n");

    writer.write_all(xml.as_bytes())
}

fn build_cyclonedx_json(output: &Output) -> Value {
    let timestamp = output
        .headers
        .first()
        .and_then(|h| convert_header_timestamp_to_iso_utc(&h.end_timestamp))
        .unwrap_or_else(|| fallback_iso_utc_timestamp().to_string());

    let components = output
        .packages
        .iter()
        .enumerate()
        .map(|(idx, pkg)| {
            let mut obj = Map::new();
            obj.insert("type".to_string(), Value::String("library".to_string()));
            obj.insert(
                "bom-ref".to_string(),
                Value::String(cyclonedx_component_ref(pkg, idx)),
            );
            obj.insert(
                "name".to_string(),
                Value::String(pkg.name.clone().unwrap_or_else(|| "unknown".to_string())),
            );
            obj.insert(
                "version".to_string(),
                Value::String(pkg.version.clone().unwrap_or_else(|| "unknown".to_string())),
            );
            if let Some(description) = &pkg.description {
                obj.insert(
                    "description".to_string(),
                    Value::String(description.clone()),
                );
            }
            if let Some(author) = package_author(pkg) {
                obj.insert("author".to_string(), Value::String(author));
            }
            obj.insert("scope".to_string(), Value::String("required".to_string()));
            if let Some(purl) = &pkg.purl {
                obj.insert("purl".to_string(), Value::String(purl.clone()));
            }
            let hashes = component_hashes(pkg)
                .into_iter()
                .map(|(alg, content)| json!({"alg": alg, "content": content}))
                .collect::<Vec<_>>();
            if !hashes.is_empty() {
                obj.insert("hashes".to_string(), Value::Array(hashes));
            }
            if let Some(license_expression) = cyclonedx_license_expression(pkg) {
                obj.insert(
                    "licenses".to_string(),
                    Value::Array(vec![json!({ "expression": license_expression })]),
                );
            }
            let external_refs = component_external_references(pkg)
                .into_iter()
                .map(|(ref_type, url)| json!({"type": ref_type, "url": url}))
                .collect::<Vec<_>>();
            if !external_refs.is_empty() {
                obj.insert(
                    "externalReferences".to_string(),
                    Value::Array(external_refs),
                );
            }
            Value::Object(obj)
        })
        .collect::<Vec<_>>();

    let dependencies = output
        .dependencies
        .iter()
        .enumerate()
        .map(|(idx, dep)| {
            let reference = dep
                .purl
                .clone()
                .unwrap_or_else(|| format!("dependency-{}", idx + 1));
            let depends_on = dep
                .resolved_package
                .as_ref()
                .and_then(|rp| rp.purl.clone())
                .into_iter()
                .collect::<Vec<_>>();
            json!({
                "ref": reference,
                "dependsOn": depends_on,
            })
        })
        .collect::<Vec<_>>();

    if components.is_empty() && dependencies.is_empty() {
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.3",
            "version": 1,
            "components": [],
            "dependencies": [],
        })
    } else {
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.3",
            "serialNumber": format!("urn:uuid:{}", Uuid::new_v4()),
            "version": 1,
                "metadata": {
                    "timestamp": timestamp,
                    "tools": [
                        {
                            "name": "Provenant",
                            "version": crate::version::BUILD_VERSION
                        }
                    ]
                },
            "components": components,
            "dependencies": dependencies,
        })
    }
}

fn cyclonedx_component_ref(pkg: &Package, idx: usize) -> String {
    pkg.purl
        .clone()
        .unwrap_or_else(|| format!("component-{}", idx + 1))
}

fn cyclonedx_license_expression(pkg: &Package) -> Option<String> {
    pkg.declared_license_expression_spdx
        .clone()
        .or_else(|| pkg.declared_license_expression.clone())
        .or_else(|| {
            pkg.license_detections
                .first()
                .map(|d| d.license_expression_spdx.clone())
        })
}

fn package_author(pkg: &Package) -> Option<String> {
    pkg.parties.iter().find_map(|party| party.name.clone())
}

fn component_hashes(pkg: &Package) -> Vec<(&'static str, String)> {
    let mut hashes = Vec::new();
    if let Some(sha1) = &pkg.sha1 {
        hashes.push(("SHA-1", sha1.clone()));
    }
    if let Some(sha256) = &pkg.sha256 {
        hashes.push(("SHA-256", sha256.clone()));
    }
    if let Some(sha512) = &pkg.sha512 {
        hashes.push(("SHA-512", sha512.clone()));
    }
    if let Some(md5) = &pkg.md5 {
        hashes.push(("MD5", md5.clone()));
    }
    hashes
}

fn component_external_references(pkg: &Package) -> Vec<(&'static str, String)> {
    let mut refs = Vec::new();
    let mut seen = HashSet::new();
    let mut push_ref = |ref_type: &'static str, url: &Option<String>| {
        if let Some(url) = url
            && seen.insert((ref_type, url.clone()))
        {
            refs.push((ref_type, url.clone()));
        }
    };
    push_ref("bom", &pkg.api_data_url);
    push_ref("issue-tracker", &pkg.bug_tracking_url);
    push_ref("distribution", &pkg.download_url);
    push_ref("distribution", &pkg.repository_download_url);
    push_ref("website", &pkg.homepage_url);
    push_ref("website", &pkg.repository_homepage_url);
    push_ref("vcs", &pkg.vcs_url);
    refs
}
