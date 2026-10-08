//! The user guide's compiled companion.
//!
//! Each item here is anchored into `docs/guide/`: the guide shows these lines,
//! and the compiler checks them. Wrappers (signatures, returns) stay outside
//! the anchors; the anchored lines match the guide verbatim.

pub mod actions;
pub mod build_script;
pub mod data_access;
pub mod detail_pages;
pub mod first_panel;
pub mod forms;
pub mod models;
pub mod panel_routing;
pub mod policy_tenancy;
pub mod resources;
pub mod tables;

pub use first_panel::Book;
