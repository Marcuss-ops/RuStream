//! Overlay merge - Rust filter_complex builder for final video compositing
//!
//! This module replaces the Python `merge_overlays_ffmpeg()` function with a
//! type-safe Rust implementation that:
//! - Builds filter_complex strings in Rust (no quoting issues)
//! - Validates inputs before execution
//! - Executes ffmpeg with the built filter
//!
//! # Performance
//! - Type-safe filter construction (no string concatenation bugs)
//! - Input validation before subprocess execution
//! - Same ffmpeg execution speed (subprocess-based)

use std::path::Path;
use serde::{Deserialize, Serialize};

/// Overlay configuration for a single overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// Path to overlay video file
    pub path: String,
    /// Start time in seconds
    pub start_time: f64,
    /// Duration in seconds
    pub duration: f64,
}

/// Middle clip timestamp range (where overlays should be disabled)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddleClipTimestamp {
    pub start: f64,
    pub end: f64,
}

/// Configuration for overlay merge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayMergeConfig {
    /// Path to base video
    pub base_video_path: String,
    /// List of overlay configurations
    pub overlays: Vec<OverlayConfig>,
    /// Output path
    pub output_path: String,
    /// Middle clip timestamps (overlays disabled during these ranges)
    pub middle_clip_timestamps: Vec<MiddleClipTimestamp>,
    /// Optional ASS subtitle path
    pub subtitles_ass_path: Option<String>,
    /// Optional fonts directory for libass
    pub subtitles_fonts_dir: Option<String>,
    /// Video width
    pub width: u32,
    /// Video height
    pub height: u32,
    /// Frame rate
    pub fps: f64,
}

impl Default for OverlayMergeConfig {
    fn default() -> Self {
        Self {
            base_video_path: String::new(),
            overlays: Vec::new(),
            output_path: String::new(),
            middle_clip_timestamps: Vec::new(),
            subtitles_ass_path: None,
            subtitles_fonts_dir: None,
            width: 1920,
            height: 1080,
            fps: 30.0,
        }
    }
}

/// Escape a path for FFmpeg filter arguments (not for shell)
fn escape_filter_path(path: &str) -> String {
    path.replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

/// Build the enable condition for an overlay, excluding middle clip ranges
fn build_enable_condition(
    start: f64,
    duration: f64,
    middle_timestamps: &[MiddleClipTimestamp],
) -> String {
    let base_enable = format!("between(t,{:.6},{:.6})", start, start + duration);

    if middle_timestamps.is_empty() {
        return base_enable;
    }

    let disable_conditions: Vec<String> = middle_timestamps
        .iter()
        .filter(|ts| ts.end > ts.start)
        .map(|ts| format!("between(t,{:.6},{:.6})", ts.start, ts.end))
        .collect();

    if disable_conditions.is_empty() {
        return base_enable;
    }

    let disable_expr = disable_conditions.join("+");
    format!("({base_enable})*(1-({disable_expr}))")
}

/// Build the filter_complex string for overlay merge
/// Returns (filter_string, video_map_label) where video_map_label is the final video output label
pub fn build_overlay_filter_complex(config: &OverlayMergeConfig) -> Result<(String, String), String> {
    if config.overlays.is_empty() {
        return Err("No overlay files provided".to_string());
    }

    let mut filter_parts: Vec<String> = Vec::new();

    // Shift each overlay to its start time
    for (idx, overlay) in config.overlays.iter().enumerate() {
        let overlay_idx = idx + 1; // 0 is base video
        filter_parts.push(format!(
            "[{idx}:v] setpts=PTS+{start:.6}/TB [ovr{num}]",
            idx = overlay_idx,
            start = overlay.start_time,
            num = overlay_idx
        ));
    }

    // Chain overlays onto base video
    let mut last = "0:v".to_string();
    for (idx, overlay) in config.overlays.iter().enumerate() {
        let overlay_idx = idx + 1;
        let tag = format!("tmp{}", overlay_idx);

        let enable = build_enable_condition(
            overlay.start_time,
            overlay.duration,
            &config.middle_clip_timestamps,
        );

        filter_parts.push(format!(
            "[{last}][ovr{idx}] overlay=enable='{enable}' [{tag}]",
            last = last,
            idx = overlay_idx,
            enable = enable,
            tag = tag
        ));
        last = tag;
    }

    // Optional: burn-in ASS subtitles
    let video_map = if let Some(ass_path) = &config.subtitles_ass_path {
        if Path::new(ass_path).exists() {
            let fonts_opt = if let Some(fonts_dir) = &config.subtitles_fonts_dir {
                if Path::new(fonts_dir).is_dir() {
                    format!(":fontsdir='{}'", escape_filter_path(fonts_dir))
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let subs = format!(
                "subtitles='{}'{}",
                escape_filter_path(ass_path),
                fonts_opt
            );
            filter_parts.push(format!("[{last}]{subs}[vsub]"));
            "[vsub]".to_string()
        } else {
            format!("[{last}]")
        }
    } else {
        format!("[{last}]")
    };

    // Audio: pass through base audio
    filter_parts.push("[0:a] volume=1.0 [aout]".to_string());

    Ok((filter_parts.join(";"), video_map))
}

/// Build the complete ffmpeg command for overlay merge
pub fn build_overlay_merge_command(config: &OverlayMergeConfig) -> Result<Vec<String>, String> {
    // Validate inputs
    if config.base_video_path.is_empty() {
        return Err("base_video_path is required".to_string());
    }
    if !Path::new(&config.base_video_path).exists() {
        return Err(format!("Base video not found: {}", config.base_video_path));
    }

    // Filter valid overlays
    let valid_overlays: Vec<&OverlayConfig> = config
        .overlays
        .iter()
        .filter(|o| {
            !o.path.is_empty()
                && Path::new(&o.path).exists()
                && o.duration > 0.0
                && o.start_time >= 0.0
        })
        .collect();

    if valid_overlays.is_empty() {
        return Err("No valid overlay files found".to_string());
    }

    let (filter, video_map) = build_overlay_filter_complex(config)?;

    let mut cmd: Vec<String> = vec![
        "ffmpeg".to_string(),
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-hwaccel".to_string(),
        "none".to_string(),
        "-i".to_string(),
        config.base_video_path.clone(),
    ];

    // Add overlay inputs
    for overlay in &valid_overlays {
        cmd.extend(["-i".to_string(), overlay.path.clone()]);
    }

    // Filter complex
    cmd.extend(["-filter_complex".to_string(), filter]);

    // Map video and audio - use the actual video_map label from the filter
    cmd.extend([
        "-map".to_string(),
        video_map,
        "-map".to_string(),
        "[aout]".to_string(),
    ]);

    // Video codec
    cmd.extend([
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "fast".to_string(),
    ]);

    // Audio codec
    cmd.extend([
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "256k".to_string(),
    ]);

    // Avoid negative timestamps
    cmd.extend([
        "-avoid_negative_ts".to_string(),
        "make_zero".to_string(),
    ]);

    cmd.push(config.output_path.clone());

    Ok(cmd)
}

/// Execute overlay merge using ffmpeg
///
/// Fail-fast on errors.
pub fn merge_overlays(config: OverlayMergeConfig) -> Result<String, String> {
    let cmd = build_overlay_merge_command(&config)?;

    let output = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg failed (exit {}): {}", output.status, stderr));
    }

    if !Path::new(&config.output_path).exists() {
        return Err("Output file was not created".to_string());
    }

    let output_size = std::fs::metadata(&config.output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    if output_size < 1024 {
        return Err(format!(
            "Output file too small ({} bytes), likely empty",
            output_size
        ));
    }

    Ok(config.output_path.clone())
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_enable_condition_no_middle() {
        let enable = build_enable_condition(1.0, 5.0, &[]);
        assert_eq!(enable, "between(t,1.000000,6.000000)");
    }

    #[test]
    fn test_build_enable_condition_with_middle() {
        let middle = vec![MiddleClipTimestamp {
            start: 2.0,
            end: 3.0,
        }];
        let enable = build_enable_condition(1.0, 5.0, &middle);
        assert!(enable.contains("between(t,1.000000,6.000000)"));
        assert!(enable.contains("between(t,2.000000,3.000000)"));
        assert!(enable.contains("1-("));
    }

    #[test]
    fn test_escape_filter_path() {
        assert_eq!(escape_filter_path("/tmp/test.mp4"), "/tmp/test.mp4");
        assert_eq!(
            escape_filter_path("/tmp/test:file.mp4"),
            "/tmp/test\\:file.mp4"
        );
        assert_eq!(
            escape_filter_path("C:\\Users\\test.mp4"),
            "C\\:\\\\Users\\\\test.mp4"
        );
    }

    #[test]
    fn test_build_overlay_filter_complex() {
        let config = OverlayMergeConfig {
            base_video_path: "base.mp4".to_string(),
            overlays: vec![
                OverlayConfig {
                    path: "overlay1.mp4".to_string(),
                    start_time: 1.0,
                    duration: 3.0,
                },
                OverlayConfig {
                    path: "overlay2.mp4".to_string(),
                    start_time: 5.0,
                    duration: 2.0,
                },
            ],
            output_path: "output.mp4".to_string(),
            ..Default::default()
        };

        let (filter, video_map) = build_overlay_filter_complex(&config).unwrap();
        assert!(filter.contains("[1:v]"));
        assert!(filter.contains("[2:v]"));
        assert!(filter.contains("overlay="));
        assert!(filter.contains("[aout]"));
        assert!(video_map.contains("[tmp2]")); // Last overlay tag
    }
}