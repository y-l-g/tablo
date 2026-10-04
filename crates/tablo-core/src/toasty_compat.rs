//! What Tablo builds on Toasty's internals because no public API offers it yet.
//!
//! Each item names the upstream gap it fills; it retires when Toasty closes that gap.
//!
//! - [`cursor`]: a pagination cursor's `toasty_core` value as a URL token (upstream #398).
//! - [`pk`]: the primary key read off an instance (upstream #119) and spelled as a URL id that
//!   parses back into a predicate through `toasty_core` (upstream #114).
//! - [`value_text`]: a `toasty_core` value's text, for URL ids and enum discriminants (upstream
//!   #398).

pub(crate) mod cursor;
pub(crate) mod pk;

use toasty_core::stmt::Value;

/// The text a scalar value is spelled as in a URL or a form: the inverse of the parse
/// [`pk::pk_eq_expr`] applies. `None` for a value with no text form.
pub(crate) fn value_text(value: &Value) -> Option<String> {
    Some(match value {
        Value::Bool(v) => v.to_string(),
        Value::I8(v) => v.to_string(),
        Value::I16(v) => v.to_string(),
        Value::I32(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::U8(v) => v.to_string(),
        Value::U16(v) => v.to_string(),
        Value::U32(v) => v.to_string(),
        Value::U64(v) => v.to_string(),
        Value::F32(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Uuid(v) => v.to_string(),
        Value::Timestamp(v) => v.to_string(),
        Value::Date(v) => v.to_string(),
        Value::Time(v) => v.to_string(),
        Value::DateTime(v) => v.to_string(),
        Value::Zoned(v) => v.to_string(),
        Value::Bytes(v) => String::from_utf8(v.clone()).ok()?,
        _ => return None,
    })
}
