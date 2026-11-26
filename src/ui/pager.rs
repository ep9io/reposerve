//! The bar of previous and next files at the foot of a page.
//!
//! The bar is fixed, so the reader can move between files without scrolling to
//! the end of a long page. It sits in the same place on every file view: the
//! shell draws it, and the views only say which files are either side.

use crate::site::html::{escape, href};

/// Links to the files either side of this one, in tree order.
///
/// Returns an empty string when the path has no neighbour, so a page at the
/// only file in a directory carries no empty bar.
#[must_use]
pub fn nav(previous: Option<&str>, next: Option<&str>) -> String {
    if previous.is_none() && next.is_none() {
        return String::new();
    }
    let mut output = String::from("<nav class=\"pager\" aria-label=\"Previous and next file\">");
    output.push_str(&link(previous, Side::Previous));
    output.push_str("<a class=\"pager-top\" href=\"#main\">↑ Top</a>");
    output.push_str(&link(next, Side::Next));
    output.push_str("</nav>");
    output
}

/// Which way a link moves through the files of a directory.
#[derive(Clone, Copy)]
enum Side {
    Previous,
    Next,
}

fn link(path: Option<&str>, side: Side) -> String {
    let Some(path) = path else {
        return String::new();
    };
    let (class, label) = match side {
        Side::Previous => ("previous", "← Previous"),
        Side::Next => ("next", "Next →"),
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    format!(
        "<a class=\"{class}\" href=\"/{}\"><span>{label}</span><strong>{}</strong></a>",
        href(path),
        escape(name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_neighbours_carry_their_name() {
        let html = nav(Some("docs/one.md"), Some("docs/three.md"));
        assert!(html.contains("class=\"previous\""), "{html}");
        assert!(html.contains("class=\"next\""), "{html}");
        assert!(html.contains("<strong>one.md</strong>"), "{html}");
        assert!(html.contains("<strong>three.md</strong>"), "{html}");
        assert!(html.contains("class=\"pager-top\""), "{html}");
    }

    #[test]
    fn a_missing_neighbour_leaves_no_empty_slot() {
        let html = nav(None, Some("docs/two.md"));
        assert!(!html.contains("class=\"previous\""), "{html}");
        assert!(html.contains("class=\"next\""), "{html}");
        assert!(!html.contains("<span></span>"), "{html}");
    }

    #[test]
    fn a_path_without_neighbours_has_no_bar() {
        assert!(nav(None, None).is_empty());
    }
}
