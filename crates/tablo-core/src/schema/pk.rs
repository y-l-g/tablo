//! Parses URL ids into typed Toasty predicates through `toasty_core` (upstream #114).

/// Parses a URL path segment into `M`'s primary-key value through `toasty_core` (upstream #114) and
/// returns `None` when the id does not parse or the key is not a single primitive field.
fn pk_field_value<M>(
    id: &str,
) -> Option<(toasty_core::schema::app::FieldId, toasty_core::stmt::Value)>
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
    let toasty_core::schema::app::FieldTy::Primitive(prim) = &model_field.ty else {
        return None;
    };
    let value = match prim.ty {
        toasty_core::stmt::Type::Uuid => toasty_core::stmt::Value::Uuid(id.parse().ok()?),
        toasty_core::stmt::Type::String => toasty_core::stmt::Value::String(id.to_string()),
        toasty_core::stmt::Type::Bool => toasty_core::stmt::Value::Bool(id.parse().ok()?),
        toasty_core::stmt::Type::I8 => toasty_core::stmt::Value::I8(id.parse().ok()?),
        toasty_core::stmt::Type::I16 => toasty_core::stmt::Value::I16(id.parse().ok()?),
        toasty_core::stmt::Type::I32 => toasty_core::stmt::Value::I32(id.parse().ok()?),
        toasty_core::stmt::Type::I64 => toasty_core::stmt::Value::I64(id.parse().ok()?),
        toasty_core::stmt::Type::U8 => toasty_core::stmt::Value::U8(id.parse().ok()?),
        toasty_core::stmt::Type::U16 => toasty_core::stmt::Value::U16(id.parse().ok()?),
        toasty_core::stmt::Type::U32 => toasty_core::stmt::Value::U32(id.parse().ok()?),
        toasty_core::stmt::Type::U64 => toasty_core::stmt::Value::U64(id.parse().ok()?),
        toasty_core::stmt::Type::F32 => toasty_core::stmt::Value::F32(id.parse().ok()?),
        toasty_core::stmt::Type::F64 => toasty_core::stmt::Value::F64(id.parse().ok()?),
        toasty_core::stmt::Type::Bytes => toasty_core::stmt::Value::Bytes(id.as_bytes().to_vec()),
        toasty_core::stmt::Type::Timestamp => toasty_core::stmt::Value::Timestamp(id.parse().ok()?),
        toasty_core::stmt::Type::Date => toasty_core::stmt::Value::Date(id.parse().ok()?),
        toasty_core::stmt::Type::Time => toasty_core::stmt::Value::Time(id.parse().ok()?),
        toasty_core::stmt::Type::DateTime => toasty_core::stmt::Value::DateTime(id.parse().ok()?),
        toasty_core::stmt::Type::Zoned => toasty_core::stmt::Value::Zoned(id.parse().ok()?),
        _ => return None,
    };
    Some((fid, value))
}

/// Reports whether `M`'s primary key is composite, which has no URL representation.
pub(crate) fn pk_is_composite<M>() -> bool
where
    M: toasty::schema::Model,
{
    M::schema()
        .as_root()
        .is_some_and(|root| root.primary_key.fields.len() > 1)
}

/// Builds the equality predicate on `M`'s primary key for a URL id and returns `None` when the id
/// does not parse or the key is not a single primitive field.
pub(crate) fn pk_eq_expr<M>(id: &str) -> Option<toasty::stmt::Expr<bool>>
where
    M: toasty::schema::Model,
{
    let (fid, value) = pk_field_value::<M>(id)?;
    let cond = toasty_core::stmt::Expr::eq(
        toasty_core::stmt::Expr::ref_self_field(fid),
        toasty_core::stmt::Expr::from(value),
    );
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
    let cond = toasty_core::stmt::Expr::in_list(
        toasty_core::stmt::Expr::ref_self_field(fid),
        toasty_core::stmt::Expr::list(values),
    );
    Some(toasty::stmt::Expr::from_untyped(cond))
}

#[cfg(test)]
mod tests;
