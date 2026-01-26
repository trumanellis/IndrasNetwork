//! JSONL stream reader for event ingestion
//!
//! Reads events from stdin or a file and sends them through a channel.

use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use super::StreamEvent;

/// Stream reader configuration
pub struct StreamConfig {
    /// Read from file instead of stdin
    pub file_path: Option<PathBuf>,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self { file_path: None }
    }
}

/// Start the event stream reader
///
/// Returns a channel receiver that will receive parsed events.
pub fn start_stream(config: StreamConfig) -> mpsc::UnboundedReceiver<StreamEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        if let Err(e) = run_stream(config, tx).await {
            tracing::error!("Stream reader error: {}", e);
        }
    });

    rx
}

async fn run_stream(
    config: StreamConfig,
    tx: mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match config.file_path {
        Some(path) => {
            let file = tokio::fs::File::open(&path).await?;
            let reader = BufReader::new(file);
            read_lines(reader, tx).await
        }
        None => {
            let stdin = tokio::io::stdin();
            let reader = BufReader::new(stdin);
            read_lines(reader, tx).await
        }
    }
}

async fn read_lines<R: tokio::io::AsyncRead + Unpin>(
    reader: BufReader<R>,
    tx: mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to parse as JSON event
        match serde_json::from_str::<StreamEvent>(line) {
            Ok(event) => {
                if tx.send(event).is_err() {
                    // Receiver dropped, stop reading
                    break;
                }
            }
            Err(e) => {
                // Log parse errors but continue
                tracing::trace!("Failed to parse line: {} - {}", e, line);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_realm_created() {
        let json = r#"{"event_type":"realm_created","tick":10,"realm_id":"abc123","members":"peer1,peer2","member_count":2}"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, StreamEvent::RealmCreated { .. }));
        assert_eq!(event.tick(), 10);
    }

    #[test]
    fn test_parse_quest_created() {
        let json = r#"{"event_type":"quest_created","tick":5,"realm_id":"r1","quest_id":"q1","creator":"peer1","title":"Test Quest"}"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, StreamEvent::QuestCreated { .. }));
    }

    #[test]
    fn test_parse_attention_switched() {
        let json = r#"{"event_type":"attention_switched","tick":15,"member":"peer1","quest_id":"q1"}"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, StreamEvent::AttentionSwitched { .. }));
    }

    #[test]
    fn test_parse_unknown_event() {
        let json = r#"{"event_type":"some_unknown_type","data":"test"}"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, StreamEvent::Unknown));
    }

    #[test]
    fn test_parse_test_events_file() {
        // Test parsing the sample events file
        let test_events = r#"
{"event_type":"realm_created","tick":1,"realm_id":"realm_abc123def456","members":"peer_001,peer_002,peer_003","member_count":3}
{"event_type":"quest_created","tick":5,"realm_id":"realm_abc123def456","quest_id":"quest_001","creator":"peer_001","title":"Review design document"}
{"event_type":"quest_created","tick":6,"realm_id":"realm_abc123def456","quest_id":"quest_002","creator":"peer_002","title":"Fix API bug"}
{"event_type":"attention_switched","tick":10,"member":"peer_001","quest_id":"quest_001"}
{"event_type":"attention_switched","tick":11,"member":"peer_002","quest_id":"quest_002"}
{"event_type":"attention_switched","tick":12,"member":"peer_003","quest_id":"quest_002"}
{"event_type":"quest_claim_submitted","tick":20,"realm_id":"realm_abc123def456","quest_id":"quest_002","claimant":"peer_003","claim_index":0,"proof_artifact":"artifact_xyz789"}
{"event_type":"quest_claim_verified","tick":25,"realm_id":"realm_abc123def456","quest_id":"quest_002","claim_index":0}
{"event_type":"contact_added","tick":30,"member":"peer_001","contact":"peer_002"}
{"event_type":"contact_added","tick":31,"member":"peer_002","contact":"peer_001"}
{"event_type":"contact_added","tick":32,"member":"peer_001","contact":"peer_003"}
{"event_type":"attention_cleared","tick":40,"member":"peer_001"}
{"event_type":"quest_completed","tick":50,"realm_id":"realm_abc123def456","quest_id":"quest_002","verified_claims":1,"pending_claims":0}
"#;
        let mut count = 0;
        for line in test_events.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let event: StreamEvent = serde_json::from_str(line).expect(&format!("Failed to parse: {}", line));
            assert!(!matches!(event, StreamEvent::Unknown), "Event parsed as Unknown: {}", line);
            count += 1;
        }
        assert_eq!(count, 13, "Expected 13 events");
    }
}
