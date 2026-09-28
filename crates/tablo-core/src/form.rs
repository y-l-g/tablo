//! Record forms: the typed value a form submission parses into, and the write
//! that stores it.
//!
//! A form-bearing resource declares one struct with
//! [`#[derive(RecordForm)]`](crate::RecordForm): one field per model column the
//! form writes, named and typed like the model's field. The panel parses every
//! submission into that struct, hydrates the edit and detail pages from it, and
//! writes it through toasty's generated builders. ADR-0022 records the design.
//!
//! On edit, a declared key the submission does not post is **completed** from
//! the stored record before the parse, so [`FormResource::update_record`]
//! receives a whole form, and [`Posted`] records which fields the submission
//! **named**. The write assigns only named fields.
//!
//! ```no_run
//! #[derive(Debug, Clone, toasty::Model)]
//! struct User {
//!     #[key]
//!     #[auto]
//!     id: uuid::Uuid,
//!     name: String,
//!     age: i64,
//! }
//!
//! #[derive(tablo_core::RecordForm)]
//! #[record_form(model = User)]
//! struct UserForm {
//!     name: String,
//!     #[record_form(blank = 0)]
//!     age: i64,
//! }
//! ```
//!
//! # Compile-time refusals
//!
//! A field the model does not have:
//!
//! ```compile_fail
//! # #[derive(Debug, Clone, toasty::Model)]
//! # struct User { #[key] #[auto] id: uuid::Uuid, name: String, age: i64 }
//! #[derive(tablo_core::RecordForm)]
//! #[record_form(model = User)]
//! struct UserForm {
//!     nickname: String,
//! }
//! ```
//!
//! A type the model's field does not have:
//!
//! ```compile_fail
//! # #[derive(Debug, Clone, toasty::Model)]
//! # struct User { #[key] #[auto] id: uuid::Uuid, name: String, age: i64 }
//! #[derive(tablo_core::RecordForm)]
//! #[record_form(model = User)]
//! struct UserForm {
//!     age: i32,
//! }
//! ```
//!
//! `embed` on a type that is not an [`EmbeddedForm`](crate::EmbeddedForm):
//!
//! ```compile_fail
//! # #[derive(Debug, Clone, toasty::Model)]
//! # struct User { #[key] #[auto] id: uuid::Uuid, name: String, age: i64 }
//! #[derive(tablo_core::RecordForm)]
//! #[record_form(model = User)]
//! struct UserForm {
//!     #[record_form(embed)]
//!     name: String,
//! }
//! ```

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use toasty::{Executor, schema::Model, stmt::IntoInsert};
use topcoat::{Result, context::Cx};

use crate::{
    resource::Resource,
    schema::{Schema, TypedValue},
    tenancy::{require_tenant, tenant_field_index},
};

/// A type one form key reads and writes: `String`, every [`TypedValue`] type,
/// and an `Option` of either.
///
/// The value the parse sees is trimmed and non-empty; an empty submission is the
/// field's **blank answer** instead (`""` for `String`, `None` for an `Option`,
/// otherwise the `blank` a record form declares).
pub trait FormScalar: Sized {
    /// The value an empty submission reads as, when the type has one.
    fn blank() -> Option<Self>;

    /// Parse a trimmed, non-empty submission, or return the error message.
    fn parse_form(value: &str) -> std::result::Result<Self, String>;

    /// The form spelling of a stored value.
    fn to_form(&self) -> String;
}

impl FormScalar for String {
    fn blank() -> Option<Self> {
        Some(String::new())
    }

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        Ok(value.to_string())
    }

    fn to_form(&self) -> String {
        self.clone()
    }
}

impl<T: TypedValue> FormScalar for T {
    fn blank() -> Option<Self> {
        None
    }

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        match value.parse::<T>() {
            Ok(parsed) if T::accepts(&parsed) => Ok(parsed),
            _ => Err(format!("`{value}` is not a valid {}", T::NOUN)),
        }
    }

    fn to_form(&self) -> String {
        self.to_string()
    }
}

impl<T: TypedValue> FormScalar for Option<T> {
    fn blank() -> Option<Self> {
        Some(None)
    }

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        T::parse_form(value).map(Some)
    }

    fn to_form(&self) -> String {
        self.as_ref().map(T::to_form).unwrap_or_default()
    }
}

impl FormScalar for Option<String> {
    fn blank() -> Option<Self> {
        Some(None)
    }

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        Ok(Some(value.to_string()))
    }

    fn to_form(&self) -> String {
        self.clone().unwrap_or_default()
    }
}

/// Why a key failed to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldErrorKind {
    /// Posted empty, and the field has no blank answer.
    Required,
    /// Posted a value the field's type does not accept.
    Invalid,
}

/// One key a parse refused, with the message the form renders under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// The form key the error renders under.
    pub key: String,
    /// Why the key failed.
    pub kind: FieldErrorKind,
    /// The sentence the form shows.
    pub message: String,
}

impl FieldError {
    /// `key` was posted empty and its field has no blank answer.
    pub fn required(key: impl Into<String>) -> Self {
        let key = key.into();
        let message = format!("{key} is required");
        Self {
            key,
            kind: FieldErrorKind::Required,
            message,
        }
    }

    /// `key` carried a value its type refuses; `message` says why.
    pub fn invalid(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: FieldErrorKind::Invalid,
            message: message.into(),
        }
    }
}

/// Read one scalar from a completed submission.
///
/// Trims the value; an empty one takes `blank` when the record form declares
/// one, else the type's own [`FormScalar::blank`], else refuses as
/// [`FieldErrorKind::Required`].
pub fn parse_scalar<T: FormScalar>(
    key: &str,
    values: &HashMap<String, String>,
    blank: Option<T>,
) -> std::result::Result<T, FieldError> {
    let raw = values.get(key).map(|value| value.trim()).unwrap_or("");
    if raw.is_empty() {
        return blank
            .or_else(T::blank)
            .ok_or_else(|| FieldError::required(key));
    }
    T::parse_form(raw).map_err(|message| FieldError::invalid(key, message))
}

/// One record-form field and the form keys it binds.
#[derive(Debug, Clone)]
pub struct FormField<K> {
    /// The field, as the form's field enum names it.
    pub field: K,
    /// The Rust field name, for build-time refusals.
    pub name: &'static str,
    /// The keys the field binds: one for a scalar, every key of an embedded
    /// value (an enum's discriminant first).
    pub keys: Vec<String>,
    /// Whether an empty submission has an answer. Always `true` for an
    /// embedded value, whose leaves read an empty key as `Default`.
    pub answers_blank: bool,
}

/// The typed value a resource's form submission parses into.
///
/// Derive it with [`tablo_core::RecordForm`](crate::RecordForm); a
/// hand-written impl is what the derive expands to.
pub trait RecordForm: Sized + Send + 'static {
    /// The model the form writes.
    type Model: Model + Send + Sync + 'static;

    /// One variant per form field: the key [`FieldErrors`] and [`Posted`] use.
    type Field: Copy + Eq + Hash + Debug + Send + Sync + 'static;

    /// Every field, in declaration order, with the keys it binds.
    fn fields(cx: &Cx) -> Vec<FormField<Self::Field>>;

    /// The stored record as the form spells it.
    fn hydrate(cx: &Cx, record: &Self::Model) -> HashMap<String, String>;

    /// Parse a completed, normalized submission.
    ///
    /// # Errors
    ///
    /// Every key that failed, each once.
    fn parse(
        cx: &Cx,
        values: &HashMap<String, String>,
    ) -> std::result::Result<Self, Vec<FieldError>>;

    /// The create builder with every field set.
    fn into_create(self) -> <Self::Model as Model>::Create;

    /// The instance update builder with one assignment per named field, or
    /// `None` when no field is named.
    ///
    /// `None` is what keeps a submission that names no field from building an
    /// empty update, which toasty refuses with an assertion.
    fn into_update<'a>(
        self,
        record: &'a mut Self::Model,
        named: &HashSet<Self::Field>,
    ) -> Option<<Self::Model as Model>::Update<'a>>;

    /// Execute a builder [`Self::into_update`] returned.
    ///
    /// Toasty's instance update builder implements no trait that carries
    /// `exec`, so the generic write reaches it through the form.
    fn exec_update<'a>(
        update: <Self::Model as Model>::Update<'a>,
        ex: &'a mut dyn Executor,
    ) -> impl Future<Output = toasty::Result<()>> + Send + 'a;
}

/// An edit submission: the parsed form, and the fields the submission named.
///
/// Derefs to the form, so a record fn reads `posted.author_id` — the posted
/// value, or the stored one when the submission did not name the field.
pub struct Posted<F: RecordForm> {
    form: F,
    named: HashSet<F::Field>,
}

impl<F: RecordForm> Posted<F> {
    /// A posted form that names `named`.
    pub fn new(form: F, named: impl IntoIterator<Item = F::Field>) -> Self {
        Self {
            form,
            named: named.into_iter().collect(),
        }
    }

    /// Whether the submission named `field`.
    pub fn named(&self, field: F::Field) -> bool {
        self.named.contains(&field)
    }

    /// The update builder assigning every named field, or `None` when the
    /// submission named none. See [`RecordForm::into_update`].
    pub fn into_update(self, record: &mut F::Model) -> Option<<F::Model as Model>::Update<'_>> {
        self.form.into_update(record, &self.named)
    }

    /// The parsed form alone.
    pub fn into_form(self) -> F {
        self.form
    }
}

impl<F: RecordForm> std::ops::Deref for Posted<F> {
    type Target = F;

    fn deref(&self) -> &F {
        &self.form
    }
}

/// Field-keyed validation errors from [`FormResource::validate_record`].
pub struct FieldErrors<F: RecordForm> {
    errors: Vec<(F::Field, String)>,
}

impl<F: RecordForm> FieldErrors<F> {
    /// No errors.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Refuse `field` with `message`.
    pub fn add(&mut self, field: F::Field, message: impl Into<String>) {
        self.errors.push((field, message.into()));
    }

    /// Whether nothing was refused.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// The errors in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = &(F::Field, String)> {
        self.errors.iter()
    }
}

impl<F: RecordForm> Default for FieldErrors<F> {
    fn default() -> Self {
        Self::new()
    }
}

/// A resource with a create and edit form.
///
/// Register it with [`Panel::form_resource`](crate::Panel::form_resource). The
/// record fns default to the derived write, [`write_create`] and
/// [`write_update`]; override one to check something inside the transaction,
/// then delegate.
pub trait FormResource: Resource {
    /// The typed value the form's submission parses into.
    type Form: RecordForm<Model = Self::Model>;

    /// The schema the create and edit forms render.
    fn form(cx: &Cx) -> Schema;

    /// App-level rules on the parsed form. The errors render inline with a
    /// 200 and nothing is written; a record fn error is a 500, so a range or
    /// cross-field rule belongs here.
    fn validate_record(_cx: &Cx, _form: &Self::Form) -> FieldErrors<Self::Form> {
        FieldErrors::new()
    }

    /// Create a record from the parsed form, inside the handler's transaction.
    ///
    /// `ex` is the open transaction: run every statement through it. Return the
    /// created row; it is what [`Resource::after_commit`] receives.
    fn create_record(
        cx: &Cx,
        form: Self::Form,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<Self::Model>> + Send {
        write_create::<Self>(cx, form, ex)
    }

    /// Update the already-authorized `record` from the posted form, inside the
    /// handler's transaction.
    ///
    /// `record` is the snapshot the handler loaded and policy-checked inside
    /// the transaction: use it, never re-query. Return the row as it now stands.
    fn update_record(
        cx: &Cx,
        record: Self::Model,
        posted: Posted<Self::Form>,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<Self::Model>> + Send {
        write_update::<Self>(cx, record, posted, ex)
    }
}

/// The derived create: the form's builder, the request tenant stamped on a
/// gated resource's tenant column, executed through `ex`.
///
/// A gated resource whose tenant is inherited (no `tenant_id` column of its
/// own) has nothing to stamp.
///
/// # Errors
///
/// A tenantless request on a gated resource (the handler answers 403 first),
/// or the driver's error.
pub async fn write_create<R: FormResource>(
    cx: &Cx,
    form: R::Form,
    ex: &mut dyn Executor,
) -> Result<R::Model> {
    let mut insert = form.into_create().into_insert();
    if R::requires_tenant()
        && let Some(index) = tenant_field_index::<R::Model>()
    {
        insert.set(index, toasty_core::stmt::Value::from(require_tenant(cx)?));
    }
    ex.exec(insert.into())
        .await
        .map_err(|error| -> topcoat::Error { error.into() })
}

/// The derived update: assign every named field and execute through `ex`.
///
/// A submission that names no field writes nothing. The instance update
/// reloads `record`, so the returned row is the written one.
///
/// # Errors
///
/// The driver's error.
pub async fn write_update<R: FormResource>(
    _cx: &Cx,
    mut record: R::Model,
    posted: Posted<R::Form>,
    ex: &mut dyn Executor,
) -> Result<R::Model> {
    // Build the execution future before awaiting: the builder itself is not
    // known to be `Send`, the future `exec_update` returns is.
    {
        let pending = posted
            .into_update(&mut record)
            .map(|update| <R::Form as RecordForm>::exec_update(update, &mut *ex));
        if let Some(pending) = pending {
            pending
                .await
                .map_err(|error| -> topcoat::Error { error.into() })?;
        }
    }
    Ok(record)
}
