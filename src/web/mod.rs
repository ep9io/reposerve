//! How a page is served: the socket, the routes, the assets and the caches.
//!
//! [`server`] binds the socket and watches the root. [`routes`] answers one
//! request. [`assets`] holds the files that ship inside the binary. [`cache`]
//! keeps a rendered page until its file changes. [`theme`] reads the reader's
//! colour choice from a cookie.

pub mod assets;
pub mod cache;
pub mod routes;
pub mod server;
pub mod theme;

pub use server::Server;
