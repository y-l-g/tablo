//! Tablo — an admin toolkit for Rust, server-rendered on Topcoat and persisted
//! with Toasty.
//!
//! This crate is the one Tablo dependency an app names, beside `topcoat` and
//! `toasty` at the versions Tablo uses:
#![doc = concat!(
    "```toml\n",
    "[dependencies]\n",
    "tablo = { version = \"", env!("CARGO_PKG_VERSION"), "\", features = [\"sqlite\"] }\n",
    "\n",
    "[build-dependencies]\n",
    "tablo-build = \"", env!("CARGO_PKG_VERSION"), "\"\n",
    "```\n",
)]
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
//!     fn declare() -> ResourceDef<Self> {
//!         ResourceDef::new().policy(Allow)
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
        Ability, Action, ActionInput, Allow, Auth, BooleanColumn, Brand, ChoiceField, Column,
        ColumnWidth, Committed, ComputedColumn, Control, ControlInput, CountColumn, CustomField,
        DateFilter, Deny, Detail, EmbeddedColumn, EmbeddedForm, Field, FieldErrors, FileColumn,
        FileField, Filter, FilterInput, Grid, Group, Includes, IntoOptions, Lens, NavigationItem,
        NoForm, Options, Page, Panel, Policy, Posted, PublicLink, QueryFilter, ReadOnly,
        RecordForm, Relation, RelationColumn, Resource, ResourceDef, RouterBuilderPanelExt, Schema,
        Section, SelectFilter, Table, Tenancy, TenantId, TernaryFilter, TextColumn, TextField,
        Toggle, can, can_list, lens, relation, scoped_query, tenant_id, when,
    };
}
