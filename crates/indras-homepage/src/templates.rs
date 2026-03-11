//! HTML template rendering for the homepage.

use crate::{ContentArtifact, ProfileFieldArtifact, fields};

/// Render the homepage as HTML with grant-filtered fields and artifacts.
pub fn render_homepage(
    fields: &[&ProfileFieldArtifact],
    artifacts: &[&ContentArtifact],
) -> String {
    // Look up identity fields by name
    let display_name = field_value(fields, fields::DISPLAY_NAME).unwrap_or_default();
    let initial = display_name.chars().next().unwrap_or('?');
    let username = field_value(fields, fields::USERNAME).unwrap_or_default();

    let bio_section = field_value(fields, fields::BIO)
        .filter(|s| !s.is_empty())
        .map(|bio| format!(r#"<p class="bio">{}</p>"#, html_escape(&bio)))
        .unwrap_or_default();

    let key_section = field_value(fields, fields::PUBLIC_KEY)
        .map(|key| {
            let short_key = if key.len() > 16 {
                format!("{}…{}", &key[..8], &key[key.len()-8..])
            } else {
                key
            };
            format!(r#"<div class="key"><span class="key-dot"></span><span>{short_key}</span></div>"#)
        })
        .unwrap_or_default();

    let stats_section = render_stats(fields);
    let social_section = render_social(fields);
    let content_section = render_content(fields);
    let artifacts_section = render_artifacts(artifacts);

    let page_title = if display_name.is_empty() { "Anonymous".to_string() } else { html_escape(&display_name) };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{page_title} — IndrasNetwork</title>
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
.artifact-list {{
  list-style: none;
}}
.artifact-item {{
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: 10px;
  padding: 12px 16px;
  margin-bottom: 8px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}}
.artifact-name {{
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}}
.artifact-meta {{
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-muted);
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
  {artifacts}
  <div class="footer">
    Powered by <a href="https://github.com/indras-network/indras-network">IndrasNetwork</a>
  </div>
</div>
</body>
</html>"#,
        name = html_escape(&display_name),
        initial = initial,
        username = html_escape(&username),
        bio = bio_section,
        key = key_section,
        stats = stats_section,
        social = social_section,
        content = content_section,
        artifacts = artifacts_section,
    )
}

/// Get a field's display value by name.
fn field_value(fields: &[&ProfileFieldArtifact], name: &str) -> Option<String> {
    fields.iter()
        .find(|f| f.field_name == name)
        .map(|f| f.display_value.clone())
}

/// Render the activity stats grid.
fn render_stats(fields: &[&ProfileFieldArtifact]) -> String {
    let mut cards = Vec::new();

    if let Some(val) = field_value(fields, fields::INTENTION_COUNT) {
        cards.push(format!(
            r#"<div class="stat-card"><div class="stat-value">{}</div><div class="stat-label">Intentions</div></div>"#,
            html_escape(&val)
        ));
    }
    if let Some(val) = field_value(fields, fields::TOKEN_COUNT) {
        cards.push(format!(
            r#"<div class="stat-card"><div class="stat-value">{}</div><div class="stat-label">Tokens</div></div>"#,
            html_escape(&val)
        ));
    }
    if let Some(val) = field_value(fields, fields::BLESSINGS_GIVEN) {
        cards.push(format!(
            r#"<div class="stat-card"><div class="stat-value">{}</div><div class="stat-label">Blessings Given</div></div>"#,
            html_escape(&val)
        ));
    }
    if let Some(val) = field_value(fields, fields::ATTENTION_CONTRIBUTED) {
        if !val.is_empty() {
            cards.push(format!(
                r#"<div class="stat-card"><div class="stat-value">{}</div><div class="stat-label">Attention</div></div>"#,
                html_escape(&val)
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
fn render_social(fields: &[&ProfileFieldArtifact]) -> String {
    let mut items = Vec::new();

    if let Some(val) = field_value(fields, fields::CONTACT_COUNT) {
        items.push(format!(
            r#"<div class="social-item"><div class="social-value">{val}</div><div class="social-label">Contacts</div></div>"#
        ));
    }

    if let Some(val) = field_value(fields, fields::HUMANNESS_FRESHNESS) {
        if let Ok(freshness) = val.parse::<f64>() {
            let pct = (freshness * 100.0).round() as u32;
            items.push(format!(
                r#"<div class="social-item"><div class="social-value">{pct}%</div><div class="social-label">Humanness</div><div class="freshness-bar"><div class="freshness-fill" style="width: {pct}%"></div></div></div>"#
            ));
        }
    }

    if items.is_empty() {
        return String::new();
    }

    format!(
        r#"<div class="section"><div class="section-title">Social</div><div class="social-row">{}</div></div>"#,
        items.join("\n    ")
    )
}

/// Render the content section (active quests + offerings from field values).
///
/// The active_quests and active_offerings fields store JSON-serialized
/// `Vec<IntentionSummary>` as their display_value.
fn render_content(fields: &[&ProfileFieldArtifact]) -> String {
    let mut sections = Vec::new();

    if let Some(val) = field_value(fields, fields::ACTIVE_QUESTS) {
        if let Ok(quests) = serde_json::from_str::<Vec<crate::IntentionSummary>>(&val) {
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
    }

    if let Some(val) = field_value(fields, fields::ACTIVE_OFFERINGS) {
        if let Ok(offerings) = serde_json::from_str::<Vec<crate::IntentionSummary>>(&val) {
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
    }

    sections.join("\n")
}

/// Render the content artifacts section.
fn render_artifacts(artifacts: &[&ContentArtifact]) -> String {
    if artifacts.is_empty() {
        return String::new();
    }

    let items: Vec<String> = artifacts.iter().map(|a| {
        let mime = a.mime_type.as_deref().unwrap_or("unknown");
        let size_str = format_size(a.size);
        format!(
            r#"<li class="artifact-item"><span class="artifact-name">{}</span><span class="artifact-meta">{} · {}</span></li>"#,
            html_escape(&a.name),
            html_escape(mime),
            size_str,
        )
    }).collect();

    format!(
        r#"<div class="section"><div class="section-title">Artifacts</div><ul class="artifact-list">{}</ul></div>"#,
        items.join("\n    ")
    )
}

/// Format byte size as human-readable string.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
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
    use indras_artifacts::access::{AccessGrant, AccessMode};

    fn public_field(name: &str, value: &str) -> ProfileFieldArtifact {
        ProfileFieldArtifact {
            field_name: name.to_string(),
            display_value: value.to_string(),
            grants: vec![AccessGrant {
                grantee: [0x00; 32],
                mode: AccessMode::Public,
                granted_at: 0,
                granted_by: [0x00; 32],
            }],
        }
    }

    #[test]
    fn render_contains_name() {
        let fields = vec![
            public_field("display_name", "Alice"),
            public_field("username", "alice"),
            public_field("public_key", "abcdef1234567890abcdef1234567890"),
        ];
        let field_refs: Vec<&ProfileFieldArtifact> = fields.iter().collect();
        let html = render_homepage(&field_refs, &[]);
        assert!(html.contains("Alice"));
        assert!(html.contains("@alice"));
        assert!(html.contains("abcdef12"));
    }

    #[test]
    fn render_with_bio() {
        let fields = vec![
            public_field("display_name", "Bob"),
            public_field("username", "bob"),
            public_field("bio", "P2P enthusiast"),
        ];
        let field_refs: Vec<&ProfileFieldArtifact> = fields.iter().collect();
        let html = render_homepage(&field_refs, &[]);
        assert!(html.contains("P2P enthusiast"));
    }

    #[test]
    fn render_escapes_html() {
        let fields = vec![
            public_field("display_name", "<script>alert('xss')</script>"),
            public_field("username", "hacker"),
        ];
        let field_refs: Vec<&ProfileFieldArtifact> = fields.iter().collect();
        let html = render_homepage(&field_refs, &[]);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn health_check_is_valid_json() {
        let health = render_health();
        assert_eq!(health, r#"{"status":"ok"}"#);
    }

    #[test]
    fn stats_section_renders() {
        let fields = vec![
            public_field("display_name", "Alice"),
            public_field("username", "alice"),
            public_field("intention_count", "5"),
            public_field("token_count", "3"),
        ];
        let field_refs: Vec<&ProfileFieldArtifact> = fields.iter().collect();
        let html = render_homepage(&field_refs, &[]);
        assert!(html.contains("Intentions"));
        assert!(html.contains("Tokens"));
        assert!(html.contains(">5<"));
        assert!(html.contains(">3<"));
    }

    #[test]
    fn humanness_bar_renders() {
        let fields = vec![
            public_field("display_name", "Alice"),
            public_field("username", "alice"),
            public_field("humanness_freshness", "0.75"),
        ];
        let field_refs: Vec<&ProfileFieldArtifact> = fields.iter().collect();
        let html = render_homepage(&field_refs, &[]);
        assert!(html.contains("75%"));
        assert!(html.contains("freshness-fill"));
    }
}
