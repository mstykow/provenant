// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use base64::Engine;
    use prost::Message;
    use tempfile::TempDir;

    use super::super::PackageParser;
    use super::super::android::{
        AndroidAabParser, AndroidApkParser, AndroidManifestParser, AndroidSoongMetadataParser,
        ProtoItem, ProtoPrimitive, ProtoSourcePosition, ProtoXmlAttribute, ProtoXmlElement,
        ProtoXmlNamespace, ProtoXmlNode, proto_item, proto_primitive, proto_xml_node,
    };
    use super::super::try_parse_file;
    use crate::models::{DatasourceId, PackageType};

    const ANDROID_NAMESPACE: &str = "http://schemas.android.com/apk/res/android";

    fn decode_binary_manifest_fixture() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(
                fs::read_to_string("testdata/android/manifest/AndroidManifest.binary.base64")
                    .expect("binary manifest base64 fixture should be readable")
                    .trim(),
            )
            .expect("binary manifest base64 fixture should decode")
    }

    fn create_temp_binary_manifest() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("temp dir");
        let manifest_path = temp_dir.path().join("AndroidManifest.xml");
        fs::write(&manifest_path, decode_binary_manifest_fixture()).expect("write binary manifest");
        (temp_dir, manifest_path)
    }

    fn create_zip(entries: &[(&str, &[u8])], filename: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("temp dir");
        let archive_path = temp_dir.path().join(filename);
        let file = fs::File::create(&archive_path).expect("create archive");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for (path, content) in entries {
            zip.start_file(*path, options).expect("start archive entry");
            zip.write_all(content).expect("write archive entry");
        }

        zip.finish().expect("finish archive");
        (temp_dir, archive_path)
    }

    fn create_proto_manifest_fixture() -> Vec<u8> {
        ProtoXmlNode {
            node: Some(proto_xml_node::Node::Element(ProtoXmlElement {
                namespace_declaration: vec![ProtoXmlNamespace {
                    prefix: "android".to_string(),
                    uri: ANDROID_NAMESPACE.to_string(),
                    source: Some(ProtoSourcePosition {
                        line_number: 1,
                        column_number: 1,
                    }),
                }],
                namespace_uri: String::new(),
                name: "manifest".to_string(),
                attribute: vec![
                    plain_attribute("package", "com.example.bundleapp"),
                    android_attribute("versionName", Some("2.5.1"), None),
                    android_attribute("versionCode", None, Some(251)),
                    android_attribute("compileSdkVersion", Some("35"), None),
                    android_attribute("compileSdkVersionCodename", Some("VanillaIceCream"), None),
                ],
                child: vec![
                    element_node(
                        "uses-sdk",
                        vec![
                            android_attribute("minSdkVersion", None, Some(24)),
                            android_attribute("targetSdkVersion", None, Some(35)),
                        ],
                        vec![],
                    ),
                    element_node(
                        "uses-permission",
                        vec![android_attribute(
                            "name",
                            Some("android.permission.INTERNET"),
                            None,
                        )],
                        vec![],
                    ),
                    element_node(
                        "application",
                        vec![android_attribute("label", Some("Bundle Example App"), None)],
                        vec![],
                    ),
                ],
            })),
            source: Some(ProtoSourcePosition {
                line_number: 1,
                column_number: 1,
            }),
        }
        .encode_to_vec()
    }

    fn plain_attribute(name: &str, value: &str) -> ProtoXmlAttribute {
        ProtoXmlAttribute {
            namespace_uri: String::new(),
            name: name.to_string(),
            value: value.to_string(),
            source: None,
            resource_id: 0,
            compiled_item: None,
        }
    }

    fn android_attribute(
        name: &str,
        value: Option<&str>,
        compiled_int: Option<i32>,
    ) -> ProtoXmlAttribute {
        ProtoXmlAttribute {
            namespace_uri: ANDROID_NAMESPACE.to_string(),
            name: name.to_string(),
            value: value.unwrap_or_default().to_string(),
            source: None,
            resource_id: 0,
            compiled_item: compiled_int.map(|compiled_int| ProtoItem {
                value: Some(proto_item::Value::Prim(ProtoPrimitive {
                    value: Some(proto_primitive::Value::IntDecimal(compiled_int)),
                })),
                flag_status: 0,
                flag_negated: false,
                flag_name: String::new(),
            }),
        }
    }

    fn element_node(
        name: &str,
        attributes: Vec<ProtoXmlAttribute>,
        child: Vec<ProtoXmlNode>,
    ) -> ProtoXmlNode {
        ProtoXmlNode {
            node: Some(proto_xml_node::Node::Element(ProtoXmlElement {
                namespace_declaration: vec![],
                namespace_uri: String::new(),
                name: name.to_string(),
                attribute: attributes,
                child,
            })),
            source: None,
        }
    }

    #[test]
    fn test_android_parser_is_match() {
        let (_apk_dir, apk_path) = create_zip(
            &[
                ("AndroidManifest.xml", &decode_binary_manifest_fixture()),
                ("classes.dex", b"dex"),
            ],
            "sample.apk",
        );
        let (_aab_dir, aab_path) = create_zip(
            &[(
                "base/manifest/AndroidManifest.xml",
                &create_proto_manifest_fixture(),
            )],
            "sample.aab",
        );

        assert!(AndroidSoongMetadataParser::is_match(&PathBuf::from(
            "vendor/fmt/METADATA"
        )));
        assert!(!AndroidSoongMetadataParser::is_match(&PathBuf::from(
            "site-packages/demo-1.0.0.dist-info/METADATA"
        )));
        assert!(AndroidManifestParser::is_match(&PathBuf::from(
            "app/src/main/AndroidManifest.xml"
        )));
        assert!(AndroidApkParser::is_match(&apk_path));
        assert!(AndroidAabParser::is_match(&aab_path));

        let broken_apk = apk_path.with_file_name("broken.apk");
        fs::write(&broken_apk, b"not a zip archive").expect("write broken apk");
        assert!(!AndroidApkParser::is_match(&broken_apk));
    }

    #[test]
    fn test_parse_soong_metadata_fixture() {
        let package = AndroidSoongMetadataParser::extract_first_package(&PathBuf::from(
            "testdata/android/metadata/METADATA",
        ));

        assert_eq!(package.package_type, Some(PackageType::Android));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::AndroidSoongMetadata)
        );
        assert_eq!(package.name.as_deref(), Some("libfmt"));
        assert_eq!(package.version.as_deref(), Some("11.0.1"));
        assert_eq!(
            package.description.as_deref(),
            Some("fmt formatting library used by Android builds")
        );
        assert_eq!(package.homepage_url.as_deref(), Some("https://fmt.dev"));
        assert_eq!(
            package.download_url.as_deref(),
            Some("https://github.com/fmtlib/fmt/archive/refs/tags/11.0.1.tar.gz")
        );
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://github.com/fmtlib/fmt.git")
        );
        assert_eq!(
            package.extracted_license_statement.as_deref(),
            Some("NOTICE, RESTRICTED_IF_STATICALLY_LINKED")
        );
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("last_upgrade_date"))
                .and_then(|value| value.as_str()),
            Some("2024-05-17")
        );
    }

    #[test]
    fn test_parse_soong_metadata_url_blocks() {
        let temp_dir = TempDir::new().expect("temp dir");
        let metadata_path = temp_dir.path().join("METADATA");
        fs::write(
            &metadata_path,
            r#"
name: "libexample"
third_party {
  version: "1.2.3"
  url {
    type: HOMEPAGE
    value: "https://example.test/home"
  }
  url {
    type: ARCHIVE
    value: "https://example.test/archive.tar.gz"
  }
  url {
    type: GIT
    value: "https://example.test/repo.git"
  }
}
"#,
        )
        .expect("write METADATA");

        let package = AndroidSoongMetadataParser::extract_first_package(&metadata_path);

        assert_eq!(package.name.as_deref(), Some("libexample"));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://example.test/home")
        );
        assert_eq!(
            package.download_url.as_deref(),
            Some("https://example.test/archive.tar.gz")
        );
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://example.test/repo.git")
        );
    }

    #[test]
    fn test_parse_soong_metadata_supports_colon_brace_maps() {
        let temp_dir = TempDir::new().expect("temp dir");
        let metadata_path = temp_dir.path().join("METADATA");
        fs::write(
            &metadata_path,
            r#"
name: "django"
description:
    "Django is a python-based web framework. As of 02/09/2011 this was listed "
    "as the latest official version."

third_party {
  url {
    type: ARCHIVE
    value: "http://www.djangoproject.com/download/1.2.5/tarball/"
  }
  version: "1.2.5"
  last_upgrade_date: {
    year: 2011
    month: 2
    day: 9
  }
  local_modifications: "http://google3/third_party/apphosting/python/django/v1_2_5_vendor/README.google?cl=19403388"
}
"#,
        )
        .expect("write METADATA");

        let package = AndroidSoongMetadataParser::extract_first_package(&metadata_path);

        assert_eq!(package.package_type, Some(PackageType::Android));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::AndroidSoongMetadata)
        );
        assert_eq!(package.name.as_deref(), Some("django"));
        assert_eq!(package.version.as_deref(), Some("1.2.5"));
        assert_eq!(
            package.download_url.as_deref(),
            Some("http://www.djangoproject.com/download/1.2.5/tarball/")
        );
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("last_upgrade_date"))
                .and_then(|value| value.as_str()),
            Some("2011-02-09")
        );
    }

    #[test]
    fn test_parse_soong_metadata_string_concatenation() {
        let temp_dir = TempDir::new().expect("temp dir");
        let metadata_path = temp_dir.path().join("METADATA");
        fs::write(
            &metadata_path,
            r#"
name: "nlohmann_json"
description: "SPM builds, making it impossible to use directly "
             "in those environments."
third_party {
  version: "3.11.2"
}
"#,
        )
        .expect("write METADATA");

        let package = AndroidSoongMetadataParser::extract_first_package(&metadata_path);

        assert_eq!(package.name.as_deref(), Some("nlohmann_json"));
        assert_eq!(package.version.as_deref(), Some("3.11.2"));
        assert_eq!(
            package.description.as_deref(),
            Some("SPM builds, making it impossible to use directly in those environments.")
        );
    }

    #[test]
    fn test_parse_text_android_manifest_fixture() {
        let package = AndroidManifestParser::extract_first_package(&PathBuf::from(
            "testdata/android/manifest/AndroidManifest.xml",
        ));

        assert_eq!(package.package_type, Some(PackageType::Android));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::AndroidManifestXml)
        );
        assert_eq!(package.name.as_deref(), Some("com.example.textapp"));
        assert_eq!(package.version.as_deref(), Some("4.2.0"));
        assert_eq!(package.description.as_deref(), Some("Text Example App"));
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("target_sdk_version"))
                .and_then(|value| value.as_str()),
            Some("35")
        );
    }

    #[test]
    fn test_parse_binary_android_manifest_fixture() {
        let (_temp_dir, manifest_path) = create_temp_binary_manifest();
        let package = AndroidManifestParser::extract_first_package(&manifest_path);

        assert_eq!(package.package_type, Some(PackageType::Android));
        assert_eq!(
            package.datasource_id,
            Some(DatasourceId::AndroidManifestXml)
        );
        assert_eq!(package.name.as_deref(), Some("eu.jgamba.myapplication"));
        assert_eq!(package.version.as_deref(), Some("1.0"));
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("version_code"))
                .and_then(|value| value.as_str()),
            Some("1")
        );
    }

    #[test]
    fn test_parse_apk_manifest_metadata_and_dispatch() {
        let (_temp_dir, apk_path) = create_zip(
            &[
                ("AndroidManifest.xml", &decode_binary_manifest_fixture()),
                ("classes.dex", b"dex"),
            ],
            "sample.apk",
        );
        let package = AndroidApkParser::extract_first_package(&apk_path);

        assert_eq!(package.package_type, Some(PackageType::Android));
        assert_eq!(package.datasource_id, Some(DatasourceId::AndroidApk));
        assert_eq!(package.name.as_deref(), Some("eu.jgamba.myapplication"));
        assert_eq!(package.version.as_deref(), Some("1.0"));

        let result =
            try_parse_file(&apk_path).expect("android apk should be claimed by parser dispatch");
        assert!(result.scan_errors.is_empty());
        assert_eq!(result.packages.len(), 1);
        assert_eq!(
            result.packages[0].datasource_id,
            Some(DatasourceId::AndroidApk)
        );
    }

    #[test]
    fn test_parse_aab_proto_manifest_metadata() {
        let (_temp_dir, aab_path) = create_zip(
            &[(
                "base/manifest/AndroidManifest.xml",
                &create_proto_manifest_fixture(),
            )],
            "sample.aab",
        );
        let package = AndroidAabParser::extract_first_package(&aab_path);

        assert_eq!(package.package_type, Some(PackageType::Android));
        assert_eq!(package.datasource_id, Some(DatasourceId::AndroidAab));
        assert_eq!(package.name.as_deref(), Some("com.example.bundleapp"));
        assert_eq!(package.version.as_deref(), Some("2.5.1"));
        assert_eq!(package.description.as_deref(), Some("Bundle Example App"));
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("version_code"))
                .and_then(|value| value.as_str()),
            Some("251")
        );
        assert_eq!(
            package
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("manifest_encoding"))
                .and_then(|value| value.as_str()),
            Some("proto")
        );
    }

    #[test]
    fn test_parse_invalid_manifest_falls_back_to_datasource() {
        let temp_dir = TempDir::new().expect("temp dir");
        let manifest_path = temp_dir.path().join("AndroidManifest.xml");
        fs::write(&manifest_path, b"not xml and not binary axml").expect("write broken manifest");

        let packages = AndroidManifestParser::extract_packages(&manifest_path);
        assert!(packages.is_empty());
    }

    #[test]
    fn test_invalid_text_manifest_reports_single_scan_error_without_package() {
        let temp_dir = TempDir::new().expect("temp dir");
        let manifest_path = temp_dir.path().join("AndroidManifest.xml");
        fs::write(&manifest_path, b"<manifest><!-- broken text manifest")
            .expect("write broken text manifest");

        let result = try_parse_file(&manifest_path).expect("android manifest should be claimed");

        assert!(result.packages.is_empty());
        assert_eq!(result.scan_errors.len(), 1);
        assert!(result.scan_errors[0].contains("Failed to parse AndroidManifest.xml as text XML"));
    }
}
