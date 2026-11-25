//! The file tree and the file-type filter in the sidebar.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::site::html::{escape, href, query_value};
use crate::site::index::{Listing, TreeNode};

use super::shell::View;

/// The whole tree, opened along the path to `current`.
///
/// A directory is a `<details>` element, so it opens and closes with the
/// keyboard and needs no script. Only the ancestors of the current page start
/// open, which keeps a large tree quiet.
#[must_use]
pub fn tree(listing: &Listing<'_>, current: &str) -> String {
    let mut output = String::from("<div class=\"tree-wrap\">");
    children(&mut output, listing.tree(), current);
    output.push_str("</div>");
    output
}

fn children(output: &mut String, node: &TreeNode, current: &str) {
    output.push_str("<ul class=\"tree\">");
    for directory in &node.dirs {
        let prefix = format!("{}/", directory.path);
        let open = current == directory.path || current.starts_with(&prefix);
        let _ = write!(
            output,
            "<li class=\"dir\"><details{}><summary>{}</summary>",
            if open { " open" } else { "" },
            escape(&directory.name)
        );
        children(output, directory, current);
        output.push_str("</details></li>");
    }
    for (name, path) in &node.files {
        let active = current == path;
        let _ = write!(
            output,
            "<li><a href=\"/{}\"{}>{}</a></li>",
            href(path),
            if active {
                " class=\"active\" aria-current=\"page\""
            } else {
                ""
            },
            escape(name)
        );
    }
    output.push_str("</ul>");
}

/// Which shelf a file type belongs on.
///
/// A document is what a reader comes for. Code is what the highlighter colours.
/// Everything else, such as an image or a data file, is an extra.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Shelf {
    Documents,
    Code,
    Other,
}

impl Shelf {
    fn title(self) -> &'static str {
        match self {
            Self::Documents => "Documents",
            Self::Code => "Code",
            Self::Other => "Other",
        }
    }
}

/// The shelf of one file type.
fn shelf(extension: &str) -> Shelf {
    match extension {
        "md" | "markdown" | "mdown" | "mdx" | "ipynb" => Shelf::Documents,
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "java" | "c" | "h" | "cc" | "cpp"
        | "hpp" | "cs" | "rb" | "php" | "swift" | "kt" | "kts" | "sh" | "bash" | "zsh" | "sql"
        | "lua" | "pl" | "r" | "ex" | "exs" | "hs" | "scala" | "dart" | "zig" | "nim" | "vim"
        | "el" | "clj" | "ml" | "mli" | "jl" | "asm" | "s" | "vue" | "svelte" => Shelf::Code,
        _ => Shelf::Other,
    }
}

/// A readable name for a file type.
fn type_name(extension: &str) -> String {
    match extension {
        "md" | "markdown" | "mdown" | "mdx" => "Markdown",
        "ipynb" => "Notebook",
        "rs" => "Rust",
        "py" => "Python",
        "js" => "JavaScript",
        "jsx" => "React",
        "ts" => "TypeScript",
        "tsx" => "React TS",
        "go" => "Go",
        "java" => "Java",
        "c" => "C",
        "h" => "C header",
        "cc" | "cpp" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "kt" | "kts" => "Kotlin",
        "sh" | "bash" | "zsh" => "Shell",
        "sql" => "SQL",
        "lua" => "Lua",
        "r" => "R",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "scss" => "SCSS",
        "txt" | "text" => "Text",
        "csv" => "CSV",
        "json" => "JSON",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "xml" => "XML",
        "png" => "PNG",
        "jpg" | "jpeg" => "JPEG",
        "gif" => "GIF",
        "webp" => "WebP",
        "avif" => "AVIF",
        "svg" => "SVG",
        "pdf" => "PDF",
        other => return format!(".{other}"),
    }
    .to_string()
}

/// The filter form: which file types the tree and the pages show.
///
/// The form offers every type the repository holds, on three shelves. Documents
/// come first, then code, then the extras. Inside a shelf the type with more
/// files comes first. A reader can turn on a type that the filter hides.
#[must_use]
pub fn filter(view: &View<'_>, listing: &Listing<'_>, current: &str) -> String {
    let target = query_value(&format!("/{current}"));
    let summary = match view.extensions.len() {
        0 => "Showing <strong>nothing</strong>".to_string(),
        1 => format!(
            "Showing <strong>{}</strong>",
            escape(&type_name(&view.extensions.label()))
        ),
        count => format!("Showing <strong>{count} types</strong>"),
    };

    // Every type the repository holds, plus each configured preset, so a quick
    // choice stays on the form even before a file of that type exists.
    let mut types = listing.type_counts();
    for preset in &view.config.presets {
        types.entry(preset.clone()).or_insert(0);
    }
    let offered: Vec<String> = types.keys().cloned().collect();

    let mut shelves: BTreeMap<Shelf, Vec<(String, usize)>> = BTreeMap::new();
    for (extension, count) in &types {
        shelves
            .entry(shelf(extension))
            .or_default()
            .push((extension.clone(), *count));
    }
    for entries in shelves.values_mut() {
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }

    let mut output = String::new();
    let _ = write!(
        output,
        "<details class=\"filter\"><summary>{summary}</summary>\
         <form action=\"/filter/apply\" method=\"get\">\
         <input type=\"hidden\" name=\"next\" value=\"/{}\">",
        escape(current)
    );
    for (shelf, entries) in &shelves {
        let _ = write!(
            output,
            "<fieldset class=\"types\"><legend>{}</legend><div class=\"type-row\">",
            shelf.title()
        );
        for (extension, count) in entries {
            let _ = write!(
                output,
                "<label class=\"type\"><input type=\"checkbox\" name=\"ext\" value=\"{}\"{}>\
                 <span>{}</span><span class=\"count\">{count}</span></label>",
                escape(extension),
                if view.extensions.contains(extension) {
                    " checked"
                } else {
                    ""
                },
                escape(&type_name(extension))
            );
        }
        output.push_str("</div></fieldset>");
    }

    // A reader can add a type that the shelves do not offer.
    let custom = view.extensions.custom(&offered);
    let _ = write!(
        output,
        "<label class=\"extra\">Add a type <input type=\"text\" name=\"extra\" value=\"{}\" \
         placeholder=\"ts, toml, yaml\" autocomplete=\"off\" autocapitalize=\"none\" \
         spellcheck=\"false\"></label>\
         <div class=\"filter-actions\"><button type=\"submit\">Apply</button>\
         <div class=\"filter-links\">\
         <a href=\"/filter/apply?ext=md&next={target}\">Markdown only</a>\
         <a href=\"/filter/apply?next={target}\">Nothing</a></div></div>\
         <p class=\"note\">A file with no extension is never shown. A page loads its \
         images and fonts whatever the filter says.</p>\
         </form></details>",
        escape(&custom)
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::site::index::{Extensions, RepoIndex};
    use crate::web::theme::Theme;
    use std::path::Path;

    fn index() -> RepoIndex {
        RepoIndex::from_listing(
            Path::new("."),
            &["README.md", "docs/README.md", "docs/guides/ui.md"],
            &[],
        )
    }

    fn view<'a>(config: &'a Config, extensions: &str) -> View<'a> {
        View {
            theme: Theme::Auto,
            extensions: Extensions::parse(extensions),
            reading: false,
            config,
        }
    }

    #[test]
    fn the_tree_opens_the_path_to_the_current_page() {
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let html = tree(&listing, "docs/guides/ui.md");
        assert!(html.matches("<details open>").count() >= 1, "{html}");
        assert!(
            html.contains("aria-current=\"page\">ui.md<"),
            "the current file is marked: {html}"
        );
        assert!(!html.contains("docs/README.md") || html.contains("<details open>"));
    }

    #[test]
    fn a_closed_directory_stays_closed() {
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let html = tree(&listing, "README.md");
        assert!(!html.contains("<details open>"), "{html}");
    }

    #[test]
    fn the_filter_shows_a_tick_for_every_shown_type() {
        let config = Config::default();
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let html = filter(&view(&config, "md"), &listing, "docs/README.md");
        assert!(
            html.contains("value=\"md\" checked"),
            "markdown is ticked: {html}"
        );
        assert!(html.contains(">Markdown<"), "{html}");
        assert!(
            html.contains("name=\"next\" value=\"/docs/README.md\""),
            "{html}"
        );
    }

    #[test]
    fn the_filter_puts_documents_before_code_and_code_before_extras() {
        let config = Config::default();
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["a.md", "b.md", "c.ipynb", "d.rs", "e.json", "f.png"],
            &[],
        );
        let listing = index.listing(&Extensions::parse("md"));
        let html = filter(&view(&config, "md"), &listing, "");
        let documents = html.find(">Documents<").expect("a Documents shelf");
        let code = html.find(">Code<").expect("a Code shelf");
        let other = html.find(">Other<").expect("an Other shelf");
        assert!(documents < code && code < other, "{html}");
        let markdown = html.find(">Markdown<").expect("Markdown");
        let notebook = html.find(">Notebook<").expect("Notebook");
        assert!(
            markdown < notebook,
            "the type with more files comes first: {html}"
        );
    }

    #[test]
    fn the_filter_offers_every_type_the_repository_holds() {
        let config = Config::default();
        let index = RepoIndex::from_listing(Path::new("."), &["a.md", "b.json"], &[]);
        let listing = index.listing(&Extensions::parse("md"));
        let html = filter(&view(&config, "md"), &listing, "");
        assert!(
            html.contains("value=\"json\""),
            "a hidden type is still offered: {html}"
        );
        assert!(html.contains(">JSON<"), "{html}");
    }

    #[test]
    fn the_filter_says_when_nothing_is_shown() {
        let config = Config::default();
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let html = filter(&view(&config, ""), &listing, "");
        assert!(html.contains("Showing <strong>nothing</strong>"), "{html}");
        assert!(!html.contains("checked"), "{html}");
    }

    #[test]
    fn the_filter_counts_the_types_it_shows() {
        let config = Config::default();
        let index = index();
        let listing = index.listing(&Extensions::parse("md"));
        let one = filter(&view(&config, "md"), &listing, "");
        assert!(one.contains("Showing <strong>Markdown</strong>"), "{one}");
        let many = filter(&view(&config, "md,rs,ts"), &listing, "");
        assert!(many.contains("Showing <strong>3 types</strong>"), "{many}");
    }
}
