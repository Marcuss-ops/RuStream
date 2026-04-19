//! Audio graph configuration and types.
//!
//! This module provides the declarative audio processing graph configuration,
//! defining how multiple audio sources are mixed, processed, and synchronized.

use crate::core::errors::{MediaError, MediaErrorCode, MediaResult};

/// Audio input to the mixing graph.
#[derive(Debug, Clone)]
pub struct AudioInput {
    /// Unique identifier for this input.
    pub id: String,
    /// Path to the audio file.
    pub path: String,
    /// Type of input (e.g., "voiceover", "music", "sfx").
    pub input_type: String,
    /// Volume level (0.0 to 1.0).
    pub volume: f64,
    /// Start offset in samples.
    pub start_offset_samples: u64,
    /// Gate configurations (sections to mute).
    pub gates: Vec<AudioGate>,
}

impl AudioInput {
    /// Create a new audio input.
    pub fn new(id: impl Into<String>, path: impl Into<String>, input_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            input_type: input_type.into(),
            volume: 1.0,
            start_offset_samples: 0,
            gates: Vec::new(),
        }
    }

    /// Set the volume level.
    pub fn with_volume(mut self, volume: f64) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Set the start offset in samples.
    pub fn with_offset(mut self, offset_samples: u64) -> Self {
        self.start_offset_samples = offset_samples;
        self
    }

    /// Add a gate range.
    pub fn with_gate(mut self, gate: AudioGate) -> Self {
        self.gates.push(gate);
        self
    }
}

/// Audio gate configuration (section to mute).
#[derive(Debug, Clone)]
pub struct AudioGate {
    /// Start sample of the gate.
    pub start_sample: u64,
    /// End sample of the gate.
    pub end_sample: u64,
}

impl AudioGate {
    /// Create a new gate range.
    pub fn new(start_sample: u64, end_sample: u64) -> Self {
        Self {
            start_sample,
            end_sample,
        }
    }
}

/// Synchronization configuration for audio/video alignment.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Target sample rate.
    pub sample_rate: u32,
    /// Frame duration in samples.
    pub frame_duration_samples: u32,
    /// Maximum allowed drift in frames.
    pub max_drift_frames: f64,
    /// Whether to auto-correct drift.
    pub auto_correct_drift: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            frame_duration_samples: 1600, // ~33.3ms at 48kHz
            max_drift_frames: 1.0,
            auto_correct_drift: true,
        }
    }
}

impl SyncConfig {
    /// Calculate frame duration in samples.
    pub fn frame_duration_samples(&self) -> u32 {
        self.frame_duration_samples
    }
}

/// Audio graph configuration.
///
/// This defines the complete audio processing pipeline including:
/// - Multiple audio inputs (voiceover, music, SFX)
/// - Volume levels and offsets
/// - Gate ranges (sections to mute)
/// - Output format settings
/// - Synchronization parameters
#[derive(Debug, Clone)]
pub struct AudioGraphConfig {
    /// Unique identifier for this graph.
    pub graph_id: String,
    /// Audio inputs to mix.
    pub inputs: Vec<AudioInput>,
    /// Output sample rate.
    pub output_sample_rate: u32,
    /// Output channels (1=mono, 2=stereo).
    pub output_channels: u8,
    /// Total expected duration in samples.
    pub total_duration_samples: u64,
    /// Synchronization configuration.
    pub sync: SyncConfig,
}

impl AudioGraphConfig {
    /// Create a new audio graph configuration.
    pub fn new(graph_id: impl Into<String>) -> Self {
        Self {
            graph_id: graph_id.into(),
            inputs: Vec::new(),
            output_sample_rate: 48000,
            output_channels: 2,
            total_duration_samples: 0,
            sync: SyncConfig::default(),
        }
    }

    /// Add an audio input.
    pub fn add_input(mut self, input: AudioInput) -> Self {
        self.inputs.push(input);
        self
    }

    /// Set the output sample rate.
    pub fn with_output_sample_rate(mut self, rate: u32) -> Self {
        self.output_sample_rate = rate;
        self.sync.sample_rate = rate;
        self
    }

    /// Set the output channels.
    pub fn with_output_channels(mut self, channels: u8) -> Self {
        self.output_channels = channels;
        self
    }

    /// Set the total duration in samples.
    pub fn with_total_duration_samples(mut self, samples: u64) -> Self {
        self.total_duration_samples = samples;
        self
    }

    /// Set the sync configuration.
    pub fn with_sync(mut self, sync: SyncConfig) -> Self {
        self.sync = sync;
        self
    }

    /// Find inputs by type.
    pub fn find_inputs_by_type(&self, input_type: &str) -> Vec<&AudioInput> {
        self.inputs
            .iter()
            .filter(|input| input.input_type == input_type)
            .collect()
    }

    /// Validate the configuration.
    pub fn validate(&self) -> MediaResult<()> {
        if self.graph_id.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::AudioGraphInvalidConfig,
                "Graph ID cannot be empty",
            ));
        }

        if self.inputs.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::AudioGraphInvalidConfig,
                "At least one audio input is required",
            ));
        }

        if self.output_sample_rate == 0 {
            return Err(MediaError::new(
                MediaErrorCode::AudioGraphInvalidConfig,
                "Output sample rate cannot be zero",
            ));
        }

        if self.output_channels == 0 || self.output_channels > 2 {
            return Err(MediaError::new(
                MediaErrorCode::AudioGraphInvalidConfig,
                "Output channels must be 1 (mono) or 2 (stereo)",
            ));
        }

        // Validate input paths
        for input in &self.inputs {
            if input.path.is_empty() {
                return Err(MediaError::new(
                    MediaErrorCode::AudioGraphInvalidConfig,
                    format!("Input '{}' has empty path", input.id),
                ));
            }
        }

        Ok(())
    }
}

/// Result from audio graph processing.
#[derive(Debug, Clone)]
pub struct AudioGraphResult {
    /// Whether processing succeeded.
    pub success: bool,
    /// Output artifact path.
    pub output_path: String,
    /// Total samples processed.
    pub samples_processed: u64,
    /// Drift metrics (if sync was enabled).
    pub drift: crate::core::instrumentation::DriftMetrics,
    /// Error message if failed.
    pub error: Option<MediaError>,
}

/// Drift measurement metrics (alias for instrumentation::DriftMetrics).
pub type DriftMetrics = crate::core::instrumentation::DriftMetrics;

impl AudioGraphResult {
    /// Create a successful result.
    pub fn success(output_path: impl Into<String>, samples_processed: u64) -> Self {
        Self {
            success: true,
            output_path: output_path.into(),
            samples_processed,
            drift: DriftMetrics::new(),
            error: None,
        }
    }

    /// Create a failed result.
    pub fn failure(error: MediaError) -> Self {
        Self {
            success: false,
            output_path: String::new(),
            samples_processed: 0,
            drift: DriftMetrics::new(),
            error: Some(error),
        }
    }

    /// Set drift metrics.
    pub fn with_drift(mut self, drift: crate::core::instrumentation::DriftMetrics) -> Self {
        self.drift = drift;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_input_creation() {
        let input = AudioInput::new("vo", "/tmp/vo.mp3", "voiceover")
            .with_volume(0.8)
            .with_offset(48000);

        assert_eq!(input.id, "vo");
        assert_eq!(input.path, "/tmp/vo.mp3");
        assert_eq!(input.input_type, "voiceover");
        assert!((input.volume - 0.8).abs() < f64::EPSILON);
        assert_eq!(input.start_offset_samples, 48000);
    }

    #[test]
    fn test_audio_graph_config_basic() {
        let config = AudioGraphConfig::new("test-graph")
            .add_input(AudioInput::new("vo", "/tmp/vo.mp3", "voiceover"))
            .with_output_sample_rate(44100)
            .with_output_channels(2);

        assert_eq!(config.graph_id, "test-graph");
        assert_eq!(config.inputs.len(), 1);
        assert_eq!(config.output_sample_rate, 44100);
        assert_eq!(config.output_channels, 2);
    }

    #[test]
    fn test_audio_graph_validation_success() {
        let config = AudioGraphConfig::new("test")
            .add_input(AudioInput::new("vo", "/tmp/vo.mp3", "voiceover"));

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_audio_graph_validation_empty_id() {
        let config = AudioGraphConfig::new("")
            .add_input(AudioInput::new("vo", "/tmp/vo.mp3", "voiceover"));

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_audio_graph_validation_no_inputs() {
        let config = AudioGraphConfig::new("test");

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_audio_graph_validation_zero_sample_rate() {
        let config = AudioGraphConfig::new("test")
            .add_input(AudioInput::new("vo", "/tmp/vo.mp3", "voiceover"))
            .with_output_sample_rate(0);

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_find_inputs_by_type() {
        let config = AudioGraphConfig::new("test")
            .add_input(AudioInput::new("vo1", "/tmp/vo1.mp3", "voiceover"))
            .add_input(AudioInput::new("vo2", "/tmp/vo2.mp3", "voiceover"))
            .add_input(AudioInput::new("music", "/tmp/music.mp3", "music"));

        let voiceovers = config.find_inputs_by_type("voiceover");
        assert_eq!(voiceovers.len(), 2);

        let music = config.find_inputs_by_type("music");
        assert_eq!(music.len(), 1);

        let sfx = config.find_inputs_by_type("sfx");
        assert_eq!(sfx.len(), 0);
    }

    #[test]
    fn test_sync_config_defaults() {
        let sync = SyncConfig::default();

        assert_eq!(sync.sample_rate, 48000);
        assert_eq!(sync.frame_duration_samples, 1600);
        assert!((sync.max_drift_frames - 1.0).abs() < f64::EPSILON);
        assert!(sync.auto_correct_drift);
    }

    #[test]
    fn test_audio_graph_result_success() {
        let result = AudioGraphResult::success("/tmp/output.m4a", 480000);

        assert!(result.success);
        assert_eq!(result.output_path, "/tmp/output.m4a");
        assert_eq!(result.samples_processed, 480000);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_audio_graph_result_failure() {
        let error = MediaError::new(MediaErrorCode::AudioGraphInvalidConfig, "test error");
        let result = AudioGraphResult::failure(error.clone());

        assert!(!result.success);
        assert!(result.output_path.is_empty());
        assert_eq!(result.samples_processed, 0);
        assert!(result.error.is_some());
    }
}
