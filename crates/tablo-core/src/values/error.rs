//! [`FieldError`] and [`FieldErrors`]: the keys a submission was refused under, and why.

use std::collections::HashSet;

use crate::Text;

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

/// One key a submission was refused under: a form key, or a record form's
/// field enum in [`Resource::validate_record`](crate::Resource::validate_record).
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

/// A refused submission.
///
/// [`Resource::validate_record`](crate::Resource::validate_record) keys it by the record form's
/// field enum (`FieldErrors<UserFormField>`), so an error always names a field the form renders;
/// one on an embedded value renders under its first control. The panel keys every error by the
/// form key it renders under (`FieldErrors`, keyed by `String`), and [`Source::form`] reads them
/// back when a schema renders.
///
/// [`Source::form`]: crate::Source::form
#[derive(Debug, Clone)]
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
    pub fn add(&mut self, key: impl Into<K>, message: impl Into<Text>) {
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
