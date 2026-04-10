use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MatchScore(f64);

impl MatchScore {
    pub const ZERO: MatchScore = MatchScore(0.0);
    pub const PERFECT: MatchScore = MatchScore(100.0);

    pub fn new_unchecked(value: f64) -> Self {
        MatchScore(value)
    }

    pub fn is_good(self) -> bool {
        self.0 >= 80.0
    }

    pub fn is_perfect(self) -> bool {
        self.0 >= 100.0 - f64::EPSILON
    }

    pub fn is_zero(self) -> bool {
        self.0 <= f64::EPSILON
    }
}

impl From<f64> for MatchScore {
    fn from(value: f64) -> Self {
        MatchScore(value)
    }
}

impl From<MatchScore> for f64 {
    fn from(score: MatchScore) -> f64 {
        score.0
    }
}

impl fmt::Display for MatchScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

impl std::ops::Add for MatchScore {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        MatchScore(self.0 + rhs.0)
    }
}

impl std::ops::Sub for MatchScore {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        MatchScore(self.0 - rhs.0)
    }
}

impl std::ops::Mul<f64> for MatchScore {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        MatchScore(self.0 * rhs)
    }
}

impl PartialOrd for MatchScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl PartialEq<f64> for MatchScore {
    fn eq(&self, other: &f64) -> bool {
        (self.0 - *other).abs() < f64::EPSILON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_unchecked() {
        let score = MatchScore::new_unchecked(95.5);
        assert!((f64::from(score) - 95.5).abs() < 0.001);
    }

    #[test]
    fn test_from_f64() {
        let score = MatchScore::from(95.5);
        assert!((f64::from(score) - 95.5).abs() < 0.001);
    }

    #[test]
    fn test_constants() {
        assert!(MatchScore::ZERO.is_zero());
        assert!(MatchScore::PERFECT.is_perfect());
    }

    #[test]
    fn test_is_good() {
        assert!(!MatchScore::new_unchecked(79.9).is_good());
        assert!(MatchScore::new_unchecked(80.0).is_good());
        assert!(MatchScore::new_unchecked(100.0).is_good());
    }

    #[test]
    fn test_serde_roundtrip() {
        let score = MatchScore::new_unchecked(95.5);
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(json, "95.5");
        let deserialized: MatchScore = serde_json::from_str(&json).unwrap();
        assert_eq!(score, deserialized);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", MatchScore::new_unchecked(95.5)), "95.50");
        assert_eq!(format!("{}", MatchScore::new_unchecked(100.0)), "100.00");
    }

    #[test]
    fn test_from_to_f64() {
        let score = MatchScore::new_unchecked(95.0);
        let value: f64 = score.into();
        assert!((value - 95.0).abs() < 0.001);
    }
}
