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

// ── Cross-platform path helpers ───────────────────────────────────────────────

/// A path that is guaranteed not to exist on Windows or Linux.
fn nonexistent_path() -> &'static str {
    #[cfg(windows)]
    return "C:\\__ruststream_nonexistent__\\file.mp4";
    #[cfg(not(windows))]
    return "/nonexistent/__ruststream__/file.mp4";
}

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
    let result = probe::probe_full(nonexistent_path());
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
    // Use a path that doesn't exist on either Windows or Linux
    #[cfg(windows)]
    let p = Path::new("C:\\__nonexistent__.mp4");
    #[cfg(not(windows))]
    let p = Path::new("/nonexistent.mp4");

    let result = probe::probe_file(p);
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

/// Test fixture directory layout exists for future real-media coverage
#[test]
fn test_fixture_directory_exists() {
    assert!(Path::new("tests/fixtures").exists(), "tests/fixtures directory should exist");
}

/// Test fixture manifest exists for future media assets
#[test]
fn test_fixture_manifest_exists() {
    assert!(
        Path::new("tests/fixtures/manifest.toml").exists(),
        "fixture manifest should exist"
    );
}
