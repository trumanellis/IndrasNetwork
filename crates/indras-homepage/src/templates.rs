//! HTML template rendering for the homepage

use crate::profile::{Profile, ViewLevel};

/// Render the profile homepage as HTML with visibility-gated sections.
pub fn render_profile(profile: &Profile, viewer: ViewLevel) -> String {
    let name = profile.display_name.for_viewer(&viewer)
        .map(|s| html_escape(s))
        .unwrap_or_default();
    let initial = profile.display_name.for_viewer(&viewer)
        .and_then(|s| s.chars().next())
        .unwrap_or('?');
    let username = profile.username.for_viewer(&viewer)
        .map(|s| html_escape(s))
        .unwrap_or_default();

    let bio_section = profile.bio.for_viewer(&viewer)
        .and_then(|opt| opt.as_deref())
        .map(|bio| format!(r#"<p class="bio">{}</p>"#, html_escape(bio)))
        .unwrap_or_default();

    let key_section = profile.public_key.for_viewer(&viewer)
        .map(|key| {
            let short_key = if key.len() > 16 {
                format!("{}…{}", &key[..8], &key[key.len()-8..])
            } else {
                key.clone()
            };
            format!(r#"<div class="key"><span class="key-dot"></span><span>{short_key}</span></div>"#)
        })
        .unwrap_or_default();

    let stats_section = render_stats(profile, &viewer);
    let social_section = render_social(profile, &viewer);
    let content_section = render_content(profile, &viewer);

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
  max-width: 600px;
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
.section {{
  margin-top: 28px;
  text-align: left;
}}
.section-title {{
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-muted);
  margin-bottom: 12px;
}}
.stats-grid {{
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}}
.stat-card {{
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: 12px;
  padding: 16px;
}}
.stat-value {{
  font-size: 24px;
  font-weight: 600;
  color: var(--accent-teal);
}}
.stat-label {{
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 4px;
}}
.social-row {{
  display: flex;
  gap: 16px;
  align-items: center;
}}
.social-item {{
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: 12px;
  padding: 16px;
  flex: 1;
}}
.social-value {{
  font-size: 20px;
  font-weight: 600;
  color: var(--text-primary);
}}
.social-label {{
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 4px;
}}
.freshness-bar {{
  height: 4px;
  background: var(--bg-raised);
  border-radius: 2px;
  margin-top: 8px;
  overflow: hidden;
}}
.freshness-fill {{
  height: 100%;
  background: linear-gradient(90deg, var(--accent-teal), var(--accent-violet));
  border-radius: 2px;
  transition: width 0.3s;
}}
.intention-list {{
  list-style: none;
}}
.intention-item {{
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: 10px;
  padding: 12px 16px;
  margin-bottom: 8px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}}
.intention-title {{
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}}
.intention-badge {{
  font-size: 11px;
  font-family: var(--font-mono);
  padding: 2px 8px;
  border-radius: 4px;
  background: var(--bg-raised);
  color: var(--accent-violet);
}}
.footer {{
  margin-top: 32px;
  font-size: 12px;
  color: var(--text-muted);
  text-align: center;
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
  .stats-grid {{ grid-template-columns: 1fr; }}
}}
</style>
</head>
<body>
<div class="card">
  <div class="avatar">{initial}</div>
  <h1>{name}</h1>
  <p class="username">@{username}</p>
  {bio}
  {key}
  {stats}
  {social}
  {content}
  <div class="footer">
    Powered by <a href="https://github.com/indras-network/indras-network">IndrasNetwork</a>
  </div>
</div>
</body>
</html>"#,
        name = name,
        initial = initial,
        username = username,
        bio = bio_section,
        key = key_section,
        stats = stats_section,
        social = social_section,
        content = content_section,
    )
}

/// Render the activity stats grid.
fn render_stats(profile: &Profile, viewer: &ViewLevel) -> String {
    let mut cards = Vec::new();

    if let Some(&count) = profile.intention_count.for_viewer(viewer) {
        cards.push(format!(
            r#"<div class="stat-card"><div class="stat-value">{count}</div><div class="stat-label">Intentions</div></div>"#
        ));
    }
    if let Some(&count) = profile.token_count.for_viewer(viewer) {
        cards.push(format!(
            r#"<div class="stat-card"><div class="stat-value">{count}</div><div class="stat-label">Tokens</div></div>"#
        ));
    }
    if let Some(&count) = profile.blessings_given.for_viewer(viewer) {
        cards.push(format!(
            r#"<div class="stat-card"><div class="stat-value">{count}</div><div class="stat-label">Blessings Given</div></div>"#
        ));
    }
    if let Some(time) = profile.attention_contributed.for_viewer(viewer) {
        if !time.is_empty() {
            cards.push(format!(
                r#"<div class="stat-card"><div class="stat-value">{}</div><div class="stat-label">Attention</div></div>"#,
                html_escape(time)
            ));
        }
    }

    if cards.is_empty() {
        return String::new();
    }

    format!(
        r#"<div class="section"><div class="section-title">Activity</div><div class="stats-grid">{}</div></div>"#,
        cards.join("\n    ")
    )
}

/// Render the social section (contacts + humanness).
fn render_social(profile: &Profile, viewer: &ViewLevel) -> String {
    let mut items = Vec::new();

    if let Some(&count) = profile.contact_count.for_viewer(viewer) {
        items.push(format!(
            r#"<div class="social-item"><div class="social-value">{count}</div><div class="social-label">Contacts</div></div>"#
        ));
    }

    if let Some(&freshness) = profile.humanness_freshness.for_viewer(viewer) {
        let pct = (freshness * 100.0).round() as u32;
        items.push(format!(
            r#"<div class="social-item"><div class="social-value">{pct}%</div><div class="social-label">Humanness</div><div class="freshness-bar"><div class="freshness-fill" style="width: {pct}%"></div></div></div>"#
        ));
    }

    if items.is_empty() {
        return String::new();
    }

    format!(
        r#"<div class="section"><div class="section-title">Social</div><div class="social-row">{}</div></div>"#,
        items.join("\n    ")
    )
}

/// Render the content section (active quests + offerings).
fn render_content(profile: &Profile, viewer: &ViewLevel) -> String {
    let mut sections = Vec::new();

    if let Some(quests) = profile.active_quests.for_viewer(viewer) {
        if !quests.is_empty() {
            let items: Vec<String> = quests.iter().map(|q| {
                format!(
                    r#"<li class="intention-item"><span class="intention-title">{}</span><span class="intention-badge">{}</span></li>"#,
                    html_escape(&q.title),
                    html_escape(&q.kind),
                )
            }).collect();
            sections.push(format!(
                r#"<div class="section"><div class="section-title">Active Quests</div><ul class="intention-list">{}</ul></div>"#,
                items.join("\n    ")
            ));
        }
    }

    if let Some(offerings) = profile.active_offerings.for_viewer(viewer) {
        if !offerings.is_empty() {
            let items: Vec<String> = offerings.iter().map(|o| {
                format!(
                    r#"<li class="intention-item"><span class="intention-title">{}</span><span class="intention-badge">{}</span></li>"#,
                    html_escape(&o.title),
                    html_escape(&o.kind),
                )
            }).collect();
            sections.push(format!(
                r#"<div class="section"><div class="section-title">Active Offerings</div><ul class="intention-list">{}</ul></div>"#,
                items.join("\n    ")
            ));
        }
    }

    sections.join("\n")
}

/// Render a JSON health check response.
pub fn render_health() -> String {
    r#"{"status":"ok"}"#.to_string()
}

/// Basic HTML escaping.
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
    use crate::profile::ViewLevel;

    #[test]
    fn render_profile_contains_name() {
        let profile = Profile::new("Alice", "alice", "abcdef1234567890abcdef1234567890");
        let html = render_profile(&profile, ViewLevel::Public);
        assert!(html.contains("Alice"));
        assert!(html.contains("@alice"));
        assert!(html.contains("abcdef12"));
    }

    #[test]
    fn render_profile_with_bio() {
        let profile = Profile::new("Bob", "bob", "1234567890abcdef1234567890abcdef")
            .with_bio("P2P enthusiast");
        let html = render_profile(&profile, ViewLevel::Public);
        assert!(html.contains("P2P enthusiast"));
    }

    #[test]
    fn render_profile_escapes_html() {
        let profile = Profile::new("<script>alert('xss')</script>", "hacker", "key123key123key123");
        let html = render_profile(&profile, ViewLevel::Public);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn health_check_is_valid_json() {
        let health = render_health();
        assert_eq!(health, r#"{"status":"ok"}"#);
    }

    #[test]
    fn private_fields_hidden_from_public() {
        let mut profile = Profile::new("Alice", "alice", "abcdef1234567890abcdef1234567890");
        profile.bio = indras_profile::Visible::private(Some("Secret bio".to_string()));
        let html = render_profile(&profile, ViewLevel::Public);
        assert!(!html.contains("Secret bio"));
    }

    #[test]
    fn private_fields_visible_to_owner() {
        let mut profile = Profile::new("Alice", "alice", "abcdef1234567890abcdef1234567890");
        profile.bio = indras_profile::Visible::private(Some("Secret bio".to_string()));
        let html = render_profile(&profile, ViewLevel::Owner);
        assert!(html.contains("Secret bio"));
    }

    #[test]
    fn stats_section_renders() {
        let mut profile = Profile::new("Alice", "alice", "abcdef1234567890abcdef1234567890");
        profile.set_intention_count(5);
        profile.set_token_count(3);
        let html = render_profile(&profile, ViewLevel::Public);
        assert!(html.contains("Intentions"));
        assert!(html.contains("Tokens"));
        assert!(html.contains(">5<"));
        assert!(html.contains(">3<"));
    }

    #[test]
    fn humanness_bar_renders() {
        let mut profile = Profile::new("Alice", "alice", "abcdef1234567890abcdef1234567890");
        profile.set_humanness_freshness(0.75);
        let html = render_profile(&profile, ViewLevel::Public);
        assert!(html.contains("75%"));
        assert!(html.contains("freshness-fill"));
    }
}
