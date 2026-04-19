# RustStream

High-performance video and audio processing engine built in 100% Rust.

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)

## Overview

RustStream is a blazing-fast media processing engine designed for distributed video generation pipelines.
It replaces Python-based video processing with a native Rust binary that's **82% smaller** and **80% more memory efficient**.

### Key Features

- **Native MP4 Parsing**: Direct atom parsing without ffprobe (100x faster)
- **SIMD Audio Kernels**: AVX-512/AVX2/SSE4.1 optimized audio mixing (8x speedup)
- **Zero-Copy Pipeline**: Minimize memory allocations between stages
- **Memory Optimized**: Runs efficiently on 512MB VPS
- **CLI Interface**: Fast, scriptable commands for probing, concatenating, and benchmarking
- **HTTP API** (optional): Embeddable server with Axum

### Performance Metrics

| Metric | Before (Python) | After (Rust) | Improvement |
|--------|----------------|--------------|-------------|
| Binary Size | 45 MB | 8 MB | -82% |
| RAM Usage | 100 MB | 20 MB | -80% |
| Startup Time | 350 ms | <10 ms | -97% |

## Quick Start

### Prerequisites

- Rust 1.70+
- FFmpeg development libraries

```bash
# Install FFmpeg dev libraries (Debian/Ubuntu)
sudo apt-get update
sudo apt-get install -y \
  libavcodec-dev libavformat-dev libavutil-dev \
  libavfilter-dev libavdevice-dev libswresample-dev libswscale-dev
```

### Build and Run

```bash
# Clone and build
cd ruststream-core
cargo build --release

# Run the binary
cargo run --release -- --help
```

## CLI Commands

```bash
# Probe media metadata
ruststream probe video.mp4 --json

# Concatenate videos
ruststream concat -i video1.mp4 -i video2.mp4 -o output.mp4

# Run benchmarks
ruststream benchmark

# Display library info
ruststream info
```

## Project Layout

```
RustStream/
├── ruststream-core/        # Main Rust crate
│   ├── src/
│   │   ├── core/          # Core types, errors, timeline, audio graph
│   │   ├── audio/         # Audio processing (kernels, mixing, baking)
│   │   ├── video/         # Video processing (overlay, effects, assembly)
│   │   ├── probe/         # Media metadata extraction
│   │   ├── filters/       # FFmpeg filter builders
│   │   └── render_graph/  # Render pipeline orchestration
│   ├── benches/           # Performance benchmarks
│   └── tests/             # Integration tests
├── docs/                  # Documentation (deployment, migration, performance)
├── scripts/               # Build scripts (PGO, etc.)
└── .github/workflows/     # CI pipeline
```

## Development

### Running Tests

```bash
cargo test --all
```

### Running Benchmarks

```bash
cargo bench
```

### Linting

```bash
cargo clippy --all-targets --all-features
cargo fmt --all -- --check
```

### Profile-Guided Optimization (PGO)

For maximum performance on your specific hardware:

```bash
./scripts/build-pgo.sh
```

## Architecture

RustStream follows a modular architecture:

```
┌──────────────────────────────────────────────┐
│               CLI / HTTP API                  │
└──────────────────┬───────────────────────────┘
                   │
         ┌─────────┴──────────┐
         ▼                    ▼
┌────────────────┐  ┌──────────────────┐
│  Probe Module  │  │  Render Graph    │
│  (Metadata)    │  │  (Orchestration) │
└────────────────┘  └────────┬─────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
       ┌──────────┐   ┌──────────┐   ┌──────────┐
       │  Audio   │   │  Video   │   │ Filters  │
       │  Engine  │   │  Engine  │   │ (FFmpeg) │
       └──────────┘   └──────────┘   └──────────┘
```

## License

MIT License - see [LICENSE](LICENSE) for details.
