//! Tablo — a server-rendered admin toolkit on Topcoat and Toasty.
//!
//! This crate holds the toolkit's foundation: the [`Resource`] trait and the
//! types that compose an admin UI (Panel, Table, Schema, Action). See the
//! workspace README and `CONTEXT.md` for the vocabulary.
#![doc = include_str!("../../../CONTEXT.md")]

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
            EmbeddedForm, Field, ResolvedLens, Schema,
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
pub mod cursor;
pub mod db;
mod error;
pub mod form;
pub mod notification;
mod page;
pub mod panel;
mod query_term;
pub mod resource;
pub mod schema;
pub mod tenancy;
#[cfg(test)]
mod test_support;
pub mod upload;

pub use auth::{Auth, Authenticator, CurrentUser, PasswordAuth};
pub use form::{
    FieldError, FieldErrorKind, FieldErrors, FormField, FormScalar, NoForm, Posted, RecordForm,
    write_create, write_update,
};
pub use notification::{Notification, NotificationStatus};
pub use page::Page;
pub use panel::{Brand, DarkMode, Panel};
pub use resource::{
    ColumnWidth, Committed, Cursor, DateFilter, Filter, IntoFilters, Mutation, NavTarget,
    NavigationItem, Relation, Resource, RowKey, SelectFilter, Sort, Table, TablePage, TableState,
    TernaryFilter, TextColumn, VariantFilter, scoped_query, scoped_view_query,
};
pub use schema::{
    EmbeddedForm, Field, FieldLens, Grid, Group, IntoSchema, Repeater, ResolvedLens, Schema,
    Section, Source,
};
pub use tablo_macros::{EmbeddedForm, RecordForm};
pub use tenancy::{Tenant, require_tenant, tenant_id};
pub use upload::Uploader;
