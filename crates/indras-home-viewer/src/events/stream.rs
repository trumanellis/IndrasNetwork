//! JSONL stream reader for home realm events.
//!
//! Supports reading from stdin or a file path.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use serde::Deserialize;

use super::HomeRealmEvent;

/// Wrapper for the indras logging format.
/// Log lines look like: {"message":"event_type","fields":"{\"event_type\":\"...\",\"tick\":1,...}"}
#[derive(Debug, Deserialize)]
struct LogWrapper {
    message: String,
    #[serde(default)]
    fields: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// Parse a line that could be either:
/// 1. Direct JSONL event: {"event_type":"note_created","tick":1,...}
/// 2. Wrapped log format: {"message":"note_created","fields":"{\"event_type\":\"note_created\",...}"}
fn parse_event_line(line: &str) -> Option<HomeRealmEvent> {
    // First, try to parse as a direct event
    if let Ok(event) = serde_json::from_str::<HomeRealmEvent>(line) {
        return Some(event);
    }

    // Try to parse as wrapped log format
    if let Ok(wrapper) = serde_json::from_str::<LogWrapper>(line) {
        // Only process lua-sourced events with fields
        if wrapper.source.as_deref() != Some("lua") {
            return None;
        }

        // Extract the fields JSON string and parse it
        if let Some(fields_str) = &wrapper.fields {
            // Quick check: only try to parse if it looks like an event (contains "event_type")
            if !fields_str.contains("\"event_type\"") {
                return None;
            }

            if let Ok(event) = serde_json::from_str::<HomeRealmEvent>(fields_str) {
                return Some(event);
            }
        }
    }

    None
}

/// Global buffer of all events for replay support.
static EVENT_BUFFER: OnceLock<Arc<Mutex<Vec<HomeRealmEvent>>>> = OnceLock::new();

/// Returns the global event buffer, initializing if needed.
pub fn event_buffer() -> Arc<Mutex<Vec<HomeRealmEvent>>> {
    EVENT_BUFFER
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone()
}

/// Configuration for the stream reader.
#[derive(Debug, Clone, Default)]
pub struct StreamConfig {
    /// Path to a JSONL file, or None to read from stdin.
    pub file_path: Option<PathBuf>,

    /// Optional member filter - only emit events for this member.
    pub member_filter: Option<String>,
}

/// Starts the event stream reader in a background task.
///
/// Returns a channel receiver that yields parsed events.
pub fn start_stream(config: StreamConfig) -> mpsc::UnboundedReceiver<HomeRealmEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        if let Err(e) = run_stream(config, tx).await {
            tracing::error!("Stream reader error: {}", e);
        }
    });

    rx
}

/// Internal stream reader implementation.
async fn run_stream(
    config: StreamConfig,
    tx: mpsc::UnboundedSender<HomeRealmEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(path) = config.file_path {
        // Read from file
        tracing::info!("Reading events from file: {:?}", path);
        let file = tokio::fs::File::open(&path).await?;
        let reader = BufReader::new(file);
        read_lines(reader, &tx, config.member_filter.as_deref()).await?;
    } else {
        // Read from stdin
        tracing::info!("Reading events from stdin");
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        read_lines(reader, &tx, config.member_filter.as_deref()).await?;
    }

    tracing::info!("Stream reader finished");
    Ok(())
}

/// Reads lines from an async reader and parses them as events.
async fn read_lines<R: tokio::io::AsyncRead + Unpin>(
    reader: BufReader<R>,
    tx: &mpsc::UnboundedSender<HomeRealmEvent>,
    member_filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to parse the event - handle both direct JSONL and wrapped log format
        let event = parse_event_line(line);

        if let Some(event) = event {
            // Apply member filter if set
            if let Some(filter) = member_filter {
                if let Some(member) = event.member() {
                    if member != filter {
                        continue;
                    }
                }
            }

            // Send to channel (buffering is handled by main.rs)
            if tx.send(event).is_err() {
                tracing::debug!("Channel closed, stopping stream");
                break;
            }
        }
    }

    Ok(())
}

/// Replays events from the buffer starting at a given position.
///
/// Returns the number of events replayed.
pub fn replay_from_position(
    start_pos: usize,
    tx: &mpsc::UnboundedSender<HomeRealmEvent>,
    member_filter: Option<&str>,
) -> usize {
    let buffer = event_buffer();
    let buf = buffer.lock().unwrap();

    let mut count = 0;
    for event in buf.iter().skip(start_pos) {
        // Apply member filter if set
        if let Some(filter) = member_filter {
            if let Some(member) = event.member() {
                if member != filter {
                    continue;
                }
            }
        }

        if tx.send(event.clone()).is_err() {
            break;
        }
        count += 1;
    }

    count
}

/// Returns the total number of events in the buffer.
pub fn buffer_len() -> usize {
    event_buffer().lock().map(|b| b.len()).unwrap_or(0)
}

/// Clears the event buffer.
pub fn clear_buffer() {
    if let Ok(mut buf) = event_buffer().lock() {
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_operations() {
        clear_buffer();
        assert_eq!(buffer_len(), 0);

        let buffer = event_buffer();
        {
            let mut buf = buffer.lock().unwrap();
            buf.push(HomeRealmEvent::Info {
                message: "test".to_string(),
            });
        }

        assert_eq!(buffer_len(), 1);
        clear_buffer();
        assert_eq!(buffer_len(), 0);
    }
}
