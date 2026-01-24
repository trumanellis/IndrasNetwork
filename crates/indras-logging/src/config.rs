//! Configuration types for the logging system

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Main logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Default log level (can be overridden by RUST_LOG)
    pub default_level: String,

    /// Console output configuration
    pub console: ConsoleConfig,

    /// File output configuration
    pub file: Option<FileConfig>,

    /// JSONL output configuration
    pub jsonl: JsonlConfig,

    /// Filtering configuration
    pub filters: FilterConfig,

    /// OpenTelemetry configuration
    pub otel: OtelConfig,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            default_level: "info".to_string(),
            console: ConsoleConfig::default(),
            file: None,
            jsonl: JsonlConfig::default(),
            filters: FilterConfig::default(),
            otel: OtelConfig::default(),
        }
    }
}

impl LogConfig {
    /// Create a config for development (verbose console output)
    pub fn development() -> Self {
        Self {
            default_level: "debug".to_string(),
            console: ConsoleConfig {
                enabled: true,
                pretty: true,
                ansi: true,
                level: Some("debug".to_string()),
            },
            ..Default::default()
        }
    }

    /// Create a config for production (JSONL file output with OTel)
    pub fn production(log_dir: PathBuf) -> Self {
        Self {
            default_level: "info".to_string(),
            console: ConsoleConfig {
                enabled: false,
                pretty: false,
                ansi: false,
                level: None,
            },
            file: Some(FileConfig {
                directory: log_dir,
                prefix: "indras".to_string(),
                rotation: RotationStrategy::Daily,
                max_files: Some(30),
            }),
            jsonl: JsonlConfig::default(),
            filters: FilterConfig::default(),
            otel: OtelConfig {
                enabled: true,
                ..OtelConfig::default()
            },
        }
    }

    /// Create a config for testing (minimal output)
    pub fn testing() -> Self {
        Self {
            default_level: "warn".to_string(),
            console: ConsoleConfig {
                enabled: true,
                pretty: false,
                ansi: false,
                level: Some("warn".to_string()),
            },
            ..Default::default()
        }
    }
}

/// Console output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleConfig {
    /// Enable console output
    pub enabled: bool,
    /// Use pretty (human-readable) format
    pub pretty: bool,
    /// Include ANSI colors
    pub ansi: bool,
    /// Level for console output (can be different from file)
    pub level: Option<String>,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pretty: false, // JSONL by default
            ansi: false,
            level: None,
        }
    }
}

/// File output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    /// Directory for log files
    pub directory: PathBuf,
    /// File name prefix
    pub prefix: String,
    /// Rotation strategy
    pub rotation: RotationStrategy,
    /// Maximum files to retain
    pub max_files: Option<usize>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("./logs"),
            prefix: "indras".to_string(),
            rotation: RotationStrategy::Daily,
            max_files: Some(7),
        }
    }
}

/// File rotation strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RotationStrategy {
    /// Rotate daily
    #[default]
    Daily,
    /// Rotate hourly
    Hourly,
    /// Never rotate (single file)
    Never,
}

/// JSONL formatting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlConfig {
    /// Flatten event fields to root level
    pub flatten_events: bool,
    /// Include span list in events
    pub include_spans: bool,
    /// Include current span details
    pub include_current_span: bool,
    /// Include thread information
    pub include_thread_info: bool,
    /// Include file/line information
    pub include_location: bool,
    /// Custom fields to always include
    pub extra_fields: HashMap<String, String>,
}

impl Default for JsonlConfig {
    fn default() -> Self {
        Self {
            flatten_events: true,
            include_spans: true,
            include_current_span: true,
            include_thread_info: false,
            include_location: true,
            extra_fields: HashMap::new(),
        }
    }
}

/// Filtering configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Crates to include (whitelist)
    pub include_crates: Vec<String>,
    /// Crates to exclude (blacklist)
    pub exclude_crates: Vec<String>,
    /// Specific target filters
    pub targets: HashMap<String, String>,
}

/// OpenTelemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelConfig {
    /// Whether OpenTelemetry is enabled
    pub enabled: bool,
    /// OTLP endpoint (e.g., "http://localhost:4317")
    pub endpoint: String,
    /// Service name for traces
    pub service_name: String,
    /// Sample ratio (1.0 = all traces, 0.1 = 10%)
    pub sample_ratio: f64,
    /// Additional resource attributes
    pub resource_attributes: HashMap<String, String>,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
            service_name: std::env::var("OTEL_SERVICE_NAME")
                .unwrap_or_else(|_| "indras-network".to_string()),
            sample_ratio: 1.0,
            resource_attributes: HashMap::new(),
        }
    }
}

impl OtelConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Set the endpoint
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Set the service name
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = name.into();
        self
    }

    /// Set the sample ratio
    pub fn with_sample_ratio(mut self, ratio: f64) -> Self {
        self.sample_ratio = ratio.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LogConfig::default();
        assert_eq!(config.default_level, "info");
        assert!(config.console.enabled);
        assert!(!config.console.pretty); // JSONL by default
        assert!(config.file.is_none());
    }

    #[test]
    fn test_development_config() {
        let config = LogConfig::development();
        assert_eq!(config.default_level, "debug");
        assert!(config.console.pretty);
        assert!(config.console.ansi);
    }

    #[test]
    fn test_production_config() {
        let config = LogConfig::production(PathBuf::from("/var/log/indras"));
        assert!(!config.console.enabled);
        assert!(config.file.is_some());
        assert!(config.otel.enabled);
    }

    #[test]
    fn test_otel_config_from_env() {
        let config = OtelConfig::from_env();
        assert!(!config.endpoint.is_empty());
        assert!(!config.service_name.is_empty());
        assert!((0.0..=1.0).contains(&config.sample_ratio));
    }
}
