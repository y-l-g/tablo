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
//! name, and the toolkit itself enables none. The `rust_decimal` and `bigdecimal` features make
//! those crates' decimal types typed values and numeric amounts, and turn on Toasty's feature of
//! the same name.
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
//!     .with(db)
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
        Abilities, Ability, Action, ActionLayout, Alignment, Allow, Auth, BooleanColumn, Brand,
        Change, Chart, ChoiceField, Color, ColorColumn, ColumnWidth, ComputedColumn, CountColumn,
        Currency, CustomField, DateColumn, DateFilter, DateValue, Deny, Detail, EmbeddedColumn,
        EmbeddedForm, Field, FieldAccess, FieldErrors, FileColumn, FileField, FormInput,
        FormValues, GlobalSearch, Grid, Group, HeaderAction, HeaderActions, IconColumn,
        ImageColumn, IntoOptions, Lens, ListTab, ModelColumns, ModelForm, NavigationItem, NoForm,
        Options, Outcome, Page, PageDef, Pagination, Panel, Places, Policy, Posted, PublicLink,
        QueryFilter, ReadOnly, Reads, RecordForm, RecordTable, Relation, RelationColumn,
        RepeaterColumn, Resource, ResourceDef, RouterBuilderPanelExt, Schema, Section,
        SelectColumn, SelectFilter, Series, Sort, Stat, Tab, Table, Tabs, Tenancy, TenantId,
        TernaryFilter, Text, TextColumn, TextField, TextInputColumn, Toggle, ToggleColumn, Trend,
        Widget, WidgetDef, WriteError, can, can_list, can_open, filter_keys, lens, list_query,
        relation, scoped_query, t, tenant_id, text, when,
    };
    #[cfg(feature = "rich-text")]
    pub use tablo_core::{RichTextColumn, RichTextField, Tool};
}
