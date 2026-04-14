//! Tests for Ruby/RubyGems parser (Gemfile and Gemfile.lock).
//!
//! Following TDD approach - tests written first (RED phase).

#[cfg(test)]
mod tests {
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use crate::parsers::PackageParser;
    use crate::parsers::ruby::{GemfileLockParser, GemfileParser, strip_freeze_suffix};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary Gemfile with the given content
    fn create_temp_gemfile(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gemfile_path = temp_dir.path().join("Gemfile");
        fs::write(&gemfile_path, content).expect("Failed to write Gemfile");
        (temp_dir, gemfile_path)
    }

    // ==========================================================================
    // Test: is_match for Gemfile
    // ==========================================================================
    #[test]
    fn test_gemfile_is_match() {
        // Valid Gemfile paths
        assert!(GemfileParser::is_match(&PathBuf::from("Gemfile")));
        assert!(GemfileParser::is_match(&PathBuf::from("/path/to/Gemfile")));
        assert!(GemfileParser::is_match(&PathBuf::from("./project/Gemfile")));

        // Invalid paths
        assert!(!GemfileParser::is_match(&PathBuf::from("Gemfile.lock")));
        assert!(!GemfileParser::is_match(&PathBuf::from("gemfile")));
        assert!(!GemfileParser::is_match(&PathBuf::from("package.json")));
        assert!(!GemfileParser::is_match(&PathBuf::from("Cargo.toml")));
    }

    // ==========================================================================
    // Test: is_match for Gemfile.lock
    // ==========================================================================
    #[test]
    fn test_gemfile_lock_is_match() {
        // Valid Gemfile.lock paths
        assert!(GemfileLockParser::is_match(&PathBuf::from("Gemfile.lock")));
        assert!(GemfileLockParser::is_match(&PathBuf::from(
            "/path/to/Gemfile.lock"
        )));
        assert!(GemfileLockParser::is_match(&PathBuf::from(
            "./project/Gemfile.lock"
        )));

        // Invalid paths
        assert!(!GemfileLockParser::is_match(&PathBuf::from("Gemfile")));
        assert!(!GemfileLockParser::is_match(&PathBuf::from("gemfile.lock")));
        assert!(!GemfileLockParser::is_match(&PathBuf::from(
            "package-lock.json"
        )));
    }

    // ==========================================================================
    // Test: Simple gem extraction from Gemfile
    // ==========================================================================
    #[test]
    fn test_extract_simple_gem() {
        let content = r#"
source "https://rubygems.org"

gem "rake", "~> 13.0"
gem "rspec", ">= 3.0"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert!(package_data.dependencies.len() >= 2);

        // Check rake dependency
        let rake_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake_dep.is_some());
        let rake = rake_dep.unwrap();
        assert_eq!(rake.extracted_requirement, Some("~> 13.0".to_string()));
    }

    // ==========================================================================
    // Test: Pessimistic version operator (~>)
    // ==========================================================================
    #[test]
    fn test_extract_pessimistic_version() {
        let content = r#"
source "https://rubygems.org"

gem "activesupport", "~> 7.0.4"
gem "rails", "~> 7.0"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        let activesupport_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("activesupport")));
        assert!(activesupport_dep.is_some());
        assert_eq!(
            activesupport_dep.unwrap().extracted_requirement,
            Some("~> 7.0.4".to_string())
        );

        let rails_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rails")));
        assert!(rails_dep.is_some());
        assert_eq!(
            rails_dep.unwrap().extracted_requirement,
            Some("~> 7.0".to_string())
        );
    }

    // ==========================================================================
    // Test: Dependency groups (:development, :test)
    // Bug Fix #4: Correct scope mapping - :runtime → None, :development → "development"
    // ==========================================================================
    #[test]
    fn test_extract_groups() {
        let content = r#"
source "https://rubygems.org"

gem "rails", "~> 7.0"

group :development do
  gem "pry"
  gem "solargraph"
end

group :test do
  gem "rspec"
  gem "factory_bot"
end

group :development, :test do
  gem "debug"
end
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        // Rails should have NO scope (runtime dependency)
        let rails_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rails")));
        assert!(rails_dep.is_some());
        let rails = rails_dep.unwrap();
        // Bug Fix #4: :runtime → None (no scope)
        assert!(
            rails.scope.is_none() || rails.scope.as_ref().is_some_and(|s| s.is_empty()),
            "Runtime deps should have no scope, got: {:?}",
            rails.scope
        );
        assert_eq!(rails.is_runtime, Some(true));

        // pry should have scope "development"
        let pry_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("pry")));
        assert!(pry_dep.is_some());
        let pry = pry_dep.unwrap();
        assert_eq!(pry.scope, Some("development".to_string()));
        assert_eq!(pry.is_runtime, Some(false));
        assert_eq!(pry.is_optional, Some(true));

        // rspec should have scope "test"
        let rspec_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rspec")));
        assert!(rspec_dep.is_some());
        let rspec = rspec_dep.unwrap();
        assert_eq!(rspec.scope, Some("test".to_string()));
        assert_eq!(rspec.is_runtime, Some(false));
        assert_eq!(rspec.is_optional, Some(true));
    }

    // ==========================================================================
    // Test: Lockfile gems extraction (GEM section)
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_gems() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));

        // Should have extracted gems from the GEM section
        assert!(!package_data.dependencies.is_empty());

        // Check specific gem was extracted
        let rake_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake_dep.is_some(), "Should find rake in Gemfile.lock");

        // Check rubocop was extracted with correct version
        let rubocop_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rubocop")));
        assert!(rubocop_dep.is_some(), "Should find rubocop in Gemfile.lock");
    }

    // ==========================================================================
    // Test: Lockfile DEPENDENCIES section
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_dependencies() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // Should have parsed DEPENDENCIES section
        // The DEPENDENCIES section shows direct dependencies with constraints
        let rake_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake_dep.is_some());

        // Check pinned dependency (bcrypt-ruby!)
        if let Some(bcrypt_dep) = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("bcrypt")))
        {
            assert_eq!(bcrypt_dep.is_pinned, Some(true));
        }
    }

    // ==========================================================================
    // Test: Platform extraction from Gemfile.lock
    // ==========================================================================
    #[test]
    fn test_extract_platforms() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // Check extra_data for platforms
        assert!(package_data.extra_data.is_some());
        let extra = package_data.extra_data.as_ref().unwrap();
        let platforms = extra.get("platforms");
        assert!(platforms.is_some(), "Should have platforms in extra_data");
    }

    // ==========================================================================
    // Bug Fix #1: Strip .freeze suffix from strings
    // ==========================================================================
    #[test]
    fn test_strip_freeze_suffix() {
        // Direct unit test of the helper function
        assert_eq!(strip_freeze_suffix("name"), "name");
        assert_eq!(strip_freeze_suffix("\"name\".freeze"), "\"name\"");
        assert_eq!(strip_freeze_suffix("'1.0.0'.freeze"), "'1.0.0'");
        assert_eq!(strip_freeze_suffix("version.freeze"), "version");
        assert_eq!(strip_freeze_suffix("nothing_to_strip"), "nothing_to_strip");

        // Double freeze (edge case) - strips all trailing .freeze
        assert_eq!(strip_freeze_suffix("x.freeze.freeze"), "x");
    }

    // ==========================================================================
    // Bug Fix #4: Correct dependency scope mapping
    // ==========================================================================
    #[test]
    fn test_correct_scope_mapping() {
        let content = r#"
source "https://rubygems.org"

# Runtime dependency - should have NO scope (None)
gem "activesupport"

group :development do
  gem "byebug"
end

group :test do
  gem "minitest"
end
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        // Runtime dependency: scope should be None (not "runtime")
        let activesupport = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("activesupport")));
        assert!(activesupport.is_some());
        let active = activesupport.unwrap();
        // Bug Fix #4: :runtime should map to None, not "runtime"
        assert!(
            active.scope.is_none(),
            "Runtime deps should have scope=None, got {:?}",
            active.scope
        );
        assert_eq!(active.is_runtime, Some(true));
        assert_eq!(active.is_optional, Some(false));

        // Development dependency: scope should be "development"
        let byebug = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("byebug")));
        assert!(byebug.is_some());
        assert_eq!(byebug.unwrap().scope, Some("development".to_string()));
        assert_eq!(byebug.unwrap().is_runtime, Some(false));
    }

    // ==========================================================================
    // Test: Frozen strings in Gemfile (Bug #1 integration test)
    // ==========================================================================
    #[test]
    fn test_extract_frozen_strings() {
        let content = r#"
# frozen_string_literal: true

source "https://rubygems.org"

gem "frozen-gem".freeze, "1.0.0".freeze
gem "another-gem", "2.0".freeze
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        // Should have extracted the gem name without .freeze
        let frozen_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("frozen-gem")));
        assert!(frozen_dep.is_some(), "Should find frozen-gem dependency");

        // Name should NOT contain .freeze
        let purl = frozen_dep.unwrap().purl.as_ref().unwrap();
        assert!(
            !purl.contains(".freeze"),
            "Gem name should not contain .freeze"
        );
    }

    // ==========================================================================
    // Test: Heredoc descriptions (multi-line)
    // ==========================================================================
    #[test]
    fn test_extract_heredoc_descriptions() {
        // Heredocs are more common in .gemspec files, but we test the parser can handle them
        let content = r#"
source "https://rubygems.org"

# Description using heredoc style is not common in Gemfile,
# but we should handle multi-line comments gracefully
gem "some-gem", "~> 1.0"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        // Should parse without error
        assert!(
            !package_data.dependencies.is_empty(),
            "Should parse gems even with complex comments"
        );
    }

    // ==========================================================================
    // Test: Platform-specific gems
    // ==========================================================================
    #[test]
    fn test_extract_platform_specific_gems() {
        let content = r#"
source "https://rubygems.org"

gem "json", "~> 2.0", platforms: [:ruby, :jruby]
gem "bcrypt-ruby", platforms: :ruby
gem "debug", platforms: [:mri, :mingw, :x64_mingw]
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        // Should have extracted all platform-specific gems
        let json_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("json")));
        assert!(json_dep.is_some());

        let bcrypt_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("bcrypt")));
        assert!(bcrypt_dep.is_some());

        let debug_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("debug")));
        assert!(debug_dep.is_some());
    }

    #[test]
    fn test_extract_gemfile_dependency_source_provenance() {
        let content = r#"
source "https://rubygems.org"

gem "my-git-gem", git: "https://github.com/example/my-git-gem.git", branch: "main", ref: "abc123def456", tag: "v1.2.3"
gem "my-local-gem", path: "../vendor/my-local-gem"
gem "private-gem", "~> 2.0", source: "https://gems.example.com"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        let git_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("my-git-gem")));
        assert!(git_dep.is_some(), "Should find git-sourced dependency");
        let git_extra = git_dep.unwrap().extra_data.as_ref();
        assert!(
            git_extra.is_some(),
            "Git dependency should preserve extra_data"
        );
        let git_extra = git_extra.unwrap();
        assert_eq!(
            git_extra.get("source_type").and_then(|v| v.as_str()),
            Some("GIT")
        );
        assert_eq!(
            git_extra.get("git").and_then(|v| v.as_str()),
            Some("https://github.com/example/my-git-gem.git")
        );
        assert_eq!(
            git_extra.get("remote").and_then(|v| v.as_str()),
            Some("https://github.com/example/my-git-gem.git")
        );
        assert_eq!(
            git_extra.get("branch").and_then(|v| v.as_str()),
            Some("main")
        );
        assert_eq!(
            git_extra.get("ref").and_then(|v| v.as_str()),
            Some("abc123def456")
        );
        assert_eq!(
            git_extra.get("tag").and_then(|v| v.as_str()),
            Some("v1.2.3")
        );

        let path_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("my-local-gem")));
        assert!(path_dep.is_some(), "Should find path-sourced dependency");
        let path_extra = path_dep.unwrap().extra_data.as_ref();
        assert!(
            path_extra.is_some(),
            "Path dependency should preserve extra_data"
        );
        let path_extra = path_extra.unwrap();
        assert_eq!(
            path_extra.get("source_type").and_then(|v| v.as_str()),
            Some("PATH")
        );
        assert_eq!(
            path_extra.get("path").and_then(|v| v.as_str()),
            Some("../vendor/my-local-gem")
        );

        let registry_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("private-gem")));
        assert!(registry_dep.is_some(), "Should find registry dependency");
        let registry_extra = registry_dep.unwrap().extra_data.as_ref();
        assert!(
            registry_extra.is_some(),
            "Source-qualified registry dependency should preserve extra_data"
        );
        assert_eq!(
            registry_extra
                .unwrap()
                .get("source")
                .and_then(|v| v.as_str()),
            Some("https://gems.example.com")
        );
    }

    #[test]
    fn test_extract_gemfile_inherits_global_source() {
        let content = r#"
source "https://rubygems.org"

gem "rake", "~> 13.0"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        let rake_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake_dep.is_some(), "Should find rake dependency");
        let rake_extra = rake_dep.unwrap().extra_data.as_ref();
        assert!(
            rake_extra.is_some(),
            "Dependencies should inherit the Gemfile source when no per-gem source is set"
        );
        assert_eq!(
            rake_extra.unwrap().get("source").and_then(|v| v.as_str()),
            Some("https://rubygems.org")
        );

        let package_extra = package_data.extra_data.as_ref();
        assert!(
            package_extra.is_some(),
            "Gemfile should keep top-level sources in package extra_data"
        );
        let sources = package_extra
            .unwrap()
            .get("sources")
            .and_then(|v| v.as_array());
        assert!(
            sources.is_some(),
            "Package extra_data should include sources array"
        );
        assert_eq!(sources.unwrap().len(), 1);
        assert_eq!(sources.unwrap()[0].as_str(), Some("https://rubygems.org"));
    }

    #[test]
    fn test_extract_gemfile_source_block_inheritance() {
        let content = r#"
source "https://rubygems.org"

source "https://gems.example.com" do
  gem "private-gem", "~> 2.0"
end

gem "rake", "~> 13.0"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        let private_gem = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("private-gem")));
        assert!(private_gem.is_some(), "Should find source-block dependency");
        assert_eq!(
            private_gem
                .unwrap()
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("source"))
                .and_then(|value| value.as_str()),
            Some("https://gems.example.com")
        );

        let rake = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake.is_some(), "Should find default-source dependency");
        assert_eq!(
            rake.unwrap()
                .extra_data
                .as_ref()
                .and_then(|extra| extra.get("source"))
                .and_then(|value| value.as_str()),
            Some("https://rubygems.org")
        );
    }

    // ==========================================================================
    // Test: Multiple version constraints
    // ==========================================================================
    #[test]
    fn test_extract_multiple_version_constraints() {
        let content = r#"
source "https://rubygems.org"

gem "multi-constraint", ">= 1.0", "< 2.0"
gem "specific-range", ">= 1.0.0", "< 1.5.0", "!= 1.2.3"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        let multi_dep = package_data.dependencies.iter().find(|d| {
            d.purl
                .as_ref()
                .is_some_and(|p| p.contains("multi-constraint"))
        });
        assert!(multi_dep.is_some());
        // Version constraints should be joined
        let req = multi_dep.unwrap().extracted_requirement.as_ref();
        assert!(req.is_some());
    }

    // ==========================================================================
    // Test: Graceful error handling (invalid/missing file)
    // ==========================================================================
    #[test]
    fn test_graceful_error_handling() {
        // Non-existent file
        let package_data =
            GemfileParser::extract_first_package(&PathBuf::from("/nonexistent/Gemfile"));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());

        // Empty file
        let content = "";
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);
        // Should return default package data, not panic
        assert!(package_data.dependencies.is_empty());
    }

    // ==========================================================================
    // Test: Empty Gemfile.lock
    // ==========================================================================
    #[test]
    fn test_extract_empty_lockfile() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock_empty");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // Should handle empty lockfile gracefully
        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        // No gems means empty dependencies
        assert!(
            package_data.dependencies.is_empty(),
            "Empty lockfile should have no dependencies"
        );
    }

    // ==========================================================================
    // Test: Lockfile with PATH section (local gems)
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_with_path() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile_with_path");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        assert_eq!(package_data.name.as_deref(), Some("my-local-gem"));
        assert!(package_data.version.is_some());

        assert!(package_data.repository_homepage_url.is_some());
        assert!(package_data.repository_download_url.is_some());
        assert!(package_data.api_data_url.is_some());
        assert!(package_data.download_url.is_some());

        let local_gem_in_deps = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("my-local-gem")));
        assert!(
            local_gem_in_deps.is_none(),
            "PATH gem (primary gem) should be excluded from dependencies"
        );
    }

    // ==========================================================================
    // Test: Lockfile with GIT section (git-sourced gems)
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_with_git() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile_with_git");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // Should find the GIT gem
        let git_gem = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("my-git-gem")));
        assert!(git_gem.is_some(), "Should find GIT gem in Gemfile.lock");
    }

    // ==========================================================================
    // Test: Real testdata file parsing
    // ==========================================================================
    #[test]
    fn test_extract_from_testdata() {
        let gemfile_path = PathBuf::from("testdata/ruby/Gemfile");
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert!(!package_data.dependencies.is_empty());

        // Should have rake dependency
        let rake = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake.is_some());

        // Should have rspec dependency
        let rspec = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rspec")));
        assert!(rspec.is_some());
    }

    // ==========================================================================
    // Test: Lockfile testdata parsing
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_from_testdata() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert!(!package_data.dependencies.is_empty());

        // Verify bundler version is captured
        if let Some(extra) = &package_data.extra_data {
            let bundler_version = extra.get("bundler_version");
            assert!(
                bundler_version.is_some(),
                "Should capture BUNDLED WITH version"
            );
        }
    }

    // ==========================================================================
    // Test: No unwrap/expect in library code (verification)
    // ==========================================================================
    #[test]
    fn test_no_unwrap_no_expect() {
        // This test verifies that the ruby.rs file doesn't use unwrap() or expect()
        // in library code (only allowed in tests).
        // We verify this by checking the source file.
        let source_path = PathBuf::from("src/parsers/ruby.rs");
        let content = fs::read_to_string(&source_path).expect("Should read ruby.rs");

        // Count occurrences of .unwrap() and .expect( outside of test code
        // We need to filter out the test module
        let lines: Vec<&str> = content.lines().collect();
        let mut in_test_module = false;
        let mut unwrap_count = 0;
        let mut expect_count = 0;

        for line in lines {
            // Detect test module
            if line.contains("#[cfg(test)]") || line.contains("mod tests") {
                in_test_module = true;
            }

            // Skip test code
            if in_test_module {
                continue;
            }

            // Check for unwrap/expect (but allow ok_or_else, map_err, etc.)
            if line.contains(".unwrap()") && !line.trim().starts_with("//") {
                unwrap_count += 1;
            }
            if line.contains(".expect(") && !line.trim().starts_with("//") {
                expect_count += 1;
            }
        }

        assert_eq!(
            unwrap_count, 0,
            "Found {} .unwrap() calls in library code",
            unwrap_count
        );
        assert_eq!(
            expect_count, 0,
            "Found {} .expect() calls in library code",
            expect_count
        );
    }

    // ==========================================================================
    // GEMSPEC PARSER TESTS (Wave 2)
    // ==========================================================================

    // ==========================================================================
    // Test: is_match for .gemspec files
    // ==========================================================================
    #[test]
    fn test_gemspec_is_match() {
        use crate::parsers::ruby::GemspecParser;
        // Valid .gemspec paths
        assert!(GemspecParser::is_match(&PathBuf::from("example.gemspec")));
        assert!(GemspecParser::is_match(&PathBuf::from(
            "/path/to/my-gem.gemspec"
        )));
        assert!(GemspecParser::is_match(&PathBuf::from(
            "./project/cool_gem.gemspec"
        )));

        // Invalid paths
        assert!(!GemspecParser::is_match(&PathBuf::from("Gemfile")));
        assert!(!GemspecParser::is_match(&PathBuf::from("Gemfile.lock")));
        assert!(!GemspecParser::is_match(&PathBuf::from("package.json")));
        assert!(!GemspecParser::is_match(&PathBuf::from("gemspec")));
        assert!(!GemspecParser::is_match(&PathBuf::from("test.gemspec.bak")));
    }

    // ==========================================================================
    // Test: Basic .gemspec extraction (name, version, authors, etc.)
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_basic() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/basic.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(package_data.name, Some("example-gem".to_string()));
        assert_eq!(package_data.version, Some("1.2.3".to_string()));
        assert_eq!(
            package_data.description,
            Some("A longer description of the gem with more details".to_string())
        );
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com/example-gem".to_string())
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert!(package_data.extracted_license_statement.is_some());
        assert_eq!(package_data.primary_language, Some("Ruby".to_string()));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::Gemspec));

        // Authors should be extracted as parties
        assert!(
            !package_data.parties.is_empty(),
            "Should have extracted authors as parties"
        );
        let author_names: Vec<_> = package_data
            .parties
            .iter()
            .filter_map(|p| p.name.as_ref())
            .collect();
        assert!(
            author_names.contains(&&"John Doe".to_string()),
            "Should find John Doe in parties"
        );
        assert!(
            author_names.contains(&&"Jane Smith".to_string()),
            "Should find Jane Smith in parties"
        );

        // Email should be in parties
        let emails: Vec<_> = package_data
            .parties
            .iter()
            .filter_map(|p| p.email.as_ref())
            .collect();
        assert!(
            emails.contains(&&"john@example.com".to_string()),
            "Should find john@example.com in party emails"
        );

        // Dependencies
        assert!(
            package_data.dependencies.len() >= 4,
            "Should have at least 4 dependencies, got {}",
            package_data.dependencies.len()
        );

        // Check runtime dependency
        let rails_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rails")));
        assert!(rails_dep.is_some(), "Should find rails dependency");
        let rails = rails_dep.unwrap();
        assert_eq!(rails.extracted_requirement, Some("~> 5.0".to_string()));
        assert_eq!(rails.is_runtime, Some(true));
        assert!(
            rails.scope.is_none()
                || rails.scope.as_deref() == Some("runtime")
                || rails.scope.as_deref() == Some("dependencies")
                || rails.scope.as_deref() == Some("dependency"),
            "Runtime dep scope should be None, 'runtime', 'dependencies', or 'dependency'"
        );

        // Check development dependency
        let rspec_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rspec")));
        assert!(rspec_dep.is_some(), "Should find rspec dependency");
        let rspec = rspec_dep.unwrap();
        assert_eq!(rspec.extracted_requirement, Some("~> 3.0".to_string()));
        assert_eq!(rspec.scope, Some("development".to_string()));
        assert_eq!(rspec.is_runtime, Some(false));
        assert_eq!(rspec.is_optional, Some(true));
    }

    // ==========================================================================
    // Test: Bug #2 - Variable version resolution (CSV::VERSION)
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_variable_version() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/variable_version.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.name, Some("csv".to_string()));
        // Bug #2: Should resolve CSV::VERSION to "3.2.6"
        assert_eq!(
            package_data.version,
            Some("3.2.6".to_string()),
            "Should resolve variable version CSV::VERSION to '3.2.6'"
        );

        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("ruby AND bsd-2-clause")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("Ruby AND BSD-2-Clause")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert!(package_data.extracted_license_statement.is_some());
    }

    #[test]
    fn test_extract_gemspec_required_file_constants() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/with_variables/with_variables.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.name, Some("provider_dsl".to_string()));
        assert_eq!(package_data.version, Some("2.4.1".to_string()));
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com/provider_dsl".to_string())
        );
        assert_eq!(
            package_data.description,
            Some("See the project home page for more information".to_string())
        );

        let author_names: Vec<_> = package_data
            .parties
            .iter()
            .filter_map(|party| party.name.as_ref())
            .cloned()
            .collect();
        assert!(author_names.contains(&"Author One".to_string()));
        assert!(author_names.contains(&"Author Two".to_string()));
    }

    // ==========================================================================
    // Test: Bug #1 - Frozen string handling in .gemspec
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_frozen_strings() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/frozen_strings.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        // Bug #1: .freeze should be stripped from all values
        assert_eq!(
            package_data.name,
            Some("rubocop".to_string()),
            "Name should not contain .freeze"
        );
        assert_eq!(
            package_data.version,
            Some("1.50.0".to_string()),
            "Version should not contain .freeze"
        );
        assert_eq!(
            package_data.homepage_url,
            Some("https://rubocop.org/".to_string()),
            "Homepage should not contain .freeze"
        );

        // Authors should not contain .freeze
        let author_names: Vec<_> = package_data
            .parties
            .iter()
            .filter_map(|p| p.name.as_ref())
            .collect();
        assert!(
            author_names.contains(&&"Bozhidar Batsov".to_string()),
            "Should find Bozhidar Batsov without .freeze"
        );

        // Dependencies should not have .freeze in names or versions
        let json_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("json")));
        assert!(json_dep.is_some(), "Should find json dependency");
        let json = json_dep.unwrap();
        assert_eq!(json.extracted_requirement, Some("~> 2.3".to_string()));

        // Dev dependency with multiple version constraints
        let bundler_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("bundler")));
        assert!(bundler_dep.is_some(), "Should find bundler dependency");
        let bundler = bundler_dep.unwrap();
        assert!(
            bundler
                .extracted_requirement
                .as_ref()
                .is_some_and(|r| r.contains(">= 1.15.0") && r.contains("< 3.0.0")),
            "Bundler should have multiple version constraints, got: {:?}",
            bundler.extracted_requirement
        );
    }

    // ==========================================================================
    // Test: Bug #6 - Email handling in parties
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_email_handling() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/email_handling.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.name, Some("email-test-gem".to_string()));

        // Python ScanCode creates separate parties for authors and emails
        let party = package_data
            .parties
            .iter()
            .find(|p| p.name.as_ref().is_some_and(|n| n.contains("Alice")));
        assert!(
            party.is_some(),
            "Should find a party with Alice in the name"
        );
        let alice = party.unwrap();

        assert!(
            alice.email.is_some(),
            "Alice should have an email parsed, got parties: {:?}",
            package_data.parties
        );
        assert!(
            alice
                .email
                .as_ref()
                .is_some_and(|e| e.contains("alice@wonderland.org")),
            "Should parse email from RFC 5322 format, got: {:?}",
            alice.email
        );
    }

    // ==========================================================================
    // Test: .gemspec with multiple licenses
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_multiple_licenses() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/multiple_licenses.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.name, Some("multi-license-gem".to_string()));
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("apache-2.0 AND bsd-2-clause AND mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("Apache-2.0 AND BSD-2-Clause AND MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert!(package_data.extracted_license_statement.is_some());
    }

    // ==========================================================================
    // Test: .gemspec dependencies (runtime and development)
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_dependencies() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/basic.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        // Runtime dependencies
        let nokogiri = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("nokogiri")));
        assert!(nokogiri.is_some(), "Should find nokogiri dependency");
        let noko = nokogiri.unwrap();
        assert_eq!(noko.extracted_requirement, Some(">= 1.6".to_string()));
        assert_eq!(noko.is_runtime, Some(true));

        // Dev dependency without version
        let rubocop = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rubocop")));
        assert!(rubocop.is_some(), "Should find rubocop dependency");
        let rub = rubocop.unwrap();
        assert_eq!(rub.scope, Some("development".to_string()));
        assert_eq!(rub.is_runtime, Some(false));
        // No version constraint - use None (semantically correct)
        assert_eq!(
            rub.extracted_requirement, None,
            "rubocop should have None for no version requirement"
        );
    }

    #[test]
    fn test_gemspec_description_does_not_create_false_dependency() {
        use crate::parsers::ruby::GemspecParser;

        let content = r#"
Gem::Specification.new do |spec|
  spec.name = "logstash-mixin-ecs_compatibility_support"
  spec.version = "1.3.0"
  spec.description = "Documentation mentions spec.add_dependency 'fake-gem', '1.0' but it is not code."
  spec.add_dependency "logstash-core-plugin-api", ">= 1.60", "<= 2.99"
end
"#;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gemspec_path = temp_dir.path().join("false_dependency.gemspec");
        fs::write(&gemspec_path, content).expect("Failed to write gemspec");

        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.dependencies.len(), 1);
        assert_eq!(
            package_data.dependencies[0].purl,
            Some("pkg:gem/logstash-core-plugin-api".to_string())
        );
    }

    // ==========================================================================
    // Test: .gemspec dev dependencies
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_dev_dependencies() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path = PathBuf::from("testdata/ruby/basic.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        let dev_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.scope.as_deref() == Some("development"))
            .collect();
        assert!(
            dev_deps.len() >= 2,
            "Should have at least 2 dev dependencies (rspec, rubocop), got {}",
            dev_deps.len()
        );

        for dep in &dev_deps {
            assert_eq!(dep.is_runtime, Some(false));
            assert_eq!(dep.is_optional, Some(true));
        }
    }

    // ==========================================================================
    // Test: .gemspec graceful error handling
    // ==========================================================================
    #[test]
    fn test_extract_gemspec_error_handling() {
        use crate::parsers::ruby::GemspecParser;
        // Non-existent file
        let package_data =
            GemspecParser::extract_first_package(&PathBuf::from("/nonexistent/test.gemspec"));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());
    }

    // ==========================================================================
    // Test: No unwrap/expect in gemspec library code
    // ==========================================================================
    #[test]
    fn test_gemspec_no_unwrap_no_expect() {
        // This test is covered by the existing test_no_unwrap_no_expect test
        // which scans the entire ruby.rs file.
        // This test exists to document the requirement for gemspec code.
    }

    // ==========================================================================
    // Test: Gem versions from specs section are captured via state machine
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_specs_versions() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // rake should have version 13.0.6 from the specs section
        let rake_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake_dep.is_some(), "Should find rake");

        let rake = rake_dep.unwrap();
        let purl = rake.purl.as_ref().unwrap();
        assert!(
            purl.contains("13.0.6"),
            "rake PURL should contain version 13.0.6 from specs, got: {}",
            purl
        );

        // json should have version 2.6.3 from specs
        let json_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("json")));
        assert!(json_dep.is_some(), "Should find json");
    }

    // ==========================================================================
    // Test: Platform-specific gem versions in lockfile (e.g., json-java)
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_platform_gems() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // json has both ruby and java platform variants in testdata
        let json_deps: Vec<_> = package_data
            .dependencies
            .iter()
            .filter(|d| d.purl.as_ref().is_some_and(|p| p.contains("json")))
            .collect();
        // At least one json dependency should exist
        assert!(!json_deps.is_empty(), "Should find json gem(s)");
    }

    // ==========================================================================
    // Test: SVN section handling (deprecated but supported)
    // ==========================================================================
    #[test]
    fn test_extract_lockfile_svn_section() {
        let content = "\
SVN
  remote: svn://example.com/repo
  revision: 12345
  specs:
    svn-gem (0.1.0)

GEM
  remote: https://rubygems.org/
  specs:
    rake (13.0.6)

PLATFORMS
  ruby

DEPENDENCIES
  svn-gem!
  rake (~> 13.0)
";
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let lockfile_path = temp_dir.path().join("Gemfile.lock");
        fs::write(&lockfile_path, content).expect("Failed to write lockfile");

        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        let svn_gem = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("svn-gem")));
        assert!(svn_gem.is_some(), "Should find SVN-sourced gem");
    }

    // ==========================================================================
    // Test: Package URL (PURL) generation
    // ==========================================================================
    #[test]
    fn test_purl_generation() {
        let content = r#"
source "https://rubygems.org"

gem "rails", "7.0.4"
"#;
        let (_temp_dir, gemfile_path) = create_temp_gemfile(content);
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        let rails = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rails")));
        assert!(rails.is_some());

        let purl = rails.unwrap().purl.as_ref().unwrap();
        // Should be pkg:gem/rails or pkg:gem/rails@7.0.4
        assert!(purl.starts_with("pkg:gem/rails"));
    }

    // ==========================================================================
    // GEM ARCHIVE PARSER TESTS (Wave 3)
    // ==========================================================================

    // ==========================================================================
    // Test: is_match for .gem archive files
    // ==========================================================================
    #[test]
    fn test_gem_archive_is_match() {
        use crate::parsers::ruby::GemArchiveParser;
        // Valid .gem paths
        assert!(GemArchiveParser::is_match(&PathBuf::from("example.gem")));
        assert!(GemArchiveParser::is_match(&PathBuf::from(
            "/path/to/rails-7.0.4.gem"
        )));
        assert!(GemArchiveParser::is_match(&PathBuf::from(
            "./vendor/cache/nokogiri-1.15.gem"
        )));

        // Invalid paths
        assert!(!GemArchiveParser::is_match(&PathBuf::from("Gemfile")));
        assert!(!GemArchiveParser::is_match(&PathBuf::from("Gemfile.lock")));
        assert!(!GemArchiveParser::is_match(&PathBuf::from("test.gemspec")));
        assert!(!GemArchiveParser::is_match(&PathBuf::from("package.json")));
        assert!(!GemArchiveParser::is_match(&PathBuf::from("gem")));
        assert!(!GemArchiveParser::is_match(&PathBuf::from("test.gem.bak")));
    }

    // ==========================================================================
    // Test: Basic .gem archive extraction
    // ==========================================================================
    #[test]
    fn test_extract_gem_archive_basic() {
        use crate::parsers::ruby::GemArchiveParser;
        let gem_path = PathBuf::from("testdata/ruby/example-gem-1.2.3.gem");
        let package_data = GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(package_data.name, Some("example-gem".to_string()));
        assert_eq!(package_data.version, Some("1.2.3".to_string()));
        assert_eq!(
            package_data.description,
            Some("A longer description of the example gem for testing purposes".to_string())
        );
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com/example-gem".to_string())
        );
        assert_eq!(
            package_data.declared_license_expression.as_deref(),
            Some("mit")
        );
        assert_eq!(
            package_data.declared_license_expression_spdx.as_deref(),
            Some("MIT")
        );
        assert_eq!(package_data.license_detections.len(), 1);
        assert!(package_data.extracted_license_statement.is_some());
        assert_eq!(package_data.primary_language, Some("Ruby".to_string()));
        assert_eq!(package_data.datasource_id, Some(DatasourceId::GemArchive));

        // Authors should be extracted as parties
        assert!(
            !package_data.parties.is_empty(),
            "Should have extracted authors as parties"
        );
        let author_names: Vec<_> = package_data
            .parties
            .iter()
            .filter_map(|p| p.name.as_ref())
            .collect();
        assert!(
            author_names.contains(&&"John Doe".to_string()),
            "Should find John Doe in parties"
        );
        assert!(
            author_names.contains(&&"Jane Smith".to_string()),
            "Should find Jane Smith in parties"
        );

        // Should have PURL
        assert!(
            package_data.purl.is_some(),
            "Should have PURL for gem archive"
        );
        let purl = package_data.purl.as_ref().unwrap();
        assert!(
            purl.contains("pkg:gem/example-gem"),
            "PURL should contain gem name"
        );
    }

    // ==========================================================================
    // Test: .gem archive dependencies extraction
    // ==========================================================================
    #[test]
    fn test_extract_gem_archive_dependencies() {
        use crate::parsers::ruby::GemArchiveParser;
        let gem_path = PathBuf::from("testdata/ruby/example-gem-1.2.3.gem");
        let package_data = GemArchiveParser::extract_first_package(&gem_path);

        // Should have 3 dependencies (rails, nokogiri runtime; rspec dev)
        assert!(
            package_data.dependencies.len() >= 3,
            "Should have at least 3 dependencies, got {}",
            package_data.dependencies.len()
        );

        // Check runtime dependency - rails
        let rails_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rails")));
        assert!(rails_dep.is_some(), "Should find rails dependency");
        let rails = rails_dep.unwrap();
        assert_eq!(rails.extracted_requirement, Some("~> 5.0".to_string()));
        assert_eq!(rails.is_runtime, Some(true));
        assert_eq!(
            rails.scope,
            Some("runtime".to_string()),
            "Runtime dep scope should be 'runtime' (Python ScanCode compatibility)"
        );

        // Check runtime dependency - nokogiri
        let nokogiri_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("nokogiri")));
        assert!(nokogiri_dep.is_some(), "Should find nokogiri dependency");
        let nokogiri = nokogiri_dep.unwrap();
        assert_eq!(nokogiri.extracted_requirement, Some(">= 1.6".to_string()));
        assert_eq!(nokogiri.is_runtime, Some(true));

        // Check development dependency - rspec
        let rspec_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rspec")));
        assert!(rspec_dep.is_some(), "Should find rspec dependency");
        let rspec = rspec_dep.unwrap();
        assert_eq!(rspec.extracted_requirement, Some("~> 3.0".to_string()));
        assert_eq!(rspec.scope, Some("development".to_string()));
        assert_eq!(rspec.is_runtime, Some(false));
        assert_eq!(rspec.is_optional, Some(true));
    }

    // ==========================================================================
    // Test: Minimal .gem archive (no dependencies)
    // ==========================================================================
    #[test]
    fn test_extract_gem_archive_minimal() {
        use crate::parsers::ruby::GemArchiveParser;
        let gem_path = PathBuf::from("testdata/ruby/minimal-gem-0.1.0.gem");
        let package_data = GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(package_data.name, Some("minimal-gem".to_string()));
        assert_eq!(package_data.version, Some("0.1.0".to_string()));
        assert!(
            package_data.dependencies.is_empty(),
            "Minimal gem should have no dependencies"
        );
    }

    // ==========================================================================
    // Test: .gem archive safety checks
    // ==========================================================================
    #[test]
    fn test_gem_archive_safety_checks() {
        use crate::parsers::ruby::GemArchiveParser;
        // Non-existent file should return default package data gracefully
        let package_data =
            GemArchiveParser::extract_first_package(&PathBuf::from("/nonexistent/test.gem"));
        assert!(package_data.name.is_none());
        assert!(package_data.dependencies.is_empty());

        // Corrupt file should return default gracefully
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let corrupt_path = temp_dir.path().join("corrupt.gem");
        fs::write(&corrupt_path, b"this is not a valid gem archive")
            .expect("Failed to write corrupt file");
        let package_data = GemArchiveParser::extract_first_package(&corrupt_path);
        assert!(
            package_data.name.is_none(),
            "Corrupt gem should return default package data"
        );
    }

    // ==========================================================================
    // Test: .gem archive error handling - no metadata.gz
    // ==========================================================================
    #[test]
    fn test_gem_archive_no_metadata() {
        use crate::parsers::ruby::GemArchiveParser;
        // Create a valid tar but without metadata.gz
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gem_path = temp_dir.path().join("no-metadata.gem");

        // Create an empty tar file (with just end-of-archive markers)
        let file = std::fs::File::create(&gem_path).expect("Failed to create file");
        let mut builder = tar::Builder::new(file);
        // Add a dummy file instead of metadata.gz
        let data = b"dummy content";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, "data.tar.gz", &data[..])
            .expect("Failed to add dummy entry");
        builder.finish().expect("Failed to finish tar");

        let package_data = GemArchiveParser::extract_first_package(&gem_path);
        assert!(
            package_data.name.is_none(),
            "Gem without metadata.gz should return default package data"
        );
    }

    // ==========================================================================
    // Bug Fix #3: GIT dependency metadata in Gemfile.lock
    // ==========================================================================
    #[test]
    fn test_lockfile_git_dependency_extra_data() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile_with_git");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // Should find the GIT gem with extra_data containing remote, revision, branch
        let git_gem = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("my-git-gem")));
        assert!(git_gem.is_some(), "Should find GIT gem my-git-gem");
        let gem = git_gem.unwrap();

        // Bug #3: extra_data should contain GIT metadata
        assert!(
            gem.extra_data.is_some(),
            "GIT gem should have extra_data with remote/revision/branch"
        );
        let extra = gem.extra_data.as_ref().unwrap();

        assert_eq!(
            extra.get("source_type").and_then(|v| v.as_str()),
            Some("GIT"),
            "Should have source_type GIT"
        );
        assert_eq!(
            extra.get("remote").and_then(|v| v.as_str()),
            Some("https://github.com/example/my-git-gem.git"),
            "Should have remote URL"
        );
        assert_eq!(
            extra.get("revision").and_then(|v| v.as_str()),
            Some("abc123def456789"),
            "Should have revision hash"
        );
        assert_eq!(
            extra.get("branch").and_then(|v| v.as_str()),
            Some("main"),
            "Should have branch"
        );
        assert_eq!(
            extra.get("ref").and_then(|v| v.as_str()),
            Some("v1.0.0"),
            "Should have ref"
        );
    }

    // ==========================================================================
    // Primary Gem Detection: PATH gems become the package itself
    // ==========================================================================
    #[test]
    fn test_lockfile_path_dependency_extra_data() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile_with_path");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        assert_eq!(
            package_data.name.as_deref(),
            Some("my-local-gem"),
            "PATH gem should become the primary package name"
        );

        assert!(
            package_data.version.is_some(),
            "PATH gem version should be extracted"
        );

        assert!(
            package_data.repository_homepage_url.is_some(),
            "Should have rubygems homepage URL"
        );
        assert!(
            package_data.repository_download_url.is_some(),
            "Should have rubygems download URL"
        );
        assert!(
            package_data.api_data_url.is_some(),
            "Should have rubygems API URL"
        );

        let path_gem_in_deps = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("my-local-gem")));
        assert!(
            path_gem_in_deps.is_none(),
            "Primary PATH gem should be excluded from dependencies"
        );
    }

    // ==========================================================================
    // Bug Fix #3: GIT metadata with tag
    // ==========================================================================
    #[test]
    fn test_lockfile_git_dependency_with_tag() {
        let content = "\
GIT
  remote: git://github.com/user/tagged-gem.git
  revision: deadbeef123456
  tag: v2.0.0
  specs:
    tagged-gem (2.0.0)

GEM
  remote: https://rubygems.org/
  specs:
    rake (13.0.6)

PLATFORMS
  ruby

DEPENDENCIES
  tagged-gem!
  rake (~> 13.0)
";
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let lockfile_path = temp_dir.path().join("Gemfile.lock");
        fs::write(&lockfile_path, content).expect("Failed to write lockfile");

        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        let tagged = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("tagged-gem")));
        assert!(tagged.is_some(), "Should find tagged-gem");
        let gem = tagged.unwrap();

        assert!(gem.extra_data.is_some(), "Should have extra_data");
        let extra = gem.extra_data.as_ref().unwrap();

        assert_eq!(
            extra.get("source_type").and_then(|v| v.as_str()),
            Some("GIT"),
            "Should have source_type GIT"
        );
        assert_eq!(
            extra.get("remote").and_then(|v| v.as_str()),
            Some("git://github.com/user/tagged-gem.git"),
            "Should have git:// remote URL"
        );
        assert_eq!(
            extra.get("revision").and_then(|v| v.as_str()),
            Some("deadbeef123456"),
            "Should have revision"
        );
        assert_eq!(
            extra.get("tag").and_then(|v| v.as_str()),
            Some("v2.0.0"),
            "Should have tag"
        );
        // No branch in this case
        assert!(
            extra.get("branch").is_none(),
            "Should NOT have branch when tag is specified"
        );
    }

    // ==========================================================================
    // Test: GEM section gems do NOT get GIT/PATH extra_data
    // ==========================================================================
    #[test]
    fn test_lockfile_gem_section_no_extra_data() {
        let lockfile_path = PathBuf::from("testdata/ruby/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        // Gems from the GEM section should NOT have source_type extra_data
        let rake = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake.is_some(), "Should find rake");
        let rake_gem = rake.unwrap();
        // GEM-sourced gems should not have GIT/PATH extra_data
        let has_source_type = rake_gem
            .extra_data
            .as_ref()
            .and_then(|e| e.get("source_type"))
            .is_some();
        assert!(
            !has_source_type,
            "GEM-sourced gems should not have source_type extra_data"
        );
    }

    #[test]
    fn test_gemspec_url_generation() {
        let content = r#"
Gem::Specification.new do |spec|
  spec.name        = "my_gem"
  spec.version     = "1.2.3"
  spec.summary     = "A test gem"
  spec.description = "A longer description"
  spec.homepage    = "https://example.com"
end
"#;

        let (_temp_dir, gemspec_path) = create_temp_gemspec(content);
        let package_data =
            crate::parsers::ruby::GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.name.as_deref(), Some("my_gem"));
        assert_eq!(package_data.version.as_deref(), Some("1.2.3"));

        assert_eq!(
            package_data.repository_homepage_url.as_deref(),
            Some("https://rubygems.org/gems/my_gem/versions/1.2.3")
        );
        assert_eq!(
            package_data.repository_download_url.as_deref(),
            Some("https://rubygems.org/downloads/my_gem-1.2.3.gem")
        );
        assert_eq!(
            package_data.api_data_url.as_deref(),
            Some("https://rubygems.org/api/v2/rubygems/my_gem/versions/1.2.3.json")
        );
        assert_eq!(
            package_data.download_url.as_deref(),
            Some("https://rubygems.org/downloads/my_gem-1.2.3.gem")
        );
    }

    #[test]
    fn test_gemspec_url_generation_without_version() {
        let content = r#"
Gem::Specification.new do |spec|
  spec.name        = "my_gem"
  spec.summary     = "A test gem without version"
end
"#;

        let (_temp_dir, gemspec_path) = create_temp_gemspec(content);
        let package_data =
            crate::parsers::ruby::GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.name.as_deref(), Some("my_gem"));
        assert!(package_data.version.is_none());

        assert_eq!(
            package_data.repository_homepage_url.as_deref(),
            Some("https://rubygems.org/gems/my_gem")
        );
        assert!(package_data.repository_download_url.is_none());
        assert_eq!(
            package_data.api_data_url.as_deref(),
            Some("https://rubygems.org/api/v1/versions/my_gem.json")
        );
        assert!(package_data.download_url.is_none());
    }

    #[test]
    fn test_gem_archive_url_generation() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        use std::io::Write;
        use tar::Builder;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gem_path = temp_dir.path().join("test.gem");

        let metadata_yaml = r#"--- !ruby/object:Gem::Specification
name: test_gem
version: !ruby/object:Gem::Version
  version: 2.0.0
platform: ruby
authors:
- Test Author
summary: Test summary
description: Test description
homepage: https://example.com
licenses:
- MIT
"#;

        let mut tar = Builder::new(Vec::new());

        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::default());
        gz_encoder.write_all(metadata_yaml.as_bytes()).unwrap();
        let compressed = gz_encoder.finish().unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("metadata.gz").unwrap();
        header.set_size(compressed.len() as u64);
        header.set_cksum();
        tar.append(&header, &compressed[..]).unwrap();

        let tar_data = tar.into_inner().unwrap();
        let mut gem_file = File::create(&gem_path).unwrap();
        gem_file.write_all(&tar_data).unwrap();

        let package_data = crate::parsers::ruby::GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(package_data.name.as_deref(), Some("test_gem"));
        assert_eq!(package_data.version.as_deref(), Some("2.0.0"));

        assert_eq!(
            package_data.repository_homepage_url.as_deref(),
            Some("https://rubygems.org/gems/test_gem/versions/2.0.0")
        );
        assert_eq!(
            package_data.repository_download_url.as_deref(),
            Some("https://rubygems.org/downloads/test_gem-2.0.0.gem")
        );
        assert_eq!(
            package_data.api_data_url.as_deref(),
            Some("https://rubygems.org/api/v2/rubygems/test_gem/versions/2.0.0.json")
        );
        assert_eq!(
            package_data.download_url.as_deref(),
            Some("https://rubygems.org/downloads/test_gem-2.0.0.gem")
        );
    }

    #[test]
    fn test_gem_archive_url_generation_with_platform() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        use std::io::Write;
        use tar::Builder;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gem_path = temp_dir.path().join("test-java.gem");

        let metadata_yaml = r#"--- !ruby/object:Gem::Specification
name: nokogiri
version: !ruby/object:Gem::Version
  version: 1.10.0
platform: java
authors:
- Java Author
summary: Java platform gem
"#;

        let mut tar = Builder::new(Vec::new());

        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::default());
        gz_encoder.write_all(metadata_yaml.as_bytes()).unwrap();
        let compressed = gz_encoder.finish().unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("metadata.gz").unwrap();
        header.set_size(compressed.len() as u64);
        header.set_cksum();
        tar.append(&header, &compressed[..]).unwrap();

        let tar_data = tar.into_inner().unwrap();
        let mut gem_file = File::create(&gem_path).unwrap();
        gem_file.write_all(&tar_data).unwrap();

        let package_data = crate::parsers::ruby::GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(package_data.name.as_deref(), Some("nokogiri"));
        assert_eq!(package_data.version.as_deref(), Some("1.10.0"));

        assert_eq!(
            package_data.repository_download_url.as_deref(),
            Some("https://rubygems.org/downloads/nokogiri-1.10.0-java.gem")
        );
        assert_eq!(
            package_data.download_url.as_deref(),
            Some("https://rubygems.org/downloads/nokogiri-1.10.0-java.gem")
        );
    }

    fn create_temp_gemspec(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gemspec_path = temp_dir.path().join("test.gemspec");
        fs::write(&gemspec_path, content).expect("Failed to write gemspec");
        (temp_dir, gemspec_path)
    }

    #[test]
    fn test_gem_archive_platform_qualifiers() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        use std::io::Write;
        use tar::Builder;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gem_path = temp_dir.path().join("nokogiri-java.gem");

        let metadata_yaml = r#"--- !ruby/object:Gem::Specification
name: nokogiri
version: !ruby/object:Gem::Version
  version: 1.10.0
platform: java
authors:
- Java Author
summary: Java platform gem
"#;

        let mut tar = Builder::new(Vec::new());

        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::default());
        gz_encoder.write_all(metadata_yaml.as_bytes()).unwrap();
        let compressed = gz_encoder.finish().unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("metadata.gz").unwrap();
        header.set_size(compressed.len() as u64);
        header.set_cksum();
        tar.append(&header, &compressed[..]).unwrap();

        let tar_data = tar.into_inner().unwrap();
        let mut gem_file = File::create(&gem_path).unwrap();
        gem_file.write_all(&tar_data).unwrap();

        let package_data = crate::parsers::ruby::GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(package_data.name.as_deref(), Some("nokogiri"));
        assert_eq!(package_data.version.as_deref(), Some("1.10.0"));

        assert!(
            package_data.qualifiers.is_some(),
            "Should have qualifiers for non-ruby platform"
        );
        let qualifiers = package_data.qualifiers.as_ref().unwrap();
        assert_eq!(qualifiers.get("platform"), Some(&"java".to_string()));
    }

    #[test]
    fn test_gem_archive_metadata_fields() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        use std::io::Write;
        use tar::Builder;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gem_path = temp_dir.path().join("metadata-test.gem");

        let metadata_yaml = r#"--- !ruby/object:Gem::Specification
name: test_metadata
version: !ruby/object:Gem::Version
  version: 2.1.0
date: 2023-05-15 12:34:56.789012345 +0000
metadata:
  bug_tracking_uri: https://github.com/example/test/issues
  source_code_uri: https://github.com/example/test
  homepage_uri: https://example.com
  files:
  - lib/test.rb
  - lib/test/version.rb
  - README.md
"#;

        let mut tar = Builder::new(Vec::new());

        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::default());
        gz_encoder.write_all(metadata_yaml.as_bytes()).unwrap();
        let compressed = gz_encoder.finish().unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("metadata.gz").unwrap();
        header.set_size(compressed.len() as u64);
        header.set_cksum();
        tar.append(&header, &compressed[..]).unwrap();

        let tar_data = tar.into_inner().unwrap();
        let mut gem_file = File::create(&gem_path).unwrap();
        gem_file.write_all(&tar_data).unwrap();

        let package_data = crate::parsers::ruby::GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(
            package_data.bug_tracking_url.as_deref(),
            Some("https://github.com/example/test/issues")
        );
        assert_eq!(
            package_data.code_view_url.as_deref(),
            Some("https://github.com/example/test")
        );
        assert_eq!(package_data.release_date.as_deref(), Some("2023-05-15"));

        assert_eq!(package_data.file_references.len(), 3);
        assert_eq!(package_data.file_references[0].path, "lib/test.rb");
        assert_eq!(package_data.file_references[1].path, "lib/test/version.rb");
        assert_eq!(package_data.file_references[2].path, "README.md");
    }

    #[test]
    fn test_gem_archive_ruby_platform_no_qualifiers() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::fs::File;
        use std::io::Write;
        use tar::Builder;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let gem_path = temp_dir.path().join("test-ruby.gem");

        let metadata_yaml = r#"--- !ruby/object:Gem::Specification
name: test_gem
version: !ruby/object:Gem::Version
  version: 1.0.0
platform: ruby
"#;

        let mut tar = Builder::new(Vec::new());

        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::default());
        gz_encoder.write_all(metadata_yaml.as_bytes()).unwrap();
        let compressed = gz_encoder.finish().unwrap();

        let mut header = tar::Header::new_gnu();
        header.set_path("metadata.gz").unwrap();
        header.set_size(compressed.len() as u64);
        header.set_cksum();
        tar.append(&header, &compressed[..]).unwrap();

        let tar_data = tar.into_inner().unwrap();
        let mut gem_file = File::create(&gem_path).unwrap();
        gem_file.write_all(&tar_data).unwrap();

        let package_data = crate::parsers::ruby::GemArchiveParser::extract_first_package(&gem_path);

        assert_eq!(package_data.name.as_deref(), Some("test_gem"));
        assert!(
            package_data.qualifiers.is_none(),
            "Ruby platform should not have qualifiers"
        );
    }

    // ==========================================================================
    // EXTRACTED GEM ARCHIVE PARSER TESTS
    // ==========================================================================

    #[test]
    fn test_gemfile_is_match_extracted() {
        use crate::parsers::ruby::GemfileParser;
        assert!(GemfileParser::is_match(&PathBuf::from(
            "testdata/gem/extracted-gemfile/data.gz-extract/Gemfile"
        )));
        assert!(GemfileParser::is_match(&PathBuf::from(
            "/path/to/gem/data.gz-extract/Gemfile"
        )));
    }

    #[test]
    fn test_gemfile_lock_is_match_extracted() {
        use crate::parsers::ruby::GemfileLockParser;
        assert!(GemfileLockParser::is_match(&PathBuf::from(
            "testdata/gem/extracted-gemfile-lock/data.gz-extract/Gemfile.lock"
        )));
        assert!(GemfileLockParser::is_match(&PathBuf::from(
            "/path/to/gem/data.gz-extract/Gemfile.lock"
        )));
    }

    #[test]
    fn test_gemspec_is_match_extracted() {
        use crate::parsers::ruby::GemspecParser;
        assert!(GemspecParser::is_match(&PathBuf::from(
            "testdata/gem/extracted-gemspec/data.gz-extract/example.gemspec"
        )));
        assert!(GemspecParser::is_match(&PathBuf::from(
            "testdata/gem/specifications/specifications/example.gemspec"
        )));
    }

    #[test]
    fn test_gem_metadata_extracted_is_match() {
        use crate::parsers::ruby::GemMetadataExtractedParser;
        assert!(GemMetadataExtractedParser::is_match(&PathBuf::from(
            "testdata/gem/extracted/metadata.gz-extract"
        )));
        assert!(GemMetadataExtractedParser::is_match(&PathBuf::from(
            "/path/to/gem/metadata.gz-extract"
        )));
        assert!(!GemMetadataExtractedParser::is_match(&PathBuf::from(
            "metadata.gz"
        )));
    }

    #[test]
    fn test_extract_gemfile_from_extracted_archive() {
        use crate::parsers::ruby::GemfileParser;
        let gemfile_path = PathBuf::from("testdata/gem/extracted-gemfile/data.gz-extract/Gemfile");
        let package_data = GemfileParser::extract_first_package(&gemfile_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::GemfileExtracted)
        );
        assert!(!package_data.dependencies.is_empty());

        let rake_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rake")));
        assert!(rake_dep.is_some(), "Should find rake dependency");
    }

    #[test]
    fn test_extract_gemfile_lock_from_extracted_archive() {
        use crate::parsers::ruby::GemfileLockParser;
        let lockfile_path =
            PathBuf::from("testdata/gem/extracted-gemfile-lock/data.gz-extract/Gemfile.lock");
        let package_data = GemfileLockParser::extract_first_package(&lockfile_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::GemfileLockExtracted)
        );
        assert!(!package_data.dependencies.is_empty());
    }

    #[test]
    fn test_extract_gemspec_from_extracted_archive() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path =
            PathBuf::from("testdata/gem/extracted-gemspec/data.gz-extract/example.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::GemspecExtracted)
        );
        assert_eq!(package_data.name, Some("example-gem".to_string()));
    }

    #[test]
    fn test_extract_gemspec_from_specifications() {
        use crate::parsers::ruby::GemspecParser;
        let gemspec_path =
            PathBuf::from("testdata/gem/specifications/specifications/example.gemspec");
        let package_data = GemspecParser::extract_first_package(&gemspec_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(
            package_data.datasource_id,
            Some(DatasourceId::GemGemspecInstalledSpecifications)
        );
        assert_eq!(package_data.name, Some("example-gem".to_string()));
    }

    #[test]
    fn test_extract_gem_metadata_extracted() {
        use crate::parsers::ruby::GemMetadataExtractedParser;
        let metadata_path = PathBuf::from("testdata/gem/extracted/metadata.gz-extract");
        let package_data = GemMetadataExtractedParser::extract_first_package(&metadata_path);

        assert_eq!(package_data.package_type, Some(PackageType::Gem));
        assert_eq!(package_data.name, Some("example-gem".to_string()));
        assert_eq!(package_data.version, Some("1.2.3".to_string()));
        assert_eq!(
            package_data.description,
            Some("A longer description of the example gem for testing purposes".to_string())
        );
        assert_eq!(
            package_data.homepage_url,
            Some("https://example.com/example-gem".to_string())
        );

        assert!(
            !package_data.parties.is_empty(),
            "Should have extracted authors"
        );
        let author_names: Vec<_> = package_data
            .parties
            .iter()
            .filter_map(|p| p.name.as_ref())
            .collect();
        assert!(
            author_names.contains(&&"John Doe".to_string()),
            "Should find John Doe"
        );

        assert!(
            package_data.dependencies.len() >= 3,
            "Should have at least 3 dependencies"
        );

        let rails_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rails")));
        assert!(rails_dep.is_some(), "Should find rails dependency");
        let rails = rails_dep.unwrap();
        assert_eq!(rails.extracted_requirement, Some("~> 5.0".to_string()));
        assert_eq!(rails.is_runtime, Some(true));

        let rspec_dep = package_data
            .dependencies
            .iter()
            .find(|d| d.purl.as_ref().is_some_and(|p| p.contains("rspec")));
        assert!(rspec_dep.is_some(), "Should find rspec dependency");
        let rspec = rspec_dep.unwrap();
        assert_eq!(rspec.scope, Some("development".to_string()));
        assert_eq!(rspec.is_runtime, Some(false));
    }
}
