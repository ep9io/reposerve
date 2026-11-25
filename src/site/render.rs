//! Markdown to HTML, with heading identifiers, math and diagram hooks.
//!
//! Every heading receives an identifier derived from its text, because the
//! documents link to each other as `page.md#section`. The identifiers are also
//! the table of contents.

use std::collections::HashMap;
use std::fmt::Write as _;

use pulldown_cmark::{html, CodeBlockKind, Event, Options as MarkdownOptions, Parser, Tag, TagEnd};

use crate::site::html::escape;

/// The copy button that sits beside a code block.
pub const COPY_BUTTON: &str = "<button class=\"copy\" type=\"button\" data-copy>Copy</button>";

/// One heading, as the table of contents needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// The heading level, 1 to 6.
    pub level: u8,
    /// The identifier in the page.
    pub id: String,
    /// The visible text.
    pub text: String,
}

/// A rendered Markdown document and the facts a page needs about it.
#[derive(Debug, Default)]
pub struct Document {
    /// The body HTML.
    pub html: String,
    /// The title a front matter block names, when it names one.
    pub title: Option<String>,
    /// Every heading, in document order.
    pub headings: Vec<Heading>,
    /// Whether the body holds math.
    pub has_math: bool,
    /// Whether the body holds a Mermaid diagram.
    pub has_mermaid: bool,
    /// Whether the body holds highlighted code.
    pub has_syntax: bool,
}

/// What the renderer may add to a document.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Whether math is rendered instead of shown as text.
    pub math: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { math: true }
    }
}

/// Renders CommonMark with the GitHub extensions this repository writes.
#[must_use]
pub fn markdown(source: &str, options: Options) -> Document {
    let (title, body) = split_front_matter(source);
    let parser = Parser::new_ext(body, markdown_options());
    let mut document = Document {
        title,
        ..Document::default()
    };

    let events: Vec<Event<'_>> = parser.collect();
    let prepared = prepare(events, options, &mut document);
    html::push_html(&mut document.html, prepared.into_iter());

    finish(&mut document.html);
    document
}

/// The Markdown features the server enables.
fn markdown_options() -> MarkdownOptions {
    let mut options = MarkdownOptions::empty();
    options.insert(MarkdownOptions::ENABLE_TABLES);
    options.insert(MarkdownOptions::ENABLE_FOOTNOTES);
    options.insert(MarkdownOptions::ENABLE_STRIKETHROUGH);
    options.insert(MarkdownOptions::ENABLE_TASKLISTS);
    options.insert(MarkdownOptions::ENABLE_SMART_PUNCTUATION);
    options.insert(MarkdownOptions::ENABLE_HEADING_ATTRIBUTES);
    options.insert(MarkdownOptions::ENABLE_DEFINITION_LIST);
    options.insert(MarkdownOptions::ENABLE_GFM);
    options.insert(MarkdownOptions::ENABLE_MATH);
    options
}

/// Rewrites the events that need it.
///
/// A fenced block becomes a highlighted code block, a Mermaid diagram or a
/// math block. A heading gains an identifier and an anchor. A table gains a
/// scrolling container, so a wide table never widens the page.
fn prepare<'a>(
    events: Vec<Event<'a>>,
    options: Options,
    document: &mut Document,
) -> Vec<Event<'a>> {
    let mut output = Vec::with_capacity(events.len() + events.len() / 8);
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut position = 0;

    while position < events.len() {
        let event = &events[position];

        if let Event::Start(Tag::CodeBlock(kind)) = event {
            if let Some((html, consumed)) = code_block(&events[position..], kind, document) {
                output.push(Event::Html(html.into()));
                position += consumed;
                continue;
            }
        }

        if let Event::Start(Tag::Heading { level, id, .. }) = event {
            let mut end = position + 1;
            while end < events.len() && !matches!(events[end], Event::End(TagEnd::Heading(_))) {
                end += 1;
            }
            if end >= events.len() {
                output.extend(events[position..].iter().cloned());
                break;
            }
            let text = plain_text(&events[position + 1..end]);
            let identifier = match id {
                Some(id) => id.to_string(),
                None => unique(slugify(&text), &mut used),
            };
            let level = heading_level(*level);
            document.headings.push(Heading {
                level,
                id: identifier.clone(),
                text,
            });
            // The heading text links to itself, so a reader can copy the address
            // of a section. The link carries the heading colour, so the heading
            // still reads as a heading rather than as a link.
            let identifier = escape(&identifier);
            output.push(Event::Html(
                format!("<h{level} id=\"{identifier}\"><a class=\"heading-link\" href=\"#{identifier}\">")
                    .into(),
            ));
            output.extend(events[position + 1..end].iter().cloned());
            output.push(Event::Html(format!("</a></h{level}>\n").into()));
            position = end + 1;
            continue;
        }

        if let Event::Start(Tag::BlockQuote(Some(kind))) = event {
            // The callout keeps a worded label, so a reader who cannot see the
            // colour still knows what the block is.
            let label = alert_label(*kind);
            output.push(Event::Html(
                format!("<blockquote class=\"alert\"><p class=\"alert-title\">{label}</p>").into(),
            ));
            position += 1;
            continue;
        }

        if let Event::End(TagEnd::BlockQuote(Some(_))) = event {
            output.push(Event::Html("</blockquote>".into()));
            position += 1;
            continue;
        }

        if let Event::Start(Tag::Table(_)) = event {
            output.push(Event::Html(
                "<div class=\"table-scroll\" tabindex=\"0\" role=\"region\" aria-label=\"Table\">"
                    .into(),
            ));
            output.push(event.clone());
            position += 1;
            continue;
        }

        if let Event::End(TagEnd::Table) = event {
            output.push(event.clone());
            output.push(Event::Html("</div>".into()));
            position += 1;
            continue;
        }

        if let Event::InlineMath(tex) | Event::DisplayMath(tex) = event {
            let display = matches!(event, Event::DisplayMath(_));
            if options.math {
                document.has_math = true;
                let class = if display { "math math-display" } else { "math" };
                output.push(Event::Html(
                    format!("<span class=\"{class}\">{}</span>", escape(tex)).into(),
                ));
            } else {
                output.push(Event::Html(
                    format!("<code class=\"math-source\">{}</code>", escape(tex)).into(),
                ));
            }
            position += 1;
            continue;
        }

        output.push(event.clone());
        position += 1;
    }

    output
}

/// Renders one fenced or indented code block.
///
/// Returns the HTML and the number of events consumed, or `None` when the block
/// has no end, in which case the caller emits the events unchanged.
fn code_block<'a>(
    events: &[Event<'a>],
    kind: &CodeBlockKind<'_>,
    document: &mut Document,
) -> Option<(String, usize)> {
    let mut end = 1;
    while end < events.len() && !matches!(events[end], Event::End(TagEnd::CodeBlock)) {
        end += 1;
    }
    if end >= events.len() {
        return None;
    }

    let source: String = events[1..end]
        .iter()
        .filter_map(|event| {
            if let Event::Text(text) = event {
                Some(text.as_ref())
            } else {
                None
            }
        })
        .collect();

    let language = match kind {
        CodeBlockKind::Fenced(info) => info
            .split([',', ' '])
            .find(|part| !part.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_string(),
        CodeBlockKind::Indented => String::new(),
    };

    let html = if language.eq_ignore_ascii_case("mermaid") {
        document.has_mermaid = true;
        format!(
            "<div class=\"diagram\"><pre class=\"mermaid\">{}</pre></div>",
            escape(&source)
        )
    } else if let Some(highlighted) =
        crate::site::highlight::global().for_language(&language, &source)
    {
        document.has_syntax = true;
        code_frame(&language, &highlighted)
    } else {
        code_frame(&language, &escape(&source))
    };

    Some((html, end + 1))
}

/// The frame around one highlighted code block: a label, a copy button, the code.
fn code_frame(language: &str, body: &str) -> String {
    let label = if language.is_empty() {
        String::new()
    } else {
        format!(
            "<span class=\"lang\">{}</span>",
            escape(&language.to_ascii_lowercase())
        )
    };
    format!(
        "<div class=\"copyable code-block\"><div class=\"code-head\">{label}{COPY_BUTTON}</div>\
         <pre><code>{body}</code></pre></div>"
    )
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

/// The word a callout shows, from the kind GitHub gives it.
fn alert_label(kind: pulldown_cmark::BlockQuoteKind) -> &'static str {
    use pulldown_cmark::BlockQuoteKind;
    match kind {
        BlockQuoteKind::Note => "Note",
        BlockQuoteKind::Tip => "Tip",
        BlockQuoteKind::Important => "Important",
        BlockQuoteKind::Warning => "Warning",
        BlockQuoteKind::Caution => "Caution",
    }
}

/// The number of a heading level, 1 to 6.
fn heading_level(level: pulldown_cmark::HeadingLevel) -> u8 {
    use pulldown_cmark::HeadingLevel;
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
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

/// Splits a leading YAML block from the body and reads its `title` key.
///
/// Only `key: value` lines at the top level are read. A document without front
/// matter keeps all of its text.
fn split_front_matter(source: &str) -> (Option<String>, &str) {
    let Some(rest) = source.strip_prefix("---\n") else {
        return (None, source);
    };
    let Some(end) = rest.find("\n---") else {
        return (None, source);
    };
    let block = &rest[..end];
    let body = rest[end + 1..].trim_start_matches(['-', '\n']);
    let mut title = None;
    for line in block.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "title" {
            let value = value.trim().trim_matches(['"', '\'']);
            if !value.is_empty() {
                title = Some(value.to_string());
            }
        }
    }
    (title, body)
}

/// Adds the attributes that keep an image from loading before it is needed.
fn finish(html: &mut String) {
    if html.contains("<img ") {
        *html = html.replace("<img ", "<img loading=\"lazy\" decoding=\"async\" ");
    }
    // A link to another site opens in a new tab, so the reader keeps the page.
    if html.contains("<a href=\"http") {
        *html = html.replace(
            "<a href=\"http",
            "<a rel=\"noopener noreferrer\" target=\"_blank\" href=\"http",
        );
    }
}

/// Renders a source file as numbered lines, with the markup escaped.
///
/// The gutter holds one link per line, so `#L12` reaches line twelve. Lines are
/// split on `\n` and each one is a span, so a long file stays one element.
#[must_use]
pub fn source_view(source: &str, highlighted: Option<&str>, label: &str) -> String {
    let html = match highlighted {
        Some(html) => html.to_string(),
        None => escape_lines(source),
    };
    let lines: Vec<&str> = {
        let mut lines: Vec<&str> = html.split_terminator('\n').collect();
        // The highlighter may add one newline at the end. The source decides
        // how many lines a file has, so drop any line the source does not hold.
        let expected = source.lines().count();
        if expected > 0 && lines.len() > expected {
            lines.truncate(expected);
        }
        lines
    };
    let mut gutter = String::new();
    let mut body = String::new();
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        // The gutter copies hold no newline: each link is a block, and a
        // newline inside `<pre>` would add an empty line between numbers.
        let _ = write!(
            gutter,
            "<a href=\"#L{number}\" tabindex=\"-1\">{number}</a>"
        );
        let _ = writeln!(body, "<span class=\"line\" id=\"L{number}\">{line}</span>");
    }
    if lines.is_empty() {
        gutter.push_str("<a href=\"#L1\" tabindex=\"-1\">1</a>");
        body.push_str("<span class=\"line\" id=\"L1\">\n</span>");
    }
    let head = if label.is_empty() {
        String::new()
    } else {
        format!("<span class=\"lang\">{}</span>", escape(label))
    };
    format!(
        "<div class=\"code copyable\">\
         <div class=\"code-head\">{head}{COPY_BUTTON}</div>\
         <pre class=\"gutter\" aria-hidden=\"true\">{gutter}</pre>\
         <pre><code>{body}</code></pre></div>"
    )
}

fn escape_lines(source: &str) -> String {
    let mut output = String::with_capacity(source.len() + 8);
    for line in source.split_terminator('\n') {
        output.push_str(&escape(line));
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: &str) -> Document {
        markdown(source, Options::default())
    }

    #[test]
    fn headings_receive_identifiers_that_match_their_text() {
        let document = render("# Configuration\nThe rules.\n\n## The config schema\n");
        assert!(
            document.html.contains("<h1 id=\"configuration\">"),
            "{}",
            document.html
        );
        assert!(
            document.html.contains("<h2 id=\"the-config-schema\">"),
            "{}",
            document.html
        );
        assert_eq!(document.headings.len(), 2);
        assert_eq!(document.headings[1].level, 2);
        assert_eq!(document.headings[1].text, "The config schema");
    }

    #[test]
    fn a_heading_carries_a_link_to_itself() {
        let document = render("## Notes\n");
        assert!(
            document.html.contains(
                "<h2 id=\"notes\"><a class=\"heading-link\" href=\"#notes\">Notes</a></h2>"
            ),
            "{}",
            document.html
        );
        assert!(
            !document.html.contains("#</a>"),
            "the heading text is the link, so no hash follows it: {}",
            document.html
        );
    }

    #[test]
    fn repeated_headings_are_numbered() {
        let document = render("## Notes\n\ntext\n\n## Notes\n");
        assert!(document.html.contains("id=\"notes\""), "{}", document.html);
        assert!(
            document.html.contains("id=\"notes-1\""),
            "{}",
            document.html
        );
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
        let document = render("| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- [x] done\n");
        assert!(document.html.contains("<table>"), "{}", document.html);
        assert!(
            document.html.contains("type=\"checkbox\""),
            "{}",
            document.html
        );
    }

    #[test]
    fn a_table_sits_in_a_scrolling_container() {
        let document = render("| A | B |\n| --- | --- |\n| 1 | 2 |\n");
        assert!(
            document.html.contains("<div class=\"table-scroll\""),
            "{}",
            document.html
        );
        assert!(
            document.html.contains("</table>\n</div>"),
            "{}",
            document.html
        );
    }
    #[test]
    fn a_callout_keeps_a_worded_label() {
        let document = render("> [!WARNING]\n> Mind the step.\n");
        assert!(
            document.html.contains("class=\"alert\""),
            "{}",
            document.html
        );
        assert!(document.html.contains(">Warning<"), "{}", document.html);
        assert!(
            document.html.contains("Mind the step."),
            "{}",
            document.html
        );
    }

    #[test]
    fn a_mermaid_block_becomes_a_diagram_element() {
        let document = render("```mermaid\ngraph TD\n  A --> B\n```\n");
        assert!(
            document.html.contains("<pre class=\"mermaid\">"),
            "{}",
            document.html
        );
        assert!(document.html.contains("A --&gt; B"), "{}", document.html);
        assert!(document.has_mermaid);
    }

    #[test]
    fn a_labelled_fence_is_highlighted_and_copies() {
        let document = render("```rust\nfn main() {}\n```\n");
        assert!(
            document.html.contains("hl-source hl-rust"),
            "{}",
            document.html
        );
        assert!(
            document.html.contains("class=\"lang\">rust<"),
            "{}",
            document.html
        );
        assert!(document.html.contains("data-copy"), "{}", document.html);
    }

    #[test]
    fn an_unlabelled_indented_block_still_copies() {
        let document = render("    let x = 1;\n");
        assert!(document.html.contains("data-copy"), "{}", document.html);
        assert!(document.html.contains("let x = 1;"), "{}", document.html);
        assert!(
            !document.html.contains("class=\"lang\""),
            "{}",
            document.html
        );
    }

    #[test]
    fn math_becomes_a_lazy_element_and_can_be_turned_off() {
        let document = render("Inline $a^2$ and display:\n\n$$\nx + y\n$$\n");
        assert!(
            document.html.contains("<span class=\"math\">a^2</span>"),
            "{}",
            document.html
        );
        assert!(
            document.html.contains("math math-display"),
            "{}",
            document.html
        );
        assert!(document.has_math);

        let plain = markdown("Inline $a^2$\n", Options { math: false });
        assert!(!plain.has_math);
        assert!(
            plain
                .html
                .contains("<code class=\"math-source\">a^2</code>"),
            "{}",
            plain.html
        );
    }

    #[test]
    fn an_image_loads_lazily_and_an_external_link_is_safe() {
        let document = render("![shot](shot.png)\n\n[site](https://example.com)\n");
        assert!(
            document
                .html
                .contains("<img loading=\"lazy\" decoding=\"async\""),
            "{}",
            document.html
        );
        assert!(
            document.html.contains(
                "<a rel=\"noopener noreferrer\" target=\"_blank\" href=\"https://example.com\">"
            ),
            "{}",
            document.html
        );
    }

    #[test]
    fn front_matter_supplies_a_title_and_leaves_the_body_alone() {
        let document = render("---\ntitle: \"The handbook\"\nowner: me\n---\n\n# Welcome\n");
        assert_eq!(document.title.as_deref(), Some("The handbook"));
        assert!(
            document.html.contains("<h1 id=\"welcome\">"),
            "{}",
            document.html
        );
        assert!(!document.html.contains("handbook"), "{}", document.html);
    }

    #[test]
    fn a_document_without_front_matter_keeps_its_first_line() {
        let document = render("# Start\n");
        assert!(document.title.is_none());
        assert!(document.html.contains("Start"), "{}", document.html);
    }

    #[test]
    fn source_lines_are_escaped_numbered_and_anchored() {
        let view = source_view("fn main() {\n    let x = a < b;\n}", None, "rs");
        assert!(view.contains("a &lt; b;"), "{view}");
        assert!(view.contains("id=\"L1\""), "{view}");
        assert!(view.contains("id=\"L3\""), "{view}");
        assert!(view.contains("href=\"#L2\""), "{view}");
        assert!(view.contains("data-copy"), "{view}");
    }

    #[test]
    fn an_empty_source_still_has_one_line() {
        let view = source_view("", None, "");
        assert!(view.contains("id=\"L1\""), "{view}");
    }

    #[test]
    fn the_gutter_matches_the_line_count_of_the_source() {
        let view = source_view("one\ntwo\nthree\n", None, "txt");
        assert_eq!(view.matches("class=\"line\"").count(), 3, "{view}");
        assert_eq!(view.matches("<a href=\"#L").count(), 3, "{view}");
        assert_eq!(
            view.matches("</a>\n").count(),
            0,
            "a newline between numbers would add an empty line"
        );
    }
}
