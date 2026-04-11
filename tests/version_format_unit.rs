#[path = "../src/version_format.rs"]
mod version_format;

use version_format::{derive_build_version, sanitize_build_version};

#[test]
fn sanitize_build_version_rejects_control_characters() {
    assert_eq!(
        sanitize_build_version("0.1.0-5-gabc1234"),
        Some("0.1.0-5-gabc1234".to_string())
    );
    assert_eq!(sanitize_build_version("0.1.0\nboom"), None);
    assert_eq!(sanitize_build_version("0.1.0 dirty"), None);
    assert_eq!(sanitize_build_version("\u{1b}[31m0.1.0"), None);
}

#[test]
fn derive_build_version_keeps_plain_release_version_on_exact_tag() {
    assert_eq!(
        derive_build_version("0.1.0", Some("v0.1.0"), Some("abc1234")),
        "0.1.0"
    );
}

#[test]
fn derive_build_version_preserves_git_describe_suffix_after_tag() {
    assert_eq!(
        derive_build_version("0.1.0", Some("v0.1.0-3-gabc1234"), Some("abc1234")),
        "0.1.0-3-gabc1234"
    );
}

#[test]
fn derive_build_version_adds_sha_for_dirty_tagged_tree() {
    assert_eq!(
        derive_build_version("0.1.0", Some("v0.1.0-dirty"), Some("abc1234")),
        "0.1.0-0-gabc1234-dirty"
    );
}

#[test]
fn derive_build_version_falls_back_to_package_version_when_missing_git_data() {
    assert_eq!(derive_build_version("0.1.0", None, None), "0.1.0");
}

#[test]
fn derive_build_version_keeps_package_version_when_describe_is_invalid() {
    assert_eq!(
        derive_build_version("0.1.0", Some("bad\ndescribe"), Some("abc1234")),
        "0.1.0"
    );
}
