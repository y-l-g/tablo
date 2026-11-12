//! [`FieldError`] and [`FieldErrors`]: the keys a submission was refused under, and why;
//! [`FormFields`]: how an app names the field it refuses.

use std::collections::HashSet;

use derive_where::derive_where;

use crate::Text;

/// The fields a submission's errors name: a record form's or a form input's, or the panel's own
/// form keys (`String`).
///
/// The [`RecordForm`](derive@crate::RecordForm), [`ModelForm`](derive@crate::ModelForm) and
/// [`FormInput`](derive@crate::FormInput) derives implement it, so an app keys its errors by its
/// form, `FieldErrors<UserForm>`, and picks a field by its name, `errors.add(|f| f.age, ..)`.
/// The field enum the derive writes, `UserFormField`, is the transport: the key the panel posts
/// and renders an error under, which an app's code never needs to spell.
pub trait FormFields {
    /// What an error names: one variant per field, or a form key.
    type Field;

    /// One key per field, named like the field, which an error picks a field from: the derive
    /// writes `UserFormKeys`, holding `age: UserFormField::Age`; a form with no field has `()`.
    type Keys;

    /// The keys.
    fn keys() -> Self::Keys;
}

/// The panel's own errors, keyed by the form key they render under.
impl FormFields for String {
    type Field = String;
    type Keys = ();

    fn keys() -> Self::Keys {}
}

/// Why a key was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldErrorKind {
    /// Posted empty, and the field has no blank answer. The form renders
    /// "{label} is required" under the control.
    Required,
    /// Posted a value the field refuses, with the sentence the form renders.
    Invalid(Text),
}

/// One key a submission was refused under: a form key, or the field enum of the form a
/// [`FieldErrors`] is keyed by.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    pub fn invalid(key: impl Into<K>, message: impl Into<Text>) -> Self {
        Self {
            key: key.into(),
            kind: FieldErrorKind::Invalid(message.into()),
        }
    }

    /// The sentence the form renders under the control labelled `label`.
    pub fn message(&self, label: impl Into<Text>) -> Text {
        match &self.kind {
            FieldErrorKind::Required => crate::text!("tablo-field-required", label = label),
            FieldErrorKind::Invalid(message) => message.clone(),
        }
    }
}

/// The refusal of a key holding another shape than its parse reads: a list or rows where it reads
/// text, or text where it reads a list. Only a schema whose controls disagree with its parse
/// holds one.
pub(crate) fn unreadable(key: &str) -> FieldError {
    FieldError::invalid(key, Text::message("tablo-field-unreadable"))
}

/// A refused submission, keyed by the form whose fields it names.
///
/// [`Resource::validate_record`](crate::Resource::validate_record) answers a
/// `FieldErrors<UserForm>`, keyed by its record form, so an error always names a field the form
/// renders; one on an embedded value renders under its first control. An error picks its field
/// off the form's keys:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, age: i64 }
/// # #[derive(Debug, Clone, tablo_core::RecordForm)]
/// # #[form(model = User)]
/// # struct UserForm { age: i64 }
/// use tablo_core::FieldErrors;
///
/// fn validate(form: &UserForm) -> FieldErrors<UserForm> {
///     let mut errors = FieldErrors::<UserForm>::new();
///     if form.age < 0 {
///         errors.add(|f| f.age, "Age must be zero or more");
///     }
///     errors
/// }
/// ```
///
/// The panel keys every error by the form key it renders under (`FieldErrors`, keyed by
/// `String`), and [`Source::form`] reads them back when a schema renders.
///
/// [`Source::form`]: crate::Source::form
#[derive_where(Debug, Clone; F::Field)]
pub struct FieldErrors<F: FormFields = String> {
    errors: Vec<FieldError<F::Field>>,
}

impl<F: FormFields> Default for FieldErrors<F> {
    fn default() -> Self {
        Self { errors: Vec::new() }
    }
}

impl<F: FormFields> FieldErrors<F> {
    /// No errors.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuse the field `pick` picks off the form's keys with `message`:
    /// `errors.add(|f| f.age, "Age must be zero or more")`.
    pub fn add(&mut self, pick: impl FnOnce(F::Keys) -> F::Field, message: impl Into<Text>) {
        self.add_field(pick(F::keys()), message);
    }

    /// Refuse `field` with `message`, the field named directly: the panel's own errors name a
    /// form key, `errors.add_field("email", ..)`.
    pub fn add_field(&mut self, field: impl Into<F::Field>, message: impl Into<Text>) {
        self.errors.push(FieldError::invalid(field.into(), message));
    }

    /// Refuse `error`'s key with `error`.
    pub fn push(&mut self, error: FieldError<F::Field>) {
        self.errors.push(error);
    }

    /// Whether nothing was refused.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Every error, in the order it was added.
    pub fn iter(&self) -> impl Iterator<Item = &FieldError<F::Field>> {
        self.errors.iter()
    }

    /// Append `other`'s errors after these.
    pub fn extend(&mut self, other: Self) {
        self.errors.extend(other.errors);
    }

    /// These errors, each keyed by the form key `key` gives its field.
    pub(crate) fn keyed(self, key: fn(F::Field) -> &'static str) -> FieldErrors {
        let mut keyed = FieldErrors::new();
        for error in self {
            keyed.push(FieldError {
                key: key(error.key).to_string(),
                kind: error.kind,
            });
        }
        keyed
    }
}

impl<F: FormFields> IntoIterator for FieldErrors<F> {
    type Item = FieldError<F::Field>;
    type IntoIter = std::vec::IntoIter<FieldError<F::Field>>;

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
