//! The HTML the server returns: one page shell, the sidebar tree and the pages.

use std::fmt::Write as _;

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS, NON_ALPHANUMERIC};

use crate::config::Config;
use crate::index::{Extensions, RepoIndex, TreeNode};
use crate::render::numbered_source;

/// Characters a repository path must not carry unencoded in a URL.
const PATH_ENCODE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'&')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'%')
    .add(b'\\')
    .add(b'^')
    .add(b'|');

const STYLE: &str = r#"
*{box-sizing:border-box}
:root{--bg:#fff;--fg:#1c1c1e;--muted:#66666d;--line:#e2e2e6;--code-bg:#f5f5f7;--accent:#0b5cad;--nav-bg:#fafafa}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121214;--fg:#e8e8ea;--muted:#9b9ba3;--line:#2b2b30;--code-bg:#1a1a1e;--accent:#7ab7ff;--nav-bg:#17171a}}
:root[data-theme=dark]{--bg:#121214;--fg:#e8e8ea;--muted:#9b9ba3;--line:#2b2b30;--code-bg:#1a1a1e;--accent:#7ab7ff;--nav-bg:#17171a}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}
a{color:var(--accent);text-decoration:none}
a:hover{text-decoration:underline}
header{position:sticky;top:0;z-index:10;display:flex;gap:.5rem;align-items:center;padding:.5rem .75rem;background:var(--bg);border-bottom:1px solid var(--line)}
.brand{font-weight:600;color:var(--fg)}
.search{flex:1;min-width:0}
.search input{width:100%;padding:.5rem .6rem;font:inherit;font-size:16px;color:var(--fg);background:var(--code-bg);border:1px solid var(--line);border-radius:.5rem}
.theme{padding:.42rem .55rem;border:1px solid var(--line);border-radius:.5rem;color:var(--fg);font-size:.82rem;white-space:nowrap}
.nav-button{padding:.5rem .7rem;border:1px solid var(--line);border-radius:.5rem;color:var(--fg);cursor:pointer;font-size:.9rem;white-space:nowrap}
.nav-toggle{position:absolute;opacity:0;pointer-events:none}
.nav{position:relative;display:none;border-bottom:1px solid var(--line);background:var(--nav-bg);padding:.25rem .75rem 1rem;max-height:60vh;overflow:auto}
.nav-toggle:checked ~ .shell .nav{display:block}
.nav-title{margin:.5rem 0 .25rem;font-size:.72rem;letter-spacing:.09em;text-transform:uppercase;color:var(--muted)}
.tree{list-style:none;margin:0;padding:0}
.tree .tree{padding-left:.7rem;margin-left:.2rem;border-left:1px solid var(--line)}
.tree a{display:block;padding:.42rem .3rem;font-size:.94rem;color:var(--fg);border-radius:.35rem;overflow-wrap:anywhere}
.tree a.dir{font-weight:600}
.tree a.active{background:var(--code-bg);color:var(--accent)}
main{padding:1rem .9rem 4rem;max-width:46rem;margin:0 auto}
.page-head{display:flex;align-items:center;justify-content:space-between;gap:.75rem;margin:0 0 .9rem}
.crumbs{font-size:.85rem;color:var(--muted);margin:0;overflow-wrap:anywhere;min-width:0}
.crumbs a{color:var(--muted)}
.action{flex:0 0 auto;font-size:.8rem;padding:.32rem .55rem;border:1px solid var(--line);border-radius:.45rem;color:var(--fg);white-space:nowrap}
.copyable{position:relative}
.copy{position:absolute;top:.4rem;right:.4rem;z-index:1;padding:.28rem .5rem;font:600 11px/1.4 system-ui,sans-serif;color:var(--muted);background:var(--bg);border:1px solid var(--line);border-radius:.35rem;cursor:pointer;opacity:0;transition:opacity .12s}
.copyable:hover .copy,.copy:focus-visible,.copy.shown{opacity:1}
@media (hover:none){.copy{opacity:.7}}
.nav-filter{margin:0 0 .5rem;font-size:.82rem;color:var(--muted)}
.nav-filter strong{color:var(--fg)}
.filter{display:grid;gap:.55rem;margin:1rem 0 1.5rem;padding:.9rem;border:1px solid var(--line);border-radius:.55rem}
.filter label{display:flex;gap:.55rem;align-items:center;padding:.35rem .1rem}
.filter input[type=checkbox]{width:1.15rem;height:1.15rem;accent-color:var(--accent)}
.filter input[type=text]{width:100%;padding:.55rem .6rem;font:inherit;font-size:16px;color:var(--fg);background:var(--code-bg);border:1px solid var(--line);border-radius:.5rem}
.filter button{justify-self:start;padding:.6rem 1.1rem;font:inherit;font-weight:600;color:var(--bg);background:var(--accent);border:0;border-radius:.5rem}
h1,h2,h3,h4{line-height:1.25;margin:1.5em 0 .5em}
h1{font-size:1.5rem;margin-top:.4em}
h2{font-size:1.22rem}
h3{font-size:1.05rem}
.md img{max-width:100%;height:auto}
.md table{display:block;overflow-x:auto;border-collapse:collapse;margin:1rem 0;font-size:.92rem}
.md th,.md td{border:1px solid var(--line);padding:.4rem .55rem;text-align:left;vertical-align:top}
.md th{background:var(--code-bg)}
.md pre{background:var(--code-bg);padding:.75rem;border-radius:.5rem;overflow-x:auto;font-size:.82rem;line-height:1.5}
.md code{background:var(--code-bg);padding:.1rem .25rem;border-radius:.25rem;font-size:.9em;overflow-wrap:anywhere}
.md pre code{background:none;padding:0}
.mermaid{background:transparent;padding:0;text-align:center;font-size:inherit;overflow-x:auto}
.md blockquote{margin:1rem 0;padding:.25rem 0 .25rem .9rem;border-left:3px solid var(--line);color:var(--muted)}
.md hr{border:0;border-top:1px solid var(--line);margin:2rem 0}
.listing{list-style:none;margin:0;padding:0}
.listing li{border-bottom:1px solid var(--line)}
.listing a{display:flex;justify-content:space-between;gap:.75rem;padding:.72rem .2rem;color:var(--fg)}
.listing .kind{color:var(--muted);font-size:.82rem;white-space:nowrap}
.code{display:flex;gap:.8rem;background:var(--code-bg);border-radius:.5rem;padding:.65rem .5rem;overflow:hidden;font:13px/1.55 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.code pre{margin:0;overflow-x:auto;white-space:pre;tab-size:4}
.code .gutter{margin:0;text-align:right;color:var(--muted);user-select:none;overflow:hidden;flex:0 0 auto}
.code code{font:inherit;white-space:pre;background:none;padding:0}
.meta{color:var(--muted);font-size:.85rem;margin:.2rem 0 1rem}
.nb-cell{border-top:1px solid var(--line);margin-top:1.1rem;padding-top:.2rem}
.nb-cell:first-child{border-top:0;margin-top:0}
.nb-cell.md h1{font-size:1.3rem;margin-top:.3em}
.nb-cell.md h2{font-size:1.15rem}
.nb-in{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;color:var(--muted);margin:.3rem 0 .35rem}
.nb-source{margin:0 0 .6rem}
.nb-out{margin:.35rem 0 .8rem}
.nb-stream{color:var(--fg)}
.nb-error{color:#b3261e;border-left:3px solid #b3261e;padding-left:.7rem}
.nb-html-wrap{margin:.35rem 0 .8rem}
.nb-html{display:block;width:100%;height:120px;border:1px solid var(--line);border-radius:.5rem;background:#fff;color:#1c1c1c}
.nb-note{font-size:.8rem;color:var(--muted)}
.empty{color:var(--muted)}
footer.page{margin-top:2.5rem;padding-top:.75rem;border-top:1px solid var(--line);font-size:.85rem;color:var(--muted)}
@media (min-width:60rem){
 .nav-toggle,.nav-button{display:none}
 .shell{display:grid;grid-template-columns:19rem minmax(0,1fr);align-items:start}
 .nav{display:block;position:sticky;top:3.3rem;max-height:calc(100vh - 3.3rem);border-bottom:0;border-right:1px solid var(--line)}
 main{max-width:52rem;padding:1.5rem 2rem 6rem}
}
"#;

/// How the page chooses its colours.
///
/// `Auto` follows the device's own light or dark preference. `Light` and `Dark`
/// override it. The choice is a cookie, so the server can put it in the markup
/// and the first paint is already correct: no flash and no script needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// Follow the device.
    Auto,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl Theme {
    /// Reads a cookie value. Anything unrecognised means `Auto`.
    #[must_use]
    pub fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::Auto,
        }
    }

    /// The value stored in the cookie and in the `data-theme` attribute.
    #[must_use]
    pub fn value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The word the header shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// The theme one tap away, so the control cycles through the three.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::Auto,
        }
    }
}

/// Sizes each HTML output frame to its content.
///
/// The frame is sandboxed without `allow-scripts`, so nothing inside it runs.
/// `allow-same-origin` is what lets this script read the height; without it every
/// frame would be a fixed box with its own scrollbar.
const NOTEBOOK_SCRIPT: &str = r#"<script>function sizeNotebooks(){document.querySelectorAll('iframe.nb-html').forEach(function(f){try{var d=f.contentDocument;if(d&&d.body){f.style.height=(d.body.scrollHeight+24)+'px';}}catch(e){}});}document.querySelectorAll('iframe.nb-html').forEach(function(f){f.addEventListener('load',sizeNotebooks);});sizeNotebooks();</script>
"#;

/// The button that copies the code beside it. One per code block, and no more.
pub const COPY_BUTTON: &str = "<button class=\"copy\" type=\"button\" data-label=\"Copy\" \
     aria-label=\"Copy to the clipboard\">Copy</button>";

/// Copies the code in the block that holds the button.
///
/// The Clipboard API needs a secure context, and this server is usually reached
/// over plain HTTP on a local address, so the older selection-based copy is the
/// path that actually runs on a phone.
const COPY_SCRIPT: &str = r#"<script>(function(){
function legacy(text,done){var area=document.createElement('textarea');area.value=text;area.setAttribute('readonly','');area.style.position='fixed';area.style.top='-1000px';area.style.opacity='0';document.body.appendChild(area);area.select();area.setSelectionRange(0,text.length);var ok=false;try{ok=document.execCommand('copy');}catch(e){ok=false;}document.body.removeChild(area);done(ok);}
function copyText(text,done){if(navigator.clipboard&&window.isSecureContext){navigator.clipboard.writeText(text).then(function(){done(true);},function(){legacy(text,done);});}else{legacy(text,done);}}
document.querySelectorAll('button.copy').forEach(function(button){button.addEventListener('click',function(){var box=button.closest('.copyable');var code=box?box.querySelector('code'):null;if(!code){return;}copyText(code.textContent,function(ok){button.textContent=ok?'Copied':'Failed';button.classList.add('shown');window.setTimeout(function(){button.textContent=button.dataset.label||'Copy';button.classList.remove('shown');},1400);});});});
})();</script>
"#;

/// Reloads the page when the server reports a change.
const RELOAD_SCRIPT: &str = r#"<script>new EventSource('/__reload').onmessage = function () { location.reload(); };</script>
"#;

/// The script that draws diagrams. Included only on a page that has one, and
/// pointed at the copy embedded in this binary, so it needs no network.
const MERMAID_SCRIPT: &str = r#"<script src="/assets/mermaid.min.js"></script>
<script>(function(){var t=document.documentElement.dataset.theme;var dark=t==='dark'||(t!=='light'&&window.matchMedia('(prefers-color-scheme: dark)').matches);mermaid.initialize({startOnLoad:true,securityLevel:'strict',theme:dark?'dark':'default'});})();</script>
"#;

/// Escapes text for use in HTML text and attribute values.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            other => output.push(other),
        }
    }
    output
}

/// Percent-encodes a repository path for use as a URL.
#[must_use]
pub fn href(path: &str) -> String {
    utf8_percent_encode(path, PATH_ENCODE).to_string()
}

/// The choices a request carries: the colour scheme and the extension filter.
#[derive(Clone, Debug)]
pub struct View<'a> {
    /// The colour scheme the reader chose.
    pub theme: Theme,
    /// The file extensions the reader asked to see.
    pub extensions: Extensions,
    /// The configuration this server runs with.
    pub config: &'a Config,
}

/// The document shell: header, file tree and one page of content.
#[must_use]
pub fn layout(
    title: &str,
    current: &str,
    tree: &TreeNode,
    view: View<'_>,
    actions: &str,
    body: &str,
) -> String {
    let next = view.theme.next();
    let target = utf8_percent_encode(&format!("/{current}"), NON_ALPHANUMERIC).to_string();
    let highlight_css = if body.contains("hl-") {
        "<link rel=\"stylesheet\" href=\"/assets/highlight.css\">"
    } else {
        ""
    };
    let notebook = if body.contains("class=\"nb-html\"") {
        NOTEBOOK_SCRIPT
    } else {
        ""
    };
    let reload = if view.config.live_reload {
        RELOAD_SCRIPT
    } else {
        ""
    };
    let copy = if body.contains("class=\"copy\"") {
        COPY_SCRIPT
    } else {
        ""
    };
    let mermaid = if body.contains("class=\"mermaid\"") {
        MERMAID_SCRIPT
    } else {
        ""
    };
    format!(
        "<!DOCTYPE html>\n\
<html lang=\"en\" data-theme=\"{theme_value}\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"color-scheme\" content=\"light dark\">\n\
<title>{title}</title>\n\
<style>{STYLE}</style>\n\
{highlight_css}\
</head>\n\
<body>\n\
<input type=\"checkbox\" id=\"nav-toggle\" class=\"nav-toggle\">\n\
<header>\n\
<a class=\"brand\" href=\"/\">{brand}</a>\n\
<form class=\"search\" action=\"/search\" method=\"get\" role=\"search\">\n\
<input type=\"search\" name=\"q\" placeholder=\"Find a file\" aria-label=\"Find a file in the repository\" enterkeyhint=\"search\">\n\
</form>\n\
<a class=\"theme\" href=\"/theme/{next_value}?next={target}\" title=\"Theme: {label}. Switch to {next_label}.\">{label}</a>\n\
<label class=\"nav-button\" for=\"nav-toggle\">Files</label>\n\
</header>\n\
<div class=\"shell\">\n\
<nav class=\"nav\" aria-label=\"Repository files\">\n\
<p class=\"nav-title\">Files</p>\n\
{filter}\n\
{nav}\n\
</nav>\n\
<main>\n\
<div class=\"page-head\">{crumbs}{actions}</div>\n\
{body}\n\
</main>\n\
</div>\n\
<script>var n=document.querySelector('.nav'),a=n&&n.querySelector('.active');if(a){{n.scrollTop=Math.max(0,a.offsetTop-n.clientHeight/3);}}</script>\n\
{mermaid}\
{notebook}\
{copy}\
{reload}\
</body>\n\
</html>\n",
        title = escape(title),
        brand = escape(&view.config.title),
        theme_value = view.theme.value(),
        next_value = next.value(),
        label = view.theme.label(),
        next_label = next.label(),
        nav = sidebar(tree, current),
        filter = filter_control(current, &view.extensions),
        crumbs = breadcrumb(current),
        actions = actions,
        body = body,
        mermaid = mermaid,
        notebook = notebook,
        copy = copy,
        reload = reload,
        highlight_css = highlight_css,
    )
}

/// The file tree, opened along the path to `current` and closed elsewhere.
///
/// Only the ancestors of the current path are rendered, so a page stays small
/// however large the repository is. A directory link opens that directory and
/// the tree is rebuilt around it.
fn sidebar(node: &TreeNode, current: &str) -> String {
    let mut output = String::from("<ul class=\"tree\">");
    for directory in &node.dirs {
        let prefix = format!("{}/", directory.path);
        let open = current == directory.path || current.starts_with(&prefix);
        let class = if open { "dir active" } else { "dir" };
        let _ = write!(
            output,
            "<li><a class=\"{class}\" href=\"/{}/\">{}</a>",
            href(&directory.path),
            escape(&directory.name)
        );
        if open {
            output.push_str(&sidebar(directory, current));
        }
        output.push_str("</li>");
    }
    for (name, path) in &node.files {
        let class = if current == path { "active" } else { "" };
        let _ = write!(
            output,
            "<li><a class=\"{class}\" href=\"/{}\">{}</a></li>",
            href(path),
            escape(name)
        );
    }
    output.push_str("</ul>");
    output
}

/// The filter line at the top of the file tree.
fn filter_control(current: &str, extensions: &Extensions) -> String {
    format!(
        "<p class=\"nav-filter\">Showing <strong>{}</strong> \u{b7} <a href=\"/filter?next={}\">Change</a></p>",
        escape(&extensions.label()),
        utf8_percent_encode(&format!("/{current}"), NON_ALPHANUMERIC)
    )
}

/// The link that shows a file as its stored source.
#[must_use]
pub fn source_action(path: &str) -> String {
    format!(
        "<a class=\"action\" href=\"/{}?raw=1\" title=\"Show the stored source\">Source</a>",
        href(path)
    )
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
            " / <a href=\"/{}\">{}</a>",
            href(&walked),
            escape(segment)
        );
    }
    output.push_str("</nav>");
    output
}

/// The landing page and every directory page.
#[must_use]
pub fn directory_page(index: &RepoIndex, directory: &str, view: View<'_>) -> String {
    let node = index.tree();
    let tree = if directory.is_empty() {
        node
    } else {
        match find(node, directory) {
            Some(node) => node,
            None => node,
        }
    };

    let title = if directory.is_empty() {
        view.config.title.clone()
    } else {
        directory.to_string()
    };
    let mut body = String::new();
    let _ = write!(body, "<h1>{}</h1>", escape(&title));
    if directory.is_empty() {
        let _ = write!(
            body,
            "<p class=\"meta\">{} files. Markdown is rendered; every other file opens as source.</p>",
            index.len()
        );
    }

    body.push_str("<ul class=\"listing\">");
    for child in &tree.dirs {
        let _ = write!(
            body,
            "<li><a href=\"/{}/\"><span>{}/</span><span class=\"kind\">dir</span></a></li>",
            href(&child.path),
            escape(&child.name)
        );
    }
    for (name, path) in &tree.files {
        let _ = write!(
            body,
            "<li><a href=\"/{}\"><span>{}</span><span class=\"kind\">{}</span></a></li>",
            href(path),
            escape(name),
            escape(&kind(name))
        );
    }
    body.push_str("</ul>");
    if tree.dirs.is_empty() && tree.files.is_empty() {
        body.push_str("<p class=\"empty\">This directory is empty.</p>");
    }

    layout(&title, directory, index.tree(), view, "", &body)
}

fn find<'a>(node: &'a TreeNode, directory: &str) -> Option<&'a TreeNode> {
    if node.path == directory {
        return Some(node);
    }
    node.dirs.iter().find_map(|child| find(child, directory))
}

fn kind(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, extension)) if !extension.is_empty() => extension.to_string(),
        _ => "file".to_string(),
    }
}

/// One Markdown document, rendered.
#[must_use]
pub fn document_page(index: &RepoIndex, path: &str, html: &str, view: View<'_>) -> String {
    let mut body = String::from("<article class=\"md\">");
    body.push_str(html);
    body.push_str("</article>");
    let _ = write!(
        body,
        "<footer class=\"page\"><a href=\"/{}?raw=1\">View source</a></footer>",
        href(path)
    );
    layout(path, path, index.tree(), view, &source_action(path), &body)
}

/// One Jupyter notebook, rendered read-only.
#[must_use]
pub fn notebook_page(index: &RepoIndex, path: &str, cells: &str, view: View<'_>) -> String {
    let mut body = String::from("<article class=\"md nb\">");
    body.push_str(cells);
    body.push_str("</article>");
    let _ = write!(
        body,
        "<footer class=\"page\"><a href=\"/{}?raw=1\">View source</a></footer>",
        href(path)
    );
    layout(path, path, index.tree(), view, &source_action(path), &body)
}

/// One source file, numbered.
#[must_use]
pub fn source_page(
    index: &RepoIndex,
    path: &str,
    source: &str,
    size: usize,
    view: View<'_>,
    highlighted: Option<&str>,
) -> String {
    let lines = source.lines().count();
    let mut body = String::new();
    let _ = write!(body, "<h1>{}</h1>", escape(path));
    let _ = write!(
        body,
        "<p class=\"meta\">{} lines. {}.</p>",
        lines,
        size_label(size)
    );
    match highlighted {
        Some(html) => body.push_str(&crate::render::numbered_highlighted(html, lines)),
        None => body.push_str(&numbered_source(source, 1)),
    }
    layout(path, path, index.tree(), view, "", &body)
}

/// A file that is not text: its size, and nothing else.
#[must_use]
pub fn binary_page(index: &RepoIndex, path: &str, size: usize, view: View<'_>) -> String {
    let mut body = String::new();
    let _ = write!(body, "<h1>{}</h1>", escape(path));
    let _ = write!(
        body,
        "<p class=\"meta\">{}. This file is not text, so the server does not display it.</p>",
        size_label(size)
    );
    layout(path, path, index.tree(), view, "", &body)
}

/// Search results for one query.
#[must_use]
pub fn search_page(index: &RepoIndex, query: &str, matches: &[&str], view: View<'_>) -> String {
    let mut body = String::new();
    let _ = write!(body, "<h1>Results for “{}”</h1>", escape(query));
    let _ = write!(
        body,
        "<p class=\"meta\">{} of {} files match.</p>",
        matches.len(),
        index.len()
    );
    if matches.is_empty() {
        body.push_str(
            "<p class=\"empty\">No file path contains that text. Search matches paths only.</p>",
        );
    } else {
        body.push_str("<ul class=\"listing\">");
        for path in matches {
            let _ = write!(
                body,
                "<li><a href=\"/{}\"><span>{}</span><span class=\"kind\">{}</span></a></li>",
                href(path),
                escape(path),
                escape(&kind(path))
            );
        }
        body.push_str("</ul>");
    }
    layout(
        &format!("Results for {query}"),
        "",
        index.tree(),
        view,
        "",
        &body,
    )
}

/// The form that chooses which file types are shown.
#[must_use]
pub fn filter_page(index: &RepoIndex, current: &str, view: View<'_>) -> String {
    let target = utf8_percent_encode(&format!("/{current}"), NON_ALPHANUMERIC).to_string();
    let mut body = String::from(
        "<h1>Files shown</h1>\n<p class=\"meta\">Choose which file types the tree and the \
         pages show. Anything you untick disappears from both. The default is markdown alone.</p>\n",
    );
    body.push_str("<form class=\"filter\" action=\"/filter/apply\" method=\"get\">");
    let _ = write!(
        body,
        "<input type=\"hidden\" name=\"next\" value=\"/{current}\">"
    );
    for preset in &view.config.presets {
        let checked = if view.extensions.contains(preset) {
            " checked"
        } else {
            ""
        };
        let label = match preset.as_str() {
            "md" => "Markdown (.md, .markdown)".to_string(),
            "rs" => "Rust (.rs)".to_string(),
            "ipynb" => "Jupyter notebooks (.ipynb)".to_string(),
            "png" => "PNG images (.png)".to_string(),
            "webp" => "WebP images (.webp)".to_string(),
            other => format!("{other} files"),
        };
        let _ = write!(
            body,
            "<label><input type=\"checkbox\" name=\"ext\" value=\"{preset}\"{checked}> {}</label>",
            escape(&label)
        );
    }
    let _ = write!(
        body,
        "<label>Other extensions<input type=\"text\" name=\"extra\" value=\"{}\" \
         placeholder=\"ts, toml, yaml\" autocomplete=\"off\" autocapitalize=\"none\" \
         spellcheck=\"false\"></label>",
        escape(&view.extensions.custom(&view.config.presets))
    );
    body.push_str("<button type=\"submit\">Apply</button></form>");
    let _ = write!(
        body,
        "<p class=\"meta\">Untick everything and the server shows markdown alone. \
         A file with no extension is never shown. <a href=\"/filter/apply?ext=md&next={target}\">\
         Back to markdown only</a>.</p>"
    );
    layout("Files shown", current, index.tree(), view, "", &body)
}

/// A path that the index does not list.
#[must_use]
pub fn not_found_page(index: &RepoIndex, path: &str, view: View<'_>) -> String {
    let body = format!(
        "<h1>Not found</h1>\n<p><code>{}</code> is not part of the repository content. \
         Ignored and generated files are never served.</p>\n<p><a href=\"/\">Back to the root</a></p>",
        escape(path)
    );
    layout("Not found", "", index.tree(), view, "", &body)
}

/// The prompt shown when a token is configured and the request carries none.
#[must_use]
pub fn token_page(index: &RepoIndex, view: View<'_>) -> String {
    let body = "<h1>Token required</h1>\n\
<p>This server asks for the token it was started with.</p>\n\
<form class=\"search\" action=\"\" method=\"get\">\n\
<input type=\"password\" name=\"token\" aria-label=\"Token\" enterkeyhint=\"go\" autocomplete=\"current-password\">\n\
</form>"
        .to_string();
    layout("Token required", "", index.tree(), view, "", &body)
}

fn size_label(size: usize) -> String {
    if size < 1024 {
        format!("{size} bytes")
    } else if size < 1024 * 1024 {
        format!("{:.1} KiB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", size as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_covers_the_dangerous_characters() {
        assert_eq!(
            escape("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn hrefs_encode_spaces_and_ampersands() {
        assert_eq!(href("work/a plan.md"), "work/a%20plan.md");
        assert_eq!(href("docs/a&b.md"), "docs/a%26b.md");
    }

    #[test]
    fn the_theme_control_cycles_and_marks_the_page() {
        assert_eq!(Theme::Auto.next(), Theme::Light);
        assert_eq!(Theme::Light.next(), Theme::Dark);
        assert_eq!(Theme::Dark.next(), Theme::Auto);
        for theme in [Theme::Auto, Theme::Light, Theme::Dark] {
            assert_eq!(Theme::from_value(theme.value()), theme);
        }
    }

    #[test]
    fn the_theme_link_returns_to_the_current_page() {
        let index = RepoIndex::from_listing(
            std::path::Path::new("."),
            &["docs/README.md"],
            &Extensions::default(),
            &[],
        );
        let config = Config::default();
        let view = View {
            theme: Theme::Auto,
            extensions: Extensions::default(),
            config: &config,
        };
        let page = layout("t", "docs/README.md", index.tree(), view, "", "");
        assert!(
            page.contains("href=\"/theme/light?next=%2Fdocs%2FREADME%2Emd\""),
            "{page}"
        );
        assert!(page.contains("data-theme=\"auto\""), "{page}");
    }

    #[test]
    fn a_document_offers_source_at_the_head_and_the_foot() {
        let index = RepoIndex::from_listing(
            std::path::Path::new("."),
            &["docs/README.md"],
            &Extensions::default(),
            &[],
        );
        let config = Config::default();
        let view = View {
            theme: Theme::Auto,
            extensions: Extensions::default(),
            config: &config,
        };
        let page = document_page(&index, "docs/README.md", "<p>text</p>", view);
        assert_eq!(
            page.matches("?raw=1").count(),
            2,
            "one link beside the path, one at the foot: {page}"
        );
        assert!(page.contains("class=\"page-head\""), "{page}");
        assert!(page.contains("class=\"action\""), "{page}");
    }

    #[test]
    fn the_breadcrumb_links_each_level() {
        let crumbs = breadcrumb("docs/guides/ui.md");
        assert!(crumbs.contains("href=\"/docs\""));
        assert!(crumbs.contains("href=\"/docs/guides\""));
        assert!(crumbs.contains("href=\"/docs/guides/ui.md\""));
    }
}
