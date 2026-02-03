//! Identity display utilities for member names, IDs, and colors.

/// Convert member ID to human-readable name.
pub fn member_name(member_id: &str) -> String {
    let names = [
        "Love", "Joy", "Peace", "Grace", "Hope", "Faith", "Light", "Truth",
        "Wisdom", "Mercy", "Valor", "Honor", "Glory", "Spirit", "Unity", "Bliss",
    ];

    if member_id.len() >= 4 {
        if let Ok(n) = u16::from_str_radix(&member_id[..4], 16) {
            let idx = (n as usize) % names.len();
            return names[idx].to_string();
        }
    }

    // Short IDs (e.g. "A", "B", "C" from mesh builder): use ordinal position
    if !member_id.is_empty() && member_id.len() < 4 {
        let first = member_id.as_bytes()[0];
        let idx = match first {
            b'A'..=b'Z' => (first - b'A') as usize,
            b'a'..=b'z' => (first - b'a') as usize,
            b'0'..=b'9' => (first - b'0') as usize,
            _ => first as usize,
        };
        return names[idx % names.len()].to_string();
    }

    // Fallback: use first few chars
    if member_id.len() > 8 {
        format!("{}...", &member_id[..8])
    } else {
        member_id.to_string()
    }
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
    let name = member_name(member).to_lowercase();
    match name.as_str() {
        "love" => "member-love",
        "joy" => "member-joy",
        "peace" => "member-peace",
        "grace" => "member-grace",
        "hope" => "member-hope",
        "faith" => "member-faith",
        "light" => "member-light",
        "truth" => "member-truth",
        "wisdom" => "member-wisdom",
        "mercy" => "member-mercy",
        "valor" => "member-valor",
        "honor" => "member-honor",
        "glory" => "member-glory",
        "spirit" => "member-spirit",
        "unity" => "member-unity",
        "bliss" => "member-bliss",
        _ => "member-default",
    }
}

/// Get the CSS variable for a member's color.
pub fn member_color_var(member: &str) -> &'static str {
    let name = member_name(member).to_lowercase();
    match name.as_str() {
        "love" => "var(--color-love)",
        "joy" => "var(--color-joy)",
        "peace" => "var(--color-peace)",
        "grace" => "var(--color-grace)",
        "hope" => "var(--color-hope)",
        "faith" => "var(--color-faith)",
        _ => "var(--accent-primary)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_name() {
        assert!(!member_name("abcd1234").is_empty());
        assert!(!member_name("0000ffff").is_empty());
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
