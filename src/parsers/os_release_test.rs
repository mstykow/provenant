// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::PackageParser;
    use super::super::os_release::*;
    use crate::models::DatasourceId;
    use crate::models::PackageType;
    use std::path::PathBuf;

    #[test]
    fn test_is_match_etc() {
        assert!(OsReleaseParser::is_match(&PathBuf::from("/etc/os-release")));
        assert!(OsReleaseParser::is_match(&PathBuf::from(
            "/some/path/etc/os-release"
        )));
    }

    #[test]
    fn test_is_match_usr_lib() {
        assert!(OsReleaseParser::is_match(&PathBuf::from(
            "/usr/lib/os-release"
        )));
        assert!(OsReleaseParser::is_match(&PathBuf::from(
            "/some/path/usr/lib/os-release"
        )));
    }

    #[test]
    fn test_is_not_match() {
        assert!(!OsReleaseParser::is_match(&PathBuf::from(
            "/etc/os-release.bak"
        )));
        assert!(!OsReleaseParser::is_match(&PathBuf::from("/etc/issue")));
        assert!(!OsReleaseParser::is_match(&PathBuf::from(
            "/usr/lib/os-release.old"
        )));
    }

    #[test]
    fn test_parse_debian() {
        let content = r#"
PRETTY_NAME="Debian GNU/Linux 11 (bullseye)"
NAME="Debian GNU/Linux"
VERSION_ID="11"
VERSION="11 (bullseye)"
ID=debian
HOME_URL="https://www.debian.org/"
SUPPORT_URL="https://www.debian.org/support"
BUG_REPORT_URL="https://bugs.debian.org/"
"#;

        let result = super::super::os_release::parse_os_release(content);

        assert_eq!(result.package_type, Some(PackageType::LinuxDistro));
        assert_eq!(result.namespace, Some("debian".to_string()));
        assert_eq!(result.name, Some("debian".to_string()));
        assert_eq!(result.version, Some("11".to_string()));
        assert_eq!(result.datasource_id, Some(DatasourceId::EtcOsRelease));

        assert_eq!(
            result.homepage_url,
            Some("https://www.debian.org/".to_string())
        );
        assert_eq!(
            result.code_view_url,
            Some("https://www.debian.org/support".to_string())
        );
        assert_eq!(
            result.bug_tracking_url,
            Some("https://bugs.debian.org/".to_string())
        );
    }

    #[test]
    fn test_parse_ubuntu() {
        let content = r#"
PRETTY_NAME="Ubuntu 22.04.1 LTS"
NAME="Ubuntu"
VERSION_ID="22.04"
VERSION="22.04.1 LTS (Jammy Jellyfish)"
ID=ubuntu
ID_LIKE=debian
HOME_URL="https://www.ubuntu.com/"
"#;

        let result = super::super::os_release::parse_os_release(content);

        assert_eq!(result.namespace, Some("debian".to_string()));
        assert_eq!(result.name, Some("ubuntu".to_string()));
        assert_eq!(result.version, Some("22.04".to_string()));
        assert_eq!(
            result.homepage_url,
            Some("https://www.ubuntu.com/".to_string())
        );
    }

    #[test]
    fn test_parse_fedora() {
        let content = r#"
NAME="Fedora Linux"
VERSION="37 (Workstation Edition)"
ID=fedora
VERSION_ID=37
PRETTY_NAME="Fedora Linux 37 (Workstation Edition)"
"#;

        let result = super::super::os_release::parse_os_release(content);

        assert_eq!(result.namespace, Some("fedora".to_string()));
        assert_eq!(result.name, Some("fedora".to_string()));
        assert_eq!(result.version, Some("37".to_string()));
    }

    #[test]
    fn test_parse_distroless() {
        let content = r#"
PRETTY_NAME="Distroless"
NAME="Debian GNU/Linux"
ID=debian
VERSION_ID="11"
"#;

        let result = super::super::os_release::parse_os_release(content);

        assert_eq!(result.namespace, Some("debian".to_string()));
        assert_eq!(result.name, Some("distroless".to_string()));
    }
}
