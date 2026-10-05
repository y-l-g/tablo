//! Tablo — an admin toolkit for Rust, server-rendered on Topcoat and persisted
//! with Toasty.
//!
//! This crate is the one Tablo dependency an app names, beside `topcoat` and
//! `toasty` at the revisions Tablo pins:
//!
//! ```toml
//! [dependencies]
//! tablo = { git = "https://github.com/y-l-g/tablo", features = ["sqlite"] }
//!
//! [build-dependencies]
//! tablo-build = { git = "https://github.com/y-l-g/tablo" }
//! ```
//!
//! It re-exports [`tablo_core`] at its root, the UI components as [`ui`], and
//! the in-memory HTTP client as `testing` with the `testing` feature; the driver
//! features (`sqlite`, `postgresql`, `mysql`) turn on Toasty's driver of the same
//! name, and the toolkit itself enables none.
//!
//! A minimal app is one model, one [`Resource`], and the [`Panel`] that serves
//! it — the full version lives in `examples/quickstart`:
//!
//! ```rust
//! # use tablo::prelude::*;
//! #[derive(Debug, Clone, toasty::Model)]
//! pub struct Book {
//!     #[key]
//!     #[auto]
//!     pub id: uuid::Uuid,
//!     pub title: String,
//! }
//!
//! #[derive(tablo::RecordForm)]
//! #[form(model = Book)]
//! pub struct BookForm {
//!     pub title: String,
//! }
//!
//! pub struct BookResource;
//!
//! impl Resource for BookResource {
//!     type Model = Book;
//!     type Form = BookForm;
//!
//!     fn policy() -> impl Policy<Book> {
//!         Allow
//!     }
//! }
//! ```
//!
//! Mounting serves it on the app's router:
//!
//! ```text
//! let router = Router::builder()
//!     .discover()
//!     .app_context(db)
//!     .panel(Panel::new("admin").resource::<BookResource>())?
//!     .build();
//! ```
//!
//! The user guide starts at "Your first panel".

pub use tablo_core::*;
#[cfg(feature = "testing")]
pub use tablo_test as testing;
pub use tablo_ui as ui;

/// The items a resource module names most: `use tablo::prelude::*;`.
pub mod prelude {
    pub use tablo_core::{
        Ability, Action, Actions, Allow, Auth, BooleanColumn, Brand, ChoiceField, Column,
        ColumnWidth, Committed, ComputedColumn, Control, ControlInput, CustomField, DateFilter,
        Deny, EmbeddedForm, Field, FieldErrors, FileField, Filter, FilterInput, Grid, Group,
        Includes, IntoOptions, Lens, NavigationItem, NoForm, Options, Page, Panel, Policy, Posted,
        QueryFilter, ReadOnly, RecordForm, Relation, Repeater, Resource, RouterBuilderPanelExt,
        Schema, Section, SelectFilter, Table, Tenancy, TernaryFilter, TextColumn, TextField,
        Toggle, can, can_list, declare, lens, scoped_query, tenant_id, when,
    };
}
