//! The reads of one key from a completed submission: a scalar, a password, a multiple choice's
//! list, and the write of that list.

use super::{FieldError, FormScalar, FormValue, FormValues, unreadable};

/// Read one scalar from a completed submission.
///
/// Trims the value; an empty one is `blank`, the field's blank answer, or
/// refuses as [`FieldErrorKind::Required`](super::FieldErrorKind::Required) when it has none.
pub fn parse_scalar<T: FormScalar>(
    key: &str,
    values: &FormValues,
    blank: Option<T>,
) -> std::result::Result<T, FieldError> {
    let raw = match values.get(key) {
        None => "",
        Some(FormValue::Text(text)) => text.trim(),
        Some(_) => return Err(unreadable(key)),
    };
    if raw.is_empty() {
        return blank.ok_or_else(|| FieldError::required(key));
    }
    T::parse_form(raw).map_err(|message| FieldError::invalid(key, message))
}

/// Read a password from a completed submission: required as [`parse_scalar`] reads a `String`,
/// but kept as typed, with its outer spaces, since a login compares the password untrimmed.
pub fn parse_password(
    key: &str,
    values: &FormValues,
    blank: Option<String>,
) -> std::result::Result<String, FieldError> {
    parse_scalar::<String>(key, values, blank)?;
    Ok(values.text(key).unwrap_or_default().to_string())
}

/// The values a multiple choice holds, as its one form key carries them: what a record form
/// hydrates a many-to-many field with, and what a hand-written one writes.
pub fn write_list<'a, T: FormScalar + 'a>(values: impl IntoIterator<Item = &'a T>) -> FormValue {
    FormValue::List(values.into_iter().map(T::to_form).collect())
}

/// Read the values a multiple choice posted, none when it posted nothing: what a record form's
/// parse reads a many-to-many field with.
///
/// # Errors
///
/// A value `T` refuses, or a key holding no list, under `key`.
pub fn parse_list<T: FormScalar>(
    key: &str,
    values: &FormValues,
) -> std::result::Result<Vec<T>, FieldError> {
    let items: &[String] = match values.get(key) {
        None => &[],
        Some(FormValue::List(items)) => items,
        Some(_) => return Err(unreadable(key)),
    };
    items
        .iter()
        .map(|item| T::parse_form(item).map_err(|message| FieldError::invalid(key, message)))
        .collect()
}

#[cfg(test)]
mod tests;
