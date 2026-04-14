#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::scan_test_utils::{
        assert_dependency_present, assert_file_links_to_package, scan_and_assemble,
    };
    use crate::models::{DatasourceId, PackageType};

    #[test]
    fn test_ruby_extracted_scan_assembles_metadata_and_gemspec() {
        let (files, result) =
            scan_and_assemble(Path::new("testdata/assembly-golden/ruby-extracted-basic"));

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("example-gem"))
            .expect("ruby extracted gem should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Gem));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.purl.as_deref(), Some("pkg:gem/example-gem@1.2.3"));
        assert_dependency_present(&result.dependencies, "pkg:gem/rails", "metadata.gz-extract");
        assert_dependency_present(&result.dependencies, "pkg:gem/rubocop", "example.gemspec");
        assert_file_links_to_package(
            &files,
            "/metadata.gz-extract",
            &package.package_uid,
            DatasourceId::GemArchiveExtracted,
        );
        assert_file_links_to_package(
            &files,
            "/example.gemspec",
            &package.package_uid,
            DatasourceId::GemspecExtracted,
        );
    }

    #[test]
    fn test_ruby_manifest_lock_scan_assembles_gemspec_and_lockfile() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/ruby/basic.gemspec",
            temp_dir.path().join("example.gemspec"),
        )
        .expect("copy gemspec fixture");
        fs::copy(
            "testdata/ruby/Gemfile.lock",
            temp_dir.path().join("Gemfile.lock"),
        )
        .expect("copy Gemfile.lock fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("example-gem"))
            .expect("ruby manifest package should be assembled");

        assert_eq!(package.package_type, Some(PackageType::Gem));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.purl.as_deref(), Some("pkg:gem/example-gem@1.2.3"));
        assert_dependency_present(&result.dependencies, "pkg:gem/rails", "example.gemspec");
        assert_dependency_present(&result.dependencies, "pkg:gem/rspec@3.12.0", "Gemfile.lock");
        assert_file_links_to_package(
            &files,
            "/example.gemspec",
            &package.package_uid,
            DatasourceId::Gemspec,
        );
        assert_file_links_to_package(
            &files,
            "/Gemfile.lock",
            &package.package_uid,
            DatasourceId::GemfileLock,
        );
    }

    #[test]
    fn test_ruby_gem_archive_scan_assembles_standalone_package() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/ruby/example-gem-1.2.3.gem",
            temp_dir.path().join("example-gem-1.2.3.gem"),
        )
        .expect("copy gem archive fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("example-gem"))
            .expect("standalone ruby gem archive should assemble into a package");

        assert_eq!(package.package_type, Some(PackageType::Gem));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.purl.as_deref(), Some("pkg:gem/example-gem@1.2.3"));
        assert_dependency_present(
            &result.dependencies,
            "pkg:gem/rails",
            "example-gem-1.2.3.gem",
        );
        assert_file_links_to_package(
            &files,
            "/example-gem-1.2.3.gem",
            &package.package_uid,
            DatasourceId::GemArchive,
        );
    }

    #[test]
    fn test_ruby_gem_archive_does_not_claim_unrelated_siblings() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        fs::copy(
            "testdata/ruby/example-gem-1.2.3.gem",
            temp_dir.path().join("example-gem-1.2.3.gem"),
        )
        .expect("copy gem archive fixture");
        fs::write(temp_dir.path().join("README.md"), "unrelated sibling")
            .expect("write unrelated file");

        let (files, result) = scan_and_assemble(temp_dir.path());

        let package = result
            .packages
            .iter()
            .find(|package| package.name.as_deref() == Some("example-gem"))
            .expect("standalone ruby gem archive should assemble into a package");

        assert_file_links_to_package(
            &files,
            "/example-gem-1.2.3.gem",
            &package.package_uid,
            DatasourceId::GemArchive,
        );

        let readme = files
            .iter()
            .find(|file| file.path.ends_with("README.md"))
            .expect("README should be present in scan results");
        assert!(
            readme.for_packages.is_empty(),
            "standalone gem archive should not claim unrelated sibling files"
        );
    }
}
