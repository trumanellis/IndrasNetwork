//! `indras-agent-hook` — Claude Code lifecycle hook relay.
//!
//! This is a short-lived subprocess invoked by Claude Code on each agent
//! lifecycle event (see `.claude/settings.json` wired by
//! `synchronicity_engine::agent_hooks::write_settings_template`). It dials
//! the Synchronicity Engine's unix IPC socket, writes one newline-framed JSON
//! status line, and exits.
//!
//! # Usage
//!
//! ```text
//! indras-agent-hook --event PreToolUse --tool Read --agent agent-foo --socket /path/to/sync.sock
//! indras-agent-hook --event UserPromptSubmit --agent agent-foo --socket /path/to/sync.sock
//! indras-agent-hook --event Stop --agent agent-foo --socket /path/to/sync.sock
//! ```
//!
//! Exits 0 on success, 1 if the socket dial or write fails.
//!
//! # Wire format
//!
//! ```json
//! {"kind":"agent_status","agent":"agent-foo","event":"PreToolUse","tool":"Read"}
//! ```
//!
//! The `"tool"` key is omitted when not applicable (e.g. `UserPromptSubmit`,
//! `Stop`).

use std::io::Write as IoWrite;
use std::os::unix::net::UnixStream;

/// CLI arguments — parsed manually to avoid a heavy clap dependency.
struct Args {
    /// Lifecycle event name (e.g. `PreToolUse`).
    event: String,
    /// Tool name, present only for `PreToolUse` / `PostToolUse`.
    tool: Option<String>,
    /// Full logical agent id (e.g. `agent-foo`).
    agent: String,
    /// Absolute path to the unix socket.
    socket: String,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut event = None;
    let mut tool = None;
    let mut agent = None;
    let mut socket = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--event" => {
                i += 1;
                event = args.get(i).cloned();
            }
            "--tool" => {
                i += 1;
                tool = args.get(i).cloned();
            }
            "--agent" => {
                i += 1;
                agent = args.get(i).cloned();
            }
            "--socket" => {
                i += 1;
                socket = args.get(i).cloned();
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
        i += 1;
    }

    Ok(Args {
        event: event.ok_or("--event is required")?,
        tool,
        agent: agent.ok_or("--agent is required")?,
        socket: socket.ok_or("--socket is required")?,
    })
}

/// Build the JSON status payload as a compact string.
///
/// Produces `{"kind":"agent_status","agent":"…","event":"…","tool":"…"}` where
/// the `"tool"` key is omitted when `tool` is `None`.
fn build_payload(args: &Args) -> String {
    // Build manually — avoids pulling in serde derive just for this struct,
    // and the format is simple enough that hand-construction is reliable.
    let tool_field = match &args.tool {
        Some(t) => {
            let escaped = t.replace('\\', "\\\\").replace('"', "\\\"");
            format!(r#","tool":"{escaped}""#)
        }
        None => String::new(),
    };
    let agent = args.agent.replace('\\', "\\\\").replace('"', "\\\"");
    let event = args.event.replace('\\', "\\\\").replace('"', "\\\"");
    format!(r#"{{"kind":"agent_status","agent":"{agent}","event":"{event}"{tool_field}}}"#)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("indras-agent-hook: {e}");
            std::process::exit(1);
        }
    };

    let payload = build_payload(&args);

    // Dial the socket synchronously — hooks are short-lived, no async needed.
    let mut stream = match UnixStream::connect(&args.socket) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("indras-agent-hook: failed to connect to socket {}: {e}", args.socket);
            std::process::exit(1);
        }
    };

    // Newline-framed JSON, matching the IPC server's line reader.
    let mut line = payload;
    line.push('\n');
    if let Err(e) = stream.write_all(line.as_bytes()) {
        eprintln!("indras-agent-hook: write failed: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_with_tool() {
        let args = Args {
            event: "PreToolUse".into(),
            tool: Some("Read".into()),
            agent: "agent-foo".into(),
            socket: "/tmp/sync.sock".into(),
        };
        let p = build_payload(&args);
        assert!(p.contains(r#""kind":"agent_status""#));
        assert!(p.contains(r#""agent":"agent-foo""#));
        assert!(p.contains(r#""event":"PreToolUse""#));
        assert!(p.contains(r#""tool":"Read""#));
    }

    #[test]
    fn payload_without_tool() {
        let args = Args {
            event: "Stop".into(),
            tool: None,
            agent: "agent-bar".into(),
            socket: "/tmp/sync.sock".into(),
        };
        let p = build_payload(&args);
        assert!(!p.contains("tool"), "tool key should be absent: {p}");
        assert!(p.contains(r#""event":"Stop""#));
    }

    #[test]
    fn payload_escapes_special_chars() {
        let args = Args {
            event: "PreToolUse".into(),
            tool: Some(r#"To"ol"#.into()),
            agent: "agent-test".into(),
            socket: "/tmp/s.sock".into(),
        };
        let p = build_payload(&args);
        // Must be valid JSON.
        let _: serde_json::Value = serde_json::from_str(&p)
            .expect("payload should be valid JSON even with special chars");
    }
}
