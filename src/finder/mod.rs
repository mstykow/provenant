mod emails;
#[cfg(all(test, feature = "golden-tests"))]
mod golden_test;
mod host;
mod junk_data;
mod urls;

pub use emails::find_emails;
pub use urls::find_urls;

#[derive(Debug, Clone)]
pub struct DetectionConfig {
    pub max_emails: usize,
    pub max_urls: usize,
    pub unique: bool,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            max_emails: 50,
            max_urls: 50,
            unique: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DetectionConfig, find_emails, find_urls};

    #[test]
    fn test_find_emails_threshold() {
        let text = "a@b.com\nc@d.com\ne@f.com\n";
        let config = DetectionConfig {
            max_emails: 2,
            ..Default::default()
        };
        let emails = find_emails(text, &config);
        assert_eq!(emails.len(), 2);
        assert_eq!(emails[0].email, "a@b.com");
        assert_eq!(emails[0].start_line, 1);
    }

    #[test]
    fn test_find_urls_threshold() {
        let text = "http://a.com\nhttp://b.com\nhttp://c.com\n";
        let config = DetectionConfig {
            max_urls: 2,
            ..Default::default()
        };
        let urls = find_urls(text, &config);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].url, "http://a.com/");
        assert_eq!(urls[1].url, "http://b.com/");
    }

    #[test]
    fn test_find_emails_filters_local_machine_domains() {
        let text = "admin@rust-lang.org\ngeisse@shopgates-mac-mini-3.local\n";
        let config = DetectionConfig::default();
        let emails = find_emails(text, &config);

        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0].email, "admin@rust-lang.org");
    }

    #[test]
    fn test_find_urls_ignores_email_like_ftp_token() {
        let text = "See ftp.mtuci@gmail.com for details.";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert!(urls.is_empty(), "urls: {urls:#?}");
    }

    #[test]
    fn test_find_urls_keeps_plain_ftp_hostname() {
        let text = "Mirror: ftp.gnu.org/gnu/tar/";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "http://ftp.gnu.org/gnu/tar/");
    }

    #[test]
    fn test_find_urls_splits_literal_escaped_newline_separated_urls() {
        let text = "https://docs.celeryq.dev/en/latest/userguide/workers.html#concurrency\\nhttps://docs.celeryq.dev/en/latest/userguide/concurrency/eventlet.html";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        let values: Vec<_> = urls.into_iter().map(|url| url.url).collect();
        assert_eq!(
            values,
            vec![
                "https://docs.celeryq.dev/en/latest/userguide/workers.html#concurrency".to_string(),
                "https://docs.celeryq.dev/en/latest/userguide/concurrency/eventlet.html"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn test_find_urls_strips_template_credentials_from_git_urls() {
        let text = "Repo: https://user:{ACCESS_TOKEN}@github.com/apache/airflow.git";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://github.com/apache/airflow.git");
    }
}
