//! Lua script runner for IndrasNetwork simulation
//!
//! This binary runs Lua test scenarios with full indras bindings
//! and structured JSONL logging output.
//!
//! # Usage
//!
//! ```bash
//! # Run a single test script
//! cargo run --bin lua_runner -- scripts/scenarios/abc_relay.lua
//!
//! # Run with JSONL output to file
//! INDRAS_LOG_FILE=test.jsonl cargo run --bin lua_runner -- scripts/scenarios/test.lua
//!
//! # Run with debug logging
//! RUST_LOG=debug cargo run --bin lua_runner -- scripts/scenarios/test.lua
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use indras_logging::{IndrasSubscriberBuilder, LogConfig};
use indras_simulation::LuaRuntime;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "lua_runner")]
#[command(about = "Run Lua test scenarios for IndrasNetwork simulation")]
#[command(version)]
struct Args {
    /// Path to the Lua script to run
    script: PathBuf,

    /// Use pretty console output instead of JSONL
    #[arg(short, long)]
    pretty: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    level: String,

    /// Don't initialize logging (useful when embedding)
    #[arg(long)]
    no_logging: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // Initialize logging
    if !args.no_logging {
        let config = if args.pretty {
            LogConfig::development()
        } else {
            LogConfig::default()
        };

        IndrasSubscriberBuilder::new()
            .with_config(config)
            .with_level(&args.level)
            .init();
    }

    // Check script exists
    if !args.script.exists() {
        error!("Script not found: {}", args.script.display());
        return ExitCode::from(1);
    }

    info!("Running Lua script: {}", args.script.display());

    // Create Lua runtime with indras bindings
    let runtime = match LuaRuntime::new() {
        Ok(rt) => rt,
        Err(e) => {
            error!("Failed to create Lua runtime: {}", e);
            return ExitCode::from(1);
        }
    };

    // Execute the script
    match runtime.exec_file(&args.script) {
        Ok(()) => {
            info!("Script completed successfully");
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!("Script failed: {}", e);
            ExitCode::from(1)
        }
    }
}
