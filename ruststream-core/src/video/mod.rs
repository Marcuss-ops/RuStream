//! Video module - Video processing
//!
//! Provides video concatenation, overlay composition, and effects processing.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

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
}

fn default_codec() -> String {
    "libx264".to_string()
}

fn default_crf() -> u32 {
    23
}

impl Default for ConcatConfig {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            output: String::new(),
            codec: default_codec(),
            crf: default_crf(),
        }
    }
}

/// Concatenate video files using FFmpeg concat demuxer.
///
/// This function:
/// 1. Validates all input files exist
/// 2. Creates a temporary concat list file
/// 3. Runs FFmpeg to concatenate
/// 4. Cleans up temporary files
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

    // Create temporary concat list file in system temp directory
    let concat_list_path = std::env::temp_dir()
        .join(format!("ruststream_concat_{}.txt", std::process::id()));
    let concat_list_path_str = concat_list_path
        .to_str()
        .ok_or("Invalid temp directory path")?
        .to_string();
    let mut concat_list = fs::File::create(&concat_list_path_str)
        .map_err(|e| format!("Failed to create concat list: {}", e))?;

    // Write input file paths to concat list
    for input in &config.inputs {
        writeln!(concat_list, "file '{}'", input)
            .map_err(|e| format!("Failed to write to concat list: {}", e))?;
    }

    drop(concat_list); // Close file

    log::info!(
        "Concatenating {} files to {} (codec={}, crf={})",
        config.inputs.len(),
        config.output,
        config.codec,
        config.crf
    );

    // Build FFmpeg command with performance tuning
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

    // Clean up concat list file
    let _ = fs::remove_file(&concat_list_path);

    match output {
        Ok(output) => {
            if output.status.success() {
                log::info!("Concatenation successful: {}", config.output);
                Ok(true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::error!("FFmpeg concat failed: {}", stderr);
                Err(format!("FFmpeg concatenation failed: {}", stderr.lines().take(3).collect::<Vec<_>>().join(" ")))
            }
        }
        Err(e) => {
            let _ = fs::remove_file(&concat_list_path);
            Err(format!("Failed to execute FFmpeg: {}", e))
        }
    }
}
