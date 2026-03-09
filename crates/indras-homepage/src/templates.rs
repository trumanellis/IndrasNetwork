//! HTML template rendering for the homepage

use crate::profile::Profile;

/// Render the profile homepage as HTML
pub fn render_profile(profile: &Profile) -> String {
    let bio_section = profile.bio.as_deref().map(|bio| {
        format!(r#"<p class="bio">{}</p>"#, html_escape(bio))
    }).unwrap_or_default();

    let short_key = if profile.public_key.len() > 16 {
        format!("{}…{}", &profile.public_key[..8], &profile.public_key[profile.public_key.len()-8..])
    } else {
        profile.public_key.clone()
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{name} — IndrasNetwork</title>
<link href="https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;400;500;600;700&family=JetBrains+Mono:wght@300;400;500&display=swap" rel="stylesheet">
<style>
:root {{
  --bg-void: #050507;
  --bg-deep: #0a0a0e;
  --bg-surface: #111116;
  --bg-raised: #18181f;
  --text-primary: #e8e4d9;
  --text-secondary: #8a8a96;
  --text-muted: #55555f;
  --accent-teal: #00d4aa;
  --accent-violet: #b19cd9;
  --border-subtle: #1a1a22;
  --font-body: 'DM Sans', -apple-system, sans-serif;
  --font-mono: 'JetBrains Mono', monospace;
}}
*, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
html {{ height: 100%; }}
body {{
  font-family: var(--font-body);
  background: var(--bg-void);
  color: var(--text-primary);
  min-height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  -webkit-font-smoothing: antialiased;
  padding: 24px;
}}
.card {{
  background: var(--bg-deep);
  border: 1px solid var(--border-subtle);
  border-radius: 20px;
  padding: 48px;
  max-width: 480px;
  width: 100%;
  text-align: center;
}}
.avatar {{
  width: 80px;
  height: 80px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--accent-teal), var(--accent-violet));
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 32px;
  font-weight: 600;
  color: var(--bg-void);
  margin: 0 auto 24px;
}}
h1 {{
  font-size: 28px;
  font-weight: 600;
  margin-bottom: 4px;
  letter-spacing: -0.02em;
}}
.username {{
  font-family: var(--font-mono);
  font-size: 14px;
  color: var(--text-muted);
  margin-bottom: 20px;
}}
.bio {{
  font-size: 16px;
  color: var(--text-secondary);
  line-height: 1.6;
  margin-bottom: 24px;
}}
.key {{
  display: inline-flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: 8px;
  padding: 8px 14px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-muted);
}}
.key-dot {{
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent-teal);
  flex-shrink: 0;
}}
.footer {{
  margin-top: 32px;
  font-size: 12px;
  color: var(--text-muted);
}}
.footer a {{
  color: var(--accent-teal);
  text-decoration: none;
}}
.footer a:hover {{
  text-decoration: underline;
}}
@media (max-width: 520px) {{
  .card {{ padding: 32px 24px; }}
  h1 {{ font-size: 24px; }}
}}
</style>
</head>
<body>
<div class="card">
  <div class="avatar">{initial}</div>
  <h1>{name}</h1>
  <p class="username">@{username}</p>
  {bio}
  <div class="key">
    <span class="key-dot"></span>
    <span>{short_key}</span>
  </div>
  <div class="footer">
    Powered by <a href="https://github.com/indras-network/indras-network">IndrasNetwork</a>
  </div>
</div>
</body>
</html>"#,
        name = html_escape(&profile.display_name),
        initial = profile.display_name.chars().next().unwrap_or('?'),
        username = html_escape(&profile.username),
        bio = bio_section,
        short_key = short_key,
    )
}

/// Render a JSON health check response
pub fn render_health() -> String {
    r#"{"status":"ok"}"#.to_string()
}

/// Basic HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;

    #[test]
    fn render_profile_contains_name() {
        let profile = Profile::new("Alice", "alice", "abcdef1234567890abcdef1234567890");
        let html = render_profile(&profile);
        assert!(html.contains("Alice"));
        assert!(html.contains("@alice"));
        assert!(html.contains("abcdef12"));
    }

    #[test]
    fn render_profile_with_bio() {
        let profile = Profile::new("Bob", "bob", "1234567890abcdef1234567890abcdef")
            .with_bio("P2P enthusiast");
        let html = render_profile(&profile);
        assert!(html.contains("P2P enthusiast"));
    }

    #[test]
    fn render_profile_escapes_html() {
        let profile = Profile::new("<script>alert('xss')</script>", "hacker", "key123key123key123");
        let html = render_profile(&profile);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn health_check_is_valid_json() {
        let health = render_health();
        assert_eq!(health, r#"{"status":"ok"}"#);
    }
}
