#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::scan_and_assemble;
    use crate::models::DatasourceId;

    #[test]
    fn test_meson_scan_silently_falls_back_for_unsupported_multiline_strings() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let manifest_path = temp_dir.path().join("meson.build");
        fs::write(
            &manifest_path,
            r#"
project(
  'manual',
  version : files('.version'),
)

custom_target(
  'manual',
  command : [
    'bash',
    '''
      echo hello
    ''',
  ],
)
"#,
        )
        .expect("write meson.build");

        let (files, _result) = scan_and_assemble(temp_dir.path());

        let file = files
            .iter()
            .find(|file| file.path.ends_with("/meson.build"))
            .expect("meson.build should be scanned");

        assert!(file.scan_errors.is_empty(), "{:?}", file.scan_errors);
        assert!(
            file.package_data
                .iter()
                .any(|package| package.datasource_id == Some(DatasourceId::MesonBuild))
        );
    }
}
