//! Tablo: a server-rendered admin toolkit on [Topcoat](topcoat) and [Toasty](toasty).
//!
//! A [`Panel`] serves one [`Resource`] per Toasty model. A resource names the model and the
//! [`RecordForm`](derive@RecordForm) struct its create and edit submissions parse into, and
//! declares the rest as one [`ResourceDef`] value: its [`Policy`], which denies by default, and
//! any [`Table`] or [`Schema`] that arranges or extends what the record form derives. The app
//! mounts the panel on its own Topcoat router with [`RouterBuilderPanelExt::panel`], which builds
//! each def once, binds it to the database schema and checks it first; one router mounts any
//! number of panels at distinct prefixes.
//!
//! ```rust,no_run
//! # use tablo_core::{Allow, Panel, Resource, ResourceDef, RouterBuilderPanelExt};
//! # use toasty::Db;
//! # use topcoat::router::{Router, RouterBuilderDiscoverExt};
//! # fn main() -> topcoat::Result<()> {
//! # #[derive(Debug, Clone, toasty::Model)]
//! # pub struct Book {
//! #     #[key]
//! #     #[auto]
//! #     pub id: uuid::Uuid,
//! #     pub title: String,
//! # }
//! #[derive(tablo_core::RecordForm)]
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
//!
//! # let db: Db = todo!();
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
//! The [user guide](https://y-l.fr/tablo/nightly/guide/) walks through a complete panel and each
//! part of it. [`ResourceDef`] lists everything a resource declares, [`Resource`] the hooks it
//! overrides, and [`Panel`] every builder call.

// The derives emit `tablo_core::` paths; this lets them expand inside this
// crate's own tests too.
extern crate self as tablo_core;

#[doc(hidden)]
pub mod __macro {
    pub use toasty::{
        Executor, Result as DbResult,
        schema::{Embed, Model},
        stmt,
        stmt::Path,
    };
    pub use toasty_core::schema::app::VariantId;
    pub use topcoat::context::Cx;

    pub use crate::{
        Lens,
        form::{FieldError, FormField, FormScalar, RecordForm, assert_form_scalar, parse_scalar},
        schema::{
            ChoiceField, CustomField, EmbeddedForm, Field, FileField, IntoSchema, Options, Schema,
            TextField,
            embedded::{
                Embedded, EmbeddedBuilder, embedded_keys, parse_leaf, take_leaf, take_value,
            },
            form_key,
        },
        table::{BooleanColumn, Table, TextColumn},
    };
}
// The toolkit surface: Panel, Resource, Table, Schema, Notification,
// Tenancy, CSRF, and the `Db` glue.
pub mod auth;
pub mod csrf;
pub mod db;
mod declaration;
mod error;
pub mod form;
mod lens;
mod naming;
pub mod navigation;
pub mod notification;
mod page;
pub mod panel;
pub mod policy;
mod query_term;
pub mod resource;
pub mod schema;
pub mod table;
pub mod tenancy;
#[cfg(test)]
mod test_support;
mod toasty_compat;
mod topcoat_compat;
pub mod upload;

pub use auth::{Auth, Authenticator, PanelUser, PasswordAuth, membership};
pub use declaration::{DeclarationError, DeclarationErrorKind, MountError, SegmentFault, Site};
pub use form::{
    FieldError, FieldErrorKind, FieldErrors, FormField, FormScalar, NoForm, Posted, RecordForm,
};
pub use lens::Lens;
pub use navigation::{NavTarget, NavigationItem};
pub use notification::{Notification, NotificationStatus};
pub use page::Page;
pub use panel::{Brand, Panel, RouterBuilderPanelExt, can_list, url};
pub use policy::{Ability, Allow, Deny, Policy, ReadOnly, when};
pub use resource::{
    Action, Committed, ForeignKey, Mutation, Relation, Resource, ResourceDef, can, scoped_query,
    scoped_view_query, write_create, write_update,
};
pub use schema::{
    ChoiceField, Control, ControlInput, CustomField, EmbeddedForm, Field, FileField, Grid, Group,
    IntoOptions, IntoSchema, Options, Repeater, Schema, Section, Source, TextField, Toggle,
    declare,
};
pub use table::{
    BooleanColumn, Column, ColumnWidth, ComputedColumn, Cursor, DateFilter, Filter, FilterInput,
    Includes, IntoColumns, IntoFilters, QueryFilter, SelectFilter, Sort, Table, TablePage,
    TableState, TernaryFilter, TextColumn,
};
pub use tablo_macros::{EmbeddedForm, Options, RecordForm};
pub use tenancy::{Membership, Tenancy, Tenant, require_tenant, tenant_id};
pub use upload::Uploader;
