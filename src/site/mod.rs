//! What a repository looks like: its files, and how a file becomes a page.
//!
//! [`index`] lists the files a reader may see. [`render`] turns Markdown into
//! HTML, [`highlight`] colours source code, and [`notebook`] reads a Jupyter
//! notebook. [`media`] answers the content type of a file that is sent as bytes.
//! None of these modules know about a socket or a request.

pub mod highlight;
pub mod html;
pub mod index;
pub mod media;
pub mod notebook;
pub mod render;

pub use index::RepoIndex;
