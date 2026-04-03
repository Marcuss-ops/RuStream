//! Render configuration for pipeline execution.

use serde::{Deserialize, Serialize};

// ============================================================================
// Render Configuration
// ============================================================================

/// Execution mode for the render pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderMode {
    /// Normal mode - Rust-only processing, no fallbacks allowed.
    Normal,
    /// Emergency mode - fallbacks permitted with audit trail.
    Emergency,
}

impl Default for RenderMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Configuration for the render pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    /// Execution mode.
    pub mode: RenderMode,
    /// Maximum execution time in seconds (0 = unlimited).
    pub timeout_secs: u64,
    /// Whether to emit drift metrics in the report.
    pub emit_drift_metrics: bool,
    /// Whether to emit per-component metrics.
    pub emit_component_metrics: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            mode: RenderMode::Normal,
            timeout_secs: 0,
            emit_drift_metrics: true,
            emit_component_metrics: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_mode_default() {
        let mode = RenderMode::default();
        assert_eq!(mode, RenderMode::Normal);
    }

    #[test]
    fn test_render_mode_clone() {
        let mode = RenderMode::Emergency;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_render_config_default() {
        let config = RenderConfig::default();
        assert_eq!(config.mode, RenderMode::Normal);
        assert_eq!(config.timeout_secs, 0);
        assert!(config.emit_drift_metrics);
        assert!(config.emit_component_metrics);
    }

    #[test]
    fn test_render_config_clone() {
        let config = RenderConfig {
            mode: RenderMode::Emergency,
            timeout_secs: 300,
            emit_drift_metrics: false,
            emit_component_metrics: true,
        };
        let cloned = config.clone();
        assert_eq!(config.mode, cloned.mode);
        assert_eq!(config.timeout_secs, cloned.timeout_secs);
        assert_eq!(config.emit_drift_metrics, cloned.emit_drift_metrics);
        assert_eq!(config.emit_component_metrics, cloned.emit_component_metrics);
    }

    #[test]
    fn test_render_config_debug() {
        let config = RenderConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("RenderConfig"));
        assert!(debug_str.contains("Normal"));
    }
}