//! RustStream - High-performance video/audio processing engine
//!
//! A 100% Rust binary with no Python dependencies.

use log::error;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("ruststream=info")
    ).init();

    // Initialize library
    if let Err(e) = ruststream_core::init() {
        error!("Failed to initialize RustStream: {}", e);
        return ExitCode::from(1);
    }

    // Parse CLI arguments
    let args = ruststream_core::cli::parse_args();

    // Execute command
    match args.command {
        ruststream_core::cli::Command::Probe { input, json } => {
            match ruststream_core::cli::run_probe(&input, json) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("Probe error: {}", e);
                    ExitCode::from(1)
                }
            }
        }

        ruststream_core::cli::Command::Concat { inputs, output } => {
            match ruststream_core::cli::run_concat(&inputs, &output) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("Concat error: {}", e);
                    ExitCode::from(2)
                }
            }
        }

        #[allow(unused_variables)]
        ruststream_core::cli::Command::Serve { port, host } => {
            #[cfg(feature = "server")]
            {
                log::info!("Starting HTTP server on http://{}:{}", host, port);

                let config = ruststream_core::server::ServerConfig {
                    host: host.clone(),
                    port,
                };

                // Run server in tokio runtime
                match tokio::runtime::Runtime::new() {
                    Ok(rt) => {
                        match rt.block_on(ruststream_core::server::start_server(config)) {
                            Ok(()) => ExitCode::SUCCESS,
                            Err(e) => {
                                error!("Server error: {}", e);
                                ExitCode::from(3)
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to create tokio runtime: {}", e);
                        ExitCode::from(3)
                    }
                }
            }

            #[cfg(not(feature = "server"))]
            {
                error!("Server feature not enabled. Rebuild with --features server");
                ExitCode::from(3)
            }
        }

        ruststream_core::cli::Command::Benchmark { duration } => {
            match ruststream_core::cli::run_benchmark(duration) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("Benchmark error: {}", e);
                    ExitCode::from(4)
                }
            }
        }

        ruststream_core::cli::Command::Info => {
            ruststream_core::cli::show_info();
            ExitCode::SUCCESS
        }
    }
}
