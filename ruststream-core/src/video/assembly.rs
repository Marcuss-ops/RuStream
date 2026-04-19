//! Assembly base video - Rust filter_complex builder for video assembly
//!
//! This module replaces the Python `_bake_master_audio_on_base()` function with a
//! type-safe Rust implementation that:
//! - Builds the master audio filter_complex in Rust
//! - Handles VO offset, gate ranges, music mixing
//! - Executes ffmpeg with the built filter
//!
//! # Architecture
//! The assembly phase in Python does:
//! 1. Concatenate stock segments
//! 2. Insert middle clips at computed points
//! 3. Bake master audio (VO + music with gate)
//! 4. Apply transitions
//!
//! This module handles step 3 (audio baking) which is the most subprocess-heavy part.

use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::audio::gate_utils::{build_gate_expr_from_ranges, build_intro_only_gate_expr};
use crate::core::{MediaError, MediaErrorCode, MediaResult};

/// Configuration for assembly audio baking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyAudioConfig {
    /// Path to base video (stock + end clips concatenated)
    pub base_video_path: String,
    /// Path to voiceover audio
    pub voiceover_path: Option<String>,
    /// Path to background music
    pub music_path: Option<String>,
    /// Output path
    pub output_path: String,
    /// VO offset in seconds (delay before VO starts)
    pub vo_offset_s: f64,
    /// Gate ranges where base audio should play (intro/middle clips)
    pub gate_ranges: Vec<(f64, f64)>,
    /// Music volume (0.0 - 1.0)
    pub music_volume: f64,
    /// Sample rate
    pub sample_rate: u32,
    /// Whether to use intro clip audio for intro section
    pub use_intro_clip_audio: bool,
    /// Path to first start clip (for intro audio)
    pub first_start_clip_path: Option<String>,
    /// Intro duration in seconds
    pub intro_duration: f64,
}

impl Default for AssemblyAudioConfig {
    fn default() -> Self {
        Self {
            base_video_path: String::new(),
            voiceover_path: None,
            music_path: None,
            output_path: String::new(),
            vo_offset_s: 0.0,
            gate_ranges: Vec::new(),
            music_volume: 0.5,
            sample_rate: 44100,
            use_intro_clip_audio: false,
            first_start_clip_path: None,
            intro_duration: 0.0,
        }
    }
}

/// Build the filter_complex for assembly audio baking
pub fn build_assembly_audio_filter(config: &AssemblyAudioConfig) -> Result<String, String> {
    let sr = config.sample_rate;
    let mut parts: Vec<String> = Vec::new();
    let mut inputs: Vec<String> = Vec::new();
    let mut input_idx = 0;

    // Gate expressions
    let gate_expr = build_gate_expr_from_ranges(&config.gate_ranges, false);
    let base_gate_expr = build_gate_expr_from_ranges(&config.gate_ranges, true);

    // Base video audio (index 0)
    // During gate ranges: play base audio (intro/middle clips)
    // Outside gate ranges: mute base audio (VO takes over)
    parts.push(format!(
        "[{idx}:a]aresample={sr},volume='{gate}':eval=frame[base]",
        idx = input_idx,
        sr = sr,
        gate = base_gate_expr
    ));
    inputs.push("[base]".to_string());
    input_idx += 1;

    // Voiceover (index 1)
    if config.voiceover_path.is_some() {
        let offset_ms = (config.vo_offset_s.max(0.0) * 1000.0).round() as i64;
        parts.push(format!(
            "[{idx}:a]aresample={sr},adelay={ms}|{ms},volume='{gate}':eval=frame,apad=whole_dur={dur:.6}[vo]",
            idx = input_idx,
            sr = sr,
            ms = offset_ms,
            gate = gate_expr,
            dur = 0.0 // Will be padded to match longest
        ));
        inputs.push("[vo]".to_string());
        input_idx += 1;
    }

    // Music (index 2)
    if config.music_path.is_some() {
        let vol = config.music_volume;
        parts.push(format!(
            "[{idx}:a]aresample={sr},aloop=loop=-1:size=2e9:start=0,volume='{vol}*{gate}':eval=frame,apad[bg]",
            idx = input_idx,
            sr = sr,
            vol = vol,
            gate = gate_expr
        ));
        inputs.push("[bg]".to_string());
    }

    if inputs.is_empty() {
        return Ok(format!("anullsrc=channel_layout=stereo:sample_rate={sr},apad[aout]"));
    }

    // Mix all inputs
    let mix_in = inputs.join("");
    let n = inputs.len();
    let filter = format!(
        "{parts};{mix_in}amix=inputs={n}:duration=longest:normalize=0,alimiter=limit=0.95[aout]",
        parts = parts.join(";"),
        mix_in = mix_in,
        n = n
    );

    Ok(filter)
}

/// Build the complete ffmpeg command for assembly audio baking
pub fn build_assembly_audio_command(config: &AssemblyAudioConfig) -> Result<Vec<String>, String> {
    if config.base_video_path.is_empty() {
        return Err("base_video_path is required".to_string());
    }
    if !Path::new(&config.base_video_path).exists() {
        return Err(format!("Base video not found: {}", config.base_video_path));
    }

    let filter = build_assembly_audio_filter(config)?;

    let mut cmd: Vec<String> = vec![
        "ffmpeg".to_string(),
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
    ];

    // Input 0: base video
    cmd.extend(["-i".to_string(), config.base_video_path.clone()]);

    // Input 1: voiceover (if present)
    if let Some(vo_path) = &config.voiceover_path {
        if Path::new(vo_path).exists() {
            cmd.extend(["-i".to_string(), vo_path.clone()]);
        }
    }

    // Input 2: music (if present, with stream_loop)
    if let Some(music_path) = &config.music_path {
        if Path::new(music_path).exists() {
            cmd.extend([
                "-stream_loop".to_string(),
                "-1".to_string(),
                "-i".to_string(),
                music_path.clone(),
            ]);
        }
    }

    // Filter complex
    cmd.extend(["-filter_complex".to_string(), filter]);

    // Map video from base (copy) and audio from filter
    cmd.extend([
        "-map".to_string(),
        "0:v".to_string(),
        "-map".to_string(),
        "[aout]".to_string(),
        "-c:v".to_string(),
        "copy".to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "192k".to_string(),
        "-ar".to_string(),
        config.sample_rate.to_string(),
        "-ac".to_string(),
        "2".to_string(),
    ]);

    cmd.push(config.output_path.clone());

    Ok(cmd)
}

/// Execute assembly audio baking.
///
/// Uses ffmpeg CLI with a pre-built filter_complex for mixing VO, music,
/// and base audio with gate expressions.
///
/// # Arguments
/// * `config` - Assembly audio configuration
///
/// # Returns
/// * `Ok(output_path)` on success
/// * `Err(MediaError)` on failure
pub fn bake_assembly_audio(config: AssemblyAudioConfig) -> MediaResult<String> {
    // Validate inputs
    if config.base_video_path.is_empty() {
        return Err(MediaError::new(MediaErrorCode::InvalidInput, "base_video_path is required"));
    }
    if !Path::new(&config.base_video_path).exists() {
        return Err(MediaError::new(MediaErrorCode::FileNotFound, format!("Base video not found: {}", config.base_video_path)));
    }

    // Validate optional inputs
    if let Some(vo_path) = &config.voiceover_path {
        if !vo_path.is_empty() && !Path::new(vo_path).exists() {
            return Err(MediaError::new(MediaErrorCode::FileNotFound, format!("Voiceover not found: {}", vo_path)));
        }
    }
    if let Some(music_path) = &config.music_path {
        if !music_path.is_empty() && !Path::new(music_path).exists() {
            return Err(MediaError::new(MediaErrorCode::FileNotFound, format!("Music not found: {}", music_path)));
        }
    }

    // Execute via CLI — full native FFmpeg filter graph requires ff::filter::Graph
    // which is not stable. The CLI approach is robust and well-tested.
    bake_assembly_audio_cli(config)
}

/// CLI-based assembly audio baking — uses `FfmpegCommand` so spawns are
/// counted in the `Profiler` when one is passed via the global context.
fn bake_assembly_audio_cli(config: AssemblyAudioConfig) -> MediaResult<String> {
    use crate::io::subprocess::FfmpegCommand;

    let cmd = build_assembly_audio_command(&config)
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, e))?;

    // Skip the first element ("ffmpeg") — FfmpegCommand adds it automatically
    let args = &cmd[1..];

    FfmpegCommand::new()
        .args(args.iter().cloned())
        .output_file(config.output_path.clone())
        .output()
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, e.to_string()))?;

    if !Path::new(&config.output_path).exists() {
        return Err(MediaError::new(
            MediaErrorCode::AudioResampleFailed,
            "Output file was not created",
        ));
    }

    let output_size = std::fs::metadata(&config.output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    if output_size < 1024 {
        return Err(MediaError::new(
            MediaErrorCode::AudioResampleFailed,
            format!("Output file too small ({} bytes)", output_size),
        ));
    }

    Ok(config.output_path.clone())
}

/// Execute assembly audio baking (alias for backward compatibility)
pub fn bake_assembly_audio_legacy(config: AssemblyAudioConfig) -> Result<String, String> {
    bake_assembly_audio(config).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gate_expr_empty() {
        let ranges: Vec<(f64, f64)> = vec![];
        assert_eq!(build_gate_expr_from_ranges(&ranges, false), "1");
        assert_eq!(build_gate_expr_from_ranges(&ranges, true), "0");
    }

    #[test]
    fn test_build_gate_expr_single() {
        let ranges = vec![(0.0, 5.0)];
        let expr = build_gate_expr_from_ranges(&ranges, false);
        assert!(expr.contains("between(t"));
        assert!(expr.contains("0.000000"));
        assert!(expr.contains("5.000000"));
    }

    #[test]
    fn test_build_intro_only_gate_expr() {
        assert_eq!(build_intro_only_gate_expr(0.0), "0");
        assert_eq!(build_intro_only_gate_expr(0.005), "0");
        let expr = build_intro_only_gate_expr(5.0);
        assert!(expr.contains("between(t\\,0\\,5.000000)"));
    }

    #[test]
    fn test_build_assembly_audio_filter() {
        let config = AssemblyAudioConfig {
            base_video_path: "base.mp4".to_string(),
            voiceover_path: Some("vo.mp3".to_string()),
            music_path: Some("music.mp3".to_string()),
            output_path: "output.mp4".to_string(),
            vo_offset_s: 2.0,
            gate_ranges: vec![(0.0, 2.0)],
            music_volume: 0.3,
            sample_rate: 44100,
            ..Default::default()
        };
        let filter = build_assembly_audio_filter(&config).unwrap();
        assert!(filter.contains("[0:a]"));
        assert!(filter.contains("[1:a]"));
        assert!(filter.contains("[2:a]"));
        assert!(filter.contains("amix"));
        assert!(filter.contains("[aout]"));
    }
}