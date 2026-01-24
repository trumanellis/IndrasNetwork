//! Terminal display utilities for the chat app
#![allow(dead_code)] // Example code with reserved display functions

use chrono::{DateTime, Local, Utc};
use colored::Colorize;

/// Print the application banner
pub fn print_banner() {
    println!();
    println!(
        "{}",
        "╔═══════════════════════════════════════════════════╗".cyan()
    );
    println!(
        "{}",
        "║      Indras Chat - P2P Encrypted Messaging        ║".cyan()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════════════════╝".cyan()
    );
    println!();
}

/// Print a success message
pub fn print_success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg.green());
}

/// Print an info message
pub fn print_info(msg: &str) {
    println!("{} {}", "ℹ".blue(), msg.dimmed());
}

/// Print an error message
pub fn print_error(msg: &str) {
    println!("{} {}", "✗".red().bold(), msg.red());
}

/// Print a warning message
pub fn print_warning(msg: &str) {
    println!("{} {}", "⚠".yellow(), msg.yellow());
}

/// Print the chat prompt
pub fn print_prompt(room_name: &str) {
    print!("{} {} ", format!("[{}]", room_name).cyan(), ">".green());
}

/// Print interactive mode help
pub fn print_interactive_help() {
    println!();
    println!("{}", "Commands:".yellow().bold());
    println!("  {}      - Send a text message", "send <message>".cyan());
    println!(
        "  {}  - Direct message shorthand (just type)",
        "<message>".cyan()
    );
    println!("  {}         - Show message history", "history".cyan());
    println!("  {}          - Show room members", "members".cyan());
    println!("  {}           - Show room info", "info".cyan());
    println!("  {}          - Clear terminal", "clear".cyan());
    println!("  {}           - Show this help", "help".cyan());
    println!("  {}           - Leave room and exit", "quit".cyan());
    println!();
}

/// Print a chat message
pub fn print_message(sender: &str, content: &str, timestamp: DateTime<Utc>, is_self: bool) {
    let local_time: DateTime<Local> = timestamp.into();
    let time_str = local_time.format("%H:%M").to_string();

    if is_self {
        println!(
            "{} {} {}",
            time_str.dimmed(),
            format!("{}:", sender).cyan().bold(),
            content
        );
    } else {
        println!(
            "{} {} {}",
            time_str.dimmed(),
            format!("{}:", sender).magenta().bold(),
            content
        );
    }
}

/// Print a system message
pub fn print_system_message(msg: &str) {
    println!("{}", format!("    *** {} ***", msg).yellow().dimmed());
}

/// Print room list
pub fn print_room_list(rooms: &[(String, String, usize)]) {
    if rooms.is_empty() {
        println!(
            "{}",
            "No chat rooms. Use 'new <name>' to create one.".dimmed()
        );
        return;
    }

    println!();
    println!("{}", "Chat Rooms:".yellow().bold());
    println!("{}", "───────────────────────────────────────".dimmed());

    for (i, (id, name, msg_count)) in rooms.iter().enumerate() {
        println!(
            "  {} {} {} {}",
            format!("{}.", i + 1).dimmed(),
            name.cyan().bold(),
            format!("({} messages)", msg_count).dimmed(),
            format!("[{}]", &id[..8]).dimmed()
        );
    }
    println!();
}

/// Print room info
pub fn print_room_info(name: &str, id: &str, members: &[String], message_count: usize) {
    println!();
    println!("{}", "Room Information:".yellow().bold());
    println!("{}", "───────────────────────────────────────".dimmed());
    println!("  {} {}", "Name:".cyan(), name);
    println!("  {} {}", "ID:".cyan(), id);
    println!("  {} {}", "Messages:".cyan(), message_count);
    println!("  {} {}", "Members:".cyan(), members.join(", "));
    println!();
}

/// Print message history
pub fn print_history_header(room_name: &str, count: usize) {
    println!();
    println!(
        "{} {} {}",
        "─".repeat(10).dimmed(),
        format!("{} ({} messages)", room_name, count)
            .yellow()
            .bold(),
        "─".repeat(10).dimmed()
    );
}

/// Print demo mode banner
pub fn print_demo_mode() {
    println!();
    println!(
        "{}",
        "════════════════════════════════════════════════════".yellow()
    );
    println!(
        "{}",
        "  Running in DEMO mode - simulating peer messages   ".yellow()
    );
    println!(
        "{}",
        "════════════════════════════════════════════════════".yellow()
    );
    println!();
}

/// Print join instructions
pub fn print_join_instructions(room_id: &str) {
    println!();
    println!(
        "{}",
        "To invite others to this room, share this ID:".dimmed()
    );
    println!("  {}", room_id.cyan().bold());
    println!();
    println!("{}", "They can join with:".dimmed());
    println!("  {} {}", "chat join".green(), room_id);
    println!();
}
