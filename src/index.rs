//! The set of files the server may serve, and the directory tree built from it.
//!
//! The list comes from Git, never from a directory walk:
//! `git ls-files -z --cached --others --exclude-standard` names every tracked
//! file and every untracked file that is not ignored. That single choice keeps
//! `target/`, generated output and machine-local files such as `.env/local` out
//! of the server, and it makes path traversal impossible: a request is answered
//! only when its path is a key in this set.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::error::Error;

/// The file extensions the server shows.
///
/// The default is `md` alone, so the site opens as a documentation reader. A
/// reader widens it to see source or images. A file with no extension never
/// matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extensions(BTreeSet<String>);

impl Default for Extensions {
    fn default() -> Self {
        Self(BTreeSet::from(["md".to_string()]))
    }
}

impl Extensions {
    /// Reads a comma or space separated list. An empty list means the default.
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
        if set.is_empty() {
            Self::default()
        } else {
            Self(set)
        }
    }

    /// The list as it is stored in the cookie, without spaces.
    #[must_use]
    pub fn value(&self) -> String {
        self.0.iter().cloned().collect::<Vec<_>>().join(",")
    }

    /// The list as it is shown to a reader.
    #[must_use]
    pub fn label(&self) -> String {
        self.0.iter().cloned().collect::<Vec<_>>().join(", ")
    }

    /// Whether one extension is shown.
    #[must_use]
    pub fn contains(&self, extension: &str) -> bool {
        self.0.contains(extension)
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
fn walk_listing(root: &Path) -> Result<BTreeSet<String>, Error> {
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
            // A dot entry is a tool's own state or a secret, never a document.
            if name.starts_with('.') {
                continue;
            }
            if file_type.is_dir() {
                if !WALK_SKIP.contains(&name.as_str()) {
                    pending.push(entry.path());
                }
            } else if file_type.is_file() {
                if let Ok(relative) = entry.path().strip_prefix(root) {
                    files.insert(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    Ok(files)
}

/// Whether `path` sits under one of the included prefixes. An empty list
/// includes everything.
fn included(path: &str, include: &[String]) -> bool {
    include.is_empty()
        || include
            .iter()
            .any(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")))
}

/// The repository content, as one sorted set of paths plus the tree over them.
#[derive(Debug)]
pub struct RepoIndex {
    root: PathBuf,
    canonical_root: PathBuf,
    all: BTreeSet<String>,
    files: BTreeSet<String>,
    assets: BTreeSet<String>,
    tree: TreeNode,
}

impl RepoIndex {
    /// Reads the file list for the configured root and builds the tree.
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
        let root = &config.root;
        if !root.is_dir() {
            return Err(Error::RootUnavailable {
                path: root.to_path_buf(),
            });
        }
        let canonical_root = std::fs::canonicalize(root).map_err(|source| Error::Read {
            path: root.to_path_buf(),
            source,
        })?;

        let all: BTreeSet<String> = match git_listing(root) {
            Ok(paths) => paths,
            Err(Error::GitUnavailable { .. } | Error::NotAWorkTree { .. }) => walk_listing(root)?,
            Err(other) => return Err(other),
        }
        .into_iter()
        .filter(|path| included(path, &config.include))
        .collect();

        let files: BTreeSet<String> = all
            .iter()
            .filter(|path| config.extensions.matches(path))
            .cloned()
            .collect();
        let assets: BTreeSet<String> = all
            .iter()
            .filter(|path| is_asset(path, &config.assets))
            .cloned()
            .collect();

        Ok(Self::from_paths(root, canonical_root, all, files, assets))
    }

    /// Every path the index knows, whatever the filter shows.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.all.iter().map(String::as_str)
    }

    /// Builds an index over an explicit path list. Used by the tests.
    #[cfg(test)]
    #[must_use]
    pub fn from_listing(
        root: &Path,
        paths: &[&str],
        extensions: &Extensions,
        assets: &[String],
    ) -> Self {
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let files: BTreeSet<String> = paths
            .iter()
            .filter(|path| extensions.matches(path))
            .map(|path| (*path).to_string())
            .collect();
        let assets: BTreeSet<String> = paths
            .iter()
            .filter(|path| is_asset(path, assets))
            .map(|path| (*path).to_string())
            .collect();
        let all: BTreeSet<String> = paths.iter().map(|path| (*path).to_string()).collect();
        Self::from_paths(root, canonical_root, all, files, assets)
    }

    fn from_paths(
        root: &Path,
        canonical_root: PathBuf,
        all: BTreeSet<String>,
        files: BTreeSet<String>,
        assets: BTreeSet<String>,
    ) -> Self {
        let tree = build_tree(&files);
        Self {
            root: root.to_path_buf(),
            canonical_root,
            all,
            files,
            assets,
            tree,
        }
    }

    /// The whole tree, for the sidebar.
    #[must_use]
    pub fn tree(&self) -> &TreeNode {
        &self.tree
    }

    /// The number of files the current filter shows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the current filter shows no file at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The number of files the repository holds, whatever the filter shows.
    #[must_use]
    pub fn total(&self) -> usize {
        self.all.len()
    }

    /// Whether `path` names a file the server may read.
    ///
    /// A media file is served even when the filter hides it, so an image a
    /// document embeds still loads. It stays out of the tree and the listings.
    #[must_use]
    pub fn has_file(&self, path: &str) -> bool {
        self.files.contains(path) || self.assets.contains(path)
    }

    /// Whether `path` names a directory that holds at least one indexed file.
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

    /// Returns the indexed paths that contain `needle`, case-insensitively.
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

    /// Reads one indexed file, refusing anything that resolves outside the root.
    ///
    /// # Errors
    ///
    /// Fails when the path is not indexed, or when the file cannot be read.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        if !self.has_file(path) {
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

    fn index() -> RepoIndex {
        RepoIndex::from_listing(
            Path::new("."),
            &[
                "README.md",
                "docs/README.md",
                "docs/guides/ui.md",
                "docs/reference/config.md",
                "work/2026-11-audit/notes.md",
            ],
            &Extensions::default(),
            &assets(),
        )
    }

    #[test]
    fn a_file_is_only_served_when_the_index_lists_it() {
        let index = index();
        assert!(index.has_file("docs/README.md"));
        assert!(!index.has_file("target/debug/reposerve"));
        assert!(!index.has_file(".env/local"));
    }

    #[test]
    fn a_directory_is_known_when_it_holds_an_indexed_file() {
        let index = index();
        assert!(index.has_directory("docs"));
        assert!(index.has_directory(""));
        assert!(!index.has_directory("docs/missing"));
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
        let root = index.tree();
        assert_eq!(root.dirs.len(), 2);
        assert_eq!(root.dirs[0].name, "docs");
        assert_eq!(root.files[0].0, "README.md");
        let docs = &root.dirs[0];
        assert_eq!(docs.dirs.len(), 2);
        assert_eq!(docs.files[0].1, "docs/README.md");
    }

    #[test]
    fn the_default_filter_shows_markdown_alone() {
        let extensions = Extensions::default();
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
        assert_eq!(Extensions::parse(""), Extensions::default());
    }

    #[test]
    fn a_filter_hides_the_files_it_does_not_name() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["README.md", "logo.png", "crates/main.rs"],
            &Extensions::parse("png"),
            &assets(),
        );
        assert_eq!(index.len(), 1);
        assert!(index.has_file("logo.png"));
        assert!(!index.has_directory("crates"));
    }

    #[test]
    fn a_chain_of_single_directories_becomes_one_entry() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["a/b/c/one.md", "a/b/c/two.md", "a/README.md"],
            &Extensions::default(),
            &assets(),
        );
        let root = index.tree();
        let a = &root.dirs[0];
        assert_eq!(a.name, "a");
        let chain = &a.dirs[0];
        assert_eq!(chain.name, "b/c", "the chain is shown as one entry");
        assert_eq!(chain.path, "a/b/c");
        assert_eq!(chain.files.len(), 2);
    }

    #[test]
    fn a_directory_that_cannot_be_skipped_is_its_own_target() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["a/b/c/one.md", "a/README.md"],
            &Extensions::default(),
            &assets(),
        );
        assert_eq!(index.collapsed_directory("a"), Some("a".to_string()));
        assert_eq!(
            index.collapsed_directory("a/b"),
            Some("a/b/c".to_string()),
            "a path that a reader typed still lands on the deepest directory"
        );
        assert_eq!(
            index.collapsed_directory("a/b/c"),
            Some("a/b/c".to_string())
        );
        assert_eq!(index.collapsed_directory("missing"), None);
    }

    #[test]
    fn a_media_file_loads_without_appearing_in_the_tree() {
        let index = RepoIndex::from_listing(
            Path::new("."),
            &["docs/README.md", "docs/ui/sign-in.webp"],
            &Extensions::default(),
            &assets(),
        );
        assert!(
            index.has_file("docs/ui/sign-in.webp"),
            "the image still loads"
        );
        assert!(
            !index.has_directory("docs/ui"),
            "but its directory is not listed"
        );
        assert!(index.has_file("docs/README.md"));
    }

    #[test]
    fn search_matches_any_part_of_a_path() {
        let index = index();
        assert_eq!(index.search("GUIDES"), vec!["docs/guides/ui.md"]);
        assert!(index.search("   ").is_empty());
    }
}
