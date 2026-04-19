//! Comprehensive benchmark suite for RustStream Core.
//!
//! Run with: `cargo bench`
//! Requires: `criterion` in dev-dependencies
//!
//! Real-media benchmarks (probe_mp4_*, concat_*, etc.) require fixture files:
//!   Windows: cd ruststream-core && .\tests\fixtures\generate_fixtures.ps1
//!   Linux:   cd ruststream-core && bash tests/fixtures/generate_fixtures.sh

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use ruststream_core::core::audio_graph::{AudioGraphConfig, AudioInput};
use ruststream_core::core::instrumentation::Profiler;
use ruststream_core::core::timeline::MediaTimelinePlan;
use ruststream_core::{init, probe, VERSION};
use std::path::Path;

// ============================================================================
// Helpers
// ============================================================================

/// Cross-platform temp dir path usable in bench args.
fn tmpdir() -> std::path::PathBuf {
    std::env::temp_dir()
}

/// Returns true if the fixture file exists (real-media benches skip if absent).
fn fixture(name: &str) -> Option<String> {
    let p = format!("tests/fixtures/{}", name);
    if Path::new(&p).exists() {
        Some(p)
    } else {
        None
    }
}

// ============================================================================
// Audio Graph Benchmarks
// ============================================================================

fn bench_audio_graph_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_graph_validation");
    let tmp = tmpdir();

    for size in [1, 5, 10, 20].iter() {
        let mut config = AudioGraphConfig::new(format!("bench-{}", size));
        for i in 0..*size {
            config = config.add_input(AudioInput::new(
                format!("input-{}", i),
                tmp.join(format!("audio-{}.mp3", i)).to_string_lossy().into_owned(),
                if i == 0 { "voiceover" } else { "music" },
            ));
        }

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| config.validate());
        });
    }

    group.finish();
}

fn bench_audio_graph_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_graph_build");
    let tmp = tmpdir();

    group.bench_function("single_input", |b| {
        b.iter(|| {
            AudioGraphConfig::new("test")
                .add_input(AudioInput::new(
                    "vo",
                    tmp.join("vo.mp3").to_string_lossy().into_owned(),
                    "voiceover",
                ))
                .with_output_sample_rate(48000)
                .with_output_channels(2)
        });
    });

    group.bench_function("multiple_inputs", |b| {
        b.iter(|| {
            AudioGraphConfig::new("test")
                .add_input(AudioInput::new(
                    "vo",
                    tmp.join("vo.mp3").to_string_lossy().into_owned(),
                    "voiceover",
                ))
                .add_input(
                    AudioInput::new(
                        "music",
                        tmp.join("music.mp3").to_string_lossy().into_owned(),
                        "music",
                    )
                    .with_volume(0.15),
                )
                .add_input(AudioInput::new(
                    "sfx",
                    tmp.join("sfx.mp3").to_string_lossy().into_owned(),
                    "sfx",
                ))
                .with_output_sample_rate(48000)
                .with_output_channels(2)
        });
    });

    group.finish();
}

// ============================================================================
// SIMD Audio Kernel Benchmarks
// ============================================================================

fn bench_audio_mix_simd(c: &mut Criterion) {
    use ruststream_core::audio::audio_mix;

    let mut group = c.benchmark_group("audio_mix_simd");

    for &samples in &[1024usize, 4096, 16384, 65536, 262144] {
        let input1: Vec<f32> = vec![0.3f32; samples];
        let input2: Vec<f32> = vec![0.4f32; samples];
        let input_buffers: Vec<&[f32]> = vec![&input1, &input2];
        let volumes = vec![0.8f32, 0.6f32];
        let mut output = vec![0.0f32; samples];

        group.throughput(Throughput::Elements(samples as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(samples),
            &samples,
            |b, _| {
                b.iter(|| {
                    audio_mix(&mut output, &input_buffers, &volumes);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Probe Benchmarks (real-media — skipped if fixtures absent)
// ============================================================================

fn bench_probe_mp4_small(c: &mut Criterion) {
    let Some(path) = fixture("black_1s_h264.mp4") else {
        eprintln!("bench_probe_mp4_small: fixture missing, skipping");
        return;
    };

    let mut group = c.benchmark_group("probe");
    group.bench_function("mp4_small_full_cold", |b| {
        b.iter(|| probe::probe_full(&path).ok());
    });
    group.bench_function("mp4_small_fast_cold", |b| {
        b.iter(|| probe::probe_fast(&path).ok());
    });
    group.finish();
}

fn bench_probe_mp4_medium(c: &mut Criterion) {
    let Some(path) = fixture("black_10s_h264.mp4") else {
        eprintln!("bench_probe_mp4_medium: fixture missing, skipping");
        return;
    };

    let mut group = c.benchmark_group("probe");
    group.bench_function("mp4_medium_full_cold", |b| {
        b.iter(|| probe::probe_full(&path).ok());
    });
    group.bench_function("mp4_medium_fast_cold", |b| {
        b.iter(|| probe::probe_fast(&path).ok());
    });
    group.finish();
}

fn bench_probe_wav_short(c: &mut Criterion) {
    let Some(path) = fixture("silence_1s.wav") else {
        eprintln!("bench_probe_wav_short: fixture missing, skipping");
        return;
    };

    let mut group = c.benchmark_group("probe");
    group.bench_function("wav_1s_full", |b| {
        b.iter(|| probe::probe_full(&path).ok());
    });
    group.bench_function("wav_1s_fast", |b| {
        b.iter(|| probe::probe_fast(&path).ok());
    });
    group.finish();
}

// ============================================================================
// Cache Key Benchmark
// ============================================================================

fn bench_cache_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_key");

    // Cold (nonexistent path — fallback to path-only)
    group.bench_function("nonexistent_path_fallback", |b| {
        b.iter(|| probe::cache_key("/nonexistent/file.mp4"));
    });

    // Warm (existing file with mtime lookup)
    if let Some(path) = fixture("black_1s_h264.mp4") {
        group.bench_function("real_file_mtime", |b| {
            b.iter(|| probe::cache_key(&path));
        });
    }

    group.finish();
}

// ============================================================================
// Instrumentation Benchmarks
// ============================================================================

fn bench_profiler_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("profiler_overhead");

    group.bench_function("create_profiler", |b| {
        b.iter(Profiler::new);
    });

    group.bench_function("record_stage_time", |b| {
        b.iter_batched(
            Profiler::new,
            |mut profiler| {
                profiler.record_stage_time("test_stage", 100);
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("record_subprocess", |b| {
        b.iter_batched(
            Profiler::new,
            |mut profiler| {
                profiler.record_subprocess();
                profiler.record_io_read(1024 * 50);
                profiler.record_io_written(1024 * 10);
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("generate_report", |b| {
        b.iter_batched(
            || {
                let mut profiler = Profiler::new();
                for i in 0..100 {
                    profiler.record_stage_time(&format!("stage-{}", i), i * 10);
                    profiler.record_cpu_time(format!("op-{}", i), i * 5);
                }
                profiler.record_bytes_processed(1_000_000);
                profiler.record_frames_processed(3000);
                profiler.record_subprocess();
                profiler.record_io_read(500_000);
                profiler.record_io_written(200_000);
                profiler
            },
            |profiler| profiler.generate_report(),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_profiler_throughput(c: &mut Criterion) {
    let mut profiler = Profiler::new();

    c.bench_function("profiler_1000_records", |b| {
        b.iter(|| {
            for i in 0..1000 {
                profiler.record_stage_time(&format!("stage-{}", i % 10), i as u64);
                profiler.record_cpu_time(format!("op-{}", i), i as u64);
            }
        });
    });
}

// ============================================================================
// Timeline Benchmarks
// ============================================================================

fn bench_timeline_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("timeline_creation");

    group.bench_function("empty_timeline", |b| {
        b.iter(|| MediaTimelinePlan::new("bench-empty"));
    });

    group.finish();
}

// ============================================================================
// Library Initialization Benchmarks
// ============================================================================

fn bench_init_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("init_overhead");

    group.bench_function("get_version", |b| {
        b.iter(|| VERSION);
    });

    group.bench_function("init_library", |b| {
        b.iter(init);
    });

    group.finish();
}

// ============================================================================
// String Operations Benchmarks
// ============================================================================

fn bench_string_concatenation(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    group.bench_function("string_concat_naive", |b| {
        b.iter(|| {
            let mut result = String::new();
            for i in 0..100 {
                result.push_str(&format!("filter_{}=", i));
                result.push_str(&format!("value={};", i * 2));
            }
            result
        });
    });

    group.bench_function("string_concat_reserve", |b| {
        b.iter(|| {
            let mut result = String::with_capacity(2000);
            for i in 0..100 {
                result.push_str(&format!("filter_{}=", i));
                result.push_str(&format!("value={};", i * 2));
            }
            result
        });
    });

    group.finish();
}

// ============================================================================
// Memory Allocation Benchmarks
// ============================================================================

fn bench_vector_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_operations");

    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let mut data: Vec<f32> = (0..size).map(|i| i as f32 * 0.001).collect();
                for val in data.iter_mut() {
                    *val *= 0.5;
                }
                data.iter().sum::<f32>()
            });
        });
    }

    group.finish();
}

// ============================================================================
// Batch Probe Benchmarks (parallel)
// ============================================================================

fn bench_batch_probe(c: &mut Criterion) {
    let Some(path) = fixture("black_1s_h264.mp4") else {
        eprintln!("bench_batch_probe: fixture missing, skipping");
        return;
    };

    use rayon::prelude::*;

    let mut group = c.benchmark_group("batch_probe");

    for &count in &[10usize, 50, 100] {
        let paths: Vec<String> = (0..count).map(|_| path.clone()).collect();

        group.bench_with_input(
            BenchmarkId::new("parallel_fast", count),
            &count,
            |b, _| {
                b.iter(|| {
                    paths
                        .par_iter()
                        .map(|p| probe::probe_fast(p).ok())
                        .collect::<Vec<_>>()
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(5));
    targets =
        bench_audio_graph_validation,
        bench_audio_graph_build,
        bench_audio_mix_simd,
        bench_probe_mp4_small,
        bench_probe_mp4_medium,
        bench_probe_wav_short,
        bench_cache_key,
        bench_profiler_overhead,
        bench_profiler_throughput,
        bench_timeline_creation,
        bench_init_overhead,
        bench_string_concatenation,
        bench_vector_operations,
        bench_batch_probe
);

criterion_main!(benches);
