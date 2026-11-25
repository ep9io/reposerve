//! Typed failures. The binary turns each one into a single plain message.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Everything the server can fail at before or during a run.
#[derive(Debug, Error)]
pub enum Error {
    /// The directory to serve is missing or is not a directory.
    #[error("the root {path} is not a readable directory")]
    RootUnavailable { path: PathBuf },

    /// The configuration file could not be read.
    #[error("cannot read the configuration at {path}: {source}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The configuration file is not valid TOML for this schema.
    #[error("cannot parse the configuration at {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    /// A setting is present but cannot work.
    #[error("the configuration is not valid: {reason}")]
    InvalidConfig { reason: String },

    /// `git` could not be started.
    #[error("cannot run `git ls-files` in {root}: {source}")]
    GitUnavailable {
        root: PathBuf,
        #[source]
        source: io::Error,
    },

    /// `git` refused the directory, which means it is not a working tree.
    #[error("{root} is not a Git working tree: {message}")]
    NotAWorkTree { root: PathBuf, message: String },

    /// The directory could not be walked, when `git` is not available.
    #[error("cannot list {path}: {source}")]
    Walk {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// A listed file could not be read, or resolved outside the root.
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The requested path is not part of the served content.
    #[error("{path} is not part of the served content")]
    NotFound { path: String },

    /// The listening socket could not be created.
    #[error("cannot listen on {address}: {source}")]
    Bind {
        address: String,
        #[source]
        source: io::Error,
    },

    /// The server stopped because of a transport failure.
    #[error("the server stopped: {source}")]
    Serve {
        #[source]
        source: io::Error,
    },
}
