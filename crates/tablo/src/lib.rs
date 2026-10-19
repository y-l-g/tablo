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
//! features (`sqlite`, `postgresql`, `mysql`, `turso`) turn on Toasty's driver of the same
//! name, and the toolkit itself enables none.
//!
//! A minimal app is one model, the record form [`ModelForm`](derive@ModelForm)
//! derives from it, one [`Resource`], and the [`Panel`] that serves it — the
//! full version lives in `examples/quickstart`:
//!
//! ```rust
//! # use tablo::prelude::*;
//! #[derive(Debug, Clone, toasty::Model, tablo::ModelColumns, tablo::ModelForm)]
//! pub struct Book {
//!     #[key]
//!     #[auto]
//!     pub id: uuid::Uuid,
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
//! ```rust,no_run
//! # use tablo::prelude::*;
//! # use topcoat::router::{Router, RouterBuilderDiscoverExt};
//! # #[derive(Debug, Clone, toasty::Model, tablo::ModelColumns, tablo::ModelForm)]
//! # pub struct Book {
//! #     #[key]
//! #     #[auto]
//! #     pub id: uuid::Uuid,
//! #     pub title: String,
//! # }
//! # pub struct BookResource;
//! # impl Resource for BookResource {
//! #     type Model = Book;
//! #     type Form = BookForm;
//! # }
//! # fn main() -> topcoat::Result<()> {
//! # let db: toasty::Db = todo!();
//! let router = Router::builder()
//!     .discover()
//!     .app_context(db)
//!     .panel(Panel::new("admin").resource::<BookResource>())?
//!     .build();
//! # let _ = router;
//! # Ok(())
//! # }
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
        Ability, Action, ActionLayout, Alignment, Allow, Auth, BooleanColumn, Brand, ChoiceField,
        Color, ColorColumn, ColumnWidth, Committed, ComputedColumn, CountColumn, Currency,
        CustomField, DateColumn, DateFilter, DateValue, Deny, Detail, EmbeddedColumn, EmbeddedForm,
        Field, FieldErrors, FileColumn, FileField, FormInput, FormValues, Grid, Group,
        HeaderAction, HeaderActions, ImageColumn, IntoOptions, Lens, ModelColumns, ModelForm,
        NavigationItem, NoForm, Notification, Options, Outcome, Page, PageDef, Pagination, Panel,
        Places, Policy, Posted, PublicLink, QueryFilter, ReadOnly, RecordForm, Relation,
        RelationColumn, RepeaterColumn, Resource, ResourceDef, RouterBuilderPanelExt, Schema,
        Section, SelectFilter, Sort, Table, Tenancy, TenantId, TernaryFilter, TextColumn,
        TextField, Toggle, can, can_list, can_open, lens, relation, scoped_query, tenant_id, when,
    };
}
