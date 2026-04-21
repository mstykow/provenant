// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::scan_and_assemble;

    #[test]
    fn test_cpan_manifest_scan_assigns_referenced_files() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let lib_dir = temp_dir.path().join("lib/Example");
        fs::create_dir_all(&lib_dir).expect("create lib dir");

        fs::write(
            temp_dir.path().join("Makefile.PL"),
            std::fs::read_to_string("testdata/cpan/makefile-pl/basic/Makefile.PL")
                .expect("read Makefile.PL fixture"),
        )
        .expect("write Makefile.PL");
        fs::write(
            temp_dir.path().join("MANIFEST"),
            "Makefile.PL\nlib/Example/WebToolkit.pm\nREADME\n",
        )
        .expect("write MANIFEST");
        fs::write(
            lib_dir.join("WebToolkit.pm"),
            "package Example::WebToolkit;\n1;\n",
        )
        .expect("write module file");
        fs::write(temp_dir.path().join("README"), "Example toolkit\n").expect("write README");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("Acme::Example"))
            .expect("cpan package should be assembled");
        let module_file = files
            .iter()
            .find(|file| file.path.ends_with("/lib/Example/WebToolkit.pm"))
            .expect("module file should be scanned");
        let readme = files
            .iter()
            .find(|file| file.path.ends_with("/README"))
            .expect("README should be scanned");

        assert!(module_file.for_packages.contains(&package.package_uid));
        assert!(readme.for_packages.contains(&package.package_uid));
    }
}
