//! Display utilities for CLI output
//!
//! Provides colored, formatted output for notes and notebooks.

use colored::*;

use crate::note::Note;
use crate::notebook::Notebook;
use crate::storage::NotebookMeta;

/// Print a welcome banner
pub fn print_banner() {
    println!("{}", "╔══════════════════════════════════════╗".cyan());
    println!("{}", "║       Indras Notes                   ║".cyan());
    println!("{}", "║  P2P Collaborative Note-Taking       ║".cyan());
    println!("{}", "╚══════════════════════════════════════╝".cyan());
    println!();
}

/// Print success message
pub fn print_success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

/// Print error message
pub fn print_error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

/// Print info message
pub fn print_info(msg: &str) {
    println!("{} {}", "ℹ".blue(), msg);
}

/// Print a note in detail
pub fn print_note(note: &Note) {
    println!("{}", "─".repeat(50).dimmed());
    println!("{}: {}", "Title".bold(), note.title.yellow());
    println!("{}: {}", "ID".dimmed(), &note.id[..8]);
    println!("{}: {}", "Author".dimmed(), note.author);
    println!(
        "{}: {}",
        "Modified".dimmed(),
        note.modified_at.format("%Y-%m-%d %H:%M")
    );
    println!();
    if note.content.is_empty() {
        println!("{}", "(empty)".dimmed().italic());
    } else {
        println!("{}", note.content);
    }
    println!("{}", "─".repeat(50).dimmed());
}

/// Print a note in list format (compact)
pub fn print_note_list_item(note: &Note, index: usize) {
    let preview = note.preview(40);
    let preview_display = if preview.is_empty() {
        "(empty)".dimmed().italic().to_string()
    } else {
        preview.dimmed().to_string()
    };

    println!(
        "  {} {} {} - {}",
        format!("[{}]", index + 1).cyan(),
        note.title.yellow(),
        format!("({})", &note.id[..8]).dimmed(),
        preview_display
    );
}

/// Print a list of notes
pub fn print_note_list(notes: &[&Note]) {
    if notes.is_empty() {
        println!("{}", "No notes yet. Create one with 'note new'".dimmed());
        return;
    }

    println!("{}", format!("Notes ({}):", notes.len()).bold());
    for (i, note) in notes.iter().enumerate() {
        print_note_list_item(note, i);
    }
}

/// Print notebook info
pub fn print_notebook(notebook: &Notebook) {
    println!("{}", "═".repeat(50).cyan());
    println!("{}: {}", "Notebook".bold(), notebook.name.cyan().bold());
    println!(
        "{}: {}",
        "ID".dimmed(),
        hex::encode(&notebook.interface_id.as_bytes()[..8])
    );
    println!("{}: {}", "Notes".dimmed(), notebook.count());
    println!(
        "{}: {}",
        "Created".dimmed(),
        notebook.created_at.format("%Y-%m-%d")
    );
    println!("{}", "═".repeat(50).cyan());
}

/// Print a notebook in list format
pub fn print_notebook_list_item(meta: &NotebookMeta, index: usize) {
    println!(
        "  {} {} - {} notes ({})",
        format!("[{}]", index + 1).cyan(),
        meta.name.yellow(),
        meta.note_count,
        hex::encode(&meta.interface_id.as_bytes()[..4]).dimmed()
    );
}

/// Print a list of notebooks
pub fn print_notebook_list(notebooks: &[NotebookMeta]) {
    if notebooks.is_empty() {
        println!("{}", "No notebooks yet. Create one with 'create'".dimmed());
        return;
    }

    println!("{}", format!("Notebooks ({}):", notebooks.len()).bold());
    for (i, meta) in notebooks.iter().enumerate() {
        print_notebook_list_item(meta, i);
    }
}

/// Print an invite key for sharing
pub fn print_invite(invite_b64: &str) {
    println!();
    println!("{}", "Share this invite with others:".bold());
    println!();
    println!("  {}", invite_b64.green());
    println!();
    println!(
        "{}",
        "They can join with: indras-notes join <invite>".dimmed()
    );
}

/// Print interactive mode help
pub fn print_interactive_help() {
    println!();
    println!("{}", "Available commands:".bold());
    println!("  {}  - List all notes", "list".yellow());
    println!("  {}  - Create a new note", "new".yellow());
    println!("  {}  - View a note", "view <id>".yellow());
    println!("  {}  - Edit a note", "edit <id>".yellow());
    println!("  {}  - Delete a note", "delete <id>".yellow());
    println!("  {}  - Show invite link", "invite".yellow());
    println!("  {}  - Show this help", "help".yellow());
    println!("  {}  - Exit interactive mode", "quit".yellow());
    println!();
}

/// Print a prompt for input
pub fn print_prompt(notebook_name: &str) {
    print!("{} {} ", notebook_name.cyan(), ">".dimmed());
    use std::io::Write;
    std::io::stdout().flush().unwrap();
}
