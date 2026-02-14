use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Clone)]
pub struct MountConfig {
    pub mount_point: PathBuf,
    pub player_id: [u8; 32],
    pub allow_other: bool,
    pub auto_unmount: bool,
    pub foreground: bool,
    pub log_level: String,
}

impl Default for MountConfig {
    fn default() -> Self {
        Self {
            mount_point: PathBuf::from("/indra"),
            player_id: [0u8; 32],
            allow_other: false,
            auto_unmount: true,
            foreground: false,
            log_level: "info".into(),
        }
    }
}

#[derive(Parser)]
#[command(name = "indras-fuse", about = "P2P Artifact Vault as a FUSE Filesystem")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Mount the vault filesystem
    Mount {
        /// Mount point path
        path: PathBuf,
        /// Player ID (hex-encoded 32 bytes)
        #[arg(long)]
        player: Option<String>,
        /// Allow other users to access the mount
        #[arg(long)]
        allow_other: bool,
        /// Automatically unmount when the process exits
        #[arg(long, default_value_t = true)]
        auto_unmount: bool,
        /// Run in foreground (for debugging)
        #[arg(long)]
        foreground: bool,
        /// Log level (trace, debug, info, warn, error)
        #[arg(long, default_value = "info")]
        log_level: String,
    },
    /// Unmount the vault filesystem
    Unmount {
        /// Mount point path
        path: PathBuf,
    },
    /// Show mount status
    Status {
        /// Mount point path
        path: PathBuf,
    },
}
