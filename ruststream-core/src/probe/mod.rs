//! Probe module - Media file metadata extraction
//!
//! Provides native MP4/MOV metadata extraction.

use serde::{Deserialize, Serialize};
use std::path::Path;
use ffmpeg_next as ff;
use crate::core::{MediaError, MediaErrorCode, MediaResult};

/// Complete media metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullMetadata {
    pub path: String,
    pub video: VideoMetadata,
    pub audio: Option<AudioMetadata>,
    pub format: FormatMetadata,
}

/// Video-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoMetadata {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub bit_rate: Option<u64>,
    pub has_alpha: bool,
}

/// Audio-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioMetadata {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bit_depth: Option<u16>,
    pub bit_rate: Option<u64>,
}

/// Format-level metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FormatMetadata {
    pub format_name: String,
    pub duration_secs: f64,
    pub bit_rate: Option<u64>,
    pub size_bytes: u64,
}

/// Probe media file for metadata
pub fn probe_full(path: &str) -> MediaResult<FullMetadata> {
    // Initialize FFmpeg (ignore error if already initialized)
    let _ = ff::init();

    // Open input file
    let context = ff::format::input(path)
        .map_err(|e| MediaError::new(
            MediaErrorCode::IoFileNotFound,
            format!("Cannot open '{}': {}", path, e)
        ))?;

    // Get format metadata
    let format_name = context.format().name().to_string();
    let duration_secs = context.duration() as f64 / ff::ffi::AV_TIME_BASE as f64;
    let bit_rate = if context.bit_rate() > 0 { Some(context.bit_rate() as u64) } else { None };

    // Get file size
    let size_bytes = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Initialize metadata
    let mut video_metadata = VideoMetadata::default();
    let mut audio_metadata_opt: Option<AudioMetadata> = None;
    video_metadata.duration_secs = duration_secs;

    // Iterate streams
    for stream in context.streams() {
        let params = stream.parameters();
        let codec_id = params.id();
        let codec_name = codec_id.name().to_string();

        // Get codec type
        let codec_type = params.medium();

        match codec_type {
            ff::media::Type::Video => {
                video_metadata.codec = codec_name;
                
                // Extract video dimensions from codec context
                if ff::decoder::find(params.id()).is_some() {
                    if let Ok(ctx) = ff::codec::context::Context::from_parameters(params) {
                        if let Ok(video_context) = ctx.decoder().video() {
                            video_metadata.width = video_context.width();
                            video_metadata.height = video_context.height();
                            
                            // Get FPS from framerate
                            if let Some(rate) = video_context.frame_rate() {
                                video_metadata.fps = rate.numerator() as f64 / rate.denominator() as f64;
                            }
                        }
                    }
                }
            }
            ff::media::Type::Audio => {
                // Extract audio details from codec context
                if ff::decoder::find(params.id()).is_some() {
                    if let Ok(ctx) = ff::codec::context::Context::from_parameters(params) {
                        if let Ok(audio_context) = ctx.decoder().audio() {
                            audio_metadata_opt = Some(AudioMetadata {
                                codec: codec_name,
                                sample_rate: audio_context.rate(),
                                channels: audio_context.channels(),
                                bit_depth: match audio_context.format() {
                                    ff::format::Sample::I16(_) => Some(16),
                                    ff::format::Sample::I32(_) => Some(32),
                                    ff::format::Sample::F32(_) => Some(32),
                                    ff::format::Sample::I64(_) => Some(64),
                                    ff::format::Sample::F64(_) => Some(64),
                                    _ => None,
                                },
                                bit_rate: None,
                            });
                        }
                    }
                } else {
                    // Fallback if codec not found
                    audio_metadata_opt = Some(AudioMetadata {
                        codec: codec_name,
                        sample_rate: 0,
                        channels: 0,
                        bit_depth: None,
                        bit_rate: None,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(FullMetadata {
        path: path.to_string(),
        video: video_metadata,
        audio: audio_metadata_opt,
        format: FormatMetadata {
            format_name,
            duration_secs,
            bit_rate,
            size_bytes,
        },
    })
}

/// Probe with file existence check
pub fn probe_file(path: &Path) -> MediaResult<FullMetadata> {
    if !path.exists() {
        return Err(MediaError::new(
            MediaErrorCode::IoFileNotFound,
            format!("File not found: {}", path.display())
        ));
    }
    
    let path_str = path.to_str().ok_or_else(|| {
        MediaError::new(MediaErrorCode::IoFileNotFound, "Invalid path encoding")
    })?;
    
    probe_full(path_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_probe_nonexistent_file() {
        let result = probe_full("/nonexistent/file.mp4");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, MediaErrorCode::IoFileNotFound);
    }
}
