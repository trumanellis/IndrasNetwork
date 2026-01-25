pub mod document_runner;

use crate::state::{EventType, SimEvent, SimMetrics, StressLevel, TestResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;

/// Metrics update sent during scenario execution
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum MetricsUpdate {
    #[serde(rename = "stats")]
    Stats(SimMetrics),
    #[serde(rename = "event")]
    Event(SimEvent),
    #[serde(rename = "tick")]
    Tick { current: u64, max: u64 },
    #[serde(rename = "complete")]
    Complete(TestResult),
    #[serde(rename = "error")]
    Error(String),
}

/// Scenario metadata including name and description
pub struct ScenarioInfo {
    pub name: &'static str,
    pub description: &'static str,
}

/// Handles executing Lua stress test scenarios
pub struct ScenarioRunner {
    scenarios_dir: PathBuf,
    lua_runner_path: PathBuf,
    logs_dir: PathBuf,
    workspace_root: PathBuf,
}

impl ScenarioRunner {
    /// Creates a new ScenarioRunner with default paths
    pub fn new() -> Self {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        Self {
            scenarios_dir: workspace_root.join("simulation/scripts/scenarios"),
            lua_runner_path: workspace_root.join("target/debug/lua_runner"),
            logs_dir: workspace_root.join("logs"),
            workspace_root,
        }
    }

    /// Creates a ScenarioRunner with custom paths (for testing)
    #[allow(dead_code)] // Reserved for testing
    pub fn with_paths(
        scenarios_dir: PathBuf,
        lua_runner_path: PathBuf,
        logs_dir: PathBuf,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            scenarios_dir,
            lua_runner_path,
            logs_dir,
            workspace_root,
        }
    }

    /// Lists available scenario files
    #[allow(dead_code)] // Reserved for dynamic scenario loading
    pub fn list_scenarios(&self) -> std::io::Result<Vec<String>> {
        let mut scenarios = Vec::new();

        let entries = std::fs::read_dir(&self.scenarios_dir)?;

        for entry in entries {
            let entry: std::fs::DirEntry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "lua" {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            scenarios.push(file_name.to_string());
                        }
                    }
                }
            }
        }

        scenarios.sort();
        Ok(scenarios)
    }

    /// Get scenarios organized by category with descriptions
    pub fn get_categorized_scenarios() -> Vec<(&'static str, Vec<ScenarioInfo>)> {
        vec![
            ("Core Modules", vec![
                ScenarioInfo {
                    name: "core_stress.lua",
                    description: "Tests PeerId generation, packet serialization/deserialization, and event priority handling under load.",
                },
                ScenarioInfo {
                    name: "crypto_stress.lua",
                    description: "Stress tests ML-DSA-65 signatures and ML-KEM-768 key encapsulation with high throughput and failure injection.",
                },
                ScenarioInfo {
                    name: "transport_stress.lua",
                    description: "Tests connection establishment, peer discovery, and connection churn with rapid connect/disconnect cycles.",
                },
                ScenarioInfo {
                    name: "routing_stress.lua",
                    description: "Tests store-and-forward routing, back-propagation confirmations, and message delivery under network churn.",
                },
            ]),
            ("Data & State", vec![
                ScenarioInfo {
                    name: "storage_stress.lua",
                    description: "Tests append-only event log throughput, pending queue pressure, and quota management under heavy writes.",
                },
                ScenarioInfo {
                    name: "sync_stress.lua",
                    description: "Tests Automerge CRDT synchronization with concurrent edits from multiple peers and convergence verification.",
                },
                ScenarioInfo {
                    name: "gossip_stress.lua",
                    description: "Tests topic-based pub/sub with high fanout, message dissemination latency, and duplicate detection.",
                },
                ScenarioInfo {
                    name: "messaging_stress.lua",
                    description: "Tests end-to-end encrypted messaging across interfaces with delivery confirmations and isolation verification.",
                },
            ]),
            ("Infrastructure", vec![
                ScenarioInfo {
                    name: "logging_stress.lua",
                    description: "Tests high-volume structured logging, correlation context propagation, and trace chain integrity.",
                },
                ScenarioInfo {
                    name: "dtn_stress.lua",
                    description: "Tests delay-tolerant networking with bundle delivery, custody transfer, and epidemic routing in high-offline scenarios.",
                },
                ScenarioInfo {
                    name: "node_stress.lua",
                    description: "Tests interface creation, member joins/leaves, and verifies cryptographic isolation between interfaces.",
                },
                ScenarioInfo {
                    name: "engine_stress.lua",
                    description: "Tests simulation engine performance: tick rate, large topology handling, and Lua binding overhead.",
                },
            ]),
            ("Integration", vec![
                ScenarioInfo {
                    name: "integration_full_stack.lua",
                    description: "Full end-to-end workflow: node creation, interface formation, encrypted messaging, network partition, and recovery.",
                },
                ScenarioInfo {
                    name: "partition_recovery.lua",
                    description: "Tests network partition scenarios with isolated groups, healing detection, and state re-synchronization.",
                },
                ScenarioInfo {
                    name: "scalability_limit.lua",
                    description: "Progressively increases load to find system limits: max peers, message throughput, and degradation thresholds.",
                },
            ]),
            ("PQ Crypto", vec![
                ScenarioInfo {
                    name: "pq_baseline_benchmark.lua",
                    description: "Baseline benchmark for post-quantum crypto operations: signature and KEM latency under normal conditions.",
                },
                ScenarioInfo {
                    name: "pq_invite_stress.lua",
                    description: "Stress tests the PQ invite flow with rapid invite creation, KEM key exchange, and acceptance processing.",
                },
                ScenarioInfo {
                    name: "pq_concurrent_joins.lua",
                    description: "Tests concurrent interface joins with overlapping KEM operations and signature verifications.",
                },
                ScenarioInfo {
                    name: "pq_signature_throughput.lua",
                    description: "Maximum throughput test for ML-DSA-65 signing and verification with latency percentile tracking.",
                },
                ScenarioInfo {
                    name: "pq_chaos_monkey.lua",
                    description: "Chaos testing with random peer failures during PQ operations to verify cryptographic resilience.",
                },
                ScenarioInfo {
                    name: "pq_large_interface_sync.lua",
                    description: "Tests large interface synchronization with many members and high signature verification load.",
                },
            ]),
            ("Relay & Routing", vec![
                ScenarioInfo {
                    name: "abc_relay.lua",
                    description: "Basic A-B-C relay testing with simple three-node topology and message forwarding verification.",
                },
                ScenarioInfo {
                    name: "relay_chain.lua",
                    description: "Multi-hop relay chain scenarios testing message propagation through extended relay paths.",
                },
                ScenarioInfo {
                    name: "backprop_verification.lua",
                    description: "Detailed verification of back-propagation confirmation paths and delivery acknowledgments.",
                },
                ScenarioInfo {
                    name: "prophet_stress.lua",
                    description: "Tests PRoPHET probabilistic routing with encounter history, transitive probability, and decay verification.",
                },
            ]),
            ("Resilience", vec![
                ScenarioInfo {
                    name: "chaos_monkey.lua",
                    description: "General chaos testing with random peer failures, network disruptions, and recovery verification.",
                },
                ScenarioInfo {
                    name: "hub_failure.lua",
                    description: "Tests network resilience when hub nodes fail, verifying alternate route discovery.",
                },
                ScenarioInfo {
                    name: "message_timeout.lua",
                    description: "Tests message timeout behavior, retry mechanisms, and expiration handling.",
                },
                ScenarioInfo {
                    name: "offline_relay.lua",
                    description: "Tests offline peer relay handling with store-and-forward when intermediaries are unavailable.",
                },
                ScenarioInfo {
                    name: "network_partition.lua",
                    description: "Tests network partitioning scenarios with group isolation and eventual reconnection.",
                },
            ]),
            ("Concurrency", vec![
                ScenarioInfo {
                    name: "bidirectional_concurrent.lua",
                    description: "Tests bidirectional concurrent message flows with simultaneous send/receive operations.",
                },
                ScenarioInfo {
                    name: "comprehensive_test.lua",
                    description: "Comprehensive end-to-end testing combining multiple stress patterns and edge cases.",
                },
            ]),
            ("IoT Constraints", vec![
                ScenarioInfo {
                    name: "iot_stress.lua",
                    description: "Tests IoT-specific constraints: duty cycling, compact wire format, and low-memory operation.",
                },
            ]),
            ("Advanced DTN", vec![
                ScenarioInfo {
                    name: "dtn_custody_stress.lua",
                    description: "Tests DTN custody transfer with acceptance/rejection, custody timeout, and transfer chains.",
                },
                ScenarioInfo {
                    name: "dtn_strategy_stress.lua",
                    description: "Tests dynamic DTN strategy switching based on network conditions and delivery requirements.",
                },
            ]),
            ("Advanced Storage", vec![
                ScenarioInfo {
                    name: "storage_compaction_stress.lua",
                    description: "Tests append-only log compaction under write load with data integrity verification.",
                },
                ScenarioInfo {
                    name: "storage_blob_stress.lua",
                    description: "Tests content-addressed blob store throughput and large binary data handling.",
                },
            ]),
            ("Schema & Validation", vec![
                ScenarioInfo {
                    name: "messaging_schema_stress.lua",
                    description: "Tests schema registry, content validation throughput, and schema migration under load.",
                },
            ]),
            ("SDK Stress Tests", vec![
                ScenarioInfo {
                    name: "sdk_stress.lua",
                    description: "Tests SDK network creation, realm formation, peer joins, and interface lifecycle.",
                },
                ScenarioInfo {
                    name: "sdk_document_stress.lua",
                    description: "Tests SDK document CRDT operations, concurrent edits, sync, and persistence.",
                },
                ScenarioInfo {
                    name: "sdk_messaging_stress.lua",
                    description: "Tests SDK message delivery, reply threading, reactions, and member presence.",
                },
            ]),
        ]
    }

    /// Get description for a specific scenario
    pub fn get_scenario_description(name: &str) -> Option<&'static str> {
        for (_category, scenarios) in Self::get_categorized_scenarios() {
            for info in scenarios {
                if info.name == name {
                    return Some(info.description);
                }
            }
        }
        None
    }

    /// Runs a scenario and streams metrics updates
    pub async fn run_scenario(
        &self,
        name: &str,
        level: StressLevel,
        metrics_tx: Sender<MetricsUpdate>,
    ) -> std::io::Result<TestResult> {
        // Validate scenario exists
        let scenario_path = self.scenarios_dir.join(name);
        if !scenario_path.exists() {
            let err_msg = format!("Scenario file not found: {}", name);
            let _ = metrics_tx.send(MetricsUpdate::Error(err_msg.clone())).await;
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, err_msg));
        }

        // Validate lua_runner exists
        if !self.lua_runner_path.exists() {
            let err_msg = format!(
                "lua_runner binary not found at {}. Run 'cargo build' first.",
                self.lua_runner_path.display()
            );
            let _ = metrics_tx.send(MetricsUpdate::Error(err_msg.clone())).await;
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, err_msg));
        }

        // Ensure logs directory exists
        std::fs::create_dir_all(&self.logs_dir)?;

        let start_time = std::time::Instant::now();

        // Spawn lua_runner process with --pretty flag for console output
        // Set working directory to simulation/scripts so Lua package paths resolve correctly
        let scripts_dir = self.workspace_root.join("simulation/scripts");
        let relative_scenario_path = scenario_path
            .strip_prefix(&scripts_dir)
            .unwrap_or(&scenario_path);
        let mut child = Command::new(&self.lua_runner_path)
            .arg("--pretty") // Output to console instead of file
            .arg(relative_scenario_path)
            .current_dir(&scripts_dir)
            .env("STRESS_LEVEL", level.as_str())
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| std::io::Error::other("Failed to capture stderr"))?;

        // Spawn tasks to process output streams
        let metrics_tx_clone = metrics_tx.clone();
        let stdout_handle =
            tokio::spawn(async move { process_output_stream(stdout, metrics_tx_clone).await });

        let stderr_handle = tokio::spawn(async move {
            let mut stderr_reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                eprintln!("[lua_runner stderr] {}", line);
            }
        });

        // Wait for process to complete
        let status: std::process::ExitStatus = child.wait().await?;

        // Wait for output processing to complete
        let final_metrics = stdout_handle.await.ok().flatten();
        let _ = stderr_handle.await;

        let duration_secs = start_time.elapsed().as_secs_f64();

        let result = TestResult {
            scenario: name.to_string(),
            level: level.to_string(),
            passed: status.success(),
            metrics: final_metrics.unwrap_or_default(),
            duration_secs,
            timestamp: Utc::now(),
            errors: if status.success() {
                vec![]
            } else {
                vec![format!("Process exited with status: {}", status)]
            },
        };

        let _ = metrics_tx
            .send(MetricsUpdate::Complete(result.clone()))
            .await;

        Ok(result)
    }
}

impl Default for ScenarioRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Processes stdout stream and sends metrics updates
async fn process_output_stream(
    stdout: impl tokio::io::AsyncRead + Unpin,
    metrics_tx: Sender<MetricsUpdate>,
) -> Option<SimMetrics> {
    let mut reader = BufReader::new(stdout).lines();
    let mut last_metrics = SimMetrics::default();

    while let Ok(Some(line)) = reader.next_line().await {
        if let Some(update) = parse_jsonl_line(&line) {
            // Track last metrics for final result
            if let MetricsUpdate::Stats(ref metrics) = update {
                last_metrics = metrics.clone();
            }

            // Send update (ignore send errors if receiver dropped)
            let _ = metrics_tx.send(update).await;
        }
    }

    Some(last_metrics)
}

/// JSONL log entry structure
#[allow(dead_code)] // Reserved for future log parsing
#[derive(Debug, Deserialize)]
struct LogEntry {
    #[serde(default)]
    fields: serde_json::Value,
    #[serde(default)]
    level: String,
    #[serde(default)]
    message: String,
}

/// Strip ANSI escape codes from a string
fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

/// Parses a log line (either JSONL or pretty format) into a MetricsUpdate
fn parse_jsonl_line(line: &str) -> Option<MetricsUpdate> {
    // Strip ANSI codes first
    let clean_line = strip_ansi(line);

    // Try to parse as pure JSON first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&clean_line) {
        if let Some(fields) = json.get("fields") {
            return parse_fields(fields, &json);
        }
    }

    // Parse pretty format: " INFO message source="lua" fields={...}"
    // Extract log level
    let level = if clean_line.contains(" INFO") {
        "INFO"
    } else if clean_line.contains(" WARN") {
        "WARN"
    } else if clean_line.contains(" ERROR") {
        "ERROR"
    } else if clean_line.contains(" DEBUG") {
        return None; // Skip debug
    } else {
        "INFO"
    };

    // Extract fields JSON if present
    if let Some(fields_start) = clean_line.find("fields=") {
        let fields_str = &clean_line[fields_start + 7..];
        if let Ok(fields) = serde_json::from_str::<serde_json::Value>(fields_str) {
            // Extract message (text before source= or fields=)
            let message = clean_line
                .split(" source=")
                .next()
                .or_else(|| clean_line.split(" fields=").next())
                .map(|s| s.trim())
                .map(|s| {
                    // Remove level prefix
                    s.trim_start_matches(" INFO")
                        .trim_start_matches(" WARN")
                        .trim_start_matches(" ERROR")
                        .trim()
                })
                .unwrap_or("");

            // Check if this has metrics (routing or PQ crypto)
            if has_metrics_fields(&fields) {
                let tick = fields.get("tick").and_then(|v| v.as_u64()).unwrap_or(0);
                let metrics = extract_metrics(&fields, tick);
                return Some(MetricsUpdate::Stats(metrics));
            }

            // Check for tick progress
            if let Some(tick) = fields.get("tick").and_then(|v| v.as_u64()) {
                if let Some(max) = fields
                    .get("max_ticks")
                    .or(fields.get("ticks"))
                    .and_then(|v| v.as_u64())
                {
                    return Some(MetricsUpdate::Tick { current: tick, max });
                }
            }

            // Create event from message
            if !message.is_empty() {
                let event_type = match level {
                    "ERROR" => EventType::Error,
                    "WARN" => EventType::Warning,
                    _ => {
                        if message.contains("passed")
                            || message.contains("completed")
                            || message.contains("success")
                        {
                            EventType::Success
                        } else {
                            EventType::Info
                        }
                    }
                };

                let tick = fields.get("tick").and_then(|v| v.as_u64()).unwrap_or(0);
                return Some(MetricsUpdate::Event(SimEvent {
                    tick,
                    event_type,
                    description: message.to_string(),
                }));
            }
        }
    }

    // Parse simple message lines (no fields)
    let message = clean_line
        .trim_start_matches(" INFO")
        .trim_start_matches(" WARN")
        .trim_start_matches(" ERROR")
        .trim();

    if !message.is_empty() && !message.starts_with("Compiling") && !message.starts_with("Finished")
    {
        let event_type = if clean_line.contains(" ERROR") {
            EventType::Error
        } else if clean_line.contains(" WARN") {
            EventType::Warning
        } else if message.contains("completed")
            || message.contains("passed")
            || message.contains("success")
        {
            EventType::Success
        } else {
            EventType::Info
        };

        return Some(MetricsUpdate::Event(SimEvent {
            tick: 0,
            event_type,
            description: message.to_string(),
        }));
    }

    None
}

/// Parse fields from JSON structure
fn parse_fields(fields: &serde_json::Value, json: &serde_json::Value) -> Option<MetricsUpdate> {
    // Check for tick information
    if let Some(tick) = fields.get("tick").and_then(|v| v.as_u64()) {
        // Check if this looks like a metrics update (routing or PQ crypto)
        if has_metrics_fields(fields) {
            let metrics = extract_metrics(fields, tick);
            return Some(MetricsUpdate::Stats(metrics));
        }

        // Check for max_ticks to create tick progress
        if let Some(max) = fields
            .get("max_ticks")
            .or(fields.get("ticks"))
            .and_then(|v| v.as_u64())
        {
            return Some(MetricsUpdate::Tick { current: tick, max });
        }
    }

    // Check for message field to create events
    if let Some(message) = fields.get("message").and_then(|v| v.as_str()) {
        let tick = fields.get("tick").and_then(|v| v.as_u64()).unwrap_or(0);
        let level = json.get("level").and_then(|v| v.as_str()).unwrap_or("INFO");

        let event_type = match level.to_uppercase().as_str() {
            "ERROR" => EventType::Error,
            "WARN" | "WARNING" => EventType::Warning,
            "DEBUG" | "TRACE" => return None,
            _ => {
                if message.contains("passed") || message.contains("complete") {
                    EventType::Success
                } else {
                    EventType::Info
                }
            }
        };

        return Some(MetricsUpdate::Event(SimEvent {
            tick,
            event_type,
            description: message.to_string(),
        }));
    }

    None
}

/// Check if fields contain routing, PQ crypto, or SDK metrics
fn has_metrics_fields(fields: &serde_json::Value) -> bool {
    // Routing/messaging metrics
    fields.get("messages_sent").is_some()
        || fields.get("delivery_rate").is_some()
        // PQ signature metrics
        || fields.get("total_signatures_created").is_some()
        || fields.get("pq_signatures_created").is_some()
        || fields.get("avg_sign_latency_us").is_some()
        || fields.get("avg_verify_latency_us").is_some()
        // KEM metrics
        || fields.get("total_kem_encapsulations").is_some()
        || fields.get("kem_encapsulations").is_some()
        || fields.get("avg_encap_latency_us").is_some()
        || fields.get("avg_decap_latency_us").is_some()
        // Throughput
        || fields.get("ops_per_second").is_some()
        // SDK Messaging metrics
        || fields.get("threads_created").is_some()
        || fields.get("reactions_sent").is_some()
        || fields.get("presence_updates").is_some()
        || fields.get("total_members").is_some()
        // SDK Document metrics
        || fields.get("documents_created").is_some()
        || fields.get("total_updates").is_some()
        || fields.get("convergence_rate").is_some()
        // SDK Network metrics
        || fields.get("networks_created").is_some()
        || fields.get("realms_created").is_some()
        // Discovery metrics
        || fields.get("peers_discovered").is_some()
        || fields.get("discovery_completeness").is_some()
        || fields.get("pq_completeness").is_some()
        || fields.get("total_discoveries").is_some()
        || fields.get("realms_formed").is_some()
        || fields.get("churn_events").is_some()
        || fields.get("reconnect_count").is_some()
        || fields.get("introduction_requests_sent").is_some()
        || fields.get("rate_limited_count").is_some()
        || fields.get("discovery_latency_ticks").is_some()
}

/// Extracts metrics from log fields
fn extract_metrics(fields: &serde_json::Value, tick: u64) -> SimMetrics {
    SimMetrics {
        // Message/routing metrics
        messages_sent: fields
            .get("messages_sent")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        messages_delivered: fields
            .get("messages_delivered")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        messages_dropped: fields
            .get("messages_dropped")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        delivery_rate: fields
            .get("delivery_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        avg_latency: fields
            .get("avg_latency")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        avg_latency_ticks: fields
            .get("avg_latency_ticks")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        avg_hops: fields
            .get("avg_hops")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        backprops_completed: fields
            .get("backprops_completed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        backprops_timed_out: fields
            .get("backprops_timed_out")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),

        // PQ signature metrics
        pq_signatures_created: fields
            .get("total_signatures_created")
            .or(fields.get("pq_signatures_created"))
            .or(fields.get("signatures_created"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        pq_signatures_verified: fields
            .get("total_signatures_verified")
            .or(fields.get("pq_signatures_verified"))
            .or(fields.get("signatures_verified"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        pq_signature_failures: fields
            .get("signature_failures")
            .or(fields.get("pq_signature_failures"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        signature_verifications: fields
            .get("total_signatures_verified")
            .or(fields.get("signatures_verified"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        signature_failures: fields
            .get("signature_failures")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),

        // PQ latencies (microseconds)
        avg_sign_latency_us: fields
            .get("avg_sign_latency_us")
            .or(fields.get("latency_avg_us"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        avg_verify_latency_us: fields
            .get("avg_verify_latency_us")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        avg_encap_latency_us: fields
            .get("avg_encap_latency_us")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        avg_decap_latency_us: fields
            .get("avg_decap_latency_us")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),

        // KEM metrics
        kem_encapsulations: fields
            .get("total_kem_encapsulations")
            .or(fields.get("kem_encapsulations"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        kem_decapsulations: fields
            .get("total_kem_decapsulations")
            .or(fields.get("kem_decapsulations"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        kem_failures: fields
            .get("kem_failures")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),

        // Failure rates
        signature_failure_rate: fields
            .get("signature_failure_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        kem_failure_rate: fields
            .get("kem_failure_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),

        // Throughput
        ops_per_second: fields
            .get("ops_per_second")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),

        // Progress
        current_tick: tick,
        max_ticks: fields
            .get("max_ticks")
            .or(fields.get("total_ticks"))
            .and_then(|v| v.as_u64())
            .unwrap_or(tick),

        // SDK-specific metrics (Messaging)
        threads_created: fields
            .get("threads_created")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        reactions_sent: fields
            .get("reactions_sent")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        presence_updates: fields
            .get("presence_updates")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        channels_created: fields
            .get("channels_created")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        members_online: fields
            .get("total_members")
            .or(fields.get("members_online"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        member_joins: fields
            .get("member_joins")
            .or(fields.get("members_joined"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        member_leaves: fields
            .get("member_leaves")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        avg_thread_depth: fields
            .get("avg_thread_depth")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        max_thread_depth: fields
            .get("max_thread_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),

        // SDK-specific metrics (Document)
        documents_created: fields
            .get("documents_created")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        total_updates: fields
            .get("total_updates")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        sync_operations: fields
            .get("sync_operations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        convergence_rate: fields
            .get("convergence_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        persistence_operations: fields
            .get("persistence_operations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        reload_operations: fields
            .get("reload_operations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),

        // SDK-specific metrics (Network Lifecycle)
        networks_created: fields
            .get("networks_created")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        networks_destroyed: fields
            .get("networks_destroyed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        realms_created: fields
            .get("realms_created")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        realm_joins: fields
            .get("realm_joins")
            .or(fields.get("members_joined"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        active_members: fields
            .get("active_members")
            .or(fields.get("total_members"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),

        // SDK latency metrics (microseconds)
        p50_latency_us: fields
            .get("p50_latency_us")
            .or(fields.get("p50_send_latency_us"))
            .or(fields.get("p50_update_latency_us"))
            .or(fields.get("message_send_p50_us"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        p95_latency_us: fields
            .get("p95_latency_us")
            .or(fields.get("p95_send_latency_us"))
            .or(fields.get("p95_update_latency_us"))
            .or(fields.get("message_send_p95_us"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        p99_latency_us: fields
            .get("p99_latency_us")
            .or(fields.get("p99_send_latency_us"))
            .or(fields.get("p99_update_latency_us"))
            .or(fields.get("message_send_p99_us"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),

        // Discovery-specific metrics
        peers_discovered: fields
            .get("peers_discovered")
            .or(fields.get("total_discoveries"))
            .or(fields.get("discoveries"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        discovery_failures: fields
            .get("discovery_failures")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        discovery_completeness: fields
            .get("discovery_completeness")
            .or(fields.get("completeness"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        pq_key_completeness: fields
            .get("pq_key_completeness")
            .or(fields.get("pq_completeness"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        introduction_requests_sent: fields
            .get("introduction_requests_sent")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        introduction_responses_received: fields
            .get("introduction_responses_received")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        rate_limited_count: fields
            .get("rate_limited_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        rate_limit_violations: fields
            .get("rate_limit_violations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        realms_available: fields
            .get("realms_available")
            .or(fields.get("realms_formed"))
            .or(fields.get("realms_created"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        churn_events: fields
            .get("churn_events")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        reconnect_count: fields
            .get("reconnect_count")
            .or(fields.get("reconnects"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        discovery_latency_p99_ticks: fields
            .get("discovery_latency_p99_ticks")
            .or(fields.get("discovery_latency_p99"))
            .or(fields.get("discovery_latency_ticks"))
            .or(fields.get("avg_discovery_latency"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stress_level_serialization() {
        assert_eq!(StressLevel::Quick.as_str(), "quick");
        assert_eq!(StressLevel::Medium.as_str(), "medium");
        assert_eq!(StressLevel::Full.as_str(), "full");
    }

    #[test]
    fn test_categorized_scenarios() {
        let categories = ScenarioRunner::get_categorized_scenarios();
        assert!(!categories.is_empty());

        // Verify all categories have scenarios
        for (name, scenarios) in &categories {
            assert!(!name.is_empty());
            assert!(!scenarios.is_empty());
        }
    }

    #[test]
    fn test_parse_metrics_jsonl() {
        let line = r#"{"timestamp":"2026-01-23T10:00:00Z","level":"INFO","fields":{"tick":42,"messages_sent":150,"messages_delivered":148,"delivery_rate":0.987}}"#;

        let update = parse_jsonl_line(line);
        assert!(update.is_some());

        if let Some(MetricsUpdate::Stats(metrics)) = update {
            assert_eq!(metrics.current_tick, 42);
            assert_eq!(metrics.messages_sent, 150);
            assert_eq!(metrics.messages_delivered, 148);
        } else {
            panic!("Expected Stats update");
        }
    }

    #[test]
    fn test_parse_event_jsonl() {
        let line = r#"{"timestamp":"2026-01-23T10:00:00Z","level":"INFO","fields":{"tick":10,"message":"Starting routing stress test"}}"#;

        let update = parse_jsonl_line(line);
        assert!(update.is_some());

        if let Some(MetricsUpdate::Event(event)) = update {
            assert_eq!(event.tick, 10);
            assert!(event.description.contains("Starting"));
        } else {
            panic!("Expected Event update");
        }
    }
}
