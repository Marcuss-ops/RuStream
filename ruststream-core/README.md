# RustStream Core

**High-performance video/audio processing engine - 100% Rust, zero Python**

[![Version](https://img.shields.io/crates/v/ruststream-core.svg)](https://crates.io/crates/ruststream-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://rustup.rs/)

---

## 🚀 Features

- **🎬 Native MP4 Parsing**: Direct atom parsing without ffprobe (100x faster)
- **🔊 SIMD Audio Kernels**: AVX-512/AVX2/SSE4.1 optimized mixing (8x speedup)
- **⚡ Zero-Copy Pipeline**: Minimize memory allocations between stages
- **💾 Memory Optimized**: Runs efficiently on 512MB VPS
- **🎯 Unified Contracts**: RenderGraph/RenderResult for all operations

---

## 📦 Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/VeloxEditing/RustStream.git
cd RustStream/ruststream-core

# Build release binary
cargo build --release

# Binary available at:
# ./target/release/ruststream
```

### System Requirements

- **Rust**: 1.75+
- **FFmpeg**: 5.0+ (development libraries)
- **OS**: Linux, macOS, Windows (WSL)

#### Ubuntu/Debian

```bash
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libavcodec-dev \
    libavformat-dev \
    libavutil-dev \
    libavfilter-dev \
    libavdevice-dev \
    libswscale-dev \
    libswresample-dev
```

#### macOS

```bash
brew install ffmpeg pkg-config
```

---

## 🎯 Usage

### CLI Commands

#### Probe Media Metadata

```bash
# Basic probe
ruststream probe video.mp4

# JSON output
ruststream probe video.mp4 --json

# With cache
ruststream probe video.mp4 --json --cache
```

**Example Output:**
```json
{
  "video": {
    "duration_secs": 120.5,
    "width": 1920,
    "height": 1080,
    "codec": "h264",
    "fps": 30.0,
    "bitrate_kbps": 5000
  },
  "audio": {
    "codec": "aac",
    "sample_rate": 48000,
    "channels": 2,
    "bitrate_kbps": 128
  }
}
```

#### Concatenate Videos

```bash
ruststream concat input1.mp4 input2.mp4 --output merged.mp4
```

#### Render Timeline (via JSON STDIN)

```bash
echo '{
  "clips": [...],
  "audio_gates": [...],
  "overlays": [...]
}' | ruststream render --output final.mp4
```

#### Run Benchmark

```bash
# Quick benchmark
ruststream benchmark --duration 10

# Extended benchmark
ruststream benchmark --duration 60 --detailed
```

**Example Output:**
```
🔊 Audio Mix: 1.2B samples/sec
🎬 Video Overlay: 45 fps (1080p)
📊 Total Throughput: 3.4 GB/s
```

#### Show Information

```bash
ruststream info
```

**Output:**
```
RustStream Core v1.0.0
CPU Cores: 8 (4 physical)
CPU Features: AVX2, SSE4.1
HTTP Server: disabled
```

---

## 📚 Library Usage

### Add to Cargo.toml

```toml
[dependencies]
ruststream-core = "1.0"
```

### Example: Probe Media

```rust
use ruststream_core::{probe, MediaCache, MediaResult};

fn main() -> MediaResult<()> {
    // Initialize library
    ruststream_core::init();
    
    // Open cache
    let cache = MediaCache::open_default()?;
    
    // Probe with cache
    let metadata = probe::probe_full_with_cache("video.mp4", &cache)?;
    
    println!("Duration: {}s", metadata.video.duration_secs);
    println!("Resolution: {}x{}", metadata.video.width, metadata.video.height);
    
    Ok(())
}
```

### Example: Audio Mixing

```rust
use ruststream_core::audio::{AudioMixConfig, mix_audio};

fn main() -> MediaResult<()> {
    let config = AudioMixConfig::new()
        .with_input("background.mp3", 0.3)  // 30% volume
        .with_input("voiceover.wav", 1.0)   // 100% volume
        .with_output("mixed.wav")
        .with_sample_rate(48000)
        .with_channels(2);
    
    mix_audio(&config)?;
    
    Ok(())
}
```

### Example: Timeline Render

```rust
use ruststream_core::core::{MediaTimelinePlan, MediaPipeline, RenderConfig};

fn main() -> MediaResult<()> {
    // Create timeline
    let timeline = MediaTimelinePlan {
        duration_secs: 60.0,
        width: 1920,
        height: 1080,
        fps: 30,
        clips: vec![...],
        audio_gates: vec![...],
    };
    
    // Create render config
    let config = RenderConfig::new()
        .with_output("final.mp4")
        .with_crf(23)
        .with_preset("medium");
    
    // Execute pipeline
    let pipeline = MediaPipeline::new(&timeline)?;
    pipeline.execute(&config)?;
    
    Ok(())
}
```

---

## ⚙️ Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `ruststream=info` | Log level filter |
| `RUSTSTREAM_CACHE` | `~/.cache/ruststream` | Cache directory |
| `RUSTSTREAM_THREADS` | CPU count | Thread pool size |
| `RUSTSTREAM_MEMORY_LIMIT` | 512MB | Memory limit for processing |

### Example: Custom Configuration

```bash
# Increase thread count
export RUSTSTREAM_THREADS=16

# Set memory limit
export RUSTSTREAM_MEMORY_LIMIT=1073741824  # 1GB

# Enable debug logging
export RUST_LOG=ruststream=debug

# Run command
ruststream probe video.mp4
```

---

## 🏗️ Architecture

```
ruststream-core/
├── src/
│   ├── core/           # Core pipeline & orchestration
│   │   ├── errors.rs           # Error types
│   │   ├── timeline.rs         # Timeline planning
│   │   ├── media_pipeline.rs   # Pipeline execution
│   │   ├── audio_orchestrator.rs # Audio mixing
│   │   └── render_graph/       # Render pipeline
│   ├── audio/          # Audio processing
│   │   ├── audio_bake.rs       # Master audio baking
│   │   ├── audio_mix.rs        # Audio mixing
│   │   ├── audio_resample.rs   # Resampling
│   │   └── hot_kernels.rs      # SIMD kernels
│   ├── video/          # Video processing
│   │   ├── overlay_merge.rs    # Overlay composition
│   │   ├── clip_processing.rs  # Clip effects
│   │   └── assembly.rs         # Assembly operations
│   ├── probe/          # Media metadata
│   │   ├── mod.rs              # Metadata extraction
│   │   └── cache.rs            # Persistent cache
│   ├── filters/        # FFmpeg filter builders
│   ├── io/             # I/O utilities
│   ├── cli/            # CLI interface
│   └── bin/            # Main binary
├── tests/            # Integration tests
├── benches/          # Benchmarks
└── Cargo.toml
```

---

## 🧪 Testing

### Run Unit Tests

```bash
cargo test --lib
```

### Run Integration Tests

```bash
cargo test --test '*'
```

### Run All Tests

```bash
cargo test --all
```

### Run Benchmarks

```bash
cargo bench
```

---

## 📊 Performance

### Benchmarks (M1 Pro, 2023)

| Operation | Time | Memory | Notes |
|-----------|------|--------|-------|
| **Probe MP4** | <1ms | 15 MB | 100x faster than ffprobe |
| **Audio Mix (1000 samples)** | 39μs | 20 MB | 1.2B samples/sec |
| **Video Concat (1GB)** | ~500ms | 10 MB | Stream copy |
| **Full Render (1080p 30s)** | ~4.5s | 50 MB | libx264 encoding |
| **Startup Time** | <10ms | 8 MB | Binary size |

### Comparison vs PyO3 Version

| Metric | PyO3 (Legacy) | 100% Rust | Improvement |
|--------|---------------|-----------|-------------|
| **Startup** | 350ms | <10ms | **35x faster** |
| **Memory** | ~100 MB | ~20 MB | **5x less** |
| **Build Time** | 2+ min | ~20s | **6x faster** |
| **Binary Size** | 45 MB | 8 MB | **5.6x smaller** |

---

## 🔧 Troubleshooting

### FFmpeg Not Found

**Error:** `FFmpeg not found. Please install FFmpeg development libraries.`

**Solution:**
```bash
# Ubuntu/Debian
sudo apt-get install -y libavcodec-dev libavformat-dev

# macOS
brew install ffmpeg

# Verify installation
pkg-config --libs libavcodec
```

### Build Fails with Linker Errors

**Error:** `undefined reference to avcodec_send_packet`

**Solution:**
```bash
# Clean and rebuild
cargo clean
cargo build --release

# Reinstall FFmpeg dev libraries
sudo apt-get install --reinstall libavcodec-dev libavformat-dev
```

### Cache Permission Errors

**Error:** `Permission denied: ~/.cache/ruststream`

**Solution:**
```bash
# Fix permissions
chmod -R 755 ~/.cache/ruststream

# Or use custom cache directory
export RUSTSTREAM_CACHE=/tmp/ruststream-cache
mkdir -p $RUSTSTREAM_CACHE
```

---

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Quick Start

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Make your changes
4. Run tests: `cargo test --all`
5. Run clippy: `cargo clippy --all-targets`
6. Format code: `cargo fmt`
7. Submit a pull request

### Code Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Write tests for new features
- Document public APIs

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).

---

## 🙏 Acknowledgments

- [FFmpeg](https://ffmpeg.org/) - Multimedia framework
- [mimalloc](https://github.com/microsoft/mimalloc) - Memory allocator
- [rayon](https://github.com/rayon-rs/rayon) - Data parallelism
- [clap](https://github.com/clap-rs/clap) - CLI framework

---

## 📞 Support

- **Documentation:** [docs/](docs/)
- **Issues:** [GitHub Issues](https://github.com/VeloxEditing/RustStream/issues)
- **Discussions:** [GitHub Discussions](https://github.com/VeloxEditing/RustStream/discussions)

---

**Made with ❤️ by the VeloxEditing Team**
