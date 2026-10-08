//! Tablo: a server-rendered admin toolkit on [Topcoat](topcoat) and [Toasty](toasty).
//!
//! A [`Panel`] serves one [`Resource`] per Toasty model. A resource names the model and the
//! [`RecordForm`](derive@RecordForm) struct its create and edit submissions parse into, and
//! declares the rest as one [`ResourceDef`] value: its [`Policy`], which denies by default, and
//! any [`Table`], [`Schema`] or [`Detail`] that arranges or extends what the record form derives.
//! The app mounts the panel on its own Topcoat router with [`RouterBuilderPanelExt::panel`], which
//! builds each def once, binds it to the database schema and checks it first; one router mounts any
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
    pub use topcoat::context::Cx;

    pub use crate::{
        Lens,
        detail::Detail,
        form::{
            FieldError, FormField, FormScalar, NullableScalar, RecordForm, assert_form_scalar,
            parse_scalar,
        },
        resource::{ActionInput, required_input},
        schema::{
            ChoiceField, CustomField, EmbeddedForm, Field, FieldResolver, FileField, IntoSchema,
            Options, Schema, TextField,
            embedded::{
                Embedded, EmbeddedBuilder, embedded_field, embedded_form, take_leaf, take_value,
            },
            form_key,
            tree::Retype,
        },
        table::{BooleanColumn, EmbeddedColumn, FileColumn, Table, TextColumn},
        toasty_compat::VariantId,
    };
}
// The toolkit surface: Panel, Resource, Table, Schema, Notification,
// Tenancy, CSRF, and the `Db` glue.
pub mod auth;
pub mod csrf;
pub mod db;
mod declaration;
pub mod detail;
mod error;
pub mod extend;
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

pub use auth::{Auth, Authenticator, LoginThrottle, PanelUser, PasswordAuth, membership};
pub use declaration::{
    ActionInputFault, DeclarationError, DeclarationErrorKind, MountError, SegmentFault, Site,
};
pub use detail::{Detail, IntoDetail};
pub use form::{
    FieldError, FieldErrorKind, FieldErrors, FormField, FormScalar, NoForm, Posted, RecordForm,
};
pub use lens::Lens;
pub use navigation::{NavTarget, NavigationItem};
pub use notification::{Notification, NotificationStatus};
pub use page::Page;
pub use panel::{Brand, Panel, PanelHandle, RouterBuilderPanelExt, can_list, url};
pub use policy::{Ability, Allow, Deny, Policy, ReadOnly, when};
pub use resource::{
    Action, ActionInput, Committed, ForeignKey, Mutation, PublicLink, Relation, Resource,
    ResourceDef, can, scoped_query, write_create, write_update,
};
pub use schema::{
    ChoiceField, CustomField, EmbeddedForm, Field, FieldResolver, FileField, Grid, Group,
    IntoOptions, IntoSchema, Options, Schema, Section, Source, TextField, Toggle,
};
pub use table::{
    BooleanColumn, ColumnWidth, ComputedColumn, CountColumn, Cursor, DateFilter, EmbeddedColumn,
    FileColumn, IntoColumns, IntoFilters, QueryFilter, RelationColumn, RelationLens, SelectFilter,
    Sort, Table, TablePage, TableState, TernaryFilter, TextColumn, ToOneRelation, WiredTable,
    contains_expr,
};
/// Derives [`ActionInput`](trait@ActionInput) for the typed value an [`Action`] asks for
/// before it runs.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, status: String }
/// # use tablo_core::{Action, ActionInput, NoForm, Resource};
/// # use topcoat::{Result, context::Cx};
/// # struct PostResource;
/// # impl Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = NoForm<Post>;
/// # }
/// #[derive(tablo_core::ActionInput)]
/// pub struct Rejection {
///     #[form(multiline = 4)]
///     pub reason: String,
///     pub notify_author: bool,
/// }
///
/// struct Reject;
///
/// impl Action<PostResource> for Reject {
///     type Input = Rejection;
///     const NAME: &'static str = "reject";
///
///     async fn run(
///         _cx: &Cx,
///         posts: &[Post],
///         rejection: Rejection,
///         ex: &mut dyn toasty::Executor,
///     ) -> Result<()> {
///         // Store `rejection.reason`, notify when `rejection.notify_author` holds…
///         # let _ = (posts, rejection.reason, rejection.notify_author, ex);
///         Ok(())
///     }
/// }
/// ```
///
/// Each field posts its own name, which no column binds, and renders the control its type
/// picks: a `bool` a checkbox, `#[form(options)]` a choice over the field type's
/// [`Options`](derive@Options), `#[form(options = T)]` one over `T`'s, and any other
/// [`FormScalar`] a text input of the type's input type.
///
/// A field's **blank answer** is its `#[form(blank = ..)]`, `""` for an `#[form(optional)]`
/// `String`, `None` for an `Option`, or `false` for a `bool`. A field with none is required:
/// its control renders required and the parse refuses an empty submission.
///
/// # Per-field attributes
///
/// - `#[form(label = "Reason")]` — the control's label (default: the field name, humanized).
/// - `#[form(multiline = 4)]` — a `<textarea>` of 4 rows.
/// - `#[form(placeholder = "rust, async")]` — a text input's placeholder.
/// - `#[form(blank = ..)]`, `#[form(optional)]` — the blank answer.
/// - `#[form(options)]`, `#[form(options = T)]` — a choice. An `Option<T>` field names its
///   options type: `#[form(options = T)]`.
///
/// A generic struct, a tuple struct, an empty struct (name `()` instead), an unknown key,
/// `multiline` or `placeholder` with `options` or on a `bool`, and a field that is not a
/// `FormScalar` are compile errors.
/// [`RouterBuilderPanelExt::panel`] refuses a field named `csrf_token`, `confirm` or `ids`,
/// the keys an action's POST carries besides its input.
///
/// A hand-written impl builds its fields with [`Field::text_input`], [`Field::choice_input`]
/// and [`Field::toggle_input`]; its controls render without the `required` mark, though its
/// `parse` still refuses what it requires.
pub use tablo_macros::ActionInput;
/// Derives `EmbeddedForm` for an embedded struct or enum.
///
/// Builds the schema node and converts the value through that node's keys.
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     publication: Publication,
/// # }
/// # use tablo_core::Section;
/// #[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
/// pub enum Publication {
///     #[column(variant = 1)]
///     Scheduled {
///         #[shared(timestamp)]
///         #[form(label = "Publication timestamp")]
///         scheduled_at: String,
///         scheduled_for: String,
///     },
///     #[column(variant = 2)]
///     Published {
///         #[shared(timestamp)]
///         published_at: String,
///         canonical_url: String,
///     },
/// }
///
/// // form declaration — no field bindings written by hand
/// Section::new("Publication").schema(Publication::form(Post::fields().publication()));
/// ```
///
/// # How a field is classified
///
/// A field marked `#[form(embed)]` is another **embedded value**, delegated to
/// its own `EmbeddedForm`. Every other field is a **scalar**: one column, read
/// and written through `FormScalar` (`String`, a `TypedValue` type, an
/// `Options` enum, or an `Option` of one). A scalar of another type fails to compile at the
/// field, naming the trait. An empty scalar is its blank answer — its declared
/// `#[form(blank = ..)]`, `None` for an `Option`, `false` for a `bool`, `""`
/// for an `#[form(optional)]` `String` — and a scalar with none is required:
/// its control renders required and the parse refuses its key.
///
/// # Which variant an enum reads
///
/// A named discriminant always wins, and an undeclared one is refused;
/// otherwise the first variant, in declaration order, with a **payload of its
/// own** submitted — a `#[shared(..)]` column belongs to several variants and
/// never selects one; otherwise the first variant.
///
/// # Per-field attributes
///
/// - `#[form(embed)]` — a nested `EmbeddedForm` value.
/// - `#[form(label = "Canonical URL")]` — the control's label (default: the field name,
///   humanized).
/// - `#[form(multiline = 3)]` — a `<textarea>` of 3 rows.
/// - `#[form(blank = ..)]` — what an empty submission reads as.
/// - `#[form(optional)]` on a `String` — an empty submission reads as `""`.
///
/// Anything else in `#[form(..)]` is a compile error, as are `label`, `multiline`, `blank` and
/// `optional` on an embedded value, `blank` or `optional` on an `Option`, and `optional` on a
/// type other than `String`.
pub use tablo_macros::EmbeddedForm;
/// Derive `Options` for a unit-variant enum: the `(value, label)` list a
/// choice field, a select filter and a column share, and the `FormScalar`
/// that posts a variant as its value and reads it as its label.
///
/// ```rust
/// # use tablo_core::Options;
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// pub enum Status {
///     Draft,
///     #[option(label = "Live")]
///     Published,
/// }
///
/// assert_eq!(Status::Published.value(), "published");
/// assert_eq!(Status::Published.label(), "Live");
/// assert_eq!(Status::from_value("draft"), Some(Status::Draft));
/// ```
///
/// Each variant's value is its `snake_case` name and its label that name in
/// sentence case. `#[option(value = "..")]` and `#[option(label = "..")]`
/// override either. A generic enum, a variant with fields, two variants
/// sharing one value or one label, and an unknown key are compile errors.
///
/// The enum, and an `Option` of it, is a form scalar through this derive, so
/// it cannot also implement `TypedValue` or `FormScalar` itself.
pub use tablo_macros::Options;
/// Derive `RecordForm` for the typed value a resource's form writes.
///
/// One field per model column the form writes, named and typed like the
/// model's field. A scalar (`String`, a `TypedValue` type, an `Options` enum,
/// or an `Option` of one) binds the key its control posts; a `#[form(embed)]` field binds
/// every key of an `EmbeddedForm` value and is written whole.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # pub struct User {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// #     role: String,
/// #     age: i64,
/// # }
/// #[derive(tablo_core::RecordForm)]
/// #[form(model = User)]
/// pub struct UserForm {
///     pub name: String,
///     #[form(blank = "member")]
///     pub role: String,
///     #[form(blank = 0)]
///     pub age: i64,
/// }
/// ```
///
/// The derive also emits `UserFormField`, one variant per field, which
/// `Posted` and `Resource::validate_record`'s `FieldErrors` key on and
/// `RecordForm::fields` answers with each variant's keys.
/// It emits `UserFormControls`, one control per field chosen from the field —
/// a `bool` is a toggle, `#[form(options = T)]` a choice over `T`'s options,
/// `#[form(options)]` a choice over the field type's options,
/// `#[form(relationship = R)]` a choice over `R`'s records, `#[form(file)]` a file field,
/// `#[form(embed)]` the embedded value's schema, and any other field a text
/// field — with `controls()` handing them over, typed by the form so only a
/// `Schema<UserForm>` places them, and `RecordForm::control` answering one
/// field's. `RecordForm::table` lists a
/// sortable column per text field, searchable over a `String` or
/// `Option<String>`, an options field by its option's label, and a toggle as
/// yes or no. `RecordForm::detail` shows the same columns, plus a relationship's
/// key, a file field as a link and an embedded value leaf by leaf. A resource's `ResourceDef`
/// defaults its form, table and detail page to them; `ResourceDef::form`,
/// `ResourceDef::table` and `ResourceDef::view` arrange or extend them instead,
/// and a form renders the controls it does not place after the ones it does.
///
/// # Attributes
///
/// - `#[form(model = User)]` on the struct: the model the form writes.
/// - `#[form(blank = <expr>)]` on a scalar: the value an empty submission reads as.
/// - `#[form(optional)]` on a `String`: an empty submission reads as `""`.
/// - `#[form(options)]` on an `Options` enum or an `Option` of one: a choice over its options.
///   Without it, the field is a text field.
/// - `#[form(options = Status)]`: a choice over `Status::options()`.
/// - `#[form(relationship = AuthorResource)]` on a foreign key: a choice over the source's
///   records, each labelled by its `record_title`. The source is any `OptionSource`, which
///   every `Resource` is.
/// - `#[form(file)]` on a `String`: a file field.
/// - `#[form(embed)]` on an `EmbeddedForm` value.
///
/// A scalar's **blank answer** is its `blank`, `""` for an `optional` `String`, `None` for an
/// `Option`, or `false` for a `bool`. A field with none is required: the panel renders its
/// control required and the parse refuses an empty submission. No control declares presence.
///
/// A generic struct, a tuple struct, an empty struct, a `Deferred<_>` field,
/// `blank` or `optional` on an `Option` or an embedded value, `optional` on a
/// type other than `String`, and an unknown key are compile errors. So are a field the model
/// lacks, a type the model's field does not have, and a scalar that is not a `FormScalar`.
pub use tablo_macros::RecordForm;
pub use tenancy::{Membership, Tenancy, Tenant, TenantColumn, TenantId, require_tenant, tenant_id};
pub use upload::Uploader;
