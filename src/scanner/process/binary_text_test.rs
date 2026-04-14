use super::{
    extract_named_author_from_binary_line, is_binary_string_author_candidate,
    is_binary_string_email_candidate, is_binary_string_url_candidate, normalize_binary_string_url,
};

#[test]
fn test_binary_string_email_candidate_rejects_gibberish() {
    assert!(!is_binary_string_email_candidate("6h@fo.lwft"));
}

#[test]
fn test_binary_string_email_candidate_keeps_gnu_bug_address() {
    assert!(is_binary_string_email_candidate("bug-coreutils@gnu.org"));
}

#[test]
fn test_binary_string_url_candidate_rejects_short_fake_host() {
    assert!(!is_binary_string_url_candidate("http://ftp.so/"));
}

#[test]
fn test_binary_string_url_candidate_keeps_gnu_help_url() {
    assert!(is_binary_string_url_candidate(
        "https://www.gnu.org/software/coreutils/"
    ));
}

#[test]
fn test_binary_string_url_candidate_rejects_bare_root_domain() {
    assert!(!is_binary_string_url_candidate("http://gmail.com/"));
}

#[test]
fn test_binary_string_url_candidate_keeps_project_subdomain_root() {
    assert!(is_binary_string_url_candidate("http://gcc.gnu.org"));
}

#[test]
fn test_binary_string_url_candidate_keeps_long_org_root_domain() {
    assert!(is_binary_string_url_candidate("https://publicsuffix.org/"));
}

#[test]
fn test_binary_string_url_candidate_keeps_short_project_path() {
    assert!(is_binary_string_url_candidate("http://tukaani.org/xz/"));
}

#[test]
fn test_normalize_binary_string_url_trims_certificate_host_tail_noise() {
    assert_eq!(
        normalize_binary_string_url("http://ocsp.digicert.com0/"),
        Some("http://ocsp.digicert.com/".to_string())
    );
    assert_eq!(
        normalize_binary_string_url("http://www.digicert.com1!0/"),
        Some("http://www.digicert.com/".to_string())
    );
}

#[test]
fn test_normalize_binary_string_url_trims_trailing_path_noise() {
    assert_eq!(
        normalize_binary_string_url(
            "http://cacerts.digicert.com/DigiCertTrustedG4TimeStampingRSA4096SHA2562025CA1.crt0_"
        ),
        Some(
            "http://cacerts.digicert.com/DigiCertTrustedG4TimeStampingRSA4096SHA2562025CA1.crt0"
                .to_string()
        )
    );
}

#[test]
fn test_normalize_binary_string_url_preserves_clean_certificate_urls() {
    assert_eq!(
        normalize_binary_string_url("http://ocsp.digicert.com/"),
        Some("http://ocsp.digicert.com/".to_string())
    );
    assert_eq!(
        normalize_binary_string_url(
            "http://cacerts.digicert.com/DigiCertTrustedG4TimeStampingRSA4096SHA2562025CA1.crt0"
        ),
        Some(
            "http://cacerts.digicert.com/DigiCertTrustedG4TimeStampingRSA4096SHA2562025CA1.crt0"
                .to_string()
        )
    );
}

#[test]
fn test_normalize_binary_string_url_does_not_trim_long_host_suffixes() {
    assert_eq!(
        normalize_binary_string_url("http://example.com0evil/"),
        None
    );
}

#[test]
fn test_normalize_binary_string_url_does_not_trim_legitimate_path_suffix() {
    assert_eq!(
        normalize_binary_string_url("http://example.com/path_/"),
        Some("http://example.com/path_/".to_string())
    );
}

#[test]
fn test_binary_string_author_candidate_keeps_named_author_with_email() {
    assert!(is_binary_string_author_candidate(
        "Andreas Schneider <asn@redhat.com>"
    ));
}

#[test]
fn test_binary_string_author_candidate_rejects_gibberish() {
    assert!(!is_binary_string_author_candidate(
        "S8@9 K @9 D @9 I,@9N(@ F@@9L,@ HD@9"
    ));
}

#[test]
fn test_binary_string_author_candidate_rejects_changelog_phrase() {
    assert!(!is_binary_string_author_candidate(
        "Developers can enable them. - revert news user back to"
    ));
}

#[test]
fn test_extract_named_author_from_binary_line_recovers_by_prefix() {
    assert_eq!(
        extract_named_author_from_binary_line("Patch by Andreas Schneider <asn@redhat.com>"),
        Some("Andreas Schneider <asn@redhat.com>".to_string())
    );
}

#[test]
fn test_extract_named_author_from_binary_line_recovers_parenthesized_email() {
    assert_eq!(
        extract_named_author_from_binary_line(
            "same for both OpenSSL and NSS by Rob Crittenden (rcritten@redhat.com)"
        ),
        Some("Rob Crittenden (rcritten@redhat.com)".to_string())
    );
}

#[test]
fn test_extract_named_author_from_binary_line_rejects_plain_changelog_packager_line() {
    assert_eq!(
        extract_named_author_from_binary_line("Rob Crittenden <rcritten@redhat.com> - 3.11.7-9"),
        None
    );
}

#[test]
fn test_extract_named_author_from_binary_line_keeps_email_only_review_author() {
    assert_eq!(
        extract_named_author_from_binary_line(
            "Changes as per initial review by panemade@gmail.com"
        ),
        Some("panemade@gmail.com".to_string())
    );
}

#[test]
fn test_binary_string_author_candidate_rejects_multiple_emails_on_one_line() {
    assert!(!is_binary_string_author_candidate(
        "Rob Crittenden (rcritten@redhat.com) jakub@redhat.com"
    ));
}
