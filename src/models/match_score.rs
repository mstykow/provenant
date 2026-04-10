use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MatchScore(f64);

impl MatchScore {
    pub const MAX: MatchScore = MatchScore(100.0);

    pub const GOOD_THRESHOLD: f64 = 80.0;

    pub fn from_rounded_percentage(value: f32) -> Self {
        MatchScore(((value as f64) * 100.0).round() / 100.0)
    }

    pub fn is_good(self) -> bool {
        self.0 >= Self::GOOD_THRESHOLD
    }

    pub fn to_f32_lossy(self) -> f32 {
        self.0 as f32
    }
}

impl fmt::Display for MatchScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

impl PartialOrd for MatchScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_rounded_percentage() {
        let score = MatchScore::from_rounded_percentage(95.5f32);
        assert!(score.is_good());
    }

    #[test]
    fn test_constants() {
        assert!(MatchScore::MAX.is_good());
    }

    #[test]
    fn test_is_good() {
        assert!(!MatchScore::from_rounded_percentage(79.9f32).is_good());
        assert!(MatchScore::from_rounded_percentage(80.0f32).is_good());
        assert!(MatchScore::MAX.is_good());
    }

    #[test]
    fn test_serde_roundtrip() {
        let score = MatchScore::from_rounded_percentage(95.5f32);
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(json, "95.5");
        let deserialized: MatchScore = serde_json::from_str(&json).unwrap();
        assert_eq!(score, deserialized);
    }

    #[test]
    fn test_display() {
        assert_eq!(
            format!("{}", MatchScore::from_rounded_percentage(95.5f32)),
            "95.50"
        );
        assert_eq!(format!("{}", MatchScore::MAX), "100.00");
    }

    #[test]
    fn test_to_f32_lossy() {
        let score = MatchScore::from_rounded_percentage(95.5f32);
        let f32_val = score.to_f32_lossy();
        assert!((f32_val - 95.5f32).abs() < 0.01);
    }
}
