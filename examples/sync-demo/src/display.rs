//! Display utilities for the sync demo
//!
//! Provides colored output and diff visualization.

use colored::Colorize;

use crate::document::{Document, DocumentMeta};

/// Print a banner
pub fn print_banner() {
    println!("{}", "=== Indras Sync Demo ===".cyan().bold());
    println!("{}", "CRDT-based document synchronization".dimmed());
    println!();
}

/// Print a success message
pub fn print_success(msg: &str) {
    println!("{} {}", "[OK]".green().bold(), msg);
}

/// Print an error message
pub fn print_error(msg: &str) {
    println!("{} {}", "[ERROR]".red().bold(), msg);
}

/// Print an info message
pub fn print_info(msg: &str) {
    println!("{} {}", "[INFO]".blue().bold(), msg);
}

/// Print a warning message
pub fn print_warning(msg: &str) {
    println!("{} {}", "[WARN]".yellow().bold(), msg);
}

/// Print a document
pub fn print_document(doc: &Document) {
    println!("{}", format!("=== {} ===", doc.title()).cyan().bold());
    println!("{}: {}", "ID".dimmed(), &doc.id()[..8]);
    println!("{}: {}", "Author".dimmed(), doc.author());
    println!(
        "{}: {}",
        "Updated".dimmed(),
        doc.updated_at().format("%Y-%m-%d %H:%M:%S")
    );
    println!("{}", "---".dimmed());
    println!("{}", doc.content());
    println!("{}", "---".dimmed());
}

/// Print document list
pub fn print_document_list(docs: &[DocumentMeta]) {
    if docs.is_empty() {
        println!("{}", "No documents found.".dimmed());
        return;
    }

    println!("{}", format!("{} document(s):", docs.len()).cyan().bold());
    println!();

    for (i, doc) in docs.iter().enumerate() {
        let short_id = &doc.id[..8.min(doc.id.len())];
        println!(
            "  {} {} {}",
            format!("[{}]", i + 1).yellow(),
            doc.title.white().bold(),
            format!("({})", short_id).dimmed()
        );
        println!(
            "      {}: {} | {}: {}",
            "Author".dimmed(),
            doc.author,
            "Updated".dimmed(),
            doc.updated_at.format("%Y-%m-%d %H:%M")
        );
    }
    println!();
}

/// Print a simple text diff
pub fn print_diff(old: &str, new: &str) {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    println!("{}", "=== Diff ===".cyan().bold());

    let max_lines = old_lines.len().max(new_lines.len());

    for i in 0..max_lines {
        let old_line = old_lines.get(i).copied().unwrap_or("");
        let new_line = new_lines.get(i).copied().unwrap_or("");

        if old_line != new_line {
            if !old_line.is_empty() {
                println!("{} {}", "-".red(), old_line.red());
            }
            if !new_line.is_empty() {
                println!("{} {}", "+".green(), new_line.green());
            }
        } else {
            println!("  {}", old_line);
        }
    }

    println!("{}", "============".dimmed());
}

/// Print sync status
pub fn print_sync_status(rounds: u32, doc_updated: bool) {
    if doc_updated {
        println!("{} Synced in {} round(s)", "[SYNC]".green().bold(), rounds);
    } else {
        println!("{} Already in sync", "[SYNC]".blue().bold());
    }
}

/// Print interactive help
pub fn print_interactive_help() {
    println!("{}", "Commands:".cyan().bold());
    println!("  {} - Show this document", "view".yellow());
    println!("  {} - Edit content", "edit".yellow());
    println!("  {} - Set title", "title <text>".yellow());
    println!(
        "  {} - Simulate sync with another instance",
        "sync".yellow()
    );
    println!("  {} - Show diff between local and synced", "diff".yellow());
    println!("  {} - Save to disk", "save".yellow());
    println!("  {} - Load from disk", "load".yellow());
    println!("  {} - Show this help", "help".yellow());
    println!("  {} - Exit", "quit".yellow());
    println!();
}

/// Print a prompt
pub fn print_prompt(name: &str) {
    print!("{} ", format!("[{}]>", name).cyan());
}
