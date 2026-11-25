//! The files a page needs, embedded in the binary.
//!
//! Every asset is served under a URL that carries a fingerprint of its content.
//! A changed asset therefore has a new URL, and the browser can cache the old
//! one for a year without risk.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One embedded file.
pub struct Asset {
    /// The path below `assets/`, which is also the last part of its URL.
    pub name: &'static str,
    /// The bytes, embedded at compile time.
    pub bytes: &'static [u8],
}

/// The stylesheet for every page.
pub const APP_CSS: &str = "app.css";
/// The one script the shell loads.
pub const APP_JS: &str = "app.js";
/// The site icon.
pub const ICON: &str = "icon.svg";
/// The diagram script. It loads only on a page that holds a diagram.
pub const MERMAID: &str = "mermaid/mermaid.min.js";
/// The math script. It loads only on a page that holds math.
pub const KATEX_JS: &str = "katex/katex.min.js";
/// The math stylesheet and its fonts.
pub const KATEX_CSS: &str = "katex/katex.min.css";

macro_rules! embed {
    ($($name:literal),* $(,)?) => {
        /// Every embedded file.
        pub const ALL: &[Asset] = &[
            $(Asset {
                name: $name,
                bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $name)),
            },)*
        ];
    };
}

embed! {
    "app.css",
    "app.js",
    "icon.svg",
    "mermaid/mermaid.min.js",
    "katex/katex.min.js",
    "katex/katex.min.css",
    "katex/fonts/KaTeX_AMS-Regular.woff2",
    "katex/fonts/KaTeX_Caligraphic-Bold.woff2",
    "katex/fonts/KaTeX_Caligraphic-Regular.woff2",
    "katex/fonts/KaTeX_Fraktur-Bold.woff2",
    "katex/fonts/KaTeX_Fraktur-Regular.woff2",
    "katex/fonts/KaTeX_Main-Bold.woff2",
    "katex/fonts/KaTeX_Main-BoldItalic.woff2",
    "katex/fonts/KaTeX_Main-Italic.woff2",
    "katex/fonts/KaTeX_Main-Regular.woff2",
    "katex/fonts/KaTeX_Math-BoldItalic.woff2",
    "katex/fonts/KaTeX_Math-Italic.woff2",
    "katex/fonts/KaTeX_SansSerif-Bold.woff2",
    "katex/fonts/KaTeX_SansSerif-Italic.woff2",
    "katex/fonts/KaTeX_SansSerif-Regular.woff2",
    "katex/fonts/KaTeX_Script-Regular.woff2",
    "katex/fonts/KaTeX_Size1-Regular.woff2",
    "katex/fonts/KaTeX_Size2-Regular.woff2",
    "katex/fonts/KaTeX_Size3-Regular.woff2",
    "katex/fonts/KaTeX_Size4-Regular.woff2",
    "katex/fonts/KaTeX_Typewriter-Regular.woff2",
}

/// Finds an embedded file by name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Asset> {
    ALL.iter().find(|asset| asset.name == name)
}

/// The content type of an embedded file, from its extension.
#[must_use]
pub fn content_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension {
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// The URL of an embedded file, with the fingerprint of its content.
///
/// An unknown name yields a URL that no route answers, which is a 404 rather
/// than a wrong file.
#[must_use]
pub fn url(name: &str) -> String {
    match fingerprint(name) {
        Some(hash) => format!("/assets/{hash}/{name}"),
        None => format!("/assets/unknown/{name}"),
    }
}

/// The fingerprint of one asset, or `None` when it is not embedded.
#[must_use]
pub fn fingerprint(name: &str) -> Option<&'static str> {
    fingerprints().get(name).map(String::as_str)
}

fn fingerprints() -> &'static HashMap<&'static str, String> {
    static FINGERPRINTS: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    FINGERPRINTS.get_or_init(|| {
        ALL.iter()
            .map(|asset| (asset.name, hash_of(asset.bytes)))
            .collect()
    })
}

/// A short FNV-1a hash. It only has to change when the bytes change.
#[must_use]
pub fn hash_of(bytes: &[u8]) -> String {
    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{value:016x}").chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asset_has_a_content_type() {
        for asset in ALL {
            assert_ne!(
                content_type(asset.name),
                "application/octet-stream",
                "{} has no content type",
                asset.name
            );
        }
    }

    #[test]
    fn a_url_carries_the_fingerprint_of_its_content() {
        let url = url(APP_CSS);
        let fingerprint = fingerprint(APP_CSS).expect("app.css is embedded");
        assert!(url.contains(fingerprint), "{url}");
        assert!(url.starts_with("/assets/"), "{url}");
        assert!(url.ends_with("/app.css"), "{url}");
    }

    #[test]
    fn an_unknown_asset_has_no_fingerprint() {
        assert!(fingerprint("missing.css").is_none());
        assert!(url("missing.css").contains("unknown"));
    }

    #[test]
    fn the_math_fonts_travel_with_the_binary() {
        assert!(find("katex/fonts/KaTeX_Main-Regular.woff2").is_some());
        assert!(find(KATEX_JS).is_some());
    }
}
