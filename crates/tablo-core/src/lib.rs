//! Tablo: a server-rendered admin toolkit on [Topcoat](topcoat) and [Toasty](toasty).
//!
//! A [`Panel`] serves one [`Resource`] per Toasty model. A resource declares its list page as a
//! [`Table`], its create and edit forms as a [`Schema`] plus a
//! [`RecordForm`](derive@RecordForm) struct the submission parses into, and its [`Policy`], which
//! denies by default. The app mounts the panel on its own Topcoat router with
//! [`RouterBuilderPanelExt::panel`], which checks every declaration first; one router mounts any
//! number of panels at distinct prefixes.
//!
//! ```ignore
//! pub struct BookResource;
//!
//! impl Resource for BookResource {
//!     type Model = Book;
//!     type Form = BookForm;
//!
//!     fn policy() -> impl Policy<Book> {
//!         Allow
//!     }
//!
//!     fn table() -> Table<Book> {
//!         Table::new(TextColumn::new(lens!(Book.title)).searchable())
//!     }
//!
//!     fn form(_dx: &crate::schema::DeclCx) -> Schema {
//!         Schema::new(Field::text(Book::fields().title()))
//!     }
//! }
//!
//! let router = Router::builder()
//!     .discover()
//!     .app_context(db)
//!     .panel(Panel::new("admin").resource::<BookResource>())?
//!     .build();
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
        form::{FieldError, FormField, FormScalar, RecordForm, assert_form_scalar, parse_scalar},
        schema::{
            ChoiceField, CustomField, DeclCx, EmbeddedForm, Field, FileField, IntoSchema, Options,
            ResolvedLens, Schema, TextField,
            embedded::{
                Embedded, EmbeddedBuilder, embedded_keys, parse_leaf, take_leaf, take_value,
            },
        },
    };
}
// The toolkit surface: Panel, Resource, Table, Schema, Notification,
// Tenancy, CSRF, and the `Db` glue.
pub mod auth;
pub mod csrf;
pub mod db;
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
pub mod upload;

pub use auth::{Auth, Authenticator, PanelUser, PasswordAuth};
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
    ChoiceField, Control, ControlInput, CustomField, DeclCx, EmbeddedForm, Field, FileField, Grid,
    Group, IntoOptions, IntoSchema, Options, Repeater, ResolvedLens, Schema, Section, Source,
    TextField, Toggle,
};
pub use tablo_macros::{EmbeddedForm, Options, RecordForm};
pub use tenancy::{Membership, Tenancy, Tenant, membership, require_tenant, tenant_id};
pub use upload::Uploader;
