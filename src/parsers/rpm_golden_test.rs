#[cfg(test)]
mod golden_tests {
    use std::path::PathBuf;
    use std::process::Command;

    use crate::parsers::PackageParser;
    use crate::parsers::golden_test_utils::compare_package_data_parser_only;
    use crate::parsers::rpm_db::{
        RpmBdbDatabaseParser, RpmNdbDatabaseParser, RpmSqliteDatabaseParser,
    };
    use crate::parsers::rpm_license_files::RpmLicenseFilesParser;
    use crate::parsers::rpm_mariner_manifest::RpmMarinerManifestParser;
    use crate::parsers::rpm_parser::*;
    use crate::parsers::rpm_specfile::RpmSpecfileParser;
    use crate::parsers::rpm_yumdb::RpmYumdbParser;

    fn rpm_command_available() -> bool {
        Command::new("rpm").arg("--version").output().is_ok()
    }

    #[test]
    fn test_golden_rpm_archive() {
        let test_file = PathBuf::from("testdata/rpm/fping-2.4b2-10.fc12.x86_64.rpm");
        let expected_file =
            PathBuf::from("testdata/rpm/fping-2.4b2-10.fc12.x86_64.rpm.expected.json");

        if !test_file.exists() {
            return;
        }

        let package_data = RpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM archive: {}", e),
        }
    }

    #[test]
    fn test_golden_source_rpm_archive() {
        let test_file = PathBuf::from("testdata/rpm/setup-2.5.49-b1.src.rpm");
        let expected_file = PathBuf::from("testdata/rpm/setup-2.5.49-b1.src.rpm.expected.json");

        if !test_file.exists() {
            return;
        }

        let package_data = RpmParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for source RPM archive: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_sqlite_db() {
        if !rpm_command_available() {
            return;
        }

        let test_file = PathBuf::from("testdata/rpm/rpmdb.sqlite");
        let expected_file = PathBuf::from("testdata/rpm/rpmdb.sqlite.expected.json");

        let package_data = RpmSqliteDatabaseParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM sqlite db: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_bdb_default() {
        if !rpm_command_available() {
            return;
        }

        let test_file = PathBuf::from("testdata/rpm/var/lib/rpm/Packages");
        let expected_file = PathBuf::from("testdata/rpm/var/lib/rpm/Packages.expected.json");

        let package_data = RpmBdbDatabaseParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM bdb db: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_ndb_default() {
        if !rpm_command_available() {
            return;
        }

        let test_file = PathBuf::from("testdata/rpm/usr/lib/sysimage/rpm/Packages.db");
        let expected_file =
            PathBuf::from("testdata/rpm/usr/lib/sysimage/rpm/Packages.db.expected.json");

        let package_data = RpmNdbDatabaseParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM ndb db: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_license_file() {
        let test_file = PathBuf::from("testdata/rpm/licenses/usr/share/licenses/openssl/LICENSE");
        let expected_file =
            PathBuf::from("testdata/rpm/licenses/usr/share/licenses/openssl/LICENSE.expected.json");

        let package_data = RpmLicenseFilesParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM license file: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_mariner_manifest() {
        let test_file = PathBuf::from("testdata/rpm/var/lib/rpmmanifest/container-manifest-2");
        let expected_file =
            PathBuf::from("testdata/rpm/var/lib/rpmmanifest/container-manifest-2.expected.json");

        let package_data = RpmMarinerManifestParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM Mariner manifest: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_specfile() {
        let test_file = PathBuf::from("testdata/rpm/specfile/cpio.spec");
        let expected_file = PathBuf::from("testdata/rpm/specfile/cpio.spec.expected.json");

        let package_data = RpmSpecfileParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM specfile: {}", e),
        }
    }

    #[test]
    fn test_golden_rpm_yumdb() {
        let test_file = PathBuf::from(
            "testdata/rpm/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64/from_repo",
        );
        let expected_file = PathBuf::from(
            "testdata/rpm/var/lib/yum/yumdb/p/abc123-bash-5.0-1.el8.x86_64/from_repo.expected.json",
        );

        let package_data = RpmYumdbParser::extract_first_package(&test_file);

        match compare_package_data_parser_only(&package_data, &expected_file) {
            Ok(_) => (),
            Err(e) => panic!("Golden test failed for RPM yumdb: {}", e),
        }
    }
}
