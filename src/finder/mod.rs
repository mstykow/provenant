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
}
