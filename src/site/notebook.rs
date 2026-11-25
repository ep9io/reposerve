//! Read-only rendering of Jupyter notebooks.
//!
//! Cells are rendered in order: Markdown through the same renderer the documents
//! use, code through the same highlighter. Outputs are shown as stored, with
//! caps, because a notebook can be several megabytes of results.
//!
//! A stored HTML output is sanitised and placed in the page. The sanitiser
//! removes every script, every event handler and every remote resource, so a
//! notebook cannot act against the reader. The page's content security policy
//! backs that up.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use serde_json::Value;

use crate::site::highlight;
use crate::site::html::escape;
use crate::site::render::{self, Options, COPY_BUTTON};

/// Largest single text output, in bytes.
const OUTPUT_LIMIT: usize = 200 * crate::site::html::KIB;

/// Largest total of rendered output, in bytes.
const TOTAL_LIMIT: usize = 2 * 1024 * crate::site::html::KIB;

/// Lines of output a reader sees before the rest folds away.
const VISIBLE_LINES: usize = 30;

/// One rendered notebook and the facts a page needs about it.
#[derive(Debug, Default)]
pub struct Rendered {
    /// The body HTML.
    pub html: String,
    /// Whether the body holds math.
    pub has_math: bool,
    /// Whether the body holds a diagram.
    pub has_mermaid: bool,
    /// Whether the body holds highlighted code.
    pub has_syntax: bool,
    /// The headings of the Markdown cells, in page order, for the outline.
    pub headings: Vec<render::Heading>,
}

/// Renders a notebook to the body of one page.
///
/// # Errors
///
/// Returns the parse failure when the file is not valid notebook JSON, so the
/// caller can fall back to showing it as source.
pub fn render(source: &str, options: Options) -> Result<Rendered, serde_json::Error> {
    let notebook: Notebook = serde_json::from_str(source)?;
    Ok(render_notebook(&notebook, options))
}

fn render_notebook(notebook: &Notebook, options: Options) -> Rendered {
    let language = notebook.language();
    let mut rendered = Rendered {
        html: String::with_capacity(notebook.cells.len() * 512),
        ..Rendered::default()
    };
    let mut budget = TOTAL_LIMIT;

    for (index, cell) in notebook.cells.iter().enumerate() {
        let anchor = index + 1;
        match cell.cell_type.as_str() {
            "markdown" => {
                let source = with_attachments(&cell.source, &cell.attachments);
                let html = absorb(&mut rendered, render::markdown(&source, options));
                let _ = write!(
                    rendered.html,
                    "<section class=\"nb-cell md\" id=\"cell-{anchor}\">{html}</section>"
                );
            }
            "code" => {
                let _ = write!(
                    rendered.html,
                    "<section class=\"nb-cell nb-code\" id=\"cell-{anchor}\">"
                );
                let count = cell
                    .execution_count
                    .map_or_else(|| " ".to_string(), |count| count.to_string());
                let _ = write!(
                    rendered.html,
                    "<p class=\"nb-in\"><a class=\"nb-in-link\" href=\"#cell-{anchor}\">In [{count}]:</a></p>"
                );
                let highlighted = highlight::global().for_language(&language, &cell.source);
                rendered.has_syntax |= highlighted.is_some();
                rendered.html.push_str("<div class=\"copyable\">");
                rendered.html.push_str(COPY_BUTTON);
                match highlighted {
                    Some(html) => {
                        let _ = write!(
                            rendered.html,
                            "<pre class=\"nb-source\"><code>{html}</code></pre>"
                        );
                    }
                    None => {
                        let _ = write!(
                            rendered.html,
                            "<pre class=\"nb-source\"><code>{}</code></pre>",
                            escape(&cell.source)
                        );
                    }
                }
                rendered.html.push_str("</div>");
                for output in &cell.outputs {
                    if let Some(html) = render_output(output, &mut budget, &mut rendered) {
                        rendered.html.push_str(&html);
                    }
                    if budget == 0 {
                        rendered.html.push_str(
                            "<p class=\"nb-note\">Further output omitted: the notebook exceeds \
                             the page budget.</p>",
                        );
                        break;
                    }
                }
                rendered.html.push_str("</section>");
            }
            "raw" => {
                let _ = write!(
                    rendered.html,
                    "<section class=\"nb-cell\" id=\"cell-{anchor}\"><pre class=\"nb-source\">\
                     <code>{}</code></pre></section>",
                    escape(&cell.source)
                );
            }
            _ => {}
        }
    }

    if rendered.html.is_empty() {
        rendered
            .html
            .push_str("<p class=\"empty\">This notebook has no cells.</p>");
    }
    rendered
}

fn render_output(output: &Output, budget: &mut usize, rendered: &mut Rendered) -> Option<String> {
    let html = match output.output_type.as_str() {
        "stream" => {
            let (text, truncated) = clip(&output.text, OUTPUT_LIMIT);
            let class = if output.name == "stderr" {
                "nb-out nb-error"
            } else {
                "nb-out nb-stream"
            };
            let mut html = text_block(class, &text);
            if truncated {
                html.push_str(truncation_note());
            }
            html
        }
        "error" => {
            let text = if output.traceback.is_empty() {
                format!("{}: {}", output.ename, output.evalue)
            } else {
                output.traceback.join("\n")
            };
            let (text, _) = clip(&strip_ansi(&text), OUTPUT_LIMIT);
            format!(
                "<pre class=\"nb-out nb-error\"><code>{}</code></pre>",
                escape(&text)
            )
        }
        "execute_result" | "display_data" => data_output(&output.data, rendered)?,
        "clear_output" => return None,
        _ => return None,
    };
    if html.len() > *budget {
        *budget = 0;
        return None;
    }
    *budget -= html.len();
    Some(html)
}

/// A text output, folded when it is long.
fn text_block(class: &str, text: &str) -> String {
    let lines = text.lines().count();
    let body = format!("<pre class=\"{class}\"><code>{}</code></pre>", escape(text));
    if lines <= VISIBLE_LINES {
        return body;
    }
    format!("<details class=\"nb-fold\"><summary>{lines} lines of output</summary>{body}</details>")
}

/// Chooses the richest representation a browser can show safely.
fn data_output(data: &BTreeMap<String, Value>, rendered: &mut Rendered) -> Option<String> {
    if let Some(html) = data.get("text/html") {
        let text = value_text(html);
        let (text, truncated) = clip(&text, OUTPUT_LIMIT);
        let mut output = format!("<div class=\"nb-out nb-html\">{}</div>", sanitise(&text));
        if truncated {
            output.push_str(truncation_note());
        }
        return Some(output);
    }
    if let Some(markdown) = data.get("text/markdown") {
        let html = absorb(
            rendered,
            render::markdown(&value_text(markdown), Options::default()),
        );
        return Some(format!("<div class=\"nb-out md\">{html}</div>"));
    }
    if let Some(latex) = data.get("text/latex") {
        rendered.has_math = true;
        return Some(format!(
            "<div class=\"nb-out\"><span class=\"math math-display\">{}</span></div>",
            escape(&undelimited(&value_text(latex)))
        ));
    }
    if let Some(image) = data.get("image/png") {
        return Some(image_tag("image/png", image));
    }
    if let Some(image) = data.get("image/jpeg") {
        return Some(image_tag("image/jpeg", image));
    }
    if let Some(image) = data.get("image/gif") {
        return Some(image_tag("image/gif", image));
    }
    if let Some(image) = data.get("image/svg+xml") {
        let source = value_text(image);
        let uri = format!(
            "data:image/svg+xml;utf8,{}",
            utf8_percent_encode(&source, NON_ALPHANUMERIC)
        );
        return Some(format!(
            "<div class=\"nb-out\"><img loading=\"lazy\" src=\"{uri}\" alt=\"SVG output\"></div>"
        ));
    }
    if let Some(pdf) = data.get("application/pdf") {
        let encoded = strip_whitespace(&value_text(pdf));
        return Some(format!(
            "<p class=\"nb-out nb-note\"><a href=\"data:application/pdf;base64,{encoded}\">\
             Open the PDF output</a></p>"
        ));
    }
    for key in data.keys() {
        if key.starts_with("application/vnd.jupyter.widget") {
            return Some(
                "<p class=\"nb-out nb-note\">Interactive widget output, not shown in a \
                 read-only view.</p>"
                    .to_string(),
            );
        }
        if key.starts_with("application/vnd.plotly") || key.starts_with("application/vnd.vega") {
            return Some(
                "<p class=\"nb-out nb-note\">Interactive chart output, not shown in a \
                 read-only view.</p>"
                    .to_string(),
            );
        }
    }
    if let Some(plain) = data.get("text/plain") {
        let (text, truncated) = clip(&value_text(plain), OUTPUT_LIMIT);
        let mut output = text_block("nb-out", &text);
        if truncated {
            output.push_str(truncation_note());
        }
        return Some(output);
    }
    None
}

fn image_tag(mime: &str, value: &Value) -> String {
    let encoded = strip_whitespace(&value_text(value));
    format!(
        "<figure class=\"nb-out\"><img loading=\"lazy\" src=\"data:{mime};base64,{encoded}\" \
         alt=\"Image output\"></figure>"
    )
}

fn truncation_note() -> &'static str {
    "<p class=\"nb-note\">Output truncated at 200 KiB.</p>"
}

/// Keeps at most `limit` bytes, cut at a character boundary.
fn clip(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

fn strip_whitespace(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Removes the colour codes a traceback carries.
fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            for next in characters.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            output.push(character);
        }
    }
    output
}

/// Cleans a stored HTML output so it cannot act against the reader.
///
/// Scripts, event handlers, forms and remote resources are removed. Tables and
/// figures keep their structure and their classes, which is what a data frame
/// needs to look right.
fn sanitise(html: &str) -> String {
    sanitiser().clean(html).to_string()
}

fn sanitiser() -> &'static ammonia::Builder<'static> {
    static SANITISER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    SANITISER.get_or_init(|| {
        let mut builder = ammonia::Builder::default();
        builder
            .add_tags(&[
                "caption",
                "col",
                "colgroup",
                "details",
                "figure",
                "figcaption",
                "summary",
                "tbody",
                "td",
                "tfoot",
                "th",
                "thead",
                "tr",
                "table",
                "small",
                "u",
                "var",
            ])
            .add_generic_attributes(&[
                "class", "id", "style", "title", "colspan", "rowspan", "align", "scope", "width",
                "height", "start", "type",
            ])
            .add_tag_attributes("img", &["src", "alt", "width", "height"])
            .add_tag_attributes("td", &["colspan", "rowspan", "align"])
            .add_tag_attributes("th", &["colspan", "rowspan", "align", "scope"])
            .link_rel(Some("noopener noreferrer"))
            .url_schemes(["http", "https", "mailto", "data"].into_iter().collect())
            .attribute_filter(|element, attribute, value| match (element, attribute) {
                ("img", "src") if !value.starts_with("data:image/") => None,
                ("a", "href") if value.starts_with("data:") => None,
                _ => Some(value.into()),
            });
        builder
    })
}

/// Replaces `attachment:name` links in a Markdown cell with the stored image.
fn with_attachments(
    source: &str,
    attachments: &BTreeMap<String, BTreeMap<String, Value>>,
) -> String {
    let mut text = source.to_string();
    for (name, images) in attachments {
        let Some((mime, value)) = preferred_image(images) else {
            continue;
        };
        let uri = format!(
            "data:{mime};base64,{}",
            strip_whitespace(&value_text(value))
        );
        text = text.replace(&format!("](attachment:{name})"), &format!("]({uri})"));
    }
    text
}

fn preferred_image(images: &BTreeMap<String, Value>) -> Option<(&'static str, &Value)> {
    for mime in ["image/png", "image/jpeg", "image/gif"] {
        if let Some(value) = images.get(mime) {
            return Some((mime, value));
        }
    }
    None
}

/// Adds one rendered Markdown block to the page and returns its HTML.
///
/// The outline lists the headings of every Markdown cell. An identifier is
/// unique inside one cell, not across the page, so a second cell with the same
/// heading text would repeat an identifier. A duplicate gets a number, and the
/// body follows, so both links reach their own heading.
fn absorb(rendered: &mut Rendered, mut document: render::Document) -> String {
    rendered.has_math |= document.has_math;
    rendered.has_mermaid |= document.has_mermaid;
    rendered.has_syntax |= document.has_syntax;
    for heading in std::mem::take(&mut document.headings) {
        let id = unique_id(&heading.id, &rendered.headings);
        if id != heading.id {
            document.html = document.html.replacen(
                &format!("id=\"{}\"", heading.id),
                &format!("id=\"{id}\""),
                1,
            );
        }
        rendered.headings.push(render::Heading { id, ..heading });
    }
    document.html
}

/// `base` when the page does not use it, or the first `base-n` that is free.
fn unique_id(base: &str, taken: &[render::Heading]) -> String {
    let used = |candidate: &str| taken.iter().any(|heading| heading.id == candidate);
    if !used(base) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !used(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// A JSON value that is either one string or a list of them.
fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(lines) => lines
            .iter()
            .map(|line| {
                if let Value::String(text) = line {
                    text.clone()
                } else {
                    line.to_string()
                }
            })
            .collect(),
        Value::Null => String::new(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Object(map) => Value::Object(map.clone()).to_string(),
    }
}

/// The body of a LaTeX string, without the delimiters that wrap it.
///
/// A notebook writes `$$…$$` or `\[…\]` for a block and `$…$` or `\(…\)` for an
/// inline expression. The page draws the math itself, so the delimiters would
/// reach the browser as text.
fn undelimited(latex: &str) -> String {
    let trimmed = latex.trim();
    for (open, close) in [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")] {
        if let Some(rest) = trimmed.strip_prefix(open) {
            if let Some(body) = rest.strip_suffix(close) {
                return body.trim().to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Accepts the two shapes nbformat allows for text: a string, or lines.
fn string_or_lines<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value_text(&value))
}

fn default_string() -> String {
    String::new()
}

#[derive(Debug, Deserialize)]
struct Notebook {
    #[serde(default)]
    cells: Vec<Cell>,
    #[serde(default)]
    metadata: Metadata,
}

impl Notebook {
    fn language(&self) -> String {
        let metadata = &self.metadata;
        let name = if metadata.language_info.name.is_empty() {
            &metadata.kernelspec.language
        } else {
            &metadata.language_info.name
        };
        if name.is_empty() {
            "python".to_string()
        } else {
            name.clone()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct Metadata {
    #[serde(default)]
    language_info: LanguageInfo,
    #[serde(default)]
    kernelspec: KernelSpec,
}

#[derive(Debug, Default, Deserialize)]
struct LanguageInfo {
    #[serde(default = "default_string")]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct KernelSpec {
    #[serde(default = "default_string")]
    language: String,
}

#[derive(Debug, Deserialize)]
struct Cell {
    #[serde(default = "default_string")]
    cell_type: String,
    #[serde(default, deserialize_with = "string_or_lines")]
    source: String,
    #[serde(default)]
    execution_count: Option<u64>,
    #[serde(default)]
    outputs: Vec<Output>,
    #[serde(default)]
    attachments: BTreeMap<String, BTreeMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
struct Output {
    #[serde(default = "default_string")]
    output_type: String,
    #[serde(default = "default_string")]
    name: String,
    #[serde(default, deserialize_with = "string_or_lines")]
    text: String,
    #[serde(default = "default_string")]
    ename: String,
    #[serde(default = "default_string")]
    evalue: String,
    #[serde(default)]
    traceback: Vec<String>,
    #[serde(default)]
    data: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notebook(cells: &str) -> String {
        format!(
            "{{\"nbformat\":4,\"nbformat_minor\":5,\"metadata\":{{\"language_info\":{{\"name\":\"python\"}}}},\"cells\":[{cells}]}}"
        )
    }

    fn render(source: &str) -> Rendered {
        super::render(source, Options::default()).expect("a valid notebook")
    }

    #[test]
    fn a_markdown_cell_renders_as_html() {
        let source =
            notebook(r##"{"cell_type":"markdown","source":["# Title\n","Some *text*."]}"##);
        let rendered = render(&source);
        assert!(
            rendered.html.contains("<h1 id=\"title\">"),
            "{}",
            rendered.html
        );
        assert!(rendered.html.contains("<em>text</em>"), "{}", rendered.html);
    }

    #[test]
    fn a_code_cell_is_highlighted_and_numbered() {
        let source = notebook(
            r#"{"cell_type":"code","execution_count":7,"source":"def f():\n    return 1","outputs":[]}"#,
        );
        let rendered = render(&source);
        assert!(rendered.html.contains("In [7]:"), "{}", rendered.html);
        assert!(
            rendered.html.contains("hl-source hl-python"),
            "{}",
            rendered.html
        );
        assert!(rendered.has_syntax);
        assert!(rendered.html.contains("id=\"cell-1\""), "{}", rendered.html);
    }

    #[test]
    fn a_stream_output_is_shown_and_long_text_is_clipped() {
        let long = "x".repeat(OUTPUT_LIMIT + 10);
        let source = notebook(&format!(
            r#"{{"cell_type":"code","source":"print('x')","outputs":[{{"output_type":"stream","name":"stdout","text":"{long}"}}]}}"#
        ));
        let rendered = render(&source);
        assert!(rendered.html.contains("nb-stream"), "{}", rendered.html);
        assert!(
            rendered.html.contains("Output truncated"),
            "{}",
            rendered.html
        );
    }

    #[test]
    fn a_long_output_folds_away() {
        let long = (0..VISIBLE_LINES + 5)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\\n");
        let source = notebook(&format!(
            r#"{{"cell_type":"code","source":"x","outputs":[{{"output_type":"stream","name":"stdout","text":"{long}"}}]}}"#
        ));
        let rendered = render(&source);
        assert!(rendered.html.contains("nb-fold"), "{}", rendered.html);
        assert!(
            rendered.html.contains("lines of output"),
            "{}",
            rendered.html
        );
    }

    #[test]
    fn an_html_output_is_sanitised_in_place() {
        let source = notebook(
            r#"{"cell_type":"code","source":"df","outputs":[{"output_type":"execute_result","data":{"text/html":["<table class=\"dataframe\"><tr><td onclick=\"steal()\">1</td></tr></table><script>bad()</script><img src=\"https://tracker.example/x.png\">"]}}]}"#,
        );
        let rendered = render(&source);
        assert!(
            rendered.html.contains("<table class=\"dataframe\">"),
            "{}",
            rendered.html
        );
        assert!(!rendered.html.contains("<script"), "{}", rendered.html);
        assert!(!rendered.html.contains("onclick"), "{}", rendered.html);
        assert!(
            !rendered.html.contains("tracker.example"),
            "a remote image is removed: {}",
            rendered.html
        );
        assert!(!rendered.html.contains("<iframe"), "{}", rendered.html);
    }

    #[test]
    fn a_png_output_becomes_a_data_uri() {
        let source = notebook(
            r#"{"cell_type":"code","source":"plot()","outputs":[{"output_type":"display_data","data":{"image/png":"aGVsbG8="}}]}"#,
        );
        let rendered = render(&source);
        assert!(
            rendered
                .html
                .contains("src=\"data:image/png;base64,aGVsbG8=\""),
            "{}",
            rendered.html
        );
    }

    #[test]
    fn an_error_output_shows_its_traceback_without_colour_codes() {
        let source = notebook(
            r#"{"cell_type":"code","source":"1/0","outputs":[{"output_type":"error","ename":"ZeroDivisionError","evalue":"division by zero","traceback":["\u001b[0;31mZeroDivisionError\u001b[0m: division by zero"]}]}"#,
        );
        let rendered = render(&source);
        assert!(
            rendered.html.contains("ZeroDivisionError"),
            "{}",
            rendered.html
        );
        assert!(
            !rendered.html.contains('\u{1b}'),
            "the escape codes are stripped: {}",
            rendered.html
        );
    }

    #[test]
    fn a_widget_output_says_it_is_not_shown() {
        let source = notebook(
            r#"{"cell_type":"code","source":"w","outputs":[{"output_type":"display_data","data":{"application/vnd.jupyter.widget-view+json":{"model_id":"x"}}}]}"#,
        );
        let rendered = render(&source);
        assert!(
            rendered.html.contains("Interactive widget"),
            "{}",
            rendered.html
        );
    }

    #[test]
    fn latex_output_sets_the_math_flag() {
        let source = notebook(
            r#"{"cell_type":"code","source":"x","outputs":[{"output_type":"display_data","data":{"text/latex":"$a^2$"}}]}"#,
        );
        let rendered = render(&source);
        assert!(rendered.has_math);
        assert!(rendered.html.contains("math-display"), "{}", rendered.html);
        assert!(
            !rendered.html.contains("$a^2$"),
            "the delimiters would reach the browser as text: {}",
            rendered.html
        );
    }

    #[test]
    fn a_latex_output_loses_its_delimiters() {
        assert_eq!(undelimited("$$a^2$$"), "a^2");
        assert_eq!(undelimited("$a^2$"), "a^2");
        assert_eq!(undelimited("\\[a^2\\]"), "a^2");
        assert_eq!(undelimited("\\(a^2\\)"), "a^2");
        assert_eq!(undelimited("  a^2  "), "a^2", "a bare expression is kept");
    }

    #[test]
    fn a_markdown_cell_can_hold_a_diagram() {
        let source = notebook(
            r##"{"cell_type":"markdown","source":"```mermaid\ngraph TD\n A-->B\n```\n"}"##,
        );
        let rendered = render(&source);
        assert!(rendered.has_mermaid);
    }

    #[test]
    fn an_attachment_replaces_its_link() {
        let source = notebook(
            r#"{"cell_type":"markdown","source":"![shot](attachment:fig.png)","attachments":{"fig.png":{"image/png":"aGVsbG8="}}}"#,
        );
        let rendered = render(&source);
        assert!(
            rendered.html.contains("data:image/png;base64,aGVsbG8="),
            "{}",
            rendered.html
        );
    }

    #[test]
    fn invalid_json_is_reported_so_the_caller_can_show_the_source() {
        assert!(super::render("not a notebook", Options::default()).is_err());
    }
    #[test]
    fn markdown_cell_headings_become_the_outline() {
        let source = notebook(
            r###"{"cell_type":"markdown","source":["# First\n","text"]},
               {"cell_type":"code","source":"1","outputs":[]},
               {"cell_type":"markdown","source":["## Second\n","more"]}"###,
        );
        let rendered = render(&source);
        let texts: Vec<&str> = rendered
            .headings
            .iter()
            .map(|heading| heading.text.as_str())
            .collect();
        assert_eq!(texts, ["First", "Second"]);
        assert_eq!(rendered.headings[1].level, 2);
        assert!(rendered.html.contains("id=\"first\""), "{}", rendered.html);
    }

    #[test]
    fn a_repeated_heading_keeps_two_working_links() {
        let source = notebook(
            r###"{"cell_type":"markdown","source":["## Results"]},
               {"cell_type":"markdown","source":["## Results"]}"###,
        );
        let rendered = render(&source);
        let ids: Vec<&str> = rendered
            .headings
            .iter()
            .map(|heading| heading.id.as_str())
            .collect();
        assert_eq!(ids, ["results", "results-2"]);
        assert!(
            rendered.html.contains("id=\"results-2\""),
            "the second heading must carry its own identifier: {}",
            rendered.html
        );
    }
}
