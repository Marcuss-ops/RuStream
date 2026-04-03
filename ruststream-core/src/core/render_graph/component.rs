//! Component identifiers and metrics for pipeline tracking.

use serde::{Deserialize, Serialize};

// ============================================================================
// Component Identifiers
// ============================================================================

/// Identifies a pipeline component for metric tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentId {
    /// Video decode stage.
    Decode,
    /// Video effects stage.
    Effects,
    /// Overlay merge stage.
    Overlay,
    /// Audio processing stage.
    Audio,
    /// Video encode stage.
    Encode,
    /// Concatenation operation.
    Concat,
    /// Probe operation.
    Probe,
}

impl ComponentId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::Effects => "effects",
            Self::Overlay => "overlay",
            Self::Audio => "audio",
            Self::Encode => "encode",
            Self::Concat => "concat",
            Self::Probe => "probe",
        }
    }
}

// ============================================================================
// Per-Component Metrics
// ============================================================================

/// Attempt/success/fallback counters for a single component.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentMetrics {
    /// Number of times this component was attempted.
    pub attempts: u64,
    /// Number of successful completions.
    pub successes: u64,
    /// Number of fallback invocations (emergency only).
    pub fallbacks: u64,
    /// Total time spent in this component (milliseconds).
    pub total_ms: u64,
    /// Last error code (if any).
    pub last_error: Option<String>,
}

impl ComponentMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an attempt.
    pub fn record_attempt(&mut self) {
        self.attempts += 1;
    }

    /// Record a success.
    pub fn record_success(&mut self, elapsed_ms: u64) {
        self.successes += 1;
        self.total_ms += elapsed_ms;
    }

    /// Record a fallback.
    pub fn record_fallback(&mut self, elapsed_ms: u64, error_code: &str) {
        self.fallbacks += 1;
        self.total_ms += elapsed_ms;
        self.last_error = Some(error_code.to_string());
    }

    /// Check if this component has any fallbacks.
    pub fn has_fallbacks(&self) -> bool {
        self.fallbacks > 0
    }

    /// Success rate (0.0 to 1.0).
    pub fn success_rate(&self) -> f64 {
        if self.attempts == 0 {
            1.0
        } else {
            self.successes as f64 / self.attempts as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_id_as_str() {
        assert_eq!(ComponentId::Decode.as_str(), "decode");
        assert_eq!(ComponentId::Effects.as_str(), "effects");
        assert_eq!(ComponentId::Overlay.as_str(), "overlay");
        assert_eq!(ComponentId::Audio.as_str(), "audio");
        assert_eq!(ComponentId::Encode.as_str(), "encode");
        assert_eq!(ComponentId::Concat.as_str(), "concat");
        assert_eq!(ComponentId::Probe.as_str(), "probe");
    }

    #[test]
    fn test_component_metrics_new() {
        let cm = ComponentMetrics::new();
        assert_eq!(cm.attempts, 0);
        assert_eq!(cm.successes, 0);
        assert_eq!(cm.fallbacks, 0);
        assert_eq!(cm.total_ms, 0);
        assert!(cm.last_error.is_none());
    }

    #[test]
    fn test_component_metrics_record_attempt() {
        let mut cm = ComponentMetrics::new();
        cm.record_attempt();
        assert_eq!(cm.attempts, 1);
    }

    #[test]
    fn test_component_metrics_record_success() {
        let mut cm = ComponentMetrics::new();
        cm.record_success(100);
        assert_eq!(cm.successes, 1);
        assert_eq!(cm.total_ms, 100);
    }

    #[test]
    fn test_component_metrics_record_fallback() {
        let mut cm = ComponentMetrics::new();
        cm.record_fallback(50, "ERROR_CODE");
        assert_eq!(cm.fallbacks, 1);
        assert_eq!(cm.total_ms, 50);
        assert_eq!(cm.last_error, Some("ERROR_CODE".to_string()));
    }

    #[test]
    fn test_component_metrics_has_fallbacks() {
        let mut cm = ComponentMetrics::new();
        assert!(!cm.has_fallbacks());
        cm.record_fallback(50, "ERROR");
        assert!(cm.has_fallbacks());
    }

    #[test]
    fn test_component_metrics_success_rate() {
        let mut cm = ComponentMetrics::new();
        assert_eq!(cm.success_rate(), 1.0);

        cm.record_attempt();
        cm.record_success(100);
        assert_eq!(cm.success_rate(), 1.0);

        cm.record_attempt();
        cm.record_fallback(50, "ERROR");
        assert_eq!(cm.success_rate(), 0.5);
    }
}