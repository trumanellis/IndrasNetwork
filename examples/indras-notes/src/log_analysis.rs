//! Log analysis utilities
//!
//! Provides structured log entry parsing and query functionality.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A parsed log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp of the log entry
    pub timestamp: Option<String>,
    /// Log level (TRACE, DEBUG, INFO, WARN, ERROR)
    pub level: String,
    /// Target/module that produced the log
    pub target: Option<String>,
    /// Log message
    pub message: String,
    /// Additional structured fields
    pub fields: Value,
    /// Span information (for tracing)
    pub spans: Option<Vec<Value>>,
}

impl LogEntry {
    /// Parse a log entry from a JSONL line
    pub fn from_jsonl(line: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(line)?;

        Ok(Self {
            timestamp: value.get("timestamp").and_then(|v| v.as_str()).map(String::from),
            level: value
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("INFO")
                .to_string(),
            target: value.get("target").and_then(|v| v.as_str()).map(String::from),
            message: value
                .get("message")
                .or_else(|| value.get("fields").and_then(|f| f.get("message")))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            fields: value.get("fields").cloned().unwrap_or(Value::Null),
            spans: value.get("spans").and_then(|v| v.as_array()).cloned(),
        })
    }

    /// Check if the message contains a pattern
    pub fn message_contains(&self, pattern: &str) -> bool {
        self.message.contains(pattern)
    }

    /// Get a field value by key
    pub fn get_field(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }
}

/// A query builder for filtering log entries
pub struct LogQuery {
    entries: Vec<LogEntry>,
}

impl LogQuery {
    /// Create a new query from a list of entries
    pub fn new(entries: Vec<LogEntry>) -> Self {
        Self { entries }
    }

    /// Parse entries from JSONL lines
    pub fn from_lines(lines: &[String]) -> Self {
        let entries = lines
            .iter()
            .filter_map(|line| LogEntry::from_jsonl(line).ok())
            .collect();
        Self { entries }
    }

    /// Filter by log level
    pub fn level(self, level: &str) -> Self {
        let level_upper = level.to_uppercase();
        Self {
            entries: self
                .entries
                .into_iter()
                .filter(|e| e.level.to_uppercase() == level_upper)
                .collect(),
        }
    }

    /// Filter by message containing a pattern
    pub fn message_contains(self, pattern: &str) -> Self {
        Self {
            entries: self
                .entries
                .into_iter()
                .filter(|e| e.message_contains(pattern))
                .collect(),
        }
    }

    /// Filter by field existence and value
    pub fn with_field(self, key: &str, value: &Value) -> Self {
        Self {
            entries: self
                .entries
                .into_iter()
                .filter(|e| e.get_field(key) == Some(value))
                .collect(),
        }
    }

    /// Filter by field existence
    pub fn has_field(self, key: &str) -> Self {
        Self {
            entries: self
                .entries
                .into_iter()
                .filter(|e| e.get_field(key).is_some())
                .collect(),
        }
    }

    /// Filter by target
    pub fn target(self, target: &str) -> Self {
        Self {
            entries: self
                .entries
                .into_iter()
                .filter(|e| e.target.as_ref() == Some(&target.to_string()))
                .collect(),
        }
    }

    /// Count matching entries
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Check if any entries match
    pub fn exists(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Get the first matching entry
    pub fn first(&self) -> Option<&LogEntry> {
        self.entries.first()
    }

    /// Get all matching entries
    pub fn all(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Consume and return all entries
    pub fn into_entries(self) -> Vec<LogEntry> {
        self.entries
    }

    /// Check if messages appear in order
    pub fn messages_in_order(&self, patterns: &[&str]) -> bool {
        let mut pattern_idx = 0;
        for entry in &self.entries {
            if pattern_idx < patterns.len() && entry.message_contains(patterns[pattern_idx]) {
                pattern_idx += 1;
            }
        }
        pattern_idx == patterns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<LogEntry> {
        vec![
            LogEntry {
                timestamp: Some("2024-01-01T00:00:00Z".to_string()),
                level: "INFO".to_string(),
                target: Some("test".to_string()),
                message: "Starting test".to_string(),
                fields: serde_json::json!({"count": 1}),
                spans: None,
            },
            LogEntry {
                timestamp: Some("2024-01-01T00:00:01Z".to_string()),
                level: "DEBUG".to_string(),
                target: Some("test".to_string()),
                message: "Processing data".to_string(),
                fields: serde_json::json!({"count": 2}),
                spans: None,
            },
            LogEntry {
                timestamp: Some("2024-01-01T00:00:02Z".to_string()),
                level: "ERROR".to_string(),
                target: Some("test".to_string()),
                message: "Something failed".to_string(),
                fields: serde_json::json!({"error": "oops"}),
                spans: None,
            },
        ]
    }

    #[test]
    fn test_level_filter() {
        let query = LogQuery::new(sample_entries()).level("ERROR");
        assert_eq!(query.count(), 1);
        assert!(query.first().unwrap().message_contains("failed"));
    }

    #[test]
    fn test_message_filter() {
        let query = LogQuery::new(sample_entries()).message_contains("test");
        assert_eq!(query.count(), 1);
    }

    #[test]
    fn test_chain_filters() {
        let query = LogQuery::new(sample_entries())
            .level("INFO")
            .message_contains("Starting");
        assert_eq!(query.count(), 1);
    }

    #[test]
    fn test_with_field() {
        let query = LogQuery::new(sample_entries()).with_field("error", &serde_json::json!("oops"));
        assert_eq!(query.count(), 1);
    }

    #[test]
    fn test_messages_in_order() {
        let query = LogQuery::new(sample_entries());
        assert!(query.messages_in_order(&["Starting", "Processing", "failed"]));
        assert!(!query.messages_in_order(&["failed", "Starting"]));
    }

    #[test]
    fn test_from_jsonl() {
        let line = r#"{"timestamp":"2024-01-01T00:00:00Z","level":"INFO","message":"test","fields":{}}"#;
        let entry = LogEntry::from_jsonl(line).unwrap();
        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.message, "test");
    }
}
