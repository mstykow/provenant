//! Core detection data structures.

use crate::license_detection::models::LicenseMatch;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FileRegion {
    pub(crate) path: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct UniqueDetection {
    pub(crate) identifier: String,
    pub(crate) file_regions: Vec<FileRegion>,
}

pub struct DetectionGroup {
    /// The matches in this group
    pub matches: Vec<LicenseMatch>,
}

impl DetectionGroup {
    pub fn new(matches: Vec<LicenseMatch>) -> Self {
        Self { matches }
    }
}

/// A LicenseDetection combines one or more LicenseMatch objects using
/// various rules and heuristics.
#[derive(Debug, Clone)]
pub struct LicenseDetection {
    /// A license expression string using SPDX license expression syntax
    /// and ScanCode license keys - the effective license expression for this detection.
    pub license_expression: Option<String>,

    /// SPDX license expression string with SPDX ids only.
    pub license_expression_spdx: Option<String>,

    /// List of license matches combined in this detection.
    pub matches: Vec<LicenseMatch>,

    /// A list of detection log entries explaining how this detection was created.
    pub detection_log: Vec<String>,

    /// An identifier unique for a license detection, containing the license
    /// expression and a UUID crafted from the match contents.
    pub identifier: Option<String>,

    pub(crate) file_regions: Vec<FileRegion>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license_detection::models::PositionSpan;

    fn create_test_match(start_line: usize, end_line: usize) -> LicenseMatch {
        LicenseMatch {
            rid: 0,
            license_expression: "mit".to_string(),
            license_expression_spdx: Some("MIT".to_string()),
            from_file: Some("test.txt".to_string()),
            start_line,
            end_line,
            start_token: 0,
            end_token: 0,
            matcher: crate::license_detection::models::MatcherKind::Hash,
            score: 95.0,
            matched_length: 100,
            match_coverage: 95.0,
            rule_relevance: 100,
            rule_identifier: "mit.LICENSE".to_string(),
            rule_url: "https://example.com".to_string(),
            matched_text: Some("MIT License".to_string()),
            referenced_filenames: None,
            rule_kind: crate::license_detection::models::RuleKind::None,
            is_from_license: false,
            rule_length: 100,
            hilen: 50,
            rule_start_token: 0,
            qspan: PositionSpan::empty(),
            ispan: PositionSpan::empty(),
            hispan: PositionSpan::empty(),
            candidate_resemblance: 0.0,
            candidate_containment: 0.0,
        }
    }

    #[test]
    fn test_detection_group_new_empty() {
        let group = DetectionGroup::new(Vec::new());
        assert_eq!(group.matches.len(), 0);
    }

    #[test]
    fn test_detection_group_new_with_matches() {
        let match1 = create_test_match(1, 5);
        let match2 = create_test_match(10, 15);
        let group = DetectionGroup::new(vec![match1, match2]);

        assert_eq!(group.matches.len(), 2);
    }
}
