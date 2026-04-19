//! Pipeline fusion — probe → compatibility check → concat in a single call.
//!
//! Instead of making the caller:
//!   1. probe each clip individually
//!   2. check compatibility
//!   3. call concat_videos
//!
//! `fused_concat` does all three in one call with a single pass over the
//! input list, eliminating redundant I/O and subprocess overhead.

use std::path::Path;
use crate::core::{MediaError, MediaErrorCode, MediaResult};
use crate::probe::probe_fast;
use crate::video::{ConcatConfig, concat_videos};

/// Result of a fused concat operation.
#[derive(Debug, Clone)]
pub struct FusedConcatResult {
    /// Path to the output file.
    pub output_path: String,
    /// Whether stream-copy was used (true = no re-encode).
    pub stream_copy_used: bool,
    /// Total duration of output in seconds (sum of input durations).
    pub total_duration_secs: f64,
    /// Number of input clips processed.
    pub clip_count: usize,
}

/// Probe → compatibility check → concat in ONE call.
///
/// # What this saves vs. calling each step separately
/// - Probes each file **once** using `probe_fast` (no decoder open)
/// - Checks stream-copy compatibility as part of the same loop
/// - Feeds result directly into `concat_videos` with `allow_stream_copy`
///   pre-set based on probe results
/// - Zero redundant file opens
///
/// # Arguments
/// - `inputs`: ordered list of video file paths
/// - `output`: destination file path
/// - `force_transcode`: set `true` to disable stream-copy even if compatible
pub fn fused_concat(
    inputs: &[&str],
    output: &str,
    force_transcode: bool,
) -> MediaResult<FusedConcatResult> {
    if inputs.is_empty() {
        return Err(MediaError::new(
            MediaErrorCode::InvalidInput,
            "fused_concat: no input files",
        ));
    }
    if output.is_empty() {
        return Err(MediaError::new(
            MediaErrorCode::InvalidInput,
            "fused_concat: no output path",
        ));
    }

    // ── Single-pass probe + compat check ─────────────────────────────────────
    let mut total_duration = 0.0f64;
    let mut all_compatible = !force_transcode;
    let mut ref_codec = String::new();
    let mut ref_width = 0u32;
    let mut ref_height = 0u32;
    let mut ref_fps = 0.0f64;

    for (i, &path) in inputs.iter().enumerate() {
        if !Path::new(path).exists() {
            return Err(MediaError::new(
                MediaErrorCode::IoFileNotFound,
                format!("fused_concat: input not found: {}", path),
            ));
        }

        let meta = probe_fast(path).map_err(|e| {
            MediaError::new(
                MediaErrorCode::DecodeFailed,
                format!("fused_concat: probe failed for {}: {}", path, e),
            )
        })?;

        total_duration += meta.format.duration_secs;

        if all_compatible {
            if i == 0 {
                ref_codec  = meta.video.codec.clone();
                ref_width  = meta.video.width;
                ref_height = meta.video.height;
                ref_fps    = meta.video.fps;
            } else {
                let fps_ok = ref_fps == 0.0 || meta.video.fps == 0.0
                    || (ref_fps - meta.video.fps).abs() < 0.01;
                if meta.video.codec != ref_codec
                    || meta.video.width  != ref_width
                    || meta.video.height != ref_height
                    || !fps_ok
                {
                    log::debug!(
                        "fused_concat: incompatible clip {} ({} {}x{}) vs ref ({} {}x{})",
                        path,
                        meta.video.codec, meta.video.width, meta.video.height,
                        ref_codec, ref_width, ref_height,
                    );
                    all_compatible = false;
                }
            }
        }
    }

    // ── Concat (stream-copy or transcode) ─────────────────────────────────────
    let config = ConcatConfig {
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
        output: output.to_string(),
        allow_stream_copy: all_compatible,
        ..Default::default()
    };

    concat_videos(&config)
        .map_err(|e| MediaError::new(MediaErrorCode::ConcatFailed, e))?;

    Ok(FusedConcatResult {
        output_path: output.to_string(),
        stream_copy_used: all_compatible,
        total_duration_secs: total_duration,
        clip_count: inputs.len(),
    })
}

/// Batch fused concat: process multiple independent groups in parallel.
///
/// Each element of `jobs` is a `(inputs, output)` pair.
/// Jobs run in parallel via rayon; each job is internally sequential.
pub fn fused_concat_batch(
    jobs: Vec<(Vec<String>, String)>,
    force_transcode: bool,
) -> Vec<MediaResult<FusedConcatResult>> {
    use rayon::prelude::*;
    jobs.into_par_iter()
        .map(|(inputs, output)| {
            let refs: Vec<&str> = inputs.iter().map(String::as_str).collect();
            fused_concat(&refs, &output, force_transcode)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fused_concat_empty_inputs() {
        let result = fused_concat(&[], "out.mp4", false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, MediaErrorCode::InvalidInput);
    }

    #[test]
    fn test_fused_concat_empty_output() {
        let result = fused_concat(&["a.mp4"], "", false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, MediaErrorCode::InvalidInput);
    }

    #[test]
    fn test_fused_concat_missing_file() {
        #[cfg(windows)]
        let missing = "C:\\__missing__\\clip.mp4";
        #[cfg(not(windows))]
        let missing = "/tmp/__missing_clip__.mp4";

        let result = fused_concat(&[missing], "out.mp4", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_fused_concat_batch_empty() {
        let results = fused_concat_batch(vec![], false);
        assert!(results.is_empty());
    }
}
