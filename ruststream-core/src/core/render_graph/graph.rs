//! Unified render graph - single API contract for all media processing.

use serde::{Deserialize, Serialize};

use crate::core::errors::{MediaError, MediaErrorCode, MediaResult};
use crate::core::timeline::MediaTimelinePlan;
use crate::core::audio_graph::AudioGraphConfig;

use super::config::{RenderConfig, RenderMode};

// ============================================================================
// Render Graph (Input Contract)
// ============================================================================

/// The unified render graph - single API contract for all media processing.
///
/// This struct replaces scattered configuration across multiple Python adapters
/// with a unified, type-safe Rust structure. It serves as the single input
/// contract for `process_render_graph()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGraph {
    /// Graph identifier (for logging/tracking).
    pub graph_id: String,

    /// Media timeline plan (video/effects/overlays).
    pub timeline: MediaTimelinePlan,

    /// Audio graph configuration (optional).
    pub audio: Option<AudioGraphConfig>,

    /// Render configuration.
    pub config: RenderConfig,
}

impl RenderGraph {
    /// Create a new render graph.
    pub fn new(graph_id: impl Into<String>, timeline: MediaTimelinePlan) -> Self {
        Self {
            graph_id: graph_id.into(),
            timeline,
            audio: None,
            config: RenderConfig::default(),
        }
    }

    /// Set audio graph configuration.
    pub fn with_audio(mut self, audio: AudioGraphConfig) -> Self {
        self.audio = Some(audio);
        self
    }

    /// Set render configuration.
    pub fn with_config(mut self, config: RenderConfig) -> Self {
        self.config = config;
        self
    }

    /// Enable emergency mode.
    pub fn with_emergency_mode(mut self) -> Self {
        self.config.mode = RenderMode::Emergency;
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self
    }

    /// Validate the render graph before execution.
    pub fn validate(&self) -> MediaResult<()> {
        if self.graph_id.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::TimelineInvalidPlan,
                "RenderGraph ID cannot be empty",
            ));
        }

        // Validate timeline
        self.timeline.validate()?;

        // Validate audio if present
        if let Some(audio) = &self.audio {
            audio.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::timeline::{VideoTrack, VideoSegment, OutputConfig};

    fn create_test_graph() -> RenderGraph {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });

        RenderGraph::new("test-graph", timeline)
    }

    #[test]
    fn test_render_graph_new() {
        let graph = create_test_graph();
        assert_eq!(graph.graph_id, "test-graph");
        assert!(graph.audio.is_none());
        assert_eq!(graph.config.mode, RenderMode::Normal);
    }

    #[test]
    fn test_render_graph_with_audio() {
        let graph = create_test_graph();
        let audio_config = AudioGraphConfig::new("test-audio");
        let graph = graph.with_audio(audio_config);
        assert!(graph.audio.is_some());
    }

    #[test]
    fn test_render_graph_with_config() {
        let graph = create_test_graph();
        let config = RenderConfig {
            mode: RenderMode::Emergency,
            timeout_secs: 300,
            emit_drift_metrics: false,
            emit_component_metrics: true,
        };
        let graph = graph.with_config(config);
        assert_eq!(graph.config.mode, RenderMode::Emergency);
        assert_eq!(graph.config.timeout_secs, 300);
        assert!(!graph.config.emit_drift_metrics);
    }

    #[test]
    fn test_render_graph_with_emergency_mode() {
        let graph = create_test_graph();
        let graph = graph.with_emergency_mode();
        assert_eq!(graph.config.mode, RenderMode::Emergency);
    }

    #[test]
    fn test_render_graph_with_timeout() {
        let graph = create_test_graph();
        let graph = graph.with_timeout(600);
        assert_eq!(graph.config.timeout_secs, 600);
    }

    #[test]
    fn test_render_graph_validate() {
        let graph = create_test_graph();
        // Structure is valid - file existence is checked in probe_sources(), not validate()
        assert!(graph.validate().is_ok());
    }

    #[test]
    fn test_render_graph_empty_id() {
        let timeline = MediaTimelinePlan::new("test")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });

        let graph = RenderGraph::new("", timeline);
        assert!(graph.validate().is_err());
    }

    #[test]
    fn test_render_graph_clone() {
        let graph = create_test_graph();
        let cloned = graph.clone();
        assert_eq!(graph.graph_id, cloned.graph_id);
    }

    #[test]
    fn test_render_graph_debug() {
        let graph = create_test_graph();
        let debug_str = format!("{:?}", graph);
        assert!(debug_str.contains("RenderGraph"));
        assert!(debug_str.contains("test-graph"));
    }
}