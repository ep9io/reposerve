//! The page shell: head, header, sidebar, content and table of contents.
//!
//! Every page is built here, so the chrome is the same everywhere and a change
//! to it lands on every view at once.

use std::fmt::Write as _;

use crate::config::Config;
use crate::site::html::{escape, href, query_value};
use crate::site::index::{Extensions, Listing};
use crate::site::render::Heading;
use crate::web::assets;
use crate::web::theme::Theme;

use super::sidebar;
use super::toc;

/// The choices a request carries: the colour scheme, the filter and the
/// reading mode.
#[derive(Clone, Debug)]
pub struct View<'a> {
    /// The colour scheme the reader chose.
    pub theme: Theme,
    /// The file extensions the reader asked to see.
    pub extensions: Extensions,
    /// Whether the reader hid the chrome.
    pub reading: bool,
    /// The configuration this server runs with.
    pub config: &'a Config,
}

/// Everything one page needs besides its body.
pub struct Page<'a> {
    /// The title in the browser tab, without the site name.
    pub title: &'a str,
    /// The path the page shows, used for the breadcrumb and the active link.
    pub current: &'a str,
    /// The files the reader's filter shows.
    pub listing: &'a Listing<'a>,
    /// The reader's choices.
    pub view: View<'a>,
    /// The controls that belong to this page, at the right of the breadcrumb.
    pub actions: &'a str,
    /// The headings of the page, for the table of contents.
    pub headings: &'a [Heading],
    /// What the table of contents holds: a page, or a notebook.
    pub toc_label: &'a str,
    /// Whether the body holds highlighted code.
    pub syntax: bool,
    /// Whether the body holds math.
    pub math: bool,
    /// Whether the body holds a diagram.
    pub mermaid: bool,
}

/// Builds one complete page.
#[must_use]
pub fn page(page: &Page<'_>, body: &str) -> String {
    let brand = escape(&page.view.config.title);
    let title = if page.title.is_empty() || page.title == page.view.config.title {
        brand.clone()
    } else {
        format!("{} · {brand}", escape(page.title))
    };
    let target = format!("/{}", page.current);
    let next_target = query_value(&target);

    let highlight = if page.syntax {
        format!(
            "<link rel=\"stylesheet\" href=\"/highlight.css?v={}\">",
            crate::site::highlight::global().fingerprint()
        )
    } else {
        String::new()
    };

    let mut scripts = String::new();
    if page.mermaid {
        let _ = write!(
            scripts,
            "<script src=\"{}\" data-lib=\"mermaid\" defer></script>",
            assets::url(assets::MERMAID)
        );
    }
    if page.math {
        let _ = write!(
            scripts,
            "<link rel=\"stylesheet\" href=\"{}\"><script src=\"{}\" data-lib=\"katex\" defer></script>",
            assets::url(assets::KATEX_CSS),
            assets::url(assets::KATEX_JS)
        );
    }

    let toc = if page.view.config.toc && page.headings.len() > 1 {
        toc::aside(page.headings, page.toc_label)
    } else {
        String::new()
    };
    let shell = if toc.is_empty() {
        "shell no-toc"
    } else {
        "shell"
    };
    let reading_class = if page.view.reading { "on" } else { "off" };
    let theme_menu = theme_links(page.view.theme, &next_target);

    format!(
        "<!DOCTYPE html>\n\
<html lang=\"en\" data-theme=\"{theme}\" data-reading=\"{reading}\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n\
<meta name=\"color-scheme\" content=\"light dark\">\n\
<meta name=\"theme-color\" content=\"#ffffff\" media=\"(prefers-color-scheme: light)\">\n\
<meta name=\"theme-color\" content=\"#0f1115\" media=\"(prefers-color-scheme: dark)\">\n\
<meta name=\"description\" content=\"{description}\">\n\
<title>{title}</title>\n\
<style>:root{{--measure:{measure}rem}}</style>\n\
<link rel=\"stylesheet\" href=\"{stylesheet}\">\n\
{highlight}\n\
<link rel=\"icon\" href=\"{icon}\" type=\"image/svg+xml\">\n\
<link rel=\"manifest\" href=\"/manifest.webmanifest\">\n\
</head>\n\
<body>\n\
<a class=\"skip\" href=\"#main\">Skip to content</a>\n\
<input type=\"checkbox\" id=\"nav-toggle\" class=\"nav-toggle\">\n\
<header class=\"bar\">\n\
<a class=\"brand\" href=\"/\">{brand}</a>\n\
<form class=\"find\" action=\"/search\" method=\"get\" role=\"search\">\n\
<input type=\"search\" name=\"q\" placeholder=\"Find a file\" aria-label=\"Find a file in the repository\" autocomplete=\"off\" spellcheck=\"false\" enterkeyhint=\"search\">\n\
</form>\n\
<details class=\"menu theme-menu\">\n\
<summary class=\"icon-button\" role=\"button\" aria-label=\"Theme: {theme_label}\" title=\"Theme: {theme_label}\">{theme_icon}</summary>\n\
<div class=\"menu-list\" role=\"group\" aria-label=\"Colour scheme\">\n\
{theme_links}\n\
</div>\n\
</details>\n\
<button class=\"icon-button\" id=\"reading-toggle\" type=\"button\" aria-pressed=\"{reading}\" aria-label=\"Reading mode\" title=\"Reading mode (f)\">{reading_icon}</button>\n\
<label class=\"icon-button nav-button\" for=\"nav-toggle\" aria-label=\"Show the file list\" title=\"File list\">{files_icon}</label>\n\
</header>\n\
<div class=\"{shell}\">\n\
<nav class=\"nav\" id=\"nav\" aria-label=\"Repository files\">\n\
<p class=\"nav-label\">Files</p>\n\
{filter}\n\
{tree}\n\
</nav>\n\
<main id=\"main\" tabindex=\"-1\">\n\
<div class=\"page-head\">{crumbs}{actions}</div>\n\
{body}\n\
<footer class=\"page-foot\"><a href=\"/\">All files</a></footer>\n\
</main>\n\
{toc}\n\
</div>\n\
<div class=\"reader\" id=\"reader\">\n\
<details class=\"menu reader-menu\">\n\
<summary class=\"icon-button reader-button\" role=\"button\" aria-label=\"Reader options\" title=\"Reader options\">{reader_icon}</summary>\n\
<div class=\"menu-list reader-list\" role=\"group\" aria-label=\"Reader options\">\n\
<a class=\"reader-leave\" href=\"/reading/off?next={next_target}\"><span>Leave reading mode</span><kbd>f</kbd></a>\n\
<p class=\"menu-label\">Colour scheme</p>\n\
{theme_menu}\n\
</div>\n\
</details>\n\
</div>\n\
<div class=\"chip\" id=\"update-chip\" hidden>This file changed. <button type=\"button\" data-reload>Reload</button></div>\n\
<div class=\"findbar\" id=\"findbar\" hidden>\n\
<label for=\"find-input\">Find in page</label>\n\
<input id=\"find-input\" type=\"search\" autocomplete=\"off\" spellcheck=\"false\" enterkeyhint=\"search\">\n\
<span class=\"count\" id=\"find-count\" aria-live=\"polite\"></span>\n\
<button type=\"button\" data-find=\"previous\" aria-label=\"Previous match\">↑</button>\n\
<button type=\"button\" data-find=\"next\" aria-label=\"Next match\">↓</button>\n\
<button type=\"button\" data-find=\"close\" aria-label=\"Close find\">×</button>\n\
</div>\n\
<dialog class=\"palette\" id=\"palette\" aria-label=\"Find a file\">\n\
<input type=\"search\" id=\"palette-input\" placeholder=\"Find a file\" aria-label=\"Find a file\" autocomplete=\"off\" spellcheck=\"false\">\n\
<ul id=\"palette-results\" role=\"listbox\" aria-label=\"Matching files\"></ul>\n\
</dialog>\n\
<script src=\"{script}\" defer></script>\n\
{scripts}\n\
</body>\n\
</html>\n",
        theme = page.view.theme.value(),
        reading = reading_class,
        description = escape(&format!("{} files from {}", page.listing.len(), page.view.config.title)),
        measure = page.view.config.reading_width,
        stylesheet = assets::url(assets::APP_CSS),
        icon = assets::url(assets::ICON),
        script = assets::url(assets::APP_JS),
        theme_label = page.view.theme.label(),
        theme_icon = theme_icon(),
        reading_icon = reading_icon(),
        files_icon = files_icon(),
        theme_links = theme_menu,
        reader_icon = reader_icon(),
        next_target = next_target,
        filter = sidebar::filter(&page.view, page.listing, page.current),
        tree = sidebar::tree(page.listing, page.current),
        crumbs = breadcrumb(page.current),
        actions = page.actions,
        body = body,
        toc = toc,
        scripts = scripts,
        highlight = highlight,
    )
}

fn theme_links(current: Theme, target: &str) -> String {
    let mut output = String::new();
    for theme in Theme::ALL {
        let marked = if theme == current {
            " aria-current=\"true\""
        } else {
            ""
        };
        let _ = write!(
            output,
            "<a href=\"/theme/{}?next={target}\" class=\"theme-option\"{marked}>{}</a>",
            theme.value(),
            theme.label()
        );
    }
    output
}

fn theme_icon() -> &'static str {
    // One icon for the appearance menu. The menu itself says which choice is
    // active, so the icon does not have to change and cannot fall out of step.
    "<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.7\"><circle cx=\"12\" cy=\"12\" r=\"8\"/><path d=\"M12 4v16\"/><path d=\"M12 4a8 8 0 0 1 0 16z\" fill=\"currentColor\" stroke=\"none\"/></svg>"
}

fn reading_icon() -> &'static str {
    "<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.7\"><path d=\"M4 5.5h16M4 18.5h16\"/><path d=\"M8.5 9.5h7M8.5 12h7M8.5 14.5h4.5\"/></svg>"
}

/// The control that leaves reading mode.
///
/// The page hides its header in reading mode, so this control floats over the
/// text with the corners pointing inward. It is a `details` element, so it works
/// without a script, and the exit is a plain link to `/reading/off`.
fn reader_icon() -> &'static str {
    "<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.7\"><path d=\"M9 4v5H4\"/><path d=\"M15 4v5h5\"/><path d=\"M9 20v-5H4\"/><path d=\"M15 20v-5h5\"/></svg>"
}

fn files_icon() -> &'static str {
    "<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.7\"><path d=\"M4 6h16M4 12h16M4 18h10\"/></svg>"
}

/// The trail from the repository root to `current`.
#[must_use]
pub fn breadcrumb(current: &str) -> String {
    let mut output =
        String::from("<nav class=\"crumbs\" aria-label=\"Breadcrumb\"><a href=\"/\">root</a>");
    let mut walked = String::new();
    for segment in current.split('/').filter(|segment| !segment.is_empty()) {
        if walked.is_empty() {
            walked.push_str(segment);
        } else {
            walked.push('/');
            walked.push_str(segment);
        }
        let _ = write!(
            output,
            "<span aria-hidden=\"true\">/</span><a href=\"/{}\">{}</a>",
            href(&walked),
            escape(segment)
        );
    }
    output.push_str("</nav>");
    output
}

/// The link that shows a file as its stored source.
#[must_use]
pub fn source_action(path: &str) -> String {
    format!(
        "<a class=\"action\" href=\"/{}?raw=1\" title=\"Show the stored source\">Source</a>",
        href(path)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::index::RepoIndex;
    use crate::web::theme::Theme;
    use std::path::Path;

    fn listing() -> RepoIndex {
        RepoIndex::from_listing(
            Path::new("."),
            &["README.md", "docs/README.md", "docs/ui.md"],
            &[],
        )
    }

    #[test]
    fn the_breadcrumb_links_each_level() {
        let crumbs = breadcrumb("docs/guides/ui.md");
        assert!(crumbs.contains("href=\"/docs\""));
        assert!(crumbs.contains("href=\"/docs/guides\""));
        assert!(crumbs.contains("href=\"/docs/guides/ui.md\""));
    }

    #[test]
    fn the_theme_menu_marks_the_current_choice() {
        let links = theme_links(Theme::Dark, "%2Fdocs");
        assert!(links.contains("href=\"/theme/dark?next=%2Fdocs\""));
        assert!(links.contains("aria-current=\"true\">Dark<"), "{links}");
        assert_eq!(links.matches("aria-current").count(), 1, "{links}");
    }

    #[test]
    fn a_reading_width_reaches_the_page() {
        let index = listing();
        let config = Config {
            reading_width: 52,
            ..Config::default()
        };
        let listing = index.listing(&Extensions::parse("md"));
        let page = page(
            &Page {
                title: "t",
                current: "README.md",
                listing: &listing,
                view: View {
                    theme: Theme::Auto,
                    extensions: Extensions::parse("md"),
                    reading: false,
                    config: &config,
                },
                actions: "",
                headings: &[],
                toc_label: "On this page",
                syntax: false,
                math: false,
                mermaid: false,
            },
            "<p>body</p>",
        );
        assert!(page.contains("--measure:52rem"), "{page}");
        assert!(page.contains("data-theme=\"auto\""), "{page}");
        assert!(page.contains("data-reading=\"off\""), "{page}");
        assert!(page.contains("class=\"skip\""), "{page}");
    }

    #[test]
    fn the_reader_control_always_holds_a_way_out() {
        let index = listing();
        let config = Config::default();
        let listing = index.listing(&Extensions::parse("md"));
        let page = page(
            &Page {
                title: "t",
                current: "docs/ui.md",
                listing: &listing,
                view: View {
                    theme: Theme::Auto,
                    extensions: Extensions::parse("md"),
                    reading: true,
                    config: &config,
                },
                actions: "",
                headings: &[],
                toc_label: "On this page",
                syntax: false,
                math: false,
                mermaid: false,
            },
            "<p>body</p>",
        );
        assert!(page.contains("data-reading=\"on\""), "{page}");
        assert!(
            page.contains("href=\"/reading/off?next="),
            "the control must leave reading mode without a script: {page}"
        );
        assert!(
            page.contains("class=\"reader\""),
            "the control is drawn on every page, and the stylesheet shows it in reading mode: {page}"
        );
    }
}
