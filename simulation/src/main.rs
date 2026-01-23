//! Indra's Network - Mesh Network Simulation
//!
//! A simulation of peer-to-peer signal propagation with store-and-forward
//! routing for offline peers, inspired by iroh-examples patterns.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::{info, debug};

use indras_logging::{IndrasSubscriberBuilder, LogConfig, FileConfig, RotationStrategy};
use indras_simulation::{types, topology, simulation, scenarios};

#[derive(Parser)]
#[command(
    name = "indras-network",
    about = "Mesh network simulation with store-and-forward routing",
    version
)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Directory for JSONL log files (optional)
    #[arg(long, global = true)]
    log_dir: Option<PathBuf>,

    /// Enable OpenTelemetry export (requires OTEL_EXPORTER_OTLP_ENDPOINT)
    #[arg(long, global = true)]
    otel: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the canonical A-B-C scenario from the spec
    Abc,
    
    /// Run a multi-hop line relay scenario
    Line,
    
    /// Run a broadcast scenario (hub-and-spoke)
    Broadcast,
    
    /// Run a random chaos simulation
    Chaos {
        /// Number of ticks to run
        #[arg(short, long, default_value = "100")]
        ticks: u64,
    },
    
    /// Run the network partition scenario
    Partition,
    
    /// Create and visualize a custom topology
    Topology {
        /// Type of topology: ring, full, random, line, star
        #[arg(short, long, default_value = "ring")]
        topology: String,
        
        /// Number of peers (max 26)
        #[arg(short, long, default_value = "6")]
        peers: usize,
        
        /// Connection probability for random topology
        #[arg(short, long, default_value = "0.4")]
        connection_prob: f64,
    },
    
    /// Interactive simulation mode
    Interactive {
        /// Number of peers
        #[arg(short, long, default_value = "5")]
        peers: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Set up logging using indras-logging
    let mut config = LogConfig::default();

    if cli.verbose {
        config.default_level = "debug".to_string();
    }

    // Determine scenario name for log file prefix
    let scenario_name = match &cli.command {
        Commands::Abc => "abc",
        Commands::Line => "line",
        Commands::Broadcast => "broadcast",
        Commands::Chaos { .. } => "chaos",
        Commands::Partition => "partition",
        Commands::Topology { .. } => "topology",
        Commands::Interactive { .. } => "interactive",
    };

    // File logging enabled by default to ./logs, override with --log-dir
    // Disable console output - only write to file
    config.console.enabled = false;

    let log_dir = cli.log_dir.unwrap_or_else(|| PathBuf::from("./logs"));
    config.file = Some(FileConfig {
        directory: log_dir,
        prefix: scenario_name.to_string(),
        rotation: RotationStrategy::Never, // Single file, overwritten each run
        max_files: None,
    });

    // Enable OTel if requested
    if cli.otel {
        config.otel.enabled = true;
    }

    // The guard must be kept alive for file logging to work
    let _guard = IndrasSubscriberBuilder::new()
        .with_config(config)
        .init();

    match cli.command {
        Commands::Abc => {
            scenarios::run_abc_scenario();
        }
        Commands::Line => {
            scenarios::run_line_relay_scenario();
        }
        Commands::Broadcast => {
            scenarios::run_broadcast_scenario();
        }
        Commands::Chaos { ticks } => {
            scenarios::run_random_chaos_scenario(ticks);
        }
        Commands::Partition => {
            scenarios::run_partition_scenario();
        }
        Commands::Topology { topology, peers, connection_prob } => {
            let mesh = match topology.as_str() {
                "ring" => topology::MeshBuilder::new(peers).ring(),
                "full" => topology::MeshBuilder::new(peers).full_mesh(),
                "random" => topology::MeshBuilder::new(peers).random(connection_prob),
                "line" => topology::MeshBuilder::new(peers).line(),
                "star" => topology::MeshBuilder::new(peers).star(),
                _ => {
                    tracing::warn!(topology = %topology, "Unknown topology, using ring");
                    topology::MeshBuilder::new(peers).ring()
                }
            };
            info!(topology = %mesh.visualize(), "Mesh topology created");
        }
        Commands::Interactive { peers } => {
            run_interactive(peers)?;
        }
    }

    Ok(())
}

fn run_interactive(peer_count: usize) -> anyhow::Result<()> {
    use std::io::{self, Write};

    let mesh = topology::MeshBuilder::new(peer_count).random(0.5);
    info!(topology = %mesh.visualize(), "Interactive mode: mesh topology created");

    let mut sim = simulation::Simulation::new(mesh, simulation::SimConfig {
        wake_probability: 0.0,
        sleep_probability: 0.0,
        trace_routing: true,
        ..Default::default()
    });

    // Start with half online
    for (i, c) in ('A'..).take(peer_count).enumerate() {
        if i % 2 == 0 {
            sim.force_online(types::PeerId(c));
        }
    }

    // User-facing help (keep as println for CLI UX)
    println!("\nInteractive mode. Commands:");
    println!("  online <peer>   - Bring peer online (e.g., 'online A')");
    println!("  offline <peer>  - Take peer offline");
    println!("  send <from> <to> <msg> - Send message");
    println!("  step [n]        - Advance n ticks (default 1)");
    println!("  status          - Show current state");
    println!("  stats           - Show statistics");
    println!("  events          - Show event log");
    println!("  quit            - Exit");
    println!();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let parts: Vec<&str> = input.split_whitespace().collect();

        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "online" | "wake" => {
                if let Some(peer_str) = parts.get(1)
                    && let Some(c) = peer_str.chars().next()
                        && let Some(peer_id) = types::PeerId::new(c.to_ascii_uppercase()) {
                            sim.force_online(peer_id);
                            info!(peer = %peer_id, "Peer is now online");
                        }
            }
            "offline" | "sleep" => {
                if let Some(peer_str) = parts.get(1)
                    && let Some(c) = peer_str.chars().next()
                        && let Some(peer_id) = types::PeerId::new(c.to_ascii_uppercase()) {
                            sim.force_offline(peer_id);
                            info!(peer = %peer_id, "Peer is now offline");
                        }
            }
            "send" => {
                if parts.len() >= 4 {
                    let from_c = parts[1].chars().next().unwrap().to_ascii_uppercase();
                    let to_c = parts[2].chars().next().unwrap().to_ascii_uppercase();
                    let msg = parts[3..].join(" ");

                    if let (Some(from), Some(to)) = (types::PeerId::new(from_c), types::PeerId::new(to_c)) {
                        sim.send_message(from, to, msg.into_bytes());
                        info!(from = %from, to = %to, "Message queued");
                    }
                } else {
                    println!("  Usage: send <from> <to> <message>");
                }
            }
            "step" => {
                let n: u64 = parts.get(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                sim.run_ticks(n);
                info!(ticks = n, state = %sim.state_summary(), "Advanced simulation");
            }
            "status" => {
                let online_peers: Vec<String> = sim.mesh.peer_ids()
                    .into_iter()
                    .filter(|&peer_id| sim.is_online(peer_id))
                    .map(|peer_id| {
                        let peer = sim.mesh.peers.get(&peer_id).unwrap();
                        format!("{}({})", peer_id, peer.relay_queue.len())
                    })
                    .collect();
                info!(
                    state = %sim.state_summary(),
                    online_peers = ?online_peers,
                    "Current status"
                );
            }
            "stats" => {
                info!(
                    messages_sent = sim.stats.messages_sent,
                    messages_delivered = sim.stats.messages_delivered,
                    messages_dropped = sim.stats.messages_dropped,
                    direct_deliveries = sim.stats.direct_deliveries,
                    relayed_deliveries = sim.stats.relayed_deliveries,
                    backprops_completed = sim.stats.backprops_completed,
                    "Statistics"
                );
            }
            "events" => {
                let event_count = sim.event_log.len();
                for event in sim.event_log.iter().rev().take(20) {
                    debug!(event = ?event, "Event log entry");
                }
                info!(total_events = event_count, shown = std::cmp::min(20, event_count), "Event log");
            }
            "quit" | "exit" | "q" => {
                info!("Interactive session ended");
                break;
            }
            _ => {
                tracing::warn!(command = parts[0], "Unknown command");
            }
        }
    }

    Ok(())
}
