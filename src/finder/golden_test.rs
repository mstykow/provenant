#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde::{Deserialize, Serialize};

    use crate::finder::emails::EmailDetection;
    use crate::finder::urls::UrlDetection;
    use crate::finder::{DetectionConfig, find_emails, find_urls};
    use crate::utils::file::extract_text_for_detection;

    #[derive(Debug, Deserialize)]
    struct ExpectedReport {
        files: Vec<ExpectedFile>,
    }

    #[derive(Debug, Deserialize)]
    struct ExpectedFile {
        path: String,
        #[serde(default)]
        emails: Option<Vec<ExpectedEmail>>,
        #[serde(default)]
        urls: Option<Vec<ExpectedUrl>>,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    struct ExpectedEmail {
        email: String,
        start_line: usize,
        end_line: usize,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    struct ExpectedUrl {
        url: String,
        start_line: usize,
        end_line: usize,
    }

    fn read_expected(name: &str) -> ExpectedReport {
        let path = PathBuf::from("testdata/plugin_email_url").join(name);
        let json = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read expected fixture {:?}: {}", path, e));
        serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("failed to parse expected fixture {:?}: {}", path, e))
    }

    fn read_input(relative_path: &str) -> String {
        let path = PathBuf::from("testdata/plugin_email_url/files").join(relative_path);
        let bytes = fs::read(&path)
            .unwrap_or_else(|e| panic!("failed to read input fixture {:?}: {}", path, e));
        let (content, _) = extract_text_for_detection(&path, &bytes);
        content
    }

    fn to_expected_emails(detections: Vec<EmailDetection>) -> Vec<ExpectedEmail> {
        detections
            .into_iter()
            .map(|d| ExpectedEmail {
                email: d.email,
                start_line: d.start_line.get(),
                end_line: d.end_line.get(),
            })
            .collect()
    }

    fn to_expected_urls(detections: Vec<UrlDetection>) -> Vec<ExpectedUrl> {
        detections
            .into_iter()
            .map(|d| ExpectedUrl {
                url: d.url,
                start_line: d.start_line.get(),
                end_line: d.end_line.get(),
            })
            .collect()
    }

    fn assert_emails_fixture(expected_name: &str, max_emails: usize) {
        let expected = read_expected(expected_name);
        let cfg = DetectionConfig {
            max_emails,
            max_urls: 50,
            unique: true,
        };

        for file in expected.files {
            let content = read_input(&file.path);
            let actual = to_expected_emails(find_emails(&content, &cfg));
            let expected_emails = file.emails.unwrap_or_default();
            assert_eq!(
                actual, expected_emails,
                "email fixture mismatch for {} using {}",
                file.path, expected_name
            );
        }
    }

    fn assert_urls_fixture(expected_name: &str, max_urls: usize) {
        let expected = read_expected(expected_name);
        let cfg = DetectionConfig {
            max_emails: 50,
            max_urls,
            unique: true,
        };

        for file in expected.files {
            let content = read_input(&file.path);
            let actual = to_expected_urls(find_urls(&content, &cfg));
            let expected_urls = file.urls.unwrap_or_default();
            assert_eq!(
                actual, expected_urls,
                "url fixture mismatch for {} using {}",
                file.path, expected_name
            );
        }
    }

    #[test]
    fn test_emails_golden() {
        assert_emails_fixture("emails.expected.json", 50);
    }

    #[test]
    fn test_emails_threshold_golden() {
        assert_emails_fixture("emails-threshold.expected.json", 2);
    }

    #[test]
    fn test_emails_image_metadata_golden() {
        assert_emails_fixture("emails-image-metadata.expected.json", 50);
    }

    #[test]
    fn test_urls_golden() {
        assert_urls_fixture("urls.expected.json", 50);
    }

    #[test]
    fn test_urls_threshold_golden() {
        assert_urls_fixture("urls-threshold.expected.json", 2);
    }

    #[test]
    fn test_urls_image_metadata_golden() {
        assert_urls_fixture("urls-image-metadata.expected.json", 50);
    }
}
