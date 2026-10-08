//! Record forms: the typed value a form submission parses into, and the write
//! that stores it.
//!
//! Declares one `RecordForm` struct with one field per written model column.
//! The struct is the form's one source of truth: it parses every key, and a
//! field's blank answer decides whether its control is required. Completes
//! unposted keys from the stored record and writes only named fields plus model
//! defaults.
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
//! `optional` on a type with no empty value:
//!
//! ```compile_fail
//! # #[derive(Debug, Clone, toasty::Model)]
//! # struct User { #[key] #[auto] id: uuid::Uuid, name: String, age: i64 }
//! #[derive(tablo_core::RecordForm)]
//! #[form(model = User)]
//! struct UserForm {
//!     #[form(optional)]
//!     age: i64,
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

use toasty::{Executor, schema::Model};
use topcoat::context::Cx;

use crate::{
    detail::Detail,
    schema::{FieldResolver, Schema, TypedValue},
    table::Table,
};

/// A type one form key reads and writes: `String`, every [`TypedValue`] type,
/// every enum deriving [`Options`](crate::Options), and an `Option` of any of them.
///
/// The value the parse sees is trimmed and non-empty; an empty submission is the
/// record-form field's **blank answer** instead, and a field with none is required.
///
/// A text field binds a path of any `FormScalar` type
/// ([`Field::text`](crate::Field::text)), and the form derives read and write
/// every field that is not `#[form(embed)]` through it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a form scalar",
    label = "a form field of this type has no text spelling",
    note = "a form scalar is `String`, a `TypedValue` type, an `Options` enum, or an `Option` of \
            one; implement `TypedValue` for an app type, derive `Options` for a unit enum, or \
            mark an `EmbeddedForm` value `#[form(embed)]`"
)]
pub trait FormScalar: Sized {
    /// The `type` attribute of the text control that edits it.
    const INPUT_TYPE: &'static str = "text";

    /// Parse a trimmed, non-empty submission, or return the error message.
    fn parse_form(value: &str) -> std::result::Result<Self, String>;

    /// The form spelling of a stored value.
    fn to_form(&self) -> String;

    /// What a column cell and a group header read: the form spelling, or an
    /// `Options` enum's label.
    fn to_label(&self) -> String {
        self.to_form()
    }
}

impl FormScalar for String {
    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        Ok(value.to_string())
    }

    fn to_form(&self) -> String {
        self.clone()
    }
}

impl<T: TypedValue> FormScalar for T {
    const INPUT_TYPE: &'static str = T::INPUT_TYPE;

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        T::parse_input(value).ok_or_else(|| format!("`{value}` is not a valid {}", T::NOUN))
    }

    fn to_form(&self) -> String {
        self.to_string()
    }
}

/// A form scalar whose `Option` is one too: every [`TypedValue`] type, and every enum deriving
/// [`Options`](crate::Options), which implements it.
#[doc(hidden)]
pub trait NullableScalar: FormScalar {}

impl<T: TypedValue> NullableScalar for T {}

impl<T: NullableScalar> FormScalar for Option<T> {
    const INPUT_TYPE: &'static str = T::INPUT_TYPE;

    fn parse_form(value: &str) -> std::result::Result<Self, String> {
        T::parse_form(value).map(Some)
    }

    fn to_form(&self) -> String {
        self.as_ref().map(T::to_form).unwrap_or_default()
    }

    fn to_label(&self) -> String {
        self.as_ref().map(T::to_label).unwrap_or_default()
    }
}

impl FormScalar for Option<String> {
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

/// Why a key was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldErrorKind {
    /// Posted empty, and the field has no blank answer. The form renders
    /// "{label} is required" under the control.
    Required,
    /// Posted a value the field refuses, with the sentence the form renders.
    Invalid(String),
}

/// One key a submission was refused under: a form key, or a record form's
/// field enum in [`Resource::validate_record`](crate::Resource::validate_record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError<K = String> {
    /// What the error renders under.
    pub key: K,
    /// Why the key was refused.
    pub kind: FieldErrorKind,
}

impl<K> FieldError<K> {
    /// `key` was posted empty and has no blank answer.
    pub fn required(key: impl Into<K>) -> Self {
        Self {
            key: key.into(),
            kind: FieldErrorKind::Required,
        }
    }

    /// `key` carried a value it refuses; `message` says why.
    pub fn invalid(key: impl Into<K>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: FieldErrorKind::Invalid(message.into()),
        }
    }

    /// The sentence the form renders under the control labelled `label`.
    pub fn message(&self, label: &str) -> String {
        match &self.kind {
            FieldErrorKind::Required => format!("{label} is required"),
            FieldErrorKind::Invalid(message) => message.clone(),
        }
    }
}

/// Read one scalar from a completed submission.
///
/// Trims the value; an empty one is `blank`, the field's blank answer, or
/// refuses as [`FieldErrorKind::Required`] when it has none.
pub fn parse_scalar<T: FormScalar>(
    key: &str,
    values: &HashMap<String, String>,
    blank: Option<T>,
) -> std::result::Result<T, FieldError> {
    let raw = values.get(key).map(|value| value.trim()).unwrap_or("");
    if raw.is_empty() {
        return blank.ok_or_else(|| FieldError::required(key));
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
    /// value (an enum's discriminant first). An error on the field renders
    /// under the first.
    pub keys: Vec<String>,
    /// The keys with no blank answer, which an empty submission refuses: the
    /// panel renders their controls required.
    pub required: Vec<String>,
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

    /// Every field, in declaration order, with the keys it binds through `resolver`'s app schema.
    fn fields(resolver: &FieldResolver) -> Vec<FormField<Self::Field>>;

    /// The control the form renders `field` with when its [`form`](crate::ResourceDef::form)
    /// does not place it: what `controls()` hands over for the field, as a one-control schema.
    ///
    /// A resource's form renders every field it does not place after the ones it does, in
    /// declaration order, so a resource that declares no form renders one control per field.
    fn control(field: Self::Field) -> Schema<Self>;

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

    /// The form's default detail page: one column per field a column can show, in declaration
    /// order. A [`ResourceDef`](crate::ResourceDef) without a [`view`](crate::ResourceDef::view)
    /// shows it.
    ///
    /// The derive shows each field [`table`](Self::table) lists, a bare choice as the key it holds,
    /// a file field as a link, and an embedded value leaf by leaf (see
    /// [`RecordForm`](derive@crate::RecordForm)). A relation's record shows through a
    /// [`RelationColumn`](crate::RelationColumn), in a declared
    /// [`view`](crate::ResourceDef::view). The default here shows none, which turns the detail page
    /// off for a resource naming [`NoForm`].
    fn detail() -> Detail<Self::Model> {
        Detail::empty()
    }

    /// The stored record as the form spells it.
    fn hydrate(cx: &Cx, record: &Self::Model) -> HashMap<String, String>;

    /// Parse a completed submission: the form's only presence and type check.
    ///
    /// # Errors
    ///
    /// Every key that failed, each once, under one of [`Self::fields`]' keys.
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

    fn fields(_resolver: &FieldResolver) -> Vec<FormField<Self::Field>> {
        Vec::new()
    }

    fn control(field: Self::Field) -> Schema<Self> {
        match field {}
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

/// A refused submission.
///
/// [`Resource::validate_record`](crate::Resource::validate_record) keys it by the record form's
/// field enum (`FieldErrors<UserFormField>`), so an error always names a field the form renders;
/// one on an embedded value renders under its first control. The panel keys every error by the
/// form key it renders under (`FieldErrors`, keyed by `String`), and [`Source::form`] reads them
/// back when a schema renders.
///
/// [`Source::form`]: crate::Source::form
#[derive(Debug)]
pub struct FieldErrors<K = String> {
    errors: Vec<FieldError<K>>,
}

impl<K> Default for FieldErrors<K> {
    fn default() -> Self {
        Self { errors: Vec::new() }
    }
}

impl<K> FieldErrors<K> {
    /// No errors.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuse `key` with `message`.
    pub fn add(&mut self, key: impl Into<K>, message: impl Into<String>) {
        self.errors.push(FieldError::invalid(key, message));
    }

    /// Refuse `error`'s key with `error`.
    pub fn push(&mut self, error: FieldError<K>) {
        self.errors.push(error);
    }

    /// Whether nothing was refused.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Every error, in the order it was added.
    pub fn iter(&self) -> impl Iterator<Item = &FieldError<K>> {
        self.errors.iter()
    }

    /// Append `other`'s errors after these.
    pub fn extend(&mut self, other: Self) {
        self.errors.extend(other.errors);
    }
}

impl<K> IntoIterator for FieldErrors<K> {
    type Item = FieldError<K>;
    type IntoIter = std::vec::IntoIter<FieldError<K>>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl FieldErrors {
    /// Whether any error renders under `key`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.errors.iter().any(|error| error.key == key)
    }

    /// The error `key` renders: the first one added.
    pub fn first(&self, key: &str) -> Option<&FieldError> {
        self.errors.iter().find(|error| error.key == key)
    }

    /// Take `other`'s errors, dropping this collection's own errors under every
    /// key `other` names.
    ///
    /// A source that owns a key answers for it: a rejected upload replaces the
    /// error the emptied control would otherwise report.
    pub(crate) fn replace(&mut self, other: Self) {
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
