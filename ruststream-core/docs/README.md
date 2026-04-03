# RustStream Core

**High-performance video/audio processing engine - 100% Rust, no Python**

[![Crates.io](https://img.shields.io/crates/v/ruststream-core.svg)](https://crates.io/crates/ruststream-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## 🚀 Perché 100% Rust

| Vantaggio | Python+Rust | 100% Rust |
|-----------|-------------|-----------|
| **RAM Usage** | 80-120 MB | **15-30 MB** |
| **Startup Time** | 200-500 ms | **<10 ms** |
| **Deploy** | pip, venv, wheel | **Single binary** |
| **Concurrency** | GIL-limited | **True parallel** |

## 📦 Installazione

### Da Sorgenti

```bash
# Prerequisiti: Rust 1.70+, FFmpeg development libraries
sudo apt-get install -y libavcodec-dev libavformat-dev libavutil-dev \
                        libavfilter-dev libavdevice-dev libswresample-dev \
                        libswscale-dev

# Build
git clone https://github.com/VeloxEditing/RustStream
cd RustStream/ruststream-core
cargo build --release

# Binary: target/release/ruststream
```

### Via Cargo

```bash
cargo install ruststream-core
```

## 🎯 Utilizzo

### CLI Commands

```bash
# Probe metadata
ruststream probe video.mp4 --json

# Concatenate videos
ruststream concat input1.mp4 input2.mp4 --output merged.mp4

# Start HTTP server (requires --features server)
ruststream serve --port 8080

# Run benchmarks
ruststream benchmark --duration 30

# System info
ruststream info
```

### Library Usage

```rust
use ruststream_core::{init, probe, MediaCache};

// Initialize
ruststream_core::init();

// Probe media
let metadata = probe::probe_full("video.mp4")?;
println!("Duration: {}s", metadata.video.duration_secs);
println!("Codec: {}", metadata.video.codec);

// With cache
let cache = MediaCache::open_default()?;
let metadata = cache.get_or_probe("video.mp4")?;
```

## ⚡ Performance

### Benchmark (M1 Pro)

| Operazione | Time |
|------------|------|
| MP4 Probe | <1ms |
| Audio Mix (48k samples) | <5ms |
| Video Concat | ~500ms/GB |

### Memoria

- **A riposo:** ~20 MB
- **Under load:** ~50 MB
- **VPS 512MB:** 18 processi concorrenti

## 🔧 Feature Flags

```bash
# Default (CLI only)
cargo build --release

# With HTTP server
cargo build --release --features server

# Full build
cargo build --release --features full
```

## 📚 Documentazione

```bash
# Library docs
cargo doc --open

# CLI help
ruststream --help
ruststream probe --help
```

## 🧪 Testing

```bash
# All tests
cargo test

# With output
cargo test -- --nocapture

# Benchmark
cargo bench
```

## 🏗️ Architettura

```
ruststream-core/
├── src/
│   ├── core/       # Error types, traits
│   ├── probe/      # Media metadata extraction
│   ├── audio/      # Audio processing (SIMD)
│   ├── video/      # Video processing
│   ├── filters/    # FFmpeg filter builders
│   ├── io/         # I/O operations
│   ├── cli/        # CLI interface
│   ├── server/     # HTTP API (optional)
│   └── bin/        # Main binary
```

## 📊 Roadmap

- [ ] Full render graph implementation
- [ ] GPU acceleration (NVENC, VAAPI)
- [ ] WebM/VP9 encoding
- [ ] Real-time streaming (RTMP)
- [ ] WASM target

## 🤝 Contributing

1. Fork il progetto
2. Crea feature branch
3. `cargo fmt && cargo clippy`
4. Aggiungi test
5. Open PR

## 📄 License

MIT License - vedi [LICENSE](LICENSE)

## 📬 Contatti

- **GitHub:** https://github.com/VeloxEditing/RustStream
- **Issues:** https://github.com/VeloxEditing/RustStream/issues

---

Built with ❤️ in Rust - No Python, No Problem.
