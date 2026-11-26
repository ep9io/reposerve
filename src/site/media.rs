//! The content type of a repository file that is served as bytes.
//!
//! A file listed here is sent unchanged. Everything else is rendered as a page
//! or shown as source.

/// The content type of a media file, or `None` when the file is not media.
#[must_use]
pub fn content_type(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension)?;
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        "pdf" => Some("application/pdf"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        "ttf" => Some("font/ttf"),
        _ => None,
    }
}

/// Whether a path is an image the page can show.
#[must_use]
pub fn is_image(path: &str) -> bool {
    content_type(path).is_some_and(|kind| kind.starts_with("image/"))
}

/// Whether a path is a notebook.
#[must_use]
pub fn is_notebook(path: &str) -> bool {
    has_extension(path, &["ipynb"])
}

/// Whether a path is Markdown.
#[must_use]
pub fn is_markdown(path: &str) -> bool {
    has_extension(path, &["md", "markdown"])
}

fn has_extension(path: &str, wanted: &[&str]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        wanted
            .iter()
            .any(|want| extension.eq_ignore_ascii_case(want))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_files_carry_their_type() {
        assert_eq!(content_type("logo.png"), Some("image/png"));
        assert_eq!(content_type("docs/manual.PDF"), Some("application/pdf"));
        assert_eq!(content_type("src/main.rs"), None);
        assert_eq!(content_type("justfile"), None);
    }

    #[test]
    fn kinds_are_recognised_by_extension() {
        assert!(is_markdown("docs/README.md"));
        assert!(is_markdown("work/notes.markdown"));
        assert!(!is_markdown("crates/main.rs"));
        assert!(is_notebook("analysis/report.IPYNB"));
        assert!(!is_notebook("docs/README.md"));
        assert!(is_image("media/chart.png"));
        assert!(is_image("media/logo.SVG"));
        assert!(!is_image("docs/manual.pdf"));
    }
}
