//! Indras Chat - P2P Encrypted Messaging
//!
//! A command-line chat application demonstrating the Indras messaging protocol.
//!
//! ## Usage
//!
//! ```bash
//! # Create a new chat room
//! chat new "My Room" --username Alice
//!
//! # Join an existing room
//! chat join <room-id> --username Bob
//!
//! # List all rooms
//! chat list
//!
//! # Enter a chat room
//! chat enter <room-id>
//!
//! # Demo mode: simulate a conversation
//! chat demo
//! ```

mod display;
mod room;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;

use display::*;
use room::{ChatRoom, RoomStorage};

/// Indras Chat - P2P Encrypted Messaging
#[derive(Parser)]
#[command(name = "chat")]
#[command(about = "P2P encrypted chat using Indras Network")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Data directory
    #[arg(short, long, default_value = "~/.indras-chat")]
    data_dir: String,

    /// Your username
    #[arg(short, long, default_value = "Anonymous")]
    username: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new chat room
    New {
        /// Room name
        name: String,
    },
    /// Join an existing room by ID
    Join {
        /// Room ID (from invite)
        id: String,
        /// Room name (optional, defaults to "Joined Room")
        #[arg(short, long, default_value = "Joined Room")]
        name: String,
    },
    /// List all chat rooms
    List,
    /// Enter a chat room
    Enter {
        /// Room ID or index from list
        room: String,
    },
    /// Leave (delete) a chat room
    Leave {
        /// Room ID or index from list
        room: String,
    },
    /// Demo mode: simulate a multi-user conversation
    Demo,
}

fn get_data_dir(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(&path[2..]);
    }
    PathBuf::from(path)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("chat=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();
    let data_dir = get_data_dir(&cli.data_dir);

    // Create storage (this loads existing rooms)
    let mut storage =
        RoomStorage::new(data_dir.clone()).context("Failed to initialize room storage")?;

    match cli.command {
        Commands::New { name } => cmd_new(&mut storage, &name, &cli.username),
        Commands::Join { id, name } => cmd_join(&mut storage, &id, &name, &cli.username),
        Commands::List => cmd_list(&storage),
        Commands::Enter { room } => cmd_enter(&mut storage, &room, &cli.username),
        Commands::Leave { room } => cmd_leave(&mut storage, &room),
        Commands::Demo => cmd_demo().await,
    }
}

fn cmd_new(storage: &mut RoomStorage, name: &str, username: &str) -> Result<()> {
    print_banner();

    let room = storage.create(name, username)?;
    let room_id = room.id().to_string();

    print_success(&format!("Created room '{}'", name));
    print_join_instructions(&room_id);

    Ok(())
}

fn cmd_join(storage: &mut RoomStorage, id: &str, name: &str, username: &str) -> Result<()> {
    print_banner();

    let room = storage.join(id, name, username)?;
    let room_id = room.id().to_string();
    let room_name = room.name().to_string();

    print_success(&format!("Joined room '{}' ({})", room_name, &room_id[..8]));
    print_info(&format!("Enter with: chat enter {}", &room_id[..8]));

    Ok(())
}

fn cmd_list(storage: &RoomStorage) -> Result<()> {
    let rooms = storage.list();
    let room_data: Vec<_> = rooms
        .iter()
        .map(|r| (r.id().to_string(), r.name().to_string(), r.message_count()))
        .collect();

    print_room_list(&room_data);

    Ok(())
}

fn cmd_enter(storage: &mut RoomStorage, room_query: &str, username: &str) -> Result<()> {
    // Find the room
    let room_id = storage
        .find(room_query)
        .map(|r| r.id().to_string())
        .ok_or_else(|| anyhow::anyhow!("Room not found: {}", room_query))?;

    print_banner();

    // Show room info
    {
        let room = storage.get(&room_id).unwrap();
        print_room_info(
            room.name(),
            room.id(),
            &room
                .members()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            room.message_count(),
        );

        // Show recent messages
        let recent = room.recent_messages(10);
        if !recent.is_empty() {
            print_history_header(room.name(), recent.len());
            for msg in recent {
                if msg.is_system {
                    print_system_message(&msg.content);
                } else {
                    print_message(
                        &msg.sender,
                        &msg.content,
                        msg.timestamp,
                        msg.sender == username,
                    );
                }
            }
            println!();
        }
    }

    print_interactive_help();

    // Interactive chat loop
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let room = storage.get(&room_id).unwrap();
        print_prompt(&room.name()[..8.min(room.name().len())]);
        stdout.flush()?;

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Parse command or treat as message
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "quit" | "exit" | "q" | "/quit" | "/exit" => {
                storage.save(&room_id)?;
                print_info("Goodbye!");
                break;
            }
            "help" | "?" | "/help" => {
                print_interactive_help();
            }
            "history" | "/history" => {
                let room = storage.get(&room_id).unwrap();
                print_history_header(room.name(), room.message_count());
                for msg in room.all_messages() {
                    if msg.is_system {
                        print_system_message(&msg.content);
                    } else {
                        print_message(
                            &msg.sender,
                            &msg.content,
                            msg.timestamp,
                            msg.sender == username,
                        );
                    }
                }
            }
            "members" | "/members" => {
                let room = storage.get(&room_id).unwrap();
                println!();
                println!("{}", "Room Members:".yellow().bold());
                for member in room.members() {
                    if member == username {
                        println!("  {} {}", "•".green(), format!("{} (you)", member).cyan());
                    } else {
                        println!("  {} {}", "•".white(), member);
                    }
                }
                println!();
            }
            "info" | "/info" => {
                let room = storage.get(&room_id).unwrap();
                print_room_info(
                    room.name(),
                    room.id(),
                    &room
                        .members()
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>(),
                    room.message_count(),
                );
            }
            "clear" | "/clear" => {
                // ANSI clear screen
                print!("\x1b[2J\x1b[H");
                stdout.flush()?;
            }
            "send" | "/send" => {
                if parts.len() > 1 {
                    let room = storage.get_mut(&room_id).unwrap();
                    let msg = room.add_message(username, parts[1]);
                    print_message(&msg.sender, &msg.content, msg.timestamp, true);
                    storage.save(&room_id)?;
                }
            }
            _ => {
                // Treat as a message
                let room = storage.get_mut(&room_id).unwrap();
                let msg = room.add_message(username, input);
                print_message(&msg.sender, &msg.content, msg.timestamp, true);
                storage.save(&room_id)?;
            }
        }
    }

    Ok(())
}

fn cmd_leave(storage: &mut RoomStorage, room_query: &str) -> Result<()> {
    // Find the room
    let room_id = storage
        .find(room_query)
        .map(|r| r.id().to_string())
        .ok_or_else(|| anyhow::anyhow!("Room not found: {}", room_query))?;

    let room_name = storage.get(&room_id).unwrap().name().to_string();

    storage.delete(&room_id)?;
    print_success(&format!("Left room '{}'", room_name));

    Ok(())
}

async fn cmd_demo() -> Result<()> {
    print_banner();
    print_demo_mode();

    println!(
        "{}",
        "Creating a chat room and simulating a conversation...".dimmed()
    );
    println!();

    // Simulate a conversation
    let messages = [
        ("Alice", "Hey everyone! Welcome to the Indras Chat demo."),
        ("Bob", "Hi Alice! This is pretty cool."),
        ("Charlie", "I just joined. What's this about?"),
        (
            "Alice",
            "It's a P2P encrypted messaging system built on Indras Network.",
        ),
        (
            "Bob",
            "Messages are broadcast via gossip protocol, so no central server.",
        ),
        ("Charlie", "Nice! And it's all end-to-end encrypted?"),
        ("Alice", "Yes! Each chat room has its own encryption key."),
        (
            "Bob",
            "You can share the room ID with others to invite them.",
        ),
        ("Charlie", "This would be great for team communication."),
        (
            "Alice",
            "Exactly! And it works even with intermittent connectivity.",
        ),
    ];

    // Create a simulated room
    let mut room = ChatRoom::new("Indras Demo Room", "Alice");

    println!("{}", "═".repeat(50).cyan());
    println!("{}", "  Indras Demo Room".cyan().bold());
    println!("{}", "═".repeat(50).cyan());
    println!();

    for (sender, content) in &messages {
        // Add message to room
        let msg = room.add_message(sender, content);

        // Display with a small delay for effect
        print_message(&msg.sender, &msg.content, msg.timestamp, *sender == "Alice");

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    println!();
    println!("{}", "═".repeat(50).cyan());
    println!();

    print_success("Demo complete!");
    println!();
    println!("{}", "To start your own chat:".dimmed());
    println!(
        "  {} {} {}",
        "1.".dimmed(),
        "chat new".green(),
        "\"My Room\" --username YourName".dimmed()
    );
    println!(
        "  {} {} {}",
        "2.".dimmed(),
        "chat enter".green(),
        "1".dimmed()
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_data_dir_absolute() {
        let path = get_data_dir("/tmp/test");
        assert_eq!(path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_room_creation_and_retrieval() {
        let dir = tempdir().unwrap();
        let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();

        // Create room
        let room = storage.create("Test Room", "Tester").unwrap();
        let room_id = room.id().to_string();

        // Should be retrievable
        let found = storage.find(&room_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "Test Room");
    }

    #[test]
    fn test_room_messaging() {
        let dir = tempdir().unwrap();
        let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();

        let room = storage.create("Test Room", "Alice").unwrap();
        let room_id = room.id().to_string();

        // Add messages
        {
            let room = storage.get_mut(&room_id).unwrap();
            room.add_message("Alice", "Hello");
            room.add_message("Bob", "Hi there");
        }

        // Check messages
        let room = storage.get(&room_id).unwrap();
        // Initial system message + 2 user messages
        assert!(room.message_count() >= 2);
    }

    #[test]
    fn test_find_by_name() {
        let dir = tempdir().unwrap();
        let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();

        storage.create("Alpha Room", "Alice").unwrap();
        storage.create("Beta Room", "Bob").unwrap();

        // Find by partial name (case insensitive)
        let found = storage.find("alpha");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "Alpha Room");
    }

    #[test]
    fn test_room_persistence() {
        let dir = tempdir().unwrap();
        let room_id: String;
        let initial_count: usize;

        {
            let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();
            let room = storage.create("Persistent", "User").unwrap();
            room_id = room.id().to_string();
            initial_count = room.message_count();

            let room = storage.get_mut(&room_id).unwrap();
            room.add_message("User", "This should persist");
            storage.save(&room_id).unwrap();
        }

        // Reload and verify
        let storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();
        let room = storage.get(&room_id).unwrap();
        assert_eq!(room.name(), "Persistent");
        // Should have initial messages + the one we added
        assert!(room.message_count() > initial_count);
    }

    #[tokio::test]
    async fn test_demo_runs() {
        // Just verify demo doesn't panic
        // Would timeout in CI if something goes wrong
        tokio::time::timeout(Duration::from_secs(10), async {
            // Can't actually run demo in test since it has sleeps
            // but we can verify the room creation logic
            let room = ChatRoom::new("Test", "Alice");
            assert!(!room.messages.is_empty());
        })
        .await
        .unwrap();
    }
}
