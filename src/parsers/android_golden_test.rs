// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use base64::Engine;
    use prost::Message;
    use tempfile::TempDir;

    use crate::parsers::PackageParser;
    use crate::parsers::android::{
        AndroidAabParser, AndroidApkParser, AndroidManifestParser, AndroidSoongMetadataParser,
        ProtoItem, ProtoPrimitive, ProtoSourcePosition, ProtoXmlAttribute, ProtoXmlElement,
        ProtoXmlNamespace, ProtoXmlNode, proto_item, proto_primitive, proto_xml_node,
    };
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;

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

    fn create_binary_manifest_file() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("temp dir");
        let manifest_path = temp_dir.path().join("AndroidManifest.xml");
        fs::write(&manifest_path, decode_binary_manifest_fixture()).expect("write binary manifest");
        (temp_dir, manifest_path)
    }

    fn proto_manifest_fixture() -> Vec<u8> {
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
    fn test_golden_android_soong_metadata() {
        let package = AndroidSoongMetadataParser::extract_first_package(&PathBuf::from(
            "testdata/android/metadata/METADATA",
        ));

        compare_package_data_parser_only(
            &package,
            &PathBuf::from("testdata/android/golden/soong-metadata-expected.json"),
        )
        .unwrap_or_else(|error| panic!("Golden test failed: {}", error));
    }

    #[test]
    fn test_golden_android_manifest_text() {
        let package = AndroidManifestParser::extract_first_package(&PathBuf::from(
            "testdata/android/manifest/AndroidManifest.xml",
        ));

        compare_package_data_parser_only(
            &package,
            &PathBuf::from("testdata/android/golden/manifest-text-expected.json"),
        )
        .unwrap_or_else(|error| panic!("Golden test failed: {}", error));
    }

    #[test]
    fn test_golden_android_manifest_binary() {
        let (_temp_dir, manifest_path) = create_binary_manifest_file();
        let package = AndroidManifestParser::extract_first_package(&manifest_path);

        compare_package_data_parser_only(
            &package,
            &PathBuf::from("testdata/android/golden/manifest-binary-expected.json"),
        )
        .unwrap_or_else(|error| panic!("Golden test failed: {}", error));
    }

    #[test]
    fn test_golden_android_apk() {
        let (_temp_dir, apk_path) = create_zip(
            &[
                ("AndroidManifest.xml", &decode_binary_manifest_fixture()),
                ("classes.dex", b"dex"),
            ],
            "sample.apk",
        );
        let package = AndroidApkParser::extract_first_package(&apk_path);

        compare_package_data_parser_only(
            &package,
            &PathBuf::from("testdata/android/golden/app-apk-expected.json"),
        )
        .unwrap_or_else(|error| panic!("Golden test failed: {}", error));
    }

    #[test]
    fn test_golden_android_aab() {
        let (_temp_dir, aab_path) = create_zip(
            &[(
                "base/manifest/AndroidManifest.xml",
                &proto_manifest_fixture(),
            )],
            "sample.aab",
        );
        let package = AndroidAabParser::extract_first_package(&aab_path);

        compare_package_data_parser_only(
            &package,
            &PathBuf::from("testdata/android/golden/app-aab-expected.json"),
        )
        .unwrap_or_else(|error| panic!("Golden test failed: {}", error));
    }
}
