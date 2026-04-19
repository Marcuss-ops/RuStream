//! Timeline data structures for the unified media pipeline.
//!
//! This module defines the `MediaTimelinePlan` and related types that serve as
//! the single source of truth for decode/effects/overlay operations.
//! All time values use sample-accurate representation where applicable.

use serde::{Deserialize, Serialize};
use crate::core::errors::{MediaErrorCode, MediaError, MediaResult};

/// Timebase for rational time representation (num/den).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timebase {
    pub num: u32,
    pub den: u32,
}

impl Timebase {
    /// Create a new timebase.
    pub fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    /// Common 1/1000 timebase (milliseconds).
    pub fn milliseconds() -> Self {
        Self { num: 1, den: 1000 }
    }

    /// Common 1/44100 timebase (44.1kHz audio samples).
    pub fn audio_44100() -> Self {
        Self { num: 1, den: 44100 }
    }

    /// Common 1/48000 timebase (48kHz audio samples).
    pub fn audio_48000() -> Self {
        Self { num: 1, den: 48000 }
    }

    /// Convert a timestamp in this timebase to seconds.
    pub fn to_seconds(&self, timestamp: i64) -> f64 {
        (timestamp as f64 * self.num as f64) / self.den as f64
    }

    /// Convert seconds to a timestamp in this timebase.
    pub fn from_seconds(&self, seconds: f64) -> i64 {
        (seconds * self.den as f64 / self.num as f64).round() as i64
    }

    /// Validate that the timebase has non-zero denominator.
    pub fn validate(&self) -> MediaResult<()> {
        if self.den == 0 {
            return Err(MediaError::new(
                MediaErrorCode::TimelineInvalidTimebase,
                "Timebase denominator cannot be zero",
            ));
        }
        if self.num == 0 {
            return Err(MediaError::new(
                MediaErrorCode::TimelineInvalidTimebase,
                "Timebase numerator cannot be zero",
            ));
        }
        Ok(())
    }
}

impl Default for Timebase {
    fn default() -> Self {
        Self { num: 1, den: 1000 }
    }
}

/// SIMD optimization level for CPU-bound operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum SimdLevel {
    /// Auto-detect best SIMD level for the current CPU.
    #[default]
    Auto,
    /// AVX2 (256-bit SIMD).
    Avx2,
    /// AVX-512 (512-bit SIMD).
    Avx512,
}


/// Thread allocation for pipeline stages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ThreadConfig {
    /// Number of threads for decode operations.
    pub decode_threads: u32,
    /// Number of threads for filter/effects operations.
    pub filter_threads: u32,
    /// Number of threads for encode operations.
    pub encode_threads: u32,
}

impl Default for ThreadConfig {
    fn default() -> Self {
        Self {
            decode_threads: 4,
            filter_threads: 4,
            encode_threads: 4,
        }
    }
}

/// A single video segment with source path and timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSegment {
    /// Source file path.
    pub path: String,
    /// Start time in the source file (seconds).
    pub source_start: f64,
    /// Duration to use from source (seconds). 0 = use full duration.
    pub duration: f64,
    /// Start time in the output timeline (seconds).
    pub timeline_start: f64,
}

impl VideoSegment {
    /// Create a new video segment.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source_start: 0.0,
            duration: 0.0,
            timeline_start: 0.0,
        }
    }

    /// Set source start time.
    pub fn with_source_start(mut self, start: f64) -> Self {
        self.source_start = start;
        self
    }

    /// Set duration.
    pub fn with_duration(mut self, duration: f64) -> Self {
        self.duration = duration;
        self
    }

    /// Set timeline start time.
    pub fn with_timeline_start(mut self, start: f64) -> Self {
        self.timeline_start = start;
        self
    }
}

/// A video track containing ordered segments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoTrack {
    /// Track identifier.
    pub id: String,
    /// Ordered list of video segments.
    pub segments: Vec<VideoSegment>,
    /// Whether this is the primary (main) video track.
    pub is_primary: bool,
}

impl VideoTrack {
    /// Create a new video track.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            segments: Vec::new(),
            is_primary: false,
        }
    }

    /// Mark this as the primary track.
    pub fn as_primary(mut self) -> Self {
        self.is_primary = true;
        self
    }

    /// Add a segment to this track.
    pub fn add_segment(mut self, segment: VideoSegment) -> Self {
        self.segments.push(segment);
        self
    }

    /// Validate the track has at least one segment.
    pub fn validate(&self) -> MediaResult<()> {
        if self.segments.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::TimelineEmptyTracks,
                format!("Video track '{}' has no segments", self.id),
            ));
        }
        Ok(())
    }
}

/// An effect node to apply to video frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectNode {
    /// Effect type identifier (e.g., "blur", "crop", "scale", "color_correct").
    pub effect_type: String,
    /// Start time in timeline (seconds). None = from beginning.
    pub start_time: Option<f64>,
    /// End time in timeline (seconds). None = until end.
    pub end_time: Option<f64>,
    /// Effect parameters as key-value pairs.
    pub params: std::collections::HashMap<String, serde_json::Value>,
}

impl EffectNode {
    /// Create a new effect node.
    pub fn new(effect_type: impl Into<String>) -> Self {
        Self {
            effect_type: effect_type.into(),
            start_time: None,
            end_time: None,
            params: std::collections::HashMap::new(),
        }
    }

    /// Set the time range for this effect.
    pub fn with_time_range(mut self, start: f64, end: f64) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Add a parameter.
    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }
}

/// An overlay to apply on top of the video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayTrack {
    /// Overlay asset path (image or video).
    pub path: String,
    /// Start time in timeline (seconds).
    pub start_time: f64,
    /// Duration in timeline (seconds).
    pub duration: f64,
    /// X position (pixels from left).
    pub x: i32,
    /// Y position (pixels from top).
    pub y: i32,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
    /// Blend mode (e.g., "normal", "multiply", "screen").
    pub blend_mode: String,
}

impl OverlayTrack {
    /// Create a new overlay track.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            start_time: 0.0,
            duration: 0.0,
            x: 0,
            y: 0,
            opacity: 1.0,
            blend_mode: "normal".to_string(),
        }
    }

    /// Set position.
    pub fn with_position(mut self, x: i32, y: i32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    /// Set time range.
    pub fn with_time_range(mut self, start: f64, duration: f64) -> Self {
        self.start_time = start;
        self.duration = duration;
        self
    }

    /// Set opacity.
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Validate the overlay has valid bounds.
    pub fn validate(&self) -> MediaResult<()> {
        if self.duration <= 0.0 {
            return Err(MediaError::new(
                MediaErrorCode::OverlayInvalidBounds,
                format!("Overlay '{}' has invalid duration: {}", self.path, self.duration),
            ));
        }
        if self.start_time < 0.0 {
            return Err(MediaError::new(
                MediaErrorCode::OverlayInvalidBounds,
                format!("Overlay '{}' has negative start time: {}", self.path, self.start_time),
            ));
        }
        Ok(())
    }
}

/// Output format configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Output file path.
    pub path: String,
    /// Video width.
    pub width: u32,
    /// Video height.
    pub height: u32,
    /// Output FPS.
    pub fps: f64,
    /// Video CRF (quality, lower = better).
    pub crf: u32,
    /// Video codec (e.g., "libx264", "h264_nvenc").
    pub video_codec: String,
    /// Audio codec (e.g., "aac", "libopus").
    pub audio_codec: String,
    /// Audio bitrate (e.g., "192k").
    pub audio_bitrate: String,
    /// Audio sample rate.
    pub sample_rate: u32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            crf: 22,
            video_codec: "libx264".to_string(),
            audio_codec: "aac".to_string(),
            audio_bitrate: "192k".to_string(),
            sample_rate: 44100,
        }
    }
}

/// The complete media timeline plan - the single source of truth for the pipeline.
///
/// This struct replaces scattered configuration across multiple Python adapters
/// with a unified, type-safe Rust structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTimelinePlan {
    /// Plan identifier (for logging/tracking).
    pub plan_id: String,

    /// Video tracks (ordered by priority).
    pub video_tracks: Vec<VideoTrack>,

    /// Effects to apply to the video.
    pub effects: Vec<EffectNode>,

    /// Overlays to apply on top.
    pub overlays: Vec<OverlayTrack>,

    /// Output configuration.
    pub output: OutputConfig,

    /// Thread allocation for pipeline stages.
    pub threads: ThreadConfig,

    /// SIMD optimization level.
    pub simd_level: SimdLevel,

    /// Timebase for the timeline.
    pub timebase: Timebase,

    /// Whether to allow intermediate disk files (false = RAM only).
    pub allow_intermediate_files: bool,

    /// Emergency fallback enabled (for audit).
    pub emergency_fallback_enabled: bool,
}

impl MediaTimelinePlan {
    /// Create a new timeline plan with the given ID.
    pub fn new(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            video_tracks: Vec::new(),
            effects: Vec::new(),
            overlays: Vec::new(),
            output: OutputConfig::default(),
            threads: ThreadConfig::default(),
            simd_level: SimdLevel::default(),
            timebase: Timebase::default(),
            allow_intermediate_files: false,
            emergency_fallback_enabled: false,
        }
    }

    /// Add a video track.
    pub fn add_video_track(mut self, track: VideoTrack) -> Self {
        self.video_tracks.push(track);
        self
    }

    /// Add an effect.
    pub fn add_effect(mut self, effect: EffectNode) -> Self {
        self.effects.push(effect);
        self
    }

    /// Add an overlay.
    pub fn add_overlay(mut self, overlay: OverlayTrack) -> Self {
        self.overlays.push(overlay);
        self
    }

    /// Set output configuration.
    pub fn with_output(mut self, output: OutputConfig) -> Self {
        self.output = output;
        self
    }

    /// Set thread configuration.
    pub fn with_threads(mut self, threads: ThreadConfig) -> Self {
        self.threads = threads;
        self
    }

    /// Set SIMD level.
    pub fn with_simd_level(mut self, level: SimdLevel) -> Self {
        self.simd_level = level;
        self
    }

    /// Set timebase.
    pub fn with_timebase(mut self, timebase: Timebase) -> Self {
        self.timebase = timebase;
        self
    }

    /// Enable emergency fallback mode.
    pub fn with_emergency_fallback(mut self) -> Self {
        self.emergency_fallback_enabled = true;
        self
    }

    /// Get the primary video track, if any.
    pub fn primary_track(&self) -> Option<&VideoTrack> {
        self.video_tracks.iter().find(|t| t.is_primary)
    }

    /// Validate the plan before execution.
    pub fn validate(&self) -> MediaResult<()> {
        // Must have at least one video track
        if self.video_tracks.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::TimelineEmptyTracks,
                "MediaTimelinePlan must have at least one video track",
            ));
        }

        // Validate timebase
        self.timebase.validate()?;

        // Validate output path is set
        if self.output.path.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::TimelineInvalidPlan,
                "Output path must be set",
            ));
        }

        // Validate all video tracks
        for track in &self.video_tracks {
            track.validate()?;
        }

        // Validate all overlays
        for overlay in &self.overlays {
            overlay.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timebase_conversions() {
        let tb = Timebase::new(1, 1000);
        assert_eq!(tb.to_seconds(1000), 1.0);
        assert_eq!(tb.from_seconds(1.0), 1000);
    }

    #[test]
    fn test_timebase_validate() {
        let valid = Timebase::new(1, 1000);
        assert!(valid.validate().is_ok());

        let invalid = Timebase::new(1, 0);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_video_segment_builder() {
        let seg = VideoSegment::new("/tmp/video.mp4")
            .with_source_start(1.0)
            .with_duration(5.0)
            .with_timeline_start(0.0);

        assert_eq!(seg.path, "/tmp/video.mp4");
        assert_eq!(seg.source_start, 1.0);
        assert_eq!(seg.duration, 5.0);
    }

    #[test]
    fn test_video_track_validate() {
        let track = VideoTrack::new("main");
        assert!(track.validate().is_err());

        let track = VideoTrack::new("main")
            .add_segment(VideoSegment::new("/tmp/video.mp4"));
        assert!(track.validate().is_ok());
    }

    #[test]
    fn test_overlay_validate() {
        let overlay = OverlayTrack::new("/tmp/overlay.png")
            .with_time_range(0.0, 5.0);
        assert!(overlay.validate().is_ok());

        let bad_overlay = OverlayTrack::new("/tmp/overlay.png")
            .with_time_range(-1.0, 5.0);
        assert!(bad_overlay.validate().is_err());
    }

    #[test]
    fn test_media_timeline_plan_validate() {
        let plan = MediaTimelinePlan::new("test-plan");
        assert!(plan.validate().is_err()); // No tracks

        let plan = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/video.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn test_effect_node_builder() {
        let effect = EffectNode::new("blur")
            .with_time_range(0.0, 5.0)
            .with_param("sigma", serde_json::json!(2.0));

        assert_eq!(effect.effect_type, "blur");
        assert_eq!(effect.start_time, Some(0.0));
        assert!(effect.params.contains_key("sigma"));
    }
}