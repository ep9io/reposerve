//! One function per kind of page.
//!
//! Each function builds the body and hands it to the shell, so the chrome and
//! the body never drift apart.

use std::fmt::Write as _;

use crate::site::html::{escape, href, size_label};
use crate::site::index::{Listing, TreeNode};
use crate::site::render::{source_view, Document};

use super::pager;
use super::shell::{self, Page, View};
use super::sidebar;

/// The label above the contents of a document.
const ON_THIS_PAGE: &str = "On this page";
/// The label above the contents of a notebook.
const IN_THIS_NOTEBOOK: &str = "In this notebook";

/// The parts every view shares.
pub struct Rendered<'a> {
    /// The files the reader's filter shows.
    pub listing: &'a Listing<'a>,
    /// The reader's choices.
    pub view: View<'a>,
    /// The path the page shows.
    pub current: &'a str,
    /// The title in the browser tab.
    pub title: String,
    /// The controls beside the breadcrumb.
    pub actions: String,
}

impl Rendered<'_> {
    fn page<'a>(
        &'a self,
        headings: &'a [crate::site::render::Heading],
        toc_label: &'a str,
        pager: &'a str,
    ) -> Page<'a> {
        Page {
            title: &self.title,
            current: self.current,
            listing: self.listing,
            view: self.view.clone(),
            actions: &self.actions,
            pager,
            headings,
            toc_label,
            syntax: false,
            math: false,
            mermaid: false,
        }
    }

    /// The bar of previous and next files for this path.
    fn pager(&self) -> String {
        let (previous, next) = self.listing.neighbours(self.current);
        pager::nav(previous, next)
    }
}

/// One Markdown document, rendered.
#[must_use]
pub fn document(context: &Rendered<'_>, document: &Document) -> String {
    let mut body = String::from("<article class=\"md\">");
    body.push_str(&document.html);
    body.push_str("</article>");
    let pager = context.pager();
    let mut page = context.page(&document.headings, ON_THIS_PAGE, &pager);
    page.syntax = document.has_syntax;
    page.math = document.has_math;
    page.mermaid = document.has_mermaid;
    shell::page(&page, &body)
}

/// One Jupyter notebook, rendered read-only.
#[must_use]
pub fn notebook(context: &Rendered<'_>, rendered: &crate::site::notebook::Rendered) -> String {
    let mut body = String::from("<article class=\"md nb\">");
    body.push_str(&rendered.html);
    body.push_str("</article>");
    let pager = context.pager();
    let mut page = context.page(&rendered.headings, IN_THIS_NOTEBOOK, &pager);
    page.syntax = rendered.has_syntax;
    page.math = rendered.has_math;
    page.mermaid = rendered.has_mermaid;
    shell::page(&page, &body)
}

/// One source file, numbered.
#[must_use]
pub fn source(
    context: &Rendered<'_>,
    source: &str,
    highlighted: Option<&str>,
    size: usize,
) -> String {
    let lines = source.lines().count();
    let mut body = String::new();
    let _ = write!(
        body,
        "<p class=\"meta\">{} {} · {}</p>",
        lines,
        if lines == 1 { "line" } else { "lines" },
        size_label(size)
    );
    body.push_str(&source_view(source, highlighted, &kind(context.current)));
    let pager = context.pager();
    let mut page = context.page(&[], ON_THIS_PAGE, &pager);
    page.syntax = highlighted.is_some();
    shell::page(&page, &body)
}

/// A file that is not text: its size, and nothing else.
#[must_use]
pub fn binary(context: &Rendered<'_>, size: usize) -> String {
    let mut body = String::new();
    let _ = write!(
        body,
        "<p class=\"meta\">{}. This file is not text, so the server does not display it.</p>\
         <p><a href=\"/{}?raw=1\">Download the file</a></p>",
        size_label(size),
        href(context.current)
    );
    shell::page(&context.page(&[], ON_THIS_PAGE, &context.pager()), &body)
}

/// An image, shown in the page rather than in the browser's own viewer.
#[must_use]
pub fn image(context: &Rendered<'_>, size: usize) -> String {
    let name = context
        .current
        .rsplit('/')
        .next()
        .unwrap_or(context.current);
    let mut body = String::new();
    let _ = write!(body, "<p class=\"meta\">{}</p>", size_label(size));
    let _ = write!(
        body,
        "<figure class=\"figure\"><a href=\"/{}?raw=1\" title=\"Open the original\">\
         <img src=\"/{}?raw=1\" alt=\"{}\"></a></figure>",
        href(context.current),
        href(context.current),
        escape(name)
    );
    shell::page(&context.page(&[], ON_THIS_PAGE, &context.pager()), &body)
}

/// The landing page and every directory listing.
#[must_use]
pub fn directory(context: &Rendered<'_>, directory: &TreeNode) -> String {
    let mut body = String::new();
    let _ = write!(
        body,
        "<h1>{}</h1>",
        escape(if context.current.is_empty() {
            &context.title
        } else {
            context.current
        })
    );
    if context.current.is_empty() {
        let _ = write!(
            body,
            "<p class=\"meta\">{} files in this repository.</p>",
            context.listing.len()
        );
    }
    if directory.dirs.is_empty() && directory.files.is_empty() {
        body.push_str(
            "<p class=\"empty\">No file of the types you chose is in this directory. \
             <a href=\"/filter\">Change the file types</a>.</p>",
        );
    } else {
        body.push_str("<ul class=\"listing\">");
        for child in &directory.dirs {
            let _ = write!(
                body,
                "<li><a href=\"/{}/\"><span class=\"name\">{}/</span>\
                 <span class=\"kind\">directory</span></a></li>",
                href(&child.path),
                escape(&child.name)
            );
        }
        for (name, path) in &directory.files {
            let _ = write!(
                body,
                "<li><a href=\"/{}\"><span class=\"name\">{}</span>\
                 <span class=\"kind\">{}</span></a></li>",
                href(path),
                escape(name),
                escape(&kind(name))
            );
        }
        body.push_str("</ul>");
    }
    shell::page(&context.page(&[], ON_THIS_PAGE, ""), &body)
}

/// Search results for one query.
#[must_use]
pub fn search(context: &Rendered<'_>, query: &str, matches: &[&str]) -> String {
    let mut body = String::new();
    let _ = write!(body, "<h1>Results for “{}”</h1>", escape(query));
    let _ = write!(
        body,
        "<p class=\"meta\">{} of {} files match.</p>",
        matches.len(),
        context.listing.len()
    );
    if matches.is_empty() {
        body.push_str(
            "<p class=\"empty\">No file path contains that text. The search matches file \
             names and paths, not file content.</p>",
        );
    } else {
        body.push_str("<ul class=\"listing\">");
        for path in matches {
            let _ = write!(
                body,
                "<li><a href=\"/{}\"><span class=\"name\">{}</span>\
                 <span class=\"kind\">{}</span></a></li>",
                href(path),
                escape(path),
                escape(&kind(path))
            );
        }
        body.push_str("</ul>");
    }
    shell::page(&context.page(&[], ON_THIS_PAGE, ""), &body)
}

/// A path that the index does not list.
#[must_use]
pub fn not_found(context: &Rendered<'_>, path: &str) -> String {
    let body = format!(
        "<h1>Not found</h1>\n<p><code>{}</code> is not part of the repository content. \
         Ignored and generated files are never served.</p>\n\
         <p><a href=\"/\">Back to the root</a></p>",
        escape(path)
    );
    shell::page(&context.page(&[], ON_THIS_PAGE, ""), &body)
}

/// The prompt shown when a token is configured and the request carries none.
#[must_use]
pub fn token(context: &Rendered<'_>) -> String {
    let body = "<h1>Token required</h1>\n\
<p>This server asks for the token it was started with.</p>\n\
<form class=\"token-form\" action=\"\" method=\"get\">\n\
<label for=\"token\">Token</label>\n\
<input id=\"token\" type=\"password\" name=\"token\" autocomplete=\"current-password\" \
enterkeyhint=\"go\" required>\n\
<button type=\"submit\">Open</button>\n\
</form>"
        .to_string();
    shell::page(&context.page(&[], ON_THIS_PAGE, ""), &body)
}

/// The note a page shows when the filter hides everything.
#[must_use]
pub fn empty_filter(context: &Rendered<'_>) -> String {
    let types = sidebar::offered(context.listing, &context.view);
    let documents = sidebar::document_types(&types);
    // Documents are what a reader comes for. A repository with none still gets
    // a way out of an empty filter.
    let (label, extensions) = if documents.is_empty() {
        (
            "Show every type",
            types.keys().cloned().collect::<Vec<String>>(),
        )
    } else {
        ("Show Documents", documents)
    };
    let link = if extensions.is_empty() {
        String::new()
    } else {
        format!(
            "<p><a href=\"/filter/apply?ext={}&amp;next=/\">{label}</a></p>",
            escape(&extensions.join(","))
        )
    };
    let body = format!(
        "<h1>No file type is shown</h1>\n\
         <p>Every file type is unticked, so the tree and the pages show nothing.</p>\n\
         {link}"
    );
    shell::page(&context.page(&[], ON_THIS_PAGE, ""), &body)
}

/// The kind of a file, from its extension.
fn kind(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, extension)) if !extension.is_empty() => extension.to_ascii_lowercase(),
        _ => "file".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::site::index::{Extensions, RepoIndex};
    use crate::web::theme::Theme;
    use std::path::Path;

    fn index() -> RepoIndex {
        RepoIndex::from_listing(Path::new("."), &["README.md", "docs/README.md"], &[])
    }

    fn context<'a>(listing: &'a Listing<'a>, config: &'a Config) -> Rendered<'a> {
        Rendered {
            listing,
            view: View {
                theme: Theme::Auto,
                extensions: Extensions::parse("md"),
                reading: false,
                width: config.reading_width,
                config,
            },
            current: "README.md",
            title: "README.md".to_string(),
            actions: String::new(),
        }
    }

    #[test]
    fn a_document_page_shows_its_headings_and_offers_the_source() {
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let config = Config::default();
        let doc = crate::site::render::markdown(
            "# Title\n\nText.\n\n## Part\n\nMore.\n",
            crate::site::render::Options::default(),
        );
        let html = document(&context(&listing, &config), &doc);
        assert!(html.contains("class=\"toc\""), "{html}");
        assert!(html.contains("href=\"#part\""), "{html}");
        assert!(html.contains("<article class=\"md\">"), "{html}");
    }

    #[test]
    fn a_directory_page_counts_its_files() {
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let config = Config::default();
        let mut context = context(&listing, &config);
        context.current = "";
        context.title = "files".to_string();
        let html = directory(&context, listing.tree());
        assert!(html.contains("2 files in this repository"), "{html}");
        assert!(html.contains("class=\"kind\">directory<"), "{html}");
    }

    #[test]
    fn a_file_page_does_not_repeat_its_path_as_a_heading() {
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let config = Config::default();
        let mut context = context(&listing, &config);
        context.current = "docs/lib.rs";
        assert!(!source(&context, "fn main() {}\n", None, 15).contains("<h1>"));
        assert!(!binary(&context, 15).contains("<h1>"));
        context.current = "media/chart.png";
        assert!(!image(&context, 15).contains("<h1>"));
    }

    #[test]
    fn an_empty_filter_offers_a_way_back() {
        let index = index();
        let listing = index.listing(&Extensions::parse(""));
        let config = Config::default();
        let html = empty_filter(&context(&listing, &config));
        assert!(html.contains("No file type is shown"), "{html}");
        assert!(html.contains("Show Documents"), "{html}");
        assert!(
            html.contains("ext=ipynb,md&amp;next=/"),
            "the offer names every document type: {html}"
        );
    }
}
