// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::super::PackageParser;
    use super::super::erlang_otp::{ErlangAppSrcParser, RebarConfigParser, RebarLockParser};
    use super::super::try_parse_file;
    use crate::models::{DatasourceId, PackageType};

    // ── is_match ──

    #[test]
    fn test_app_src_is_match() {
        assert!(ErlangAppSrcParser::is_match(&PathBuf::from(
            "src/myapp.app.src"
        )));
        assert!(ErlangAppSrcParser::is_match(&PathBuf::from(
            "apps/web/src/web.app.src"
        )));
        assert!(!ErlangAppSrcParser::is_match(&PathBuf::from(
            "src/myapp.erl"
        )));
        assert!(!ErlangAppSrcParser::is_match(&PathBuf::from(
            "src/myapp.app"
        )));
    }

    #[test]
    fn test_rebar_config_is_match() {
        assert!(RebarConfigParser::is_match(&PathBuf::from("rebar.config")));
        assert!(RebarConfigParser::is_match(&PathBuf::from(
            "apps/web/rebar.config"
        )));
        assert!(!RebarConfigParser::is_match(&PathBuf::from(
            "rebar.config.script"
        )));
    }

    #[test]
    fn test_rebar_lock_is_match() {
        assert!(RebarLockParser::is_match(&PathBuf::from("rebar.lock")));
        assert!(!RebarLockParser::is_match(&PathBuf::from("rebar.config")));
    }

    // ── app.src parsing ──

    #[test]
    fn test_parse_app_src_fixture() {
        let package = ErlangAppSrcParser::extract_first_package(&PathBuf::from(
            "testdata/erlang-otp/app-src/lager.app.src",
        ));

        assert_eq!(package.package_type, Some(PackageType::Hex));
        assert_eq!(package.datasource_id, Some(DatasourceId::ErlangOtpAppSrc));
        assert_eq!(package.name.as_deref(), Some("lager"));
        assert_eq!(package.version.as_deref(), Some("3.9.2"));
        assert_eq!(
            package.description.as_deref(),
            Some("Erlang logging framework")
        );
        assert_eq!(
            package.extracted_license_statement.as_deref(),
            Some("Apache 2")
        );
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://github.com/erlang-lager/lager")
        );

        // goldrush should be a dependency, kernel/stdlib should be excluded
        assert_eq!(package.dependencies.len(), 1);
        assert!(
            package.dependencies[0]
                .purl
                .as_deref()
                .unwrap()
                .contains("goldrush")
        );
    }

    #[test]
    fn test_parse_app_src_with_multiple_deps() {
        let package = ErlangAppSrcParser::extract_first_package(&PathBuf::from(
            "testdata/erlang-otp/app-src/fast_xml.app.src",
        ));

        assert_eq!(package.name.as_deref(), Some("fast_xml"));
        assert_eq!(package.version.as_deref(), Some("1.1.60"));
        assert_eq!(
            package.description.as_deref(),
            Some("Fast Expat-based Erlang / Elixir XML parsing library")
        );
        assert_eq!(
            package.extracted_license_statement.as_deref(),
            Some("Apache 2.0")
        );

        // p1_utils should be a dependency, kernel/stdlib should be excluded
        assert_eq!(package.dependencies.len(), 1);
        assert!(
            package.dependencies[0]
                .purl
                .as_deref()
                .unwrap()
                .contains("p1_utils")
        );
    }

    #[test]
    fn test_parse_app_src_template_version_skipped() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("myapp.app.src");
        fs::write(
            &path,
            r#"{application, myapp, [{vsn, "%VSN%"}, {description, "test"}]}."#,
        )
        .expect("write");

        let package = ErlangAppSrcParser::extract_first_package(&path);
        assert_eq!(package.name.as_deref(), Some("myapp"));
        assert!(package.version.is_none());
    }

    #[test]
    fn test_parse_app_src_runtime_dependencies() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("stdlib.app.src");
        fs::write(
            &path,
            r#"{application, stdlib, [
                {vsn, "5.0"},
                {runtime_dependencies, ["sasl-3.0","kernel-9.0","crypto-4.5"]}
            ]}."#,
        )
        .expect("write");

        let package = ErlangAppSrcParser::extract_first_package(&path);
        assert_eq!(package.name.as_deref(), Some("stdlib"));
        assert_eq!(package.version.as_deref(), Some("5.0"));
        // sasl, kernel, crypto are all OTP stdlib — should be filtered
        assert!(package.dependencies.is_empty());
    }

    #[test]
    fn test_parse_app_src_with_maps_and_link_map() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("myapp.app.src");
        fs::write(
            &path,
            r#"{application, myapp, [
                {vsn, "1.2.3"},
                {description, "Map-aware app"},
                {env, #{transport => <<"mqtt">>, retries => 3, nested => #{}}},
                {links, #{"Docs" => "https://example.com/docs", "Github" => "https://github.com/example/myapp"}}
            ]}."#,
        )
        .expect("write");

        let package = ErlangAppSrcParser::extract_first_package(&path);
        assert_eq!(package.name.as_deref(), Some("myapp"));
        assert_eq!(package.version.as_deref(), Some("1.2.3"));
        assert_eq!(package.description.as_deref(), Some("Map-aware app"));
        assert_eq!(
            package.homepage_url.as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(
            package.vcs_url.as_deref(),
            Some("https://github.com/example/myapp")
        );
    }

    #[test]
    fn test_parse_app_src_with_non_stdlib_runtime_deps() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("myapp.app.src");
        fs::write(
            &path,
            r#"{application, myapp, [
                {vsn, "1.0.0"},
                {runtime_dependencies, ["cowboy-2.10.0","ranch-2.1.0"]}
            ]}."#,
        )
        .expect("write");

        let package = ErlangAppSrcParser::extract_first_package(&path);
        assert_eq!(package.dependencies.len(), 2);
        assert_eq!(
            package.dependencies[0].extracted_requirement.as_deref(),
            Some("2.10.0")
        );
        assert!(
            package.dependencies[0]
                .purl
                .as_deref()
                .unwrap()
                .contains("cowboy")
        );
    }

    #[test]
    fn test_parse_app_src_malformed_returns_fallback() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("bad.app.src");
        fs::write(&path, "not valid erlang at all!!!").expect("write");

        let package = ErlangAppSrcParser::extract_first_package(&path);
        assert_eq!(package.package_type, Some(PackageType::Hex));
        assert_eq!(package.datasource_id, Some(DatasourceId::ErlangOtpAppSrc));
        assert!(package.name.is_none());
    }

    // ── rebar.config parsing ──

    #[test]
    fn test_parse_rebar_config_fixture() {
        let package = RebarConfigParser::extract_first_package(&PathBuf::from(
            "testdata/erlang-otp/rebar-config/rebar.config",
        ));

        assert_eq!(package.package_type, Some(PackageType::Hex));
        assert_eq!(package.datasource_id, Some(DatasourceId::RebarConfig));

        // 3 main deps + 1 test profile dep
        assert_eq!(package.dependencies.len(), 4);

        let cowboy = &package.dependencies[0];
        assert!(cowboy.purl.as_deref().unwrap().contains("cowboy"));
        assert_eq!(cowboy.extracted_requirement.as_deref(), Some("2.10.0"));
        assert_eq!(cowboy.scope.as_deref(), Some("dependencies"));

        let jiffy = &package.dependencies[1];
        assert!(jiffy.purl.as_deref().unwrap().contains("jiffy"));
        assert_eq!(jiffy.extracted_requirement.as_deref(), Some("1.1.1"));
        assert!(
            jiffy
                .extra_data
                .as_ref()
                .unwrap()
                .get("vcs_url")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("jiffy")
        );

        let proper = &package.dependencies[3];
        assert!(proper.purl.as_deref().unwrap().contains("proper"));
        assert_eq!(proper.scope.as_deref(), Some("test"));
    }

    #[test]
    fn test_parse_rebar_config_git_only_dep() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.config");
        fs::write(
            &path,
            r#"{deps, [{lager, {git, "https://github.com/erlang-lager/lager.git", {branch, "master"}}}]}."#,
        )
        .expect("write");

        let package = RebarConfigParser::extract_first_package(&path);
        assert_eq!(package.dependencies.len(), 1);
        let dep = &package.dependencies[0];
        assert!(dep.purl.as_deref().unwrap().contains("lager"));
        // branch deps don't get a version
        assert!(dep.extracted_requirement.is_none());
    }

    #[test]
    fn test_parse_rebar_config_empty_deps() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.config");
        fs::write(&path, "{deps, []}.\n{erl_opts, [debug_info]}.\n").expect("write");

        let package = RebarConfigParser::extract_first_package(&path);
        assert_eq!(package.datasource_id, Some(DatasourceId::RebarConfig));
        assert!(package.dependencies.is_empty());
    }

    #[test]
    fn test_parse_rebar_config_skips_conditional_wrappers_instead_of_guessing() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.config");
        fs::write(
            &path,
            r#"{deps, [{if_var_true, coverage, {proper, "1.4.0"}}, {cowboy, "2.10.0"}]}.
"#,
        )
        .expect("write");

        let package = RebarConfigParser::extract_first_package(&path);
        assert_eq!(package.dependencies.len(), 1);
        assert_eq!(
            package.dependencies[0].purl.as_deref(),
            Some("pkg:hex/cowboy@2.10.0")
        );
    }

    #[test]
    fn test_parse_rebar_config_pkg_alias_dep() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.config");
        fs::write(
            &path,
            r#"{deps, [{uuid, "1.2.0", {pkg, uuid_erl}}, {cowboy_alias, {pkg, cowboy}}]}."#,
        )
        .expect("write");

        let package = RebarConfigParser::extract_first_package(&path);
        assert_eq!(package.dependencies.len(), 2);

        let uuid = &package.dependencies[0];
        assert_eq!(uuid.purl.as_deref(), Some("pkg:hex/uuid_erl@1.2.0"));
        assert_eq!(uuid.extracted_requirement.as_deref(), Some("1.2.0"));
        assert_eq!(
            uuid.extra_data
                .as_ref()
                .and_then(|extra| extra.get("app_name"))
                .and_then(|value| value.as_str()),
            Some("uuid")
        );

        let cowboy = &package.dependencies[1];
        assert_eq!(cowboy.purl.as_deref(), Some("pkg:hex/cowboy"));
        assert!(cowboy.extracted_requirement.is_none());
    }

    #[test]
    fn test_parse_rebar_config_malformed_returns_fallback() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.config");
        fs::write(&path, "}}}}garbage").expect("write");

        let package = RebarConfigParser::extract_first_package(&path);
        assert_eq!(package.datasource_id, Some(DatasourceId::RebarConfig));
    }

    // ── rebar.lock parsing ──

    #[test]
    fn test_parse_rebar_lock_fixture() {
        let package = RebarLockParser::extract_first_package(&PathBuf::from(
            "testdata/erlang-otp/rebar-lock/rebar.lock",
        ));

        assert_eq!(package.package_type, Some(PackageType::Hex));
        assert_eq!(package.datasource_id, Some(DatasourceId::RebarLock));

        // 4 dependencies: cowboy, cowlib, ranch (pkg), jiffy (git)
        assert_eq!(package.dependencies.len(), 4);

        let cowboy = &package.dependencies[0];
        assert!(cowboy.purl.as_deref().unwrap().contains("cowboy"));
        assert_eq!(cowboy.extracted_requirement.as_deref(), Some("2.10.0"));
        assert_eq!(cowboy.is_pinned, Some(true));
        assert!(cowboy.resolved_package.is_some());

        let jiffy = &package.dependencies[3];
        assert!(jiffy.purl.as_deref().unwrap().contains("jiffy"));
        // git ref dep gets the ref as version
        assert_eq!(jiffy.extracted_requirement.as_deref(), Some("abc123def456"));
        assert!(
            jiffy
                .extra_data
                .as_ref()
                .unwrap()
                .get("vcs_url")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("jiffy")
        );
    }

    #[test]
    fn test_parse_rebar_lock_with_hashes() {
        let package = RebarLockParser::extract_first_package(&PathBuf::from(
            "testdata/erlang-otp/rebar-lock/rebar.lock",
        ));

        // cowboy has a pkg_hash entry
        let cowboy = &package.dependencies[0];
        let resolved = cowboy.resolved_package.as_ref().unwrap();
        assert!(resolved.sha256.is_some());
    }

    #[test]
    fn test_parse_rebar_lock_pkg_alias_dep() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.lock");
        fs::write(
            &path,
            concat!(
                "{\"1.2.0\", [{<<\"uuid\">>, {pkg, <<\"uuid_erl\">>, <<\"1.2.0\">>}, 0}]}.\n",
                "[{pkg_hash, [{<<\"uuid\">>, <<\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\">>}]}].\n"
            ),
        )
        .expect("write");

        let package = RebarLockParser::extract_first_package(&path);
        assert_eq!(package.dependencies.len(), 1);

        let dep = &package.dependencies[0];
        assert_eq!(dep.purl.as_deref(), Some("pkg:hex/uuid_erl@1.2.0"));
        assert_eq!(dep.extracted_requirement.as_deref(), Some("1.2.0"));
        assert_eq!(
            dep.extra_data
                .as_ref()
                .and_then(|extra| extra.get("app_name"))
                .and_then(|value| value.as_str()),
            Some("uuid")
        );

        let resolved = dep.resolved_package.as_ref().expect("resolved package");
        assert_eq!(resolved.name, "uuid_erl");
        assert!(resolved.sha256.is_some());
    }

    #[test]
    fn test_parse_rebar_lock_malformed_returns_fallback() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("rebar.lock");
        fs::write(&path, "not valid erlang lock").expect("write");

        let package = RebarLockParser::extract_first_package(&path);
        assert_eq!(package.datasource_id, Some(DatasourceId::RebarLock));
    }

    // ── Scanner dispatch ──

    #[test]
    fn test_dispatch_app_src() {
        let result = try_parse_file(&PathBuf::from("testdata/erlang-otp/app-src/lager.app.src"))
            .expect("should be claimed by parser dispatch");
        assert!(result.scan_errors.is_empty());
        assert_eq!(result.packages.len(), 1);
        assert_eq!(
            result.packages[0].datasource_id,
            Some(DatasourceId::ErlangOtpAppSrc)
        );
    }

    #[test]
    fn test_dispatch_rebar_config() {
        let result = try_parse_file(&PathBuf::from(
            "testdata/erlang-otp/rebar-config/rebar.config",
        ))
        .expect("should be claimed by parser dispatch");
        assert!(result.scan_errors.is_empty());
        assert_eq!(result.packages.len(), 1);
        assert_eq!(
            result.packages[0].datasource_id,
            Some(DatasourceId::RebarConfig)
        );
    }

    #[test]
    fn test_dispatch_rebar_lock() {
        let result = try_parse_file(&PathBuf::from("testdata/erlang-otp/rebar-lock/rebar.lock"))
            .expect("should be claimed by parser dispatch");
        assert!(result.scan_errors.is_empty());
        assert_eq!(result.packages.len(), 1);
        assert_eq!(
            result.packages[0].datasource_id,
            Some(DatasourceId::RebarLock)
        );
    }
}
