//! Internal stage functions for the render pipeline.

use crate::core::errors::{MediaError, MediaErrorCode, MediaResult};
use crate::core::audio_graph::AudioGraphConfig;

use super::graph::RenderGraph;

// ============================================================================
// Internal Stage Functions
// ============================================================================

/// Probe source files for metadata.
pub(crate) fn probe_sources(graph: &RenderGraph) -> MediaResult<()> {
    // Validate all source files exist
    for track in &graph.timeline.video_tracks {
        for segment in &track.segments {
            if !std::path::Path::new(&segment.path).exists() {
                return Err(MediaError::new(
                    MediaErrorCode::IoFileNotFound,
                    format!("Source video not found: {}", segment.path),
                )
                .with_stage("probe")
                .with_path(&segment.path));
            }
        }
    }

    // Validate overlay sources
    for overlay in &graph.timeline.overlays {
        if !std::path::Path::new(&overlay.path).exists() {
            return Err(MediaError::new(
                MediaErrorCode::OverlayMissingAsset,
                format!("Overlay asset not found: {}", overlay.path),
            )
            .with_stage("probe")
            .with_path(&overlay.path));
        }
    }

    // Validate audio sources
    if let Some(audio) = &graph.audio {
        for input in &audio.inputs {
            if !std::path::Path::new(&input.path).exists() {
                return Err(MediaError::new(
                    MediaErrorCode::IoFileNotFound,
                    format!("Audio source not found: {}", input.path),
                )
                .with_stage("probe")
                .with_path(&input.path));
            }
        }
    }

    Ok(())
}

/// Decode video/audio from source files.
pub(crate) fn decode_sources(graph: &RenderGraph) -> MediaResult<()> {
    // Decode validation is done in probe_sources
    // This stage prepares decode parameters
    for track in &graph.timeline.video_tracks {
        for segment in &track.segments {
            if segment.duration < 0.0 {
                return Err(MediaError::new(
                    MediaErrorCode::DecodeFailed,
                    format!("Invalid segment duration: {}", segment.duration),
                )
                .with_stage("decode")
                .with_path(&segment.path));
            }
        }
    }
    Ok(())
}

/// Apply video effects.
pub(crate) fn apply_effects(graph: &RenderGraph) -> MediaResult<()> {
    for effect in &graph.timeline.effects {
        if effect.effect_type.is_empty() {
            return Err(MediaError::new(
                MediaErrorCode::EffectsFailed,
                "Effect type cannot be empty",
            )
            .with_stage("effects"));
        }

        // Validate time range if specified
        if let (Some(start), Some(end)) = (effect.start_time, effect.end_time) {
            if end <= start {
                return Err(MediaError::new(
                    MediaErrorCode::EffectsFailed,
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
pub(crate) fn apply_overlays(graph: &RenderGraph) -> MediaResult<()> {
    for overlay in &graph.timeline.overlays {
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

/// Process audio graph.
pub(crate) fn process_audio(
    audio: &AudioGraphConfig,
) -> MediaResult<crate::core::audio_graph::AudioGraphResult> {
    audio.validate()?;

    // Check for drift threshold
    if audio.sync.max_drift_frames > 1.0 {
        return Err(MediaError::new(
            MediaErrorCode::AudioDriftExceeded,
            format!(
                "Max drift threshold too high: {} frames (max 1.0)",
                audio.sync.max_drift_frames
            ),
        )
        .with_stage("audio"));
    }

    // Return placeholder - actual processing happens in audio_bake
    Ok(crate::core::audio_graph::AudioGraphResult::success("", 0))
}

/// Encode final output.
pub(crate) fn encode_output(graph: &RenderGraph) -> MediaResult<String> {
    let output = &graph.timeline.output;

    if output.width == 0 || output.height == 0 {
        return Err(MediaError::new(
            MediaErrorCode::EncodeFailed,
            format!("Invalid output dimensions: {}x{}", output.width, output.height),
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

    Ok(output.path.clone())
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
    fn test_probe_sources_missing_file() {
        let graph = create_test_graph();
        let result = probe_sources(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::IoFileNotFound);
        assert_eq!(err.stage, Some("probe".to_string()));
    }

    #[test]
    fn test_decode_sources_invalid_duration() {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4").with_duration(-1.0))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = decode_sources(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::DecodeFailed);
        assert_eq!(err.stage, Some("decode".to_string()));
    }

    #[test]
    fn test_apply_effects_empty_type() {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .add_effect(crate::core::timeline::EffectNode::new(""))
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = apply_effects(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::EffectsFailed);
        assert_eq!(err.stage, Some("effects".to_string()));
    }

    #[test]
    fn test_apply_effects_invalid_time_range() {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .add_effect(
                crate::core::timeline::EffectNode::new("blur")
                    .with_time_range(10.0, 5.0)
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = apply_effects(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::EffectsFailed);
        assert_eq!(err.stage, Some("effects".to_string()));
    }

    #[test]
    fn test_apply_overlays_invalid_opacity() {
        let mut overlay = crate::core::timeline::OverlayTrack::new("/tmp/overlay.png");
        overlay.opacity = 1.5; // Directly set invalid opacity
        
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .add_overlay(overlay)
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = apply_overlays(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::OverlayInvalidBounds);
        assert_eq!(err.stage, Some("overlay".to_string()));
    }

    #[test]
    fn test_encode_output_invalid_dimensions() {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                width: 0,
                height: 0,
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = encode_output(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::EncodeFailed);
        assert_eq!(err.stage, Some("encode".to_string()));
    }

    #[test]
    fn test_encode_output_invalid_fps() {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                fps: 0.0,
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = encode_output(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::EncodeFailed);
        assert_eq!(err.stage, Some("encode".to_string()));
    }

    #[test]
    fn test_encode_output_invalid_crf() {
        let timeline = MediaTimelinePlan::new("test-plan")
            .add_video_track(
                VideoTrack::new("main").as_primary()
                    .add_segment(VideoSegment::new("/tmp/test.mp4"))
            )
            .with_output(OutputConfig {
                path: "/tmp/output.mp4".to_string(),
                crf: 52,
                ..Default::default()
            });

        let graph = RenderGraph::new("test-graph", timeline);
        let result = encode_output(&graph);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, MediaErrorCode::EncodeFailed);
        assert_eq!(err.stage, Some("encode".to_string()));
    }
}