//! Debug test for GFDL-1.1 selection issue.

#[cfg(test)]
mod tests {
    use crate::license_detection::LicenseDetectionEngine;
    use crate::license_detection::models::MatcherKind;

    #[test]
    fn test_gfdl_1_1_selection() {
        let engine = LicenseDetectionEngine::from_embedded().unwrap();

        let text = r#"Copyright (c) 2020 Go Gopher.
Permission is granted to copy, distribute and/or
modify this document under the terms of the GNU Free Documentation License,
Version 1.1 published by the Free Software Foundation;
with the Invariant Sections being GCD(x, y) = GCD(a, b),
with the Front-Cover Texts being My Front Cover,
and with the Back-Cover Texts being My Back Cover. A copy of the
license is included in the section entitled "GNU Free Documentation License"."#;

        let detections = engine.detect_with_kind(text, false, false).unwrap();

        // Should detect gfdl-1.1, NOT gfdl-1.1-plus
        // The input says "Version 1.1" without "or later version"
        assert!(!detections.is_empty(), "Should have detections");

        let det = &detections[0];
        eprintln!("Detection: {:?}", det.license_expression);
        for m in &det.matches {
            eprintln!(
                "  Rule: {}, score: {}, coverage: {:.2}%",
                m.rule_identifier, m.score, m.match_coverage
            );
        }

        // The primary match should be gfdl-1.1, not gfdl-1.1-plus
        let primary_match = det
            .matches
            .iter()
            .find(|m| m.matcher == MatcherKind::Seq)
            .expect("Should have a sequence match");

        assert!(
            primary_match.license_expression.contains("gfdl-1.1")
                && !primary_match.license_expression.contains("plus"),
            "Expected gfdl-1.1, got {}",
            primary_match.license_expression
        );
    }
}
