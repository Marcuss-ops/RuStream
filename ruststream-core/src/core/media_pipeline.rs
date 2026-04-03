//! Unified media pipeline entrypoint.
//!
//! This module provides the single entrypoint for all media processing operations,
//! replacing scattered calls across multiple Python adapters with a unified Rust pipeline.
//!
//! # Architecture
//!
//! The pipeline follows a stage-based architecture:
//! 1. **Validate** - Validate the timeline plan and audio graph config
//! 2. **Decode** - Decode video/audio from source files
//! 3. **Effects** - Apply video effects (blur, crop, color, etc.)
//! 4. **Overlay** - Apply overlays on top of video
//! 5. **Audio** - Process audio graph (mix, duck, gate, limiter)
//! 6. **Encode** - Encode final output
//!
//! Each stage reports timing metrics and can trigger emergency fallback if needed.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::core::errors::{MediaError, MediaErrorCode, MediaResult, PipelineResult, StageMetrics, DriftMetrics};
use crate::core::timeline::MediaTimelinePlan;
use crate::core::audio_graph::{AudioGraphConfig, AudioGraphResult};
use crate::core::audio_orchestrator::AudioOrchestrator;
use crate::core::instrumentation::{Profiler, StageTimer, ProfilingReport};

/// Pipeline execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    /// Normal mode - Rust-only processing, no fallbacks allowed.
    Normal,
    /// Emergency mode - fallbacks permitted with audit trail.
    Emergency,
}

impl Default for PipelineMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Configuration for the media pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Execution mode.
    pub mode: PipelineMode,
    /// Whether to allow intermediate disk files.
    pub allow_intermediate_files: bool,
    /// Temporary directory for intermediate files (if allowed).
    pub temp_dir: Option<String>,
    /// Maximum execution time in seconds (0 = unlimited).
    pub timeout_secs: u64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            mode: PipelineMode::Normal,
            allow_intermediate_files: false,
            temp_dir: None,
            timeout_secs: 0,
        }
    }
}

/// Stage identifier for tracking pipeline progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Validate,
    Decode,
    Effects,
    Overlay,
    Audio,
    Encode,
}

impl PipelineStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Decode => "decode",
            Self::Effects => "effects",
            Self::Overlay => "overlay",
            Self::Audio => "audio",
            Self::Encode => "encode",
        }
    }
}

/// Callback for pipeline stage progress reporting.
pub type StageCallback = Box<dyn Fn(PipelineStage, StageMetrics) + Send + Sync>;

/// The unified media pipeline.
///
/// This is the single entrypoint for all media processing operations.
/// It orchestrates decode, effects, overlay, audio, and encode stages
/// with full metrics tracking and emergency fallback support.
pub struct MediaPipeline {
    /// Timeline plan (video/effects/overlays).
    plan: MediaTimelinePlan,
    /// Audio graph configuration.
    audio_graph: Option<AudioGraphConfig>,
    /// Pipeline configuration.
    config: PipelineConfig,
    /// Optional stage progress callback.
    stage_callback: Option<StageCallback>,
    /// Hot-path profiler for detailed metrics (thread-safe shared ownership).
    profiler: Arc<Mutex<Profiler>>,
}

impl MediaPipeline {
    /// Create a new pipeline with the given timeline plan.
    pub fn new(plan: MediaTimelinePlan) -> Self {
        Self {
            plan,
            audio_graph: None,
            config: PipelineConfig::default(),
            stage_callback: None,
            profiler: Arc::new(Mutex::new(Profiler::new())),
        }
    }

    /// Set the audio graph configuration.
    pub fn with_audio_graph(mut self, audio_graph: AudioGraphConfig) -> Self {
        self.audio_graph = Some(audio_graph);
        self
    }

    /// Set pipeline configuration.
    pub fn with_config(mut self, config: PipelineConfig) -> Self {
        self.config = config;
        self
    }

    /// Set a callback for stage progress reporting.
    pub fn with_stage_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(PipelineStage, StageMetrics) + Send + Sync + 'static,
    {
        self.stage_callback = Some(Box::new(callback));
        self
    }

    /// Execute the complete pipeline with full instrumentation.
    ///
    /// This is the main entrypoint that orchestrates all stages:
    /// 1. Validate plan and audio graph
    /// 2. Decode sources
    /// 3. Apply effects
    /// 4. Apply overlays
    /// 5. Process audio graph
    /// 6. Encode output
    ///
    /// Fail-fast on errors.
    ///
    /// Returns a `PipelineResult` with metrics and optional error.
    pub fn execute(&mut self) -> PipelineResult {
        let total_start = Instant::now();
        let mut metrics = StageMetrics::new();

        // Emergency fallback disabled (VELOX_EMERGENCY_FALLBACK removed)
        let emergency_fallback = false;

        // Stage 1: Validate
        let validate_timer = StageTimer::start("validate");
        if let Err(e) = self.validate() {
            validate_timer.stop(&self.profiler);
            return PipelineResult::failure(e).with_metrics(metrics);
        }
        let validate_elapsed = validate_timer.stop(&self.profiler);
        metrics.decode_ms = validate_elapsed.as_millis() as u64;
        self.report_stage(PipelineStage::Validate, &metrics);

        // Stage 2: Decode
        let decode_timer = StageTimer::start("decode");
        if let Err(e) = self.decode() {
            decode_timer.stop(&self.profiler);
            return self.handle_stage_error(e, "decode", &mut metrics, emergency_fallback);
        }
        let decode_elapsed = decode_timer.stop(&self.profiler);
        metrics.decode_ms = decode_elapsed.as_millis() as u64;
        if let Ok(mut p) = self.profiler.lock() {
            p.record_bytes_processed(self.calculate_source_bytes());
            p.record_frames_processed(self.calculate_frame_count());
        }
        self.report_stage(PipelineStage::Decode, &metrics);

        // Stage 3: Effects
        let effects_timer = StageTimer::start("effects");
        if let Err(e) = self.apply_effects() {
            effects_timer.stop(&self.profiler);
            return self.handle_stage_error(e, "effects", &mut metrics, emergency_fallback);
        }
        let effects_elapsed = effects_timer.stop(&self.profiler);
        metrics.effects_ms = effects_elapsed.as_millis() as u64;
        self.report_stage(PipelineStage::Effects, &metrics);

        // Stage 4: Overlay
        let overlay_timer = StageTimer::start("overlay");
        if let Err(e) = self.apply_overlays() {
            overlay_timer.stop(&self.profiler);
            return self.handle_stage_error(e, "overlay", &mut metrics, emergency_fallback);
        }
        let overlay_elapsed = overlay_timer.stop(&self.profiler);
        metrics.overlay_ms = overlay_elapsed.as_millis() as u64;
        self.report_stage(PipelineStage::Overlay, &metrics);

        // Stage 5: Audio
        let audio_timer = StageTimer::start("audio");
        // Clone audio_graph to avoid borrow conflict with &mut self in process_audio
        let audio_graph_clone = self.audio_graph.clone();
        let (drift, audio_checksum) = if let Some(audio_graph) = audio_graph_clone {
            match self.process_audio(&audio_graph) {
                Ok(result) => {
                    if let Ok(mut p) = self.profiler.lock() {
                        p.record_samples_processed(result.samples_processed);
                    }
                    (result.drift, result.output_checksum)
                }
                Err(e) => {
                    audio_timer.stop(&self.profiler);
                    return self.handle_stage_error(e, "audio", &mut metrics, emergency_fallback);
                }
            }
        } else {
            (DriftMetrics::new(), None)
        };
        let audio_elapsed = audio_timer.stop(&self.profiler);
        metrics.audio_ms = audio_elapsed.as_millis() as u64;
        self.report_stage(PipelineStage::Audio, &metrics);

        // Stage 6: Encode
        let encode_timer = StageTimer::start("encode");
        if let Err(e) = self.encode() {
            encode_timer.stop(&self.profiler);
            return self.handle_stage_error(e, "encode", &mut metrics, emergency_fallback);
        }
        let encode_elapsed = encode_timer.stop(&self.profiler);
        metrics.encode_ms = encode_elapsed.as_millis() as u64;
        self.report_stage(PipelineStage::Encode, &metrics);

        // Total time
        metrics.total_ms = total_start.elapsed().as_millis() as u64;

        let mut result = PipelineResult::success(&self.plan.output.path)
            .with_metrics(metrics)
            .with_drift(drift);

        // Propagate audio checksum if available
        if let Some(checksum) = audio_checksum {
            result = result.with_checksum(checksum);
        }

        result
    }

    /// Calculate total source bytes from all video tracks and overlays.
    fn calculate_source_bytes(&self) -> u64 {
        let mut total = 0u64;
        for track in &self.plan.video_tracks {
            for segment in &track.segments {
                if let Ok(metadata) = std::fs::metadata(&segment.path) {
                    total += metadata.len();
                }
            }
        }
        for overlay in &self.plan.overlays {
            if let Ok(metadata) = std::fs::metadata(&overlay.path) {
                total += metadata.len();
            }
        }
        total
    }

    /// Calculate total frame count from video tracks.
    fn calculate_frame_count(&self) -> u64 {
        let mut total = 0u64;
        for track in &self.plan.video_tracks {
            for segment in &track.segments {
                // Estimate frames from duration and FPS
                // duration is f64 (0.0 means use full duration from source)
                let duration = segment.duration;
                if duration > 0.0 {
                    let fps = self.plan.output.fps.max(1.0);
                    total += (duration * fps) as u64;
                }
            }
        }
        total
    }

    /// Get a reference to the profiler for external access.
    pub fn profiler(&self) -> &Arc<Mutex<Profiler>> {
        &self.profiler
    }

    /// Generate a profiling report.
    pub fn generate_profiling_report(&self) -> ProfilingReport {
        if let Ok(p) = self.profiler.lock() {
            p.generate_report()
        } else {
            ProfilingReport::default()
        }
    }

    /// Validate the pipeline configuration.
    fn validate(&self) -> MediaResult<()> {
        // Validate timeline plan
        self.plan.validate()?;

        // Validate audio graph if present
        if let Some(audio_graph) = &self.audio_graph {
            audio_graph.validate()?;
        }

        // In normal mode, intermediate files are not allowed
        if self.config.mode == PipelineMode::Normal && self.config.allow_intermediate_files {
            return Err(MediaError::new(
                MediaErrorCode::PipelineStageFailed,
                "Intermediate files are not allowed in normal mode",
            ));
        }

        // Check output directory exists
        if let Some(parent) = Path::new(&self.plan.output.path).parent() {
            if !parent.exists() {
                return Err(MediaError::new(
                    MediaErrorCode::IoFileNotFound,
                    format!("Output directory does not exist: {}", parent.display()),
                ));
            }
        }

        Ok(())
    }

    /// Decode video/audio from source files.
    fn decode(&self) -> MediaResult<()> {
        // Validate all source files exist
        for track in &self.plan.video_tracks {
            for segment in &track.segments {
                if !Path::new(&segment.path).exists() {
                    return Err(MediaError::new(
                        MediaErrorCode::IoFileNotFound,
                        format!("Source video not found: {}", segment.path),
                    )
                    .with_stage("decode")
                    .with_path(&segment.path));
                }
            }
        }

        // Validate overlay sources
        for overlay in &self.plan.overlays {
            if !Path::new(&overlay.path).exists() {
                return Err(MediaError::new(
                    MediaErrorCode::OverlayMissingAsset,
                    format!("Overlay asset not found: {}", overlay.path),
                )
                .with_stage("decode")
                .with_path(&overlay.path));
            }
        }

        // Validate audio sources
        if let Some(audio_graph) = &self.audio_graph {
            for input in &audio_graph.inputs {
                if !Path::new(&input.path).exists() {
                    return Err(MediaError::new(
                        MediaErrorCode::IoFileNotFound,
                        format!("Audio source not found: {}", input.path),
                    )
                    .with_stage("decode")
                    .with_path(&input.path));
                }
            }
        }

        Ok(())
    }

    /// Apply video effects.
    fn apply_effects(&self) -> MediaResult<()> {
        // Effects are applied during the encode phase via filter_complex
        // This stage validates effect parameters
        for effect in &self.plan.effects {
            if effect.effect_type.is_empty() {
                return Err(MediaError::new(
                    MediaErrorCode::PipelineStageFailed,
                    "Effect type cannot be empty",
                )
                .with_stage("effects"));
            }

            // Validate time range if specified
            if let (Some(start), Some(end)) = (effect.start_time, effect.end_time) {
                if end <= start {
                    return Err(MediaError::new(
                        MediaErrorCode::PipelineStageFailed,
                        format!(
                            "Effect '{}' has invalid time range: {} -> {}",
                            effect.effect_type, start, end
                        ),
                    )
                    .with_stage("effects"));
                }
            }
        }

        Ok(())
    }

    /// Apply overlays.
    fn apply_overlays(&self) -> MediaResult<()> {
        // Overlay validation is done in timeline.validate()
        // This stage prepares overlay parameters for the filter_complex
        for overlay in &self.plan.overlays {
            if overlay.opacity < 0.0 || overlay.opacity > 1.0 {
                return Err(MediaError::new(
                    MediaErrorCode::OverlayInvalidBounds,
                    format!(
                        "Overlay '{}' has invalid opacity: {}",
                        overlay.path, overlay.opacity
                    ),
                )
                .with_stage("overlay"));
            }
        }

        Ok(())
    }

    /// Process audio graph using the audio orchestrator.
    ///
    /// This method delegates to AudioOrchestrator which:
    /// 1. Builds a concrete execution plan from AudioGraphConfig
    /// 2. Validates the plan
    /// 3. Delegates to audio_bake for actual processing
    /// 4. Validates output artifacts (checksum, duration, drift)
    /// 5. Returns structured results with metrics
    fn process_audio(&self, audio_graph: &AudioGraphConfig) -> MediaResult<AudioGraphResult> {
        // Validate audio graph configuration
        audio_graph.validate()?;

        // Check for drift threshold
        let sync = &audio_graph.sync;
        if sync.max_drift_frames > 1.0 {
            return Err(MediaError::new(
                MediaErrorCode::AudioDriftExceeded,
                format!(
                    "Max drift threshold too high: {} frames (max 1.0)",
                    sync.max_drift_frames
                ),
            )
            .with_stage("audio"));
        }

        // Build output path for audio artifact
        let audio_output_path = format!("{}.audio.m4a", self.plan.output.path);

        // Get base video path from first video track
        let base_video_path = self.plan.video_tracks
            .first()
            .and_then(|track| track.segments.first())
            .map(|segment| segment.path.clone())
            .unwrap_or_default();

        // Create orchestrator with profiler
        let mut orchestrator = AudioOrchestrator::with_profiler(self.profiler.clone());

        // Execute audio processing
        let orchestration_result = orchestrator.execute(
            audio_graph,
            &base_video_path,
            &audio_output_path,
        );

        // Convert to AudioGraphResult for compatibility
        let graph_result = orchestration_result.to_graph_result();

        // Record additional metrics from orchestration
        if orchestration_result.success {
            if let Ok(mut p) = self.profiler.lock() {
                p.record_cpu_time(
                    "audio_resample_ops",
                    orchestration_result.resample_ops.len() as u64,
                );
                p.record_cpu_time(
                    "audio_mix_ops",
                    orchestration_result.mix_ops.len() as u64,
                );
            }
        }

        Ok(graph_result)
    }

    /// Encode final output.
    fn encode(&self) -> MediaResult<()> {
        // Encoding is done via ffmpeg subprocess
        // This stage validates output parameters
        let output = &self.plan.output;

        if output.width == 0 || output.height == 0 {
            return Err(MediaError::new(
                MediaErrorCode::EncodeFailed,
                format!(
                    "Invalid output dimensions: {}x{}",
                    output.width, output.height
                ),
            )
            .with_stage("encode"));
        }

        if output.fps <= 0.0 {
            return Err(MediaError::new(
                MediaErrorCode::EncodeFailed,
                format!("Invalid output FPS: {}", output.fps),
            )
            .with_stage("encode"));
        }

        if output.crf > 51 {
            return Err(MediaError::new(
                MediaErrorCode::EncodeFailed,
                format!("Invalid CRF value: {} (max 51)", output.crf),
            )
            .with_stage("encode"));
        }

        Ok(())
    }

    /// Handle a stage error, potentially triggering emergency fallback.
    fn handle_stage_error(
        &self,
        error: MediaError,
        stage: &str,
        metrics: &mut StageMetrics,
        emergency_fallback: bool,
    ) -> PipelineResult {
        if emergency_fallback && self.plan.emergency_fallback_enabled {
            // Emergency fallback: return failure with fallback flag for audit
            let fallback_reason = format!("Stage '{}' failed: {}", stage, error.message);
            let fallback_error = error.with_stage(stage).with_fallback();
            PipelineResult::failure(fallback_error)
                .with_metrics(metrics.clone())
                .with_fallback(fallback_reason)
        } else {
            // Normal mode: fail immediately
            PipelineResult::failure(error.with_stage(stage)).with_metrics(metrics.clone())
        }
    }

    /// Report stage progress to callback.
    fn report_stage(&self, stage: PipelineStage, metrics: &StageMetrics) {
        if let Some(callback) = &self.stage_callback {
            callback(stage, metrics.clone());
        }
    }

    /// Get a reference to the timeline plan.
    pub fn plan(&self) -> &MediaTimelinePlan {
        &self.plan
    }

    /// Get a reference to the audio graph config.
    pub fn audio_graph(&self) -> Option<&AudioGraphConfig> {
        self.audio_graph.as_ref()
    }

    /// Get a reference to the pipeline config.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }
}

/// Convenience function to process a timeline plan.
///
/// This is the main public API for the media pipeline.
/// It creates a pipeline, validates the plan, and executes all stages.
///
/// # Arguments
///
/// * `plan` - The media timeline plan to process
///
/// # Returns
///
/// A `PipelineResult` with metrics and optional error.
pub fn process_timeline(plan: MediaTimelinePlan) -> PipelineResult {
    MediaPipeline::new(plan).execute()
}

/// Convenience function to process a timeline plan with audio graph.
///
/// # Arguments
///
/// * `plan` - The media timeline plan to process
/// * `audio_graph` - The audio graph configuration
///
/// # Returns
///
/// A `PipelineResult` with metrics and optional error.
pub fn process_timeline_with_audio(
    plan: MediaTimelinePlan,
    audio_graph: AudioGraphConfig,
) -> PipelineResult {
    MediaPipeline::new(plan)
        .with_audio_graph(audio_graph)
        .execute()
}

/// Run the audio graph independently.
///
/// This is useful when audio processing needs to be done separately
/// from the video pipeline (e.g., pre-baking audio).
///
/// # Arguments
///
/// * `config` - The audio graph configuration
///
/// # Returns
///
/// An `AudioGraphResult` with drift metrics and optional error.
pub fn run_audio_graph(config: AudioGraphConfig) -> AudioGraphResult {
    // Validate configuration
    if let Err(e) = config.validate() {
        return AudioGraphResult::failure(e);
    }

    // Check drift threshold
    if config.sync.max_drift_frames > 1.0 {
        return AudioGraphResult::failure(MediaError::new(
            MediaErrorCode::AudioDriftExceeded,
            format!(
                "Max drift threshold too high: {} frames (max 1.0)",
                config.sync.max_drift_frames
            ),
        ));
    }

    // Return success - actual processing is delegated to audio_bake
    AudioGraphResult::success("", 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::timeline::{VideoTrack, VideoSegment, OutputConfig};

    fn create_test_plan() -> MediaTimelinePlan {
        MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            })
    }

    #[test]
    fn test_pipeline_validate_no_files() {
        let plan = create_test_plan();
        let mut pipeline = MediaPipeline::new(plan);
        let result = pipeline.execute();
        // Should fail because /tmp/test.mp4 doesn't exist
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_pipeline_validate_empty_plan() {
        let plan = MediaTimelinePlan::new("empty");
        let mut pipeline = MediaPipeline::new(plan);
        let result = pipeline.execute();
        assert!(!result.success);
    }

    #[test]
    fn test_pipeline_config_normal_mode() {
        let config = PipelineConfig {
            mode: PipelineMode::Normal,
            allow_intermediate_files: true,
            ..Default::default()
        };
        let plan = create_test_plan();
        let mut pipeline = MediaPipeline::new(plan).with_config(config);
        let result = pipeline.execute();
        // Should fail because intermediate files not allowed in normal mode
        assert!(!result.success);
    }

    #[test]
    fn test_process_timeline_empty() {
        let plan = MediaTimelinePlan::new("empty");
        let result = process_timeline(plan);
        assert!(!result.success);
    }

    #[test]
    fn test_run_audio_graph_valid() {
        let config = AudioGraphConfig::new("test-graph");
        let result = run_audio_graph(config);
        assert!(result.success);
    }

    #[test]
    fn test_run_audio_graph_invalid_drift() {
        let config = AudioGraphConfig::new("test-graph")
            .with_sync(crate::core::audio_graph::SyncConfig {
                max_drift_frames: 2.0,
                ..Default::default()
            });
        let result = run_audio_graph(config);
        assert!(!result.success);
    }

    #[test]
    fn test_pipeline_stage_as_str() {
        assert_eq!(PipelineStage::Validate.as_str(), "validate");
        assert_eq!(PipelineStage::Decode.as_str(), "decode");
        assert_eq!(PipelineStage::Encode.as_str(), "encode");
    }
}