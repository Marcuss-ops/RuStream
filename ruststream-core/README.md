# RuStream Core

The main Rust crate behind RuStream for probing media, clip concatenation, and backend-oriented rendering workflows.

[![CI](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml/badge.svg)](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml)
[![Version](https://img.shields.io/crates/v/ruststream-core.svg)](https://crates.io/crates/ruststream-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://rustup.rs/)

## Why this crate exists

`ruststream-core` is meant for CLI and server-side media automation where startup time, predictable memory use, and straightforward integration matter more than GUI features.

Use it when you want to:
- probe media metadata from scripts or workers
- concatenate clips from a native Rust binary
- embed media operations in backend pipelines
- keep orchestration in Rust while still using FFmpeg where needed today

## Current status

This crate is Rust-first, but it is not fully FFmpeg-free yet.
Some paths use native Rust modules directly, while other parts still depend on FFmpeg development libraries and bindings.

## What is included today

- probe APIs for media metadata extraction
- CLI commands for probe, concat, benchmark, and info
- optional HTTP server support behind a feature flag
- SIMD-oriented audio processing components
- integration tests and CI coverage at the repository level

## Build requirements

- Rust 1.75+
- FFmpeg development libraries
- Linux, macOS, or Windows via supported native toolchains

### Build from source

```bash
cd ruststream-core
cargo build --release
```

## Common commands

```bash
ruststream probe video.mp4 --json
ruststream concat input1.mp4 input2.mp4 --output merged.mp4
ruststream benchmark --duration 10
ruststream info
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

## Testing direction

The current tests already cover synthetic audio paths, library info, invalid-media behavior, and fixture directory layout.

The next layer of coverage should come from tiny real-media fixtures under `tests/fixtures/`:
- a small valid MP4 for probe coverage
- a small WAV for audio-oriented smoke tests
- a clearly invalid binary sample for failure-path checks

## Related docs

- repository overview: [`../README.md`](../README.md)
- contribution guide: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
- CI workflow: [`../.github/workflows/ci.yml`](../.github/workflows/ci.yml)
- fixture notes: [`tests/fixtures/README.md`](tests/fixtures/README.md)
- fixture manifest: [`tests/fixtures/manifest.toml`](tests/fixtures/manifest.toml)

## License

MIT License. See [../LICENSE](../LICENSE).
