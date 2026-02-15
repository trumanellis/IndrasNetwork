//! Identity display utilities for member names, IDs, and colors.

use std::sync::Mutex;

/// Global registry of member IDs → sequential index.
/// First member encountered gets index 0 (→ "A"), second gets 1 (→ "B"), etc.
static MEMBER_REGISTRY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Get or assign a sequential index for a member ID.
fn member_index(member_id: &str) -> usize {
    // Short IDs (e.g. "A", "B", "C" from mesh builder): use ordinal position directly
    if !member_id.is_empty() && member_id.len() <= 2 {
        let first = member_id.as_bytes()[0];
        return match first {
            b'A'..=b'Z' => (first - b'A') as usize,
            b'a'..=b'z' => (first - b'a') as usize,
            b'0'..=b'9' => (first - b'0') as usize,
            _ => first as usize,
        };
    }

    // Long IDs (hex hashes): use registry for sequential assignment
    let mut registry = MEMBER_REGISTRY.lock().unwrap();
    if let Some(idx) = registry.iter().position(|id| id == member_id) {
        return idx;
    }
    let idx = registry.len();
    registry.push(member_id.to_string());
    idx
}

/// Convert member ID to a single-letter display name (A, B, C, ...).
/// Names are assigned sequentially as new member IDs are encountered.
pub fn member_name(member_id: &str) -> String {
    let idx = member_index(member_id) % 26;
    let letter = (b'A' + idx as u8) as char;
    letter.to_string()
}

/// Reset the member name registry (call between scenarios).
pub fn reset_member_names() {
    let mut registry = MEMBER_REGISTRY.lock().unwrap();
    registry.clear();
}

/// Shorten an ID for display.
pub fn short_id(id: &str) -> String {
    if id.len() > 8 {
        format!("{}...", &id[..8])
    } else {
        id.to_string()
    }
}

/// Format milliseconds as human-readable duration (e.g., "2h 30m").
pub fn format_duration_millis(millis: u64) -> String {
    let seconds = millis / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        let remaining_mins = minutes % 60;
        if remaining_mins > 0 {
            format!("{}h {}m", hours, remaining_mins)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}

/// Get the CSS class for a member's color.
pub fn member_color_class(member: &str) -> &'static str {
    const CLASSES: [&str; 16] = [
        "member-love", "member-joy", "member-peace", "member-grace",
        "member-hope", "member-faith", "member-light", "member-truth",
        "member-wisdom", "member-mercy", "member-valor", "member-honor",
        "member-glory", "member-spirit", "member-unity", "member-bliss",
    ];
    let idx = member_index(member) % CLASSES.len();
    CLASSES[idx]
}

/// Get the CSS variable for a member's color.
pub fn member_color_var(member: &str) -> &'static str {
    const VARS: [&str; 6] = [
        "var(--color-love)", "var(--color-joy)", "var(--color-peace)",
        "var(--color-grace)", "var(--color-hope)", "var(--color-faith)",
    ];
    let idx = member_index(member) % VARS.len();
    VARS[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_name_sequential() {
        reset_member_names();
        assert_eq!(member_name("abcd1234abcd1234"), "A");
        assert_eq!(member_name("ffff0000ffff0000"), "B");
        assert_eq!(member_name("12345678deadbeef"), "C");
        // Same ID returns same name
        assert_eq!(member_name("abcd1234abcd1234"), "A");
        reset_member_names();
    }

    #[test]
    fn test_member_name_short_ids() {
        assert_eq!(member_name("A"), "A");
        assert_eq!(member_name("B"), "B");
        assert_eq!(member_name("C"), "C");
    }

    #[test]
    fn test_short_id() {
        assert_eq!(short_id("abcdefghij"), "abcdefgh...");
        assert_eq!(short_id("short"), "short");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_millis(0), "0s");
        assert_eq!(format_duration_millis(30000), "30s");
        assert_eq!(format_duration_millis(90000), "1m");
        assert_eq!(format_duration_millis(5400000), "1h 30m");
        assert_eq!(format_duration_millis(7200000), "2h");
    }
}
