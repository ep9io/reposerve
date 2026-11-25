//! Server-side syntax highlighting.
//!
//! The highlighter emits class names, not colours, and the stylesheet carries a
//! light and a dark palette. That keeps the colours in step with the page theme
//! without re-rendering: the classes are the same either way.

use std::sync::OnceLock;

use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{css_for_theme_with_class_style, ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// The class prefix, so a syntax class cannot collide with the page's own classes.
const CLASS_PREFIX: &str = "hl-";

fn class_style() -> ClassStyle {
    ClassStyle::SpacedPrefixed {
        prefix: CLASS_PREFIX,
    }
}

/// The light palette, used when the page is light.
const LIGHT_THEME: &str = "InspiredGitHub";
/// The dark palette, used when the page is dark.
const DARK_THEME: &str = "base16-ocean.dark";

/// A syntax set and two palettes, built once for the whole process.
pub struct Highlighter {
    syntaxes: SyntaxSet,
    light: Theme,
    dark: Theme,
}

/// The process-wide highlighter.
#[must_use]
pub fn global() -> &'static Highlighter {
    static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();
    HIGHLIGHTER.get_or_init(Highlighter::new)
}

impl Highlighter {
    fn new() -> Self {
        let syntaxes = two_face::syntax::extra_newlines();
        let themes = ThemeSet::load_defaults();
        let light = themes.themes.get(LIGHT_THEME).cloned().unwrap_or_default();
        let dark = themes.themes.get(DARK_THEME).cloned().unwrap_or_default();
        Self {
            syntaxes,
            light,
            dark,
        }
    }

    /// Highlights a file, choosing the syntax from its name and first line.
    ///
    /// Returns `None` when no syntax matches, so the caller can show plain text.
    #[must_use]
    pub fn for_path(&self, path: &str, source: &str) -> Option<String> {
        let name = path.rsplit('/').next().unwrap_or(path);
        let extension = name.rsplit_once('.').map(|(_, extension)| extension);
        let syntax = extension
            .and_then(|extension| self.syntaxes.find_syntax_by_extension(extension))
            .or_else(|| self.syntaxes.find_syntax_by_token(name))
            .or_else(|| {
                source
                    .lines()
                    .next()
                    .and_then(|line| self.syntaxes.find_syntax_by_first_line(line))
            })?;
        self.render(syntax, source)
    }

    /// Highlights a fenced code block, choosing the syntax from its label.
    #[must_use]
    pub fn for_language(&self, language: &str, source: &str) -> Option<String> {
        let syntax = self.syntaxes.find_syntax_by_token(language.trim())?;
        self.render(syntax, source)
    }

    fn render(&self, syntax: &SyntaxReference, source: &str) -> Option<String> {
        let mut generator =
            ClassedHTMLGenerator::new_with_class_style(syntax, &self.syntaxes, class_style());
        for line in syntect::util::LinesWithEndings::from(source) {
            generator
                .parse_html_for_line_which_includes_newline(line)
                .ok()?;
        }
        Some(generator.finalize())
    }

    /// The stylesheet for both palettes.
    ///
    /// The light rules are plain; the dark rules repeat under a device-preference
    /// query and under an explicit dark choice, so one stylesheet serves `Auto`,
    /// `Light` and `Dark`.
    #[must_use]
    pub fn css(&self) -> String {
        let light = css_for_theme_with_class_style(&self.light, class_style()).unwrap_or_default();
        let dark = css_for_theme_with_class_style(&self.dark, class_style()).unwrap_or_default();
        format!(
            "{}\n@media (prefers-color-scheme:dark){{\n{}\n}}\n{}\n",
            scoped(&light, ":root:not([data-theme=light])"),
            scoped(&dark, ":root:not([data-theme=light])"),
            scoped(&dark, ":root[data-theme=dark]"),
        )
    }
}

/// Prefixes every selector in a stylesheet, so it applies only under a scope.
fn scoped(css: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(css.len() + css.len() / 4);
    for rule in strip_comments(css).split('}') {
        let Some((selectors, body)) = rule.split_once('{') else {
            continue;
        };
        let selectors: Vec<String> = selectors
            .split(',')
            .map(str::trim)
            .filter(|selector| !selector.is_empty())
            .map(|selector| format!("{prefix} {selector}"))
            .collect();
        if selectors.is_empty() {
            continue;
        }
        // Only colour and style survive: the page decides every background, so a
        // palette cannot paint a white block inside a dark page.
        let declarations: Vec<&str> = body
            .split(';')
            .map(str::trim)
            .filter(|declaration| {
                !declaration.is_empty() && !declaration.starts_with("background-color")
            })
            .collect();
        if declarations.is_empty() {
            continue;
        }
        output.push_str(&selectors.join(","));
        output.push('{');
        output.push_str(&declarations.join(";"));
        output.push_str("}\n");
    }
    output
}

fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        output.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return output,
        }
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_source_becomes_coloured_spans() {
        let html = global().for_path("main.rs", "fn main() {\n    let x = 1;\n}\n");
        let html = html.expect("Rust has a syntax");
        assert!(html.contains("hl-source hl-rust"), "{html}");
        assert!(html.contains("fn"), "{html}");
    }

    #[test]
    fn an_unknown_type_has_no_highlighting() {
        assert!(global()
            .for_path("notes.unknownext", "plain words\n")
            .is_none());
    }

    #[test]
    fn a_fenced_block_uses_its_label() {
        let html = global().for_language("toml", "[a]\nb = 1\n");
        let html = html.expect("TOML has a syntax");
        assert!(html.contains("hl-source hl-toml"), "{html}");
    }

    #[test]
    fn the_stylesheet_scopes_the_dark_palette() {
        let css = global().css();

        assert!(
            css.contains(":root:not([data-theme=light]) .hl-"),
            "the light rules are present"
        );
        assert!(
            css.contains(":root[data-theme=dark] .hl-"),
            "the dark rules are scoped to an explicit choice"
        );
        assert!(
            css.contains("@media (prefers-color-scheme:dark)"),
            "the dark rules follow the device preference"
        );
        assert!(
            !css.contains("background-color"),
            "no palette may set a background"
        );
    }

    #[test]
    fn comments_are_stripped_before_scoping() {
        let css = strip_comments("/* Theme: x */\npre code { color: #000; }\n");
        assert_eq!(css.trim(), "pre code { color: #000; }");
    }
}
