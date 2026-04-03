//! Unified render metrics for pipeline tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::component::{ComponentId, ComponentMetrics};

// ============================================================================
// Unified Render Metrics
// ============================================================================

/// Complete metrics for a render operation.
/// Schema is identical between Rust and Go (no ambiguous Python mapping).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderMetrics {
    /// Per-component metrics.
    pub components: HashMap<String, ComponentMetrics>,

    /// Total render time (milliseconds).
    pub total_ms: u64,

    /// Decode time (milliseconds).
    pub decode_ms: u64,

    /// Effects time (milliseconds).
    pub effects_ms: u64,

    /// Overlay time (milliseconds).
    pub overlay_ms: u64,

    /// Audio time (milliseconds).
    pub audio_ms: u64,

    /// Encode time (milliseconds).
    pub encode_ms: u64,

    /// Concat time (milliseconds).
    pub concat_ms: u64,

    /// Probe time (milliseconds).
    pub probe_ms: u64,

    /// Number of source files processed.
    pub source_count: u32,

    /// Output file size (bytes).
    pub output_size_bytes: u64,

    /// Output duration (seconds).
    pub output_duration_secs: f64,
}

impl RenderMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get metrics for a specific component.
    pub fn get_component(&self, id: ComponentId) -> Option<&ComponentMetrics> {
        self.components.get(id.as_str())
    }

    /// Get mutable metrics for a specific component.
    pub fn get_component_mut(&mut self, id: ComponentId) -> &mut ComponentMetrics {
        self.components
            .entry(id.as_str().to_string())
            .or_insert_with(ComponentMetrics::new)
    }

    /// Record an attempt for a component.
    pub fn record_attempt(&mut self, id: ComponentId) {
        self.get_component_mut(id).record_attempt();
    }

    /// Record a success for a component.
    pub fn record_success(&mut self, id: ComponentId, elapsed_ms: u64) {
        self.get_component_mut(id).record_success(elapsed_ms);
    }

    /// Record a fallback for a component.
    pub fn record_fallback(&mut self, id: ComponentId, elapsed_ms: u64, error_code: &str) {
        self.get_component_mut(id).record_fallback(elapsed_ms, error_code);
    }

    /// Check if any component used fallback.
    pub fn has_any_fallback(&self) -> bool {
        self.components.values().any(|c| c.has_fallbacks())
    }

    /// Get all components that used fallback.
    pub fn fallback_components(&self) -> Vec<String> {
        self.components
            .iter()
            .filter(|(_, c)| c.has_fallbacks())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Sum of all stage times.
    pub fn stage_sum(&self) -> u64 {
        self.decode_ms + self.effects_ms + self.overlay_ms + self.audio_ms + self.encode_ms + self.concat_ms + self.probe_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_metrics_new() {
        let metrics = RenderMetrics::new();
        assert_eq!(metrics.total_ms, 0);
        assert_eq!(metrics.decode_ms, 0);
        assert_eq!(metrics.effects_ms, 0);
        assert_eq!(metrics.overlay_ms, 0);
        assert_eq!(metrics.audio_ms, 0);
        assert_eq!(metrics.encode_ms, 0);
        assert_eq!(metrics.concat_ms, 0);
        assert_eq!(metrics.probe_ms, 0);
        assert_eq!(metrics.source_count, 0);
        assert_eq!(metrics.output_size_bytes, 0);
        assert_eq!(metrics.output_duration_secs, 0.0);
    }

    #[test]
    fn test_render_metrics_record_attempt() {
        let mut metrics = RenderMetrics::new();
        metrics.record_attempt(ComponentId::Decode);
        let decode = metrics.get_component(ComponentId::Decode).unwrap();
        assert_eq!(decode.attempts, 1);
    }

    #[test]
    fn test_render_metrics_record_success() {
        let mut metrics = RenderMetrics::new();
        metrics.record_attempt(ComponentId::Decode);
        metrics.record_success(ComponentId::Decode, 100);

        let decode = metrics.get_component(ComponentId::Decode).unwrap();
        assert_eq!(decode.attempts, 1);
        assert_eq!(decode.successes, 1);
        assert_eq!(decode.total_ms, 100);
    }

    #[test]
    fn test_render_metrics_record_fallback() {
        let mut metrics = RenderMetrics::new();
        metrics.record_attempt(ComponentId::Overlay);
        metrics.record_fallback(ComponentId::Overlay, 50, "OVERLAY_FAILED");

        assert!(metrics.has_any_fallback());
        assert_eq!(metrics.fallback_components(), vec!["overlay"]);
    }

    #[test]
    fn test_render_metrics_stage_sum() {
        let mut metrics = RenderMetrics::new();
        metrics.decode_ms = 100;
        metrics.effects_ms = 50;
        metrics.overlay_ms = 30;
        metrics.audio_ms = 20;
        metrics.encode_ms = 80;
        metrics.concat_ms = 10;
        metrics.probe_ms = 5;

        assert_eq!(metrics.stage_sum(), 295);
    }

    #[test]
    fn test_render_metrics_get_component_mut() {
        let mut metrics = RenderMetrics::new();
        let decode = metrics.get_component_mut(ComponentId::Decode);
        decode.record_attempt();
        decode.record_success(100);

        let decode = metrics.get_component(ComponentId::Decode).unwrap();
        assert_eq!(decode.attempts, 1);
        assert_eq!(decode.successes, 1);
        assert_eq!(decode.total_ms, 100);
    }
}