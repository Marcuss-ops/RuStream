//! Comprehensive benchmark suite for RustStream Core.
//!
//! Run with: `cargo bench`
//! Requires: `criterion` in dev-dependencies

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use ruststream_core::core::audio_graph::{AudioGraphConfig, AudioInput};
use ruststream_core::core::instrumentation::Profiler;
use ruststream_core::core::timeline::MediaTimelinePlan;
use ruststream_core::{init, VERSION};

// ============================================================================
// Audio Graph Benchmarks
// ============================================================================

fn bench_audio_graph_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_graph_validation");

    for size in [1, 5, 10, 20].iter() {
        let mut config = AudioGraphConfig::new(format!("bench-{}", size));
        for i in 0..*size {
            config = config.add_input(AudioInput::new(
                format!("input-{}", i),
                format!("/tmp/audio-{}.mp3", i),
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

    group.bench_function("single_input", |b| {
        b.iter(|| {
            AudioGraphConfig::new("test")
                .add_input(AudioInput::new("vo", "/tmp/vo.mp3", "voiceover"))
                .with_output_sample_rate(48000)
                .with_output_channels(2)
        });
    });

    group.bench_function("multiple_inputs", |b| {
        b.iter(|| {
            AudioGraphConfig::new("test")
                .add_input(AudioInput::new("vo", "/tmp/vo.mp3", "voiceover"))
                .add_input(AudioInput::new("music", "/tmp/music.mp3", "music").with_volume(0.15))
                .add_input(AudioInput::new("sfx", "/tmp/sfx.mp3", "sfx"))
                .with_output_sample_rate(48000)
                .with_output_channels(2)
        });
    });

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
// String Operations Benchmarks (for FMA optimization tracking)
// ============================================================================

fn bench_string_concatenation(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    // Naive concatenation
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

    // Optimized with reserve
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
// Criterion Configuration
// ============================================================================

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .warm_up_time(std::time::Duration::from_secs(2))
        .measurement_time(std::time::Duration::from_secs(5))
        .with_plots();
    targets = 
        bench_audio_graph_validation,
        bench_audio_graph_build,
        bench_profiler_overhead,
        bench_profiler_throughput,
        bench_timeline_creation,
        bench_init_overhead,
        bench_string_concatenation,
        bench_vector_operations
);

criterion_main!(benches);
