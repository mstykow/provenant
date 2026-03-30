#[cfg(all(test, feature = "golden-tests"))]
mod golden_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use crate::parsers::golden_test_utils::compare_package_data_collection_parser_only;
    use crate::parsers::try_parse_compiled_bytes;

    #[test]
    fn test_golden_rust_compiled_binary() {
        let test_file = PathBuf::from(
            "reference/scancode-toolkit/tests/packagedcode/data/cargo/binary/cargo_dependencies",
        );
        let expected_file =
            PathBuf::from("testdata/compiled-binary-golden/rust/cargo_dependencies.expected.json");

        if !test_file.exists() || !expected_file.exists() {
            return;
        }

        let bytes = fs::read(&test_file).expect("read rust compiled binary fixture");
        let parse_result = try_parse_compiled_bytes(&bytes).expect("compiled Rust binary packages");

        match compare_package_data_collection_parser_only(&parse_result.packages, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!(
                "Golden test failed for rust compiled binary: {error}\nactual={}",
                serde_json::to_string_pretty(&parse_result.packages).unwrap_or_default()
            ),
        }
    }

    #[test]
    fn test_golden_go_compiled_binary() {
        if Command::new("go").arg("version").output().is_err() {
            return;
        }

        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(
            temp.path().join("go.mod"),
            "module example.com/demo\n\ngo 1.23.0\n",
        )
        .expect("write go.mod");
        fs::write(
            temp.path().join("main.go"),
            "package main\nfunc main() {}\n",
        )
        .expect("write main.go");

        let binary = temp.path().join("demo");
        let status = Command::new("go")
            .current_dir(temp.path())
            .args(["build", "-o"])
            .arg(&binary)
            .status()
            .expect("run go build");
        assert!(status.success());

        let bytes = fs::read(&binary).expect("read go compiled binary fixture");
        let parse_result = try_parse_compiled_bytes(&bytes).expect("compiled Go binary packages");
        let expected_file =
            PathBuf::from("testdata/compiled-binary-golden/go-basic/demo.expected.json");

        match compare_package_data_collection_parser_only(&parse_result.packages, &expected_file) {
            Ok(_) => (),
            Err(error) => panic!(
                "Golden test failed for go compiled binary: {error}\nactual={}",
                serde_json::to_string_pretty(&parse_result.packages).unwrap_or_default()
            ),
        }
    }
}
