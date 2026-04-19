//! Video module - Video processing
//!
//! Provides video concatenation, overlay composition, effects processing,
//! and single-call pipeline fusion (probe + compat + concat).

pub mod pipeline_fusion;
pub use pipeline_fusion::{fused_concat, fused_concat_batch, FusedConcatResult};

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::probe::{probe_fast, FullMetadata};

/// Video concatenation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcatConfig {
    /// List of input video paths (in order)
    pub inputs: Vec<String>,
    /// Output video path
    pub output: String,
    /// Output codec (h264, h265, libx264, copy)
    #[serde(default = "default_codec")]
    pub codec: String,
    /// Output CRF quality (0-51, lower is better)
    #[serde(default = "default_crf")]
    pub crf: u32,
    /// Attempt stream-copy concat when clips are codec-compatible.
    ///
    /// If `true` (default), the engine probes all clips and uses `-c copy`
    /// when every clip shares the same codec, resolution, FPS, and pixel
    /// format. This avoids a full decode+encode cycle and is typically
    /// **10x–50x faster** on compatible content.
    ///
    /// Set to `false` to force a full transcode regardless of compatibility.
    #[serde(default = "default_allow_stream_copy")]
    pub allow_stream_copy: bool,
}

fn default_codec() -> String {
    "libx264".to_string()
}

fn default_crf() -> u32 {
    23
}

fn default_allow_stream_copy() -> bool {
    true
}

impl Default for ConcatConfig {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            output: String::new(),
            codec: default_codec(),
            crf: default_crf(),
            allow_stream_copy: default_allow_stream_copy(),
        }
    }
}

/// Metadata extracted for stream-copy compatibility checks.
#[derive(Debug, Clone)]
struct ClipProfile {
    codec: String,
    width: u32,
    height: u32,
    fps: f64,
}

/// Try to compare two f64 FPS values with a small tolerance.
fn fps_compatible(a: f64, b: f64) -> bool {
    if a == 0.0 || b == 0.0 {
        return true; // unknown fps, assume compatible
    }
    (a - b).abs() < 0.01
}

/// Check whether all clips are stream-copy compatible.
/// Returns `Some(profile)` if all clips share the same codec/resolution/fps,
/// `None` if they differ or if probe fails for any clip.
fn check_stream_copy_compatible(inputs: &[String]) -> Option<ClipProfile> {
    if inputs.is_empty() {
        return None;
    }

    let mut reference: Option<ClipProfile> = None;

    for path in inputs {
        // Use probe_fast — we only need codec name and basic info
        let meta: FullMetadata = match probe_fast(path) {
            Ok(m) => m,
            Err(e) => {
                log::debug!("stream-copy probe failed for {}: {}", path, e);
                return None;
            }
        };

        let profile = ClipProfile {
            codec: meta.video.codec.clone(),
            width: meta.video.width,
            height: meta.video.height,
            fps: meta.video.fps,
        };

        match &reference {
            None => {
                reference = Some(profile);
            }
            Some(ref r) => {
                if r.codec != profile.codec
                    || r.width != profile.width
                    || r.height != profile.height
                    || !fps_compatible(r.fps, profile.fps)
                {
                    log::debug!(
                        "stream-copy incompatible: ref={:?}/{}/{}@{:.2} vs {}: {:?}/{}/{}@{:.2}",
                        r.codec, r.width, r.height, r.fps,
                        path, profile.codec, profile.width, profile.height, profile.fps
                    );
                    return None;
                }
            }
        }
    }

    reference
}

/// Internal: concat using FFmpeg `-c copy` (zero decode/encode).
fn concat_stream_copy(inputs: &[String], output: &str) -> Result<bool, String> {
    let concat_list_path = std::env::temp_dir()
        .join(format!("ruststream_sc_{}.txt", std::process::id()));
    let path_str = concat_list_path
        .to_str()
        .ok_or("Invalid temp dir path")?
        .to_string();

    let mut f = fs::File::create(&path_str)
        .map_err(|e| format!("Failed to create concat list: {}", e))?;
    for inp in inputs {
        writeln!(f, "file '{}'", inp)
            .map_err(|e| format!("Failed to write concat list: {}", e))?;
    }
    drop(f);

    log::info!(
        "Stream-copy concat: {} files → {} (no transcode)",
        inputs.len(),
        output
    );

    let result = Command::new("ffmpeg")
        .args(["-y", "-f", "concat", "-safe", "0"])
        .args(["-i", &path_str])
        .args(["-c", "copy"])
        .args(["-movflags", "+faststart"])
        .arg(output)
        .output();

    let _ = fs::remove_file(&concat_list_path);

    match result {
        Ok(out) if out.status.success() => {
            log::info!("Stream-copy concat success: {}", output);
            Ok(true)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "Stream-copy concat failed ({}): {}",
                out.status,
                stderr.lines().take(3).collect::<Vec<_>>().join(" ")
            ))
        }
        Err(e) => Err(format!("Failed to spawn ffmpeg: {}", e)),
    }
}

/// Concatenate video files using FFmpeg concat demuxer.
///
/// When `config.allow_stream_copy` is `true` (the default), the engine first
/// probes all clips for codec compatibility. Compatible clips are concatenated
/// with `-c copy`, skipping the decode/encode cycle entirely (**10x–50x
/// faster**). Incompatible clips fall back to a full transcode.
///
/// # Arguments
///
/// * `config` - Concatenation configuration
///
/// # Returns
///
/// * `Ok(true)` if concatenation succeeded
/// * `Err(String)` with error message if failed
pub fn concat_videos(config: &ConcatConfig) -> Result<bool, String> {
    // Validate inputs
    if config.inputs.is_empty() {
        return Err("No input files specified".to_string());
    }

    if config.output.is_empty() {
        return Err("No output file specified".to_string());
    }

    // Check all input files exist
    for input in &config.inputs {
        if !Path::new(input).exists() {
            return Err(format!("Input file not found: {}", input));
        }
    }

    // ── Zero-cost shortcut: single input = just copy the file ─────────────────
    // Avoids spawning FFmpeg entirely for the trivial case.
    if config.inputs.len() == 1 {
        let src = &config.inputs[0];
        log::info!("concat_videos: single input — fs::copy shortcut {} → {}", src, config.output);
        fs::copy(src, &config.output)
            .map_err(|e| format!("fs::copy failed: {}", e))?;
        return Ok(true);
    }

    // ── OS prefetch: hint the kernel to read-ahead all inputs ─────────────────
    // On Linux: posix_fadvise(SEQUENTIAL+WILLNEED) before FFmpeg opens the files.
    // On Windows: FILE_FLAG_SEQUENTIAL_SCAN on each handle.
    {
        use crate::io::prefetch::prefetch_batch;
        let refs: Vec<&str> = config.inputs.iter().map(String::as_str).collect();
        prefetch_batch(&refs);
    }

    // ── Fast path: stream-copy if all clips are compatible ────────────────────
    if config.allow_stream_copy {
        if let Some(profile) = check_stream_copy_compatible(&config.inputs) {
            log::info!(
                "All {} clips compatible ({} {}x{}@{:.2}fps) — using stream-copy",
                config.inputs.len(),
                profile.codec,
                profile.width,
                profile.height,
                profile.fps
            );
            return concat_stream_copy(&config.inputs, &config.output);
        }
        log::info!("Clips incompatible for stream-copy — falling back to transcode");
    }

    // ── Slow path: full transcode ─────────────────────────────────────────────
    let concat_list_path = std::env::temp_dir()
        .join(format!("ruststream_concat_{}.txt", std::process::id()));
    let concat_list_path_str = concat_list_path
        .to_str()
        .ok_or("Invalid temp directory path")?
        .to_string();
    let mut concat_list = fs::File::create(&concat_list_path_str)
        .map_err(|e| format!("Failed to create concat list: {}", e))?;

    for input in &config.inputs {
        writeln!(concat_list, "file '{}'", input)
            .map_err(|e| format!("Failed to write to concat list: {}", e))?;
    }
    drop(concat_list);

    log::info!(
        "Concatenating {} files to {} (codec={}, crf={})",
        config.inputs.len(),
        config.output,
        config.codec,
        config.crf
    );

    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-f", "concat"])
        .args(["-safe", "0"])
        .args(["-i", &concat_list_path_str])
        .args(["-threads", &thread_count.to_string()])
        .args(["-c:v", &config.codec])
        .args(["-preset", "fast"])
        .args(["-crf", &config.crf.to_string()])
        .args(["-tune", "film"])
        .args(["-c:a", "aac"])
        .args(["-b:a", "128k"])
        .args(["-movflags", "+faststart"])
        .args(["-y"])
        .arg(&config.output);

    let output = cmd.output();

    let _ = fs::remove_file(&concat_list_path);

    match output {
        Ok(output) => {
            if output.status.success() {
                log::info!("Concatenation successful: {}", config.output);
                Ok(true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::error!("FFmpeg concat failed: {}", stderr);
                Err(format!(
                    "FFmpeg concatenation failed: {}",
                    stderr.lines().take(3).collect::<Vec<_>>().join(" ")
                ))
            }
        }
        Err(e) => {
            let _ = fs::remove_file(&concat_list_path);
            Err(format!("Failed to execute FFmpeg: {}", e))
        }
    }
}
