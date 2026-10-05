//! Record forms: the typed value a form submission parses into, and the write
//! that stores it.
//!
//! Declares one `RecordForm` struct with one field per written model column.
//! Completes unposted keys from the stored record and writes only named fields
//! plus model defaults.
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
//! #[form(model = User)]
//! struct UserForm {
//!     name: String,
//!     #[form(blank = 0)]
//!     age: i64,
//! }
//!
//! // A column type toasty stores but the form edge cannot spell.
//! #[derive(Debug, Clone, toasty::Model)]
//! struct Tagged {
//!     #[key]
//!     #[auto]
//!     id: uuid::Uuid,
//!     tags: Vec<String>,
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
//! #[form(model = User)]
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
//! #[form(model = User)]
//! struct UserForm {
//!     age: i32,
//! }
//! ```
//!
//! A scalar type the form edge cannot spell:
//!
//! ```compile_fail
//! #[derive(Debug, Clone, toasty::Model)]
//! struct Tagged {
//!     #[key]
//!     #[auto]
//!     id: uuid::Uuid,
//!     tags: Vec<String>,
//! }
//!
//! #[derive(tablo_core::RecordForm)]
//! #[form(model = Tagged)]
//! struct TaggedForm {
//!     tags: Vec<String>,
//! }
//! ```
//!
//! `embed` on a type that is not an [`EmbeddedForm`](crate::EmbeddedForm):
//!
//! ```compile_fail
//! # #[derive(Debug, Clone, toasty::Model)]
//! # struct User { #[key] #[auto] id: uuid::Uuid, name: String, age: i64 }
//! #[derive(tablo_core::RecordForm)]
//! #[form(model = User)]
//! struct UserForm {
//!     #[form(embed)]
//!     name: String,
//! }
//! ```

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use toasty::{Executor, schema::Model, stmt::IntoInsert};
use topcoat::context::Cx;

use crate::{
    schema::{Schema, TypedValue},
    table::Table,
};

/// A type one form key reads and writes: `String`, every [`TypedValue`] type,
/// and an `Option` of either.
///
/// The value the parse sees is trimmed and non-empty; an empty submission is the
/// field's **blank answer** instead (the `blank` a record form declares, else
/// `""` for `String` or `None` for an `Option`).
///
/// A text field binds a path of any `FormScalar` type
/// ([`Field::text`](crate::Field::text)), and the form derives read and write
/// every field that is not `#[form(embed)]` through it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a form scalar",
    label = "a form field of this type has no text spelling",
    note = "a form scalar is `String`, a `TypedValue` type, or an `Option` of one; implement \
            `TypedValue` for an app type, or mark an `EmbeddedForm` value `#[form(embed)]`"
)]
pub trait FormScalar: Sized {
    /// The `type` attribute of the text control that edits it.
    const INPUT_TYPE: &'static str = "text";

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
    const INPUT_TYPE: &'static str = T::INPUT_TYPE;

    fn blank() -> Option<Self> {
        None
    }

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        T::parse_input(value).ok_or_else(|| format!("`{value}` is not a valid {}", T::NOUN))
    }

    fn to_form(&self) -> String {
        self.to_string()
    }
}

impl<T: TypedValue> FormScalar for Option<T> {
    const INPUT_TYPE: &'static str = T::INPUT_TYPE;

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

/// Compiles only for a form scalar: the form derives call it, spanned on a
/// field's type, so a field of another type fails there.
#[doc(hidden)]
pub fn assert_form_scalar<T: FormScalar>() {}

/// Why a key failed to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldErrorKind {
    /// Posted empty, and the field has no blank answer.
    Required,
    /// Posted a value the field's type does not accept.
    Invalid,
}

/// One key a submission was refused under, with the sentence the form renders
/// beneath it.
///
/// The entry type of [`FieldErrors`]. [`Self::required`] is what
/// [`RecordForm::parse`] answers for an unanswered key, whose control supplies
/// the rendered wording.
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
    /// The submission left `key` unanswered — an empty field with no blank
    /// answer, or a required group whose inputs were all empty; `message` is
    /// the wording its control renders in the slot.
    pub fn unanswered(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: FieldErrorKind::Required,
            message: message.into(),
        }
    }

    /// `key` was posted empty and has no blank answer, worded as its own key
    /// names it.
    pub fn required(key: impl Into<String>) -> Self {
        let key = key.into();
        let message = format!("{key} is required");
        Self::unanswered(key, message)
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
    /// Whether an empty submission has an answer. The build check refuses the
    /// field only where one of its keys can be posted empty with nothing to
    /// resolve it; a scalar's answer is its declared `#[form(blank = ..)]` or
    /// its type's own, and an embedded value answers when every leaf does
    /// (`EmbeddedForm::answers_blank`).
    pub answers_blank: bool,
}

/// The typed value a resource's form submission parses into.
///
/// Derive it with [`tablo_core::RecordForm`](crate::RecordForm); a
/// hand-written impl is what the derive expands to.
pub trait RecordForm: Sized + Send + 'static {
    /// The model the form writes.
    type Model: Model + toasty::stmt::IntoExpr<Self::Model> + Send + Sync + 'static;

    /// One variant per form field: the key [`Posted`] uses. Its bound form keys
    /// are [`Self::fields`]'.
    type Field: Copy + Eq + Hash + Debug + Send + Sync + 'static;

    /// Every field, in declaration order, with the keys it binds.
    fn fields() -> Vec<FormField<Self::Field>>;

    /// The form's default schema: one control per field, in declaration
    /// order. A [`ResourceDef`](crate::ResourceDef) without a [`form`](crate::ResourceDef::form)
    /// renders it.
    ///
    /// The derive chooses each control from the field (see
    /// [`RecordForm`](derive@crate::RecordForm)); the default here declares
    /// none, which is what [`NoForm`] wants.
    fn schema() -> Schema {
        Schema::empty()
    }

    /// The form's default table: one column per field a column can show, in declaration order.
    /// A [`ResourceDef`](crate::ResourceDef) without a [`table`](crate::ResourceDef::table) lists
    /// it.
    ///
    /// The derive lists each text field in a sortable column, searchable over a `String` or
    /// `Option<String>`, an options field by its option's label, and a toggle as yes or no (see
    /// [`RecordForm`](derive@crate::RecordForm)). The default here lists none, so a resource
    /// whose form lists nothing, such as one naming [`NoForm`], declares its own.
    fn table() -> Table<Self::Model> {
        Table::new(())
    }

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

    /// Whether the resource naming this form has create and edit pages.
    ///
    /// [`Panel::resource`](crate::Panel::resource) registers the create, edit,
    /// and options routes only when this holds. [`NoForm`] sets it to `false`.
    const HAS_FORM: bool = true;
}

/// The record form of a resource with no create or edit page.
///
/// A list-only resource names it as [`Resource::Form`](crate::Resource::Form):
/// `type Form = NoForm<Self::Model>;`. [`fields`](RecordForm::fields) and
/// [`hydrate`](RecordForm::hydrate) are empty and
/// [`into_update`](RecordForm::into_update) answers `None`. No route parses or
/// writes it, so [`parse`](RecordForm::parse) and
/// [`into_create`](RecordForm::into_create) panic naming the model, and the
/// future [`exec_update`](RecordForm::exec_update) returns panics when polled.
pub struct NoForm<M>(std::marker::PhantomData<fn() -> M>);

impl<M: Model + toasty::stmt::IntoExpr<M> + Send + Sync + 'static> NoForm<M> {
    fn unreachable() -> ! {
        panic!("`NoForm<{}>` has no form", std::any::type_name::<M>())
    }
}

impl<M: Model + toasty::stmt::IntoExpr<M> + Send + Sync + 'static> RecordForm for NoForm<M> {
    type Model = M;
    type Field = std::convert::Infallible;

    const HAS_FORM: bool = false;

    fn fields() -> Vec<FormField<Self::Field>> {
        Vec::new()
    }

    fn hydrate(_cx: &Cx, _record: &M) -> HashMap<String, String> {
        HashMap::new()
    }

    fn parse(
        _cx: &Cx,
        _values: &HashMap<String, String>,
    ) -> std::result::Result<Self, Vec<FieldError>> {
        Self::unreachable()
    }

    fn into_create(self) -> M::Create {
        Self::unreachable()
    }

    fn into_update<'a>(
        self,
        _record: &'a mut M,
        _named: &HashSet<Self::Field>,
    ) -> Option<M::Update<'a>> {
        None
    }

    fn exec_update<'a>(
        update: M::Update<'a>,
        ex: &'a mut dyn Executor,
    ) -> impl Future<Output = toasty::Result<()>> + Send + 'a {
        // `into_update` answers `None`, so no builder reaches here. The
        // builder is not known to be `Send`, so the future must not hold it.
        drop((update, ex));
        async { Self::unreachable() }
    }
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
}

impl<F: RecordForm> std::ops::Deref for Posted<F> {
    type Target = F;

    fn deref(&self) -> &F {
        &self.form
    }
}

/// A refused submission, keyed by the form key each error renders under.
///
/// One type for every source: the schema's own rules
/// ([`Schema::validate`](crate::schema::Schema::validate)), a rejected upload, a
/// failed uniqueness probe, and an app's
/// [`validate_record`](crate::Resource::validate_record). The submit handler
/// merges them without translating, and the form render reads each field's own
/// key from the result.
///
/// A key the rendered form owns: a control's own key, or a
/// [`Repeater`](crate::Repeater) group's label. A key no slot owns has nowhere
/// to render, and the submit handler refuses it as a declaration error rather
/// than writing past it.
#[derive(Debug, Default)]
pub struct FieldErrors {
    errors: Vec<FieldError>,
}

impl FieldErrors {
    /// No errors.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuse `key` with `message`.
    pub fn add(&mut self, key: impl Into<String>, message: impl Into<String>) {
        self.errors.push(FieldError::invalid(key, message));
    }

    /// Refuse `key` as unanswered, with the message its control renders.
    pub fn add_required(&mut self, key: impl Into<String>, message: impl Into<String>) {
        self.errors.push(FieldError::unanswered(key, message));
    }

    /// Refuse `error`'s key with `error`, keeping the error's own kind.
    pub fn push(&mut self, error: FieldError) {
        self.errors.push(error);
    }

    /// Whether any error renders under `key`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.errors.iter().any(|error| error.key == key)
    }

    /// The error `key` renders: the first one added, like the render's own
    /// first-message slot.
    pub fn first(&self, key: &str) -> Option<&FieldError> {
        self.errors.iter().find(|error| error.key == key)
    }

    /// Whether nothing was refused.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Every error, in the order it was added.
    pub fn iter(&self) -> impl Iterator<Item = &FieldError> {
        self.errors.iter()
    }

    /// Append `other`'s errors after these.
    pub fn extend(&mut self, other: Self) {
        self.errors.extend(other.errors);
    }

    /// Take `other`'s errors, dropping this collection's own errors under every
    /// key `other` names.
    ///
    /// A source that owns a key answers for it: a rejected upload replaces the
    /// "required" the emptied control would otherwise report.
    pub fn replace(&mut self, other: Self) {
        let owned: HashSet<&str> = other
            .errors
            .iter()
            .map(|error| error.key.as_str())
            .collect();
        self.errors
            .retain(|kept| !owned.contains(kept.key.as_str()));
        self.errors.extend(other.errors);
    }
}

/// Which of `M`'s root fields the create builder fills before any setter runs:
/// `#[auto]` fields, which the database fills, and `#[default(..)]` ones.
///
/// Read off `M::Create::default()`, because toasty keeps a `#[default]` in its
/// generated code only, never in the app schema.
pub(crate) fn prefilled_fields<M: Model>() -> Vec<bool> {
    let insert = <M::Create as Default>::default().into_insert();
    let toasty_core::stmt::Expr::Stmt(statement) = toasty_core::stmt::Expr::from(insert) else {
        return Vec::new();
    };
    statement
        .stmt
        .as_insert()
        .and_then(|insert| insert.source.body.as_values())
        .and_then(|values| values.rows.last())
        .and_then(|row| row.as_record())
        .map(|record| {
            record
                .fields
                .iter()
                .map(|expr| !expr.is_value_null())
                .collect()
        })
        .unwrap_or_default()
}
