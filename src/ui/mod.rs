//! The user interface: the shell and the views.
//!
//! [`shell`] assembles one page. [`views`] builds the body of each kind of page.
//! [`sidebar`] and [`toc`] hold the two navigation blocks.

pub mod shell;
pub mod sidebar;
pub mod toc;
pub mod views;

pub use shell::View;
