use clap::Parser;
use fuser::MountOption;
use indras_artifacts::Vault;
use indras_fuse::config::{Cli, Command};
use indras_fuse::IndraFS;
use rand::Rng;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("Hex string must have even length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Mount {
            path,
            player,
            allow_other,
            auto_unmount,
            foreground: _,
            log_level,
        } => {
            // Set up logging
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new(&log_level)),
                )
                .init();

            // Parse or generate player ID
            let player_id: [u8; 32] = if let Some(ref hex_str) = player {
                decode_hex(hex_str)
                    .map_err(|e| anyhow::anyhow!("Invalid hex player ID: {}", e))?
                    .try_into()
                    .map_err(|v: Vec<u8>| {
                        anyhow::anyhow!("Player ID must be exactly 32 bytes, got {}", v.len())
                    })?
            } else {
                rand::rng().random()
            };

            // Create mount point if it doesn't exist
            std::fs::create_dir_all(&path)?;

            // Create in-memory vault
            let now = chrono::Utc::now().timestamp_millis();
            let vault = Vault::in_memory(player_id, now)?;

            // Get current user's uid/gid
            let uid = unsafe { libc::getuid() };
            let gid = unsafe { libc::getgid() };

            // Create filesystem
            let fs = IndraFS::new(vault, uid, gid);

            // Build mount options
            let mut options = vec![
                MountOption::FSName("indras-fuse".into()),
                MountOption::AutoUnmount,
                MountOption::AllowRoot,
            ];

            if allow_other {
                options.push(MountOption::AllowOther);
            }

            if !auto_unmount {
                // Remove AutoUnmount if explicitly disabled
                options.retain(|opt| !matches!(opt, MountOption::AutoUnmount));
            }

            println!("Mounted at {}", path.display());
            fuser::mount2(fs, &path, &options)?;
            println!("Unmounted");
        }

        Command::Unmount { path } => {
            unmount(&path)?;
            println!("Unmounted {}", path.display());
        }

        Command::Status { path } => {
            if path.exists() {
                if is_mounted(&path)? {
                    println!("Status: MOUNTED at {}", path.display());
                } else {
                    println!("Status: NOT MOUNTED (path exists but is not a mount point)");
                }
            } else {
                println!("Status: NOT MOUNTED (path does not exist)");
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn unmount(path: &PathBuf) -> anyhow::Result<()> {
    let output = std::process::Command::new("umount")
        .arg(path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "umount failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn unmount(path: &PathBuf) -> anyhow::Result<()> {
    let output = std::process::Command::new("fusermount")
        .arg("-u")
        .arg(path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "fusermount -u failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn unmount(_path: &PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("Unmount not supported on this platform")
}

fn is_mounted(path: &PathBuf) -> anyhow::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("mount")
            .output()?;

        let mount_output = String::from_utf8_lossy(&output.stdout);
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        Ok(mount_output.contains(&canonical.display().to_string()))
    }

    #[cfg(target_os = "linux")]
    {
        let mounts = std::fs::read_to_string("/proc/mounts")?;
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        Ok(mounts.contains(&canonical.display().to_string()))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(false)
    }
}
