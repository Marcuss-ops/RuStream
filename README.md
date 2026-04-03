# RustStream

RustStream is a Rust video and audio processing project published as a standalone GitHub repository.

[![CI](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml/badge.svg)](https://github.com/Marcuss-ops/RuStream/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Overview

- `ruststream-core` is the Rust crate
- `ruststream` is the binary
- The code lives in `ruststream-core/`

## Quick Start

```bash
git clone https://github.com/Marcuss-ops/RuStream.git
cd RuStream/ruststream-core
cargo build --release
```

Run the binary:

```bash
cargo run --release -- --help
```

## Features

- Video and audio processing pipeline
- CLI commands for probing, rendering, concatenating, and benchmarking
- Optional HTTP server support
- FFmpeg-based media handling

## Requirements

- Rust stable
- FFmpeg development libraries

On Debian or Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y \
  libavcodec-dev libavformat-dev libavutil-dev \
  libavfilter-dev libavdevice-dev libswresample-dev libswscale-dev
```

## Project Layout

```text
RuStream/
├── ruststream-core/   # Rust crate and binary
├── docs/              # Migration and design notes
├── scripts/           # Helper scripts
└── .github/           # GitHub Actions workflows
```

## Publishing

The repository is tagged with releases such as `v1.0.0`.
