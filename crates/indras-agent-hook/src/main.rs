//! `indras-agent-hook` — Claude Code lifecycle hook relay + cooperative
//! path sandbox.
//!
//! This is a short-lived subprocess invoked by Claude Code on each agent
//! lifecycle event (see `.claude/settings.json` wired by
//! `synchronicity_engine::agent_hooks::write_settings_template`). It has
//! two jobs:
//!
//! 1. Dial the Synchronicity Engine's unix IPC socket and write one
//!    newline-framed JSON status line so the engine UI can track runtime
//!    state in real time.
//! 2. On `PreToolUse`, read the full hook JSON from stdin and refuse tool
//!    calls whose resolved file path escapes the agent's sandbox root.
//!    This is the cooperative half of the Phase 3 sandbox — an honest
//!    Claude Code client will not run the tool when the hook exits with
//!    code 2.
//!
//! # Usage
//!
//! ```text
//! indras-agent-hook --event PreToolUse --tool Read --agent agent-foo \
//!                   --socket /path/to/sync.sock \
//!                   --sandbox-root /path/to/agent-foo
//! ```
//!
//! # Exit codes
//!
//! | Code | Meaning                                              |
//! |------|------------------------------------------------------|
//! | 0    | Success (status line emitted, tool call permitted).  |
//! | 1    | Internal error (bad args, socket dial/write failed). |
//! | 2    | Sandbox violation — Claude Code blocks the tool.     |
//!
//! # Wire format
//!
//! ```json
//! {"kind":"agent_status","agent":"agent-foo","event":"PreToolUse","tool":"Read"}
//! ```
//!
//! The `"tool"` key is omitted when not applicable (e.g. `UserPromptSubmit`,
//! `Stop`).

use std::io::{Read as IoRead, Write as IoWrite};
use std::os::unix::net::UnixStream;
use std::path::{Component, Path, PathBuf};

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
    /// Optional sandbox root (absolute path). When set, `PreToolUse`
    /// events whose target path escapes this root are rejected.
    sandbox_root: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut event = None;
    let mut tool = None;
    let mut agent = None;
    let mut socket = None;
    let mut sandbox_root = None;

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
            "--sandbox-root" => {
                i += 1;
                sandbox_root = args.get(i).map(PathBuf::from);
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
        sandbox_root,
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

/// Resolve `target` against `sandbox` and report whether the resolved path
/// is contained within `sandbox`.
///
/// Canonicalizes both paths first (following symlinks, collapsing `..`). If
/// `target` is relative, it is joined onto `sandbox` before canonicalizing.
/// If the target path does not yet exist, we walk up to the nearest ancestor
/// that *does* and canonicalize that, so an unresolved `./new/file.txt`
/// inside the sandbox still returns `true`. A `..` segment that pops above
/// `sandbox` is rejected.
///
/// Returns `true` if `target` is inside `sandbox`, `false` otherwise.
pub fn path_in_sandbox(sandbox: &Path, target: &Path) -> bool {
    // Canonicalize sandbox first; bail open if we can't resolve it (treat
    // an unreadable sandbox as "allow everything" rather than breaking
    // the user's agent with an unresolvable config).
    let sandbox_canon = match sandbox.canonicalize() {
        Ok(p) => p,
        Err(_) => return true,
    };

    // Join relative target onto sandbox so PWD-free tools still resolve
    // predictably (Claude Code sometimes passes paths relative to the
    // project root).
    let joined: PathBuf = if target.is_absolute() {
        target.to_path_buf()
    } else {
        sandbox_canon.join(target)
    };

    // Walk up to the nearest existing ancestor for canonicalization, then
    // re-append the non-existent tail. This lets "write a new file inside
    // sandbox" succeed without requiring the file to exist first.
    let canon = canonicalize_with_fallback(&joined);

    canon.starts_with(&sandbox_canon)
}

/// Canonicalize `p` if it exists; otherwise canonicalize the longest
/// existing ancestor and re-attach the unresolved tail, with `..`
/// components applied lexically.
fn canonicalize_with_fallback(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    // Find longest existing ancestor.
    let mut existing = p.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(name) => tail.push(name.to_os_string()),
            None => break,
        }
        if !existing.pop() {
            break;
        }
    }
    tail.reverse();

    let base = existing.canonicalize().unwrap_or(existing);
    let mut out = base;
    for seg in tail {
        let seg_path = Path::new(&seg);
        match seg_path.components().next() {
            Some(Component::ParentDir) => {
                out.pop();
            }
            Some(Component::CurDir) | None => {}
            _ => out.push(seg_path),
        }
    }
    out
}

/// Extract a candidate file-system path from a parsed Claude Code hook
/// JSON body, checking the common tool-input keys (`file_path`, `path`,
/// `notebook_path`, `cwd`). Returns `None` if no recognizable path is
/// present — callers should treat that as "nothing to gate".
fn extract_path(body: &serde_json::Value) -> Option<PathBuf> {
    let tool_input = body.get("tool_input")?;
    for key in ["file_path", "path", "notebook_path", "cwd"] {
        if let Some(s) = tool_input.get(key).and_then(|v| v.as_str())
            && !s.is_empty()
        {
            return Some(PathBuf::from(s));
        }
    }
    None
}

/// Enforce the cooperative sandbox on a `PreToolUse` event.
///
/// Reads the full hook JSON body from stdin, extracts the target path from
/// `tool_input`, and returns `Err((code, message))` when the path escapes
/// `sandbox`. Returns `Ok(())` when the body is missing, unparseable, or
/// has no recognizable path — "no target" is treated as "no gate", so the
/// engine's existing status line still fires for tools like `Bash` that
/// don't carry a file path.
fn enforce_sandbox(sandbox: &Path) -> Result<(), (i32, String)> {
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() || buf.trim().is_empty() {
        return Ok(());
    }
    let body: serde_json::Value = match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(target) = extract_path(&body) else {
        return Ok(());
    };
    if path_in_sandbox(sandbox, &target) {
        Ok(())
    } else {
        Err((
            2,
            format!(
                "sandbox violation: path {} is outside {}",
                target.display(),
                sandbox.display()
            ),
        ))
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("indras-agent-hook: {e}");
            std::process::exit(1);
        }
    };

    // Enforce the cooperative sandbox before emitting the status line.
    // A rejected PreToolUse still counts as "the agent tried X"; reporting
    // it upstream is future work.
    if args.event == "PreToolUse"
        && let Some(sandbox) = args.sandbox_root.as_deref()
        && let Err((code, msg)) = enforce_sandbox(sandbox)
    {
        eprintln!("indras-agent-hook: {msg}");
        std::process::exit(code);
    }

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
    use tempfile::TempDir;

    #[test]
    fn payload_with_tool() {
        let args = Args {
            event: "PreToolUse".into(),
            tool: Some("Read".into()),
            agent: "agent-foo".into(),
            socket: "/tmp/sync.sock".into(),
            sandbox_root: None,
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
            sandbox_root: None,
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
            sandbox_root: None,
        };
        let p = build_payload(&args);
        // Must be valid JSON.
        let _: serde_json::Value = serde_json::from_str(&p)
            .expect("payload should be valid JSON even with special chars");
    }

    /// Path directly inside the sandbox is allowed — both an existing file
    /// and a not-yet-created one.
    #[test]
    fn sandbox_allows_paths_inside() {
        let tmp = TempDir::new().unwrap();
        let sandbox = tmp.path().join("agent-a");
        std::fs::create_dir_all(&sandbox).unwrap();

        let inside_existing = sandbox.join("existing.txt");
        std::fs::write(&inside_existing, b"").unwrap();
        assert!(path_in_sandbox(&sandbox, &inside_existing));

        let inside_new = sandbox.join("new/file.txt");
        assert!(
            path_in_sandbox(&sandbox, &inside_new),
            "not-yet-created file inside sandbox must be allowed"
        );
    }

    /// `..` escaping above the sandbox is rejected, even when the resolved
    /// path is itself a sibling that exists.
    #[test]
    fn sandbox_rejects_dotdot_escape() {
        let tmp = TempDir::new().unwrap();
        let sandbox = tmp.path().join("agent-a");
        std::fs::create_dir_all(&sandbox).unwrap();
        // sibling folder the agent is trying to reach
        let sibling = tmp.path().join("agent-b");
        std::fs::create_dir_all(&sibling).unwrap();

        let escape = sandbox.join("../agent-b/secret.txt");
        assert!(
            !path_in_sandbox(&sandbox, &escape),
            "../ escape must be rejected"
        );
    }

    /// Absolute path outside the sandbox is rejected.
    #[test]
    fn sandbox_rejects_absolute_outside() {
        let tmp = TempDir::new().unwrap();
        let sandbox = tmp.path().join("agent-a");
        std::fs::create_dir_all(&sandbox).unwrap();

        let outside = tmp.path().join("other/file.txt");
        assert!(
            !path_in_sandbox(&sandbox, &outside),
            "absolute path outside sandbox must be rejected"
        );
    }

    /// Extractor returns `None` when no recognizable path key is present —
    /// callers must treat that as "don't block".
    #[test]
    fn extract_path_missing_returns_none() {
        let body: serde_json::Value =
            serde_json::from_str(r#"{"tool_input":{"command":"echo hi"}}"#).unwrap();
        assert!(extract_path(&body).is_none());
    }

    /// Extractor picks up each of the four recognized keys.
    #[test]
    fn extract_path_recognizes_known_keys() {
        for (key, expected) in [
            ("file_path", "/tmp/a.txt"),
            ("path", "/tmp/b.txt"),
            ("notebook_path", "/tmp/c.ipynb"),
            ("cwd", "/tmp/d"),
        ] {
            let raw = format!(r#"{{"tool_input":{{"{key}":"{expected}"}}}}"#);
            let body: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let got = extract_path(&body).expect("extract_path must find the key");
            assert_eq!(got, PathBuf::from(expected));
        }
    }
}
