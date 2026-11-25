//! The set of files the server may serve, and the tree built over a subset.
//!
//! The list comes from Git, never from a directory walk:
//! `git ls-files -z --cached --others --exclude-standard` names every tracked
//! file and every untracked file that is not ignored. That single choice keeps
//! `target/`, generated output and machine-local files such as `.env/local` out
//! of the server, and it makes path traversal impossible: a request is answered
//! only when its path is a key in this set.
//!
//! [`RepoIndex`] holds every path and does not depend on the file-type filter.
//! [`Listing`] applies the filter and builds the tree. The index is therefore
//! cheap to cache across requests, whatever filter each reader chooses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::error::Error;

/// The value stored in the filter cookie when the reader wants no file shown.
///
/// A single dot cannot be an extension, so `parse` drops it and the set stays
/// empty. An empty cookie value would be a deleted cookie in some browsers.
const NONE_VALUE: &str = ".";

/// The file extensions the server shows.
///
/// A file with no extension never matches. An empty set shows nothing, which is
/// a choice a reader can make on the filter form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extensions(BTreeSet<String>);

impl Extensions {
    /// Reads a comma or space separated list. An empty list means nothing shown.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let mut set = BTreeSet::new();
        for token in value.split([',', ' ', ';']) {
            let token = token
                .trim()
                .trim_start_matches('*')
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if !token.is_empty()
                && token.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '+')
                })
            {
                set.insert(token);
            }
        }
        Self(set)
    }

    /// The list as it is stored in the cookie, without spaces.
    #[must_use]
    pub fn value(&self) -> String {
        if self.0.is_empty() {
            NONE_VALUE.to_string()
        } else {
            self.0.iter().cloned().collect::<Vec<_>>().join(",")
        }
    }

    /// The list as it is shown to a reader.
    #[must_use]
    pub fn label(&self) -> String {
        if self.0.is_empty() {
            "no file type".to_string()
        } else {
            self.0.iter().cloned().collect::<Vec<_>>().join(", ")
        }
    }

    /// Whether one extension is shown.
    #[must_use]
    pub fn contains(&self, extension: &str) -> bool {
        self.0.contains(extension)
    }

    /// Whether the filter shows no file at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The number of file types the filter shows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The entries that are not one of the advertised extensions.
    #[must_use]
    pub fn custom(&self, presets: &[String]) -> String {
        self.0
            .iter()
            .filter(|extension| !presets.contains(extension))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Whether a path is shown. The comparison ignores case.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.rsplit_once('.')
            .is_some_and(|(_, extension)| self.0.contains(&extension.to_ascii_lowercase()))
    }
}

/// One directory of the tree the sidebar renders.
#[derive(Debug, Default)]
pub struct TreeNode {
    /// The directory's own name; empty for the root.
    pub name: String,
    /// The repository-relative path; empty for the root.
    pub path: String,
    /// Subdirectories, ordered by name.
    pub dirs: Vec<Self>,
    /// Files in this directory, ordered by name.
    pub files: Vec<(String, String)>,
}

/// Directories a plain walk never enters.
const WALK_SKIP: [&str; 3] = [".git", "target", "node_modules"];

/// Whether a path carries an extension the configuration always serves.
///
/// An image, a PDF or a font is page furniture, not a destination, so a document
/// that embeds one keeps working when the filter hides those types.
#[must_use]
pub fn is_asset(path: &str, assets: &[String]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| assets.contains(&extension.to_ascii_lowercase()))
}

/// Lists a working tree through Git.
///
/// # Errors
///
/// Fails when `git` cannot be started, or when it refuses the directory because
/// it is not a working tree.
fn git_listing(root: &Path) -> Result<BTreeSet<String>, Error> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .map_err(|source| Error::GitUnavailable {
            root: root.to_path_buf(),
            source,
        })?;

    if !output.status.success() {
        return Err(Error::NotAWorkTree {
            root: root.to_path_buf(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

/// Lists a plain directory, one pass per directory, without following links.
///
/// Dot entries are skipped: a `.env`, a `.git` or a `.venv` is a tool's own
/// state, and the walk has no ignore file to consult.
///
/// # Errors
///
/// Fails when a directory cannot be read.
fn walk_listing(root: &Path, hidden: bool) -> Result<BTreeSet<String>, Error> {
    let mut files = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|source| Error::Walk {
            path: directory.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Walk {
                path: directory.clone(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| Error::Walk {
                path: directory.clone(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // A tool keeps its own state in a dot directory. Even a reader who
            // asks for hidden entries does not want `.git` in the tree.
            if WALK_SKIP.contains(&name.as_str()) {
                continue;
            }
            if name.starts_with('.') && !hidden {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(relative) = entry.path().strip_prefix(root) {
                    files.insert(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    Ok(files)
}

/// Removes every path that holds a component starting with a dot.
///
/// `git ls-files` lists a tracked `.gitignore` and a tracked `.github/`, while a
/// plain walk has to choose. This function applies the choice to both.
fn without_hidden(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .into_iter()
        .filter(|path| !path.split('/').any(|part| part.starts_with('.')))
        .collect()
}

/// Whether `path` sits under one of the included prefixes. An empty list
/// includes everything.
fn included(path: &str, include: &[String]) -> bool {
    include.is_empty()
        || include
            .iter()
            .any(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")))
}

/// Everything the server may read: every path, split into documents and assets.
#[derive(Debug)]
pub struct RepoIndex {
    root: PathBuf,
    canonical_root: PathBuf,
    all: BTreeSet<String>,
    assets: BTreeSet<String>,
}

impl RepoIndex {
    /// Reads the file list for the configured root.
    ///
    /// The list comes from Git when the root is a working tree, so ignored and
    /// generated files stay out of the site. A plain directory is walked
    /// instead, and the walk skips `.git`, `target` and `node_modules`.
    ///
    /// # Errors
    ///
    /// Fails when the root is not a directory, when Git lists it but the listing
    /// cannot be read, or when a plain directory cannot be walked.
    pub fn load(config: &Config) -> Result<Self, Error> {
        if !config.root.is_dir() {
            return Err(Error::RootUnavailable {
                path: config.root.clone(),
            });
        }
        let canonical_root = std::fs::canonicalize(&config.root).map_err(|source| Error::Read {
            path: config.root.clone(),
            source,
        })?;

        let all: BTreeSet<String> = match git_listing(&config.root) {
            Ok(paths) => paths,
            Err(Error::GitUnavailable { .. } | Error::NotAWorkTree { .. }) => {
                walk_listing(&config.root, config.hidden)?
            }
            Err(other) => return Err(other),
        }
        .into_iter()
        .filter(|path| included(path, &config.include))
        .collect();
        let all = if config.hidden {
            all
        } else {
            without_hidden(all)
        };

        let assets: BTreeSet<String> = all
            .iter()
            .filter(|path| is_asset(path, &config.assets))
            .cloned()
            .collect();

        Ok(Self {
            root: config.root.clone(),
            canonical_root,
            all,
            assets,
        })
    }

    /// Builds an index over an explicit path list. Used by the tests.
    #[cfg(test)]
    #[must_use]
    pub fn from_listing(root: &Path, paths: &[&str], assets: &[String]) -> Self {
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let all: BTreeSet<String> = paths.iter().map(|path| (*path).to_string()).collect();
        let assets: BTreeSet<String> = all
            .iter()
            .filter(|path| is_asset(path, assets))
            .cloned()
            .collect();
        Self {
            root: root.to_path_buf(),
            canonical_root,
            all,
            assets,
        }
    }

    /// The number of paths the index holds.
    #[must_use]
    pub fn total(&self) -> usize {
        self.all.len()
    }

    /// Every path the index knows, whatever the filter shows.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.all.iter().map(String::as_str)
    }

    /// Whether the index names this path, as a document or an asset.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.all.contains(path) || self.assets.contains(path)
    }

    /// How many files the repository holds of each type.
    ///
    /// A file of a type that the current filter hides still counts, so the filter
    /// form can offer every type the repository holds. A file with no extension
    /// has no type and is not counted.
    #[must_use]
    pub fn type_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for path in &self.all {
            let name = path.rsplit('/').next().unwrap_or(path);
            if let Some((_, extension)) = name.rsplit_once('.') {
                *counts.entry(extension.to_ascii_lowercase()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Applies a file-type filter and builds the tree over what is left.
    #[must_use]
    pub fn listing(&self, extensions: &Extensions) -> Listing<'_> {
        let files: BTreeSet<String> = self
            .all
            .iter()
            .filter(|path| extensions.matches(path))
            .cloned()
            .collect();
        let tree = build_tree(&files);
        Listing {
            index: self,
            files,
            tree,
        }
    }

    /// Reads one indexed file, refusing anything that resolves outside the root.
    ///
    /// # Errors
    ///
    /// Fails when the path is not indexed, or when the file cannot be read.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        if !self.contains(path) {
            return Err(Error::NotFound {
                path: path.to_string(),
            });
        }
        let absolute = self.root.join(path);
        let resolved = std::fs::canonicalize(&absolute).map_err(|source| Error::Read {
            path: absolute.clone(),
            source,
        })?;
        if !resolved.starts_with(&self.canonical_root) {
            // A symlink inside the tree that points outside it. The index lists the
            // link, not its target, so the content is not repository content.
            return Err(Error::NotFound {
                path: path.to_string(),
            });
        }
        std::fs::read(&resolved).map_err(|source| Error::Read {
            path: absolute,
            source,
        })
    }
}

/// The files one reader's filter shows, plus the tree over them.
#[derive(Debug)]
pub struct Listing<'a> {
    index: &'a RepoIndex,
    files: BTreeSet<String>,
    tree: TreeNode,
}

impl Listing<'_> {
    /// How many files the repository holds of each type, shown or not.
    ///
    /// The filter form offers every type here, so a reader can turn on a type
    /// that the current filter hides.
    #[must_use]
    pub fn type_counts(&self) -> BTreeMap<String, usize> {
        self.index.type_counts()
    }
}

impl Listing<'_> {
    /// The whole tree, for the sidebar.
    #[must_use]
    pub fn tree(&self) -> &TreeNode {
        &self.tree
    }

    /// The number of files the filter shows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Every shown path, in order. Used by the quick-open palette.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(String::as_str)
    }

    /// Whether the filter shows no file at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Whether `path` names a file the server may read.
    ///
    /// A media file is served even when the filter hides it, so an image a
    /// document embeds still loads. It stays out of the tree and the listings.
    #[must_use]
    pub fn has_file(&self, path: &str) -> bool {
        self.files.contains(path) || self.index.assets.contains(path)
    }

    /// Whether `path` names a directory that holds at least one shown file.
    #[must_use]
    pub fn has_directory(&self, path: &str) -> bool {
        if path.is_empty() {
            return !self.files.is_empty();
        }
        let prefix = format!("{path}/");
        self.files
            .range(prefix.clone()..)
            .next()
            .is_some_and(|file| file.starts_with(&prefix))
    }

    /// The deepest directory on the single chain that starts at `path`.
    ///
    /// A directory that holds no matching file and has exactly one subdirectory
    /// adds nothing for a reader, so the server skips it: `/a` becomes `/a/b/c`
    /// when only files under `c` match. Returns `None` when `path` holds nothing.
    #[must_use]
    pub fn collapsed_directory(&self, path: &str) -> Option<String> {
        if !self.has_directory(path) {
            return None;
        }
        let mut directory = path.to_string();
        loop {
            let prefix = if directory.is_empty() {
                String::new()
            } else {
                format!("{directory}/")
            };
            let mut children: BTreeSet<String> = BTreeSet::new();
            let mut has_files = false;
            for file in self.files.range(prefix.clone()..) {
                let Some(rest) = file.strip_prefix(prefix.as_str()) else {
                    break;
                };
                match rest.split_once('/') {
                    Some((name, _)) => {
                        children.insert(name.to_string());
                    }
                    None => has_files = true,
                }
            }
            if has_files || children.len() != 1 {
                return Some(directory);
            }
            let Some(name) = children.into_iter().next() else {
                return Some(directory);
            };
            directory = if directory.is_empty() {
                name
            } else {
                format!("{directory}/{name}")
            };
        }
    }

    /// The shown paths that contain `needle`, case-insensitively.
    #[must_use]
    pub fn search(&self, needle: &str) -> Vec<&str> {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.files
            .iter()
            .filter(|path| path.to_lowercase().contains(&needle))
            .map(String::as_str)
            .collect()
    }

    /// The Markdown document a directory offers as its front page.
    ///
    /// A reader who opens `docs/` wants `docs/README.md`, not a list of names.
    #[must_use]
    pub fn front_page(&self, directory: &str) -> Option<String> {
        let candidate = if directory.is_empty() {
            "README.md".to_string()
        } else {
            format!("{directory}/README.md")
        };
        self.files.contains(&candidate).then_some(candidate)
    }

    /// The files before and after `path` in its own directory.
    ///
    /// The order is the order the tree shows, so `next` is the file below this
    /// one in the sidebar.
    #[must_use]
    pub fn neighbours(&self, path: &str) -> (Option<&str>, Option<&str>) {
        if !self.files.contains(path) {
            return (None, None);
        }
        let (directory, name) = match path.rsplit_once('/') {
            Some((directory, name)) => (directory, name),
            None => ("", path),
        };
        let prefix = if directory.is_empty() {
            String::new()
        } else {
            format!("{directory}/")
        };
        let mut previous = None;
        let mut found = false;
        for file in self.files.range(prefix.clone()..) {
            let Some(rest) = file.strip_prefix(prefix.as_str()) else {
                break;
            };
            if rest.contains('/') {
                // Another directory, and a different neighbour.
                continue;
            }
            if found {
                return (previous, Some(file.as_str()));
            }
            if rest == name {
                found = true;
            } else {
                previous = Some(file.as_str());
            }
        }
        (previous, None)
    }

    /// Reads one shown file.
    ///
    /// # Errors
    ///
    /// Fails when the path is not shown, or when the file cannot be read.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        if !self.has_file(path) {
            return Err(Error::NotFound {
                path: path.to_string(),
            });
        }
        self.index.read(path)
    }
}

/// Turns a requested URL path into a repository-relative path.
///
/// Returns `None` when the path could address something outside the repository,
/// or when it is not a plain path. An empty string means the root.
#[must_use]
pub fn normalise_request_path(raw: &str) -> Option<String> {
    if raw.contains('\0') || raw.contains('\\') {
        return None;
    }
    let mut segments = Vec::new();
    for segment in raw.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

#[derive(Debug, Default)]
struct DirBuilder {
    dirs: BTreeMap<String, Self>,
    files: BTreeSet<String>,
}

fn build_tree(files: &BTreeSet<String>) -> TreeNode {
    let mut root = DirBuilder::default();
    for file in files {
        let segments: Vec<&str> = file.split('/').collect();
        insert(&mut root, &segments);
    }
    let mut tree = into_node(&root, String::new(), String::new());
    // Merge a chain of single-child directories into one entry, so a reader
    // never steps through a directory that holds nothing they asked for.
    for child in &mut tree.dirs {
        collapse(child);
    }
    tree
}

fn collapse(node: &mut TreeNode) {
    for child in &mut node.dirs {
        collapse(child);
    }
    while node.files.is_empty() && node.dirs.len() == 1 {
        let child = node.dirs.remove(0);
        node.name = format!("{}/{}", node.name, child.name);
        node.path = child.path;
        node.dirs = child.dirs;
        node.files = child.files;
    }
}

fn insert(builder: &mut DirBuilder, segments: &[&str]) {
    match segments {
        [] => {}
        [file] => {
            builder.files.insert((*file).to_string());
        }
        [directory, rest @ ..] => {
            let child = builder.dirs.entry((*directory).to_string()).or_default();
            insert(child, rest);
        }
    }
}

fn into_node(builder: &DirBuilder, name: String, path: String) -> TreeNode {
    let dirs = builder
        .dirs
        .iter()
        .map(|(child_name, child)| {
            let child_path = if path.is_empty() {
                child_name.clone()
            } else {
                format!("{path}/{child_name}")
            };
            into_node(child, child_name.clone(), child_path)
        })
        .collect();
    let files = builder
        .files
        .iter()
        .map(|file| {
            let file_path = if path.is_empty() {
                file.clone()
            } else {
                format!("{path}/{file}")
            };
            (file.clone(), file_path)
        })
        .collect();
    TreeNode {
        name,
        path,
        dirs,
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Vec<String> {
        crate::config::DEFAULT_ASSETS
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }

    fn presets() -> Vec<String> {
        crate::config::DEFAULT_PRESETS
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }

    const PATHS: [&str; 5] = [
        "README.md",
        "docs/README.md",
        "docs/guides/ui.md",
        "docs/reference/config.md",
        "work/2026-11-audit/notes.md",
    ];

    fn index() -> RepoIndex {
        RepoIndex::from_listing(Path::new("."), &PATHS, &assets())
    }

    fn markdown() -> Extensions {
        Extensions::parse("md")
    }

    #[test]
    fn a_file_is_only_served_when_the_index_lists_it() {
        let index = index();
        let listing = index.listing(&markdown());
        assert!(listing.has_file("docs/README.md"));
        assert!(!listing.has_file("target/debug/reposerve"));
        assert!(!listing.has_file(".env/local"));
    }

    #[test]
    fn a_directory_is_known_when_it_holds_a_shown_file() {
        let index = index();
        let listing = index.listing(&markdown());
        assert!(listing.has_directory("docs"));
        assert!(listing.has_directory(""));
        assert!(!listing.has_directory("docs/missing"));
    }

    #[test]
    fn traversal_attempts_are_rejected() {
        assert_eq!(
            normalise_request_path("/docs/README.md"),
            Some("docs/README.md".into())
        );
        assert_eq!(
            normalise_request_path("docs//guides/./ui.md"),
            Some("docs/guides/ui.md".into())
        );
        assert_eq!(normalise_request_path("/"), Some(String::new()));
        assert_eq!(normalise_request_path("../secrets"), None);
        assert_eq!(normalise_request_path("docs/../../etc/passwd"), None);
        assert_eq!(normalise_request_path("docs\\windows"), None);
    }

    #[test]
    fn the_tree_lists_directories_before_files() {
        let index = index();
        let listing = index.listing(&markdown());
        let root = listing.tree();
        assert_eq!(root.dirs.len(), 2);
        assert_eq!(root.dirs[0].name, "docs");
        assert_eq!(root.files[0].0, "README.md");
        let docs = &root.dirs[0];
        assert_eq!(docs.dirs.len(), 2);
        assert_eq!(docs.files[0].1, "docs/README.md");
    }

    #[test]
    fn the_default_filter_shows_markdown_alone() {
        let extensions = Extensions::parse("md");
        assert_eq!(extensions.value(), "md");
        assert!(extensions.matches("docs/README.md"));
        assert!(!extensions.matches("crates/main.rs"));
        assert!(!extensions.matches("logo.png"));
        assert!(!extensions.matches("justfile"));
    }

    #[test]
    fn a_custom_list_accepts_dots_spaces_and_case() {
        let extensions = Extensions::parse(" .RS, png;ts md ");
        assert_eq!(extensions.value(), "md,png,rs,ts");
        assert_eq!(extensions.custom(&presets()), "ts");
        assert!(extensions.matches("crates/shared/src/lib.RS"));
        assert!(!extensions.matches("docs/README.md.bak"));
    }

    #[test]
    fn an_empty_filter_shows_nothing_and_survives_a_cookie() {
        let extensions = Extensions::parse("");
        assert!(extensions.is_empty());
        assert_eq!(extensions.label(), "no file type");
        let stored = extensions.value();
        assert_eq!(stored, NONE_VALUE, "an empty cookie value can be dropped");
        assert_eq!(
            Extensions::parse(&stored),
            extensions,
            "the stored value parses back to nothing"
        );
    }

    #[test]
    fn a_filter_hides_the_files_it_does_not_name() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["README.md", "logo.png", "crates/main.rs"],
            &assets(),
        );
        let listing = index.listing(&Extensions::parse("png"));
        assert_eq!(listing.len(), 1);
        assert!(listing.has_file("logo.png"));
        assert!(!listing.has_directory("crates"));
    }

    #[test]
    fn a_chain_of_single_directories_becomes_one_entry() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["a/b/c/one.md", "a/b/c/two.md", "a/README.md"],
            &assets(),
        );
        let listing = index.listing(&markdown());
        let root = listing.tree();
        let a = &root.dirs[0];
        assert_eq!(a.name, "a");
        let chain = &a.dirs[0];
        assert_eq!(chain.name, "b/c", "the chain is shown as one entry");
        assert_eq!(chain.path, "a/b/c");
        assert_eq!(chain.files.len(), 2);
    }

    #[test]
    fn a_directory_that_cannot_be_skipped_is_its_own_target() {
        let index =
            RepoIndex::from_listing(Path::new("."), &["a/b/c/one.md", "a/README.md"], &assets());
        let listing = index.listing(&markdown());
        assert_eq!(listing.collapsed_directory("a"), Some("a".to_string()));
        assert_eq!(
            listing.collapsed_directory("a/b"),
            Some("a/b/c".to_string()),
            "a path that a reader typed still lands on the deepest directory"
        );
        assert_eq!(
            listing.collapsed_directory("a/b/c"),
            Some("a/b/c".to_string())
        );
        assert_eq!(listing.collapsed_directory("missing"), None);
    }

    #[test]
    fn a_media_file_loads_without_appearing_in_the_tree() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["docs/README.md", "docs/ui/sign-in.webp"],
            &assets(),
        );
        let listing = index.listing(&markdown());
        assert!(
            listing.has_file("docs/ui/sign-in.webp"),
            "the image still loads"
        );
        assert!(
            !listing.has_directory("docs/ui"),
            "but its directory is not listed"
        );
        assert!(listing.has_file("docs/README.md"));
    }

    #[test]
    fn search_matches_any_part_of_a_path() {
        let index = index();
        let listing = index.listing(&markdown());
        assert_eq!(listing.search("GUIDES"), vec!["docs/guides/ui.md"]);
        assert!(listing.search("   ").is_empty());
    }

    #[test]
    fn a_hidden_entry_is_left_out_unless_it_is_asked_for() {
        let paths: BTreeSet<String> = [
            "README.md",
            ".gitignore",
            ".github/workflows/check.yml",
            "docs/.notes.md",
        ]
        .iter()
        .map(|path| (*path).to_string())
        .collect();
        let kept = without_hidden(paths);
        assert_eq!(kept.into_iter().collect::<Vec<String>>(), vec!["README.md"]);
    }

    #[test]
    fn a_directory_front_page_is_its_readme() {
        let index = index();
        let listing = index.listing(&markdown());
        assert_eq!(listing.front_page(""), Some("README.md".to_string()));
        assert_eq!(
            listing.front_page("docs"),
            Some("docs/README.md".to_string())
        );
        assert_eq!(listing.front_page("work"), None);
    }

    #[test]
    fn the_files_before_and_after_are_in_tree_order() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["a/one.md", "a/two.md", "a/three.md", "b/four.md"],
            &assets(),
        );
        let listing = index.listing(&markdown());
        // The index sorts by path bytes, so `one` comes before `three`.
        assert_eq!(listing.neighbours("a/one.md"), (None, Some("a/three.md")));
        assert_eq!(
            listing.neighbours("a/three.md"),
            (Some("a/one.md"), Some("a/two.md"))
        );
        assert_eq!(listing.neighbours("a/two.md"), (Some("a/three.md"), None));
        assert_eq!(
            listing.neighbours("b/four.md"),
            (None, None),
            "a directory with one file has no neighbours"
        );
        assert_eq!(
            listing.neighbours("a/missing.md"),
            (None, None),
            "a path that is not shown has no neighbours"
        );
    }
}
