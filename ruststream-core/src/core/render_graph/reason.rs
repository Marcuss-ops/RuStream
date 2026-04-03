//! Machine-readable reason codes for render outcomes.

use serde::{Deserialize, Serialize};

// ============================================================================
// Reason Codes
// ============================================================================

/// Machine-readable reason codes for render outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonCode {
    /// Render completed successfully.
    Success,
    /// Render completed with emergency fallback.
    SuccessWithFallback,
    /// Decode stage failed.
    DecodeFailed,
    /// Effects stage failed.
    EffectsFailed,
    /// Overlay stage failed.
    OverlayFailed,
    /// Audio stage failed.
    AudioFailed,
    /// Encode stage failed.
    EncodeFailed,
    /// Concat stage failed.
    ConcatFailed,
    /// Probe stage failed.
    ProbeFailed,
    /// Input validation failed.
    ValidationFailed,
    /// Timeout exceeded.
    Timeout,
    /// Resource exhausted (memory, disk, etc.).
    ResourceExhausted,
    /// Emergency fallback triggered.
    EmergencyFallback,
}

impl ReasonCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::SuccessWithFallback => "SUCCESS_WITH_FALLBACK",
            Self::DecodeFailed => "DECODE_FAILED",
            Self::EffectsFailed => "EFFECTS_FAILED",
            Self::OverlayFailed => "OVERLAY_FAILED",
            Self::AudioFailed => "AUDIO_FAILED",
            Self::EncodeFailed => "ENCODE_FAILED",
            Self::ConcatFailed => "CONCAT_FAILED",
            Self::ProbeFailed => "PROBE_FAILED",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::Timeout => "TIMEOUT",
            Self::ResourceExhausted => "RESOURCE_EXHAUSTED",
            Self::EmergencyFallback => "EMERGENCY_FALLBACK",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reason_code_as_str() {
        assert_eq!(ReasonCode::Success.as_str(), "SUCCESS");
        assert_eq!(ReasonCode::SuccessWithFallback.as_str(), "SUCCESS_WITH_FALLBACK");
        assert_eq!(ReasonCode::DecodeFailed.as_str(), "DECODE_FAILED");
        assert_eq!(ReasonCode::EffectsFailed.as_str(), "EFFECTS_FAILED");
        assert_eq!(ReasonCode::OverlayFailed.as_str(), "OVERLAY_FAILED");
        assert_eq!(ReasonCode::AudioFailed.as_str(), "AUDIO_FAILED");
        assert_eq!(ReasonCode::EncodeFailed.as_str(), "ENCODE_FAILED");
        assert_eq!(ReasonCode::ConcatFailed.as_str(), "CONCAT_FAILED");
        assert_eq!(ReasonCode::ProbeFailed.as_str(), "PROBE_FAILED");
        assert_eq!(ReasonCode::ValidationFailed.as_str(), "VALIDATION_FAILED");
        assert_eq!(ReasonCode::Timeout.as_str(), "TIMEOUT");
        assert_eq!(ReasonCode::ResourceExhausted.as_str(), "RESOURCE_EXHAUSTED");
        assert_eq!(ReasonCode::EmergencyFallback.as_str(), "EMERGENCY_FALLBACK");
    }

    #[test]
    fn test_reason_code_clone() {
        let code = ReasonCode::Success;
        let cloned = code.clone();
        assert_eq!(code, cloned);
    }

    #[test]
    fn test_reason_code_debug() {
        let code = ReasonCode::Success;
        let debug_str = format!("{:?}", code);
        assert_eq!(debug_str, "Success");
    }
}