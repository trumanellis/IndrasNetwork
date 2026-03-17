//! Indras Relay Server
//!
//! A blind relay server that stores and forwards encrypted events
//! for the Indras P2P mesh network.

use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use indras_relay::RelayConfig;

/// Indras Relay Server - blind store-and-forward for encrypted P2P events
#[derive(Parser, Debug)]
#[command(name = "indras-relay", version, about)]
struct Cli {
    /// Path to configuration file (TOML)
    #[arg(short, long, default_value = "relay.toml")]
    config: PathBuf,

    /// Data directory (overrides config file)
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// Admin API bind address (overrides config file)
    #[arg(long)]
    admin_bind: Option<std::net::SocketAddr>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("indras_relay=info".parse()?))
        .init();

    let cli = Cli::parse();

    // Load config
    let mut config = if cli.config.exists() {
        RelayConfig::from_file(&cli.config)?
    } else {
        tracing::info!("No config file found, using defaults");
        RelayConfig::default()
    };

    // Apply CLI overrides
    if let Some(data_dir) = cli.data_dir {
        config.data_dir = data_dir;
    }
    if let Some(admin_bind) = cli.admin_bind {
        config.admin_bind = admin_bind;
    }

    if config.admin_token == "change-me" {
        tracing::warn!("Admin API token is set to default 'change-me' — change this for production use");
    }

    tracing::info!(
        name = %config.display_name,
        data_dir = %config.data_dir.display(),
        admin = %config.admin_bind,
        "Starting Indras Relay"
    );

    // Create and run relay node
    let relay = indras_relay::RelayNode::new(config).await?;
    relay.run().await?;

    Ok(())
}
