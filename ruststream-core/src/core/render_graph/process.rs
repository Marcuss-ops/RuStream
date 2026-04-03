//! Main entry point for render graph processing.

use std::time::Instant;

use crate::core::errors::{MediaError, DriftMetrics};

use super::component::ComponentId;
use super::graph::RenderGraph;
use super::metrics::RenderMetrics;
use super::reason::ReasonCode;
use super::result::RenderResult;
use super::stages::{probe_sources, decode_sources, apply_effects, apply_overlays, process_audio, encode_output};

// ============================================================================
// Main Entry Point
// ============================================================================

/// Process a render graph through the complete pipeline.
///
/// This is the single entrypoint for all media processing operations.
/// It replaces scattered calls across multiple Python adapters with a
/// unified Rust pipeline.
///
/// # Pipeline Stages
///
/// 1. **Validate** - Validate the render graph
/// 2. **Probe** - Probe source files for metadata
/// 3. **Decode** - Decode video/audio from source files
/// 4. **Effects** - Apply video effects
/// 5. **Overlay** - Apply overlays
/// 6. **Audio** - Process audio graph
/// 7. **Encode** - Encode final output
///
/// # Arguments
///
/// * `graph` - The render graph to process
///
/// # Returns
///
/// A `RenderResult` with artifact path, metrics, reason codes, and drift metrics.
pub fn process_render_graph(graph: &RenderGraph) -> RenderResult {
    let total_start = Instant::now();
    let mut metrics = RenderMetrics::new();

    // Stage 0: Validate
    metrics.record_attempt(ComponentId::Decode); // Validation is part of decode
    let validate_start = Instant::now();
    if let Err(e) = graph.validate() {
        let elapsed = validate_start.elapsed().as_millis() as u64;
        metrics.record_fallback(ComponentId::Decode, elapsed, e.code.as_str());
        return RenderResult::failure(e)
            .with_metrics(metrics)
            .with_reason_code(ReasonCode::ValidationFailed);
    }

    // Stage 1: Probe
    metrics.record_attempt(ComponentId::Probe);
    let probe_start = Instant::now();
    if let Err(e) = probe_sources(graph) {
        let elapsed = probe_start.elapsed().as_millis() as u64;
        metrics.record_fallback(ComponentId::Probe, elapsed, e.code.as_str());
        metrics.probe_ms = elapsed;
        return RenderResult::failure(e)
            .with_metrics(metrics)
            .with_reason_code(ReasonCode::ProbeFailed);
    }
    let probe_elapsed = probe_start.elapsed().as_millis() as u64;
    metrics.record_success(ComponentId::Probe, probe_elapsed);
    metrics.probe_ms = probe_elapsed;

    // Stage 2: Decode
    metrics.record_attempt(ComponentId::Decode);
    let decode_start = Instant::now();
    if let Err(e) = decode_sources(graph) {
        let elapsed = decode_start.elapsed().as_millis() as u64;
        metrics.record_fallback(ComponentId::Decode, elapsed, e.code.as_str());
        metrics.decode_ms = elapsed;
        return handle_stage_error(e, "decode", &mut metrics);
    }
    let decode_elapsed = decode_start.elapsed().as_millis() as u64;
    metrics.record_success(ComponentId::Decode, decode_elapsed);
    metrics.decode_ms = decode_elapsed;

    // Stage 3: Effects
    metrics.record_attempt(ComponentId::Effects);
    let effects_start = Instant::now();
    if let Err(e) = apply_effects(graph) {
        let elapsed = effects_start.elapsed().as_millis() as u64;
        metrics.record_fallback(ComponentId::Effects, elapsed, e.code.as_str());
        metrics.effects_ms = elapsed;
        return handle_stage_error(e, "effects", &mut metrics);
    }
    let effects_elapsed = effects_start.elapsed().as_millis() as u64;
    metrics.record_success(ComponentId::Effects, effects_elapsed);
    metrics.effects_ms = effects_elapsed;

    // Stage 4: Overlay
    metrics.record_attempt(ComponentId::Overlay);
    let overlay_start = Instant::now();
    if let Err(e) = apply_overlays(graph) {
        let elapsed = overlay_start.elapsed().as_millis() as u64;
        metrics.record_fallback(ComponentId::Overlay, elapsed, e.code.as_str());
        metrics.overlay_ms = elapsed;
        return handle_stage_error(e, "overlay", &mut metrics);
    }
    let overlay_elapsed = overlay_start.elapsed().as_millis() as u64;
    metrics.record_success(ComponentId::Overlay, overlay_elapsed);
    metrics.overlay_ms = overlay_elapsed;

    // Stage 5: Audio
    let drift = if let Some(audio) = &graph.audio {
        metrics.record_attempt(ComponentId::Audio);
        let audio_start = Instant::now();
        match process_audio(audio) {
            Ok(result) => {
                let audio_elapsed = audio_start.elapsed().as_millis() as u64;
                metrics.record_success(ComponentId::Audio, audio_elapsed);
                metrics.audio_ms = audio_elapsed;
                result.drift
            }
            Err(e) => {
                let elapsed = audio_start.elapsed().as_millis() as u64;
                metrics.record_fallback(ComponentId::Audio, elapsed, e.code.as_str());
                metrics.audio_ms = elapsed;
                return handle_stage_error(e, "audio", &mut metrics);
            }
        }
    } else {
        DriftMetrics::new()
    };

    // Stage 6: Encode
    metrics.record_attempt(ComponentId::Encode);
    let encode_start = Instant::now();
    match encode_output(graph) {
        Ok(artifact_path) => {
            let encode_elapsed = encode_start.elapsed().as_millis() as u64;
            metrics.record_success(ComponentId::Encode, encode_elapsed);
            metrics.encode_ms = encode_elapsed;

            // Total time
            metrics.total_ms = total_start.elapsed().as_millis() as u64;

            // Build success result
            let mut result = RenderResult::success(&artifact_path)
                .with_metrics(metrics)
                .with_drift(drift);

            // Check if any fallbacks were used
            if result.metrics.has_any_fallback() {
                let fallback_components = result.metrics.fallback_components();
                result = result
                    .with_fallback(fallback_components)
                    .with_reason_code(ReasonCode::SuccessWithFallback);
            }

            result
        }
        Err(e) => {
            let elapsed = encode_start.elapsed().as_millis() as u64;
            metrics.record_fallback(ComponentId::Encode, elapsed, e.code.as_str());
            metrics.encode_ms = elapsed;
            handle_stage_error(e, "encode", &mut metrics)
        }
    }
}

/// Run the audio graph independently.
///
/// This is useful when audio processing needs to be done separately
/// from the video pipeline (e.g., pre-baking audio).
pub fn run_audio_graph(graph: &crate::core::audio_graph::AudioGraphConfig) -> crate::core::audio_graph::AudioGraphResult {
    // Validate configuration
    if let Err(e) = graph.validate() {
        return crate::core::audio_graph::AudioGraphResult::failure(e);
    }

    // Check drift threshold
    if graph.sync.max_drift_frames > 1.0 {
        return crate::core::audio_graph::AudioGraphResult::failure(MediaError::new(
            crate::core::errors::MediaErrorCode::AudioDriftExceeded,
            format!(
                "Max drift threshold too high: {} frames (max 1.0)",
                graph.sync.max_drift_frames
            ),
        ));
    }

    // Return success - actual processing is delegated to audio_bake
    crate::core::audio_graph::AudioGraphResult::success("", 0)
}

// ============================================================================
// Internal Helper Functions
// ============================================================================

/// Handle a stage error, potentially triggering emergency fallback.
fn handle_stage_error(
    error: MediaError,
    stage: &str,
    metrics: &mut RenderMetrics,
) -> RenderResult {
    // In normal mode, fail immediately
    RenderResult::failure(error.with_stage(stage)).with_metrics(metrics.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::timeline::{MediaTimelinePlan, VideoTrack, VideoSegment, OutputConfig};

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
    fn test_process_render_graph_validate() {
        let graph = create_test_graph();
        // Structure is valid - file existence is checked in probe_sources(), not validate()
        assert!(graph.validate().is_ok());
    }

    #[test]
    fn test_process_render_graph_empty_id() {
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
    fn test_handle_stage_error() {
        let mut metrics = RenderMetrics::new();
        let error = MediaError::new(crate::core::errors::MediaErrorCode::DecodeFailed, "Test error");
        let result = handle_stage_error(error, "decode", &mut metrics);
        assert!(!result.success);
        assert_eq!(result.error_stage, Some("decode".to_string()));
    }
}