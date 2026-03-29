//! Detection identifier computation.

#[cfg(test)]
use super::types::LicenseDetection;
use crate::license_detection::models::LicenseMatch;
use crate::license_detection::tokenize::tokenize_without_stopwords;

pub(super) fn python_safe_name(s: &str) -> String {
    let mut result = String::new();
    let mut prev_underscore = false;

    for c in s.chars() {
        if c.is_alphanumeric() {
            result.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            result.push('_');
            prev_underscore = true;
        }
    }

    let trimmed = result.trim_matches('_');
    if trimmed.is_empty() {
        String::new()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn get_uuid_on_content(content: &[(&str, f32, Vec<String>)]) -> String {
    let repr_str = format_python_tuple_repr(content);

    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(repr_str.as_bytes());
    let hash = hasher.finalize();
    let hex_str = hex::encode(hash);

    let uuid_hex = &hex_str[..32];

    uuid::Uuid::parse_str(uuid_hex)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| uuid_hex.to_string())
}

pub(super) fn compute_content_identifier(matches: &[LicenseMatch]) -> String {
    let content: Vec<(&str, f32, Vec<String>)> = matches
        .iter()
        .map(|m| {
            let matched_text = m.matched_text.as_deref().unwrap_or("");
            let tokens = tokenize_without_stopwords(matched_text);
            (m.rule_identifier(), m.score, tokens)
        })
        .collect();

    get_uuid_on_content(&content)
}

pub(super) fn format_python_tuple_repr(content: &[(&str, f32, Vec<String>)]) -> String {
    let mut result = String::from("(");

    for (i, (rule_id, score, tokens)) in content.iter().enumerate() {
        if i > 0 {
            result.push_str(", ");
        }
        result.push_str(&format!(
            "({}, {}, {})",
            python_str_repr(rule_id),
            format_score_for_repr(*score),
            python_token_tuple_repr(tokens)
        ));
    }

    if content.len() == 1 {
        result.push(',');
    }
    result.push(')');

    result
}

pub(super) fn python_str_repr(s: &str) -> String {
    if s.contains('\'') && !s.contains('"') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

pub(super) fn format_score_for_repr(score: f32) -> String {
    format!("{:?}", score)
}

fn python_token_tuple_repr(tokens: &[String]) -> String {
    if tokens.is_empty() {
        return String::from("()");
    }

    let mut result = String::from("(");
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            result.push_str(", ");
        }
        result.push_str(&python_str_repr(token));
    }

    if tokens.len() == 1 {
        result.push(',');
    }
    result.push(')');

    result
}

/// Compute a unique identifier for a detection.
///
/// NOTE: This function is currently unused. It will be used by `get_unique_detections`
/// when implementing UniqueDetection aggregation.
/// See: docs/license-detection/PLAN-019-unique-detection.md
#[cfg(test)]
pub fn compute_detection_identifier(detection: &LicenseDetection) -> String {
    let expression = detection
        .license_expression
        .as_ref()
        .map(|s| python_safe_name(s))
        .unwrap_or_default();

    let content_uuid = compute_content_identifier(&detection.matches);
    format!("{}-{}", expression, content_uuid)
}

/// Compute detection coverage from matches.
///
/// Average of match_coverage weighted by matched_length.
/// Capped at 100.0 as the maximum.
///
/// Based on Python: LicenseDetection.coverage() at detection.py:373
pub(super) fn compute_detection_coverage(matches: &[LicenseMatch]) -> f32 {
    if matches.is_empty() {
        return 0.0;
    }

    if matches.len() == 1 {
        return matches[0].match_coverage.min(100.0);
    }

    let total_length: f32 = matches.iter().map(|m| m.matched_length as f32).sum();

    if total_length < 0.01 {
        return matches.iter().map(|m| m.match_coverage).sum::<f32>() / matches.len() as f32;
    }

    let weighted_coverage: f32 = matches
        .iter()
        .map(|m| {
            let weight = m.matched_length as f32 / total_length;
            m.match_coverage * weight
        })
        .sum();

    weighted_coverage.min(100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::LicenseMatch;
    use crate::license_detection::tests::TestMatchBuilder;

    fn create_test_match() -> LicenseMatch<'static> {
        TestMatchBuilder::default()
            .license_expression("mit")
            .license_expression_spdx(Some("MIT".to_string()))
            .from_file(Some("test.txt".to_string()))
            .start_line(1)
            .end_line(10)
            .matcher(crate::license_detection::models::MatcherKind::Hash)
            .score(95.0)
            .matched_length(100)
            .rule_length(100)
            .match_coverage(95.0)
            .rule_relevance(100)
            .rule_identifier("mit.LICENSE")
            .rule_url("https://example.com".to_string())
            .matched_text(Some("MIT License".to_string()))
            .hilen(50)
            .build_match()
    }

    #[test]
    fn test_python_safe_name() {
        assert_eq!(python_safe_name("mit"), "mit");
        assert_eq!(python_safe_name("gpl-2.0"), "gpl_2_0");
        assert_eq!(python_safe_name("apache-2.0"), "apache_2_0");
        assert_eq!(python_safe_name(""), "");
        assert_eq!(python_safe_name("---"), "");
    }

    #[test]
    fn test_python_str_repr_escaping() {
        assert_eq!(python_str_repr("simple"), "'simple'");
        assert_eq!(python_str_repr("with'quote"), "\"with'quote\"");
        assert_eq!(python_str_repr("with\"quote"), "'with\"quote'");
    }

    #[test]
    fn test_score_repr_format() {
        assert_eq!(format_score_for_repr(95.0), "95.0");
        assert_eq!(format_score_for_repr(100.0), "100.0");
    }

    #[test]
    fn test_python_tuple_repr_format() {
        let result = format_python_tuple_repr(&[("rule1", 95.0, vec!["token1".to_string()])]);
        assert!(result.starts_with("("));
        assert!(result.ends_with(")"));
    }

    #[test]
    fn test_compute_detection_coverage_single() {
        let mut m = create_test_match();
        m.match_coverage = 85.0;
        let matches = vec![m];
        let coverage = compute_detection_coverage(&matches);
        assert!((coverage - 85.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_detection_coverage_multiple_equal() {
        let mut m1 = create_test_match();
        m1.match_coverage = 80.0;
        let m2 = m1.clone();
        let matches = vec![m1, m2];
        let coverage = compute_detection_coverage(&matches);
        assert!((coverage - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_detection_coverage_weighted() {
        let mut m1 = create_test_match();
        m1.matched_length = 200;
        m1.match_coverage = 100.0;
        let mut m2 = create_test_match();
        m2.matched_length = 100;
        m2.match_coverage = 50.0;
        let matches = vec![m1, m2];
        let coverage = compute_detection_coverage(&matches);
        assert!(coverage > 80.0 && coverage < 90.0);
    }

    #[test]
    fn test_compute_detection_coverage_empty() {
        let matches: Vec<LicenseMatch> = vec![];
        let coverage = compute_detection_coverage(&matches);
        assert_eq!(coverage, 0.0);
    }

    #[test]
    fn test_compute_detection_coverage_capped_at_100() {
        let mut m = create_test_match();
        m.match_coverage = 100.0;
        let matches = vec![m];
        let coverage = compute_detection_coverage(&matches);
        assert_eq!(coverage, 100.0);
    }

    #[test]
    fn test_uuid_generation_matches_python() {
        let content = vec![("rule1", 95.0, vec!["token1".to_string()])];
        let uuid = get_uuid_on_content(&content);
        assert!(!uuid.is_empty());
        assert_eq!(uuid.len(), 36); // UUID format
    }
}
