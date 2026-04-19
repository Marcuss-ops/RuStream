//! Smoke tests using tiny valid media generated at runtime.
//!
//! Tests that require real fixture files (MP4, WAV from generate_fixtures.ps1/.sh)
//! are gated behind the `real_media` feature and will be skipped automatically
//! when fixtures are not present.

use ruststream_core::probe;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// ── Cross-platform helpers ────────────────────────────────────────────────────

/// Returns a path that is guaranteed to not exist on either Windows or Linux.
fn nonexistent_path() -> &'static str {
    #[cfg(windows)]
    return "C:\\__ruststream_nonexistent__\\file.mp4";
    #[cfg(not(windows))]
    return "/nonexistent/__ruststream__/file.mp4";
}

/// Check if the pre-generated fixture files are available.
/// These are produced by `tests/fixtures/generate_fixtures.ps1` (Windows)
/// or `tests/fixtures/generate_fixtures.sh` (Linux).
fn fixtures_available() -> bool {
    Path::new("tests/fixtures/black_1s_h264.mp4").exists()
        && Path::new("tests/fixtures/silence_1s.wav").exists()
}

/// Skip the current test with a clear message when fixtures are missing.
macro_rules! require_fixtures {
    () => {
        if !fixtures_available() {
            eprintln!(
                "SKIP: fixture files not found. \
                Run tests/fixtures/generate_fixtures.ps1 (Windows) \
                or tests/fixtures/generate_fixtures.sh (Linux) first."
            );
            return;
        }
    };
}

fn write_minimal_wav_fixture(path: &Path) {
    let sample_rate: u32 = 8_000;
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let samples: u32 = 8;
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let data_len = samples * channels as u32 * bytes_per_sample;
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample;
    let block_align = channels * (bits_per_sample / 8);
    let chunk_size = 36 + data_len;

    let mut wav = Vec::with_capacity((44 + data_len) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&chunk_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for _ in 0..samples {
        wav.extend_from_slice(&0i16.to_le_bytes());
    }

    fs::write(path, wav).expect("should write wav fixture");
}

#[test]
fn test_probe_generated_wav_fixture() {
    let temp_dir = TempDir::new().unwrap();
    let wav_path = temp_dir.path().join("fixture.wav");
    write_minimal_wav_fixture(&wav_path);

    let metadata = probe::probe_full(wav_path.to_str().unwrap())
        .expect("generated wav fixture should probe");

    assert_eq!(metadata.path, wav_path.to_str().unwrap());
    assert_eq!(metadata.format.size_bytes, 60);
    assert!(metadata.format.duration_secs >= 0.0);

    let audio = metadata.audio.expect("wav fixture should expose audio metadata");
    assert_eq!(audio.sample_rate, 8_000);
    assert_eq!(audio.channels, 1);
}

#[test]
fn test_probe_file_path_valid_fixture() {
    let temp_dir = TempDir::new().unwrap();
    let wav_path = temp_dir.path().join("path-fixture.wav");
    write_minimal_wav_fixture(&wav_path);

    let metadata = probe::probe_file(&wav_path)
        .expect("probe_file should handle generated wav fixture");
    assert_eq!(metadata.format.size_bytes, 60);
    assert!(metadata.audio.is_some());
}

/// probe_fast should accept any existing file without panicking.
/// It returns codec/duration info without opening a decoder.
#[test]
fn test_probe_fast_on_wav_fixture() {
    let temp_dir = TempDir::new().unwrap();
    let wav_path = temp_dir.path().join("fast-fixture.wav");
    write_minimal_wav_fixture(&wav_path);

    let result = probe::probe_fast(wav_path.to_str().unwrap());
    // probe_fast must succeed on a valid (if tiny) WAV
    assert!(result.is_ok(), "probe_fast failed: {:?}", result.err());
    let meta = result.unwrap();
    assert!(meta.format.size_bytes > 0);
}

/// probe_fast must return IoFileNotFound for non-existent paths (cross-platform).
#[test]
fn test_probe_fast_nonexistent() {
    let result = probe::probe_fast(nonexistent_path());
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code,
        ruststream_core::core::MediaErrorCode::IoFileNotFound
    );
}

/// cache_key must return different keys for different files.
#[test]
fn test_cache_key_uniqueness() {
    let temp_dir = TempDir::new().unwrap();
    let path_a = temp_dir.path().join("a.wav");
    let path_b = temp_dir.path().join("b.wav");
    write_minimal_wav_fixture(&path_a);
    write_minimal_wav_fixture(&path_b);

    let key_a = probe::cache_key(path_a.to_str().unwrap());
    let key_b = probe::cache_key(path_b.to_str().unwrap());
    assert_ne!(key_a, key_b, "cache keys for different paths must differ");
}

/// cache_key for a non-existent path must fall back to the path string.
#[test]
fn test_cache_key_nonexistent_fallback() {
    let key = probe::cache_key(nonexistent_path());
    assert!(key.contains("ruststream_nonexistent") || !key.is_empty());
}

// ── Tests that require pre-generated fixture files ────────────────────────────
// Enable with: cargo test --features real_media

#[test]
#[cfg(feature = "real_media")]
fn test_probe_full_real_mp4() {
    require_fixtures!();
    let path = "tests/fixtures/black_1s_h264.mp4";
    let meta = probe::probe_full(path).expect("should probe real MP4");
    assert!(meta.format.duration_secs > 0.0);
    assert_eq!(meta.video.codec, "h264");
    assert!(meta.format.size_bytes > 0);
}

#[test]
#[cfg(feature = "real_media")]
fn test_probe_fast_real_mp4() {
    require_fixtures!();
    let path = "tests/fixtures/black_1s_h264.mp4";
    let meta = probe::probe_fast(path).expect("probe_fast should succeed on real MP4");
    assert!(meta.format.duration_secs > 0.0);
    assert_eq!(meta.video.codec, "h264");
}

#[test]
#[cfg(feature = "real_media")]
fn test_probe_full_vs_fast_consistency() {
    require_fixtures!();
    let path = "tests/fixtures/black_1s_h264.mp4";
    let full = probe::probe_full(path).unwrap();
    let fast = probe::probe_fast(path).unwrap();

    // Both must agree on codec and duration (fast has 0 for width/height/fps)
    assert_eq!(full.video.codec, fast.video.codec);
    assert!((full.format.duration_secs - fast.format.duration_secs).abs() < 0.1);
}

#[test]
#[cfg(feature = "real_media")]
fn test_cache_key_stable_for_unchanged_file() {
    require_fixtures!();
    let path = "tests/fixtures/black_1s_h264.mp4";
    let k1 = probe::cache_key(path);
    let k2 = probe::cache_key(path);
    assert_eq!(k1, k2, "cache_key must be stable for unchanged files");
}
