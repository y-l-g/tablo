//! What Tablo builds on Toasty's internals because no public API offers it yet.
//!
//! This is the only module that names `toasty_core` or reads a model's `app`, `mapping` or `db`
//! schema; `tests/layers.rs` refuses either anywhere else. An item that fills an upstream gap
//! names it, and retires when Toasty closes that gap.
//!
//! - [`cursor`]: a pagination cursor's value as a URL token (upstream #398).
//! - [`model`]: a model's fields, columns, indices and relations, and an embedded path's column
//!   (upstream #183).
//! - [`pk`]: the primary key read off an instance and spelled as a URL id that parses back into a
//!   predicate through `toasty_core`'s untyped expressions.
//! - [`value_text`]: a value's text, for URL ids and enum discriminants.
//! - [`UntypedInclude`] and [`Assignments`]: the `toasty_core` types a public Toasty API takes
//!   without re-exporting them.
//! - [`VariantId`]: an embedded enum variant's id, re-exported for the derives.

pub(crate) mod cursor;
pub(crate) mod model;
pub(crate) mod pk;

use toasty::stmt::Value;

/// An include with its model and target types erased, which `Query::include` takes.
pub(crate) type UntypedInclude = toasty_core::stmt::Include;

/// An embedded enum variant's id, which the derives name.
pub use toasty::schema::app::VariantId;
/// The assignments `toasty::schema::Field::new_update` takes.
pub(crate) use toasty_core::stmt::Assignments;

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
