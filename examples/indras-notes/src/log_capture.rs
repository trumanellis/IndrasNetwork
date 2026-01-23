//! Log capture for testing and assertions
//!
//! Captures log output for analysis in Lua scripts.

use mlua::UserData;
use parking_lot::Mutex;
use std::io::Write;
use std::sync::Arc;
use tracing_subscriber::fmt::MakeWriter;

/// A writer that captures log output to a buffer
#[derive(Clone)]
pub struct LogCapture {
    buffer: Arc<Mutex<Vec<u8>>>,
}

// Implement UserData so LogCapture can be stored in Lua registry
impl UserData for LogCapture {}

impl LogCapture {
    /// Create a new log capture
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get all captured log lines
    pub fn get_logs(&self) -> Vec<String> {
        let buffer = self.buffer.lock();
        String::from_utf8_lossy(&buffer)
            .lines()
            .map(|s| s.to_string())
            .collect()
    }

    /// Get captured logs as parsed JSON values
    pub fn get_json_logs(&self) -> Vec<serde_json::Value> {
        self.get_logs()
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Clear the captured logs
    pub fn clear(&self) {
        self.buffer.lock().clear();
    }

    /// Get raw buffer contents
    pub fn get_raw(&self) -> Vec<u8> {
        self.buffer.lock().clone()
    }
}

impl Default for LogCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Writer instance for a single log event
pub struct LogCaptureWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Write for LogCaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.lock().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogCapture {
    type Writer = LogCaptureWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogCaptureWriter {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_capture() {
        let capture = LogCapture::new();

        // Simulate writing some logs
        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"test"}}"#).unwrap();
            writeln!(writer, r#"{{"level":"ERROR","message":"error"}}"#).unwrap();
        }

        let logs = capture.get_logs();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].contains("INFO"));
        assert!(logs[1].contains("ERROR"));
    }

    #[test]
    fn test_json_parsing() {
        let capture = LogCapture::new();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"test"}}"#).unwrap();
        }

        let json_logs = capture.get_json_logs();
        assert_eq!(json_logs.len(), 1);
        assert_eq!(json_logs[0]["level"], "INFO");
    }

    #[test]
    fn test_clear() {
        let capture = LogCapture::new();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, "test").unwrap();
        }

        assert!(!capture.get_logs().is_empty());

        capture.clear();
        assert!(capture.get_logs().is_empty());
    }
}
