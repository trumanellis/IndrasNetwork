//! File loading utilities for UI display.

/// Convert a local file path to a data URL for display in webview.
pub fn load_image_as_data_url(path: &str) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let full_path = if path.starts_with('/') {
        std::path::PathBuf::from(path)
    } else {
        std::env::current_dir().ok()?.join(path)
    };

    let data = std::fs::read(&full_path).ok()?;

    let mime = match full_path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };

    let encoded = STANDARD.encode(&data);
    Some(format!("data:{};base64,{}", mime, encoded))
}

/// Load a text file's full content from an asset path.
pub fn load_text_file_content(path: &str) -> Option<String> {
    let full_path = if path.starts_with('/') {
        std::path::PathBuf::from(path)
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    std::fs::read_to_string(&full_path).ok()
}
