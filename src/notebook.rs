//! Read-only rendering of Jupyter notebooks.
//!
//! Cells are rendered in order: Markdown through the same renderer the documents
//! use, code through the same highlighter. Outputs are shown as stored, with
//! caps, because a notebook can be several megabytes of results.
//!
//! An HTML output is placed in a sandboxed iframe. The frame carries
//! `allow-same-origin` so the page can measure it and size it, and nothing else:
//! a stored widget cannot run script against the reader's session.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use serde_json::Value;

use crate::highlight;
use crate::pages::escape;
use crate::render::markdown_to_html;

/// Largest single text output, in bytes.
const OUTPUT_LIMIT: usize = 200 * 1024;

/// Largest total of rendered output, in bytes.
const TOTAL_LIMIT: usize = 2 * 1024 * 1024;

/// Whether a path names a notebook.
#[must_use]
pub fn is_notebook(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("ipynb"))
}

/// Renders a notebook to the body of one page.
///
/// # Errors
///
/// Returns the parse failure when the file is not valid notebook JSON, so the
/// caller can fall back to showing it as source.
pub fn render(source: &str) -> Result<String, serde_json::Error> {
    let notebook: Notebook = serde_json::from_str(source)?;
    Ok(render_notebook(&notebook))
}

fn render_notebook(notebook: &Notebook) -> String {
    let language = notebook.language();
    let mut body = String::with_capacity(notebook.cells.len() * 512);
    let mut budget = TOTAL_LIMIT;

    for cell in &notebook.cells {
        match cell.cell_type.as_str() {
            "markdown" => {
                let source = with_attachments(&cell.source, &cell.attachments);
                let _ = write!(
                    body,
                    "<section class=\"nb-cell md\">{}</section>",
                    markdown_to_html(&source)
                );
            }
            "code" => {
                let _ = write!(body, "<section class=\"nb-cell nb-code\">");
                let count = cell
                    .execution_count
                    .map_or_else(|| " ".to_string(), |count| count.to_string());
                let _ = write!(body, "<p class=\"nb-in\">In [{count}]:</p>");
                let highlighted = highlight::global().for_language(&language, &cell.source);
                body.push_str("<div class=\"copyable\">");
                body.push_str(crate::pages::COPY_BUTTON);
                match highlighted {
                    Some(html) => {
                        let _ = write!(body, "<pre class=\"nb-source\"><code>{html}</code></pre>");
                    }
                    None => {
                        let _ = write!(
                            body,
                            "<pre class=\"nb-source\"><code>{}</code></pre>",
                            escape(&cell.source)
                        );
                    }
                }
                body.push_str("</div>");
                for output in &cell.outputs {
                    if let Some(html) = render_output(output, &mut budget) {
                        body.push_str(&html);
                    }
                    if budget == 0 {
                        body.push_str(
                            "<p class=\"nb-note\">Further output omitted: the notebook exceeds \
                             the page budget.</p>",
                        );
                        break;
                    }
                }
                body.push_str("</section>");
            }
            "raw" => {
                let _ = write!(
                    body,
                    "<section class=\"nb-cell\"><pre class=\"nb-source\"><code>{}</code></pre></section>",
                    escape(&cell.source)
                );
            }
            _ => {}
        }
    }

    if body.is_empty() {
        body.push_str("<p class=\"empty\">This notebook has no cells.</p>");
    }
    body
}

fn render_output(output: &Output, budget: &mut usize) -> Option<String> {
    let html = match output.output_type.as_str() {
        "stream" => {
            let (text, truncated) = clip(&output.text, OUTPUT_LIMIT);
            let class = if output.name == "stderr" {
                "nb-out nb-error"
            } else {
                "nb-out nb-stream"
            };
            let mut html = format!(
                "<pre class=\"{class}\"><code>{}</code></pre>",
                escape(&text)
            );
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
        "execute_result" | "display_data" => data_output(&output.data)?,
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

/// Chooses the richest representation a browser can show safely.
fn data_output(data: &BTreeMap<String, Value>) -> Option<String> {
    if let Some(html) = data.get("text/html") {
        let text = value_text(html);
        let (text, truncated) = clip(&text, OUTPUT_LIMIT);
        let mut output = format!(
            "<div class=\"nb-html-wrap\"><iframe class=\"nb-html\" sandbox=\"allow-same-origin\" \
             title=\"Notebook HTML output\" srcdoc=\"{}\"></iframe></div>",
            escape(&text)
        );
        if truncated {
            output.push_str(truncation_note());
        }
        return Some(output);
    }
    if let Some(markdown) = data.get("text/markdown") {
        return Some(format!(
            "<div class=\"nb-out md\">{}</div>",
            markdown_to_html(&value_text(markdown))
        ));
    }
    if let Some(image) = data.get("image/png") {
        return Some(image_tag("image/png", image, None));
    }
    if let Some(image) = data.get("image/jpeg") {
        return Some(image_tag("image/jpeg", image, None));
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
    }
    if let Some(plain) = data.get("text/plain") {
        let (text, truncated) = clip(&value_text(plain), OUTPUT_LIMIT);
        let mut output = format!("<pre class=\"nb-out\"><code>{}</code></pre>", escape(&text));
        if truncated {
            output.push_str(truncation_note());
        }
        return Some(output);
    }
    None
}

fn image_tag(mime: &str, value: &Value, caption: Option<&str>) -> String {
    let encoded = strip_whitespace(&value_text(value));
    let caption = caption.map_or_else(String::new, |caption| {
        format!("<figcaption>{}</figcaption>", escape(caption))
    });
    format!(
        "<figure class=\"nb-out\"><img loading=\"lazy\" src=\"data:{mime};base64,{encoded}\" \
         alt=\"Image output\">{caption}</figure>"
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

    #[test]
    fn a_markdown_cell_renders_as_html() {
        let source =
            notebook(r##"{"cell_type":"markdown","source":["# Title\n","Some *text*."]}"##);
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("<h1 id=\"title\">"), "{html}");
        assert!(html.contains("<em>text</em>"), "{html}");
    }

    #[test]
    fn a_code_cell_is_highlighted_and_numbered() {
        let source = notebook(
            r#"{"cell_type":"code","execution_count":7,"source":"def f():\n    return 1","outputs":[]}"#,
        );
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("In [7]:"), "{html}");
        assert!(html.contains("hl-source hl-python"), "{html}");
    }

    #[test]
    fn a_stream_output_is_shown_and_long_text_is_clipped() {
        let long = "x".repeat(OUTPUT_LIMIT + 10);
        let source = notebook(&format!(
            r#"{{"cell_type":"code","source":"print('x')","outputs":[{{"output_type":"stream","name":"stdout","text":"{long}"}}]}}"#
        ));
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("nb-stream"), "{html}");
        assert!(html.contains("Output truncated"), "{html}");
    }

    #[test]
    fn an_html_output_goes_into_a_sandboxed_frame() {
        let source = notebook(
            r#"{"cell_type":"code","source":"df","outputs":[{"output_type":"execute_result","data":{"text/html":["<table><tr><td>1</td></tr></table>"],"text/plain":"1"}}]}"#,
        );
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("sandbox=\"allow-same-origin\""), "{html}");
        assert!(
            html.contains("&lt;table&gt;"),
            "the HTML is escaped into srcdoc: {html}"
        );
        assert!(!html.contains("<script"), "{html}");
    }

    #[test]
    fn a_png_output_becomes_a_data_uri() {
        let source = notebook(
            r#"{"cell_type":"code","source":"plot()","outputs":[{"output_type":"display_data","data":{"image/png":"aGVsbG8="}}]}"#,
        );
        let html = render(&source).expect("valid notebook");
        assert!(
            html.contains("src=\"data:image/png;base64,aGVsbG8=\""),
            "{html}"
        );
    }

    #[test]
    fn an_error_output_shows_its_traceback_without_colour_codes() {
        let source = notebook(
            r#"{"cell_type":"code","source":"1/0","outputs":[{"output_type":"error","ename":"ZeroDivisionError","evalue":"division by zero","traceback":["\u001b[0;31mZeroDivisionError\u001b[0m: division by zero"]}]}"#,
        );
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("ZeroDivisionError"), "{html}");
        assert!(
            !html.contains('\u{1b}'),
            "the escape codes are stripped: {html}"
        );
    }

    #[test]
    fn a_widget_output_says_it_is_not_shown() {
        let source = notebook(
            r#"{"cell_type":"code","source":"w","outputs":[{"output_type":"display_data","data":{"application/vnd.jupyter.widget-view+json":{"model_id":"x"}}}]}"#,
        );
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("Interactive widget"), "{html}");
    }

    #[test]
    fn an_attachment_replaces_its_link() {
        let source = notebook(
            r#"{"cell_type":"markdown","source":"![shot](attachment:fig.png)","attachments":{"fig.png":{"image/png":"aGVsbG8="}}}"#,
        );
        let html = render(&source).expect("valid notebook");
        assert!(html.contains("data:image/png;base64,aGVsbG8="), "{html}");
    }

    #[test]
    fn invalid_json_is_reported_so_the_caller_can_show_the_source() {
        assert!(render("not a notebook").is_err());
    }

    #[test]
    fn only_a_json_notebook_counts_as_one() {
        assert!(is_notebook("analysis/report.ipynb"));
        assert!(is_notebook("notes.IPYNB"));
        assert!(!is_notebook("docs/README.md"));
    }
}
