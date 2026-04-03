//! Integration tests for RustStream Core
//!
//! These tests verify end-to-end functionality of the library.

use ruststream_core::{
    audio::{audio_mix, apply_volume},
    core::MediaErrorCode,
    probe, get_info, VERSION,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Test audio mix kernel with zero inputs
#[test]
fn test_audio_mix_no_inputs() {
    let output_buffer = &mut [0.0f32; 1024];
    let input_buffers: Vec<&[f32]> = vec![];
    let volumes: Vec<f32> = vec![];
    
    audio_mix(output_buffer, &input_buffers, &volumes);
    assert!(output_buffer.iter().all(|&x| x == 0.0), "Empty mix should produce silence");
}

/// Test audio mix kernel with single input
#[test]
fn test_audio_mix_single_input() {
    let input = [0.5f32; 1024];
    let input_buffers = vec![&input[..]];
    let volumes = vec![1.0f32];
    let output_buffer = &mut [0.0f32; 1024];
    
    audio_mix(output_buffer, &input_buffers, &volumes);
    
    for i in 0..1024 {
        assert!((output_buffer[i] - input[i]).abs() < 1e-6, "Single input should pass through");
    }
}

/// Test audio mix kernel with multiple inputs
#[test]
fn test_audio_mix_multiple_inputs() {
    let input1 = [0.3f32; 1024];
    let input2 = [0.4f32; 1024];
    let input_buffers = vec![&input1[..], &input2[..]];
    let volumes = vec![1.0f32, 1.0f32];
    let output_buffer = &mut [0.0f32; 1024];
    
    audio_mix(output_buffer, &input_buffers, &volumes);
    
    for i in 0..1024 {
        assert!((output_buffer[i] - 0.7).abs() < 1e-6, "Mix should sum inputs");
    }
}

/// Test audio mix with volume attenuation
#[test]
fn test_audio_mix_with_volume() {
    let input = [1.0f32; 1024];
    let input_buffers = vec![&input[..]];
    let volumes = vec![0.5f32];
    let output_buffer = &mut [0.0f32; 1024];
    
    audio_mix(output_buffer, &input_buffers, &volumes);
    
    for i in 0..1024 {
        assert!((output_buffer[i] - 0.5).abs() < 1e-6, "Volume should attenuate");
    }
}

/// Test apply volume with zero gain
#[test]
fn test_apply_volume_zero() {
    let mut buffer = [1.0f32; 1024];
    apply_volume(&mut buffer, 0.0);
    assert!(buffer.iter().all(|&x| x == 0.0), "Zero volume should produce silence");
}

/// Test apply volume with unity gain
#[test]
fn test_apply_volume_unity() {
    let mut buffer = [0.5f32; 1024];
    apply_volume(&mut buffer, 1.0);
    assert!(buffer.iter().all(|&x| (x - 0.5).abs() < 1e-6), "Unity gain should preserve input");
}

/// Test apply volume with attenuation
#[test]
fn test_apply_volume_attenuate() {
    let mut buffer = [1.0f32; 1024];
    apply_volume(&mut buffer, 0.5);
    assert!(buffer.iter().all(|&x| (x - 0.5).abs() < 1e-6), "0.5 volume should halve samples");
}

/// Test probe non-existent file
#[test]
fn test_probe_nonexistent_file() {
    let result = probe::probe_full("/nonexistent/file.mp4");
    assert!(result.is_err(), "Probing non-existent file should fail");
    
    if let Err(e) = result {
        assert_eq!(e.code, MediaErrorCode::IoFileNotFound, "Expected file not found error");
    }
}

/// Test error handling for invalid media
#[test]
fn test_error_handling_invalid_media() {
    let temp_dir = TempDir::new().unwrap();
    let fake_media = temp_dir.path().join("fake.mp4");
    fs::write(&fake_media, b"not a real mp4").unwrap();
    
    let result = probe::probe_full(fake_media.to_str().unwrap());
    assert!(result.is_err(), "Probing invalid media should fail");
    
    if let Err(e) = result {
        assert!(
            matches!(e.code, MediaErrorCode::DecodeFailed | MediaErrorCode::IoFileNotFound),
            "Expected decode or IO error, got: {:?}",
            e.code
        );
    }
}

/// Test probe file path API
#[test]
fn test_probe_file_path() {
    let result = probe::probe_file(Path::new("/nonexistent.mp4"));
    assert!(result.is_err(), "Probing non-existent file should fail");
    
    if let Err(e) = result {
        assert_eq!(e.code, MediaErrorCode::IoFileNotFound);
    }
}

/// Test library info
#[test]
fn test_library_info() {
    let info = get_info();
    
    assert_eq!(info.version, VERSION);
    assert!(info.cpu_cores > 0);
    assert!(info.physical_cores > 0);
}

/// Test version string
#[test]
fn test_version_not_empty() {
    assert!(!VERSION.is_empty(), "Version string should not be empty");
    assert!(VERSION.contains('.'), "Version should contain dot separator");
}
