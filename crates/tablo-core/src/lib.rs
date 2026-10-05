//! Tablo: a server-rendered admin toolkit on [Topcoat](topcoat) and [Toasty](toasty).
//!
//! A [`Panel`] serves one [`Resource`] per Toasty model. A resource names the model, the
//! [`RecordForm`](derive@RecordForm) struct its create and edit submissions parse into, and its
//! [`Policy`], which denies by default. The record form derives the rest: the list page's
//! [`Table`], the form's [`Schema`] and the detail page's, each of which the resource overrides
//! to arrange or extend. The app mounts the panel on its own Topcoat router with
//! [`RouterBuilderPanelExt::panel`], which binds every declaration to the database schema and
//! checks it first; one router mounts any number of panels at distinct prefixes.
//!
//! ```rust,no_run
//! # use tablo_core::{Allow, Panel, Policy, Resource, RouterBuilderPanelExt};
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
//!     fn policy() -> impl Policy<Book> {
//!         Allow
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
//! part of it. [`Resource`] lists every item a resource can declare, and [`Panel`] every builder
//! call.

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
        resource::{BooleanColumn, Table, TextColumn},
        schema::{
            ChoiceField, CustomField, EmbeddedForm, Field, FileField, IntoSchema, Options, Schema,
            TextField,
            embedded::{
                Embedded, EmbeddedBuilder, embedded_keys, parse_leaf, take_leaf, take_value,
            },
            form_key,
        },
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
pub mod notification;
mod page;
pub mod panel;
pub mod policy;
mod query_term;
pub mod resource;
pub mod schema;
pub mod tenancy;
#[cfg(test)]
mod test_support;
mod toasty_compat;
mod topcoat_compat;
pub mod upload;

pub use auth::{Auth, Authenticator, PanelUser, PasswordAuth};
pub use declaration::{DeclarationError, DeclarationErrorKind, MountError, SegmentFault, Site};
pub use form::{
    FieldError, FieldErrorKind, FieldErrors, FormField, FormScalar, NoForm, Posted, RecordForm,
    write_create, write_update,
};
pub use lens::Lens;
pub use notification::{Notification, NotificationStatus};
pub use page::Page;
pub use panel::{Brand, Panel, RouterBuilderPanelExt, can_list, url};
pub use policy::{Ability, Allow, Deny, Policy, ReadOnly, can, when};
pub use resource::{
    Action, Actions, BooleanColumn, Column, ColumnWidth, Committed, ComputedColumn, Cursor,
    DateFilter, Filter, FilterInput, ForeignKey, Includes, IntoColumns, IntoFilters, Mutation,
    NavTarget, NavigationItem, QueryFilter, Relation, Resource, SelectFilter, Sort, Table,
    TablePage, TableState, TernaryFilter, TextColumn, scoped_query, scoped_view_query,
};
pub use schema::{
    ChoiceField, Control, ControlInput, CustomField, EmbeddedForm, Field, FileField, Grid, Group,
    IntoOptions, IntoSchema, Options, Repeater, Schema, Section, Source, TextField, Toggle,
    declare,
};
pub use tablo_macros::{EmbeddedForm, Options, RecordForm};
pub use tenancy::{Membership, Tenancy, Tenant, membership, require_tenant, tenant_id};
pub use upload::Uploader;
