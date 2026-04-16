use super::extract_email_url_information;
use crate::models::{FileInfoBuilder, FileType};
use crate::scanner::TextDetectionOptions;
use std::path::Path;

#[test]
fn test_extract_email_url_information_skips_binary_string_text() {
    let mut builder = FileInfoBuilder::default();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
    };

    extract_email_url_information(
        &mut builder,
        Path::new("binary.bin"),
        "contact 6h@fo.lwft and visit http://gmail.com/",
        &options,
        true,
    );

    let file = builder
        .name("binary.bin".to_string())
        .base_name("binary".to_string())
        .extension(".bin".to_string())
        .path("binary.bin".to_string())
        .file_type(FileType::File)
        .size(1)
        .build()
        .expect("builder should produce file info");

    assert!(file.emails.is_empty(), "emails: {:?}", file.emails);
    assert!(file.urls.is_empty(), "urls: {:?}", file.urls);
}

#[test]
fn test_extract_email_url_information_keeps_good_binary_contacts() {
    let mut builder = FileInfoBuilder::default();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
    };

    extract_email_url_information(
        &mut builder,
        Path::new("binary.bin"),
        "report bugs to bug-coreutils@gnu.org and see https://www.gnu.org/software/coreutils/",
        &options,
        true,
    );

    let file = builder
        .name("binary.bin".to_string())
        .base_name("binary".to_string())
        .extension(".bin".to_string())
        .path("binary.bin".to_string())
        .file_type(FileType::File)
        .size(1)
        .build()
        .expect("builder should produce file info");

    assert_eq!(file.emails.len(), 1, "emails: {:?}", file.emails);
    assert_eq!(file.emails[0].email, "bug-coreutils@gnu.org");
    assert_eq!(file.urls.len(), 1, "urls: {:?}", file.urls);
    assert_eq!(file.urls[0].url, "https://www.gnu.org/software/coreutils/");
}

#[test]
fn test_extract_email_url_information_deduplicates_binary_emails_before_cap() {
    let mut builder = FileInfoBuilder::default();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: false,
        max_emails: 2,
        max_urls: 50,
        timeout_seconds: 120.0,
    };

    extract_email_url_information(
        &mut builder,
        Path::new("binary.bin"),
        "first jakub@redhat.com second jakub@redhat.com third contyk@redhat.com",
        &options,
        true,
    );

    let file = builder
        .name("binary.bin".to_string())
        .base_name("binary".to_string())
        .extension(".bin".to_string())
        .path("binary.bin".to_string())
        .file_type(FileType::File)
        .size(1)
        .build()
        .expect("builder should produce file info");

    assert_eq!(file.emails.len(), 2, "emails: {:?}", file.emails);
    assert_eq!(file.emails[0].email, "jakub@redhat.com");
    assert_eq!(file.emails[1].email, "contyk@redhat.com");
}

#[test]
fn test_extract_email_url_information_caps_after_binary_normalization() {
    let mut builder = FileInfoBuilder::default();
    let text = [
        "http://ocsp.digicert.com0/",
        "http://ocsp.digicert.com0a/",
        "http://www.digicert.com1!0/",
    ]
    .join("\n");
    let options = TextDetectionOptions {
        detect_urls: true,
        max_urls: 2,
        ..TextDetectionOptions::default()
    };

    extract_email_url_information(&mut builder, Path::new("binary.txt"), &text, &options, true);
    let file_info = builder
        .name("binary.txt".to_string())
        .base_name("binary".to_string())
        .extension(".txt".to_string())
        .path("binary.txt".to_string())
        .file_type(FileType::File)
        .size(0)
        .build()
        .expect("file info");

    assert_eq!(file_info.urls.len(), 2);
    assert_eq!(file_info.urls[0].url, "http://ocsp.digicert.com/");
    assert_eq!(file_info.urls[1].url, "http://www.digicert.com/");
}

#[test]
fn test_extract_email_url_information_keeps_gettext_mo_contacts() {
    let mut builder = FileInfoBuilder::default();
    let options = TextDetectionOptions {
        collect_info: false,
        detect_packages: false,
        detect_application_packages: false,
        detect_system_packages: false,
        detect_packages_in_compiled: false,
        detect_copyrights: false,
        detect_generated: false,
        detect_emails: true,
        detect_urls: true,
        max_emails: 50,
        max_urls: 50,
        timeout_seconds: 120.0,
    };

    extract_email_url_information(
        &mut builder,
        Path::new("locale/de/LC_MESSAGES/django.mo"),
        "translator LL@li.org cs@sweetgood.de",
        &options,
        true,
    );

    let file = builder
        .name("django.mo".to_string())
        .base_name("django".to_string())
        .extension(".mo".to_string())
        .path("locale/de/LC_MESSAGES/django.mo".to_string())
        .file_type(FileType::File)
        .size(1)
        .build()
        .expect("builder should produce file info");

    assert_eq!(file.emails.len(), 2, "emails: {:?}", file.emails);
    assert_eq!(file.emails[0].email, "ll@li.org");
    assert_eq!(file.emails[1].email, "cs@sweetgood.de");
}
