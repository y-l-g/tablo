//! A model's primary key read off an instance, spelled as a URL id, and parsed back into a
//! predicate.
//!
//! The key is read through the model's `IntoExpr::by_ref`, which Toasty's derive builds from the
//! primary-key fields.

use toasty::{
    schema::app::{FieldId, FieldTy},
    stmt::{IntoExpr, Type, Value},
};
use toasty_core::stmt::Expr;

use super::value_text;

/// `M`'s primary-key fields, in declaration order.
fn pk_fields<M>() -> Vec<FieldId>
where
    M: toasty::schema::Model,
{
    M::schema()
        .as_root()
        .map(|root| root.primary_key.fields.clone())
        .unwrap_or_default()
}

/// The primary-key values of `record`, one per key field, or none when the derived expression
/// has another shape.
pub(crate) fn pk_values<M>(record: &M) -> Vec<Value>
where
    M: toasty::schema::Model + IntoExpr<M>,
{
    let values = match Expr::from(record.by_ref()) {
        Expr::Value(Value::Record(record)) => record.fields,
        Expr::Record(record) => record
            .fields
            .into_iter()
            .map(|field| match field {
                Expr::Value(value) => Some(value),
                _ => None,
            })
            .collect::<Option<_>>()
            .unwrap_or_default(),
        Expr::Value(value) => vec![value],
        _ => Vec::new(),
    };
    if values.len() == pk_fields::<M>().len() {
        values
    } else {
        Vec::new()
    }
}

/// The URL id of `record`: its primary key's text, with a composite key's parts joined by `,`.
///
/// A single key's text parses back through [`pk_eq_expr`]. A composite key has no URL
/// representation ([`pk_is_composite`]); its text identifies the row within a page only, and its
/// table renders no row action.
pub(crate) fn pk_text<M>(record: &M) -> String
where
    M: toasty::schema::Model + IntoExpr<M>,
{
    pk_values(record)
        .iter()
        .map(|value| value_text(value).unwrap_or_else(|| format!("{value:?}")))
        .collect::<Vec<_>>()
        .join(",")
}

/// The predicate selecting `record` by its primary key, composite keys included.
pub(crate) fn pk_filter<M>(record: &M) -> toasty::stmt::Expr<bool>
where
    M: toasty::schema::Model + IntoExpr<M>,
{
    // `pk_values` answers one value per key field, or none.
    let operands = pk_fields::<M>()
        .into_iter()
        .zip(pk_values(record))
        .map(|(field, value)| Expr::eq(Expr::ref_self_field(field), Expr::from(value)))
        .collect::<Vec<_>>();
    // No key selects no row, never every row in scope.
    if operands.is_empty() {
        return toasty::stmt::Expr::from_untyped(Expr::from(false));
    }
    toasty::stmt::Expr::from_untyped(Expr::and_from_vec(operands))
}

/// `record`'s single primary key as an expression of the foreign-key type `T` referencing it.
pub(crate) fn pk_expr<M, T>(record: &M) -> toasty::stmt::Expr<T>
where
    M: toasty::schema::Model + IntoExpr<M>,
{
    toasty::stmt::Expr::from_untyped(record.by_ref())
}

/// Parses a URL path segment into `M`'s primary-key value and returns `None` when the id does not
/// parse or the key is not a single primitive field.
fn pk_field_value<M>(id: &str) -> Option<(FieldId, Value)>
where
    M: toasty::schema::Model,
{
    let app_model = M::schema();
    let root = app_model.as_root()?;
    if root.primary_key.fields.len() != 1 {
        return None;
    }
    let fid = root.primary_key.fields.first().copied()?;
    let model_field = app_model.fields().get(fid.index)?;
    let FieldTy::Primitive(prim) = &model_field.ty else {
        return None;
    };
    Some((fid, parse_key(&prim.ty, id)?))
}

/// The value of type `ty` that `text` spells, as [`value_text`] writes it, or `None` when it does
/// not parse or `ty` has no text form.
pub(crate) fn parse_key(ty: &Type, text: &str) -> Option<Value> {
    Some(match ty {
        Type::Uuid => Value::Uuid(text.parse().ok()?),
        Type::String => Value::String(text.to_string()),
        Type::Bool => Value::Bool(text.parse().ok()?),
        Type::I8 => Value::I8(text.parse().ok()?),
        Type::I16 => Value::I16(text.parse().ok()?),
        Type::I32 => Value::I32(text.parse().ok()?),
        Type::I64 => Value::I64(text.parse().ok()?),
        Type::U8 => Value::U8(text.parse().ok()?),
        Type::U16 => Value::U16(text.parse().ok()?),
        Type::U32 => Value::U32(text.parse().ok()?),
        Type::U64 => Value::U64(text.parse().ok()?),
        Type::F32 => Value::F32(text.parse().ok()?),
        Type::F64 => Value::F64(text.parse().ok()?),
        Type::Timestamp => Value::Timestamp(text.parse().ok()?),
        Type::Date => Value::Date(text.parse().ok()?),
        Type::Time => Value::Time(text.parse().ok()?),
        Type::DateTime => Value::DateTime(text.parse().ok()?),
        Type::Zoned => Value::Zoned(text.parse().ok()?),
        Type::Bytes => Value::Bytes(text.as_bytes().to_vec()),
        _ => return None,
    })
}

/// Ascending order-bys over `M`'s primary key: the deterministic order a page falls back to.
pub(crate) fn pk_order_bys<M>() -> Vec<toasty::stmt::OrderByExpr>
where
    M: toasty::schema::Model,
{
    debug_assert!(
        M::schema().as_root().is_some(),
        "pk_order_bys: {} is not a root model; deterministic pagination needs its primary key",
        std::any::type_name::<M>()
    );
    pk_fields::<M>()
        .iter()
        .map(|fid| M::path_field::<toasty::stmt::Value>(fid.index).asc())
        .collect()
}

/// Reports whether `M`'s primary key is composite, which has no URL representation.
pub(crate) fn pk_is_composite<M>() -> bool
where
    M: toasty::schema::Model,
{
    pk_fields::<M>().len() > 1
}

/// Builds the equality predicate on `M`'s primary key for a URL id and returns `None` when the id
/// does not parse or the key is not a single primitive field.
pub(crate) fn pk_eq_expr<M>(id: &str) -> Option<toasty::stmt::Expr<bool>>
where
    M: toasty::schema::Model,
{
    let (fid, value) = pk_field_value::<M>(id)?;
    let cond = Expr::eq(Expr::ref_self_field(fid), Expr::from(value));
    Some(toasty::stmt::Expr::from_untyped(cond))
}

/// Builds the `IN` predicate over primary keys for a bulk id list and returns `None` when any id
/// fails to parse or the key is not a single primitive field.
pub(crate) fn pk_in_expr<M>(ids: &[&str]) -> Option<toasty::stmt::Expr<bool>>
where
    M: toasty::schema::Model,
{
    let mut parsed = ids.iter().map(|id| pk_field_value::<M>(id));
    let (fid, first) = parsed.next()??;
    let mut values = vec![first];
    for item in parsed {
        let (f, v) = item?;
        debug_assert_eq!(
            f.index, fid.index,
            "pk_in_expr: one model, one PK field — mixed fields are a bug"
        );
        values.push(v);
    }
    let cond = Expr::in_list(Expr::ref_self_field(fid), Expr::list(values));
    Some(toasty::stmt::Expr::from_untyped(cond))
}

#[cfg(test)]
mod tests;
