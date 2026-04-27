// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

mod emails;
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
    use crate::models::LineNumber;

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
        assert_eq!(emails[0].start_line, LineNumber::ONE);
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
    fn test_find_emails_ignores_literal_escaped_newline_code_artifacts() {
        let text = r#"email": "global_writer@email.com\n@app.route\n@csrf.exempt\nuser5@email.com"#;
        let config = DetectionConfig::default();
        let emails = find_emails(text, &config);

        let values: Vec<_> = emails.into_iter().map(|email| email.email).collect();
        assert_eq!(
            values,
            vec![
                "global_writer@email.com".to_string(),
                "user5@email.com".to_string(),
            ]
        );
    }

    #[test]
    fn test_find_emails_ignores_r_slot_access_false_positives() {
        let text = "element@arrow.fill <- element@colour\ntt@inherit.blank <- FALSE\n";
        let config = DetectionConfig::default();
        let emails = find_emails(text, &config);

        assert!(emails.is_empty(), "emails: {emails:#?}");
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
        let text = "Repo: https://user:{ACCESS_TOKEN}@github.com/example/project.git";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://github.com/example/project.git");
    }

    #[test]
    fn test_find_urls_strips_percent_encoded_template_credentials_from_git_urls() {
        let text = "Repo: https://user:%7BACCESS_TOKEN%7D@github.com/example/project.git";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://github.com/example/project.git");
    }

    #[test]
    fn test_find_urls_dedupes_plain_and_templated_git_urls_after_sanitization() {
        let text = concat!(
            "https://github.com/example/project.git\n",
            "https://user:%7BACCESS_TOKEN%7D@github.com/example/project.git\n",
        );
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://github.com/example/project.git");
    }

    #[test]
    fn test_find_urls_strips_trailing_backticks() {
        let text = "Docs: https://github.com/example/project.git``";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://github.com/example/project.git");
    }

    #[test]
    fn test_find_urls_strips_rd_url_braces() {
        let text = r#"\\url{https://dplyr.tidyverse.org}"#;
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://dplyr.tidyverse.org/");
    }

    #[test]
    fn test_find_urls_strips_rd_href_trailing_braces() {
        let text = r#"\\href{https://orcid.org/0000-0003-4757-117X}{ORCID}"#;
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://orcid.org/0000-0003-4757-117X");
    }

    #[test]
    fn test_find_urls_strips_rd_url_double_closing_braces() {
        let text = r#"\\url{https://fred.stlouisfed.org/series/PCE}}"#;
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://fred.stlouisfed.org/series/PCE");
    }

    #[test]
    fn test_find_urls_strips_rd_closing_brace_before_punctuation() {
        let text = r#"\\url{https://fred.stlouisfed.org/}."#;
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://fred.stlouisfed.org/");
    }

    #[test]
    fn test_find_urls_keeps_closed_template_placeholders() {
        let text =
            "https://flutter-dashboard.appspot.com/#/build?repo=flutter&branch=${branchName}";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(
            urls[0].url,
            "https://flutter-dashboard.appspot.com/#/build?repo=flutter&branch=${branchName}"
        );
    }

    #[test]
    fn test_find_urls_trims_open_template_suffixes() {
        let text =
            "https://github.com/flutter/flutter/pull/${{ github.event.pull_request.number }}";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert_eq!(urls.len(), 1, "urls: {urls:#?}");
        assert_eq!(urls[0].url, "https://github.com/flutter/flutter/pull");
    }

    #[test]
    fn test_find_urls_ignores_markdown_emphasis_inside_hostname() {
        let text = "Use https://**yourcompany**.atlassian.net for Jira Cloud.";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert!(urls.is_empty(), "urls: {urls:#?}");
    }

    #[test]
    fn test_find_urls_filters_code_variable_host_artifacts() {
        let text = "loginUrl = \"http://os.environ['DD_BASE_URL']/login\"";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        assert!(urls.is_empty(), "urls: {urls:#?}");
    }

    #[test]
    fn test_find_emails_ignores_file_like_domains() {
        let text = "s@index.html version@.tar.gz real@rust-lang.org";
        let config = DetectionConfig::default();
        let emails = find_emails(text, &config);

        let values: Vec<_> = emails.into_iter().map(|email| email.email).collect();
        assert_eq!(values, vec!["real@rust-lang.org".to_string()]);
    }

    #[test]
    fn test_find_urls_ignores_file_like_fake_hosts() {
        let text = "http://ftp.sftp/ http://www.classes.hint/ http://www.conf.default/ https://rust-lang.org/real";
        let config = DetectionConfig::default();
        let urls = find_urls(text, &config);

        let values: Vec<_> = urls.into_iter().map(|url| url.url).collect();
        assert_eq!(values, vec!["https://rust-lang.org/real".to_string()]);
    }
}
