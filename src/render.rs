//! Markdown to HTML, with heading identifiers so cross-document anchors resolve.

use std::collections::HashMap;
use std::fmt::Write as _;

use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Renders CommonMark with the GitHub extensions this repository writes.
///
/// Every heading receives an identifier derived from its text, because the
/// documents link to each other as `page.md#section`.
#[must_use]
pub fn markdown_to_html(source: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let events: Vec<Event<'_>> = Parser::new_ext(source, options).collect();
    let events = prepare(events);

    let mut output = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut output, events.into_iter());
    output
}

/// Rewrites the parsed events that need it: heading identifiers, so the anchors
/// between documents resolve, and Mermaid blocks, so a diagram is an element the
/// browser script can draw instead of a code listing.
fn prepare<'a>(events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut output = Vec::with_capacity(events.len());
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut position = 0;

    while position < events.len() {
        match &events[position] {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let mut end = position + 1;
                while end < events.len() && !matches!(events[end], Event::End(TagEnd::CodeBlock)) {
                    end += 1;
                }
                if end >= events.len() {
                    output.extend(events[position..].iter().cloned());
                    break;
                }
                let source: String = events[position + 1..end]
                    .iter()
                    .filter_map(|event| {
                        if let Event::Text(text) = event {
                            Some(text.as_ref())
                        } else {
                            None
                        }
                    })
                    .collect();
                let language = info
                    .split([',', ' '])
                    .find(|part| !part.trim().is_empty())
                    .unwrap_or_default()
                    .trim();
                if language.eq_ignore_ascii_case("mermaid") {
                    output.push(Event::Html(
                        format!(
                            "<pre class=\"mermaid\">{}</pre>",
                            crate::pages::escape(&source)
                        )
                        .into(),
                    ));
                } else if let Some(highlighted) =
                    crate::highlight::global().for_language(language, &source)
                {
                    output.push(Event::Html(
                        format!(
                            "<div class=\"copyable code-block\">{copy}                             <pre><code class=\"language-{}\">{highlighted}</code></pre></div>",
                            crate::pages::escape(language),
                            copy = crate::pages::COPY_BUTTON
                        )
                        .into(),
                    ));
                } else {
                    // No syntax for this label: escaped text, still copyable.
                    output.push(Event::Html(
                        format!(
                            "<div class=\"copyable code-block\">{copy}\
                             <pre><code>{}</code></pre></div>",
                            crate::pages::escape(&source),
                            copy = crate::pages::COPY_BUTTON
                        )
                        .into(),
                    ));
                }
                position = end + 1;
            }
            Event::Start(Tag::Heading {
                level,
                id: None,
                classes,
                attrs,
            }) => {
                let mut end = position + 1;
                while end < events.len() && !matches!(events[end], Event::End(TagEnd::Heading(_))) {
                    end += 1;
                }
                if end >= events.len() {
                    // Malformed input: no closing event. Emit the rest unchanged.
                    output.extend(events[position..].iter().cloned());
                    break;
                }
                let text = plain_text(&events[position + 1..end]);
                let id = unique(slugify(&text), &mut used);
                output.push(Event::Start(Tag::Heading {
                    level: *level,
                    id: Some(id.into()),
                    classes: classes.clone(),
                    attrs: attrs.clone(),
                }));
                output.extend(events[position + 1..end].iter().cloned());
                output.push(events[end].clone());
                position = end + 1;
            }
            other @ (Event::Start(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_)) => {
                output.push(other.clone());
                position += 1;
            }
        }
    }

    output
}

/// The visible text of an inline event run, which is what an identifier is made from.
fn plain_text(events: &[Event<'_>]) -> String {
    let mut text = String::new();
    for event in events {
        match event {
            Event::Text(value) | Event::Code(value) => text.push_str(value),
            Event::SoftBreak | Event::HardBreak => text.push(' '),
            Event::Start(_)
            | Event::End(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    text
}

/// A GitHub-style identifier: lower case, words joined by single hyphens.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_alphanumeric() {
            slug.extend(character.to_lowercase());
        } else if matches!(character, ' ' | '-' | '_') && !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// Keeps identifiers unique within one document, as GitHub does.
fn unique(base: String, used: &mut HashMap<String, usize>) -> String {
    let base = if base.is_empty() {
        "section".to_string()
    } else {
        base
    };
    let count = used.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count - 1)
    }
}

/// Renders a source file as numbered lines, with the markup escaped.
///
/// Lines are split on `\n` and the gutter holds one number per line. Both halves
/// share a line height, so the numbers stay aligned while the code scrolls
/// sideways on a narrow screen.
#[must_use]
pub fn numbered_source(source: &str, first_line: usize) -> String {
    let mut gutter = String::new();
    let mut body = String::new();
    for (offset, line) in source.lines().enumerate() {
        let _ = writeln!(gutter, "{}", first_line + offset);
        body.push_str(&crate::pages::escape(line));
        body.push('\n');
    }
    if source.is_empty() {
        return format!(
            "<div class=\"code copyable\">{copy}\
             <pre class=\"gutter\">1</pre><pre><code>\n</code></pre></div>",
            copy = crate::pages::COPY_BUTTON
        );
    }
    format!(
        "<div class=\"code copyable\">{copy}\
         <pre class=\"gutter\" aria-hidden=\"true\">{gutter}</pre>\
         <pre><code>{body}</code></pre></div>",
        copy = crate::pages::COPY_BUTTON
    )
}

/// Wraps already-highlighted HTML in the numbered code layout.
///
/// The gutter is built from the line count, so it stays aligned: the highlighter
/// emits one line per source line.
#[must_use]
pub fn numbered_highlighted(html: &str, lines: usize) -> String {
    let mut gutter = String::new();
    for number in 1..=lines.max(1) {
        let _ = writeln!(gutter, "{number}");
    }
    format!(
        "<div class=\"code copyable\">{copy}\
         <pre class=\"gutter\" aria-hidden=\"true\">{gutter}</pre>\
         <pre><code>{html}</code></pre></div>",
        copy = crate::pages::COPY_BUTTON
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_receive_identifiers_that_match_their_text() {
        let html = markdown_to_html("# Configuration\nThe rules.\n\n## The config schema\n");
        assert!(html.contains("<h1 id=\"configuration\">"), "{html}");
        assert!(html.contains("<h2 id=\"the-config-schema\">"), "{html}");
    }

    #[test]
    fn repeated_headings_are_numbered() {
        let html = markdown_to_html("## Notes\n\ntext\n\n## Notes\n");
        assert!(html.contains("id=\"notes\""), "{html}");
        assert!(html.contains("id=\"notes-1\""), "{html}");
    }

    #[test]
    fn punctuation_is_dropped_from_identifiers() {
        assert_eq!(
            slugify("Shell: The loop, step by step"),
            "shell-the-loop-step-by-step"
        );
        assert_eq!(
            slugify("`build-ios` and `check-ios-app`"),
            "build-ios-and-check-ios-app"
        );
    }

    #[test]
    fn tables_and_task_lists_render() {
        let html = markdown_to_html("| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- [x] done\n");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
    }

    #[test]
    fn a_mermaid_block_becomes_a_diagram_element() {
        let html = markdown_to_html("```mermaid\ngraph TD\n  A --> B\n```\n");
        assert!(html.contains("<pre class=\"mermaid\">"), "{html}");
        assert!(html.contains("A --&gt; B"), "{html}");
    }

    #[test]
    fn a_labelled_fence_is_highlighted() {
        let html = markdown_to_html("```rust\nfn main() {}\n```\n");
        assert!(html.contains("hl-source hl-rust"), "{html}");
        assert!(
            html.contains("<pre><code class=\"language-rust\">"),
            "{html}"
        );
    }

    #[test]
    fn other_fences_stay_code() {
        let html = markdown_to_html("```rust\nlet x = 1 < 2;\n```\n");
        assert!(html.contains("<code class=\"language-rust\">"), "{html}");
        assert!(!html.contains("class=\"mermaid\""), "{html}");
    }

    #[test]
    fn source_lines_are_escaped_and_numbered() {
        let html = numbered_source("fn main() {\n    let x = a < b;\n}\n", 1);
        assert!(html.contains("a &lt; b;"), "{html}");
        assert!(html.contains("3"), "{html}");
    }
}
