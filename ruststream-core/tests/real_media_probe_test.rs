//! Smoke tests using tiny valid media generated at runtime.

use ruststream_core::probe;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

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

    let metadata = probe::probe_full(wav_path.to_str().unwrap()).expect("generated wav fixture should probe");

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

    let metadata = probe::probe_file(&wav_path).expect("probe_file should handle generated wav fixture");
    assert_eq!(metadata.format.size_bytes, 60);
    assert!(metadata.audio.is_some());
}
