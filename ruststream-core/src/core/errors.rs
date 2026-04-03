//! Typed media-domain errors for the unified media pipeline.
//!
//! This module provides structured error types with reason codes for
//! audit trails and incident tracking. Every error includes a machine-readable
//! code and a human-readable message.

use std::fmt;
use serde::{Deserialize, Serialize};

/// Machine-readable error codes for media pipeline operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaErrorCode {
    // Decode errors
    DecodeFailed,
    DecodeTimeout,
    DecodeCorruptStream,
    DecodeUnsupportedCodec,

    // Encode errors
    EncodeFailed,
    EncodeTimeout,
    EncodeOutOfMemory,

    // Effects errors
    EffectsFailed,

    // Overlay errors
    OverlayInvalidBounds,
    OverlayMissingAsset,
    OverlayBlendFailed,

    // Audio errors
    AudioResampleFailed,
    AudioMixFailed,
    AudioDriftExceeded,
    AudioSyncFailed,
    AudioGraphInvalidConfig,

    // Timeline errors
    TimelineInvalidPlan,
    TimelineEmptyTracks,
    TimelineOverlappingSegments,
    TimelineInvalidTimebase,

    // Pipeline errors
    PipelineStageFailed,
    PipelineCancelled,
    PipelineResourceExhausted,

    // Init errors
    InitFailed,

    // I/O errors
    IoFileNotFound,
    IoPermissionDenied,
    IoDiskFull,

    // Fallback/emergency
    EmergencyFallbackTriggered,

    // Cache errors
    CacheOpenFailed,
    CacheWriteFailed,
    CacheEvictionFailed,
    CacheClearFailed,
    CacheFlushFailed,
    CacheMaintenanceFailed,
    CacheKeyGenerationFailed,
}

impl MediaErrorCode {
    /// Returns a static string representation of the error code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DecodeFailed => "DECODE_FAILED",
            Self::DecodeTimeout => "DECODE_TIMEOUT",
            Self::DecodeCorruptStream => "DECODE_CORRUPT_STREAM",
            Self::DecodeUnsupportedCodec => "DECODE_UNSUPPORTED_CODEC",
            Self::EncodeFailed => "ENCODE_FAILED",
            Self::EncodeTimeout => "ENCODE_TIMEOUT",
            Self::EncodeOutOfMemory => "ENCODE_OUT_OF_MEMORY",
            Self::EffectsFailed => "EFFECTS_FAILED",
            Self::OverlayInvalidBounds => "OVERLAY_INVALID_BOUNDS",
            Self::OverlayMissingAsset => "OVERLAY_MISSING_ASSET",
            Self::OverlayBlendFailed => "OVERLAY_BLEND_FAILED",
            Self::AudioResampleFailed => "AUDIO_RESAMPLE_FAILED",
            Self::AudioMixFailed => "AUDIO_MIX_FAILED",
            Self::AudioDriftExceeded => "AUDIO_DRIFT_EXCEEDED",
            Self::AudioSyncFailed => "AUDIO_SYNC_FAILED",
            Self::AudioGraphInvalidConfig => "AUDIO_GRAPH_INVALID_CONFIG",
            Self::TimelineInvalidPlan => "TIMELINE_INVALID_PLAN",
            Self::TimelineEmptyTracks => "TIMELINE_EMPTY_TRACKS",
            Self::TimelineOverlappingSegments => "TIMELINE_OVERLAPPING_SEGMENTS",
            Self::TimelineInvalidTimebase => "TIMELINE_INVALID_TIMEBASE",
            Self::PipelineStageFailed => "PIPELINE_STAGE_FAILED",
            Self::PipelineCancelled => "PIPELINE_CANCELLED",
            Self::PipelineResourceExhausted => "PIPELINE_RESOURCE_EXHAUSTED",
            Self::InitFailed => "INIT_FAILED",
            Self::IoFileNotFound => "IO_FILE_NOT_FOUND",
            Self::IoPermissionDenied => "IO_PERMISSION_DENIED",
            Self::IoDiskFull => "IO_DISK_FULL",
            Self::EmergencyFallbackTriggered => "EMERGENCY_FALLBACK_TRIGGERED",
            Self::CacheOpenFailed => "CACHE_OPEN_FAILED",
            Self::CacheWriteFailed => "CACHE_WRITE_FAILED",
            Self::CacheEvictionFailed => "CACHE_EVICTION_FAILED",
            Self::CacheClearFailed => "CACHE_CLEAR_FAILED",
            Self::CacheFlushFailed => "CACHE_FLUSH_FAILED",
            Self::CacheMaintenanceFailed => "CACHE_MAINTENANCE_FAILED",
            Self::CacheKeyGenerationFailed => "CACHE_KEY_GENERATION_FAILED",
        }
    }
}

impl fmt::Display for MediaErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A structured media pipeline error with reason code and context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaError {
    /// Machine-readable error code.
    pub code: MediaErrorCode,
    /// Human-readable error message.
    pub message: String,
    /// Optional stage where the error occurred (e.g., "decode", "overlay", "audio_mix").
    pub stage: Option<String>,
    /// Optional file path related to the error.
    pub path: Option<String>,
    /// Whether this error triggered emergency fallback.
    pub fallback_triggered: bool,
}

impl MediaError {
    /// Create a new media error with the given code and message.
    pub fn new(code: MediaErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            stage: None,
            path: None,
            fallback_triggered: false,
        }
    }

    /// Set the stage where the error occurred.
    pub fn with_stage(mut self, stage: impl Into<String>) -> Self {
        self.stage = Some(stage.into());
        self
    }

    /// Set the file path related to the error.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Mark this error as having triggered emergency fallback.
    pub fn with_fallback(mut self) -> Self {
        self.fallback_triggered = true;
        self
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(stage) = &self.stage {
            write!(f, " (stage: {})", stage)?;
        }
        if let Some(path) = &self.path {
            write!(f, " (path: {})", path)?;
        }
        if self.fallback_triggered {
            write!(f, " [FALLBACK_TRIGGERED]")?;
        }
        Ok(())
    }
}

impl std::error::Error for MediaError {}

impl From<MediaError> for String {
    fn from(err: MediaError) -> String {
        err.to_string()
    }
}

/// Result type for media pipeline operations.
pub type MediaResult<T> = Result<T, MediaError>;

/// Stage-level timing metrics for pipeline execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageMetrics {
    /// Time spent in decode stage (milliseconds).
    pub decode_ms: u64,
    /// Time spent in effects stage (milliseconds).
    pub effects_ms: u64,
    /// Time spent in overlay stage (milliseconds).
    pub overlay_ms: u64,
    /// Time spent in encode stage (milliseconds).
    pub encode_ms: u64,
    /// Time spent in audio processing (milliseconds).
    pub audio_ms: u64,
    /// Total pipeline time (milliseconds).
    pub total_ms: u64,
}

impl StageMetrics {
    /// Create a new empty metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sum of all stage times.
    pub fn stage_sum(&self) -> u64 {
        self.decode_ms + self.effects_ms + self.overlay_ms + self.encode_ms + self.audio_ms
    }
}

/// Drift correction metrics for audio/video sync.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriftMetrics {
    /// Maximum drift observed (in frames).
    pub drift_frames_max: f64,
    /// 95th percentile drift (in frames).
    pub drift_frames_p95: f64,
    /// Number of drift corrections applied.
    pub drift_corrections_count: u32,
    /// Average resample ratio used for corrections.
    pub resample_ratio_avg: f64,
}

impl DriftMetrics {
    /// Create a new empty drift metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if drift is within acceptable threshold (<= 1 frame).
    pub fn is_acceptable(&self) -> bool {
        self.drift_frames_max <= 1.0
    }
}

/// Complete pipeline result with metrics and optional error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    /// Whether the pipeline completed successfully.
    pub success: bool,
    /// Output file path (if successful).
    pub output_path: Option<String>,
    /// Stage-level timing metrics.
    pub metrics: StageMetrics,
    /// Drift correction metrics.
    pub drift: DriftMetrics,
    /// Error details (if failed).
    pub error: Option<MediaError>,
    /// Whether emergency fallback was used.
    pub fallback_used: bool,
    /// Reason code for fallback (if used).
    pub fallback_reason: Option<String>,
    /// Output file checksum for parity validation (if available).
    pub output_checksum: Option<String>,
}

impl PipelineResult {
    /// Create a successful pipeline result.
    pub fn success(output_path: impl Into<String>) -> Self {
        Self {
            success: true,
            output_path: Some(output_path.into()),
            metrics: StageMetrics::new(),
            drift: DriftMetrics::new(),
            error: None,
            fallback_used: false,
            fallback_reason: None,
            output_checksum: None,
        }
    }

    /// Create a failed pipeline result.
    pub fn failure(error: MediaError) -> Self {
        let fallback_used = error.fallback_triggered;
        Self {
            success: false,
            output_path: None,
            metrics: StageMetrics::new(),
            drift: DriftMetrics::new(),
            error: Some(error),
            fallback_used,
            fallback_reason: None,
            output_checksum: None,
        }
    }

    /// Set stage metrics.
    pub fn with_metrics(mut self, metrics: StageMetrics) -> Self {
        self.metrics = metrics;
        self
    }

    /// Set drift metrics.
    pub fn with_drift(mut self, drift: DriftMetrics) -> Self {
        self.drift = drift;
        self
    }

    /// Mark as using emergency fallback.
    pub fn with_fallback(mut self, reason: impl Into<String>) -> Self {
        self.fallback_used = true;
        self.fallback_reason = Some(reason.into());
        self
    }

    /// Set output checksum for parity validation.
    pub fn with_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.output_checksum = Some(checksum.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_as_str() {
        assert_eq!(MediaErrorCode::DecodeFailed.as_str(), "DECODE_FAILED");
        assert_eq!(MediaErrorCode::AudioDriftExceeded.as_str(), "AUDIO_DRIFT_EXCEEDED");
    }

    #[test]
    fn test_media_error_builder() {
        let err = MediaError::new(MediaErrorCode::DecodeFailed, "Failed to decode video")
            .with_stage("decode")
            .with_path("/tmp/video.mp4")
            .with_fallback();

        assert_eq!(err.code, MediaErrorCode::DecodeFailed);
        assert_eq!(err.stage, Some("decode".to_string()));
        assert_eq!(err.path, Some("/tmp/video.mp4".to_string()));
        assert!(err.fallback_triggered);
    }

    #[test]
    fn test_media_error_display() {
        let err = MediaError::new(MediaErrorCode::DecodeFailed, "test")
            .with_stage("decode");
        let display = format!("{}", err);
        assert!(display.contains("DECODE_FAILED"));
        assert!(display.contains("decode"));
    }

    #[test]
    fn test_stage_metrics_default() {
        let metrics = StageMetrics::new();
        assert_eq!(metrics.stage_sum(), 0);
    }

    #[test]
    fn test_drift_metrics_acceptable() {
        let mut drift = DriftMetrics::new();
        assert!(drift.is_acceptable());

        drift.drift_frames_max = 0.8;
        assert!(drift.is_acceptable());

        drift.drift_frames_max = 1.5;
        assert!(!drift.is_acceptable());
    }

    #[test]
    fn test_pipeline_result_success() {
        let result = PipelineResult::success("/tmp/output.mp4");
        assert!(result.success);
        assert_eq!(result.output_path, Some("/tmp/output.mp4".to_string()));
        assert!(!result.fallback_used);
    }

    #[test]
    fn test_pipeline_result_failure() {
        let err = MediaError::new(MediaErrorCode::DecodeFailed, "test").with_fallback();
        let result = PipelineResult::failure(err);
        assert!(!result.success);
        assert!(result.fallback_used);
    }
}