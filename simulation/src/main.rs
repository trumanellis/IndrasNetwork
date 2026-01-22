//! Indra's Network - Mesh Network Simulation
//!
//! A simulation of peer-to-peer signal propagation with store-and-forward
//! routing for offline peers, inspired by iroh-examples patterns.

use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

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

    // Set up tracing
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
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
                    eprintln!("Unknown topology: {}. Using ring.", topology);
                    topology::MeshBuilder::new(peers).ring()
                }
            };
            println!("{}", mesh.visualize());
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
    println!("{}", mesh.visualize());

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
        let parts: Vec<&str> = input.trim().split_whitespace().collect();

        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "online" | "wake" => {
                if let Some(peer_str) = parts.get(1) {
                    if let Some(c) = peer_str.chars().next() {
                        if let Some(peer_id) = types::PeerId::new(c.to_ascii_uppercase()) {
                            sim.force_online(peer_id);
                            println!("  {} is now online", peer_id);
                        }
                    }
                }
            }
            "offline" | "sleep" => {
                if let Some(peer_str) = parts.get(1) {
                    if let Some(c) = peer_str.chars().next() {
                        if let Some(peer_id) = types::PeerId::new(c.to_ascii_uppercase()) {
                            sim.force_offline(peer_id);
                            println!("  {} is now offline", peer_id);
                        }
                    }
                }
            }
            "send" => {
                if parts.len() >= 4 {
                    let from_c = parts[1].chars().next().unwrap().to_ascii_uppercase();
                    let to_c = parts[2].chars().next().unwrap().to_ascii_uppercase();
                    let msg = parts[3..].join(" ");

                    if let (Some(from), Some(to)) = (types::PeerId::new(from_c), types::PeerId::new(to_c)) {
                        sim.send_message(from, to, msg.into_bytes());
                        println!("  Queued message {} -> {}", from, to);
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
                println!("  Advanced {} tick(s). {}", n, sim.state_summary());
            }
            "status" => {
                println!("  {}", sim.state_summary());
                println!("  Online peers:");
                for peer_id in sim.mesh.peer_ids() {
                    if sim.is_online(peer_id) {
                        let peer = sim.mesh.peers.get(&peer_id).unwrap();
                        println!("    {} - {} relayed packets", peer_id, peer.relay_queue.len());
                    }
                }
            }
            "stats" => {
                println!("  Messages sent: {}", sim.stats.messages_sent);
                println!("  Messages delivered: {}", sim.stats.messages_delivered);
                println!("  Messages dropped: {}", sim.stats.messages_dropped);
                println!("  Direct deliveries: {}", sim.stats.direct_deliveries);
                println!("  Relayed deliveries: {}", sim.stats.relayed_deliveries);
                println!("  Back-props completed: {}", sim.stats.backprops_completed);
            }
            "events" => {
                println!("  Event log ({} events):", sim.event_log.len());
                for event in sim.event_log.iter().rev().take(20) {
                    println!("    {:?}", event);
                }
                if sim.event_log.len() > 20 {
                    println!("    ... ({} more)", sim.event_log.len() - 20);
                }
            }
            "quit" | "exit" | "q" => {
                println!("Goodbye!");
                break;
            }
            _ => {
                println!("  Unknown command: {}", parts[0]);
            }
        }
    }

    Ok(())
}
