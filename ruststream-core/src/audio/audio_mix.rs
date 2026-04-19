//! Audio mixing utilities for FFmpeg
//!
//! This module provides filter string builders for generating FFmpeg
//! audio mixing filter expressions (amix, acrossfade, volume).
//!
//! Actual audio processing is handled by `audio_bake.rs` which invokes
//! FFmpeg via subprocess.

// ============================================================================
// Filter String Builders (existing functionality)
// ============================================================================

/// Audio input configuration
#[derive(Debug, Clone)]
pub struct AudioInput {
    pub start_offset: f64,
    pub volume_db: f64,
}

/// Build amix filter for multiple audio inputs
pub fn build_amix_filter(inputs: &[AudioInput], duration: Option<f64>) -> Result<String, String> {
    if inputs.is_empty() {
        return Err("No audio inputs provided".to_string());
    }

    let mut filter_parts = Vec::new();
    let mut concat_labels = String::with_capacity(inputs.len() * 4); // "[a0]" per input

    for (i, input) in inputs.iter().enumerate() {
        let mut parts = Vec::new();

        // Handle start offset with atrim
        if input.start_offset > 0.0 {
            parts.push(format!("atrim=start={}", input.start_offset));
        }

        // Apply volume adjustment
        if (input.volume_db - 0.0).abs() > 0.001 {
            parts.push(format!("volume={}dB", input.volume_db));
        }

        let filter_label = if parts.is_empty() {
            format!("[{}:a]", i)
        } else {
            format!("[{}:a]{}[a{}];", i, parts.join(","), i)
        };

        filter_parts.push(filter_label);
        concat_labels.push_str(&format!("[a{}]", i));
    }

    // Mix all audio streams
    let mix_expr = if duration.is_some() {
        format!("{}amix=inputs={}:duration=first:dropout_transition=0[a]", concat_labels, inputs.len())
    } else {
        format!("{}amix=inputs={}:duration=first[a]", concat_labels, inputs.len())
    };

    Ok(format!("{}{}[a]", filter_parts.join(""), mix_expr))
}

/// Build acrossfade filter for audio concatenation with crossfade
pub fn build_acrossfade_filter(input1_idx: usize, input2_idx: usize, duration_sec: f64) -> String {
    format!(
        "[{}:a][{}:a]acrossfade=c1={}:c2={}:d={}[aout]",
        input1_idx, input2_idx, duration_sec, duration_sec, duration_sec
    )
}

/// Build audio filter chain for background music
pub fn build_background_music_filter(
    main_audio_idx: usize,
    bgm_idx: usize,
    bgm_volume_db: f64,
    main_volume_db: f64,
    fade_in_sec: f64,
    fade_out_sec: f64,
    duration_sec: f64,
) -> Result<String, String> {
    let mut filters = Vec::new();
    
    // Main audio: trim to duration + fade
    let mut main_parts = Vec::new();
    main_parts.push(format!("atrim=0:{},setpts=PTS-STARTPTS", duration_sec));
    if fade_in_sec > 0.0 {
        main_parts.push(format!("afade=t=in:st=0:d={}", fade_in_sec));
    }
    if fade_out_sec > 0.0 {
        main_parts.push(format!("afade=t=out:st={}:d={}", duration_sec - fade_out_sec, fade_out_sec));
    }
    if (main_volume_db - 1.0).abs() > 0.001 {
        main_parts.push(format!("volume={}dB", main_volume_db));
    }
    filters.push(format!("[{}:a]{}[main{}];", main_audio_idx, main_parts.join(","), main_audio_idx));
    
    // BGM: loop + trim + fade
    let mut bgm_parts = Vec::new();
    bgm_parts.push("aloop=loop=-1:size=2e9:start=0".to_string());
    bgm_parts.push(format!("atrim=0:{},setpts=PTS-STARTPTS", duration_sec));
    if fade_in_sec > 0.0 {
        bgm_parts.push(format!("afade=t=in:st=0:d={}", fade_in_sec));
    }
    if fade_out_sec > 0.0 {
        bgm_parts.push(format!("afade=t=out:st={}:d={}", duration_sec - fade_out_sec, fade_out_sec));
    }
    if (bgm_volume_db - 1.0).abs() > 0.001 {
        bgm_parts.push(format!("volume={}dB", bgm_volume_db));
    }
    filters.push(format!("[{}:a]{}[bgm{}];", bgm_idx, bgm_parts.join(","), bgm_idx));
    
    // Mix
    filters.push(format!(
        "[main{}][bgm{}]amix=inputs=2:duration=first:dropout_transition=0[aout]",
        main_audio_idx, bgm_idx
    ));
    
    Ok(filters.join(""))
}

/// Build filter for audio delay
pub fn build_audio_delay_filter(delay_ms: i32) -> String {
    format!("adelay={}|{}", delay_ms, delay_ms)
}

/// Build filter for audio pitch shift (using rubberband)
pub fn build_audio_pitch_filter(semitones: f64) -> String {
    let rate_multiplier = 2.0f64.powf(semitones / 12.0);
    format!("rubberband=tempo={}:pitch=none", rate_multiplier)
}

/// Generate FFmpeg command for audio concatenation
pub fn build_concat_audio_filter(inputs: &[String], _output_has_video: bool) -> String {
    let n = inputs.len();
    let mut filter = String::with_capacity(n * 6 + 30); // "[N:a]" per input + concat

    for (i, _input) in inputs.iter().enumerate() {
        if i > 0 {
            filter.push_str(&format!("[a{}]", i));
        } else {
            filter.push_str(&format!("[{}:a]", i));
        }
    }

    filter.push_str(&format!("concat=n={}:v=0:a=1[aout]", n));

    filter
}

// NOTE: The old native_baking module (ac-ffmpeg based) has been removed.
// Audio baking is now handled by audio_bake.rs (ffmpeg-next based, always available).
// The filter string builders above are still used for FFmpeg CLI filter_complex construction.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amix() {
        let inputs = vec![
            AudioInput {
                start_offset: 0.0,
                volume_db: 0.0,
            },
            AudioInput {
                start_offset: 0.0,
                volume_db: -6.0,
            },
        ];
        let result = build_amix_filter(&inputs, Some(60.0));
        assert!(result.is_ok());
    }

    #[test]
    fn test_background_music() {
        let result = build_background_music_filter(0, 1, -6.0, 0.0, 1.0, 2.0, 60.0);
        assert!(result.is_ok());
        let filter = result.unwrap();
        assert!(filter.contains("amix"));
    }
}