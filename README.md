# RustStream

A Rust-first media processing engine for probing, concatenation, and automated rendering pipelines.

[![CI](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml/badge.svg)](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)

## Why RustStream?

RustStream is built for backend and CLI-driven media workflows where startup time, predictable memory use, and scriptability matter.

It is a good fit when you want to:
- probe media metadata from automation pipelines
- concatenate clips from a Rust-native command-line tool
- run media jobs on lean servers or VPS environments
- keep the orchestration layer in Rust instead of Python wrappers

## Current Status

RustStream is an actively evolving project with a modular core under `ruststream-core`.

Today the repository already includes:
- a CLI for probing, concatenation, benchmarking, and library info
- an optional HTTP API layer with Axum
- SIMD-oriented audio processing components
- integration tests, documentation, and CI workflows

Some processing paths still rely on FFmpeg development libraries at build time, so the project is not yet fully independent from FFmpeg.

## Project Layout

```text
RuStream/
├── ruststream-core/        # Main Rust crate
│   ├── src/
│   │   ├── core/           # Core types, errors, timeline, audio graph
│   │   ├── audio/          # Audio processing and mixing
│   │   ├── video/          # Video processing and assembly
│   │   ├── probe/          # Media metadata extraction
│   │   ├── filters/        # FFmpeg filter builders
│   │   └── render_graph/   # Render pipeline orchestration
│   ├── benches/            # Performance benchmarks
│   └── tests/              # Integration tests
├── docs/                   # Documentation and archived project notes
├── scripts/                # Build and optimization scripts
└── .github/workflows/      # CI pipeline
```

## Quick Start

### Requirements

- Rust 1.70+
- FFmpeg development libraries

```bash
# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y \
  libavcodec-dev libavformat-dev libavutil-dev \
  libavfilter-dev libavdevice-dev libswresample-dev libswscale-dev
```

### Build

```bash
cd ruststream-core
cargo build --release
```

### Run

```bash
cargo run --release -- --help
```

## CLI Examples

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

## Development

### Test

```bash
cargo test --all
```

### Lint

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

### Benchmarks

```bash
cargo bench
```

### PGO Build

```bash
./scripts/build-pgo.sh
```

## Documentation

- `ruststream-core/README.md` for crate-specific usage
- `docs/` for repository notes and archived migration documents

## Contributing

Contributions are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) for workflow and scope expectations.

## License

MIT License. See [LICENSE](LICENSE) for details.
