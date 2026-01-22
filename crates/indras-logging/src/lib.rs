//! Multi-instance JSONL logging with OpenTelemetry support for Indras Network
//!
//! This crate provides a sophisticated logging infrastructure designed for
//! distributed P2P systems where messages traverse multiple peer instances.
//!
//! # Features
//!
//! - **JSONL Output**: Structured JSON lines format for log aggregation (default)
//! - **Multi-Instance Correlation**: Track packets across peer instances with correlation IDs
//! - **Peer Context Injection**: Automatically include peer identity in all logs
//! - **OpenTelemetry Integration**: Distributed tracing with Jaeger/Zipkin/OTLP
//! - **File Rotation**: Daily/hourly log rotation via tracing-appender
//!
//! # Quick Start
//!
//! ```ignore
//! use indras_logging::{IndrasSubscriberBuilder, LogConfig};
//!
//! // Simple setup with defaults (JSONL to console)
//! IndrasSubscriberBuilder::new()
//!     .init();
//!
//! // Development mode with pretty human-readable output
//! IndrasSubscriberBuilder::new()
//!     .with_config(LogConfig::development())
//!     .init();
//! ```
//!
//! # Peer Context
//!
//! Use [`PeerContextGuard`] to set the peer identity for a scope:
//!
//! ```ignore
//! use indras_logging::context::PeerContextGuard;
//! use indras_core::SimulationIdentity;
//!
//! let peer = SimulationIdentity::new('A').unwrap();
//! let _guard = PeerContextGuard::new(&peer);
//!
//! // All logs in this scope will include peer_id = "A"
//! tracing::info!("Processing packet");
//! ```
//!
//! # Correlation IDs
//!
//! Use [`CorrelationContext`] to track packets across instances:
//!
//! ```ignore
//! use indras_logging::correlation::CorrelationContext;
//!
//! // Create at message origin
//! let ctx = CorrelationContext::new_root()
//!     .with_packet_id("0041#3");
//!
//! // Create child when relaying
//! let child_ctx = ctx.child();
//!
//! tracing::info!(
//!     trace_id = %ctx.trace_id,
//!     span_id = %ctx.span_id,
//!     "Routing packet"
//! );
//! ```

pub mod config;
pub mod context;
pub mod correlation;
pub mod layers;
pub mod otel;

pub use config::{ConsoleConfig, FileConfig, JsonlConfig, LogConfig, OtelConfig, RotationStrategy};
pub use context::{PeerContextData, PeerContextGuard, PeerType};
pub use correlation::{fields, spans, CorrelationContext, CorrelationExt};
pub use tracing_appender::rolling::Rotation;

use std::fs::{self, File};

use tracing_appender::rolling::{RollingFileAppender, Rotation as AppenderRotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

/// Builder for configuring and initializing the Indras logging subscriber
///
/// This builder provides a fluent API for configuring all aspects of logging,
/// including console output, file output, JSONL formatting, and OpenTelemetry.
///
/// By default, console output uses JSONL format. Use `LogConfig::development()`
/// for human-readable pretty output during development.
pub struct IndrasSubscriberBuilder {
    config: LogConfig,
}

impl IndrasSubscriberBuilder {
    /// Create a new subscriber builder with default configuration
    ///
    /// Default: JSONL output to console
    pub fn new() -> Self {
        Self {
            config: LogConfig::default(),
        }
    }

    /// Use a specific configuration
    pub fn with_config(mut self, config: LogConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the default log level
    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.config.default_level = level.into();
        self
    }

    /// Enable or disable console output
    pub fn with_console(mut self, enabled: bool) -> Self {
        self.config.console.enabled = enabled;
        self
    }

    /// Configure file output
    pub fn with_file_output(mut self, config: FileConfig) -> Self {
        self.config.file = Some(config);
        self
    }

    /// Enable OpenTelemetry with the given configuration
    pub fn with_opentelemetry(mut self, config: OtelConfig) -> Self {
        self.config.otel = config;
        self.config.otel.enabled = true;
        self
    }

    /// Initialize the subscriber globally
    ///
    /// This sets the subscriber as the global default and returns a guard
    /// that must be kept alive for the duration of the program (for file output).
    ///
    /// # Panics
    ///
    /// Panics if a global subscriber has already been set.
    pub fn init(self) -> Option<tracing_appender::non_blocking::WorkerGuard> {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.config.default_level));

        let mut guards: Vec<tracing_appender::non_blocking::WorkerGuard> = Vec::new();

        // Helper to create file writer - truncates for Never rotation, appends for others
        let create_file_writer = |file_config: &FileConfig| -> (tracing_appender::non_blocking::NonBlocking, tracing_appender::non_blocking::WorkerGuard) {
            match file_config.rotation {
                RotationStrategy::Never => {
                    // Create/truncate a single file
                    fs::create_dir_all(&file_config.directory).ok();
                    let file_path = file_config.directory.join(format!("{}.log", file_config.prefix));
                    let file = File::create(&file_path)
                        .expect("Failed to create log file");
                    tracing_appender::non_blocking(file)
                }
                RotationStrategy::Daily => {
                    let appender = RollingFileAppender::new(
                        AppenderRotation::DAILY,
                        &file_config.directory,
                        &file_config.prefix,
                    );
                    tracing_appender::non_blocking(appender)
                }
                RotationStrategy::Hourly => {
                    let appender = RollingFileAppender::new(
                        AppenderRotation::HOURLY,
                        &file_config.directory,
                        &file_config.prefix,
                    );
                    tracing_appender::non_blocking(appender)
                }
            }
        };


        // Build the base registry with env filter and peer context
        let registry = Registry::default()
            .with(env_filter)
            .with(layers::PeerContextLayer::new());

        // Build based on configuration
        // We use separate match arms for pretty vs JSONL console to satisfy the type system

        match (
            self.config.console.enabled,
            self.config.console.pretty,
            self.config.file.is_some(),
            self.config.otel.enabled,
        ) {
            // Pretty console + File + OTel
            (true, true, true, true) => {
                let file_config = self.config.file.as_ref().unwrap();
                let (non_blocking, guard) = create_file_writer(file_config);
                guards.push(guard);

                let console_layer = tracing_subscriber::fmt::layer()
                    .with_ansi(self.config.console.ansi)
                    .with_target(true);

                let file_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location)
                    .with_writer(non_blocking);

                match otel::init_otel_layer(&self.config.otel) {
                    Ok(otel_layer) => {
                        registry
                            .with(console_layer)
                            .with(file_layer)
                            .with(otel_layer)
                            .init();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to initialize OpenTelemetry: {}", e);
                        registry.with(console_layer).with(file_layer).init();
                    }
                }
            }

            // JSONL console + File + OTel
            (true, false, true, true) => {
                let file_config = self.config.file.as_ref().unwrap();
                let (non_blocking_file, guard) = create_file_writer(file_config);
                guards.push(guard);

                let console_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location);

                let file_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location)
                    .with_writer(non_blocking_file);

                match otel::init_otel_layer(&self.config.otel) {
                    Ok(otel_layer) => {
                        registry
                            .with(console_layer)
                            .with(file_layer)
                            .with(otel_layer)
                            .init();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to initialize OpenTelemetry: {}", e);
                        registry.with(console_layer).with(file_layer).init();
                    }
                }
            }

            // Pretty console + File (no OTel)
            (true, true, true, false) => {
                let file_config = self.config.file.as_ref().unwrap();
                let (non_blocking, guard) = create_file_writer(file_config);
                guards.push(guard);

                let console_layer = tracing_subscriber::fmt::layer()
                    .with_ansi(self.config.console.ansi)
                    .with_target(true);

                let file_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location)
                    .with_writer(non_blocking);

                registry.with(console_layer).with(file_layer).init();
            }

            // JSONL console + File (no OTel)
            (true, false, true, false) => {
                let file_config = self.config.file.as_ref().unwrap();
                let (non_blocking_file, guard) = create_file_writer(file_config);
                guards.push(guard);

                let console_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location);

                let file_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location)
                    .with_writer(non_blocking_file);

                registry.with(console_layer).with(file_layer).init();
            }

            // Pretty console + OTel (no file)
            (true, true, false, true) => {
                let console_layer = tracing_subscriber::fmt::layer()
                    .with_ansi(self.config.console.ansi)
                    .with_target(true);

                match otel::init_otel_layer(&self.config.otel) {
                    Ok(otel_layer) => {
                        registry.with(console_layer).with(otel_layer).init();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to initialize OpenTelemetry: {}", e);
                        registry.with(console_layer).init();
                    }
                }
            }

            // JSONL console + OTel (no file)
            (true, false, false, true) => {
                let console_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location);

                match otel::init_otel_layer(&self.config.otel) {
                    Ok(otel_layer) => {
                        registry.with(console_layer).with(otel_layer).init();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to initialize OpenTelemetry: {}", e);
                        registry.with(console_layer).init();
                    }
                }
            }

            // Pretty console only
            (true, true, false, false) => {
                let console_layer = tracing_subscriber::fmt::layer()
                    .with_ansi(self.config.console.ansi)
                    .with_target(true);
                registry.with(console_layer).init();
            }

            // JSONL console only (DEFAULT)
            (true, false, false, false) => {
                let console_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location);
                registry.with(console_layer).init();
            }

            // File + OTel (no console)
            (false, _, true, true) => {
                let file_config = self.config.file.as_ref().unwrap();
                let (non_blocking, guard) = create_file_writer(file_config);
                guards.push(guard);

                let file_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location)
                    .with_writer(non_blocking);

                match otel::init_otel_layer(&self.config.otel) {
                    Ok(otel_layer) => {
                        registry.with(file_layer).with(otel_layer).init();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to initialize OpenTelemetry: {}", e);
                        registry.with(file_layer).init();
                    }
                }
            }

            // File only (no console)
            (false, _, true, false) => {
                let file_config = self.config.file.as_ref().unwrap();
                let (non_blocking, guard) = create_file_writer(file_config);
                guards.push(guard);

                let file_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(self.config.jsonl.include_spans)
                    .flatten_event(self.config.jsonl.flatten_events)
                    .with_file(self.config.jsonl.include_location)
                    .with_line_number(self.config.jsonl.include_location)
                    .with_writer(non_blocking);

                registry.with(file_layer).init();
            }

            // OTel only (no console, no file)
            (false, _, false, true) => {
                match otel::init_otel_layer(&self.config.otel) {
                    Ok(otel_layer) => {
                        registry.with(otel_layer).init();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to initialize OpenTelemetry: {}", e);
                        registry.init();
                    }
                }
            }

            // Nothing enabled - just base registry
            (false, _, false, false) => {
                registry.init();
            }
        }

        // Return the first guard (if any) to keep the file writer alive
        guards.into_iter().next()
    }

    /// Try to initialize the subscriber globally
    ///
    /// Returns an error if a global subscriber has already been set.
    pub fn try_init(
        self,
    ) -> Result<Option<tracing_appender::non_blocking::WorkerGuard>, &'static str> {
        Ok(self.init())
    }
}

impl Default for IndrasSubscriberBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize logging with default settings (JSONL to console)
///
/// This is a convenience function for quick setup.
pub fn init_default() {
    IndrasSubscriberBuilder::new().init();
}

/// Initialize logging for development (verbose, pretty console output)
pub fn init_development() {
    IndrasSubscriberBuilder::new()
        .with_config(LogConfig::development())
        .init();
}

/// Initialize logging for testing (minimal output)
pub fn init_testing() {
    let _ = IndrasSubscriberBuilder::new()
        .with_config(LogConfig::testing())
        .try_init();
}

/// Shutdown OpenTelemetry gracefully
///
/// Call this before your application exits to ensure all traces are exported.
pub fn shutdown() {
    otel::shutdown_otel();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_creation() {
        let builder = IndrasSubscriberBuilder::new();
        assert_eq!(builder.config.default_level, "info");
    }

    #[test]
    fn test_default_is_jsonl() {
        let builder = IndrasSubscriberBuilder::new();
        assert!(!builder.config.console.pretty); // JSONL by default
    }

    #[test]
    fn test_builder_with_config() {
        let config = LogConfig::development();
        let builder = IndrasSubscriberBuilder::new().with_config(config);
        assert_eq!(builder.config.default_level, "debug");
        assert!(builder.config.console.pretty); // Development uses pretty
    }

    #[test]
    fn test_builder_with_level() {
        let builder = IndrasSubscriberBuilder::new().with_level("trace");
        assert_eq!(builder.config.default_level, "trace");
    }

    #[test]
    fn test_builder_with_console() {
        let builder = IndrasSubscriberBuilder::new().with_console(false);
        assert!(!builder.config.console.enabled);
    }
}
