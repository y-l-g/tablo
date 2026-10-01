//! Tablo — an admin toolkit for Rust, server-rendered on Topcoat and persisted
//! with Toasty.
//!
//! This crate is the one Tablo dependency an app names, beside `topcoat` and
//! `toasty` at the revisions Tablo pins. It re-exports
//! [`tablo_core`] at its root, the UI components as [`ui`], and, with the
//! `testing` feature, the in-memory HTTP client as `testing`. The derives
//! (`RecordForm`, `EmbeddedForm`) work through this crate alone.
//!
//! ```toml
//! [dependencies]
//! tablo = { git = "https://github.com/y-l-g/tablo", features = ["sqlite"] }
//!
//! [build-dependencies]
//! tablo-build = { git = "https://github.com/y-l-g/tablo" }
//! ```
//!
//! The driver features (`sqlite`, `postgresql`, `mysql`) turn on Toasty's
//! driver of the same name; the toolkit itself enables none. The app's
//! `build.rs` calls `tablo_build::tailwind()`, which finds Tablo's sources
//! through this crate. The user guide starts at "Your first panel".

pub use tablo_core::*;
#[cfg(feature = "testing")]
pub use tablo_test as testing;
pub use tablo_ui as ui;

/// The items a resource module names most: `use tablo::prelude::*;`.
pub mod prelude {
    pub use tablo_core::{
        Action, Actions, Auth, BooleanColumn, Brand, Column, ColumnWidth, Committed, Control,
        ControlInput, DateFilter, EmbeddedForm, Field, FieldErrors, Filter, FilterInput, Grid,
        Group, NavigationItem, NoForm, Page, Panel, Posted, RecordForm, Relation, Repeater,
        Resource, Schema, Section, SelectFilter, Table, TernaryFilter, TextColumn, VariantFilter,
        scoped_query, tenant_id,
    };
}
