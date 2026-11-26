//! The user interface: the shell and the views.
//!
//! [`shell`] assembles one page. [`views`] builds the body of each kind of page.
//! [`sidebar`], [`toc`] and [`pager`] hold the navigation blocks.

pub mod pager;
pub mod shell;
pub mod sidebar;
pub mod toc;
pub mod views;

pub use shell::View;
