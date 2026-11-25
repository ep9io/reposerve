//! Small helpers that turn text into safe HTML and URLs.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS, NON_ALPHANUMERIC};

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

/// One kibibyte. Sizes and limits stay readable by naming the unit.
pub const KIB: usize = 1024;

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

/// Percent-encodes a whole value for use as a query parameter.
#[must_use]
pub fn query_value(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// Formats a byte count for a reader.
#[must_use]
pub fn size_label(size: usize) -> String {
    if size < KIB {
        format!("{size} bytes")
    } else if size < KIB * KIB {
        format!("{:.1} KiB", size as f64 / KIB as f64)
    } else {
        format!("{:.1} MiB", size as f64 / (KIB * KIB) as f64)
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
    fn sizes_use_the_largest_sensible_unit() {
        assert_eq!(size_label(512), "512 bytes");
        assert_eq!(size_label(2048), "2.0 KiB");
        assert_eq!(size_label(3 * KIB * KIB), "3.0 MiB");
    }
}
