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

pub struct DetectionGroup<'a> {
    /// The matches in this group
    pub matches: Vec<LicenseMatch<'a>>,
}

impl<'a> DetectionGroup<'a> {
    pub fn new(matches: Vec<LicenseMatch<'a>>) -> Self {
        Self { matches }
    }
}

/// A LicenseDetection combines one or more LicenseMatch objects using
/// various rules and heuristics.
#[derive(Debug, Clone)]
pub struct LicenseDetection<'a> {
    /// A license expression string using SPDX license expression syntax
    /// and ScanCode license keys - the effective license expression for this detection.
    pub license_expression: Option<String>,

    /// SPDX license expression string with SPDX ids only.
    pub license_expression_spdx: Option<String>,

    /// List of license matches combined in this detection.
    pub matches: Vec<LicenseMatch<'a>>,

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
    use crate::license_detection::tests::TestMatchBuilder;

    fn create_test_match(start_line: usize, end_line: usize) -> LicenseMatch<'static> {
        TestMatchBuilder::default()
            .license_expression("mit")
            .license_expression_spdx(Some("MIT".to_string()))
            .from_file(Some("test.txt".to_string()))
            .start_line(start_line)
            .end_line(end_line)
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
    fn test_detection_group_new_empty() {
        let group: DetectionGroup<'static> = DetectionGroup::new(Vec::new());
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
