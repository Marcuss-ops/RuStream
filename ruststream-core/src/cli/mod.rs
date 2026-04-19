//! CLI module - Command-line interface

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use log::info;

/// RustStream CLI
#[derive(Parser, Debug)]
#[command(name = "ruststream")]
#[command(author = "VeloxEditing Team")]
#[command(version = "1.0.0")]
#[command(about = "High-performance video/audio processing - 100% Rust", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Probe media file metadata
    Probe {
        /// Input media file
        #[arg(required = true)]
        input: PathBuf,
        
        /// Output as JSON
        #[arg(short, long, default_value = "false")]
        json: bool,
    },
    
    /// Concatenate videos
    Concat {
        /// Input video files
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        
        /// Output file
        #[arg(short, long)]
        output: PathBuf,
    },
    
    /// Start HTTP API server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        
        /// Host to bind to
        #[arg(short, long, default_value = "0.0.0.0")]
        host: String,
    },
    
    /// Run benchmarks
    Benchmark {
        /// Duration in seconds
        #[arg(short, long, default_value = "30")]
        duration: u64,
    },
    
    /// Show system information
    Info,
}

/// Parse CLI arguments
pub fn parse_args() -> Cli {
    Cli::parse()
}

/// Run probe command
pub fn run_probe(path: &Path, as_json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = path.to_str().ok_or("Invalid path")?;

    info!("Probing: {}", path_str);

    let metadata = crate::probe::probe_full(path_str)?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&metadata)?);
    } else {
        println!("File: {}", metadata.path);
        println!("Duration: {:.2}s", metadata.video.duration_secs);
        println!("Video: {}x{} @ {:.2} fps ({})",
            metadata.video.width, metadata.video.height,
            metadata.video.fps, metadata.video.codec);

        if let Some(audio) = &metadata.audio {
            println!("Audio: {} ({} Hz, {} channels)",
                audio.codec, audio.sample_rate, audio.channels);
        }
    }

    Ok(())
}

/// Run concat command
pub fn run_concat(inputs: &[PathBuf], output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input_paths: Vec<String> = inputs
        .iter()
        .filter_map(|p| p.to_str().map(String::from))
        .collect();
    
    let output_path = output.to_str().ok_or("Invalid output path")?;

    info!("Concatenating {} files to {}", inputs.len(), output_path);

    let config = crate::video::ConcatConfig {
        inputs: input_paths,
        output: output_path.to_string(),
        codec: "libx264".to_string(),
        crf: 23,
    };
    
    let result = crate::video::concat_videos(&config)?;
    
    if result {
        info!("Concat completed successfully");
        Ok(())
    } else {
        Err("Concat failed".into())
    }
}

/// Run benchmark command
pub fn run_benchmark(duration_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;

    info!("Running benchmarks for {} seconds", duration_secs);

    // Allocate buffers ONCE outside the loop to avoid measuring allocation speed
    let mut output = vec![0.0f32; 48000];
    let input1 = vec![0.5f32; 48000];
    let input2 = vec![0.3f32; 48000];
    let inputs = [&input1[..], &input2[..]];
    let volumes = [1.0f32, 1.0f32];

    let start = Instant::now();
    let mut iterations = 0u64;

    while start.elapsed().as_secs() < duration_secs {
        crate::audio::audio_mix(&mut output, &inputs, &volumes);
        iterations += 1;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let samples_per_sec = (iterations * 48000) as f64 / elapsed;

    println!("Audio Mix Benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Duration: {:.2}s", elapsed);
    println!("  Samples/sec: {:.2}M", samples_per_sec / 1_000_000.0);

    Ok(())
}

/// Show system info
pub fn show_info() {
    let info = crate::get_info();
    
    println!("RustStream v{}", info.version);
    println!();
    println!("System:");
    println!("  CPU cores: {} ({} physical)", info.cpu_cores, info.physical_cores);
    
    #[cfg(target_arch = "x86_64")]
    {
        if info.features.avx512 {
            println!("  SIMD: AVX-512 ✓");
        } else if info.features.avx2 {
            println!("  SIMD: AVX2 ✓");
        } else if info.features.sse41 {
            println!("  SIMD: SSE4.1 ✓");
        }
    }
    
    #[cfg(target_arch = "aarch64")]
    {
        println!("  SIMD: NEON ✓ (ARM64)");
    }
    
    println!();
    println!("Features:");
    println!("  HTTP Server: {}", if info.features.http_server { "✓" } else { "✗" });
}
