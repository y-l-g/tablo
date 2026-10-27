//! Tablo: a server-rendered admin toolkit on [Topcoat](topcoat) and [Toasty](toasty).
//!
//! A [`Panel`] serves one [`Resource`] per Toasty model. A resource names the model, whose
//! [`ModelColumns`](derive@ModelColumns) give its default table and detail page, and the record
//! form its create and edit submissions parse into, which [`ModelForm`](derive@ModelForm) derives
//! from the model or [`RecordForm`](derive@RecordForm) from a struct of its own. It declares the
//! rest as one [`ResourceDef`] value: its [`Policy`], which denies by default, and any [`Table`],
//! [`Schema`] or [`Detail`] that replaces or extends the defaults.
//! The app mounts the panel on its own Topcoat router with [`RouterBuilderPanelExt::panel`], which
//! builds each def once and checks it against the database schema first; one router mounts any
//! number of panels at distinct prefixes.
//!
//! ```rust,no_run
//! # use tablo_core::{Allow, Panel, Resource, ResourceDef, RouterBuilderPanelExt};
//! # use toasty::Db;
//! # use topcoat::router::{Router, RouterBuilderDiscoverExt};
//! # fn main() -> topcoat::Result<()> {
//! #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns, tablo_core::ModelForm)]
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
//!
//! # let db: Db = todo!();
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
        codegen_support::Version,
        schema::{Embed, Model},
        stmt,
        stmt::{List, Path},
    };
    pub use topcoat::context::Cx;

    pub use crate::{
        Color, Lens, Text,
        detail::Detail,
        extend::{Includes, OptionSource},
        form::{
            FieldError, FormField, FormInput, FormScalar, FormValues, NullableScalar, RecordForm,
            assert_form_scalar, links_include, links_key, parse_list, parse_password, parse_scalar,
            required_input, write_links,
        },
        i18n::option_label,
        schema::{
            ChoiceField, CustomField, EmbeddedForm, Field, FileField, HoldsTabs, IntoSchema,
            Options, RepeaterField, Schema, TextField,
            embedded::{Embedded, EmbeddedBuilder, embedded_field, take_leaf, take_value},
            form_key, parse_items,
            tree::Retype,
            write_items,
        },
        table::{ModelColumns, Table, model::pick},
        toasty_compat::VariantId,
    };
}
// Each public item has one path. Types live at the root; a public module is a namespace for
// the functions and constants that read best qualified (`auth::user`, `csrf::verify`).
pub mod auth;
mod bus;
mod chart;
mod column;
mod cookie;
pub mod csrf;
pub mod db;
mod declaration;
mod derive;
mod detail;
mod display;
#[doc(hidden)]
pub mod dom;
mod error;
pub mod extend;
mod flash;
mod form;
mod i18n;
mod lens;
mod models;
mod naming;
mod navigation;
pub mod notification;
mod outcome;
mod page;
mod panel;
mod policy;
mod resource;
#[cfg(feature = "rich-text")]
pub mod rich_text;
mod schema;
mod table;
mod tenancy;
#[cfg(test)]
mod test_support;
mod toasty_compat;
mod topcoat_compat;
mod upload;
mod values;
mod widget;

/// The future a type-erased, dyn-compatible method returns.
pub(crate) type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub use auth::{
    config::Auth,
    mail::AuthMail,
    panel_user::{Authenticator, PanelUser},
    password::{PasswordAuth, PasswordSignUp},
    password_reset::PasswordReset,
    profile::{Profile, ProfileEditor, ProfileUpdate},
    registration::{Registrar, SignUp},
    tenant_registration::TenantRegistration,
    throttle::LoginThrottle,
    two_factor::TwoFactor,
    verification::EmailVerification,
};
pub use chart::{Chart, Series, chart};
#[cfg(feature = "rich-text")]
pub use column::RichTextColumn;
pub use column::{
    BooleanColumn, ColorColumn, ColumnToggle, ColumnWidth, ComputedColumn, CountColumn, DateColumn,
    DateValue, EmbeddedColumn, FileColumn, ImageColumn, IntoColumns, RelationColumn, RelationLens,
    RepeaterColumn, TextColumn, ToOneRelation, contains_expr,
};
pub use declaration::{
    ActionInputFault, AuthMailFault, DeclarationError, DeclarationErrorKind, FieldAccessFault,
    FieldRuleFault, LocaleFault, ManyToManyFault, MountError, ProfileFault, ResetFault,
    SegmentFault, SignUpFault, Site, TenancyFault, TenantProfileFault, TenantRegistrationFault,
    TwoFactorFault, VerificationFault,
};
pub use derive::{EmbeddedForm, FormInput, ModelColumns, ModelForm, Options, RecordForm};
pub use detail::{Detail, IntoDetail};
pub use display::{Alignment, Color, Currency, Number, Numeric};
pub use form::{NoForm, Posted, RecordForm};
pub use i18n::Text;
pub use lens::{Lens, Reads};
pub use models::register_models;
pub use navigation::{NavTarget, NavigationItem};
pub use notification::stored::Notifications;
pub use outcome::{Notification, NotificationAction, NotificationStatus, Outcome};
pub use page::{Page, PageDef};
pub use panel::{
    Brand, MAX_IMPORT_ROWS, Panel, PanelHandle, RouterBuilderPanelExt, WiredTable, can_list,
    can_open, navigation_item, pages::header_action_buttons, stats, url, widgets, wired_table,
};
pub use policy::{Abilities, Ability, Allow, And, Deny, Or, Policy, ReadOnly, When, when};
pub use resource::{
    Action, Change, FieldAccess, ForeignKey, GlobalSearch, HeaderAction, HeaderActions, Places,
    PublicLink, Relation, Resource, ResourceDef, Trashed, WriteError, can, scoped_query,
    scoped_query_in, write_create, write_update,
};
#[cfg(feature = "rich-text")]
pub use rich_text::tool::Tool;
#[cfg(feature = "rich-text")]
pub use schema::RichTextField;
pub use schema::{
    ChoiceField, CustomField, EmbeddedForm, Field, FileField, Grid, Group, IntoFormValue,
    IntoOptions, IntoSchema, MAX_RELATIONSHIP_OPTIONS, MAX_REPEATER_ROWS, Options, RepeaterField,
    Schema, Section, Source, Switch, Tab, Tabs, TextField, Toggle, Watched, input::FormInput,
    parse_items, write_items,
};
pub use table::{
    ActionLayout, Cursor, DEFAULT_PAGE_SIZE, DEFAULT_PER_PAGE_OPTIONS, DateFilter, FilterValue,
    IntoFilters, ModelColumns, PagePosition, Pagination, QueryFilter, SelectFilter, Sort, Table,
    TablePage, TableState, TernaryFilter,
};
pub use tenancy::{Membership, Tenancy, Tenant, TenantColumn, TenantId, require_tenant, tenant_id};
pub use upload::{DirUploader, Uploader, sanitize_filename};
pub use values::{
    FieldError, FieldErrorKind, FieldErrors, FormField, FormScalar, FormValue, FormValues,
    parse_list, parse_password, parse_scalar, time_zone, write_list,
};
pub use widget::{Stat, Trend, Widget, WidgetDef};
