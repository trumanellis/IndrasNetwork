//! Shared JSONL stream reader infrastructure for Indras viewer crates.
//!
//! Provides a generic async line reader that works over stdin or a file path.
//! Each viewer crate supplies its own event type `E: DeserializeOwned` and
//! calls [`read_jsonl_lines`] or uses [`StreamSource`] / [`start_jsonl_stream`]
//! directly.
//!
//! # Example
//!
//! ```rust,ignore
//! use indras_viewer_common::stream::{StreamSource, start_jsonl_stream};
//!
//! let source = StreamSource::stdin();
//! let mut rx = start_jsonl_stream::<MyEvent>(source);
//! while let Some(event) = rx.recv().await { /* ... */ }
//! ```

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// StreamSource
// ---------------------------------------------------------------------------

/// Where a JSONL event stream comes from.
pub enum StreamSource {
    /// Read from stdin (piped input).
    Stdin,
    /// Read from a JSONL file on disk.
    File(PathBuf),
}

impl StreamSource {
    /// Create a source that reads from stdin.
    pub fn stdin() -> Self {
        Self::Stdin
    }

    /// Create a source that reads from a file.
    pub fn file(path: PathBuf) -> Self {
        Self::File(path)
    }
}

impl Default for StreamSource {
    fn default() -> Self {
        Self::Stdin
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Starts an async JSONL stream reader in a background tokio task.
///
/// Returns an unbounded channel receiver that yields successfully-parsed
/// events of type `E`. Lines that fail to deserialize are logged at TRACE
/// level and skipped.
pub fn start_jsonl_stream<E>(source: StreamSource) -> mpsc::UnboundedReceiver<E>
where
    E: DeserializeOwned + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        if let Err(e) = run_stream(source, tx).await {
            tracing::error!("Stream reader error: {}", e);
        }
    });

    rx
}

/// Reads JSONL lines from an async reader and sends parsed events through `tx`.
///
/// Empty lines are skipped. Lines that fail to deserialize are logged at TRACE
/// level and skipped.  Returns when the reader reaches EOF or the channel is
/// closed.
pub async fn read_jsonl_lines<R, E>(
    reader: BufReader<R>,
    tx: &mpsc::UnboundedSender<E>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: tokio::io::AsyncRead + Unpin,
    E: DeserializeOwned,
{
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<E>(line) {
            Ok(event) => {
                if tx.send(event).is_err() {
                    tracing::debug!("Channel closed, stopping stream");
                    break;
                }
            }
            Err(e) => {
                tracing::trace!("Failed to parse line: {} — {}", e, line);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

async fn run_stream<E>(
    source: StreamSource,
    tx: mpsc::UnboundedSender<E>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    E: DeserializeOwned,
{
    match source {
        StreamSource::File(path) => {
            tracing::info!("Reading events from file: {}", path.display());
            let file = tokio::fs::File::open(&path).await?;
            read_jsonl_lines(BufReader::new(file), &tx).await?;
        }
        StreamSource::Stdin => {
            tracing::info!("Reading events from stdin");
            read_jsonl_lines(BufReader::new(tokio::io::stdin()), &tx).await?;
        }
    }

    tracing::info!("Stream reader finished");
    Ok(())
}
