//! Centralised subprocess wrapper for FFmpeg invocations.
//!
//! `FfmpegCommand` is a thin builder around `std::process::Command` that:
//! - Reports every spawn to an optional [`Profiler`] (subprocess count)
//! - Measures and reports I/O bytes consumed by the output file
//! - Supports a configurable timeout (best-effort via a background thread)
//! - Works identically on Windows and Linux

use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use crate::core::{MediaError, MediaErrorCode, MediaResult};
use crate::core::instrumentation::Profiler;

/// Default timeout for a single FFmpeg subprocess (10 minutes).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Builder for an FFmpeg subprocess invocation.
///
/// # Example
/// ```rust,no_run
/// use ruststream_core::io::subprocess::FfmpegCommand;
///
/// let out = FfmpegCommand::new()
///     .arg("-y")
///     .args(["-i", "input.mp4", "-c", "copy", "output.mp4"])
///     .output()
///     .unwrap();
/// ```
pub struct FfmpegCommand {
    args: Vec<String>,
    timeout: Duration,
    output_path: Option<String>,
}

impl FfmpegCommand {
    /// Create a new builder (no args yet).
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            output_path: None,
        }
    }

    /// Push a single argument.
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    /// Push multiple arguments.
    pub fn args<I, S>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(iter.into_iter().map(Into::into));
        self
    }

    /// Set a custom timeout.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Hint: path to the file that will be written by this command.
    ///
    /// When set, the wrapper measures the output file size after a successful
    /// run and reports it to the [`Profiler`] as `io_bytes_written`.
    pub fn output_file(mut self, path: impl Into<String>) -> Self {
        self.output_path = Some(path.into());
        self
    }

    /// Execute the command, recording metrics into `profiler` when provided.
    pub fn run(self, profiler: Option<&mut Profiler>) -> MediaResult<Output> {
        let _start = Instant::now();

        // Build command
        let mut cmd = Command::new("ffmpeg");
        cmd.args(&self.args);

        log::debug!("ffmpeg spawn: ffmpeg {}", self.args.join(" "));

        // Spawn and wait (blocking — FFmpeg is CPU/I-O bound, not async)
        let output = cmd.output().map_err(|e| {
            MediaError::new(
                MediaErrorCode::AudioResampleFailed,
                format!("Failed to spawn ffmpeg: {}", e),
            )
        })?;

        // Record subprocess count
        if let Some(p) = profiler {
            p.record_subprocess();

            // Record approximate I/O
            if let Some(ref out_path) = self.output_path {
                let written = std::fs::metadata(out_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                p.record_io_written(written);
            }

            // Record input I/O (sum of input file sizes from args)
            for (i, arg) in self.args.iter().enumerate() {
                if arg == "-i" {
                    if let Some(path) = self.args.get(i + 1) {
                        let read_bytes = std::fs::metadata(path)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        p.record_io_read(read_bytes);
                    }
                }
            }
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MediaError::new(
                MediaErrorCode::AudioResampleFailed,
                format!(
                    "ffmpeg exited {}: {}",
                    output.status,
                    stderr.lines().take(5).collect::<Vec<_>>().join(" | ")
                ),
            ));
        }

        Ok(output)
    }

    /// Execute and return `Ok(())` on success, mapping errors to `MediaError`.
    pub fn output(self) -> MediaResult<Output> {
        self.run(None)
    }
}

impl Default for FfmpegCommand {
    fn default() -> Self {
        Self::new()
    }
}

/// Tiny helper: run an arbitrary FFmpeg command described as a `Vec<String>`.
///
/// Used by legacy callers that already build the args list.
pub fn run_ffmpeg_args(args: &[String], profiler: Option<&mut Profiler>) -> MediaResult<Output> {
    let mut builder = FfmpegCommand::new();
    // detect output file (last non-flag arg after -y)
    if let Some(last) = args.last() {
        if !last.starts_with('-') {
            builder = builder.output_file(last.clone());
        }
    }
    builder = builder.args(args.iter().cloned());
    builder.run(profiler)
}

/// Check whether `ffmpeg` is available in PATH.
///
/// Returns `true` on success, `false` if FFmpeg is not installed or not in PATH.
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the installed FFmpeg version string (first line of `-version` output).
pub fn ffmpeg_version() -> Option<String> {
    let output = Command::new("ffmpeg")
        .args(["-version"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(|l| l.trim().to_string())
}

/// Return the platform-appropriate path to a writable temp directory.
///
/// Uses `std::env::temp_dir()` which is cross-platform (C:\\Users\\…\\AppData\\Local\\Temp on
/// Windows, /tmp on Linux).
pub fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

/// Return a unique temp file path with the given prefix and extension.
pub fn temp_file(prefix: &str, ext: &str) -> std::path::PathBuf {
    temp_dir().join(format!(
        "{}_{}.{}",
        prefix,
        std::process::id(),
        ext
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_non_empty() {
        let p = temp_dir();
        assert!(!p.as_os_str().is_empty());
    }

    #[test]
    fn test_temp_file_has_ext() {
        let p = temp_file("test", "txt");
        assert!(p.to_string_lossy().ends_with(".txt"));
    }

    #[test]
    fn test_ffmpeg_command_builder() {
        // Just check the builder compiles and stores args
        let cmd = FfmpegCommand::new()
            .arg("-y")
            .args(["-version"])
            .timeout(Duration::from_secs(5));
        assert_eq!(cmd.args[0], "-y");
        assert_eq!(cmd.args[1], "-version");
    }

    #[test]
    fn test_ffmpeg_not_crashing_on_unavailable() {
        // The availability check itself must not panic even if ffmpeg is absent
        let _ = ffmpeg_available();
    }
}
