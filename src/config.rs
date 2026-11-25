//! Configuration: defaults, an optional TOML file, and command-line overrides.
//!
//! Precedence is fixed and testable: a command-line value wins over the file,
//! and the file wins over the default. A caller that passes no file gets the
//! defaults for everything it does not name.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;
use crate::index::Extensions;

/// The extensions always served, because a page may embed one of them.
pub const DEFAULT_ASSETS: [&str; 11] = [
    "png", "jpg", "jpeg", "gif", "webp", "avif", "ico", "svg", "pdf", "woff", "woff2",
];

/// The extensions offered as checkboxes in the filter form.
pub const DEFAULT_PRESETS: [&str; 5] = ["md", "rs", "ipynb", "png", "webp"];

/// The default port. It avoids the usual development ports of 3000, 5173, 8080
/// and 8443.
pub const DEFAULT_PORT: u16 = 8095;

/// How the server behaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The directory to serve. Every path the server answers comes from here.
    pub root: PathBuf,
    /// The address to listen on. `0.0.0.0` reaches the local network.
    pub bind: String,
    /// The port to listen on.
    pub port: u16,
    /// The name shown in the header and in the browser title.
    pub title: String,
    /// A token required on the first visit. `None` lets every request through.
    pub token: Option<String>,
    /// Whether a page reloads itself when a file changes.
    pub live_reload: bool,
    /// The extensions shown when the request carries no filter cookie.
    pub extensions: Extensions,
    /// The extensions offered as checkboxes.
    pub presets: Vec<String>,
    /// Directory prefixes to serve. Empty means the whole tree.
    pub include: Vec<String>,
    /// Extensions always served, even when the filter hides them.
    pub assets: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            bind: "0.0.0.0".to_string(),
            port: DEFAULT_PORT,
            title: "files".to_string(),
            token: None,
            live_reload: true,
            extensions: Extensions::parse("md"),
            presets: DEFAULT_PRESETS
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
            include: Vec::new(),
            assets: DEFAULT_ASSETS
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        }
    }
}

/// The shape of a TOML configuration file. Every key is optional, so a file
/// can set one value and leave the rest to the defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    root: Option<PathBuf>,
    bind: Option<String>,
    port: Option<u16>,
    title: Option<String>,
    token: Option<String>,
    live_reload: Option<bool>,
    extensions: Option<Vec<String>>,
    presets: Option<Vec<String>>,
    include: Option<Vec<String>>,
    assets: Option<Vec<String>>,
}

/// Values from the command line. Each one wins over the file and the default.
#[derive(Debug, Default)]
pub struct Overrides {
    pub root: Option<PathBuf>,
    pub bind: Option<String>,
    pub port: Option<u16>,
    pub title: Option<String>,
    pub token: Option<String>,
    pub live_reload: Option<bool>,
    pub extensions: Option<String>,
    pub include: Option<String>,
}

impl Config {
    /// Reads a configuration file.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read or is not valid TOML for this schema.
    pub fn read_file(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let file: FileConfig = toml::from_str(&text).map_err(|source| Error::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
        Ok(Self::default().with_file(file))
    }

    /// Applies the keys a file sets, leaving the rest alone.
    #[must_use]
    pub fn with_file(mut self, file: FileConfig) -> Self {
        if let Some(root) = file.root {
            self.root = root;
        }
        if let Some(bind) = file.bind {
            self.bind = bind;
        }
        if let Some(port) = file.port {
            self.port = port;
        }
        if let Some(title) = file.title {
            self.title = title;
        }
        if let Some(token) = file.token {
            self.token = Some(token);
        }
        if let Some(live_reload) = file.live_reload {
            self.live_reload = live_reload;
        }
        if let Some(extensions) = file.extensions {
            self.extensions = Extensions::parse(&extensions.join(","));
        }
        if let Some(presets) = file.presets {
            self.presets = presets;
        }
        if let Some(include) = file.include {
            self.include = include;
        }
        if let Some(assets) = file.assets {
            self.assets = assets;
        }
        self
    }

    /// Applies the values given on the command line, which win over everything.
    #[must_use]
    pub fn with_overrides(mut self, overrides: Overrides) -> Self {
        if let Some(root) = overrides.root {
            self.root = root;
        }
        if let Some(bind) = overrides.bind {
            self.bind = bind;
        }
        if let Some(port) = overrides.port {
            self.port = port;
        }
        if let Some(title) = overrides.title {
            self.title = title;
        }
        if let Some(token) = overrides.token {
            self.token = Some(token);
        }
        if let Some(live_reload) = overrides.live_reload {
            self.live_reload = live_reload;
        }
        if let Some(extensions) = overrides.extensions {
            self.extensions = Extensions::parse(&extensions);
        }
        if let Some(include) = overrides.include {
            self.include = split_list(&include);
        }
        self
    }

    /// Checks the values a file or a flag can get wrong.
    ///
    /// # Errors
    ///
    /// Fails when the root is not a directory, when the title is empty, or when
    /// an include entry is not a relative path inside the root.
    pub fn validate(&self) -> Result<(), Error> {
        if !self.root.is_dir() {
            return Err(Error::RootUnavailable {
                path: self.root.clone(),
            });
        }
        if self.title.trim().is_empty() {
            return Err(Error::InvalidConfig {
                reason: "the title is empty".to_string(),
            });
        }
        for entry in &self.include {
            if entry.starts_with('/') || entry.split('/').any(|part| part == "..") {
                return Err(Error::InvalidConfig {
                    reason: format!(
                        "`{entry}` in `include` is not a relative path inside the root"
                    ),
                });
            }
        }
        if self.assets.is_empty() {
            return Err(Error::InvalidConfig {
                reason: "`assets` is empty, so no image or font would ever load".to_string(),
            });
        }
        Ok(())
    }
}

/// Splits a comma or space separated list, dropping empty entries.
#[must_use]
pub fn split_list(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_open_as_a_markdown_reader() {
        let config = Config::default();
        assert_eq!(config.extensions.value(), "md");
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(config.live_reload);
        assert!(config.presets.contains(&"ipynb".to_string()));
    }

    #[test]
    fn a_file_sets_only_the_keys_it_names() {
        let file: FileConfig = toml::from_str("title = \"notes\"\nport = 9000\n").unwrap();
        let config = Config::default().with_file(file);
        assert_eq!(config.title, "notes");
        assert_eq!(config.port, 9000);
        assert_eq!(config.bind, "0.0.0.0", "untouched keys keep the default");
    }

    #[test]
    fn the_command_line_wins_over_the_file() {
        let file: FileConfig = toml::from_str("port = 9000\ntitle = \"notes\"\n").unwrap();
        let overrides = Overrides {
            port: Some(9100),
            ..Overrides::default()
        };
        let config = Config::default().with_file(file).with_overrides(overrides);
        assert_eq!(config.port, 9100);
        assert_eq!(config.title, "notes", "the file still supplies the rest");
    }

    #[test]
    fn a_list_flag_accepts_commas_and_spaces() {
        let overrides = Overrides {
            extensions: Some("md, rs  ipynb".to_string()),
            include: Some("docs, work/notes".to_string()),
            ..Overrides::default()
        };
        let config = Config::default().with_overrides(overrides);
        assert_eq!(config.extensions.value(), "ipynb,md,rs");
        assert_eq!(config.include, vec!["docs", "work/notes"]);
    }

    #[test]
    fn an_include_entry_may_not_leave_the_root() {
        let outside = Config {
            include: vec!["../elsewhere".to_string()],
            ..Config::default()
        };
        assert!(outside.validate().is_err());

        let absolute = Config {
            include: vec!["/etc".to_string()],
            ..Config::default()
        };
        assert!(absolute.validate().is_err());

        let inside = Config {
            include: vec!["docs".to_string()],
            ..Config::default()
        };
        assert!(inside.validate().is_ok());
    }

    #[test]
    fn a_config_file_round_trips_through_toml() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("reposerve.toml");
        std::fs::write(
            &path,
            "root = \".\"\ntitle = \"handbook\"\nlive_reload = false\nextensions = [\"md\", \"txt\"]\n",
        )
        .expect("the file is written");

        let config = Config::read_file(&path).expect("the file parses");
        assert_eq!(config.title, "handbook");
        assert!(!config.live_reload);
        assert_eq!(config.extensions.value(), "md,txt");
    }

    #[test]
    fn an_unknown_key_is_reported_rather_than_ignored() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("reposerve.toml");
        std::fs::write(&path, "prot = 8095\n").expect("the file is written");
        let error = Config::read_file(&path).expect_err("`prot` is a typo for `port`");
        assert!(matches!(error, Error::ConfigParse { .. }));
    }
}
