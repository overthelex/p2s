use serde::{Deserialize, Serialize};

/// Structured verdict returned by the verification agent (§3.6 requirement 4).
/// The agent returns STRUCTURED data, not prose — injection can make a model
/// write "approved" in prose; forging structured signals is much harder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationVerdict {
    /// Set by deterministic precheck, NOT by the agent (§3.6 requirement 1).
    pub domain_verified: bool,

    /// Did the agent find evidence of copied content?
    pub copy_found: bool,

    /// Source URL if copy was detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_source_url: Option<String>,

    /// How well does the card description match what the site actually offers?
    /// 0.0 = no correspondence, 1.0 = perfect match.
    pub correspondence_score: f64,

    /// Brief structured notes from the agent (not used for decisions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AdmitDecision {
    /// Admitted with a reputation weight (0.0 - 1.0).
    Admitted { reputation_weight: f64 },

    /// Rejected — only for failed deterministic checks (§1.4).
    Rejected { reason: String },
}

/// Compute the final admit decision from the structured verdict (§3.6 requirement 5).
///
/// The agent does NOT "approve" — this code computes the decision from structured
/// fields, several of which the agent does not control.
///
/// Hard blocks are ONLY for failed deterministic checks:
/// - domain_verified = false → reject (deterministic, not judgment)
///
/// Quality/correspondence is a REPUTATION signal, not a hard block (§1.4):
/// - Low correspondence → admitted with lower reputation weight
/// - Copy found → admitted with penalty, not censored
pub fn compute_admit_decision(verdict: &VerificationVerdict) -> AdmitDecision {
    // Gate 1: deterministic domain ownership check — hard block
    if !verdict.domain_verified {
        return AdmitDecision::Rejected {
            reason: "domain ownership verification failed".into(),
        };
    }

    // Gate 2: quality signals → reputation weight, never hard block
    let mut weight = 1.0;

    // Correspondence penalty
    weight *= verdict.correspondence_score.clamp(0.0, 1.0);

    // Copy penalty: 50% reputation reduction if copy detected
    if verdict.copy_found {
        weight *= 0.5;
    }

    weight = weight.clamp(0.0, 1.0);

    AdmitDecision::Admitted { reputation_weight: weight }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_not_verified_is_hard_reject() {
        let verdict = VerificationVerdict {
            domain_verified: false,
            copy_found: false,
            copy_source_url: None,
            correspondence_score: 1.0,
            notes: None,
        };
        assert!(matches!(
            compute_admit_decision(&verdict),
            AdmitDecision::Rejected { .. }
        ));
    }

    #[test]
    fn perfect_card_gets_full_reputation() {
        let verdict = VerificationVerdict {
            domain_verified: true,
            copy_found: false,
            copy_source_url: None,
            correspondence_score: 1.0,
            notes: None,
        };
        match compute_admit_decision(&verdict) {
            AdmitDecision::Admitted { reputation_weight } => {
                assert!((reputation_weight - 1.0).abs() < f64::EPSILON);
            }
            _ => panic!("expected admitted"),
        }
    }

    #[test]
    fn low_correspondence_reduces_reputation_not_reject() {
        let verdict = VerificationVerdict {
            domain_verified: true,
            copy_found: false,
            copy_source_url: None,
            correspondence_score: 0.3,
            notes: None,
        };
        match compute_admit_decision(&verdict) {
            AdmitDecision::Admitted { reputation_weight } => {
                assert!((reputation_weight - 0.3).abs() < f64::EPSILON);
            }
            _ => panic!("low correspondence must NOT reject — §1.4"),
        }
    }

    #[test]
    fn zero_correspondence_still_admitted() {
        let verdict = VerificationVerdict {
            domain_verified: true,
            copy_found: false,
            copy_source_url: None,
            correspondence_score: 0.0,
            notes: None,
        };
        match compute_admit_decision(&verdict) {
            AdmitDecision::Admitted { reputation_weight } => {
                assert!(reputation_weight.abs() < f64::EPSILON);
            }
            _ => panic!("even zero correspondence must admit, not censor — §1.4"),
        }
    }

    #[test]
    fn copy_detected_reduces_reputation_not_reject() {
        let verdict = VerificationVerdict {
            domain_verified: true,
            copy_found: true,
            copy_source_url: Some("https://original.com".into()),
            correspondence_score: 0.8,
            notes: None,
        };
        match compute_admit_decision(&verdict) {
            AdmitDecision::Admitted { reputation_weight } => {
                assert!((reputation_weight - 0.4).abs() < f64::EPSILON);
            }
            _ => panic!("copy detection must NOT reject — §1.4"),
        }
    }

    #[test]
    fn correspondence_score_clamped() {
        let verdict = VerificationVerdict {
            domain_verified: true,
            copy_found: false,
            copy_source_url: None,
            correspondence_score: 1.5, // agent returned out-of-range
            notes: None,
        };
        match compute_admit_decision(&verdict) {
            AdmitDecision::Admitted { reputation_weight } => {
                assert!(reputation_weight <= 1.0);
            }
            _ => panic!("expected admitted"),
        }
    }

    #[test]
    fn negative_correspondence_score_clamped() {
        let verdict = VerificationVerdict {
            domain_verified: true,
            copy_found: false,
            copy_source_url: None,
            correspondence_score: -0.5, // agent returned negative
            notes: None,
        };
        match compute_admit_decision(&verdict) {
            AdmitDecision::Admitted { reputation_weight } => {
                assert!(reputation_weight >= 0.0);
            }
            _ => panic!("expected admitted"),
        }
    }
}
