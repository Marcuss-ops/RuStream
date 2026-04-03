//! Unified render result - single output contract for all media processing.

use serde::{Deserialize, Serialize};

use crate::core::errors::{MediaError, DriftMetrics};

use super::metrics::RenderMetrics;
use super::reason::ReasonCode;

// ============================================================================
// Render Result (Output Contract)
// ============================================================================

/// The unified render result - single output contract for all media processing.
///
/// This struct provides a consistent output schema between Rust and Go,
/// with artifact path, metrics, reason codes, and drift metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    /// Whether the render completed successfully.
    pub success: bool,

    /// Output artifact path (if successful).
    pub artifact_path: Option<String>,

    /// Complete render metrics.
    pub metrics: RenderMetrics,

    /// Drift correction metrics (always emitted when emit_drift_metrics=true).
    pub drift: DriftMetrics,

    /// Machine-readable reason codes.
    pub reason_codes: Vec<String>,

    /// Human-readable error message (if failed).
    pub error_message: Option<String>,

    /// Error code (if failed).
    pub error_code: Option<String>,

    /// Stage where the error occurred (if failed).
    pub error_stage: Option<String>,

    /// Whether emergency fallback was used.
    pub fallback_used: bool,

    /// Components that used fallback.
    pub fallback_components: Vec<String>,
}

impl RenderResult {
    /// Create a successful result.
    pub fn success(artifact_path: impl Into<String>) -> Self {
        Self {
            success: true,
            artifact_path: Some(artifact_path.into()),
            metrics: RenderMetrics::new(),
            drift: DriftMetrics::new(),
            reason_codes: vec![ReasonCode::Success.as_str().to_string()],
            error_message: None,
            error_code: None,
            error_stage: None,
            fallback_used: false,
            fallback_components: Vec::new(),
        }
    }

    /// Create a failed result.
    pub fn failure(error: MediaError) -> Self {
        Self {
            success: false,
            artifact_path: None,
            metrics: RenderMetrics::new(),
            drift: DriftMetrics::new(),
            reason_codes: vec![error.code.as_str().to_string()],
            error_message: Some(error.message.clone()),
            error_code: Some(error.code.as_str().to_string()),
            error_stage: error.stage.clone(),
            fallback_used: error.fallback_triggered,
            fallback_components: Vec::new(),
        }
    }

    /// Set metrics.
    pub fn with_metrics(mut self, metrics: RenderMetrics) -> Self {
        self.metrics = metrics;
        self
    }

    /// Set drift metrics.
    pub fn with_drift(mut self, drift: DriftMetrics) -> Self {
        self.drift = drift;
        self
    }

    /// Add a reason code.
    pub fn with_reason_code(mut self, code: ReasonCode) -> Self {
        self.reason_codes.push(code.as_str().to_string());
        self
    }

    /// Mark as using emergency fallback.
    pub fn with_fallback(mut self, components: Vec<String>) -> Self {
        self.fallback_used = true;
        self.fallback_components = components;
        if !self.reason_codes.contains(&ReasonCode::EmergencyFallback.as_str().to_string()) {
            self.reason_codes.push(ReasonCode::EmergencyFallback.as_str().to_string());
        }
        self
    }

    /// Check if the result indicates a clean success (no fallbacks).
    pub fn is_clean_success(&self) -> bool {
        self.success && !self.fallback_used && !self.metrics.has_any_fallback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::errors::MediaErrorCode;

    #[test]
    fn test_render_result_success() {
        let result = RenderResult::success("/tmp/output.mp4");
        assert!(result.success);
        assert_eq!(result.artifact_path, Some("/tmp/output.mp4".to_string()));
        assert!(!result.fallback_used);
        assert!(result.error_message.is_none());
        assert!(result.error_code.is_none());
        assert!(result.error_stage.is_none());
    }

    #[test]
    fn test_render_result_failure() {
        let error = MediaError::new(MediaErrorCode::DecodeFailed, "Decode failed");
        let result = RenderResult::failure(error);
        assert!(!result.success);
        assert!(result.artifact_path.is_none());
        assert_eq!(result.error_message, Some("Decode failed".to_string()));
        assert_eq!(result.error_code, Some("DECODE_FAILED".to_string()));
    }

    #[test]
    fn test_render_result_with_metrics() {
        let mut metrics = RenderMetrics::new();
        metrics.total_ms = 1000;
        let result = RenderResult::success("/tmp/output.mp4").with_metrics(metrics);
        assert_eq!(result.metrics.total_ms, 1000);
    }

    #[test]
    fn test_render_result_with_drift() {
        let drift = DriftMetrics::new();
        let result = RenderResult::success("/tmp/output.mp4").with_drift(drift);
        assert_eq!(result.drift.drift_frames_max, 0.0);
    }

    #[test]
    fn test_render_result_with_reason_code() {
        let result = RenderResult::success("/tmp/output.mp4")
            .with_reason_code(ReasonCode::SuccessWithFallback);
        assert!(result.reason_codes.contains(&"SUCCESS_WITH_FALLBACK".to_string()));
    }

    #[test]
    fn test_render_result_with_fallback() {
        let result = RenderResult::success("/tmp/output.mp4")
            .with_fallback(vec!["overlay".to_string()]);
        assert!(result.fallback_used);
        assert_eq!(result.fallback_components, vec!["overlay"]);
        assert!(result.reason_codes.contains(&"EMERGENCY_FALLBACK".to_string()));
    }

    #[test]
    fn test_render_result_is_clean_success() {
        let result = RenderResult::success("/tmp/output.mp4");
        assert!(result.is_clean_success());
    }

    #[test]
    fn test_render_result_is_not_clean_success_with_fallback() {
        let result = RenderResult::success("/tmp/output.mp4")
            .with_fallback(vec!["overlay".to_string()]);
        assert!(!result.is_clean_success());
        assert!(result.success);
    }

    #[test]
    fn test_render_result_clone() {
        let result = RenderResult::success("/tmp/output.mp4");
        let cloned = result.clone();
        assert_eq!(result.success, cloned.success);
        assert_eq!(result.artifact_path, cloned.artifact_path);
    }

    #[test]
    fn test_render_result_debug() {
        let result = RenderResult::success("/tmp/output.mp4");
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("RenderResult"));
        assert!(debug_str.contains("true"));
    }
}