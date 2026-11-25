//! A small cache of rendered pages.
//!
//! Rendering a page costs a few milliseconds, and a reader who moves back and
//! forth asks for the same page again. The cache holds the rendered HTML until
//! a file changes, when the whole cache is dropped. Dropping everything is
//! simpler than tracking which page one edit touched, and it is always correct.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The largest number of pages the cache holds.
const MAX_ENTRIES: usize = 256;

/// One rendered page.
#[derive(Debug)]
pub struct Page {
    /// The complete HTML.
    pub html: String,
    /// The entity tag of that HTML.
    pub etag: String,
}

/// The process-wide page cache.
#[derive(Debug, Default)]
pub struct PageCache {
    entries: Mutex<HashMap<String, Arc<Page>>>,
}

impl PageCache {
    /// Returns a cached page, when the key is still present.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Arc<Page>> {
        self.entries.lock().ok()?.get(key).cloned()
    }

    /// Stores a page under a key.
    pub fn insert(&self, key: String, html: String) {
        let etag = etag(&html);
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(key, Arc::new(Page { html, etag }));
    }

    /// Drops every page, because a file changed.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    /// The number of pages held. Used by the tests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }

    /// Whether the cache holds no page.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The entity tag of a body.
#[must_use]
pub fn etag(body: &str) -> String {
    etag_bytes(body.as_bytes())
}

/// The entity tag of a byte body.
#[must_use]
pub fn etag_bytes(body: &[u8]) -> String {
    format!("\"{}\"", crate::web::assets::hash_of(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_page_comes_back_until_the_cache_is_cleared() {
        let cache = PageCache::default();
        cache.insert("k".to_string(), "<p>x</p>".to_string());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("k").expect("a hit").html, "<p>x</p>");
        cache.clear();
        assert!(cache.get("k").is_none());
    }

    #[test]
    fn the_entity_tag_changes_with_the_body() {
        assert_ne!(etag("a"), etag("b"));
        assert_eq!(etag("a"), etag("a"));
        assert!(etag("a").starts_with('"'));
    }

    #[test]
    fn the_cache_empties_itself_when_it_is_full() {
        let cache = PageCache::default();
        for index in 0..MAX_ENTRIES + 1 {
            cache.insert(index.to_string(), "x".to_string());
        }
        assert!(cache.len() <= MAX_ENTRIES);
    }
}
