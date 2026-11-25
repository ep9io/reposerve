//! Serve a repository's Markdown, source files and notebooks to a browser on
//! the local network.
//!
//! The crate has two layers. [`Config`] describes what to serve, and [`Server`]
//! binds a socket and answers requests. A host program can call both; the
//! `reposerve` binary is the small command-line front end.
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
//! Pages are rendered on the server: Markdown, syntax highlighting and notebook
//! cells all arrive as HTML. The browser runs two small scripts at most, one for
//! diagrams and one for live reload.

pub mod config;
pub mod error;
pub mod highlight;
pub mod index;
pub mod notebook;
pub mod pages;
pub mod render;
pub mod routes;
pub mod server;

pub use config::Config;
pub use error::Error;
pub use server::Server;
