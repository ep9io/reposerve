//! A small word counter, to show how Rust is coloured on this page.

use std::collections::BTreeMap;

/// The number of times each word occurs, in alphabetical order.
#[derive(Debug, Default)]
pub struct Counts {
    words: BTreeMap<String, usize>,
}

impl Counts {
    /// Counts the words in one line of text.
    pub fn add(&mut self, line: &str) {
        for word in line.split_whitespace() {
            let key = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            if !key.is_empty() {
                *self.words.entry(key).or_insert(0) += 1;
            }
        }
    }

    /// The words seen more than once, most frequent first.
    pub fn repeated(&self) -> Vec<(&str, usize)> {
        let mut seen: Vec<(&str, usize)> = self
            .words
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(word, count)| (word.as_str(), *count))
            .collect();
        seen.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        seen
    }
}

fn main() {
    let text = "the quick brown fox jumps over the lazy dog, the dog sleeps";
    let mut counts = Counts::default();
    counts.add(text);

    for (word, count) in counts.repeated() {
        println!("{word:<8} {count}");
    }
}
