//! Serve a repository's Markdown, source files and notebooks to a browser on
//! the local network.
//!
//! The crate has three layers. [`site`] turns a repository into a list of files
//! and a file into a page. [`web`] binds a socket and answers requests. [`ui`]
//! writes the markup of a page. [`Config`] and [`Server`] are the public front
//! door; the `reposerve` binary is the small command-line front end.
//!
//! ```no_run
//! use reposerve::{Config, Server};
//!
//! # async fn run() -> Result<(), reposerve::Error> {
//! let config = Config::default();
//! let server = Server::bind(config).await?;
//! server.report();
//! server.serve().await
//! # }
//! ```
//!
//! Pages are rendered on the server: Markdown, syntax highlighting, math and
//! notebook cells all arrive as HTML. The browser loads one small script for
//! the controls, and a diagram or math script only when the page needs one.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod site;
pub mod ui;
pub mod web;

pub use config::Config;
pub use error::Error;
pub use site::RepoIndex;
pub use web::Server;
