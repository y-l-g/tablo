//! Form values: what a form holds, one typed value per key; the [`FormScalar`] types a key reads
//! and writes, the [`TypedValue`]s among them, and the [`FieldError`]s a key is refused with.
//!
//! The module sits below `schema`, which reads and renders form values, and `form`, whose record
//! forms hydrate and parse them; `form` re-exports it.

mod error;
mod parse;
mod scalar;
mod typed;
mod zone;

use std::collections::BTreeMap;

pub(crate) use error::unreadable;
pub use error::{FieldError, FieldErrorKind, FieldErrors};
pub use parse::{parse_list, parse_password, parse_scalar, write_list};
pub use scalar::{FormScalar, NullableScalar, assert_form_scalar};
pub use typed::TypedValue;
pub(crate) use typed::{is_email, is_http_url, read_datetime_local};
pub(crate) use zone::ZoneSource;
pub use zone::time_zone;

/// What a form holds, one [`FormValue`] per key: a submission its schema
/// [read](crate::Schema::read), or a stored record as its record form spells it.
///
/// A record form's [`hydrate`](crate::RecordForm::hydrate) writes one and its
/// [`parse`](crate::RecordForm::parse) reads one, as a [`FormInput`](crate::FormInput)'s parse
/// does. Each key holds the shape its control posts: [`Text`](FormValue::Text) for a scalar
/// control, a [`List`](FormValue::List) for a multiple choice, and [`Rows`](FormValue::Rows) for a
/// repeater.
///
/// ```
/// use tablo_core::FormValues;
///
/// let mut values = FormValues::new();
/// values.insert("title", "Hello");
/// values.insert("tags", vec!["rust".to_string(), "web".to_string()]);
/// assert_eq!(values.text("title"), Some("Hello"));
/// assert_eq!(values.list("tags").map(<[String]>::len), Some(2));
/// assert_eq!(values.text("tags"), None);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormValues {
    values: BTreeMap<String, FormValue>,
}

impl FormValues {
    /// No value.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether no key holds a value.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// How many keys hold a value.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// The value `key` holds.
    pub fn get(&self, key: &str) -> Option<&FormValue> {
        self.values.get(key)
    }

    /// The text `key` holds; `None` when it holds none, or a list or rows.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(FormValue::as_text)
    }

    /// The list `key` holds; `None` when it holds none, or text or rows.
    pub fn list(&self, key: &str) -> Option<&[String]> {
        self.get(key).and_then(FormValue::as_list)
    }

    /// The rows `key` holds; `None` when it holds none, or text or a list.
    pub fn rows(&self, key: &str) -> Option<&[FormValues]> {
        self.get(key).and_then(FormValue::as_rows)
    }

    /// Whether `key` holds a value.
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Sets `key` to `value`, returning what it held.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<FormValue>,
    ) -> Option<FormValue> {
        self.values.insert(key.into(), value.into())
    }

    /// Removes `key`, returning what it held.
    pub fn remove(&mut self, key: &str) -> Option<FormValue> {
        self.values.remove(key)
    }

    /// Every key holding a value, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// Every key and its value, sorted by key.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &FormValue)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }
}

impl<K: Into<String>, V: Into<FormValue>> FromIterator<(K, V)> for FormValues {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut values = Self::new();
        values.extend(iter);
        values
    }
}

/// Sets each key in turn: a later value replaces an earlier one.
impl<K: Into<String>, V: Into<FormValue>> Extend<(K, V)> for FormValues {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl IntoIterator for FormValues {
    type Item = (String, FormValue);
    type IntoIter = std::collections::btree_map::IntoIter<String, FormValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

/// One key's value in [`FormValues`], in the shape its control posts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormValue {
    /// A scalar control's value, as typed: a text field's, a single choice's option, a toggle's
    /// `true` or `false`, a file field's stored path.
    Text(String),
    /// A multiple choice's options. [`Schema::read`](crate::Schema::read) keeps each once and
    /// none blank, in the order they posted.
    List(Vec<String>),
    /// A repeater's rows, in the order they show, each holding its item's values.
    Rows(Vec<FormValues>),
}

impl FormValue {
    /// The text this value holds, if it is one.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The list this value holds, if it is one.
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// The rows this value holds, if it is rows.
    pub fn as_rows(&self) -> Option<&[FormValues]> {
        match self {
            Self::Rows(rows) => Some(rows),
            _ => None,
        }
    }
}

impl From<String> for FormValue {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for FormValue {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

impl From<&String> for FormValue {
    fn from(text: &String) -> Self {
        Self::Text(text.clone())
    }
}

impl From<Vec<String>> for FormValue {
    fn from(items: Vec<String>) -> Self {
        Self::List(items)
    }
}

impl From<Vec<FormValues>> for FormValue {
    fn from(rows: Vec<FormValues>) -> Self {
        Self::Rows(rows)
    }
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
