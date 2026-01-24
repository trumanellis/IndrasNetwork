//! Indras Sync Demo - CRDT-based Document Synchronization
//!
//! A demo application showing how to use Automerge CRDTs for
//! collaborative document editing over Indras Network.
//!
//! ## Usage
//!
//! ```bash
//! # Create a new document
//! sync-demo new "My Document"
//!
//! # Open and edit a document
//! sync-demo open <id>
//!
//! # List all documents
//! sync-demo list
//!
//! # Sync two documents (demo mode)
//! sync-demo sync-pair
//!
//! # Show diff between document versions
//! sync-demo diff <id>
//! ```

mod display;
mod document;
mod sync;

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use indras_core::SimulationIdentity;

use display::*;
use document::{Document, DocumentMeta};
use sync::{fork_document, sync_documents};

/// Indras Sync Demo - Document Synchronization
#[derive(Parser)]
#[command(name = "sync-demo")]
#[command(about = "CRDT-based document synchronization demo")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Data directory
    #[arg(short, long, default_value = "~/.sync-demo")]
    data_dir: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new document
    New {
        /// Document title
        title: String,
        /// Author name
        #[arg(short, long, default_value = "Anonymous")]
        author: String,
    },
    /// List all documents
    List,
    /// Open a document in interactive mode
    Open {
        /// Document ID (full or partial)
        id: String,
    },
    /// Show diff between document versions
    Diff {
        /// Document ID
        id: String,
    },
    /// Demo: Sync two documents showing conflict resolution
    SyncPair,
    /// Export document to file
    Export {
        /// Document ID
        id: String,
        /// Output file
        output: PathBuf,
    },
    /// Import document from file
    Import {
        /// Input file
        input: PathBuf,
        /// Author name for imported doc
        #[arg(short, long, default_value = "Imported")]
        author: String,
    },
}

fn get_data_dir(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(&path[2..]);
    }
    PathBuf::from(path)
}

fn ensure_data_dir(data_dir: &PathBuf) -> Result<()> {
    fs::create_dir_all(data_dir)?;
    fs::create_dir_all(data_dir.join("documents"))?;
    Ok(())
}

fn list_documents(data_dir: &PathBuf) -> Result<Vec<DocumentMeta>> {
    let docs_dir = data_dir.join("documents");
    let mut docs = Vec::new();

    if !docs_dir.exists() {
        return Ok(docs);
    }

    for entry in fs::read_dir(&docs_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|e| e == "json") {
            let meta_content = fs::read_to_string(&path)?;
            if let Ok(meta) = serde_json::from_str::<DocumentMeta>(&meta_content) {
                docs.push(meta);
            }
        }
    }

    docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(docs)
}

fn load_document(data_dir: &PathBuf, id: &str) -> Result<Document> {
    let docs_dir = data_dir.join("documents");

    // Load metadata
    let meta_path = docs_dir.join(format!("{}.json", id));
    let meta_content =
        fs::read_to_string(&meta_path).context(format!("Document metadata not found: {}", id))?;
    let meta: DocumentMeta = serde_json::from_str(&meta_content)?;

    // Load document data
    let data_path = docs_dir.join(format!("{}.am", id));
    let data = fs::read(&data_path).context(format!("Document data not found: {}", id))?;

    // Create peer identity from author's first char (uppercase)
    let peer_char = meta
        .author
        .chars()
        .next()
        .unwrap_or('A')
        .to_ascii_uppercase();
    let local_peer =
        SimulationIdentity::new(peer_char).unwrap_or_else(|| SimulationIdentity::new('A').unwrap());

    Document::from_bytes(&data, meta, local_peer).context("Failed to load document")
}

fn save_document(data_dir: &PathBuf, doc: &mut Document) -> Result<()> {
    let docs_dir = data_dir.join("documents");
    fs::create_dir_all(&docs_dir)?;

    // Save metadata
    let meta_path = docs_dir.join(format!("{}.json", doc.id()));
    fs::write(&meta_path, serde_json::to_string_pretty(&doc.meta)?)?;

    // Save document data
    let data_path = docs_dir.join(format!("{}.am", doc.id()));
    fs::write(&data_path, doc.to_bytes())?;

    Ok(())
}

fn find_document_by_partial_id(docs: &[DocumentMeta], partial: &str) -> Result<String> {
    // Try by index
    if let Ok(index) = partial.parse::<usize>()
        && index > 0
        && index <= docs.len()
    {
        return Ok(docs[index - 1].id.clone());
    }

    // Try by partial ID
    for doc in docs {
        if doc.id.starts_with(partial) || doc.id == partial {
            return Ok(doc.id.clone());
        }
    }

    // Try by title
    for doc in docs {
        if doc.title.to_lowercase().contains(&partial.to_lowercase()) {
            return Ok(doc.id.clone());
        }
    }

    anyhow::bail!("Document not found: {}", partial)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("sync_demo=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();
    let data_dir = get_data_dir(&cli.data_dir);
    ensure_data_dir(&data_dir)?;

    match cli.command {
        Commands::New { title, author } => cmd_new(&data_dir, &title, &author),
        Commands::List => cmd_list(&data_dir),
        Commands::Open { id } => cmd_open(&data_dir, &id),
        Commands::Diff { id } => cmd_diff(&data_dir, &id),
        Commands::SyncPair => cmd_sync_pair(),
        Commands::Export { id, output } => cmd_export(&data_dir, &id, &output),
        Commands::Import { input, author } => cmd_import(&data_dir, &input, &author),
    }
}

fn cmd_new(data_dir: &PathBuf, title: &str, author: &str) -> Result<()> {
    print_banner();

    let mut doc = Document::new(title, author);
    save_document(data_dir, &mut doc)?;

    print_success(&format!("Created document '{}'", title));
    print_info(&format!("ID: {}", &doc.id()[..8]));
    print_info(&format!("Author: {}", author));

    Ok(())
}

fn cmd_list(data_dir: &PathBuf) -> Result<()> {
    let docs = list_documents(data_dir)?;
    print_document_list(&docs);
    Ok(())
}

fn cmd_open(data_dir: &PathBuf, id: &str) -> Result<()> {
    let docs = list_documents(data_dir)?;
    let full_id = find_document_by_partial_id(&docs, id)?;
    let mut doc = load_document(data_dir, &full_id)?;

    print_document(&doc);
    print_interactive_help();

    // Interactive mode
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print_prompt(&doc.title()[..8.min(doc.title().len())]);
        stdout.flush()?;

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
            "view" | "show" => {
                print_document(&doc);
            }
            "edit" => {
                println!("Enter new content (empty line to finish):");
                let content = read_multiline()?;
                if !content.is_empty() {
                    doc.set_content(&content)?;
                    print_success("Content updated");
                }
            }
            "append" => {
                println!("Enter text to append (empty line to finish):");
                let text = read_multiline()?;
                if !text.is_empty() {
                    doc.append_content(&format!("\n{}", text))?;
                    print_success("Content appended");
                }
            }
            "title" => {
                if args.is_empty() {
                    println!("Current title: {}", doc.title());
                } else {
                    let new_title = args.join(" ");
                    doc.set_title(&new_title)?;
                    print_success(&format!("Title changed to '{}'", new_title));
                }
            }
            "save" => {
                save_document(data_dir, &mut doc)?;
                print_success("Document saved");
            }
            "load" | "reload" => {
                doc = load_document(data_dir, &full_id)?;
                print_success("Document reloaded from disk");
            }
            "diff" => {
                if let Ok(disk_doc) = load_document(data_dir, &full_id) {
                    print_diff(disk_doc.content(), doc.content());
                } else {
                    print_info("No saved version to compare against");
                }
            }
            "sync" => {
                // Demo sync with a copy (simulating a peer)
                let mut doc_copy = fork_document(&mut doc, 'B')?;
                doc_copy.append_content("\n[Synced from peer]")?;

                let result = sync_documents(&mut doc, &mut doc_copy)?;
                print_sync_status(result.rounds, result.a_updated);

                if result.a_updated {
                    print_info("Document updated from peer");
                }
            }
            "export" => {
                if args.is_empty() {
                    print_error("Usage: export <filename>");
                    continue;
                }
                let path = PathBuf::from(args[0]);
                fs::write(&path, doc.content())?;
                print_success(&format!("Exported to {}", path.display()));
            }
            "help" | "?" => {
                print_interactive_help();
            }
            "quit" | "exit" | "q" => {
                // Auto-save on exit
                save_document(data_dir, &mut doc)?;
                print_info("Document auto-saved");
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

fn cmd_diff(data_dir: &PathBuf, id: &str) -> Result<()> {
    let docs = list_documents(data_dir)?;
    let full_id = find_document_by_partial_id(&docs, id)?;
    let doc = load_document(data_dir, &full_id)?;

    // For demo, show diff with empty string (shows all content as "added")
    print_diff("", doc.content());
    Ok(())
}

fn cmd_sync_pair() -> Result<()> {
    print_banner();
    println!(
        "{}",
        "Demonstrating CRDT sync between two documents...".cyan()
    );
    println!();

    // Create two documents
    let mut doc_a = Document::new("Shared Doc", "Alice");
    doc_a.set_content("Initial content from Alice")?;

    // Clone to create doc_b (as if received over network)
    let mut doc_b = fork_document(&mut doc_a, 'B')?;

    println!("{}", "Initial state:".yellow().bold());
    println!("  Alice: {}", doc_a.content().dimmed());
    println!("  Bob:   {}", doc_b.content().dimmed());
    println!();

    // Alice makes changes
    doc_a.set_content("Alice's updated content")?;
    println!("{}", "Alice makes changes:".yellow().bold());
    println!("  Alice: {}", doc_a.content().green());
    println!("  Bob:   {}", doc_b.content().dimmed());
    println!();

    // Bob makes concurrent changes
    doc_b.set_content("Bob's different content")?;
    println!("{}", "Bob makes concurrent changes:".yellow().bold());
    println!("  Alice: {}", doc_a.content().dimmed());
    println!("  Bob:   {}", doc_b.content().green());
    println!();

    // Sync
    println!("{}", "Syncing...".cyan().bold());
    let result = sync_documents(&mut doc_a, &mut doc_b)?;

    println!();
    println!("{}", "After sync:".yellow().bold());
    println!("  Alice: {}", doc_a.content().green());
    println!("  Bob:   {}", doc_b.content().green());
    println!();

    print_sync_status(result.rounds, result.a_updated || result.b_updated);

    if doc_a.content() == doc_b.content() {
        print_success("Documents are now identical (CRDT convergence)");
    } else {
        print_warning("Documents differ (unexpected)");
    }

    Ok(())
}

fn cmd_export(data_dir: &PathBuf, id: &str, output: &PathBuf) -> Result<()> {
    let docs = list_documents(data_dir)?;
    let full_id = find_document_by_partial_id(&docs, id)?;
    let doc = load_document(data_dir, &full_id)?;

    fs::write(output, doc.content())?;
    print_success(&format!(
        "Exported '{}' to {}",
        doc.title(),
        output.display()
    ));

    Ok(())
}

fn cmd_import(data_dir: &PathBuf, input: &PathBuf, author: &str) -> Result<()> {
    let content = fs::read_to_string(input)?;
    let title = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Document");

    let mut doc = Document::new(title, author);
    doc.set_content(&content)?;
    save_document(data_dir, &mut doc)?;

    print_success(&format!("Imported as '{}' ({})", title, &doc.id()[..8]));

    Ok(())
}

/// Read multiline input until empty line
fn read_multiline() -> Result<String> {
    let stdin = io::stdin();
    let mut lines = Vec::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        lines.push(line);
    }

    Ok(lines.join("\n"))
}
