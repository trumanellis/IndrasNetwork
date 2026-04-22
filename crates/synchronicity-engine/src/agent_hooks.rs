//! Agent hook-settings template writer.
//!
//! When the Agent Roster creates a new agent folder it calls
//! [`write_settings_template`] to drop a `.claude/settings.json` into that
//! folder. The settings file wires all four Claude Code lifecycle hooks
//! (`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`) to the
//! `indras-agent-hook` binary so the Synchronicity Engine can track each
//! agent's runtime status in real time.
//!
//! The hook binary is invoked as a subprocess by Claude Code; it dials the
//! IPC unix socket, emits one JSON status line, and exits. The hook binary
//! path and socket path are baked into the generated settings file at
//! creation time so the file is self-contained and survives renames.

use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

/// Resolve the absolute path of the `indras-agent-hook` sibling binary.
///
/// Looks for the binary next to the currently running executable (which is
/// how cargo places release builds and `cargo run` symlinks). Falls back to
/// a `debug` sibling path for development. Returns `None` if neither is
/// found — callers should surface this as a non-fatal warning.
pub fn resolve_hook_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    // Release / `cargo install` layout: hook sits next to the main binary.
    let sibling = exe_dir.join("indras-agent-hook");
    if sibling.exists() {
        return Some(sibling);
    }

    // Development layout: main binary is in `target/debug/synchronicity-engine`;
    // hook is `target/debug/indras-agent-hook`.
    let debug_hook = exe_dir.join("indras-agent-hook");
    if debug_hook.exists() {
        return Some(debug_hook);
    }

    None
}

/// Write `.claude/settings.json` into `agent_folder` wiring all four
/// Claude Code lifecycle hooks to `hook_binary_path`, emitting events to
/// `socket_path`.
///
/// Creates `.claude/` if it does not exist. Overwrites an existing
/// `settings.json` — the template is always regenerated from the live
/// socket and binary paths.
///
/// `agent_sandbox_root` is the absolute path the cooperative sandbox check
/// in [`indras-agent-hook`](../indras_agent_hook/index.html) will use to
/// reject `PreToolUse` file operations whose resolved target escapes this
/// folder. Typically the agent's own working-tree folder, but any
/// directory prefix is accepted (the check is a canonical-path prefix
/// match).
///
/// # Hook events wired
///
/// | Hook name        | `--event` value    | When Claude fires it           |
/// |------------------|--------------------|--------------------------------|
/// | `UserPromptSubmit` | `UserPromptSubmit` | User sends a message           |
/// | `PreToolUse`     | `PreToolUse`       | Before each tool call          |
/// | `PostToolUse`    | `PostToolUse`      | After each tool call succeeds  |
/// | `Stop`           | `Stop`             | Agent session ends             |
pub fn write_settings_template(
    agent_folder: &Path,
    socket_path: &Path,
    hook_binary_path: &Path,
    agent_sandbox_root: &Path,
) -> std::io::Result<()> {
    let claude_dir = agent_folder.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;

    let settings_path = claude_dir.join("settings.json");

    // Canonicalize paths to absolute strings — the hook subprocess is
    // launched by Claude Code from an arbitrary cwd.
    let hook = hook_binary_path.to_string_lossy();
    let sock = socket_path.to_string_lossy();
    let sandbox = agent_sandbox_root.to_string_lossy();

    let hook_command = |event: &str, tool_flag: &str| -> String {
        if tool_flag.is_empty() {
            format!(
                "{hook} --event {event} --agent {{agent_id}} --socket {sock} --sandbox-root {sandbox}"
            )
        } else {
            format!(
                "{hook} --event {event} {tool_flag} --agent {{agent_id}} --socket {sock} --sandbox-root {sandbox}"
            )
        }
    };

    let json = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [
                {
                    "type": "command",
                    "command": hook_command("UserPromptSubmit", "")
                }
            ],
            "PreToolUse": [
                {
                    "type": "command",
                    "command": hook_command("PreToolUse", "--tool {tool_name}")
                }
            ],
            "PostToolUse": [
                {
                    "type": "command",
                    "command": hook_command("PostToolUse", "--tool {tool_name}")
                }
            ],
            "Stop": [
                {
                    "type": "command",
                    "command": hook_command("Stop", "")
                }
            ]
        }
    });

    let mut file = std::fs::File::create(&settings_path)?;
    file.write_all(serde_json::to_string_pretty(&json)?.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn write_settings_creates_claude_dir_and_file() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_folder = tmp.path().join("agent-test");
        std::fs::create_dir_all(&agent_folder).unwrap();

        let socket = PathBuf::from("/tmp/sync.sock");
        let hook_bin = PathBuf::from("/usr/local/bin/indras-agent-hook");
        let sandbox = agent_folder.clone();

        write_settings_template(&agent_folder, &socket, &hook_bin, &sandbox)
            .expect("write_settings_template should succeed");

        let settings_path = agent_folder.join(".claude/settings.json");
        assert!(settings_path.exists(), ".claude/settings.json not created");

        let contents = std::fs::read_to_string(&settings_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert!(val["hooks"]["UserPromptSubmit"].is_array());
        assert!(val["hooks"]["PreToolUse"].is_array());
        assert!(val["hooks"]["PostToolUse"].is_array());
        assert!(val["hooks"]["Stop"].is_array());

        // Verify the socket path is embedded in the command strings.
        let pre = val["hooks"]["PreToolUse"][0]["command"].as_str().unwrap();
        assert!(pre.contains("/tmp/sync.sock"), "socket path missing from command: {pre}");
        assert!(pre.contains("indras-agent-hook"), "binary path missing from command: {pre}");
        assert!(
            pre.contains("--sandbox-root"),
            "sandbox-root flag missing from command: {pre}"
        );
        assert!(
            pre.contains(&*sandbox.to_string_lossy()),
            "sandbox path missing from command: {pre}"
        );
    }

    #[test]
    fn write_settings_overwrites_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_folder = tmp.path().join("agent-overwrite");
        std::fs::create_dir_all(&agent_folder.join(".claude")).unwrap();
        std::fs::write(agent_folder.join(".claude/settings.json"), b"old content").unwrap();

        write_settings_template(
            &agent_folder,
            &PathBuf::from("/tmp/sync.sock"),
            &PathBuf::from("/bin/hook"),
            &agent_folder,
        )
        .unwrap();

        let contents =
            std::fs::read_to_string(agent_folder.join(".claude/settings.json")).unwrap();
        assert!(contents.starts_with('{'), "expected JSON, got: {contents}");
    }
}
