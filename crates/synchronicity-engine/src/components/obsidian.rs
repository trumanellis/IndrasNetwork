//! Obsidian integration — detect and register vaults via `obsidian.json`.
//!
//! Obsidian offers no supported mechanism (CLI, URL scheme, or API) for
//! another app to register a vault. The unofficial-but-stable approach is
//! to edit its config file directly:
//!
//!   macOS:   ~/Library/Application Support/obsidian/obsidian.json
//!   Linux:   ~/.config/obsidian/obsidian.json
//!   Windows: %APPDATA%\obsidian\obsidian.json
//!
//! Shape:
//! ```json
//! {
//!   "vaults": {
//!     "<16-hex-id>": { "path": "<abs>", "ts": <ms>, "open": true }
//!   }
//! }
//! ```
//!
//! This schema has been stable across many Obsidian releases, but is
//! undocumented — touch with care.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Locate Obsidian's config file for the current platform.
pub fn obsidian_json_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        return Some(PathBuf::from(home).join("Library/Application Support/obsidian/obsidian.json"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var_os("HOME")?;
        return Some(PathBuf::from(home).join(".config/obsidian/obsidian.json"));
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA")?;
        return Some(PathBuf::from(appdata).join("obsidian/obsidian.json"));
    }
    #[allow(unreachable_code)]
    None
}

/// Is a directory already registered as an Obsidian vault (by exact path)?
pub fn is_vault_registered(vault_dir: &Path) -> bool {
    let Some(cfg_path) = obsidian_json_path() else { return false; };
    let Ok(raw) = std::fs::read_to_string(&cfg_path) else { return false; };
    let Ok(json): Result<Value, _> = serde_json::from_str(&raw) else { return false; };
    let Some(vaults) = json.get("vaults").and_then(|v| v.as_object()) else { return false; };

    let target = vault_dir.to_string_lossy();
    vaults.values().any(|entry| {
        entry
            .get("path")
            .and_then(|p| p.as_str())
            .map(|p| p == target.as_ref())
            .unwrap_or(false)
    })
}

/// Register a directory as an Obsidian vault by appending to `obsidian.json`.
///
/// Creates the file (and parent dir) if missing. Fails silently-ish —
/// returns an error which the caller can log; the UI should just re-check
/// registration state afterwards.
pub fn register_vault(vault_dir: &Path) -> std::io::Result<()> {
    let cfg_path = obsidian_json_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Unsupported, "unsupported platform")
    })?;

    if let Some(parent) = cfg_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut root: Value = match std::fs::read_to_string(&cfg_path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    };

    if !root.is_object() {
        root = json!({});
    }
    let root_obj = root.as_object_mut().expect("root is object");
    let vaults = root_obj
        .entry("vaults".to_string())
        .or_insert_with(|| json!({}));
    if !vaults.is_object() {
        *vaults = json!({});
    }
    let vaults_obj = vaults.as_object_mut().expect("vaults is object");

    let id = fresh_vault_id();
    let ts_ms = chrono::Utc::now().timestamp_millis();
    vaults_obj.insert(
        id,
        json!({
            "path": vault_dir.to_string_lossy(),
            "ts": ts_ms,
            "open": true,
        }),
    );

    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&cfg_path, serialized)
}

/// Quit any running Obsidian instance so the next launch re-reads
/// `obsidian.json` (and thus sees the newly-registered vault). No-op if
/// Obsidian isn't running. macOS uses `osascript` for a graceful quit;
/// Linux/Windows fall back to a best-effort `pkill` / `taskkill`.
pub fn quit_obsidian() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .args(["-e", "tell application \"Obsidian\" to quit"])
            .status();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-x", "obsidian"])
            .status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "Obsidian.exe", "/F"])
            .status();
    }
}

/// Generate a 16-hex-char ID matching Obsidian's own format. Uses
/// nanosecond time + PID as entropy — collisions are astronomically
/// unlikely in this one-shot usage.
fn fresh_vault_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mixed = nanos ^ (pid.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    format!("{:016x}", (mixed as u64))
}
