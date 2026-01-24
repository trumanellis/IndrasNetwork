//! Indras Notes - P2P Collaborative Note-Taking
//!
//! A reference application demonstrating Indras Network capabilities:
//! - CRDT-based sync (Automerge)
//! - Offline-first storage
//! - P2P networking (iroh)
//!
//! ## Usage
//!
//! ```bash
#![allow(dead_code)] // Example code with reserved features
//! # Initialize with your name
//! indras-notes init --name "Alice"
//!
//! # Create a new notebook
//! indras-notes create "My Notebook"
//!
//! # List notebooks
//! indras-notes list
//!
//! # Open a notebook (interactive mode)
//! indras-notes open <id>
//!
//! # Generate invite for others
//! indras-notes invite <notebook-id>
//!
//! # Join via invite
//! indras-notes join <invite>
//! ```

mod app;
mod display;
mod log_analysis;
mod log_capture;
mod lua;
mod note;
mod notebook;
mod storage;
mod syncable_notebook;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::sync::Mutex;

use app::{App, AppError};
use display::*;
use indras_core::InterfaceId;
use log_capture::LogCapture;
use lua::NotesLuaRuntime;

/// Indras Notes - P2P Collaborative Note-Taking
#[derive(Parser)]
#[command(name = "indras-notes")]
#[command(about = "P2P collaborative note-taking with Indras Network")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize with your identity
    Init {
        /// Your display name
        #[arg(short, long)]
        name: String,
    },
    /// Create a new notebook
    Create {
        /// Notebook name
        name: String,
    },
    /// List all notebooks
    List,
    /// Open a notebook in interactive mode
    Open {
        /// Notebook ID (full or partial)
        id: String,
    },
    /// Generate an invite link for a notebook
    Invite {
        /// Notebook ID (full or partial)
        id: String,
    },
    /// Join a notebook via invite
    Join {
        /// Invite link
        invite: String,
    },
    /// Show your identity
    Whoami,
    /// Execute a Lua script file
    RunScript {
        /// Path to the Lua script
        path: PathBuf,
    },
    /// Evaluate a Lua expression
    Eval {
        /// Lua code to evaluate
        code: String,
    },
    /// Start an interactive Lua REPL
    LuaRepl,
}

#[tokio::main]
async fn main() {
    // Initialize tracing - use JSONL format if NOTES_JSONL=1
    let use_jsonl = std::env::var("NOTES_JSONL")
        .map(|v| v == "1")
        .unwrap_or(false);

    if use_jsonl {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("indras_notes=info".parse().unwrap()),
            )
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("indras_notes=info".parse().unwrap()),
            )
            .init();
    }

    if let Err(e) = run().await {
        print_error(&e.to_string());
        std::process::exit(1);
    }
}

async fn run() -> Result<(), AppError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => cmd_init(&name).await,
        Commands::Create { name } => cmd_create(&name).await,
        Commands::List => cmd_list().await,
        Commands::Open { id } => cmd_open(&id).await,
        Commands::Invite { id } => cmd_invite(&id).await,
        Commands::Join { invite } => cmd_join(&invite).await,
        Commands::Whoami => cmd_whoami().await,
        Commands::RunScript { path } => cmd_run_script(&path).await,
        Commands::Eval { code } => cmd_eval(&code).await,
        Commands::LuaRepl => cmd_lua_repl().await,
    }
}

async fn cmd_init(name: &str) -> Result<(), AppError> {
    print_banner();

    let mut app = App::new().await?;

    if app.is_initialized().await {
        print_error("Already initialized. Your identity exists.");
        return Ok(());
    }

    app.init(name).await?;

    print_success(&format!("Initialized as '{}'", name));
    if let Some(short_id) = app.user_short_id() {
        print_info(&format!("Your ID: {}", short_id));
    }

    Ok(())
}

async fn cmd_create(name: &str) -> Result<(), AppError> {
    let mut app = App::new().await?;
    app.load().await?;

    let interface_id = app.create_notebook(name).await?;

    print_success(&format!("Created notebook '{}'", name));
    print_info(&format!(
        "ID: {}",
        hex::encode(&interface_id.as_bytes()[..8])
    ));

    // Generate invite
    app.open_notebook(&interface_id).await?;
    if let Ok(invite) = app.get_invite() {
        print_invite(&invite);
    }

    Ok(())
}

async fn cmd_list() -> Result<(), AppError> {
    let app = App::new().await?;

    if !app.is_initialized().await {
        return Err(AppError::NotInitialized);
    }

    let notebooks = app.list_notebooks().await?;
    print_notebook_list(&notebooks);

    Ok(())
}

async fn cmd_open(id: &str) -> Result<(), AppError> {
    let mut app = App::new().await?;
    app.load().await?;

    // Find notebook by partial ID
    let notebooks = app.list_notebooks().await?;
    let interface_id = find_notebook_by_id(&notebooks, id)?;

    app.open_notebook(&interface_id).await?;

    if let Some(notebook) = app.current_notebook() {
        print_notebook(notebook);
    }

    // Enter interactive mode
    interactive_mode(&mut app).await?;

    app.close_notebook().await?;
    Ok(())
}

async fn cmd_invite(id: &str) -> Result<(), AppError> {
    let mut app = App::new().await?;
    app.load().await?;

    // Find notebook by partial ID
    let notebooks = app.list_notebooks().await?;
    let interface_id = find_notebook_by_id(&notebooks, id)?;

    app.open_notebook(&interface_id).await?;

    let invite = app.get_invite()?;
    print_invite(&invite);

    Ok(())
}

async fn cmd_join(invite: &str) -> Result<(), AppError> {
    let mut app = App::new().await?;
    app.load().await?;

    let interface_id = app.join_notebook(invite).await?;

    print_success("Joined notebook successfully");
    print_info(&format!(
        "ID: {}",
        hex::encode(&interface_id.as_bytes()[..8])
    ));

    Ok(())
}

async fn cmd_whoami() -> Result<(), AppError> {
    let mut app = App::new().await?;
    app.load().await?;

    if let Some(name) = app.user_name() {
        println!("Name: {}", name);
    }
    if let Some(id) = app.user_short_id() {
        println!("ID: {}", id);
    }

    Ok(())
}

/// Find a notebook by partial ID match
fn find_notebook_by_id(
    notebooks: &[storage::NotebookMeta],
    partial_id: &str,
) -> Result<InterfaceId, AppError> {
    // Try exact match first
    for nb in notebooks {
        let hex_id = hex::encode(nb.interface_id.as_bytes());
        if hex_id == partial_id || hex_id.starts_with(partial_id) {
            return Ok(nb.interface_id);
        }
    }

    // Try by name
    for nb in notebooks {
        if nb.name.to_lowercase().contains(&partial_id.to_lowercase()) {
            return Ok(nb.interface_id);
        }
    }

    // Try by index
    if let Ok(index) = partial_id.parse::<usize>()
        && index > 0
        && index <= notebooks.len()
    {
        return Ok(notebooks[index - 1].interface_id);
    }

    Err(AppError::NotebookNotFound(partial_id.to_string()))
}

/// Interactive mode for editing notes
async fn interactive_mode(app: &mut App) -> Result<(), AppError> {
    print_interactive_help();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let notebook_name = app
            .current_notebook()
            .map(|n| n.name.as_str())
            .unwrap_or("notes");

        print_prompt(notebook_name);
        stdout.flush().unwrap();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "list" | "ls" => {
                if let Some(notebook) = app.current_notebook() {
                    let notes = notebook.list();
                    print_note_list(&notes);
                }
            }
            "new" | "create" => {
                let title = if args.is_empty() {
                    prompt_input("Title: ")?
                } else {
                    args.join(" ")
                };

                let id = app.create_note(&title).await?;
                print_success(&format!("Created note '{}' ({})", title, &id[..8]));
            }
            "view" | "show" => {
                if args.is_empty() {
                    print_error("Usage: view <note-id>");
                    continue;
                }

                if let Some(note) = app.find_note(args[0]) {
                    print_note(note);
                } else {
                    print_error(&format!("Note not found: {}", args[0]));
                }
            }
            "edit" => {
                if args.is_empty() {
                    print_error("Usage: edit <note-id>");
                    continue;
                }

                let note_id = if let Some(note) = app.find_note(args[0]) {
                    note.id.clone()
                } else {
                    print_error(&format!("Note not found: {}", args[0]));
                    continue;
                };

                println!("Enter new content (empty line to finish):");
                let content = read_multiline()?;

                app.update_note_content(&note_id, &content).await?;
                print_success("Note updated");
            }
            "delete" | "rm" => {
                if args.is_empty() {
                    print_error("Usage: delete <note-id>");
                    continue;
                }

                let note_id = if let Some(note) = app.find_note(args[0]) {
                    note.id.clone()
                } else {
                    print_error(&format!("Note not found: {}", args[0]));
                    continue;
                };

                app.delete_note(&note_id).await?;
                print_success("Note deleted");
            }
            "invite" => {
                if let Ok(invite) = app.get_invite() {
                    print_invite(&invite);
                }
            }
            "help" | "?" => {
                print_interactive_help();
            }
            "quit" | "exit" | "q" => {
                break;
            }
            _ => {
                print_error(&format!(
                    "Unknown command: {}. Type 'help' for commands.",
                    cmd
                ));
            }
        }
    }

    Ok(())
}

/// Prompt for single-line input
fn prompt_input(prompt: &str) -> Result<String, AppError> {
    print!("{}", prompt);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .lock()
        .read_line(&mut input)
        .map_err(|e| AppError::Storage(storage::StorageError::Io(e)))?;

    Ok(input.trim().to_string())
}

/// Read multiline input until empty line
fn read_multiline() -> Result<String, AppError> {
    let stdin = io::stdin();
    let mut lines = Vec::new();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| AppError::Storage(storage::StorageError::Io(e)))?;
        if line.is_empty() {
            break;
        }
        lines.push(line);
    }

    Ok(lines.join("\n"))
}

/// Run a Lua script file
async fn cmd_run_script(path: &PathBuf) -> Result<(), AppError> {
    let log_capture = LogCapture::new();
    let runtime = NotesLuaRuntime::with_log_capture(Some(log_capture))
        .map_err(|e| AppError::Lua(e.to_string()))?;

    // Optionally set up an app instance for the script
    // Scripts can also create their own via notes.App.new() or notes.App.new_with_temp_storage()
    if let Ok(app) = App::new().await {
        let app = Arc::new(Mutex::new(app));
        if let Ok(()) = runtime.set_app(app) {
            // App set successfully
        }
    }

    print_info(&format!("Running script: {}", path.display()));

    match runtime.exec_file(path) {
        Ok(()) => {
            print_success("Script completed successfully");
            Ok(())
        }
        Err(e) => {
            print_error(&format!("Script error: {}", e));
            Err(AppError::Lua(e.to_string()))
        }
    }
}

/// Evaluate a Lua expression
async fn cmd_eval(code: &str) -> Result<(), AppError> {
    let runtime = NotesLuaRuntime::new().map_err(|e| AppError::Lua(e.to_string()))?;

    // Set up app for eval
    if let Ok(app) = App::new().await {
        let app = Arc::new(Mutex::new(app));
        let _ = runtime.set_app(app);
    }

    match runtime.eval::<mlua::Value>(code) {
        Ok(value) => {
            // Print the result
            match value {
                mlua::Value::Nil => println!("nil"),
                mlua::Value::Boolean(b) => println!("{}", b),
                mlua::Value::Integer(i) => println!("{}", i),
                mlua::Value::Number(n) => println!("{}", n),
                mlua::Value::String(s) => {
                    if let Ok(str) = s.to_str() {
                        println!("{}", str);
                    } else {
                        println!("<invalid utf8>");
                    }
                }
                _ => println!("{:?}", value),
            }
            Ok(())
        }
        Err(e) => {
            print_error(&format!("Eval error: {}", e));
            Err(AppError::Lua(e.to_string()))
        }
    }
}

/// Start an interactive Lua REPL
async fn cmd_lua_repl() -> Result<(), AppError> {
    let log_capture = LogCapture::new();
    let runtime = NotesLuaRuntime::with_log_capture(Some(log_capture))
        .map_err(|e| AppError::Lua(e.to_string()))?;

    // Set up app for REPL
    if let Ok(app) = App::new().await {
        let app = Arc::new(Mutex::new(app));
        let _ = runtime.set_app(app);
    }

    println!("Indras Notes Lua REPL");
    println!("Type 'exit' or 'quit' to exit, 'help' for available functions");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("lua> ");
        stdout.flush().unwrap();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "exit" | "quit" => break,
            "help" => {
                println!("Available modules:");
                println!("  notes.App        - Application management");
                println!("  notes.Note       - Note type");
                println!("  notes.Notebook   - Notebook type");
                println!("  notes.NoteOperation - Note operations");
                println!("  notes.log        - Logging (trace, debug, info, warn, error)");
                println!("  notes.assert     - Assertions (eq, ne, gt, lt, etc.)");
                println!("  notes.log_assert - Log assertions (has_message, no_errors, etc.)");
                println!();
                println!("Example:");
                println!("  local app = notes.App.new_with_temp_storage()");
                println!("  app:init('Alice')");
                println!("  local nb_id = app:create_notebook('My Notes')");
                continue;
            }
            _ => {}
        }

        // Try to evaluate as expression first (prepend "return")
        let result = runtime.eval::<mlua::Value>(&format!("return {}", input));

        match result {
            Ok(value) => {
                match value {
                    mlua::Value::Nil => {} // Don't print nil for expressions
                    mlua::Value::Boolean(b) => println!("=> {}", b),
                    mlua::Value::Integer(i) => println!("=> {}", i),
                    mlua::Value::Number(n) => println!("=> {}", n),
                    mlua::Value::String(s) => {
                        if let Ok(str) = s.to_str() {
                            println!("=> \"{}\"", str);
                        } else {
                            println!("=> <invalid utf8>");
                        }
                    }
                    _ => println!("=> {:?}", value),
                }
            }
            Err(_) => {
                // If eval failed, try executing as statement
                if let Err(e) = runtime.exec(input) {
                    println!("Error: {}", e);
                }
            }
        }
    }

    println!("Goodbye!");
    Ok(())
}
