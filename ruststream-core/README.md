# RuStream Core

High-performance video and audio processing engine for headless media workflows in Rust.

[![CI](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml/badge.svg)](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml)
[![Version](https://img.shields.io/crates/v/ruststream-core.svg)](https://crates.io/crates/ruststream-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://rustup.rs/)

## What it is

`ruststream-core` is the main crate behind RuStream.
It provides a CLI-first media engine for probing, concatenation, rendering workflows, and backend integration.

## Current architecture

This crate is Rust-first, but it is not yet fully FFmpeg-free.
Today it uses native Rust modules where practical and FFmpeg bindings where the media stack still needs them.

## Installation

### From source

```bash
git clone https://github.com/Marcuss-ops/RuStream.git
cd RuStream/ruststream-core
cargo build --release
```

### System requirements

- Rust 1.75+
- FFmpeg 5.0+ development libraries
- Linux, macOS, or Windows via WSL/native toolchains where supported

## Common commands

```bash
ruststream probe video.mp4 --json
ruststream concat input1.mp4 input2.mp4 --output merged.mp4
ruststream benchmark --duration 10
ruststream info
```

## Development workflow

```bash
cargo test --all
cargo clippy --all-targets --all-features
cargo fmt --all -- --check
```

## Repository links

- Main project: [github.com/Marcuss-ops/RuStream](https://github.com/Marcuss-ops/RuStream)
- Issues: [github.com/Marcuss-ops/RuStream/issues](https://github.com/Marcuss-ops/RuStream/issues)

## Contributing

See the repository-level [CONTRIBUTING.md](../CONTRIBUTING.md).

## License

MIT License. See [../LICENSE](../LICENSE).
