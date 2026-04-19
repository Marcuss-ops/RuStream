# Changelog

All notable changes to RustStream Core will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive benchmark suite (`benches/benchmark_suite.rs`) covering audio graph validation, profiler overhead, string operations, and vector operations
- Full video concatenation implementation with FFmpeg concat demuxer (`concat_videos`)
- Enhanced `probe_full` to extract video width, height, FPS, audio sample rate, channels, and bit depth
- SIMD intrinsics for audio processing: AVX2 (256-bit) and SSE4.1 (128-bit) with runtime CPU feature detection and automatic fallback
- `audio_graph` module with complete `AudioGraphConfig`, `AudioInput`, `SyncConfig`, and `AudioGraphResult` types
- `instrumentation` module with `Profiler`, `StageTimer`, `StageMetrics`, and `DriftMetrics`

### Changed
- Consolidated duplicate `StageMetrics` and `DriftMetrics` types (previously defined in both `errors.rs` and `instrumentation.rs`)
- Updated `StageMetrics` to include both `stage_sum()` and `has_any()` methods
- Updated `DriftMetrics` to include `is_acceptable()` method
- Fixed `probe/cache.rs` `clear()` method to use correct redb `drain(..)` API
- Replaced `unwrap()` in server startup with proper error handling
- Feature-gated `server` module with `#[cfg(feature = "server")]`
- Updated stale documentation in `audio_mix.rs`

### Fixed
- **CRITICAL**: Cache `clear()` was silently failing due to incorrect redb API usage
- **CRITICAL**: Missing `audio_graph` and `instrumentation` modules causing compilation failures
- **CRITICAL**: Dead code files (`media_pipeline.rs`, `audio_orchestrator.rs`) not part of compilation tree
- Compilation errors with `--all-features` flag
- All Clippy warnings and code quality issues
- Incorrect PGO build script binary name

### Removed
- Dead code files: `media_pipeline.rs` (660 lines), `audio_orchestrator.rs` (590 lines) - backed up as `.bak` files
- Broken CI badge from README.md
- Stale references to removed `ac-ffmpeg` native module
- Unused `bumpalo` dependency

## [1.0.0] - 2026-04-01

### Added
- Initial release of RustStream Core
- 100% Rust video/audio processing engine (no Python dependencies)
- CLI interface with `probe`, `concat`, `serve`, `benchmark`, and `info` commands
- Optional HTTP server with Axum (feature-gated)
- FFmpeg-based media handling
- mimalloc global allocator for 5-10% performance boost
- LTO + codegen-units=1 release profile for maximum optimization
- Media cache with redb KV store + LRU eviction
- Audio mixing, gate utilities, and resampling
- Video overlay composition and clip processing
- Filter builders for FFmpeg xfade transitions
- Integration test suite (12 tests)
- Comprehensive CI pipeline (lint, test, security, build, coverage)

### Performance
- Binary size: 45 MB → 8 MB (-82%) vs Python implementation
- RAM usage: 100 MB → 20 MB (-80%)
- Startup time: 350 ms → <10 ms (-97%)
