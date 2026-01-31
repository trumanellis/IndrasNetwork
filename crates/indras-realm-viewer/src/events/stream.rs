//! JSONL stream reader for event ingestion
//!
//! Reads events from stdin, a file, or a subprocess and sends them through a channel.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use super::StreamEvent;

/// Where the event stream comes from
pub enum StreamSource {
    /// Read from stdin (piped input)
    Stdin,
    /// Read from a JSONL file on disk
    File(PathBuf),
    /// Spawn lua_runner as a subprocess and read its stdout
    Subprocess {
        scenario_path: PathBuf,
        manifest_path: PathBuf,
    },
}

/// Stream reader configuration
pub struct StreamConfig {
    pub source: StreamSource,
}

impl StreamConfig {
    /// Create a config that reads from stdin
    pub fn stdin() -> Self {
        Self {
            source: StreamSource::Stdin,
        }
    }

    /// Create a config that reads from a file
    pub fn file(path: PathBuf) -> Self {
        Self {
            source: StreamSource::File(path),
        }
    }

    /// Create a config that spawns a subprocess
    pub fn subprocess(scenario_path: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            source: StreamSource::Subprocess {
                scenario_path,
                manifest_path,
            },
        }
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::stdin()
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
    match config.source {
        StreamSource::File(path) => {
            let file = tokio::fs::File::open(&path).await?;
            let reader = BufReader::new(file);
            read_lines(reader, tx).await
        }
        StreamSource::Stdin => {
            let stdin = tokio::io::stdin();
            let reader = BufReader::new(stdin);
            read_lines(reader, tx).await
        }
        StreamSource::Subprocess {
            scenario_path,
            manifest_path,
        } => {
            run_subprocess(scenario_path, manifest_path, tx).await
        }
    }
}

async fn run_subprocess(
    scenario_path: PathBuf,
    manifest_path: PathBuf,
    tx: mpsc::UnboundedSender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stress_level = std::env::var("STRESS_LEVEL").unwrap_or_else(|_| "quick".to_string());

    tracing::info!(
        "Spawning lua_runner for scenario: {}",
        scenario_path.display()
    );

    let mut child = tokio::process::Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("lua_runner")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .arg("--")
        .arg(&scenario_path)
        .env("STRESS_LEVEL", &stress_level)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture subprocess stdout")?;

    let reader = BufReader::new(stdout);
    let result = read_lines(reader, tx).await;

    // Wait for child to finish
    let status = child.wait().await?;
    if !status.success() {
        tracing::warn!("lua_runner exited with status: {}", status);
    }

    result
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

// =============================================================================
// Scenario Discovery
// =============================================================================

/// Metadata about a discovered Lua scenario file
#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioInfo {
    /// Human-readable name (filename without .lua)
    pub name: String,
    /// Full path to the .lua file
    pub path: PathBuf,
    /// First comment line from the file (description)
    pub description: String,
    /// Whether this is a SyncEngine scenario (name starts with "sync_engine_")
    pub is_sync_engine: bool,
}

/// Discover available scenario files in a directory.
///
/// Reads `*.lua` files, extracts the first `-- ` comment line as description,
/// and sorts SyncEngine scenarios first, then alphabetically.
pub fn discover_scenarios(dir: &Path) -> Vec<ScenarioInfo> {
    let mut scenarios = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("Failed to read scenarios directory {}: {}", dir.display(), e);
            return scenarios;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lua") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let description = extract_first_comment(&path);
        let is_sync_engine = name.starts_with("sync_engine_");

        scenarios.push(ScenarioInfo {
            name,
            path,
            description,
            is_sync_engine,
        });
    }

    // Sort: SyncEngine first, then alphabetically within each group
    scenarios.sort_by(|a, b| {
        b.is_sync_engine
            .cmp(&a.is_sync_engine)
            .then_with(|| a.name.cmp(&b.name))
    });

    scenarios
}

/// Extract the first `-- ` comment line from a Lua file as a description.
fn extract_first_comment(path: &Path) -> String {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };

    let reader = std::io::BufReader::new(file);
    for line in reader.lines().flatten() {
        let trimmed = line.trim();
        if let Some(comment) = trimmed.strip_prefix("-- ") {
            return comment.to_string();
        }
        // Skip empty lines at the top
        if !trimmed.is_empty() && !trimmed.starts_with("--") {
            break;
        }
    }

    String::new()
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

    #[test]
    fn test_discover_scenarios() {
        // Test with the actual scenarios directory if it exists
        let dir = Path::new("../../simulation/scripts/scenarios");
        if dir.exists() {
            let scenarios = discover_scenarios(dir);
            assert!(!scenarios.is_empty(), "Should find at least one scenario");
            // SyncEngine scenarios should come first
            let first_non_sync_engine = scenarios.iter().position(|s| !s.is_sync_engine);
            let last_sync_engine = scenarios.iter().rposition(|s| s.is_sync_engine);
            if let (Some(first_non_sync_engine), Some(last_sync_engine)) = (first_non_sync_engine, last_sync_engine) {
                assert!(last_sync_engine < first_non_sync_engine, "SyncEngine scenarios should be sorted first");
            }
        }
    }
}
