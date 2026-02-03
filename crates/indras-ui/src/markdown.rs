//! Markdown rendering utilities.

/// Render markdown to HTML.
pub fn render_markdown_to_html(markdown: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Check if a file is markdown based on name or MIME type.
pub fn is_markdown_file(name: &str, mime_type: &str) -> bool {
    name.ends_with(".md")
        || name.ends_with(".markdown")
        || mime_type == "text/markdown"
        || mime_type == "application/markdown"
}
