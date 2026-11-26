//! The table of contents beside a long page.

use std::fmt::Write as _;

use crate::site::html::escape;
use crate::site::render::Heading;

/// The headings of a page, as a list of links.
///
/// A page with fewer than two headings has nothing to list, so the caller skips
/// this block. A `<details>` element keeps the list out of the way on a phone.
/// The list starts closed: the script opens it on a wide screen, where it is a
/// sidebar rather than an overlay.
///
/// `label` names what the list holds. A document says "On this page", and a
/// notebook says "In this notebook".
#[must_use]
pub fn aside(headings: &[Heading], label: &str) -> String {
    let label = escape(label);
    let mut output = format!(
        "<aside class=\"toc\" aria-label=\"{label}\"><details>\
         <summary aria-label=\"{label}\">\
         <svg class=\"toc-icon\" viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" \
         fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.7\">\
         <path d=\"M4 6h16M4 12h16M4 18h16\"/></svg>\
         <span class=\"toc-label\">{label}</span></summary><ul>"
    );
    for heading in headings {
        let _ = write!(
            output,
            "<li class=\"level{level}\"><a href=\"#{id}\">{text}</a></li>",
            level = heading.level,
            id = escape(&heading.id),
            text = escape(&heading.text)
        );
    }
    output.push_str("</ul></details></aside>");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_heading_becomes_one_link() {
        let headings = vec![
            Heading {
                level: 1,
                id: "start".to_string(),
                text: "Start".to_string(),
            },
            Heading {
                level: 2,
                id: "next".to_string(),
                text: "A < b".to_string(),
            },
        ];
        let html = aside(&headings, "In this notebook");
        assert!(html.contains("href=\"#start\""), "{html}");
        assert!(html.contains("A &lt; b"), "{html}");
        assert_eq!(html.matches("<li").count(), 2);
        assert!(html.contains("In this notebook"), "{html}");
        assert!(
            html.contains("aria-label=\"In this notebook\""),
            "the label names the list for a screen reader: {html}"
        );
    }
}
